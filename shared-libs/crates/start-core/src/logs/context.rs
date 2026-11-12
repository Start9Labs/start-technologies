use std::time::Duration;

use futures::stream::BoxStream;
use imbl_value::InternedString;
use rpc_toolkit::{Context, RpcStream};

use super::{LogEntry, forward_log_socket};
use crate::Error;
use crate::context::{DiagnosticContext, InitContext, RpcContext, SetupContext};
use crate::middleware::auth::session::SessionContext;
use crate::rpc_continuations::{RpcContinuation, RpcContinuations, WebSocketPolicy};

pub type LogEntries = BoxStream<'static, Result<LogEntry, Error>>;

/// Binds log observation to its context's existing revocation and continuation owners.
/// Enrolled contexts recheck live membership after registering revocation.
pub trait LogContext: Context + AsRef<RpcContinuations> + SessionContext {
    fn forward_logs(
        &self,
        signer: Option<InternedString>,
        stream: RpcStream<LogEntry>,
        source: LogEntries,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn log_continuation(
        &self,
        signer: Option<InternedString>,
        policy: WebSocketPolicy,
        source: LogEntries,
    ) -> impl Future<Output = Result<RpcContinuation, Error>> + Send;
}

impl LogContext for RpcContext {
    async fn forward_logs(
        &self,
        signer: Option<InternedString>,
        stream: RpcStream<LogEntry>,
        source: LogEntries,
    ) -> Result<(), Error> {
        crate::auth::continuation_subscription(self, signer)
            .await?
            .forward_stream(stream, source);
        Ok(())
    }

    async fn log_continuation(
        &self,
        signer: Option<InternedString>,
        policy: WebSocketPolicy,
        source: LogEntries,
    ) -> Result<RpcContinuation, Error> {
        Ok(RpcContinuation::ws_authed(
            crate::auth::continuation_subscription(self, signer).await?,
            policy,
            self.secure_session().cloned(),
            |socket| forward_log_socket(socket, source),
            Duration::from_secs(30),
        ))
    }
}

macro_rules! startup_logs {
    ($($context:ty),+) => {$(
        impl LogContext for $context {
            async fn forward_logs(&self, _: Option<InternedString>, stream: RpcStream<LogEntry>, source: LogEntries) -> Result<(), Error> {
                tokio::spawn(async move {
                    let result = stream.forward(source).await;
                    let _ = stream.close(result);
                });
                Ok(())
            }

            async fn log_continuation(&self, _: Option<InternedString>, policy: WebSocketPolicy, source: LogEntries) -> Result<RpcContinuation, Error> {
                Ok(RpcContinuation::ws(policy, self.secure_session().cloned(), |socket| forward_log_socket(socket, source), Duration::from_secs(30)))
            }
        }
    )+};
}

startup_logs!(InitContext, SetupContext, DiagnosticContext);
