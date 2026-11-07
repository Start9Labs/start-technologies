use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use futures::future::{BoxFuture, ready};
use futures::{Future, FutureExt, TryFutureExt};
use tokio::sync::{mpsc, oneshot};

use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::util::actor::background::{BackgroundJobQueue, BackgroundJobRunner};
use crate::util::actor::{Actor, ConflictFn, Handler, PendingMessageStrategy, Request};
use crate::util::future::NonDetachingJoinHandle;

#[pin_project::pin_project]
struct ConcurrentRunner<A> {
    actor: A,
    shutdown: Option<oneshot::Receiver<()>>,
    waiting: Vec<Request<A>>,
    recv: mpsc::UnboundedReceiver<Request<A>>,
    handlers: Vec<(
        &'static str,
        Guid,
        Arc<ConflictFn<A>>,
        oneshot::Sender<Box<dyn Any + Send>>,
        BoxFuture<'static, Box<dyn Any + Send>>,
    )>,
    queue: BackgroundJobQueue,
    #[pin]
    bg_runner: BackgroundJobRunner,
}
impl<A: Actor + Clone> Future for ConcurrentRunner<A> {
    type Output = ();
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let mut this = self.project();
        *this.shutdown = this.shutdown.take().and_then(|mut s| {
            if s.poll_unpin(cx).is_pending() {
                Some(s)
            } else {
                None
            }
        });
        if this.shutdown.is_some() {
            while let std::task::Poll::Ready(Some((id, msg, reply))) = this.recv.poll_recv(cx) {
                if this
                    .handlers
                    .iter()
                    .any(|(_, hid, f, _, _)| &id != hid && f(&*msg))
                {
                    #[cfg(feature = "unstable")]
                    {
                        tracing::debug!("{} must wait...", msg.type_name());
                        tracing::debug!(
                            "waiting on {:?}",
                            this.handlers
                                .iter()
                                .filter(|h| h.2(&*msg))
                                .map(|h| (h.0, &h.1))
                                .collect::<Vec<_>>()
                        );
                    }
                    this.waiting.push((id, msg, reply));
                } else {
                    let mut actor = this.actor.clone();
                    let queue = this.queue.clone();
                    this.handlers.push((
                        msg.type_name(),
                        id.clone(),
                        msg.conflicts_with(),
                        reply,
                        async move { msg.handle_with(id, &mut actor, &queue).await }.boxed(),
                    ))
                }
            }
        }
        // handlers
        while {
            let mut cont = false;
            let complete = this
                .handlers
                .iter_mut()
                .enumerate()
                .filter_map(|(i, (_, _, _, _, f))| match f.poll_unpin(cx) {
                    std::task::Poll::Pending => None,
                    std::task::Poll::Ready(res) => Some((i, res)),
                })
                .collect::<Vec<_>>();
            for (idx, res) in complete.into_iter().rev() {
                #[allow(clippy::let_underscore_future)]
                let (_, _, f, reply, _) = this.handlers.swap_remove(idx);
                reply.send(res).ok();
                if this.shutdown.is_some() {
                    let mut i = 0;
                    while i < this.waiting.len() {
                        if f(&*this.waiting[i].1)
                            && !this
                                .handlers
                                .iter()
                                .any(|(_, _, f, _, _)| f(&*this.waiting[i].1))
                        {
                            let (id, msg, reply) = this.waiting.remove(i);
                            let mut actor = this.actor.clone();
                            let queue = this.queue.clone();
                            this.handlers.push((
                                msg.type_name(),
                                id.clone(),
                                msg.conflicts_with(),
                                reply,
                                async move { msg.handle_with(id, &mut actor, &queue).await }
                                    .boxed(),
                            ));
                            cont = true;
                        } else {
                            #[cfg(feature = "unstable")]
                            {
                                tracing::debug!("{} must wait...", this.waiting[i].1.type_name());
                                tracing::debug!(
                                    "waiting on {:?}",
                                    this.handlers
                                        .iter()
                                        .filter(|h| h.2(&*this.waiting[i].1))
                                        .map(|h| (h.0, &h.1))
                                        .collect::<Vec<_>>()
                                );
                            }
                            i += 1;
                        }
                    }
                }
            }
            cont
        } {}
        let _ = this.bg_runner.as_mut().poll(cx);
        if this.waiting.is_empty() && this.handlers.is_empty() && this.recv.is_closed() {
            std::task::Poll::Ready(())
        } else {
            std::task::Poll::Pending
        }
    }
}

pub struct ConcurrentActor<A: Actor + Clone> {
    shutdown: oneshot::Sender<()>,
    runtime: NonDetachingJoinHandle<()>,
    messenger: mpsc::UnboundedSender<Request<A>>,
}
impl<A: Actor + Clone> ConcurrentActor<A> {
    pub fn new(mut actor: A) -> Self {
        let (shutdown_send, shutdown_recv) = oneshot::channel();
        let (messenger_send, messenger_recv) = mpsc::unbounded_channel::<Request<A>>();
        let runtime = NonDetachingJoinHandle::from(tokio::spawn(async move {
            let (queue, runner) = BackgroundJobQueue::new();
            actor.init(&queue);
            ConcurrentRunner {
                actor,
                shutdown: Some(shutdown_recv),
                waiting: Vec::new(),
                recv: messenger_recv,
                handlers: Vec::new(),
                queue,
                bg_runner: runner,
            }
            .await
        }));
        Self {
            shutdown: shutdown_send,
            runtime,
            messenger: messenger_send,
        }
    }

    /// Message is guaranteed to be queued immediately
    pub fn queue<M: Send + 'static>(
        &self,
        id: Guid,
        message: M,
    ) -> impl Future<Output = Result<A::Response, Error>>
    where
        A: Handler<M>,
    {
        if self.runtime.is_finished() {
            return futures::future::Either::Left(ready(Err(Error::new(
                eyre!("actor runtime has exited"),
                ErrorKind::Unknown,
            ))));
        }
        let (reply_send, reply_recv) = oneshot::channel();
        self.messenger
            .send((id, Box::new(message), reply_send))
            .unwrap();
        futures::future::Either::Right(
            reply_recv
                .map_err(|_| Error::new(eyre!("actor runtime has exited"), ErrorKind::Unknown))
                .and_then(|a| {
                    ready(
                        a.downcast()
                            .map_err(|_| {
                                Error::new(
                                    eyre!("received incorrect type in response"),
                                    ErrorKind::Incoherent,
                                )
                            })
                            .map(|a| *a),
                    )
                }),
        )
    }

    pub async fn send<M: Send + 'static>(&self, id: Guid, message: M) -> Result<A::Response, Error>
    where
        A: Handler<M>,
    {
        self.queue(id, message).await
    }

    pub async fn shutdown(mut self, strategy: PendingMessageStrategy) {
        drop(self.messenger);
        let timeout = match strategy {
            PendingMessageStrategy::CancelAll => {
                self.shutdown.send(()).unwrap();
                Some(Duration::from_secs(0))
            }
            PendingMessageStrategy::FinishCurrentCancelPending { timeout } => {
                self.shutdown.send(()).unwrap();
                timeout
            }
            PendingMessageStrategy::FinishAll { timeout } => timeout,
        };
        let aborter = if let Some(timeout) = timeout {
            let hdl = self.runtime.abort_handle();
            async move {
                tokio::time::sleep(timeout).await;
                hdl.abort();
            }
            .boxed()
        } else {
            futures::future::pending().boxed()
        };
        tokio::select! {
            _ = aborter => { let _ = (&mut self.runtime).await; },
            _ = &mut self.runtime => (),
        }
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use crate::rpc_continuations::Guid;
    use crate::util::actor::background::BackgroundJobQueue;
    use crate::util::actor::{Actor, ConflictBuilder, Handler};

    #[derive(Clone)]
    struct CActor;
    impl Actor for CActor {
        fn init(&mut self, _: &BackgroundJobQueue) {}
    }
    struct Pending;
    impl Handler<Pending> for CActor {
        type Response = ();
        fn conflicts_with(_: &Pending) -> ConflictBuilder<Self> {
            ConflictBuilder::everything().except::<NoConflicts>()
        }
        async fn handle(&mut self, _: Guid, _: Pending, _: &BackgroundJobQueue) -> Self::Response {
            futures::future::pending().await
        }
    }
    struct Conflicts;
    impl Handler<Conflicts> for CActor {
        type Response = ();
        fn conflicts_with(_: &Conflicts) -> ConflictBuilder<Self> {
            ConflictBuilder::everything().except::<NoConflicts>()
        }
        async fn handle(
            &mut self,
            _: Guid,
            _: Conflicts,
            _: &BackgroundJobQueue,
        ) -> Self::Response {
        }
    }
    struct NoConflicts;
    impl Handler<NoConflicts> for CActor {
        type Response = ();
        fn conflicts_with(_: &NoConflicts) -> ConflictBuilder<Self> {
            ConflictBuilder::nothing()
        }
        async fn handle(
            &mut self,
            _: Guid,
            _: NoConflicts,
            _: &BackgroundJobQueue,
        ) -> Self::Response {
        }
    }
    #[derive(Clone)]
    struct HeldActor(std::sync::Arc<()>);
    impl Actor for HeldActor {
        fn init(&mut self, jobs: &BackgroundJobQueue) {
            let held = self.0.clone();
            jobs.add_job(async move {
                let _held = held;
                futures::future::pending::<()>().await;
            });
        }
    }
    impl Handler<Pending> for HeldActor {
        type Response = ();
        fn conflicts_with(_: &Pending) -> ConflictBuilder<Self> {
            ConflictBuilder::everything()
        }
        async fn handle(&mut self, _: Guid, _: Pending, _: &BackgroundJobQueue) {
            futures::future::pending::<()>().await;
        }
    }

    #[tokio::test]
    async fn force_stop_actor_cancellation_releases_seed_before_returning() {
        let held = std::sync::Arc::new(());
        let actor = super::ConcurrentActor::new(HeldActor(held.clone()));
        drop(actor.queue(Guid::new(), Pending));
        tokio::task::yield_now().await;
        assert!(std::sync::Arc::strong_count(&held) > 1);
        actor
            .shutdown(crate::util::actor::PendingMessageStrategy::CancelAll)
            .await;
        assert_eq!(std::sync::Arc::strong_count(&held), 1);
    }

    #[tokio::test]
    async fn test_conflicts() {
        let actor = super::ConcurrentActor::new(CActor);
        let guid = Guid::new();
        let pending = actor.queue(guid.clone(), Pending);
        assert!(
            tokio::time::timeout(Duration::from_secs(1), actor.send(Guid::new(), Conflicts))
                .await
                .is_err()
        );
        assert!(
            tokio::time::timeout(Duration::from_secs(1), actor.send(Guid::new(), NoConflicts))
                .await
                .is_ok()
        );
        assert!(
            tokio::time::timeout(Duration::from_secs(1), actor.send(guid, Conflicts))
                .await
                .is_ok()
        );
        assert!(
            tokio::time::timeout(Duration::from_secs(1), pending)
                .await
                .is_err()
        );
    }
}
