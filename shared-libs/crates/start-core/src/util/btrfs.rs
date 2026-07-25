use std::path::Path;

use tokio::process::Command;

use crate::prelude::*;
use crate::util::Invoke;

/// Subvolume roots always have this inode number (BTRFS_FIRST_FREE_OBJECTID).
const SUBVOL_INO: u64 = 256;

#[cfg(target_os = "linux")]
pub async fn is_btrfs(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref().to_owned();
    tokio::task::spawn_blocking(move || {
        nix::sys::statfs::statfs(&path)
            .map(|s| s.filesystem_type() == nix::sys::statfs::BTRFS_SUPER_MAGIC)
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
pub async fn is_btrfs(_path: impl AsRef<Path>) -> bool {
    false
}

pub async fn is_subvolume(path: impl AsRef<Path>) -> bool {
    use std::os::unix::fs::MetadataExt;
    match tokio::fs::metadata(path.as_ref()).await {
        Ok(m) if m.is_dir() && m.ino() == SUBVOL_INO => is_btrfs(path).await,
        _ => false,
    }
}

pub async fn create_subvolume(path: impl AsRef<Path>) -> Result<(), Error> {
    Command::new("btrfs")
        .args(["subvolume", "create"])
        .arg(path.as_ref())
        .timeout(Some(std::time::Duration::from_secs(60)))
        .invoke(ErrorKind::Filesystem)
        .await?;
    Ok(())
}

/// Creates a writable point-in-time snapshot of `src` at `dst`. Constant-time
/// and constant-space regardless of the subvolume's size or fragmentation.
pub async fn snapshot_subvolume(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<(), Error> {
    Command::new("btrfs")
        .args(["subvolume", "snapshot"])
        .arg(src.as_ref())
        .arg(dst.as_ref())
        .timeout(Some(std::time::Duration::from_secs(60)))
        .invoke(ErrorKind::Filesystem)
        .await?;
    Ok(())
}

/// Deletes `path` whether it is a subvolume (freed asynchronously by the
/// btrfs cleaner), a plain directory, or absent. `btrfs subvolume delete`
/// refuses a subvolume containing nested ones; rather than walk the tree we
/// fall back to a plain recursive remove, accepting that nested subvolume
/// roots (which package volumes should never contain) may survive it.
pub async fn delete_tree(path: impl AsRef<Path>) -> Result<(), Error> {
    let path = path.as_ref();
    if is_subvolume(path).await
        && Command::new("btrfs")
            .args(["subvolume", "delete"])
            .arg(path)
            .timeout(Some(std::time::Duration::from_secs(60)))
            .invoke(ErrorKind::Filesystem)
            .await
            .is_ok()
    {
        return Ok(());
    }
    crate::util::io::delete_dir(path).await
}
