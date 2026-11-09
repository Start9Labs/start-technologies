//! Directory entry storage.
//!
//! A directory's `name -> child` map is stored one of three ways:
//!
//! * **Inline** — the whole `OrdMap` lives inside the directory inode's
//!   sealed blob, for directories up to the spill threshold
//!   ([`Controller::dir_spill`]).
//!
//! * **Spilled** (format 1) — the listing is sharded by a keyed hash across
//!   `buckets` separate bucket objects, and the inode keeps a marker (`gen`,
//!   `buckets`, cached `len`/`subdirs`). Spilling and re-sharding write a new
//!   generation before the marker names it.
//!
//! * **Chunked** (format 2) — entries are appended to bounded chunks (see
//!   [`crate::chunks`]), so adding names rewrites only the last chunk.
//!   Format 2 moves a spilled directory into chunks on its first change.

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
    pub(crate) fn is_dir(&self) -> bool {
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
    Chunked {
        generation: u64,
        chunks: u32,
        len: u64,
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

    pub(crate) fn bucket_geometry(&self) -> (u64, u32) {
        match self.0 {
            DirEntries::Inline(_) => (0, 0),
            DirEntries::Spilled { gen, buckets, .. } => (gen, buckets),
            DirEntries::Chunked {
                generation, chunks, ..
            } => (generation, chunks),
        }
    }

    pub fn len(&self) -> u64 {
        match &self.0 {
            DirEntries::Inline(m) => m.len() as u64,
            DirEntries::Spilled { len, .. } | DirEntries::Chunked { len, .. } => *len,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `nlink` for a directory = 1 (".") + one per child subdirectory ("..").
    pub fn nlink(&self) -> usize {
        let subdirs = match &self.0 {
            DirEntries::Inline(m) => m.values().filter(|e| e.is_dir()).count() as u64,
            DirEntries::Spilled { subdirs, .. } | DirEntries::Chunked { subdirs, .. } => *subdirs,
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
            DirEntries::Chunked {
                generation, chunks, ..
            } => Ok(ctrl
                .chunk_cache()
                .open(ctrl, dir, *generation, *chunks)?
                .get(name)
                .cloned()),
        }
    }

    /// Moves a format-2 directory's entries into new chunks.
    fn rechunk(&mut self, ctrl: &Controller, dir: Inode) -> BkfsResult<()> {
        let entries = self.snapshot(ctrl, dir)?;
        let generation = match self.0 {
            DirEntries::Spilled { gen, buckets, .. } => {
                for idx in 0..buckets {
                    ctrl.remove_dir_bucket(dir, gen, idx)?;
                }
                gen + 1
            }
            _ => 1,
        };
        let mut cache = ctrl.chunk_cache();
        let chunks = cache.create(ctrl, dir, generation, &entries)?;
        self.0 = DirEntries::Chunked {
            generation,
            chunks: chunks.count(),
            len: chunks.len(),
            subdirs: chunks.subdirs(),
        };
        Ok(())
    }

    /// Format 2 writes no hashed buckets.
    fn prepare_change(&mut self, ctrl: &Controller, dir: Inode) -> BkfsResult<()> {
        if !ctrl.is_legacy() && matches!(self.0, DirEntries::Spilled { .. }) {
            self.rechunk(ctrl, dir)?;
        }
        Ok(())
    }

    /// Insert/replace one entry.
    ///
    /// Returns the superseded `(gen, buckets)` after a reshard. The caller
    /// must log the new-generation inode before [`Self::reap_generation`].
    pub fn insert(
        &mut self,
        ctrl: &Controller,
        dir: Inode,
        name: OsString,
        entry: DirectoryEntry,
    ) -> BkfsResult<Option<(u64, u32)>> {
        ctrl.check_rw()?;
        self.prepare_change(ctrl, dir)?;
        if let DirEntries::Inline(m) = &self.0 {
            if !m.contains_key(&name) && m.len() >= ctrl.dir_spill() {
                if ctrl.is_legacy() {
                    self.spill(ctrl, dir, choose_buckets(m.len() as u64 + 1))?;
                } else {
                    self.rechunk(ctrl, dir)?;
                }
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
                ctrl.save_dir_bucket(dir, *gen, idx, &bucket)?;
                if needs_reshard {
                    return self.reshard(ctrl, dir);
                }
                Ok(None)
            }
            DirEntries::Chunked {
                generation,
                chunks,
                len,
                subdirs,
            } => {
                let mut cache = ctrl.chunk_cache();
                let index = cache.open(ctrl, dir, *generation, *chunks)?;
                let added = entry.is_dir() as u64;
                let previous = index.insert(ctrl, dir, name, entry)?;
                *subdirs = (*subdirs + added)
                    .saturating_sub(previous.as_ref().is_some_and(DirectoryEntry::is_dir) as u64);
                *len = index.len();
                *chunks = index.count();
                cache.evict(ctrl, dir)?;
                Ok(None)
            }
        }
    }

    /// Requires the new-generation inode to be logged.
    pub fn reap_generation(
        ctrl: &Controller,
        dir: Inode,
        gen: u64,
        buckets: u32,
    ) -> BkfsResult<()> {
        ctrl.sync_legacy()?;
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
    ) -> BkfsResult<Option<DirectoryEntry>> {
        ctrl.check_rw()?;
        self.prepare_change(ctrl, dir)?;
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
                    ctrl.save_dir_bucket(dir, *gen, idx, &bucket)?;
                }
                Ok(prev)
            }
            DirEntries::Chunked {
                generation,
                chunks,
                len,
                subdirs,
            } => {
                let mut cache = ctrl.chunk_cache();
                let index = cache.open(ctrl, dir, *generation, *chunks)?;
                let removed = index.remove(ctrl, dir, name)?;
                *subdirs = subdirs
                    .saturating_sub(removed.as_ref().is_some_and(DirectoryEntry::is_dir) as u64);
                *len = index.len();
                *chunks = index.count();
                Ok(removed)
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
            DirEntries::Chunked {
                generation, chunks, ..
            } => Ok(ctrl
                .chunk_cache()
                .open(ctrl, dir, *generation, *chunks)?
                .len()
                == 0),
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
            DirEntries::Chunked {
                generation, chunks, ..
            } => Ok(ctrl
                .chunk_cache()
                .open(ctrl, dir, *generation, *chunks)?
                .snapshot()),
        }
    }

    /// Recompute the cached `len`/`subdirs` of a spilled directory from its
    /// buckets (fsck uses this to repair counts that drifted across a crash
    /// between a bucket write and its marker save). Returns true if changed.
    pub fn recompute_counts(&mut self, ctrl: &Controller, dir: Inode) -> BkfsResult<bool> {
        if matches!(self.0, DirEntries::Inline(_)) {
            return Ok(false);
        }
        let entries = self.snapshot(ctrl, dir)?;
        let new_len = entries.len() as u64;
        let new_subdirs = entries.values().filter(|e| e.is_dir()).count() as u64;
        if let DirEntries::Spilled { len, subdirs, .. } | DirEntries::Chunked { len, subdirs, .. } =
            &mut self.0
        {
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
        match &self.0 {
            DirEntries::Inline(_) => {}
            DirEntries::Spilled { gen, buckets, .. } => {
                for idx in 0..*buckets {
                    ctrl.remove_dir_bucket(dir, *gen, idx)?;
                }
            }
            DirEntries::Chunked {
                generation, chunks, ..
            } => {
                let cached = ctrl.chunk_cache().forget(dir).unwrap_or(0);
                for idx in 0..cached.max(*chunks) {
                    ctrl.remove_dir_bucket(dir, *generation, idx)?;
                }
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

    /// Returns the superseded generation for reclamation after logging the inode.
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
            DirEntries::Chunked {
                generation, chunks, ..
            } => (true, *generation, *chunks),
        }
    }

    /// Distribute `entries` into `buckets` buckets under `gen`.
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
            ctrl.save_dir_bucket(dir, gen, idx as u32, bucket)?;
        }
        ctrl.sync_legacy()
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
    fn bucket_updates_coalesce_until_commit() {
        let tmp = tempfile::TempDir::with_prefix("dirtest").unwrap();
        let c = ctrl(tmp.path());
        let dir = Inode(2);
        let mut dc = DirectoryContents::new();
        for i in 0..1100 {
            dc.insert(&c, dir, OsString::from(format!("f{i:05}")), entry(i))
                .unwrap();
        }
        c.syncfs().unwrap();
        let (_, _, buckets) = dc.debug_state();
        let committed = c.indexed_storage();
        let before = c.appended_bytes();
        for i in 1100..1600 {
            dc.insert(&c, dir, OsString::from(format!("f{i:05}")), entry(i))
                .unwrap();
        }
        assert_eq!(c.indexed_storage(), committed);
        assert_eq!(
            dc.get(&c, dir, &OsString::from("f01599"))
                .unwrap()
                .unwrap()
                .inode,
            Inode(2599)
        );
        c.syncfs().unwrap();
        let rewritten = c.indexed_storage();
        assert_eq!(rewritten.len(), buckets as usize);
        let largest = rewritten
            .values()
            .map(|&(_, _, len)| len as u64)
            .max()
            .unwrap();
        assert!(c.appended_bytes() - before <= buckets as u64 * largest);
        assert_eq!(dc.snapshot(&c, dir).unwrap().len(), 1600);
    }

    fn legacy_ctrl(dir: &std::path::Path, readonly: bool) -> Controller {
        crate::legacy::fixture(dir);
        Controller::new(BackupFSOptions {
            data_dir: dir.to_owned(),
            setuid_support: false,
            password: "password".to_owned(),
            file_size_padding: None,
            readonly,
            idmapped: false,
        })
        .unwrap()
    }

    fn name(i: u64) -> OsString {
        OsString::from(format!("f{i:05}"))
    }

    fn fill(c: &Controller, dir: Inode, dc: &mut DirectoryContents, names: std::ops::Range<u64>) {
        for i in names {
            dc.insert(c, dir, name(i), entry(i)).unwrap();
        }
    }

    #[test]
    fn adding_names_appends_instead_of_rewriting_the_directory() {
        let tmp = tempfile::TempDir::with_prefix("dirtest").unwrap();
        let c = ctrl(tmp.path());
        let dir = Inode(2);
        let mut dc = DirectoryContents::new();
        fill(&c, dir, &mut dc, 0..50_000);
        c.syncfs().unwrap();
        let stored: u64 = c
            .indexed_storage()
            .values()
            .map(|&(_, _, len)| len as u64)
            .sum();
        let before = c.appended_bytes();
        fill(&c, dir, &mut dc, 50_000..52_000);
        c.syncfs().unwrap();
        let appended = c.appended_bytes() - before;
        assert!(
            appended * 4 < stored,
            "appended {appended} of {stored} bytes"
        );
        assert_eq!(dc.snapshot(&c, dir).unwrap().len(), 52_000);
    }

    #[test]
    fn chunked_directories_survive_reopen_with_replacements_and_removals() {
        let tmp = tempfile::TempDir::with_prefix("dirtest").unwrap();
        let dir = Inode(2);
        let mut dc = DirectoryContents::new();
        let mut expected = Bucket::new();
        {
            let c = ctrl(tmp.path());
            fill(&c, dir, &mut dc, 0..20_000);
            for i in 0..20_000 {
                expected.insert(name(i), entry(i));
            }
            c.syncfs().unwrap();
            for i in (0..20_000).step_by(3) {
                dc.remove(&c, dir, &name(i)).unwrap();
                expected.remove(&name(i));
            }
            for i in (1..20_000).step_by(7) {
                let replacement = DirectoryEntry {
                    inode: Inode(500_000 + i),
                    ty: FileType::Directory,
                };
                dc.insert(&c, dir, name(i), replacement.clone()).unwrap();
                expected.insert(name(i), replacement);
            }
            c.syncfs().unwrap();
        }
        let c = ctrl(tmp.path());
        let restored = dc.snapshot(&c, dir).unwrap();
        assert_eq!(restored.len(), expected.len());
        for (name, entry) in expected.iter() {
            let found = restored.get(name).unwrap();
            assert_eq!((found.inode, found.ty), (entry.inode, entry.ty));
        }
        assert_eq!(dc.len(), expected.len() as u64);
        assert_eq!(
            dc.nlink() as u64,
            1 + expected.values().filter(|e| e.is_dir()).count() as u64
        );
        assert!(!dc.recompute_counts(&c, dir).unwrap());
    }

    #[test]
    fn removing_most_names_repacks_the_chunks() {
        let tmp = tempfile::TempDir::with_prefix("dirtest").unwrap();
        let c = ctrl(tmp.path());
        let dir = Inode(2);
        let mut dc = DirectoryContents::new();
        fill(&c, dir, &mut dc, 0..40_000);
        c.syncfs().unwrap();
        let (_, _, full) = dc.debug_state();
        for i in 0..39_000 {
            dc.remove(&c, dir, &name(i)).unwrap();
        }
        c.syncfs().unwrap();
        let (_, _, sparse) = dc.debug_state();
        assert!(sparse * 4 <= full, "{sparse} chunks after repacking {full}");
        assert_eq!(c.indexed_storage().len(), sparse as usize);
        let listed = dc.snapshot(&c, dir).unwrap();
        assert_eq!(listed.len(), 1_000);
        assert!(listed.contains_key(&name(39_999)));
    }

    #[test]
    fn a_hashed_directory_moves_into_chunks_on_its_first_change() {
        let tmp = tempfile::TempDir::with_prefix("dirtest").unwrap();
        let c = ctrl(tmp.path());
        let dir = Inode(2);
        let mut entries = Bucket::new();
        for i in 0..2_000 {
            entries.insert(name(i), entry(i));
        }
        let mut dc = DirectoryContents(DirEntries::Spilled {
            gen: 3,
            buckets: 64,
            len: 2_000,
            subdirs: 0,
        });
        dc.write_generation(&c, dir, 3, 64, &entries).unwrap();
        c.syncfs().unwrap();
        assert_eq!(
            dc.get(&c, dir, &name(7)).unwrap().unwrap().inode,
            Inode(1007)
        );
        dc.insert(&c, dir, name(2_000), entry(2_000)).unwrap();
        c.syncfs().unwrap();
        let (_, generation, chunks) = dc.debug_state();
        assert_eq!(generation, 4);
        assert!(matches!(dc.0, DirEntries::Chunked { .. }));
        assert_eq!(c.indexed_storage().len(), chunks as usize);
        assert_eq!(dc.snapshot(&c, dir).unwrap().len(), 2_001);
    }

    #[test]
    fn evicted_directories_keep_their_unstaged_changes() {
        let tmp = tempfile::TempDir::with_prefix("dirtest").unwrap();
        let c = ctrl(tmp.path());
        c.chunk_cache().set_limit(3_000);
        let (first, second) = (Inode(2), Inode(3));
        let mut a = DirectoryContents::new();
        let mut b = DirectoryContents::new();
        fill(&c, first, &mut a, 0..2_000);
        fill(&c, second, &mut b, 0..2_000);
        assert!(c.chunk_cache().entry_count() <= 3_000);
        assert_eq!(
            a.get(&c, first, &name(1_999)).unwrap().unwrap().inode,
            Inode(2_999)
        );
        a.remove(&c, first, &name(5)).unwrap();
        fill(&c, second, &mut b, 2_000..2_100);
        assert!(a.get(&c, first, &name(5)).unwrap().is_none());
        c.syncfs().unwrap();
        drop(c);
        let c = ctrl(tmp.path());
        assert_eq!(a.snapshot(&c, first).unwrap().len(), 1_999);
        assert_eq!(b.snapshot(&c, second).unwrap().len(), 2_100);
    }

    #[test]
    fn growing_cached_directories_evicts_older_indexes() {
        let tmp = tempfile::TempDir::with_prefix("dirtest").unwrap();
        let c = ctrl(tmp.path());
        c.chunk_cache().set_limit(3_000);
        let (first, second) = (Inode(2), Inode(3));
        let mut a = DirectoryContents::new();
        let mut b = DirectoryContents::new();
        fill(&c, first, &mut a, 0..1_200);
        fill(&c, second, &mut b, 0..1_200);
        assert_eq!(c.chunk_cache().entry_count(), 2_400);

        fill(&c, second, &mut b, 1_200..2_200);
        assert!(c.chunk_cache().entry_count() <= 3_000);
        c.syncfs().unwrap();
        assert!(c.chunk_cache().entry_count() <= 3_000);
        drop(c);

        let c = ctrl(tmp.path());
        c.chunk_cache().set_limit(3_000);
        assert_eq!(a.snapshot(&c, first).unwrap().len(), 1_200);
        assert_eq!(b.snapshot(&c, second).unwrap().len(), 2_200);
        assert!(c.chunk_cache().entry_count() <= 3_000);
    }

    #[test]
    fn spill_then_reshard_preserves_all_entries() {
        let tmp = tempfile::TempDir::with_prefix("dirtest").unwrap();
        let c = legacy_ctrl(tmp.path(), false);
        let dir = Inode(2);
        let mut dc = DirectoryContents::new();
        let n: u64 = 3000; // > 1024 → spills

        for i in 0..n {
            let reap = dc
                .insert(&c, dir, OsString::from(format!("f{i:05}")), entry(i))
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

    #[test]
    fn legacy_reaping_publishes_the_replacement_marker_first() {
        use crate::handle::Handler;
        use crate::inode::{FileData, InodeAttributes};

        let tmp = tempfile::tempdir().unwrap();
        let c = legacy_ctrl(tmp.path(), false);
        assert!(c.is_legacy());
        let dir = Inode(90_000);
        let entries: Bucket = [(OsString::from("retained"), entry(1))]
            .into_iter()
            .collect();
        let mut dc = DirectoryContents(DirEntries::Spilled {
            gen: 1,
            buckets: MIN_BUCKETS,
            len: 1,
            subdirs: 0,
        });
        dc.write_generation(&c, dir, 1, MIN_BUCKETS, &entries)
            .unwrap();
        let mut attrs = InodeAttributes::new(dir, None, FileData::Directory(dc.clone()));
        c.save(&attrs).unwrap();
        c.syncfs().unwrap();

        let (generation, buckets) = dc.reshard(&c, dir).unwrap().unwrap();
        attrs.attrs.contents = FileData::Directory(dc);
        Handler::new(c.clone()).save_inode_logged(&attrs).unwrap();
        DirectoryContents::reap_generation(&c, dir, generation, buckets).unwrap();

        // Reopen before dropping the writer, whose Drop would flush pending markers.
        let reopened = Controller::new(BackupFSOptions {
            data_dir: tmp.path().to_owned(),
            setuid_support: false,
            password: "password".to_owned(),
            file_size_padding: None,
            readonly: true,
            idmapped: false,
        })
        .unwrap();
        let restored = reopened.load::<InodeAttributes>(dir).unwrap();
        let FileData::Directory(restored) = restored.attrs.contents else {
            panic!("expected a directory");
        };
        let restored = restored.snapshot(&reopened, dir).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(
            restored.get(OsStr::new("retained")).unwrap().inode,
            Inode(1001)
        );
    }

    #[test]
    fn counts_saturate_and_recompute() {
        let tmp = tempfile::TempDir::with_prefix("dirtest").unwrap();
        let c = ctrl(tmp.path());
        let dir = Inode(3);
        let mut dc = DirectoryContents::new();
        let n: u64 = 1500;
        for i in 0..n {
            dc.insert(&c, dir, OsString::from(format!("f{i:05}")), entry(i))
                .unwrap();
        }
        for i in 0..500 {
            dc.remove(&c, dir, &OsString::from(format!("f{i:05}")))
                .unwrap();
        }
        assert_eq!(dc.len(), n - 500);
        // Corrupt the cached count, then recompute from buckets.
        if let DirEntries::Spilled { len, .. } | DirEntries::Chunked { len, .. } = &mut dc.0 {
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
