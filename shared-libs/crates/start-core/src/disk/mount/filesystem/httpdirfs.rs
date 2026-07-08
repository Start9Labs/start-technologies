use std::path::Path;

use digest::Digest;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use super::{FileSystem, MountType};
use crate::Error;
use crate::util::Invoke;

pub async fn mount_httpdirfs(url: &Url, mountpoint: impl AsRef<Path>) -> Result<(), Error> {
    tokio::fs::create_dir_all(mountpoint.as_ref()).await?;
    let mut cmd = tokio::process::Command::new("httpdirfs");
    cmd.arg("--cache")
        .arg("--single-file-mode")
        .arg(url.as_str())
        .arg(mountpoint.as_ref());
    cmd.invoke(crate::ErrorKind::Filesystem).await?;
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpDirFS {
    url: Url,
}
impl HttpDirFS {
    pub fn new(url: Url) -> Self {
        HttpDirFS { url }
    }
}
impl FileSystem for HttpDirFS {
    async fn mount<P: AsRef<Path> + Send>(
        &self,
        mountpoint: P,
        _mount_type: MountType,
    ) -> Result<(), Error> {
        mount_httpdirfs(&self.url, mountpoint).await
    }
    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        let mut sha = Sha256::new();
        sha.update("HttpDirFS");
        sha.update(self.url.as_str());
        Ok(sha.finalize())
    }
}
