use super::*;

async fn start(
    client: &RpcClient,
    requests: &mut mpsc::Receiver<String>,
    responses: &mut mpsc::Sender<Result<String, Error>>,
) -> (Value, RpcResponseStream<Value, u64, String>) {
    let mut pending =
        Box::pin(client.request_stream::<_, u64, String>(method("progress"), json!({})));
    assert!(futures::poll!(&mut pending).is_pending());
    let request: Value = serde_json::from_str(&requests.next().await.unwrap()).unwrap();
    assert_eq!(request["rpcstream"], rpc_toolkit::RPCSTREAM_VERSION);
    responses.send(Ok(json!({"jsonrpc":"2.0","id":request["id"],"result":"initial","meta":{"patchSequence":7}}).to_string())).await.unwrap();
    (request["id"].clone(), pending.await.unwrap())
}

async fn event(
    responses: &mut mpsc::Sender<Result<String, Error>>,
    id: &Value,
    value: Value,
    end: bool,
) {
    responses
        .send(Ok(
            json!({"rpcstream":rpc_toolkit::RPCSTREAM_VERSION,"id":id,"result":value,"end":end})
                .to_string(),
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn buffered_items_preserve_initial_metadata_and_reserved_terminal_result() {
    let (client, mut requests, mut responses) = channel();
    let (id, mut stream) = start(&client, &mut requests, &mut responses).await;
    let capacity = rpc_toolkit::WebSocketOptions::default()
        .stream_buffer_capacity
        .get();
    for n in 0..capacity {
        event(&mut responses, &id, json!(n), false).await;
    }
    event(&mut responses, &id, json!("finished"), true).await;
    assert_eq!(stream.initial, json!("initial"));
    assert_eq!(stream.meta["patchSequence"].as_u64(), Some(7));
    for n in 0..capacity {
        assert_eq!(stream.items.next().await.unwrap(), Some(n as u64));
    }
    assert_eq!(stream.items.next().await.unwrap(), None);
    assert_eq!(stream.items.finish().await.unwrap(), "finished");
    assert!(!client.is_closed());
}

#[tokio::test]
async fn dropping_pending_initialization_or_unread_items_closes_only_their_id() {
    let (client, mut requests, mut responses) = channel();
    let mut pending =
        Box::pin(client.request_stream::<_, u64, String>(method("pending"), json!({})));
    assert!(futures::poll!(&mut pending).is_pending());
    let old: Value = serde_json::from_str(&requests.next().await.unwrap()).unwrap();
    drop(pending);
    let close: Value = serde_json::from_str(&requests.next().await.unwrap()).unwrap();
    assert_eq!(close["close"], old["id"]);
    assert!(!client.is_closed());
    let (id, stream) = start(&client, &mut requests, &mut responses).await;
    drop(stream);
    let close: Value = serde_json::from_str(&requests.next().await.unwrap()).unwrap();
    assert_eq!(close["close"], id);
    assert!(!client.is_closed());
}

#[tokio::test]
async fn terminal_application_error_drains_accepted_items_and_does_not_poison_the_connection() {
    let (client, mut requests, mut responses) = channel();
    let (id, mut stream) = start(&client, &mut requests, &mut responses).await;
    event(&mut responses, &id, json!(42), false).await;
    responses.send(Ok(json!({"rpcstream":rpc_toolkit::RPCSTREAM_VERSION,"id":id,"error":{"code":34,"message":"revoked"},"end":true}).to_string())).await.unwrap();
    assert_eq!(stream.items.next().await.unwrap(), Some(42));
    assert_eq!(stream.items.next().await.unwrap_err().code, 34);
    assert_eq!(stream.items.finish().await.unwrap_err().code, 34);
    assert!(!client.is_closed());
}

#[tokio::test]
async fn unread_burst_beyond_the_server_item_queue_is_retained_without_blocking_a_sibling() {
    let (client, mut requests, mut responses) = channel();
    let (id, mut stream) = start(&client, &mut requests, &mut responses).await;
    let burst = 4 * rpc_toolkit::WebSocketOptions::default()
        .stream_buffer_capacity
        .get();
    for n in 0..burst {
        event(&mut responses, &id, json!(n), false).await;
    }
    let request = client.request(method("sibling"), json!({}));
    let server = async {
        let request: Value = serde_json::from_str(&requests.next().await.unwrap()).unwrap();
        assert!(request.get("close").is_none(), "{request}");
        responses
            .send(Ok(
                json!({"jsonrpc":"2.0","id":request["id"],"result":"kept"}).to_string(),
            ))
            .await
            .unwrap();
    };
    let (value, ()) = tokio::join!(request, server);
    assert_eq!(value.unwrap(), json!("kept"));
    for n in 0..burst {
        assert_eq!(stream.items.next().await.unwrap(), Some(n as u64));
    }
}

#[tokio::test]
async fn loss_cancels_established_queues_pending_initialization_and_a_sibling_without_replay() {
    let (client, mut requests, mut responses) = channel();
    let (a, mut first) = start(&client, &mut requests, &mut responses).await;
    let (_, mut second) = start(&client, &mut requests, &mut responses).await;
    event(&mut responses, &a, json!(99), false).await;
    let mut pending =
        Box::pin(client.request_stream::<_, u64, String>(method("pending"), json!({})));
    let mut sibling = Box::pin(client.request(method("side-effect"), json!({})));
    assert!(futures::poll!(&mut pending).is_pending());
    assert!(futures::poll!(&mut sibling).is_pending());
    requests.next().await.unwrap();
    requests.next().await.unwrap();
    drop(responses);
    tokio::time::timeout(Duration::from_secs(2), async {
        while !client.is_closed() {
            tokio::task::yield_now().await
        }
    })
    .await
    .unwrap();
    assert!(first.items.next().await.is_err());
    assert!(second.items.next().await.is_err());
    assert!(pending.await.is_err());
    assert!(sibling.await.is_err());
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn item_type_failure_cancels_instead_of_retaining_a_live_producer() {
    let (client, mut requests, mut responses) = channel();
    let (id, mut stream) = start(&client, &mut requests, &mut responses).await;
    event(&mut responses, &id, json!("not an integer"), false).await;
    assert!(stream.items.next().await.is_err());
    let close: Value = serde_json::from_str(&requests.next().await.unwrap()).unwrap();
    assert_eq!(close["close"], id);
}

#[tokio::test]
async fn serialized_byte_budget_is_independent_of_item_count() {
    let (client, mut requests, mut responses) = channel();
    let mut pending =
        Box::pin(client.request_stream::<_, String, String>(method("bytes"), json!({})));
    assert!(futures::poll!(&mut pending).is_pending());
    let request: Value = serde_json::from_str(&requests.next().await.unwrap()).unwrap();
    responses
        .send(Ok(
            json!({"jsonrpc":"2.0","id":request["id"],"result":null}).to_string(),
        ))
        .await
        .unwrap();
    let mut stream = pending.await.unwrap();
    let text = "😀".repeat(crate::util::net::secure::DEFAULT_RPC_RESPONSE_LIMIT / 8);
    for _ in 0..2 {
        event(&mut responses, &request["id"], json!(text), false).await;
    }
    let close: Value = serde_json::from_str(&requests.next().await.unwrap()).unwrap();
    assert_eq!(close["close"], request["id"]);
    assert_eq!(
        stream.items.next().await.unwrap_err().code,
        rpc_toolkit::RPCSTREAM_CANCELLED_ERROR.code
    );
}

#[tokio::test]
async fn pending_stream_admission_is_bounded_even_while_the_writer_is_blocked() {
    let (client, _requests, _responses) = channel();
    let mut pending = Vec::new();
    for _ in 0..rpc_toolkit::WebSocketOptions::default().max_streams.get() {
        let mut call =
            Box::pin(client.request_stream::<_, u64, String>(method("pending"), json!({})));
        assert!(futures::poll!(&mut call).is_pending());
        pending.push(call);
    }
    assert_eq!(
        client
            .request_stream::<_, u64, String>(method("over capacity"), json!({}))
            .await
            .err()
            .unwrap()
            .code,
        rpc_toolkit::RPCSTREAM_CANCELLED_ERROR.code
    );
    assert!(!client.is_closed());
}

#[tokio::test]
async fn blocked_cancellation_writes_do_not_suspend_the_owned_reader() {
    let (write, mut requests) = mpsc::channel::<String>(8);
    let (mut responses, read) = mpsc::channel(8);
    let blocked = Arc::new(tokio::sync::Notify::new());
    let observed = blocked.clone();
    let writer = futures::sink::unfold(write, move |mut writer, body: String| {
        let observed = observed.clone();
        async move {
            if body.contains("\"close\"") {
                observed.notify_one();
                futures::future::pending::<()>().await;
            }
            writer
                .send(body)
                .await
                .map_err(|error| Error::new(error, ErrorKind::Network))?;
            Ok(writer)
        }
    });
    let client = RpcClient::messages(writer, read, Arc::new(AtomicUsize::new(0)));
    let mut stream = Box::pin(client.request_stream::<_, u64, String>(method("cancel"), json!({})));
    let mut sibling = Box::pin(client.request(method("sibling"), json!({})));
    assert!(futures::poll!(&mut stream).is_pending());
    assert!(futures::poll!(&mut sibling).is_pending());
    requests.next().await.unwrap();
    let request: Value = serde_json::from_str(&requests.next().await.unwrap()).unwrap();
    drop(stream);
    blocked.notified().await;
    responses
        .send(Ok(
            json!({"jsonrpc":"2.0","id":request["id"],"result":"reader alive"}).to_string(),
        ))
        .await
        .unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), sibling)
            .await
            .unwrap()
            .unwrap(),
        json!("reader alive")
    );
}

#[tokio::test]
async fn full_unsigned_ids_items_and_terminal_values_remain_exact() {
    let (client, mut requests, mut responses) = channel();
    client.id.store(usize::MAX, Ordering::SeqCst);
    let mut pending = Box::pin(client.request_stream::<_, u64, u64>(method("integers"), json!({})));
    assert!(futures::poll!(&mut pending).is_pending());
    let request: Value = serde_json::from_str(&requests.next().await.unwrap()).unwrap();
    assert_eq!(request["id"], json!(usize::MAX));
    responses
        .send(Ok(
            json!({"jsonrpc":"2.0","id":request["id"],"result":u64::MAX}).to_string(),
        ))
        .await
        .unwrap();
    event(&mut responses, &request["id"], json!(u64::MAX), false).await;
    event(&mut responses, &request["id"], json!(u64::MAX), true).await;
    let mut stream = pending.await.unwrap();
    assert_eq!(stream.initial, json!(u64::MAX));
    assert_eq!(stream.items.next().await.unwrap(), Some(u64::MAX));
    assert_eq!(stream.items.finish().await.unwrap(), u64::MAX);
}

#[tokio::test]
async fn initial_application_error_is_not_replaced_by_a_transport_error() {
    let (client, mut requests, mut responses) = channel();
    let mut pending =
        Box::pin(client.request_stream::<_, u64, String>(method("denied"), json!({})));
    assert!(futures::poll!(&mut pending).is_pending());
    let request: Value = serde_json::from_str(&requests.next().await.unwrap()).unwrap();
    responses
        .send(Ok(
            json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":34,"message":"denied"}})
                .to_string(),
        ))
        .await
        .unwrap();
    assert_eq!(pending.await.err().unwrap().code, 34);
    assert!(!client.is_closed());
}
