use super::*;
use crate::middleware::auth::session::RpcSessionUpgrader;
use crate::middleware::auth::signature::AUTH_SIG_HEADER;
use crate::sign::session::Target;
use crate::sign::{AnySigningKey, Signer};
use crate::util::net::secure::EncryptedSocket;
use crate::util::net::tungstenite::transport;

fn signer() -> Signer {
    Signer::Key(AnySigningKey::Ed25519(
        ed25519_dalek::SigningKey::from_bytes(&[71; 32]),
    ))
}

pub(super) async fn encrypted_server(
    socket: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) -> EncryptedSocket {
    let identity = Signer::Key(AnySigningKey::Ed25519(
        ed25519_dalek::SigningKey::from_bytes(&[72; 32]),
    ));
    let (_, _, socket) = EncryptedSocket::accept(
        transport(socket),
        &identity,
        &["registry.test".into()],
        std::num::NonZeroUsize::new(1024 * 1024).unwrap(),
        &Target::Rpc,
        Some(&signer().verifying_key().await.unwrap()),
        |_| async { Ok(()) },
    )
    .await
    .unwrap();
    socket
}

pub(super) async fn native_connection(
    registry: &Url,
    signer: &Signer,
) -> (Result<EncryptedSocket, Error>, NonDetachingJoinHandle<()>) {
    let (client, task) = crate::registry::context::test_utils::socks_client(registry).await;
    (
        super::super::connection::connect(&client, registry, signer, http::HeaderMap::new()).await,
        task,
    )
}

#[test]
fn channel_authenticated_watches_keep_distinct_ids_and_projection_versions() {
    let version = device().os.version;
    let index = subscription_request(&Subscription::Index(url()), &version);
    let os = subscription_request(&Subscription::Os(url()), &version);
    assert_ne!(index["id"], os["id"]);
    assert_eq!(index["params"]["pointer"], json!("/index"));
    assert_eq!(os["params"]["pointer"], json!("/index/os"));
    for request in [index, os] {
        assert_eq!(request["method"], json!("db.subscribe"));
        assert_eq!(request["rpcstream"], json!(rpc_toolkit::RPCSTREAM_VERSION));
        assert_eq!(request["params"]["osVersion"], json!(version));
        assert!(request.get("signature").is_none());
    }
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
    let registry_url = format!(
        "http://registry.test:{}/catalog",
        listener.local_addr().unwrap().port()
    )
    .parse()
    .unwrap();
    let server: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let socket = tokio_tungstenite::accept_hdr_async(
            stream,
            |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                assert!(!request.headers().contains_key(AUTH_SIG_HEADER));
                assert_eq!(request.uri().path(), "/catalog/rpc/v2");
                let query: BTreeMap<String, String> =
                    serde_urlencoded::from_str(request.uri().query().unwrap_or_default()).unwrap();
                assert!(query.is_empty());
                Ok(response)
            },
        )
        .await
        .unwrap();
        let mut socket = encrypted_server(socket).await;
        let mut ids = BTreeMap::new();
        for _ in 0..2 {
            let message = socket.next().await.unwrap().unwrap();
            let Message::Text(text) = message else {
                panic!("expected encrypted JSON request")
            };
            let request: Value = serde_json::from_str(&text).unwrap();
            assert!(request.get("signature").is_none());
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
    let (socket, _proxy) = native_connection(&registry_url, &signer()).await;
    let socket = socket.unwrap();
    let mut backoff = 300;
    tokio::time::timeout(
        Duration::from_secs(30),
        receive_updates(
            socket,
            &db,
            &BTreeSet::from([Subscription::Index(url()), Subscription::Os(url())]),
            &device(),
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
    assert_eq!(cache.status, RegistryStatus::Connected);
    assert_eq!(cache.info.name.unwrap().len(), 5 * 1024 * 1024);
    let os_cache = snapshot
        .as_public()
        .as_os_update_cache()
        .de()
        .unwrap()
        .unwrap();
    assert_eq!(os_cache.status, RegistryStatus::Connected);
    assert!(os_cache.versions.contains_key(&"0.4.0.6".parse().unwrap()));
}

#[tokio::test]
async fn native_connection_rejects_plaintext_registry_without_fallback() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let registry = format!(
        "http://registry.test:{}/catalog",
        listener.local_addr().unwrap().port()
    )
    .parse()
    .unwrap();
    let server: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        assert!(matches!(
            socket.next().await.unwrap().unwrap(),
            tokio_tungstenite::tungstenite::Message::Binary(_)
        ));
        socket
            .send(tokio_tungstenite::tungstenite::Message::Text(
                r#"{"jsonrpc":"2.0","id":1,"result":{"id":0,"value":{}}}"#.into(),
            ))
            .await
            .unwrap();
    })
    .into();
    let (result, _proxy) = tokio::time::timeout(
        Duration::from_secs(5),
        native_connection(&registry, &signer()),
    )
    .await
    .unwrap();
    assert!(matches!(
        result,
        Err(Error {
            kind: ErrorKind::InvalidRequest,
            ..
        })
    ));
    server.await.unwrap();
}

#[test]
fn rpc_endpoint_preserves_deployment_prefix_and_existing_query_without_negotiation() {
    let target = RpcSessionUpgrader::url(
        "https://registry.test:8443/catalog?keep=value"
            .parse()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        target.as_str(),
        "wss://registry.test:8443/catalog/rpc/v2?keep=value"
    );
    let target = RpcSessionUpgrader::url("http://[::1]:5959".parse().unwrap()).unwrap();
    assert_eq!(target.as_str(), "ws://[::1]:5959/rpc/v2");
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
async fn resubscribing_discovers_later_private_grants_and_keeps_live_revocation() {
    let (_registry_temp, registry, _admin, reader, _) = crate::registry::tests::fixture().await;
    let secret = "secret".parse().unwrap();
    let readers = registry
        .db
        .peek()
        .await
        .as_index()
        .as_package()
        .as_packages()
        .as_idx(&secret)
        .unwrap()
        .as_access()
        .as_readers()
        .de()
        .unwrap();
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
    let (url, _server) = crate::registry::tests::server(registry.clone()).await;
    let url = registry_url(url.as_str()).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let db = TypedPatchDb::<Database>::load_unchecked(
        PatchDb::open(temp.path().join("db")).await.unwrap(),
    );
    let mut model = database();
    model
        .as_public_mut()
        .as_server_info_mut()
        .mutate(|info| {
            info.registries = from_value(json!({url.as_str():"Registry"}))?;
            info.os_registry = url.clone();
            Ok::<_, Error>(())
        })
        .unwrap();
    db.put(&ROOT, model.as_value()).await.unwrap();
    let subscribe = || -> NonDetachingJoinHandle<Result<(), Error>> {
        let db = db.clone();
        let url = url.clone();
        let reader = reader.clone();
        tokio::spawn(async move {
            let (socket, _proxy) = native_connection(&url, &reader).await;
            let socket = socket.unwrap();
            receive_updates(
                socket,
                &db,
                &BTreeSet::from([Subscription::Index(url.clone()), Subscription::Os(url)]),
                &device(),
                &mut 300,
            )
            .await
        })
        .into()
    };
    let receiver = subscribe();
    wait_for(&db, |db| {
        db.as_public()
            .as_os_update_cache()
            .de()
            .unwrap()
            .is_some_and(|cache| cache.status == RegistryStatus::Connected)
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
                .ser(&readers)?;
            db.as_index_mut()
                .as_name_mut()
                .ser(&Some("Grant barrier".into()))
        })
        .await
        .result
        .unwrap();
    wait_for(&db, |db| {
        db.as_private()
            .as_registry_indexes()
            .as_idx(&url)
            .is_some_and(|index| index.as_name().as_value() == &json!("Grant barrier"))
    })
    .await;
    assert!(
        db.peek()
            .await
            .as_private()
            .as_registry_indexes()
            .as_idx(&url)
            .unwrap()
            .as_package()
            .as_packages()
            .as_idx(&secret)
            .is_none()
    );
    let _ = receiver.wait_for_abort().await;
    let receiver = subscribe();
    wait_for(&db, |db| {
        db.as_public()
            .as_os_update_cache()
            .de()
            .unwrap()
            .is_some_and(|cache| cache.status == RegistryStatus::Connected)
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
