use super::*;

async fn observed(signal: &Notify) {
    tokio::time::timeout(Duration::from_secs(3), signal.notified())
        .await
        .unwrap();
}

#[tokio::test]
async fn ordinary_call_cancellation_drops_the_handler_without_a_response_or_late_poll() {
    for id in [wire!(7), wire!("7"), wire!(u64::MAX), Value::Null] {
        let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
        send(
            &mut socket,
            wire!({"jsonrpc":"2.0","id":id,"method":"blocked","params":{"value":1}}),
        )
        .await;
        observed(&context.started).await;
        send(&mut socket, wire!({"rpcstream":"1.0","close":id})).await;
        observed(&context.drop_signal).await;
        assert_eq!(context.dropped.load(Ordering::SeqCst), 1);
        context.release.notify_one();
        send(&mut socket, wire!({"rpcstream":"1.0","close":id})).await;
        send(
            &mut socket,
            wire!({"jsonrpc":"2.0","id":id,"method":"echo","params":{"kept":true}}),
        )
        .await;
        assert_eq!(
            receive(&mut socket).await,
            wire!({"jsonrpc":"2.0","id":id,"result":{"kept":true}})
        );
        assert_eq!(context.completed.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn canceling_a_batch_member_releases_its_future_and_preserves_sibling_responses() {
    let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
    send(
        &mut socket,
        wire!([
            {"jsonrpc":"2.0","id":1,"method":"blocked","params":{"value":1}},
            {"jsonrpc":"2.0","id":2,"method":"echo","params":{"kept":true}}
        ]),
    )
    .await;
    observed(&context.started).await;
    send(&mut socket, wire!({"rpcstream":"1.0","close":1})).await;
    observed(&context.drop_signal).await;
    assert_eq!(
        receive(&mut socket).await,
        wire!([{"jsonrpc":"2.0","id":2,"result":{"kept":true}}])
    );
    send(&mut socket, wire!({"jsonrpc":"2.0","id":1,"method":"echo"})).await;
    assert_eq!(receive(&mut socket).await["id"], 1);
}

#[tokio::test]
async fn numeric_and_string_cancellation_ids_are_distinct() {
    let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
    for id in [wire!(8), wire!("8")] {
        send(
            &mut socket,
            wire!({"jsonrpc":"2.0","id":id,"method":"blocked","params":{"value":1}}),
        )
        .await;
        observed(&context.started).await;
    }
    send(&mut socket, wire!({"rpcstream":"1.0","close":8})).await;
    observed(&context.drop_signal).await;
    assert_eq!(context.dropped.load(Ordering::SeqCst), 1);
    context.release.notify_one();
    assert_eq!(
        receive(&mut socket).await,
        wire!({"jsonrpc":"2.0","id":"8","result":{"snapshot":1}})
    );
    assert_eq!(context.completed.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn socket_close_drops_all_pending_calls_and_notifications_without_polling_them_again() {
    let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
    for id in [Some(wire!(1)), Some(wire!(2)), None] {
        let mut request = wire!({"jsonrpc":"2.0","method":"blocked","params":{"value":1}});
        if let Some(id) = id {
            request["id"] = id;
        }
        send(&mut socket, request).await;
        observed(&context.started).await;
    }
    socket.close(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while context.dropped.load(Ordering::SeqCst) != 3 {
            context.drop_signal.notified().await;
        }
    })
    .await
    .unwrap();
    context.release.notify_waiters();
    assert_eq!(context.completed.load(Ordering::SeqCst), 0);
}
