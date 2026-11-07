use std::num::NonZeroUsize;
use std::time::Duration;

use axum::extract::ws::Message;
use futures::{SinkExt, StreamExt};
use http::HeaderMap;
use reqwest::Client;
use reqwest_websocket::RequestBuilderExt;
use rpc_toolkit::RpcResponse;
use rpc_toolkit::yajrc::RpcError;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use url::Url;

use super::{RpcSessionUpgrader, prologue, session_url};
use crate::middleware::auth::signature::rpc_request_body;
use crate::prelude::*;
use crate::sign::Signer;
use crate::sign::noise::NoiseIdentity;
use crate::util::net::secure::{DEFAULT_RPC_RESPONSE_LIMIT, EncryptedSocket, wire_message_size};
use crate::util::net::tungstenite::reqwest_transport;

#[cfg(test)]
mod tests;

/// Maps a versioned HTTP RPC URL to its sibling encrypted WebSocket endpoint.
pub fn websocket_url(mut rpc_url: Url) -> Result<Url, Error> {
    rpc_url
        .path_segments_mut()
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))
        .with_kind(ErrorKind::ParseUrl)?
        .pop_if_empty()
        .pop()
        .pop();
    RpcSessionUpgrader::url(rpc_url)
}

/// Calls an encrypted RPC using an HTTP/1-only client with the caller's network policy.
pub async fn call(
    client: &Client,
    signer: &Signer,
    websocket_url: Url,
    address: &str,
    headers: HeaderMap,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    let mut body = rpc_request_body(method, params)?;
    body.insert_str(1, "\"jsonrpc\":\"2.0\",");
    let identity = NoiseIdentity::new(signer).await?;
    let url = session_url(websocket_url, address)?;
    let limit = NonZeroUsize::new(DEFAULT_RPC_RESPONSE_LIMIT).unwrap();
    let wire_limit = wire_message_size(limit).get();
    let config = WebSocketConfig::default()
        .read_buffer_size(4096)
        .write_buffer_size(0)
        .max_write_buffer_size(wire_limit)
        .max_message_size(Some(wire_limit))
        .max_frame_size(Some(wire_limit));
    let mut socket = tokio::time::timeout(Duration::from_secs(30), async {
        let socket = client
            .get(url)
            .headers(headers)
            .upgrade()
            .web_socket_config(config)
            .send()
            .await
            .with_kind(ErrorKind::Network)?
            .into_websocket()
            .await
            .with_kind(ErrorKind::Network)?;
        let (_, socket) = EncryptedSocket::initiate(
            reqwest_transport(socket),
            identity,
            &prologue(address),
            limit,
        )
        .await?;
        Ok::<_, Error>(socket)
    })
    .await
    .with_kind(ErrorKind::Network)??;
    tokio::time::timeout(
        Duration::from_secs(15),
        socket.send(Message::Text(body.into())),
    )
    .await
    .with_kind(ErrorKind::Network)?
    .with_kind(ErrorKind::Network)?;
    loop {
        let message = socket
            .next()
            .await
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::UnexpectedEof))
            .with_kind(ErrorKind::Network)?
            .with_kind(ErrorKind::Network)?;
        match message {
            Message::Text(text) => {
                return serde_json::from_str::<RpcResponse>(&text)
                    .with_kind(ErrorKind::Deserialization)?
                    .result;
            }
            Message::Ping(payload) => {
                tokio::time::timeout(Duration::from_secs(15), socket.send(Message::Pong(payload)))
                    .await
                    .with_kind(ErrorKind::Network)?
                    .with_kind(ErrorKind::Network)?;
            }
            Message::Pong(_) => (),
            _ => {
                return Err(Error::new(
                    std::io::Error::from(std::io::ErrorKind::UnexpectedEof),
                    ErrorKind::Network,
                )
                .into());
            }
        }
    }
}
