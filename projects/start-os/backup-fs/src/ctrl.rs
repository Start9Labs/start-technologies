use std::collections::HashSet;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chacha20::Key;
use fuser::FileType;
use log::error;
use rand::rand_core::UnwrapErr;
use rand::RngExt;
use zeroize::Zeroizing;

use crate::directory::{DirectoryContents, DirectoryEntry};
use crate::error::{BkfsError, BkfsResult, BkfsResultExt};
use crate::inode::{Attributes, ContentId, FileData, Inode, InodeAttributes};
use crate::references::StorageReferences;
use crate::seglog::{self, RecordKey, SegmentLog};
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
    /// Log-structured inode store. Inodes are records here, not per-inode
    /// files. Behind a Mutex because the Controller is shared across the
    /// worker pool; sealing happens outside the lock (see `seglog`).
    log: Mutex<SegmentLog>,
    /// Seeded above the committed inode high-water mark.
    next_inode: AtomicU64,
    /// Content id whose block writes fail, for fault-injection tests.
    #[cfg(test)]
    fail_block_puts: AtomicU64,
    /// Counts saves since the last commit.
    pending_saves: AtomicUsize,
    /// Saves after which a commit is due.
    sync_batch: AtomicUsize,
    /// Set once the batch threshold is reached; cleared by the next commit.
    commit_due: AtomicBool,
    chunks: Mutex<crate::chunks::ChunkCache>,
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

/// Live bytes a compaction pass may relocate when this mount appended fewer.
/// `BACKUPFS_COMPACT_BUDGET` is in MiB.
fn compact_budget() -> u64 {
    use std::sync::OnceLock;
    static BUDGET: OnceLock<u64> = OnceLock::new();
    *BUDGET.get_or_init(|| {
        std::env::var("BACKUPFS_COMPACT_BUDGET")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(256)
            .saturating_mul(1 << 20)
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
            log: Mutex::new(log),
            next_inode: AtomicU64::new(next_inode),
            pending_saves: AtomicUsize::new(0),
            sync_batch: AtomicUsize::new(crate::util::sync_batch()),
            commit_due: AtomicBool::new(false),
            chunks: Mutex::default(),
            #[cfg(test)]
            fail_block_puts: AtomicU64::new(u64::MAX),
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

    fn binding(&self) -> Binding {
        if self.is_legacy() {
            Binding::Legacy
        } else {
            Binding::Record
        }
    }

    fn seal_record(&self, record: seglog::Record) -> BkfsResult<seglog::SealedRecord> {
        seglog::seal_record(self.key(), self.ecc(), record, self.binding())
    }

    fn require_snapshots(&self) -> BkfsResult<()> {
        if self.is_legacy() {
            return Err(BkfsError::unsupported(
                "convert this backup before using snapshots",
            ));
        }
        Ok(())
    }

    /// The format-1 file holding a block or directory bucket.
    fn legacy_object(&self, object: RecordKey) -> PathBuf {
        crate::legacy::object_path(&self.0.config.data_dir, self.key(), object).expect("object key")
    }

    fn load_legacy_object(&self, object: RecordKey) -> BkfsResult<Option<Vec<u8>>> {
        match std::fs::read(self.legacy_object(object)) {
            Ok(blob) => Ok(Some(
                crate::vault::open_bound(&blob, self.key(), Binding::Legacy)?.plaintext,
            )),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Durability follows the next commit's backing barrier.
    fn save_legacy_object(&self, object: RecordKey, plain: &[u8]) -> BkfsResult<()> {
        use std::io::Write;
        let sealed = crate::vault::seal_bound(plain, self.key(), self.ecc(), Binding::Legacy);
        let mut file = crate::atomic_file::AtomicFile::create_buffered(self.legacy_object(object))?;
        file.write_all(&sealed.bytes)?;
        file.save_fast()?;
        self.tick_save();
        Ok(())
    }

    fn remove_legacy_object(&self, object: RecordKey) -> BkfsResult<()> {
        self.check_rw()?;
        match std::fs::remove_file(self.legacy_object(object)) {
            Ok(()) => self.tick_save(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
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

    /// Append/replace an inode record; the next commit publishes it.
    pub fn log_put(&self, inode: Inode, attrs: &Attributes) -> BkfsResult<()> {
        self.check_rw()?;
        let rec = self.seal_record(seglog::Record::Inode {
            inode: inode.0,
            attrs: attrs.clone(),
        })?;
        self.0.log.lock().unwrap().append(&rec)
    }

    /// Removes the inode from the index at the next commit.
    pub fn log_tombstone(&self, inode: Inode) -> BkfsResult<()> {
        self.check_rw()?;
        let rec = self.seal_record(seglog::Record::Tombstone { inode: inode.0 })?;
        self.0.log.lock().unwrap().append(&rec)
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
    ) -> BkfsResult<()> {
        self.check_rw()?;
        // Compress before sealing (ciphertext is incompressible); the
        // extent's stored payload carries the compression tag.
        let stored = crate::compress::compress(bytes, codec);
        let rec = self.seal_record(seglog::Record::Content {
            id: id.0,
            bytes: stored,
        })?;
        self.0.log.lock().unwrap().append(&rec)
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

    pub fn cpack_tombstone(&self, id: ContentId) -> BkfsResult<()> {
        self.check_rw()?;
        let rec = self.seal_record(seglog::Record::ContentTombstone { id: id.0 })?;
        self.0.log.lock().unwrap().append(&rec)
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
        let mut log = self.0.log.lock().unwrap();
        let budget = compact_budget().max(log.appended());
        log.compact(ratio, budget)
    }

    /// Bounds replay work; the committed journal remains authoritative on failure.
    pub fn save_checkpoint(&self) {
        if self.0.config.readonly {
            return;
        }
        let catalog = self.0.catalog.lock().unwrap();
        if let Err(e) = self.0.log.lock().unwrap().close_session(&catalog.views()) {
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
                        dir.remove(self, inode.inode, name)?;
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

    /// The stored block, or `None` for a hole.
    pub(crate) fn block_load(&self, content: ContentId, idx: u64) -> BkfsResult<Option<Vec<u8>>> {
        if self.is_legacy() {
            return self.load_legacy_object(RecordKey::Block(content.0, idx));
        }
        self.0.log.lock().unwrap().load_block(content.0, idx)
    }

    /// Durability follows the next commit.
    pub(crate) fn block_put(
        &self,
        content: ContentId,
        idx: u64,
        stored: Vec<u8>,
    ) -> BkfsResult<()> {
        self.check_rw()?;
        #[cfg(test)]
        if self.0.fail_block_puts.load(Ordering::SeqCst) == content.0 {
            return BkfsResult::errno(libc::EIO);
        }
        if self.is_legacy() {
            return self.save_legacy_object(RecordKey::Block(content.0, idx), &stored);
        }
        let rec = seglog::seal_block(self.key(), self.ecc(), content.0, idx, stored)?;
        self.0.log.lock().unwrap().append(&rec)?;
        self.tick_save();
        Ok(())
    }

    pub(crate) fn block_remove(&self, content: ContentId, idx: u64) -> BkfsResult<()> {
        if self.is_legacy() {
            return self.remove_legacy_object(RecordKey::Block(content.0, idx));
        }
        if self
            .0
            .log
            .lock()
            .unwrap()
            .remove(RecordKey::Block(content.0, idx))?
        {
            self.tick_save();
        }
        Ok(())
    }

    /// Makes block writes of one file fail until cleared.
    #[cfg(test)]
    pub(crate) fn fail_block_puts(&self, content: Option<ContentId>) {
        self.0
            .fail_block_puts
            .store(content.map_or(u64::MAX, |c| c.0), Ordering::SeqCst);
    }

    /// Commits, then locates a block's frame in its segment file.
    #[cfg(test)]
    pub(crate) fn block_frame(&self, content: ContentId, idx: u64) -> (PathBuf, seglog::Location) {
        let mut log = self.0.log.lock().unwrap();
        log.sync().unwrap();
        let loc = log.roots().blocks[&(content.0, idx)];
        (seglog::segment_path(log.directory(), loc.segment), loc)
    }

    /// Bytes appended to segments since the store was opened.
    #[cfg(test)]
    pub(crate) fn appended_bytes(&self) -> u64 {
        self.0.log.lock().unwrap().appended()
    }

    /// Forgets cached segment bytes after a test edits segment files.
    #[cfg(test)]
    pub(crate) fn drop_read_caches(&self) {
        self.0.log.lock().unwrap().drop_read_caches();
    }

    /// Overwrites a committed block's frame so reading it fails.
    #[cfg(test)]
    pub(crate) fn corrupt_block(&self, content: ContentId, idx: u64) {
        use std::os::unix::fs::FileExt;
        let (path, loc) = self.block_frame(content, idx);
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .write_all_at(&vec![0; loc.len as usize], loc.offset)
            .unwrap();
        self.drop_read_caches();
    }

    /// Block and directory-bucket records of the current view, by key.
    #[cfg(test)]
    pub(crate) fn indexed_storage(&self) -> std::collections::BTreeMap<String, (u64, u64, u32)> {
        self.0
            .log
            .lock()
            .unwrap()
            .roots()
            .entries()
            .filter(|(key, _)| matches!(key, RecordKey::Block(..) | RecordKey::Bucket(..)))
            .map(|(key, loc)| (format!("{key:?}"), (loc.segment, loc.offset, loc.len)))
            .collect()
    }

    /// Whether the current view indexes the block.
    #[cfg(test)]
    pub(crate) fn block_exists(&self, content: ContentId, idx: u64) -> bool {
        self.0
            .log
            .lock()
            .unwrap()
            .roots()
            .blocks
            .contains_key(&(content.0, idx))
    }

    /// Load one directory bucket; a missing bucket is empty.
    pub fn load_dir_bucket(
        &self,
        dir: Inode,
        gen: u64,
        idx: u32,
    ) -> BkfsResult<crate::directory::Bucket> {
        let plain = if self.is_legacy() {
            self.load_legacy_object(RecordKey::Bucket(dir.0, gen, idx))?
        } else {
            self.0.log.lock().unwrap().load_bucket(dir.0, gen, idx)?
        };
        match plain {
            Some(plain) => serde::decode(&plain, serde::data_config()),
            None => Ok(Default::default()),
        }
    }

    /// Persist one directory bucket. An empty bucket is removed rather than
    /// written, so a bucket is indexed iff it has entries.
    pub fn save_dir_bucket(
        &self,
        dir: Inode,
        gen: u64,
        idx: u32,
        bucket: &crate::directory::Bucket,
    ) -> BkfsResult<()> {
        self.check_rw()?;
        if bucket.is_empty() {
            return self.remove_dir_bucket(dir, gen, idx);
        }
        let plain = serde::encode(bucket, serde::data_config())?;
        if self.is_legacy() {
            return self.save_legacy_object(RecordKey::Bucket(dir.0, gen, idx), &plain);
        }
        self.0
            .log
            .lock()
            .unwrap()
            .stage_bucket(dir.0, gen, idx, plain)?;
        self.tick_save();
        Ok(())
    }

    /// Remove one directory bucket, tolerating absence.
    pub fn remove_dir_bucket(&self, dir: Inode, gen: u64, idx: u32) -> BkfsResult<()> {
        if self.is_legacy() {
            return self.remove_legacy_object(RecordKey::Bucket(dir.0, gen, idx));
        }
        if self
            .0
            .log
            .lock()
            .unwrap()
            .remove(RecordKey::Bucket(dir.0, gen, idx))?
        {
            self.tick_save();
        }
        Ok(())
    }

    fn referenced_storage(
        &self,
        log: &SegmentLog,
        roots: &seglog::Roots,
    ) -> BkfsResult<(HashSet<RecordKey>, HashSet<u64>)> {
        let mut keys = HashSet::new();
        let mut packed = HashSet::new();
        log.visit_records(
            roots
                .index
                .iter()
                .map(|(&inode, loc)| (RecordKey::Inode(inode), loc)),
            |record| {
                let seglog::Record::Inode { inode, attrs } = record else {
                    return Err(crate::error::BkfsError::unsupported("invalid inode root"));
                };
                let references = StorageReferences::for_inode(inode, &attrs);
                if let Some(content) = references.packed() {
                    packed.insert(content);
                }
                references.visit(|key| {
                    if roots.get(key).is_some() {
                        keys.insert(key);
                    }
                });
                Ok(())
            },
        )?;
        Ok((keys, packed))
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
        self.compact()?;
        let catalog = self.0.catalog.lock().unwrap();
        self.0
            .log
            .lock()
            .unwrap()
            .save_checkpoint(&catalog.views())?;
        Ok(())
    }

    /// Drops index entries no current inode refers to.
    pub(crate) fn collect_garbage(&self) -> BkfsResult<()> {
        self.check_rw()?;
        let mut log = self.0.log.lock().unwrap();
        log.sync()?;
        let (keys, packed) = self.referenced_storage(&log, log.roots())?;
        log.retain_storage(&keys, &packed)?;
        log.sync()
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

    /// The next commit publishes the saved item.
    pub fn save<T: Save>(&self, item: T) -> BkfsResult<()> {
        self.check_rw()?;
        item.save(self)
    }

    pub fn load<T: Load>(&self, args: T::Args<'_>) -> BkfsResult<T> {
        T::load(self, args)
    }

    pub fn exists<T: Exists>(&self, args: T::Args<'_>) -> bool {
        T::exists(self, args)
    }

    /// Publishes everything logged so far. Callers outside the format-1
    /// barriers must log a consistent filesystem first; see
    /// [`crate::handle::Handler::flush_all_dirty`].
    pub fn syncfs(&self) -> io::Result<()> {
        self.chunk_cache()
            .stage(self)
            .map_err(|error| io::Error::from_raw_os_error(error.to_errno_log()))?;
        self.0.pending_saves.store(0, Ordering::Relaxed);
        self.0.commit_due.store(false, Ordering::Relaxed);
        self.0
            .log
            .lock()
            .unwrap()
            .sync()
            .map_err(|error| io::Error::from_raw_os_error(error.to_errno_log()))
    }

    /// Commits a format-1 store. Format-1 object removals take effect immediately.
    pub(crate) fn sync_legacy(&self) -> BkfsResult<()> {
        if self.is_legacy() {
            self.syncfs()?;
        }
        Ok(())
    }

    /// Counts one save toward the commit batch threshold.
    pub fn tick_save(&self) {
        if self.is_legacy() {
            self.0.log.lock().unwrap().note_data_write();
        }
        let n = self.0.pending_saves.fetch_add(1, Ordering::Relaxed) + 1;
        if n >= self.0.sync_batch.load(Ordering::Relaxed) {
            self.0.commit_due.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn chunk_cache(&self) -> std::sync::MutexGuard<'_, crate::chunks::ChunkCache> {
        self.0.chunks.lock().unwrap()
    }

    /// Whether enough saves have accumulated for the next request boundary to commit.
    pub fn commit_due(&self) -> bool {
        self.0.commit_due.load(Ordering::Relaxed)
    }

    #[cfg(test)]
    pub(crate) fn set_sync_batch(&self, saves: usize) {
        self.0.sync_batch.store(saves, Ordering::Relaxed);
    }
}

pub trait Save {
    fn save(self, ctrl: &Controller) -> BkfsResult<()>;
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
