use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use rpc_toolkit::{Empty, HandlerArgs, HandlerExt, ParentHandler, Server, from_fn_async};
use tokio::sync::{Notify, oneshot};

use super::OpenAuthedContinuations;
use crate::util::GeneralGuard;
use crate::util::future::NonDetachingJoinHandle;

#[derive(Clone)]
struct Context;
impl rpc_toolkit::Context for Context {}

#[tokio::test]
async fn revocation_interrupts_a_blocked_producer_without_waiting_for_initialization() {
    let open = Arc::new(OpenAuthedContinuations::new());
    let started = Arc::new(Notify::new());
    let initialize = Arc::new(Notify::new());
    let (dropped, observed) = oneshot::channel();
    let dropped = Arc::new(std::sync::Mutex::new(Some(dropped)));
    let notifications = started.clone();
    let gate = initialize.clone();
    let registry = open.clone();
    let handler = ParentHandler::new().subcommand(
        "observe",
        from_fn_async(move |args: HandlerArgs<Context, Empty, Empty, u64>| {
            let started = notifications.clone();
            let initialize = gate.clone();
            let open = registry.clone();
            let dropped = dropped.clone();
            async move {
                let dropper = GeneralGuard::new(move || {
                    if let Some(sender) = dropped.lock().unwrap().take() {
                        let _ = sender.send(());
                    }
                });
                let source = futures::stream::unfold((0u64, dropper), move |(n, guard)| {
                    let started = started.clone();
                    async move {
                        if n == 1 {
                            started.notify_one();
                        }
                        Some((Ok(n), (n + 1, guard)))
                    }
                });
                open.subscribe_to_kill("key")
                    .forward_stream(args.rpc_stream.unwrap(), source);
                initialize.notified().await;
                Ok::<_, crate::Error>(())
            }
        })
        .no_cli()
        .no_ts(),
    );
    let server = Server::new(|| async { Ok(Context) }, handler)
        .for_websocket()
        .with_options(rpc_toolkit::WebSocketOptions {
            stream_buffer_capacity: std::num::NonZeroUsize::new(1).unwrap(),
            ..Default::default()
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/rpc", listener.local_addr().unwrap());
    let _server: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new().route("/rpc", axum::routing::get(server)),
        )
        .await
        .unwrap();
    })
    .into();
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    socket.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::json!({"jsonrpc":"2.0","rpcstream":rpc_toolkit::RPCSTREAM_VERSION,"id":1,"method":"observe","params":{}}).to_string().into())).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), started.notified())
        .await
        .unwrap();
    open.kill(&"key");
    tokio::time::timeout(Duration::from_secs(2), observed)
        .await
        .unwrap()
        .unwrap();
    initialize.notify_one();
    let initial: serde_json::Value =
        serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(initial["jsonrpc"], "2.0");
    let accepted: serde_json::Value =
        serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(accepted["result"], 0);
    assert_eq!(accepted["end"], false);
    let terminal: serde_json::Value =
        serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(terminal["end"], true);
    assert!(terminal.get("error").is_some());
}
