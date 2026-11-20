use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use futures::{SinkExt, StreamExt};
use imbl_value::json;
use patch_db::json_ptr::ROOT;
use rpc_toolkit::{Empty, HandlerExt, HttpServer, ParentHandler, Server, from_fn_async};
use tokio_tungstenite::tungstenite::Message;

use super::*;
use crate::util::future::NonDetachingJoinHandle;

#[derive(Serialize, Deserialize, HasModel)]
#[model = "Model<Self>"]
struct Store {
    count: u64,
}

#[derive(Clone)]
struct TestContext {
    db: TypedPatchDb<Store>,
    published: Arc<AtomicUsize>,
    sequence: tokio::sync::watch::Sender<u64>,
}

impl Context for TestContext {}
impl DbContext for TestContext {
    type Database = Store;
    fn db(&self) -> &TypedPatchDb<Store> {
        &self.db
    }
}
impl SyncDbContext for TestContext {
    fn publish_sync_sequence(&self, sequence: u64) {
        self.published.fetch_add(1, Ordering::SeqCst);
        self.sequence.send_replace(sequence);
    }
}

async fn context() -> TestContext {
    let db = TypedPatchDb::load_unchecked(PatchDb::in_memory());
    db.put(&ROOT, &Store { count: 0 }).await.unwrap();
    TestContext {
        db,
        published: Arc::default(),
        sequence: tokio::sync::watch::channel(0).0,
    }
}

async fn add(context: TestContext, _: Empty) -> Result<u64, Error> {
    context
        .db
        .mutate(|db| {
            db.as_count_mut().mutate(|count| {
                *count += 1;
                Ok(*count)
            })
        })
        .await
        .result
}

async fn fail(context: TestContext, params: Empty) -> Result<u64, Error> {
    add(context, params).await?;
    Err(Error::new(
        eyre!("failed after committing"),
        ErrorKind::Unknown,
    ))
}

fn server(context: TestContext) -> HttpServer<TestContext> {
    Server::new(
        move || {
            let context = context.clone();
            async move { Ok(context) }
        },
        ParentHandler::new()
            .subcommand(
                "add",
                from_fn_async(add)
                    .with_metadata("sync_db", true.into())
                    .no_cli()
                    .no_ts(),
            )
            .subcommand(
                "fail",
                from_fn_async(fail)
                    .with_metadata("sync_db", true.into())
                    .no_cli()
                    .no_ts(),
            )
            .subcommand(
                "read",
                from_fn_async(|_: TestContext, _: Empty| async { Ok::<_, Error>(()) })
                    .no_cli()
                    .no_ts(),
            ),
    )
    .middleware(SyncDb::new())
    .middleware(SyncDbHeaderCompat::default())
}

fn request(id: u64, method: &str) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "method":method, "params":{}})
}

async fn http(server: &HttpServer<TestContext>, body: Value) -> (Value, Option<u64>) {
    let response = server
        .handle(
            http::Request::builder()
                .method("POST")
                .uri("/rpc")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await;
    let sequence = response
        .headers()
        .get("X-Patch-Sequence")
        .map(|header| header.to_str().unwrap().parse().unwrap());
    let value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap(),
    )
    .unwrap();
    (value, sequence)
}

#[tokio::test]
async fn sequence_metadata_and_signal_follow_success_and_failure_without_leaking_to_reads() {
    let context = context().await;
    let server = server(context.clone());
    for method in ["add", "fail"] {
        let (response, header) = http(&server, request(1, method)).await;
        let response: RpcResponse = from_value(response).unwrap();
        assert_eq!(response.result.is_err(), method == "fail");
        let expected = context.db.sequence().await;
        assert_eq!(
            response
                .read_meta::<SyncDbMetadata>()
                .unwrap()
                .patch_sequence,
            Some(expected)
        );
        assert_eq!(header, Some(expected));
        assert_eq!(*context.sequence.borrow(), expected);
    }
    assert_eq!(context.published.load(Ordering::SeqCst), 2);
    let (response, header) = http(&server, request(2, "read")).await;
    assert!(response.get("meta").is_none());
    assert_eq!(header, None);
    assert_eq!(context.published.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn batches_keep_per_call_metadata_and_project_one_maximum_legacy_header() {
    let context = context().await;
    let server = server(context.clone());
    let (response, header) = http(
        &server,
        json!([request(1, "add"), request(2, "read"), request(3, "fail")]),
    )
    .await;
    let response: Vec<RpcResponse> = from_value(response).unwrap();
    assert!(
        response[0]
            .read_meta::<SyncDbMetadata>()
            .unwrap()
            .patch_sequence
            .is_some()
    );
    assert!(response[1].meta.is_empty());
    assert!(
        response[2]
            .read_meta::<SyncDbMetadata>()
            .unwrap()
            .patch_sequence
            .is_some()
    );
    assert_eq!(header, Some(context.db.sequence().await));
    assert_eq!(context.published.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn websocket_calls_publish_sequences_without_running_http_response_hooks_again() {
    let context = context().await;
    let mut sequence = context.sequence.subscribe();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/rpc", listener.local_addr().unwrap());
    let router = axum::Router::new().route(
        "/rpc",
        axum::routing::get(server(context.clone()).for_websocket()),
    );
    let _server: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    })
    .into();
    let (mut socket, upgrade) = tokio_tungstenite::connect_async(url).await.unwrap();
    assert!(upgrade.headers().get("X-Patch-Sequence").is_none());
    for (id, method) in [(1, "add"), (2, "read"), (3, "fail")] {
        socket
            .send(Message::Text(request(id, method).to_string().into()))
            .await
            .unwrap();
        let message = tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let response: RpcResponse = serde_json::from_str(message.to_text().unwrap()).unwrap();
        let actual = response
            .read_meta::<SyncDbMetadata>()
            .unwrap()
            .patch_sequence;
        if method == "read" {
            assert_eq!(actual, None);
            assert!(!sequence.has_changed().unwrap());
        } else {
            assert_eq!(actual, Some(context.db.sequence().await));
            assert!(sequence.has_changed().unwrap());
            assert_eq!(actual, Some(*sequence.borrow_and_update()));
        }
    }
    assert_eq!(context.published.load(Ordering::SeqCst), 2);
}
