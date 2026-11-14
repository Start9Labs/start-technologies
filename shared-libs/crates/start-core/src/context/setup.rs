use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures::future::{BoxFuture, Shared};
use futures::{Future, FutureExt};
use imbl_value::InternedString;
use josekit::jwk::Jwk;
use openssl::x509::X509;
use patch_db::PatchDb;
use rpc_toolkit::Context;
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;
use tokio::sync::broadcast::Sender;
use tracing::instrument;

use crate::MAIN_DATA;
use crate::context::RpcContext;
use crate::context::config::ServerConfig;
use crate::context::startup::StartupRpc;
use crate::disk::mount::guard::{MountGuard, TmpMountGuard};
use crate::hostname::ServerHostname;
use crate::middleware::auth::session::{SecureSession, SessionContext};
use crate::middleware::auth::signature::{NonceCache, RequestReplayCheck, SignatureAuthContext};
use crate::net::gateway::WildcardListener;
use crate::net::web_server::{WebServer, WebServerAcceptorSetter};
use crate::prelude::*;
use crate::progress::FullProgressTracker;
use crate::rpc_continuations::{Guid, RpcContinuation, RpcContinuations, WebSocketPolicy};
use crate::setup::{SetupInfo, SetupProgress};
use crate::shutdown::Shutdown;
use crate::system::KeyboardOptions;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::serde::Pem;
use crate::util::sync::SyncMutex;

lazy_static::lazy_static! {
    pub static ref CURRENT_SECRET: Jwk = Jwk::generate_ec_key(josekit::jwk::alg::ec::EcCurve::P256).unwrap_or_else(|e| {
        tracing::debug!("{:?}", e);
        tracing::error!("{}", t!("context.setup.couldnt-generate-ec-key"));
        panic!("Couldn't generate ec key")
    });
}

#[derive(Debug, Clone, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SetupResult {
    pub hostname: ServerHostname,
    pub root_ca: Pem<X509>,
    pub needs_restart: bool,
}

rpc_toolkit::reflect_ts!(SetupResult);
rpc_toolkit::ts_export!(SetupResult, namespaces = [""]);

type SetupTask = Shared<BoxFuture<'static, Result<(), Arc<tokio::task::JoinError>>>>;

pub struct SetupContextSeed {
    pub(crate) startup_rpc: StartupRpc,
    pub(crate) dns: Arc<crate::net::dns::DnsController>,
    pub webserver: WebServerAcceptorSetter<WildcardListener>,
    pub config: SyncMutex<ServerConfig>,
    pub disable_encryption: bool,
    pub progress: FullProgressTracker,
    task: SyncMutex<Option<SetupTask>>,
    pub result: OnceCell<Result<(SetupResult, RpcContext), Error>>,
    pub disk_guid: OnceCell<InternedString>,
    pub shutdown: Sender<Option<Shutdown>>,
    pub rpc_continuations: RpcContinuations,
    pub install_rootfs: SyncMutex<Option<(TmpMountGuard, MountGuard)>>,
    pub install_os_future:
        SyncMutex<Option<Shared<BoxFuture<'static, Result<SetupInfo, Arc<Error>>>>>>,
    pub keyboard: SyncMutex<Option<KeyboardOptions>>,
    pub language: SyncMutex<Option<InternedString>>,
}

#[derive(Clone)]
pub struct SetupContext(Arc<SetupContextSeed>, Option<SecureSession>);

impl SessionContext for SetupContext {
    fn secure_session(&self) -> Option<&SecureSession> {
        self.1.as_ref()
    }
}
impl SignatureAuthContext for SetupContext {
    fn server_identity(&self) -> Result<crate::sign::Signer, Error> {
        Ok(self.startup_rpc.identity.clone())
    }
    fn replay_check(&self) -> RequestReplayCheck {
        RequestReplayCheck::new(&self.0, |seed| Some(Self(seed, None)))
    }
    fn with_secure_session(self, session: SecureSession) -> Self {
        Self(self.0, Some(session))
    }
    fn mutate_nonce_cache<F: FnOnce(&mut NonceCache) -> T, T>(&self, f: F) -> T {
        self.startup_rpc.nonces.mutate(f)
    }
    async fn clock_synced(&self) -> bool {
        StartupRpc::clock_synced().await
    }
    async fn sig_context(
        &self,
        extensions: &http::Extensions,
    ) -> impl IntoIterator<Item = Result<impl AsRef<str> + Send, Error>> + Send {
        [crate::net::static_server::local_ui_identity(extensions)]
    }
}
impl SetupContext {
    #[instrument(skip_all)]
    pub fn init(
        webserver: &WebServer<WildcardListener>,
        config: ServerConfig,
        dns: Arc<crate::net::dns::DnsController>,
    ) -> Result<Self, Error> {
        let (shutdown, _) = tokio::sync::broadcast::channel(1);
        let mut progress = FullProgressTracker::new();
        progress.enable_logging(true);
        Ok(Self(
            Arc::new(SetupContextSeed {
                startup_rpc: StartupRpc::new(),
                dns,
                webserver: webserver.acceptor_setter(),
                disable_encryption: config.disable_encryption.unwrap_or(false),
                config: SyncMutex::new(config),
                progress,
                task: SyncMutex::new(None),
                result: OnceCell::new(),
                disk_guid: OnceCell::new(),
                shutdown,
                rpc_continuations: RpcContinuations::new(),
                install_rootfs: SyncMutex::new(None),
                install_os_future: SyncMutex::new(None),
                language: SyncMutex::new(None),
                keyboard: SyncMutex::new(None),
            }),
            None,
        ))
    }
    #[instrument(skip_all)]
    pub async fn db(&self) -> Result<PatchDb, Error> {
        let db_path = Path::new(MAIN_DATA).join("embassy.db");
        let db = PatchDb::open(&db_path)
            .await
            .with_ctx(|_| (crate::ErrorKind::Filesystem, db_path.display().to_string()))?;
        Ok(db)
    }

    pub(crate) fn setup_started(&self) -> bool {
        self.task.peek(Option::is_some)
    }

    /// Reserves execution before starting work; observation never owns the task.
    pub fn run_setup<F, Fut>(&self, f: F) -> Result<(), Error>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(SetupResult, RpcContext), Error>> + Send,
    {
        self.task.mutate(|slot| {
            if slot.is_some() {
                return Err(if self.result.initialized() {
                    Error::new(
                        eyre!("{}", t!("context.setup.setup-already-complete")),
                        ErrorKind::InvalidRequest,
                    )
                } else {
                    Error::new(
                        eyre!("{}", t!("context.setup.setup-already-in-progress")),
                        ErrorKind::InvalidRequest,
                    )
                });
            }
            let local_ctx = self.clone();
            let task: NonDetachingJoinHandle<()> = tokio::spawn(async move {
                let result = local_ctx
                    .result
                    .get_or_init(|| async {
                        match f().await {
                            Ok(res) => {
                                tracing::info!("{}", t!("context.setup.setup-complete"));
                                Ok(res)
                            }
                            Err(e) => {
                                tracing::error!("{}", t!("context.setup.setup-failed", error = e));
                                tracing::debug!("{e:?}");
                                Err(e)
                            }
                        }
                    })
                    .await;
                if result.is_ok() {
                    local_ctx.progress.complete();
                }
            })
            .into();
            *slot = Some(task.map(|result| result.map_err(Arc::new)).boxed().shared());
            Ok(())
        })
    }

    pub(crate) fn progress_stream(
        &self,
    ) -> impl futures::Stream<Item = Result<crate::progress::FullProgress, Error>> + Send + 'static
    {
        let task = self.task.peek(|task| task.as_ref().unwrap().clone());
        let owner = Arc::downgrade(&self.0);
        crate::progress::progress_until_complete(self.progress.clone(), async move {
            task.await.with_kind(ErrorKind::Unknown)?;
            let owner = owner.upgrade().ok_or_else(|| {
                Error::new(eyre!("{}", t!("error.cancelled")), ErrorKind::Cancelled)
            })?;
            owner
                .result
                .get()
                .unwrap()
                .as_ref()
                .map(|_| ())
                .map_err(Error::clone_output)
        })
    }

    /// Requires accepted setup work; observation reports its actual outcome.
    pub async fn progress(
        &self,
        rpc_stream: Option<rpc_toolkit::RpcStream<crate::progress::FullProgress>>,
    ) -> SetupProgress {
        let progress = self.progress.snapshot();
        let source = self.progress_stream();
        if let Some(stream) = rpc_stream {
            tokio::spawn(async move {
                let result = stream.forward(source).await;
                let _ = stream.close(result);
            });
            return SetupProgress {
                progress,
                guid: None,
            };
        }
        let guid = Guid::new();
        self.rpc_continuations
            .add(
                guid.clone(),
                RpcContinuation::ws(
                    WebSocketPolicy::Public,
                    self.secure_session().cloned(),
                    |ws| async move {
                        if let Err(e) = crate::progress::forward_progress(ws, source).await {
                            if !crate::util::net::is_ws_reset_without_close(&e) {
                                tracing::error!(
                                    "{}",
                                    t!(
                                        "context.setup.error-in-setup-progress-websocket",
                                        error = e
                                    )
                                );
                                tracing::debug!("{e:?}");
                            }
                        }
                    },
                    Duration::from_secs(30),
                ),
            )
            .await;

        SetupProgress {
            progress,
            guid: Some(guid),
        }
    }
}

impl AsRef<Jwk> for SetupContext {
    fn as_ref(&self) -> &Jwk {
        &*CURRENT_SECRET
    }
}

impl AsRef<RpcContinuations> for SetupContext {
    fn as_ref(&self) -> &RpcContinuations {
        &self.rpc_continuations
    }
}

impl Context for SetupContext {}
impl Deref for SetupContext {
    type Target = SetupContextSeed;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

#[cfg(test)]
mod tests;
