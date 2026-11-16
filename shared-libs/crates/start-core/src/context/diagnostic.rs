use std::ops::Deref;
use std::sync::Arc;

use rpc_toolkit::Context;
use rpc_toolkit::yajrc::RpcError;
use tokio::sync::broadcast::Sender;
use tracing::instrument;

use crate::context::config::ServerConfig;
use crate::context::startup::StartupRpc;
use crate::middleware::auth::local::LocalAuthContext;
use crate::middleware::auth::session::{SecureSession, SessionContext};
use crate::middleware::auth::signature::{NonceCache, RequestReplayCheck, SignatureAuthContext};
use crate::prelude::*;
use crate::rpc_continuations::RpcContinuations;
use crate::shutdown::Shutdown;
use crate::util::sync::SyncMutex;

pub struct DiagnosticContextSeed {
    pub(crate) startup_rpc: StartupRpc,
    pub(crate) dns: Arc<crate::net::dns::DnsController>,
    pub shutdown: Sender<Shutdown>,
    pub error: Arc<RpcError>,
    pub disk_guid: Option<InternedString>,
    pub rpc_continuations: RpcContinuations,
    /// A strong count above one means an update is running.
    pub update_in_progress: SyncMutex<Arc<()>>,
}

#[derive(Clone)]
pub struct DiagnosticContext(Arc<DiagnosticContextSeed>, Option<SecureSession>);

impl SessionContext for DiagnosticContext {
    fn secure_session(&self) -> Option<&SecureSession> {
        self.1.as_ref()
    }
}
impl SignatureAuthContext for DiagnosticContext {
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
impl DiagnosticContext {
    #[instrument(skip_all)]
    pub async fn init(
        _config: &ServerConfig,
        disk_guid: Option<InternedString>,
        error: Error,
        dns: Arc<crate::net::dns::DnsController>,
    ) -> Result<Self, Error> {
        tracing::error!(
            "{}",
            t!("context.diagnostic.starting-diagnostic-ui", error = error)
        );
        tracing::debug!("{:?}", error);

        Self::init_auth_cookie().await.log_err();
        let (shutdown, _) = tokio::sync::broadcast::channel(1);

        Ok(Self(
            Arc::new(DiagnosticContextSeed {
                startup_rpc: StartupRpc::new(),
                dns,
                shutdown,
                disk_guid,
                error: Arc::new(error.into()),
                rpc_continuations: RpcContinuations::new(),
                update_in_progress: SyncMutex::new(Arc::new(())),
            }),
            None,
        ))
    }
}
impl AsRef<RpcContinuations> for DiagnosticContext {
    fn as_ref(&self) -> &RpcContinuations {
        &self.rpc_continuations
    }
}
impl Context for DiagnosticContext {}
impl Deref for DiagnosticContext {
    type Target = DiagnosticContextSeed;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}
