use std::collections::{BTreeMap, BTreeSet};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::Path;
use std::sync::{Arc, Weak};
use std::time::Duration;

use clap::builder::ValueParserFactory;
use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use imbl_value::InternedString;
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{RpcRequest, RpcResponse};
use serde::{Deserialize, Serialize};
use tokio::fs::ReadDir;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::Instant;
use ts_rs::TS;

use crate::context::RpcContext;
use crate::disk::mount::filesystem::bind::Bind;
use crate::disk::mount::filesystem::block_dev::BlockDev;
use crate::disk::mount::filesystem::idmapped::{IdMap, IdMapped};
use crate::disk::mount::filesystem::overlayfs::OverlayGuard;
use crate::disk::mount::filesystem::{MountType, ReadOnly, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, MountGuard, TmpMountGuard};
use crate::disk::mount::util::unmount;
use crate::prelude::*;
use crate::rpc_continuations::{Guid, RpcContinuation};
use crate::service::ServiceStats;
use crate::util::io::{open_file, write_file_owned_atomic};
use crate::util::rpc_client::UnixRpcClient;
use crate::util::{FromStrParser, Invoke, new_guid};
use crate::{InvalidId, PackageId};

const LXC_CONTAINER_DIR: &str = "/var/lib/lxc";
const RPC_DIR: &str = "media/startos/rpc"; // must not be absolute path
pub const CONTAINER_RPC_SERVER_SOCKET: &str = "service.sock"; // must not be absolute path
pub const HOST_RPC_SERVER_SOCKET: &str = "host.sock"; // must not be absolute path
const CONTAINER_DHCP_TIMEOUT: Duration = Duration::from_secs(30);
const HARDWARE_ACCELERATION_PATHS: &[&str] = &["/dev/dri", "/dev/nvidia*", "/dev/kfd"];
// /dev/fuse: fuse-overlayfs storage, the only viable rootless storage driver
// inside a userns LXC (kernel overlayfs-on-overlayfs is denied for
// unprivileged users) — needed by rootless OCI engines (podman/docker) opted
// in via `manifest.userspaceFilesystems`.
const USERSPACE_FILESYSTEM_PATHS: &[&str] = &["/dev/fuse"];
// /dev/net/tun: kernel tunnel interface for VPN / WireGuard / tun-class
// services (and slirp4netns / pasta networking for nested containers) — opted
// in via `manifest.virtualNetworking`.
const VIRTUAL_NETWORKING_PATHS: &[&str] = &["/dev/net/tun"];

#[derive(
    Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq, PartialOrd, Ord, Hash, TS,
)]
#[ts(type = "string")]
pub struct ContainerId(InternedString);
impl std::ops::Deref for ContainerId {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::fmt::Display for ContainerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &*self.0)
    }
}
impl TryFrom<&str> for ContainerId {
    type Error = InvalidId;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(ContainerId(InternedString::intern(value)))
    }
}
impl std::str::FromStr for ContainerId {
    type Err = InvalidId;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s)
    }
}
impl ValueParserFactory for ContainerId {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        FromStrParser::new()
    }
}

#[derive(Default)]
pub struct LxcManager {
    containers: Mutex<Vec<Weak<ContainerId>>>,
}

impl LxcManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn create(
        self: &Arc<Self>,
        log_mount: Option<&Path>,
        config: LxcConfig,
    ) -> Result<LxcContainer, Error> {
        let mut guard = self.containers.lock().await;
        let container = tokio::time::timeout(
            Duration::from_secs(30),
            LxcContainer::new(self, log_mount, config),
        )
        .await
        .with_kind(ErrorKind::Timeout)??;
        *guard = std::mem::take(&mut *guard)
            .into_iter()
            .filter(|g| g.strong_count() > 0)
            .chain(std::iter::once(Arc::downgrade(&container.guid)))
            .collect();
        Ok(container)
    }

    pub async fn gc(&self) -> Result<(), Error> {
        let expected = BTreeSet::from_iter(
            self.containers
                .lock()
                .await
                .iter()
                .filter_map(|g| g.upgrade())
                .map(|g| (*g).clone()),
        );
        for container in String::from_utf8(
            Command::new("lxc-ls")
                .arg("-1")
                .invoke(ErrorKind::Lxc)
                .await?,
        )?
        .lines()
        .map(|s| s.trim())
        {
            if !expected.contains(&ContainerId::try_from(container)?) {
                let rootfs_path = Path::new(LXC_CONTAINER_DIR).join(container).join("rootfs");
                if tokio::fs::metadata(&rootfs_path).await.is_ok() {
                    unmount(
                        Path::new(LXC_CONTAINER_DIR).join(container).join("rootfs"),
                        true,
                    )
                    .await
                    .log_err();
                    if tokio_stream::wrappers::ReadDirStream::new(
                        tokio::fs::read_dir(&rootfs_path).await?,
                    )
                    .count()
                    .await
                        > 0
                    {
                        return Err(Error::new(
                            eyre!("{}", t!("lxc.mod.rootfs-not-empty")),
                            ErrorKind::InvalidRequest,
                        ));
                    }
                }
                Command::new("lxc-destroy")
                    .arg("--force")
                    .arg("--name")
                    .arg(container)
                    .invoke(ErrorKind::Lxc)
                    .await?;
            }
        }
        Ok(())
    }
}

pub struct LxcContainer {
    manager: Weak<LxcManager>,
    rootfs: OverlayGuard<TmpMountGuard>,
    pub guid: Arc<ContainerId>,
    rpc_bind: TmpMountGuard,
    log_mount: Option<MountGuard>,
    config: LxcConfig,
    exited: bool,
}
impl LxcContainer {
    async fn new(
        manager: &Arc<LxcManager>,
        log_mount: Option<&Path>,
        config: LxcConfig,
    ) -> Result<Self, Error> {
        let guid = new_guid();
        let lang = std::env::var("LANG").unwrap_or_else(|_| "C.UTF-8".into());
        let machine_id = hex::encode(rand::random::<[u8; 16]>());
        let container_dir = Path::new(LXC_CONTAINER_DIR).join(&*guid);
        tokio::fs::create_dir_all(&container_dir).await?;
        let config_str = format!(
            include_str!("./config.template"),
            guid = &*guid,
            lang = &lang,
        );
        tokio::fs::write(container_dir.join("config"), config_str).await?;
        let rootfs_dir = container_dir.join("rootfs");
        let rootfs = OverlayGuard::mount(
            TmpMountGuard::mount(
                &IdMapped::new(
                    BlockDev::new("/usr/lib/startos/container-runtime/rootfs.squashfs"),
                    vec![IdMap {
                        from_id: 0,
                        to_id: 100000,
                        range: 65536,
                    }],
                ),
                ReadOnly,
            )
            .await?,
            &rootfs_dir,
        )
        .await?;
        Command::new("chown")
            .arg("100000:100000")
            .arg(&rootfs_dir)
            .invoke(ErrorKind::Filesystem)
            .await?;
        write_file_owned_atomic(
            rootfs_dir.join("etc/machine-id"),
            format!("{machine_id}\n"),
            100000,
            100000,
        )
        .await?;
        write_file_owned_atomic(
            rootfs_dir.join("etc/hostname"),
            format!("{guid}\n"),
            100000,
            100000,
        )
        .await?;
        write_file_owned_atomic(
            rootfs_dir.join("etc/default/locale"),
            format!("LANG={lang}\n"),
            100000,
            100000,
        )
        .await?;
        Command::new("sed")
            .arg("-i")
            .arg(format!("s/LXC_NAME/{guid}/g"))
            .arg(rootfs_dir.join("etc/hosts"))
            .invoke(ErrorKind::Filesystem)
            .await?;
        Command::new("mount")
            .arg("--make-rshared")
            .arg(rootfs.path())
            .invoke(ErrorKind::Filesystem)
            .await?;
        let rpc_dir = rootfs_dir.join(RPC_DIR);
        tokio::fs::create_dir_all(&rpc_dir).await?;
        let rpc_bind = TmpMountGuard::mount(&Bind::new(rpc_dir), ReadWrite).await?;
        Command::new("chown")
            .arg("-R")
            .arg("100000:100000")
            .arg(rpc_bind.path())
            .invoke(ErrorKind::Filesystem)
            .await?;
        let log_mount = if let Some(path) = log_mount {
            let log_mount_point = rootfs_dir.join("var/log/journal").join(machine_id);
            let log_mount =
                MountGuard::mount(&Bind::new(path), &log_mount_point, MountType::ReadWrite).await?;
            Command::new("chown")
                // This was needed as 100999 because the group id of journald
                .arg("100000:100999")
                .arg(&log_mount_point)
                .invoke(crate::ErrorKind::Filesystem)
                .await?;
            match Command::new("chattr")
                .arg("-R")
                .arg("+C")
                .arg(&log_mount_point)
                .env("LANG", "C.UTF-8")
                .invoke(ErrorKind::Filesystem)
                .await
            {
                Ok(_) => Ok(()),
                Err(e) if e.source.to_string().contains("Operation not supported") => Ok(()),
                Err(e) => Err(e),
            }?;
            Some(log_mount)
        } else {
            None
        };
        Command::new("lxc-start")
            .arg("-d")
            .arg("--name")
            .arg(&*guid)
            .arg("-o")
            .arg(format!("/run/startos/LXC_{guid}.log"))
            .arg("-l")
            .arg("DEBUG")
            .invoke(ErrorKind::Lxc)
            .await?;
        let res = Self {
            manager: Arc::downgrade(manager),
            rootfs,
            guid: Arc::new(ContainerId::try_from(&*guid)?),
            rpc_bind,
            config,
            exited: false,
            log_mount,
        };
        if res.config.hardware_acceleration {
            res.handle_devices(
                tokio::fs::read_dir("/dev")
                    .await
                    .with_ctx(|_| (ErrorKind::Filesystem, "readdir /dev"))?,
                HARDWARE_ACCELERATION_PATHS,
            )
            .await?;
        }
        if res.config.userspace_filesystems {
            res.handle_devices(
                tokio::fs::read_dir("/dev")
                    .await
                    .with_ctx(|_| (ErrorKind::Filesystem, "readdir /dev"))?,
                USERSPACE_FILESYSTEM_PATHS,
            )
            .await?;
        }
        if res.config.virtual_networking {
            res.handle_devices(
                tokio::fs::read_dir("/dev")
                    .await
                    .with_ctx(|_| (ErrorKind::Filesystem, "readdir /dev"))?,
                VIRTUAL_NETWORKING_PATHS,
            )
            .await?;
        }
        Ok(res)
    }

    #[cfg(not(target_os = "linux"))]
    async fn handle_devices(&self, _: ReadDir, _: &[&str]) -> Result<(), Error> {
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn handle_devices<'a>(
        &'a self,
        mut dir: ReadDir,
        matches: &'a [&'a str],
    ) -> BoxFuture<'a, Result<(), Error>> {
        use std::os::linux::fs::MetadataExt;
        use std::os::unix::fs::FileTypeExt;
        async move {
            while let Some(ent) = dir.next_entry().await? {
                let path = ent.path();
                if let Some(matches) = if matches.is_empty() {
                    Some(Vec::new())
                } else {
                    let mut new_matches = Vec::new();
                    for mut m in matches.iter().copied() {
                        let could_match = if let Some(prefix) = m.strip_suffix("*") {
                            m = prefix;
                            path.to_string_lossy().starts_with(m)
                        } else {
                            path.starts_with(m)
                        } || Path::new(m).starts_with(&path);
                        if could_match {
                            new_matches.push(m);
                        }
                    }
                    if new_matches.is_empty() {
                        None
                    } else {
                        Some(new_matches)
                    }
                } {
                    let meta = ent.metadata().await?;
                    let ty = meta.file_type();
                    if ty.is_dir() {
                        self.handle_devices(
                            tokio::fs::read_dir(&path).await.with_ctx(|_| {
                                (ErrorKind::Filesystem, format!("readdir {path:?}"))
                            })?,
                            &matches,
                        )
                        .await?;
                    } else {
                        let ty = if ty.is_char_device() {
                            'c'
                        } else if ty.is_block_device() {
                            'b'
                        } else {
                            continue;
                        };
                        let rdev = meta.st_rdev();
                        let major = ((rdev >> 8) & 0xfff) as u32;
                        let minor = ((rdev & 0xff) | ((rdev >> 12) & 0xfff00)) as u32;
                        self.mknod(&path, ty, major, minor, meta.st_mode() & 0o7777)
                            .await?;
                    }
                }
            }
            Ok(())
        }
        .boxed()
    }

    pub fn rootfs_dir(&self) -> &Path {
        self.rootfs.path()
    }

    pub async fn ip(&self) -> Result<Ipv4Addr, Error> {
        let start = Instant::now();
        let guid: &str = &self.guid;
        loop {
            let output = String::from_utf8(
                Command::new("lxc-info")
                    .arg("--name")
                    .arg(guid)
                    .arg("-iH")
                    .invoke(ErrorKind::Docker)
                    .await?,
            )?;
            for line in output.lines() {
                if let Ok(ip) = line.trim().parse() {
                    return Ok(ip);
                }
            }
            if start.elapsed() > CONTAINER_DHCP_TIMEOUT {
                return Err(Error::new(
                    eyre!("{}", t!("lxc.mod.dhcp-timeout")),
                    ErrorKind::Timeout,
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// The container's SLAAC **ULA** (`fd00:3::/64` off lxcbr0), the DNAT target
    /// for a non-SSL GUA forward, or `None` if it never gets one.
    ///
    /// Deliberately the ULA only — StartOS is v4-parity for v6 (one server GUA
    /// port-forwarded to container ULAs), so a container never holds a GUA and
    /// any non-ULA address is ignored. Polls with the same retry loop as [`ip`]
    /// since SLAAC, like DHCP, lands a moment after boot. lxcbr0 always advertises
    /// the prefix, so kernel SLAAC gives every container a ULA — a timeout here
    /// should never happen short of a source-code bug, and is logged at error
    /// level. It's still non-fatal (unlike the mandatory v4 lease): a container
    /// runs fine on v4 without v6, so v6 forwarding is skipped rather than failing
    /// startup.
    ///
    /// [`ip`]: Self::ip
    pub async fn ipv6(&self) -> Result<Option<Ipv6Addr>, Error> {
        let start = Instant::now();
        let guid: &str = &self.guid;
        loop {
            let output = String::from_utf8(
                Command::new("lxc-info")
                    .arg("--name")
                    .arg(guid)
                    .arg("-iH")
                    .invoke(ErrorKind::Docker)
                    .await?,
            )?;
            if let Some(ula) = output.lines().find_map(|line| {
                line.trim()
                    .parse::<Ipv6Addr>()
                    .ok()
                    .filter(|ip| crate::net::utils::ipv6_is_ula(*ip))
            }) {
                return Ok(Some(ula));
            }
            if start.elapsed() > CONTAINER_DHCP_TIMEOUT {
                tracing::error!(
                    "container {guid} has no IPv6 ULA after SLAAC timeout — lxcbr0 always advertises the prefix, so this indicates a bug; v6 forwarding disabled for it"
                );
                return Ok(None);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub fn rpc_dir(&self) -> &Path {
        self.rpc_bind.path()
    }

    pub async fn command(&self, commands: &[&str]) -> Result<String, Error> {
        let mut cmd = Command::new("lxc-attach");
        cmd.kill_on_drop(true);

        let output = cmd
            .arg(&**self.guid)
            .arg("--")
            .args(commands)
            .output()
            .await?;

        if !output.status.success() {
            return Err(Error::new(
                eyre!(
                    "{}",
                    t!(
                        "lxc.mod.command-failed",
                        code = format!("{:?}", output.status.code()),
                        message = format!("{:?}", String::from_utf8(output.stderr))
                    )
                ),
                ErrorKind::Docker,
            ));
        }
        Ok(String::from_utf8(output.stdout)?)
    }

    #[instrument(skip_all)]
    pub async fn exit(mut self) -> Result<(), Error> {
        Command::new("lxc-stop")
            .arg("--name")
            .arg(&**self.guid)
            .invoke(ErrorKind::Lxc)
            .await?;
        self.rpc_bind.take().unmount().await?;
        if let Some(log_mount) = self.log_mount.take() {
            log_mount.unmount(false).await?;
        }
        self.rootfs.take().unmount(true).await?;
        let rootfs_path = self.rootfs_dir();
        if tokio::fs::metadata(&rootfs_path).await.is_ok()
            && tokio_stream::wrappers::ReadDirStream::new(tokio::fs::read_dir(&rootfs_path).await?)
                .count()
                .await
                > 0
        {
            return Err(Error::new(
                eyre!("{}", t!("lxc.mod.rootfs-not-empty")),
                ErrorKind::InvalidRequest,
            ));
        }
        Command::new("lxc-destroy")
            .arg("--force")
            .arg("--name")
            .arg(&**self.guid)
            .invoke(ErrorKind::Lxc)
            .await?;

        #[allow(unused_assignments)]
        {
            self.exited = true;
        }

        Ok(())
    }

    pub async fn connect_rpc(&self, timeout: Option<Duration>) -> Result<UnixRpcClient, Error> {
        let started = Instant::now();
        let sock_path = self.rpc_dir().join(CONTAINER_RPC_SERVER_SOCKET);
        while tokio::fs::metadata(&sock_path).await.is_err() {
            if timeout.map_or(false, |t| started.elapsed() > t) {
                tracing::error!(
                    "{:?}",
                    Command::new("lxc-attach")
                        .arg(&**self.guid)
                        .arg("--")
                        .arg("systemctl")
                        .arg("status")
                        .arg("container-runtime")
                        .invoke(ErrorKind::Unknown)
                        .await
                );
                return Err(Error::new(
                    eyre!("{}", t!("lxc.mod.socket-timeout")),
                    ErrorKind::Timeout,
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        tracing::info!(
            "{}",
            t!(
                "lxc.mod.connected-to-socket",
                elapsed = format!("{:?}", started.elapsed())
            )
        );
        Ok(UnixRpcClient::new(sock_path))
    }

    pub async fn mknod(
        &self,
        path: &Path,
        ty: char,
        major: u32,
        minor: u32,
        mode: u32,
    ) -> Result<(), Error> {
        // -m so the node carries the host's mode instead of `0666 & ~umask(startd)`.
        let mode = format!("{mode:04o}");
        if let Ok(dev_rel) = path.strip_prefix("/dev") {
            let parent = dev_rel.parent();
            let media_dev = self.rootfs_dir().join("media/startos/dev");
            let target_path = media_dev.join(dev_rel);
            if tokio::fs::metadata(&target_path).await.is_ok() {
                return Ok(());
            }
            if let Some(parent) = parent {
                let p = media_dev.join(parent);
                tokio::fs::create_dir_all(&p)
                    .await
                    .with_ctx(|_| (ErrorKind::Filesystem, format!("mkdir -p {p:?}")))?;
                for p in parent.ancestors() {
                    Command::new("chown")
                        .arg("100000:100000")
                        .arg(media_dev.join(p))
                        .invoke(ErrorKind::Filesystem)
                        .await?;
                }
            }
            Command::new("mknod")
                .arg("-m")
                .arg(&mode)
                .arg(&target_path)
                .arg(&*InternedString::from_display(&ty))
                .arg(&*InternedString::from_display(&major))
                .arg(&*InternedString::from_display(&minor))
                .invoke(ErrorKind::Filesystem)
                .await?;
            Command::new("chown")
                .arg("100000:100000")
                .arg(&target_path)
                .invoke(ErrorKind::Filesystem)
                .await?;
            if let Some(parent) = parent {
                Command::new("lxc-attach")
                    .arg(&**self.guid)
                    .arg("--")
                    .arg("mkdir")
                    .arg("-p")
                    .arg(Path::new("/dev").join(parent))
                    .invoke(ErrorKind::Lxc)
                    .await?;
            }
            Command::new("lxc-attach")
                .arg(&**self.guid)
                .arg("--")
                .arg("touch")
                .arg(&path)
                .invoke(ErrorKind::Lxc)
                .await?;
            Command::new("lxc-attach")
                .arg(&**self.guid)
                .arg("--")
                .arg("mount")
                .arg("--bind")
                .arg(Path::new("/media/startos/dev").join(dev_rel))
                .arg(&path)
                .invoke(ErrorKind::Lxc)
                .await?;
        } else {
            let target_path = self
                .rootfs_dir()
                .join(path.strip_prefix("/").unwrap_or(&path));
            if tokio::fs::metadata(&target_path).await.is_ok() {
                return Ok(());
            }
            Command::new("mknod")
                .arg("-m")
                .arg(&mode)
                .arg(&target_path)
                .arg(&*InternedString::from_display(&ty))
                .arg(&*InternedString::from_display(&major))
                .arg(&*InternedString::from_display(&minor))
                .invoke(ErrorKind::Filesystem)
                .await?;
            Command::new("chown")
                .arg("100000:100000")
                .arg(&target_path)
                .invoke(ErrorKind::Filesystem)
                .await?;
        }
        Ok(())
    }
}
impl Drop for LxcContainer {
    fn drop(&mut self) {
        if !self.exited {
            tracing::warn!(
                "{}",
                t!(
                    "lxc.mod.container-ungracefully-dropped",
                    container = &**self.guid
                )
            );
            let rootfs = self.rootfs.take();
            let guid = std::mem::take(&mut self.guid);
            if let Some(manager) = self.manager.upgrade() {
                tokio::spawn(async move {
                    if let Err(e) = async {
                        let err_path = rootfs.path().join("var/log/containerRuntime.err");
                        if tokio::fs::metadata(&err_path).await.is_ok() {
                            let mut lines = BufReader::new(open_file(&err_path).await?).lines();
                            while let Some(line) = lines.next_line().await? {
                                let container = &**guid;
                                tracing::error!(container, "{}", line);
                            }
                        }
                        Ok::<_, Error>(())
                    }
                    .await
                    {
                        tracing::error!(
                            "{}",
                            t!("lxc.mod.error-reading-crashed-logs", error = e.to_string())
                        );
                        tracing::debug!("{e:?}")
                    }
                    rootfs.unmount(true).await.log_err();
                    drop(guid);
                    if let Err(e) = manager.gc().await {
                        tracing::error!(
                            "{}",
                            t!(
                                "lxc.mod.error-cleaning-up-containers",
                                error = e.to_string()
                            )
                        );
                        tracing::debug!("{e:?}")
                    } else {
                        tracing::info!("{}", t!("lxc.mod.cleaned-up-containers"));
                    }
                });
            }
        }
    }
}

#[derive(Default, Serialize)]
pub struct LxcConfig {
    pub hardware_acceleration: bool,
    pub userspace_filesystems: bool,
    pub virtual_networking: bool,
}

pub async fn connect(ctx: &RpcContext, container: &LxcContainer) -> Result<Guid, Error> {
    use axum::extract::ws::Message;

    let rpc = container.connect_rpc(Some(Duration::from_secs(30))).await?;
    let guid = Guid::new();
    ctx.rpc_continuations
        .add(
            guid.clone(),
            RpcContinuation::ws(
                |mut ws| async move {
                    if let Err(e) = async {
                        loop {
                            match ws.recv().await {
                                None => break,
                                Some(Ok(Message::Text(txt))) => {
                                    let mut id = None;
                                    let result = async {
                                        let req: RpcRequest =
                                            serde_json::from_str(&txt).map_err(|e| RpcError {
                                                data: Some(serde_json::Value::String(
                                                    e.to_string(),
                                                )),
                                                ..rpc_toolkit::yajrc::PARSE_ERROR
                                            })?;
                                        id = req.id;
                                        rpc.request(req.method, req.params).await
                                    }
                                    .await;
                                    ws.send(Message::Text(
                                        serde_json::to_string(&RpcResponse { id, result })
                                            .with_kind(ErrorKind::Serialization)?
                                            .into(),
                                    ))
                                    .await
                                    .with_kind(ErrorKind::Network)?;
                                }
                                Some(Ok(_)) => (),
                                Some(Err(e)) => {
                                    return Err(Error::new(e, ErrorKind::Network));
                                }
                            }
                        }
                        Ok::<_, Error>(())
                    }
                    .await
                    {
                        tracing::error!("{e}");
                        tracing::debug!("{e:?}");
                    }
                },
                Duration::from_secs(30),
            ),
        )
        .await;
    Ok(guid)
}

pub async fn stats(ctx: RpcContext) -> Result<BTreeMap<PackageId, Option<ServiceStats>>, Error> {
    let ids = ctx.db.peek().await.as_public().as_package_data().keys()?;

    let mut stats = BTreeMap::new();
    for id in ids {
        let Some(service) = ctx.services.try_get(&id) else {
            stats.insert(id, None);
            continue;
        };

        let Some(service_ref) = service.as_ref() else {
            stats.insert(id, None);
            continue;
        };

        stats.insert(id, Some(service_ref.stats().await?));
    }
    Ok(stats)
}
