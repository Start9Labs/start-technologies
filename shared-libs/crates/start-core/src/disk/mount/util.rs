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

/// Flushes the containing filesystem via `syncfs(2)`.
/// Use `sync_directory` for daemons implementing durability through `FUSE_FSYNCDIR`.
#[instrument(skip_all)]
pub async fn sync_filesystem<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    tokio::process::Command::new("sync")
        .arg("-f")
        .arg(path.as_ref())
        .invoke(crate::ErrorKind::Filesystem)
        .await?;
    Ok(())
}

#[instrument(skip_all)]
pub async fn unmount<P: AsRef<Path>>(mountpoint: P, lazy: bool) -> Result<(), Error> {
    tracing::debug!("Unmounting {}.", mountpoint.as_ref().display());
    let mut cmd = tokio::process::Command::new("umount");
    cmd.env("LANG", "C.UTF-8");
    if lazy {
        cmd.arg("-l");
    }
    match cmd
        .arg(mountpoint.as_ref())
        .invoke(crate::ErrorKind::Filesystem)
        .await
    {
        Ok(_) => Ok(()),
        Err(e) if e.to_string().contains("not mounted") => Ok(()),
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
