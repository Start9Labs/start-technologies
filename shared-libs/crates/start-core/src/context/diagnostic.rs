use std::ops::Deref;
use std::sync::Arc;

use rpc_toolkit::Context;
use rpc_toolkit::yajrc::RpcError;
use tokio::sync::broadcast::Sender;
use tracing::instrument;

use crate::context::config::ServerConfig;
use crate::prelude::*;
use crate::rpc_continuations::RpcContinuations;
use crate::shutdown::Shutdown;
use crate::util::sync::SyncMutex;

pub struct DiagnosticContextSeed {
    pub(crate) dns: Arc<crate::net::dns::DnsController>,
    pub shutdown: Sender<Shutdown>,
    pub error: Arc<RpcError>,
    pub disk_guid: Option<InternedString>,
    pub rpc_continuations: RpcContinuations,
    /// Single-flight token for `diagnostic.update`; strong count > 1 means an update is running.
    pub update_in_progress: SyncMutex<Arc<()>>,
}

#[derive(Clone)]
pub struct DiagnosticContext(Arc<DiagnosticContextSeed>);
impl DiagnosticContext {
    #[instrument(skip_all)]
    pub fn init(
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

        let (shutdown, _) = tokio::sync::broadcast::channel(1);

        Ok(Self(Arc::new(DiagnosticContextSeed {
            dns,
            shutdown,
            disk_guid,
            error: Arc::new(error.into()),
            rpc_continuations: RpcContinuations::new(),
            update_in_progress: SyncMutex::new(Arc::new(())),
        })))
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
