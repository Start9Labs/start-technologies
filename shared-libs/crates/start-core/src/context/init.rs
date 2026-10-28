use std::ops::Deref;
use std::sync::Arc;

use rpc_toolkit::Context;
use tokio::sync::broadcast::Sender;
use tokio::sync::watch;
use tracing::instrument;

use crate::Error;
use crate::context::config::ServerConfig;
use crate::net::dns::DnsController;
use crate::progress::FullProgressTracker;
use crate::rpc_continuations::RpcContinuations;

pub struct InitContextSeed {
    pub(crate) dns: Arc<DnsController>,
    pub config: ServerConfig,
    pub error: watch::Sender<Option<Error>>,
    pub progress: FullProgressTracker,
    pub shutdown: Sender<()>,
    pub rpc_continuations: RpcContinuations,
}

#[derive(Clone)]
pub struct InitContext(Arc<InitContextSeed>);
impl InitContext {
    #[instrument(skip_all)]
    pub async fn init(cfg: &ServerConfig, dns: Arc<DnsController>) -> Result<Self, Error> {
        let (shutdown, _) = tokio::sync::broadcast::channel(1);
        let mut progress = FullProgressTracker::new();
        progress.enable_logging(true);
        Ok(Self(Arc::new(InitContextSeed {
            dns,
            config: cfg.clone(),
            error: watch::channel(None).0,
            progress,
            shutdown,
            rpc_continuations: RpcContinuations::new(),
        })))
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
