//! Directory entry storage.
//!
//! A directory's `name -> child` map is stored one of two ways:
//!
//! * **Inline** — the whole `OrdMap` lives inside the directory inode's
//!   sealed blob. This is the original layout and stays in force for small
//!   directories, so the common case is byte-for-byte unchanged.
//!
//! * **Spilled** — once a directory grows past the spill threshold
//!   ([`Controller::dir_spill`]) the listing is sharded across `buckets`
//!   separate sealed bucket files, and
//!   the inode keeps only a small marker (`gen`, `buckets`, and cached
//!   `len`/`subdirs` counts). A name maps to exactly one bucket by a keyed
//!   hash, so a single lookup/create/unlink reads or rewrites just *one*
//!   bucket instead of re-serializing the entire listing. That turns the
//!   O(n²) "rewrite the whole directory on every create" cost into O(1) per
//!   operation — the small-file backup bottleneck.
//!
//! Spilling and re-sharding write the new buckets under a fresh `gen` and
//! then commit by saving the inode marker; a crash before the marker save
//! leaves the new buckets as harmless orphans (the marker still points at
//! the old generation / inline data), so the transition is atomic.

use std::ffi::{OsStr, OsString};

use fuser::FileType;
use imbl::OrdMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ctrl::Controller;
use crate::error::BkfsResult;
use crate::inode::Inode;

/// Inline directories with at most this many entries; spill beyond it.
/// Default chosen comfortably above the sizes any test or small directory
/// hits, so typical directories never leave the inline (legacy) path.
///
/// The inline→spill threshold (`dir_spill`) and the per-bucket reshard trigger
/// (`dir_max_bucket`) are recorded in the superblock at creation and adopted on
/// every mount (`Controller::dir_spill` / `Controller::dir_max_bucket`), so
/// they stay consistent regardless of environment drift. They only affect new
/// layout decisions — an existing spilled directory reads its own persisted
/// bucket count from its `Spilled` marker.
///
/// The bucketing-math constants below are *frozen* for this format version:
/// `bucket_of` does `hash % buckets`, so the per-directory bucket count (chosen
/// by `choose_buckets`) is persisted and reads are self-describing; these
/// bounds only shape new/resharded directories. Do not change them without a
/// format-version bump.
const TARGET_PER_BUCKET: u64 = 64;
const MIN_BUCKETS: u32 = 16;
const MAX_BUCKETS: u32 = 8192;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DirectoryEntry {
    pub inode: Inode,
    pub ty: FileType,
}

impl DirectoryEntry {
    fn is_dir(&self) -> bool {
        self.ty == FileType::Directory
    }
}

/// The serialized contents of one bucket (or of an inline directory).
pub type Bucket = OrdMap<OsString, DirectoryEntry>;

/// **Append-only: never reorder or remove variants** (the serialized form is a
/// bincode variant index; a reorder would misread every stored directory). Any
/// reorder requires a `superblock::FORMAT_VERSION` bump.
#[derive(Clone, Debug, Deserialize, Serialize)]
enum DirEntries {
    Inline(Bucket),
    Spilled {
        /// Generation of the current bucket set; bumped on every (re)shard
        /// so a torn transition can't mix old and new buckets.
        gen: u64,
        buckets: u32,
        /// Cached total entry count (avoids scanning every bucket).
        len: u64,
        /// Cached count of child subdirectories (for `nlink`).
        subdirs: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DirectoryContents(DirEntries);

fn choose_buckets(entries: u64) -> u32 {
    let want = entries.div_ceil(TARGET_PER_BUCKET).max(1);
    let mut b = MIN_BUCKETS;
    while (b as u64) < want && b < MAX_BUCKETS {
        b <<= 1;
    }
    b.clamp(MIN_BUCKETS, MAX_BUCKETS)
}

/// Stable, key-dependent bucket index for `name` given a bucket count.
///
/// FROZEN as superblock `dirent_hash_scheme == 1`: the domain tag, the
/// `[..8]` little-endian truncation, and the `% buckets` mapping decide which
/// bucket holds an entry on *read*. Changing it strands every spilled-directory
/// entry, so it must be gated by a `FORMAT_VERSION` / `dirent_hash_scheme` bump.
fn bucket_of(ctrl: &Controller, name: &OsStr, buckets: u32) -> u32 {
    use std::os::unix::ffi::OsStrExt;
    let mut hasher = Sha256::new();
    hasher.update(ctrl.key().as_slice());
    hasher.update(b"dirent");
    hasher.update(name.as_bytes());
    let digest = hasher.finalize();
    let h = u64::from_le_bytes(digest[..8].try_into().unwrap());
    (h % buckets as u64) as u32
}

impl DirectoryContents {
    pub fn new() -> Self {
        Self(DirEntries::Inline(OrdMap::new()))
    }

    pub fn len(&self) -> u64 {
        match &self.0 {
            DirEntries::Inline(m) => m.len() as u64,
            DirEntries::Spilled { len, .. } => *len,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `nlink` for a directory = 1 (".") + one per child subdirectory ("..").
    pub fn nlink(&self) -> usize {
        let subdirs = match &self.0 {
            DirEntries::Inline(m) => m.values().filter(|e| e.is_dir()).count() as u64,
            DirEntries::Spilled { subdirs, .. } => *subdirs,
        };
        1 + subdirs as usize
    }

    /// Look up one entry. Touches a single bucket when spilled.
    pub fn get(
        &self,
        ctrl: &Controller,
        dir: Inode,
        name: &OsStr,
    ) -> BkfsResult<Option<DirectoryEntry>> {
        match &self.0 {
            DirEntries::Inline(m) => Ok(m.get(name).cloned()),
            DirEntries::Spilled { gen, buckets, .. } => {
                let idx = bucket_of(ctrl, name, *buckets);
                let bucket = ctrl.load_dir_bucket(dir, *gen, idx)?;
                Ok(bucket.get(name).cloned())
            }
        }
    }

    /// Insert/replace one entry.
    ///
    /// Returns `Some((gen, buckets))` of a now-superseded bucket generation
    /// that a reshard produced — the caller MUST make the (new-generation)
    /// inode marker durable via `save_inode_durable` and only *then* reap
    /// that old generation (see [`Self::reap_generation`]); reaping it
    /// before the marker is durable would, on crash, leave the marker
    /// pointing at deleted buckets and lose the whole directory.
    ///
    /// `durable` forces the affected bucket to stable storage before
    /// returning (ordering-sensitive callers); the marker is persisted by
    /// the caller via `save_inode`.
    pub fn insert(
        &mut self,
        ctrl: &Controller,
        dir: Inode,
        name: OsString,
        entry: DirectoryEntry,
        durable: bool,
    ) -> BkfsResult<Option<(u64, u32)>> {
        // Spill an inline directory that is about to exceed the threshold.
        // (Spilling never deletes the inline source — it lives in the inode
        // blob until the marker is saved — so a torn spill only orphans the
        // new buckets; no durable-ordering dance is needed here.)
        if let DirEntries::Inline(m) = &self.0 {
            if !m.contains_key(&name) && m.len() >= ctrl.dir_spill() {
                self.spill(ctrl, dir, choose_buckets(m.len() as u64 + 1))?;
            }
        }
        match &mut self.0 {
            DirEntries::Inline(m) => {
                m.insert(name, entry);
                Ok(None)
            }
            DirEntries::Spilled {
                gen,
                buckets,
                len,
                subdirs,
            } => {
                let idx = bucket_of(ctrl, &name, *buckets);
                let mut bucket = ctrl.load_dir_bucket(dir, *gen, idx)?;
                let was_dir = entry.is_dir();
                let prev = bucket.insert(name, entry);
                let prev_dir = prev.as_ref().map(DirectoryEntry::is_dir).unwrap_or(false);
                if prev.is_none() {
                    *len += 1;
                }
                *subdirs = (*subdirs + was_dir as u64).saturating_sub(prev_dir as u64);
                let needs_reshard = bucket.len() > ctrl.dir_max_bucket() && *buckets < MAX_BUCKETS;
                ctrl.save_dir_bucket(dir, *gen, idx, &bucket, durable)?;
                if needs_reshard {
                    return self.reshard(ctrl, dir);
                }
                Ok(None)
            }
        }
    }

    /// Reap a superseded bucket generation. Call ONLY after the new-gen
    /// marker has been durably persisted (see [`Self::insert`]).
    pub fn reap_generation(
        ctrl: &Controller,
        dir: Inode,
        gen: u64,
        buckets: u32,
    ) -> BkfsResult<()> {
        for idx in 0..buckets {
            ctrl.remove_dir_bucket(dir, gen, idx)?;
        }
        Ok(())
    }

    /// Remove one entry, returning it if present.
    pub fn remove(
        &mut self,
        ctrl: &Controller,
        dir: Inode,
        name: &OsStr,
        durable: bool,
    ) -> BkfsResult<Option<DirectoryEntry>> {
        match &mut self.0 {
            DirEntries::Inline(m) => Ok(m.remove(name)),
            DirEntries::Spilled {
                gen,
                buckets,
                len,
                subdirs,
            } => {
                let idx = bucket_of(ctrl, name, *buckets);
                let mut bucket = ctrl.load_dir_bucket(dir, *gen, idx)?;
                let prev = bucket.remove(name);
                if let Some(e) = &prev {
                    *len = len.saturating_sub(1);
                    *subdirs = subdirs.saturating_sub(e.is_dir() as u64);
                    ctrl.save_dir_bucket(dir, *gen, idx, &bucket, durable)?;
                }
                Ok(prev)
            }
        }
    }

    /// Exact emptiness check. Unlike [`Self::is_empty`] (which trusts the
    /// cached count, off by at most one after a crash between a bucket
    /// removal and its marker save), this scans buckets, so `rmdir`'s
    /// ENOTEMPTY decision is always correct.
    pub fn is_empty_exact(&self, ctrl: &Controller, dir: Inode) -> BkfsResult<bool> {
        match &self.0 {
            DirEntries::Inline(m) => Ok(m.is_empty()),
            DirEntries::Spilled { gen, buckets, .. } => {
                for idx in 0..*buckets {
                    if !ctrl.load_dir_bucket(dir, *gen, idx)?.is_empty() {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
        }
    }

    /// Materialize the full `name -> entry` map. Reads every bucket; used by
    /// `opendir` (snapshot for paging) and `fsck`, never on the per-op path.
    pub fn snapshot(&self, ctrl: &Controller, dir: Inode) -> BkfsResult<Bucket> {
        match &self.0 {
            DirEntries::Inline(m) => Ok(m.clone()),
            DirEntries::Spilled { gen, buckets, .. } => {
                let mut all = OrdMap::new();
                for idx in 0..*buckets {
                    for (name, entry) in ctrl.load_dir_bucket(dir, *gen, idx)?.iter() {
                        all.insert(name.clone(), entry.clone());
                    }
                }
                Ok(all)
            }
        }
    }

    /// Recompute the cached `len`/`subdirs` of a spilled directory from its
    /// buckets (fsck uses this to repair counts that drifted across a crash
    /// between a bucket write and its marker save). Returns true if changed.
    pub fn recompute_counts(&mut self, ctrl: &Controller, dir: Inode) -> BkfsResult<bool> {
        if !matches!(self.0, DirEntries::Spilled { .. }) {
            return Ok(false);
        }
        let entries = self.snapshot(ctrl, dir)?;
        let new_len = entries.len() as u64;
        let new_subdirs = entries.values().filter(|e| e.is_dir()).count() as u64;
        if let DirEntries::Spilled { len, subdirs, .. } = &mut self.0 {
            if *len != new_len || *subdirs != new_subdirs {
                *len = new_len;
                *subdirs = new_subdirs;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Remove all on-disk bucket files for a (spilled) directory — called
    /// when the directory inode is garbage-collected.
    pub fn gc(&self, ctrl: &Controller, dir: Inode) -> BkfsResult<()> {
        if let DirEntries::Spilled { gen, buckets, .. } = &self.0 {
            for idx in 0..*buckets {
                ctrl.remove_dir_bucket(dir, *gen, idx)?;
            }
        }
        Ok(())
    }

    /// Convert an inline directory to a freshly-sharded spilled one.
    fn spill(&mut self, ctrl: &Controller, dir: Inode, buckets: u32) -> BkfsResult<()> {
        let DirEntries::Inline(m) = &self.0 else {
            return Ok(());
        };
        let entries = m.clone();
        self.write_generation(ctrl, dir, 1, buckets, &entries)?;
        let subdirs = entries.values().filter(|e| e.is_dir()).count() as u64;
        self.0 = DirEntries::Spilled {
            gen: 1,
            buckets,
            len: entries.len() as u64,
            subdirs,
        };
        Ok(())
    }

    /// Grow a spilled directory's bucket count (rare). Writes a new
    /// generation durably and switches the in-memory marker to it; returns
    /// the OLD `(gen, buckets)` so the caller can reap it *after* making the
    /// new marker durable. Does NOT delete the old generation itself —
    /// deleting it before the new marker is on stable storage would lose the
    /// whole directory on crash.
    fn reshard(&mut self, ctrl: &Controller, dir: Inode) -> BkfsResult<Option<(u64, u32)>> {
        let DirEntries::Spilled {
            gen, buckets, len, ..
        } = self.0
        else {
            return Ok(None);
        };
        let entries = self.snapshot(ctrl, dir)?;
        let new_buckets = choose_buckets(len)
            .max(buckets.saturating_mul(2))
            .min(MAX_BUCKETS);
        if new_buckets == buckets {
            return Ok(None);
        }
        let new_gen = gen + 1;
        self.write_generation(ctrl, dir, new_gen, new_buckets, &entries)?;
        // Recompute counts from the materialized snapshot rather than
        // carrying the cached (possibly drifted) values forward.
        let subdirs = entries.values().filter(|e| e.is_dir()).count() as u64;
        self.0 = DirEntries::Spilled {
            gen: new_gen,
            buckets: new_buckets,
            len: entries.len() as u64,
            subdirs,
        };
        Ok(Some((gen, buckets)))
    }

    #[cfg(test)]
    fn debug_state(&self) -> (bool, u64, u32) {
        match &self.0 {
            DirEntries::Inline(_) => (false, 0, 0),
            DirEntries::Spilled { gen, buckets, .. } => (true, *gen, *buckets),
        }
    }

    /// Distribute `entries` into `buckets` bucket files under `gen` and make
    /// them durable (so the subsequent marker save is the atomic commit).
    fn write_generation(
        &self,
        ctrl: &Controller,
        dir: Inode,
        gen: u64,
        buckets: u32,
        entries: &Bucket,
    ) -> BkfsResult<()> {
        let mut sharded: Vec<Bucket> = vec![OrdMap::new(); buckets as usize];
        for (name, entry) in entries.iter() {
            let idx = bucket_of(ctrl, name, buckets);
            sharded[idx as usize].insert(name.clone(), entry.clone());
        }
        for (idx, bucket) in sharded.iter().enumerate() {
            ctrl.save_dir_bucket(dir, gen, idx as u32, bucket, true)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctrl::Controller;
    use crate::BackupFSOptions;

    fn ctrl(dir: &std::path::Path) -> Controller {
        Controller::new(BackupFSOptions {
            data_dir: dir.to_owned(),
            setuid_support: false,
            password: "x".to_owned(),
            file_size_padding: None,
            readonly: false,
            idmapped: false,
        })
        .unwrap()
    }

    fn entry(i: u64) -> DirectoryEntry {
        DirectoryEntry {
            inode: Inode(1000 + i),
            ty: FileType::RegularFile,
        }
    }

    /// Drive spill + reshard directly: insert past the spill threshold, then
    /// force a reshard. Every entry must remain resolvable, counts stay
    /// exact, reshard hands back the old generation for reaping, and reaping
    /// it (after the simulated durable marker) removes the old buckets while
    /// the new generation still resolves everything.
    #[test]
    fn spill_then_reshard_preserves_all_entries() {
        let tmp = tempdir::TempDir::new("dirtest").unwrap();
        let c = ctrl(tmp.path());
        let dir = Inode(2);
        let mut dc = DirectoryContents::new();
        let n: u64 = 3000; // > 1024 → spills

        for i in 0..n {
            let reap = dc
                .insert(&c, dir, OsString::from(format!("f{i:05}")), entry(i), false)
                .unwrap();
            assert!(reap.is_none(), "no reshard expected during initial fill");
        }
        let (spilled, gen0, buckets0) = dc.debug_state();
        assert!(spilled, "directory should have spilled");
        assert_eq!(dc.len(), n);

        // Every entry resolves through the bucketed lookup.
        for i in 0..n {
            let got = dc
                .get(&c, dir, &OsString::from(format!("f{i:05}")))
                .unwrap();
            assert_eq!(got.unwrap().inode, Inode(1000 + i));
        }

        // Force a reshard; it must hand back the old generation and grow.
        let reap = dc.reshard(&c, dir).unwrap();
        assert_eq!(
            reap,
            Some((gen0, buckets0)),
            "reshard must return old gen for reaping"
        );
        let (_, gen1, buckets1) = dc.debug_state();
        assert!(
            gen1 > gen0 && buckets1 > buckets0,
            "reshard should grow buckets/gen"
        );
        assert_eq!(dc.len(), n, "len preserved across reshard");

        // New generation resolves everything; old-gen buckets still exist.
        for i in 0..n {
            assert_eq!(
                dc.get(&c, dir, &OsString::from(format!("f{i:05}")))
                    .unwrap()
                    .unwrap()
                    .inode,
                Inode(1000 + i)
            );
        }
        assert!(
            !c.load_dir_bucket(dir, gen0, 0).unwrap().is_empty(),
            "old gen present pre-reap"
        );

        // Simulated durable-marker commit, then reap the old generation.
        DirectoryContents::reap_generation(&c, dir, gen0, buckets0).unwrap();
        for idx in 0..buckets0 {
            assert!(
                c.load_dir_bucket(dir, gen0, idx).unwrap().is_empty(),
                "old gen reaped"
            );
        }
        // New generation still intact after reap.
        for i in (0..n).step_by(101) {
            assert_eq!(
                dc.get(&c, dir, &OsString::from(format!("f{i:05}")))
                    .unwrap()
                    .unwrap()
                    .inode,
                Inode(1000 + i)
            );
        }
    }

    /// Removing entries keeps counts non-negative and exact, and recompute
    /// repairs a deliberately corrupted cached count.
    #[test]
    fn counts_saturate_and_recompute() {
        let tmp = tempdir::TempDir::new("dirtest").unwrap();
        let c = ctrl(tmp.path());
        let dir = Inode(3);
        let mut dc = DirectoryContents::new();
        let n: u64 = 1500;
        for i in 0..n {
            dc.insert(&c, dir, OsString::from(format!("f{i:05}")), entry(i), false)
                .unwrap();
        }
        for i in 0..500 {
            dc.remove(&c, dir, &OsString::from(format!("f{i:05}")), false)
                .unwrap();
        }
        assert_eq!(dc.len(), n - 500);
        // Corrupt the cached count, then recompute from buckets.
        if let DirEntries::Spilled { len, .. } = &mut dc.0 {
            *len = 7;
        }
        assert!(dc.recompute_counts(&c, dir).unwrap());
        assert_eq!(dc.len(), n - 500);
        assert!(
            !dc.recompute_counts(&c, dir).unwrap(),
            "recompute is idempotent"
        );
    }
}
