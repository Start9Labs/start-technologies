use std::ffi::{OsStr, OsString};
use std::io;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fuser::{Request, TimeOrNow};

use crate::FUSE_ROOT_ID;
use imbl::{OrdMap, OrdSet};
use log::debug;
use serde::{Deserialize, Serialize};

use crate::ctrl::{Controller, Exists, Load, Save};
use crate::directory::DirectoryContents;
use crate::error::{BkfsResult, BkfsResultExt};
use crate::handle::{FileHandleId, Handler};
use crate::{get_groups, IdMappedRoot};

pub const BLOCK_SIZE: u64 = 4096;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct Inode(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct ContentId(pub u64);

impl From<Inode> for ContentId {
    fn from(value: Inode) -> Self {
        Self(value.0)
    }
}

/// On-disk file-content descriptor.
///
/// **Append-only: never reorder or remove variants.** The serialized form is a
/// bincode variant index (declaration order); reordering would silently
/// reinterpret every existing inode (a stored `Packed` would decode as
/// `Directory`, etc.). New variants must be appended, and any unavoidable
/// reorder requires a `superblock::FORMAT_VERSION` bump.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FileData {
    File(ContentId),
    /// A small regular file whose entire content is stored inline in this
    /// inode record (so a tiny file needs no separate content block file).
    /// Files larger than the inline threshold use `File` + block storage.
    Inline(Vec<u8>),
    /// A small/medium regular file (≤ one chunk) whose content is a single
    /// extent packed into the shared content log, addressed by this id. The
    /// bytes live in the log, not here — only the pointer does.
    Packed(ContentId),
    Directory(DirectoryContents),
    Symlink(PathBuf),
    /// Character special file; payload is the device number (`rdev`).
    CharDevice(u32),
    /// Block special file; payload is the device number (`rdev`).
    BlockDevice(u32),
    Fifo,
    Socket,
}
impl FileData {
    fn nlink(&self) -> usize {
        if let Self::Directory(c) = self {
            c.nlink()
        } else {
            0
        }
    }
    pub fn is_dir(&self) -> bool {
        matches!(self, Self::Directory(_))
    }
    pub fn is_file(&self) -> bool {
        matches!(self, Self::File(_) | Self::Inline(_) | Self::Packed(_))
    }
    /// Device number for special files, `0` otherwise.
    pub fn rdev(&self) -> u32 {
        match self {
            Self::CharDevice(rdev) | Self::BlockDevice(rdev) => *rdev,
            _ => 0,
        }
    }
}

impl From<&FileData> for fuser::FileType {
    fn from(kind: &FileData) -> Self {
        match kind {
            FileData::File(_) | FileData::Inline(_) | FileData::Packed(_) => {
                fuser::FileType::RegularFile
            }
            FileData::Directory(_) => fuser::FileType::Directory,
            FileData::Symlink(_) => fuser::FileType::Symlink,
            FileData::CharDevice(_) => fuser::FileType::CharDevice,
            FileData::BlockDevice(_) => fuser::FileType::BlockDevice,
            FileData::Fifo => fuser::FileType::NamedPipe,
            FileData::Socket => fuser::FileType::Socket,
        }
    }
}

#[derive(Debug)]
enum XattrNamespace {
    Security,
    System,
    Trusted,
    User,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attributes {
    pub size: u64,
    pub crtime: (i64, u32),
    pub atime: (i64, u32),
    pub mtime: (i64, u32),
    pub ctime: (i64, u32),
    pub contents: FileData,
    // Permissions and special mode bits
    pub mode: u16,
    pub parents: OrdSet<(Inode, OsString)>,
    pub uid: u32,
    pub gid: u32,
    pub xattrs: OrdMap<Vec<u8>, Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct InodeAttributes {
    pub inode: Inode,
    pub attrs: Attributes,
}

impl InodeAttributes {
    pub fn new(inode: Inode, parent: Option<(Inode, OsString)>, contents: FileData) -> Self {
        let now = time_now();
        Self {
            inode,
            attrs: Attributes {
                size: 0,
                crtime: now,
                atime: now,
                mtime: now,
                ctime: now,
                contents,
                mode: 0o777,
                parents: parent.into_iter().collect(),
                uid: 0,
                gid: 0,
                xattrs: Default::default(),
            },
        }
    }

    pub fn lookup(&self, ctrl: &Controller, name: &OsStr) -> BkfsResult<Inode> {
        let FileData::Directory(dir) = &self.attrs.contents else {
            return BkfsResult::errno(libc::ENOTDIR);
        };

        if name == OsStr::new(".") {
            return Ok(self.inode);
        } else if name == OsStr::new("..") {
            return Ok(self
                .attrs
                .parents
                .get_min()
                .map(|(p, _)| *p)
                .unwrap_or(self.inode));
        }

        match dir.get(ctrl, self.inode, name)? {
            Some(entry) => Ok(entry.inode),
            None => BkfsResult::errno_notrace(libc::ENOENT),
        }
    }
}

fn system_time_from_time(secs: i64, nsecs: u32) -> SystemTime {
    if secs >= 0 {
        UNIX_EPOCH + Duration::new(secs as u64, nsecs)
    } else {
        UNIX_EPOCH - Duration::new((-secs) as u64, nsecs)
    }
}

pub fn time_now() -> (i64, u32) {
    time_from_system_time(&SystemTime::now())
}

pub fn time_from_system_time(system_time: &SystemTime) -> (i64, u32) {
    // Convert to signed 64-bit time with epoch at 0
    match system_time.duration_since(UNIX_EPOCH) {
        Ok(duration) => (duration.as_secs() as i64, duration.subsec_nanos()),
        Err(before_epoch_error) => (
            -(before_epoch_error.duration().as_secs() as i64),
            before_epoch_error.duration().subsec_nanos(),
        ),
    }
}

impl From<&InodeAttributes> for fuser::FileAttr {
    fn from(InodeAttributes { inode, attrs, .. }: &InodeAttributes) -> Self {
        fuser::FileAttr {
            ino: fuser::INodeNo(inode.0),
            size: attrs.size,
            // st_blocks is reported in 512-byte units per POSIX, independent of blksize
            blocks: attrs.size.div_ceil(512),
            atime: system_time_from_time(attrs.atime.0, attrs.atime.1),
            mtime: system_time_from_time(attrs.mtime.0, attrs.mtime.1),
            ctime: system_time_from_time(attrs.ctime.0, attrs.ctime.1),
            crtime: system_time_from_time(attrs.crtime.0, attrs.crtime.1),
            kind: (&attrs.contents).into(),
            perm: attrs.mode,
            nlink: if inode.0 == FUSE_ROOT_ID {
                2
            } else {
                (attrs.parents.len() + attrs.contents.nlink()) as u32
            },
            uid: attrs.uid,
            gid: attrs.gid,
            rdev: attrs.contents.rdev(),
            blksize: BLOCK_SIZE as u32,
            flags: 0,
        }
    }
}

fn parse_xattr_namespace(key: &[u8]) -> BkfsResult<XattrNamespace> {
    let user = b"user.";
    if key.len() < user.len() {
        return BkfsResult::errno(libc::ENOTSUP);
    }
    if key[..user.len()].eq(user) {
        return Ok(XattrNamespace::User);
    }

    let system = b"system.";
    if key.len() < system.len() {
        return BkfsResult::errno(libc::ENOTSUP);
    }
    if key[..system.len()].eq(system) {
        return Ok(XattrNamespace::System);
    }

    let trusted = b"trusted.";
    if key.len() < trusted.len() {
        return BkfsResult::errno(libc::ENOTSUP);
    }
    if key[..trusted.len()].eq(trusted) {
        return Ok(XattrNamespace::Trusted);
    }

    let security = b"security";
    if key.len() < security.len() {
        return BkfsResult::errno(libc::ENOTSUP);
    }
    if key[..security.len()].eq(security) {
        return Ok(XattrNamespace::Security);
    }

    return BkfsResult::errno(libc::ENOTSUP);
}

impl<'a> Save for &'a InodeAttributes {
    fn save(self, ctrl: &Controller) -> BkfsResult<()> {
        ctrl.log_put(self.inode, &self.attrs, true)
    }
    fn save_fast(self, ctrl: &Controller) -> BkfsResult<()> {
        ctrl.log_put(self.inode, &self.attrs, false)
    }
}

impl Load for InodeAttributes {
    type Args<'a> = Inode;
    fn load(ctrl: &Controller, inode: Self::Args<'_>) -> BkfsResult<Self> {
        match ctrl.log_load(inode)? {
            Some(attrs) => Ok(InodeAttributes { inode, attrs }),
            // Absent from the index = never written or tombstoned. Surface
            // as NotFound so the stale-parent self-heal paths fire.
            None => BkfsResult::errno_notrace(libc::ENOENT),
        }
    }
}

impl Exists for InodeAttributes {
    fn exists(ctrl: &Controller, inode: Self::Args<'_>) -> bool {
        ctrl.log_contains(inode)
    }
}

impl Attributes {
    pub fn setattr(
        &mut self,
        handler: &mut Handler,
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
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
    ) -> BkfsResult<bool> {
        let mut changed = false;
        let mut now_cell = None;
        let mut lazy_now = || *now_cell.get_or_insert(time_now());

        if let Some(mode) = mode {
            debug!("chmod() called with {:?}, {:o}", inode, mode);
            if !self.is_root_for(&handler.ctrl().config().idmapped_root, req.uid(), [])
                && req.uid() != self.uid
            {
                return BkfsResult::errno(libc::EPERM);
            }
            if !self.is_root_for(&handler.ctrl().config().idmapped_root, req.uid(), [])
                && req.gid() != self.gid
                && !get_groups(req.pid()).contains(&self.gid)
            {
                // If SGID is set and the file belongs to a group that the caller is not part of
                // then the SGID bit is suppose to be cleared during chmod
                self.mode = (mode & !libc::S_ISGID as u32) as u16;
            } else {
                self.mode = mode as u16;
            }
            changed = true;
        }

        if uid.is_some() || gid.is_some() {
            debug!("chown() called with {:?} {:?} {:?}", inode, uid, gid);
            if let Some(gid) = gid {
                // Non-root users can only change gid to a group they're in
                // Only owner may change the group
                if !self.is_root_for(&handler.ctrl().config().idmapped_root, req.uid(), [gid])
                    && (!get_groups(req.pid()).contains(&gid) || req.uid() != self.uid)
                {
                    return BkfsResult::errno(libc::EPERM);
                }
            }
            if let Some(uid) = uid {
                if !self.is_root_for(&handler.ctrl().config().idmapped_root, req.uid(), [uid])
                        // but no-op changes by the owner are not an error
                        && !(uid == self.uid && req.uid() == self.uid)
                {
                    return BkfsResult::errno(libc::EPERM);
                }
            }

            if self.mode & (libc::S_IXUSR | libc::S_IXGRP | libc::S_IXOTH) as u16 != 0 {
                // SUID & SGID are suppose to be cleared when chown'ing an executable file
                self.clear_suid_sgid();
            }

            if let Some(uid) = uid {
                self.uid = uid;
                // Clear SETUID on owner change
                self.mode &= !libc::S_ISUID as u16;
            }
            if let Some(gid) = gid {
                self.gid = gid;
                // Clear SETGID unless user is root
                if !self.is_root_for(&handler.ctrl().config().idmapped_root, req.uid(), []) {
                    self.mode &= !libc::S_ISGID as u16;
                }
            }
            changed = true;
        }

        if let Some(size) = size {
            debug!("truncate() called with {:?} {:?}", inode, size);
            if let Some(fh) = fh {
                // If the file handle is available, check access locally.
                // This is important as it preserves the semantic that a file handle opened
                // with W_OK will never fail to truncate, even if the file has been subsequently
                // chmod'ed
                if !handler
                    .handle(fh)
                    .ok_or(libc::EACCES)
                    .map_err(io::Error::from_raw_os_error)?
                    .write
                {
                    return BkfsResult::errno(libc::EACCES);
                }
            } else {
                self.check_access(
                    &handler.ctrl().config().idmapped_root,
                    req.uid(),
                    req.gid(),
                    libc::W_OK,
                )?;
            }
            self.size = size;
            changed = true;
        }

        if let Some(atime) = atime {
            debug!("utimens() called with {:?}, atime={:?}", inode, atime);

            if self.uid != req.uid()
                && !self.is_root_for(&handler.ctrl().config().idmapped_root, req.uid(), [])
                && atime != TimeOrNow::Now
            {
                return BkfsResult::errno(libc::EPERM);
            }

            if self.uid != req.uid() {
                self.check_access(
                    &handler.ctrl().config().idmapped_root,
                    req.uid(),
                    req.gid(),
                    libc::W_OK,
                )?;
            }

            self.atime = match atime {
                TimeOrNow::SpecificTime(time) => time_from_system_time(&time),
                TimeOrNow::Now => lazy_now(),
            };
            changed = true;
        }

        if let Some(mtime) = mtime {
            debug!("utimens() called with {:?}, mtime={:?}", inode, mtime);

            if self.uid != req.uid()
                && !self.is_root_for(&handler.ctrl().config().idmapped_root, req.uid(), [])
                && mtime != TimeOrNow::Now
            {
                return BkfsResult::errno(libc::EPERM);
            }

            if self.uid != req.uid() {
                self.check_access(
                    &handler.ctrl().config().idmapped_root,
                    req.uid(),
                    req.gid(),
                    libc::W_OK,
                )?;
            }

            self.mtime = match mtime {
                TimeOrNow::SpecificTime(time) => time_from_system_time(&time),
                TimeOrNow::Now => lazy_now(),
            };
            changed = true;
        }

        if let Some(ctime) = ctime {
            debug!("utimens() called with {:?}, ctime={:?}", inode, ctime);

            if self.uid != req.uid()
                && !self.is_root_for(&handler.ctrl().config().idmapped_root, req.uid(), [])
            {
                return BkfsResult::errno(libc::EPERM);
            }

            self.ctime = time_from_system_time(&ctime);
            changed = true;
        }

        if let Some(crtime) = crtime {
            debug!("utimens() called with {:?}, crtime={:?}", inode, crtime);

            if self.uid != req.uid()
                && !self.is_root_for(&handler.ctrl().config().idmapped_root, req.uid(), [])
            {
                return BkfsResult::errno(libc::EPERM);
            }

            self.crtime = time_from_system_time(&crtime);
            changed = true;
        }

        if changed && ctime.is_none() {
            self.ctime = lazy_now();
        }

        Ok(changed)
    }

    pub fn is_root_for(
        &self,
        idmap: &[IdMappedRoot],
        uid: u32,
        uids: impl IntoIterator<Item = u32> + Clone,
    ) -> bool {
        uid == 0
            || idmap
                .into_iter()
                .any(|idmap| idmap.is_root_for(uid, [self.uid].into_iter().chain(uids.clone())))
    }

    pub fn check_access(
        &self,
        idmap: &[IdMappedRoot],
        uid: u32,
        gid: u32,
        mut access_mask: i32,
    ) -> BkfsResult<()> {
        // F_OK tests for existence of file
        if access_mask == libc::F_OK {
            return Ok(());
        }
        let file_mode = i32::from(self.mode);

        // root is allowed to read & write anything
        if self.is_root_for(idmap, uid, []) {
            // root only allowed to exec if one of the X bits is set
            access_mask &= libc::X_OK;
            access_mask -= access_mask & (file_mode >> 6);
            access_mask -= access_mask & (file_mode >> 3);
            access_mask -= access_mask & file_mode;
            return if access_mask == 0 {
                Ok(())
            } else {
                BkfsResult::errno(libc::EACCES)
            };
        }

        if uid == self.uid {
            access_mask -= access_mask & (file_mode >> 6);
        } else if gid == self.gid {
            access_mask -= access_mask & (file_mode >> 3);
        } else {
            access_mask -= access_mask & file_mode;
        }

        if access_mask == 0 {
            Ok(())
        } else {
            BkfsResult::errno(libc::EACCES)
        }
    }

    pub fn check_sticky(&self, child: &Self, uid: u32) -> BkfsResult<()> {
        if self.mode & libc::S_ISVTX as u16 != 0 && uid != 0 && uid != self.uid && uid != child.uid
        {
            BkfsResult::errno(libc::EACCES)
        } else {
            Ok(())
        }
    }

    pub fn clear_suid_sgid(&mut self) {
        self.mode &= !libc::S_ISUID as u16;
        // SGID is only suppose to be cleared if XGRP is set
        if self.mode & libc::S_IXGRP as u16 != 0 {
            self.mode &= !libc::S_ISGID as u16;
        }
    }

    pub fn creation_gid(&self, gid: u32) -> u32 {
        if self.mode & libc::S_ISGID as u16 != 0 {
            return self.gid;
        }

        gid
    }

    pub fn xattr_access_check(
        &self,
        idmap: &[IdMappedRoot],
        key: &[u8],
        value: Option<Option<&[u8]>>,
        request: &Request,
    ) -> BkfsResult<()> {
        match parse_xattr_namespace(key)? {
            XattrNamespace::Security => {
                if value.is_some() && request.uid() != 0 {
                    return BkfsResult::errno(libc::EPERM);
                }
            }
            XattrNamespace::Trusted => {
                if request.uid() != 0 {
                    return BkfsResult::errno(libc::EPERM);
                }
            }
            XattrNamespace::System => {
                if value.is_some() {
                    if key.eq(b"system.posix_acl_access")
                        && self.uid != request.uid()
                        && !self.is_root_for(idmap, request.uid(), [])
                    {
                        return BkfsResult::errno(libc::EPERM);
                    } else if key.eq(b"system.posix_acl_access")
                        && request.uid() != 0
                        && !self.is_root_for(idmap, request.uid(), [])
                    {
                        return BkfsResult::errno(libc::EPERM);
                    } else if request.uid() != 0 {
                        return BkfsResult::errno(libc::EPERM);
                    }
                }
            }
            XattrNamespace::User => {
                self.check_access(
                    idmap,
                    request.uid(),
                    request.gid(),
                    if value.is_some() {
                        libc::W_OK
                    } else {
                        libc::R_OK
                    },
                )
                .map_err(|_| io::Error::from_raw_os_error(libc::EPERM))?;
            }
        }

        Ok(())
    }

    pub fn modified(&mut self) {
        let now = time_now();
        self.mtime = now;
        self.ctime = now;
    }

    pub fn changed(&mut self) {
        self.ctime = time_now();
    }
}
