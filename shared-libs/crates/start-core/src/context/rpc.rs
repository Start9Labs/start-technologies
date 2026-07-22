use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::future::Future;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::{TimeDelta, Utc};
use imbl::OrdMap;
use imbl_value::InternedString;
use josekit::jwk::Jwk;
use reqwest::{Client, Proxy};
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{CallRemote, Context, Empty};
use tokio::process::Command;
use tokio::sync::{RwLock, broadcast, oneshot, watch};
use tokio::time::Instant;
use tracing::instrument;

use super::setup::CURRENT_SECRET;
use crate::account::AccountInfo;
use crate::auth::AuthKeys;
use crate::context::config::ServerConfig;
use crate::db::model::Database;
use crate::disk::OsPartitionInfo;
use crate::disk::mount::filesystem::bind::Bind;
use crate::disk::mount::filesystem::block_dev::BlockDev;
use crate::disk::mount::filesystem::{FileSystem, ReadOnly};
use crate::disk::mount::guard::MountGuard;
use crate::init::{InitResult, check_time_is_synchronized};
use crate::install::PKG_ARCHIVE_DIR;
use crate::lxc::LxcManager;
use crate::middleware::auth::signature::{HasUnenrolledKeys, NonceCache};
use crate::net::gateway::WildcardListener;
use crate::net::net_controller::{NetController, NetService};
use crate::net::socks::DEFAULT_SOCKS_LISTEN;
use crate::net::web_server::WebServerAcceptorSetter;
use crate::net::wifi::WpaCli;
use crate::prelude::*;
use crate::progress::{FullProgressTracker, PhaseProgressTrackerHandle};
use crate::rpc_continuations::{Guid, OpenAuthedContinuations, RpcContinuations};
use crate::service::ServiceMap;
use crate::service::effects::callbacks::ServiceCallbacks;
use crate::service::effects::subcontainer::NVIDIA_OVERLAY_PATH;
use crate::shutdown::Shutdown;
use crate::util::Invoke;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::{TmpDir, delete_file};
use crate::util::lshw::LshwDevice;
use crate::util::sync::{SyncMutex, SyncRwLock, Watch};
use crate::{DATA_DIR, PLATFORM, PackageId};

pub struct RpcContextSeed {
    is_closed: AtomicBool,
    closed: watch::Sender<bool>,
    pub os_partitions: OsPartitionInfo,
    pub disk_guid: InternedString,
    pub ephemeral_auth_keys: SyncMutex<AuthKeys>,
    pub auth_sig_nonce_cache: SyncMutex<NonceCache>,
    pub db: TypedPatchDb<Database>,
    pub sync_db: watch::Sender<u64>,
    pub account: SyncRwLock<AccountInfo>,
    pub os_net_service: NetService,
    pub net_controller: Arc<NetController>,
    pub s9pk_arch: Option<&'static str>,
    pub services: ServiceMap,
    pub cancellable_installs: SyncMutex<BTreeMap<PackageId, oneshot::Sender<()>>>,
    pub metrics_cache: Watch<Option<crate::system::Metrics>>,
    pub shutdown: broadcast::Sender<Option<Shutdown>>,
    pub lxc_manager: Arc<LxcManager>,
    pub open_authed_continuations: OpenAuthedContinuations<Option<InternedString>>,
    pub rpc_continuations: RpcContinuations,
    pub callbacks: Arc<ServiceCallbacks>,
    pub wifi_manager: RwLock<Option<WpaCli>>,
    pub current_secret: Arc<Jwk>,
    pub client: Client,
    pub start_time: Instant,
    pub crons: SyncMutex<BTreeMap<Guid, NonDetachingJoinHandle<()>>>,
}
impl Drop for RpcContextSeed {
    fn drop(&mut self) {
        tracing::info!("{}", t!("context.rpc.rpc-context-dropped"));
    }
}

pub struct Hardware {
    pub devices: Vec<LshwDevice>,
    pub ram: u64,
}

pub struct InitRpcContextPhases {
    load_db: PhaseProgressTrackerHandle,
    init_net_ctrl: PhaseProgressTrackerHandle,
    cleanup_init: CleanupInitPhases,
    run_migrations: PhaseProgressTrackerHandle,
}
impl InitRpcContextPhases {
    pub fn new(handle: &FullProgressTracker) -> Self {
        Self {
            load_db: handle.add_phase("Loading database".into(), Some(5)),
            init_net_ctrl: handle.add_phase("Initializing network".into(), Some(1)),
            cleanup_init: CleanupInitPhases::new(handle),
            run_migrations: handle.add_phase("Running migrations".into(), Some(10)),
        }
    }
}

pub struct CleanupInitPhases {
    cleanup_sessions: PhaseProgressTrackerHandle,
    init_services: PhaseProgressTrackerHandle,
    prune_s9pks: PhaseProgressTrackerHandle,
}
impl CleanupInitPhases {
    pub fn new(handle: &FullProgressTracker) -> Self {
        Self {
            cleanup_sessions: handle.add_phase("Cleaning up sessions".into(), Some(1)),
            init_services: handle.add_phase("Initializing services".into(), Some(10)),
            prune_s9pks: handle.add_phase("Pruning S9PKs".into(), Some(1)),
        }
    }
}

#[derive(Clone)]
pub struct RpcContext(Arc<RpcContextSeed>);

/// Drop enrolled keys idle for more than 30 days. No-op until the clock is
/// NTP-synced, so a wrong boot-time clock can't reap live sessions.
fn reap_idle_sessions(
    continuations: &OpenAuthedContinuations<Option<InternedString>>,
    ephemeral: &SyncMutex<AuthKeys>,
    db: &mut Model<Database>,
) -> Result<(), Error> {
    if !db.as_public().as_server_info().as_ntp_synced().de()? {
        return Ok(());
    }
    let now = Utc::now();
    let expired = db
        .as_private()
        .as_session_pubkeys()
        .as_entries()?
        .into_iter()
        .map(|(id, session)| Ok::<_, Error>((id, session.de()?.last_active)))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|(_, last_active)| now - *last_active > TimeDelta::days(30))
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    HasUnenrolledKeys::unenroll::<RpcContext>(continuations, Some(ephemeral), db, expired)?;
    Ok(())
}

impl RpcContext {
    #[instrument(skip_all)]
    pub async fn init(
        webserver: &WebServerAcceptorSetter<WildcardListener>,
        config: &ServerConfig,
        disk_guid: InternedString,
        init_result: Option<InitResult>,
        InitRpcContextPhases {
            mut load_db,
            mut init_net_ctrl,
            cleanup_init,
            run_migrations,
        }: InitRpcContextPhases,
    ) -> Result<Self, Error> {
        let socks_proxy = config.socks_listen.unwrap_or(DEFAULT_SOCKS_LISTEN);
        let (shutdown, _) = tokio::sync::broadcast::channel(1);

        load_db.start();
        let db = if let Some(InitResult { net_ctrl, .. }) = &init_result {
            net_ctrl.db.clone()
        } else {
            TypedPatchDb::<Database>::load(config.db().await?).await?
        };
        let peek = db.peek().await;
        let account = AccountInfo::load(&peek)?;
        load_db.complete();
        tracing::info!("{}", t!("context.rpc.opened-patchdb"));

        init_net_ctrl.start();
        let (net_controller, os_net_service) = if let Some(InitResult {
            net_ctrl,
            os_net_service,
        }) = init_result
        {
            (net_ctrl, os_net_service)
        } else {
            let net_ctrl = Arc::new(
                NetController::init(
                    db.clone(),
                    socks_proxy,
                    config
                        .max_proxy_conns_per_target
                        .unwrap_or(crate::net::vhost::MAX_PROXY_CONNS_PER_TARGET),
                )
                .await?,
            );
            webserver.send_modify(|wl| wl.set_ip_info(net_ctrl.net_iface.watcher.subscribe()));
            let os_net_service = net_ctrl.os_bindings().await?;
            (net_ctrl, os_net_service)
        };
        init_net_ctrl.complete();
        tracing::info!("{}", t!("context.rpc.initialized-net-controller"));

        if PLATFORM.ends_with("-nvidia") {
            if let Err(e) = Command::new("nvidia-smi")
                .invoke(ErrorKind::ParseSysInfo)
                .await
            {
                tracing::warn!("{}", t!("context.rpc.nvidia-smi-error", error = e));
                tracing::info!("{}", t!("context.rpc.nvidia-warning-can-be-ignored"));
            } else {
                async {
                    let version: InternedString = String::from_utf8(
                        Command::new("modinfo")
                            .arg("-F")
                            .arg("version")
                            .arg("nvidia")
                            .invoke(ErrorKind::ParseSysInfo)
                            .await?,
                    )?
                    .trim()
                    .into();

                    let nvidia_dir =
                        Path::new("/media/startos/data/package-data/nvidia").join(&*version);

                    // Generate single squashfs with both debian and generic overlays
                    let sqfs = nvidia_dir.join("container-overlay.squashfs");
                    if tokio::fs::metadata(&sqfs).await.is_err() {
                        let tmp = TmpDir::new().await?;

                        // Generate debian overlay (libs in /usr/lib/aarch64-linux-gnu/)
                        let debian_dir = tmp.join("debian");
                        tokio::fs::create_dir_all(&debian_dir).await?;
                        // Create /etc/debian_version to trigger debian path detection
                        tokio::fs::create_dir_all(debian_dir.join("etc")).await?;
                        tokio::fs::write(debian_dir.join("etc/debian_version"), "").await?;
                        let procfs = MountGuard::mount(
                            &Bind::new("/proc"),
                            debian_dir.join("proc"),
                            ReadOnly,
                        )
                        .await?;
                        Command::new("nvidia-container-cli")
                            .arg("configure")
                            .arg("--no-devbind")
                            .arg("--no-cgroups")
                            .arg("--utility")
                            .arg("--compute")
                            .arg("--graphics")
                            .arg("--video")
                            .arg(&debian_dir)
                            .invoke(ErrorKind::Unknown)
                            .await?;
                        procfs.unmount(true).await?;
                        // Run ldconfig to create proper symlinks for all NVIDIA libraries
                        Command::new("ldconfig")
                            .arg("-r")
                            .arg(&debian_dir)
                            .invoke(ErrorKind::Unknown)
                            .await?;
                        // Remove /etc/debian_version - it was only needed for nvidia-container-cli detection
                        tokio::fs::remove_file(debian_dir.join("etc/debian_version")).await?;

                        // Generate generic overlay (libs in /usr/lib64/)
                        let generic_dir = tmp.join("generic");
                        tokio::fs::create_dir_all(&generic_dir).await?;
                        // No /etc/debian_version - will use generic /usr/lib64 paths
                        let procfs = MountGuard::mount(
                            &Bind::new("/proc"),
                            generic_dir.join("proc"),
                            ReadOnly,
                        )
                        .await?;
                        Command::new("nvidia-container-cli")
                            .arg("configure")
                            .arg("--no-devbind")
                            .arg("--no-cgroups")
                            .arg("--utility")
                            .arg("--compute")
                            .arg("--graphics")
                            .arg("--video")
                            .arg(&generic_dir)
                            .invoke(ErrorKind::Unknown)
                            .await?;
                        procfs.unmount(true).await?;
                        // Run ldconfig to create proper symlinks for all NVIDIA libraries
                        Command::new("ldconfig")
                            .arg("-r")
                            .arg(&generic_dir)
                            .invoke(ErrorKind::Unknown)
                            .await?;

                        // Create squashfs with UID/GID mapping (avoids chown on readonly mounts)
                        if let Some(p) = sqfs.parent() {
                            tokio::fs::create_dir_all(p)
                                .await
                                .with_ctx(|_| (ErrorKind::Filesystem, format!("mkdir -p {p:?}")))?;
                        }
                        Command::new("mksquashfs")
                            .arg(&*tmp)
                            .arg(&sqfs)
                            .arg("-force-uid")
                            .arg("100000")
                            .arg("-force-gid")
                            .arg("100000")
                            .invoke(ErrorKind::Filesystem)
                            .await?;
                        // tmp.unmount_and_delete().await?;
                    }
                    BlockDev::new(&sqfs)
                        .mount(NVIDIA_OVERLAY_PATH, ReadOnly)
                        .await?;

                    Ok::<_, Error>(())
                }
                .await
                .log_err();
            }
        }

        let services = ServiceMap::default();
        let metrics_cache = Watch::<Option<crate::system::Metrics>>::new(None);
        let socks_proxy_url = format!("socks5h://{socks_proxy}");

        let crons = SyncMutex::new(BTreeMap::new());

        if !db
            .peek()
            .await
            .as_public()
            .as_server_info()
            .as_ntp_synced()
            .de()?
        {
            let db = db.clone();
            crons.mutate(|c| {
                c.insert(
                    Guid::new(),
                    tokio::spawn(async move {
                        // a failed query must not kill this task — the sync
                        // warning would stick for the rest of the boot
                        while !check_time_is_synchronized()
                            .await
                            .log_err()
                            .unwrap_or(false)
                        {
                            tokio::time::sleep(Duration::from_secs(30)).await;
                        }
                        db.mutate(|v| {
                            v.as_public_mut()
                                .as_server_info_mut()
                                .as_ntp_synced_mut()
                                .ser(&true)
                        })
                        .await
                        .result
                        .log_err();
                    })
                    .into(),
                )
            });
        }

        let seed = Arc::new(RpcContextSeed {
            is_closed: AtomicBool::new(false),
            closed: watch::Sender::new(false),
            os_partitions: OsPartitionInfo::from_fstab().await?,
            disk_guid,
            ephemeral_auth_keys: SyncMutex::new(AuthKeys::new()),
            auth_sig_nonce_cache: SyncMutex::new(Default::default()),
            sync_db: watch::Sender::new(db.sequence().await),
            db,
            account: SyncRwLock::new(account),
            callbacks: net_controller.callbacks.clone(),
            net_controller,
            os_net_service,
            s9pk_arch: if config.multi_arch_s9pks.unwrap_or(false) {
                None
            } else {
                Some(crate::ARCH)
            },
            services,
            cancellable_installs: SyncMutex::new(BTreeMap::new()),
            metrics_cache,
            rpc_continuations: RpcContinuations::new(),
            shutdown,
            lxc_manager: Arc::new(LxcManager::new()),
            open_authed_continuations: OpenAuthedContinuations::new(),
            wifi_manager: RwLock::new(None),
            current_secret: Arc::new(
                Jwk::generate_ec_key(josekit::jwk::alg::ec::EcCurve::P256).map_err(|e| {
                    tracing::debug!("{:?}", e);
                    tracing::error!("{}", t!("context.rpc.couldnt-generate-ec-key"));
                    Error::new(
                        color_eyre::eyre::eyre!("{}", t!("context.rpc.couldnt-generate-ec-key")),
                        crate::ErrorKind::Unknown,
                    )
                })?,
            ),
            client: Client::builder()
                .proxy(Proxy::all(socks_proxy_url)?)
                .build()
                .with_kind(crate::ErrorKind::ParseUrl)?,
            start_time: Instant::now(),
            crons,
        });

        let res = Self(seed.clone());
        res.cleanup_and_initialize(cleanup_init).await?;
        tracing::info!("{}", t!("context.rpc.cleaned-up-transient-states"));

        crate::version::post_init(&res, run_migrations).await?;
        tracing::info!("{}", t!("context.rpc.completed-migrations"));
        Ok(res)
    }

    #[instrument(skip_all)]
    pub async fn shutdown(self) -> Result<(), Error> {
        self.crons.mutate(|c| std::mem::take(c));
        self.services.shutdown_all().await?;
        self.is_closed.store(true, Ordering::SeqCst);
        self.closed.send_replace(true);
        tracing::info!("{}", t!("context.rpc.rpc-context-shutdown"));
        Ok(())
    }

    /// Resolves once graceful teardown (`shutdown`) has completed. Used by the
    /// `wait` path of the server shutdown/restart RPC so a caller can block
    /// until containers are stopped (it won't outlive the webserver teardown
    /// that immediately follows).
    pub async fn wait_closed(&self) {
        let mut rx = self.0.closed.subscribe();
        let _ = rx.wait_for(|closed| *closed).await;
    }

    pub fn add_cron<F: Future<Output = ()> + Send + 'static>(&self, fut: F) -> Guid {
        let guid = Guid::new();
        self.crons
            .mutate(|c| c.insert(guid.clone(), tokio::spawn(fut).into()));
        guid
    }

    #[instrument(skip_all)]
    pub async fn cleanup_and_initialize(
        &self,
        CleanupInitPhases {
            mut cleanup_sessions,
            mut init_services,
            mut prune_s9pks,
        }: CleanupInitPhases,
    ) -> Result<(), Error> {
        cleanup_sessions.start();
        let continuations = &self.open_authed_continuations;
        let ephemeral = &self.ephemeral_auth_keys;
        self.db
            .mutate(|db| reap_idle_sessions(continuations, ephemeral, db))
            .await
            .result?;
        // Weak, so the cron doesn't hold the context that owns it alive.
        let weak = Arc::downgrade(&self.0);
        self.add_cron(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(86400)).await;
                let Some(seed) = weak.upgrade() else { break };
                let continuations = &seed.open_authed_continuations;
                let ephemeral = &seed.ephemeral_auth_keys;
                if let Err(e) = seed
                    .db
                    .mutate(|db| reap_idle_sessions(continuations, ephemeral, db))
                    .await
                    .result
                {
                    tracing::error!(
                        "{}",
                        t!("context.rpc.error-in-session-cleanup-cron", error = e)
                    );
                    tracing::debug!("{e:?}");
                }
            }
        });
        cleanup_sessions.complete();

        init_services.start();
        self.services.init(&self).await?;
        init_services.complete();

        prune_s9pks.start();
        let peek = self.db.peek().await;
        let keep = peek
            .as_public()
            .as_package_data()
            .as_entries()?
            .into_iter()
            .map(|(_, pde)| pde.as_s9pk().de())
            .collect::<Result<BTreeSet<PathBuf>, Error>>()?;
        let installed_dir = &Path::new(DATA_DIR).join(PKG_ARCHIVE_DIR).join("installed");
        if tokio::fs::metadata(&installed_dir).await.is_ok() {
            let mut dir = tokio::fs::read_dir(&installed_dir)
                .await
                .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("dir {installed_dir:?}")))?;
            while let Some(file) = dir
                .next_entry()
                .await
                .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("dir {installed_dir:?}")))?
            {
                let path = file.path();
                if path.extension() == Some(OsStr::new("s9pk")) && !keep.contains(&path) {
                    delete_file(path).await?;
                }
            }
        }
        prune_s9pks.complete();

        Ok(())
    }
    pub async fn call_remote<RemoteContext>(
        &self,
        method: &str,
        metadata: OrdMap<&'static str, Value>,
        params: Value,
    ) -> Result<Value, RpcError>
    where
        Self: CallRemote<RemoteContext>,
    {
        <Self as CallRemote<RemoteContext, Empty>>::call_remote(
            &self,
            method,
            metadata,
            params,
            Empty {},
        )
        .await
    }
    pub async fn call_remote_with<RemoteContext, T>(
        &self,
        method: &str,
        metadata: OrdMap<&'static str, Value>,
        params: Value,
        extra: T,
    ) -> Result<Value, RpcError>
    where
        Self: CallRemote<RemoteContext, T>,
    {
        <Self as CallRemote<RemoteContext, T>>::call_remote(&self, method, metadata, params, extra)
            .await
    }
}
impl AsRef<Client> for RpcContext {
    fn as_ref(&self) -> &Client {
        &self.client
    }
}
impl AsRef<Jwk> for RpcContext {
    fn as_ref(&self) -> &Jwk {
        &CURRENT_SECRET
    }
}
impl AsRef<RpcContinuations> for RpcContext {
    fn as_ref(&self) -> &RpcContinuations {
        &self.rpc_continuations
    }
}
impl AsRef<OpenAuthedContinuations<Option<InternedString>>> for RpcContext {
    fn as_ref(&self) -> &OpenAuthedContinuations<Option<InternedString>> {
        &self.open_authed_continuations
    }
}
impl Context for RpcContext {}
impl Deref for RpcContext {
    type Target = RpcContextSeed;
    fn deref(&self) -> &Self::Target {
        #[cfg(feature = "unstable")]
        if self.0.is_closed.load(Ordering::SeqCst) {
            panic!(
                "RpcContext used after shutdown! {}",
                tracing_error::SpanTrace::capture()
            );
        }
        &self.0
    }
}
impl Drop for RpcContext {
    fn drop(&mut self) {
        #[cfg(feature = "unstable")]
        if self.0.is_closed.load(Ordering::SeqCst) {
            let count = Arc::strong_count(&self.0) - 1;
            tracing::info!("RpcContext dropped. {} left.", count);
            if count > 0 {
                tracing::debug!("{}", std::backtrace::Backtrace::force_capture());
                tracing::debug!("{:?}", eyre!(""))
            }
        }
    }
}
