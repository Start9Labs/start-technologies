use std::marker::PhantomData;
use std::sync::{Arc, Weak};

use rpc_toolkit::{RPCSTREAM_CANCELLED_ERROR, RpcStreamResponse};
use serde::de::DeserializeOwned;
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore, mpsc, oneshot};

use super::*;

pub(super) enum ResponseSlot {
    Call(oneshot::Sender<Result<Value, RpcError>>),
    Stream(StreamSlot),
    /// Retains the ID until the owned reader sends its close frame.
    Canceled,
}

pub(super) struct StreamSlot {
    initial: Option<oneshot::Sender<Result<rpc_toolkit::RpcResponse, RpcError>>>,
    events: mpsc::UnboundedSender<QueuedItem>,
    terminal: Option<oneshot::Sender<Completion>>,
    budget: Arc<Semaphore>,
}

struct QueuedItem {
    value: imbl_value::Value,
    _bytes: OwnedSemaphorePermit,
}

enum Completion {
    Terminal(Result<imbl_value::Value, RpcError>),
    Failed(RpcError),
}

pub(super) struct Cancellation {
    pub responses: Weak<Responses>,
    pub notify: Arc<Notify>,
    pub id: Id,
}

impl Cancellation {
    fn cancel(&self) {
        if let Some(responses) = self.responses.upgrade() {
            responses.mutate(|pending| {
                if let Some(slot) = pending.as_mut().and_then(|map| map.get_mut(&self.id)) {
                    if matches!(slot, ResponseSlot::Call(_) | ResponseSlot::Stream(_)) {
                        *slot = ResponseSlot::Canceled;
                        self.notify.notify_one();
                    }
                }
            });
        }
    }
}

impl Drop for Cancellation {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Initiating success is independent of progress and the successful terminal value.
pub struct RpcResponseStream<Initial, Item, Final> {
    /// The initiating method result.
    pub initial: Initial,
    /// Metadata attached to the initiating reply.
    pub meta: imbl_value::Value,
    /// Decoded items and the terminal result, retrieved with `finish`.
    pub items: RpcResponseItems<Item, Final>,
}

/// Dropping the receiver cancels its request, including before the first item.
/// Queued events share a byte budget including per-item storage, independent of item count.
pub struct RpcResponseItems<Item, Final> {
    events: mpsc::UnboundedReceiver<QueuedItem>,
    terminal: oneshot::Receiver<Completion>,
    outcome: Option<Completion>,
    finished: bool,
    cancel: Cancellation,
    types: PhantomData<fn() -> (Item, Final)>,
}

impl StreamSlot {
    pub(super) fn new<Item, Final>(
        cancel: Cancellation,
    ) -> (
        Self,
        oneshot::Receiver<Result<rpc_toolkit::RpcResponse, RpcError>>,
        RpcResponseItems<Item, Final>,
    ) {
        let (initial, result) = oneshot::channel();
        let (events, receiver) = mpsc::unbounded_channel();
        let (terminal, completed) = oneshot::channel();
        (
            Self {
                initial: Some(initial),
                events,
                terminal: Some(terminal),
                budget: Arc::new(Semaphore::new(
                    crate::util::net::secure::DEFAULT_RPC_RESPONSE_LIMIT,
                )),
            },
            result,
            RpcResponseItems {
                events: receiver,
                terminal: completed,
                outcome: None,
                finished: false,
                cancel,
                types: PhantomData,
            },
        )
    }

    pub(super) fn initial(&mut self, result: rpc_toolkit::RpcResponse) -> SlotAction {
        let Some(sender) = self.initial.take() else {
            self.fail(rpc_toolkit::yajrc::INVALID_REQUEST_ERROR);
            return SlotAction::Cancel;
        };
        let success = result.result.is_ok();
        let delivered = sender.send(Ok(result)).is_ok();
        if !delivered {
            SlotAction::Cancel
        } else if success {
            SlotAction::Keep
        } else {
            SlotAction::Remove
        }
    }

    pub(super) fn event(&mut self, response: RpcStreamResponse, bytes: usize) -> SlotAction {
        if self.initial.is_some() {
            self.fail(rpc_toolkit::yajrc::INVALID_REQUEST_ERROR);
            return SlotAction::Cancel;
        }
        if response.end {
            let _ = self
                .terminal
                .take()
                .unwrap()
                .send(Completion::Terminal(response.result));
            return SlotAction::Remove;
        }
        let value = match response.result {
            Ok(value) => value,
            Err(error) => {
                self.fail(error);
                return SlotAction::Cancel;
            }
        };
        let bytes = u32::try_from(bytes.saturating_add(std::mem::size_of::<QueuedItem>()))
            .ok()
            .and_then(|bytes| self.budget.clone().try_acquire_many_owned(bytes).ok());
        if let Some(bytes) = bytes {
            if self
                .events
                .send(QueuedItem {
                    value,
                    _bytes: bytes,
                })
                .is_ok()
            {
                return SlotAction::Keep;
            }
        }
        self.fail(RPCSTREAM_CANCELLED_ERROR);
        SlotAction::Cancel
    }

    fn fail(&mut self, error: RpcError) {
        if let Some(initial) = self.initial.take() {
            let _ = initial.send(Err(error.clone()));
        }
        if let Some(terminal) = self.terminal.take() {
            let _ = terminal.send(Completion::Failed(error));
        }
    }
}

pub(super) enum SlotAction {
    Keep,
    Remove,
    Cancel,
}

impl<Item: DeserializeOwned, Final: DeserializeOwned> RpcResponseItems<Item, Final> {
    /// Yields accepted items before a terminal error; transport loss discards queued items.
    pub async fn next(&mut self) -> Result<Option<Item>, RpcError> {
        if self.finished {
            return Ok(None);
        }
        loop {
            if let Some(Completion::Failed(error)) = &self.outcome {
                self.finished = true;
                self.cancel.cancel();
                self.events.close();
                while self.events.try_recv().is_ok() {}
                return Err(error.clone());
            }
            tokio::select! {
                biased;
                terminal = &mut self.terminal, if self.outcome.is_none() => {
                    self.outcome = Some(terminal.unwrap_or_else(|_| Completion::Failed(disconnected().into())));
                }
                event = self.events.recv() => {
                    if let Some(event) = event {
                        match imbl_value::from_value(event.value) {
                            Ok(value) => return Ok(Some(value)),
                            Err(error) => {
                                self.outcome = Some(Completion::Failed(RpcError::from(error)));
                            }
                        }
                    } else {
                        if self.outcome.is_none() {
                            self.outcome = Some((&mut self.terminal).await.unwrap_or_else(|_| Completion::Failed(disconnected().into())));
                            continue;
                        }
                        self.finished = true;
                        return match self.outcome.as_ref().unwrap() {
                            Completion::Terminal(Err(error)) => Err(error.clone()),
                            _ => Ok(None),
                        };
                    }
                }
            }
        }
    }

    /// Drains remaining items and returns the actual terminal result.
    pub async fn finish(mut self) -> Result<Final, RpcError> {
        while self.next().await?.is_some() {}
        match self.outcome.take().unwrap() {
            Completion::Terminal(result) => Ok(imbl_value::from_value(result?)?),
            Completion::Failed(error) => Err(error),
        }
    }
}
