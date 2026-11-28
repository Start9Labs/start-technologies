#[cfg(test)]
#[path = "namespace_tests.rs"]
mod namespace_tests;
#[cfg(test)]
#[path = "unmount_tests.rs"]
mod tests;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
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

/// Native propagation releases copies held by other mount namespaces.
#[instrument(skip_all)]
pub async fn unmount<P: AsRef<Path>>(mountpoint: P, lazy: bool) -> Result<(), Error> {
    unmount_command(mountpoint.as_ref(), unmount_options(lazy)).await
}

/// Retires every cloned child with native namespace-copy propagation.
/// The caller excludes new users and uses source-isolating bind propagation.
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
/// The caller keeps paths and mount membership stable through completion.
#[instrument(skip_all)]
pub async fn unmount_all_under<P: AsRef<Path>>(path: P, lazy: bool) -> Result<(), Error> {
    for mountpoint in mountpoints_under(path.as_ref()).await? {
        tracing::debug!("Unmounting nested mountpoint: {}", mountpoint.display());
        unmount(mountpoint, lazy).await?;
    }

    Ok(())
}

pub(crate) async fn mountpoints_under(path: &Path) -> Result<Vec<PathBuf>, Error> {
    let root = tokio::fs::canonicalize(path).await?;
    let mut mountpoints = read_mountpoints(Path::new("/proc/self/mountinfo"))
        .await?
        .into_iter()
        .filter(|target| target.starts_with(&root))
        .collect::<Vec<_>>();
    mountpoints.sort_by_key(|target| std::cmp::Reverse(target.components().count()));
    Ok(mountpoints)
}

async fn read_mountpoints(table: &Path) -> Result<Vec<PathBuf>, Error> {
    #[derive(Deserialize)]
    struct Mounts {
        filesystems: Vec<Mount>,
    }
    #[derive(Deserialize)]
    struct Mount {
        target: PathBuf,
    }
    let bytes = tokio::process::Command::new("/usr/bin/findmnt")
        .current_dir("/")
        .env_clear()
        .env("LC_ALL", "C")
        .args(["--kernel", "--tab-file"])
        .arg(table)
        .args(["--nocanonicalize", "--list", "--json", "--output", "TARGET"])
        .timeout(Some(Duration::from_secs(10)))
        .invoke(ErrorKind::Filesystem)
        .await?;
    let mounts: Mounts = serde_json::from_slice(&bytes).with_kind(ErrorKind::Filesystem)?;
    if mounts
        .filesystems
        .iter()
        .any(|mount| !mount.target.is_absolute())
    {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidData).into());
    }
    Ok(mounts
        .filesystems
        .into_iter()
        .map(|mount| mount.target)
        .collect())
}
