use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::future::Future;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use fast_socks5::client::{Config as SocksConfig, Socks5Stream};
use imbl::OrdMap;
use imbl_value::InternedString;
use josekit::jwk::Jwk;
use reqwest::{Client, Proxy};
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{CallRemote, Context, Empty};
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::sync::{RwLock, mpsc, oneshot, watch};
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::instrument;
use url::Url;

use super::setup::CURRENT_SECRET;
use crate::account::AccountInfo;
use crate::auth::AuthKeys;
use crate::context::config::ServerConfig;
use crate::db::model::package::PackageStateMatchModelRef;
use crate::db::model::{Database, DatabaseModel};
use crate::disk::OsPartitionInfo;
use crate::disk::mount::filesystem::bind::Bind;
use crate::disk::mount::filesystem::block_dev::BlockDev;
use crate::disk::mount::filesystem::{FileSystem, ReadOnly};
use crate::disk::mount::guard::MountGuard;
use crate::init::{InitResult, check_time_is_synchronized};
use crate::install::PKG_ARCHIVE_DIR;
use crate::lxc::LxcManager;
use crate::middleware::auth::signature::NonceCache;
use crate::net::gateway::WildcardListener;
use crate::net::net_controller::{NetController, NetService};
use crate::net::socks::DEFAULT_SOCKS_LISTEN;
use crate::net::web_server::WebServerAcceptorSetter;
use crate::net::wifi::WpaCli;
use crate::prelude::*;
use crate::progress::{FullProgressTracker, PhaseProgressTrackerHandle};
use crate::rpc_continuations::{OpenAuthedContinuations, RpcContinuations};
use crate::s9pk::S9pk;
use crate::s9pk::merkle_archive::source::DynFileSource;
use crate::service::ServiceMap;
use crate::service::effects::callbacks::ServiceCallbacks;
use crate::service::effects::subcontainer::NVIDIA_OVERLAY_PATH;
use crate::shutdown::{Shutdown, ShutdownSender};
use crate::util::Invoke;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::{TmpDir, delete_file};
use crate::util::lshw::LshwDevice;
use crate::util::sync::{SyncMutex, SyncRwLock, Watch};
use crate::{DATA_DIR, PLATFORM, PackageId};

/// Clears context-owned backup observations without discarding pending restart intent.
fn reset_backup_status(db: &mut DatabaseModel) -> Result<(), Error> {
    let status = db.as_public_mut().as_server_info_mut().as_status_info_mut();
    status.as_backup_progress_mut().ser(&None)?;
    status.as_backup_conversions_mut().ser(&Default::default())
}

type TaskHandles = Vec<NonDetachingJoinHandle<()>>;

/// Shutdown closes admission and joins every accepted task before service unmounts.
pub(crate) type BackgroundTasks = SyncMutex<Option<TaskHandles>>;
/// Teardown joins rollback spawned by cancelled workers before service unmounts.
pub(crate) type CleanupTasks = SyncMutex<TaskHandles>;

fn spawn_owned<F: Future<Output = ()> + Send + 'static>(tasks: &mut TaskHandles, future: F) {
    tasks.retain(|task| !task.is_finished());
    tasks.push(tokio::spawn(future).into());
}

pub(crate) fn spawn_cleanup<F: Future<Output = ()> + Send + 'static>(
    tasks: &CleanupTasks,
    future: F,
) {
    tasks.mutate(|tasks| spawn_owned(tasks, future));
}

pub(crate) fn spawn_background<F: Future<Output = ()> + Send + 'static>(
    tasks: &BackgroundTasks,
    future: F,
) -> Result<(), Error> {
    // Job destructors may re-enter task admission.
    let mut future = Some(future);
    tasks.mutate(|tasks| {
        let tasks = tasks.as_mut().ok_or_else(|| {
            Error::new(
                eyre!(t!("shutdown.already-shutting-down")),
                ErrorKind::InvalidRequest,
            )
        })?;
        spawn_owned(tasks, future.take().unwrap());
        Ok(())
    })
}

async fn stop_background_tasks(tasks: &BackgroundTasks, cleanup: &CleanupTasks) {
    let tasks = tasks.replace(None).expect("RPC tasks already stopped");
    for task in &tasks {
        task.abort();
    }
    for task in tasks {
        if let Err(error) = task.await {
            if !error.is_cancelled() {
                tracing::error!("{error}");
            }
        }
    }
    for task in cleanup.replace(Vec::new()) {
        task.await.log_err();
    }
}

pub struct RpcContextSeed {
    is_closed: AtomicBool,
    closed: watch::Sender<bool>,
    pub os_partitions: OsPartitionInfo,
    pub disk_guid: InternedString,
    pub ephemeral_auth_keys: SyncMutex<AuthKeys>,
    pub auth_sig_nonce_cache: SyncMutex<NonceCache>,
    pub db: TypedPatchDb<Database>,
    pub sync_db: watch::Sender<u64>,
    registry_refresh: mpsc::Sender<Url>,
    pub disk_updates: watch::Sender<()>,
    pub account: SyncRwLock<AccountInfo>,
    pub os_net_service: NetService,
    pub net_controller: Arc<NetController>,
    pub s9pk_arch: Option<&'static str>,
    pub services: ServiceMap,
    pub cancellable_installs: SyncMutex<BTreeMap<PackageId, oneshot::Sender<()>>>,
    pub installing_s9pks: SyncMutex<BTreeMap<PackageId, S9pk<DynFileSource>>>,
    pub metrics_cache: Watch<Option<crate::system::Metrics>>,
    pub shutdown: Arc<ShutdownSender>,
    /// Taken once by startd after setup hands over the RPC context.
    pub shutdown_recv: SyncMutex<Option<oneshot::Receiver<Option<Shutdown>>>>,
    pub lxc_manager: Arc<LxcManager>,
    pub open_authed_continuations: OpenAuthedContinuations<Option<InternedString>>,
    pub rpc_continuations: RpcContinuations,
    pub callbacks: Arc<ServiceCallbacks>,
    pub wifi_manager: RwLock<Option<WpaCli>>,
    pub current_secret: Arc<Jwk>,
    pub client: ReloadableHttpClient,
    pub start_time: Instant,
    pub background_tasks: Arc<BackgroundTasks>,
    pub cleanup_tasks: Arc<CleanupTasks>,
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
    optimize_storage: PhaseProgressTrackerHandle,
    init_services: PhaseProgressTrackerHandle,
    prune_s9pks: PhaseProgressTrackerHandle,
}
impl CleanupInitPhases {
    pub fn new(handle: &FullProgressTracker) -> Self {
        Self {
            cleanup_sessions: handle.add_phase("Cleaning up sessions".into(), Some(1)),
            optimize_storage: handle.add_phase("Optimizing storage".into(), Some(5)),
            init_services: handle.add_phase("Initializing services".into(), Some(10)),
            prune_s9pks: handle.add_phase("Pruning S9PKs".into(), Some(1)),
        }
    }
}

#[derive(Clone)]
pub struct RpcContext(
    Arc<RpcContextSeed>,
    Option<Arc<crate::middleware::auth::session::SecureSession>>,
);

impl SessionContext for RpcContext {
    fn secure_session(&self) -> Option<&crate::middleware::auth::session::SecureSession> {
        self.1.as_deref()
    }
}
impl RpcContext {
    pub(crate) fn replay_check(&self) -> crate::middleware::auth::signature::RequestReplayCheck {
        crate::middleware::auth::signature::RequestReplayCheck::new(&self.0, |seed| {
            Some(Self(seed, None))
        })
    }
    pub(crate) fn server_identity(&self) -> Result<crate::sign::Signer, Error> {
        crate::middleware::auth::signature::SigningContext::signing_key(self)
    }
    pub(crate) fn with_secure_session(
        mut self,
        session: crate::middleware::auth::session::SecureSession,
    ) -> Self {
        self.1 = Some(Arc::new(session));
        self
    }
}

/// Clients acquired before a reload retain their previous trust configuration.
pub struct ReloadableHttpClient {
    clients: SyncRwLock<HttpClients>,
    socks_proxy_url: String,
}

#[derive(Clone)]
struct HttpClients {
    redirecting: Client,
    without_redirects: Client,
}
impl ReloadableHttpClient {
    pub(crate) fn new(socks_proxy_url: String) -> Result<Self, Error> {
        let clients = Self::build_clients(&socks_proxy_url)?;
        Ok(Self {
            clients: SyncRwLock::new(clients),
            socks_proxy_url,
        })
    }

    fn build_clients(socks_proxy_url: &str) -> Result<HttpClients, Error> {
        Ok(HttpClients {
            redirecting: Self::build_for_proxy(
                socks_proxy_url,
                reqwest::redirect::Policy::default(),
            )?,
            without_redirects: Self::build_for_proxy(
                socks_proxy_url,
                reqwest::redirect::Policy::none(),
            )?,
        })
    }

    fn build_for_proxy(
        socks_proxy_url: &str,
        redirects: reqwest::redirect::Policy,
    ) -> Result<Client, Error> {
        Client::builder()
            .proxy(Proxy::all(socks_proxy_url)?)
            .redirect(redirects)
            .build()
            .with_kind(ErrorKind::ParseUrl)
    }

    pub fn get(&self) -> Client {
        self.clients.read().redirecting
    }

    pub(crate) fn get_without_redirects(&self) -> Client {
        self.clients.read().without_redirects
    }

    pub async fn websocket(
        &self,
        request: impl IntoClientRequest + Send,
        config: WebSocketConfig,
    ) -> Result<WebSocketStream<MaybeTlsStream<Socks5Stream<TcpStream>>>, Error> {
        let request = request
            .into_client_request()
            .with_kind(ErrorKind::Network)?;
        let url: Url = request.uri().to_string().parse()?;
        let proxy: Url = self.socks_proxy_url.parse()?;
        let stream = Socks5Stream::connect(
            (
                proxy.host_str().or_not_found("SOCKS proxy host")?,
                proxy.port().or_not_found("SOCKS proxy port")?,
            ),
            url.host_str().or_not_found("WebSocket host")?.to_owned(),
            url.port_or_known_default().or_not_found("WebSocket port")?,
            SocksConfig::default(),
        )
        .await
        .with_kind(ErrorKind::Network)?;
        let (socket, _) =
            tokio_tungstenite::client_async_tls_with_config(request, stream, Some(config), None)
                .await
                .with_kind(ErrorKind::Network)?;
        Ok(socket)
    }

    pub fn reload(&self) -> Result<(), Error> {
        let clients = Self::build_clients(&self.socks_proxy_url)?;
        self.clients.replace(clients);
        Ok(())
    }
}

/// No-op until the clock is NTP-synced, so a wrong boot-time clock can't reap
/// live sessions.
fn reap_idle_sessions(
    continuations: &OpenAuthedContinuations<Option<InternedString>>,
    ephemeral: &SyncMutex<AuthKeys>,
    db: &mut Model<Database>,
) -> Result<(), Error> {
    if !db.as_public().as_server_info().as_ntp_synced().de()? {
        return Ok(());
    }
    crate::auth::reap_idle_keys_in::<RpcContext>(continuations, Some(ephemeral), db)
}

impl RpcContext {
    #[instrument(skip_all)]
    pub async fn init(
        webserver: &WebServerAcceptorSetter<WildcardListener>,
        dns: Arc<crate::net::dns::DnsController>,
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
        let (shutdown_send, shutdown_recv) = oneshot::channel();
        let shutdown = Arc::new(SyncMutex::new((Some(shutdown_send), false)));

        load_db.start();
        let db = if let Some(InitResult { net_ctrl, .. }) = &init_result {
            net_ctrl.db.clone()
        } else {
            TypedPatchDb::<Database>::load(config.db().await?).await?
        };
        db.mutate(|db| {
            db.as_public_mut().as_disks_mut().ser(&Vec::new())?;
            for (_, volume) in db
                .as_public_mut()
                .as_shared_volumes_mut()
                .as_entries_mut()?
            {
                volume.mutate(|volume| {
                    volume.inspection = crate::shared_volumes::SharedVolumeInspection::default();
                    Ok(())
                })?;
            }
            reset_backup_status(db)
        })
        .await
        .result?;
        crate::shutdown::publish_shutdown_status(&db, &shutdown).await;
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
                    dns,
                    webserver,
                    socks_proxy,
                    config
                        .max_proxy_conns_per_target
                        .unwrap_or(crate::net::vhost::MAX_PROXY_CONNS_PER_TARGET),
                )
                .await?,
            );
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

                        if let Some(p) = sqfs.parent() {
                            tokio::fs::create_dir_all(p)
                                .await
                                .with_ctx(|_| (ErrorKind::Filesystem, format!("mkdir -p {p:?}")))?;
                        }
                        Command::new("mksquashfs")
                            .arg(&*tmp)
                            .arg(&sqfs)
                            .arg("-force-uid")
                            .arg(crate::lxc::BASE_IDMAP.to_id.to_string())
                            .arg("-force-gid")
                            .arg(crate::lxc::BASE_IDMAP.to_id.to_string())
                            .invoke(ErrorKind::Filesystem)
                            .await?;
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

        let background_tasks = Arc::new(SyncMutex::new(Some(Vec::new())));

        let (registry_refresh, registry_refreshes) = mpsc::channel(8);
        let seed = Arc::new(RpcContextSeed {
            is_closed: AtomicBool::new(false),
            closed: watch::Sender::new(false),
            os_partitions: OsPartitionInfo::from_fstab().await?,
            disk_guid,
            ephemeral_auth_keys: SyncMutex::new(AuthKeys::new()),
            auth_sig_nonce_cache: SyncMutex::new(Default::default()),
            sync_db: watch::Sender::new(db.sequence().await),
            registry_refresh,
            disk_updates: watch::Sender::new(()),
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
            installing_s9pks: SyncMutex::new(BTreeMap::new()),
            metrics_cache,
            rpc_continuations: RpcContinuations::new(),
            shutdown,
            shutdown_recv: SyncMutex::new(Some(shutdown_recv)),
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
            client: ReloadableHttpClient::new(socks_proxy_url)?,
            start_time: Instant::now(),
            background_tasks,
            cleanup_tasks: Arc::new(SyncMutex::new(Vec::new())),
        });

        let res = Self(seed.clone(), None);
        let initialized = async {
            if !res
                .db
                .peek()
                .await
                .as_public()
                .as_server_info()
                .as_ntp_synced()
                .de()?
            {
                let db = res.db.clone();
                res.spawn_background(async move {
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
                })?;
            }
            crate::update::signer_files::initialize(&res.db).await?;
            res.cleanup_and_initialize(cleanup_init).await?;
            #[cfg(target_os = "linux")]
            {
                let weak = Arc::downgrade(&seed);
                res.spawn_background(crate::shared_volumes::watch::run(move || {
                    weak.upgrade().map(|seed| Self(seed, None))
                }))?;
            }
            tracing::info!("{}", t!("context.rpc.cleaned-up-transient-states"));

            crate::version::post_init(&res, run_migrations).await?;
            let weak = Arc::downgrade(&seed);
            let db = res.db.clone();
            res.spawn_background(crate::registry::sync::watch_registries(
                db,
                registry_refreshes,
                move || weak.upgrade().map(|seed| Self(seed, None)),
            ))?;
            tracing::info!("{}", t!("context.rpc.completed-migrations"));
            let weak = Arc::downgrade(&res.0);
            let db = res.db.clone();
            res.spawn_background(async move {
                let mut watch = db.watch("/private/push".parse().unwrap()).await;
                loop {
                    let Some(seed) = weak.upgrade() else { break };
                    let client = seed.client.get();
                    drop(seed);
                    if crate::notifications::push::drain(&db, &client)
                        .await
                        .is_err()
                    {
                        tracing::warn!("{}", t!("notifications.push.queue-failed"));
                    }
                    tokio::select! {
                        result = watch.changed() => if result.is_err() { break },
                        _ = tokio::time::sleep(Duration::from_secs(60)) => {}
                    }
                }
            })?;
            Ok::<_, Error>(())
        }
        .await;
        if let Err(error) = initialized {
            res.shutdown().await.log_err();
            return Err(error);
        }
        Ok(res)
    }

    pub(crate) async fn request_registry_refresh(&self, registry: Url) -> Result<(), Error> {
        self.registry_refresh.send(registry).await.map_err(|_| {
            Error::new(
                eyre!("{}", t!("context.rpc.registry-sync-unavailable")),
                ErrorKind::Network,
            )
        })
    }

    #[instrument(skip_all)]
    pub async fn shutdown(self) -> Result<(), Error> {
        stop_background_tasks(&self.background_tasks, &self.cleanup_tasks).await;
        self.services.shutdown_all().await?;
        self.is_closed.store(true, Ordering::SeqCst);
        self.closed.send_replace(true);
        tracing::info!("{}", t!("context.rpc.rpc-context-shutdown"));
        Ok(())
    }

    /// Resolves after background jobs and containers stop, before webserver teardown.
    pub async fn wait_closed(&self) {
        let mut rx = self.0.closed.subscribe();
        let _ = rx.wait_for(|closed| *closed).await;
    }

    /// Runs daemon-owned work; rejects registration after task teardown starts.
    pub fn spawn_background<F: Future<Output = ()> + Send + 'static>(
        &self,
        fut: F,
    ) -> Result<(), Error> {
        spawn_background(&self.background_tasks, fut)
    }

    #[instrument(skip_all)]
    pub async fn cleanup_and_initialize(
        &self,
        CleanupInitPhases {
            mut cleanup_sessions,
            mut optimize_storage,
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
        // Pending session cleanup must not keep its RPC context alive.
        let weak = Arc::downgrade(&self.0);
        self.spawn_background(async move {
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
        })?;
        cleanup_sessions.complete();

        // Conversion requires unmounted, quiescent roots before observers and services start.
        optimize_storage.start();
        self.lxc_manager.gc().await?;
        let peek = self.db.peek().await;
        let mut installed = BTreeSet::new();
        let mut all = BTreeSet::new();
        for (id, pde) in peek.as_public().as_package_data().as_entries()? {
            if matches!(
                pde.as_state_info().as_match(),
                PackageStateMatchModelRef::Installed(_)
            ) {
                installed.insert(id.clone());
            }
            all.insert(id);
        }
        if let Err(e) =
            crate::volume::boot_volume_maintenance(&installed, &all, &mut optimize_storage).await
        {
            tracing::warn!("{}", t!("context.rpc.volume-maintenance-failed", error = e));
            tracing::debug!("{e:?}");
        }
        crate::volume::convert_directories_to_subvolumes(
            crate::shared_volumes::managed_local_paths(&peek)?,
            &mut optimize_storage,
        )
        .await?;
        optimize_storage.complete();

        init_services.start();
        self.net_controller.net_iface.settle().await;
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

#[cfg(test)]
mod tests {
    use futures::FutureExt;
    use http::{HeaderMap, HeaderValue};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn background_observations_reset_without_discarding_restart_intent() {
        use crate::backup::target::conversion::{ConversionJob, ConversionState};
        use crate::db::model::public::{RestartReason, ServerStatus};
        use crate::progress::FullProgress;

        let status = ServerStatus {
            backup_progress: Some(FullProgress::new()),
            backup_conversions: [(
                "cifs-1".into(),
                [(
                    "server-a".into(),
                    ConversionJob {
                        state: ConversionState::Running,
                        progress: FullProgress::new(),
                        error: None,
                    },
                )]
                .into(),
            )]
            .into(),
            restart: Some(RestartReason::Update),
            ..Default::default()
        };
        let db = TypedPatchDb::<Database>::load_unchecked(PatchDb::in_memory());
        db.put(
            &patch_db::json_ptr::ROOT,
            &imbl_value::json!({
                "public": {"serverInfo": {"statusInfo": status}}
            }),
        )
        .await
        .unwrap();

        db.mutate(reset_backup_status).await.result.unwrap();

        let status = db
            .peek()
            .await
            .as_public()
            .as_server_info()
            .as_status_info()
            .de()
            .unwrap();
        assert!(status.backup_progress.is_none());
        assert!(status.backup_conversions.is_empty());
        assert_eq!(status.restart, Some(RestartReason::Update));
    }

    #[tokio::test]
    async fn background_teardown_joins_jobs_and_closes_admission() {
        use crate::util::future::DropSignaling;

        let tasks = SyncMutex::new(Some(Vec::new()));
        let mut stopped = Vec::new();
        for _ in 0..2 {
            let job = DropSignaling::new(std::future::pending::<()>());
            stopped.push(job.subscribe());
            spawn_background(&tasks, job).unwrap();
        }

        stop_background_tasks(&tasks, &CleanupTasks::new(Vec::new())).await;

        for mut stopped in stopped {
            assert!(stopped.wait().now_or_never().is_some());
        }
        assert!(tasks.peek(Option::is_none));
        assert!(spawn_background(&tasks, async { panic!("late job started") }).is_err());
    }

    #[tokio::test]
    async fn background_teardown_finishes_rollback_created_by_cancellation() {
        let tasks = BackgroundTasks::new(Some(Vec::new()));
        let cleanup = Arc::new(CleanupTasks::new(Vec::new()));
        let rollback = cleanup.clone();
        let (completed, completion) = oneshot::channel();
        let guard = crate::util::GeneralBoxedGuard::new(move || {
            spawn_cleanup(&rollback, async move {
                tokio::task::yield_now().await;
                let _ = completed.send(());
            });
        });
        spawn_background(&tasks, async move {
            let _guard = guard;
            std::future::pending::<()>().await;
        })
        .unwrap();

        stop_background_tasks(&tasks, &cleanup).await;

        assert!(completion.now_or_never().unwrap().is_ok());
        assert!(cleanup.peek(Vec::is_empty));
    }

    #[test]
    fn background_rejection_drops_jobs_outside_the_admission_lock() {
        let (completed, completion) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let tasks = Arc::new(BackgroundTasks::new(None));
            let cleanup_owner = tasks.clone();
            let guard = crate::util::GeneralBoxedGuard::new(move || {
                assert!(spawn_background(&cleanup_owner, async {}).is_err());
            });
            assert!(
                spawn_background(&tasks, async move {
                    std::future::pending::<()>().await;
                    drop(guard);
                })
                .is_err()
            );
            completed.send(()).unwrap();
        });
        completion
            .recv_timeout(Duration::from_secs(1))
            .expect("job destructor re-entered a held admission lock");
        worker.join().unwrap();
    }

    #[tokio::test]
    async fn background_registration_releases_finished_handles() {
        let tasks = SyncMutex::new(Some(Vec::new()));
        let (finished, completed) = oneshot::channel();
        spawn_background(&tasks, async move {
            let _ = finished.send(());
        })
        .unwrap();
        completed.await.unwrap();
        spawn_background(&tasks, std::future::pending()).unwrap();
        assert_eq!(tasks.peek(|tasks| tasks.as_ref().unwrap().len()), 1);
        stop_background_tasks(&tasks, &CleanupTasks::new(Vec::new())).await;
    }

    fn clients_with_generation(generation: &'static str) -> HttpClients {
        let mut headers = HeaderMap::new();
        headers.insert("x-client-generation", HeaderValue::from_static(generation));
        HttpClients {
            redirecting: Client::builder()
                .default_headers(headers.clone())
                .build()
                .unwrap(),
            without_redirects: Client::builder()
                .default_headers(headers)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
        }
    }

    #[tokio::test]
    async fn reloadable_http_client_replaces_future_clones() {
        let proxy = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_url = format!("socks5h://{}", proxy.local_addr().unwrap());
        let proxy_server = tokio::spawn(async move {
            let (mut stream, _) = proxy.accept().await.unwrap();
            let mut request = [0; 64];
            assert!(stream.read(&mut request).await.unwrap() > 0);
        });
        let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_url = format!("http://{}", target.local_addr().unwrap());
        let target_server = tokio::spawn(async move {
            let (mut stream, _) = target.accept().await.unwrap();
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).await.unwrap() > 0);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });
        let http = ReloadableHttpClient {
            clients: SyncRwLock::new(clients_with_generation("old")),
            socks_proxy_url: proxy_url,
        };
        let old = http.get();
        http.reload().unwrap();

        assert_client_generation(old, "old").await;
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_secs(1),
                http.get().get(target_url).send()
            )
            .await
            .unwrap()
            .is_err()
        );
        proxy_server.await.unwrap();
        target_server.abort();
    }

    #[tokio::test]
    async fn websocket_uses_the_os_socks_proxy_and_remote_dns() {
        use fast_socks5::server::Socks5ServerProtocol;
        use fast_socks5::util::target_addr::TargetAddr;
        use futures::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        let proxy = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client =
            ReloadableHttpClient::new(format!("socks5h://{}", proxy.local_addr().unwrap()))
                .unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = proxy.accept().await.unwrap();
            let (protocol, _, target) = Socks5ServerProtocol::accept_no_auth(stream)
                .await
                .unwrap()
                .read_command()
                .await
                .unwrap();
            assert_eq!(target, TargetAddr::Domain("registry.invalid".into(), 5959));
            let stream = protocol
                .reply_success("127.0.0.1:0".parse().unwrap())
                .await
                .unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            socket.send(Message::Text("{}".into())).await.unwrap();
        });
        let mut socket = tokio::time::timeout(
            Duration::from_secs(5),
            client.websocket(
                "ws://registry.invalid:5959/ws/rpc/test-continuation"
                    .parse::<Url>()
                    .unwrap(),
                WebSocketConfig::default().read_buffer_size(4096),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            socket.next().await.unwrap().unwrap(),
            Message::Text("{}".into())
        );
        assert_eq!(socket.get_config().read_buffer_size, 4096);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn scoped_http_clients_do_not_forward_redirect_credentials_after_reload() {
        let proxy = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let http =
            ReloadableHttpClient::new(format!("http://{}", proxy.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move {
            let mut forwarded = 0;
            for _ in 0..4 {
                let (mut stream, _) = proxy.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut bytes = [0; 1024];
                    let count = stream.read(&mut bytes).await.unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&bytes[..count]);
                    if request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8(request).unwrap();
                let response = if request.contains("/source ") {
                    "HTTP/1.1 307 Temporary Redirect\r\nLocation: http://another.invalid/forwarded\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                } else {
                    assert!(request.contains("/forwarded "));
                    assert!(!request.to_ascii_lowercase().contains("x-start-auth-sig"));
                    forwarded += 1;
                    "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                };
                stream.write_all(response.as_bytes()).await.unwrap();
            }
            forwarded
        });
        let url = "http://registry.invalid/source";
        for _ in 0..2 {
            http.reload().unwrap();
            let response = http
                .get_without_redirects()
                .get(url)
                .header(
                    crate::middleware::auth::signature::AUTH_SIG_HEADER,
                    "private-proof",
                )
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), http::StatusCode::TEMPORARY_REDIRECT);
        }
        assert!(
            http.get()
                .get(url)
                .send()
                .await
                .unwrap()
                .status()
                .is_success()
        );
        assert_eq!(server.await.unwrap(), 1);
    }

    #[tokio::test]
    async fn reloadable_http_client_keeps_client_when_rebuild_fails() {
        let http = ReloadableHttpClient {
            clients: SyncRwLock::new(clients_with_generation("old")),
            socks_proxy_url: "http://[::1".to_owned(),
        };

        assert!(http.reload().is_err());
        assert_client_generation(http.get(), "old").await;
        assert_client_generation(http.get_without_redirects(), "old").await;
    }

    async fn assert_client_generation(client: Client, generation: &str) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 4096];
            let len = stream.read(&mut request).await.unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
            String::from_utf8(request[..len].to_vec()).unwrap()
        });

        client.get(&url).send().await.unwrap();
        assert!(
            server
                .await
                .unwrap()
                .contains(&format!("x-client-generation: {generation}"))
        );
    }
}
