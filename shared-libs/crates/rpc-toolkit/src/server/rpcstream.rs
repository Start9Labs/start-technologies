use std::borrow::Cow;
use std::future::Future;
#[cfg(feature = "websocket")]
use std::num::NonZeroUsize;

#[cfg(feature = "websocket")]
use futures::stream::BoxStream;
#[cfg(feature = "websocket")]
use futures::StreamExt;
use imbl_value::Value;
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};
#[cfg(feature = "websocket")]
use yajrc::Id;
use yajrc::RpcError;

use crate::util::internal_error;
#[cfg(feature = "websocket")]
use crate::RpcResponse;

/// Version of the opt-in response-stream envelope.
pub const RPCSTREAM_VERSION: &str = "1.0";

/// Cancellation of an opted-in call or an explicitly unclosed producer.
pub const RPCSTREAM_CANCELLED_ERROR: RpcError = RpcError {
    code: -32800,
    message: Cow::Borrowed("Response stream canceled"),
    data: None,
};

#[cfg(feature = "websocket")]
pub(crate) type ResponseEvents = BoxStream<'static, (Result<Value, RpcError>, bool)>;

/// A single-owner producer for an opted-in WebSocket response stream.
///
/// Each event and the terminal result may have independent serializable types.
/// The transport sends the initiating JSON-RPC response before draining events.
/// Dropping this handle without `close` terminates with `RPCSTREAM_CANCELLED_ERROR`
/// after queued events. Initial failure, client closure, and socket loss close
/// the receiver instead; `closed` and pending sends then wake the producer.
#[derive(Debug)]
pub struct RpcStream {
    events: mpsc::Sender<Value>,
    terminal: oneshot::Sender<Result<Value, RpcError>>,
}

impl RpcStream {
    #[cfg(feature = "websocket")]
    pub(crate) fn new(capacity: NonZeroUsize) -> (Self, ResponseEvents) {
        let (events, mut receiver) = mpsc::channel(capacity.get());
        let (terminal, result) = oneshot::channel();
        let source = async_stream::stream! {
            while let Some(value) = receiver.recv().await {
                yield (Ok(value), false);
            }
            yield (result.await.unwrap_or(Err(RPCSTREAM_CANCELLED_ERROR)), true);
        }
        .boxed();
        (Self { events, terminal }, source)
    }

    /// Serializes an event immediately; awaiting waits for bounded queue capacity.
    ///
    /// A serialization error sends nothing and leaves the handle usable. Filling
    /// the queue before returning the initiating result can block that result.
    pub fn send<T: Serialize>(
        &self,
        value: T,
    ) -> impl Future<Output = Result<(), RpcError>> + Send + '_ {
        let value = serialize(value);
        async move {
            self.events
                .send(value?)
                .await
                .map_err(|_| RPCSTREAM_CANCELLED_ERROR)
        }
    }

    /// Sends an event from an ordinary thread, waiting for queue capacity.
    ///
    /// # Panics
    ///
    /// Panics inside an asynchronous execution context, as Tokio's blocking sender does.
    pub fn blocking_send<T: Serialize>(&self, value: T) -> Result<(), RpcError> {
        self.events
            .blocking_send(serialize(value)?)
            .map_err(|_| RPCSTREAM_CANCELLED_ERROR)
    }

    /// Consumes the handle and queues its terminal result after pending events.
    ///
    /// Uses a reserved slot, independent of event-buffer capacity. A terminal
    /// serialization failure becomes an internal-error result on the wire.
    pub fn close<T: Serialize, E: Into<RpcError>>(
        self,
        result: Result<T, E>,
    ) -> Result<(), RpcError> {
        let result = result.map_err(Into::into).and_then(serialize);
        self.terminal
            .send(result)
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
    let mut value = imbl_value::to_value(&RpcResponse::new(Some(id), result)).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("jsonrpc");
    object.insert("rpcstream".into(), RPCSTREAM_VERSION.into());
    object.insert("end".into(), end.into());
    value
}

#[cfg(all(test, feature = "websocket"))]
mod tests {
    use std::rc::Rc;

    use super::*;

    fn pair() -> (RpcStream, ResponseEvents) {
        RpcStream::new(NonZeroUsize::new(1).unwrap())
    }

    fn assert_cancelled(error: RpcError) {
        assert_eq!(error.code, -32800);
        assert_eq!(error.message, "Response stream canceled");
        assert!(error.data.is_none());
    }

    #[tokio::test]
    async fn close_uses_a_reserved_slot_after_a_full_event_queue() {
        let (producer, mut events) = pair();
        producer.send(1u64).await.unwrap();
        producer.close(Ok::<_, RpcError>("finished")).unwrap();
        assert_eq!(
            events.next().await.unwrap().0.unwrap(),
            imbl_value::json!(1)
        );
        let (result, end) = events.next().await.unwrap();
        assert_eq!(result.unwrap(), imbl_value::json!("finished"));
        assert!(end);
        assert!(events.next().await.is_none());
    }

    #[tokio::test]
    async fn dropping_without_close_drains_events_then_emits_fixed_cancellation() {
        let (producer, mut events) = pair();
        producer.send(7u64).await.unwrap();
        drop(producer);
        let (value, end) = events.next().await.unwrap();
        assert_eq!(value.unwrap(), imbl_value::json!(7));
        assert!(!end);
        let (error, end) = events.next().await.unwrap();
        assert_cancelled(error.unwrap_err());
        assert!(end);
        assert!(events.next().await.is_none());
    }

    #[tokio::test]
    async fn send_is_backpressured_and_receiver_loss_unblocks_it() {
        let (producer, events) = pair();
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
        let (producer, mut events) = pair();
        producer.send("last event").await.unwrap();
        producer.close::<(), _>(Err(Failure)).unwrap();
        assert_eq!(
            events.next().await.unwrap().0.unwrap(),
            imbl_value::json!("last event")
        );
        let (result, end) = events.next().await.unwrap();
        assert_eq!(result.unwrap_err().code, 456);
        assert!(end);
        assert!(events.next().await.is_none());
    }

    #[tokio::test]
    async fn close_reports_cancellation_when_the_receiver_has_gone() {
        let (producer, events) = pair();
        drop(events);
        assert_cancelled(
            producer
                .close(Ok::<_, RpcError>("late result"))
                .unwrap_err(),
        );
    }

    #[tokio::test]
    async fn serialization_accepts_borrowed_non_send_values_before_awaiting() {
        fn is_send<T: Send>(value: T) -> T {
            value
        }
        let (producer, mut events) = pair();
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
