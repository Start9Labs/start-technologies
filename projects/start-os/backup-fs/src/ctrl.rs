use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use chacha20::Key;
use fuser::FileType;
use log::error;
use rand::rand_core::UnwrapErr;
use rand::RngExt;
use sha2::{Digest, Sha256};

use crate::atomic_file::AtomicFile;
use crate::directory::{DirectoryContents, DirectoryEntry};
use crate::error::{BkfsResult, BkfsResultExt};
use crate::inode::{Attributes, ContentId, FileData, Inode, InodeAttributes};
use crate::seglog::{self, SegmentLog};
use crate::superblock::{Constants, Superblock};
use crate::vault::EccParams;
use crate::{serde, BackupFSOptions, FUSE_ROOT_ID};

#[derive(Clone)]
pub struct Controller(Arc<ControllerSeed>);

// Controller is cloned across worker threads once the worker pool is
// wired up. Compile-time check that interior mutability stays
// thread-safe.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Controller>();
};

pub struct ControllerSeed {
    config: BackupFSOptions,
    /// Path to the canonical superblock replica (`data_dir/superblock`).
    superblock_path: PathBuf,
    /// Format constants adopted from the superblock at mount — the
    /// authoritative source for ECC params, tier thresholds, etc. (the
    /// environment is consulted only at filesystem creation).
    constants: Constants,
    /// Superblock creation time, carried through password changes.
    created_unix: u64,
    /// Live superblock write generation; bumped on every password change so a
    /// crash mid-rewrite converges to the newest replica on the next mount.
    sb_generation: AtomicU64,
    key: Key,
    contents_dir: PathBuf,
    dirents_dir: PathBuf,
    /// Log-structured inode store. Inodes are records here, not per-inode
    /// files. Behind a Mutex because the Controller is shared across the
    /// worker pool; sealing happens outside the lock (see `seglog`).
    log: Mutex<SegmentLog>,
    /// Monotonic inode-number allocator. Seeded past the highest number ever
    /// seen in the log at mount (ids are never reused), so a recovered
    /// allocator can't collide with a surviving inode.
    next_inode: AtomicU64,
    /// Counts fast (non-durable) saves since the last syncfs. Both
    /// dispatch-thread eviction and worker-thread content saves bump
    /// this; when it reaches the batch threshold, the next caller
    /// triggers a syncfs that flushes everything accumulated.
    pending_saves: AtomicUsize,
    /// Cached fd on the data dir for syncfs. Lazily opened and reused
    /// so the batched syncfs path doesn't pay an open(2) on every call.
    data_dir_fd: OnceLock<File>,
}

/// Dead-byte ratio above which a sealed segment is compacted on the next
/// reclamation pass. Overridable via `BACKUPFS_COMPACT_RATIO`; ≥1.0 disables.
fn compact_ratio() -> f64 {
    use std::sync::OnceLock;
    static R: OnceLock<f64> = OnceLock::new();
    *R.get_or_init(|| {
        std::env::var("BACKUPFS_COMPACT_RATIO")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|r| *r > 0.0)
            .unwrap_or(0.6)
    })
}

impl Controller {
    pub fn new(config: BackupFSOptions) -> BkfsResult<Self> {
        // The superblock is the anchor: it yields the master key and the
        // authoritative format constants (validated/version-gated on open).
        let superblock_path = config.data_dir.join("superblock");
        let sb = Superblock::open_or_create(&superblock_path, &config.password, config.readonly)?;
        let key = sb.key;
        let constants = sb.constants;
        // Opening the log replays existing segments, rebuilding the in-RAM
        // index and the max-inode high-water mark. The segment size is pinned
        // by the superblock, not the environment.
        let log = SegmentLog::open_sized(
            config.data_dir.join("segments"),
            key,
            constants.segment_size,
        )?;
        let next_inode = (log.max_inode() + 1).max(FUSE_ROOT_ID + 1);
        Ok(Self(Arc::new(ControllerSeed {
            key,
            contents_dir: config.data_dir.join("contents"),
            dirents_dir: config.data_dir.join("dirents"),
            log: Mutex::new(log),
            next_inode: AtomicU64::new(next_inode),
            pending_saves: AtomicUsize::new(0),
            data_dir_fd: OnceLock::new(),
            config,
            superblock_path,
            constants,
            created_unix: sb.created_unix,
            sb_generation: AtomicU64::new(sb.generation),
        })))
    }

    /// ECC parameters for new writes, adopted from the superblock.
    pub fn ecc(&self) -> EccParams {
        self.0.constants.ecc()
    }

    /// Inline-tier upper bound (bytes), from the superblock.
    pub fn inline_threshold(&self) -> u64 {
        self.0.constants.inline_threshold
    }

    /// Packed-tier upper bound (bytes), from the superblock.
    pub fn pack_max(&self) -> u64 {
        self.0.constants.pack_max
    }

    /// Directory inline→spill entry threshold, from the superblock.
    pub fn dir_spill(&self) -> usize {
        self.0.constants.dir_spill as usize
    }

    /// Directory per-bucket reshard trigger, from the superblock.
    pub fn dir_max_bucket(&self) -> usize {
        self.0.constants.dir_max_bucket as usize
    }

    /// Append/replace an inode record in the log. `durable` fdatasyncs the
    /// active segment before returning; otherwise durability rides the
    /// batched syncfs. Sealing is done before taking the log lock.
    pub fn log_put(&self, inode: Inode, attrs: &Attributes, durable: bool) -> BkfsResult<()> {
        let rec = seglog::seal_inode(&self.key(), self.ecc(), inode, attrs)?;
        let mut log = self.0.log.lock().unwrap();
        log.append(&rec)?;
        if durable {
            log.sync()?;
        }
        Ok(())
    }

    /// Append a tombstone for `inode`. Callers that need the deletion durable
    /// before removing the inode's content (block files / dir buckets) pass
    /// `durable = true`.
    pub fn log_tombstone(&self, inode: Inode, durable: bool) -> BkfsResult<()> {
        let rec = seglog::seal_tombstone(&self.key(), self.ecc(), inode)?;
        let mut log = self.0.log.lock().unwrap();
        log.append(&rec)?;
        if durable {
            log.sync()?;
        }
        Ok(())
    }

    pub fn log_load(&self, inode: Inode) -> BkfsResult<Option<Attributes>> {
        self.0.log.lock().unwrap().load(inode)
    }

    pub fn log_contains(&self, inode: Inode) -> bool {
        self.0.log.lock().unwrap().contains(inode)
    }

    /// Append/replace a packed-content extent (small/medium file content
    /// stored in shared segments rather than its own block file). Sealing is
    /// done outside the log lock.
    pub fn cpack_put(
        &self,
        id: ContentId,
        bytes: &[u8],
        codec: crate::compress::Codec,
        durable: bool,
    ) -> BkfsResult<()> {
        // Compress before sealing (ciphertext is incompressible); the
        // extent's stored payload carries the compression tag.
        let stored = crate::compress::compress(bytes, codec);
        let rec = seglog::seal_content(&self.key(), self.ecc(), id.0, &stored)?;
        let mut log = self.0.log.lock().unwrap();
        log.append(&rec)?;
        if durable {
            log.sync()?;
        }
        Ok(())
    }

    pub fn cpack_load(&self, id: ContentId) -> BkfsResult<Option<Vec<u8>>> {
        match self.0.log.lock().unwrap().load_content(id.0)? {
            // A packed extent is never size-padded and is ≤ pack_max ≤ one
            // chunk, so cap the decompressed size at a chunk.
            Some(stored) => Ok(Some(crate::compress::decompress(
                &stored,
                crate::blockstore::CHUNK_SIZE as usize,
            )?)),
            None => Ok(None),
        }
    }

    pub fn cpack_tombstone(&self, id: ContentId, durable: bool) -> BkfsResult<()> {
        let rec = seglog::seal_content_tombstone(&self.key(), self.ecc(), id.0)?;
        let mut log = self.0.log.lock().unwrap();
        log.append(&rec)?;
        if durable {
            log.sync()?;
        }
        Ok(())
    }

    /// Number of live inodes in the log index (used by tests to detect
    /// orphan/zombie inodes the old layout would have left as files).
    pub fn live_inode_count(&self) -> usize {
        self.0.log.lock().unwrap().live_count()
    }

    /// Number of live packed-content extents (tests check packed reaping).
    #[cfg(test)]
    pub fn live_content_count(&self) -> usize {
        self.0.log.lock().unwrap().content_count()
    }

    /// No-op kept for the mount path: the log's replay (in `new`) already
    /// rebuilt the index and the inode-number high-water mark.
    pub fn load_inode_pool(&self) -> BkfsResult<()> {
        Ok(())
    }

    /// Reclaim dead space in the log by compacting heavily-dead sealed
    /// segments (live frames relocated verbatim, then the segment deleted).
    /// Gated on `BACKUPFS_COMPACT_RATIO` (default 0.6; ≥1.0 disables): only a
    /// segment more than that fraction dead is rewritten, so mostly-live
    /// segments aren't needlessly re-transferred by the next rsync/rclone.
    /// "Speed over footprint" — run as a larger pass (on unmount), not inline.
    pub fn compact(&self) -> BkfsResult<usize> {
        if self.0.config.readonly {
            return Ok(0);
        }
        let ratio = compact_ratio();
        if ratio >= 1.0 {
            return Ok(0);
        }
        self.0.log.lock().unwrap().compact(ratio)
    }

    pub fn fsck(&self, find_orphans: bool) -> BkfsResult<()> {
        self.fsck_inode(Inode(FUSE_ROOT_ID), None)?;
        if find_orphans {
            self.find_orphans()?;
        }
        Ok(())
    }

    fn fsck_inode(
        &self,
        inode: Inode,
        parent: Option<(&(Inode, OsString), &DirectoryEntry)>,
    ) -> BkfsResult<bool> {
        let mut prune = false;
        let mut changed = false;
        let mut inode = match self.load::<InodeAttributes>(inode) {
            Ok(mut inode) => {
                if let Some((parent, _)) = parent {
                    if inode.attrs.parents.is_empty() {
                        inode.attrs.parents.insert(parent.clone());
                        changed = true;
                    } else if !inode.attrs.parents.contains(parent) {
                        prune = true;
                    }
                }
                inode
            }
            Err(e) => {
                error!("failed to load inode: {e}\n    Reconstructing...");
                changed = true;
                if let Some((parent, entry)) = parent {
                    InodeAttributes::new(
                        inode,
                        Some(parent.clone()),
                        match entry.ty {
                            FileType::Directory => FileData::Directory(DirectoryContents::new()),
                            FileType::Symlink => FileData::Symlink(PathBuf::new()),
                            FileType::RegularFile => FileData::File(ContentId(inode.0)),
                            _ => {
                                return Err(
                                    io::Error::other("unsupported filetype in directory").into()
                                )
                            }
                        },
                    )
                } else {
                    InodeAttributes::new(inode, None, FileData::Directory(DirectoryContents::new()))
                }
            }
        };
        if let FileData::Directory(dir) = &inode.attrs.contents {
            let entries = dir.snapshot(self, inode.inode)?;
            let mut to_prune = Vec::new();
            for (name, entry) in entries.iter() {
                let parent = (inode.inode, name.clone());
                if self.fsck_inode(entry.inode, Some((&parent, entry)))? {
                    to_prune.push(name.clone());
                }
            }
            if !to_prune.is_empty() {
                if let FileData::Directory(dir) = &mut inode.attrs.contents {
                    for name in &to_prune {
                        dir.remove(self, inode.inode, name, true)?;
                    }
                }
                changed = true;
            }
            // Repair any cached len/subdirs that drifted across a crash.
            let inode_no = inode.inode;
            if let FileData::Directory(dir) = &mut inode.attrs.contents {
                if dir.recompute_counts(self, inode_no)? {
                    changed = true;
                }
            }
        }
        if changed {
            self.save(&inode)?;
        }

        Ok(prune)
    }

    fn find_orphans(&self) -> BkfsResult<()> {
        // TODO
        Ok(())
    }

    pub fn change_password(&self, password: &str) -> BkfsResult<()> {
        self.check_rw()?;
        // Re-seal the superblock under the new password with a bumped
        // generation, so a crash mid-rewrite converges to the new password
        // (highest generation wins on load), never rolling back to the old.
        let generation = self.0.sb_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let sb = Superblock {
            key: self.0.key,
            constants: self.0.constants,
            generation,
            created_unix: self.0.created_unix,
        };
        sb.persist(&self.0.superblock_path, password)
    }

    pub fn check_rw(&self) -> BkfsResult<()> {
        if self.0.config.readonly {
            BkfsResult::errno_notrace(libc::EROFS)
        } else {
            Ok(())
        }
    }

    /// Deterministic, key-dependent filename for content block
    /// `(content, idx)`. The name is a keyed SHA-256 hash, so it leaks no
    /// information about the inode or offset, yet is stable across runs —
    /// editing one region of a file rewrites exactly one block file and
    /// every other block keeps its name (the property that makes
    /// rsync/rclone incremental copies cheap).
    pub fn block_path(&self, content: ContentId, idx: u64) -> PathBuf {
        // FROZEN as superblock `path_hash_scheme == 1`: the domain tag, the
        // little-endian id encoding, and the 16-bit-dir / 112-bit-name split
        // below all determine where bytes physically live and how reads find
        // them. Changing any of it makes every existing block file
        // unresolvable, so it must be gated by a FORMAT_VERSION /
        // path_hash_scheme bump (validate-equality refuses a foreign scheme).
        let mut hasher = Sha256::new();
        hasher.update(self.0.key.as_slice());
        hasher.update(b"block");
        hasher.update(content.0.to_le_bytes());
        hasher.update(idx.to_le_bytes());
        let tag = hasher.finalize();
        // 2-level layout: 16-bit bucket (≤65 536 dirs) + 112-bit filename.
        // The 128-bit digest makes collisions cryptographically negligible.
        let dir = u16::from_be_bytes([tag[0], tag[1]]);
        let mut name = String::with_capacity(30);
        for b in &tag[2..16] {
            name.push_str(&format!("{b:02x}"));
        }
        self.0.contents_dir.join(format!("{dir:04x}/{name}"))
    }

    /// Blocks have no legacy layout, so resolution is just [`Self::block_path`].
    pub fn resolve_block_path(&self, content: ContentId, idx: u64) -> PathBuf {
        self.block_path(content, idx)
    }

    /// Path of a spilled-directory bucket file, keyed by `(dir, gen, idx)`.
    /// Part of FROZEN superblock `path_hash_scheme == 1` (see [`Self::block_path`]).
    fn dir_bucket_path(&self, dir: Inode, gen: u64, idx: u32) -> PathBuf {
        let mut hasher = Sha256::new();
        hasher.update(self.0.key.as_slice());
        hasher.update(b"dirbucket");
        hasher.update(dir.0.to_le_bytes());
        hasher.update(gen.to_le_bytes());
        hasher.update(idx.to_le_bytes());
        let tag = hasher.finalize();
        let bucket = u16::from_be_bytes([tag[0], tag[1]]);
        let mut name = String::with_capacity(30);
        for b in &tag[2..16] {
            name.push_str(&format!("{b:02x}"));
        }
        self.0.dirents_dir.join(format!("{bucket:04x}/{name}"))
    }

    /// Load one directory bucket; a missing file is an empty bucket.
    pub fn load_dir_bucket(
        &self,
        dir: Inode,
        gen: u64,
        idx: u32,
    ) -> BkfsResult<crate::directory::Bucket> {
        match std::fs::read(self.dir_bucket_path(dir, gen, idx)) {
            Ok(blob) => serde::deserialize_sealed(&blob, self.key()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Default::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Persist one directory bucket. An empty bucket is removed rather than
    /// written, so a bucket file exists iff it has entries.
    pub fn save_dir_bucket(
        &self,
        dir: Inode,
        gen: u64,
        idx: u32,
        bucket: &crate::directory::Bucket,
        durable: bool,
    ) -> BkfsResult<()> {
        self.check_rw()?;
        if bucket.is_empty() {
            return self.remove_dir_bucket(dir, gen, idx);
        }
        let blob = serde::serialize_sealed(bucket, self.key(), self.ecc())?;
        let mut file = AtomicFile::create_buffered(self.dir_bucket_path(dir, gen, idx))?;
        file.write_all(&blob)?;
        if durable {
            file.save()
        } else {
            file.save_fast()?;
            self.tick_save()?;
            Ok(())
        }
    }

    /// Remove one directory bucket file, tolerating absence.
    pub fn remove_dir_bucket(&self, dir: Inode, gen: u64, idx: u32) -> BkfsResult<()> {
        match std::fs::remove_file(self.dir_bucket_path(dir, gen, idx)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn next_inode(&self) -> BkfsResult<Inode> {
        self.check_rw()?;
        // Monotonic, never-reused. Needs no separate persistence: replay
        // reseeds it past the highest inode ever recorded in the log, so a
        // crash can never re-hand a number a surviving inode still uses.
        let id = self.0.next_inode.fetch_add(1, Ordering::Relaxed);
        if id == 0 || id == u64::MAX {
            return BkfsResult::errno(libc::EMFILE);
        }
        Ok(Inode(id))
    }

    pub fn file_pad(&self, size: u64) -> u64 {
        size + (self
            .0
            .config
            .file_size_padding
            .map(|p| p * size as f64)
            .map(|p| p * UnwrapErr(rand::rng()).random_range(0_f64..=1_f64))
            .map(|p| p as u64)
            .unwrap_or(0))
    }

    pub fn key(&self) -> &Key {
        &self.0.key
    }

    pub fn config(&self) -> &BackupFSOptions {
        &self.0.config
    }

    pub fn save<T: Save>(&self, item: T) -> BkfsResult<()> {
        self.check_rw()?;
        item.save(self)
    }

    /// As `save`, but skips per-file sync_all. The caller must account for
    /// the write in `tick_save` (or call `syncfs` directly) so durability
    /// eventually catches up.
    pub fn save_fast<T: Save>(&self, item: T) -> BkfsResult<()> {
        self.check_rw()?;
        item.save_fast(self)
    }

    pub fn load<T: Load>(&self, args: T::Args<'_>) -> BkfsResult<T> {
        T::load(self, args)
    }

    pub fn exists<T: Exists>(&self, args: T::Args<'_>) -> bool {
        T::exists(self, args)
    }

    fn data_dir_fd(&self) -> io::Result<&File> {
        if let Some(f) = self.0.data_dir_fd.get() {
            return Ok(f);
        }
        let fd = File::open(&self.0.config.data_dir)?;
        // Another thread may have raced us; either outcome is fine,
        // the loser drops its fd.
        let _ = self.0.data_dir_fd.set(fd);
        Ok(self.0.data_dir_fd.get().unwrap())
    }

    /// Flush the entire backing filesystem's page cache and device
    /// write cache. One syncfs replaces many per-file fsync calls when
    /// batching writes with `save_fast`.
    pub fn syncfs(&self) -> io::Result<()> {
        let fd = self.data_dir_fd()?.as_raw_fd();
        // Zero the pending counter under the same call — any races
        // just cause an extra syncfs later, which is harmless.
        self.0.pending_saves.store(0, Ordering::Relaxed);
        // SAFETY: fd is a valid fd we own (held in data_dir_fd).
        if unsafe { libc::syncfs(fd) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Batch threshold for group-commit. Overridden via
    /// `BACKUPFS_SYNC_BATCH`. Larger batches amortize more per-fsync
    /// cost but widen the durability window.
    fn sync_batch() -> usize {
        static BATCH: OnceLock<usize> = OnceLock::new();
        *BATCH.get_or_init(|| {
            std::env::var("BACKUPFS_SYNC_BATCH")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|&n| n > 0)
                .unwrap_or(256)
        })
    }

    /// Account for one fast save. When the batch threshold is reached,
    /// issue a syncfs that flushes everything accumulated.
    pub fn tick_save(&self) -> io::Result<()> {
        let n = self.0.pending_saves.fetch_add(1, Ordering::Relaxed) + 1;
        if n >= Self::sync_batch() {
            self.syncfs()?;
        }
        Ok(())
    }
}

pub trait Save {
    fn save(self, ctrl: &Controller) -> BkfsResult<()>;
    /// Save without sync_all. Default falls back to `save`; implement
    /// this for types whose save path supports the fast variant.
    fn save_fast(self, ctrl: &Controller) -> BkfsResult<()>
    where
        Self: Sized,
    {
        self.save(ctrl)
    }
}

pub trait Load: Sized {
    type Args<'a>;
    fn load(ctrl: &Controller, args: Self::Args<'_>) -> BkfsResult<Self>;
}

pub trait Exists: Load {
    fn exists(ctrl: &Controller, args: Self::Args<'_>) -> bool;
}

pub struct StatFs {
    pub files: u64,
    pub ffree: u64,
}

impl Controller {
    pub fn statfs(&self) -> StatFs {
        // Approximate: every number below the monotonic allocator counts as
        // "used" (deleted ids are never reclaimed). Good enough for df.
        let used = self.0.next_inode.load(Ordering::Relaxed).saturating_sub(1);
        StatFs {
            files: used,
            ffree: u64::MAX - used,
        }
    }
}
