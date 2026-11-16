use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::extract::ws::Message;
use futures::{SinkExt, StreamExt};

use super::*;
use crate::rpc_continuations::websocket_tests::{echo, serve};
use crate::rpc_continuations::{OpenAuthedContinuations, RpcContinuation, WebSocketPolicy};
use crate::sign::AnySigningKey;
use crate::util::net::secure::tests::raw_socket;

struct ScopeOwner {
    _context: crate::registry::context::RegistryContext,
    _directory: tempfile::TempDir,
}

async fn scope() -> (SecureSession, Signer, ScopeOwner) {
    let signer: Signer = AnySigningKey::generate().into();
    let directory = tempfile::tempdir().unwrap();
    let peer = signer.verifying_key().await.unwrap();
    let context =
        crate::registry::context::test_context(directory.path().into(), peer.clone()).await;
    let scope = SecureSession {
        peer,
        address: "localhost".into(),
        server_identity: context.server_identity().unwrap(),
        replay_check: context.replay_check(),
    };
    (
        scope,
        signer,
        ScopeOwner {
            _directory: directory,
            _context: context,
        },
    )
}

async fn connect(url: &str, signer: &Signer) -> EncryptedSocket {
    let guid = Url::parse(url)
        .unwrap()
        .path()
        .rsplit('/')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let prepared =
        crate::sign::session::PreparedClient::new(signer, "localhost", Target::Continuation(guid))
            .await
            .unwrap();
    EncryptedSocket::initiate(
        raw_socket(url).await,
        prepared,
        NonZeroUsize::new(64 * 1024 * 1024).unwrap(),
    )
    .await
    .unwrap()
    .1
}

#[tokio::test]
async fn secured_public_continuations_preserve_exact_plaintext_budget() {
    let (scope, signer, _owner) = scope().await;
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
    let (scope, signer, _owner) = scope().await;
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
    let (scope, _signer, _owner) = scope().await;
    let open = Arc::new(OpenAuthedContinuations::new());
    let called = Arc::new(AtomicBool::new(false));
    let observer = called.clone();
    let (url, _server) = serve(RpcContinuation::ws_authed(
        open.subscribe_to_kill("client"),
        WebSocketPolicy::Authenticated,
        Some(scope),
        move |_| async move {
            observer.store(true, Ordering::SeqCst);
        },
        Duration::from_secs(30),
    ))
    .await;
    let mut raw = raw_socket(&url).await;
    raw.send(Message::Binary(vec![0].into())).await.unwrap();
    open.kill(&"client");
    assert!(!matches!(
        tokio::time::timeout(Duration::from_secs(3), raw.next())
            .await
            .unwrap(),
        Some(Ok(Message::Text(_)))
    ));
    assert!(!called.load(Ordering::SeqCst));
}
