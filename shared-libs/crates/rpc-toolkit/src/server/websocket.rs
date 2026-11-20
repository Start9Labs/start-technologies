use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequestParts, Request};
use axum::handler::Handler;
use axum::response::{IntoResponse, Response};
use futures::future::BoxFuture;
use futures::stream::Once;
use futures::{FutureExt, Sink, SinkExt, Stream, StreamExt};
use imbl_value::imbl::Vector;
use imbl_value::Value;
use serde::Serialize;
use tokio::time::Instant;
use tokio_stream::StreamMap;
use yajrc::{Id, RpcError};

use super::rpcstream::{stream_event, ResponseEvents, RPCSTREAM_CANCELLED_ERROR};
use crate::util::{invalid_request, parse_error};
use crate::{
    Context, DynMiddleware, HttpServer, RpcRequest, RpcResponse, RpcStream, Server,
    RPCSTREAM_VERSION,
};

/// Connection-wide limits, independent of application authentication policy.
#[derive(Clone, Debug)]
pub struct WebSocketOptions {
    /// Pending calls and buffered initiating responses per connection.
    pub max_in_flight: NonZeroUsize,
    /// Concurrent opted-in calls and established response streams.
    pub max_streams: NonZeroUsize,
    /// Queued non-terminal events per stream; must fit Tokio's bounded-channel capacity.
    /// Terminal results have a separate reserved slot.
    pub stream_buffer_capacity: NonZeroUsize,
    /// Maximum inbound logical message size; also bounds raw frames without an upgrader.
    pub max_message_size: NonZeroUsize,
    /// Deadline for each socket write or flush.
    pub send_timeout: Duration,
}

impl Default for WebSocketOptions {
    fn default() -> Self {
        Self {
            max_in_flight: NonZeroUsize::new(64).unwrap(),
            max_streams: NonZeroUsize::new(64).unwrap(),
            stream_buffer_capacity: NonZeroUsize::new(16).unwrap(),
            max_message_size: NonZeroUsize::new(1024 * 1024).unwrap(),
            send_timeout: Duration::from_secs(15),
        }
    }
}

/// A message transport retaining WebSocket data and control frame semantics.
pub trait WebSocketTransport:
    Stream<Item = Result<Message, axum::Error>> + Sink<Message, Error = axum::Error> + Send + Unpin
{
}

impl<T> WebSocketTransport for T where
    T: Stream<Item = Result<Message, axum::Error>>
        + Sink<Message, Error = axum::Error>
        + Send
        + Unpin
{
}

/// A connection-owned transport supplied by a WebSocket upgrader.
pub type BoxWebSocket = Box<dyn WebSocketTransport>;

/// The context and message transport admitted after a successful handshake.
pub struct UpgradedWebSocket<C: Context> {
    /// Connection-scoped state made available to RPC middleware and handlers.
    pub context: Arc<C>,
    /// Carries logical messages and WebSocket control frames after admission.
    pub socket: BoxWebSocket,
}

/// Establishes a transport before any RPC request is dispatched.
/// Implementations own handshake deadlines and return authenticated context only after verification.
pub trait WebSocketUpgrader<C: Context>: Send + Sync + 'static {
    /// Converts the logical byte budget into the required inbound wire budget.
    fn max_wire_message_size(&self, logical: NonZeroUsize) -> NonZeroUsize;

    /// Creates owned handler context from the admitted connection context.
    fn request_context(&self, context: &C) -> C;

    /// Returns admitted context and transport; an error closes the connection.
    fn upgrade(
        &self,
        context: Arc<C>,
        request: axum::http::request::Parts,
        socket: WebSocket,
        options: WebSocketOptions,
    ) -> BoxFuture<'static, Result<UpgradedWebSocket<C>, axum::Error>>;
}

/// JSON-RPC calls and opted-in response streams on one WebSocket.
///
/// HTTP middleware runs during the upgrade. Each call receives its own clone of
/// that middleware state and runs the normal RPC request/response middleware.
pub struct WebSocketServer<C: Context> {
    server: HttpServer<C>,
    options: WebSocketOptions,
    upgrader: Option<Arc<dyn WebSocketUpgrader<C>>>,
}

impl<C: Context> Clone for WebSocketServer<C> {
    fn clone(&self) -> Self {
        Self {
            server: self.server.clone(),
            options: self.options.clone(),
            upgrader: self.upgrader.clone(),
        }
    }
}

impl<C: Context> Server<C> {
    /// Serves calls and opted-in response streams over WebSocket.
    pub fn for_websocket(self) -> WebSocketServer<C> {
        self.for_http().for_websocket()
    }
}

impl<C: Context> HttpServer<C> {
    /// Preserves configured middleware for the WebSocket transport.
    pub fn for_websocket(self) -> WebSocketServer<C> {
        WebSocketServer {
            server: self,
            options: WebSocketOptions::default(),
            upgrader: None,
        }
    }
}

/// Marks HTTP requests routed through the WebSocket transport, not authenticated callers.
#[derive(Clone, Copy, Debug)]
pub struct WebSocketRequest;

impl<C: Context> WebSocketServer<C> {
    /// Sets resource limits for each accepted connection.
    pub fn with_options(mut self, options: WebSocketOptions) -> Self {
        self.options = options;
        self
    }

    /// Installs a connection handshake and transport adapter before RPC dispatch.
    pub fn with_upgrader(mut self, upgrader: impl WebSocketUpgrader<C>) -> Self {
        self.upgrader = Some(Arc::new(upgrader));
        self
    }

    /// Runs upgrade middleware and accepts a WebSocket connection.
    pub fn handle(&self, mut request: Request) -> BoxFuture<'static, Response> {
        request.extensions_mut().insert(WebSocketRequest);
        let server = self.clone();
        async move {
            let (context, mut middleware, _) =
                match server.server.begin_http_request(&mut request).await {
                    Ok(prepared) => prepared,
                    Err(response) => return response,
                };
            let (mut parts, _) = request.into_parts();
            let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
                Ok(upgrade) => upgrade,
                Err(error) => return error.into_response(),
            };
            let context = Arc::new(context);
            let socket_context = context.clone();
            let socket_middleware = middleware.clone();
            let options = server.options.clone();
            let wire_limit = server
                .upgrader
                .as_ref()
                .map_or(options.max_message_size, |upgrader| {
                    upgrader.max_wire_message_size(options.max_message_size)
                });
            let mut response = upgrade
                .read_buffer_size(2048)
                .write_buffer_size(0)
                .max_message_size(wire_limit.get())
                .max_frame_size(wire_limit.get())
                .on_upgrade(move |socket| async move {
                    let mut connection_server = server.server;
                    let admitted = match server.upgrader {
                        Some(upgrader) => {
                            let admitted = match upgrader
                                .upgrade(socket_context, parts, socket, options.clone())
                                .await
                            {
                                Ok(admitted) => admitted,
                                Err(_) => return,
                            };
                            let context = admitted.context.clone();
                            connection_server.inner.make_ctx = Arc::new(move || {
                                futures::future::ready(Ok(upgrader.request_context(&context)))
                                    .boxed()
                            });
                            admitted
                        }
                        None => UpgradedWebSocket {
                            context: socket_context,
                            socket: Box::new(socket),
                        },
                    };
                    let mut connection = Connection::new(
                        admitted.socket,
                        connection_server,
                        admitted.context,
                        socket_middleware,
                        options,
                    );
                    let _ = connection.run().await;
                });
            HttpServer::<C>::finish_http_response(&context, &mut middleware, &mut response).await;
            response
        }
        .boxed()
    }
}

impl<C: Context> Handler<(), ()> for WebSocketServer<C> {
    type Future = BoxFuture<'static, Response>;
    fn call(self, request: Request, _: ()) -> Self::Future {
        self.handle(request)
    }
}

struct CompletedCall {
    group: usize,
    id: Option<Id>,
    response: RpcResponse,
    respond: bool,
    source: Option<ResponseEvents>,
}

struct ResponseGroup {
    batch: bool,
    remaining: usize,
    responses: Vec<RpcResponse>,
    ids: Vec<Id>,
}

enum CallState {
    Pending(usize),
    Ready(Option<ResponseEvents>),
}

struct TrackedCall {
    group: usize,
    opted_in: bool,
    state: CallState,
}

struct Connection<C: Context> {
    socket: BoxWebSocket,
    server: HttpServer<C>,
    context: Arc<C>,
    middleware: Vector<DynMiddleware<C>>,
    options: WebSocketOptions,
    calls: StreamMap<(usize, usize), Once<BoxFuture<'static, CompletedCall>>>,
    active: HashMap<Id, TrackedCall>,
    streams: StreamMap<Id, ResponseEvents>,
    groups: HashMap<usize, ResponseGroup>,
    next_group: usize,
}

impl<C: Context> Connection<C> {
    fn new(
        socket: BoxWebSocket,
        server: HttpServer<C>,
        context: Arc<C>,
        middleware: Vector<DynMiddleware<C>>,
        options: WebSocketOptions,
    ) -> Self {
        Self {
            socket,
            server,
            context,
            middleware,
            options,
            calls: StreamMap::new(),
            active: HashMap::new(),
            streams: StreamMap::new(),
            groups: HashMap::new(),
            next_group: 0,
        }
    }

    async fn run(&mut self) -> Result<(), ()> {
        let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
        heartbeat.tick().await;
        let mut last_received = Instant::now();
        loop {
            tokio::select! {
                message = self.socket.next() => {
                    last_received = Instant::now();
                    let bytes = match &message {
                        Some(Ok(Message::Text(text))) => text.len(),
                        Some(Ok(Message::Binary(data))) => data.len(),
                        _ => 0,
                    };
                    if bytes > self.options.max_message_size.get() {
                        self.send_message(Message::Close(Some(CloseFrame {
                            code: 1009,
                            reason: "RPC message limit exceeded".into(),
                        }))).await?;
                        return Err(());
                    }
                    match message {
                        Some(Ok(Message::Text(text))) => self.receive_text(&text).await?,
                        Some(Ok(Message::Ping(_))) => {
                            tokio::time::timeout(self.options.send_timeout, self.socket.flush()).await.map_err(|_| ())?.map_err(|_| ())?;
                        }
                        Some(Ok(Message::Pong(_))) => (),
                        Some(Ok(Message::Binary(_))) => {
                            self.send(&RpcResponse::from(invalid_request("WebSocket RPC requires JSON text messages"))).await?;
                        }
                        Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return Ok(()),
                    }
                }
                completed = self.calls.next(), if !self.calls.is_empty() => {
                    if let Some((key, completed)) = completed {
                        self.calls.remove(&key);
                        self.complete(completed).await?;
                    }
                }
                event = self.streams.next(), if !self.streams.is_empty() => {
                    if let Some((id, (result, end, flushed))) = event {
                        if end { self.streams.remove(&id); }
                        self.send(&stream_event(id, result, end)).await?;
                        if let Some(flushed) = flushed { let _ = flushed.send(()); }
                    }
                }
                _ = heartbeat.tick() => {
                    if last_received.elapsed() >= Duration::from_secs(90) { return Ok(()); }
                    self.send_message(Message::Ping(Default::default())).await?;
                }
            }
        }
    }

    async fn send<T: Serialize + Sync>(&mut self, value: &T) -> Result<(), ()> {
        let text = serde_json::to_string(value).map_err(|_| ())?;
        self.send_message(Message::Text(text.into())).await
    }

    async fn send_message(&mut self, message: Message) -> Result<(), ()> {
        tokio::time::timeout(self.options.send_timeout, self.socket.send(message))
            .await
            .map_err(|_| ())?
            .map_err(|_| ())
    }

    async fn send_replies(
        &mut self,
        mut responses: Vec<RpcResponse>,
        batch: bool,
    ) -> Result<(), ()> {
        if responses.is_empty() {
            Ok(())
        } else if batch {
            self.send(&responses).await
        } else {
            self.send(&responses.pop().unwrap()).await
        }
    }

    async fn receive_text(&mut self, text: &str) -> Result<(), ()> {
        let value: Value = match serde_json::from_str(text) {
            Ok(value) => value,
            Err(error) => return self.send(&RpcResponse::from(parse_error(error))).await,
        };
        if value.get("close").is_some() && value.get("jsonrpc").is_none() {
            if value.get("rpcstream").and_then(Value::as_str) != Some(RPCSTREAM_VERSION) {
                return self
                    .send(&RpcResponse::from(invalid_request(
                        "unsupported rpcstream version",
                    )))
                    .await;
            }
            let id: Id = match value
                .get("close")
                .cloned()
                .map(imbl_value::from_value)
                .transpose()
            {
                Ok(Some(id)) => id,
                _ => {
                    return self
                        .send(&RpcResponse::from(invalid_request(
                            "cancellation requires a request ID",
                        )))
                        .await;
                }
            };
            return self.cancel_request(id).await;
        }
        let batch = value.is_array();
        let requests: Vec<Value> = if let Some(values) = value.as_array() {
            values.iter().cloned().collect()
        } else {
            vec![value]
        };
        if requests.is_empty() {
            return self
                .send(&RpcResponse::from(yajrc::INVALID_REQUEST_ERROR))
                .await;
        }
        if requests.len() > self.options.max_in_flight.get() {
            self.send_message(Message::Close(Some(CloseFrame {
                code: 1008,
                reason: "RPC batch exceeds the request limit".into(),
            })))
            .await?;
            return Err(());
        }
        let requests: Vec<_> = requests.into_iter().map(decode_request).collect();
        let in_flight: usize = self
            .groups
            .values()
            .map(|group| group.remaining + group.responses.len())
            .sum();
        if requests.len() + in_flight.max(self.calls.len()) > self.options.max_in_flight.get() {
            let responses: Vec<_> = requests
                .into_iter()
                .filter_map(|request| match request {
                    Ok((request, _)) => request.id.map(|id| {
                        RpcResponse::new(
                            Some(id),
                            Err(limit_error("WebSocket request limit reached")),
                        )
                    }),
                    Err(response) => Some(response),
                })
                .collect();
            return self.send_replies(responses, batch).await;
        }
        let group = self.next_group;
        self.next_group += 1;
        self.groups.insert(
            group,
            ResponseGroup {
                batch,
                remaining: requests.len(),
                responses: Vec::new(),
                ids: Vec::new(),
            },
        );
        for (member, request) in requests.into_iter().enumerate() {
            match request {
                Ok((request, opted_in)) => {
                    self.start_call(group, member, request, opted_in).await?
                }
                Err(response) => {
                    self.complete(CompletedCall {
                        group,
                        id: None,
                        response,
                        respond: true,
                        source: None,
                    })
                    .await?
                }
            }
        }
        Ok(())
    }

    async fn start_call(
        &mut self,
        group: usize,
        member: usize,
        request: RpcRequest,
        opted_in: bool,
    ) -> Result<(), ()> {
        let id = request.id.clone();
        let rejection = if id
            .as_ref()
            .is_some_and(|id| self.active.contains_key(id) || self.streams.contains_key(id))
        {
            Some(invalid_request("request ID is already active"))
        } else if opted_in
            && self.streams.len() + self.active.values().filter(|call| call.opted_in).count()
                >= self.options.max_streams.get()
        {
            Some(limit_error("WebSocket stream limit reached"))
        } else {
            None
        };
        if let Some(error) = rejection {
            return self
                .complete(CompletedCall {
                    group,
                    id: None,
                    response: RpcResponse::new(id, Err(error)),
                    respond: true,
                    source: None,
                })
                .await;
        }
        let (registration, receiver) = if opted_in {
            let (registration, receiver) = RpcStream::new(self.options.stream_buffer_capacity);
            (Some(registration), Some(receiver))
        } else {
            (None, None)
        };
        let server = self.server.clone();
        let context = self.context.clone();
        let mut middleware = self.middleware.clone();
        let call_id = id.clone();
        let call = async move {
            let response = server
                .process_rpc_request_with_stream(&context, &mut middleware, request, registration)
                .await;
            let source = if response.result.is_ok() {
                receiver
            } else {
                None
            };
            let respond = call_id.is_some();
            CompletedCall {
                group,
                id: call_id,
                response,
                respond,
                source,
            }
        };
        if let Some(id) = id {
            self.groups.get_mut(&group).unwrap().ids.push(id.clone());
            self.active.insert(
                id,
                TrackedCall {
                    group,
                    opted_in,
                    state: CallState::Pending(member),
                },
            );
        }
        self.calls
            .insert((group, member), futures::stream::once(call.boxed()));
        Ok(())
    }

    async fn complete(&mut self, completed: CompletedCall) -> Result<(), ()> {
        let CompletedCall {
            group,
            id,
            response,
            respond,
            source,
        } = completed;
        if let Some(id) = id {
            self.active.get_mut(&id).unwrap().state = CallState::Ready(source);
        }
        if respond {
            self.groups
                .get_mut(&group)
                .unwrap()
                .responses
                .push(response);
        }
        let response_group = self.groups.get_mut(&group).unwrap();
        response_group.remaining -= 1;
        if response_group.remaining != 0 {
            return Ok(());
        }
        let group = self.groups.remove(&group).unwrap();
        self.send_replies(group.responses, group.batch).await?;
        for id in group.ids {
            let call = self.active.remove(&id).unwrap();
            if let CallState::Ready(Some(source)) = call.state {
                self.streams.insert(id, source);
            }
        }
        Ok(())
    }

    async fn cancel_request(&mut self, id: Id) -> Result<(), ()> {
        if self.streams.remove(&id).is_some() {
            return self.send(&stream_event(id, Ok(Value::Null), true)).await;
        }
        let Some(call) = self.active.get_mut(&id) else {
            return Ok(());
        };
        let pending = match &mut call.state {
            CallState::Pending(member) => {
                // Cancellation must release handler resources before any subsequent response write.
                self.calls.remove(&(call.group, *member));
                Some((call.group, call.opted_in))
            }
            CallState::Ready(source) => {
                source.take();
                None
            }
        };
        if let Some((group, opted_in)) = pending {
            self.complete(CompletedCall {
                group,
                id: Some(id.clone()),
                response: RpcResponse::new(Some(id), Err(RPCSTREAM_CANCELLED_ERROR)),
                respond: opted_in,
                source: None,
            })
            .await?;
        }
        Ok(())
    }
}

fn request_id(value: &Value) -> Option<Id> {
    value
        .get("id")
        .cloned()
        .and_then(|id| imbl_value::from_value(id).ok())
}

fn decode_request(mut value: Value) -> Result<(RpcRequest, bool), RpcResponse> {
    let id = request_id(&value);
    let parse = || -> Result<bool, RpcError> {
        if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Err(invalid_request("jsonrpc must be 2.0"));
        }
        let opted_in = match value.get("rpcstream") {
            None => false,
            Some(version) if version.as_str() == Some(RPCSTREAM_VERSION) => true,
            _ => return Err(invalid_request("unsupported rpcstream version")),
        };
        if opted_in && id.as_ref().is_none_or(|id| *id == Id::Null) {
            return Err(invalid_request("rpcstream requires a non-null request ID"));
        }
        if value
            .get("params")
            .is_some_and(|params| !params.is_object() && !params.is_array())
        {
            return Err(invalid_request("params must be an object or array"));
        }
        Ok(opted_in)
    };
    let opted_in = parse().map_err(|error| RpcResponse::new(id.clone(), Err(error)))?;
    if value.get("params").is_none() {
        value["params"] = imbl_value::json!({});
    }
    let request = imbl_value::from_value(value)
        .map_err(|error| RpcResponse::new(id, Err(invalid_request(error))))?;
    Ok((request, opted_in))
}

fn limit_error(message: &'static str) -> RpcError {
    RpcError {
        code: -32000,
        message: message.into(),
        data: None,
    }
}
