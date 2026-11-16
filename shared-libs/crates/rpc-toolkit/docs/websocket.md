# WebSocket JSON-RPC and response streams

Enable the `websocket` Cargo feature and mount `Server::for_websocket()` or
`HttpServer::for_websocket()` as an Axum GET handler. The latter preserves the
server's configured middleware. HTTP, CLI, and line-delimited socket transports
remain available independently.

Each WebSocket text message contains one JSON-RPC request or a batch. Calls may
complete out of order; notifications execute without response messages. Missing
`params` means an empty object. Binary RPC messages are rejected. WebSocket
Ping/Pong frames provide keepalive.

## Connection upgraders

`WebSocketServer::with_upgrader` installs an application-owned `WebSocketUpgrader`.
After the HTTP upgrade middleware, it receives the connection context, request
parts, raw socket, and logical limits. No RPC is dispatched until it returns `UpgradedWebSocket`;
an error drops the connection. The upgrader owns its handshake deadline and returns
both a connection-scoped `Arc<Context>` and a `BoxWebSocket` message transport.
RPC middleware reads that context; `request_context` creates owned handler context
from it for each call. The connection's private server clone uses this factory,
while HTTP response middleware retains its pre-handshake context. Context changes
do not affect the original server's factory or another connection.

`WebSocketTransport` retains text, binary and control-frame semantics. An adapter
can authenticate or transform wire data without replacing RPC dispatch, stream
ownership, cancellation or heartbeat handling. `max_wire_message_size` supplies
the raw frame/message allowance for the configured logical budget. The RPC actor
also enforces the logical data-message limit after adaptation and before parsing;
wire overhead must not expand the RPC payload budget. Adapters must bound partial
message buffering and preserve control frames. Without an upgrader, the raw socket
and its original context are used directly.

## Opting into a response stream

A client adds `rpcstream: "1.0"` to an ordinary request with a non-null ID:

```json
{ "jsonrpc": "2.0", "rpcstream": "1.0", "id": 42, "method": "db.subscribe", "params": { "osVersion": "0.4.1.3" } }
```

The initiating response is an ordinary JSON-RPC response, with its own result type:

```json
{ "jsonrpc": "2.0", "id": 42, "result": { "id": 100, "value": {} } }
```

Subsequent events use the same request ID and a separate protocol discriminator:

```json
{ "rpcstream": "1.0", "id": 42, "result": { "id": 101, "patch": [] }, "end": false }
```

Rust clients decode initiating replies with `RpcResponse` and subsequent replies with
`RpcStreamResponse`. The latter owns the stream envelope's encoding and validation for
both the server and clients, including non-null IDs and exclusive result/error fields.

## Producer handle

`HandlerArgs<Context, Params, InheritedParams, Item, Final>::rpc_stream` contains an optional
`RpcStream<Item, Final>` producer. `Item` defaults to `Never` for a non-streaming endpoint;
`Final` defaults to `()`. The initiating result remains the handler's separate `Ok` type. It is
absent without the client's opt-in and on other transports. Existing handlers
can use that distinction to retain their legacy continuation path; the transport
never interprets a legacy GUID as a response stream.

The handle has one owner and is not cloneable. Move it into a producer task or
ordinary thread, or retain it after the handler returns. `HandlerArgs` is also
not cloneable; clone context and parameter fields separately when needed.

- `send(value).await` borrows the declared `Item`, serializes immediately and waits
  for bounded queue capacity. Borrowed or non-`Send` inputs need no `'static` bound. A serialization failure returns an internal
  error without enqueuing an event; the handle remains usable.
- `forward(source).await` forwards fallible item streams with the same synchronous
  serialization boundary. It stops when the source ends, fails, or the transport closes.
  It does not consume the producer or choose its terminal value; the caller closes it
  separately and may select on application revocation around forwarding.
- `blocking_send(value)` performs the same operation from an
  ordinary thread. Tokio panics if it is used inside an async execution context.
- `close<E: Into<RpcError>>(Result<Final, E>)` consumes the handle and queues
  one terminal result. It is synchronous and uses a reserved slot even when the
  event buffer is full. A terminal serialization failure becomes an internal
  error on the wire.
- `close_and_flush(result).await` queues the same reserved terminal and waits for
  the connection actor's transport flush. Cancellation, initial failure and failed writes
  release the waiter with `RPCSTREAM_CANCELLED_ERROR`. Serialization still occurs
  synchronously. Await outside the initiating handler: its response precedes the terminal.
- `closed().await` resolves when the transport drops the receiver. Pending sends
  then fail with `RPCSTREAM_CANCELLED_ERROR`; producers waiting on other work can
  select on `closed()` to stop promptly.

A successful send or close accepts data locally; it is not a delivery
acknowledgment. `close_and_flush` confirms a terminal transport flush, not peer receipt
or application success. Its receipt stays internal; the wire protocol is unchanged.
Applications own their producer tasks and threads and must act
on transport closure. The runnable [example](../examples/websocket.rs) exposes
`watch` from an async task and `watch-thread` from an ordinary thread.

The initiating result, stream item and terminal value have independent serializable types.
Generated method trees export `_ITEM` and `_FINAL` output schemas alongside `_RETURN`.
`RpcStreamItemType`, `RpcStreamFinalType` and `RpcIsStreaming` project these facts by method path;
opted-out TypeScript bindings leave them unknown. For example, an explicit successful close can finish with a
summary rather than a null result:

```json
{ "rpcstream": "1.0", "id": 42, "result": { "count": 5 }, "end": true }
```

`close(Err(error))` produces a terminal JSON-RPC-shaped error instead. Dropping
the producer without calling `close` drains queued events and then emits this
fixed cancellation error, including when a handler ignores its handle:

```json
{ "rpcstream": "1.0", "id": 42, "error": { "code": -32800, "message": "Response stream canceled" }, "end": true }
```

## Ordering and cancellation

The connection drains the event queue after its successful initiating response
is sent. For a batch, the entire batch response precedes every associated event
and terminal result. Awaiting more events than the buffer can hold from inside
the initiating handler can prevent that response completing; run a producing
loop independently. Synchronous `close` does not wait for buffer space.

Accepted events precede the terminal result or drop-cancellation error. After a
terminal event, no further events are sent for that request ID. An initiating
error drops the receiver and creates no response stream. Client cancellation
and socket loss can discard queued events and terminal results.

The client cancels a request or response stream with a standalone control message:

```json
{ "rpcstream": "1.0", "close": 42 }
```

Canceling an established stream drops its receiver and acknowledges closure with
`result: null, end: true`. Canceling an initiating stream request still in
progress aborts the handler and returns a JSON-RPC error with code `-32800`,
without a response stream. If its initiating result is already buffered inside
an unfinished batch, cancellation drops its receiver; the initiating response
remains part of the batch and no stream events follow it. Unknown or already-ended
IDs are ignored. Canceling a pending ordinary call drops its handler future and sends no
cancellation response. Its batch siblings still receive their results. An ordinary result
already buffered in an unfinished batch can still arrive.

Cancellation drops the connection-owned handler future before awaiting any subsequent
response write, including a stalled sibling batch flush. It is never polled again. Closing
or losing the WebSocket drops every pending handler future, including notifications.
Applications retain responsibility for independently accepted work and external side effects;
dropping a future cannot roll those back or preempt an already running blocking callback.

Clients own per-request reply deadlines. A timeout cancels that ID through the same control
message, without closing healthy siblings. Clients must ignore subsequent replies or events
for a canceled ID; no cancellation acknowledgment is required. Connection keepalive and
write deadlines remain independent of a handler's response time.

The client must tolerate events already in flight when it cancels a request. IDs
are scoped to the connection and must remain unique while their initiating call
or response stream is active. Reusing an active ID returns an invalid-request
error without replacing the original operation. String and numeric IDs are
separate; integer IDs retain their JSON precision. The version discriminator is
the string `"1.0"`, including in close messages. A null ID can identify an ordinary
call; response-stream opt-in still requires a non-null ID.

## Ownership and limits

One connection task owns all initiating calls, buffered replies, and response
receivers. The transport creates no per-stream forwarding task. Each opted-in
request has one bounded event channel and one reserved terminal slot. Closing or
losing the socket drops pending handlers and receivers, waking producers.
Applications must also bound upstream queues such as database subscriptions.

`WebSocketOptions` bounds in-flight calls/buffered initiating responses, active
streams, queued events per stream, inbound message/frame size, and stalled sends.
Defaults are 64 in-flight calls, 64 streams, 16 queued events per stream plus its
terminal slot, 1 MiB inbound logical messages, and a 15-second send deadline. Plain
connections apply that same byte limit to raw messages/frames; an upgrader derives
the raw allowance for its wire format. The
buffer limit counts serialized values, not bytes, and must fit Tokio's bounded
channel capacity. These are resource limits, not authentication decisions. The message/frame byte limit is
server-inbound: outgoing dumps, results, and events may be larger. Native receiving clients must
configure both their own message and frame limits for the expected response size; a large dump
can exceed a client's frame limit even when it fits its message limit.

HTTP upgrade bodies use the separate `HttpServer` request-body limit (1 MiB by default),
enforced before upgrade middleware runs. `with_max_request_body_size` configures that HTTP limit.

A batch larger than the in-flight limit closes the connection with policy code 1008. Capacity rejection does not execute rejected calls. Receivers are polled
fairly; a stalled network write ends the connection after its send deadline.

HTTP middleware runs once during upgrade. Each RPC call clones that prepared
middleware state and runs typed request middleware before dispatch. Per-call state mutations
do not leak to other calls or batch members. Middleware response
rewrites apply to the initiating result; stream event payloads follow their own
handler-defined contract. HTTP-response middleware runs on the upgrade response,
not on later WebSocket frames. RPC response middleware can add per-call fields with
`RpcResponse::add_meta`; initiating replies carry them in a top-level `meta` object on both
HTTP and WebSocket transports. Stream events and terminal replies retain their handler-defined
contract and do not rerun RPC response middleware.

`WebSocketRequest` marks the upgrade request in HTTP extensions. It identifies the transport,
not an authenticated caller; HTTP body-authentication adapters can avoid treating the upgrade
body as proof for later messages.

Applications own authentication, authorization, origin policy, and revocation.
Authentication of an upgrade must authorize the connection, not merely an
unrelated HTTP body. The connection's RPC middleware still authorizes individual
methods. A producer must observe any application-specific revocation signal.
Unauthenticated continuation limits must follow verified authentication or
explicit endpoint policy, not whether a legacy continuation happens to use `ws`
or `ws_authed`.
