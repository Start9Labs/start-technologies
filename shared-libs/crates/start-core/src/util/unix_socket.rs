use std::path::Path;

use tokio::net::UnixStream;

/// Connects to a pinned socket inode without following a leaf symlink.
/// Callers must keep ancestor entries outside container control.
#[cfg(target_os = "linux")]
pub(crate) async fn connect_nofollow(path: &Path) -> std::io::Result<UnixStream> {
    use std::os::fd::AsFd;
    use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};

    let socket = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    if !socket.metadata()?.file_type().is_socket() {
        return Err(std::io::Error::from_raw_os_error(libc::ENOTSOCK));
    }
    UnixStream::connect(super::directory::descriptor_path(socket.as_fd())).await
}

#[cfg(not(target_os = "linux"))]
pub(crate) async fn connect_nofollow(_path: &Path) -> std::io::Result<UnixStream> {
    Err(std::io::ErrorKind::Unsupported.into())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::os::unix::fs::symlink;

    use tokio::net::UnixListener;

    use super::*;

    #[tokio::test]
    async fn real_sockets_connect_but_symlinks_and_non_sockets_do_not() {
        let dir = crate::util::io::TmpDir::new().await.unwrap();
        let path = dir.join("service.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let connected = connect_nofollow(&path).await.unwrap();
        let accepted = listener.accept().await.unwrap();
        drop((connected, accepted));
        let link = dir.join("redirect.sock");
        symlink(&path, &link).unwrap();
        assert!(connect_nofollow(&link).await.is_err());
        std::fs::write(dir.join("file"), b"not a socket").unwrap();
        assert!(connect_nofollow(&dir.join("file")).await.is_err());
        assert!(connect_nofollow(dir.as_ref()).await.is_err());
    }

    #[tokio::test]
    async fn long_host_paths_need_no_symlink_connect_workaround() {
        let dir = crate::util::io::TmpDir::new().await.unwrap();
        let long = dir.join("x".repeat(128));
        std::fs::create_dir(&long).unwrap();
        let alias = dir.join("d");
        symlink(&long, &alias).unwrap();
        let listener = UnixListener::bind(alias.join("service.sock")).unwrap();
        let path = long.join("service.sock");
        assert!(path.as_os_str().len() >= 108);
        let connected = connect_nofollow(&path).await.unwrap();
        let accepted = listener.accept().await.unwrap();
        drop((connected, accepted));
    }
}
