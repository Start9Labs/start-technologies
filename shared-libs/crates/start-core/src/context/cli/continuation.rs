use futures::{Sink, Stream};
use http::HeaderMap;
use http::header::AUTHORIZATION;
use rpc_toolkit::BoxWebSocket;
use tokio_tungstenite::tungstenite::Message;
use url::Url;

use super::CliContext;
use crate::context::{DiagnosticContext, InitContext, RpcContext, SetupContext};
use crate::middleware::auth::local::local_auth_header_for_url;
use crate::middleware::auth::session::{RpcSessionUpgrader, client};
use crate::middleware::auth::signature::SigningContext;
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::sign::session::Target;
use crate::util::net::tungstenite::native_messages;

#[cfg(test)]
mod tests;

/// Selects RPC and continuation transport from the originating context.
pub trait CliWebSocketContext {
    fn stream_cli<
        Item: serde::de::DeserializeOwned + Send + 'static,
        Final: serde::de::DeserializeOwned + Send + 'static,
    >(
        ctx: &CliContext,
        method: &str,
        params: Value,
    ) -> impl Future<
        Output = Result<crate::util::rpc_client::RpcResponseStream<Value, Item, Final>, Error>,
    > + Send;

    fn connect_cli(
        ctx: &CliContext,
        guid: Guid,
    ) -> impl Future<Output = Result<BoxWebSocket, Error>> + Send;
}

impl CliWebSocketContext for RpcContext {
    async fn stream_cli<
        Item: serde::de::DeserializeOwned + Send + 'static,
        Final: serde::de::DeserializeOwned + Send + 'static,
    >(
        ctx: &CliContext,
        method: &str,
        params: Value,
    ) -> Result<crate::util::rpc_client::RpcResponseStream<Value, Item, Final>, Error> {
        ctx.call_remote_stream(method, params).await
    }

    async fn connect_cli(ctx: &CliContext, guid: Guid) -> Result<BoxWebSocket, Error> {
        let url = ctx.continuation_url(&guid)?;
        let mut headers = HeaderMap::new();
        if let Some(auth) = local_auth_header_for_url::<RpcContext>(&url).await {
            headers.insert(AUTHORIZATION, auth);
        }
        if let Some(signer) = ctx.rpc_signer(&headers)? {
            let (_, socket) = client::connect(
                &ctx.websocket_client,
                &signer,
                url,
                ctx.host_identity.as_deref().unwrap_or_default(),
                Target::Continuation(guid),
                HeaderMap::new(),
            )
            .await?;
            Ok(Box::new(socket))
        } else {
            client::legacy(&ctx.websocket_client, url).await
        }
    }
}

impl CliWebSocketContext for InitContext {
    async fn stream_cli<
        Item: serde::de::DeserializeOwned + Send + 'static,
        Final: serde::de::DeserializeOwned + Send + 'static,
    >(
        ctx: &CliContext,
        method: &str,
        params: Value,
    ) -> Result<crate::util::rpc_client::RpcResponseStream<Value, Item, Final>, Error> {
        ctx.call_startup_stream(method, params, HeaderMap::new())
            .await
            .map_err(Error::from)
    }

    async fn connect_cli(ctx: &CliContext, guid: Guid) -> Result<BoxWebSocket, Error> {
        ctx.startup_continuation(guid).await
    }
}

impl CliWebSocketContext for DiagnosticContext {
    async fn stream_cli<
        Item: serde::de::DeserializeOwned + Send + 'static,
        Final: serde::de::DeserializeOwned + Send + 'static,
    >(
        ctx: &CliContext,
        method: &str,
        params: Value,
    ) -> Result<crate::util::rpc_client::RpcResponseStream<Value, Item, Final>, Error> {
        ctx.call_diagnostic_stream(method, params).await
    }

    async fn connect_cli(ctx: &CliContext, guid: Guid) -> Result<BoxWebSocket, Error> {
        ctx.startup_continuation(guid).await
    }
}

impl CliWebSocketContext for SetupContext {
    async fn stream_cli<
        Item: serde::de::DeserializeOwned + Send + 'static,
        Final: serde::de::DeserializeOwned + Send + 'static,
    >(
        ctx: &CliContext,
        method: &str,
        params: Value,
    ) -> Result<crate::util::rpc_client::RpcResponseStream<Value, Item, Final>, Error> {
        ctx.call_startup_stream(method, params, HeaderMap::new())
            .await
            .map_err(Error::from)
    }

    async fn connect_cli(ctx: &CliContext, guid: Guid) -> Result<BoxWebSocket, Error> {
        ctx.startup_continuation(guid).await
    }
}

impl CliContext {
    fn continuation_url(&self, guid: &Guid) -> Result<Url, Error> {
        let mut url = RpcSessionUpgrader::url(self.base_url.get()?.clone())?;
        url.path_segments_mut()
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))
            .with_kind(ErrorKind::ParseUrl)?
            .push(guid.as_ref());
        Ok(url)
    }

    async fn startup_continuation(&self, guid: Guid) -> Result<BoxWebSocket, Error> {
        let (_, socket) = client::connect(
            &self.websocket_client,
            self.rpc_client.ephemeral_signer(),
            self.continuation_url(&guid)?,
            self.host_identity.as_deref().unwrap_or_default(),
            Target::Continuation(guid),
            HeaderMap::new(),
        )
        .await?;
        Ok(Box::new(socket))
    }

    /// Opens a fresh continuation using its originating context's transport contract.
    pub async fn ws_continuation<C: CliWebSocketContext>(
        &self,
        guid: Guid,
    ) -> Result<
        impl Stream<Item = Result<Message, Error>> + Sink<Message, Error = Error> + Unpin + Send,
        Error,
    > {
        let mut socket = crate::util::net::WebSocket::new(C::connect_cli(self, guid).await?);
        socket.set_send_timeout(client::WRITE_TIMEOUT);
        Ok(native_messages(Box::new(socket)))
    }
}
