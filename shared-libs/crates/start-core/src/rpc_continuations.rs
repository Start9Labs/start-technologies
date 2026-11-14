use std::collections::BTreeMap;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Mutex as SyncMutex;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::extract::Request;
use axum::extract::ws::{WebSocket as AxumWebSocket, WebSocketUpgrade};
use axum::response::Response;
use clap::builder::ValueParserFactory;
use futures::future::BoxFuture;
use futures::{Future, FutureExt, Stream, TryStreamExt};
use imbl_value::InternedString;
use tokio::sync::{Mutex as AsyncMutex, broadcast};

use crate::middleware::auth::session::SecureSession;
#[allow(unused_imports)]
use crate::prelude::*;
use crate::util::future::TimedResource;

pub type WebSocket = crate::util::net::WebSocket<rpc_toolkit::BoxWebSocket>;
use crate::util::{FromStrParser, new_guid};

pub(crate) async fn forward_json_stream<T: serde::Serialize>(
    socket: &mut WebSocket,
    mut source: impl Stream<Item = Result<T, Error>> + Unpin,
) -> Result<&'static str, Error> {
    loop {
        tokio::select! {
            item = source.try_next() => {
                let Some(item) = item? else { return Ok("complete") };
                socket.send(axum::extract::ws::Message::Text(serde_json::to_string(&item).with_kind(ErrorKind::Serialization)?.into())).await.with_kind(ErrorKind::Network)?;
            }
            message = socket.recv() => {
                if message.transpose().with_kind(ErrorKind::Network)?.is_none() { return Ok("complete") }
            }
        }
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize, VisitFields,
)]
#[visit(ts(type = "z.string()"))]
pub struct Guid(InternedString);

rpc_toolkit::reflect_ts!(Guid);
impl Guid {
    pub fn new() -> Self {
        Self(new_guid())
    }

    pub fn from(r: &str) -> Option<Guid> {
        if r.len() != 32 {
            return None;
        }
        for c in r.chars() {
            if !(c >= 'A' && c <= 'Z' || c >= '2' && c <= '7') {
                return None;
            }
        }
        Some(Guid(InternedString::intern(r)))
    }
}
impl Default for Guid {
    fn default() -> Self {
        Self::new()
    }
}
impl AsRef<str> for Guid {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}
impl FromStr for Guid {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from(s).ok_or_else(|| Error::new(eyre!("invalid guid"), ErrorKind::Deserialization))
    }
}
impl ValueParserFactory for Guid {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        Self::Parser::new()
    }
}

#[test]
fn parse_guid() {
    println!("{:?}", Guid::from(&format!("{}", Guid::new())))
}

impl std::fmt::Display for Guid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// An already-registered revocation listener; registration does not grant authority.
pub struct RevocationSubscription(BoxFuture<'static, ()>);

impl Future for RevocationSubscription {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.poll_unpin(cx)
    }
}

impl RevocationSubscription {
    pub(crate) fn forward_stream<V: serde::Serialize + Send + 'static>(
        self,
        stream: rpc_toolkit::RpcStream<V>,
        source: impl Stream<Item = Result<V, Error>> + Send + 'static,
    ) {
        tokio::spawn(async move {
            let result: Result<(), rpc_toolkit::yajrc::RpcError> = tokio::select! {
                biased;
                _ = self => Err(session_killed().into()),
                result = stream.forward(source) => result,
            };
            let _ = stream.close(result);
        });
    }
}

pub struct RestFuture {
    kill: Option<RevocationSubscription>,
    fut: BoxFuture<'static, Result<Response, Error>>,
}
impl Future for RestFuture {
    type Output = Result<Response, Error>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self
            .kill
            .as_mut()
            .is_some_and(|kill| kill.poll_unpin(cx).is_ready())
        {
            Poll::Ready(Err(session_killed()))
        } else {
            self.fut.poll_unpin(cx)
        }
    }
}
pub type RestHandler = Box<dyn FnOnce(Request) -> RestFuture + Send>;

fn session_killed() -> Error {
    Error::new(eyre!("session killed"), ErrorKind::Authorization)
}

pub struct WebSocketFuture {
    kill: Option<RevocationSubscription>,
    fut: BoxFuture<'static, ()>,
}
impl Future for WebSocketFuture {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self
            .kill
            .as_mut()
            .is_some_and(|kill| kill.poll_unpin(cx).is_ready())
        {
            Poll::Ready(())
        } else {
            self.fut.poll_unpin(cx)
        }
    }
}
type WebSocketCallback = Box<dyn FnOnce(WebSocket) -> BoxFuture<'static, ()> + Send>;

/// Resource policy of the RPC issuing a WebSocket continuation.
#[derive(Clone, Copy, Debug)]
pub enum WebSocketPolicy {
    /// Available without caller authentication, including optional signed browsing.
    Public,
    /// Requires caller authentication before issuing the continuation.
    Authenticated,
}

pub struct WebSocketHandler {
    callback: WebSocketCallback,
    policy: WebSocketPolicy,
    secure: Option<SecureSession>,
    kill: Option<RevocationSubscription>,
}
impl WebSocketHandler {
    /// The redeemed GUID must identify this handler.
    pub fn on_upgrade(self, ws: WebSocketUpgrade, guid: Guid) -> Response {
        let incoming = std::num::NonZeroUsize::new(match self.policy {
            WebSocketPolicy::Public => 1024,
            WebSocketPolicy::Authenticated => 64 * 1024 * 1024,
        })
        .unwrap();
        let (mut ws, send_timeout) = match self.policy {
            WebSocketPolicy::Public => (
                ws.read_buffer_size(2048)
                    .write_buffer_size(0)
                    .max_frame_size(incoming.get()),
                Some(Duration::from_secs(15)),
            ),
            WebSocketPolicy::Authenticated => (ws, None),
        };
        ws = ws.max_message_size(incoming.get());
        if self.secure.is_some() {
            let wire = crate::util::net::secure::wire_chunk_size(incoming).get();
            ws = ws.max_message_size(wire).max_frame_size(wire);
        }
        ws.on_upgrade(move |socket: AxumWebSocket| WebSocketFuture {
            kill: self.kill,
            fut: async move {
                let socket: rpc_toolkit::BoxWebSocket = match self.secure {
                    Some(secure) => match secure.accept(Box::new(socket), incoming, &guid).await {
                        Ok(socket) => Box::new(socket),
                        Err(_) => return,
                    },
                    None => Box::new(socket),
                };
                let mut socket = WebSocket::new(socket);
                if let Some(timeout) = send_timeout {
                    socket.set_send_timeout(timeout);
                }
                (self.callback)(socket).await;
            }
            .boxed(),
        })
    }
}

pub enum RpcContinuation {
    Rest(TimedResource<RestHandler>),
    WebSocket(TimedResource<WebSocketHandler>),
}
impl RpcContinuation {
    pub fn rest<F, Fut>(handler: F, timeout: Duration) -> Self
    where
        F: FnOnce(Request) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Response, Error>> + Send + 'static,
    {
        RpcContinuation::Rest(TimedResource::new(
            Box::new(|req| RestFuture {
                kill: None,
                fut: handler(req).boxed(),
            }),
            timeout,
        ))
    }
    pub fn ws<F, Fut>(
        policy: WebSocketPolicy,
        secure: Option<SecureSession>,
        handler: F,
        timeout: Duration,
    ) -> Self
    where
        F: FnOnce(WebSocket) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        RpcContinuation::WebSocket(TimedResource::new(
            WebSocketHandler {
                callback: Box::new(|ws| handler(ws).boxed()),
                policy,
                secure,
                kill: None,
            },
            timeout,
        ))
    }
    pub fn rest_authed<F, Fut>(
        subscription: RevocationSubscription,
        handler: F,
        timeout: Duration,
    ) -> Self
    where
        F: FnOnce(Request) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Response, Error>> + Send + 'static,
    {
        let kill = Some(subscription);
        RpcContinuation::Rest(TimedResource::new(
            Box::new(|req| RestFuture {
                kill,
                fut: handler(req).boxed(),
            }),
            timeout,
        ))
    }
    pub fn ws_authed<F, Fut>(
        subscription: RevocationSubscription,
        policy: WebSocketPolicy,
        secure: Option<SecureSession>,
        handler: F,
        timeout: Duration,
    ) -> Self
    where
        F: FnOnce(WebSocket) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let kill = Some(subscription);
        RpcContinuation::WebSocket(TimedResource::new(
            WebSocketHandler {
                callback: Box::new(|ws| handler(ws).boxed()),
                policy,
                secure,
                kill,
            },
            timeout,
        ))
    }
    pub fn is_timed_out(&self) -> bool {
        match self {
            RpcContinuation::Rest(a) => a.is_timed_out(),
            RpcContinuation::WebSocket(a) => a.is_timed_out(),
        }
    }
}

pub struct RpcContinuations(AsyncMutex<BTreeMap<Guid, RpcContinuation>>);
impl RpcContinuations {
    pub const LEGACY_WS_PATH: &'static str = "/ws/rpc";

    pub fn new() -> Self {
        RpcContinuations(AsyncMutex::new(BTreeMap::new()))
    }

    #[instrument(skip_all)]
    pub async fn clean(&self) {
        let mut continuations = self.0.lock().await;
        let mut to_remove = Vec::new();
        for (guid, cont) in &*continuations {
            if cont.is_timed_out() {
                to_remove.push(guid.clone());
            }
        }
        for guid in to_remove {
            continuations.remove(&guid);
        }
    }

    #[instrument(skip_all)]
    pub async fn add(&self, guid: Guid, handler: RpcContinuation) {
        self.clean().await;
        self.0.lock().await.insert(guid, handler);
    }

    pub async fn get_ws_handler(&self, guid: &Guid) -> Option<WebSocketHandler> {
        let mut continuations = self.0.lock().await;
        if !matches!(continuations.get(guid), Some(RpcContinuation::WebSocket(_))) {
            return None;
        }
        let Some(RpcContinuation::WebSocket(x)) = continuations.remove(guid) else {
            return None;
        };
        x.get().await
    }

    pub async fn get_rest_handler(&self, guid: &Guid) -> Option<RestHandler> {
        let mut continuations: tokio::sync::MutexGuard<'_, BTreeMap<Guid, RpcContinuation>> =
            self.0.lock().await;
        if !matches!(continuations.get(guid), Some(RpcContinuation::Rest(_))) {
            return None;
        }
        let Some(RpcContinuation::Rest(x)) = continuations.remove(guid) else {
            return None;
        };
        x.get().await
    }
}

#[cfg(test)]
mod stream_tests;

pub struct OpenAuthedContinuations<Key: Eq + Ord>(SyncMutex<BTreeMap<Key, broadcast::Sender<()>>>);
impl<T> OpenAuthedContinuations<T>
where
    T: Eq + Ord,
{
    pub fn new() -> Self {
        Self(SyncMutex::new(BTreeMap::new()))
    }
    /// Revocation cancels continuation sockets and rpcstream producers, including blocked writes.
    pub fn kill(&self, session: &T) {
        if let Some(channel) = self.0.lock().unwrap().remove(session) {
            channel.send(()).ok();
        }
    }
    /// Authentication must recheck live membership after acquiring this listener.
    pub(crate) fn subscribe_to_kill(&self, session: T) -> RevocationSubscription {
        let mut map = self.0.lock().unwrap();
        let mut recv = if let Some(send) = map.get(&session) {
            send.subscribe()
        } else {
            let (send, recv) = broadcast::channel(1);
            map.insert(session, send);
            recv
        };
        RevocationSubscription(
            async move {
                let _ = recv.recv().await;
            }
            .boxed(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use futures::task::{ArcWake, waker};

    use super::*;

    #[derive(Default)]
    struct WakeFlag(AtomicBool);

    impl ArcWake for WakeFlag {
        fn wake_by_ref(arc_self: &Arc<Self>) {
            arc_self.0.store(true, Ordering::SeqCst);
        }
    }

    fn revoke_pending<F: Future + Unpin>(open: &OpenAuthedContinuations<&str>, future: &mut F) {
        let flag = Arc::new(WakeFlag::default());
        let waker = waker(flag.clone());
        let mut cx = Context::from_waker(&waker);
        assert!(Pin::new(future).poll(&mut cx).is_pending());
        open.kill(&"key");
        assert!(
            flag.0.load(Ordering::SeqCst),
            "revocation must wake an idle continuation"
        );
    }

    #[test]
    fn revocation_wakes_idle_websocket() {
        let open = OpenAuthedContinuations::new();
        let mut future = WebSocketFuture {
            kill: Some(open.subscribe_to_kill("key")),
            fut: futures::future::pending().boxed(),
        };
        revoke_pending(&open, &mut future);
        assert!(future.now_or_never().is_some());
    }

    #[test]
    fn revocation_wakes_idle_rest_request() {
        let open = OpenAuthedContinuations::new();
        let mut future = RestFuture {
            kill: Some(open.subscribe_to_kill("key")),
            fut: futures::future::pending().boxed(),
        };
        revoke_pending(&open, &mut future);
        let error = future.now_or_never().unwrap().unwrap_err();
        assert_eq!(error.kind, ErrorKind::Authorization);
    }
}

#[cfg(test)]
pub(crate) mod websocket_tests {
    use std::sync::Arc;

    use axum::Router;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::routing::get;
    use futures::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::protocol::frame::Frame;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::{Data, OpCode};

    use super::*;
    use crate::util::future::NonDetachingJoinHandle;

    pub(crate) async fn serve(cont: RpcContinuation) -> (String, NonDetachingJoinHandle<()>) {
        let continuations = Arc::new(RpcContinuations::new());
        let guid = Guid::new();
        continuations.add(guid.clone(), cont).await;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/ws/{guid}", listener.local_addr().unwrap());
        let router = Router::new().route(
            &format!("/ws/{guid}"),
            get(move |ws: WebSocketUpgrade| {
                let continuations = continuations.clone();
                let guid = guid.clone();
                async move {
                    match continuations.get_ws_handler(&guid).await {
                        Some(handler) => handler.on_upgrade(ws, guid),
                        None => StatusCode::NOT_FOUND.into_response(),
                    }
                }
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (url, server.into())
    }

    pub(crate) async fn echo(mut socket: WebSocket) {
        while let Some(Ok(message)) = socket.recv().await {
            if socket.send(message).await.is_err() {
                break;
            }
        }
    }

    #[tokio::test]
    async fn unauthenticated_frame_limit_and_one_shot_upgrade() {
        let (url, _server) = serve(RpcContinuation::ws(
            WebSocketPolicy::Public,
            None,
            echo,
            Duration::from_secs(30),
        ))
        .await;
        let (mut socket, _) = connect_async(&url).await.unwrap();
        assert!(connect_async(&url).await.is_err());
        socket
            .send(Message::Text("x".repeat(1024).into()))
            .await
            .unwrap();
        assert_eq!(socket.next().await.unwrap().unwrap().len(), 1024);
        socket
            .send(Message::Text("x".repeat(1025).into()))
            .await
            .unwrap();
        assert!(!matches!(socket.next().await, Some(Ok(Message::Text(_)))));
    }

    #[tokio::test]
    async fn unauthenticated_message_limit_covers_fragmented_frames() {
        let (url, _server) = serve(RpcContinuation::ws(
            WebSocketPolicy::Public,
            None,
            echo,
            Duration::from_secs(30),
        ))
        .await;
        let (mut socket, _) = connect_async(&url).await.unwrap();
        for (opcode, last) in [
            (OpCode::Data(Data::Text), false),
            (OpCode::Data(Data::Continue), true),
        ] {
            socket
                .send(Message::Frame(Frame::message(
                    vec![b'x'; 600],
                    opcode,
                    last,
                )))
                .await
                .unwrap();
        }
        assert!(!matches!(socket.next().await, Some(Ok(Message::Text(_)))));
    }

    #[tokio::test]
    async fn authenticated_local_session_retains_bulk_frames_and_revocation() {
        let open = Arc::new(OpenAuthedContinuations::new());
        let (url, _server) = serve(RpcContinuation::ws_authed(
            open.subscribe_to_kill(None::<&str>),
            WebSocketPolicy::Authenticated,
            None,
            echo,
            Duration::from_secs(30),
        ))
        .await;
        let (mut socket, _) = connect_async(&url).await.unwrap();
        let data = "x".repeat(2 * 1024 * 1024);
        socket
            .send(Message::Text(data.clone().into()))
            .await
            .unwrap();
        assert_eq!(
            socket.next().await.unwrap().unwrap().into_text().unwrap(),
            data
        );
        open.kill(&None);
        assert!(!matches!(socket.next().await, Some(Ok(Message::Text(_)))));
    }

    #[tokio::test]
    async fn authenticated_nonrevocable_continuations_retain_bulk_frames() {
        let (url, _server) = serve(RpcContinuation::ws(
            WebSocketPolicy::Authenticated,
            None,
            echo,
            Duration::from_secs(30),
        ))
        .await;
        let (mut socket, _) = connect_async(&url).await.unwrap();
        let data = "x".repeat(2 * 1024 * 1024);
        socket
            .send(Message::Text(data.clone().into()))
            .await
            .unwrap();
        assert_eq!(
            socket.next().await.unwrap().unwrap().into_text().unwrap(),
            data
        );
    }

    #[tokio::test]
    async fn revocation_tracking_does_not_exempt_public_continuations_from_limits() {
        let open = Arc::new(OpenAuthedContinuations::new());
        let (url, _server) = serve(RpcContinuation::ws_authed(
            open.subscribe_to_kill("public"),
            WebSocketPolicy::Public,
            None,
            echo,
            Duration::from_secs(30),
        ))
        .await;
        let (mut socket, _) = connect_async(&url).await.unwrap();
        socket
            .send(Message::Text("x".repeat(1024).into()))
            .await
            .unwrap();
        assert_eq!(socket.next().await.unwrap().unwrap().len(), 1024);
        socket
            .send(Message::Text("x".repeat(1025).into()))
            .await
            .unwrap();
        assert!(!matches!(socket.next().await, Some(Ok(Message::Text(_)))));
    }

    #[tokio::test]
    async fn unauthenticated_outbound_bulk_remains_allowed() {
        let (url, _server) = serve(RpcContinuation::ws(
            WebSocketPolicy::Public,
            None,
            |mut socket| async move {
                socket
                    .send(axum::extract::ws::Message::Text(
                        "x".repeat(2 * 1024 * 1024).into(),
                    ))
                    .await
                    .unwrap();
            },
            Duration::from_secs(30),
        ))
        .await;
        let (mut socket, _) = connect_async(&url).await.unwrap();
        assert_eq!(socket.next().await.unwrap().unwrap().len(), 2 * 1024 * 1024);
    }

    #[tokio::test]
    async fn unauthenticated_blocked_sends_time_out_including_split_sink() {
        for split in [false, true] {
            let (pending_send, pending_recv) = oneshot::channel();
            let (done_send, done_recv) = oneshot::channel();
            let (url, _server) = serve(RpcContinuation::ws(
                WebSocketPolicy::Public,
                None,
                move |mut socket| async move {
                    let message =
                        axum::extract::ws::Message::Binary(vec![0; 16 * 1024 * 1024].into());
                    let result = if split {
                        let (mut sink, _stream) = socket.split();
                        let mut send = Box::pin(sink.send(message));
                        assert!(futures::poll!(&mut send).is_pending());
                        pending_send.send(()).unwrap();
                        send.await
                    } else {
                        let mut send = Box::pin(socket.send(message));
                        assert!(futures::poll!(&mut send).is_pending());
                        pending_send.send(()).unwrap();
                        send.await
                    };
                    done_send.send(result.unwrap_err().to_string()).unwrap();
                },
                Duration::from_secs(30),
            ))
            .await;
            let (_socket, _) = connect_async(&url).await.unwrap();
            pending_recv.await.unwrap();
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(14)).await;
            let mut done_recv = Box::pin(done_recv);
            assert!(futures::poll!(&mut done_recv).is_pending());
            tokio::time::advance(Duration::from_secs(1)).await;
            assert!(
                tokio::time::timeout(Duration::from_secs(1), done_recv)
                    .await
                    .unwrap()
                    .unwrap()
                    .contains("timed out")
            );
            tokio::time::resume();
        }
    }

    #[tokio::test]
    async fn flushed_keepalive_does_not_leave_a_send_deadline() {
        let (started_send, started_recv) = oneshot::channel();
        let (done_send, mut done_recv) = oneshot::channel();
        let (url, _server) = serve(RpcContinuation::ws(
            WebSocketPolicy::Public,
            None,
            move |mut socket| async move {
                let mut receive = Box::pin(socket.recv());
                assert!(futures::poll!(&mut receive).is_pending());
                started_send.send(()).unwrap();
                let _ = receive.await;
                let _ = done_send.send(());
            },
            Duration::from_secs(30),
        ))
        .await;
        let (mut socket, _) = connect_async(&url).await.unwrap();
        started_recv.await.unwrap();
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(30)).await;
        tokio::task::yield_now().await;
        tokio::time::resume();
        assert!(matches!(socket.next().await, Some(Ok(Message::Ping(_)))));
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(16)).await;
        tokio::task::yield_now().await;
        assert!(matches!(
            done_recv.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        tokio::time::resume();
        drop(socket);
        tokio::time::timeout(Duration::from_secs(2), done_recv)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn unauthenticated_successful_send_resets_deadline() {
        let (next_send, next_recv) = oneshot::channel();
        let (url, _server) = serve(RpcContinuation::ws(
            WebSocketPolicy::Public,
            None,
            move |mut socket| async move {
                socket
                    .send(axum::extract::ws::Message::Text("first".into()))
                    .await
                    .unwrap();
                next_recv.await.unwrap();
                socket
                    .send(axum::extract::ws::Message::Text("second".into()))
                    .await
                    .unwrap();
            },
            Duration::from_secs(30),
        ))
        .await;
        let (mut socket, _) = connect_async(&url).await.unwrap();
        assert_eq!(
            socket.next().await.unwrap().unwrap().into_text().unwrap(),
            "first"
        );
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(16)).await;
        next_send.send(()).unwrap();
        tokio::task::yield_now().await;
        tokio::time::resume();
        assert_eq!(
            socket.next().await.unwrap().unwrap().into_text().unwrap(),
            "second"
        );
    }

    #[tokio::test]
    async fn authenticated_blocked_send_has_no_deadline_and_is_revocable() {
        let open = Arc::new(OpenAuthedContinuations::new());
        let (pending_send, pending_recv) = oneshot::channel();
        let (dropped_send, mut dropped_recv) = oneshot::channel::<()>();
        let (url, _server) = serve(RpcContinuation::ws_authed(
            open.subscribe_to_kill(Some("key")),
            WebSocketPolicy::Authenticated,
            None,
            move |mut socket| async move {
                let _dropped = dropped_send;
                let mut send = Box::pin(socket.send(axum::extract::ws::Message::Binary(
                    vec![0; 16 * 1024 * 1024].into(),
                )));
                assert!(futures::poll!(&mut send).is_pending());
                pending_send.send(()).unwrap();
                let _ = send.await;
            },
            Duration::from_secs(30),
        ))
        .await;
        let (_socket, _) = connect_async(&url).await.unwrap();
        pending_recv.await.unwrap();
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(16)).await;
        tokio::task::yield_now().await;
        assert!(matches!(
            dropped_recv.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        open.kill(&Some("key"));
        assert!(dropped_recv.await.is_err());
    }

    #[tokio::test]
    async fn unauthenticated_close_handshake_is_bounded() {
        let (started_send, started_recv) = oneshot::channel();
        let (done_send, done_recv) = oneshot::channel();
        let (url, _server) = serve(RpcContinuation::ws(
            WebSocketPolicy::Public,
            None,
            move |socket| async move {
                let mut close = Box::pin(socket.normal_close("complete"));
                assert!(futures::poll!(&mut close).is_pending());
                started_send.send(()).unwrap();
                done_send.send(close.await.is_err()).unwrap();
            },
            Duration::from_secs(30),
        ))
        .await;
        let (_socket, _) = connect_async(&url).await.unwrap();
        started_recv.await.unwrap();
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(15)).await;
        assert!(
            tokio::time::timeout(Duration::from_secs(1), done_recv)
                .await
                .unwrap()
                .unwrap()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn expired_websocket_is_unavailable_without_consuming_rest() {
        let continuations = RpcContinuations::new();
        let guid = Guid::new();
        continuations
            .add(
                guid.clone(),
                RpcContinuation::ws(WebSocketPolicy::Public, None, echo, Duration::from_secs(1)),
            )
            .await;
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(2)).await;
        tokio::task::yield_now().await;
        assert!(continuations.get_ws_handler(&guid).await.is_none());
        let rest = Guid::new();
        continuations
            .add(
                rest.clone(),
                RpcContinuation::rest(
                    |_| async { Ok(StatusCode::OK.into_response()) },
                    Duration::from_secs(30),
                ),
            )
            .await;
        assert!(continuations.get_ws_handler(&rest).await.is_none());
        assert!(continuations.get_rest_handler(&rest).await.is_some());
        assert!(continuations.get_rest_handler(&rest).await.is_none());
    }
}
