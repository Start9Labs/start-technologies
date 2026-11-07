use std::num::NonZeroUsize;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use futures::FutureExt;
use futures::future::BoxFuture;
use rpc_toolkit::{UpgradedWebSocket, WebSocketOptions, WebSocketUpgrader};
use serde::Deserialize;

use super::signature::{SignatureAuthContext, request_identities};
use crate::prelude::*;
use crate::sign::noise::NoiseIdentity;
use crate::sign::{AnyVerifyingKey, Signer};
use crate::util::net::secure::{EncryptedSocket, wire_message_size};

const PROLOGUE: &[u8] = b"Start-RPC-Noise v1\0";

/// Immutable provenance of a completed encrypted exchange, without cached enrollment or roles.
#[derive(Clone)]
pub struct SecureSession {
    peer: AnyVerifyingKey,
    address: InternedString,
    server_identity: Signer,
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
    ) -> Result<EncryptedSocket, Error> {
        let identity = NoiseIdentity::new(&self.server_identity).await?;
        let (_, socket) = EncryptedSocket::accept(
            socket,
            identity,
            &prologue(&self.address),
            incoming,
            Some(self.peer.clone()),
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
    fn with_secure_session(self, session: SecureSession) -> Self;
}

/// Binds the Noise transcript to the selected, ingress-validated server name.
pub fn prologue(address: &str) -> Vec<u8> {
    [PROLOGUE, address.as_bytes()].concat()
}

#[derive(Deserialize)]
struct Negotiation {
    noise: Option<u8>,
    context: Option<InternedString>,
}

/// Opts into encrypted admission with `noise=1&context=<server-name>`; missing opt-in stays legacy.
pub struct RpcSessionUpgrader;

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
            match negotiation.noise {
                None => {
                    return Ok(UpgradedWebSocket {
                        context,
                        socket: Box::new(socket),
                    });
                }
                Some(1) => (),
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
            let identity = NoiseIdentity::new(&server_identity)
                .await
                .map_err(|error| axum::Error::new(error.source))?;
            let (peer, socket) = EncryptedSocket::accept(
                Box::new(socket),
                identity,
                &prologue(&address),
                options.max_message_size,
                None,
            )
            .await
            .map_err(|error| axum::Error::new(error.source))?;
            let scoped = (*context).clone().with_secure_session(SecureSession {
                peer,
                address,
                server_identity,
            });
            Ok(UpgradedWebSocket {
                context: Arc::new(scoped),
                socket: Box::new(socket),
            })
        }
        .boxed()
    }
}
