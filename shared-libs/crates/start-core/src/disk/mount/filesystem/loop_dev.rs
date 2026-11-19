#[cfg(test)]
#[path = "loop_dev_tests.rs"]
mod tests;

use std::fmt::Display;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use digest::Digest;
use lazy_format::lazy_format;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use super::FileSystem;
use crate::prelude::*;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopDev<LogicalName: AsRef<Path>> {
    logicalname: LogicalName,
    offset: u64,
    size: u64,
}
impl<LogicalName: AsRef<Path>> LoopDev<LogicalName> {
    pub fn new(logicalname: LogicalName, offset: u64, size: u64) -> Self {
        Self {
            logicalname,
            offset,
            size,
        }
    }
}
/// A SquashFS-only loop mount without filesystem-selected external helpers.
/// The caller retains the image and excludes writes until unmount completes.
pub struct SquashfsImage<Source: AsRef<Path>>(LoopDev<Source>);

impl<Source: AsRef<Path>> SquashfsImage<Source> {
    pub fn new(source: Source) -> Self {
        Self(LoopDev::new(source, 0, 0))
    }
}

impl<Source: AsRef<Path> + Send + Sync> FileSystem for SquashfsImage<Source> {
    fn mount_type(&self) -> Option<impl AsRef<str>> {
        Some("squashfs")
    }
    fn extra_args(&self) -> impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>> {
        ["--internal-only"]
    }
    async fn source(&self) -> Result<Option<impl AsRef<Path>>, Error> {
        self.0.source().await
    }
    fn mount_options(&self) -> impl IntoIterator<Item = impl Display> {
        self.0.mount_options()
    }
    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        let mut hash = Sha256::new();
        hash.update("SquashfsImage");
        hash.update(self.0.source_hash().await?);
        Ok(hash.finalize())
    }
}

impl<LogicalName: AsRef<Path> + Send + Sync> FileSystem for LoopDev<LogicalName> {
    async fn source(&self) -> Result<Option<impl AsRef<Path>>, Error> {
        Ok(Some(
            tokio::fs::canonicalize(self.logicalname.as_ref()).await?,
        ))
    }
    fn mount_options(&self) -> impl IntoIterator<Item = impl Display> {
        [
            Box::new("loop") as Box<dyn Display>,
            Box::new(lazy_format!("offset={}", self.offset)),
            Box::new(lazy_format!("sizelimit={}", self.size)),
        ]
    }
    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        let mut sha = Sha256::new();
        sha.update("LoopDev");
        sha.update(
            tokio::fs::canonicalize(self.logicalname.as_ref())
                .await
                .with_ctx(|_| {
                    (
                        crate::ErrorKind::Filesystem,
                        self.logicalname.as_ref().display().to_string(),
                    )
                })?
                .as_os_str()
                .as_bytes(),
        );
        sha.update(&u64::to_be_bytes(self.offset)[..]);
        Ok(sha.finalize())
    }
}
