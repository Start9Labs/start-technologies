use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::extract::ws::Message;
use futures::{SinkExt, StreamExt};

use super::*;
use crate::rpc_continuations::websocket_tests::{echo, serve};
use crate::rpc_continuations::{OpenAuthedContinuations, RpcContinuation, WebSocketPolicy};
use crate::sign::AnySigningKey;
use crate::util::net::secure::tests::raw_socket;

async fn scope() -> (SecureSession, Signer) {
    let signer: Signer = AnySigningKey::generate().into();
    let scope = SecureSession {
        peer: signer.verifying_key().await.unwrap(),
        address: "localhost".into(),
        server_identity: AnySigningKey::generate().into(),
    };
    (scope, signer)
}

async fn connect(url: &str, signer: &Signer) -> EncryptedSocket {
    EncryptedSocket::initiate(
        raw_socket(url).await,
        NoiseIdentity::new(signer).await.unwrap(),
        &prologue("localhost"),
        NonZeroUsize::new(64 * 1024 * 1024).unwrap(),
    )
    .await
    .unwrap()
    .1
}

#[tokio::test]
async fn secured_public_continuations_preserve_exact_plaintext_budget() {
    let (scope, signer) = scope().await;
    let (url, _server) = serve(RpcContinuation::ws(
        WebSocketPolicy::Public,
        Some(scope),
        echo,
        Duration::from_secs(30),
    ))
    .await;
    let mut socket = connect(&url, &signer).await;
    socket
        .send(Message::Binary(vec![255; 1024].into()))
        .await
        .unwrap();
    let echoed = socket.next().await.unwrap().unwrap();
    assert!(matches!(echoed, Message::Binary(ref data) if data.len() ==1024));
    socket
        .send(Message::Binary(vec![255; 1025].into()))
        .await
        .unwrap();
    assert!(!matches!(
        tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap(),
        Some(Ok(Message::Binary(_)))
    ));
}

#[tokio::test]
async fn secured_authenticated_continuations_preserve_text_and_binary_bulk() {
    let (scope, signer) = scope().await;
    let (url, _server) = serve(RpcContinuation::ws(
        WebSocketPolicy::Authenticated,
        Some(scope),
        echo,
        Duration::from_secs(30),
    ))
    .await;
    let mut socket = connect(&url, &signer).await;
    for message in [
        Message::Text("x".repeat(2 * 1024 * 1024).into()),
        Message::Binary(vec![255; 2 * 1024 * 1024].into()),
    ] {
        socket.send(message.clone()).await.unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
            message
        );
    }
}

#[tokio::test]
async fn revocation_cancels_the_pending_encrypted_handshake_before_callback() {
    let (scope, signer) = scope().await;
    let open = Arc::new(OpenAuthedContinuations::new());
    let called = Arc::new(AtomicBool::new(false));
    let observer = called.clone();
    let (url, _server) = serve(RpcContinuation::ws_authed(
        open.clone(),
        "client",
        WebSocketPolicy::Authenticated,
        Some(scope),
        move |_| async move {
            observer.store(true, Ordering::SeqCst);
        },
        Duration::from_secs(30),
    ))
    .await;
    let mut raw = raw_socket(&url).await;
    let identity = NoiseIdentity::new(&signer).await.unwrap();
    let mut state = identity.initiator(&prologue("localhost")).unwrap();
    let mut message = vec![0; 1024];
    let length = state.write_message(&[], &mut message).unwrap();
    let mut framed = (length as u16).to_be_bytes().to_vec();
    framed.extend_from_slice(&message[..length]);
    raw.send(Message::Binary(framed.into())).await.unwrap();
    assert!(matches!(raw.next().await, Some(Ok(Message::Binary(_)))));
    open.kill(&"client");
    assert!(!matches!(
        tokio::time::timeout(Duration::from_secs(3), raw.next())
            .await
            .unwrap(),
        Some(Ok(Message::Text(_)))
    ));
    assert!(!called.load(Ordering::SeqCst));
}
