#![cfg(feature = "websocket")]

#[path = "websocket/cancellation.rs"]
mod cancellation;
#[path = "websocket/flush.rs"]
mod flush;
#[path = "websocket/upgrade.rs"]
mod upgrade;

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::routing::get;
use axum::Router;
use futures::{SinkExt, StreamExt};
use rpc_toolkit::{
    from_fn_async, Context, HandlerArgs, HandlerExt, ParentHandler, RpcStream, Server,
    WebSocketOptions,
};
use serde::{Deserialize, Serialize};
use serde_json::{json as wire, Value};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use yajrc::RpcError;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WatchArgs<Final = ()> =
    HandlerArgs<TestContext, WatchParams, rpc_toolkit::Empty, Revision, Final>;

#[derive(Clone, Default)]
struct TestContext {
    dropped: Arc<AtomicUsize>,
    drop_signal: Arc<Notify>,
    started: Arc<Notify>,
    release: Arc<Notify>,
    producer: Arc<Mutex<Option<RpcStream<Revision, Value>>>>,
    echo_calls: Arc<AtomicUsize>,
    completed: Arc<AtomicUsize>,
    deny: Arc<AtomicBool>,
    http_calls: Arc<AtomicUsize>,
    rpc_calls: Arc<AtomicUsize>,
    rpc_responses: Arc<AtomicUsize>,
}
impl Context for TestContext {}

#[derive(Deserialize)]
struct WatchParams {
    value: u64,
}

#[derive(Serialize, Deserialize)]
struct Dump {
    snapshot: u64,
}
#[derive(Debug, Serialize)]
struct Revision {
    patch: u64,
}

struct ProducerGuard(TestContext);
impl Drop for ProducerGuard {
    fn drop(&mut self) {
        self.0.dropped.fetch_add(1, Ordering::SeqCst);
        self.0.drop_signal.notify_one();
    }
}

async fn watch(args: WatchArgs) -> Result<Dump, RpcError> {
    let value = args.params.value;
    if let Some(producer) = args.rpc_stream {
        let context = args.context;
        tokio::spawn(async move {
            let _guard = ProducerGuard(context);
            if producer.send(Revision { patch: value + 1 }).await.is_ok() {
                producer.closed().await;
            }
        });
    }
    Ok(Dump { snapshot: value })
}

async fn fail(args: WatchArgs) -> Result<Dump, RpcError> {
    watch(args).await?;
    Err(RpcError {
        code: 123,
        message: "subscription failed".into(),
        data: Some(wire!({"reason":"fixture"})),
    })
}

async fn finite(args: WatchArgs) -> Result<Dump, RpcError> {
    let producer = args.rpc_stream.unwrap();
    producer
        .send(Revision {
            patch: args.params.value + 1,
        })
        .await?;
    producer.close(Ok::<_, RpcError>(()))?;
    Ok(Dump {
        snapshot: args.params.value,
    })
}

async fn finish(args: WatchArgs<Value>) -> Result<Dump, RpcError> {
    let producer = args.rpc_stream.unwrap();
    producer
        .send(Revision {
            patch: args.params.value + 1,
        })
        .await?;
    producer.close(Ok::<_, RpcError>(wire!({"finished":args.params.value})))?;
    Ok(Dump {
        snapshot: args.params.value,
    })
}

async fn finish_then_fail(args: WatchArgs<Value>) -> Result<Dump, RpcError> {
    finish(args).await?;
    Err(RpcError {
        code: 789,
        message: "initial failure".into(),
        data: None,
    })
}

async fn close_error(args: WatchArgs) -> Result<Dump, RpcError> {
    let _guard = ProducerGuard(args.context);
    args.rpc_stream.unwrap().close(Err(RpcError {
        code: 456,
        message: "source failed".into(),
        data: Some(wire!({"detail":1})),
    }))?;
    Ok(Dump {
        snapshot: args.params.value,
    })
}

async fn push(args: WatchArgs<Value>) -> Result<Dump, RpcError> {
    let producer = args.rpc_stream.unwrap();
    producer
        .send(Revision {
            patch: args.params.value + 1,
        })
        .await?;
    *args.context.producer.lock().unwrap() = Some(producer);
    Ok(Dump {
        snapshot: args.params.value,
    })
}

async fn push_fail(args: WatchArgs<Value>) -> Result<Dump, RpcError> {
    push(args).await?;
    Err(RpcError {
        code: 123,
        message: "subscription failed".into(),
        data: None,
    })
}

async fn push_blocked(args: WatchArgs<Value>) -> Result<Dump, RpcError> {
    let context = args.context.clone();
    let dump = push(args).await?;
    context.started.notify_one();
    context.release.notified().await;
    Ok(dump)
}

async fn blocked(args: WatchArgs) -> Result<Dump, RpcError> {
    let guard = ProducerGuard(args.context.clone());
    args.context.started.notify_one();
    args.context.release.notified().await;
    args.context.completed.fetch_add(1, Ordering::SeqCst);
    drop(guard);
    watch(args).await
}

async fn remember(args: WatchArgs<Value>) -> Result<Dump, RpcError> {
    *args.context.producer.lock().unwrap() = args.rpc_stream;
    Ok(Dump {
        snapshot: args.params.value,
    })
}

struct BadSerialization;
impl Serialize for BadSerialization {
    fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("fixture cannot serialize"))
    }
}

async fn bad_serialization(args: WatchArgs<BadSerialization>) -> Result<Dump, RpcError> {
    args.rpc_stream
        .unwrap()
        .close(Ok::<_, RpcError>(BadSerialization))?;
    Ok(Dump {
        snapshot: args.params.value,
    })
}

async fn bulk(
    args: HandlerArgs<TestContext, WatchParams, rpc_toolkit::Empty, String>,
) -> Result<Dump, RpcError> {
    let producer = args.rpc_stream.unwrap();
    let context = args.context;
    tokio::spawn(async move {
        let _guard = ProducerGuard(context);
        if producer.send("x".repeat(16 * 1024 * 1024)).await.is_ok() {
            producer.closed().await;
        }
    });
    Ok(Dump {
        snapshot: args.params.value,
    })
}

struct ServerGuard(tokio::task::JoinHandle<()>);
impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn tree() -> ParentHandler<TestContext> {
    ParentHandler::new()
        .subcommand(
            "echo",
            from_fn_async(
                |context: TestContext, params: imbl_value::Value| async move {
                    context.echo_calls.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, RpcError>(params)
                },
            )
            .no_cli()
            .no_ts(),
        )
        .subcommand(
            "nested",
            ParentHandler::<TestContext>::new()
                .subcommand("watch", from_fn_async(watch).no_cli().no_ts()),
        )
        .subcommand("fail", from_fn_async(fail).no_cli().no_ts())
        .subcommand("finite", from_fn_async(finite).no_cli().no_ts())
        .subcommand("finish", from_fn_async(finish).no_cli().no_ts())
        .subcommand(
            "finish-then-fail",
            from_fn_async(finish_then_fail).no_cli().no_ts(),
        )
        .subcommand("close-error", from_fn_async(close_error).no_cli().no_ts())
        .subcommand("push", from_fn_async(push).no_cli().no_ts())
        .subcommand("push-fail", from_fn_async(push_fail).no_cli().no_ts())
        .subcommand("push-blocked", from_fn_async(push_blocked).no_cli().no_ts())
        .subcommand("blocked", from_fn_async(blocked).no_cli().no_ts())
        .subcommand("remember", from_fn_async(remember).no_cli().no_ts())
        .subcommand(
            "bad-serialization",
            from_fn_async(bad_serialization).no_cli().no_ts(),
        )
        .subcommand("bulk", from_fn_async(bulk).no_cli().no_ts())
}

fn rpc_server(context: TestContext) -> Server<TestContext> {
    Server::new(
        move || {
            let context = context.clone();
            async { Ok(context) }
        },
        tree(),
    )
}

async fn serve<C: Context>(server: rpc_toolkit::WebSocketServer<C>) -> (String, ServerGuard) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/rpc", listener.local_addr().unwrap());
    let router = Router::new().route("/rpc", get(server));
    let guard = ServerGuard(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    (url, guard)
}

async fn connect(options: WebSocketOptions) -> (Socket, TestContext, ServerGuard) {
    let context = TestContext::default();
    let (url, guard) = serve(
        rpc_server(context.clone())
            .for_websocket()
            .with_options(options),
    )
    .await;
    let (socket, _) = connect_async(url).await.unwrap();
    (socket, context, guard)
}

async fn send(socket: &mut Socket, value: Value) {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
}

async fn receive(socket: &mut Socket) -> Value {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match socket.next().await.unwrap().unwrap() {
                Message::Text(text) => return serde_json::from_str(&text).unwrap(),
                Message::Ping(_) => socket.flush().await.unwrap(),
                other => panic!("unexpected frame: {:?}", other),
            }
        }
    })
    .await
    .unwrap()
}

fn request(id: Value, method: &str, value: u64) -> Value {
    wire!({"jsonrpc":"2.0","rpcstream":"1.0","id":id,"method":method,"params":{"value":value}})
}

#[tokio::test]
async fn initiating_result_precedes_separately_typed_stream_items_on_the_same_id() {
    let (mut socket, _, _server) = connect(WebSocketOptions::default()).await;
    send(&mut socket, request(wire!(42), "nested.watch", 100)).await;
    assert_eq!(
        receive(&mut socket).await,
        wire!({"jsonrpc":"2.0","id":42,"result":{"snapshot":100}})
    );
    assert_eq!(
        receive(&mut socket).await,
        wire!({"rpcstream":"1.0","id":42,"result":{"patch":101},"end":false})
    );
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0","id":"other","method":"echo","params":{"answer":7}}),
    )
    .await;
    assert_eq!(
        receive(&mut socket).await,
        wire!({"jsonrpc":"2.0","id":"other","result":{"answer":7}})
    );
}

#[tokio::test]
async fn async_and_thread_producers_keep_event_order_and_close_with_a_distinct_result() {
    let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
    send(&mut socket, request(wire!(42), "push", 100)).await;
    assert_eq!(
        receive(&mut socket).await,
        wire!({"jsonrpc":"2.0","id":42,"result":{"snapshot":100}})
    );
    assert_eq!(
        receive(&mut socket).await,
        wire!({"rpcstream":"1.0","id":42,"result":{"patch":101},"end":false})
    );
    let sender = context.producer.lock().unwrap().take().unwrap();
    sender.send(Revision { patch: 102 }).await.unwrap();
    assert_eq!(receive(&mut socket).await["result"], wire!({"patch":102}));
    let producer = std::thread::spawn(move || {
        sender.blocking_send(Revision { patch: 103 }).unwrap();
        sender.close(Ok::<_, RpcError>(wire!({"total":3}))).unwrap();
    });
    assert_eq!(receive(&mut socket).await["result"], wire!({"patch":103}));
    producer.join().unwrap();
    assert_eq!(
        receive(&mut socket).await,
        wire!({"rpcstream":"1.0","id":42,"result":{"total":3},"end":true})
    );
}

#[tokio::test]
async fn explicit_error_close_terminates_the_stream_without_closing_the_socket() {
    let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
    send(&mut socket, request(wire!(42), "push", 100)).await;
    receive(&mut socket).await;
    receive(&mut socket).await;
    let sender = context.producer.lock().unwrap().take().unwrap();
    sender
        .close(Err(RpcError {
            code: 456,
            message: "producer failed".into(),
            data: None,
        }))
        .unwrap();
    assert_eq!(
        receive(&mut socket).await,
        wire!({"rpcstream":"1.0","id":42,"error":{"code":456,"message":"producer failed"},"end":true})
    );
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0","id":43,"method":"echo"}),
    )
    .await;
    assert_eq!(receive(&mut socket).await["id"], 43);
}

#[tokio::test]
async fn initiating_failure_closes_the_push_producer_without_sending_queued_events() {
    let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
    send(&mut socket, request(wire!(42), "push-fail", 100)).await;
    assert_eq!(receive(&mut socket).await["error"]["code"], 123);
    let sender = context.producer.lock().unwrap().take().unwrap();
    tokio::time::timeout(Duration::from_secs(3), sender.closed())
        .await
        .unwrap();
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0","id":43,"method":"echo"}),
    )
    .await;
    assert_eq!(receive(&mut socket).await["id"], 43);
}

#[tokio::test]
async fn client_close_and_disconnect_notify_push_producers() {
    let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
    send(&mut socket, request(wire!(42), "push", 100)).await;
    receive(&mut socket).await;
    receive(&mut socket).await;
    let sender = context.producer.lock().unwrap().take().unwrap();
    send(&mut socket, wire!({"rpcstream":"1.0","close":42})).await;
    assert_eq!(receive(&mut socket).await["end"], true);
    tokio::time::timeout(Duration::from_secs(3), sender.closed())
        .await
        .unwrap();
    send(&mut socket, request(wire!(43), "push", 200)).await;
    assert_eq!(
        receive(&mut socket).await["result"],
        wire!({"snapshot":200})
    );
    receive(&mut socket).await;
    let sender = context.producer.lock().unwrap().take().unwrap();
    drop(socket);
    tokio::time::timeout(Duration::from_secs(3), sender.closed())
        .await
        .unwrap();
}

#[tokio::test]
async fn cancelling_pending_initialization_unblocks_a_backpressured_push_producer() {
    let (mut socket, context, _server) = connect(WebSocketOptions {
        stream_buffer_capacity: NonZeroUsize::new(1).unwrap(),
        ..Default::default()
    })
    .await;
    send(&mut socket, request(wire!(42), "push-blocked", 100)).await;
    context.started.notified().await;
    let sender = context.producer.lock().unwrap().take().unwrap();
    let pending = sender.send(Revision { patch: 102 });
    futures::pin_mut!(pending);
    assert!(futures::poll!(pending.as_mut()).is_pending());
    send(&mut socket, wire!({"rpcstream":"1.0","close":42})).await;
    assert_eq!(receive(&mut socket).await["error"]["code"], -32800);
    assert!(tokio::time::timeout(Duration::from_secs(3), pending)
        .await
        .unwrap()
        .is_err());
}

#[tokio::test]
async fn terminal_result_uses_reserved_capacity_and_waits_for_the_entire_batch() {
    let (mut socket, context, _server) = connect(WebSocketOptions {
        stream_buffer_capacity: NonZeroUsize::new(1).unwrap(),
        ..Default::default()
    })
    .await;
    send(
        &mut socket,
        wire!([
            request(wire!(50), "finish", 100),
            {"jsonrpc":"2.0","id":51,"method":"blocked","params":{"value":1}}
        ]),
    )
    .await;
    context.started.notified().await;
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0","id":52,"method":"echo"}),
    )
    .await;
    assert_eq!(receive(&mut socket).await["id"], 52);
    context.release.notify_one();
    assert_eq!(receive(&mut socket).await.as_array().unwrap().len(), 2);
    assert_eq!(
        receive(&mut socket).await,
        wire!({
            "rpcstream":"1.0","id":50,"result":{"patch":101},"end":false
        })
    );
    assert_eq!(
        receive(&mut socket).await,
        wire!({
            "rpcstream":"1.0","id":50,"result":{"finished":100},"end":true
        })
    );
}

#[tokio::test]
async fn initial_failure_discards_previously_queued_events_and_terminal_result() {
    let (mut socket, _, _server) = connect(WebSocketOptions::default()).await;
    send(&mut socket, request(wire!(50), "finish-then-fail", 100)).await;
    assert_eq!(receive(&mut socket).await["error"]["code"], 789);
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0","id":51,"method":"echo"}),
    )
    .await;
    assert_eq!(receive(&mut socket).await["id"], 51);
}

#[tokio::test]
async fn no_opt_in_preserves_the_ordinary_handler_result_without_a_stream() {
    let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0","id":1,"method":"nested.watch","params":{"value":10}}),
    )
    .await;
    assert_eq!(receive(&mut socket).await["result"], wire!({"snapshot":10}));
    send(&mut socket, wire!({"jsonrpc":"2.0","id":2,"method":"echo"})).await;
    assert_eq!(
        receive(&mut socket).await,
        wire!({"jsonrpc":"2.0","id":2,"result":{}})
    );
    assert_eq!(context.dropped.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn initiating_failure_drops_the_source_and_emits_only_a_jsonrpc_error() {
    let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
    send(&mut socket, request(wire!(3), "fail", 10)).await;
    assert_eq!(
        receive(&mut socket).await,
        wire!({"jsonrpc":"2.0","id":3,"error":{"code":123,"message":"subscription failed","data":{"reason":"fixture"}}})
    );
    context.drop_signal.notified().await;
    send(&mut socket, wire!({"jsonrpc":"2.0","id":4,"method":"echo"})).await;
    assert_eq!(receive(&mut socket).await["id"], 4);
}

#[tokio::test]
async fn explicit_success_and_error_closes_have_terminal_events() {
    let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
    send(&mut socket, request(wire!(5), "finite", 20)).await;
    assert_eq!(receive(&mut socket).await["result"], wire!({"snapshot":20}));
    assert_eq!(receive(&mut socket).await["result"], wire!({"patch":21}));
    assert_eq!(
        receive(&mut socket).await,
        wire!({"rpcstream":"1.0","id":5,"result":null,"end":true})
    );
    send(&mut socket, request(wire!(6), "close-error", 20)).await;
    assert_eq!(receive(&mut socket).await["result"], wire!({"snapshot":20}));
    assert_eq!(
        receive(&mut socket).await,
        wire!({"rpcstream":"1.0","id":6,"error":{"code":456,"message":"source failed","data":{"detail":1}},"end":true})
    );
    context.drop_signal.notified().await;
    send(&mut socket, wire!({"jsonrpc":"2.0","id":7,"method":"echo"})).await;
    assert_eq!(receive(&mut socket).await["id"], 7);
}

#[tokio::test]
async fn closing_one_stream_preserves_other_streams_and_ignores_repeated_closure() {
    let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
    for id in ["first", "second"] {
        send(&mut socket, request(wire!(id), "nested.watch", 1)).await;
        receive(&mut socket).await;
        receive(&mut socket).await;
    }
    send(&mut socket, wire!({"rpcstream":"1.0","close":"first"})).await;
    assert_eq!(
        receive(&mut socket).await,
        wire!({"rpcstream":"1.0","id":"first","result":null,"end":true})
    );
    context.drop_signal.notified().await;
    assert_eq!(context.dropped.load(Ordering::SeqCst), 1);
    send(&mut socket, wire!({"rpcstream":"1.0","close":"first"})).await;
    send(&mut socket, wire!({"rpcstream":"1.0","close":"second"})).await;
    assert_eq!(receive(&mut socket).await["id"], "second");
    context.drop_signal.notified().await;
    assert_eq!(context.dropped.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn closure_can_cancel_an_in_flight_initiating_call() {
    let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
    send(&mut socket, request(wire!(8), "blocked", 1)).await;
    context.started.notified().await;
    send(&mut socket, wire!({"rpcstream":"1.0","close":8})).await;
    let response = receive(&mut socket).await;
    assert_eq!(response["id"], 8);
    assert_eq!(response["error"]["code"], -32800);
    context.drop_signal.notified().await;
    send(&mut socket, request(wire!(8), "finite", 2)).await;
    assert_eq!(receive(&mut socket).await["result"], wire!({"snapshot":2}));
}

#[tokio::test]
async fn disconnect_drops_pending_calls_and_active_sources() {
    let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
    for id in [9, 11] {
        send(&mut socket, request(wire!(id), "nested.watch", 1)).await;
        receive(&mut socket).await;
        receive(&mut socket).await;
    }
    send(&mut socket, request(wire!(10), "blocked", 1)).await;
    context.started.notified().await;
    drop(socket);
    tokio::time::timeout(Duration::from_secs(3), async {
        while context.dropped.load(Ordering::SeqCst) < 3 {
            context.drop_signal.notified().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn batch_response_precedes_every_associated_stream_and_omits_notifications() {
    let (mut socket, _, _server) = connect(WebSocketOptions::default()).await;
    send(
        &mut socket,
        wire!([
            request(wire!(11), "finite", 1),
            {"jsonrpc":"2.0","method":"echo","params":{}},
            {"jsonrpc":"2.0","id":12,"method":"echo","params":{"answer":12}}
        ]),
    )
    .await;
    let response = receive(&mut socket).await;
    assert_eq!(response.as_array().unwrap().len(), 2);
    assert!(response
        .as_array()
        .unwrap()
        .iter()
        .any(|response| response["id"] == 11 && response["result"] == wire!({"snapshot":1})));
    assert_eq!(receive(&mut socket).await["result"], wire!({"patch":2}));
    assert_eq!(receive(&mut socket).await["end"], true);
}

#[tokio::test]
async fn handle_can_send_after_initial_reply_and_drop_emits_fixed_cancellation() {
    let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
    send(&mut socket, request(wire!(13), "remember", 1)).await;
    assert_eq!(receive(&mut socket).await["result"], wire!({"snapshot":1}));
    let producer = context.producer.lock().unwrap().take().unwrap();
    producer.send(Revision { patch: 2 }).await.unwrap();
    drop(producer);
    assert_eq!(receive(&mut socket).await["result"], wire!({"patch":2}));
    assert_eq!(
        receive(&mut socket).await,
        wire!({
            "rpcstream":"1.0", "id":13,
            "error":{"code":-32800,"message":"Response stream canceled"}, "end":true
        })
    );
    send(&mut socket, request(wire!(14), "echo", 1)).await;
    assert_eq!(receive(&mut socket).await["result"], wire!({"value":1}));
    assert_eq!(receive(&mut socket).await["error"]["code"], -32800);
}

#[tokio::test]
async fn numeric_request_ids_above_f64_integer_precision_remain_distinct() {
    let (mut socket, _, _server) = connect(WebSocketOptions::default()).await;
    for id in [9_007_199_254_740_992u64, 9_007_199_254_740_993] {
        send(&mut socket, request(wire!(id), "nested.watch", 1)).await;
        assert_eq!(receive(&mut socket).await["id"], id);
        assert_eq!(receive(&mut socket).await["id"], id);
    }
}

#[tokio::test]
async fn invalid_protocol_messages_do_not_disconnect_unrelated_operations() {
    let (mut socket, _, _server) = connect(WebSocketOptions::default()).await;
    for invalid in [
        wire!({"jsonrpc":"2.0","rpcstream":1.0,"id":14,"method":"echo"}),
        wire!({"jsonrpc":"2.0","rpcstream":"1.0","method":"echo"}),
        wire!({"jsonrpc":"2.0","rpcstream":"1.0","id":null,"method":"echo"}),
        wire!({"jsonrpc":"1.0","id":14,"method":"echo"}),
        wire!([]),
    ] {
        send(&mut socket, invalid).await;
        assert_eq!(receive(&mut socket).await["error"]["code"], -32600);
    }
    socket.send(Message::Text("{".into())).await.unwrap();
    assert_eq!(receive(&mut socket).await["error"]["code"], -32700);
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0","id":15,"method":"echo"}),
    )
    .await;
    assert_eq!(receive(&mut socket).await["id"], 15);
}

#[tokio::test]
async fn duplicate_ids_do_not_replace_producers_and_closed_ids_can_be_reused() {
    let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
    send(&mut socket, request(wire!(16), "nested.watch", 1)).await;
    receive(&mut socket).await;
    receive(&mut socket).await;
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0","id":16,"method":"echo"}),
    )
    .await;
    assert_eq!(receive(&mut socket).await["error"]["code"], -32600);
    assert_eq!(context.dropped.load(Ordering::SeqCst), 0);
    send(&mut socket, wire!({"rpcstream":"1.0","close":16})).await;
    assert_eq!(receive(&mut socket).await["end"], true);
    context.drop_signal.notified().await;
    send(&mut socket, request(wire!(16), "finite", 2)).await;
    assert_eq!(receive(&mut socket).await["result"], wire!({"snapshot":2}));
    assert_eq!(receive(&mut socket).await["result"], wire!({"patch":3}));
    assert_eq!(receive(&mut socket).await["end"], true);
}

#[tokio::test]
async fn terminal_serialization_failure_is_an_error_without_closing_the_socket() {
    let (mut socket, _, _server) = connect(WebSocketOptions::default()).await;
    send(&mut socket, request(wire!(17), "bad-serialization", 1)).await;
    assert_eq!(receive(&mut socket).await["result"], wire!({"snapshot":1}));
    let event = receive(&mut socket).await;
    assert_eq!(event["error"]["code"], -32603);
    assert_eq!(event["end"], true);
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0","id":18,"method":"echo"}),
    )
    .await;
    assert_eq!(receive(&mut socket).await["id"], 18);
}

#[tokio::test]
async fn successful_and_failed_notifications_never_receive_responses() {
    let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
    send(
        &mut socket,
        wire!([
            {"jsonrpc":"2.0","method":"echo"},
            {"jsonrpc":"2.0","method":"fail","params":{"value":1}}
        ]),
    )
    .await;
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0","id":19,"method":"echo"}),
    )
    .await;
    assert_eq!(receive(&mut socket).await["id"], 19);
    assert_eq!(context.echo_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn stream_capacity_is_released_by_closure_and_does_not_block_ordinary_calls() {
    let options = WebSocketOptions {
        max_streams: NonZeroUsize::new(1).unwrap(),
        ..Default::default()
    };
    let (mut socket, _, _server) = connect(options).await;
    send(&mut socket, request(wire!(20), "nested.watch", 1)).await;
    receive(&mut socket).await;
    receive(&mut socket).await;
    send(&mut socket, request(wire!(21), "nested.watch", 1)).await;
    assert_eq!(receive(&mut socket).await["error"]["code"], -32000);
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0","id":22,"method":"echo"}),
    )
    .await;
    assert_eq!(receive(&mut socket).await["id"], 22);
    send(&mut socket, wire!({"rpcstream":"1.0","close":20})).await;
    assert_eq!(receive(&mut socket).await["end"], true);
    send(&mut socket, request(wire!(21), "nested.watch", 1)).await;
    assert_eq!(receive(&mut socket).await["result"], wire!({"snapshot":1}));
}

#[tokio::test]
async fn request_capacity_rejects_work_before_execution_and_cancellation_releases_it() {
    let options = WebSocketOptions {
        max_in_flight: NonZeroUsize::new(1).unwrap(),
        ..Default::default()
    };
    let (mut socket, context, _server) = connect(options).await;
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0","id":23,"method":"blocked","params":{"value":1}}),
    )
    .await;
    context.started.notified().await;
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0","id":24,"method":"echo"}),
    )
    .await;
    assert_eq!(receive(&mut socket).await["error"]["code"], -32000);
    assert_eq!(context.echo_calls.load(Ordering::SeqCst), 0);
    send(&mut socket, wire!({"rpcstream":"1.0","close":23})).await;
    tokio::time::timeout(Duration::from_secs(3), context.drop_signal.notified())
        .await
        .unwrap();
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0","id":24,"method":"echo"}),
    )
    .await;
    assert_eq!(receive(&mut socket).await["id"], 24);
    assert_eq!(context.echo_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn buffered_batch_results_and_closure_do_not_leave_an_unreachable_stream() {
    let (mut socket, context, _server) = connect(WebSocketOptions::default()).await;
    send(
        &mut socket,
        wire!([
            request(wire!(25), "nested.watch", 1),
            {"jsonrpc":"2.0","id":26,"method":"blocked","params":{"value":1}}
        ]),
    )
    .await;
    context.started.notified().await;
    send(&mut socket, wire!({"rpcstream":"1.0","close":25})).await;
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0","id":27,"method":"echo"}),
    )
    .await;
    assert_eq!(receive(&mut socket).await["id"], 27);
    context.release.notify_one();
    let response = receive(&mut socket).await;
    assert_eq!(response.as_array().unwrap().len(), 2);
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0","id":28,"method":"echo"}),
    )
    .await;
    assert_eq!(receive(&mut socket).await["id"], 28);
    assert!(context.dropped.load(Ordering::SeqCst) >= 2);
}

#[tokio::test]
async fn oversized_batches_and_frames_enforce_connection_limits() {
    let options = WebSocketOptions {
        max_in_flight: NonZeroUsize::new(1).unwrap(),
        ..Default::default()
    };
    let (mut socket, context, _server) = connect(options).await;
    send(
        &mut socket,
        wire!([
            {"jsonrpc":"2.0","id":1,"method":"echo"},
            {"jsonrpc":"2.0","id":2,"method":"echo"}
        ]),
    )
    .await;
    let frame = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(frame, Message::Close(Some(frame)) if u16::from(frame.code) == 1008));
    assert_eq!(context.echo_calls.load(Ordering::SeqCst), 0);
    let options = WebSocketOptions {
        max_message_size: NonZeroUsize::new(1024).unwrap(),
        ..Default::default()
    };
    let (mut socket, _, _server) = connect(options).await;
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0","id":1,"method":"echo","params":{"large":"x".repeat(2048)}}),
    )
    .await;
    assert!(!matches!(
        tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap(),
        Some(Ok(Message::Text(_)))
    ));
    let options = WebSocketOptions {
        max_message_size: NonZeroUsize::new(1024).unwrap(),
        ..Default::default()
    };
    let (mut socket, _, _server) = connect(options).await;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::{Data, OpCode};
    use tokio_tungstenite::tungstenite::protocol::frame::Frame;
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
    assert!(!matches!(
        tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap(),
        Some(Ok(Message::Text(_)))
    ));
}

#[tokio::test]
async fn stalled_writes_release_sources_at_the_send_deadline() {
    let options = WebSocketOptions {
        send_timeout: Duration::from_millis(50),
        ..Default::default()
    };
    let (mut socket, context, _server) = connect(options).await;
    send(&mut socket, request(wire!(28), "bulk", 1)).await;
    assert_eq!(receive(&mut socket).await["result"], wire!({"snapshot":1}));
    tokio::time::timeout(Duration::from_secs(5), context.drop_signal.notified())
        .await
        .unwrap();
    assert_eq!(context.dropped.load(Ordering::SeqCst), 1);
}

#[derive(Clone)]
struct RejectUpgrade;
impl rpc_toolkit::Middleware<TestContext> for RejectUpgrade {
    type Metadata = rpc_toolkit::Empty;
    async fn process_http_request(
        &mut self,
        context: &TestContext,
        _: &mut axum::extract::Request,
    ) -> Result<(), axum::response::Response> {
        context.http_calls.fetch_add(1, Ordering::SeqCst);
        Err(axum::response::IntoResponse::into_response(
            axum::http::StatusCode::UNAUTHORIZED,
        ))
    }
}

#[tokio::test]
async fn upgrade_middleware_can_refuse_the_socket_before_rpc_dispatch() {
    let context = TestContext::default();
    let (url, _server) = serve(
        rpc_server(context.clone())
            .middleware(RejectUpgrade)
            .for_websocket(),
    )
    .await;
    assert!(
        matches!(connect_async(url).await, Err(tokio_tungstenite::tungstenite::Error::Http(response)) if response.status() == 401)
    );
    assert_eq!(context.http_calls.load(Ordering::SeqCst), 1);
    assert_eq!(context.echo_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn large_dumps_and_events_are_independent_of_the_inbound_request_limit() {
    const SNAPSHOT_BYTES: usize = 17 * 1024 * 1024;
    const EVENT_BYTES: usize = 5 * 1024 * 1024;

    async fn large(
        args: HandlerArgs<TestContext, rpc_toolkit::Empty, rpc_toolkit::Empty, Value>,
    ) -> Result<Value, RpcError> {
        let producer = args.rpc_stream.unwrap();
        tokio::spawn(async move {
            let event = wire!({"id":1, "patch":[{"op":"add", "path":"/large", "value":"y".repeat(EVENT_BYTES)}]});
            producer.send(event).await.unwrap();
            producer.close(Ok::<_, RpcError>(())).unwrap();
        });
        Ok(wire!({"id":0, "value":{"text":"x".repeat(SNAPSHOT_BYTES)}}))
    }

    let server = Server::new(
        || async { Ok(TestContext::default()) },
        ParentHandler::new().subcommand("large", from_fn_async(large).no_cli().no_ts()),
    )
    .for_websocket();
    let (url, _server) = serve(server).await;
    let receive_limit = 64 * 1024 * 1024;
    let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(receive_limit))
        .max_frame_size(Some(receive_limit));
    let (mut socket, _) = tokio_tungstenite::connect_async_with_config(url, Some(config), false)
        .await
        .unwrap();
    send(
        &mut socket,
        wire!({"jsonrpc":"2.0", "rpcstream":"1.0", "id":90, "method":"large", "params":{}}),
    )
    .await;
    let initial = receive(&mut socket).await;
    assert_eq!(initial["id"], 90);
    assert_eq!(
        initial["result"]["value"]["text"].as_str().unwrap().len(),
        SNAPSHOT_BYTES
    );
    let event = receive(&mut socket).await;
    assert_eq!(event["id"], 90);
    assert_eq!(
        event["result"]["patch"][0]["value"].as_str().unwrap().len(),
        EVENT_BYTES
    );
    assert_eq!(receive(&mut socket).await["end"], true);
}

#[derive(Clone, Default)]
struct RecordingMiddleware {
    seen: usize,
}

impl rpc_toolkit::Middleware<TestContext> for RecordingMiddleware {
    type Metadata = rpc_toolkit::Empty;
    async fn process_http_request(
        &mut self,
        context: &TestContext,
        _: &mut axum::extract::Request,
    ) -> Result<(), axum::response::Response> {
        context.http_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn process_rpc_request(
        &mut self,
        context: &TestContext,
        _: Self::Metadata,
        request: &mut rpc_toolkit::RpcRequest,
    ) -> Result<(), rpc_toolkit::RpcResponse> {
        context.rpc_calls.fetch_add(1, Ordering::SeqCst);
        self.seen += 1;
        request.params["middlewareCalls"] = self.seen.into();
        if context.deny.load(Ordering::SeqCst) {
            return Err(rpc_toolkit::RpcResponse::from(RpcError {
                code: 403,
                message: "denied".into(),
                data: None,
            }));
        }
        Ok(())
    }
    async fn process_rpc_response(
        &mut self,
        context: &TestContext,
        response: &mut rpc_toolkit::RpcResponse,
    ) {
        context.rpc_responses.fetch_add(1, Ordering::SeqCst);
        if let Ok(result) = &mut response.result {
            result["middlewareResponse"] = true.into();
        }
    }
    async fn process_http_response(
        &mut self,
        _: &TestContext,
        response: &mut axum::response::Response,
    ) {
        response
            .headers_mut()
            .insert("x-rpc-upgrade", "yes".parse().unwrap());
    }
}

#[tokio::test]
async fn upgrade_and_per_call_middleware_are_reused_with_isolated_state_and_authorization() {
    let context = TestContext::default();
    let server = rpc_server(context.clone())
        .middleware(RecordingMiddleware::default())
        .for_websocket();
    let (url, _server) = serve(server).await;
    let (mut socket, response) = connect_async(url).await.unwrap();
    assert_eq!(response.headers()["x-rpc-upgrade"], "yes");
    for id in 29..31 {
        send(
            &mut socket,
            wire!({"jsonrpc":"2.0","id":id,"method":"echo","params":{"middlewareCalls":999}}),
        )
        .await;
        assert_eq!(
            receive(&mut socket).await["result"],
            wire!({"middlewareCalls":1,"middlewareResponse":true})
        );
    }
    send(&mut socket, request(wire!(31), "finite", 1)).await;
    assert_eq!(
        receive(&mut socket).await["result"],
        wire!({"snapshot":1,"middlewareResponse":true})
    );
    assert_eq!(receive(&mut socket).await["result"], wire!({"patch":2}));
    receive(&mut socket).await;
    context.deny.store(true, Ordering::SeqCst);
    send(&mut socket, request(wire!(32), "nested.watch", 1)).await;
    assert_eq!(
        receive(&mut socket).await,
        wire!({"jsonrpc":"2.0","id":32,"error":{"code":403,"message":"denied"}})
    );
    assert_eq!(context.http_calls.load(Ordering::SeqCst), 1);
    assert_eq!(context.rpc_calls.load(Ordering::SeqCst), 4);
    assert_eq!(context.rpc_responses.load(Ordering::SeqCst), 4);
    assert_eq!(context.dropped.load(Ordering::SeqCst), 0);
}
