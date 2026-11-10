use std::num::NonZeroUsize;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use futures::FutureExt;
use futures::future::BoxFuture;
use rpc_toolkit::{UpgradedWebSocket, WebSocketOptions, WebSocketUpgrader};
use serde::{Deserialize, Serialize};
use url::Url;

use super::signature::{RequestReplayCheck, SignatureAuthContext, request_identities};
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::sign::session::{Target, VERSION};
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
        let (_, socket) = EncryptedSocket::accept(
            socket,
            &self.server_identity,
            &self.address,
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

/// Supplies the server identity and installs verified, connection-local provenance.
pub trait SecureRpcContext: SignatureAuthContext + Clone {
    fn server_identity(&self) -> Result<Signer, Error>;
    /// Acquires admission replay policy without retaining its context.
    fn replay_check(&self) -> super::signature::RequestReplayCheck;
    fn with_secure_session(self, session: SecureSession) -> Self;
}

/// Selects encrypted admission while preserving other endpoint parameters.
pub fn session_url(mut url: Url, address: &str) -> Result<Url, Error> {
    let query = serde_urlencoded::to_string(Negotiation {
        secure: Some(VERSION),
        context: Some(address.into()),
    })
    .with_kind(ErrorKind::Serialization)?;
    let query = match url.query() {
        Some(existing) => format!("{existing}&{query}"),
        None => query,
    };
    url.set_query(Some(&query));
    Ok(url)
}

/// Browser transport settings generated from the native protocol owners.
pub fn browser_profile() -> serde_json::Value {
    let mut identity = crate::sign::session::wire_profile();
    identity["fingerprint"] = crate::sign::fingerprint::wire_profile();
    serde_json::json!({
        "identity": identity,
        "transport": crate::util::net::secure::browser_profile(),
        "negotiation": Negotiation {
            secure: Some(VERSION),
            context: Some("".into()),
        },
        "rpcPath": RpcSessionUpgrader::PATH,
    })
}

#[derive(Deserialize, Serialize)]
struct Negotiation {
    secure: Option<u8>,
    context: Option<InternedString>,
}

/// Opts into encrypted admission with `secure=1&context=<server-name>`; missing opt-in stays legacy.
pub struct RpcSessionUpgrader;

impl RpcSessionUpgrader {
    /// The shared encrypted RPC endpoint.
    pub const PATH: &'static str = "/ws/rpc";

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

    /// WebSocket RPC admission with optional encrypted peer authentication.
    pub fn route<C: SecureRpcContext>(
        server: rpc_toolkit::HttpServer<C>,
    ) -> axum::routing::MethodRouter {
        axum::routing::any(server.for_websocket().with_upgrader(Self))
            .layer(crate::middleware::cors::layer())
    }
}

pub mod client;

#[cfg(test)]
mod tests;

impl<C: SecureRpcContext> WebSocketUpgrader<C> for RpcSessionUpgrader {
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
            let axum::extract::Query(negotiation) =
                axum::extract::Query::<Negotiation>::try_from_uri(request.uri())
                    .map_err(axum::Error::new)?;
            match negotiation.secure {
                None => {
                    return Ok(UpgradedWebSocket {
                        context,
                        socket: Box::new(socket),
                    });
                }
                Some(VERSION) => (),
                _ => {
                    return Err(axum::Error::new(std::io::Error::from(
                        std::io::ErrorKind::InvalidInput,
                    )));
                }
            }
            let address = negotiation.context.ok_or_else(|| {
                axum::Error::new(std::io::Error::from(std::io::ErrorKind::InvalidInput))
            })?;
            let allowed = request_identities(&*context, &request)
                .await
                .map_err(|error| axum::Error::new(error.source))?;
            if !allowed.contains(&address) {
                return Err(axum::Error::new(std::io::Error::from(
                    std::io::ErrorKind::PermissionDenied,
                )));
            }
            let server_identity = context
                .server_identity()
                .map_err(|error| axum::Error::new(error.source))?;
            let replay_check = context.replay_check();
            let (peer, socket) = EncryptedSocket::accept(
                Box::new(socket),
                &server_identity,
                &address,
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
