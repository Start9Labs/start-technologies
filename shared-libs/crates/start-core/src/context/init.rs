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
use crate::prelude::{eyre, t};
use crate::progress::FullProgressTracker;
use crate::rpc_continuations::RpcContinuations;
use crate::util::sync::SyncMutex;
use crate::{Error, ErrorKind, ResultExt};

pub struct InitContextSeed {
    pub(crate) startup_rpc: StartupRpc,
    pub(crate) dns: Arc<DnsController>,
    pub config: ServerConfig,
    result: watch::Sender<Option<Result<(), Error>>>,
    pub(crate) observers: InitObservers,
    pub progress: FullProgressTracker,
    pub shutdown: Sender<()>,
    pub rpc_continuations: RpcContinuations,
}

#[derive(Clone)]
pub struct InitContext(Arc<InitContextSeed>, Option<SecureSession>);

#[derive(Clone)]
pub(crate) struct InitObservers(Arc<SyncMutex<Option<watch::Sender<()>>>>);

impl InitObservers {
    fn new() -> Self {
        Self(Arc::new(SyncMutex::new(Some(watch::channel(()).0))))
    }

    /// Hold the lease through the terminal transport write or cancellation.
    pub(crate) fn subscribe(&self) -> Result<watch::Receiver<()>, Error> {
        self.0
            .peek(|observers| observers.as_ref().map(watch::Sender::subscribe))
            .ok_or_else(|| Error::new(eyre!("{}", t!("error.cancelled")), ErrorKind::Cancelled))
    }

    #[cfg(test)]
    pub(crate) fn has_observers(&self) -> bool {
        self.0.peek(|observers| {
            observers
                .as_ref()
                .is_some_and(|sender| sender.receiver_count() > 0)
        })
    }

    async fn retire(&self) {
        let observers = self.0.replace(None);
        if let Some(observers) = observers {
            let _ = tokio::time::timeout(
                rpc_toolkit::WebSocketOptions::default().send_timeout,
                observers.closed(),
            )
            .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn retirement_waits_for_accepted_observers_and_rejects_late_registration() {
        let observers = InitObservers::new();
        let lease = observers.subscribe().unwrap();
        let retirement = observers.retire();
        tokio::pin!(retirement);
        assert!(futures::poll!(retirement.as_mut()).is_pending());
        assert_eq!(
            observers.subscribe().unwrap_err().kind,
            ErrorKind::Cancelled
        );
        drop(lease);
        retirement.await;
        assert_eq!(
            observers.subscribe().unwrap_err().kind,
            ErrorKind::Cancelled
        );
    }

    #[tokio::test(start_paused = true)]
    async fn stalled_observers_cannot_extend_the_rpc_write_deadline() {
        let observers = InitObservers::new();
        let _lease = observers.subscribe().unwrap();
        let retirement = observers.retire();
        tokio::pin!(retirement);
        assert!(futures::poll!(retirement.as_mut()).is_pending());
        let budget = rpc_toolkit::WebSocketOptions::default().send_timeout;
        tokio::time::advance(budget - std::time::Duration::from_millis(1)).await;
        assert!(futures::poll!(retirement.as_mut()).is_pending());
        tokio::time::advance(std::time::Duration::from_millis(1)).await;
        retirement.await;
        assert_eq!(
            observers.subscribe().unwrap_err().kind,
            ErrorKind::Cancelled
        );
    }

    #[tokio::test]
    async fn retirement_without_observers_does_not_wait_for_its_deadline() {
        let observers = InitObservers::new();
        let retirement = observers.retire();
        tokio::pin!(retirement);
        assert!(futures::poll!(retirement.as_mut()).is_ready());
    }
}

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

    /// Drains accepted observers within the RPC write deadline before router replacement.
    /// New subscriptions are canceled once retirement starts; peer receipt is not guaranteed.
    pub(crate) async fn complete_and_retire(&self, result: Result<(), Error>) {
        self.complete(result);
        self.observers.retire().await;
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
                observers: InitObservers::new(),
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
