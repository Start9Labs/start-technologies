use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::http::StatusCode;
use axum::routing::{any, get};

use super::*;
use crate::middleware::auth::signature::AUTH_SIG_HEADER;
use crate::registry::context::test_utils::socks_client;
use crate::registry::context::{call, test_context_for_hosts};
use crate::sign::{AnySigningKey, Signer};

fn signer() -> Signer {
    AnySigningKey::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[81; 32])).into()
}

async fn server(
    temp: &tempfile::TempDir,
    status: StatusCode,
) -> (
    crate::registry::context::RegistryContext,
    Url,
    Arc<AtomicUsize>,
    NonDetachingJoinHandle<()>,
) {
    let ctx = test_context_for_hosts(
        temp.path().join("registry"),
        signer().verifying_key().await.unwrap(),
        vec!["registry.test".into()],
    )
    .await;
    let mut packages = index().package;
    packages.packages.get_mut("test").unwrap().access.private = true;
    ctx.db
        .put(&"/index/package".parse().unwrap(), &packages)
        .await
        .unwrap();
    ctx.db
        .put(&"/index/name".parse().unwrap(), &"Legacy Registry")
        .await
        .unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    let http = crate::registry::registry_server(ctx.clone());
    let router = Router::new()
        .route(
            "/catalog/rpc/v0",
            any(http).layer(axum::middleware::from_fn(
                move |request: axum::extract::Request, next: axum::middleware::Next| {
                    let count = count.clone();
                    async move {
                        assert!(request.headers().contains_key(AUTH_SIG_HEADER));
                        assert_eq!(request.uri().path(), "/catalog/rpc/v0");
                        count.fetch_add(1, Ordering::SeqCst);
                        next.run(request).await
                    }
                },
            )),
        )
        .route(
            "/catalog/rpc/v2",
            get(move || async move {
                (
                    status,
                    axum::Json(serde_json::json!({
                        "jsonrpc": "2.0", "error": {"code": -32700, "message": "Parse error",
                            "data": "EOF while parsing a value at line 1 column 0"}, "id": null
                    })),
                )
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://registry.test:{}/catalog",
        listener.local_addr().unwrap().port()
    )
    .parse()
    .unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() }).into();
    (ctx, url, requests, task)
}

#[test]
fn fallback_only_recognizes_missing_upgrade_support() {
    use tokio_tungstenite::tungstenite::Error as WebSocketError;
    for status in [200, 301, 302, 400, 401, 403, 404, 405, 426, 500, 501, 503] {
        let response = http::Response::builder().status(status).body(None).unwrap();
        let error = Error::new(WebSocketError::Http(response), ErrorKind::Network);
        assert_eq!(
            connection::websocket_unsupported(&error),
            matches!(status, 200 | 404 | 405 | 501)
        );
    }
    for kind in [
        std::io::ErrorKind::TimedOut,
        std::io::ErrorKind::ConnectionReset,
    ] {
        let error = Error::new(std::io::Error::from(kind), ErrorKind::Network);
        assert!(!connection::websocket_unsupported(&error));
    }
    assert_eq!(super::super::polling::INTERVAL, Duration::from_secs(900));
}

#[tokio::test]
async fn unsupported_ws_discovery_and_polling_use_signed_http_and_preserve_selection() {
    let temp = tempfile::tempdir().unwrap();
    let (ctx, registry, requests, _server) = server(&temp, StatusCode::OK).await;
    let (client, _proxy) = socks_client(&registry).await;
    let info = call(
        &client,
        &signer(),
        &registry,
        http::HeaderMap::new(),
        "info",
        json!({}),
    )
    .await
    .unwrap();
    assert_eq!(info["name"], json!("Legacy Registry"));
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    let db = TypedPatchDb::<Database>::load_unchecked(
        PatchDb::open(temp.path().join("os-db")).await.unwrap(),
    );
    let mut initial = database();
    initial
        .as_public_mut()
        .as_server_info_mut()
        .mutate(|info| {
            info.registries = from_value(json!({registry.as_str():"Legacy"}))?;
            info.os_registry = registry.clone();
            Ok::<_, Error>(())
        })
        .unwrap();
    db.put(&ROOT, initial.as_value()).await.unwrap();
    let subscriptions = BTreeSet::from([
        Subscription::Index(registry.clone()),
        Subscription::Os(registry.clone()),
    ]);
    super::super::polling::poll(&db, &subscriptions, &device(), &client, &signer())
        .await
        .unwrap();
    assert_eq!(requests.load(Ordering::SeqCst), 4);
    let snapshot = db.peek().await;
    let cache = snapshot
        .as_public()
        .as_registry_cache()
        .as_idx(&registry)
        .unwrap()
        .de()
        .unwrap();
    assert_eq!(cache.status, RegistryStatus::Connected);
    assert_eq!(cache.info.name.as_deref(), Some("Legacy Registry"));
    assert!(cache.packages.contains_key("test"));
    assert_eq!(
        snapshot
            .as_public()
            .as_os_update_cache()
            .de()
            .unwrap()
            .unwrap()
            .status,
        RegistryStatus::Connected
    );
    let selection = cached_response(
        &snapshot,
        &registry,
        "package.get",
        &json!({"id":"test", "sourceVersion":"1.0.0:0"}),
        Some(&device()),
    )
    .unwrap()
    .unwrap();
    assert!(selection["best"].get("1.0.0:0").is_some());
    assert!(selection["best"].get("2.0.0:0").is_none());
    assert_eq!(
        snapshot
            .as_private()
            .as_registry_indexes()
            .as_idx(&registry)
            .unwrap()
            .as_package()
            .as_packages()
            .as_idx(&"test".parse().unwrap())
            .unwrap()
            .as_versions()
            .de()
            .unwrap()
            .len(),
        2
    );
    for key in cache
        .info
        .trust_signers
        .values()
        .flat_map(|signer| &signer.keys)
    {
        let trusted = snapshot
            .as_public()
            .as_server_info()
            .as_trusted_keys()
            .de()
            .unwrap();
        let entry = trusted
            .values()
            .find(|entry| &entry.public_key == key)
            .unwrap();
        assert!(!entry.marketplace);
        assert!(!entry.os);
    }
    drop(snapshot);
    let before = db.dump(&ROOT).await.value;
    ctx.db
        .put(
            &"/index/package/packages".parse().unwrap(),
            &json!("invalid registry response"),
        )
        .await
        .unwrap();
    assert!(
        super::super::polling::poll(&db, &subscriptions, &device(), &client, &signer())
            .await
            .is_err()
    );
    assert_eq!(db.dump(&ROOT).await.value, before);
    ctx.db
        .put(&"/index/package/packages".parse().unwrap(), &json!({}))
        .await
        .unwrap();
    super::super::polling::poll(&db, &subscriptions, &device(), &client, &signer())
        .await
        .unwrap();
    let snapshot = db.peek().await;
    let selection = cached_response(
        &snapshot,
        &registry,
        "package.get",
        &json!({"id":"test"}),
        Some(&device()),
    )
    .unwrap()
    .unwrap();
    assert_eq!(selection["best"], json!({}));
}

#[tokio::test]
async fn authentication_and_redirect_errors_do_not_trigger_http_requests() {
    for status in [
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::FOUND,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let (_, registry, requests, _server) = server(&temp, status).await;
        let (client, _proxy) = socks_client(&registry).await;
        assert!(
            call(
                &client,
                &signer(),
                &registry,
                http::HeaderMap::new(),
                "info",
                json!({})
            )
            .await
            .is_err()
        );
        assert_eq!(requests.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn accepted_websocket_with_failed_admission_never_retries_http() {
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    let router = Router::new()
        .route(
            "/rpc/v2",
            get(|ws: axum::extract::WebSocketUpgrade| async move {
                ws.on_upgrade(|mut socket| async move {
                    assert!(matches!(
                        socket.recv().await.unwrap().unwrap(),
                        Message::Binary(_)
                    ));
                    socket
                        .send(Message::Text("plaintext".into()))
                        .await
                        .unwrap();
                })
            }),
        )
        .route(
            "/rpc/v0",
            any(move || {
                let count = count.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    StatusCode::OK
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let registry = format!(
        "http://registry.test:{}",
        listener.local_addr().unwrap().port()
    )
    .parse()
    .unwrap();
    let _server: NonDetachingJoinHandle<()> =
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() }).into();
    let (client, _proxy) = socks_client(&registry).await;
    assert!(
        call(
            &client,
            &signer(),
            &registry,
            http::HeaderMap::new(),
            "info",
            json!({})
        )
        .await
        .is_err()
    );
    assert_eq!(requests.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn legacy_json_reply_limit_covers_chunked_responses() {
    use axum::body::Body;
    use http::header::CONTENT_TYPE;

    use crate::middleware::auth::signature::send_http;
    let router = Router::new().route(
        "/rpc/v0",
        any(|| async {
            let chunks = futures::stream::iter([
                Ok::<_, std::io::Error>(bytes::Bytes::from_static(
                    b"{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":\"",
                )),
                Ok(bytes::Bytes::from("x".repeat(64))),
                Ok(bytes::Bytes::from_static(b"\"}")),
            ]);
            (
                [(CONTENT_TYPE, "application/json")],
                Body::from_stream(chunks),
            )
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url: Url = format!("http://{}/rpc/v0", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let _server: NonDetachingJoinHandle<()> =
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() }).into();
    assert!(
        send_http(
            reqwest::Client::new(),
            url.clone(),
            http::HeaderMap::new(),
            "{}".into(),
            32
        )
        .await
        .is_err()
    );
    assert_eq!(
        send_http(
            reqwest::Client::new(),
            url,
            http::HeaderMap::new(),
            "{}".into(),
            128
        )
        .await
        .unwrap(),
        json!("x".repeat(64))
    );
}
