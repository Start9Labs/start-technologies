use std::ops::Deref;
use std::sync::Arc;

use rpc_toolkit::Context;
use tokio::sync::broadcast::Sender;
use tokio::sync::watch;
use tracing::instrument;

use crate::context::config::ServerConfig;
use crate::context::startup::StartupRpc;
use crate::middleware::auth::session::{SecureSession, SessionContext};
use crate::middleware::auth::signature::{NonceCache, RequestReplayCheck, SignatureAuthContext};
use crate::net::dns::DnsController;
use crate::progress::FullProgressTracker;
use crate::rpc_continuations::RpcContinuations;
use crate::{Error, ErrorKind, ResultExt};

pub struct InitContextSeed {
    pub(crate) startup_rpc: StartupRpc,
    pub(crate) dns: Arc<DnsController>,
    pub config: ServerConfig,
    result: watch::Sender<Option<Result<(), Error>>>,
    pub progress: FullProgressTracker,
    pub shutdown: Sender<()>,
    pub rpc_continuations: RpcContinuations,
}

#[derive(Clone)]
pub struct InitContext(Arc<InitContextSeed>, Option<SecureSession>);

impl SessionContext for InitContext {
    fn secure_session(&self) -> Option<&SecureSession> {
        self.1.as_ref()
    }
}
impl SignatureAuthContext for InitContext {
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
impl InitContext {
    /// Records RpcContext initialization, independently of percentages and observation.
    pub(crate) fn complete(&self, result: Result<(), Error>) {
        if result.is_ok() {
            self.progress.complete();
        }
        self.result.send_replace(Some(result));
    }

    pub(crate) fn progress_stream(
        &self,
    ) -> impl futures::Stream<Item = Result<crate::progress::FullProgress, Error>> + Send + 'static
    {
        let mut result = self.result.subscribe();
        crate::progress::progress_until_complete(self.progress.clone(), async move {
            let result = result
                .wait_for(Option::is_some)
                .await
                .with_kind(ErrorKind::Cancelled)?;
            result
                .as_ref()
                .unwrap()
                .as_ref()
                .map(|_| ())
                .map_err(Error::clone_output)
        })
    }

    #[instrument(skip_all)]
    pub async fn init(cfg: &ServerConfig, dns: Arc<DnsController>) -> Result<Self, Error> {
        let (shutdown, _) = tokio::sync::broadcast::channel(1);
        let mut progress = FullProgressTracker::new();
        progress.enable_logging(true);
        Ok(Self(
            Arc::new(InitContextSeed {
                startup_rpc: StartupRpc::new(),
                dns,
                config: cfg.clone(),
                result: watch::channel(None).0,
                progress,
                shutdown,
                rpc_continuations: RpcContinuations::new(),
            }),
            None,
        ))
    }
}

impl AsRef<RpcContinuations> for InitContext {
    fn as_ref(&self) -> &RpcContinuations {
        &self.rpc_continuations
    }
}

impl Context for InitContext {}
impl Deref for InitContext {
    type Target = InitContextSeed;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}
