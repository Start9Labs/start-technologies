use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;

use crate::prelude::*;
use crate::util::Invoke;

const SCRIPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../projects/start-os/build/lib/scripts/os-image-store"
));

fn command() -> Command {
    let mut command = Command::new("/bin/bash");
    command
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .args(["--noprofile", "--norc", "-euc"])
        .arg(format!("{SCRIPT}\n\"$@\""))
        .arg("startos-image-store")
        .stdin(Stdio::null());
    command
}

async fn paths(mut command: Command) -> Result<Vec<PathBuf>, Error> {
    command
        .invoke(ErrorKind::Filesystem)
        .await?
        .split_inclusive(|byte| *byte == 0)
        .map(|path| {
            path.strip_suffix(&[0])
                .map(|path| std::ffi::OsString::from_vec(path.to_vec()).into())
                .ok_or_else(super::invalid_signature)
        })
        .collect()
}

/// Native image namespace, independent of boot eligibility or source trust.
/// The caller retains the directory and excludes image-store writers through use.
pub(crate) async fn image_paths(directory: &Path) -> Result<Vec<PathBuf>, Error> {
    drop(super::read_generation_directory(directory).await?);
    let mut command = command();
    command.arg("startos_image_paths").arg(directory);
    paths(command).await
}

/// Resolves native selection without conferring public-source or boot authority.
/// The caller excludes selection writers and retains the resulting source.
pub(crate) async fn selected_image(configuration: &Path) -> Result<PathBuf, Error> {
    let mut command = command();
    command.arg("startos_selected_image").arg(configuration);
    let mut paths = paths(command).await?;
    if paths.len() != 1 {
        return Err(super::invalid_signature());
    }
    Ok(paths.remove(0))
}

/// Repairs legacy selection before readonly capture or retention inspection.
pub(crate) async fn repair_selection(
    lease: &crate::util::FileLock,
    volume: &Path,
) -> Result<(), Error> {
    let mut command = command();
    super::inherit_image_store(lease, &mut command)?;
    command.arg("startos_repair_image_selection").arg(volume);
    command.invoke(ErrorKind::Filesystem).await?;
    Ok(())
}

#[cfg(test)]
#[path = "image_store_tests.rs"]
mod tests;
