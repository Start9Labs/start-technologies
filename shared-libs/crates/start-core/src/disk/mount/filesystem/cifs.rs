use std::net::IpAddr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use digest::Digest;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tokio::process::Command;
use tracing::instrument;
use ts_rs::TS;

use super::{BackupWrite, FileSystem, MountType, ReadOnly, ReadWrite};
use crate::Error;
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
use crate::util::Invoke;

async fn resolve_hostname(hostname: &str) -> Result<IpAddr, Error> {
    if let Ok(addr) = hostname.parse() {
        return Ok(addr);
    }
    if hostname.ends_with(".local") {
        return Ok(IpAddr::V4(crate::net::mdns::resolve_mdns(hostname).await?));
    }
    Ok(String::from_utf8(
        Command::new("nmblookup")
            .arg(hostname)
            .invoke(crate::ErrorKind::Network)
            .await?,
    )?
    .split(" ")
    .next()
    .unwrap()
    .trim()
    .parse()?)
}

#[instrument(skip_all)]
pub async fn mount_cifs(
    hostname: &str,
    path: impl AsRef<Path>,
    username: &str,
    password: Option<&str>,
    mountpoint: impl AsRef<Path>,
    mount_type: MountType,
) -> Result<(), Error> {
    tokio::fs::create_dir_all(mountpoint.as_ref()).await?;
    let ip: IpAddr = resolve_hostname(hostname).await?;
    let absolute_path = Path::new("/").join(path.as_ref());
    let mut cmd = Command::new("mount");
    cmd.arg("-t")
        .arg("cifs")
        .env("USER", username)
        .env("PASSWD", password.unwrap_or_default())
        .arg(format!("//{}{}", ip, absolute_path.display()))
        .arg(mountpoint.as_ref());
    let mut opts =
        String::from("vers=3,hard,actimeo=0,wsize=1048576,rsize=1048576,nobrl,noserverino");
    match mount_type {
        ReadOnly => opts.push_str(",ro,cache=strict"),
        ReadWrite => opts.push_str(",cache=strict"),
        BackupWrite => opts.push_str(",cache=none"),
    }
    cmd.arg("-o").arg(opts);
    cmd.invoke(crate::ErrorKind::Filesystem).await?;
    Ok(())
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Cifs {
    pub hostname: String,
    pub path: PathBuf,
    pub username: String,
    pub password: Option<String>,
}
impl Cifs {
    pub async fn mountable(&self) -> Result<(), Error> {
        let guard = TmpMountGuard::mount(self, ReadOnly).await?;
        guard.unmount().await?;
        Ok(())
    }
}
impl FileSystem for Cifs {
    async fn mount<P: AsRef<std::path::Path> + Send>(
        &self,
        mountpoint: P,
        mount_type: MountType,
    ) -> Result<(), Error> {
        mount_cifs(
            &self.hostname,
            &self.path,
            &self.username,
            self.password.as_ref().map(|p| p.as_str()),
            mountpoint,
            mount_type,
        )
        .await
    }
    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        let mut sha = Sha256::new();
        sha.update("Cifs");
        sha.update(self.hostname.as_bytes());
        sha.update(self.path.as_os_str().as_bytes());
        Ok(sha.finalize())
    }
}
