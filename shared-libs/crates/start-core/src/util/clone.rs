use std::path::Path;
use std::sync::Arc;

use crate::prelude::*;
use crate::util::io::Counter;

/// Recursively clones `src` into `dst` using reflinks exclusively — no file
/// data is ever byte-copied, so the clone consumes no data space and fails
/// rather than degrade on a filesystem without reflink support. Unlike
/// `cp --reflink=always`, nodatacow (chattr +C) files are cloned by applying
/// the flag to the destination first (btrfs refuses clones between datacow and
/// nodatacow inodes). `dst` may already exist as an empty directory or
/// subvolume; its metadata is overwritten from `src`. Progress is reported to
/// `ctr` in bytes, ticking per 1 GiB chunk within large files. Not atomic
/// against concurrent writers: a file written mid-clone may be captured torn,
/// and one truncated mid-clone fails the clone (callers degrade to
/// no-backup) — same exposure as the `cp --reflink` this replaces.
#[cfg(target_os = "linux")]
pub async fn clone_tree(
    src: impl AsRef<Path>,
    dst: impl AsRef<Path>,
    ctr: Arc<Counter>,
) -> Result<(), Error> {
    let src = src.as_ref().to_owned();
    let dst = dst.as_ref().to_owned();
    tokio::task::spawn_blocking(move || {
        linux::Cloner {
            ctr,
            hardlinks: std::collections::HashMap::new(),
        }
        .clone_root(&src, &dst)
        .map_err(|e| {
            Error::new(
                eyre!("cloning {src:?} -> {dst:?}: {e}"),
                ErrorKind::Filesystem,
            )
        })
    })
    .await
    .with_kind(ErrorKind::Unknown)?
}

#[cfg(not(target_os = "linux"))]
pub async fn clone_tree(
    _src: impl AsRef<Path>,
    _dst: impl AsRef<Path>,
    _ctr: Arc<Counter>,
) -> Result<(), Error> {
    Err(Error::new(
        eyre!("reflink clone is not supported on this platform"),
        ErrorKind::Filesystem,
    ))
}

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::HashMap;
    use std::fs::{File, OpenOptions};
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{
        FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt, fchown, lchown,
    };
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use crate::util::io::Counter;

    /// Clone granularity for regular files. Must be a multiple of the
    /// filesystem block size so every chunk boundary is FICLONERANGE-aligned.
    const CHUNK: u64 = 1 << 30;

    /// FS_NOCOW_FL from linux/fs.h; libc does not export the FS_*_FL family.
    const FS_NOCOW_FL: libc::c_long = 0x0080_0000;

    pub struct Cloner {
        pub ctr: Arc<Counter>,
        pub hardlinks: HashMap<(u64, u64), PathBuf>,
    }

    impl Cloner {
        pub fn clone_root(&mut self, src: &Path, dst: &Path) -> std::io::Result<()> {
            let meta = std::fs::symlink_metadata(src)?;
            if !dst.is_dir() {
                std::fs::create_dir(dst)?;
            }
            copy_dir_flags(src, dst)?;
            copy_xattrs_nofollow(src, dst)?;
            self.clone_dir_contents(src, dst)?;
            apply_dir_metadata(dst, &meta)
        }

        fn clone_dir_contents(&mut self, src: &Path, dst: &Path) -> std::io::Result<()> {
            for entry in std::fs::read_dir(src)? {
                let entry = entry?;
                let src_path = entry.path();
                let dst_path = dst.join(entry.file_name());
                let meta = std::fs::symlink_metadata(&src_path)?;
                let ft = meta.file_type();
                if ft.is_dir() {
                    std::fs::create_dir(&dst_path)?;
                    copy_dir_flags(&src_path, &dst_path)?;
                    copy_xattrs_nofollow(&src_path, &dst_path)?;
                    self.clone_dir_contents(&src_path, &dst_path)?;
                    apply_dir_metadata(&dst_path, &meta)?;
                } else if ft.is_file() {
                    if meta.nlink() > 1 {
                        if let Some(existing) = self.hardlinks.get(&(meta.dev(), meta.ino())) {
                            std::fs::hard_link(existing, &dst_path)?;
                            continue;
                        }
                        self.hardlinks
                            .insert((meta.dev(), meta.ino()), dst_path.clone());
                    }
                    self.clone_file(&src_path, &dst_path, &meta)?;
                } else if ft.is_symlink() {
                    let target = std::fs::read_link(&src_path)?;
                    std::os::unix::fs::symlink(&target, &dst_path)?;
                    copy_xattrs_nofollow(&src_path, &dst_path)?;
                    lchown(&dst_path, Some(meta.uid()), Some(meta.gid()))?;
                    set_times_nofollow(&dst_path, &meta)?;
                } else if ft.is_fifo() {
                    let dst_c = cpath(&dst_path)?;
                    if unsafe { libc::mkfifo(dst_c.as_ptr(), meta.mode() as libc::mode_t) } != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    copy_xattrs_nofollow(&src_path, &dst_path)?;
                    lchown(&dst_path, Some(meta.uid()), Some(meta.gid()))?;
                    // mkfifo's mode is masked by umask
                    std::fs::set_permissions(
                        &dst_path,
                        PermissionsExt::from_mode(meta.mode() & 0o7777),
                    )?;
                    set_times_nofollow(&dst_path, &meta)?;
                } else {
                    tracing::warn!("clone_tree: skipping special file {src_path:?}");
                }
            }
            Ok(())
        }

        fn clone_file(
            &mut self,
            src: &Path,
            dst: &Path,
            meta: &std::fs::Metadata,
        ) -> std::io::Result<()> {
            let src_f = File::open(src)?;
            let dst_f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(meta.mode() & 0o777)
                .open(dst)?;
            // Flags must match before cloning: btrfs returns EINVAL for a
            // clone between nodatacow and datacow inodes, and +C only sticks
            // while the destination has no extents. Mirror exactly — the
            // destination may have inherited +C from its parent dir even when
            // the source file is datacow.
            mirror_nocow(&src_f, &dst_f)?;
            let size = meta.len();
            let mut offset = 0u64;
            while offset < size {
                let len = CHUNK.min(size - offset);
                clone_range(&src_f, &dst_f, offset, len)?;
                self.ctr.add(len);
                offset += len;
            }
            copy_xattrs(&src_f, &dst_f)?;
            fchown(&dst_f, Some(meta.uid()), Some(meta.gid()))?;
            dst_f.set_permissions(PermissionsExt::from_mode(meta.mode() & 0o7777))?;
            set_times(&dst_f, meta)
        }
    }

    fn clone_range(src: &File, dst: &File, offset: u64, len: u64) -> std::io::Result<()> {
        let range = libc::file_clone_range {
            src_fd: src.as_raw_fd() as i64,
            src_offset: offset,
            src_length: len,
            dest_offset: offset,
        };
        if unsafe { libc::ioctl(dst.as_raw_fd(), libc::FICLONERANGE as _, &range) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    fn get_fs_flags(f: &File) -> std::io::Result<libc::c_long> {
        let mut flags: libc::c_long = 0;
        if unsafe { libc::ioctl(f.as_raw_fd(), libc::FS_IOC_GETFLAGS as _, &mut flags) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(flags)
    }

    fn mirror_nocow(src: &File, dst: &File) -> std::io::Result<()> {
        let src_nocow = get_fs_flags(src)? & FS_NOCOW_FL != 0;
        let dst_flags = get_fs_flags(dst)?;
        if src_nocow != (dst_flags & FS_NOCOW_FL != 0) {
            let flags = if src_nocow {
                dst_flags | FS_NOCOW_FL
            } else {
                dst_flags & !FS_NOCOW_FL
            };
            if unsafe { libc::ioctl(dst.as_raw_fd(), libc::FS_IOC_SETFLAGS as _, &flags) } != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    }

    /// Mirrors +C on a directory so files created inside inherit it, matching
    /// the source's inheritance behavior.
    fn copy_dir_flags(src: &Path, dst: &Path) -> std::io::Result<()> {
        mirror_nocow(&File::open(src)?, &File::open(dst)?)
    }

    fn apply_dir_metadata(dst: &Path, meta: &std::fs::Metadata) -> std::io::Result<()> {
        let dst_f = File::open(dst)?;
        fchown(&dst_f, Some(meta.uid()), Some(meta.gid()))?;
        dst_f.set_permissions(PermissionsExt::from_mode(meta.mode() & 0o7777))?;
        set_times(&dst_f, meta)
    }

    fn timespecs(meta: &std::fs::Metadata) -> [libc::timespec; 2] {
        [
            libc::timespec {
                tv_sec: meta.atime() as _,
                tv_nsec: meta.atime_nsec() as _,
            },
            libc::timespec {
                tv_sec: meta.mtime() as _,
                tv_nsec: meta.mtime_nsec() as _,
            },
        ]
    }

    fn set_times(f: &File, meta: &std::fs::Metadata) -> std::io::Result<()> {
        if unsafe { libc::futimens(f.as_raw_fd(), timespecs(meta).as_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    fn set_times_nofollow(path: &Path, meta: &std::fs::Metadata) -> std::io::Result<()> {
        let path_c = cpath(path)?;
        if unsafe {
            libc::utimensat(
                libc::AT_FDCWD,
                path_c.as_ptr(),
                timespecs(meta).as_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    fn copy_xattrs(src: &File, dst: &File) -> std::io::Result<()> {
        for name in list_xattrs(|buf, len| unsafe { libc::flistxattr(src.as_raw_fd(), buf, len) })?
        {
            let value = read_xattr(|buf, len| unsafe {
                libc::fgetxattr(src.as_raw_fd(), name.as_ptr(), buf, len)
            })?;
            if unsafe {
                libc::fsetxattr(
                    dst.as_raw_fd(),
                    name.as_ptr(),
                    value.as_ptr() as *const libc::c_void,
                    value.len(),
                    0,
                )
            } != 0
            {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    }

    fn copy_xattrs_nofollow(src: &Path, dst: &Path) -> std::io::Result<()> {
        let src_c = cpath(src)?;
        let dst_c = cpath(dst)?;
        for name in list_xattrs(|buf, len| unsafe { libc::llistxattr(src_c.as_ptr(), buf, len) })? {
            let value = read_xattr(|buf, len| unsafe {
                libc::lgetxattr(src_c.as_ptr(), name.as_ptr(), buf, len)
            })?;
            if unsafe {
                libc::lsetxattr(
                    dst_c.as_ptr(),
                    name.as_ptr(),
                    value.as_ptr() as *const libc::c_void,
                    value.len(),
                    0,
                )
            } != 0
            {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    }

    fn cpath(path: &Path) -> std::io::Result<std::ffi::CString> {
        std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))
    }

    fn list_xattrs(
        list: impl Fn(*mut libc::c_char, libc::size_t) -> libc::ssize_t,
    ) -> std::io::Result<Vec<std::ffi::CString>> {
        let len = match list(std::ptr::null_mut(), 0) {
            n if n < 0 => {
                let e = std::io::Error::last_os_error();
                return if e.raw_os_error() == Some(libc::EOPNOTSUPP) {
                    Ok(Vec::new())
                } else {
                    Err(e)
                };
            }
            0 => return Ok(Vec::new()),
            n => n as usize,
        };
        let mut buf = vec![0u8; len];
        let n = list(buf.as_mut_ptr() as *mut libc::c_char, buf.len());
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        buf.truncate(n as usize);
        Ok(buf
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .filter_map(|s| std::ffi::CString::new(s).ok())
            .collect())
    }

    fn read_xattr(
        get: impl Fn(*mut libc::c_void, libc::size_t) -> libc::ssize_t,
    ) -> std::io::Result<Vec<u8>> {
        let len = match get(std::ptr::null_mut(), 0) {
            n if n < 0 => return Err(std::io::Error::last_os_error()),
            n => n as usize,
        };
        let mut buf = vec![0u8; len];
        let n = get(buf.as_mut_ptr() as *mut libc::c_void, buf.len());
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        buf.truncate(n as usize);
        Ok(buf)
    }

    #[cfg(test)]
    mod tests {
        use std::io::Write;

        use super::super::clone_tree;
        use super::*;

        struct TestDir(PathBuf);
        impl TestDir {
            fn new(name: &str) -> Self {
                let path = std::env::temp_dir()
                    .join(format!("clone-tree-test-{}-{name}", std::process::id()));
                std::fs::create_dir(&path).unwrap();
                Self(path)
            }
        }
        impl Drop for TestDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        fn ctr() -> Arc<Counter> {
            Arc::new(Counter::new(0, std::sync::atomic::Ordering::Relaxed))
        }

        /// Trees without regular files make no FICLONERANGE calls; trees with
        /// them skip on filesystems without reflink support.
        fn reflink_supported(dir: &Path) -> bool {
            let src_path = dir.join("probe-src");
            let dst_path = dir.join("probe-dst");
            std::fs::write(&src_path, b"x").unwrap();
            let src = File::open(&src_path).unwrap();
            let dst = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&dst_path)
                .unwrap();
            let supported = clone_range(&src, &dst, 0, 1).is_ok();
            if !supported {
                eprintln!("skipping: no reflink support on this filesystem");
            }
            supported
        }

        fn xattr(path: &Path, name: &str) -> Option<Vec<u8>> {
            let path = cpath(path).unwrap();
            let name = std::ffi::CString::new(name).unwrap();
            let len =
                unsafe { libc::getxattr(path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0) };
            if len < 0 {
                return None;
            }
            let mut buf = vec![0u8; len as usize];
            let n = unsafe {
                libc::getxattr(
                    path.as_ptr(),
                    name.as_ptr(),
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                )
            };
            assert_eq!(n as usize, buf.len());
            Some(buf)
        }

        #[tokio::test]
        async fn clones_tree_preserving_data_and_metadata() {
            let tmp = TestDir::new("tree");
            let src = tmp.0.join("src");
            let dst = tmp.0.join("dst");
            std::fs::create_dir(&src).unwrap();
            std::fs::create_dir(src.join("sub")).unwrap();
            std::fs::write(src.join("file"), b"hello").unwrap();
            std::fs::write(src.join("sub").join("empty"), b"").unwrap();
            std::fs::set_permissions(src.join("file"), PermissionsExt::from_mode(0o640)).unwrap();
            std::fs::set_permissions(src.join("sub"), PermissionsExt::from_mode(0o750)).unwrap();
            let set_xattr = |path: &Path| {
                let path_c = cpath(path).unwrap();
                let name = std::ffi::CString::new("user.clone-test").unwrap();
                let value = b"present";
                assert_eq!(
                    unsafe {
                        libc::setxattr(
                            path_c.as_ptr(),
                            name.as_ptr(),
                            value.as_ptr() as *const libc::c_void,
                            value.len(),
                            0,
                        )
                    },
                    0
                );
            };
            set_xattr(&src.join("file"));
            set_xattr(&src.join("sub"));
            let mtime = std::fs::metadata(src.join("file")).unwrap().mtime();

            if !reflink_supported(&tmp.0) {
                return;
            }
            let progress = ctr();
            clone_tree(&src, &dst, progress.clone()).await.unwrap();

            assert_eq!(std::fs::read(dst.join("file")).unwrap(), b"hello");
            assert_eq!(std::fs::read(dst.join("sub").join("empty")).unwrap(), b"");
            assert_eq!(
                std::fs::metadata(dst.join("file")).unwrap().mode() & 0o777,
                0o640
            );
            assert_eq!(
                std::fs::metadata(dst.join("sub")).unwrap().mode() & 0o777,
                0o750
            );
            assert_eq!(std::fs::metadata(dst.join("file")).unwrap().mtime(), mtime);
            assert_eq!(
                xattr(&dst.join("file"), "user.clone-test").as_deref(),
                Some(&b"present"[..])
            );
            assert_eq!(
                xattr(&dst.join("sub"), "user.clone-test").as_deref(),
                Some(&b"present"[..])
            );
            assert_eq!(progress.load(), 5);
        }

        #[tokio::test]
        async fn clones_symlinks_and_fifos() {
            let tmp = TestDir::new("special");
            let src = tmp.0.join("src");
            let dst = tmp.0.join("dst");
            std::fs::create_dir(&src).unwrap();
            std::os::unix::fs::symlink("file", src.join("link")).unwrap();
            std::os::unix::fs::symlink("missing", src.join("dangling")).unwrap();
            let fifo_c = cpath(&src.join("fifo")).unwrap();
            assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o644) }, 0);

            clone_tree(&src, &dst, ctr()).await.unwrap();

            let meta = std::fs::symlink_metadata(dst.join("link")).unwrap();
            assert!(meta.file_type().is_symlink());
            assert_eq!(
                std::fs::read_link(dst.join("link")).unwrap(),
                Path::new("file")
            );
            assert_eq!(
                std::fs::read_link(dst.join("dangling")).unwrap(),
                Path::new("missing")
            );
            assert!(
                std::fs::symlink_metadata(dst.join("fifo"))
                    .unwrap()
                    .file_type()
                    .is_fifo()
            );
        }

        #[tokio::test]
        async fn preserves_hardlinks() {
            let tmp = TestDir::new("hardlinks");
            let src = tmp.0.join("src");
            let dst = tmp.0.join("dst");
            std::fs::create_dir(&src).unwrap();
            std::fs::write(src.join("a"), b"shared").unwrap();
            std::fs::hard_link(src.join("a"), src.join("b")).unwrap();
            std::fs::write(src.join("c"), b"shared").unwrap();

            if !reflink_supported(&tmp.0) {
                return;
            }
            clone_tree(&src, &dst, ctr()).await.unwrap();

            let a = std::fs::metadata(dst.join("a")).unwrap();
            let b = std::fs::metadata(dst.join("b")).unwrap();
            let c = std::fs::metadata(dst.join("c")).unwrap();
            assert_eq!(a.ino(), b.ino());
            assert_eq!(a.nlink(), 2);
            assert_ne!(a.ino(), c.ino());
            assert_eq!(std::fs::read(dst.join("b")).unwrap(), b"shared");
        }

        #[tokio::test]
        async fn mirrors_nodatacow() {
            let tmp = TestDir::new("nodatacow");
            let src = tmp.0.join("src");
            let dst = tmp.0.join("dst");
            std::fs::create_dir(&src).unwrap();
            let mut nocow = File::create(src.join("nocow")).unwrap();
            // +C only sticks while the file has no extents
            let flags = FS_NOCOW_FL;
            if unsafe { libc::ioctl(nocow.as_raw_fd(), libc::FS_IOC_SETFLAGS as _, &flags) } != 0 {
                eprintln!("skipping: FS_IOC_SETFLAGS unsupported here");
                return;
            }
            nocow.write_all(b"cowed-out").unwrap();
            drop(nocow);

            if !reflink_supported(&tmp.0) {
                return;
            }
            clone_tree(&src, &dst, ctr()).await.unwrap();

            let cloned = File::open(dst.join("nocow")).unwrap();
            assert_ne!(get_fs_flags(&cloned).unwrap() & FS_NOCOW_FL, 0);
            assert_eq!(std::fs::read(dst.join("nocow")).unwrap(), b"cowed-out");
        }

        #[tokio::test]
        async fn clones_across_chunk_boundaries() {
            let tmp = TestDir::new("chunked");
            let src = tmp.0.join("src");
            let dst = tmp.0.join("dst");
            std::fs::create_dir(&src).unwrap();
            let size = CHUNK + 4096;
            let mut big = File::create(src.join("big")).unwrap();
            big.write_all(b"start").unwrap();
            big.set_len(size).unwrap();

            if !reflink_supported(&tmp.0) {
                return;
            }
            let progress = ctr();
            clone_tree(&src, &dst, progress.clone()).await.unwrap();

            assert_eq!(std::fs::metadata(dst.join("big")).unwrap().len(), size);
            assert_eq!(progress.load(), size);
        }
    }
}
