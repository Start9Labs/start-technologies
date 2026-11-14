use std::borrow::{Borrow, Cow};
use std::future::Future;
use std::marker::PhantomData;
#[cfg(feature = "websocket")]
use std::num::NonZeroUsize;

#[cfg(feature = "websocket")]
use futures::stream::BoxStream;
use futures::StreamExt;
use imbl_value::Value;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tokio::sync::{mpsc, oneshot};
use yajrc::{Id, RpcError};

use crate::util::internal_error;
use crate::RpcResponse;

/// Version of the opt-in response-stream envelope.
pub const RPCSTREAM_VERSION: &str = "1.0";

/// Cancellation of an opted-in call or an explicitly unclosed producer.
pub const RPCSTREAM_CANCELLED_ERROR: RpcError = RpcError {
    code: -32800,
    message: Cow::Borrowed("Response stream canceled"),
    data: None,
};

/// An event or terminal reply correlated to its initiating JSON-RPC call.
#[derive(Clone, Debug)]
pub struct RpcStreamResponse {
    /// The initiating call's ID.
    pub id: Id,
    /// Event payload, terminal value, or error.
    pub result: Result<Value, RpcError>,
    /// Whether this reply terminates the response stream.
    pub end: bool,
}

impl RpcStreamResponse {
    fn into_value(self) -> Value {
        let mut value =
            imbl_value::to_value(&RpcResponse::new(Some(self.id), self.result)).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("jsonrpc");
        object.insert("rpcstream".into(), RPCSTREAM_VERSION.into());
        object.insert("end".into(), self.end.into());
        value
    }
}

impl Serialize for RpcStreamResponse {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.clone().into_value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RpcStreamResponse {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut value = Value::deserialize(deserializer)?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| D::Error::custom("expected a response-stream object"))?;
        if object.remove("rpcstream").as_ref().and_then(Value::as_str) != Some(RPCSTREAM_VERSION) {
            return Err(D::Error::custom("unsupported response-stream version"));
        }
        if object.contains_key("jsonrpc") {
            return Err(D::Error::custom(
                "a response-stream reply cannot contain jsonrpc",
            ));
        }
        let end = object
            .remove("end")
            .ok_or_else(|| D::Error::missing_field("end"))?;
        let end = imbl_value::from_value(end).map_err(D::Error::custom)?;
        object.insert("jsonrpc".into(), "2.0".into());
        let response: RpcResponse = imbl_value::from_value(value).map_err(D::Error::custom)?;
        let id = response
            .id
            .ok_or_else(|| D::Error::custom("a response-stream reply requires an ID"))?;
        Ok(Self {
            id,
            result: response.result,
            end,
        })
    }
}

type FlushReceipt = Option<oneshot::Sender<()>>;
type Terminal = (Result<Value, RpcError>, FlushReceipt);

#[cfg(feature = "websocket")]
pub(crate) type ResponseEvents = BoxStream<'static, (Result<Value, RpcError>, bool, FlushReceipt)>;

/// A single-owner producer for an opted-in WebSocket response stream.
///
/// Events use `Item`; successful termination uses `Final`.
/// The transport sends the initiating JSON-RPC response before draining events.
/// Dropping this handle without `close` terminates with `RPCSTREAM_CANCELLED_ERROR`
/// after queued events. Initial failure, client closure, and socket loss close
/// the receiver instead; `closed` and pending sends then wake the producer.
///
/// ```compile_fail
/// fn wrong_item(stream: rpc_toolkit::RpcStream<u64, bool>) {
///     let _ = stream.send("not a number");
/// }
/// ```
///
/// ```compile_fail
/// fn wrong_terminal(stream: rpc_toolkit::RpcStream<u64, bool>) {
///     let _ = stream.close(Ok::<_, rpc_toolkit::yajrc::RpcError>(42u64));
/// }
/// ```
#[derive(Debug)]
pub struct RpcStream<Item = crate::Never, Final = ()> {
    events: mpsc::Sender<Value>,
    terminal: oneshot::Sender<Terminal>,
    types: PhantomData<fn(Item) -> Final>,
}

impl RpcStream {
    #[cfg(feature = "websocket")]
    pub(crate) fn new(capacity: NonZeroUsize) -> (Self, ResponseEvents) {
        let (events, mut receiver) = mpsc::channel(capacity.get());
        let (terminal, result) = oneshot::channel();
        let source = async_stream::stream! {
            while let Some(value) = receiver.recv().await {
                yield (Ok(value), false, None);
            }
            let (result, flushed) = result.await.unwrap_or((Err(RPCSTREAM_CANCELLED_ERROR), None));
            yield (result, true, flushed);
        }
        .boxed();
        (
            Self {
                events,
                terminal,
                types: PhantomData,
            },
            source,
        )
    }

    pub(crate) fn typed<Item, Final>(self) -> RpcStream<Item, Final> {
        RpcStream {
            events: self.events,
            terminal: self.terminal,
            types: PhantomData,
        }
    }
}

impl<Item, Final> RpcStream<Item, Final> {
    /// Serializes an event immediately; awaiting waits for bounded queue capacity.
    ///
    /// A serialization error sends nothing and leaves the handle usable. Filling
    /// the queue before returning the initiating result can block that result.
    pub fn send(
        &self,
        value: impl Borrow<Item>,
    ) -> impl Future<Output = Result<(), RpcError>> + Send + '_
    where
        Item: Serialize,
    {
        self.send_serialized(serialize(value.borrow()))
    }

    async fn send_serialized(&self, value: Result<Value, RpcError>) -> Result<(), RpcError> {
        self.events
            .send(value?)
            .await
            .map_err(|_| RPCSTREAM_CANCELLED_ERROR)
    }

    /// Forwards items until the source ends, fails, or the transport closes.
    /// The caller owns the terminal value and must call `close` separately.
    pub fn forward<'a, S, V, E>(
        &'a self,
        source: S,
    ) -> impl Future<Output = Result<(), RpcError>> + 'a
    where
        S: futures::Stream<Item = Result<V, E>> + 'a,
        V: Borrow<Item>,
        E: Into<RpcError>,
        Item: Serialize,
    {
        let source = source.map(|value| {
            value
                .map_err(Into::into)
                .and_then(|value| serialize(value.borrow()))
        });
        async move {
            let mut source = Box::pin(source);
            tokio::select! {
                _ = self.closed() => Err(RPCSTREAM_CANCELLED_ERROR),
                result = async {
                    while let Some(value) = source.next().await {
                        self.send_serialized(value).await?;
                    }
                    Ok(())
                } => result,
            }
        }
    }

    /// Sends an event from an ordinary thread, waiting for queue capacity.
    ///
    /// # Panics
    ///
    /// Panics inside an asynchronous execution context, as Tokio's blocking sender does.
    pub fn blocking_send(&self, value: impl Borrow<Item>) -> Result<(), RpcError>
    where
        Item: Serialize,
    {
        self.events
            .blocking_send(serialize(value.borrow())?)
            .map_err(|_| RPCSTREAM_CANCELLED_ERROR)
    }

    /// Consumes the handle and queues its terminal result after pending events.
    ///
    /// Uses a reserved slot, independent of event-buffer capacity. A terminal
    /// serialization failure becomes an internal-error result on the wire.
    pub fn close<E: Into<RpcError>>(self, result: Result<Final, E>) -> Result<(), RpcError>
    where
        Final: Serialize,
    {
        self.queue_terminal(result, None)
    }

    /// Queues the terminal immediately and waits for its transport flush, not peer receipt.
    ///
    /// Cancellation and failed writes return `RPCSTREAM_CANCELLED_ERROR`. Await this outside
    /// the initiating handler; its response must be sent before the terminal can drain.
    pub fn close_and_flush<E: Into<RpcError>>(
        self,
        result: Result<Final, E>,
    ) -> impl Future<Output = Result<(), RpcError>> + Send
    where
        Final: Serialize,
    {
        let (flushed, receipt) = oneshot::channel();
        let queued = self.queue_terminal(result, Some(flushed));
        async move {
            queued?;
            receipt.await.map_err(|_| RPCSTREAM_CANCELLED_ERROR)
        }
    }

    fn queue_terminal<E: Into<RpcError>>(
        self,
        result: Result<Final, E>,
        flushed: FlushReceipt,
    ) -> Result<(), RpcError>
    where
        Final: Serialize,
    {
        let result = result.map_err(Into::into).and_then(serialize);
        self.terminal
            .send((result, flushed))
            .map_err(|_| RPCSTREAM_CANCELLED_ERROR)
    }

    /// Resolves when the transport can no longer receive events.
    pub async fn closed(&self) {
        self.events.closed().await;
    }
}

fn serialize<T: Serialize>(value: T) -> Result<Value, RpcError> {
    imbl_value::to_value(&value).map_err(internal_error)
}

#[cfg(feature = "websocket")]
pub(crate) fn stream_event(id: Id, result: Result<Value, RpcError>, end: bool) -> Value {
    RpcStreamResponse { id, result, end }.into_value()
}

#[cfg(test)]
mod codec_tests;

#[cfg(all(test, feature = "websocket"))]
mod tests {
    use std::rc::Rc;

    use super::*;

    fn pair<Item, Final>() -> (RpcStream<Item, Final>, ResponseEvents) {
        let (producer, events) = RpcStream::new(NonZeroUsize::new(1).unwrap());
        (producer.typed(), events)
    }

    fn assert_cancelled(error: RpcError) {
        assert_eq!(error.code, -32800);
        assert_eq!(error.message, "Response stream canceled");
        assert!(error.data.is_none());
    }

    #[tokio::test]
    async fn close_uses_a_reserved_slot_after_a_full_event_queue() {
        let (producer, mut events) = pair::<u64, &str>();
        producer.send(1u64).await.unwrap();
        producer.close(Ok::<_, RpcError>("finished")).unwrap();
        assert_eq!(
            events.next().await.unwrap().0.unwrap(),
            imbl_value::json!(1)
        );
        let (result, end, _) = events.next().await.unwrap();
        assert_eq!(result.unwrap(), imbl_value::json!("finished"));
        assert!(end);
        assert!(events.next().await.is_none());
    }

    #[tokio::test]
    async fn flush_receipt_waits_beyond_queue_acceptance_and_preserves_items() {
        let (producer, mut events) = pair::<u64, bool>();
        producer.send(7u64).await.unwrap();
        let flushed = producer.close_and_flush(Ok::<_, RpcError>(true));
        futures::pin_mut!(flushed);
        assert!(futures::poll!(flushed.as_mut()).is_pending());
        assert_eq!(
            events.next().await.unwrap().0.unwrap(),
            imbl_value::json!(7)
        );
        let (result, end, receipt) = events.next().await.unwrap();
        assert_eq!(result.unwrap(), imbl_value::json!(true));
        assert!(end);
        assert!(futures::poll!(flushed.as_mut()).is_pending());
        receipt.unwrap().send(()).unwrap();
        flushed.await.unwrap();
    }

    #[tokio::test]
    async fn losing_a_terminal_write_or_receiver_cancels_its_flush_receipt() {
        let (producer, mut events) = pair::<u64, ()>();
        let flushed = producer.close_and_flush(Ok::<_, RpcError>(()));
        let (_, end, receipt) = events.next().await.unwrap();
        assert!(end);
        drop(receipt);
        assert_cancelled(flushed.await.unwrap_err());

        let (producer, events) = pair::<u64, ()>();
        let flushed = producer.close_and_flush(Ok::<_, RpcError>(()));
        drop(events);
        assert_cancelled(flushed.await.unwrap_err());
    }

    #[tokio::test]
    async fn terminal_flush_serializes_borrowed_non_send_values_before_suspending() {
        fn is_send<T: Send>(value: T) -> T {
            value
        }
        #[derive(Serialize)]
        struct LocalValue<'a> {
            text: &'a str,
            #[serde(skip)]
            _not_send: Rc<()>,
        }
        let text = "borrowed".to_owned();
        let (producer, mut events) = pair::<(), LocalValue<'_>>();
        let flushed = is_send(producer.close_and_flush(Ok::<_, RpcError>(LocalValue {
            text: &text,
            _not_send: Rc::new(()),
        })));
        let (result, end, receipt) = events.next().await.unwrap();
        assert_eq!(result.unwrap(), imbl_value::json!({"text":"borrowed"}));
        assert!(end);
        receipt.unwrap().send(()).unwrap();
        flushed.await.unwrap();
    }

    #[tokio::test]
    async fn dropping_without_close_drains_events_then_emits_fixed_cancellation() {
        let (producer, mut events) = pair::<u64, ()>();
        producer.send(7u64).await.unwrap();
        drop(producer);
        let (value, end, _) = events.next().await.unwrap();
        assert_eq!(value.unwrap(), imbl_value::json!(7));
        assert!(!end);
        let (error, end, _) = events.next().await.unwrap();
        assert_cancelled(error.unwrap_err());
        assert!(end);
        assert!(events.next().await.is_none());
    }

    #[tokio::test]
    async fn send_is_backpressured_and_receiver_loss_unblocks_it() {
        let (producer, events) = pair::<u64, ()>();
        producer.send(1u64).await.unwrap();
        let pending = producer.send(2u64);
        futures::pin_mut!(pending);
        assert!(futures::poll!(pending.as_mut()).is_pending());
        drop(events);
        producer.closed().await;
        assert_cancelled(pending.await.unwrap_err());
    }

    #[tokio::test]
    async fn explicit_errors_convert_and_follow_queued_events() {
        struct Failure;
        impl From<Failure> for RpcError {
            fn from(_: Failure) -> Self {
                RpcError {
                    code: 456,
                    message: "finished with error".into(),
                    data: None,
                }
            }
        }
        let (producer, mut events) = pair::<&str, ()>();
        producer.send("last event").await.unwrap();
        producer.close(Err(Failure)).unwrap();
        assert_eq!(
            events.next().await.unwrap().0.unwrap(),
            imbl_value::json!("last event")
        );
        let (result, end, _) = events.next().await.unwrap();
        assert_eq!(result.unwrap_err().code, 456);
        assert!(end);
        assert!(events.next().await.is_none());
    }

    #[tokio::test]
    async fn forwarding_stops_on_transport_loss_while_the_source_is_idle() {
        let (producer, events) = pair::<u64, ()>();
        let forward = producer.forward(futures::stream::pending::<Result<u64, RpcError>>());
        futures::pin_mut!(forward);
        assert!(futures::poll!(forward.as_mut()).is_pending());
        drop(events);
        assert_cancelled(forward.await.unwrap_err());
    }

    #[tokio::test]
    async fn forwarding_preserves_accepted_items_and_leaves_the_terminal_to_the_caller() {
        let (producer, mut events) = pair::<u64, bool>();
        let failure = RpcError {
            code: 456,
            message: "source failed".into(),
            data: None,
        };
        let result = producer
            .forward(futures::stream::iter([Ok(7u64), Err(failure)]))
            .await;
        assert_eq!(result.unwrap_err().code, 456);
        producer.close(Ok::<_, RpcError>(true)).unwrap();
        assert_eq!(
            events.next().await.unwrap().0.unwrap(),
            imbl_value::json!(7)
        );
        let (result, end, _) = events.next().await.unwrap();
        assert_eq!(result.unwrap(), imbl_value::json!(true));
        assert!(end);
    }

    #[tokio::test]
    async fn forwarding_serializes_non_send_borrowed_items_before_suspending() {
        fn is_send<T: Send>(value: T) -> T {
            value
        }
        #[derive(Serialize)]
        struct LocalValue<'a> {
            text: &'a str,
            #[serde(skip)]
            _not_send: Rc<()>,
        }
        let text = "borrowed".to_owned();
        let (producer, mut events) = pair::<LocalValue<'_>, ()>();
        is_send(producer.forward(futures::stream::once(async {
            Ok::<_, RpcError>(LocalValue {
                text: &text,
                _not_send: Rc::new(()),
            })
        })))
        .await
        .unwrap();
        assert_eq!(
            events.next().await.unwrap().0.unwrap(),
            imbl_value::json!({"text":"borrowed"})
        );
    }

    #[tokio::test]
    async fn close_reports_cancellation_when_the_receiver_has_gone() {
        let (producer, events) = pair::<(), &str>();
        drop(events);
        assert_cancelled(
            producer
                .close(Ok::<_, RpcError>("late result"))
                .unwrap_err(),
        );
    }

    #[tokio::test]
    async fn failed_item_serialization_does_not_enqueue_or_consume_the_producer() {
        struct Item(bool);
        impl Serialize for Item {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                if self.0 {
                    serializer.serialize_bool(true)
                } else {
                    Err(serde::ser::Error::custom("cannot serialize fixture"))
                }
            }
        }
        let (producer, mut events) = pair::<Item, ()>();
        assert_eq!(producer.send(Item(false)).await.unwrap_err().code, -32603);
        producer.send(Item(true)).await.unwrap();
        producer.close(Ok::<_, RpcError>(())).unwrap();
        assert_eq!(
            events.next().await.unwrap().0.unwrap(),
            imbl_value::json!(true)
        );
        let (result, end, _) = events.next().await.unwrap();
        assert_eq!(result.unwrap(), Value::Null);
        assert!(end);
        assert!(events.next().await.is_none());
    }

    #[tokio::test]
    async fn serialization_accepts_borrowed_non_send_values_before_awaiting() {
        fn is_send<T: Send>(value: T) -> T {
            value
        }
        let (producer, mut events) = pair::<LocalValue<'_>, u64>();
        #[derive(Serialize)]
        struct LocalValue<'a> {
            text: &'a str,
            #[serde(skip)]
            _not_send: Rc<()>,
        }
        let text = "borrowed".to_owned();
        let value = LocalValue {
            text: &text,
            _not_send: Rc::new(()),
        };
        is_send(producer.send(value)).await.unwrap();
        producer.close(Ok::<_, RpcError>(42u64)).unwrap();
        assert_eq!(
            events.next().await.unwrap().0.unwrap(),
            imbl_value::json!({"text":"borrowed"})
        );
        assert_eq!(
            events.next().await.unwrap().0.unwrap(),
            imbl_value::json!(42)
        );
    }
}
