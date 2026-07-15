use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Weak};
use std::time::Duration;

use futures::Future;
use futures::future::ready;
use imbl::{Vector, vector};
use imbl_value::InternedString;
use rpc_toolkit::{Empty, Server, ShutdownHandle};
use serde::de::DeserializeOwned;
use tokio::process::Command;
use tokio::sync::{Mutex, OnceCell, oneshot, watch};
use tracing::instrument;

use crate::context::RpcContext;
use crate::disk::mount::filesystem::bind::Bind;
use crate::disk::mount::filesystem::idmapped::{IdMap, IdMapped};
use crate::disk::mount::filesystem::loop_dev::LoopDev;
use crate::disk::mount::filesystem::overlayfs::OverlayGuard;
use crate::disk::mount::filesystem::{MountType, ReadOnly};
use crate::disk::mount::guard::{GenericMountGuard, MountGuard};
use crate::lxc::{HOST_RPC_SERVER_SOCKET, LxcConfig, LxcContainer};
use crate::net::net_controller::NetService;
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::s9pk::S9pk;
use crate::s9pk::merkle_archive::source::FileSource;
use crate::service::effects::context::EffectContext;
use crate::service::effects::handler;
use crate::service::rpc::{
    CallbackHandle, CallbackId, CallbackParams, ExitParams, InitKind, InitParams,
};
use crate::service::{ProcedureName, Service, rpc};
use crate::util::Invoke;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::create_file;
use crate::util::rpc_client::UnixRpcClient;
use crate::volume::data_dir;
use crate::{ARCH, DATA_DIR, ImageId, PACKAGE_DATA, VolumeId};

const RPC_CONNECT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug)]
pub struct ServiceState {
    // indicates whether the service container runtime has been initialized yet
    pub(super) rt_initialized: bool,
    // This tracks references to callbacks registered by the running service:
    pub(super) callbacks: BTreeSet<Arc<CallbackId>>,
}

impl ServiceState {
    pub fn new() -> Self {
        Self {
            rt_initialized: false,
            callbacks: Default::default(),
        }
    }
}

/// Want to have a wrapper for uses like the inject where we are going to be finding the subcontainer and doing some filtering on it.
/// As well, the imageName is also used for things like env.
pub struct Subcontainer {
    pub(super) name: InternedString,
    pub(super) image_id: ImageId,
    pub(super) overlay: OverlayGuard<Arc<MountGuard>>,
}

// @DRB On top of this we need to also have  the procedures to have the effects and get the results back for them, maybe lock them to the running instance?
/// This contains the LXC container running the javascript init system
/// that can be used via a JSON RPC Client connected to a unix domain
/// socket served by the container
pub struct PersistentContainer {
    pub(super) s9pk: S9pk,
    pub(super) lxc_container: OnceCell<LxcContainer>,
    pub(super) rpc_client: UnixRpcClient,
    pub(super) rpc_server: watch::Sender<Option<(NonDetachingJoinHandle<()>, ShutdownHandle)>>,
    js_mount: MountGuard,
    volumes: BTreeMap<VolumeId, MountGuard>,
    assets: Vec<MountGuard>,
    pub(super) images: BTreeMap<ImageId, Arc<MountGuard>>,
    pub(super) subcontainers: Arc<Mutex<BTreeMap<Guid, Subcontainer>>>,
    pub(super) state: Arc<watch::Sender<ServiceState>>,
    pub(super) net_service: NetService,
    destroyed: bool,
}

impl PersistentContainer {
    #[instrument(skip_all)]
    pub async fn new(ctx: &RpcContext, s9pk: S9pk) -> Result<Self, Error> {
        let lxc_container = ctx
            .lxc_manager
            .create(
                Some(
                    &Path::new(PACKAGE_DATA)
                        .join("logs")
                        .join(&s9pk.as_manifest().id),
                ),
                LxcConfig {
                    hardware_acceleration: s9pk.manifest.metadata.hardware_acceleration,
                    userspace_filesystems: s9pk.manifest.metadata.userspace_filesystems,
                    virtual_networking: s9pk.manifest.metadata.virtual_networking,
                },
            )
            .await?;
        let rpc_client = lxc_container.connect_rpc(Some(RPC_CONNECT_TIMEOUT)).await?;
        let js_mount = MountGuard::mount(
            &LoopDev::from(
                &**s9pk
                    .as_archive()
                    .contents()
                    .get_path("javascript.squashfs")
                    .and_then(|f| f.as_file())
                    .or_not_found("javascript")?,
            ),
            lxc_container.rootfs_dir().join("usr/lib/startos/package"),
            ReadOnly,
        )
        .await?;
        let is_compat = tokio::fs::metadata(js_mount.path().join("embassy.js"))
            .await
            .is_ok();

        let mut volumes = BTreeMap::new();

        // TODO: remove once packages are reconverted
        let added = if is_compat {
            ["embassy".parse().unwrap()].into_iter().collect()
        } else {
            BTreeSet::default()
        };
        for volume in s9pk.as_manifest().volumes.union(&added) {
            let mountpoint = lxc_container
                .rootfs_dir()
                .join("media/startos/volumes")
                .join(volume);
            let mount = MountGuard::mount(
                &IdMapped::new(
                    Bind::new(data_dir(DATA_DIR, &s9pk.as_manifest().id, volume)),
                    vec![IdMap {
                        from_id: 0,
                        to_id: 100000,
                        range: 65536,
                    }],
                ),
                mountpoint,
                MountType::ReadWrite,
            )
            .await?;
            volumes.insert(volume.clone(), mount);
        }

        let mountpoint = lxc_container.rootfs_dir().join("media/startos/assets");
        let assets = if let Some(sqfs) = s9pk
            .as_archive()
            .contents()
            .get_path("assets.squashfs")
            .and_then(|e| e.as_file())
        {
            vec![
                MountGuard::mount(
                    &IdMapped::new(
                        LoopDev::from(&**sqfs),
                        vec![IdMap {
                            from_id: 0,
                            to_id: 100000,
                            range: 65536,
                        }],
                    ),
                    mountpoint,
                    MountType::ReadWrite,
                )
                .await?,
            ]
        } else if let Some(dir) = s9pk
            .as_archive()
            .contents()
            .get_path("assets")
            .and_then(|e| e.as_directory())
        {
            // backwards compatibility for alpha s9pks - remove eventually
            let mut assets = Vec::new();
            for (asset, entry) in &**dir {
                let mountpoint = lxc_container
                    .rootfs_dir()
                    .join("media/startos/assets")
                    .join(Path::new(asset).with_extension(""));
                let Some(sqfs) = entry.as_file() else {
                    continue;
                };
                assets.push(
                    MountGuard::mount(
                        &IdMapped::new(
                            LoopDev::from(&**sqfs),
                            vec![IdMap {
                                from_id: 0,
                                to_id: 100000,
                                range: 65536,
                            }],
                        ),
                        mountpoint,
                        MountType::ReadWrite,
                    )
                    .await?,
                );
            }
            assets
        } else {
            Vec::new()
        };

        let mut images = BTreeMap::new();
        let image_path = lxc_container.rootfs_dir().join("media/startos/images");
        tokio::fs::create_dir_all(&image_path).await?;
        for (image, config) in &s9pk.as_manifest().images {
            let mut arch = ARCH;
            let mut sqfs_path = Path::new("images")
                .join(arch)
                .join(image)
                .with_extension("squashfs");
            if !s9pk
                .as_archive()
                .contents()
                .get_path(&sqfs_path)
                .and_then(|e| e.as_file())
                .is_some()
            {
                arch = if let Some(arch) = config.emulate_missing_as.as_deref() {
                    arch
                } else {
                    continue;
                };
                sqfs_path = Path::new("images")
                    .join(arch)
                    .join(image)
                    .with_extension("squashfs");
            }
            let sqfs = s9pk
                .as_archive()
                .contents()
                .get_path(&sqfs_path)
                .and_then(|e| e.as_file())
                .or_not_found(sqfs_path.display())?;
            let mountpoint = image_path.join(image);
            images.insert(
                image.clone(),
                Arc::new(
                    MountGuard::mount(
                        &IdMapped::new(
                            LoopDev::from(&**sqfs),
                            vec![IdMap {
                                from_id: 0,
                                to_id: 100000,
                                range: 65536,
                            }],
                        ),
                        &mountpoint,
                        ReadOnly,
                    )
                    .await?,
                ),
            );
            let env_filename = Path::new(image.as_ref()).with_extension("env");
            if let Some(env) = s9pk
                .as_archive()
                .contents()
                .get_path(Path::new("images").join(arch).join(&env_filename))
                .and_then(|e| e.as_file())
            {
                env.copy(&mut create_file(image_path.join(&env_filename)).await?)
                    .await?;
            }
            let json_filename = Path::new(image.as_ref()).with_extension("json");
            if let Some(json) = s9pk
                .as_archive()
                .contents()
                .get_path(Path::new("images").join(arch).join(&json_filename))
                .and_then(|e| e.as_file())
            {
                json.copy(&mut create_file(image_path.join(&json_filename)).await?)
                    .await?;
            }
        }
        let ip = lxc_container.ip().await?;
        let ipv6 = lxc_container.ipv6().await?;
        let net_service = ctx
            .net_controller
            .create_service(s9pk.as_manifest().id.clone(), ip, ipv6)
            .await?;
        if let Some(callbacks) = ctx.callbacks.get_container_ip(&s9pk.as_manifest().id) {
            callbacks
                .call(vector![Value::String(Arc::new(ip.to_string()))])
                .await?;
        }
        Ok(Self {
            s9pk,
            lxc_container: OnceCell::new_with(Some(lxc_container)),
            rpc_client,
            rpc_server: watch::channel(None).0,
            // procedures: Default::default(),
            js_mount,
            volumes,
            assets,
            images,
            subcontainers: Arc::new(Mutex::new(BTreeMap::new())),
            state: Arc::new(watch::channel(ServiceState::new()).0),
            net_service,
            destroyed: false,
        })
    }

    #[instrument(skip_all)]
    pub async fn mount_backup(
        &self,
        backup_path: impl AsRef<Path>,
        mount_type: MountType,
    ) -> Result<MountGuard, Error> {
        let backup_path = backup_path.as_ref();
        let mountpoint = self
            .lxc_container
            .get()
            .ok_or_else(|| {
                Error::new(
                    eyre!("{}", t!("service.persistent-container.container-destroyed")),
                    ErrorKind::Incoherent,
                )
            })?
            .rootfs_dir()
            .join("media/startos/backup");
        tokio::fs::create_dir_all(&mountpoint).await?;
        tokio::fs::create_dir_all(backup_path).await?;
        // Idmap the container's backup bind like a volume (on-disk 0-based
        // <-> container userns 100000+), so backup-fs stores container-space
        // ids and the container-root owns its backup tree. Replaces the old
        // chown-to-100000 + backup-fs's FUSE-internal idmapped-root.
        let bind = IdMapped::new(
            Bind::new(backup_path),
            vec![IdMap {
                from_id: 0,
                to_id: 100000,
                range: 65536,
            }],
        );
        MountGuard::mount(&bind, &mountpoint, mount_type).await
    }

    #[instrument(skip_all)]
    pub async fn init(
        &self,
        seed: Weak<Service>,
        procedure_id: Guid,
        kind: Option<InitKind>,
    ) -> Result<(), Error> {
        let socket_server_context = EffectContext::new(seed);
        let server = Server::new(move || ready(Ok(socket_server_context.clone())), handler());
        let path = self
            .lxc_container
            .get()
            .ok_or_else(|| {
                Error::new(
                    eyre!("{}", t!("service.persistent-container.container-destroyed")),
                    ErrorKind::Incoherent,
                )
            })?
            .rpc_dir()
            .join(HOST_RPC_SERVER_SOCKET);
        let (send, recv) = oneshot::channel();
        let handle = NonDetachingJoinHandle::from(tokio::spawn(async move {
            let chown_status = async {
                let res = server.run_unix(&path, |err| {
                    tracing::error!(
                        "{}",
                        t!(
                            "service.persistent-container.error-on-unix-socket",
                            path = path.display(),
                            error = err
                        )
                    )
                })?;
                Command::new("chown")
                    .arg("100000:100000")
                    .arg(&path)
                    .invoke(ErrorKind::Filesystem)
                    .await?;
                Ok::<_, Error>(res)
            };
            let (shutdown, fut) = match chown_status.await {
                Ok((shutdown, fut)) => (Ok(shutdown), Some(fut)),
                Err(e) => (Err(e), None),
            };
            if send.send(shutdown).is_err() {
                panic!("failed to send shutdown handle");
            }
            if let Some(fut) = fut {
                fut.await;
            }
        }));
        let shutdown = recv.await.map_err(|_| {
            Error::new(
                eyre!(
                    "{}",
                    t!("service.persistent-container.unix-socket-server-panicked")
                ),
                ErrorKind::Unknown,
            )
        })??;
        if self
            .rpc_server
            .send_replace(Some((handle, shutdown)))
            .is_some()
        {
            return Err(Error::new(
                eyre!("{}", t!("service.persistent-container.already-initialized")),
                ErrorKind::InvalidRequest,
            ));
        }

        self.rpc_client
            .request(
                rpc::Init,
                InitParams {
                    id: procedure_id,
                    kind,
                },
            )
            .await?;

        self.state.send_modify(|s| s.rt_initialized = true);

        Ok(())
    }

    #[instrument(skip_all)]
    fn destroy(
        &mut self,
        uninit: Option<ExitParams>,
    ) -> Option<impl Future<Output = Result<(), Error>> + 'static> {
        if self.destroyed {
            return None;
        }
        let version = self.s9pk.as_manifest().version.clone();
        let rpc_client = self.rpc_client.clone();
        let rpc_server = self.rpc_server.send_replace(None);
        let js_mount = self.js_mount.take();
        let volumes = std::mem::take(&mut self.volumes);
        let assets = std::mem::take(&mut self.assets);
        let images = std::mem::take(&mut self.images);
        let subcontainers = self.subcontainers.clone();
        let lxc_container = self.lxc_container.take();
        let net_service = std::mem::replace(&mut self.net_service, NetService::dummy());
        self.destroyed = true;
        Some(async move {
            let mut errs = ErrorCollection::new();
            if let Some((hdl, shutdown)) = rpc_server {
                errs.handle(
                    rpc_client
                        .request(
                            rpc::Exit,
                            uninit.unwrap_or_else(|| ExitParams::target_version(&*version)),
                        )
                        .await,
                );
                shutdown.shutdown();
                errs.handle(hdl.await.with_kind(ErrorKind::Cancelled));
            }
            net_service.remove_all().await.log_err();
            for (_, volume) in volumes {
                errs.handle(volume.unmount(true).await);
            }
            for assets in assets {
                errs.handle(assets.unmount(true).await);
            }
            for (_, overlay) in std::mem::take(&mut *subcontainers.lock().await) {
                errs.handle(overlay.overlay.unmount(true).await);
            }
            for (_, images) in images {
                errs.handle(images.unmount().await);
            }
            errs.handle(js_mount.unmount(true).await);
            if let Some(lxc_container) = lxc_container {
                errs.handle(lxc_container.exit().await);
            }
            errs.into_result()
        })
    }

    #[instrument(skip_all)]
    pub async fn exit(mut self, uninit: Option<ExitParams>) -> Result<(), Error> {
        if let Some(destroy) = self.destroy(uninit) {
            destroy.await?;
        }
        tracing::info!(
            "{}",
            t!(
                "service.persistent-container.service-exited",
                id = self.s9pk.as_manifest().id
            )
        );

        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn start(&self) -> Result<(), Error> {
        self.rpc_client.request(rpc::Start, Empty {}).await?;
        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn stop(&self) -> Result<(), Error> {
        self.rpc_client.request(rpc::Stop, Empty {}).await?;
        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn execute<O>(
        &self,
        id: Guid,
        name: ProcedureName,
        input: Value,
        timeout: Option<Duration>,
    ) -> Result<O, Error>
    where
        O: DeserializeOwned,
    {
        self._execute(id, name, input, timeout)
            .await
            .and_then(from_value)
    }

    #[instrument(skip_all)]
    pub async fn sanboxed<O>(
        &self,
        id: Guid,
        name: ProcedureName,
        input: Value,
        timeout: Option<Duration>,
    ) -> Result<O, Error>
    where
        O: DeserializeOwned,
    {
        self._sandboxed(id, name, input, timeout)
            .await
            .and_then(from_value)
    }

    #[instrument(skip_all)]
    pub async fn callback(&self, handle: CallbackHandle, args: Vector<Value>) -> Result<(), Error> {
        let mut params = None;
        self.state.send_if_modified(|s| {
            params = handle.params(&mut s.callbacks, args);
            params.is_some()
        });
        if let Some(params) = params {
            self._callback(params).await?;
        }
        Ok(())
    }

    #[instrument(skip_all)]
    async fn _execute(
        &self,
        id: Guid,
        name: ProcedureName,
        input: Value,
        timeout: Option<Duration>,
    ) -> Result<Value, Error> {
        let fut = self.rpc_client.request(
            rpc::Execute,
            rpc::ExecuteParams::new(id, name, input, timeout),
        );

        Ok(if let Some(timeout) = timeout {
            tokio::time::timeout(timeout, fut)
                .await
                .with_kind(ErrorKind::Timeout)??
        } else {
            fut.await?
        })
    }

    #[instrument(skip_all)]
    async fn _sandboxed(
        &self,
        id: Guid,
        name: ProcedureName,
        input: Value,
        timeout: Option<Duration>,
    ) -> Result<Value, Error> {
        let fut = self.rpc_client.request(
            rpc::Sandbox,
            rpc::ExecuteParams::new(id, name, input, timeout),
        );

        Ok(if let Some(timeout) = timeout {
            tokio::time::timeout(timeout, fut)
                .await
                .with_kind(ErrorKind::Timeout)??
        } else {
            fut.await?
        })
    }

    #[instrument(skip_all)]
    async fn _callback(&self, params: CallbackParams) -> Result<(), Error> {
        self.rpc_client.notify(rpc::Callback, params).await?;
        Ok(())
    }
}

impl Drop for PersistentContainer {
    fn drop(&mut self) {
        if let Some(destroy) = self.destroy(None) {
            tokio::spawn(async move { destroy.await.log_err() });
        }
    }
}
