#![allow(clippy::needless_return)]
#![allow(clippy::unnecessary_cast)] // libc::S_* are u16 or u32 depending on the platform

use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::os::raw::c_int;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use fd_lock_rs::{FdLock, LockType};
use fuser::{
    AccessFlags, BsdFileFlags, CopyFileRangeFlags, Errno, FileHandle, Filesystem, FopenFlags,
    Generation, INodeNo, InitFlags, KernelConfig, LockOwner, OpenFlags, RenameFlags, ReplyAttr,
    ReplyCreate, ReplyData, ReplyDirectory, ReplyDirectoryPlus, ReplyEmpty, ReplyEntry, ReplyOpen,
    ReplyStatfs, ReplyWrite, ReplyXattr, Request, TimeOrNow, WriteFlags,
};
use log::{debug, error};

fn errno(e: c_int) -> Errno {
    Errno::from_i32(e)
}

use crate::ctrl::{Controller, StatFs};
use crate::directory::DirectoryContents;
use crate::error::BkfsResult;
use crate::handle::{FileHandleId, Handler};
use crate::inode::{FileData, Inode, InodeAttributes, BLOCK_SIZE};

mod aligned_io;
mod atomic_file;
mod blockstore;
mod compress;
mod contents;
mod ctrl;
mod directory;
mod ecc;
pub mod error;
mod handle;
mod inode;
mod pool;
mod seglog;
mod serde;
mod superblock;
#[cfg(test)]
mod tests;
#[allow(dead_code)]
mod util;
mod vault;

pub const FUSE_ROOT_ID: u64 = INodeNo::ROOT.0;

pub const MAX_NAME_LENGTH: u32 = 255;
// const MAX_FILE_SIZE: u64 = 1024 * 1024 * 1024 * 1024;
pub const ENTRY_TTL: Duration = Duration::new(3600, 0);

#[cfg(test)]
pub(crate) static SYNCFS_CALL_COUNT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[cfg(test)]
pub(crate) static FSYNCDIR_CALL_COUNT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

pub(crate) fn open_direct(
    path: &Path,
    create: bool,
) -> io::Result<aligned_io::BufferedDirectFile<File>> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut opts = File::options();
    opts.read(true).custom_flags(libc::O_DIRECT);
    if create {
        opts.write(true).create(true).truncate(true);
    }
    aligned_io::BufferedDirectFile::new(opts.open(path)?)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
}

#[cfg_attr(feature = "cli", derive(clap::Parser))]
pub struct BackupFSOptions {
    pub data_dir: PathBuf,
    #[cfg_attr(feature = "cli", arg(long))]
    pub setuid_support: bool,
    #[cfg_attr(feature = "cli", arg(long))]
    pub password: String,
    #[cfg_attr(feature = "cli", arg(long))]
    pub file_size_padding: Option<f64>,
    #[cfg_attr(feature = "cli", arg(short, long))]
    pub readonly: bool,
    /// True for the production mount, which start-core wraps in a kernel
    /// idmapped mount and mounts with default_permissions. Gates the
    /// FUSE_ALLOW_IDMAP request — asking for it on a plain mount (no
    /// default_permissions) aborts the FUSE connection.
    #[cfg_attr(feature = "cli", arg(skip))]
    pub idmapped: bool,
}

// Stores inode metadata data in "$data_dir/inodes" and file contents in "$data_dir/contents"
// Directory data is stored in the file's contents, as a serialized DirectoryDescriptor
pub struct BackupFS {
    lock: FdLock<File>,
    handler: Mutex<Handler>,
}

// The master key and all format-affecting constants live in the versioned
// superblock (`data_dir/superblock`); see [`crate::superblock`].

impl BackupFS {
    pub fn new(config: BackupFSOptions) -> BkfsResult<BackupFS> {
        let BackupFSOptions { data_dir, .. } = &config;
        let lock = fd_lock_rs::FdLock::lock(
            File::create(data_dir.join(".lock"))?,
            LockType::Exclusive,
            false,
        )
        .map_err(io::Error::from)?;

        let ctrl = Controller::new(config)?;

        if !ctrl.exists::<InodeAttributes>(Inode(FUSE_ROOT_ID)) {
            // Initialize with empty filesystem
            let root = InodeAttributes::new(
                Inode(FUSE_ROOT_ID),
                None,
                FileData::Directory(DirectoryContents::new()),
            );
            ctrl.save(&root)?;
        } else {
            ctrl.load::<InodeAttributes>(Inode(FUSE_ROOT_ID))?;
        }

        ctrl.load_inode_pool()?;

        Ok(BackupFS {
            lock,
            handler: Mutex::new(Handler::new(ctrl)),
        })
    }

    pub fn fsck(&mut self) -> BkfsResult<()> {
        self.handler.get_mut().unwrap().ctrl().fsck(false)
    }

    pub fn change_password(&mut self, password: &str) -> BkfsResult<()> {
        self.handler
            .get_mut()
            .unwrap()
            .ctrl()
            .change_password(password)
    }
}

impl Filesystem for BackupFS {
    fn init(&mut self, _req: &Request, config: &mut KernelConfig) -> io::Result<()> {
        config
            .add_capabilities(InitFlags::FUSE_HANDLE_KILLPRIV)
            .unwrap();
        // Only request idmap support on the production (kernel-idmapped,
        // default_permissions) mount — requesting it on a plain mount aborts
        // the FUSE connection. Tolerant: pre-6.12 kernels lack the cap.
        if self.handler.get_mut().unwrap().ctrl().config().idmapped {
            let _ = config.add_capabilities(InitFlags::FUSE_ALLOW_IDMAP);
        }

        log::info!("filesystem initialized");

        Ok(())
    }

    fn destroy(&mut self) {
        if let Err(e) = self.handler.get_mut().unwrap().close_all() {
            error!("error closing FS: {e}");
        }
        // Every individual AtomicFile::save already calls sync_all
        // before its rename, so data itself is durable. syncfs here is
        // a belt-and-braces flush of the whole backing filesystem —
        // ensures any last journal commits, CIFS writeback, etc. drain
        // before we release the data_dir lock and exit.
        use std::os::fd::AsRawFd;
        let fd = self.lock.as_raw_fd();
        // SAFETY: fd is a valid fd we own (held by the FdLock).
        if unsafe { libc::syncfs(fd) } != 0 {
            error!("syncfs on unmount failed: {}", io::Error::last_os_error());
        }
        // Persist the index checkpoint so the next mount skips the log replay.
        // After compaction (in close_all) + syncfs above, the on-disk segments
        // are final and durable, so the checkpoint matches them. Best-effort.
        self.handler.get_mut().unwrap().ctrl().save_checkpoint();
    }

    fn lookup(&self, req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        let mut h = self.handler.lock().unwrap();
        match h.lookup(req, Inode(parent.into()), name) {
            Ok(inode) => reply.entry(&ENTRY_TTL, &(&inode).into(), Generation(0)),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn forget(&self, _req: &Request, _ino: INodeNo, _nlookup: u64) {}

    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        let mut h = self.handler.lock().unwrap();
        match h.mutate_inode(Inode(ino.into()), |_, inode| Ok((&*inode).into())) {
            Ok(attr) => reply.attr(&ENTRY_TTL, &attr),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn setattr(
        &self,
        req: &Request,
        ino: INodeNo,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        ctime: Option<SystemTime>,
        fh: Option<FileHandle>,
        crtime: Option<SystemTime>,
        chgtime: Option<SystemTime>,
        bkuptime: Option<SystemTime>,
        flags: Option<BsdFileFlags>,
        reply: ReplyAttr,
    ) {
        let mut h = self.handler.lock().unwrap();
        match h.setattr(
            req,
            Inode(ino.into()),
            mode,
            uid,
            gid,
            size,
            atime,
            mtime,
            ctime,
            fh.map(|fh| FileHandleId(fh.into())),
            crtime,
            chgtime,
            bkuptime,
            flags.map(|f| f.bits()),
        ) {
            Ok(inode) => reply.attr(&ENTRY_TTL, &(&inode).into()),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn readlink(&self, req: &Request, ino: INodeNo, reply: ReplyData) {
        let mut h = self.handler.lock().unwrap();
        match h.readlink(req, Inode(ino.into())) {
            Ok(path) => reply.data(path.as_os_str().as_bytes()),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn mknod(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        umask: u32,
        rdev: u32,
        reply: ReplyEntry,
    ) {
        let mut h = self.handler.lock().unwrap();
        match h.mknod(
            req,
            Inode(parent.into()),
            name,
            mode,
            umask,
            rdev,
            None::<fn(Inode) -> FileData>,
        ) {
            Ok(inode) => reply.entry(&ENTRY_TTL, &(&inode).into(), Generation(0)),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn mkdir(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        umask: u32,
        reply: ReplyEntry,
    ) {
        self.mknod(req, parent, name, mode | libc::S_IFDIR, umask, 0, reply)
    }

    fn unlink(&self, req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        let mut h = self.handler.lock().unwrap();
        match h.unlink(req, Inode(parent.into()), name) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn rmdir(&self, req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        let mut h = self.handler.lock().unwrap();
        match h.unlink(req, Inode(parent.into()), name) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn symlink(
        &self,
        req: &Request,
        parent: INodeNo,
        link_name: &OsStr,
        target: &Path,
        reply: ReplyEntry,
    ) {
        let mut h = self.handler.lock().unwrap();
        match h.mknod(
            req,
            Inode(parent.into()),
            link_name,
            libc::S_IFLNK | 0o777,
            0,
            0,
            Some(|_| FileData::Symlink(target.to_owned())),
        ) {
            Ok(inode) => reply.entry(&ENTRY_TTL, &(&inode).into(), Generation(0)),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn rename(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        newparent: INodeNo,
        newname: &OsStr,
        flags: RenameFlags,
        reply: ReplyEmpty,
    ) {
        let mut h = self.handler.lock().unwrap();
        match h.rename(
            req,
            Inode(parent.into()),
            name,
            Inode(newparent.into()),
            newname,
            flags.contains(RenameFlags::RENAME_EXCHANGE),
        ) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn link(
        &self,
        req: &Request,
        ino: INodeNo,
        newparent: INodeNo,
        newname: &OsStr,
        reply: ReplyEntry,
    ) {
        debug!("link() called for {}, {}, {:?}", ino, newparent, newname);
        let mut h = self.handler.lock().unwrap();
        match h.link(
            req,
            Inode(ino.into()),
            Inode(newparent.into()),
            newname,
            None,
        ) {
            Ok(inode) => reply.entry(&ENTRY_TTL, &(&inode).into(), Generation(0)),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn open(&self, req: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        debug!("open() called for {:?}", ino);
        let mut h = self.handler.lock().unwrap();
        match h.open(req, Inode(ino.into()), flags.0) {
            Ok(FileHandleId(fh)) => reply.opened(FileHandle(fh), FopenFlags::FOPEN_DIRECT_IO),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn read(
        &self,
        _req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        size: u32,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyData,
    ) {
        debug!(
            "read() called on {:?} offset={:?} size={:?}",
            ino, offset, size
        );
        let handle = self
            .handler
            .lock()
            .unwrap()
            .handle(FileHandleId(fh.into()))
            .cloned();
        let Some(handle) = handle else {
            reply.error(errno(libc::EBADF));
            return;
        };
        if !handle.read {
            reply.error(errno(libc::EACCES));
            return;
        }
        // Offload the actual disk I/O — a stuck CIFS read must not
        // block unrelated FUSE ops from other clients. Route by inode
        // so reads and writes on the same file stay in order.
        let key = handle.inode.0;
        pool::global().submit_for(key, move || {
            let mut contents = handle.contents.lock().unwrap();
            let available = contents.inode.attrs.size.saturating_sub(offset) as usize;
            let size = (size as usize).min(available);
            let mut buf = vec![0_u8; size];
            match contents.read_exact_at(&mut buf, offset) {
                Ok(()) => reply.data(&buf),
                Err(e) => reply.error(errno(e.to_errno_log())),
            }
        });
    }

    fn write(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        data: &[u8],
        write_flags: WriteFlags,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        let handle = self
            .handler
            .lock()
            .unwrap()
            .handle(FileHandleId(fh.into()))
            .cloned();
        let Some(handle) = handle else {
            reply.error(errno(libc::EBADF));
            return;
        };
        if !handle.write {
            reply.error(errno(libc::EACCES));
            return;
        }
        // Copy the bytes now (the kernel's buffer is reused after
        // return) and move ownership into the worker.
        let mut buf = data.to_vec();
        let key = handle.inode.0;
        pool::global().submit_for(key, move || {
            let mut contents = handle.contents.lock().unwrap();
            match contents.write_all_at(&mut buf, offset) {
                Ok(()) => {
                    if write_flags.contains(WriteFlags::FUSE_WRITE_KILL_SUIDGID) {
                        contents.inode.attrs.clear_suid_sgid();
                    }
                    reply.written(buf.len() as u32);
                }
                Err(e) => reply.error(errno(e.to_errno_log())),
            }
        });
    }

    fn flush(
        &self,
        _req: &Request,
        _ino: INodeNo,
        _fh: FileHandle,
        _lock_owner: LockOwner,
        reply: ReplyEmpty,
    ) {
        reply.ok()
    }

    fn release(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        let mut h = self.handler.lock().unwrap();
        match h.fclose(FileHandleId(fh.into())) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn fsync(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        let handle = self
            .handler
            .lock()
            .unwrap()
            .handle(FileHandleId(fh.into()))
            .cloned();
        let Some(handle) = handle else {
            reply.error(errno(libc::EBADF));
            return;
        };
        // fsync drives the content-file atomic save + inode save —
        // both hit the backing store and can stall on CIFS / slow USB.
        // Route by inode so fsync observes prior write jobs for the
        // same file (shard FIFO order).
        let key = handle.inode.0;
        pool::global().submit_for(key, move || match handle.contents.lock().unwrap().fsync() {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(errno(e.to_errno_log())),
        });
    }

    fn opendir(&self, req: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        debug!("opendir() called on {:?}", ino);
        let mut h = self.handler.lock().unwrap();
        match h.opendir(req, Inode(ino.into()), flags.0) {
            Ok(FileHandleId(fh)) => reply.opened(FileHandle(fh), FopenFlags::FOPEN_DIRECT_IO),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn readdir(
        &self,
        req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectory,
    ) {
        let mut h = self.handler.lock().unwrap();
        match h.readdir(
            req,
            Inode(ino.into()),
            FileHandleId(fh.into()),
            offset as i64,
            |_, name, entry, offset| {
                Ok(reply.add(INodeNo(entry.inode.0), offset as u64, entry.ty, name))
            },
        ) {
            Ok(done) => {
                if done {
                    // todo!();
                }
                reply.ok()
            }
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn readdirplus(
        &self,
        req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectoryPlus,
    ) {
        let mut h = self.handler.lock().unwrap();
        match h.readdir(
            req,
            Inode(ino.into()),
            FileHandleId(fh.into()),
            offset as i64,
            |handler, name, entry, offset| {
                match handler.mutate_inode(entry.inode, |_, inode| {
                    Ok(reply.add(
                        INodeNo(inode.inode.0),
                        offset as u64,
                        name,
                        &ENTRY_TTL,
                        &(&*inode).into(),
                        Generation(0),
                    ))
                }) {
                    Ok(full) => Ok(full),
                    // The opendir snapshot can name a child that was unlinked
                    // (and gc'd) since: skip the stale entry rather than
                    // failing the whole listing with EIO.
                    Err(e) if e.to_errno() == libc::ENOENT => Ok(false),
                    Err(e) => Err(e),
                }
            },
        ) {
            Ok(done) => {
                if done {
                    // todo!();
                }
                reply.ok()
            }
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn releasedir(
        &self,
        req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        flags: OpenFlags,
        reply: ReplyEmpty,
    ) {
        let mut h = self.handler.lock().unwrap();
        match h.releasedir(req, Inode(ino.into()), FileHandleId(fh.into()), flags.0) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn fsyncdir(
        &self,
        _req: &Request,
        _ino: INodeNo,
        _fh: FileHandle,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        // Callers like start-os need a way to force a whole-fs
        // durability checkpoint before `umount -l`. FUSE_SYNCFS would
        // be the natural hook, but the Linux kernel's `fc->sync_fs`
        // defaults to 0 for non-bdev FUSE mounts and is never enabled
        // via any INIT flag — so `syncfs(2)` / `sync -f` silently does
        // a VFS-level sync and never dispatches to us. FUSE_FSYNCDIR,
        // however, reaches the daemon reliably (`fsync(dirfd)` /
        // `fsync .`). Route it to the same whole-fs flush so callers
        // have a working checkpoint.
        #[cfg(test)]
        FSYNCDIR_CALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut h = self.handler.lock().unwrap();
        match h.flush_all_dirty() {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn syncfs(&self, _req: &Request, reply: ReplyEmpty) {
        // `sync -f <mountpoint>` / syncfs(2). flush_all_dirty drains
        // both the dirty inode cache and every open Contents with
        // per-file sync_all, so the checkpoint is on stable storage
        // before we return — the caller can lazy-unmount afterwards
        // without losing anything that was written before sync -f.
        #[cfg(test)]
        SYNCFS_CALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut h = self.handler.lock().unwrap();
        match h.flush_all_dirty() {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn statfs(&self, _req: &Request, _ino: INodeNo, reply: ReplyStatfs) {
        let StatFs { files, ffree } = self.handler.lock().unwrap().ctrl().statfs();
        // TODO: real implementation of this
        reply.statfs(
            10_000,
            10_000,
            10_000,
            files,
            ffree,
            BLOCK_SIZE as u32,
            MAX_NAME_LENGTH,
            BLOCK_SIZE as u32,
        );
    }

    fn setxattr(
        &self,
        request: &Request,
        ino: INodeNo,
        name: &OsStr,
        value: &[u8],
        _flags: i32,
        _position: u32,
        reply: ReplyEmpty,
    ) {
        let mut h = self.handler.lock().unwrap();
        match h.setxattr(request, Inode(ino.into()), name.as_bytes(), value) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn getxattr(
        &self,
        request: &Request,
        ino: INodeNo,
        name: &OsStr,
        size: u32,
        reply: ReplyXattr,
    ) {
        let h = self.handler.lock().unwrap();
        match h.getxattr(request, Inode(ino.into()), name.as_bytes()) {
            Ok(data) => {
                if size == 0 {
                    reply.size(data.len() as u32);
                } else if data.len() <= size as usize {
                    reply.data(&data);
                } else {
                    reply.error(errno(libc::ERANGE))
                }
            }
            Err(e) => reply.error(errno(e.to_errno())),
        }
    }

    fn listxattr(&self, request: &Request, ino: INodeNo, size: u32, reply: ReplyXattr) {
        let h = self.handler.lock().unwrap();
        match h.listxattr(request, Inode(ino.into())) {
            Ok(attrs) => {
                let mut bytes = vec![];
                // Convert to concatenated null-terminated strings
                for (key, _) in attrs {
                    bytes.extend(key);
                    bytes.push(0);
                }
                if size == 0 {
                    reply.size(bytes.len() as u32);
                } else if bytes.len() <= size as usize {
                    reply.data(&bytes);
                } else {
                    reply.error(errno(libc::ERANGE));
                }
            }
            Err(_) => reply.error(errno(libc::EBADF)),
        }
    }

    fn removexattr(&self, request: &Request, ino: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        let mut h = self.handler.lock().unwrap();
        match h.removexattr(request, Inode(ino.into()), name.as_bytes()) {
            Ok(_) => reply.ok(),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn access(&self, _req: &Request, _ino: INodeNo, _mask: AccessFlags, reply: ReplyEmpty) {
        reply.ok()
    }

    fn create(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        umask: u32,
        flags: i32,
        reply: ReplyCreate,
    ) {
        let mut h = self.handler.lock().unwrap();
        match h.create(req, Inode(parent.into()), name, mode, umask, flags) {
            Ok((attrs, handle)) => reply.created(
                &Duration::new(0, 0),
                &(&attrs).into(),
                Generation(0),
                FileHandle(handle.0),
                FopenFlags::empty(),
            ),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn fallocate(
        &self,
        req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        length: u64,
        mode: i32,
        reply: ReplyEmpty,
    ) {
        let mut h = self.handler.lock().unwrap();
        match h.fallocate(
            req,
            Inode(ino.into()),
            FileHandleId(fh.into()),
            offset,
            length,
            mode,
        ) {
            Ok(_) => reply.ok(),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }

    fn copy_file_range(
        &self,
        req: &Request,
        ino_in: INodeNo,
        fh_in: FileHandle,
        offset_in: u64,
        ino_out: INodeNo,
        fh_out: FileHandle,
        offset_out: u64,
        len: u64,
        flags: CopyFileRangeFlags,
        reply: ReplyWrite,
    ) {
        let mut h = self.handler.lock().unwrap();
        match h.copy_file_range(
            req,
            Inode(ino_in.into()),
            FileHandleId(fh_in.into()),
            offset_in,
            Inode(ino_out.into()),
            FileHandleId(fh_out.into()),
            offset_out,
            len as usize,
            flags.bits() as u32,
        ) {
            Ok(written) => reply.written(written as u32),
            Err(e) => reply.error(errno(e.to_errno_log())),
        }
    }
}

/*
fn as_file_kind(mut mode: u32) -> FileKind {
    mode &= libc::S_IFMT as u32;

    if mode == libc::S_IFREG as u32 {
        return FileKind::File;
    } else if mode == libc::S_IFLNK as u32 {
        return FileKind::Symlink;
    } else if mode == libc::S_IFDIR as u32 {
        return FileKind::Directory;
    } else {
        unimplemented!("{}", mode);
    }
}
*/

pub fn fuse_allow_other_enabled() -> BkfsResult<bool> {
    let file = File::open("/etc/fuse.conf")?;
    for line in BufReader::new(file).lines() {
        if line?.trim_start().starts_with("user_allow_other") {
            return Ok(true);
        }
    }
    Ok(false)
}
