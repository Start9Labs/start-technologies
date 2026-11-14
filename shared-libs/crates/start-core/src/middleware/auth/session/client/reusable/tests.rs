use std::num::NonZeroUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::Router;
use axum::extract::ws::WebSocketUpgrade;
use axum::routing::get;

use super::*;

mod cancellation;
mod streaming;
use crate::middleware::auth::session::RpcSessionUpgrader;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::net::secure::EncryptedSocket;

#[tokio::test]
async fn transport_loss_never_replays_a_sent_call_and_the_next_call_reconnects_with_an_idle_reader()
{
    let connections = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(AtomicUsize::new(0));
    let pong = Arc::new(tokio::sync::Notify::new());
    let dropped = Arc::new(tokio::sync::Notify::new());
    let server_key: Signer = crate::sign::AnySigningKey::generate().into();
    let observed = connections.clone();
    let seen = requests.clone();
    let pong_seen = pong.clone();
    let drop_seen = dropped.clone();
    let app = Router::new().route(RpcSessionUpgrader::PATH, get(move |upgrade: WebSocketUpgrade| {
        let key = server_key.clone();
        let index = observed.fetch_add(1, Ordering::SeqCst);
        let seen = seen.clone();
        let pong = pong_seen.clone();
        let dropped = drop_seen.clone();
        async move { upgrade.on_upgrade(move |socket| async move {
            let (_, _, mut socket) = EncryptedSocket::accept(
                Box::new(socket), &key, &["server.test".into()], NonZeroUsize::new(1024 * 1024).unwrap(),
                &Target::Rpc, None, |_| async { Ok(()) },
            ).await.unwrap();
            let body = socket.next().await.unwrap().unwrap().into_text().unwrap();
            let request: serde_json::Value = serde_json::from_str(&body).unwrap();
            seen.fetch_add(1, Ordering::SeqCst);
            if index == 0 { return }
            socket.send(Message::Text(serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":"second"}).to_string().into())).await.unwrap();
            socket.send(Message::Ping(b"idle".as_slice().into())).await.unwrap();
            while let Some(message) = socket.next().await {
                match message {
                    Ok(Message::Pong(payload)) if payload.as_ref() == b"idle" => pong.notify_one(),
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => (),
                }
            }
            dropped.notify_one();
        }) }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url: Url = format!(
        "ws://{}{path}",
        listener.local_addr().unwrap(),
        path = RpcSessionUpgrader::PATH
    )
    .parse()
    .unwrap();
    let _server: NonDetachingJoinHandle<()> =
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }).into();
    let network = reqwest::Client::builder()
        .http1_only()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let client = Client::new(network);
    let key: Signer = crate::sign::AnySigningKey::generate().into();
    assert_eq!(connections.load(Ordering::SeqCst), 0);
    let first = tokio::time::timeout(
        Duration::from_secs(3),
        client.call(
            &key,
            url.clone(),
            "server.test",
            HeaderMap::new(),
            "side-effect",
            imbl_value::json!({}),
        ),
    )
    .await
    .unwrap();
    assert!(first.is_err());
    assert_eq!(connections.load(Ordering::SeqCst), 1);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    assert_eq!(
        client
            .call(
                &key,
                url,
                "server.test",
                HeaderMap::new(),
                "next",
                imbl_value::json!({})
            )
            .await
            .unwrap()
            .as_str(),
        Some("second")
    );
    tokio::time::timeout(Duration::from_secs(3), pong.notified())
        .await
        .unwrap();
    assert_eq!(connections.load(Ordering::SeqCst), 2);
    assert_eq!(requests.load(Ordering::SeqCst), 2);
    drop(client);
    tokio::time::timeout(Duration::from_secs(3), dropped.notified())
        .await
        .unwrap();
}
