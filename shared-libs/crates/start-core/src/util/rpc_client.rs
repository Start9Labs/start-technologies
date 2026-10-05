use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Weak};

use futures::future::BoxFuture;
use futures::{FutureExt, TryFutureExt};
use lazy_async_pool::Pool;
use rpc_toolkit::yajrc::{self, Id, RpcError, RpcMethod, RpcRequest, RpcResponse};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::runtime::Handle;
use tokio::sync::{Mutex, OnceCell, oneshot};

use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::TmpDir;
use crate::{Error, ErrorKind, ResultExt};

type DynWrite = Box<dyn AsyncWrite + Unpin + Send + Sync + 'static>;
type ResponseMap = BTreeMap<Id, oneshot::Sender<Result<Value, RpcError>>>;

const MAX_TRIES: u64 = 3;

pub struct RpcClient {
    id: Arc<AtomicUsize>,
    handler: NonDetachingJoinHandle<()>,
    writer: DynWrite,
    responses: Weak<Mutex<ResponseMap>>,
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
        let writer: DynWrite = Box::new(writer);
        let responses = Arc::new(Mutex::new(ResponseMap::new()));
        let weak_responses = Arc::downgrade(&responses);
        RpcClient {
            id,
            handler: tokio::spawn(async move {
                let mut lines = BufReader::new(reader).lines();
                while let Some(line) = lines.next_line().await.transpose() {
                    match line.map_err(Error::from).and_then(|l| {
                        serde_json::from_str::<RpcResponse>(&l)
                            .with_kind(ErrorKind::Deserialization)
                    }) {
                        Ok(l) => {
                            if let Some(id) = l.id {
                                if let Some(res) = responses.lock().await.remove(&id) {
                                    if let Err(e) = res.send(l.result) {
                                        tracing::warn!(
                                            "RpcClient Response after request aborted: {:?}",
                                            e
                                        );
                                    }
                                } else {
                                    tracing::warn!(
                                        "RpcClient Response for Unknown ID: {:?}",
                                        l.result
                                    );
                                }
                            } else {
                                tracing::info!("RpcClient Notification: {:?}", l);
                            }
                        }
                        Err(e) => {
                            tracing::error!("RpcClient Error: {}", e);
                            tracing::debug!("{:?}", e);
                        }
                    }
                }
                for (_, res) in std::mem::take(&mut *responses.lock().await) {
                    if let Err(e) = res.send(Err(RpcError {
                        data: Some("client disconnected before response received".into()),
                        ..yajrc::INTERNAL_ERROR
                    })) {
                        tracing::warn!("RpcClient Response after request aborted: {:?}", e);
                    }
                }
            })
            .into(),
            writer,
            responses: weak_responses,
        }
    }

    pub async fn request<T: RpcMethod>(
        &mut self,
        method: T,
        params: T::Params,
    ) -> Result<T::Response, RpcError>
    where
        T: Serialize,
        T::Params: Serialize,
        T::Response: for<'de> Deserialize<'de>,
    {
        let id = Id::Number(
            self.id
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                .into(),
        );
        let request = RpcRequest {
            id: Some(id.clone()),
            method,
            params,
        };
        if let Some(w) = self.responses.upgrade() {
            let (send, recv) = oneshot::channel();
            w.lock().await.insert(id.clone(), send);
            self.writer
                .write_all((serde_json::to_string(&request)? + "\n").as_bytes())
                .await
                .map_err(|e| {
                    let mut err = rpc_toolkit::yajrc::INTERNAL_ERROR.clone();
                    err.data = Some(json!(e.to_string()));
                    err
                })?;
            match recv.await {
                Ok(val) => {
                    return Ok(serde_json::from_value(val?)?);
                }
                Err(_err) => {
                    tokio::task::yield_now().await;
                }
            }
        }
        tracing::debug!(
            "Client has finished {:?}",
            futures::poll!(&mut self.handler)
        );
        let mut err = rpc_toolkit::yajrc::INTERNAL_ERROR.clone();
        err.data = Some(json!("RpcClient thread has terminated"));
        Err(err)
    }

    pub async fn notify<T: RpcMethod>(
        &mut self,
        method: T,
        params: T::Params,
    ) -> Result<(), RpcError>
    where
        T: Serialize,
        T::Params: Serialize,
    {
        let request = RpcRequest {
            id: None,
            method,
            params,
        };
        self.writer
            .write_all((serde_json::to_string(&request)? + "\n").as_bytes())
            .await
            .map_err(|e| {
                let mut err = rpc_toolkit::yajrc::INTERNAL_ERROR.clone();
                err.data = Some(json!(e.to_string()));
                err
            })?;
        Ok(())
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
                        if path.as_os_str().len() >= 108
                        // libc::sockaddr_un.sun_path.len()
                        {
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

    pub async fn request<T: RpcMethod>(
        &self,
        method: T,
        params: T::Params,
    ) -> Result<T::Response, RpcError>
    where
        T: Serialize + Clone,
        T::Params: Serialize + Clone,
        T::Response: for<'de> Deserialize<'de>,
    {
        let mut tries = 0;
        let res = loop {
            let mut client = self.pool.clone().get().await?;
            if client.handler.is_finished() {
                client.destroy();
                continue;
            }
            let res = client.request(method.clone(), params.clone()).await;
            match &res {
                Err(e) if e.code == rpc_toolkit::yajrc::INTERNAL_ERROR.code => {
                    let mut e = Error::from(e.clone());
                    e.kind = ErrorKind::Filesystem;
                    tracing::error!("{e}");
                    tracing::debug!("{e:?}");
                    client.destroy();
                }
                _ => break res,
            }
            tries += 1;
            if tries > MAX_TRIES {
                tracing::warn!("Max Tries exceeded");
                break res;
            }
        };
        res
    }

    pub async fn notify<T: RpcMethod>(&self, method: T, params: T::Params) -> Result<(), RpcError>
    where
        T: Serialize + Clone,
        T::Params: Serialize + Clone,
    {
        let mut tries = 0;
        let res = loop {
            let mut client = self.pool.clone().get().await?;
            if client.handler.is_finished() {
                client.destroy();
                continue;
            }
            let res = client.notify(method.clone(), params.clone()).await;
            match &res {
                Err(e) if e.code == rpc_toolkit::yajrc::INTERNAL_ERROR.code => {
                    let mut e = Error::from(e.clone());
                    e.kind = ErrorKind::Filesystem;
                    tracing::error!("{e}");
                    tracing::debug!("{e:?}");
                    client.destroy();
                }
                _ => break res,
            }
            tries += 1;
            if tries > MAX_TRIES {
                tracing::warn!("Max Tries exceeded");
                break res;
            }
        };
        res
    }
}
