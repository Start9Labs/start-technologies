use std::os::unix::ffi::OsStrExt;
use std::path::Path;

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
