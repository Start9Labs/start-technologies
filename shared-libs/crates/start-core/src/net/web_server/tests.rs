use futures::SinkExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use super::*;
use crate::util::GeneralBoxedGuard;
use crate::util::future::DropSignaling;

#[tokio::test]
async fn shutdown_joins_cancelled_server_tasks_and_rejects_late_work() {
    let tasks = ServerTasks::new();
    let spawner = tasks.spawner();
    let (started, running) = oneshot::channel();
    let job = DropSignaling::new(async move {
        let _ = started.send(());
        std::future::pending::<()>().await;
    });
    let mut stopped = job.subscribe();
    spawner.spawn(job);
    running.await.unwrap();

    tasks.shutdown().await;

    assert!(stopped.wait().now_or_never().is_some());
    let rejected = DropSignaling::new(async { panic!("late task started") });
    let mut rejected_dropped = rejected.subscribe();
    spawner.spawn(rejected);
    assert!(rejected_dropped.wait().now_or_never().is_some());
}

#[tokio::test]
async fn server_task_ownership_does_not_cycle_through_hyper_executors() {
    let tasks = ServerTasks::new();
    let spawner = tasks.spawner();
    let child_spawner = spawner.clone();
    let job = DropSignaling::new(async move {
        std::future::pending::<()>().await;
        drop(child_spawner);
    });
    let mut stopped = job.subscribe();
    spawner.spawn(job);

    drop(tasks);

    tokio::time::timeout(Duration::from_secs(1), stopped.wait())
        .await
        .expect("connection retained its own task owner");
}

#[tokio::test]
async fn router_reset_joins_in_flight_request_cancellation() {
    let (started, running) = oneshot::channel();
    let (stopped, ended) = oneshot::channel();
    let started = Arc::new(SyncMutex::new(Some(started)));
    let stopped = Arc::new(SyncMutex::new(Some(stopped)));
    let router = Router::new().route(
        "/hold",
        axum::routing::get(move || {
            let started = started.clone();
            let stopped = stopped.clone();
            async move {
                let _guard = GeneralBoxedGuard::new(move || {
                    stopped.mutate(|stopped| stopped.take().unwrap().send(()).unwrap());
                });
                started.mutate(|started| started.take().unwrap().send(()).unwrap());
                std::future::pending::<http::StatusCode>().await
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut server = WebServer::new(Acceptor::new(vec![listener]), router);
    let mut client = TcpStream::connect(address).await.unwrap();
    client
        .write_all(b"GET /hold HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), running)
        .await
        .unwrap()
        .unwrap();

    server.reset_router(Router::new()).await;

    assert!(ended.now_or_never().unwrap().is_ok());
    server.shutdown().await;
}

#[tokio::test]
async fn router_reset_joins_upgraded_rpc_calls() {
    use rpc_toolkit::{Context, Empty, HandlerArgs, HandlerExt, ParentHandler};

    #[derive(Clone)]
    struct CallContext {
        started: Arc<SyncMutex<Option<oneshot::Sender<()>>>>,
        stopped: Arc<SyncMutex<Option<oneshot::Sender<()>>>>,
    }
    impl Context for CallContext {}

    async fn hold(
        args: HandlerArgs<CallContext, Empty, Empty>,
    ) -> Result<(), rpc_toolkit::yajrc::RpcError> {
        let _guard = GeneralBoxedGuard::new(move || {
            args.context
                .stopped
                .mutate(|stopped| stopped.take().unwrap().send(()).unwrap());
        });
        args.context
            .started
            .mutate(|started| started.take().unwrap().send(()).unwrap());
        std::future::pending().await
    }

    let (started, running) = oneshot::channel();
    let (stopped, ended) = oneshot::channel();
    let context = CallContext {
        started: Arc::new(SyncMutex::new(Some(started))),
        stopped: Arc::new(SyncMutex::new(Some(stopped))),
    };
    let rpc = rpc_toolkit::Server::new(
        move || futures::future::ready(Ok(context.clone())),
        ParentHandler::new().subcommand("hold", rpc_toolkit::from_fn_async(hold).no_cli().no_ts()),
    );
    let router = Router::new().route("/rpc", axum::routing::get(rpc.for_websocket()));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut server = WebServer::new(Acceptor::new(vec![listener]), router);
    let (mut client, _) = tokio_tungstenite::connect_async(format!("ws://{address}/rpc"))
        .await
        .unwrap();
    client
        .send(tokio_tungstenite::tungstenite::Message::text(
            r#"{"jsonrpc":"2.0","id":1,"method":"hold","params":{}}"#,
        ))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), running)
        .await
        .unwrap()
        .unwrap();

    server.reset_router(Router::new()).await;

    assert!(ended.now_or_never().unwrap().is_ok());
    server.shutdown().await;
}

#[tokio::test]
async fn router_reset_joins_upgrade_ownership_before_the_handshake_completes() {
    let (started, running) = oneshot::channel();
    let (stopped, ended) = oneshot::channel();
    let started = Arc::new(SyncMutex::new(Some(started)));
    let stopped = Arc::new(SyncMutex::new(Some(stopped)));
    let router = Router::new().route(
        "/upgrade",
        axum::routing::get(
            move |owner: axum::Extension<rpc_toolkit::WebSocketTaskOwner>,
                  upgrade: axum::extract::ws::WebSocketUpgrade| {
                let started = started.clone();
                let stopped = stopped.clone();
                async move {
                    let guard = GeneralBoxedGuard::new(move || {
                        stopped.mutate(|stopped| stopped.take().unwrap().send(()).unwrap());
                    });
                    let response = rpc_toolkit::on_websocket_upgrade(
                        upgrade,
                        Some(owner.0),
                        move |_| async move {
                            let _guard = guard;
                            std::future::pending::<()>().await;
                        },
                    );
                    started.mutate(|started| started.take().unwrap().send(()).unwrap());
                    std::future::pending::<()>().await;
                    response
                }
            },
        ),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut server = WebServer::new(Acceptor::new(vec![listener]), router);
    let mut client = TcpStream::connect(address).await.unwrap();
    client.write_all(b"GET /upgrade HTTP/1.1\r\nHost: localhost\r\nConnection: upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n").await.unwrap();
    tokio::time::timeout(Duration::from_secs(1), running)
        .await
        .unwrap()
        .unwrap();

    server.reset_router(Router::new()).await;

    assert!(ended.now_or_never().unwrap().is_ok());
    server.shutdown().await;
}
