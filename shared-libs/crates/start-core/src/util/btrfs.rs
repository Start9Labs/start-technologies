use std::path::Path;

use tokio::process::Command;

use crate::prelude::*;
use crate::util::Invoke;

#[cfg(test)]
pub(crate) mod tests;

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

/// Removes subvolume roots without walking files or waiting for space reclamation.
/// Nested deletion can partially complete before an error; plain directories delete recursively.
pub async fn delete_tree(path: impl AsRef<Path>) -> Result<(), Error> {
    let path = path.as_ref();
    if is_subvolume(path).await {
        Command::new("btrfs")
            .args(["subvolume", "delete", "--recursive"])
            .arg(path)
            .timeout(Some(std::time::Duration::from_secs(60)))
            .invoke(ErrorKind::Filesystem)
            .await?;
        Ok(())
    } else {
        crate::util::io::delete_dir(path).await
    }
}

pub(crate) const CONVERT_TMP_SUFFIX: &str = ".convert-tmp";
pub(crate) const CONVERT_OLD_SUFFIX: &str = ".convert-old";

fn conversion_paths(path: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let name = path.file_name().expect("volume root");
    let mut temporary = name.to_os_string();
    temporary.push(CONVERT_TMP_SUFFIX);
    let mut original = name.to_os_string();
    original.push(CONVERT_OLD_SUFFIX);
    (
        path.with_file_name(temporary),
        path.with_file_name(original),
    )
}

pub(crate) async fn recover_conversion(path: &Path) -> Result<(), Error> {
    let (temporary, original) = conversion_paths(path);
    delete_tree(&temporary).await?;
    if tokio::fs::try_exists(&original).await? {
        if tokio::fs::try_exists(path).await? {
            delete_tree(&original).await?;
        } else {
            crate::util::io::rename(&original, path).await?;
        }
    }
    Ok(())
}

/// Commits deletion even when a preceding interrupted attempt removed the root.
pub(crate) async fn delete_volume_root(path: &Path) -> Result<(), Error> {
    recover_conversion(path).await?;
    delete_tree(path).await?;
    let parent = path.parent().expect("volume root");
    if is_btrfs(parent).await {
        Command::new("btrfs")
            .args(["filesystem", "sync"])
            .arg(parent)
            .timeout(Some(std::time::Duration::from_secs(60)))
            .invoke(ErrorKind::Filesystem)
            .await?;
    }
    Ok(())
}

/// The caller must exclude mounts and writers throughout conversion.
pub(crate) async fn convert_directory(
    path: &Path,
    counter: std::sync::Arc<crate::util::io::Counter>,
) -> Result<(), Error> {
    recover_conversion(path).await?;
    if !tokio::fs::try_exists(path).await? || is_subvolume(path).await {
        return Ok(());
    }
    let (temporary, original) = conversion_paths(path);
    create_subvolume(&temporary).await?;
    if let Err(error) = crate::util::clone::clone_tree(path, &temporary, counter).await {
        delete_tree(&temporary).await.log_err();
        return Err(error);
    }
    if let Err(error) = crate::util::io::rename(path, &original).await {
        delete_tree(&temporary).await.log_err();
        return Err(error);
    }
    if let Err(error) = crate::util::io::rename(&temporary, path).await {
        crate::util::io::rename(&original, path).await.log_err();
        delete_tree(&temporary).await.log_err();
        return Err(error);
    }
    delete_tree(&original).await
}
