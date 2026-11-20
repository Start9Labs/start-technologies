use std::path::Path;
use std::sync::Arc;

use futures::{Future, Stream, StreamExt, TryStreamExt};
use imbl_value::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, ToSocketAddrs, UnixListener};
use tokio::sync::Notify;
use yajrc::RpcError;

use crate::util::{parse_error, JobRunner, StreamUntil};
use crate::Server;

#[derive(Clone)]
pub struct ShutdownHandle(Arc<Notify>);
impl ShutdownHandle {
    pub fn shutdown(self) {
        self.0.notify_one();
    }
}

impl<Context: crate::Context> Server<Context> {
    /// Shutdown stops accepting connections and drains existing connections and their requests.
    /// Input EOF does not cancel accepted requests.
    pub fn run_socket<'a, T: AsyncRead + AsyncWrite + Send>(
        &'a self,
        listener: impl Stream<Item = std::io::Result<T>> + 'a,
        error_handler: impl Fn(std::io::Error) + Sync + 'a,
    ) -> (ShutdownHandle, impl Future<Output = ()> + 'a) {
        let shutdown = Arc::new(Notify::new());
        (ShutdownHandle(shutdown.clone()), async move {
            let mut runner = JobRunner::<std::io::Result<()>>::new();
            let jobs = StreamUntil::new(listener, shutdown.notified()).map(|pipe| async {
                let pipe = pipe?;
                let (r, mut w) = tokio::io::split(pipe);
                let stream = self.stream(
                    tokio_stream::wrappers::LinesStream::new(BufReader::new(r).lines())
                        .map_err(|e| RpcError {
                            data: Some(e.to_string().into()),
                            ..yajrc::INTERNAL_ERROR
                        })
                        .try_filter_map(|a| async move {
                            Ok(if a.is_empty() {
                                None
                            } else {
                                Some(serde_json::from_str::<Value>(&a).map_err(parse_error)?)
                            })
                        }),
                );
                tokio::pin!(stream);
                while let Some(res) = stream.next().await {
                    if let Err(e) = async {
                        let mut buf = serde_json::to_vec(
                            &res.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?,
                        )
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                        buf.push(b'\n');
                        w.write_all(&buf).await
                    }
                    .await
                    {
                        error_handler(e)
                    }
                }
                Ok(())
            });
            tokio::pin!(jobs);
            while let Some(res) = runner.next_result(&mut jobs).await {
                if let Err(e) = res {
                    error_handler(e)
                }
            }
        })
    }
    pub fn run_unix<'a>(
        &'a self,
        path: impl AsRef<Path> + 'a,
        error_handler: impl Fn(std::io::Error) + Sync + 'a,
    ) -> std::io::Result<(ShutdownHandle, impl Future<Output = ()> + 'a)> {
        let listener = UnixListener::bind(path)?;
        Ok(self.run_socket(
            tokio_stream::wrappers::UnixListenerStream::new(listener),
            error_handler,
        ))
    }
    pub async fn run_tcp<'a>(
        &'a self,
        addr: impl ToSocketAddrs + 'a,
        error_handler: impl Fn(std::io::Error) + Sync + 'a,
    ) -> std::io::Result<(ShutdownHandle, impl Future<Output = ()> + 'a)> {
        let listener = TcpListener::bind(addr).await?;
        Ok(self.run_socket(
            tokio_stream::wrappers::TcpListenerStream::new(listener),
            error_handler,
        ))
    }
}
