use super::*;

#[tokio::test]
async fn encrypted_streams_deliver_typed_terminals_and_cancel_pending_initialization_on_the_reused_session()
 {
    let connections = Arc::new(AtomicUsize::new(0));
    let pending = Arc::new(tokio::sync::Notify::new());
    let canceled = Arc::new(tokio::sync::Notify::new());
    let seen = connections.clone();
    let pending_seen = pending.clone();
    let canceled_seen = canceled.clone();
    let server_key: Signer = crate::sign::AnySigningKey::generate().into();
    let app = Router::new().route(RpcSessionUpgrader::PATH, get(move |upgrade: WebSocketUpgrade| {
        seen.fetch_add(1, Ordering::SeqCst);
        let key = server_key.clone();
        let pending = pending_seen.clone();
        let canceled = canceled_seen.clone();
        async move { upgrade.on_upgrade(move |socket| async move {
            let (_, _, mut socket) = EncryptedSocket::accept(Box::new(socket), &key, &["server.test".into()], NonZeroUsize::new(1024 * 1024).unwrap(), &Target::Rpc, None, |_| async { Ok(()) }).await.unwrap();
            for step in 0..4 {
                let body = socket.next().await.unwrap().unwrap().into_text().unwrap();
                let request: serde_json::Value = serde_json::from_str(&body).unwrap();
                let id = request["id"].clone();
                if step == 0 {
                    assert_eq!(request["rpcstream"], rpc_toolkit::RPCSTREAM_VERSION);
                    for response in [
                        serde_json::json!({"jsonrpc":"2.0","id":id,"result":"initial","meta":{"patchSequence":7}}),
                        serde_json::json!({"rpcstream":rpc_toolkit::RPCSTREAM_VERSION,"id":id,"result":42,"end":false}),
                        serde_json::json!({"rpcstream":rpc_toolkit::RPCSTREAM_VERSION,"id":id,"result":true,"end":true}),
                    ] { socket.send(Message::Text(response.to_string().into())).await.unwrap(); }
                } else if step == 1 {
                    socket.send(Message::Text(serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":34,"message":"denied"}}).to_string().into())).await.unwrap();
                } else if step == 2 {
                    socket.send(Message::Text(serde_json::json!({"jsonrpc":"2.0","id":id,"result":"kept"}).to_string().into())).await.unwrap();
                } else {
                    pending.notify_one();
                    let close = socket.next().await.unwrap().unwrap().into_text().unwrap();
                    let close: serde_json::Value = serde_json::from_str(&close).unwrap();
                    assert_eq!(close["close"], id);
                    canceled.notify_one();
                }
            }
        }) }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url: Url = format!(
        "ws://{}{}",
        listener.local_addr().unwrap(),
        RpcSessionUpgrader::PATH
    )
    .parse()
    .unwrap();
    let _server: NonDetachingJoinHandle<()> =
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }).into();
    let client = Client::new(
        reqwest::Client::builder()
            .http1_only()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap(),
    );
    let signer: Signer = crate::sign::AnySigningKey::generate().into();
    let mut stream = client
        .stream::<u64, bool>(
            &signer,
            url.clone(),
            "server.test",
            HeaderMap::new(),
            "progress",
            imbl_value::json!({}),
        )
        .await
        .unwrap();
    assert_eq!(stream.initial.as_str(), Some("initial"));
    assert_eq!(stream.meta["patchSequence"].as_u64(), Some(7));
    assert_eq!(stream.items.next().await.unwrap(), Some(42));
    assert!(stream.items.finish().await.unwrap());
    assert_eq!(
        client
            .call(
                &signer,
                url.clone(),
                "server.test",
                HeaderMap::new(),
                "denied",
                imbl_value::json!({})
            )
            .await
            .unwrap_err()
            .code,
        34
    );
    assert_eq!(
        client
            .call(
                &signer,
                url.clone(),
                "server.test",
                HeaderMap::new(),
                "kept",
                imbl_value::json!({})
            )
            .await
            .unwrap()
            .as_str(),
        Some("kept")
    );
    let mut request = Box::pin(client.stream::<u64, bool>(
        &signer,
        url,
        "server.test",
        HeaderMap::new(),
        "pending",
        imbl_value::json!({}),
    ));
    tokio::select! { _ = pending.notified() => (), result = &mut request => panic!("unexpected initial reply: {}", result.err().unwrap()) }
    drop(request);
    tokio::time::timeout(Duration::from_secs(3), canceled.notified())
        .await
        .unwrap();
    assert_eq!(connections.load(Ordering::SeqCst), 1);
}
