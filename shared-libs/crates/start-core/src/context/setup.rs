use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures::future::{BoxFuture, Shared};
use futures::{Future, StreamExt};
use imbl_value::InternedString;
use josekit::jwk::Jwk;
use openssl::x509::X509;
use patch_db::PatchDb;
use rpc_toolkit::Context;
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;
use tokio::sync::broadcast::Sender;
use tracing::instrument;
use visit_rs::ts::TS;

use crate::MAIN_DATA;
use crate::context::RpcContext;
use crate::context::config::ServerConfig;
use crate::disk::mount::guard::{MountGuard, TmpMountGuard};
use crate::hostname::ServerHostname;
use crate::net::gateway::WildcardListener;
use crate::net::web_server::{WebServer, WebServerAcceptorSetter};
use crate::prelude::*;
use crate::progress::FullProgressTracker;
use crate::rpc_continuations::{Guid, RpcContinuation, RpcContinuations};
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

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SetupResult {
    #[ts(type = "string")]
    pub hostname: ServerHostname,
    pub root_ca: Pem<X509>,
    pub needs_restart: bool,
}

pub struct SetupContextSeed {
    pub webserver: WebServerAcceptorSetter<WildcardListener>,
    pub config: SyncMutex<ServerConfig>,
    pub disable_encryption: bool,
    pub progress: FullProgressTracker,
    pub task: OnceCell<NonDetachingJoinHandle<()>>,
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
pub struct SetupContext(Arc<SetupContextSeed>);
impl SetupContext {
    #[instrument(skip_all)]
    pub fn init(
        webserver: &WebServer<WildcardListener>,
        config: ServerConfig,
    ) -> Result<Self, Error> {
        let (shutdown, _) = tokio::sync::broadcast::channel(1);
        let mut progress = FullProgressTracker::new();
        progress.enable_logging(true);
        Ok(Self(Arc::new(SetupContextSeed {
            webserver: webserver.acceptor_setter(),
            disable_encryption: config.disable_encryption.unwrap_or(false),
            config: SyncMutex::new(config),
            progress,
            task: OnceCell::new(),
            result: OnceCell::new(),
            disk_guid: OnceCell::new(),
            shutdown,
            rpc_continuations: RpcContinuations::new(),
            install_rootfs: SyncMutex::new(None),
            install_os_future: SyncMutex::new(None),
            language: SyncMutex::new(None),
            keyboard: SyncMutex::new(None),
        })))
    }
    #[instrument(skip_all)]
    pub async fn db(&self) -> Result<PatchDb, Error> {
        let db_path = Path::new(MAIN_DATA).join("embassy.db");
        let db = PatchDb::open(&db_path)
            .await
            .with_ctx(|_| (crate::ErrorKind::Filesystem, db_path.display().to_string()))?;
        Ok(db)
    }

    pub fn run_setup<F, Fut>(&self, f: F) -> Result<(), Error>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(SetupResult, RpcContext), Error>> + Send,
    {
        let local_ctx = self.clone();
        self.task
            .set(
                tokio::spawn(async move {
                    local_ctx
                        .result
                        .get_or_init(|| async {
                            match f().await {
                                Ok(res) => {
                                    tracing::info!("{}", t!("context.setup.setup-complete"));
                                    Ok(res)
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "{}",
                                        t!("context.setup.setup-failed", error = e)
                                    );
                                    tracing::debug!("{e:?}");
                                    Err(e)
                                }
                            }
                        })
                        .await;
                    local_ctx.progress.complete();
                })
                .into(),
            )
            .map_err(|_| {
                if self.result.initialized() {
                    Error::new(
                        eyre!("{}", t!("context.setup.setup-already-complete")),
                        ErrorKind::InvalidRequest,
                    )
                } else {
                    Error::new(
                        eyre!("{}", t!("context.setup.setup-already-in-progress")),
                        ErrorKind::InvalidRequest,
                    )
                }
            })?;
        Ok(())
    }

    pub async fn progress(&self) -> SetupProgress {
        use axum::extract::ws;

        let guid = Guid::new();
        let progress_tracker = self.progress.clone();
        let progress = progress_tracker.snapshot();
        self.rpc_continuations
            .add(
                guid.clone(),
                RpcContinuation::ws(
                    |mut ws| async move {
                        if let Err(e) = async {
                            let mut stream =
                                progress_tracker.stream(Some(Duration::from_millis(100)));
                            loop {
                                tokio::select! {
                                    progress = stream.next() => {
                                        if let Some(progress) = progress {
                                            ws.send(ws::Message::Text(
                                                serde_json::to_string(&progress)
                                                    .with_kind(ErrorKind::Serialization)?
                                                    .into(),
                                            ))
                                            .await
                                            .with_kind(ErrorKind::Network)?;
                                            if progress.overall.is_complete() {
                                                return ws.normal_close("complete").await;
                                            }
                                        } else {
                                            return ws.normal_close("complete").await;
                                        }
                                    }
                                    msg = ws.recv() => {
                                        if msg.transpose().with_kind(ErrorKind::Network)?.is_none() {
                                            return Ok(())
                                        }
                                    }
                                }
                            }
                        }
                        .await
                        {
                            if !crate::util::net::is_ws_reset_without_close(&e) {
                                tracing::error!("{}", t!("context.setup.error-in-setup-progress-websocket", error = e));
                                tracing::debug!("{e:?}");
                            }
                        }
                    },
                    Duration::from_secs(30),
                ),
            )
            .await;

        SetupProgress { progress, guid }
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
