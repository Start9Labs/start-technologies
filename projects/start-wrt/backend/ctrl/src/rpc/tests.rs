use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::time::Duration;

use axum::extract::ws::Message;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use startos::middleware::auth::session::RpcSessionUpgrader;
use startos::sign::session::{PreparedClient, Target};
use startos::sign::{AnySigningKey, Signer};
use startos::util::future::NonDetachingJoinHandle;
use startos::util::net::secure::EncryptedSocket;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use url::Url;

type TestServer = (
    Url,
    crate::ServerContext,
    NonDetachingJoinHandle<()>,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
);

pub(crate) async fn server() -> TestServer {
    server_with_router(super::router).await
}

async fn server_with_router(
    router: impl FnOnce(crate::ServerContext) -> axum::Router,
) -> TestServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let ctx = crate::ServerContext::default();
    use startos::middleware::auth::DbContext;
    ctx.db()
        .put(
            &patch_db::json_ptr::ROOT,
            &crate::auth::AuthDatabase::default(),
        )
        .await
        .unwrap();
    let served = ctx.clone();
    let http_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = http_calls.clone();
    let upgrades = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let upgraded = upgrades.clone();
    let local_addr = listener.local_addr().unwrap();
    let app = router(served).layer(axum::middleware::from_fn(
        move |mut request: axum::extract::Request, next: axum::middleware::Next| {
            let observed = observed.clone();
            let upgraded = upgraded.clone();
            async move {
                let peer_addr = request
                    .extensions()
                    .get::<axum::extract::ConnectInfo<SocketAddr>>()
                    .unwrap()
                    .0;
                request
                    .extensions_mut()
                    .insert(startos::net::web_server::TcpMetadata {
                        peer_addr,
                        local_addr,
                    });
                if request.uri().path() == "/rpc/v1" {
                    observed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                if request.uri().path() == RpcSessionUpgrader::PATH {
                    upgraded.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                next.run(request).await
            }
        },
    ));
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (url, ctx, task.into(), http_calls, upgrades)
}

pub(crate) async fn enroll(
    ctx: &crate::ServerContext,
    signer: &Signer,
) -> imbl_value::InternedString {
    use startos::middleware::auth::DbContext;
    let key = signer.verifying_key().await.unwrap().interned_der64();
    let now = chrono::Utc::now();
    ctx.db()
        .put(
            &patch_db::json_ptr::ROOT,
            &crate::auth::AuthDatabase {
                session_pubkeys: startos::auth::AuthKeys(
                    [(
                        key.clone(),
                        startos::auth::Session {
                            name: None,
                            logged_in: now,
                            last_active: now,
                            user_agent: None,
                        },
                    )]
                    .into(),
                ),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    key
}

fn check(reply: &Value) {
    assert_eq!(reply["id"], 42);
    assert_eq!(reply["error"]["code"], 34, "{reply}");
    assert_eq!(reply["meta"]["gitHash"], super::GIT_HASH);
}

#[tokio::test]
async fn http_retains_firmware_header_and_authentication_errors() {
    let (url, _, _task, _, _) = server().await;
    let response = reqwest::Client::new()
        .post(url.join("rpc/v1").unwrap())
        .json(&json!({"jsonrpc":"2.0", "id":42, "method":"auth.session.list", "params":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.headers().get(super::GIT_HASH_HEADER).unwrap(),
        super::GIT_HASH
    );
    check(&response.json::<Value>().await.unwrap());
}

#[tokio::test]
async fn encrypted_rpc_uses_live_auth_and_authenticated_firmware_metadata() {
    let (url, ctx, _task, _, _) = server().await;
    let signer: Signer = AnySigningKey::generate().into();
    let prepared = PreparedClient::new(&signer, "router.lan", Target::Rpc)
        .await
        .unwrap();
    let mut url = url.join(RpcSessionUpgrader::PATH).unwrap();
    url.set_scheme("ws").unwrap();
    let mut request = url.as_str().into_client_request().unwrap();
    request
        .headers_mut()
        .insert("host", "router.lan".parse().unwrap());
    let (socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let (_, mut socket) = EncryptedSocket::initiate(
        startos::util::net::tungstenite::transport(socket),
        prepared,
        NonZeroUsize::new(1024 * 1024).unwrap(),
    )
    .await
    .unwrap();
    check(&call(&mut socket).await);
    use startos::middleware::auth::signature::SignatureAuthorizationContext;
    let key = enroll(&ctx, &signer).await;
    let reply = call(&mut socket).await;
    assert_eq!(reply["result"]["current"], key.to_string(), "{reply}");
    assert_eq!(reply["meta"]["gitHash"], super::GIT_HASH);
    ctx.unenroll([key]).await.unwrap();
    check(&call(&mut socket).await);
}

#[tokio::test]
async fn startup_passwords_use_actual_admission_and_retain_http_jwe_without_enrollment() {
    use rpc_toolkit::{from_fn_async, HandlerExt, Server};
    use startos::middleware::auth::session::client::SessionClient;
    use startos::middleware::auth::Auth;
    use startos::util::crypto::EncryptedWire;

    let (url, ctx, _task, _, upgrades) = server_with_router(|ctx| {
        let api =
                crate::main_api::<crate::ServerContext>().subcommand(
                    "decode",
                    from_fn_async(
                        |ctx: crate::ServerContext,
                         params: crate::auth::SetInitialPasswordParams| async move {
                            crate::auth::decrypt_password(&ctx, params.password)
                        },
                    )
                    .with_metadata("authenticated", imbl_value::Value::Bool(false))
                    .no_cli()
                    .no_ts(),
                );
        let server = Server::new(move || std::future::ready(Ok(ctx.clone())), api)
            .middleware(Auth::new().with_local_auth().with_signature_auth());
        axum::Router::new()
            .route("/rpc/v1", axum::routing::post(server.clone()))
            .route(RpcSessionUpgrader::PATH, RpcSessionUpgrader::route(server))
    })
    .await;
    let network = reqwest::Client::new();
    let client = SessionClient::new(network.clone());
    let signer = client.ephemeral_signer();
    let password = "  fixture password\n";
    let response = client
        .call(
            signer,
            url.join(RpcSessionUpgrader::PATH).unwrap(),
            "router.lan",
            Default::default(),
            "decode",
            imbl_value::json!({"password":password}),
        )
        .await
        .unwrap();
    assert_eq!(response.as_str(), Some(password));
    let rejected = client
        .call(
            signer,
            url.join(RpcSessionUpgrader::PATH).unwrap(),
            "router.lan",
            Default::default(),
            "auth.session.list",
            imbl_value::json!({}),
        )
        .await
        .unwrap_err();
    assert_eq!(rejected.code, 34);
    assert_eq!(upgrades.load(std::sync::atomic::Ordering::Relaxed), 1);

    let http = |password: serde_json::Value| {
        network
            .post(url.join("rpc/v1").unwrap())
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"decode","params":{"password":password}}))
    };
    let rejected: Value = http(json!(password))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        rejected["error"]["code"],
        startos::ErrorKind::InvalidRequest as i32
    );
    let key = <crate::ServerContext as AsRef<josekit::jwk::Jwk>>::as_ref(&ctx)
        .to_public_key()
        .unwrap();
    let wire = EncryptedWire::encrypt(password, &key).unwrap();
    let response: Value = http(serde_json::to_value(wire).unwrap())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(response["result"], password);
    let other = josekit::jwk::Jwk::generate_ec_key(josekit::jwk::alg::ec::EcCurve::P256).unwrap();
    let bad = EncryptedWire::encrypt(password, &other.to_public_key().unwrap()).unwrap();
    let rejected: Value = http(serde_json::to_value(bad).unwrap())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        rejected["error"]["code"],
        startos::ErrorKind::InvalidRequest as i32
    );
}

#[tokio::test]
async fn flash_uses_typed_terminals_and_the_same_gate_as_legacy_ndjson() {
    use startos::middleware::auth::session::client::SessionClient;
    use startos::util::crypto::EncryptedWire;
    let (url, ctx, _task, _, _) = server_with_router(|ctx| {
        super::router(ctx.clone())
            .route(
                "/api/setup/flash",
                axum::routing::post(crate::bins::daemon::setup_flash_handler),
            )
            .layer(axum::Extension(ctx))
    })
    .await;
    let network = reqwest::Client::new();
    let client = SessionClient::new(network.clone());
    let signer = client.ephemeral_signer();
    let parameters = imbl_value::json!({"mode":"fresh-start","password":"short"});
    let start = || {
        client.stream::<crate::setup::SetupEvent, ()>(
            signer,
            url.join(RpcSessionUpgrader::PATH).unwrap(),
            "router.lan",
            Default::default(),
            "setup.flash",
            parameters.clone(),
        )
    };
    let mut operation = start().await.unwrap();
    assert!(operation.initial.is_null());
    assert!(matches!(
        operation.items.next().await.unwrap(),
        Some(crate::setup::SetupEvent::Status { step: 1, .. })
    ));
    assert_eq!(
        operation.items.finish().await.unwrap_err().code,
        crate::error::ErrorKind::InvalidRequest as i32
    );

    let key = <crate::ServerContext as AsRef<josekit::jwk::Jwk>>::as_ref(&ctx)
        .to_public_key()
        .unwrap();
    let wire = EncryptedWire::encrypt("short", &key).unwrap();
    let legacy = || {
        network
            .post(url.join("api/setup/flash").unwrap())
            .json(&json!({"mode":"fresh-start","password":wire}))
    };
    let guard = ctx.setup_flash.clone().lock_owned().await;
    let rejected = start().await.err().unwrap();
    assert!(rejected.to_string().contains("flash already in progress"));
    let busy = legacy().send().await.unwrap().text().await.unwrap();
    assert!(busy.contains("flash already in progress"));
    drop(guard);
    let response = legacy().send().await.unwrap();
    assert_eq!(response.headers()["content-type"], "application/x-ndjson");
    let events = response
        .text()
        .await
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["phase"], "status");
    assert_eq!(events[1]["phase"], "error");
    assert!(events[1]["message"].as_str().unwrap().contains("12"));
    let rejected = network
        .post(url.join("api/setup/flash").unwrap())
        .json(&json!({"mode":"fresh-start","password":"short"}))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let error: Value = serde_json::from_str(&rejected).unwrap();
    assert_eq!(error["phase"], "error");
}

async fn call(socket: &mut EncryptedSocket) -> Value {
    socket
        .send(Message::Text(
            json!({"jsonrpc":"2.0", "id":42, "method":"auth.session.list", "params":{}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let Message::Text(response) = response else {
        panic!("expected encrypted RPC text")
    };
    serde_json::from_str(&response).unwrap()
}
