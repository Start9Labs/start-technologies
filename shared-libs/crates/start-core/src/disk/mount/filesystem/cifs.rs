use std::future::Future;
use std::net::IpAddr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use color_eyre::eyre::eyre;
use digest::Digest;
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tokio::process::Command;
use tracing::instrument;

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
    resolve_dns_or_netbios(
        hostname,
        async {
            tokio::net::lookup_host((hostname, 445))
                .await
                .map_err(|error| Error::new(error, crate::ErrorKind::Network))?
                .next()
                .map(|addr| addr.ip())
                .ok_or_else(|| {
                    Error::new(
                        eyre!(t!("disk.mount.filesystem.cifs.no-dns-addresses")),
                        crate::ErrorKind::Network,
                    )
                })
        },
        async {
            Ok(String::from_utf8(
                Command::new("nmblookup")
                    .kill_on_drop(true)
                    .arg(hostname)
                    .invoke(crate::ErrorKind::Network)
                    .await?,
            )?
            .split(" ")
            .next()
            .unwrap()
            .trim()
            .parse()?)
        },
    )
    .await
}

async fn resolve_dns_or_netbios(
    hostname: &str,
    dns: impl Future<Output = Result<IpAddr, Error>>,
    netbios: impl Future<Output = Result<IpAddr, Error>>,
) -> Result<IpAddr, Error> {
    let (resolver, result) = if hostname.contains('.') {
        ("DNS", dns.await)
    } else {
        ("NetBIOS", netbios.await)
    };
    result.map_err(|error| {
        Error::new(
            eyre!(t!(
                "disk.mount.filesystem.cifs.hostname-unresolved",
                hostname = hostname,
                resolver = resolver,
                error = error.display_src()
            )),
            crate::ErrorKind::Network,
        )
    })
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

#[cfg(test)]
mod tests {
    use std::future::ready;

    use super::*;

    #[tokio::test]
    async fn ip_literals_bypass_name_lookup() {
        for hostname in ["192.0.2.10", "2001:db8::10"] {
            assert_eq!(
                resolve_hostname(hostname).await.unwrap(),
                hostname.parse::<IpAddr>().unwrap()
            );
        }
    }

    #[tokio::test]
    async fn dotted_system_hostname_resolves_without_netbios() {
        assert!(resolve_hostname("localhost.").await.unwrap().is_loopback());
    }

    #[tokio::test]
    async fn dotted_names_use_dns_only() {
        let addr = "2001:db8::10".parse().unwrap();
        for hostname in ["nas.lan", "nas.home.arpa", "nas.example.com", "nas."] {
            assert_eq!(
                resolve_dns_or_netbios(hostname, ready(Ok(addr)), async {
                    panic!("Dotted names must not use NetBIOS")
                })
                .await
                .unwrap(),
                addr
            );
        }
    }

    #[tokio::test]
    async fn bare_names_use_netbios_without_leaking_to_dns() {
        let addr = "192.0.2.10".parse().unwrap();
        for hostname in ["NAS", "fileserver", "backup-server"] {
            assert_eq!(
                resolve_dns_or_netbios(
                    hostname,
                    async { panic!("Bare names must not reach DNS") },
                    ready(Ok(addr)),
                )
                .await
                .unwrap(),
                addr
            );
        }
    }

    #[tokio::test]
    async fn dns_failure_does_not_fall_back_to_netbios() {
        let error = resolve_dns_or_netbios(
            "nas.home.arpa",
            ready(Err(Error::new(
                eyre!("DNS unavailable"),
                crate::ErrorKind::Network,
            ))),
            async { panic!("Dotted names must not fall back to NetBIOS") },
        )
        .await
        .unwrap_err();
        assert!(matches!(error.kind, crate::ErrorKind::Network));
        let message = error.to_string();
        for expected in ["nas.home.arpa", "DNS", "DNS unavailable"] {
            assert!(message.contains(expected), "{message}");
        }
        assert!(!message.contains("NetBIOS"), "{message}");
    }

    #[tokio::test]
    async fn netbios_failure_does_not_fall_back_to_dns() {
        let error = resolve_dns_or_netbios(
            "NAS",
            async { panic!("Bare names must not fall back to DNS") },
            ready(Err(Error::new(
                eyre!("NetBIOS unavailable"),
                crate::ErrorKind::Network,
            ))),
        )
        .await
        .unwrap_err();
        assert!(matches!(error.kind, crate::ErrorKind::Network));
        let message = error.to_string();
        for expected in ["NAS", "NetBIOS", "NetBIOS unavailable"] {
            assert!(message.contains(expected), "{message}");
        }
        assert!(!message.contains("DNS"), "{message}");
    }
}

#[derive(Debug, Deserialize, Serialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct Cifs {
    pub hostname: String,
    pub path: PathBuf,
    pub username: String,
    pub password: Option<String>,
}

rpc_toolkit::reflect_ts!(Cifs);
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
