use std::any::Any;
use std::sync::Arc;

use futures::future::BoxFuture;
use futures::{Future, FutureExt};
use tokio::sync::{mpsc, oneshot};

use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::util::actor::background::{BackgroundJobQueue, BackgroundJobRunner};
use crate::util::actor::{Actor, ConflictFn, Handler, PendingMessageStrategy, Request};
use crate::util::future::{ControlledTask, TaskControl};

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
        let was_accepting = this.shutdown.is_some();
        *this.shutdown = this.shutdown.take().and_then(|mut s| {
            if s.poll_unpin(cx).is_pending() {
                Some(s)
            } else {
                None
            }
        });
        if was_accepting && this.shutdown.is_none() {
            this.waiting.clear();
            this.recv.close();
        }
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
    runtime: ControlledTask,
    messenger: mpsc::UnboundedSender<Request<A>>,
}
impl<A: Actor + Clone> ConcurrentActor<A> {
    pub fn new(mut actor: A) -> Self {
        let (shutdown_send, shutdown_recv) = oneshot::channel();
        let (messenger_send, messenger_recv) = mpsc::unbounded_channel::<Request<A>>();
        let runtime = ControlledTask::from(tokio::spawn(async move {
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

    /// Enqueues before the returned future is polled.
    pub fn queue<M: Send + 'static>(
        &self,
        id: Guid,
        message: M,
    ) -> impl Future<Output = Result<A::Response, Error>>
    where
        A: Handler<M>,
    {
        super::enqueue(&self.messenger, id, message)
    }

    pub async fn send<M: Send + 'static>(&self, id: Guid, message: M) -> Result<A::Response, Error>
    where
        A: Handler<M>,
    {
        self.queue(id, message).await
    }

    /// Cancels handlers and background work, awaiting release of their resources.
    pub async fn cancel(&self) {
        let _ = self.control().cancel_and_wait().await;
    }

    pub fn control(&self) -> TaskControl {
        self.runtime.control()
    }

    pub async fn shutdown(self, strategy: PendingMessageStrategy) {
        drop(self.messenger);
        super::shutdown_runtime(self.runtime, self.shutdown, strategy).await;
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
    struct HoldResources {
        resource: std::sync::Arc<()>,
        started: tokio::sync::oneshot::Sender<()>,
        background_started: tokio::sync::oneshot::Sender<()>,
    }
    impl Handler<HoldResources> for CActor {
        type Response = ();
        fn conflicts_with(_: &HoldResources) -> ConflictBuilder<Self> {
            ConflictBuilder::nothing()
        }
        async fn handle(
            &mut self,
            _: Guid,
            message: HoldResources,
            jobs: &BackgroundJobQueue,
        ) -> Self::Response {
            let background = message.resource.clone();
            jobs.add_job(async move {
                let _held = background;
                message.background_started.send(()).unwrap();
                futures::future::pending::<()>().await;
            });
            message.started.send(()).unwrap();
            let _held = message.resource;
            futures::future::pending().await
        }
    }

    async fn held_resources(enqueue: impl FnOnce(HoldResources)) -> std::sync::Arc<()> {
        let resource = std::sync::Arc::new(());
        let (started, ready) = tokio::sync::oneshot::channel();
        let (background_started, background_ready) = tokio::sync::oneshot::channel();
        enqueue(HoldResources {
            resource: resource.clone(),
            started,
            background_started,
        });
        ready.await.unwrap();
        background_ready.await.unwrap();
        assert_eq!(std::sync::Arc::strong_count(&resource), 3);
        resource
    }

    async fn holding_actor() -> (super::ConcurrentActor<CActor>, std::sync::Arc<()>) {
        let actor = super::ConcurrentActor::new(CActor);
        let resource = held_resources(|message| drop(actor.queue(Guid::new(), message))).await;
        (actor, resource)
    }

    #[tokio::test]
    async fn cancellation_joins_handler_and_background_resource_release() {
        let (actor, resource) = holding_actor().await;
        actor.cancel().await;
        assert_eq!(std::sync::Arc::strong_count(&resource), 1);
        assert!(actor.send(Guid::new(), NoConflicts).await.is_err());
        actor.cancel().await;
        actor
            .shutdown(crate::util::actor::PendingMessageStrategy::CancelAll)
            .await;
    }

    #[tokio::test]
    async fn cancel_all_shutdown_joins_resource_release() {
        let (actor, resource) = holding_actor().await;
        actor
            .shutdown(crate::util::actor::PendingMessageStrategy::CancelAll)
            .await;
        assert_eq!(std::sync::Arc::strong_count(&resource), 1);
    }

    #[tokio::test]
    async fn simple_actor_cancel_all_joins_resource_release() {
        let actor = crate::util::actor::simple::SimpleActor::new(CActor);
        let resource = held_resources(|message| drop(actor.queue(message))).await;
        actor
            .shutdown(crate::util::actor::PendingMessageStrategy::CancelAll)
            .await;
        assert_eq!(std::sync::Arc::strong_count(&resource), 1);
    }

    #[tokio::test]
    async fn retained_control_interrupts_an_owned_graceful_shutdown() {
        let (actor, resource) = holding_actor().await;
        let control = actor.control();
        let shutdown =
            actor.shutdown(crate::util::actor::PendingMessageStrategy::FinishAll { timeout: None });
        let (_, result) = tokio::join!(shutdown, control.cancel_and_wait());
        assert!(result.unwrap_err().is_cancelled());
        assert_eq!(std::sync::Arc::strong_count(&resource), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cancellation_racing_admission_returns_without_panicking() {
        for _ in 0..128 {
            let actor = super::ConcurrentActor::new(CActor);
            let _ = tokio::join!(actor.cancel(), actor.send(Guid::new(), NoConflicts));
            assert!(actor.send(Guid::new(), NoConflicts).await.is_err());
        }
    }

    struct AwaitRelease {
        started: tokio::sync::oneshot::Sender<()>,
        release: tokio::sync::oneshot::Receiver<()>,
    }
    impl Handler<AwaitRelease> for CActor {
        type Response = ();
        fn conflicts_with(_: &AwaitRelease) -> ConflictBuilder<Self> {
            ConflictBuilder::everything().except::<NoConflicts>()
        }
        async fn handle(&mut self, _: Guid, message: AwaitRelease, _: &BackgroundJobQueue) {
            message.started.send(()).unwrap();
            message.release.await.unwrap();
        }
    }
    struct TrackWork(std::sync::Arc<std::sync::atomic::AtomicUsize>);
    impl Handler<TrackWork> for CActor {
        type Response = ();
        async fn handle(&mut self, _: Guid, message: TrackWork, _: &BackgroundJobQueue) {
            message.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn finish_current_discards_waiting_messages_without_a_timeout() {
        let actor = super::ConcurrentActor::new(CActor);
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, released) = tokio::sync::oneshot::channel();
        drop(actor.queue(
            Guid::new(),
            AwaitRelease {
                started,
                release: released,
            },
        ));
        ready.await.unwrap();
        let pending = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        drop(actor.queue(Guid::new(), TrackWork(pending.clone())));
        actor.send(Guid::new(), NoConflicts).await.unwrap();
        let shutdown = actor.shutdown(
            crate::util::actor::PendingMessageStrategy::FinishCurrentCancelPending {
                timeout: None,
            },
        );
        let (result, ()) = tokio::join!(
            tokio::time::timeout(Duration::from_secs(2), shutdown),
            async {
                release.send(()).unwrap();
            },
        );
        result.unwrap();
        assert_eq!(pending.load(std::sync::atomic::Ordering::SeqCst), 0);
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
