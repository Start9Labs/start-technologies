use std::future::Future;
use std::net::IpAddr;
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
    let source = Cifs {
        hostname: hostname.into(),
        path: path.as_ref().into(),
        username: username.into(),
        password: password.map(str::to_owned),
    };
    source
        .mount_with_options(
            mountpoint.as_ref(),
            &format!(
                "vers=3,hard,actimeo=0,wsize=1048576,rsize=1048576,nobrl,noserverino,{}",
                access_options(mount_type)
            ),
        )
        .await
}

fn access_options(mount_type: MountType) -> &'static str {
    match mount_type {
        ReadOnly => "ro,cache=strict",
        ReadWrite => "cache=strict",
        BackupWrite => "cache=none",
    }
}

#[cfg(test)]
mod tests {
    use std::future::ready;

    use super::*;

    #[tokio::test]
    async fn temporary_mount_identity_includes_credentials_and_runtime_policy() {
        let mut source = Cifs {
            hostname: "nas.lan".into(),
            path: "/share".into(),
            username: "alice".into(),
            password: Some("secret".into()),
        };
        let first = source.source_hash().await.unwrap();
        source.username = "bob".into();
        assert_ne!(first, source.source_hash().await.unwrap());
        let second = source.source_hash().await.unwrap();
        source.password = Some("other-secret".into());
        assert_ne!(second, source.source_hash().await.unwrap());
        let display = format!("{source:?}");
        assert!(!display.contains("bob"));
        assert!(!display.contains("secret"));
        let backup_hash = source.source_hash().await.unwrap();
        assert_ne!(backup_hash, SharedCifs(source).source_hash().await.unwrap());
    }

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

#[derive(Deserialize, Serialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct Cifs {
    pub hostname: String,
    pub path: PathBuf,
    pub username: String,
    pub password: Option<String>,
}

rpc_toolkit::reflect_ts!(Cifs);

impl std::fmt::Debug for Cifs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cifs")
            .field("hostname", &self.hostname)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Cifs {
    #[instrument(skip_all)]
    async fn mount_with_options(&self, mountpoint: &Path, options: &str) -> Result<(), Error> {
        tokio::fs::create_dir_all(mountpoint).await?;
        let ip = resolve_hostname(&self.hostname).await?;
        Command::new("mount")
            .arg("-t")
            .arg("cifs")
            .env("USER", &self.username)
            .env("PASSWD", self.password.as_deref().unwrap_or_default())
            .arg(format!(
                "//{}{}",
                ip,
                Path::new("/").join(&self.path).display()
            ))
            .arg(mountpoint)
            .arg("-o")
            .arg(options)
            .invoke(crate::ErrorKind::Filesystem)
            .await?;
        Ok(())
    }

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
        sha.update(
            serde_json::to_vec(self)
                .map_err(|error| Error::new(error, crate::ErrorKind::Serialization))?,
        );
        Ok(sha.finalize())
    }
}

pub struct SharedCifs(pub Cifs);

impl FileSystem for SharedCifs {
    async fn mount<P: AsRef<Path> + Send>(
        &self,
        mountpoint: P,
        mount_type: MountType,
    ) -> Result<(), Error> {
        tokio::time::timeout(std::time::Duration::from_secs(30), self.0.mount_with_options(
            mountpoint.as_ref(),
            &format!("vers=3,soft,echo_interval=5,noserverino,uid=100000,gid=100000,forceuid,forcegid,file_mode=0666,dir_mode=0777,{}", access_options(mount_type)),
        )).await.map_err(|error| Error::new(error, crate::ErrorKind::Timeout))?
    }

    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        let mut sha = Sha256::new();
        sha.update("SharedCifs");
        sha.update(self.0.source_hash().await?);
        Ok(sha.finalize())
    }
}
