use std::num::NonZeroUsize;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use rpc_toolkit::{UpgradedWebSocket, WebSocketOptions, WebSocketUpgrader};
use url::Url;

use super::signature::{RequestReplayCheck, SignatureAuthContext, request_identities};
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::sign::session::Target;
use crate::sign::{AnyVerifyingKey, Signer};
use crate::util::net::secure::{EncryptedSocket, wire_message_size};

/// Immutable provenance of a completed encrypted exchange, without cached enrollment or roles.
#[derive(Clone)]
pub struct SecureSession {
    peer: AnyVerifyingKey,
    address: InternedString,
    server_identity: Signer,
    replay_check: RequestReplayCheck,
}

impl SecureSession {
    /// The cryptographically verified remote identity, independently of enrollment.
    pub fn peer(&self) -> &AnyVerifyingKey {
        &self.peer
    }
    /// The ingress-validated server name bound into the exchange.
    pub fn address(&self) -> &InternedString {
        &self.address
    }

    /// Admits a fresh encrypted continuation only for the initiating identity.
    pub(crate) async fn accept(
        &self,
        socket: rpc_toolkit::BoxWebSocket,
        incoming: NonZeroUsize,
        guid: &Guid,
    ) -> Result<EncryptedSocket, Error> {
        let (_, _, socket) = EncryptedSocket::accept(
            socket,
            &self.server_identity,
            std::slice::from_ref(&self.address),
            incoming,
            &Target::Continuation(guid.clone()),
            Some(&self.peer),
            |commitment| self.replay_check.check(commitment),
        )
        .await?;
        Ok(socket)
    }
}

/// Provides connection provenance independently of database or authentication policy.
pub trait SessionContext {
    fn secure_session(&self) -> Option<&SecureSession> {
        None
    }
}

/// Browser transport settings generated from the native protocol owners.
pub fn browser_profile() -> serde_json::Value {
    let mut identity = crate::sign::session::wire_profile();
    identity["fingerprint"] = crate::sign::fingerprint::wire_profile();
    serde_json::json!({
        "identity": identity,
        "transport": crate::util::net::secure::browser_profile(),
        "rpcPath": RpcSessionUpgrader::PATH,
    })
}

/// Encrypted admission is mandatory for the bare RPC endpoint.
pub struct RpcSessionUpgrader;

impl RpcSessionUpgrader {
    /// The shared encrypted RPC endpoint.
    pub const PATH: &'static str = "/rpc/v2";

    /// RPC WebSocket endpoint under an HTTP deployment base.
    pub fn url(mut base: Url) -> Result<Url, Error> {
        let scheme = match base.scheme() {
            "https" => "wss",
            "http" => "ws",
            _ => {
                return Err(Error::new(
                    eyre!("{}", t!("context.cli.cannot-parse-scheme-from-base-url")),
                    ErrorKind::ParseUrl,
                ));
            }
        };
        base.path_segments_mut()
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))
            .with_kind(ErrorKind::ParseUrl)?
            .pop_if_empty()
            .extend(Self::PATH.trim_start_matches('/').split('/'));
        base.set_scheme(scheme)
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))
            .with_kind(ErrorKind::ParseUrl)?;
        Ok(base)
    }

    /// Signed-DH admission before RPC parsing.
    pub fn route<C: SignatureAuthContext + Clone>(
        server: rpc_toolkit::HttpServer<C>,
    ) -> axum::routing::MethodRouter {
        axum::routing::any(server.for_websocket().with_upgrader(Self))
            .layer(crate::middleware::cors::layer())
    }
}

pub mod client;

#[cfg(test)]
mod tests;

impl<C: SignatureAuthContext + Clone> WebSocketUpgrader<C> for RpcSessionUpgrader {
    fn max_wire_message_size(&self, logical: NonZeroUsize) -> NonZeroUsize {
        wire_message_size(logical)
    }
    fn request_context(&self, context: &C) -> C {
        context.clone()
    }
    fn upgrade(
        &self,
        context: Arc<C>,
        parts: http::request::Parts,
        socket: axum::extract::ws::WebSocket,
        options: WebSocketOptions,
    ) -> BoxFuture<'static, Result<UpgradedWebSocket<C>, axum::Error>> {
        async move {
            let request = Request::from_parts(parts, Body::empty());
            let socket: rpc_toolkit::BoxWebSocket = match request
                .extensions()
                .get::<crate::net::web_server::RouterEpoch>()
                .cloned()
            {
                Some(epoch) => Box::new(socket.take_until(Box::pin(epoch.changed()))),
                None => Box::new(socket),
            };
            let allowed = request_identities(&*context, &request)
                .await
                .map_err(|error| axum::Error::new(error.source))?;
            let server_identity = context
                .server_identity()
                .map_err(|error| axum::Error::new(error.source))?;
            let replay_check = context.replay_check();
            let (peer, address, socket) = EncryptedSocket::accept(
                socket,
                &server_identity,
                &allowed,
                options.max_message_size,
                &Target::Rpc,
                None,
                |commitment| replay_check.check(commitment),
            )
            .await
            .map_err(|error| axum::Error::new(error.source))?;
            let scoped = (*context).clone().with_secure_session(SecureSession {
                peer,
                address,
                server_identity,
                replay_check,
            });
            Ok(UpgradedWebSocket {
                context: Arc::new(scoped),
                socket: Box::new(socket),
            })
        }
        .boxed()
    }
}
