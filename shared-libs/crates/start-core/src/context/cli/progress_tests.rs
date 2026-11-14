use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::routing::get;
use clap::Parser;
use futures::{SinkExt, StreamExt};
use imbl_value::json;

use super::*;
use crate::middleware::auth::session::RpcSessionUpgrader;
use crate::progress::{FullProgress, with_cli_progress};
use crate::sign::session::Target;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::net::secure::EncryptedSocket;

#[tokio::test]
async fn progress_preserves_local_credential_precedence_session_reuse_and_terminal_errors() {
    let connections = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(AtomicUsize::new(0));
    let observed = connections.clone();
    let received = requests.clone();
    let key: Signer = AnySigningKey::generate().into();
    let app = axum::Router::new().route(RpcSessionUpgrader::PATH, get(move |upgrade: WebSocketUpgrade, headers: HeaderMap| {
        assert_eq!(headers.get(AUTHORIZATION).unwrap(), "Bearer local-credential");
        observed.fetch_add(1, Ordering::SeqCst);
        let key = key.clone();
        let received = received.clone();
        async move { upgrade.on_upgrade(move |socket| async move {
            let (_, _, mut socket) = EncryptedSocket::accept(Box::new(socket), &key, &["127.0.0.1".into()], NonZeroUsize::new(1024 * 1024).unwrap(), &Target::Rpc, None, |_| async { Ok(()) }).await.unwrap();
            for success in [true, false] {
                let text = socket.next().await.unwrap().unwrap().into_text().unwrap();
                let request: serde_json::Value = serde_json::from_str(&text).unwrap();
                received.fetch_add(1, Ordering::SeqCst);
                assert_eq!(request["rpcstream"], rpc_toolkit::RPCSTREAM_VERSION);
                let id = &request["id"];
                let mut snapshot = FullProgress::new();
                snapshot.set_complete();
                socket.send(Message::Text(serde_json::json!({"jsonrpc":"2.0","id":id,"result":null}).to_string().into())).await.unwrap();
                socket.send(Message::Text(serde_json::json!({"rpcstream":rpc_toolkit::RPCSTREAM_VERSION,"id":id,"result":snapshot,"end":false}).to_string().into())).await.unwrap();
                let terminal = if success {
                    serde_json::json!({"rpcstream":rpc_toolkit::RPCSTREAM_VERSION,"id":id,"result":true,"end":true})
                } else {
                    serde_json::json!({"rpcstream":rpc_toolkit::RPCSTREAM_VERSION,"id":id,"error":{"code":34,"message":"operation rejected"},"end":true})
                };
                socket.send(Message::Text(terminal.to_string().into())).await.unwrap();
            }
        }) }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let _server: NonDetachingJoinHandle<()> =
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }).into();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("malformed-key.pem");
    tokio::fs::write(&path, "not a private key").await.unwrap();
    let context = CliContext::init(
        ClientConfig::try_parse_from([
            "test",
            "-H",
            &host,
            "--id-key-path",
            path.to_str().unwrap(),
        ])
        .unwrap(),
    )
    .unwrap();
    assert!(
        context
            .stream_rpc::<FullProgress, bool>(
                context.rpc_url().unwrap(),
                HeaderMap::new(),
                "progress",
                json!({})
            )
            .await
            .is_err()
    );
    assert_eq!(connections.load(Ordering::SeqCst), 0);
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, "Bearer local-credential".parse().unwrap());
    let response = context
        .stream_rpc::<FullProgress, bool>(
            context.rpc_url().unwrap(),
            headers.clone(),
            "progress",
            json!({}),
        )
        .await
        .unwrap();
    assert!(with_cli_progress(response.items, "Test").await.unwrap());
    let response = context
        .stream_rpc::<FullProgress, bool>(
            context.rpc_url().unwrap(),
            headers,
            "progress",
            json!({}),
        )
        .await
        .unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        with_cli_progress(response.items, "Test"),
    )
    .await
    .unwrap();
    assert_eq!(result.unwrap_err().kind, ErrorKind::Authorization);
    assert_eq!(connections.load(Ordering::SeqCst), 1);
    assert_eq!(requests.load(Ordering::SeqCst), 2);
    assert!(context.id_key.get().is_none());
    assert_eq!(
        tokio::fs::read_to_string(path).await.unwrap(),
        "not a private key"
    );
}
