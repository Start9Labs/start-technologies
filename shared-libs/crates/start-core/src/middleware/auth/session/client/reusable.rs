use std::collections::BTreeMap;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, OnceLock};

use axum::extract::ws::Message;
use futures::{SinkExt, StreamExt};
use http::HeaderMap;
use rpc_toolkit::yajrc::{GenericRpcMethod, RpcError};
use tokio::sync::Mutex;
use url::Url;

use super::{WRITE_TIMEOUT, connect};
use crate::prelude::Value;
use crate::sign::session::Target;
use crate::sign::{AnyVerifyingKey, Signer};
use crate::util::net::WebSocket;
use crate::util::rpc_client::{CapturedRequest, RpcClient, RpcResponseStream};
use crate::util::sync::SyncMutex;
use crate::{Error, ErrorKind};

#[cfg(test)]
mod tests;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Endpoint {
    url: Url,
    address: String,
    signer: AnyVerifyingKey,
    headers: Vec<(String, Vec<Vec<u8>>)>,
}

type Entry = Arc<Mutex<Option<Arc<Connection>>>>;

/// Lazily reuses encrypted RPC connections within one immutable network policy.
/// Transport failure invalidates a connection without replaying a possibly sent request.
pub struct Client {
    network: reqwest::Client,
    ids: Arc<AtomicUsize>,
    connections: SyncMutex<BTreeMap<Endpoint, Entry>>,
    ephemeral_signer: OnceLock<Signer>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Client").finish_non_exhaustive()
    }
}

pub(crate) struct Connection {
    pub peer: AnyVerifyingKey,
    dispatcher: RpcClient,
}

impl Connection {
    pub(crate) async fn request(&self, request: CapturedRequest) -> Result<Value, RpcError> {
        self.dispatcher
            .send_cancellable_request(request)
            .await?
            .map(Into::into)
    }
}

impl Client {
    pub fn new(network: reqwest::Client) -> Self {
        Self {
            network,
            ids: Arc::new(AtomicUsize::new(0)),
            connections: SyncMutex::new(BTreeMap::new()),
            ephemeral_signer: OnceLock::new(),
        }
    }

    /// Client-lifetime proof identity for callers authorized by independent credentials.
    pub fn ephemeral_signer(&self) -> &Signer {
        self.ephemeral_signer
            .get_or_init(|| crate::sign::AnySigningKey::generate().into())
    }

    pub(crate) fn capture(&self, method: &str, params: Value) -> Result<CapturedRequest, RpcError> {
        RpcClient::capture(
            &self.ids,
            GenericRpcMethod::<_, _, Value>::new(method),
            params,
        )
    }

    pub(crate) async fn session(
        &self,
        signer: &Signer,
        url: Url,
        address: &str,
        headers: HeaderMap,
    ) -> Result<Arc<Connection>, Error> {
        let mut header_names = headers.keys().collect::<Vec<_>>();
        header_names.sort_unstable_by_key(|name| name.as_str());
        let endpoint = Endpoint {
            url: url.clone(),
            address: address.to_owned(),
            signer: signer.verifying_key().await?,
            headers: header_names
                .into_iter()
                .map(|name| {
                    (
                        name.to_string(),
                        headers
                            .get_all(name)
                            .iter()
                            .map(|value| value.as_bytes().to_vec())
                            .collect(),
                    )
                })
                .collect(),
        };
        let entry = self
            .connections
            .mutate(|connections| connections.entry(endpoint).or_default().clone());
        let mut connection = entry.lock().await;
        if let Some(connection) = &*connection {
            if !connection.dispatcher.is_closed() {
                return Ok(connection.clone());
            }
        }
        *connection = None;
        let (peer, socket) =
            connect(&self.network, signer, url, address, Target::Rpc, headers).await?;
        let mut socket = WebSocket::new(socket);
        socket.set_send_timeout(WRITE_TIMEOUT);
        let (writer, reader) = socket.split();
        let writer = writer
            .sink_map_err(|error| Error::new(error, ErrorKind::Network))
            .with(|body: String| futures::future::ready(Ok(Message::Text(body.into()))));
        let reader = reader.filter_map(|message| async move {
            match message {
                Ok(Message::Text(body)) => Some(Ok(body.to_string())),
                Ok(Message::Ping(_) | Message::Pong(_)) => None,
                Err(error) => Some(Err(Error::new(error, ErrorKind::Network))),
                _ => Some(Err(Error::new(
                    std::io::Error::from(std::io::ErrorKind::UnexpectedEof),
                    ErrorKind::Network,
                ))),
            }
        });
        let established = Arc::new(Connection {
            peer,
            dispatcher: RpcClient::messages(writer, reader, self.ids.clone()),
        });
        *connection = Some(established.clone());
        Ok(established)
    }

    /// Cancels only this operation when its future or item receiver is dropped.
    pub async fn stream<Item: serde::de::DeserializeOwned, Final: serde::de::DeserializeOwned>(
        &self,
        signer: &Signer,
        url: Url,
        address: &str,
        headers: HeaderMap,
        method: &str,
        params: Value,
    ) -> Result<RpcResponseStream<Value, Item, Final>, RpcError> {
        let request = self.capture(method, params)?;
        let connection = self.session(signer, url, address, headers).await?;
        connection.dispatcher.send_stream(request).await
    }

    /// Dropping this future cancels only its WebSocket request.
    /// Use `tokio::time::timeout` for a per-call deadline; no reply deadline applies by default.
    pub async fn call(
        &self,
        signer: &Signer,
        url: Url,
        address: &str,
        headers: HeaderMap,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        let request = self.capture(method, params)?;
        self.session(signer, url, address, headers)
            .await?
            .request(request)
            .await
    }
}
