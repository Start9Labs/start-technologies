use futures::Future;
use tokio::sync::oneshot::error::TryRecvError;
use tokio::sync::{mpsc, oneshot};

use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::util::actor::background::BackgroundJobQueue;
use crate::util::actor::{Actor, Handler, PendingMessageStrategy, Request};
use crate::util::future::NonDetachingJoinHandle;

pub struct SimpleActor<A: Actor> {
    shutdown: oneshot::Sender<()>,
    runtime: NonDetachingJoinHandle<()>,
    messenger: mpsc::UnboundedSender<Request<A>>,
}
impl<A: Actor> SimpleActor<A> {
    pub fn new(mut actor: A) -> Self {
        let (shutdown_send, mut shutdown_recv) = oneshot::channel();
        let (messenger_send, mut messenger_recv) = mpsc::unbounded_channel::<Request<A>>();
        let runtime = NonDetachingJoinHandle::from(tokio::spawn(async move {
            let (queue, mut runner) = BackgroundJobQueue::new();
            actor.init(&queue);
            loop {
                tokio::select! {
                    _ = &mut runner => (),
                    msg = messenger_recv.recv() => match msg {
                        Some((id, msg, reply)) if shutdown_recv.try_recv() == Err(TryRecvError::Empty) => {
                            tokio::select! {
                                res = msg.handle_with(id, &mut actor, &queue) => { let _ = reply.send(res); },
                                _ = &mut runner => (),
                            }
                        }
                        _ => break,
                    },
                }
            }
        }));
        Self {
            shutdown: shutdown_send,
            runtime,
            messenger: messenger_send,
        }
    }

    /// Enqueues before the returned future is polled.
    pub fn queue<M: Send + 'static>(
        &self,
        message: M,
    ) -> impl Future<Output = Result<A::Response, Error>>
    where
        A: Handler<M>,
    {
        super::enqueue(&self.messenger, Guid::new(), message)
    }

    pub async fn send<M: Send + 'static>(&self, message: M) -> Result<A::Response, Error>
    where
        A: Handler<M>,
    {
        self.queue(message).await
    }

    pub async fn shutdown(self, strategy: PendingMessageStrategy) {
        drop(self.messenger);
        super::shutdown_runtime(self.runtime, self.shutdown, strategy).await;
    }
}
