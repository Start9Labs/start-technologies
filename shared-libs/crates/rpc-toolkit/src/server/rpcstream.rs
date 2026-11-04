use std::fmt;
use std::sync::{Arc, Mutex};

use futures::stream::BoxStream;
use futures::{Stream, StreamExt};
use imbl_value::Value;
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
#[cfg(feature = "websocket")]
use yajrc::Id;
use yajrc::RpcError;

use crate::util::{internal_error, invalid_request};
#[cfg(feature = "websocket")]
use crate::RpcResponse;

/// Version of the opt-in response-stream envelope.
pub const RPCSTREAM_VERSION: &str = "1.0";

pub(crate) type RpcValueStream = BoxStream<'static, Result<Value, RpcError>>;

/// Registers a response stream for an opted-in WebSocket request.
///
/// Attach the source before returning the initiating result. The transport sends
/// that result first, then polls the source. Initial failure drops the source;
/// closing the stream or socket drops it without closing unrelated streams.
#[derive(Clone)]
pub struct RpcStream {
    sender: Arc<Mutex<Option<oneshot::Sender<RpcValueStream>>>>,
}

impl fmt::Debug for RpcStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("RpcStream").finish_non_exhaustive()
    }
}

impl RpcStream {
    #[cfg(feature = "websocket")]
    pub(crate) fn registration() -> (Self, oneshot::Receiver<RpcValueStream>) {
        let (sender, receiver) = oneshot::channel();
        (
            Self {
                sender: Arc::new(Mutex::new(Some(sender))),
            },
            receiver,
        )
    }

    /// Attaches a bounded event channel and returns its producer.
    ///
    /// Send `Ok(item)` for an event or `Err(error)` to terminate the stream.
    /// Dropping every sender ends the stream after queued items are delivered.
    /// Cancellation, initial failure, or disconnection closes the receiver;
    /// producers can await `Sender::closed` or observe a send error.
    ///
    /// Async tasks use `Sender::send`; ordinary threads use `Sender::blocking_send`.
    /// The transport does not drain the channel before the initiating reply, so
    /// filling it from the initiating handler can prevent that reply completing.
    ///
    /// # Panics
    ///
    /// Panics if capacity is zero or exceeds Tokio's semaphore limit.
    pub fn channel<T>(&self, capacity: usize) -> Result<mpsc::Sender<Result<T, RpcError>>, RpcError>
    where
        T: Serialize + Send + 'static,
    {
        let (sender, receiver) = mpsc::channel(capacity);
        self.attach(ReceiverStream::new(receiver))?;
        Ok(sender)
    }

    /// Attaches one typed source; a source error terminates its response stream.
    pub fn attach<S, T, E>(&self, source: S) -> Result<(), RpcError>
    where
        S: Stream<Item = Result<T, E>> + Send + 'static,
        T: Serialize + Send + 'static,
        E: Into<RpcError> + Send + 'static,
    {
        let sender = self
            .sender
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| invalid_request("a response stream is already attached"))?;
        sender
            .send(
                source
                    .map(|item| {
                        item.map_err(Into::into)
                            .and_then(|item| imbl_value::to_value(&item).map_err(internal_error))
                    })
                    .boxed(),
            )
            .map_err(|_| invalid_request("the initiating request has finished"))
    }
}

#[cfg(feature = "websocket")]
pub(crate) fn stream_event(id: Id, result: Result<Value, RpcError>, end: bool) -> Value {
    let mut value = imbl_value::to_value(&RpcResponse {
        id: Some(id),
        result,
    })
    .unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("jsonrpc");
    object.insert("rpcstream".into(), RPCSTREAM_VERSION.into());
    object.insert("end".into(), end.into());
    value
}

#[cfg(feature = "websocket")]
pub(crate) fn stream_events(
    mut source: RpcValueStream,
) -> BoxStream<'static, (Result<Value, RpcError>, bool)> {
    async_stream::stream! {
        while let Some(item) = source.next().await {
            let end = item.is_err();
            yield (item, end);
            if end { return; }
        }
        yield (Ok(Value::Null), true);
    }
    .boxed()
}

#[cfg(all(test, feature = "websocket"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn event_channel_bounds_the_queue_and_notifies_producers_on_drop() {
        let (registration, receiver) = RpcStream::registration();
        let sender = registration.channel::<u64>(1).unwrap();
        sender.try_send(Ok(1)).unwrap();
        assert!(matches!(
            sender.try_send(Ok(2)),
            Err(mpsc::error::TrySendError::Full(_))
        ));
        let mut source = receiver.await.unwrap();
        assert_eq!(source.next().await.unwrap().unwrap(), imbl_value::json!(1));
        sender.try_send(Ok(2)).unwrap();
        drop(source);
        assert!(sender.is_closed());
        assert!(sender.send(Ok(3)).await.is_err());
    }

    #[tokio::test]
    async fn channel_and_stream_registration_share_one_source_slot() {
        let (registration, receiver) = RpcStream::registration();
        let sender = registration.channel::<u64>(1).unwrap();
        assert!(registration.clone().channel::<u64>(1).is_err());
        assert!(registration
            .attach(futures::stream::empty::<Result<u64, RpcError>>())
            .is_err());
        sender.try_send(Ok(7)).unwrap();
        let mut source = receiver.await.unwrap();
        assert_eq!(source.next().await.unwrap().unwrap(), imbl_value::json!(7));
        drop(sender);
        assert!(source.next().await.is_none());
    }
}
