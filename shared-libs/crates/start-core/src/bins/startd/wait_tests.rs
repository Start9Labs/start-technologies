use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use rpc_toolkit::{Empty, HandlerArgs, HandlerExt, ParentHandler, Server, from_fn_async};
use tokio::sync::{Notify, oneshot, watch};

use super::*;
use crate::middleware::auth::session::RpcSessionUpgrader;
use crate::middleware::auth::session::client::SessionClient;
use crate::registry::context::RegistryContext;
use crate::sign::Signer;
use crate::util::GeneralBoxedGuard;
use crate::util::cleanup::{CleanupTasks, drain_cleanup, spawn_cleanup};
use crate::util::sync::SyncMutex;

struct Fixture {
    _directory: tempfile::TempDir,
    server: WebServer<tokio::net::TcpListener>,
    url: url::Url,
    signer: Signer,
    requests: oneshot::Receiver<Option<Shutdown>>,
    started: Arc<Notify>,
    dropped: Arc<AtomicBool>,
    late: Arc<AtomicBool>,
    upgrades: Arc<AtomicUsize>,
    cleanup: Arc<CleanupTasks>,
    cleanup_started: oneshot::Receiver<()>,
    cleanup_release: oneshot::Sender<()>,
    closed: watch::Sender<bool>,
}

#[derive(Clone)]
struct ResponseContextProbe {
    closed: watch::Receiver<bool>,
    callbacks: Arc<AtomicUsize>,
}
impl ResponseContextProbe {
    fn record(&self) {
        assert!(
            !*self.closed.borrow(),
            "response middleware used a closed context"
        );
        self.callbacks.fetch_add(1, Ordering::SeqCst);
    }
}
impl rpc_toolkit::Middleware<RegistryContext> for ResponseContextProbe {
    type Metadata = Empty;
    async fn process_rpc_response(
        &mut self,
        _: &RegistryContext,
        _: &mut rpc_toolkit::RpcResponse,
    ) {
        self.record();
    }
    async fn process_http_response(
        &mut self,
        _: &RegistryContext,
        _: &mut axum::response::Response,
    ) {
        self.record();
    }
}

async fn fixture() -> Fixture {
    let (directory, context, signer, _, _) = crate::registry::tests::fixture().await;
    let (send, requests) = oneshot::channel();
    let send = Arc::new(SyncMutex::new((Some(send), false)));
    let (closed, completion) = watch::channel(false);
    let started = Arc::new(Notify::new());
    let dropped = Arc::new(AtomicBool::new(false));
    let late = Arc::new(AtomicBool::new(false));
    let cleanup = Arc::new(CleanupTasks::new(Vec::new()));
    let (cleanup_begin, cleanup_started) = oneshot::channel();
    let (cleanup_release, cleanup_end) = oneshot::channel();
    let cleanup_signals = Arc::new(SyncMutex::new(Some((cleanup_begin, cleanup_end))));
    let wait_handler = from_fn_async(move |_: HandlerArgs<RegistryContext, Empty, Empty>| {
        let send = send.clone();
        let mut completion = completion.clone();
        async move {
            crate::shutdown::send_shutdown(
                &send,
                Some(Shutdown {
                    disk_guid: None,
                    restart: true,
                }),
            )?;
            crate::shutdown::wait_for_services(
                async move {
                    completion.wait_for(|closed| *closed).await.unwrap();
                },
                true,
            )
            .await;
            Ok::<_, Error>(())
        }
    })
    .no_cli()
    .no_ts();
    let producer = {
        let started = started.clone();
        let dropped = dropped.clone();
        let cleanup = cleanup.clone();
        from_fn_async(move |_: HandlerArgs<RegistryContext, Empty, Empty>| {
            let started = started.clone();
            let dropped = dropped.clone();
            let cleanup = cleanup.clone();
            let signals = cleanup_signals.replace(None).unwrap();
            async move {
                let _guard = GeneralBoxedGuard::new(move || {
                    dropped.store(true, Ordering::SeqCst);
                    let _ = spawn_cleanup(&cleanup, async move {
                        signals.0.send(()).unwrap();
                        signals.1.await.unwrap();
                    });
                });
                started.notify_one();
                std::future::pending::<()>().await;
                Ok::<_, Error>(())
            }
        })
        .no_cli()
        .no_ts()
    };
    let late_handler = {
        let late = late.clone();
        from_fn_async(move |_: HandlerArgs<RegistryContext, Empty, Empty>| {
            late.store(true, Ordering::SeqCst);
            async { Ok::<_, Error>(()) }
        })
        .no_cli()
        .no_ts()
    };
    let callbacks = Arc::new(AtomicUsize::new(0));
    let response_probe = ResponseContextProbe {
        closed: closed.subscribe(),
        callbacks: callbacks.clone(),
    };
    let rpc = Server::new(
        move || futures::future::ready(Ok(context.clone())),
        ParentHandler::new()
            .subcommand("wait", wait_handler)
            .subcommand("hold", producer)
            .subcommand("late", late_handler)
            .subcommand(
                "ping",
                from_fn_async(|_: HandlerArgs<RegistryContext>| async { Ok::<_, Error>(()) })
                    .no_cli()
                    .no_ts(),
            ),
    )
    .middleware(response_probe);
    let upgrades = Arc::new(AtomicUsize::new(0));
    let observed = upgrades.clone();
    let app = Router::new()
        .route("/rpc/v1", axum::routing::post(rpc.clone()))
        .route(RpcSessionUpgrader::PATH, RpcSessionUpgrader::route(rpc))
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                if request.uri().path() == RpcSessionUpgrader::PATH {
                    observed.fetch_add(1, Ordering::SeqCst);
                }
                async move { next.run(request).await }
            },
        ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let fixture = Fixture {
        _directory: directory,
        server: WebServer::new(Acceptor::new(listener), app),
        url,
        signer,
        requests,
        started,
        dropped,
        late,
        upgrades,
        cleanup,
        cleanup_started,
        cleanup_release,
        closed,
    };
    let response = within(
        reqwest::Client::new()
            .post(fixture.url.join("rpc/v1").unwrap())
            .json(&serde_json::json!({"jsonrpc":"2.0","id":0,"method":"ping","params":{}}))
            .send(),
    )
    .await
    .unwrap();
    let response = within(response.json::<serde_json::Value>()).await.unwrap();
    assert_eq!(response.get("result"), Some(&serde_json::Value::Null));
    assert_eq!(callbacks.load(Ordering::SeqCst), 2);
    fixture
}

fn teardown(
    mut server: WebServer<tokio::net::TcpListener>,
    cleanup: Arc<CleanupTasks>,
    dropped: Arc<AtomicBool>,
    closed: watch::Sender<bool>,
) -> (
    tokio::task::JoinHandle<()>,
    oneshot::Receiver<()>,
    oneshot::Sender<()>,
) {
    let (started, stopping) = oneshot::channel();
    let (release, completion) = oneshot::channel();
    let job = tokio::spawn(async move {
        server.reject_requests(crate::shutdown::in_progress_error(true).into());
        teardown_runtime(&mut server, async move {
            assert!(dropped.load(Ordering::SeqCst));
            drain_cleanup(&cleanup).await;
            started.send(()).unwrap();
            completion.await.unwrap();
            closed.send_replace(true);
            Ok::<(), Error>(())
        })
        .await
        .unwrap();
        server.shutdown().await;
    });
    (job, stopping, release)
}

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .unwrap()
}

#[tokio::test]
async fn http_wait_survives_producer_cancellation_and_service_teardown() {
    let fixture = fixture().await;
    let client = reqwest::Client::new();
    let rpc = fixture.url.join("rpc/v1").unwrap();
    let producer = {
        let client = client.clone();
        let rpc = rpc.clone();
        tokio::spawn(async move {
            client
                .post(rpc)
                .json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"hold","params":{}}))
                .send()
                .await
                .unwrap()
        })
    };
    within(fixture.started.notified()).await;
    let mut waiter = {
        let client = client.clone();
        let rpc = rpc.clone();
        tokio::spawn(async move {
            client.post(rpc).json(&serde_json::json!({"jsonrpc":"2.0","id":2,"method":"wait","params":{"wait":true}})).send().await.unwrap()
        })
    };
    assert!(within(fixture.requests).await.unwrap().unwrap().restart);
    let (job, stopping, release) = teardown(
        fixture.server,
        fixture.cleanup,
        fixture.dropped,
        fixture.closed,
    );
    within(fixture.cleanup_started).await.unwrap();
    assert!(!waiter.is_finished());
    fixture.cleanup_release.send(()).unwrap();
    within(stopping).await.unwrap();
    assert!(!waiter.is_finished());
    let rejected = client
        .post(rpc)
        .json(&serde_json::json!({"jsonrpc":"2.0","id":3,"method":"late","params":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), http::StatusCode::SERVICE_UNAVAILABLE);
    let rejected: serde_json::Value = rejected.json().await.unwrap();
    assert_eq!(
        rejected["error"]["data"]["details"],
        crate::shutdown::in_progress_error(true).source.to_string()
    );
    assert!(!fixture.late.load(Ordering::SeqCst));
    let cancelled: serde_json::Value = within(producer).await.unwrap().json().await.unwrap();
    assert_eq!(
        cancelled["error"]["data"]["details"],
        crate::shutdown::in_progress_error(true).source.to_string()
    );
    release.send(()).unwrap();
    let response: serde_json::Value = within(&mut waiter).await.unwrap().json().await.unwrap();
    assert_eq!(
        response,
        serde_json::json!({"jsonrpc":"2.0","id":2,"result":null})
    );
    within(job).await.unwrap();
}

#[tokio::test]
async fn an_http_batch_waiter_does_not_exempt_its_producing_sibling() {
    let fixture = fixture().await;
    let url = fixture.url.join("rpc/v1").unwrap();
    let reply = tokio::spawn(async move {
        reqwest::Client::new()
            .post(url)
            .json(&serde_json::json!([
                {"jsonrpc":"2.0","id":2,"method":"wait","params":{"wait":true}},
                {"jsonrpc":"2.0","id":1,"method":"hold","params":{}}
            ]))
            .send()
            .await
            .unwrap()
    });
    within(fixture.started.notified()).await;
    within(fixture.requests).await.unwrap();
    let (job, stopping, release) = teardown(
        fixture.server,
        fixture.cleanup,
        fixture.dropped,
        fixture.closed,
    );
    within(fixture.cleanup_started).await.unwrap();
    assert!(!reply.is_finished());
    fixture.cleanup_release.send(()).unwrap();
    within(stopping).await.unwrap();
    assert!(!reply.is_finished());
    release.send(()).unwrap();
    let response: serde_json::Value = within(reply).await.unwrap().json().await.unwrap();
    assert_eq!(
        response[0],
        serde_json::json!({"jsonrpc":"2.0","id":2,"result":null})
    );
    assert_eq!(
        response[1]["error"]["data"]["details"],
        crate::shutdown::in_progress_error(true).source.to_string()
    );
    within(job).await.unwrap();
}

fn encrypted_call(
    client: Arc<SessionClient>,
    signer: Signer,
    url: url::Url,
    method: &'static str,
) -> tokio::task::JoinHandle<Result<Value, rpc_toolkit::yajrc::RpcError>> {
    tokio::spawn(async move {
        client
            .call(
                &signer,
                url,
                "127.0.0.1",
                http::HeaderMap::new(),
                method,
                imbl_value::json!({"wait":true}),
            )
            .await
    })
}

#[tokio::test]
async fn cli_wait_survives_while_active_and_new_siblings_are_stopped() {
    let fixture = fixture().await;
    let client = Arc::new(SessionClient::new(
        reqwest::Client::builder().http1_only().build().unwrap(),
    ));
    let ws = RpcSessionUpgrader::url(fixture.url).unwrap();
    let producer = encrypted_call(client.clone(), fixture.signer.clone(), ws.clone(), "hold");
    within(fixture.started.notified()).await;
    let waiter = encrypted_call(client.clone(), fixture.signer.clone(), ws.clone(), "wait");
    assert!(within(fixture.requests).await.unwrap().unwrap().restart);
    let (job, stopping, release) = teardown(
        fixture.server,
        fixture.cleanup,
        fixture.dropped,
        fixture.closed,
    );
    within(fixture.cleanup_started).await.unwrap();
    assert!(!waiter.is_finished());
    fixture.cleanup_release.send(()).unwrap();
    within(stopping).await.unwrap();
    assert!(!waiter.is_finished());
    let cancelled = within(producer).await.unwrap().unwrap_err();
    assert_eq!(
        Error::from(cancelled).source.to_string(),
        crate::shutdown::in_progress_error(true).source.to_string()
    );
    let duplicate = encrypted_call(client.clone(), fixture.signer.clone(), ws.clone(), "wait");
    let duplicate = within(duplicate).await.unwrap().unwrap_err();
    assert_eq!(
        Error::from(duplicate).source.to_string(),
        crate::shutdown::in_progress_error(true).source.to_string()
    );
    let rejected = encrypted_call(client, fixture.signer, ws, "late");
    let rejected = within(rejected).await.unwrap().unwrap_err();
    assert_eq!(
        Error::from(rejected).source.to_string(),
        crate::shutdown::in_progress_error(true).source.to_string()
    );
    assert!(!fixture.late.load(Ordering::SeqCst));
    assert_eq!(fixture.upgrades.load(Ordering::SeqCst), 1);
    release.send(()).unwrap();
    assert_eq!(within(waiter).await.unwrap().unwrap(), Value::Null);
    within(job).await.unwrap();
}
