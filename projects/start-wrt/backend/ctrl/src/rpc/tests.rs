use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::time::Duration;

use axum::extract::ws::Message;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use startos::middleware::auth::session::{prologue, session_url, RpcSessionUpgrader};
use startos::sign::noise::NoiseIdentity;
use startos::sign::{AnySigningKey, Signer};
use startos::util::future::NonDetachingJoinHandle;
use startos::util::net::secure::EncryptedSocket;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use url::Url;

pub(crate) async fn server() -> (
    Url,
    crate::ServerContext,
    NonDetachingJoinHandle<()>,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
) {
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
    let local_addr = listener.local_addr().unwrap();
    let app = super::router(served).layer(axum::middleware::from_fn(
        move |mut request: axum::extract::Request, next: axum::middleware::Next| {
            let observed = observed.clone();
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
    (url, ctx, task.into(), http_calls)
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
    let (url, _, _task, _) = server().await;
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
    let (url, ctx, _task, _) = server().await;
    let signer: Signer = AnySigningKey::generate().into();
    let identity = NoiseIdentity::new(&signer).await.unwrap();
    let mut url = session_url(url.join(RpcSessionUpgrader::PATH).unwrap(), "router.lan").unwrap();
    url.set_scheme("ws").unwrap();
    let mut request = url.as_str().into_client_request().unwrap();
    request
        .headers_mut()
        .insert("host", "router.lan".parse().unwrap());
    let (socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let (_, mut socket) = EncryptedSocket::initiate(
        startos::util::net::tungstenite::transport(socket),
        identity,
        &prologue("router.lan"),
        NonZeroUsize::new(1024 * 1024).unwrap(),
    )
    .await
    .unwrap();
    check(&call(&mut socket).await);
    use startos::middleware::auth::signature::SignatureAuthContext;
    let key = enroll(&ctx, &signer).await;
    let reply = call(&mut socket).await;
    assert_eq!(reply["result"]["current"], key.to_string(), "{reply}");
    assert_eq!(reply["meta"]["gitHash"], super::GIT_HASH);
    ctx.unenroll([key]).await.unwrap();
    check(&call(&mut socket).await);
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
