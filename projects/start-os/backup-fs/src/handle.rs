use std::borrow::Cow;
use std::cell::RefCell;
use std::cmp::min;
use std::collections::{BTreeMap, HashMap};
use std::ffi::{OsStr, OsString};
use std::io;
use std::ops::Bound;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::SystemTime;

use fuser::consts::FUSE_WRITE_KILL_PRIV;
use fuser::{FileType, Request, TimeOrNow, FUSE_ROOT_ID};
use log::{debug, warn};

use crate::contents::Contents;
use crate::ctrl::Controller;
use crate::directory::{DirectoryContents, DirectoryEntry};
use crate::error::{BkfsError, BkfsErrorKind, BkfsResult, BkfsResultExt};
use crate::inode::{FileData, Inode, InodeAttributes};
use crate::{FMODE_EXEC, MAX_NAME_LENGTH};

/// True if `e` is "the inode's backing file isn't on disk" — either
/// from a same-session create+delete that never touched disk, or a
/// stale parent dir entry left by an unclean shutdown. Distinguishes
/// from other I/O errors which remain fatal.
fn is_missing_inode(e: &BkfsError) -> bool {
    match &e.kind {
        BkfsErrorKind::Io(io) => io.kind() == std::io::ErrorKind::NotFound,
        _ => false,
    }
}

/// Upper bound on in-memory unpersisted inode attributes before we start
/// evicting oldest-first. The cap exists only so memory use is bounded
/// on pathological workloads — for typical rsync traffic the set stays
/// small because each release-on-close opens+removes the entry.
const DIRTY_INODE_LIMIT: usize = 1024;

/// Default entries kept in the clean-load read cache. rsync and
/// similar tools re-stat the whole tree several times per run; caching
/// a few thousand loaded inodes turns those passes from O(open+decrypt
/// +deserialize) per entry into an in-memory clone.
const DEFAULT_READ_CACHE_CAP: usize = 4096;

fn read_cache_cap() -> usize {
    static CAP: OnceLock<usize> = OnceLock::new();
    *CAP.get_or_init(|| {
        std::env::var("BACKUPFS_READ_CACHE")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(DEFAULT_READ_CACHE_CAP)
    })
}

/// Bounded LRU of InodeAttributes loaded from disk. Tracks access
/// recency via a monotonically-increasing generation counter —
/// O(log n) per op, no intrusive list bookkeeping.
struct InodeReadCache {
    entries: HashMap<Inode, (u64, InodeAttributes)>,
    by_gen: BTreeMap<u64, Inode>,
    next_gen: u64,
    cap: usize,
}

impl InodeReadCache {
    fn new(cap: usize) -> Self {
        Self {
            entries: HashMap::new(),
            by_gen: BTreeMap::new(),
            next_gen: 0,
            cap,
        }
    }

    fn next_gen(&mut self) -> u64 {
        let g = self.next_gen;
        self.next_gen += 1;
        g
    }

    /// Clone the attrs and bump LRU order so hot entries stay resident.
    fn get(&mut self, inode: Inode) -> Option<InodeAttributes> {
        let (gen, attrs) = self.entries.get_mut(&inode)?;
        let attrs = attrs.clone();
        self.by_gen.remove(gen);
        *gen = self.next_gen;
        self.next_gen += 1;
        self.by_gen.insert(*gen, inode);
        Some(attrs)
    }

    /// Remove and return — caller owns the attrs and the cache forgets
    /// this inode. Used by mutate paths where a stale cache entry would
    /// be a correctness hazard.
    fn take(&mut self, inode: Inode) -> Option<InodeAttributes> {
        let (gen, attrs) = self.entries.remove(&inode)?;
        self.by_gen.remove(&gen);
        Some(attrs)
    }

    fn put(&mut self, inode: Inode, attrs: InodeAttributes) {
        if self.cap == 0 {
            return;
        }
        if let Some((old_gen, _)) = self.entries.get(&inode) {
            self.by_gen.remove(&*old_gen);
        }
        let gen = self.next_gen();
        self.entries.insert(inode, (gen, attrs));
        self.by_gen.insert(gen, inode);
        while self.entries.len() > self.cap {
            let Some((_, victim)) = self.by_gen.pop_first() else {
                break;
            };
            self.entries.remove(&victim);
        }
    }

    fn remove(&mut self, inode: Inode) {
        self.take(inode);
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.by_gen.clear();
    }
}

pub struct Handler {
    ctrl: Controller,
    next_fh: FileHandleId,
    inodes: BTreeMap<Inode, Weak<Mutex<Contents>>>,
    /// Inode attrs with unpersisted changes. Only non-open inodes go
    /// here — for open inodes, Contents holds the authoritative copy and
    /// will persist via Contents::fsync on release.
    ///
    /// rsync's typical per-file pattern is create → write → release →
    /// utime → chmod → chown. The utime/chmod/chown trio all mutate the
    /// same inode after release, and before this cache they each fired a
    /// full AtomicFile::save barrier. Here they collapse into a single
    /// save when the inode is evicted (or on destroy).
    dirty: BTreeMap<Inode, InodeAttributes>,
    /// Clean LRU cache of InodeAttributes loaded from disk. Covers
    /// multi-pass read workloads (rsync's checksum / verify phases)
    /// where the same inode is stat'd many times and the open+decrypt
    /// cost dominates.
    read_cache: RefCell<InodeReadCache>,
    open_files: BTreeMap<FileHandleId, FileHandle>,
    open_dirs: BTreeMap<FileHandleId, DirHandle>,
}
impl Handler {
    pub fn new(ctrl: Controller) -> Self {
        Self {
            ctrl,
            next_fh: FileHandleId(1),
            inodes: BTreeMap::new(),
            dirty: BTreeMap::new(),
            read_cache: RefCell::new(InodeReadCache::new(read_cache_cap())),
            open_files: BTreeMap::new(),
            open_dirs: BTreeMap::new(),
        }
    }

    /// Queue an inode save. For non-open inodes the attrs go into the
    /// dirty cache and get coalesced until eviction or destroy — this is
    /// where rsync's post-close utime/chmod/chown trio collapses to one
    /// disk write.
    ///
    /// For open inodes we save through immediately (same as the historical
    /// behaviour). We can't mark Contents::changed from here without
    /// re-borrowing — most callers are already inside a mutate_inode
    /// closure that holds the RefCell borrow. Luckily the important
    /// coalescing case (setattrs on closed files) is the one rsync uses.
    pub fn save_inode(&mut self, attrs: &InodeAttributes) -> BkfsResult<()> {
        // Any save invalidates the read cache — the cached copy is
        // now stale relative to the authoritative state.
        self.read_cache.borrow_mut().remove(attrs.inode);
        if self
            .inodes
            .get(&attrs.inode)
            .and_then(Weak::upgrade)
            .is_some()
        {
            // Open inodes skip the dirty cache, but still go through
            // the batched sync path — per-file sync_all on inode saves
            // was a major small-file bottleneck on slow backing stores.
            self.ctrl.save_fast(attrs)?;
            self.ctrl.tick_save()?;
            return Ok(());
        }
        self.dirty.insert(attrs.inode, attrs.clone());
        self.evict_dirty_overflow()
    }

    /// Persist inode attrs durably (sync_all) to disk and keep the
    /// in-memory caches in sync. Used where ordering against a
    /// subsequent file removal (gc_inode) matters: a pointer update
    /// that lands in the dirty cache but crashes before flush leaves
    /// the on-disk parent referencing an already-deleted inode, which
    /// the heal then prunes — destroying the user's new data.
    pub fn save_inode_durable(&mut self, attrs: &InodeAttributes) -> BkfsResult<()> {
        self.read_cache.borrow_mut().remove(attrs.inode);
        self.ctrl.save(attrs)?;
        if self
            .inodes
            .get(&attrs.inode)
            .and_then(Weak::upgrade)
            .is_none()
        {
            // Closed inode: keep dirty cache consistent with what we
            // just wrote, so subsequent loads don't fetch the stale
            // pre-modification clone that may still be sitting there.
            self.dirty.insert(attrs.inode, attrs.clone());
            self.evict_dirty_overflow()?;
        }
        Ok(())
    }

    /// Append an inode record to the log *now* (closed-inode safe), via the
    /// batched (non-durable) path. Used where ordering against a following
    /// content tombstone matters: the record must land in the log at a lower
    /// offset than the tombstone so the batched `syncfs` — which flushes the
    /// log prefix and the block files together — can never make the tombstone
    /// durable without also making this record and its blocks durable.
    ///
    /// Unlike `save_inode`, this never parks the record in the dirty cache
    /// (which is invisible to the log-offset ordering the tombstone relies
    /// on); unlike `save_inode_durable`, it does not fsync the log segment in
    /// isolation (which would risk a durable record outliving its not-yet-
    /// synced block files across a crash).
    pub fn save_inode_logged(&mut self, attrs: &InodeAttributes) -> BkfsResult<()> {
        self.read_cache.borrow_mut().remove(attrs.inode);
        // Drop any stale dirty clone so a later flush_all_dirty can't overwrite
        // the log record we're about to append with an older copy.
        self.dirty.remove(&attrs.inode);
        self.ctrl.save_fast(attrs)?;
        self.ctrl.tick_save()?;
        Ok(())
    }

    /// Drop a dirty entry without saving — used when an inode is about
    /// to be removed (unlink+gc path).
    pub fn forget_dirty(&mut self, inode: Inode) {
        self.dirty.remove(&inode);
        self.read_cache.borrow_mut().remove(inode);
    }

    /// Remove and return a dirty entry if present. Used by fopen so a
    /// newly-opened Contents starts from the freshest unpersisted state.
    pub fn take_dirty(&mut self, inode: Inode) -> Option<InodeAttributes> {
        self.dirty.remove(&inode)
    }

    fn evict_dirty_overflow(&mut self) -> BkfsResult<()> {
        while self.dirty.len() > DIRTY_INODE_LIMIT {
            // BTreeMap's first key is the smallest Inode — roughly "oldest
            // allocated," which correlates with "least recently rsync'd"
            // for a sequential backup. Good enough as an eviction hint.
            let Some((&inode, _)) = self.dirty.iter().next() else {
                break;
            };
            let attrs = self.dirty.remove(&inode).unwrap();
            self.ctrl.save_fast(&attrs)?;
            self.ctrl.tick_save()?;
        }
        Ok(())
    }

    /// Persist all pending state to disk durably. Called on destroy
    /// and on FUSE_SYNCFS (`sync -f <mountpoint>`).
    ///
    /// Uses per-file `ctrl.save` (sync_all) instead of `save_fast`
    /// so durability does not depend on the trailing `syncfs` being
    /// effective — which it isn't under start-os's `umount -l` on a
    /// backing FS whose teardown has already begun.
    pub fn flush_all_dirty(&mut self) -> BkfsResult<()> {
        let mut errs = Vec::new();

        // Flush open Contents first — their in-memory state is
        // authoritative for the inode, and its post-rename parents
        // may differ from whatever save_fast last wrote to disk.
        // Taking a snapshot of the inode set avoids holding a borrow
        // across the save, since each save may mutate caches.
        let open: Vec<_> = self
            .inodes
            .iter()
            .filter_map(|(ino, weak)| weak.upgrade().map(|arc| (*ino, arc)))
            .collect();
        for (_, contents) in open {
            let mut contents = contents.lock().unwrap();
            if let Err(e) = contents.flush() {
                errs.push(e);
            }
            // Now also durably re-save the inode attrs — contents.flush
            // uses save_fast, same reason as above.
            if let Err(e) = self.ctrl.save(&contents.inode) {
                errs.push(e);
            }
        }

        let pending = std::mem::take(&mut self.dirty);
        for (_, attrs) in pending {
            if let Err(e) = self.ctrl.save(&attrs) {
                errs.push(e);
            }
        }
        // Dirty-cache entries that shared an inode number with an open
        // file were cleared into `dirty` before this call or handled
        // via the open path above.
        self.read_cache.borrow_mut().clear();
        // Best-effort trailing syncfs — durability is already on the
        // platter from the per-file sync_all, so a failure here is
        // only an early warning of backing-FS issues.
        if let Err(e) = self.ctrl.syncfs() {
            errs.push(e.into());
        }
        BkfsResult::multiple((), errs)
    }

    /// Cache-or-disk read. Populates the cache on miss. Used by paths
    /// that only need to inspect the attrs — mutators go through
    /// `take_cached_or_load` instead so stale cache entries can't
    /// outlive the mutation.
    fn fetch_cached(&self, inode: Inode) -> BkfsResult<InodeAttributes> {
        if let Some(attrs) = self.read_cache.borrow_mut().get(inode) {
            return Ok(attrs);
        }
        let attrs = self.ctrl.load::<InodeAttributes>(inode)?;
        self.read_cache.borrow_mut().put(inode, attrs.clone());
        Ok(attrs)
    }

    /// Fetch for mutation: take from the cache if present (dropping
    /// the entry), else load from disk without populating the cache.
    fn take_cached_or_load(&self, inode: Inode) -> BkfsResult<InodeAttributes> {
        if let Some(attrs) = self.read_cache.borrow_mut().take(inode) {
            return Ok(attrs);
        }
        self.ctrl.load::<InodeAttributes>(inode)
    }

    /// Load an inode, preferring the dirty cache and open Contents over
    /// the disk copy. Direct `ctrl.load` bypasses both caches and would
    /// return a stale version for any inode whose most recent mutation
    /// hasn't been flushed.
    pub fn load_inode(&self, inode: Inode) -> BkfsResult<InodeAttributes> {
        if let Some(contents) = self.inodes.get(&inode).and_then(Weak::upgrade) {
            return Ok(contents.lock().unwrap().inode.clone());
        }
        if let Some(attrs) = self.dirty.get(&inode) {
            return Ok(attrs.clone());
        }
        self.fetch_cached(inode)
    }
    pub fn ctrl(&self) -> &Controller {
        &self.ctrl
    }
    pub fn fopen(
        &mut self,
        inode: Inode,
        read: bool,
        write: bool,
        access: impl FnOnce(&mut Self, &InodeAttributes) -> BkfsResult<()>,
    ) -> BkfsResult<FileHandleId> {
        let contents = if let Some(contents) = self.inodes.get(&inode).and_then(Weak::upgrade) {
            contents
        } else {
            // Build a fresh Contents from whichever source has the
            // freshest state. `dirty` carries forward unpersisted
            // metadata changes (changed=true); `read_cache` is a clean
            // load and doesn't.
            let (attrs, changed) = if let Some(dirty) = self.dirty.remove(&inode) {
                (Some(dirty), true)
            } else if let Some(cached) = self.read_cache.borrow_mut().take(inode) {
                (Some(cached), false)
            } else {
                (None, false)
            };
            let contents = Arc::new(Mutex::new(match attrs {
                Some(a) => Contents::open_with_attrs(self.ctrl.clone(), a, changed)?,
                None => Contents::open(self.ctrl.clone(), inode)?,
            }));
            self.inodes.insert(inode, Arc::downgrade(&contents));
            contents
        };
        access(self, &contents.lock().unwrap().inode)?;
        let fh = self.next_fh;
        self.next_fh.0 += 1;
        self.open_files.insert(
            fh,
            FileHandle {
                inode,
                read,
                write,
                contents,
            },
        );
        Ok(fh)
    }
    pub fn handle(&self, fh: FileHandleId) -> Option<&FileHandle> {
        self.open_files.get(&fh)
    }
    pub fn fclose(&mut self, fh: FileHandleId) -> BkfsResult<()> {
        let Some(handle) = self.open_files.remove(&fh) else {
            return BkfsResult::errno(libc::EBADF);
        };
        handle.close(self)?;
        Ok(())
    }

    fn creation_mode(&self, mode: u32) -> u16 {
        if !self.ctrl().config().setuid_support {
            (mode & !(libc::S_ISUID | libc::S_ISGID) as u32) as u16
        } else {
            mode as u16
        }
    }

    pub fn mutate_inode<F: FnOnce(&mut Self, &mut InodeAttributes) -> BkfsResult<T>, T>(
        &mut self,
        inode: Inode,
        f: F,
    ) -> BkfsResult<T> {
        if let Some(contents) = self.inodes.get(&inode).and_then(Weak::upgrade) {
            let mut contents = contents.lock().unwrap();
            return f(self, &mut contents.inode);
        }
        if let Some(mut attrs) = self.dirty.remove(&inode) {
            // Any mutation invalidates the read cache — clear it now
            // so a subsequent load can't return a stale clone.
            self.read_cache.borrow_mut().remove(inode);
            let result = f(self, &mut attrs);
            // An inode with no parents (and which isn't root) has been
            // fully unlinked — the closure likely ran gc_inode which
            // removed the disk file. Re-inserting the now-orphan attrs
            // into dirty would let flush_all_dirty resurrect them as
            // zombies on the next unmount.
            if attrs.inode.0 == FUSE_ROOT_ID || !attrs.attrs.parents.is_empty() {
                self.dirty.insert(inode, attrs);
            }
            return result;
        }
        let mut attrs = self.take_cached_or_load(inode)?;
        f(self, &mut attrs)
    }

    pub fn peek_inode<F: FnOnce(&InodeAttributes) -> BkfsResult<T>, T>(
        &self,
        inode: Inode,
        f: F,
    ) -> BkfsResult<T> {
        if let Some(contents) = self.inodes.get(&inode).and_then(Weak::upgrade) {
            return f(&contents.lock().unwrap().inode);
        }
        if let Some(attrs) = self.dirty.get(&inode) {
            return f(attrs);
        }
        f(&self.fetch_cached(inode)?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileHandleId(pub u64);

#[derive(Clone)]
pub struct FileHandle {
    pub inode: Inode,
    pub read: bool,
    pub write: bool,
    pub contents: Arc<Mutex<Contents>>,
}
impl FileHandle {
    pub fn close(self, handler: &mut Handler) -> BkfsResult<()> {
        match Arc::try_unwrap(self.contents) {
            Ok(contents) => contents.into_inner().unwrap().close(handler)?,
            Err(contents) => contents.lock().unwrap().fsync()?,
        }
        Ok(())
    }
}

pub struct DirHandle {
    pub inode: Inode,
    pub cursors: BTreeMap<i64, OsString>,
    /// Snapshot of the directory's entries taken at opendir. Paging over a
    /// stable snapshot (rather than re-reading buckets each readdir call)
    /// keeps a spilled directory's listing consistent across the readdir
    /// sequence; the persistent OrdMap clones in O(1).
    pub entries: crate::directory::Bucket,
}

pub struct OverwriteOptions {
    pub gc: bool,
}

impl Handler {
    pub fn close_all(&mut self) -> BkfsResult<()> {
        let mut errs = Vec::new();
        // Close all open files first — each FileHandle::close runs
        // Contents::fsync which persists the inode+content file. This
        // drops strong refs from self.inodes as a side effect (Weak
        // upgrades start failing), but we still clear the map below.
        for (_, handle) in std::mem::take(&mut self.open_files) {
            if let Err(e) = handle.close(self) {
                errs.push(e);
            }
        }
        std::mem::take(&mut self.inodes);
        // Persist any metadata changes (setattr / xattr / link / unlink
        // on non-open inodes) that are still sitting in the dirty cache.
        // Without this, an unmount would drop those changes silently —
        // which has been the behaviour the user hit when the daemon was
        // killed mid-backup.
        if let Err(e) = self.flush_all_dirty() {
            errs.push(e);
        }
        // Reclaim dead log space now that everything is durable. Best-effort:
        // the data is already safe, so a compaction error must not fail the
        // unmount — just log it.
        match self.ctrl().compact() {
            Ok(n) if n > 0 => debug!("compacted {n} dead log segment(s) on unmount"),
            Ok(_) => {}
            Err(e) => warn!("log compaction on unmount failed (non-fatal): {e}"),
        }
        BkfsResult::multiple((), errs)
    }

    pub fn lookup(
        &mut self,
        req: &Request,
        parent: Inode,
        name: &OsStr,
    ) -> BkfsResult<InodeAttributes> {
        if name.len() > MAX_NAME_LENGTH as usize {
            return BkfsResult::errno(libc::ENAMETOOLONG);
        }
        let mut parent = self.load_inode(parent)?;
        parent.attrs.check_access(
            &self.ctrl().config().idmapped_root,
            req.uid(),
            req.gid(),
            libc::X_OK,
        )?;

        let inode = parent.lookup(self.ctrl(), name)?;
        let result = self.mutate_inode(inode, |_, attrs| {
            if !attrs
                .attrs
                .parents
                .contains(&(parent.inode, name.to_owned()))
            {
                return BkfsResult::errno_notrace(libc::ENOENT);
            }
            Ok(attrs.clone())
        });

        match result {
            Ok(attrs) => Ok(attrs),
            Err(e) if is_missing_inode(&e) => {
                // Parent's dir entry references an inode whose backing
                // file doesn't exist — a stale pointer from an unclean
                // shutdown or an earlier bug. Without healing, every
                // subsequent lookup returns ENOENT, which means rmdir
                // never gets a chance to run (rm fails at the lookup
                // step) and the phantom entry hangs around forever.
                //
                // Prune the entry from the parent now. Subsequent
                // readdir will see a clean listing and userspace can
                // proceed.
                warn!(
                    "lookup: pruning stale parent {:?}/{:?} → missing inode {:?}",
                    parent.inode, name, inode
                );
                let ctrl = self.ctrl().clone();
                let parent_inode = parent.inode;
                if let FileData::Directory(dir) = &mut parent.attrs.contents {
                    dir.remove(&ctrl, parent_inode, name, false)?;
                }
                self.save_inode(&parent)?;
                BkfsResult::errno_notrace(libc::ENOENT)
            }
            Err(e) => Err(e),
        }
    }

    pub fn setattr(
        &mut self,
        req: &Request,
        inode: Inode,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        ctime: Option<SystemTime>,
        fh: Option<FileHandleId>,
        crtime: Option<SystemTime>,
        chgtime: Option<SystemTime>,
        bkuptime: Option<SystemTime>,
        flags: Option<u32>,
    ) -> BkfsResult<InodeAttributes> {
        let inode = self.mutate_inode(inode, |handler, inode| {
            let changed = inode.attrs.setattr(
                handler,
                req,
                inode.inode,
                mode,
                uid,
                gid,
                size,
                atime,
                mtime,
                ctime,
                fh,
                crtime,
                chgtime,
                bkuptime,
                flags,
            )?;
            if changed {
                handler.save_inode(&*inode)?;
            }
            Ok(inode.clone())
        })?;
        Ok(inode)
    }

    pub fn readlink(&mut self, req: &Request, inode: Inode) -> BkfsResult<PathBuf> {
        debug!("readlink() called on {:?}", inode);
        let inode = self.load_inode(inode)?;
        inode.attrs.check_access(
            &self.ctrl().config().idmapped_root,
            req.uid(),
            req.gid(),
            libc::R_OK,
        )?;
        let FileData::Symlink(p) = inode.attrs.contents else {
            return BkfsResult::errno(libc::EINVAL);
        };
        Ok(p)
    }

    pub fn mknod<F: FnOnce(Inode) -> FileData>(
        &mut self,
        req: &Request,
        parent: Inode,
        name: &OsStr,
        mut mode: u32,
        umask: u32,
        rdev: u32,
        contents: Option<F>,
    ) -> BkfsResult<InodeAttributes> {
        let file_type = mode & libc::S_IFMT as u32;

        if ![
            libc::S_IFREG,
            libc::S_IFLNK,
            libc::S_IFDIR,
            libc::S_IFCHR,
            libc::S_IFBLK,
            libc::S_IFIFO,
            libc::S_IFSOCK,
        ]
        .contains(&(file_type as _))
        {
            warn!("mknod() called with unsupported file type {:o}", mode);
            return BkfsResult::errno(libc::ENOSYS);
        }

        let mut parent = self.load_inode(parent)?;

        parent.attrs.check_access(
            &self.ctrl().config().idmapped_root,
            req.uid(),
            req.gid(),
            libc::W_OK,
        )?;

        let gid = parent.attrs.creation_gid(req.gid());
        let parent_inode = parent.inode;
        let ctrl = self.ctrl().clone();

        let FileData::Directory(dir) = &mut parent.attrs.contents else {
            return BkfsResult::errno(libc::ENOTDIR);
        };

        if dir.get(&ctrl, parent_inode, name)?.is_some() {
            return BkfsResult::errno(libc::EEXIST);
        }

        if req.uid() != 0 {
            mode &= !(libc::S_ISUID | libc::S_ISGID) as u32;
        }

        let inode = self.ctrl().next_inode()?;

        let contents = if let Some(contents) = contents {
            contents(inode)
        } else {
            let mode = mode & libc::S_IFMT as u32;

            if mode == libc::S_IFREG as u32 {
                // Start inline; grows into block storage past the threshold.
                FileData::Inline(Vec::new())
            } else if mode == libc::S_IFLNK as u32 {
                FileData::Symlink(PathBuf::new())
            } else if mode == libc::S_IFDIR as u32 {
                FileData::Directory(DirectoryContents::new())
            } else if mode == libc::S_IFCHR as u32 {
                FileData::CharDevice(rdev)
            } else if mode == libc::S_IFBLK as u32 {
                FileData::BlockDevice(rdev)
            } else if mode == libc::S_IFIFO as u32 {
                FileData::Fifo
            } else if mode == libc::S_IFSOCK as u32 {
                FileData::Socket
            } else {
                return BkfsResult::errno(libc::ENOSYS);
            }
        };

        let mut new = InodeAttributes::new(inode, Some((parent.inode, name.to_owned())), contents);
        new.attrs.uid = req.uid();
        new.attrs.gid = gid;
        new.attrs.mode = self.creation_mode(mode & !umask);

        let reap = dir.insert(
            &ctrl,
            parent_inode,
            name.to_owned(),
            DirectoryEntry {
                inode: new.inode,
                ty: (&new.attrs.contents).into(),
            },
            false,
        )?;

        self.save_inode(&new)?;

        if let Some((gen, buckets)) = reap {
            // A reshard produced a superseded bucket generation: the new
            // marker must be durable BEFORE the old generation is deleted.
            self.save_inode_durable(&parent)?;
            DirectoryContents::reap_generation(&ctrl, parent_inode, gen, buckets)?;
        } else {
            self.save_inode(&parent)?;
        }

        Ok(new)
    }

    pub fn gc_inode(&mut self, inode: &InodeAttributes) -> BkfsResult<bool> {
        if inode.inode.0 == FUSE_ROOT_ID {
            return Ok(false);
        }
        if !inode.attrs.parents.is_empty() {
            return Ok(false);
        }
        if inode.attrs.contents.is_file()
            && self
                .inodes
                .get(&inode.inode)
                .filter(|rc| Weak::strong_count(rc) > 0)
                .is_some()
        {
            return Ok(false);
        }
        if inode.attrs.contents.is_dir() && self.open_dirs.values().any(|d| d.inode == inode.inode)
        {
            return Ok(false);
        }

        debug!("deleting inode {:?}", inode);
        self.inodes.remove(&inode.inode);
        // Drop any pending metadata save for this inode — otherwise
        // flush_all_dirty on the next unmount would re-create the disk
        // file we're about to remove.
        self.dirty.remove(&inode.inode);
        self.read_cache.borrow_mut().remove(inode.inode);

        // A packed extent's tombstone is appended (non-durable) BEFORE the
        // inode tombstone, so the inode tombstone's fsync below makes both
        // durable together — no window where the inode is gone but its extent
        // lingers as an unreferenced orphan.
        if let FileData::Packed(content) = &inode.attrs.contents {
            self.ctrl().cpack_tombstone(*content, false)?;
        }
        // Tombstone the inode in the log durably BEFORE removing its block /
        // bucket content: a crash with the content gone but the tombstone not
        // yet durable would leave the inode "live" on replay pointing at
        // deleted content. (An inode that only ever lived in the dirty cache
        // and was never appended still gets a harmless tombstone.)
        self.ctrl().log_tombstone(inode.inode, true)?;
        match &inode.attrs.contents {
            FileData::File(contents) => {
                crate::blockstore::remove_all_blocks(self.ctrl(), *contents, inode.attrs.size)?;
            }
            FileData::Directory(dir) => {
                // Reap any spilled-directory bucket files.
                dir.gc(self.ctrl(), inode.inode)?;
            }
            _ => {}
        }

        Ok(true)
    }

    pub fn unlink(&mut self, req: &Request, parent: Inode, name: &OsStr) -> BkfsResult<()> {
        let mut parent = self.load_inode(parent)?;
        parent.attrs.check_access(
            &self.ctrl().config().idmapped_root,
            req.uid(),
            req.gid(),
            libc::W_OK,
        )?;

        let ctrl = self.ctrl().clone();
        let parent_inode = parent.inode;

        // Look the entry up WITHOUT removing it yet: a spilled-directory
        // removal hits the backing store immediately, so it must not be
        // applied until the ENOTEMPTY/sticky checks pass (an inline
        // directory's in-memory removal could be discarded on error, but a
        // bucket write cannot).
        let entry = {
            let FileData::Directory(dir) = &parent.attrs.contents else {
                return BkfsResult::errno(libc::ENOTDIR);
            };
            dir.get(&ctrl, parent_inode, name)?
                .ok_or_else(|| io::Error::from_raw_os_error(libc::ENOENT))?
        };

        let result = self.mutate_inode(entry.inode, |handler, inode| {
            if let FileData::Directory(dir) = &inode.attrs.contents {
                if inode.attrs.parents.len() <= 1 && !dir.is_empty_exact(&ctrl, inode.inode)? {
                    return BkfsResult::errno(libc::ENOTEMPTY);
                }
            }

            parent.attrs.check_sticky(&inode.attrs, req.uid())?;

            inode.attrs.parents.remove(&(parent.inode, name.to_owned()));

            // Checks passed — now remove the directory entry durably, then
            // make the parent durable BEFORE gc. A crash between the entry
            // removal and the child file removal below leaves parent.dir
            // pointing at a deleted inode, which subsequent lookups heal by
            // pruning — the data-loss path users hit when the daemon is
            // killed mid-op, so the ordering matters.
            if let FileData::Directory(dir) = &mut parent.attrs.contents {
                dir.remove(&ctrl, parent_inode, name, true)?;
            }
            handler.save_inode_durable(&parent)?;
            if !handler.gc_inode(&*inode)? {
                handler.save_inode(&*inode)?;
            }

            Ok(())
        });

        match result {
            Ok(()) => Ok(()),
            Err(e) if is_missing_inode(&e) => {
                // Parent referenced a child whose inode file isn't on
                // disk — a stale entry left by an unclean shutdown or an
                // earlier bug. The closure errored before removing the
                // entry, so prune it now and persist the parent so the dir
                // listing reflects reality and the user can get rid of the
                // phantom.
                warn!(
                    "unlink: healing stale parent {:?}/{:?} → missing inode {:?}",
                    parent.inode, name, entry.inode
                );
                if let FileData::Directory(dir) = &mut parent.attrs.contents {
                    dir.remove(&ctrl, parent_inode, name, false)?;
                }
                self.save_inode(&parent)?;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub fn link(
        &mut self,
        req: &Request,
        inode: Inode,
        new_parent: Inode,
        new_name: &OsStr,
        overwrite: Option<OverwriteOptions>,
    ) -> BkfsResult<InodeAttributes> {
        self.mutate_inode(inode, |handler, inode| {
            let ctrl = handler.ctrl().clone();
            let mut ancestor_queue = vec![new_parent];
            while let Some(ancestor) = ancestor_queue.pop() {
                if ancestor == inode.inode {
                    // libc seems to check for this case internally, but we should be safe
                    warn!("tried to create a loop");
                    return BkfsResult::errno(libc::EINVAL);
                }
                handler.peek_inode(ancestor, |ancestor_inode| {
                    ancestor_queue.extend(ancestor_inode.attrs.parents.iter().map(|pair| pair.0));
                    Ok(())
                })?;
            }

            let mut new_parent = handler.load_inode(new_parent)?;
            let new_parent_inode = new_parent.inode;

            new_parent.attrs.check_access(
                &handler.ctrl().config().idmapped_root,
                req.uid(),
                req.gid(),
                libc::W_OK,
            )?;

            let sticky_res = new_parent.attrs.check_sticky(&inode.attrs, req.uid());

            let new_entry = DirectoryEntry {
                inode: inode.inode,
                ty: (&inode.attrs.contents).into(),
            };

            // Look up any existing occupant of new_name (single bucket).
            let prev = {
                let FileData::Directory(dir) = &new_parent.attrs.contents else {
                    return BkfsResult::errno(libc::ENOTDIR);
                };
                dir.get(&ctrl, new_parent_inode, new_name)?
            };
            // Record which inode (if any) to gc after the new pointers are
            // durable. Doing the gc inline removed the prev inode's files
            // before the parent's new entry reached disk; a crash there left
            // the parent pointing to a deleted inode, which lookup then
            // healed by pruning the entry (losing the new one).
            let mut prev_to_gc: Option<Inode> = None;
            let mut reap_gen: Option<(u64, u32)> = None;
            match (prev, overwrite) {
                (Some(_), None) => return BkfsResult::errno(libc::EEXIST),
                (Some(prev_entry), Some(overwrite)) => {
                    let prev_ino = prev_entry.inode;
                    let result = handler.mutate_inode(prev_ino, |handler, prev_inode| {
                        if let FileData::Directory(dir) = &prev_inode.attrs.contents {
                            if prev_inode.attrs.parents.len() <= 1
                                && !dir.is_empty_exact(&ctrl, prev_inode.inode)?
                            {
                                return BkfsResult::errno(libc::ENOTEMPTY);
                            }
                        }

                        sticky_res?;

                        prev_inode
                            .attrs
                            .parents
                            .remove(&(new_parent_inode, new_name.to_owned()));

                        // Always save the prev inode's updated state (may
                        // have other parents via hard links). The actual gc
                        // — if parents went to empty — is deferred to after
                        // the new pointers below are durable.
                        handler.save_inode(&*prev_inode)?;
                        Ok(overwrite.gc && prev_inode.attrs.parents.is_empty())
                    });
                    let should_gc = match result {
                        Ok(v) => v,
                        Err(e) if is_missing_inode(&e) => {
                            // Stale dir entry left by a previous
                            // crash-window bug: parent.dir references an
                            // inode whose file is no longer on disk. Without
                            // this branch, every rename over the ghost name
                            // fails with NotFound and can never be reclaimed.
                            warn!(
                                "link: overwriting stale parent {:?}/{:?} → missing inode {:?}",
                                new_parent_inode, new_name, prev_ino
                            );
                            false
                        }
                        Err(e) => return Err(e),
                    };
                    if should_gc {
                        prev_to_gc = Some(prev_ino);
                    }
                    // Replace the entry. When we're about to gc the old
                    // target, the new pointer must be durable first.
                    if let FileData::Directory(dir) = &mut new_parent.attrs.contents {
                        reap_gen = dir.insert(
                            &ctrl,
                            new_parent_inode,
                            new_name.to_owned(),
                            new_entry,
                            should_gc,
                        )?;
                    }
                }
                (None, _) => {
                    if let FileData::Directory(dir) = &mut new_parent.attrs.contents {
                        reap_gen = dir.insert(
                            &ctrl,
                            new_parent_inode,
                            new_name.to_owned(),
                            new_entry,
                            false,
                        )?;
                    } else {
                        return BkfsResult::errno(libc::ENOTDIR);
                    }
                }
            }

            inode
                .attrs
                .parents
                .insert((new_parent.inode, new_name.to_owned()));

            if prev_to_gc.is_some() || reap_gen.is_some() {
                // Disk ordering matters: make the new pointer + the new
                // directory marker durable BEFORE removing the old target's
                // files or a superseded bucket generation.
                handler.save_inode_durable(&*inode)?;
                handler.save_inode_durable(&new_parent)?;
            } else {
                handler.save_inode(&*inode)?;
                handler.save_inode(&new_parent)?;
            }

            if let Some((gen, buckets)) = reap_gen {
                DirectoryContents::reap_generation(&ctrl, new_parent_inode, gen, buckets)?;
            }

            if let Some(prev_ino) = prev_to_gc {
                // Reload so gc_inode sees the saved parents-empty state
                // rather than a stale cache entry — the dirty cache was
                // consumed by mutate_inode above.
                let prev = handler.load_inode(prev_ino)?;
                handler.gc_inode(&prev)?;
            }

            Ok(inode.clone())
        })
    }

    pub fn rename(
        &mut self,
        req: &Request,
        parent: Inode,
        name: &OsStr,
        new_parent: Inode,
        new_name: &OsStr,
        exchange: bool,
    ) -> BkfsResult<()> {
        let parent = self.load_inode(parent)?;

        parent.attrs.check_access(
            &self.ctrl().config().idmapped_root,
            req.uid(),
            req.gid(),
            libc::W_OK,
        )?;

        let inode = parent.lookup(self.ctrl(), name)?;

        if exchange {
            let new_parent = self.load_inode(new_parent)?;

            new_parent.attrs.check_access(
                &self.ctrl().config().idmapped_root,
                req.uid(),
                req.gid(),
                libc::W_OK,
            )?;

            if new_parent.inode == parent.inode && name == new_name {
                // libc handles this case internally, but we should check
                warn!("rename noop");
                return Ok(());
            }

            let new_inode = new_parent.lookup(self.ctrl(), new_name)?;

            self.link(
                req,
                inode,
                new_parent.inode,
                new_name,
                Some(OverwriteOptions { gc: false }),
            )?;
            self.link(
                req,
                new_inode,
                parent.inode,
                name,
                Some(OverwriteOptions { gc: true }),
            )?;
        } else {
            self.link(
                req,
                inode,
                new_parent,
                new_name,
                Some(OverwriteOptions { gc: true }),
            )?;
            self.unlink(req, parent.inode, name)?;
        }

        Ok(())
    }

    pub fn open(&mut self, req: &Request, inode: Inode, flags: i32) -> BkfsResult<FileHandleId> {
        let (access_mask, read, write) = match flags & libc::O_ACCMODE {
            libc::O_RDONLY => {
                // Behavior is undefined, but most filesystems return EACCES
                if flags & libc::O_TRUNC != 0 {
                    return BkfsResult::errno(libc::EACCES);
                }
                if flags & FMODE_EXEC != 0 {
                    // Open is from internal exec syscall
                    (libc::X_OK, true, false)
                } else {
                    (libc::R_OK, true, false)
                }
            }
            libc::O_WRONLY => (libc::W_OK, false, true),
            libc::O_RDWR => (libc::R_OK | libc::W_OK, true, true),
            // Exactly one access mode flag must be specified
            _ => {
                return BkfsResult::errno(libc::EINVAL);
            }
        };

        self.fopen(inode, read, write, |handler, inode| {
            inode.attrs.check_access(
                &handler.ctrl().config().idmapped_root,
                req.uid(),
                req.gid(),
                access_mask,
            )
        })
    }

    pub fn read(
        &mut self,
        _req: &Request,
        _inode: Inode,
        fh: FileHandleId,
        offset: u64,
        size: usize,
        _flags: i32,
        _lock_owner: Option<u64>,
    ) -> BkfsResult<Vec<u8>> {
        let fh = self
            .handle(fh)
            .ok_or(libc::EBADF)
            .map_err(io::Error::from_raw_os_error)?;
        if !fh.read {
            return BkfsResult::errno(libc::EACCES);
        }

        let mut contents = fh.contents.lock().unwrap();

        let size = min(size, (contents.inode.attrs.size - offset) as usize);

        let mut buf = vec![0_u8; size];

        contents.read_exact_at(&mut buf, offset)?;

        Ok(buf)
    }

    pub fn write(
        &mut self,
        _req: &Request,
        _inode: Inode,
        fh: FileHandleId,
        offset: u64,
        data: &[u8],
        _write_flags: u32,
        flags: i32,
        _lock_owner: Option<u64>,
    ) -> BkfsResult<usize> {
        let fh = self
            .handle(fh)
            .ok_or(libc::EBADF)
            .map_err(io::Error::from_raw_os_error)?;
        if !fh.write {
            return BkfsResult::errno(libc::EACCES);
        }

        let mut contents = fh.contents.lock().unwrap();

        let mut buf = data.to_vec();

        contents.write_all_at(&mut buf, offset)?;

        if flags & FUSE_WRITE_KILL_PRIV as i32 != 0 {
            contents.inode.attrs.clear_suid_sgid();
        }

        Ok(buf.len())
    }

    pub fn fsync(&mut self, _req: &Request, _inode: Inode, fh: FileHandleId) -> BkfsResult<()> {
        let fh = self
            .handle(fh)
            .ok_or(libc::EBADF)
            .map_err(io::Error::from_raw_os_error)?;
        fh.contents.lock().unwrap().fsync()?;
        Ok(())
    }

    pub fn opendir(&mut self, req: &Request, inode: Inode, flags: i32) -> BkfsResult<FileHandleId> {
        let inode = self.load_inode(inode)?;
        let (access_mask, _read, _) = match flags & libc::O_ACCMODE {
            libc::O_RDONLY => {
                // Behavior is undefined, but most filesystems return EACCES
                if flags & libc::O_TRUNC != 0 {
                    return BkfsResult::errno(libc::EACCES);
                }
                (libc::R_OK, true, false)
            }
            libc::O_WRONLY => (libc::W_OK, false, true),
            libc::O_RDWR => (libc::R_OK | libc::W_OK, true, true),
            // Exactly one access mode flag must be specified
            _ => {
                return BkfsResult::errno(libc::EINVAL);
            }
        };
        inode.attrs.check_access(
            &self.ctrl().config().idmapped_root,
            req.uid(),
            req.gid(),
            access_mask,
        )?;
        let entries = match &inode.attrs.contents {
            FileData::Directory(dir) => dir.snapshot(self.ctrl(), inode.inode)?,
            _ => Default::default(),
        };
        let fh = self.next_fh;
        self.next_fh.0 += 1;
        self.open_dirs.insert(
            fh,
            DirHandle {
                inode: inode.inode,
                cursors: BTreeMap::new(),
                entries,
            },
        );
        Ok(fh)
    }

    pub fn readdir(
        &mut self,
        _req: &Request,
        inode: Inode,
        fh: FileHandleId,
        mut offset: i64,
        mut handle_entry: impl FnMut(&mut Self, &OsStr, &DirectoryEntry, i64) -> BkfsResult<bool>,
    ) -> BkfsResult<bool> {
        let inode = self.load_inode(inode)?;
        if !inode.attrs.contents.is_dir() {
            return BkfsResult::errno(libc::ENOTDIR);
        }
        // Take the cursor + an O(1) clone of the opendir snapshot, then drop
        // the handle borrow so handle_entry can take &mut self in the loop.
        let (mut cur, entries) = {
            let Some(handle) = self.open_dirs.get_mut(&fh) else {
                return BkfsResult::errno(libc::EACCES); // opened without read perm
            };
            (
                handle.cursors.remove(&offset).map(Cow::Owned),
                handle.entries.clone(),
            )
        };

        let mut range = if let Some(cursor) = cur.as_deref() {
            entries.range::<_, OsStr>((Bound::Excluded(cursor), Bound::Unbounded))
        } else {
            entries.range::<_, OsStr>(..)
        };

        let self_entry = DirectoryEntry {
            inode: inode.inode,
            ty: FileType::Directory,
        };
        let parent_entry = DirectoryEntry {
            inode: inode
                .attrs
                .parents
                .get_min()
                .map(|(inode, _)| *inode)
                .unwrap_or(inode.inode),
            ty: FileType::Directory,
        };
        let special = if cur.is_none() {
            [
                Some((OsStr::new("."), &self_entry)),
                Some((OsStr::new(".."), &parent_entry)),
            ]
        } else if cur.as_deref() == Some(OsStr::new(".")) {
            [None, Some((OsStr::new(".."), &parent_entry))]
        } else {
            [None, None]
        };

        let mut res: BkfsResult<bool> = Ok(false);
        for (name, entry) in special
            .into_iter()
            .flatten()
            .chain((&mut range).map(|(s, e)| (&**s, e)))
        {
            res = handle_entry(self, name, entry, offset + 1);
            if res.as_ref().ok().copied() == Some(true) || res.is_err() {
                break;
            }
            offset += 1;
            cur = Some(Cow::Borrowed(name));
        }

        let Some(handle) = self.open_dirs.get_mut(&fh) else {
            return BkfsResult::errno(libc::EACCES); // opened without read perm
        };

        if let Some(cur) = cur {
            handle.cursors.insert(offset, cur.into_owned());
        }

        res?;

        Ok(range.next().is_none())
    }

    pub fn releasedir(
        &mut self,
        _req: &Request,
        _inode: Inode,
        fh: FileHandleId,
        _flags: i32,
    ) -> BkfsResult<()> {
        let Some(ent) = self.open_dirs.remove(&fh) else {
            return BkfsResult::errno(libc::EBADF);
        };
        self.gc_inode(&self.load_inode(ent.inode)?)?;

        Ok(())
    }

    pub fn setxattr(
        &mut self,
        req: &Request,
        inode: Inode,
        key: &[u8],
        value: &[u8],
    ) -> BkfsResult<()> {
        self.mutate_inode(inode, |handler, inode| {
            let attrs = &mut inode.attrs;
            attrs.xattr_access_check(
                &handler.ctrl().config().idmapped_root,
                key,
                Some(Some(value)),
                req,
            )?;
            attrs.xattrs.insert(key.to_vec(), value.to_vec());
            attrs.changed();
            handler.save_inode(&*inode)?;
            Ok(())
        })
    }

    pub fn getxattr(&self, req: &Request, inode: Inode, key: &[u8]) -> BkfsResult<Vec<u8>> {
        self.peek_inode(inode, |inode| {
            inode
                .attrs
                .xattr_access_check(&self.ctrl().config().idmapped_root, key, None, req)?;
            match inode.attrs.xattrs.get(key) {
                Some(v) => Ok(v.clone()),
                #[cfg(target_os = "linux")]
                None => BkfsResult::errno_notrace(libc::ENODATA),
                #[cfg(not(target_os = "linux"))]
                None => BkfsResult::errno_notrace(libc::ENOATTR),
            }
        })
    }

    pub fn listxattr<'r>(
        &self,
        req: &'r Request,
        inode: Inode,
    ) -> BkfsResult<impl Iterator<Item = (Vec<u8>, Vec<u8>)> + 'r> {
        // TODO: peek_inode and serialize to bytes here
        let inode = self.load_inode(inode)?;
        let mut attrs = inode.attrs;
        let xattrs = std::mem::replace(&mut attrs.xattrs, Default::default());
        let idmap = self.ctrl().config().idmapped_root.clone();
        Ok(xattrs
            .into_iter()
            .filter(move |(key, _)| attrs.xattr_access_check(&idmap, key, None, req).is_ok()))
    }

    pub fn removexattr(&mut self, req: &Request, inode: Inode, key: &[u8]) -> BkfsResult<Vec<u8>> {
        let value = self.mutate_inode(inode, |handler, inode| {
            let attrs = &mut inode.attrs;
            attrs.xattr_access_check(
                &handler.ctrl().config().idmapped_root,
                key,
                Some(None),
                req,
            )?;
            let value = attrs.xattrs.remove(key);
            attrs.changed();
            handler.save_inode(&*inode)?;
            Ok(value)
        })?;
        match value {
            Some(v) => Ok(v),
            #[cfg(target_os = "linux")]
            None => BkfsResult::errno_notrace(libc::ENODATA),
            #[cfg(not(target_os = "linux"))]
            None => BkfsResult::errno_notrace(libc::ENOATTR),
        }
    }

    pub fn create(
        &mut self,
        req: &Request,
        parent: Inode,
        name: &OsStr,
        mode: u32,
        umask: u32,
        flags: i32,
    ) -> BkfsResult<(InodeAttributes, FileHandleId)> {
        let attrs = self.mknod(
            req,
            parent,
            name,
            mode,
            umask,
            0,
            None::<fn(Inode) -> FileData>,
        )?;
        let handle = self.open(req, attrs.inode, flags)?;
        Ok((attrs, handle))
    }

    pub fn copy_file_range(
        &mut self,
        req: &Request,
        src_inode: Inode,
        src_fh: FileHandleId,
        src_offset: u64,
        dest_inode: Inode,
        dest_fh: FileHandleId,
        dest_offset: u64,
        size: usize,
        flags: u32,
    ) -> BkfsResult<usize> {
        if flags != 0 {
            return BkfsResult::errno(libc::EINVAL);
        }
        let bytes = self.read(req, src_inode, src_fh, src_offset, size, 0, None)?;
        self.write(req, dest_inode, dest_fh, dest_offset, &bytes, 0, 0, None)
    }

    pub fn fallocate(
        &mut self,
        _req: &Request,
        _inode: Inode,
        fh: FileHandleId,
        offset: u64,
        length: u64,
        mode: i32,
    ) -> BkfsResult<()> {
        let fh = self
            .handle(fh)
            .ok_or(libc::EBADF)
            .map_err(io::Error::from_raw_os_error)?;
        if !fh.write {
            return BkfsResult::errno(libc::EACCES);
        }

        let mut contents = fh.contents.lock().unwrap();
        contents.fallocate(offset, length, mode, mode & libc::FALLOC_FL_KEEP_SIZE != 0)?;
        Ok(())
    }
}
