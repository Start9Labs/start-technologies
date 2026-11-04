use std::fmt;
use std::sync::{Arc, Mutex};

use futures::stream::BoxStream;
use futures::{Stream, StreamExt};
use imbl_value::Value;
use serde::Serialize;
use tokio::sync::oneshot;
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
    pub(crate) fn channel() -> (Self, oneshot::Receiver<RpcValueStream>) {
        let (sender, receiver) = oneshot::channel();
        (
            Self {
                sender: Arc::new(Mutex::new(Some(sender))),
            },
            receiver,
        )
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
