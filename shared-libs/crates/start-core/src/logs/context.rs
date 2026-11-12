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
/// Enrolled contexts must cancel observations when their signer is revoked.
pub trait LogContext: Context + AsRef<RpcContinuations> + SessionContext {
    fn forward_logs(
        &self,
        signer: Option<InternedString>,
        stream: RpcStream<LogEntry>,
        source: LogEntries,
    );
    fn log_continuation(
        &self,
        signer: Option<InternedString>,
        policy: WebSocketPolicy,
        source: LogEntries,
    ) -> RpcContinuation;
}

impl LogContext for RpcContext {
    fn forward_logs(
        &self,
        signer: Option<InternedString>,
        stream: RpcStream<LogEntry>,
        source: LogEntries,
    ) {
        self.open_authed_continuations
            .forward_stream(signer, stream, source);
    }

    fn log_continuation(
        &self,
        signer: Option<InternedString>,
        policy: WebSocketPolicy,
        source: LogEntries,
    ) -> RpcContinuation {
        RpcContinuation::ws_authed(
            self,
            signer,
            policy,
            self.secure_session().cloned(),
            |socket| forward_log_socket(socket, source),
            Duration::from_secs(30),
        )
    }
}

macro_rules! startup_logs {
    ($($context:ty),+) => {$(
        impl LogContext for $context {
            fn forward_logs(&self, _: Option<InternedString>, stream: RpcStream<LogEntry>, source: LogEntries) {
                tokio::spawn(async move {
                    let result = stream.forward(source).await;
                    let _ = stream.close(result);
                });
            }

            fn log_continuation(&self, _: Option<InternedString>, policy: WebSocketPolicy, source: LogEntries) -> RpcContinuation {
                RpcContinuation::ws(policy, self.secure_session().cloned(), |socket| forward_log_socket(socket, source), Duration::from_secs(30))
            }
        }
    )+};
}

startup_logs!(InitContext, SetupContext, DiagnosticContext);
