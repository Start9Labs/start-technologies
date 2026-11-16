use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{ready, Context as TaskContext, Poll};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use futures::future::BoxFuture;
use futures::{FutureExt, Sink, SinkExt, Stream, StreamExt};
use rpc_toolkit::{
    from_fn_async, BoxWebSocket, Context, HandlerArgs, HandlerExt, ParentHandler, Server,
    UpgradedWebSocket, WebSocketOptions, WebSocketUpgrader,
};
use tokio::sync::Notify;
use yajrc::RpcError;

use super::{receive, send, serve};

#[derive(Clone, Default)]
struct FlushContext {
    started: Arc<Notify>,
    release: Arc<Notify>,
    done: Arc<Notify>,
    pending_started: Arc<Notify>,
    pending_dropped: Arc<Notify>,
    result: Arc<Mutex<Option<Result<(), RpcError>>>>,
    fail: bool,
}
impl Context for FlushContext {}

struct FlushSocket {
    inner: BoxWebSocket,
    context: Arc<FlushContext>,
    released: Pin<Box<dyn Future<Output = ()> + Send>>,
    terminal: bool,
    finished: bool,
}

impl Stream for FlushSocket {
    type Item = Result<Message, axum::Error>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().inner.poll_next_unpin(cx)
    }
}
impl Sink<Message> for FlushSocket {
    type Error = axum::Error;
    fn poll_ready(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.get_mut().inner.poll_ready_unpin(cx)
    }
    fn start_send(self: Pin<&mut Self>, message: Message) -> Result<(), Self::Error> {
        let this = self.get_mut();
        if let Message::Text(text) = &message {
            let value: serde_json::Value = serde_json::from_str(text).unwrap();
            this.terminal |= value.is_array()
                || value.get("end").and_then(serde_json::Value::as_bool) == Some(true);
        }
        this.inner.start_send_unpin(message)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.get_mut();
        ready!(this.inner.poll_flush_unpin(cx))?;
        if this.terminal && !this.finished {
            this.context.started.notify_one();
            ready!(this.released.as_mut().poll(cx));
            this.finished = true;
            if this.context.fail {
                return Poll::Ready(Err(axum::Error::new(std::io::Error::from(
                    std::io::ErrorKind::BrokenPipe,
                ))));
            }
        }
        Poll::Ready(Ok(()))
    }
    fn poll_close(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.get_mut().inner.poll_close_unpin(cx)
    }
}

struct FlushUpgrader;
impl WebSocketUpgrader<FlushContext> for FlushUpgrader {
    fn max_wire_message_size(&self, logical: std::num::NonZeroUsize) -> std::num::NonZeroUsize {
        logical
    }
    fn request_context(&self, context: &FlushContext) -> FlushContext {
        context.clone()
    }
    fn upgrade(
        &self,
        context: Arc<FlushContext>,
        _: axum::http::request::Parts,
        socket: WebSocket,
        _: WebSocketOptions,
    ) -> BoxFuture<'static, Result<UpgradedWebSocket<FlushContext>, axum::Error>> {
        async move {
            let released = Box::pin(context.release.clone().notified_owned());
            Ok(UpgradedWebSocket {
                socket: Box::new(FlushSocket {
                    inner: Box::new(socket),
                    context: context.clone(),
                    released,
                    terminal: false,
                    finished: false,
                }),
                context,
            })
        }
        .boxed()
    }
}

struct PendingHandler(Arc<Notify>);
impl Drop for PendingHandler {
    fn drop(&mut self) {
        self.0.notify_one();
    }
}

#[tokio::test]
async fn cancellation_drops_the_handler_before_a_sibling_batch_flush_can_stall() {
    let context = FlushContext::default();
    let base = context.clone();
    let server = Server::new(
        move || {
            let base = base.clone();
            async move { Ok(base) }
        },
        ParentHandler::new()
            .subcommand(
                "blocked",
                from_fn_async(|args: HandlerArgs<FlushContext>| async move {
                    let _guard = PendingHandler(args.context.pending_dropped.clone());
                    args.context.pending_started.notify_one();
                    futures::future::pending::<()>().await;
                    Ok::<_, RpcError>(())
                })
                .no_cli()
                .no_ts(),
            )
            .subcommand(
                "echo",
                from_fn_async(|_: HandlerArgs<FlushContext>| async { Ok::<_, RpcError>(()) })
                    .no_cli()
                    .no_ts(),
            ),
    )
    .for_websocket()
    .with_upgrader(FlushUpgrader);
    let (url, _guard) = serve(server).await;
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    send(
        &mut socket,
        serde_json::json!([
            {"jsonrpc":"2.0","id":1,"method":"blocked"},
            {"jsonrpc":"2.0","id":2,"method":"echo"}
        ]),
    )
    .await;
    tokio::time::timeout(Duration::from_secs(3), context.pending_started.notified())
        .await
        .unwrap();
    send(
        &mut socket,
        serde_json::json!({"rpcstream":"1.0","close":1}),
    )
    .await;
    tokio::time::timeout(Duration::from_secs(3), context.started.notified())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), context.pending_dropped.notified())
        .await
        .unwrap();
    context.release.notify_one();
    assert_eq!(
        receive(&mut socket).await,
        serde_json::json!([{"jsonrpc":"2.0","id":2,"result":null}])
    );
}

#[tokio::test]
async fn terminal_receipt_resolves_after_transport_flush_and_fails_with_the_write() {
    for fail in [false, true] {
        let context = FlushContext {
            fail,
            ..Default::default()
        };
        let base = context.clone();
        let server = Server::new(
            move || {
                let base = base.clone();
                async move { Ok(base) }
            },
            ParentHandler::new().subcommand(
                "finish",
                from_fn_async(|args: HandlerArgs<FlushContext>| async move {
                    let producer = args.rpc_stream.unwrap();
                    let context = args.context;
                    tokio::spawn(async move {
                        let result = producer.close_and_flush(Ok::<_, RpcError>(())).await;
                        *context.result.lock().unwrap() = Some(result);
                        context.done.notify_one();
                    });
                    Ok::<_, RpcError>(())
                })
                .no_cli()
                .no_ts(),
            ),
        )
        .for_websocket()
        .with_upgrader(FlushUpgrader);
        let (url, _guard) = serve(server).await;
        let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        send(
            &mut socket,
            serde_json::json!({"jsonrpc":"2.0","rpcstream":"1.0","id":1,"method":"finish"}),
        )
        .await;
        assert!(receive(&mut socket).await.get("result").is_some());
        tokio::time::timeout(Duration::from_secs(3), context.started.notified())
            .await
            .unwrap();
        assert!(context.result.lock().unwrap().is_none());
        context.release.notify_one();
        tokio::time::timeout(Duration::from_secs(3), context.done.notified())
            .await
            .unwrap();
        assert_eq!(
            context.result.lock().unwrap().take().unwrap().is_err(),
            fail
        );
    }
}
