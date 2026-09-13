use std::ops::Deref;
use std::sync::Arc;

use rpc_toolkit::Context;
use rpc_toolkit::yajrc::RpcError;
use tokio::sync::OnceCell;
use tokio::sync::broadcast::Sender;
use tracing::instrument;

use crate::context::config::ServerConfig;
use crate::prelude::*;
use crate::rpc_continuations::RpcContinuations;
use crate::shutdown::Shutdown;
use crate::util::sync::SyncMutex;

pub struct DiagnosticContextSeed {
    pub shutdown: Sender<Shutdown>,
    pub error: Arc<RpcError>,
    pub disk_guid: Option<InternedString>,
    pub rpc_continuations: RpcContinuations,
    /// Single-flight token for `diagnostic.update`; strong count > 1 means an update is running.
    pub update_in_progress: SyncMutex<Arc<()>>,
    custom_ca_trust_store_refreshed: OnceCell<()>,
}

#[derive(Clone)]
pub struct DiagnosticContext(Arc<DiagnosticContextSeed>);
impl DiagnosticContext {
    #[instrument(skip_all)]
    pub fn init(
        _config: &ServerConfig,
        disk_guid: Option<InternedString>,
        error: Error,
    ) -> Result<Self, Error> {
        tracing::error!(
            "{}",
            t!("context.diagnostic.starting-diagnostic-ui", error = error)
        );
        tracing::debug!("{:?}", error);

        let (shutdown, _) = tokio::sync::broadcast::channel(1);

        Ok(Self(Arc::new(DiagnosticContextSeed {
            shutdown,
            disk_guid,
            error: Arc::new(error.into()),
            rpc_continuations: RpcContinuations::new(),
            update_in_progress: SyncMutex::new(Arc::new(())),
            custom_ca_trust_store_refreshed: OnceCell::new(),
        })))
    }

    pub(crate) async fn refresh_custom_ca_trust_store(&self) -> Result<(), Error> {
        self.custom_ca_trust_store_refreshed
            .get_or_try_init(|| async {
                crate::system::trust_ca::update_trust_store_if_custom_roots_present().await
            })
            .await
            .map(|_| ())
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
