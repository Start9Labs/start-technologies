use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use axum::Router;
use axum::routing::post;
use http::HeaderMap;
use rpc_toolkit::CallRemote;
use url::Url;

use crate::context::CliContext;
use crate::context::config::ClientConfig;
use crate::middleware::auth::session::RpcSessionUpgrader;
use crate::registry::context::RegistryContext;
use crate::sign::Signer;
use crate::util::future::NonDetachingJoinHandle;

async fn server(app: Router) -> (Url, NonDetachingJoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/nested/", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let app = Router::new().nest("/nested", app);
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }).into();
    (url, task)
}

fn cli(url: &Url, signer: Signer) -> (CliContext, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let ctx = CliContext::init(ClientConfig {
        id_key_path: Some(directory.path().join("id.pem")),
        registry: Some(url.as_str().parse().unwrap()),
        ..Default::default()
    })
    .unwrap();
    ctx.id_key.set(signer).unwrap();
    (ctx, directory)
}

#[tokio::test]
async fn actual_cli_reuses_connections_after_application_errors_and_scopes_headers_and_signers() {
    let (_temp, registry, admin, _, _) = crate::registry::tests::fixture().await;
    let upgrades = Arc::new(AtomicUsize::new(0));
    let observed = upgrades.clone();
    let app = crate::registry::registry_router(registry).layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            if request.uri().path().ends_with(RpcSessionUpgrader::PATH) {
                observed.fetch_add(1, Ordering::SeqCst);
            }
            async move { next.run(request).await }
        },
    ));
    let (url, _server) = server(app).await;
    let (ctx, _identity) = cli(&url, admin.clone());
    assert_eq!(upgrades.load(Ordering::SeqCst), 0);
    for method in ["registry.info", "registry.missing-method", "registry.info"] {
        let result = <CliContext as CallRemote<RegistryContext>>::call_remote(
            &ctx,
            method,
            Default::default(),
            imbl_value::json!({}),
            rpc_toolkit::Empty {},
        )
        .await;
        assert_eq!(result.is_err(), method == "registry.missing-method");
    }
    assert_eq!(upgrades.load(Ordering::SeqCst), 1);
    let ws = super::RpcSessionUpgrader::url(url).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert("x-instance", "other".parse().unwrap());
    ctx.rpc_client
        .call(
            &admin,
            ws.clone(),
            "127.0.0.1",
            headers,
            "info",
            imbl_value::json!({}),
        )
        .await
        .unwrap();
    assert_eq!(upgrades.load(Ordering::SeqCst), 2);
    let unknown: Signer = crate::sign::AnySigningKey::generate().into();
    ctx.rpc_client
        .call(
            &unknown,
            ws,
            "127.0.0.1",
            HeaderMap::new(),
            "info",
            imbl_value::json!({}),
        )
        .await
        .unwrap();
    assert_eq!(upgrades.load(Ordering::SeqCst), 3);
}

#[test]
fn rpc_endpoint_replacement_retains_prefix_query_and_ipv6() {
    for (source, expected) in [
        (
            "https://registry.test/prefix/rpc/v0?keep=value",
            "wss://registry.test/prefix/rpc/v2?keep=value",
        ),
        (
            "http://[2001:db8::1]:5959/rpc/v1",
            "ws://[2001:db8::1]:5959/rpc/v2",
        ),
    ] {
        assert_eq!(
            super::websocket_url(source.parse().unwrap())
                .unwrap()
                .as_str(),
            expected
        );
    }
}

#[test]
fn http_rpc_urls_share_versioned_prefix_and_query_normalization() {
    for base in [
        "https://registry.test/prefix/",
        "https://registry.test/prefix",
    ] {
        let url: Url = format!("{base}?keep=value").parse().unwrap();
        assert_eq!(
            crate::registry::package::promote::registry_rpc_url(&url)
                .unwrap()
                .as_str(),
            "https://registry.test/prefix/rpc/v0?keep=value"
        );
        let ctx = CliContext::init(ClientConfig {
            host: Some(url.as_str().parse().unwrap()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(
            ctx.rpc_url().unwrap().as_str(),
            "https://registry.test/prefix/rpc/v1?keep=value"
        );
        assert_eq!(
            super::RpcSessionUpgrader::url(url).unwrap().as_str(),
            "wss://registry.test/prefix/rpc/v2?keep=value"
        );
    }
}

#[tokio::test]
async fn legacy_cli_continuations_and_resources_keep_prefixed_urls_and_bytes() {
    use axum::extract::ws::{Message, WebSocketUpgrade};
    use axum::routing::get;
    use futures::StreamExt;
    let app = Router::new()
        .route(
            "/rpc/v2/{guid}",
            get(|upgrade: WebSocketUpgrade| async move {
                upgrade.on_upgrade(|mut socket| async move {
                    socket.send(Message::Text("legacy".into())).await.unwrap();
                })
            }),
        )
        .route(
            "/rest/rpc/{guid}",
            post(|headers: HeaderMap, body: bytes::Bytes| async move {
                assert_eq!(headers.get("x-fixture").unwrap(), "resource");
                body
            }),
        );
    let (url, _task) = server(app).await;
    let temp = tempfile::tempdir().unwrap();
    let ctx = CliContext::init(ClientConfig {
        host: Some(url.as_str().parse().unwrap()),
        id_key_path: Some(temp.path().join("uninitialized-legacy-key")),
        ..Default::default()
    })
    .unwrap();
    let guid = crate::rpc_continuations::Guid::new();
    let mut socket = ctx
        .ws_continuation::<crate::context::RpcContext>(guid.clone())
        .await
        .unwrap();
    assert_eq!(
        socket.next().await.unwrap().unwrap().into_text().unwrap(),
        "legacy"
    );
    let mut headers = HeaderMap::new();
    headers.insert("x-fixture", "resource".parse().unwrap());
    let captured = b"\xef\xbb\xbf{\"id\":18446744073709551615}";
    let response = ctx
        .rest_continuation(
            guid,
            reqwest::Body::from(captured.as_slice().to_owned()),
            headers,
        )
        .await
        .unwrap();
    assert_eq!(response.status(), http::StatusCode::OK);
    assert_eq!(response.bytes().await.unwrap().as_ref(), captured);
}

#[test]
fn native_request_body_preserves_integer_width_and_float_kind() {
    let params = imbl_value::json!({"integer":u64::MAX, "float":1.0, "negative":-0.0});
    let body = crate::middleware::auth::signature::rpc_request_body("echo", params).unwrap();
    assert!(body.contains("18446744073709551615"));
    assert!(body.contains("1.0"));
    assert!(body.contains("-0.0"));
}

#[tokio::test]
async fn configured_client_receives_a_bulk_private_index() {
    let (_temp, registry, admin, _, _) = crate::registry::tests::fixture().await;
    let mut database: crate::registry::RegistryDatabase = registry.db.peek().await.de().unwrap();
    database.index.name = Some("x".repeat(17 * 1024 * 1024));
    registry
        .db
        .put(&patch_db::json_ptr::ROOT, &database)
        .await
        .unwrap();
    let (url, _task) = server(crate::registry::registry_router(registry)).await;
    let (ctx, _identity) = cli(&url, admin);
    let result = <CliContext as CallRemote<RegistryContext>>::call_remote(
        &ctx,
        "registry.index",
        Default::default(),
        imbl_value::json!({}),
        rpc_toolkit::Empty {},
    )
    .await
    .unwrap();
    assert_eq!(result["name"].as_str().unwrap().len(), 17 * 1024 * 1024);
    assert!(result["package"]["packages"].get("secret").is_some());
}

#[tokio::test]
async fn normal_cli_rpc_and_guid_stay_encrypted_after_public_startup_reply() {
    use crate::context::{InitContext, RpcContext};
    let (_temp, registry, admin, _, _) = crate::registry::tests::fixture().await;
    let http_calls = Arc::new(AtomicUsize::new(0));
    let observed = http_calls.clone();
    use rpc_toolkit::HandlerExt;
    let api = crate::registry::registry_api::<RegistryContext>().subcommand(
        "state",
        rpc_toolkit::from_fn(|_: RegistryContext| Ok::<_, crate::Error>("initializing".to_owned()))
            .with_metadata("authenticated", imbl_value::json!(false))
            .no_cli(),
    );
    let factory = registry.clone();
    let rpc = rpc_toolkit::Server::new(
        move || {
            let ctx = factory.clone();
            async move { Ok(ctx) }
        },
        api,
    )
    .middleware(
        crate::middleware::auth::Auth::new()
            .with_local_auth()
            .with_signature_auth(),
    )
    .middleware(crate::registry::device_info::DeviceInfoMiddleware::new());
    let app = crate::net::static_server::session_rpc_router(
        registry.clone(),
        rpc.clone(),
        axum::routing::any(rpc).layer(crate::middleware::cors::layer()),
    )
    .route(
        "/rpc/v1",
        post(move |axum::Json(body): axum::Json<serde_json::Value>| {
            let observed = observed.clone();
            async move {
                observed.fetch_add(1, Ordering::Relaxed);
                assert_eq!(body["method"], "state");
                axum::Json(serde_json::json!({"jsonrpc":"2.0","id":0,"result":"initializing"}))
            }
        }),
    );
    let (url, _server) = server(app).await;
    let ctx = CliContext::init(ClientConfig {
        host: Some(url.as_str().parse().unwrap()),
        ..Default::default()
    })
    .unwrap();
    ctx.id_key.set(admin).unwrap();
    let state = ctx
        .call_remote::<InitContext>("state", imbl_value::json!({}))
        .await
        .unwrap();
    assert_eq!(state.as_str(), Some("initializing"));
    let dump = ctx
        .call_remote::<RpcContext>("db.dump", imbl_value::json!({}))
        .await
        .unwrap();
    assert!(
        dump["value"]["index"]["package"]["packages"]
            .get("secret")
            .is_some()
    );
    let subscription = ctx
        .call_remote::<RpcContext>("db.subscribe", imbl_value::json!({"pointer":"/index"}))
        .await
        .unwrap();
    let guid = imbl_value::from_value(subscription["guid"].clone()).unwrap();
    let mut socket = ctx.ws_continuation::<RpcContext>(guid).await.unwrap();
    let mut database: crate::registry::RegistryDatabase = registry.db.peek().await.de().unwrap();
    database.index.name = Some("after-admission".into());
    registry
        .db
        .put(&patch_db::json_ptr::ROOT, &database)
        .await
        .unwrap();
    let message = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        crate::util::net::tungstenite::next_text(&mut socket),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    assert!(message.contains("after-admission"));
    assert_eq!(http_calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn all_startup_contexts_ignore_an_unavailable_stored_signer() {
    use crate::context::{DiagnosticContext, InitContext, SetupContext};
    let dns = Arc::new(crate::net::dns::DnsController::new(
        crate::util::sync::Watch::new(Default::default()),
    ));
    let startup = InitContext::init(&crate::context::config::ServerConfig::default(), dns)
        .await
        .unwrap();
    let app = crate::net::static_server::ui_router(startup.clone());
    let (url, _server) = server(app).await;
    let ctx = CliContext::init(ClientConfig {
        host: Some(url.as_str().parse().unwrap()),
        id_key_gpg: Some("unavailable-startup-fixture".into()),
        ..Default::default()
    })
    .unwrap();
    for value in [
        ctx.call_remote::<InitContext>("state", imbl_value::json!({}))
            .await,
        ctx.call_remote::<DiagnosticContext>("state", imbl_value::json!({}))
            .await,
        ctx.call_remote::<SetupContext>("state", imbl_value::json!({}))
            .await,
    ] {
        assert_eq!(value.unwrap().as_str(), Some("initializing"));
    }
    async fn check<C: crate::context::cli::CliWebSocketContext>(ctx: &CliContext) {
        let subscription: crate::init::InitProgressRes = imbl_value::from_value(
            ctx.call_remote::<InitContext>("init.subscribe", imbl_value::json!({}))
                .await
                .unwrap(),
        )
        .unwrap();
        let mut socket = ctx
            .ws_continuation::<C>(subscription.guid.unwrap())
            .await
            .unwrap();
        let snapshot: crate::progress::FullProgress =
            crate::util::net::tungstenite::next_json(&mut socket)
                .await
                .unwrap();
        assert!(!snapshot.overall.is_complete());
    }
    check::<InitContext>(&ctx).await;
    check::<DiagnosticContext>(&ctx).await;
    check::<SetupContext>(&ctx).await;
    startup.progress.complete();
    assert!(ctx.id_key.get().is_none());
}

#[tokio::test]
async fn actual_registry_cli_dispatch_and_promotion_use_encrypted_admission() {
    let (_temp, registry, admin, _, _) = crate::registry::tests::fixture().await;
    let (url, _task) = server(crate::registry::registry_router(registry)).await;
    let (ctx, _identity) = cli(&url, admin);
    let result = <CliContext as CallRemote<RegistryContext>>::call_remote(
        &ctx,
        "registry.package.access.get",
        Default::default(),
        imbl_value::json!({"id":"secret"}),
        rpc_toolkit::Empty {},
    )
    .await
    .unwrap();
    assert_eq!(result["private"].as_bool(), Some(true));
    let result = crate::registry::package::promote::call_registry(
        &ctx,
        crate::registry::package::promote::registry_rpc_url(&url).unwrap(),
        "package.access.get",
        imbl_value::json!({"id":"secret"}),
    )
    .await
    .unwrap();
    assert_eq!(result["private"].as_bool(), Some(true));
}

#[tokio::test]
async fn registry_cli_websocket_keeps_the_configured_http_proxy() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (_temp, registry, admin, _, _) = crate::registry::tests::fixture().await;
    let (url, _task) = server(crate::registry::registry_router(registry)).await;
    let target = (url.host_str().unwrap().to_owned(), url.port().unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy: Url = format!("http://{}", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let (observed, request) = tokio::sync::oneshot::channel();
    let _proxy: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        let (mut downstream, _) = listener.accept().await.unwrap();
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            headers.push(downstream.read_u8().await.unwrap());
            assert!(headers.len() < 16 * 1024);
        }
        observed
            .send(String::from_utf8(headers.clone()).unwrap())
            .unwrap();
        let mut upstream = tokio::net::TcpStream::connect(target).await.unwrap();
        upstream.write_all(&headers).await.unwrap();
        let _ = tokio::io::copy_bidirectional(&mut downstream, &mut upstream).await;
    })
    .into();
    let ctx = CliContext::init(ClientConfig {
        id_key_path: Some(_temp.path().join("id.pem")),
        registry: Some(url.as_str().parse().unwrap()),
        proxy: Some(proxy),
        ..Default::default()
    })
    .unwrap();
    ctx.id_key.set(admin).unwrap();
    let response = <CliContext as CallRemote<RegistryContext>>::call_remote(
        &ctx,
        "registry.package.access.get",
        Default::default(),
        imbl_value::json!({"id":"secret"}),
        rpc_toolkit::Empty {},
    )
    .await
    .unwrap();
    assert_eq!(response["private"].as_bool(), Some(true));
    let headers = request.await.unwrap();
    assert!(headers.starts_with("GET http://"), "{headers}");
    assert!(headers.contains("/nested/rpc/v2 "), "{headers}");
    assert!(!headers.to_lowercase().contains("x-start-auth-sig"));
}

#[tokio::test]
async fn registry_cli_websocket_keeps_socks_remote_dns() {
    use fast_socks5::server::Socks5ServerProtocol;
    use fast_socks5::util::target_addr::TargetAddr;
    let (_temp, registry, admin, _, _) = crate::registry::tests::fixture().await;
    let (mut url, _task) = server(crate::registry::registry_router(registry)).await;
    let port = url.port().unwrap();
    url.set_host(Some("localhost")).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy: Url = format!("socks5h://{}", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let _proxy: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let (protocol, _, target) = Socks5ServerProtocol::accept_no_auth(socket)
            .await
            .unwrap()
            .read_command()
            .await
            .unwrap();
        assert_eq!(target, TargetAddr::Domain("localhost".into(), port));
        let mut downstream = protocol
            .reply_success("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let mut upstream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let _ = tokio::io::copy_bidirectional(&mut downstream, &mut upstream).await;
    })
    .into();
    let ctx = CliContext::init(ClientConfig {
        id_key_path: Some(_temp.path().join("id.pem")),
        registry: Some(url.as_str().parse().unwrap()),
        proxy: Some(proxy),
        ..Default::default()
    })
    .unwrap();
    ctx.id_key.set(admin).unwrap();
    let response = <CliContext as CallRemote<RegistryContext>>::call_remote(
        &ctx,
        "registry.package.access.get",
        Default::default(),
        imbl_value::json!({"id":"secret"}),
        rpc_toolkit::Empty {},
    )
    .await
    .unwrap();
    assert_eq!(response["private"].as_bool(), Some(true));
}

#[tokio::test]
async fn websocket_client_retains_ca_and_insecure_policy_without_altering_ordinary_http() {
    use axum::body::Body;
    use axum::response::IntoResponse;
    use hyper_util::rt::{TokioExecutor, TokioIo};
    use tower_service::Service;
    let (_temp, registry, admin, _, _) = crate::registry::tests::fixture().await;
    let (key, cert) = crate::net::tls::test::self_signed_for_loopback();
    let mut config = crate::net::tls::test::server_config(&key, &cert);
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url: Url = format!("https://{}/nested/", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let refuse = Arc::new(AtomicBool::new(false));
    let refusing = refuse.clone();
    let root = crate::registry::registry_router(registry);
    let app = Router::new()
        .merge(root.clone())
        .nest("/nested", root)
        .route(
            "/rpc/v0",
            post(move || {
                let observed = observed.clone();
                async move {
                    observed.fetch_add(1, Ordering::Relaxed);
                    axum::Json(serde_json::json!({"jsonrpc":"2.0","id":0,"result":false}))
                }
            }),
        )
        .route(
            "/legacy/rpc/v2/{guid}",
            axum::routing::get(|upgrade: axum::extract::ws::WebSocketUpgrade| async move {
                upgrade.on_upgrade(|mut socket| async move {
                    socket
                        .send(axum::extract::ws::Message::Text("legacy".into()))
                        .await
                        .unwrap();
                })
            }),
        )
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let refusing = refusing.clone();
                async move {
                    if request.uri().path() == RpcSessionUpgrader::PATH
                        && refusing.load(Ordering::Relaxed)
                    {
                        http::StatusCode::BAD_REQUEST.into_response()
                    } else {
                        next.run(request).await
                    }
                }
            },
        ));
    let _server: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let acceptor = acceptor.clone();
            let app = app.clone();
            connections.spawn(async move {
                let Ok(socket) = acceptor.accept(socket).await else {
                    return;
                };
                let service = hyper::service::service_fn(
                    move |request: http::Request<hyper::body::Incoming>| {
                        let mut app = app.clone();
                        async move { app.call(request.map(Body::new)).await }
                    },
                );
                let _ = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                    .serve_connection_with_upgrades(TokioIo::new(socket), service)
                    .await;
            });
        }
    })
    .into();
    let ca = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(ca.path(), cert.to_pem().unwrap()).unwrap();
    let (rejected, _identity) = cli(&url, admin.clone());
    assert!(
        crate::registry::package::promote::call_registry(
            &rejected,
            crate::registry::package::promote::registry_rpc_url(&url).unwrap(),
            "package.access.get",
            imbl_value::json!({"id":"secret"}),
        )
        .await
        .is_err()
    );
    let trusted_config = || ClientConfig {
        id_key_path: Some(_temp.path().join("id.pem")),
        root_ca: Some(vec![ca.path().to_owned()]),
        registry: Some(url.as_str().parse().unwrap()),
        tunnel: Some(
            format!("127.0.0.1:{}", url.port().unwrap())
                .parse()
                .unwrap(),
        ),
        ..Default::default()
    };
    let trusted = CliContext::init(trusted_config()).unwrap();
    trusted.id_key.set(admin.clone()).unwrap();
    let result = crate::registry::package::promote::call_registry(
        &trusted,
        crate::registry::package::promote::registry_rpc_url(&url).unwrap(),
        "package.access.get",
        imbl_value::json!({"id":"secret"}),
    )
    .await
    .unwrap();
    assert_eq!(result["private"].as_bool(), Some(true));
    let ordinary = trusted
        .client
        .post(crate::registry::package::promote::registry_rpc_url(&url).unwrap())
        .json(&serde_json::json!({"id":0,"method":"package.index","params":{}}))
        .send()
        .await
        .unwrap();
    assert!(ordinary.status().is_success());
    let dump = <CliContext as CallRemote<crate::tunnel::context::TunnelContext>>::call_remote(
        &trusted,
        "tunnel.db.dump",
        Default::default(),
        imbl_value::json!({}),
        rpc_toolkit::Empty {},
    )
    .await
    .unwrap();
    assert!(
        dump["value"]["index"]["package"]["packages"]
            .get("secret")
            .is_some()
    );
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    refuse.store(true, Ordering::Relaxed);
    let reused = <CliContext as CallRemote<crate::tunnel::context::TunnelContext>>::call_remote(
        &trusted,
        "tunnel.db.dump",
        Default::default(),
        imbl_value::json!({}),
        rpc_toolkit::Empty {},
    )
    .await
    .unwrap();
    assert!(
        reused["value"]["index"]["package"]["packages"]
            .get("secret")
            .is_some()
    );
    let fresh = CliContext::init(trusted_config()).unwrap();
    fresh.id_key.set(admin.clone()).unwrap();
    let rejected = <CliContext as CallRemote<crate::tunnel::context::TunnelContext>>::call_remote(
        &fresh,
        "tunnel.db.dump",
        Default::default(),
        imbl_value::json!({}),
        rpc_toolkit::Empty {},
    )
    .await;
    assert!(rejected.is_err());
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    let insecure = CliContext::init(ClientConfig {
        id_key_path: Some(_temp.path().join("id.pem")),
        insecure: true,
        ..Default::default()
    })
    .unwrap();
    insecure.id_key.set(admin).unwrap();
    let result = crate::registry::package::promote::call_registry(
        &insecure,
        crate::registry::package::promote::registry_rpc_url(&url).unwrap(),
        "package.access.get",
        imbl_value::json!({"id":"secret"}),
    )
    .await
    .unwrap();
    assert_eq!(result["private"].as_bool(), Some(true));
    let legacy_url = format!("https://localhost:{}/legacy/", url.port().unwrap());
    let legacy = CliContext::init(ClientConfig {
        host: Some(legacy_url.parse().unwrap()),
        id_key_path: Some(_temp.path().join("uninitialized-legacy-key")),
        insecure: true,
        ..Default::default()
    })
    .unwrap();
    let mut socket = legacy
        .ws_continuation::<crate::context::RpcContext>(crate::rpc_continuations::Guid::new())
        .await
        .unwrap();
    use futures::StreamExt;
    assert_eq!(
        socket.next().await.unwrap().unwrap().into_text().unwrap(),
        "legacy"
    );
}

#[tokio::test]
async fn secure_registry_refusal_never_sends_a_plaintext_rpc() {
    let (_temp, _, admin, _, _) = crate::registry::tests::fixture().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let app = Router::new().route(
        "/rpc/v0",
        post(move || {
            let observed = observed.clone();
            async move {
                observed.fetch_add(1, Ordering::Relaxed);
                axum::Json(serde_json::json!({"jsonrpc":"2.0","id":0,"result":null}))
            }
        }),
    );
    let (url, _task) = server(app).await;
    let (ctx, _identity) = cli(&url, admin);
    let result = crate::registry::package::promote::call_registry(
        &ctx,
        crate::registry::package::promote::registry_rpc_url(&url).unwrap(),
        "package.index",
        imbl_value::json!({}),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert!(
        !ctx.id_key_path
            .with_file_name("registry-identities")
            .exists()
    );
}

#[tokio::test]
#[ignore = "Requires GnuPG and START_CLI_BIN pointing to the built CLI"]
async fn actual_gpg_cli_authenticates_without_exporting_the_identity() {
    use std::os::unix::fs::PermissionsExt;

    use tokio::process::Command;

    struct Agent(std::path::PathBuf);
    impl Drop for Agent {
        fn drop(&mut self) {
            let _ = std::process::Command::new("gpgconf")
                .arg("--homedir")
                .arg(&self.0)
                .args(["--kill", "gpg-agent"])
                .output();
        }
    }
    let cli_bin = std::env::var("START_CLI_BIN").expect("build start-cli and set START_CLI_BIN");
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("gpg");
    std::fs::create_dir(&home).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    let _agent = Agent(home.clone());
    let uid = "Signed RPC CLI fixture <signed-rpc-cli@example.invalid>";
    let generated = Command::new("gpg")
        .arg("--homedir")
        .arg(&home)
        .args([
            "--batch",
            "--pinentry-mode",
            "loopback",
            "--passphrase",
            "",
            "--quick-generate-key",
            uid,
            "ed25519",
            "sign",
            "0",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let exported = Command::new("gpg")
        .arg("--homedir")
        .arg(&home)
        .args(["--batch", "--armor", "--export", uid])
        .output()
        .await
        .unwrap();
    assert!(exported.status.success());
    let public =
        crate::sign::gpg::parse_public_keys(&[String::from_utf8(exported.stdout).unwrap()])
            .unwrap()
            .keys
            .into_iter()
            .next()
            .unwrap();
    let registry =
        crate::registry::context::test_context(temp.path().join("registry"), public).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let app = crate::registry::registry_router(registry).route(
        "/rpc/v0",
        post(move || {
            let observed = observed.clone();
            async move {
                observed.fetch_add(1, Ordering::Relaxed);
                axum::Json(serde_json::json!({"jsonrpc":"2.0","id":0,"result":null}))
            }
        }),
    );
    let (url, _task) = server(app).await;
    let dns = Arc::new(crate::net::dns::DnsController::new(
        crate::util::sync::Watch::new(Default::default()),
    ));
    let startup =
        crate::context::InitContext::init(&crate::context::config::ServerConfig::default(), dns)
            .await
            .unwrap();
    let observed = calls.clone();
    let startup = crate::net::static_server::ui_router(startup).layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let observed = observed.clone();
            async move {
                if request.method() == http::Method::POST {
                    observed.fetch_add(1, Ordering::Relaxed);
                }
                next.run(request).await
            }
        },
    ));
    let (startup_url, _startup_task) = server(startup).await;
    let output = Command::new(&cli_bin)
        .current_dir(temp.path())
        .env("HOME", temp.path())
        .env("GNUPGHOME", &home)
        .args([
            "--id-key-gpg",
            uid,
            "--registry",
            url.as_str(),
            "registry",
            "admin",
            "list",
            "--format",
            "json",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result.as_object().unwrap().len(), 1);
    assert!(result.to_string().contains("Uploader"));
    let os = Command::new(&cli_bin)
        .current_dir(temp.path())
        .env("HOME", temp.path())
        .env("GNUPGHOME", &home)
        .args([
            "--id-key-gpg",
            uid,
            "--host",
            url.as_str(),
            "db",
            "dump",
            "--include-private",
            "--format",
            "json",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        os.status.success(),
        "{}",
        String::from_utf8_lossy(&os.stderr)
    );
    let dump: serde_json::Value = serde_json::from_slice(&os.stdout).unwrap();
    assert!(dump["value"]["index"].to_string().contains("Uploader"));
    let state = Command::new(&cli_bin)
        .current_dir(temp.path())
        .env("HOME", temp.path())
        .env("GNUPGHOME", &home)
        .args([
            "--id-key-gpg",
            "missing-key@example.invalid",
            "--host",
            startup_url.as_str(),
            "state",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        state.status.success(),
        "{}",
        String::from_utf8_lossy(&state.stderr)
    );
    assert!(
        String::from_utf8(state.stdout)
            .unwrap()
            .to_lowercase()
            .contains("initializing")
    );
    let failed = Command::new(cli_bin)
        .current_dir(temp.path())
        .env("HOME", temp.path())
        .env("GNUPGHOME", &home)
        .args([
            "--id-key-gpg",
            "missing-key@example.invalid",
            "--registry",
            url.as_str(),
            "registry",
            "admin",
            "list",
            "--format",
            "json",
        ])
        .output()
        .await
        .unwrap();
    assert!(!failed.status.success());
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn registry_cli_remembers_verified_peer_before_authorization_rejection() {
    let (_temp, registry, _admin, reader, _) = crate::registry::tests::fixture().await;
    let peer = registry.server_identity.verifying_key().await.unwrap();
    let (url, _server) = server(crate::registry::registry_router(registry)).await;
    let (ctx, _identity) = cli(&url, reader);
    let error = ctx
        .call_remote::<RegistryContext>("registry.admin.list", imbl_value::json!({}))
        .await
        .unwrap_err();
    assert_eq!(error.kind, crate::ErrorKind::Authorization);
    let path = std::fs::read_dir(ctx.id_key_path.with_file_name("registry-identities"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(
        std::fs::read_to_string(path)
            .unwrap()
            .parse::<crate::sign::AnyVerifyingKey>()
            .unwrap(),
        peer
    );
}

#[tokio::test]
#[ignore = "Requires START_CLI_BIN pointing to the built CLI"]
async fn actual_cli_tofu_preserves_stdout_and_exit_status_across_rotation_and_storage_errors() {
    use patch_db::ModelExt;
    use pkcs8::EncodePrivateKey;
    use tokio::process::Command;
    use tower_service::Service;
    let cli_bin = std::env::var("START_CLI_BIN").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let key = crate::sign::AnySigningKey::generate();
    let key_path = temp.path().join("id.pem");
    std::fs::write(
        &key_path,
        key.to_pkcs8_pem(Default::default()).unwrap().as_bytes(),
    )
    .unwrap();
    let signer: Signer = key.into();
    let public = signer.verifying_key().await.unwrap();
    let first =
        crate::registry::context::test_context(temp.path().join("first"), public.clone()).await;
    let second = crate::registry::context::test_context(temp.path().join("second"), public).await;
    second
        .db
        .put(&patch_db::json_ptr::ROOT, first.db.peek().await.as_value())
        .await
        .unwrap();
    let old = crate::sign::fingerprint::fingerprint(
        &first.server_identity.verifying_key().await.unwrap(),
    );
    let new = crate::sign::fingerprint::fingerprint(
        &second.server_identity.verifying_key().await.unwrap(),
    );
    let selected = Arc::new(tokio::sync::RwLock::new(first));
    let serving = selected.clone();
    let app = Router::new().fallback(move |request: axum::extract::Request| {
        let serving = serving.clone();
        async move {
            crate::registry::registry_router(serving.read().await.clone())
                .call(request)
                .await
                .unwrap()
        }
    });
    let (url, _server) = server(app).await;
    let command = || {
        let mut command = Command::new(&cli_bin);
        command
            .current_dir(temp.path())
            .env("HOME", temp.path())
            .args([
                "--id-key-path",
                key_path.to_str().unwrap(),
                "--registry",
                url.as_str(),
                "registry",
                "admin",
                "list",
                "--format",
                "json",
            ]);
        command
    };
    let initial = command().output().await.unwrap();
    assert!(
        initial.status.success(),
        "{}",
        String::from_utf8_lossy(&initial.stderr)
    );
    let _: serde_json::Value = serde_json::from_slice(&initial.stdout).unwrap();
    assert!(!String::from_utf8_lossy(&initial.stderr).contains("registry identity changed"));
    *selected.write().await = second;
    let rotated = command().output().await.unwrap();
    assert!(rotated.status.success());
    assert_eq!(rotated.stdout, initial.stdout);
    let warning = String::from_utf8(rotated.stderr).unwrap();
    assert!(warning.contains("registry identity changed"));
    assert!(warning.contains(&old) && warning.contains(&new));
    assert!(warning.contains(url.as_str().trim_end_matches('/')));
    let unchanged = command().output().await.unwrap();
    assert!(unchanged.status.success());
    assert_eq!(unchanged.stdout, initial.stdout);
    assert!(!String::from_utf8_lossy(&unchanged.stderr).contains("registry identity changed"));
    let directory = key_path.with_file_name("registry-identities");
    let path = std::fs::read_dir(&directory)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    let failed_storage = command().output().await.unwrap();
    assert!(failed_storage.status.success());
    assert_eq!(failed_storage.stdout, initial.stdout);
    assert!(String::from_utf8_lossy(&failed_storage.stderr).contains("could not remember"));
    let mut child = command()
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stderr.take());
    let closed_stderr = child.wait_with_output().await.unwrap();
    assert!(closed_stderr.status.success());
    assert_eq!(closed_stderr.stdout, initial.stdout);
}

#[tokio::test]
async fn missing_identity_keeps_public_http_but_malformed_identity_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let app = Router::new().route(
        "/rpc/v0",
        post(move |headers: HeaderMap| {
            let observed = observed.clone();
            async move {
                assert!(
                    headers
                        .get(crate::middleware::auth::signature::AUTH_SIG_HEADER)
                        .is_none()
                );
                observed.fetch_add(1, Ordering::Relaxed);
                axum::Json(serde_json::json!({"jsonrpc":"2.0","id":0,"result":"anonymous"}))
            }
        }),
    );
    let (url, _task) = server(app).await;
    let ctx = CliContext::init(ClientConfig {
        id_key_path: Some(temp.path().join("identity.pem")),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        ctx.id_key().unwrap_err().kind,
        crate::ErrorKind::Uninitialized
    );
    let endpoint = crate::registry::package::promote::registry_rpc_url(&url).unwrap();
    let result = crate::registry::context::call_cli(
        &ctx,
        endpoint.clone(),
        HeaderMap::new(),
        "127.0.0.1",
        "package.index",
        imbl_value::json!({}),
    )
    .await
    .unwrap();
    assert_eq!(result.as_str(), Some("anonymous"));
    std::fs::write(&ctx.id_key_path, "not a private key").unwrap();
    assert!(
        crate::registry::context::call_cli(
            &ctx,
            endpoint,
            HeaderMap::new(),
            "127.0.0.1",
            "package.index",
            imbl_value::json!({})
        )
        .await
        .is_err()
    );
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn local_bearer_keeps_the_http_and_exact_header_contract() {
    let (_temp, _, admin, _, _) = crate::registry::tests::fixture().await;
    let app = Router::new().route(
        "/rpc/v0",
        post(|headers: HeaderMap| async move {
            assert_eq!(
                headers.get(http::header::AUTHORIZATION).unwrap(),
                "Bearer local-fixture"
            );
            axum::Json(serde_json::json!({"jsonrpc":"2.0","id":0,"result":"local"}))
        }),
    );
    let (url, _task) = server(app).await;
    let (ctx, _identity) = cli(&url, admin);
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        "Bearer local-fixture".parse().unwrap(),
    );
    let result = crate::registry::context::call_cli(
        &ctx,
        crate::registry::package::promote::registry_rpc_url(&url).unwrap(),
        headers,
        "127.0.0.1",
        "package.index",
        imbl_value::json!({}),
    )
    .await
    .unwrap();
    assert_eq!(result.as_str(), Some("local"));
    assert!(
        !ctx.id_key_path
            .with_file_name("registry-identities")
            .exists()
    );
}
