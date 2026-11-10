use std::collections::BTreeMap;
use std::ops::Deref;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

use futures::future::BoxFuture;
use futures::{FutureExt, Sink, SinkExt, Stream, StreamExt, TryFutureExt};
use lazy_async_pool::Pool;
use rpc_toolkit::yajrc::{Id, RpcError, RpcMethod, RpcRequest, RpcResponse};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::runtime::Handle;
use tokio::sync::{Mutex, OnceCell, oneshot};

use crate::util::GeneralGuard;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::TmpDir;
use crate::util::sync::SyncMutex;
use crate::{Error, ErrorKind, ResultExt};

#[cfg(test)]
mod tests;

type MessageWriter = Pin<Box<dyn Sink<String, Error = Error> + Send>>;
type ResponseMap = BTreeMap<Id, oneshot::Sender<Result<Value, RpcError>>>;
type Responses = SyncMutex<Option<ResponseMap>>;

pub(crate) struct CapturedRequest {
    id: Id,
    body: String,
}

pub struct RpcClient {
    id: Arc<AtomicUsize>,
    handler: NonDetachingJoinHandle<()>,
    writer: Mutex<MessageWriter>,
    responses: Weak<Responses>,
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
        let handler = tokio::spawn(async move {
            let closed = responses.clone();
            let _disconnect = GeneralGuard::new(move || closed.replace(None));
            let mut messages = Box::pin(reader);
            while let Some(message) = messages.next().await {
                let response = message.and_then(|body| {
                    serde_json::from_str::<RpcResponse>(&body).with_kind(ErrorKind::Deserialization)
                });
                match response {
                    Ok(response) => {
                        if let Some(id) = response.id {
                            if let Some(sender) = responses.mutate(|pending| {
                                pending.as_mut().and_then(|pending| pending.remove(&id))
                            }) {
                                let _ = sender.send(response.result);
                            }
                        } else {
                            tracing::info!("RpcClient Notification: {:?}", response);
                        }
                    }
                    Err(error) => {
                        tracing::error!("RpcClient Error: {}", error);
                        tracing::debug!("{:?}", error);
                        break;
                    }
                }
            }
        });
        Self {
            id,
            handler: handler.into(),
            writer: Mutex::new(Box::pin(writer)),
            responses: weak_responses,
        }
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

    async fn send(&self, body: String) -> Result<(), Error> {
        let mut writer = self.writer.lock().await;
        if self.is_closed() {
            return Err(disconnected());
        }
        let result = writer.send(body).await;
        if let Err(error) = result {
            if let Some(responses) = self.responses.upgrade() {
                responses.replace(None);
            }
            self.handler.abort();
            return Err(error);
        }
        Ok(())
    }

    /// Consumes a captured request once; transport failure never replays it.
    pub(crate) async fn send_request(
        &self,
        request: CapturedRequest,
    ) -> Result<Result<Value, RpcError>, Error> {
        let responses = self.responses.upgrade().ok_or_else(disconnected)?;
        let (sender, receiver) = oneshot::channel();
        responses.mutate(|pending| {
            pending
                .as_mut()
                .ok_or_else(disconnected)?
                .insert(request.id.clone(), sender);
            Ok::<_, Error>(())
        })?;
        let pending = responses.clone();
        let _cancel = GeneralGuard::new(move || {
            pending.mutate(|map| map.as_mut().and_then(|map| map.remove(&request.id)))
        });
        self.send(request.body).await?;
        receiver.await.map_err(|_| disconnected())
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
