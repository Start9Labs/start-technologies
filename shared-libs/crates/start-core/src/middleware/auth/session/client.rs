use std::num::NonZeroUsize;
use std::time::Duration;

use axum::extract::ws::Message;
use futures::{SinkExt, StreamExt};
use http::HeaderMap;
use reqwest::Client;
use reqwest_websocket::RequestBuilderExt;
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{BoxWebSocket, RpcResponse};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use url::Url;

use super::RpcSessionUpgrader;
use crate::middleware::auth::signature::rpc_request_body;
use crate::prelude::*;
use crate::sign::session::{PreparedClient, Target};
use crate::sign::{AnyVerifyingKey, Signer};
use crate::util::net::secure::{DEFAULT_RPC_RESPONSE_LIMIT, EncryptedSocket, wire_message_size};
use crate::util::net::tungstenite::reqwest_transport;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const WRITE_TIMEOUT: Duration = Duration::from_secs(15);

mod reusable;
pub use reusable::Client as SessionClient;

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

async fn upgrade(
    client: &Client,
    url: Url,
    headers: HeaderMap,
    maximum: usize,
) -> Result<BoxWebSocket, Error> {
    let config = WebSocketConfig::default()
        .read_buffer_size(4096)
        .write_buffer_size(0)
        .max_write_buffer_size(maximum)
        .max_message_size(Some(maximum))
        .max_frame_size(Some(maximum));
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
    Ok(reqwest_transport(socket))
}

/// Opens a legacy socket with the caller's HTTP/1-only network client.
pub async fn legacy(client: &Client, url: Url) -> Result<BoxWebSocket, Error> {
    tokio::time::timeout(
        CONNECT_TIMEOUT,
        upgrade(client, url, HeaderMap::new(), DEFAULT_RPC_RESPONSE_LIMIT),
    )
    .await
    .with_kind(ErrorKind::Network)?
}

/// Returns the verified peer and socket after completed encrypted admission.
/// The selected hostname supplies HTTP authority; the URL selects the dial address.
pub async fn connect(
    client: &Client,
    signer: &Signer,
    url: Url,
    address: &str,
    target: Target,
    mut headers: HeaderMap,
) -> Result<(AnyVerifyingKey, EncryptedSocket), Error> {
    let mut logical = url.clone();
    logical
        .set_host(Some(address))
        .with_kind(ErrorKind::ParseUrl)?;
    headers.insert(
        http::header::HOST,
        logical[url::Position::BeforeHost..url::Position::AfterPort]
            .parse::<http::HeaderValue>()
            .with_kind(ErrorKind::ParseUrl)?,
    );
    admit(
        signer,
        address,
        target,
        upgrade(
            client,
            url,
            headers,
            wire_message_size(NonZeroUsize::new(DEFAULT_RPC_RESPONSE_LIMIT).unwrap()).get(),
        ),
    )
    .await
}

pub(crate) async fn admit(
    signer: &Signer,
    address: &str,
    target: Target,
    upgrade: impl Future<Output = Result<BoxWebSocket, Error>>,
) -> Result<(AnyVerifyingKey, EncryptedSocket), Error> {
    let prepared = PreparedClient::new(signer, address, target).await?;
    let limit = NonZeroUsize::new(DEFAULT_RPC_RESPONSE_LIMIT).unwrap();
    tokio::time::timeout(CONNECT_TIMEOUT, async {
        let socket = upgrade.await?;
        EncryptedSocket::initiate(socket, prepared, limit).await
    })
    .await
    .with_kind(ErrorKind::Network)?
}

pub(crate) fn request_body(method: &str, params: Value) -> Result<String, RpcError> {
    let mut body = rpc_request_body(method, params)?;
    body.insert_str(1, "\"jsonrpc\":\"2.0\",");
    Ok(body)
}

pub(crate) async fn request(socket: &mut EncryptedSocket, body: String) -> Result<Value, RpcError> {
    tokio::time::timeout(WRITE_TIMEOUT, socket.send(Message::Text(body.into())))
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
                tokio::time::timeout(WRITE_TIMEOUT, socket.send(Message::Pong(payload)))
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
