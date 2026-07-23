use std::path::{Path, PathBuf};

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

/// Lists subvolumes nested anywhere below `path` (exclusive), deepest first.
pub async fn nested_subvolumes(path: &Path) -> Result<Vec<PathBuf>, Error> {
    use std::os::unix::fs::MetadataExt;
    let mut found = Vec::new();
    let mut stack = vec![path.to_owned()];
    while let Some(dir) = stack.pop() {
        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("read dir {dir:?}")))?;
        while let Some(entry) = entries.next_entry().await? {
            let m = match entry.metadata().await {
                Ok(m) => m,
                Err(_) => continue,
            };
            if m.is_dir() {
                if m.ino() == SUBVOL_INO {
                    found.push(entry.path());
                }
                stack.push(entry.path());
            }
        }
    }
    found.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    Ok(found)
}

async fn delete_subvolume(path: &Path) -> Result<(), Error> {
    for nested in nested_subvolumes(path).await? {
        Command::new("btrfs")
            .args(["subvolume", "delete"])
            .arg(&nested)
            .timeout(Some(std::time::Duration::from_secs(60)))
            .invoke(ErrorKind::Filesystem)
            .await?;
    }
    Command::new("btrfs")
        .args(["subvolume", "delete"])
        .arg(path)
        .timeout(Some(std::time::Duration::from_secs(60)))
        .invoke(ErrorKind::Filesystem)
        .await?;
    Ok(())
}

/// Deletes `path` whether it is a subvolume (freed asynchronously by the btrfs
/// cleaner), a plain directory, or absent.
pub async fn delete_tree(path: impl AsRef<Path>) -> Result<(), Error> {
    let path = path.as_ref();
    if is_subvolume(path).await {
        delete_subvolume(path).await
    } else {
        crate::util::io::delete_dir(path).await
    }
}
