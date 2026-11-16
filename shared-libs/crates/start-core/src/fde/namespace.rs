use std::path::{Path, PathBuf};

use super::root::VerifiedMounts;
use crate::disk::mount::filesystem::bind::{Bind, FileType};
use crate::disk::mount::filesystem::private_tmpfs::PrivateTmpfs;
use crate::disk::mount::filesystem::{ReadOnly, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, MountGuard};
use crate::prelude::*;

pub(super) fn invalid_source() -> Error {
    Error::new(
        eyre!(t!("fde.boot.invalid-descriptor")),
        ErrorKind::InvalidSignature,
    )
}

pub(super) async fn confined_path(root: &Path, relative: &Path) -> Result<PathBuf, Error> {
    let root = crate::util::io::canonicalize(root, true).await?;
    let path = crate::util::io::canonicalize(root.join(relative), true).await?;
    if !path.starts_with(&root) {
        return Err(invalid_source());
    }
    Ok(path)
}

pub(super) async fn mountpoint(root: &Path, relative: &Path) -> Result<PathBuf, Error> {
    let target = root.join(relative);
    if confined_path(root, relative).await? != target {
        return Err(invalid_source());
    }
    Ok(target)
}

pub(super) async fn prepare<L: GenericMountGuard>(
    layers: &mut VerifiedMounts<L>,
    root: &Path,
) -> Result<(), Error> {
    let scratch = ["dev", "run", "tmp", "var/tmp"];
    for path in scratch.into_iter().chain(["proc"]) {
        mountpoint(root, Path::new(path)).await?;
    }
    for path in scratch {
        layers.push(MountGuard::mount(&PrivateTmpfs::new(), root.join(path), ReadWrite).await?);
    }
    for device in ["null", "zero", "random", "urandom"] {
        layers.push(
            MountGuard::mount(
                &Bind::new(Path::new("/dev").join(device)).with_type(FileType::File),
                root.join("dev").join(device),
                ReadOnly,
            )
            .await?,
        );
    }
    Ok(())
}

pub(super) fn command(root: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("/usr/bin/unshare");
    command
        .args([
            "--mount",
            "--pid",
            "--fork",
            "--kill-child=SIGKILL",
            "--ipc",
            "--uts",
            "--propagation=private",
            "--mount-proc",
        ])
        .arg("--root")
        .arg(root)
        .arg("--wd=/")
        .env_clear();
    command
}
