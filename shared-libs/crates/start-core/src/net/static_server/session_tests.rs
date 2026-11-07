use std::num::NonZeroUsize;
use std::time::Duration;

use axum::extract::ws::Message;
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as WireMessage;
use url::Url;

use super::session_rpc_router;
use crate::middleware::auth::session::{RpcSessionUpgrader, prologue, session_url};
use crate::registry::context::RegistryContext;
use crate::registry::registry_server;
use crate::sign::Signer;
use crate::sign::noise::NoiseIdentity;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::net::secure::EncryptedSocket;
use crate::util::net::secure::tests::raw_socket;

async fn server(ctx: RegistryContext) -> (Url, NonDetachingJoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let app = session_rpc_router(ctx.clone(), registry_server(ctx));
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }).into();
    (url, task)
}

async fn encrypted(url: &Url, path: &str, signer: &Signer) -> EncryptedSocket {
    let mut url = url.join(path).unwrap();
    url.set_scheme("ws").unwrap();
    let url = session_url(url, "127.0.0.1").unwrap();
    let identity = NoiseIdentity::new(signer).await.unwrap();
    EncryptedSocket::initiate(
        raw_socket(url.as_str()).await,
        identity,
        &prologue("127.0.0.1"),
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
    let mut url = url
        .join(&format!("{}/{guid}", RpcSessionUpgrader::PATH))
        .unwrap();
    url.set_scheme("ws").unwrap();
    let (mut changes, _) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .unwrap();
    ctx.db
        .mutate(|db| {
            db.as_index_mut()
                .as_name_mut()
                .ser(&Some("legacy".to_owned()))
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
