use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use digest::Digest;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use ts_rs::TS;

use super::FileSystem;
use crate::prelude::*;

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(concrete(LogicalName = std::path::PathBuf))]
pub struct BlockDev<LogicalName: AsRef<Path>> {
    logicalname: LogicalName,
}
impl<LogicalName: AsRef<Path>> BlockDev<LogicalName> {
    pub fn new(logicalname: LogicalName) -> Self {
        BlockDev { logicalname }
    }
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
