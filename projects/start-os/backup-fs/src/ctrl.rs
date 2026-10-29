use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use chacha20::Key;
use fuser::FileType;
use log::error;
use rand::rand_core::UnwrapErr;
use rand::RngExt;
use zeroize::Zeroizing;

use crate::atomic_file::AtomicFile;
use crate::directory::{DirectoryContents, DirectoryEntry};
use crate::error::{BkfsResult, BkfsResultExt};
use crate::inode::{Attributes, ContentId, FileData, Inode, InodeAttributes};
use crate::objects::{ObjectId, ObjectVersion};
use crate::seglog::{self, SegmentLog};
use crate::snapshot::{Catalog, Snapshot, SnapshotInfo, COW_FORMAT_VERSION};
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
    format_version: u32,
    catalog: Mutex<Catalog>,
    snapshot: Option<Snapshot>,
    uncertain_catalog: AtomicBool,
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
        Self::open(config, None)
    }

    pub(crate) fn open(mut config: BackupFSOptions, snapshot: Option<&str>) -> BkfsResult<Self> {
        if let Some(name) = snapshot {
            crate::snapshot::validate_name(name)?;
            config.readonly = true;
        }
        let password = Zeroizing::new(std::mem::take(&mut config.password));
        let superblock_path = config.data_dir.join("superblock");
        let t = std::time::Instant::now();
        let mut sb = Superblock::open_or_create(&superblock_path, &password, config.readonly)?;
        if !config.readonly
            && config.enable_snapshots
            && sb.format_version == crate::superblock::FORMAT_VERSION
            && crate::superblock::SUPPORTED_FORMAT_VERSION >= COW_FORMAT_VERSION
        {
            sb.format_version = COW_FORMAT_VERSION;
            sb.generation = sb.generation.checked_add(1).ok_or_else(|| {
                crate::error::BkfsError::unsupported("superblock generation exhausted")
            })?;
        }
        if !config.readonly && sb.format_version >= COW_FORMAT_VERSION {
            sb.persist(&superblock_path, &password)?;
        }
        drop(password);
        log::info!("superblock opened in {:?}", t.elapsed());
        let key = sb.key;
        let constants = sb.constants;
        // Opening the log replays existing segments, rebuilding the in-RAM
        // index and the max-inode high-water mark. The segment size is pinned
        // by the superblock, not the environment.
        let t = std::time::Instant::now();
        let mut log = SegmentLog::open_sized(
            config.data_dir.join("segments"),
            key,
            constants.segment_size,
        )?;
        log::info!("segment log opened in {:?}", t.elapsed());
        if sb.format_version >= COW_FORMAT_VERSION {
            log.enable_cow();
        }
        let next_inode = (log.max_inode() + 1).max(FUSE_ROOT_ID + 1);
        let catalog = Catalog::load(&config.data_dir, &key)?;
        log.pin(catalog.pins());
        let snapshot = snapshot
            .map(|name| {
                catalog
                    .entries
                    .get(name)
                    .cloned()
                    .ok_or_else(|| io::Error::from_raw_os_error(libc::ENOENT))
            })
            .transpose()?;
        if let Some(snapshot) = &snapshot {
            log.adopt_roots(&snapshot.roots);
        }
        Ok(Self(Arc::new(ControllerSeed {
            key,
            format_version: sb.format_version,
            catalog: Mutex::new(catalog),
            snapshot,
            uncertain_catalog: AtomicBool::new(false),
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
        self.check_rw()?;
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
        self.check_rw()?;
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
        self.check_rw()?;
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
        self.check_rw()?;
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
        if self.0.format_version >= COW_FORMAT_VERSION {
            self.collect_garbage()?;
        }
        self.0.log.lock().unwrap().compact(ratio)
    }

    /// Persist the in-RAM index checkpoint so the next mount can skip the log
    /// replay. Best-effort and skipped on a read-only mount; a failure just
    /// means the next mount replays as before.
    pub fn save_checkpoint(&self) {
        if self.0.config.readonly {
            return;
        }
        if let Err(e) = self.0.log.lock().unwrap().save_checkpoint() {
            log::warn!("failed to write backup index checkpoint (non-fatal): {e}");
        }
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
            format_version: self.0.format_version,
            migration_pending: false,
        };
        sb.persist(&self.0.superblock_path, password)
    }

    pub fn check_rw(&self) -> BkfsResult<()> {
        if self.0.uncertain_catalog.load(Ordering::Relaxed) {
            BkfsResult::errno_notrace(libc::EIO)
        } else if self.0.config.readonly {
            BkfsResult::errno_notrace(libc::EROFS)
        } else {
            Ok(())
        }
    }

    pub fn block_path(&self, content: ContentId, idx: u64) -> PathBuf {
        self.block_id(content, idx)
            .path(&self.0.config.data_dir, None)
            .unwrap()
    }

    pub(crate) fn block_id(&self, content: ContentId, idx: u64) -> ObjectId {
        ObjectId::block(self.key(), content.0, idx)
    }

    pub fn resolve_block_path(&self, content: ContentId, idx: u64) -> PathBuf {
        self.object_path(self.block_id(content, idx))
            .ok()
            .flatten()
            .unwrap_or_else(|| self.block_path(content, idx))
    }

    fn dir_bucket_id(&self, dir: Inode, gen: u64, idx: u32) -> ObjectId {
        ObjectId::bucket(self.key(), dir.0, gen, idx)
    }

    pub(crate) fn object_path(&self, id: ObjectId) -> BkfsResult<Option<PathBuf>> {
        let version = self.0.log.lock().unwrap().object(id)?;
        if version.is_none()
            && self
                .0
                .snapshot
                .as_ref()
                .is_some_and(|s| !s.legacy.contains(&id))
        {
            return Ok(None);
        }
        Ok(id.path(&self.0.config.data_dir, version))
    }

    pub(crate) fn write_object(
        &self,
        id: ObjectId,
        durable: bool,
        write: impl FnOnce(PathBuf, bool) -> BkfsResult<()>,
    ) -> BkfsResult<()> {
        self.check_rw()?;
        if self.0.format_version < COW_FORMAT_VERSION {
            write(id.path(&self.0.config.data_dir, None).unwrap(), durable)?;
            if !durable {
                self.tick_save()?;
            }
            return Ok(());
        }
        let version = ObjectVersion::new();
        let path = id.path(&self.0.config.data_dir, Some(version)).unwrap();
        write(path.clone(), durable)?;
        if durable {
            crate::objects::sync_object_path(&path)?;
        }
        let rec = seglog::seal_object(self.key(), self.ecc(), id, version)?;
        let mut log = self.0.log.lock().unwrap();
        log.append(&rec)?;
        if durable {
            log.sync()?;
        }
        drop(log);
        if !durable {
            self.tick_save()?;
        }
        Ok(())
    }

    pub(crate) fn remove_object(&self, id: ObjectId) -> BkfsResult<()> {
        self.check_rw()?;
        if self.0.format_version < COW_FORMAT_VERSION {
            return match std::fs::remove_file(id.path(&self.0.config.data_dir, None).unwrap()) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.into()),
            };
        }
        let rec = seglog::seal_object(self.key(), self.ecc(), id, ObjectVersion::Deleted)?;
        self.0.log.lock().unwrap().append(&rec)?;
        self.tick_save()?;
        Ok(())
    }

    /// Load one directory bucket; a missing file is an empty bucket.
    pub fn load_dir_bucket(
        &self,
        dir: Inode,
        gen: u64,
        idx: u32,
    ) -> BkfsResult<crate::directory::Bucket> {
        let Some(path) = self.object_path(self.dir_bucket_id(dir, gen, idx))? else {
            return Ok(Default::default());
        };
        match std::fs::read(path) {
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
        self.write_object(
            self.dir_bucket_id(dir, gen, idx),
            durable,
            |path, durable| {
                let mut file = AtomicFile::create_buffered(path)?;
                file.write_all(&blob)?;
                if durable {
                    file.save()
                } else {
                    file.save_fast()
                }
            },
        )
    }

    /// Remove one directory bucket file, tolerating absence.
    pub fn remove_dir_bucket(&self, dir: Inode, gen: u64, idx: u32) -> BkfsResult<()> {
        self.remove_object(self.dir_bucket_id(dir, gen, idx))
    }

    fn legacy_exists(&self, id: ObjectId) -> BkfsResult<bool> {
        match std::fs::symlink_metadata(id.path(&self.0.config.data_dir, None).unwrap()) {
            Ok(metadata) if metadata.is_file() => Ok(true),
            Ok(_) => Err(crate::error::BkfsError::unsupported(
                "unexpected legacy object",
            )),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    fn referenced_storage(
        &self,
        roots: &seglog::Roots,
    ) -> BkfsResult<(HashSet<ObjectId>, HashSet<u64>)> {
        let mut ids = HashSet::new();
        let mut packed = HashSet::new();
        let log = self.0.log.lock().unwrap();
        for (&inode, &loc) in &roots.index {
            let seglog::Record::Inode { attrs, .. } = log.read_at(loc)? else {
                return Err(crate::error::BkfsError::unsupported("invalid inode root"));
            };
            match attrs.contents {
                FileData::File(content) => {
                    for idx in 0..crate::blockstore::block_count(attrs.size) {
                        let id = self.block_id(content, idx);
                        if roots.objects.contains_key(&id) || self.legacy_exists(id)? {
                            ids.insert(id);
                        }
                    }
                }
                FileData::Packed(content) => {
                    packed.insert(content.0);
                }
                FileData::Directory(dir) => {
                    for (gen, idx) in dir.bucket_ids() {
                        let id = self.dir_bucket_id(Inode(inode), gen, idx);
                        if roots.objects.contains_key(&id) || self.legacy_exists(id)? {
                            ids.insert(id);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok((ids, packed))
    }

    pub(crate) fn list_snapshots(&self) -> Vec<SnapshotInfo> {
        self.0
            .catalog
            .lock()
            .unwrap()
            .entries
            .values()
            .map(|s| s.info.clone())
            .collect()
    }

    pub(crate) fn create_snapshot(&self, name: &str) -> BkfsResult<SnapshotInfo> {
        self.check_rw()?;
        crate::snapshot::validate_name(name)?;
        if self.0.format_version < COW_FORMAT_VERSION {
            return Err(crate::error::BkfsError::unsupported(
                "snapshots require the authenticated COW format gate",
            ));
        }
        let mut catalog = self.0.catalog.lock().unwrap();
        if catalog.entries.contains_key(name) {
            return Err(io::Error::from_raw_os_error(libc::EEXIST).into());
        }
        self.syncfs()?;
        let mut roots = self.0.log.lock().unwrap().roots();
        let (ids, packed) = self.referenced_storage(&roots)?;
        roots.objects.retain(|id, _| ids.contains(id));
        roots.content.retain(|id, _| packed.contains(id));
        let mut legacy = HashSet::new();
        for id in ids {
            if !roots.objects.contains_key(&id) && self.legacy_exists(id)? {
                legacy.insert(id);
            }
        }
        self.0.log.lock().unwrap().persist_roots()?;
        let info = SnapshotInfo {
            name: name.to_owned(),
            created_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        let mut next = catalog.clone();
        next.entries.insert(
            name.to_owned(),
            Snapshot {
                info: info.clone(),
                roots,
                legacy,
            },
        );
        if let Err(e) = next.persist(&self.0.config.data_dir, self.key(), self.ecc()) {
            self.0.uncertain_catalog.store(true, Ordering::Relaxed);
            return Err(e);
        }
        self.0.log.lock().unwrap().pin(next.pins());
        *catalog = next;
        Ok(info)
    }

    pub(crate) fn delete_snapshot(&self, name: &str) -> BkfsResult<()> {
        self.check_rw()?;
        crate::snapshot::validate_name(name)?;
        {
            let mut catalog = self.0.catalog.lock().unwrap();
            let mut next = catalog.clone();
            if next.entries.remove(name).is_none() {
                return Err(io::Error::from_raw_os_error(libc::ENOENT).into());
            }
            if let Err(e) = next.persist(&self.0.config.data_dir, self.key(), self.ecc()) {
                self.0.uncertain_catalog.store(true, Ordering::Relaxed);
                return Err(e);
            }
            self.0.log.lock().unwrap().pin(next.pins());
            *catalog = next;
        }
        self.collect_garbage()?;
        Ok(())
    }

    pub(crate) fn collect_garbage(&self) -> BkfsResult<usize> {
        self.check_rw()?;
        if self.0.format_version < COW_FORMAT_VERSION {
            return Ok(0);
        }
        self.syncfs()?;
        let roots = self.0.log.lock().unwrap().roots();
        let (ids, packed) = self.referenced_storage(&roots)?;
        let mut keep = HashSet::new();
        for id in &ids {
            if let Some(path) = self.object_path(*id)? {
                keep.insert(path);
            }
        }
        let inventory = crate::objects::Inventory::load(&self.0.config.data_dir)?;
        let legacy = inventory.legacy_ids().collect();
        let catalog = self.0.catalog.lock().unwrap();
        {
            let mut log = self.0.log.lock().unwrap();
            for snapshot in catalog.entries.values() {
                for id in self.snapshot_object_ids(snapshot) {
                    let version = match snapshot.roots.objects.get(&id) {
                        Some(&loc) => match log.read_at(loc)? {
                            seglog::Record::Object { version, .. } => Some(version),
                            _ => {
                                return Err(crate::error::BkfsError::unsupported(
                                    "invalid snapshot object root",
                                ))
                            }
                        },
                        None => None,
                    };
                    if let Some(path) = id.path(&self.0.config.data_dir, version) {
                        keep.insert(path);
                    }
                }
            }
            log.retain_storage(&ids, &legacy, &packed);
            log.persist_roots()?;
        }
        let removed = inventory.collect_garbage(&keep)?;
        self.0.log.lock().unwrap().compact(0.0)?;
        Ok(removed)
    }

    fn snapshot_object_ids(&self, snapshot: &Snapshot) -> HashSet<ObjectId> {
        snapshot
            .roots
            .objects
            .keys()
            .copied()
            .chain(snapshot.legacy.iter().copied())
            .collect()
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
