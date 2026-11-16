use std::collections::BTreeMap;
use std::ops::Deref;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

use futures::future::BoxFuture;
use futures::{FutureExt, Sink, SinkExt, Stream, StreamExt, TryFutureExt};
use lazy_async_pool::Pool;
use rpc_toolkit::yajrc::{Id, RpcError, RpcMethod, RpcRequest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::runtime::Handle;
use tokio::sync::{Mutex, Notify, OnceCell, oneshot};

use crate::util::GeneralGuard;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::TmpDir;
use crate::util::sync::SyncMutex;
use crate::{Error, ErrorKind, ResultExt};

mod stream;
#[cfg(test)]
mod tests;
use stream::{Cancellation, ResponseSlot, SlotAction, StreamSlot};
pub use stream::{RpcResponseItems, RpcResponseStream};

type MessageWriter = Pin<Box<dyn Sink<String, Error = Error> + Send>>;
type ResponseMap = BTreeMap<Id, ResponseSlot>;
type Responses = SyncMutex<Option<ResponseMap>>;
type CallResponse = oneshot::Receiver<Result<Value, RpcError>>;

pub(crate) struct CapturedRequest {
    id: Id,
    body: String,
}

pub struct RpcClient {
    id: Arc<AtomicUsize>,
    handler: NonDetachingJoinHandle<()>,
    writer: Arc<Mutex<MessageWriter>>,
    responses: Weak<Responses>,
    cancel: Arc<Notify>,
}

fn disconnected() -> Error {
    Error::new(
        std::io::Error::from(std::io::ErrorKind::UnexpectedEof),
        ErrorKind::Network,
    )
}

impl RpcClient {
    pub fn new<
        W: AsyncWrite + Unpin + Send + Sync + 'static,
        R: AsyncRead + Unpin + Send + Sync + 'static,
    >(
        writer: W,
        reader: R,
        id: Arc<AtomicUsize>,
    ) -> Self {
        let writer = futures::sink::unfold(writer, |mut writer, body: String| async move {
            writer.write_all((body + "\n").as_bytes()).await?;
            Ok(writer)
        });
        let reader =
            futures::stream::try_unfold(BufReader::new(reader).lines(), |mut lines| async move {
                Ok(lines.next_line().await?.map(|line| (line, lines)))
            });
        Self::messages(writer, reader, id)
    }

    pub(crate) fn messages(
        writer: impl Sink<String, Error = Error> + Send + 'static,
        reader: impl Stream<Item = Result<String, Error>> + Send + 'static,
        id: Arc<AtomicUsize>,
    ) -> Self {
        let responses = Arc::new(Responses::new(Some(ResponseMap::new())));
        let weak_responses = Arc::downgrade(&responses);
        let writer: Arc<Mutex<MessageWriter>> = Arc::new(Mutex::new(Box::pin(writer)));
        let cancel = Arc::new(Notify::new());
        let handler_writer = writer.clone();
        let handler_cancel = cancel.clone();
        let handler = tokio::spawn(async move {
            let closed = responses.clone();
            let _disconnect = GeneralGuard::new(move || closed.replace(None));
            let mut messages = Box::pin(reader);
            let cancellations =
                Self::send_cancellations(&handler_writer, &responses, &handler_cancel);
            tokio::pin!(cancellations);
            loop {
                let message = tokio::select! {
                    message = messages.next() => message,
                    error = &mut cancellations => { if let Err(error) = error { tracing::error!("RpcClient cancellation: {error}"); } break; }
                };
                let Some(message) = message else { break };
                if let Err(error) =
                    message.and_then(|body| Self::receive(&responses, &handler_cancel, &body))
                {
                    tracing::error!("RpcClient Error: {}", error);
                    tracing::debug!("{:?}", error);
                    break;
                }
            }
        });
        Self {
            id,
            handler: handler.into(),
            writer,
            responses: weak_responses,
            cancel,
        }
    }

    async fn send_cancellations(
        writer: &Mutex<MessageWriter>,
        responses: &Responses,
        cancel: &Notify,
    ) -> Result<(), Error> {
        loop {
            cancel.notified().await;
            loop {
                let id = responses.peek(|pending| {
                    pending.as_ref().and_then(|map| {
                        map.iter().find_map(|(id, slot)| {
                            matches!(slot, ResponseSlot::Canceled).then(|| id.clone())
                        })
                    })
                });
                let Some(id) = id else { break };
                let body =
                    serde_json::json!({"rpcstream": rpc_toolkit::RPCSTREAM_VERSION, "close": id})
                        .to_string();
                Self::write(writer, responses, body).await?;
                responses.mutate(|pending| pending.as_mut().and_then(|map| map.remove(&id)));
            }
        }
    }

    fn receive(responses: &Responses, cancel: &Notify, body: &str) -> Result<(), Error> {
        enum Incoming {
            Reply(rpc_toolkit::RpcResponse),
            Event(rpc_toolkit::RpcStreamResponse),
        }
        let value: Value = serde_json::from_str(body).with_kind(ErrorKind::Deserialization)?;
        let incoming = if value.get("rpcstream").is_some() {
            Incoming::Event(serde_json::from_value(value).with_kind(ErrorKind::Deserialization)?)
        } else {
            Incoming::Reply(serde_json::from_value(value).with_kind(ErrorKind::Deserialization)?)
        };
        let id = match &incoming {
            Incoming::Reply(reply) => reply.id.clone(),
            Incoming::Event(reply) => Some(reply.id.clone()),
        };
        let Some(id) = id else {
            if let Incoming::Reply(response) = incoming {
                tracing::info!("RpcClient Notification: {:?}", response);
            }
            return Ok(());
        };
        responses.mutate(|pending| {
            let Some(map) = pending.as_mut() else { return };
            let Some(slot) = map.remove(&id) else { return };
            match slot {
                ResponseSlot::Call(sender) => {
                    let result = match incoming {
                        Incoming::Reply(reply) => reply
                            .result
                            .and_then(|value| Ok(serde_json::to_value(value)?)),
                        Incoming::Event(_) => Err(rpc_toolkit::yajrc::INVALID_REQUEST_ERROR),
                    };
                    let _ = sender.send(result);
                }
                ResponseSlot::Stream(mut stream) => {
                    let action = match incoming {
                        Incoming::Reply(reply) => stream.initial(reply),
                        Incoming::Event(reply) => stream.event(reply, body.len()),
                    };
                    match action {
                        SlotAction::Keep => {
                            map.insert(id, ResponseSlot::Stream(stream));
                        }
                        SlotAction::Remove => (),
                        SlotAction::Cancel => {
                            map.insert(id, ResponseSlot::Canceled);
                            cancel.notify_one();
                        }
                    }
                }
                ResponseSlot::Canceled => {
                    map.insert(id, ResponseSlot::Canceled);
                }
            }
        });
        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        self.responses
            .upgrade()
            .is_none_or(|responses| responses.peek(Option::is_none))
    }

    pub(crate) fn capture<T: RpcMethod + Serialize>(
        ids: &AtomicUsize,
        method: T,
        params: T::Params,
    ) -> Result<CapturedRequest, RpcError>
    where
        T::Params: Serialize,
    {
        let id = Id::Number(ids.fetch_add(1, Ordering::SeqCst).into());
        let mut body = serde_json::to_string(&RpcRequest {
            id: Some(id.clone()),
            method,
            params,
        })?;
        body.insert_str(1, "\"jsonrpc\":\"2.0\",");
        Ok(CapturedRequest { id, body })
    }

    async fn write(
        writer: &Mutex<MessageWriter>,
        responses: &Responses,
        body: String,
    ) -> Result<(), Error> {
        let mut writer = writer.lock().await;
        if responses.peek(Option::is_none) {
            return Err(disconnected());
        }
        let result = writer.send(body).await;
        if result.is_err() {
            responses.replace(None);
        }
        result
    }

    async fn send(&self, body: String) -> Result<(), Error> {
        let responses = self.responses.upgrade().ok_or_else(disconnected)?;
        let result = Self::write(&self.writer, &responses, body).await;
        if result.is_err() {
            self.handler.abort();
        }
        result
    }

    fn register_call(&self, id: &Id) -> Result<(Arc<Responses>, CallResponse), Error> {
        let responses = self.responses.upgrade().ok_or_else(disconnected)?;
        let (sender, receiver) = oneshot::channel();
        responses.mutate(|pending| {
            pending
                .as_mut()
                .ok_or_else(disconnected)?
                .insert(id.clone(), ResponseSlot::Call(sender));
            Ok::<_, Error>(())
        })?;
        Ok((responses, receiver))
    }

    async fn send_registered_call(
        &self,
        body: String,
        receiver: CallResponse,
    ) -> Result<Result<Value, RpcError>, Error> {
        self.send(body).await?;
        receiver.await.map_err(|_| disconnected())
    }

    /// Consumes a captured Unix-line request once, with local cancellation.
    pub(crate) async fn send_request(
        &self,
        request: CapturedRequest,
    ) -> Result<Result<Value, RpcError>, Error> {
        let (responses, receiver) = self.register_call(&request.id)?;
        let _cancel = GeneralGuard::new(move || {
            responses.mutate(|map| map.as_mut().and_then(|map| map.remove(&request.id)))
        });
        self.send_registered_call(request.body, receiver).await
    }

    /// Dropping the WebSocket call cancels its server handler without closing siblings.
    pub(crate) async fn send_cancellable_request(
        &self,
        request: CapturedRequest,
    ) -> Result<Result<Value, RpcError>, Error> {
        let (responses, receiver) = self.register_call(&request.id)?;
        let _cancel = Cancellation {
            responses: Arc::downgrade(&responses),
            notify: self.cancel.clone(),
            id: request.id,
        };
        self.send_registered_call(request.body, receiver).await
    }

    /// Opts into response streaming without replaying a possibly sent request.
    pub(crate) async fn send_stream<Item, Final>(
        &self,
        mut request: CapturedRequest,
    ) -> Result<RpcResponseStream<imbl_value::Value, Item, Final>, RpcError> {
        let responses = self.responses.upgrade().ok_or_else(disconnected)?;
        let cancel = Cancellation {
            responses: Arc::downgrade(&responses),
            notify: self.cancel.clone(),
            id: request.id.clone(),
        };
        let (slot, initial, items) = StreamSlot::new(cancel);
        responses.mutate(|pending| {
            let map = pending.as_mut().ok_or_else(disconnected)?;
            if map
                .values()
                .filter(|slot| !matches!(slot, ResponseSlot::Call(_)))
                .count()
                >= rpc_toolkit::WebSocketOptions::default().max_streams.get()
            {
                return Err(rpc_toolkit::RPCSTREAM_CANCELLED_ERROR);
            }
            map.insert(request.id.clone(), ResponseSlot::Stream(slot));
            Ok(())
        })?;
        request.body.insert_str(
            1,
            &format!("\"rpcstream\":{:?},", rpc_toolkit::RPCSTREAM_VERSION),
        );
        self.send(request.body).await?;
        let response = initial.await.map_err(|_| disconnected())??;
        Ok(RpcResponseStream {
            initial: response.result?,
            meta: imbl_value::Value::Object(response.meta),
            items,
        })
    }

    /// Requires peer rpcstream support; never retries through another transport.
    pub async fn request_stream<
        T: RpcMethod + Serialize,
        Item: serde::de::DeserializeOwned,
        Final: serde::de::DeserializeOwned,
    >(
        &self,
        method: T,
        params: T::Params,
    ) -> Result<RpcResponseStream<T::Response, Item, Final>, RpcError>
    where
        T::Params: Serialize,
        T::Response: serde::de::DeserializeOwned,
    {
        let response = self
            .send_stream(Self::capture(&self.id, method, params)?)
            .await?;
        Ok(RpcResponseStream {
            initial: imbl_value::from_value(response.initial)?,
            meta: response.meta,
            items: response.items,
        })
    }

    pub async fn request<T: RpcMethod + Serialize>(
        &self,
        method: T,
        params: T::Params,
    ) -> Result<T::Response, RpcError>
    where
        T::Params: Serialize,
        T::Response: for<'de> Deserialize<'de>,
    {
        let request = Self::capture(&self.id, method, params)?;
        Ok(serde_json::from_value(self.send_request(request).await??)?)
    }

    pub async fn notify<T: RpcMethod + Serialize>(
        &self,
        method: T,
        params: T::Params,
    ) -> Result<(), RpcError>
    where
        T::Params: Serialize,
    {
        Ok(self
            .send(serde_json::to_string(&RpcRequest {
                id: None,
                method,
                params,
            })?)
            .await?)
    }
}

#[derive(Clone)]
pub struct UnixRpcClient {
    pool: Pool<
        RpcClient,
        Box<dyn Fn() -> BoxFuture<'static, Result<RpcClient, std::io::Error>> + Send + Sync>,
        BoxFuture<'static, Result<RpcClient, std::io::Error>>,
        std::io::Error,
    >,
}
impl UnixRpcClient {
    pub fn new(path: PathBuf) -> Self {
        let tmpdir = Arc::new(OnceCell::new());
        let rt = Handle::current();
        let id = Arc::new(AtomicUsize::new(0));
        Self {
            pool: Pool::new(
                0,
                Box::new(move || {
                    let mut path = path.clone();
                    let id = id.clone();
                    let tmpdir = tmpdir.clone();
                    NonDetachingJoinHandle::from(rt.spawn(async move {
                        if path.as_os_str().len() >= 108 {
                            let new_path = tmpdir
                                .get_or_try_init(|| TmpDir::new())
                                .await
                                .map_err(|e| {
                                    std::io::Error::new(std::io::ErrorKind::Other, e.source)
                                })?
                                .join("link.sock");
                            if tokio::fs::metadata(&new_path).await.is_err() {
                                tokio::fs::symlink(&path, &new_path).await?;
                            }
                            path = new_path;
                        }
                        let (r, w) = UnixStream::connect(&path).await?.into_split();
                        Ok(RpcClient::new(w, r, id))
                    }))
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                    .and_then(|x| async move { x })
                    .boxed()
                }),
            ),
        }
    }

    async fn client(&self) -> Result<impl Deref<Target = RpcClient>, std::io::Error> {
        loop {
            let client = self.pool.clone().get().await?;
            if client.is_closed() {
                client.destroy();
            } else {
                return Ok(client);
            }
        }
    }

    pub async fn request<T: RpcMethod + Serialize>(
        &self,
        method: T,
        params: T::Params,
    ) -> Result<T::Response, RpcError>
    where
        T::Params: Serialize,
        T::Response: for<'de> Deserialize<'de>,
    {
        self.client().await?.request(method, params).await
    }

    pub async fn notify<T: RpcMethod + Serialize>(
        &self,
        method: T,
        params: T::Params,
    ) -> Result<(), RpcError>
    where
        T::Params: Serialize,
    {
        self.client().await?.notify(method, params).await
    }
}
