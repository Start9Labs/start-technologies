use std::num::NonZeroUsize;
use std::time::Duration;

use axum::extract::ws::Message;
use axum::http::StatusCode;
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as WireMessage;
use url::Url;

use super::session_rpc_router;
use crate::middleware::auth::session::RpcSessionUpgrader;
use crate::registry::context::RegistryContext;
use crate::registry::registry_server;
use crate::sign::Signer;
use crate::sign::session::{PreparedClient, Target};
use crate::util::future::NonDetachingJoinHandle;
use crate::util::net::secure::EncryptedSocket;
use crate::util::net::secure::tests::raw_socket;

async fn server(ctx: RegistryContext) -> (Url, NonDetachingJoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let rpc = registry_server(ctx.clone());
    let app = session_rpc_router(
        ctx,
        rpc.clone(),
        axum::routing::any(rpc).layer(crate::middleware::cors::layer()),
    );
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }).into();
    (url, task)
}

#[tokio::test]
async fn session_router_accepts_http2_extended_connect() {
    use axum::body::{Body, Bytes};
    use http_body_util::Empty;
    use hyper_util::rt::{TokioExecutor, TokioIo};
    use tower_service::Service;

    let (_temp, ctx, _, _, _) = crate::registry::tests::fixture().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let rpc = registry_server(ctx.clone());
    let app = session_rpc_router(
        ctx,
        rpc.clone(),
        axum::routing::any(rpc).layer(crate::middleware::cors::layer()),
    );
    let _server: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let service =
            hyper::service::service_fn(move |request: http::Request<hyper::body::Incoming>| {
                let mut app = app.clone();
                async move { app.call(request.map(Body::new)).await }
            });
        hyper::server::conn::http2::Builder::new(TokioExecutor::new())
            .enable_connect_protocol()
            .serve_connection(TokioIo::new(socket), service)
            .await
            .unwrap();
    })
    .into();
    let socket = tokio::net::TcpStream::connect(address).await.unwrap();
    let (mut sender, connection) =
        hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(socket))
            .await
            .unwrap();
    let _client: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        let _ = connection.await;
    })
    .into();
    // Wait for peer SETTINGS before requesting extended CONNECT.
    sender
        .send_request(
            http::Request::builder()
                .uri(format!("http://{address}/"))
                .body(Empty::<Bytes>::new())
                .unwrap(),
        )
        .await
        .unwrap();
    sender.ready().await.unwrap();
    let mut request = http::Request::builder()
        .method(http::Method::CONNECT)
        .version(http::Version::HTTP_2)
        .uri(format!("http://{address}{}", RpcSessionUpgrader::PATH))
        .header("sec-websocket-version", "13")
        .body(Empty::<Bytes>::new())
        .unwrap();
    request
        .extensions_mut()
        .insert(hyper::ext::Protocol::from_static("websocket"));
    let response = tokio::time::timeout(Duration::from_secs(3), sender.send_request(request))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.status(), http::StatusCode::OK);
}

async fn encrypted(url: &Url, path: &str, signer: &Signer) -> EncryptedSocket {
    let mut url = url.join(path).unwrap();
    url.set_scheme("ws").unwrap();
    let target = if path == RpcSessionUpgrader::PATH {
        Target::Rpc
    } else {
        Target::Continuation(path.rsplit('/').next().unwrap().parse().unwrap())
    };
    let prepared = PreparedClient::new(signer, "127.0.0.1", target)
        .await
        .unwrap();
    EncryptedSocket::initiate(
        raw_socket(url.as_str()).await,
        prepared,
        NonZeroUsize::new(64 * 1024 * 1024).unwrap(),
    )
    .await
    .unwrap()
    .1
}

async fn receive(socket: &mut EncryptedSocket) -> serde_json::Value {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match socket.next().await.unwrap().unwrap() {
                Message::Text(text) => return serde_json::from_str(&text).unwrap(),
                Message::Ping(_) => socket.flush().await.unwrap(),
                other => panic!("unexpected message {other:?}"),
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn session_router_admits_scoped_rpc_and_its_same_peer_continuation() {
    let (_temp, ctx, admin, _, _) = crate::registry::tests::fixture().await;
    let (url, _task) = server(ctx.clone()).await;
    let mut socket = encrypted(&url, RpcSessionUpgrader::PATH, &admin).await;
    socket
        .send(Message::Text(
            serde_json::json!({"jsonrpc":"2.0", "id":7,
        "method":"package.access.get", "params":{"id":"secret"}})
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let reply = receive(&mut socket).await;
    assert_eq!(reply["id"], 7);
    assert_eq!(reply["result"]["private"], true);
    socket
        .send(Message::Text(
            serde_json::json!({"jsonrpc":"2.0", "id":8,
        "method":"db.subscribe", "params":{"pointer":"/index"}})
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let reply = receive(&mut socket).await;
    let guid = reply["result"]["guid"].as_str().unwrap();
    let mut changes = encrypted(
        &url,
        &format!("{}/{guid}", RpcSessionUpgrader::PATH),
        &admin,
    )
    .await;
    ctx.db
        .mutate(|db| {
            db.as_index_mut()
                .as_name_mut()
                .ser(&Some("changed".to_owned()))
        })
        .await
        .result
        .unwrap();
    assert!(receive(&mut changes).await.get("patch").is_some());
}

#[tokio::test]
async fn session_router_keeps_legacy_http_and_plaintext_guid_routes() {
    let (_temp, ctx, _, _, _) = crate::registry::tests::fixture().await;
    let (url, _task) = server(ctx.clone()).await;
    assert_eq!(RpcSessionUpgrader::PATH, "/rpc/v2");
    let mut unversioned = url.join("ws/rpc").unwrap();
    unversioned.set_scheme("ws").unwrap();
    let error = tokio_tungstenite::connect_async(unversioned.as_str())
        .await
        .unwrap_err();
    assert!(
        matches!(error, tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == StatusCode::NOT_FOUND)
    );
    for path in [
        RpcSessionUpgrader::PATH,
        crate::rpc_continuations::RpcContinuations::LEGACY_WS_PATH,
    ] {
        let reply: serde_json::Value = reqwest::Client::new()
            .post(url.join("rpc/v1").unwrap())
            .json(&serde_json::json!({"method":"db.subscribe", "params":{"pointer":"/index"}}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let guid = reply["result"]["guid"].as_str().unwrap();
        let mut url = url.join(&format!("{path}/{guid}")).unwrap();
        url.set_scheme("ws").unwrap();
        let (mut changes, _) = tokio_tungstenite::connect_async(url.as_str())
            .await
            .unwrap();
        ctx.db
            .mutate(|db| {
                db.as_index_mut()
                    .as_name_mut()
                    .ser(&Some(format!("legacy{path}")))
            })
            .await
            .result
            .unwrap();
        let message = tokio::time::timeout(Duration::from_secs(3), changes.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let WireMessage::Text(text) = message else {
            panic!("expected plaintext revision")
        };
        assert!(
            serde_json::from_str::<serde_json::Value>(&text)
                .unwrap()
                .get("patch")
                .is_some()
        );
    }
}
