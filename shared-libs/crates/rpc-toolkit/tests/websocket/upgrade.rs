use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message as ServerMessage, WebSocket};
use futures::future::BoxFuture;
use futures::{FutureExt, SinkExt, StreamExt};
use rpc_toolkit::{
    from_fn_async, BoxWebSocket, Context, Empty, HandlerExt, ParentHandler, Server,
    UpgradedWebSocket, WebSocketOptions, WebSocketUpgrader,
};
use serde_json::json;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use yajrc::RpcError;

use super::{receive, send, serve};

#[derive(Clone, Default)]
struct ScopedContext {
    peer: Option<u64>,
    calls: Arc<AtomicUsize>,
}
impl Context for ScopedContext {}

struct FixtureUpgrader;
impl WebSocketUpgrader<ScopedContext> for FixtureUpgrader {
    fn request_context(&self, context: &ScopedContext) -> ScopedContext {
        context.clone()
    }

    fn max_wire_message_size(&self, logical: NonZeroUsize) -> NonZeroUsize {
        NonZeroUsize::new(logical.get() + 1024).unwrap()
    }

    fn upgrade(
        &self,
        context: Arc<ScopedContext>,
        _request: axum::http::request::Parts,
        mut socket: WebSocket,
        _: WebSocketOptions,
    ) -> BoxFuture<'static, Result<UpgradedWebSocket<ScopedContext>, axum::Error>> {
        async move {
            let message = tokio::time::timeout(Duration::from_secs(3), socket.recv())
                .await
                .map_err(axum::Error::new)?;
            let peer = match message {
                Some(Ok(ServerMessage::Text(text))) => text.parse::<u64>().ok(),
                _ => None,
            }
            .ok_or_else(|| {
                axum::Error::new(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            })?;
            let mut scoped = (*context).clone();
            scoped.peer = Some(peer);
            Ok(UpgradedWebSocket {
                context: Arc::new(scoped),
                socket: Box::new(socket) as BoxWebSocket,
            })
        }
        .boxed()
    }
}

fn server(context: ScopedContext) -> rpc_toolkit::WebSocketServer<ScopedContext> {
    Server::new(
        move || {
            let context = context.clone();
            async move { Ok(context) }
        },
        ParentHandler::new().subcommand(
            "peer",
            from_fn_async(|context: ScopedContext, _: Empty| async move {
                context.calls.fetch_add(1, Ordering::SeqCst);
                Ok::<_, RpcError>(context.peer)
            })
            .no_cli()
            .no_ts(),
        ),
    )
    .for_websocket()
    .with_upgrader(FixtureUpgrader)
}

#[tokio::test]
async fn admitted_context_is_connection_scoped_and_shared_by_its_calls() {
    let base = ScopedContext::default();
    let (url, _guard) = serve(server(base.clone())).await;
    let (mut first, _) = connect_async(&url).await.unwrap();
    first.send(Message::Text("11".into())).await.unwrap();
    for id in [1, 2] {
        send(&mut first, json!({"jsonrpc":"2.0","id":id,"method":"peer"})).await;
        let reply = receive(&mut first).await;
        assert!(reply.get("error").is_none(), "{:?}", reply);
        assert_eq!(reply["result"], json!(11));
    }
    let (mut second, _) = connect_async(&url).await.unwrap();
    second.send(Message::Text("22".into())).await.unwrap();
    send(&mut second, json!({"jsonrpc":"2.0","id":3,"method":"peer"})).await;
    assert_eq!(receive(&mut second).await["result"], json!(22));
    assert_eq!(base.peer, None);
    assert_eq!(base.calls.load(Ordering::SeqCst), 3);
    first.close(None).await.unwrap();
    second.close(None).await.unwrap();
}

#[tokio::test]
async fn rejected_handshake_never_dispatches_rpc() {
    let base = ScopedContext::default();
    let (url, _guard) = serve(server(base.clone())).await;
    let (mut socket, _) = connect_async(&url).await.unwrap();
    send(&mut socket, json!({"jsonrpc":"2.0","id":1,"method":"peer"})).await;
    let result = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap();
    assert!(!matches!(result, Some(Ok(Message::Text(_)))));
    assert_eq!(base.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn expanded_wire_budget_does_not_expand_logical_rpc_budget() {
    let base = ScopedContext::default();
    let options = WebSocketOptions {
        max_message_size: NonZeroUsize::new(128).unwrap(),
        ..Default::default()
    };
    let (url, _guard) = serve(server(base.clone()).with_options(options)).await;
    let request =
        json!({"jsonrpc":"2.0","id":1,"method":"peer","params":{"padding":"x".repeat(160)}})
            .to_string();
    assert!(request.len() > 128 && request.len() < 1152);
    for message in [
        Message::Text(request.into()),
        Message::Binary(vec![0; 160].into()),
    ] {
        let (mut socket, _) = connect_async(&url).await.unwrap();
        socket.send(Message::Text("33".into())).await.unwrap();
        socket.send(message).await.unwrap();
        match tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap()
        {
            Some(Ok(Message::Close(Some(frame)))) => assert_eq!(u16::from(frame.code), 1009),
            other => panic!("expected logical-budget close, got {:?}", other),
        }
    }
    assert_eq!(base.calls.load(Ordering::SeqCst), 0);
}

struct BinaryUpgrader;
impl WebSocketUpgrader<ScopedContext> for BinaryUpgrader {
    fn request_context(&self, context: &ScopedContext) -> ScopedContext {
        context.clone()
    }

    fn max_wire_message_size(&self, logical: NonZeroUsize) -> NonZeroUsize {
        logical
    }

    fn upgrade(
        &self,
        context: Arc<ScopedContext>,
        request: axum::http::request::Parts,
        socket: WebSocket,
        options: WebSocketOptions,
    ) -> BoxFuture<'static, Result<UpgradedWebSocket<ScopedContext>, axum::Error>> {
        async move {
            let admitted = FixtureUpgrader
                .upgrade(context, request, socket, options)
                .await?;
            let socket = admitted
                .socket
                .with(|message| {
                    std::future::ready(Ok::<_, axum::Error>(match message {
                        ServerMessage::Text(text) => {
                            ServerMessage::Binary(text.as_bytes().to_vec().into())
                        }
                        other => other,
                    }))
                })
                .map(|message| {
                    message.and_then(|message| match message {
                        ServerMessage::Binary(data) => String::from_utf8(data.to_vec())
                            .map(|text| ServerMessage::Text(text.into()))
                            .map_err(axum::Error::new),
                        other => Ok(other),
                    })
                });
            Ok(UpgradedWebSocket {
                context: admitted.context,
                socket: Box::new(socket),
            })
        }
        .boxed()
    }
}

#[tokio::test]
async fn admitted_transport_transforms_both_directions() {
    let (url, _guard) = serve(server(ScopedContext::default()).with_upgrader(BinaryUpgrader)).await;
    let (mut socket, _) = connect_async(&url).await.unwrap();
    socket.send(Message::Text("44".into())).await.unwrap();
    let request = json!({"jsonrpc":"2.0","id":1,"method":"peer"}).to_string();
    socket
        .send(Message::Binary(request.into_bytes().into()))
        .await
        .unwrap();
    match tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
    {
        Message::Binary(reply) => {
            let reply: serde_json::Value = serde_json::from_slice(&reply).unwrap();
            assert!(reply.get("error").is_none(), "{:?}", reply);
            assert_eq!(reply["result"], json!(44));
        }
        other => panic!("expected adapted binary reply, got {:?}", other),
    }
    socket.close(None).await.unwrap();
}
