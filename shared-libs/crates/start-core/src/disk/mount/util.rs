#[cfg(test)]
#[path = "unmount_tests.rs"]
mod tests;

use std::path::Path;

use tracing::instrument;

use crate::Error;
use crate::prelude::*;
use crate::util::Invoke;

pub async fn is_mountpoint(path: impl AsRef<Path>) -> Result<bool, Error> {
    let is_mountpoint = tokio::process::Command::new("mountpoint")
        .arg(path.as_ref())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await?;
    Ok(is_mountpoint.success())
}

#[instrument(skip_all)]
pub async fn bind<P0: AsRef<Path>, P1: AsRef<Path>>(
    src: P0,
    dst: P1,
    read_only: bool,
) -> Result<(), Error> {
    tracing::info!(
        "{}",
        t!(
            "disk.mount.binding",
            src = src.as_ref().display(),
            dst = dst.as_ref().display()
        )
    );
    if is_mountpoint(&dst).await? {
        unmount(dst.as_ref(), true).await?;
    }
    tokio::fs::create_dir_all(&src).await?;
    tokio::fs::create_dir_all(&dst).await?;
    let mut mount_cmd = tokio::process::Command::new("mount");
    mount_cmd.arg("--bind");
    if read_only {
        mount_cmd.arg("-o").arg("ro");
    }
    mount_cmd
        .arg(src.as_ref())
        .arg(dst.as_ref())
        .invoke(crate::ErrorKind::Filesystem)
        .await?;
    Ok(())
}

/// Flush the filesystem containing `path` via `syncfs(2)` (`sync -f`).
///
/// Use this before lazy-unmounting a layered fs whose front layer flushes
/// dirty data to its backing on `syncfs` (e.g. overlayfs). FUSE filesystems
/// that don't implement `FUSE_SYNCFS` won't see this signal — use
/// [`sync_directory`] for those.
#[instrument(skip_all)]
pub async fn sync_filesystem<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    tokio::process::Command::new("sync")
        .arg("-f")
        .arg(path.as_ref())
        .invoke(crate::ErrorKind::Filesystem)
        .await?;
    Ok(())
}

/// Flush the directory at `path` via `fsync(2)` (`sync` with no flag).
///
/// For FUSE-based filesystems (e.g. backup-fs) this routes to
/// `FUSE_FSYNCDIR`, which the daemon implements to drain its dirty cache —
/// `syncfs` is not enough because FUSE doesn't propagate it.
#[instrument(skip_all)]
pub async fn sync_directory<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    tokio::process::Command::new("sync")
        .arg(path.as_ref())
        .invoke(crate::ErrorKind::Filesystem)
        .await?;
    Ok(())
}

/// Preserves source peers while retiring the owned attachment.
/// The caller excludes replacement mounts through retirement.
#[instrument(skip_all)]
pub async fn unmount<P: AsRef<Path>>(mountpoint: P, lazy: bool) -> Result<(), Error> {
    unmount_command(mountpoint.as_ref(), unmount_options(lazy)).await
}

/// Preserves source peers while retiring the owned tree.
/// The caller excludes new users and mount-tree mutations through retirement.
#[instrument(skip_all)]
pub async fn unmount_tree<P: AsRef<Path>>(mountpoint: P, lazy: bool) -> Result<(), Error> {
    let mountpoint = mountpoint.as_ref();
    let mut command = unmount_options(lazy);
    command.arg("--recursive");
    unmount_command(mountpoint, command).await
}

fn unmount_options(lazy: bool) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("umount");
    command.env("LANG", "C.UTF-8");
    if lazy {
        command.arg("-l");
    }
    command
}

async fn unmount_command(
    mountpoint: &Path,
    mut command: tokio::process::Command,
) -> Result<(), Error> {
    tracing::debug!("Unmounting {}.", mountpoint.display());
    if is_mountpoint(mountpoint).await? {
        super::filesystem::syscall::make_tree_private(mountpoint)?;
    }
    match command
        .arg("--")
        .arg(mountpoint)
        .invoke(crate::ErrorKind::Filesystem)
        .await
    {
        Ok(_) => Ok(()),
        Err(e)
            if e.to_string().contains("not mounted")
                || matches!(tokio::fs::try_exists(mountpoint).await, Ok(false)) =>
        {
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Unmounts all mountpoints under (and including) the given path, in reverse
/// depth order so that nested mounts are unmounted before their parents.
#[instrument(skip_all)]
pub async fn unmount_all_under<P: AsRef<Path>>(path: P, lazy: bool) -> Result<(), Error> {
    let path = path.as_ref();
    let canonical_path = tokio::fs::canonicalize(path)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("canonicalize {path:?}")))?;

    let mounts_content = tokio::fs::read_to_string("/proc/mounts")
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, "read /proc/mounts"))?;

    // Collect all mountpoints under our path
    let mut mountpoints: Vec<&str> = mounts_content
        .lines()
        .filter_map(|line| {
            let mountpoint = line.split_whitespace().nth(1)?;
            // Check if this mountpoint is under our target path
            let mp_path = Path::new(mountpoint);
            if mp_path.starts_with(&canonical_path) {
                Some(mountpoint)
            } else {
                None
            }
        })
        .collect();

    // Sort by path length descending so we unmount deepest first
    mountpoints.sort_by(|a, b| b.len().cmp(&a.len()));

    for mountpoint in mountpoints {
        tracing::debug!("Unmounting nested mountpoint: {}", mountpoint);
        unmount(mountpoint, lazy).await?;
    }

    Ok(())
}
