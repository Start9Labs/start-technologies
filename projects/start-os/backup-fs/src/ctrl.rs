use std::collections::HashSet;
use std::ffi::OsString;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chacha20::Key;
use fuser::FileType;
use log::error;
use rand::rand_core::UnwrapErr;
use rand::RngExt;
use zeroize::Zeroizing;

use crate::atomic_file::AtomicFile;
use crate::directory::{DirectoryContents, DirectoryEntry};
use crate::error::{BkfsError, BkfsResult, BkfsResultExt};
use crate::inode::{Attributes, ContentId, FileData, Inode, InodeAttributes};
use crate::objects::{ObjectId, ObjectVersion};
use crate::seglog::{self, SegmentLog};
use crate::snapshot::{Catalog, Snapshot, SnapshotInfo};
use crate::superblock::{Constants, Superblock};
use crate::vault::{Binding, EccParams};
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
    data_dir: PathBuf,
    /// Log-structured inode store. Inodes are records here, not per-inode
    /// files. Behind a Mutex because the Controller is shared across the
    /// worker pool; sealing happens outside the lock (see `seglog`).
    log: Mutex<SegmentLog>,
    /// Seeded above the committed inode high-water mark.
    next_inode: AtomicU64,
    /// Counts fast (non-durable) saves since the last syncfs. Both
    /// dispatch-thread eviction and worker-thread content saves bump
    /// this; when it reaches the batch threshold, the next caller
    /// triggers a syncfs that flushes everything accumulated.
    pending_saves: AtomicUsize,
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
        let sb = Superblock::open_or_create(&superblock_path, &password, config.readonly)?;
        drop(password);
        log::info!("superblock opened in {:?}", t.elapsed());
        if snapshot.is_some() && sb.format_version == 1 {
            return Err(BkfsError::unsupported(
                "convert this backup before using snapshots",
            ));
        }
        let key = sb.key;
        let constants = sb.constants;
        let t = std::time::Instant::now();
        let mut log = SegmentLog::open_format(
            config.data_dir.join("segments"),
            key,
            constants.segment_size,
            constants.ecc(),
            config.readonly,
            sb.format_version,
        )?;
        log::info!("segment log opened in {:?}", t.elapsed());
        let data_dir = log.directory().join("..");
        let next_inode = log.max_inode().saturating_add(1).max(FUSE_ROOT_ID + 1);
        let catalog = Catalog::load(log.directory(), &key, log.catalog_reference())?;
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
            log.adopt_view(snapshot.index)?;
        }
        Ok(Self(Arc::new(ControllerSeed {
            key,
            format_version: sb.format_version,
            catalog: Mutex::new(catalog),
            data_dir,
            log: Mutex::new(log),
            next_inode: AtomicU64::new(next_inode),
            pending_saves: AtomicUsize::new(0),
            config,
            superblock_path,
            constants,
            created_unix: sb.created_unix,
            sb_generation: AtomicU64::new(sb.generation),
        })))
    }

    pub(crate) fn is_legacy(&self) -> bool {
        self.0.format_version == 1
    }

    fn seal_record(&self, record: seglog::Record) -> BkfsResult<seglog::SealedRecord> {
        seglog::seal_record(
            self.key(),
            self.ecc(),
            record,
            if self.is_legacy() {
                Binding::Legacy
            } else {
                Binding::Record
            },
        )
    }

    fn require_snapshots(&self) -> BkfsResult<()> {
        if self.is_legacy() {
            return Err(BkfsError::unsupported(
                "convert this backup before using snapshots",
            ));
        }
        Ok(())
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
        let rec = self.seal_record(seglog::Record::Inode {
            inode: inode.0,
            attrs: attrs.clone(),
        })?;
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
        let rec = self.seal_record(seglog::Record::Tombstone { inode: inode.0 })?;
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
        let rec = self.seal_record(seglog::Record::Content {
            id: id.0,
            bytes: stored,
        })?;
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
        let rec = self.seal_record(seglog::Record::ContentTombstone { id: id.0 })?;
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
        self.collect_garbage()?;
        self.0.log.lock().unwrap().compact(ratio)
    }

    /// Bounds replay work; the committed journal remains authoritative on failure.
    pub fn save_checkpoint(&self) {
        if self.0.config.readonly {
            return;
        }
        let catalog = self.0.catalog.lock().unwrap();
        if let Err(e) = self.0.log.lock().unwrap().save_checkpoint(&catalog.views()) {
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
        self.0.log.lock().unwrap().check_rw()
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
        let id = self.block_id(content, idx);
        let version = self.0.log.lock().unwrap().object(id).ok().flatten();
        id.path(&self.0.config.data_dir, version)
            .unwrap_or_else(|| self.block_path(content, idx))
    }

    fn dir_bucket_id(&self, dir: Inode, gen: u64, idx: u32) -> ObjectId {
        ObjectId::bucket(self.key(), dir.0, gen, idx)
    }

    fn object_reference(
        &self,
        id: ObjectId,
        token: crate::objects::VersionToken,
    ) -> (PathBuf, Binding) {
        (
            id.path(&self.0.data_dir, Some(ObjectVersion::Version(token)))
                .unwrap(),
            Binding::Object { id, version: token },
        )
    }

    pub(crate) fn resolve_object(&self, id: ObjectId) -> BkfsResult<Option<(PathBuf, Binding)>> {
        if self.is_legacy() {
            let path = id.path(&self.0.data_dir, None).unwrap();
            return Ok(path.try_exists()?.then_some((path, Binding::Legacy)));
        }
        Ok(match self.0.log.lock().unwrap().object(id)? {
            Some(ObjectVersion::Version(token)) => Some(self.object_reference(id, token)),
            Some(ObjectVersion::Deleted) | None => None,
        })
    }

    pub(crate) fn write_object(
        &self,
        id: ObjectId,
        durable: bool,
        write: impl FnOnce(PathBuf, Binding) -> BkfsResult<()>,
    ) -> BkfsResult<()> {
        self.check_rw()?;
        if self.is_legacy() {
            write(id.path(&self.0.data_dir, None).unwrap(), Binding::Legacy)?;
            if durable {
                self.syncfs()?;
            } else {
                self.tick_save()?;
            }
            return Ok(());
        }
        let token = ObjectVersion::new_token();
        let (path, binding) = self.object_reference(id, token);
        write(path, binding)?;
        let rec = seglog::seal_object(self.key(), self.ecc(), id, ObjectVersion::Version(token))?;
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
        if self.is_legacy() {
            self.check_rw()?;
            match std::fs::remove_file(id.path(&self.0.data_dir, None).unwrap()) {
                Ok(()) => self.tick_save()?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            return Ok(());
        }
        let mut log = self.0.log.lock().unwrap();
        log.check_rw()?;
        if !log.roots().objects.contains_key(&id) {
            return Ok(());
        }
        let rec = seglog::seal_object(self.key(), self.ecc(), id, ObjectVersion::Deleted)?;
        log.append(&rec)?;
        drop(log);
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
        let Some((path, binding)) = self.resolve_object(self.dir_bucket_id(dir, gen, idx))? else {
            return Ok(Default::default());
        };
        let blob = std::fs::read(path).map_err(BkfsError::referenced_io)?;
        let plain = crate::vault::open_bound(&blob, self.key(), binding)?.plaintext;
        serde::decode(&plain, serde::data_config())
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
        let plain = serde::encode(bucket, serde::data_config())?;
        self.write_object(
            self.dir_bucket_id(dir, gen, idx),
            durable,
            |path, binding| {
                let blob = crate::vault::seal_bound(&plain, self.key(), self.ecc(), binding);
                let mut file = AtomicFile::create_buffered(path)?;
                file.write_all(&blob.bytes)?;
                file.save_fast()
            },
        )
    }

    /// Remove one directory bucket file, tolerating absence.
    pub fn remove_dir_bucket(&self, dir: Inode, gen: u64, idx: u32) -> BkfsResult<()> {
        self.remove_object(self.dir_bucket_id(dir, gen, idx))
    }

    fn referenced_storage(
        &self,
        log: &SegmentLog,
        roots: &seglog::Roots,
    ) -> BkfsResult<(HashSet<ObjectId>, HashSet<u64>)> {
        let mut ids = HashSet::new();
        let mut packed = HashSet::new();
        log.visit_records(
            roots
                .index
                .iter()
                .map(|(&inode, loc)| (seglog::RecordKey::Inode(inode), loc)),
            |record| {
                let seglog::Record::Inode { inode, attrs } = record else {
                    return Err(crate::error::BkfsError::unsupported("invalid inode root"));
                };
                let references = crate::objects::StorageReferences::for_inode(inode, &attrs);
                if let Some(content) = references.packed() {
                    packed.insert(content);
                }
                references.visit_objects(self.key(), |id| {
                    if self.is_legacy() || roots.objects.contains_key(&id) {
                        ids.insert(id);
                    }
                });
                Ok(())
            },
        )?;
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
        self.require_snapshots()?;
        self.check_rw()?;
        crate::snapshot::validate_name(name)?;
        let mut catalog = self.0.catalog.lock().unwrap();
        if catalog.entries.contains_key(name) {
            return Err(io::Error::from_raw_os_error(libc::EEXIST).into());
        }
        let mut log = self.0.log.lock().unwrap();
        let (ids, packed) = self.referenced_storage(&log, log.roots())?;
        log.retain_storage(&ids, &packed)?;
        let view = log
            .save_checkpoint(&catalog.views())?
            .ok_or_else(|| BkfsError::unsupported("snapshots require a converted backup"))?;
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
            Snapshot::new(info.clone(), view, log.roots()),
        );
        let reference = next.write(log.directory(), self.key(), self.ecc())?;
        log.publish_catalog(reference, next.pins())?;
        *catalog = next;
        Ok(info)
    }

    pub(crate) fn delete_snapshot(&self, name: &str) -> BkfsResult<()> {
        self.require_snapshots()?;
        self.check_rw()?;
        crate::snapshot::validate_name(name)?;
        {
            let mut catalog = self.0.catalog.lock().unwrap();
            let mut next = catalog.clone();
            if next.entries.remove(name).is_none() {
                return Err(io::Error::from_raw_os_error(libc::ENOENT).into());
            }
            let mut log = self.0.log.lock().unwrap();
            let reference = next.write(log.directory(), self.key(), self.ecc())?;
            log.publish_catalog(reference, next.pins())?;
            *catalog = next;
        }
        self.collect_garbage()?;
        let catalog = self.0.catalog.lock().unwrap();
        self.0
            .log
            .lock()
            .unwrap()
            .save_checkpoint(&catalog.views())?;
        Ok(())
    }

    fn retain_object_paths(
        &self,
        log: &SegmentLog,
        roots: &seglog::Roots,
        ids: impl Iterator<Item = ObjectId>,
        keep: &mut HashSet<PathBuf>,
    ) -> BkfsResult<()> {
        if self.is_legacy() {
            keep.extend(ids.filter_map(|id| id.path(&self.0.data_dir, None)));
            return Ok(());
        }
        log.visit_records(
            ids.filter_map(|id| {
                roots
                    .objects
                    .get(&id)
                    .map(|loc| (seglog::RecordKey::Object(id), loc))
            }),
            |record| {
                let seglog::Record::Object { id, version } = record else {
                    return Err(BkfsError::unsupported("invalid object root"));
                };
                if let ObjectVersion::Version(token) = version {
                    keep.insert(self.object_reference(id, token).0);
                }
                Ok(())
            },
        )
    }

    pub(crate) fn collect_garbage(&self) -> BkfsResult<usize> {
        self.check_rw()?;
        let catalog = self.0.catalog.lock().unwrap();
        let mut log = self.0.log.lock().unwrap();
        log.sync()?;
        let (ids, packed) = self.referenced_storage(&log, log.roots())?;
        let mut keep = HashSet::new();
        self.retain_object_paths(&log, log.roots(), ids.iter().copied(), &mut keep)?;
        for snapshot in catalog.entries.values() {
            let roots = log.load_view(snapshot.index)?;
            self.retain_object_paths(&log, &roots, roots.objects.keys().copied(), &mut keep)?;
        }
        let inventory = crate::objects::Inventory::load(&self.0.data_dir)?;
        log.retain_storage(&ids, &packed)?;
        log.sync()?;
        inventory.collect_garbage(&keep)
    }

    pub fn next_inode(&self) -> BkfsResult<Inode> {
        self.check_rw()?;
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

    pub fn syncfs(&self) -> io::Result<()> {
        self.0.pending_saves.store(0, Ordering::Relaxed);
        self.0
            .log
            .lock()
            .unwrap()
            .sync()
            .map_err(|error| io::Error::from_raw_os_error(error.to_errno_log()))
    }

    /// Account for one fast save. When the batch threshold is reached,
    /// issue a syncfs that flushes everything accumulated.
    pub fn tick_save(&self) -> io::Result<()> {
        if self.is_legacy() {
            self.0.log.lock().unwrap().note_data_write();
        }
        let n = self.0.pending_saves.fetch_add(1, Ordering::Relaxed) + 1;
        if n >= crate::util::sync_batch() {
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
