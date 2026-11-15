use std::time::Duration;

use futures::channel::mpsc;
use rpc_toolkit::yajrc::{self, GenericRpcMethod};
use serde_json::json;

use super::*;

mod streaming;

fn method(name: &str) -> GenericRpcMethod<&str, Value, Value> {
    GenericRpcMethod::new(name)
}

fn channel() -> (
    RpcClient,
    mpsc::Receiver<String>,
    mpsc::Sender<Result<String, Error>>,
) {
    let (write, requests) = mpsc::channel(8);
    let (responses, read) = mpsc::channel(8);
    let client = RpcClient::messages(
        write.sink_map_err(|error| Error::new(error, ErrorKind::Network)),
        read,
        Arc::new(AtomicUsize::new(0)),
    );
    (client, requests, responses)
}

#[tokio::test]
async fn message_dispatch_correlates_concurrent_out_of_order_replies() {
    let (client, mut requests, mut responses) = channel();
    let server = async move {
        let a: Value = serde_json::from_str(&requests.next().await.unwrap()).unwrap();
        let b: Value = serde_json::from_str(&requests.next().await.unwrap()).unwrap();
        assert_ne!(a["id"], b["id"]);
        for request in [b, a] {
            responses
                .send(Ok(
                    json!({"jsonrpc":"2.0", "id":request["id"], "result":request["method"]})
                        .to_string(),
                ))
                .await
                .unwrap();
        }
    };
    let (a, b, ()) = tokio::join!(
        client.request(method("one"), json!({})),
        client.request(method("two"), json!({})),
        server,
    );
    assert_eq!(a.unwrap(), json!("one"));
    assert_eq!(b.unwrap(), json!("two"));
}

#[tokio::test]
async fn cancellation_releases_its_pending_slot_without_disconnecting_siblings() {
    let (client, mut requests, mut responses) = channel();
    let mut canceled = Box::pin(client.request(method("canceled"), json!({})));
    assert!(futures::poll!(&mut canceled).is_pending());
    let old: Value = serde_json::from_str(&requests.next().await.unwrap()).unwrap();
    drop(canceled);
    assert_eq!(
        client
            .responses
            .upgrade()
            .unwrap()
            .peek(|pending| pending.as_ref().unwrap().len()),
        0
    );
    let request = client.request(method("kept"), json!({}));
    let server = async move {
        let current: Value = serde_json::from_str(&requests.next().await.unwrap()).unwrap();
        for (id, value) in [
            (old["id"].clone(), "stale"),
            (current["id"].clone(), "current"),
        ] {
            responses
                .send(Ok(
                    json!({"jsonrpc":"2.0","id":id,"result":value}).to_string()
                ))
                .await
                .unwrap();
        }
    };
    let (response, ()) = tokio::join!(request, server);
    assert_eq!(response.unwrap(), json!("current"));
}

#[tokio::test]
async fn canceled_websocket_calls_send_one_close_and_ignore_late_replies_without_harming_siblings()
{
    let (client, mut requests, mut responses) = channel();
    let captured = RpcClient::capture(&client.id, method("canceled"), json!({})).unwrap();
    let mut canceled = Box::pin(client.send_cancellable_request(captured));
    assert!(futures::poll!(&mut canceled).is_pending());
    let old: Value = serde_json::from_str(&requests.next().await.unwrap()).unwrap();
    drop(canceled);
    let close: Value = serde_json::from_str(
        &tokio::time::timeout(Duration::from_secs(2), requests.next())
            .await
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        close,
        json!({"rpcstream":rpc_toolkit::RPCSTREAM_VERSION,"close":old["id"]})
    );
    let captured = RpcClient::capture(&client.id, method("kept"), json!({})).unwrap();
    let mut sibling = Box::pin(client.send_cancellable_request(captured));
    assert!(futures::poll!(&mut sibling).is_pending());
    let current: Value = serde_json::from_str(&requests.next().await.unwrap()).unwrap();
    assert_eq!(current["method"], "kept");
    for (id, result) in [(old["id"].clone(), "late"), (current["id"].clone(), "kept")] {
        responses
            .send(Ok(
                json!({"jsonrpc":"2.0","id":id,"result":result}).to_string()
            ))
            .await
            .unwrap();
    }
    assert_eq!(sibling.await.unwrap().unwrap(), json!("kept"));
    assert_eq!(
        client
            .responses
            .upgrade()
            .unwrap()
            .peek(|pending| pending.as_ref().unwrap().len()),
        0
    );
    assert!(futures::poll!(requests.next()).is_pending());
}

#[tokio::test]
async fn a_failed_writer_invalidates_queued_requests_without_replaying_or_reentering_it() {
    let writes = Arc::new(AtomicUsize::new(0));
    let observed = writes.clone();
    let writer = futures::sink::unfold((), move |(), _: String| {
        observed.fetch_add(1, Ordering::SeqCst);
        async { Err::<(), Error>(std::io::Error::from(std::io::ErrorKind::BrokenPipe).into()) }
    });
    let client = RpcClient::messages(
        writer,
        futures::stream::pending(),
        Arc::new(AtomicUsize::new(0)),
    );
    let (a, b) = tokio::join!(
        client.request(method("side-effect"), json!({})),
        client.request(method("queued"), json!({})),
    );
    assert!(a.is_err());
    assert!(b.is_err());
    assert!(client.is_closed());
    assert_eq!(writes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn dropping_the_client_releases_its_idle_reader() {
    let (dropped, observed) = oneshot::channel();
    let guard = GeneralGuard::new(move || dropped.send(()));
    let reader = futures::stream::pending::<Result<String, Error>>().map(move |item| {
        let _ = &guard;
        item
    });
    let writer = futures::sink::drain().sink_map_err(|never| match never {});
    let client = RpcClient::messages(writer, reader, Arc::new(AtomicUsize::new(0)));
    drop(client);
    tokio::time::timeout(Duration::from_secs(2), observed)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn unix_application_internal_error_keeps_the_socket_and_never_repeats_the_method() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rpc.sock");
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let (read, mut write) = socket.into_split();
        let mut lines = BufReader::new(read).lines();
        for error in [true, false] {
            let request: Value =
                serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            let response = if error {
                json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":yajrc::INTERNAL_ERROR.code,"message":"method failed"}})
            } else {
                json!({"jsonrpc":"2.0","id":request["id"],"result":"next call"})
            };
            write
                .write_all((response.to_string() + "\n").as_bytes())
                .await
                .unwrap();
        }
    });
    let client = UnixRpcClient::new(path);
    let first = tokio::time::timeout(
        Duration::from_secs(2),
        client.request(method("side-effect"), json!({})),
    )
    .await
    .unwrap();
    assert_eq!(first.unwrap_err().code, yajrc::INTERNAL_ERROR.code);
    assert_eq!(
        client.request(method("next"), json!({})).await.unwrap(),
        json!("next call")
    );
    server.await.unwrap();
}
