use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::routing::get;
use futures::FutureExt;
use futures::future::BoxFuture;
use rpc_toolkit::{
    Context as RpcContext, Empty, HandlerArgs, HandlerExt, ParentHandler, Server,
    UpgradedWebSocket, WebSocketOptions, WebSocketUpgrader,
};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message as ClientMessage;

use super::*;
use crate::sign::{AnySigningKey, Signer};

const PROLOGUE: &[u8] = b"Start-RPC-Noise test v1\0";

fn signer(seed: u8) -> Signer {
    AnySigningKey::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[seed; 32])).into()
}

async fn states() -> (TransportState, TransportState) {
    let a = NoiseIdentity::new(&signer(41)).await.unwrap();
    let b = NoiseIdentity::new(&signer(42)).await.unwrap();
    let mut client = a.initiator(PROLOGUE).unwrap();
    let mut server = b.responder(PROLOGUE).unwrap();
    let mut wire = vec![0; MAX_RECORD];
    let mut payload = vec![0; MAX_RECORD];
    let length = client.write_message(&[], &mut wire).unwrap();
    server.read_message(&wire[..length], &mut payload).unwrap();
    let length = server.write_message(b.certificate(), &mut wire).unwrap();
    let decoded = client.read_message(&wire[..length], &mut payload).unwrap();
    authenticate_peer(&client, &payload[..decoded]).unwrap();
    let length = client.write_message(a.certificate(), &mut wire).unwrap();
    let decoded = server.read_message(&wire[..length], &mut payload).unwrap();
    authenticate_peer(&server, &payload[..decoded]).unwrap();
    (
        client.into_transport_mode().unwrap(),
        server.into_transport_mode().unwrap(),
    )
}

async fn codecs(incoming: usize) -> (NoiseCodec, NoiseCodec) {
    let (a, b) = states().await;
    let limit = NonZeroUsize::new(incoming).unwrap();
    (
        NoiseCodec::new(a, limit, BytesMut::new()),
        NoiseCodec::new(b, limit, BytesMut::new()),
    )
}

fn wire_bytes(messages: VecDeque<Message>) -> Bytes {
    let mut bytes = BytesMut::new();
    for message in messages {
        let Message::Binary(data) = message else {
            panic!("expected ciphertext")
        };
        assert!(data.len() <= MAX_RECORD + RECORD_HEADER);
        bytes.extend_from_slice(&data);
    }
    bytes.freeze()
}

fn encrypted_plaintext(sender: &mut NoiseCodec, plaintext: &[u8]) -> Bytes {
    let mut wire = vec![0; MAX_RECORD];
    let length = sender.state.write_message(plaintext, &mut wire).unwrap();
    let mut output = BytesMut::new();
    sender
        .record_writer
        .encode(Bytes::copy_from_slice(&wire[..length]), &mut output)
        .unwrap();
    output.freeze()
}

#[tokio::test]
async fn text_binary_fragmentation_and_message_boundaries_are_preserved() {
    let (mut sender, mut receiver) = codecs(1024).await;
    let text = Message::Text(
        "{\"id\":18446744073709551615,\"numbers\":[1,1.0,-0,1e+21],\"text\":\"雪\"}".into(),
    );
    let binary = Message::Binary(Bytes::from_static(&[0, 255, 13, 10]));
    let mut wire = BytesMut::new();
    wire.extend_from_slice(&wire_bytes(sender.seal(text.clone()).unwrap()));
    wire.extend_from_slice(&wire_bytes(sender.seal(binary.clone()).unwrap()));
    let mut decoded = Vec::new();
    for byte in wire {
        receiver.feed(&[byte]);
        while let Some(message) = receiver.next_message().unwrap() {
            decoded.push(message);
        }
    }
    assert_eq!(decoded, vec![text, binary]);
    assert!(!receiver.has_partial_message());
}

#[tokio::test]
async fn large_outgoing_messages_are_independent_of_incoming_budget() {
    let (a, b) = states().await;
    let mut sender = NoiseCodec::new(a, NonZeroUsize::new(1024).unwrap(), BytesMut::new());
    let mut receiver = NoiseCodec::new(
        b,
        NonZeroUsize::new(64 * 1024 * 1024).unwrap(),
        BytesMut::new(),
    );
    for length in [17 * 1024 * 1024, 5 * 1024 * 1024] {
        let records = sender
            .seal(Message::Text("x".repeat(length).into()))
            .unwrap();
        for message in records {
            let Message::Binary(bytes) = message else {
                panic!("expected encrypted record")
            };
            assert!(bytes.len() <= MAX_RECORD + RECORD_HEADER);
            receiver.feed(&bytes);
        }
        let Some(Message::Text(text)) = receiver.next_message().unwrap() else {
            panic!("expected complete message")
        };
        assert_eq!(text.len(), length);
        assert!(text.bytes().all(|byte| byte == b'x'));
        assert!(!receiver.has_partial_message());
    }
}

#[tokio::test]
async fn incoming_budget_counts_payload_without_type_or_record_overhead() {
    for length in [1023, 1024, 1025] {
        let (mut sender, mut receiver) = codecs(1024).await;
        receiver.feed(&wire_bytes(
            sender
                .seal(Message::Binary(vec![0; length].into()))
                .unwrap(),
        ));
        let result = receiver.next_message();
        if length <= 1024 {
            let Some(Message::Binary(bytes)) = result.unwrap() else {
                panic!("expected binary message")
            };
            assert_eq!(bytes.len(), length);
        } else {
            assert_eq!(result.unwrap_err().kind, ErrorKind::InvalidRequest);
        }
    }
}

#[tokio::test]
async fn overlong_declared_messages_are_rejected_before_receiving_the_body() {
    let (mut sender, mut receiver) = codecs(1024).await;
    receiver.feed(&encrypted_plaintext(&mut sender, &2049u32.to_be_bytes()));
    assert_eq!(
        receiver.next_message().unwrap_err().kind,
        ErrorKind::InvalidRequest
    );
}

#[tokio::test]
async fn header_only_truncation_remains_observable_without_extra_progress_state() {
    let (mut sender, mut receiver) = codecs(1024).await;
    receiver.feed(&encrypted_plaintext(&mut sender, &12u32.to_be_bytes()));
    assert!(receiver.next_message().unwrap().is_none());
    assert!(receiver.has_partial_message());
    let (_, mut receiver) = codecs(1024).await;
    receiver.feed(&12u16.to_be_bytes());
    assert!(receiver.next_message().unwrap().is_none());
    assert!(receiver.has_partial_message());
}

#[tokio::test]
async fn replay_tampering_and_unknown_message_kinds_are_rejected() {
    let (mut sender, mut receiver) = codecs(1024).await;
    let wire = wire_bytes(sender.seal(Message::Text("hello".into())).unwrap());
    receiver.feed(&wire);
    assert!(receiver.next_message().unwrap().is_some());
    receiver.feed(&wire);
    assert_eq!(
        receiver.next_message().unwrap_err().kind,
        ErrorKind::InvalidSignature
    );
    let (mut sender, mut receiver) = codecs(1024).await;
    let mut wire = wire_bytes(sender.seal(Message::Text("hello".into())).unwrap()).to_vec();
    *wire.last_mut().unwrap() ^= 1;
    receiver.feed(&wire);
    assert_eq!(
        receiver.next_message().unwrap_err().kind,
        ErrorKind::InvalidSignature
    );
    let (mut sender, mut receiver) = codecs(1024).await;
    receiver.feed(&encrypted_plaintext(&mut sender, &[0, 0, 0, 1, 99]));
    assert_eq!(
        receiver.next_message().unwrap_err().kind,
        ErrorKind::InvalidRequest
    );
}

#[tokio::test]
async fn wire_budget_covers_record_boundaries() {
    for length in [
        1,
        1024,
        MAX_PLAINTEXT - 5,
        MAX_PLAINTEXT - 4,
        MAX_PLAINTEXT * 2,
    ] {
        let (mut sender, _) = codecs(1024).await;
        let wire = wire_bytes(
            sender
                .seal(Message::Binary(vec![0; length].into()))
                .unwrap(),
        );
        let budget = wire_message_size(NonZeroUsize::new(length).unwrap()).get();
        assert!(wire.len() <= budget);
        if length >= 1024 {
            assert_eq!(wire.len(), budget);
        }
    }
}

struct StalledSocket;
impl Stream for StalledSocket {
    type Item = Result<Message, axum::Error>;
    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Pending
    }
}
impl Sink<Message> for StalledSocket {
    type Error = axum::Error;
    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Pending
    }
    fn start_send(self: Pin<&mut Self>, _: Message) -> Result<(), Self::Error> {
        unreachable!()
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Pending
    }
    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Pending
    }
}

#[tokio::test(start_paused = true)]
async fn existing_send_deadline_covers_stalled_encrypted_transport() {
    let (state, _) = states().await;
    let encrypted = EncryptedSocket {
        inner: Box::new(StalledSocket),
        codec: NoiseCodec::new(state, NonZeroUsize::new(1024).unwrap(), BytesMut::new()),
        pending: VecDeque::new(),
        fused: false,
    };
    let mut socket = crate::util::net::WebSocket::new(encrypted);
    socket.set_send_timeout(Duration::from_secs(15));
    assert!(socket.send(Message::Text("hello".into())).await.is_err());
    assert!(socket.recv().await.is_none());
}

#[tokio::test(start_paused = true)]
async fn silent_handshake_is_bounded() {
    let identity = NoiseIdentity::new(&signer(42)).await.unwrap();
    let result = EncryptedSocket::accept(
        Box::new(StalledSocket),
        identity,
        PROLOGUE,
        NonZeroUsize::new(1024).unwrap(),
        None,
    )
    .await;
    assert!(matches!(
        result,
        Err(Error {
            kind: ErrorKind::Network,
            ..
        })
    ));
}

#[derive(Clone, Default)]
struct PeerContext {
    peer: Option<AnyVerifyingKey>,
    calls: Arc<AtomicUsize>,
}
impl RpcContext for PeerContext {}

struct SecureUpgrade;
impl WebSocketUpgrader<PeerContext> for SecureUpgrade {
    fn max_wire_message_size(&self, logical: NonZeroUsize) -> NonZeroUsize {
        wire_message_size(logical)
    }
    fn request_context(&self, context: &PeerContext) -> PeerContext {
        context.clone()
    }
    fn upgrade(
        &self,
        context: Arc<PeerContext>,
        _request: axum::http::request::Parts,
        socket: axum::extract::ws::WebSocket,
        options: WebSocketOptions,
    ) -> BoxFuture<'static, Result<UpgradedWebSocket<PeerContext>, axum::Error>> {
        async move {
            let identity = NoiseIdentity::new(&signer(42))
                .await
                .map_err(|error| axum::Error::new(error.source))?;
            let expected = signer(41)
                .verifying_key()
                .await
                .map_err(|error| axum::Error::new(error.source))?;
            let (peer, socket) = EncryptedSocket::accept(
                Box::new(socket),
                identity,
                PROLOGUE,
                options.max_message_size,
                Some(expected),
            )
            .await
            .map_err(|error| axum::Error::new(error.source))?;
            let mut context = (*context).clone();
            context.peer = Some(peer);
            Ok(UpgradedWebSocket {
                context: Arc::new(context),
                socket: Box::new(socket),
            })
        }
        .boxed()
    }
}

fn to_client(message: Message) -> ClientMessage {
    match message {
        Message::Text(text) => ClientMessage::Text(text.to_string().into()),
        Message::Binary(bytes) => ClientMessage::Binary(bytes),
        Message::Ping(bytes) => ClientMessage::Ping(bytes),
        Message::Pong(bytes) => ClientMessage::Pong(bytes),
        Message::Close(frame) => ClientMessage::Close(frame.map(|frame| {
            tokio_tungstenite::tungstenite::protocol::CloseFrame {
                code: frame.code.into(),
                reason: frame.reason.to_string().into(),
            }
        })),
    }
}

fn to_server(message: ClientMessage) -> Message {
    match message {
        ClientMessage::Text(text) => Message::Text(text.to_string().into()),
        ClientMessage::Binary(bytes) => Message::Binary(bytes),
        ClientMessage::Ping(bytes) => Message::Ping(bytes),
        ClientMessage::Pong(bytes) => Message::Pong(bytes),
        ClientMessage::Close(frame) => {
            Message::Close(frame.map(|frame| axum::extract::ws::CloseFrame {
                code: frame.code.into(),
                reason: frame.reason.to_string().into(),
            }))
        }
        ClientMessage::Frame(_) => unreachable!("WebSocketStream yields decoded messages"),
    }
}

struct ServerGuard(tokio::task::JoinHandle<()>);
impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn server() -> (String, PeerContext, ServerGuard) {
    let context = PeerContext::default();
    let source = context.clone();
    let rpc = Server::new(
        move || {
            let context = source.clone();
            async move { Ok(context) }
        },
        ParentHandler::new()
            .subcommand(
                "peer",
                rpc_toolkit::from_fn_async(|context: PeerContext, _: Empty| async move {
                    context.calls.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, rpc_toolkit::yajrc::RpcError>(context.peer)
                })
                .no_cli()
                .no_ts(),
            )
            .subcommand(
                "bulk",
                rpc_toolkit::from_fn_async(|args: HandlerArgs<PeerContext, Empty>| async move {
                    args.context.calls.fetch_add(1, Ordering::SeqCst);
                    let stream = args.rpc_stream.unwrap();
                    tokio::spawn(async move {
                        if stream.send("r".repeat(5 * 1024 * 1024)).await.is_ok() {
                            let _ = stream.close(Ok::<_, rpc_toolkit::yajrc::RpcError>(()));
                        }
                    });
                    Ok::<_, rpc_toolkit::yajrc::RpcError>("d".repeat(17 * 1024 * 1024))
                })
                .no_cli()
                .no_ts(),
            ),
    )
    .for_websocket()
    .with_upgrader(SecureUpgrade);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/rpc", listener.local_addr().unwrap());
    let guard = ServerGuard(tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/rpc", get(rpc)))
            .await
            .unwrap();
    }));
    (url, context, guard)
}

pub(crate) async fn raw_socket(url: &str) -> BoxWebSocket {
    let limits = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
        .max_frame_size(Some(64 * 1024 * 1024))
        .max_message_size(Some(64 * 1024 * 1024));
    let (socket, _) = tokio_tungstenite::connect_async_with_config(url, Some(limits), false)
        .await
        .unwrap();
    let raw = socket
        .with(|message| {
            futures::future::ready(Ok::<_, tokio_tungstenite::tungstenite::Error>(to_client(
                message,
            )))
        })
        .sink_map_err(axum::Error::new)
        .map(|result| result.map(to_server).map_err(axum::Error::new));
    Box::new(raw)
}

async fn connect(url: &str, seed: u8) -> EncryptedSocket {
    let identity = NoiseIdentity::new(&signer(seed)).await.unwrap();
    let (peer, socket) = EncryptedSocket::initiate(
        raw_socket(url).await,
        identity,
        PROLOGUE,
        NonZeroUsize::new(64 * 1024 * 1024).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(peer, signer(42).verifying_key().await.unwrap());
    socket
}

async fn receive<S: rpc_toolkit::WebSocketTransport>(socket: &mut S) -> Value {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match socket.next().await.unwrap().unwrap() {
                Message::Text(text) => return serde_json::from_str(&text).unwrap(),
                Message::Ping(_) => socket.flush().await.unwrap(),
                other => panic!("unexpected message: {other:?}"),
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn real_socket_encryption_admits_context_and_preserves_large_rpc_streams() {
    let (url, context, _server) = server().await;
    let mut socket = crate::util::net::WebSocket::new(connect(&url, 41).await);
    socket.set_send_timeout(Duration::from_secs(15));
    socket
        .send(Message::Text(
            json!({"jsonrpc":"2.0","id":1,"method":"peer"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert_eq!(
        receive(&mut socket).await["result"],
        serde_json::to_value(signer(41).verifying_key().await.unwrap()).unwrap()
    );
    socket.send(Message::Text("{\"jsonrpc\":\"2.0\",\"rpcstream\":\"1.0\",\"id\":18446744073709551615,\"method\":\"bulk\"}".into())).await.unwrap();
    let dump = receive(&mut socket).await;
    assert_eq!(dump["id"], json!(u64::MAX));
    assert_eq!(dump["result"].as_str().unwrap().len(), 17 * 1024 * 1024);
    let revision = receive(&mut socket).await;
    assert_eq!(revision["id"], json!(u64::MAX));
    assert_eq!(revision["result"].as_str().unwrap().len(), 5 * 1024 * 1024);
    assert_eq!(revision["end"], false);
    assert_eq!(receive(&mut socket).await["end"], true);
    assert_eq!(context.calls.load(Ordering::SeqCst), 2);
    socket.close().await.unwrap();
}

#[tokio::test]
async fn completed_channel_rejects_plaintext_before_rpc_dispatch() {
    let (url, context, _server) = server().await;
    let mut socket = connect(&url, 41).await;
    socket
        .inner
        .send(Message::Text(
            json!({"jsonrpc":"2.0","id":1,"method":"peer"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let received = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap();
    assert!(!matches!(received, Some(Ok(Message::Text(_)))));
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn expected_peer_restriction_rejects_different_authenticated_identity() {
    let (url, context, _server) = server().await;
    let mut socket = connect(&url, 43).await;
    let _ = socket
        .send(Message::Text(
            json!({"jsonrpc":"2.0","id":1,"method":"peer"})
                .to_string()
                .into(),
        ))
        .await;
    let received = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap();
    assert!(!matches!(received, Some(Ok(Message::Text(_)))));
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
}
