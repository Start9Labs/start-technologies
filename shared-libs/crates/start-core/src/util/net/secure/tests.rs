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

use super::*;

#[test]
fn browser_profile_preserves_native_framing_and_close_validation() {
    let profile = browser_profile();
    assert_eq!(profile["recordHeaderBytes"], 2);
    assert_eq!(profile["messageHeaderBytes"], 4);
    assert_eq!(profile["maxWireChunkBytes"], 65537);
    assert_eq!(
        profile["kinds"],
        serde_json::json!({"text": 0, "binary": 1, "close": 2})
    );
    let ranges: Vec<[u16; 2]> = serde_json::from_value(profile["closeCodes"].clone()).unwrap();
    for code in [
        0,
        999,
        1000,
        1003,
        1004,
        1005,
        1006,
        1007,
        1013,
        1014,
        1015,
        2999,
        3000,
        3999,
        4000,
        4999,
        5000,
        u16::MAX,
    ] {
        let frame = CloseFrame {
            code,
            reason: "".into(),
        };
        assert_eq!(
            ranges
                .iter()
                .any(|[start, end]| (*start..=*end).contains(&code)),
            validate_close(&frame).is_ok(),
            "{code}",
        );
    }
}
use crate::sign::{AnySigningKey, Signer};

const CONTEXT: &str = "server.test";

fn signer(seed: u8) -> Signer {
    AnySigningKey::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[seed; 32])).into()
}

async fn states() -> (SessionCipher, SessionCipher) {
    let prepared = PreparedClient::new(&signer(41), CONTEXT, Target::Rpc)
        .await
        .unwrap();
    let (_, _, response, server) = crate::sign::session::accept(
        &signer(42),
        &[CONTEXT.into()],
        prepared.message(),
        &Target::Rpc,
        None,
        |_| async { Ok(()) },
    )
    .await
    .unwrap();
    (prepared.finish(&response).await.unwrap().1, server)
}

async fn codecs(incoming: usize) -> (SessionCodec, SessionCodec) {
    let (a, b) = states().await;
    let limit = NonZeroUsize::new(incoming).unwrap();
    (
        SessionCodec::new(a, limit, BytesMut::new()),
        SessionCodec::new(b, limit, BytesMut::new()),
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

fn encrypted_plaintext(sender: &mut SessionCodec, plaintext: &[u8]) -> Bytes {
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
async fn profile_kind_values_match_frozen_wire_messages() {
    let (mut sender, mut receiver) = codecs(1024).await;
    for (plaintext, expected) in [
        (&[0, 0, 0, 2, 0, b'x'][..], Message::Text("x".into())),
        (
            &[0, 0, 0, 2, 1, 0xff][..],
            Message::Binary(Bytes::from_static(&[0xff])),
        ),
        (
            &[0, 0, 0, 3, 2, 0x03, 0xe8][..],
            Message::Close(Some(CloseFrame {
                code: 1000,
                reason: "".into(),
            })),
        ),
    ] {
        receiver.feed(&encrypted_plaintext(&mut sender, plaintext));
        assert_eq!(receiver.next_message().unwrap(), Some(expected));
    }
    assert!(!receiver.has_partial_message());
}

#[tokio::test]
async fn close_results_are_encrypted_and_preserve_control_payload_boundaries() {
    for (code, reason) in [
        (1000, "complete".to_owned()),
        (1011, "private failure details".to_owned()),
        (1000, "雪".repeat(MAX_CLOSE_REASON_BYTES / 3)),
    ] {
        let (mut sender, mut receiver) = codecs(1).await;
        let close = Message::Close(Some(CloseFrame {
            code,
            reason: reason.clone().into(),
        }));
        let mut records = sender.seal(close.clone()).unwrap();
        assert_eq!(records.pop_back(), Some(Message::Close(None)));
        let wire = wire_bytes(records);
        assert!(
            !wire
                .windows(reason.len())
                .any(|part| part == reason.as_bytes())
        );
        receiver.feed(&wire);
        assert_eq!(receiver.next_message().unwrap(), Some(close));
        assert!(!receiver.has_partial_message());
    }
    let (mut sender, mut receiver) = codecs(1).await;
    let mut records = sender.seal(Message::Close(None)).unwrap();
    assert_eq!(records.pop_back(), Some(Message::Close(None)));
    receiver.feed(&wire_bytes(records));
    assert_eq!(receiver.next_message().unwrap(), Some(Message::Close(None)));
}

#[tokio::test]
async fn malformed_authenticated_close_results_are_rejected() {
    for payload in [
        vec![3],
        vec![0x03, 0xed],
        vec![0x03, 0xe8, 0xff],
        [vec![0x03, 0xe8], vec![b'x'; MAX_CLOSE_REASON_BYTES + 1]].concat(),
    ] {
        let (mut sender, mut receiver) = codecs(1024).await;
        let mut message = Vec::new();
        message.extend_from_slice(
            &u32::try_from(payload.len() + MESSAGE_KIND)
                .unwrap()
                .to_be_bytes(),
        );
        message.push(MessageKind::Close.into());
        message.extend_from_slice(&payload);
        receiver.feed(&encrypted_plaintext(&mut sender, &message));
        assert_eq!(
            receiver.next_message().unwrap_err().kind,
            ErrorKind::InvalidRequest
        );
    }
    for close in [
        CloseFrame {
            code: 1005,
            reason: "".into(),
        },
        CloseFrame {
            code: 1000,
            reason: "x".repeat(MAX_CLOSE_REASON_BYTES + 1).into(),
        },
    ] {
        let (mut sender, _) = codecs(1024).await;
        assert_eq!(
            sender.seal(Message::Close(Some(close))).unwrap_err().kind,
            ErrorKind::InvalidRequest
        );
    }
}

#[tokio::test]
async fn close_allowance_does_not_expand_small_data_budgets() {
    for message in [
        Message::Text("ab".into()),
        Message::Binary(Bytes::from_static(b"ab")),
    ] {
        let (mut sender, mut receiver) = codecs(1).await;
        receiver.feed(&wire_bytes(sender.seal(message).unwrap()));
        assert_eq!(
            receiver.next_message().unwrap_err().kind,
            ErrorKind::InvalidRequest
        );
    }
}

#[tokio::test]
async fn keepalive_controls_remain_ordinary_websocket_messages() {
    let (mut sender, _) = codecs(1024).await;
    for message in [
        Message::Ping(Bytes::from_static(b"ping")),
        Message::Pong(Bytes::from_static(b"pong")),
    ] {
        assert_eq!(
            sender.seal(message.clone()).unwrap(),
            VecDeque::from([message])
        );
    }
}

#[tokio::test]
async fn large_outgoing_messages_are_independent_of_incoming_budget() {
    let (a, b) = states().await;
    let mut sender = SessionCodec::new(a, NonZeroUsize::new(1024).unwrap(), BytesMut::new());
    let mut receiver = SessionCodec::new(
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
        codec: SessionCodec::new(state, NonZeroUsize::new(1024).unwrap(), BytesMut::new()),
        pending: VecDeque::new(),
        closing: None,
        fused: false,
    };
    let mut socket = crate::util::net::WebSocket::new(encrypted);
    socket.set_send_timeout(Duration::from_secs(15));
    assert!(socket.send(Message::Text("hello".into())).await.is_err());
    assert!(socket.recv().await.is_none());
}

#[tokio::test(start_paused = true)]
async fn close_result_reuses_the_existing_send_deadline() {
    let (state, _) = states().await;
    let encrypted = EncryptedSocket {
        inner: Box::new(StalledSocket),
        codec: SessionCodec::new(state, NonZeroUsize::new(1024).unwrap(), BytesMut::new()),
        pending: VecDeque::new(),
        closing: None,
        fused: false,
    };
    let mut socket = crate::util::net::WebSocket::new(encrypted);
    socket.set_send_timeout(Duration::from_secs(15));
    let started = tokio::time::Instant::now();
    assert!(
        socket
            .close_result(Ok::<_, &str>("private result"))
            .await
            .is_err()
    );
    assert_eq!(started.elapsed(), Duration::from_secs(15));
}

#[tokio::test(start_paused = true)]
async fn silent_handshake_is_bounded() {
    let result = EncryptedSocket::accept(
        Box::new(StalledSocket),
        &signer(42),
        &[CONTEXT.into()],
        NonZeroUsize::new(1024).unwrap(),
        &Target::Rpc,
        None,
        |_| async { Ok(()) },
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
            let expected = signer(41)
                .verifying_key()
                .await
                .map_err(|error| axum::Error::new(error.source))?;
            let (peer, _, socket) = EncryptedSocket::accept(
                Box::new(socket),
                &signer(42),
                &[CONTEXT.into()],
                options.max_message_size,
                &Target::Rpc,
                Some(&expected),
                |_| async { Ok(()) },
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
                rpc_toolkit::from_fn_async(
                    |args: HandlerArgs<PeerContext, Empty, Empty, String>| async move {
                        args.context.calls.fetch_add(1, Ordering::SeqCst);
                        let stream = args.rpc_stream.unwrap();
                        tokio::spawn(async move {
                            if stream.send("r".repeat(5 * 1024 * 1024)).await.is_ok() {
                                let _ = stream.close(Ok::<_, rpc_toolkit::yajrc::RpcError>(()));
                            }
                        });
                        Ok::<_, rpc_toolkit::yajrc::RpcError>("d".repeat(17 * 1024 * 1024))
                    },
                )
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

async fn closing_server(
    result: Result<&'static str, &'static str>,
) -> (
    String,
    tokio::sync::oneshot::Receiver<Result<(), Error>>,
    ServerGuard,
) {
    let (done, completion) = tokio::sync::oneshot::channel();
    let done = Arc::new(std::sync::Mutex::new(Some(done)));
    let route = get(move |upgrade: axum::extract::ws::WebSocketUpgrade| {
        let done = done.clone();
        async move {
            upgrade.on_upgrade(move |socket| async move {
                let (_, _, socket) = EncryptedSocket::accept(
                    Box::new(socket),
                    &signer(42),
                    &[CONTEXT.into()],
                    NonZeroUsize::new(1024).unwrap(),
                    &Target::Rpc,
                    None,
                    |_| async { Ok(()) },
                )
                .await
                .unwrap();
                let socket = crate::util::net::WebSocket::new(socket);
                let result =
                    tokio::time::timeout(Duration::from_secs(3), socket.close_result(result))
                        .await
                        .unwrap();
                done.lock().unwrap().take().unwrap().send(result).unwrap();
            })
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/close", listener.local_addr().unwrap());
    let guard = ServerGuard(tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/close", route))
            .await
            .unwrap();
    }));
    (url, completion, guard)
}

#[tokio::test]
async fn actual_close_result_keeps_application_outcomes_encrypted() {
    for result in [
        Ok("private close-result success"),
        Err("private close-result failure"),
    ] {
        let (url, completion, _server) = closing_server(result).await;
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let copy = received.clone();
        let raw = raw_socket(&url).await.inspect(move |result| {
            if let Ok(message) = result {
                copy.lock().unwrap().push(message.clone());
            }
        });
        let (_, mut socket) = EncryptedSocket::initiate(
            Box::new(raw),
            PreparedClient::new(&signer(41), CONTEXT, Target::Rpc)
                .await
                .unwrap(),
            NonZeroUsize::new(1).unwrap(),
        )
        .await
        .unwrap();
        let Some(Ok(Message::Close(Some(frame)))) =
            tokio::time::timeout(Duration::from_secs(3), socket.next())
                .await
                .unwrap()
        else {
            panic!("expected authenticated close result")
        };
        assert_eq!(frame.code, if result.is_err() { 1011 } else { 1000 });
        assert_eq!(frame.reason.as_str(), result.unwrap_or_else(|error| error));
        assert!(socket.next().await.is_none());
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(3), socket.inner.next())
                .await
                .unwrap(),
            Some(Ok(Message::Close(None)))
        ));
        for message in received.lock().unwrap().iter() {
            match message {
                Message::Binary(bytes) => assert!(
                    !bytes
                        .windows(frame.reason.len())
                        .any(|part| part == frame.reason.as_bytes())
                ),
                Message::Close(frame) => assert!(frame.is_none()),
                _ => panic!("unexpected plaintext message"),
            }
        }
        completion.await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn authenticated_close_result_survives_failed_transport_acknowledgement() {
    let (url, completion, _server) = closing_server(Ok("completed result")).await;
    let mut socket = connect(&url, 41).await;
    socket.inner = Box::new(socket.inner.with(|_: Message| {
        futures::future::ready(Err::<Message, _>(axum::Error::new(std::io::Error::from(
            std::io::ErrorKind::BrokenPipe,
        ))))
    }));
    let Some(Ok(Message::Close(Some(frame)))) =
        tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap()
    else {
        panic!("expected verified result despite failed acknowledgement")
    };
    assert_eq!(frame.code, 1000);
    assert_eq!(frame.reason, "completed result");
    drop(socket);
    assert!(completion.await.unwrap().is_err());
}

#[tokio::test]
async fn plaintext_close_cannot_fabricate_an_authenticated_success() {
    let (url, _context, _server) = server().await;
    let mut socket = connect(&url, 41).await;
    socket.inner = Box::new(socket.inner.map(|result| {
        result.map(|_| {
            Message::Close(Some(CloseFrame {
                code: 1000,
                reason: "forged result".into(),
            }))
        })
    }));
    socket
        .inner
        .send(Message::Ping(Bytes::from_static(b"trigger")))
        .await
        .unwrap();
    let message = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap();
    assert!(matches!(message, Some(Ok(Message::Close(None)))));
}

pub(crate) async fn raw_socket(url: &str) -> BoxWebSocket {
    let limits = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
        .max_frame_size(Some(64 * 1024 * 1024))
        .max_message_size(Some(64 * 1024 * 1024));
    let (socket, _) = tokio_tungstenite::connect_async_with_config(url, Some(limits), false)
        .await
        .unwrap();
    crate::util::net::tungstenite::transport(socket)
}

async fn connect(url: &str, seed: u8) -> EncryptedSocket {
    let prepared = PreparedClient::new(&signer(seed), CONTEXT, Target::Rpc)
        .await
        .unwrap();
    let (peer, socket) = EncryptedSocket::initiate(
        raw_socket(url).await,
        prepared,
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
    let prepared = PreparedClient::new(&signer(43), CONTEXT, Target::Rpc)
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(
            Duration::from_secs(3),
            EncryptedSocket::initiate(
                raw_socket(&url).await,
                prepared,
                NonZeroUsize::new(1024).unwrap(),
            )
        )
        .await
        .unwrap()
        .is_err()
    );
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
}
