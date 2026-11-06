use super::*;
use crate::middleware::auth::signature::rpc::{RPC_SIGNATURE_FIELD, rpc_signing_body};
use crate::middleware::auth::signature::{AUTH_SIG_HEADER, SignatureHeader};
use crate::sign::{AnySigningKey, Signer};

fn signer() -> Signer {
    Signer::Key(AnySigningKey::Ed25519(
        ed25519_dalek::SigningKey::from_bytes(&[71; 32]),
    ))
}

async fn verify_watch(request: &Value, signer: &Signer, context: &str) -> SignatureHeader {
    let proof = SignatureHeader::from_header(
        &http::HeaderValue::from_str(request[RPC_SIGNATURE_FIELD].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(proof.signer, signer.verifying_key().await.unwrap());
    proof.verify_rpc_commitment(context).unwrap();
    proof
        .commitment
        .verify_bytes(&rpc_signing_body(request).unwrap())
        .await
        .unwrap();
    proof
}

#[tokio::test]
async fn watch_proofs_bind_each_projection_and_use_distinct_nonces() {
    let signer = signer();
    let index = watch_request(&Subscription::Index(url()), &signer)
        .await
        .unwrap();
    let os = watch_request(&Subscription::Os(url()), &signer)
        .await
        .unwrap();
    let first = verify_watch(&index, &signer, "registry.test").await;
    let second = verify_watch(&os, &signer, "registry.test").await;
    assert_ne!(first.commitment.nonce, second.commitment.nonce);
    assert_ne!(index["id"], os["id"]);
    assert_eq!(index["params"]["pointer"], json!("/index"));
    assert_eq!(os["params"]["pointer"], json!("/index/os"));
    assert!(first.verify_rpc_commitment("another.test").is_err());
    assert!(
        first
            .commitment
            .verify_bytes(&rpc_signing_body(&os).unwrap())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn one_socket_demultiplexes_large_dumps_and_interleaved_revisions() {
    let temp = tempfile::tempdir().unwrap();
    let db = TypedPatchDb::<Database>::load_unchecked(
        PatchDb::open(temp.path().join("db")).await.unwrap(),
    );
    db.put(&ROOT, database().as_value()).await.unwrap();
    let index_source = PatchDb::in_memory();
    let mut index = index();
    index.name = Some("x".repeat(17 * 1024 * 1024));
    index_source.put(&ROOT, &index).await.unwrap();
    let index_dump = index_source.dump(&ROOT).await;
    index.name = Some("y".repeat(5 * 1024 * 1024));
    let index_revision = index_source.put(&ROOT, &index).await.unwrap().unwrap();
    let os_source = PatchDb::in_memory();
    os_source.put(&ROOT, &OsIndex::default()).await.unwrap();
    let os_dump = os_source.dump(&ROOT).await;
    let mut os = OsIndex::default();
    let asset = from_value(json!({"publishedAt":"2026-01-01T00:00:00Z", "urls":[], "commitment":{"hash":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","size":0}, "signatures":{}})).unwrap();
    os.versions.0.insert(
        "0.4.0.6".parse().unwrap(),
        OsVersionInfo {
            source_version: "*".parse().unwrap(),
            squashfs: [("x86_64".into(), asset)].into(),
            ..Default::default()
        },
    );
    let os_revision = os_source.put(&ROOT, &os).await.unwrap().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let socket_url = format!("ws://{}", listener.local_addr().unwrap());
    let server: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_hdr_async(
            stream,
            |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                assert!(!request.headers().contains_key(AUTH_SIG_HEADER));
                Ok(response)
            },
        )
        .await
        .unwrap();
        let mut ids = BTreeMap::new();
        for _ in 0..2 {
            let message = socket.next().await.unwrap().unwrap();
            let request: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
            verify_watch(&request, &signer(), "registry.test").await;
            ids.insert(
                request["params"]["pointer"].as_str().unwrap().to_owned(),
                request["id"].clone(),
            );
        }
        for message in [
            json!({"jsonrpc":"2.0", "id":ids["/index/os"], "result":os_dump}),
            json!({"jsonrpc":"2.0", "id":ids["/index"], "result":index_dump}),
            json!({"rpcstream":"1.0", "id":ids["/index/os"], "result":os_revision, "end":false}),
            json!({"rpcstream":"1.0", "id":ids["/index"], "result":index_revision, "end":false}),
        ] {
            socket
                .send(Message::Text(message.to_string().into()))
                .await
                .unwrap();
        }
        socket.send(Message::Ping(vec![8, 9].into())).await.unwrap();
        assert_eq!(
            socket.next().await.unwrap().unwrap(),
            Message::Pong(vec![8, 9].into())
        );
        socket.send(Message::Close(None)).await.unwrap();
    })
    .into();
    let (socket, _) =
        tokio_tungstenite::connect_async_with_config(socket_url, Some(response_config()), false)
            .await
            .unwrap();
    let mut backoff = 300;
    tokio::time::timeout(
        Duration::from_secs(30),
        receive_updates(
            socket,
            &db,
            &BTreeSet::from([Subscription::Index(url()), Subscription::Os(url())]),
            &device(),
            &signer(),
            &mut backoff,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    server.await.unwrap();
    assert_eq!(backoff, 30);
    let snapshot = db.peek().await;
    let cache = snapshot
        .as_public()
        .as_registry_cache()
        .as_idx(&url())
        .unwrap()
        .de()
        .unwrap();
    assert!(cache.connected);
    assert_eq!(cache.info.name.unwrap().len(), 5 * 1024 * 1024);
    let os_cache = snapshot
        .as_public()
        .as_os_update_cache()
        .de()
        .unwrap()
        .unwrap();
    assert!(os_cache.connected);
    assert!(os_cache.versions.contains_key(&"0.4.0.6".parse().unwrap()));
}

async fn wait_for(db: &TypedPatchDb<Database>, predicate: impl Fn(&DatabaseModel) -> bool) {
    let mut watch = db.watch(ROOT.to_owned()).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if predicate(&Model::from_value(watch.peek_and_mark_seen().unwrap())) {
                break;
            }
            watch.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn signed_client_caches_private_packages_and_live_revocation_from_the_registry() {
    let (_registry_temp, registry, _admin, reader, _) = crate::registry::tests::fixture().await;
    let (url, _server) = crate::registry::tests::server(registry.clone()).await;
    let url = registry_url(url.as_str()).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let db = TypedPatchDb::<Database>::load_unchecked(
        PatchDb::open(temp.path().join("db")).await.unwrap(),
    );
    let mut model = database();
    model
        .as_public_mut()
        .as_ui_mut()
        .ser(&json!({"registries":{url.as_str():"Registry"},"startosRegistry":url}))
        .unwrap();
    db.put(&ROOT, model.as_value()).await.unwrap();
    let (socket, _) = tokio_tungstenite::connect_async_with_config(
        websocket_url(&url).unwrap(),
        Some(response_config()),
        false,
    )
    .await
    .unwrap();
    let receiver: NonDetachingJoinHandle<Result<(), Error>> = {
        let db = db.clone();
        let url = url.clone();
        tokio::spawn(async move {
            receive_updates(
                socket,
                &db,
                &BTreeSet::from([Subscription::Index(url.clone()), Subscription::Os(url)]),
                &device(),
                &reader,
                &mut 300,
            )
            .await
        })
        .into()
    };
    let secret = "secret".parse().unwrap();
    wait_for(&db, |db| {
        db.as_public()
            .as_os_update_cache()
            .de()
            .unwrap()
            .is_some_and(|cache| cache.connected)
            && db
                .as_private()
                .as_registry_indexes()
                .as_idx(&url)
                .is_some_and(|index| index.as_package().as_packages().as_idx(&secret).is_some())
    })
    .await;
    registry
        .db
        .mutate(|db| {
            db.as_index_mut()
                .as_package_mut()
                .as_packages_mut()
                .as_idx_mut(&secret)
                .unwrap()
                .as_access_mut()
                .as_readers_mut()
                .ser(&BTreeSet::new())
        })
        .await
        .result
        .unwrap();
    wait_for(&db, |db| {
        db.as_private()
            .as_registry_indexes()
            .as_idx(&url)
            .is_some_and(|index| index.as_package().as_packages().as_idx(&secret).is_none())
    })
    .await;
    assert!(!receiver.is_finished());
    drop(receiver);
}
