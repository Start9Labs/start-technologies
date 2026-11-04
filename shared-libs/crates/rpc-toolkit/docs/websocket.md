# WebSocket JSON-RPC and response streams

Enable the `websocket` Cargo feature and mount `Server::for_websocket()` or
`HttpServer::for_websocket()` as an Axum GET handler. The latter preserves the
server's configured middleware. HTTP, CLI, and line-delimited socket transports
remain available independently.

Each WebSocket text message contains one JSON-RPC request or a batch. Calls may
complete out of order; notifications execute without response messages. Missing
`params` means an empty object. Binary RPC messages are rejected. WebSocket
Ping/Pong frames provide keepalive.

## Opting into a response stream

A client adds `rpcstream: "1.0"` to an ordinary request with a non-null ID:

```json
{ "jsonrpc": "2.0", "rpcstream": "1.0", "id": 42, "method": "db.watch", "params": {} }
```

The initiating response is an ordinary JSON-RPC response, with its own result type:

```json
{ "jsonrpc": "2.0", "id": 42, "result": { "id": 100, "value": {} } }
```

Subsequent events use the same request ID and a separate protocol discriminator:

```json
{ "rpcstream": "1.0", "id": 42, "result": { "id": 101, "patch": [] }, "end": false }
```

A handler accesses `HandlerArgs::rpc_stream` to attach one source with
`RpcStream::attach`. The capability is absent without the client's opt-in and on
other transports. Existing handlers can use that distinction to retain their
legacy continuation path; the transport never interprets a legacy GUID as a
response stream.

`attach` accepts a typed `Stream<Item = Result<T, E>>`, where `T: Serialize` and
`E: Into<RpcError>`. The initiating result and item type are independent. The
transport serializes stream items, preserving JSON-RPC's result/error shapes.
Attach the source before the handler returns. Registering twice, or after the
initiating call has finished, fails. Cloning the capability does not create
another stream.

The source is not polled until its successful initiating response has been sent.
For a batch, that means the entire batch response precedes every associated
stream event. An initiating error drops an attached source and creates no stream.
An opted-in successful call without an attached source creates an empty stream.

Source completion sends a terminal event with `result: null`:

```json
{ "rpcstream": "1.0", "id": 42, "result": null, "end": true }
```

A source or item-serialization error sends a terminal error instead:

```json
{ "rpcstream": "1.0", "id": 42, "error": { "code": -32603, "message": "Internal error" }, "end": true }
```

After a terminal event, no more events are sent for that stream. The connection
remains open for other calls and streams.

## Closing a stream

The client sends a standalone control message:

```json
{ "rpcstream": "1.0", "close": 42 }
```

Closing an established stream drops its source and acknowledges closure with the
terminal `result: null` event. Closing an initiating stream request still in
progress cancels the handler and produces a JSON-RPC error with code `-32800`,
without a response stream. If its initiating result is already buffered inside
an unfinished batch, closure drops the source; the initiating response remains
part of the batch and no stream events follow it. Unknown or already-ended IDs
are ignored. Closing an ordinary, non-opted-in call has no effect.

The client must tolerate events already in flight when it closes a stream. IDs
are scoped to the connection, and must remain unique while their initiating call
or response stream is active. Reusing an active ID returns an invalid-request
error without replacing the original operation. String and numeric IDs are
separate; integer IDs retain their JSON precision. The version discriminator is
the string `"1.0"`, including in close messages.

## Ownership and limits

One connection task owns all initiating calls and stream sources. Closing or
losing the socket drops pending handlers, buffered results, and sources; no
per-stream task survives the connection. Sources are polled directly by the
connection, without unbounded per-stream forwarding queues. Applications must
also bound upstream queues, such as database revision subscriptions.

`WebSocketOptions` bounds in-flight calls/buffered initiating responses, active
streams, inbound message/frame size, and stalled sends. A batch larger than the
in-flight limit closes the connection with policy-violation code 1008. Capacity
rejection does not execute the rejected calls. The sender polls streams fairly;
a stalled network write ends the connection after its send deadline. The default
limits are 64 in-flight calls, 64 streams, 1 MiB inbound messages/frames, and a
15-second send deadline. These are resource limits, not authentication decisions.

HTTP middleware runs once during upgrade. Each RPC call clones that prepared
middleware state and runs the existing RPC middleware pipeline. Per-call state
mutations do not leak to other calls or batch members. Middleware response
rewrites apply to the initiating result; stream item payloads follow their own
handler-defined contract. HTTP-response middleware runs on the upgrade response,
not on later WebSocket frames.

Applications own authentication, authorization, origin policy, and revocation.
Authentication of an upgrade must authorize the connection, not merely an
unrelated HTTP body. The connection's RPC middleware still authorizes individual
methods. A source that must terminate when credentials are revoked must include
that revocation signal in its lifetime. Unauthenticated continuation limits must
follow verified authentication or explicit endpoint policy, not whether a
legacy continuation happens to use `ws` or `ws_authed`.
