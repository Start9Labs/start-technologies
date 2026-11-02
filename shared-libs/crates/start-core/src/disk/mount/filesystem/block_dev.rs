use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use digest::Digest;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use super::FileSystem;
use crate::prelude::*;

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct BlockDev<LogicalName: AsRef<Path>> {
    logicalname: LogicalName,
}

rpc_toolkit::reflect_ts!(impl [LogicalName: AsRef<Path>] for BlockDev<LogicalName> where [LogicalName: rpc_toolkit::ts::TS]);
rpc_toolkit::ts_export!(
    BlockDev<std::path::PathBuf>,
    name = "BlockDev",
    namespaces = [""]
);
impl<LogicalName: AsRef<Path>> BlockDev<LogicalName> {
    pub fn new(logicalname: LogicalName) -> Self {
        BlockDev { logicalname }
    }

    /// Returns the filesystem UUID link, failing when none exists.
    pub async fn persistent_path(&self) -> Result<PathBuf, Error> {
        uuid_path(self.logicalname.as_ref(), Path::new("/dev/disk/by-uuid")).await
    }
}
async fn uuid_path(device: &Path, directory: &Path) -> Result<PathBuf, Error> {
    let device = tokio::fs::canonicalize(device).await?;
    let mut entries = tokio::fs::read_dir(directory).await?;
    while let Some(entry) = entries.next_entry().await? {
        if tokio::fs::canonicalize(entry.path())
            .await
            .is_ok_and(|path| path == device)
        {
            return Ok(entry.path());
        }
    }
    Err(Error::new(
        eyre!(t!("disk.util.missing-filesystem-uuid")),
        ErrorKind::BlockDevice,
    ))
}

impl<LogicalName: AsRef<Path> + Send + Sync> FileSystem for BlockDev<LogicalName> {
    async fn source(&self) -> Result<Option<impl AsRef<Path>>, Error> {
        Ok(Some(&self.logicalname))
    }
    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        let mut sha = Sha256::new();
        sha.update("BlockDev");
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
        Ok(sha.finalize())
    }
}

#[cfg(test)]
mod tests;
