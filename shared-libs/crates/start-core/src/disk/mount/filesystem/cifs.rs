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

pub(crate) async fn locations_overlap(
    source: (&str, &Path),
    destination: (&str, &Path),
) -> Result<bool, Error> {
    let components = |path: &Path| {
        path.components()
            .filter_map(|part| match part {
                std::path::Component::Normal(name) => Some(name.to_string_lossy().to_lowercase()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let source_path = components(source.1);
    let destination_path = components(destination.1);
    if !source_path.starts_with(&destination_path) && !destination_path.starts_with(&source_path) {
        return Ok(false);
    }
    if source.0.eq_ignore_ascii_case(destination.0) {
        return Ok(true);
    }
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let (source, destination) =
            tokio::try_join!(resolve_hostname(source.0), resolve_hostname(destination.0))?;
        Ok(source.to_canonical() == destination.to_canonical())
    })
    .await
    .map_err(|error| Error::new(error, crate::ErrorKind::Timeout))?
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
    async fn overlapping_locations_compare_share_components_and_resolved_endpoints() {
        for (source, destination, expected) in [
            ("share", "/share", true),
            ("/share", "/share/StartOSBackupsV2/server/crypt", true),
            ("/SHARE/backups/old", "/share/backups", true),
            ("/share/photos", "/share/photos-archive", false),
            ("/share/photos", "/share/backups", false),
            ("/share", "/other-share", false),
        ] {
            assert_eq!(
                locations_overlap(("NAS", Path::new(source)), ("nas", Path::new(destination)))
                    .await
                    .unwrap(),
                expected
            );
        }
        assert!(
            locations_overlap(
                ("2001:0db8:0:0:0:0:0:10", Path::new("/share")),
                ("2001:db8::10", Path::new("/share/backups"))
            )
            .await
            .unwrap()
        );
        assert!(
            !locations_overlap(
                ("192.0.2.1", Path::new("/share")),
                ("192.0.2.2", Path::new("/share/backups"))
            )
            .await
            .unwrap()
        );
    }

    #[tokio::test]
    #[ignore = "requires the isolated SMB fixture and root in a private mount namespace"]
    async fn real_cifs_handles_auth_guest_outage_cancellation_and_recovery() {
        use std::os::unix::fs::MetadataExt;
        use std::time::{Duration, Instant};

        use crate::disk::mount::filesystem::bind::Bind;
        use crate::disk::mount::guard::MountGuard;
        use crate::rpc_continuations::Guid;
        use crate::shared_volumes::SharedVolumeId;
        use crate::shared_volumes::mounts::{PreparedSource, SharedVolumeMounts};
        assert_eq!(std::env::var("VOLUME_CIFS_FIXTURE").as_deref(), Ok("1"));
        fn definition(hostname: &str, path: &str) -> SharedCifs {
            SharedCifs(Cifs {
                hostname: hostname.into(),
                path: path.into(),
                username: "start9".into(),
                password: Some("fixture-only".into()),
            })
        }
        fn rule(ip: &str) -> [&str; 10] {
            [
                "OUTPUT", "-d", ip, "-p", "tcp", "--dport", "445", "-j", "DROP", "--wait",
            ]
        }
        struct BlockTraffic(Vec<&'static str>);
        impl Drop for BlockTraffic {
            fn drop(&mut self) {
                for ip in &self.0 {
                    let _ = std::process::Command::new("iptables")
                        .arg("-D")
                        .args(rule(ip))
                        .output();
                }
            }
        }
        let source = TmpMountGuard::mount(&definition("127.0.0.1", "/volume"), ReadWrite)
            .await
            .unwrap();
        tokio::fs::write(source.path().join("sentinel"), b"remote contents")
            .await
            .unwrap();
        assert_eq!(
            tokio::fs::metadata(source.path().join("sentinel"))
                .await
                .unwrap()
                .uid(),
            100000
        );
        let mut guest = definition("127.0.0.1", "/guest");
        guest.0.username.clear();
        guest.0.password = None;
        let guest = TmpMountGuard::mount(&guest, ReadOnly).await.unwrap();
        assert_eq!(
            tokio::fs::read(guest.path().join("sentinel"))
                .await
                .unwrap(),
            b"remote contents"
        );
        guest.unmount().await.unwrap();
        let fixture = tempfile::tempdir().unwrap();
        tokio::fs::create_dir(fixture.path().join("data"))
            .await
            .unwrap();
        let tree = MountGuard::mount(&Bind::new(fixture.path()), fixture.path(), ReadWrite)
            .await
            .unwrap();
        let mut mounts = SharedVolumeMounts::default();
        let guid = Guid::new();
        let location = mounts
            .prepare(
                &guid,
                fixture.path(),
                Path::new("data"),
                std::sync::Arc::new(
                    crate::disk::mount::filesystem::syscall::MountNamespace::for_process(
                        std::process::id(),
                    )
                    .unwrap(),
                ),
            )
            .await
            .unwrap();
        mounts
            .attach(
                &guid,
                location,
                SharedVolumeId::new(),
                PreparedSource::open(source.clone(), None).await.unwrap(),
                true,
            )
            .unwrap();
        assert!(
            tokio::fs::write(fixture.path().join("data/forbidden"), b"no")
                .await
                .is_err()
        );
        let mount_info = String::from_utf8(
            Command::new("unshare")
                .args([
                    "--user",
                    "--map-root-user",
                    "--mount",
                    "cat",
                    "/proc/self/mountinfo",
                ])
                .invoke(crate::ErrorKind::Filesystem)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(!mount_info.contains("fixture-only"));
        std::println!(
            "CIFS username visible in mount metadata: {}",
            mount_info.contains("username=start9")
        );
        let mut blocked = BlockTraffic(Vec::new());
        for ip in ["127.0.0.1", "127.0.0.2"] {
            Command::new("iptables")
                .arg("-I")
                .args(rule(ip))
                .invoke(crate::ErrorKind::Network)
                .await
                .unwrap();
            blocked.0.push(ip);
        }
        let began = Instant::now();
        let io = tokio::time::timeout(
            Duration::from_secs(40),
            tokio::fs::write(source.path().join("offline-write"), b"offline"),
        )
        .await;
        assert!(io.is_ok(), "soft source I/O did not return within40seconds");
        assert!(io.unwrap().is_err());
        std::println!(
            "Unavailable source I/O returned after {:?}",
            began.elapsed()
        );
        let unavailable = definition("127.0.0.2", "/volume");
        assert!(
            tokio::time::timeout(
                Duration::from_millis(100),
                TmpMountGuard::mount(&unavailable, ReadWrite)
            )
            .await
            .is_err()
        );
        let began = Instant::now();
        let error = TmpMountGuard::mount(&unavailable, ReadWrite)
            .await
            .unwrap_err();
        assert!(
            matches!(
                error.kind,
                crate::ErrorKind::Timeout | crate::ErrorKind::Filesystem
            ),
            "{error}"
        );
        assert!(began.elapsed() < Duration::from_secs(33));
        drop(blocked);
        let recovered = TmpMountGuard::mount(&unavailable, ReadWrite).await.unwrap();
        tokio::fs::write(recovered.path().join("recovered"), b"recovered")
            .await
            .unwrap();
        let recovered_path = recovered.path().to_owned();
        recovered.unmount().await.unwrap();
        assert!(
            !crate::disk::mount::util::is_mountpoint(&recovered_path)
                .await
                .unwrap(),
            "cancelled helper left a late mount"
        );
        mounts.clear();
        source.unmount().await.unwrap();
        tree.unmount(false).await.unwrap();
    }

    #[test]
    fn invalid_sources_are_rejected_before_network_access() {
        for (hostname, path, password) in [
            ("", "/share", None),
            ("//nas", "/share", None),
            ("nas", "/", None),
            ("nas", "/share/../other", None),
            ("nas", "/share\0", None),
            ("nas", "/share", Some("secret\0")),
        ] {
            let error = Cifs {
                hostname: hostname.into(),
                path: path.into(),
                username: "guest".into(),
                password: password.map(str::to_owned),
            }
            .validate()
            .unwrap_err();
            assert!(matches!(error.kind, crate::ErrorKind::InvalidRequest));
            assert!(!error.to_string().contains("secret"));
        }
        assert!(
            Cifs {
                hostname: "nas.lan".into(),
                path: "/Family Photos/albums".into(),
                username: "".into(),
                password: None
            }
            .validate()
            .is_ok()
        );
    }

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
    pub fn validate(&self) -> Result<(), Error> {
        use std::path::Component;
        let path = self.path.as_os_str().as_encoded_bytes();
        if self.hostname.trim().is_empty()
            || self.hostname.contains(['/', '\\', '\0'])
            || path.contains(&0)
            || !self
                .path
                .components()
                .any(|part| matches!(part, Component::Normal(_)))
            || self
                .path
                .components()
                .any(|part| matches!(part, Component::ParentDir | Component::Prefix(_)))
            || self.username.contains('\0')
            || self
                .password
                .as_deref()
                .is_some_and(|password| password.contains('\0'))
        {
            return Err(Error::new(
                eyre!(t!("disk.mount.filesystem.cifs.invalid-source")),
                crate::ErrorKind::InvalidRequest,
            ));
        }
        Ok(())
    }

    #[instrument(skip_all)]
    async fn mount_with_options(&self, mountpoint: &Path, options: &str) -> Result<(), Error> {
        self.validate()?;
        tokio::fs::create_dir_all(mountpoint).await?;
        let ip = resolve_hostname(&self.hostname).await?;
        Command::new("mount.cifs")
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
            &format!("vers=3,soft,echo_interval=5,noserverino,uid={0},gid={0},forceuid,forcegid,file_mode=0666,dir_mode=0777,{1}", crate::lxc::BASE_IDMAP.to_id, access_options(mount_type)),
        )).await.map_err(|error| Error::new(error, crate::ErrorKind::Timeout))?
    }

    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        let mut sha = Sha256::new();
        sha.update("SharedCifs");
        sha.update(self.0.source_hash().await?);
        Ok(sha.finalize())
    }
}
