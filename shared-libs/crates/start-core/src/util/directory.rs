use std::ffi::{CStr, CString};
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

pub(crate) fn descriptor_path(fd: BorrowedFd<'_>) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", fd.as_raw_fd()))
}

/// The caller supplies a single entry beneath an already-pinned directory.
pub(crate) fn open_at_nofollow(
    parent: &File,
    name: &CStr,
    flags: libc::c_int,
    mode: u32,
) -> io::Result<File> {
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            mode as libc::c_uint,
        )
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

fn entry_name(component: Component<'_>) -> io::Result<CString> {
    match component {
        Component::Normal(name) => CString::new(name.as_bytes())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error)),
        _ => Err(io::Error::from(io::ErrorKind::InvalidInput)),
    }
}

fn open_directory_at(parent: &File, name: &CStr) -> io::Result<File> {
    open_at_nofollow(parent, name, libc::O_RDONLY | libc::O_DIRECTORY, 0)
}

pub(crate) fn open_directory_nofollow(path: &Path) -> io::Result<File> {
    if !path.is_absolute() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    let mut directory = File::open("/")?;
    for component in path.components() {
        if component == Component::RootDir {
            continue;
        }
        directory = open_directory_at(&directory, &entry_name(component)?)?;
    }
    Ok(directory)
}

/// The caller supplies a single entry beneath an already-pinned directory.
pub(crate) fn create_directory_at(parent: &File, name: &CStr) -> io::Result<(File, bool)> {
    match open_directory_at(parent, name) {
        Ok(child) => Ok((child, false)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let created = if unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o755) } < 0
            {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::AlreadyExists {
                    return Err(error);
                }
                false
            } else {
                true
            };
            Ok((open_directory_at(parent, name)?, created))
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn create_directory_beneath(root: &File, relative: &Path) -> io::Result<File> {
    let mut directory = root.try_clone()?;
    for component in relative.components() {
        directory = create_directory_at(&directory, &entry_name(component)?)?.0;
    }
    Ok(directory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_walk_rejects_symlinks_and_parent_components() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let pinned = open_directory_nofollow(root.path()).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("planted")).unwrap();
        assert!(create_directory_beneath(&pinned, Path::new("planted/child")).is_err());
        assert!(open_directory_nofollow(&root.path().join("planted")).is_err());
        assert!(create_directory_beneath(&pinned, Path::new("../child")).is_err());
        assert!(std::fs::read_dir(outside.path()).unwrap().next().is_none());
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn atomic_file_retains_its_directory_after_the_caller_closes_it() {
        use tokio::io::AsyncWriteExt;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("etc")).unwrap();
        let pinned = open_directory_nofollow(&root.path().join("etc")).unwrap();
        let mut file = crate::util::io::AtomicFile::new_at(&pinned, c"hostname")
            .await
            .unwrap();
        drop(pinned);
        std::fs::rename(root.path().join("etc"), root.path().join("moved")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("etc")).unwrap();
        file.write_all(b"container value").await.unwrap();
        file.save().await.unwrap();
        assert_eq!(
            std::fs::read_to_string(root.path().join("moved/hostname")).unwrap(),
            "container value"
        );
        assert!(std::fs::read_dir(outside.path()).unwrap().next().is_none());
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn owned_file_writes_remain_pinned_and_replace_leaf_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("etc")).unwrap();
        let pinned = open_directory_nofollow(&root.path().join("etc")).unwrap();
        let host_file = outside.path().join("hostname");
        std::fs::write(&host_file, "host value").unwrap();
        std::os::unix::fs::symlink(&host_file, root.path().join("etc/hostname")).unwrap();
        std::fs::rename(root.path().join("etc"), root.path().join("moved")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("etc")).unwrap();
        crate::util::io::write_file_owned_at(
            &pinned,
            c"hostname",
            "container value",
            nix::unistd::Uid::current().as_raw(),
            nix::unistd::Gid::current().as_raw(),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read_to_string(&host_file).unwrap(), "host value");
        assert_eq!(
            std::fs::read_to_string(root.path().join("moved/hostname")).unwrap(),
            "container value"
        );
        assert!(
            !std::fs::symlink_metadata(root.path().join("moved/hostname"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn directory_walk_stays_on_a_pinned_parent_after_replacement() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("original")).unwrap();
        let pinned = open_directory_nofollow(&root.path().join("original")).unwrap();
        std::fs::rename(root.path().join("original"), root.path().join("moved")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("original")).unwrap();
        create_directory_beneath(&pinned, Path::new("child")).unwrap();
        assert!(root.path().join("moved/child").is_dir());
        assert!(std::fs::read_dir(outside.path()).unwrap().next().is_none());
    }
}
