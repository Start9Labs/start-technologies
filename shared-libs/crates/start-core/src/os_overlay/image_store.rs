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

/// Native image namespace, independent of boot eligibility or source trust.
/// The caller retains the directory and excludes image-store writers through use.
pub(crate) async fn image_paths(directory: &Path) -> Result<Vec<PathBuf>, Error> {
    drop(super::read_generation_directory(directory).await?);
    let bytes = Command::new("/bin/bash")
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .args(["--noprofile", "--norc", "-euc"])
        .arg(format!("{SCRIPT}\nstartos_image_paths \"$1\""))
        .arg("startos-image-paths")
        .arg(directory)
        .stdin(Stdio::null())
        .invoke(ErrorKind::Filesystem)
        .await?;
    bytes
        .split_inclusive(|byte| *byte == 0)
        .map(|path| {
            path.strip_suffix(&[0])
                .map(|path| std::ffi::OsString::from_vec(path.to_vec()).into())
                .ok_or_else(super::invalid_signature)
        })
        .collect()
}

#[cfg(test)]
#[path = "image_store_tests.rs"]
mod tests;
