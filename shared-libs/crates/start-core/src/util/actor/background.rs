use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::{Future, FutureExt, StreamExt};
use tokio::sync::mpsc;

use crate::util::future::{CancellationHandle, RemoteCancellable};

#[derive(Clone)]
pub struct BackgroundJobQueue(mpsc::UnboundedSender<BoxFuture<'static, ()>>);
impl BackgroundJobQueue {
    pub fn new() -> (Self, BackgroundJobRunner) {
        let (send, recv) = mpsc::unbounded_channel();
        (
            Self(send),
            BackgroundJobRunner {
                recv,
                jobs: FuturesUnordered::new(),
            },
        )
    }
    /// Runs on the runner's task. Dropping the handle does not cancel the job.
    pub fn add_cancellable_job(&self, fut: impl Future + Send + 'static) -> CancellationHandle {
        let future = RemoteCancellable::new(fut);
        let handle = future.cancellation_handle();
        self.add_job(future);
        handle
    }

    pub fn add_job(&self, fut: impl Future + Send + 'static) {
        let _ = self.0.send(
            async {
                fut.await;
            }
            .boxed(),
        );
    }
}

pub struct BackgroundJobRunner {
    recv: mpsc::UnboundedReceiver<BoxFuture<'static, ()>>,
    jobs: FuturesUnordered<BoxFuture<'static, ()>>,
}
impl BackgroundJobRunner {
    pub fn is_empty(&self) -> bool {
        self.recv.is_empty() && self.jobs.is_empty()
    }
}
impl Future for BackgroundJobRunner {
    type Output = ();
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        while let std::task::Poll::Ready(Some(job)) = self.recv.poll_recv(cx) {
            self.jobs.push(job);
        }
        while let std::task::Poll::Ready(Some(())) = self.jobs.poll_next_unpin(cx) {}
        if self.jobs.is_empty() && self.recv.is_closed() {
            std::task::Poll::Ready(())
        } else {
            std::task::Poll::Pending
        }
    }
}
impl BackgroundJobRunner {
    pub fn run_while<Fut: Future + Send>(
        &mut self,
        fut: Fut,
    ) -> impl Future<Output = Fut::Output> + Send {
        #[pin_project::pin_project]
        struct RunWhile<'a, Fut> {
            #[pin]
            runner: &'a mut BackgroundJobRunner,
            #[pin]
            fut: Fut,
        }
        impl<'a, Fut: Future> Future for RunWhile<'a, Fut> {
            type Output = Fut::Output;
            fn poll(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Self::Output> {
                let this = self.project();
                let _ = this.runner.poll(cx);
                this.fut.poll(cx)
            }
        }
        RunWhile { runner: self, fut }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::{Mutex, oneshot};

    use super::*;

    #[tokio::test]
    async fn cancellable_jobs_run_on_the_callers_task() {
        let (jobs, mut runner) = BackgroundJobQueue::new();
        let parent = tokio::task::try_id();
        let (send, recv) = oneshot::channel();
        let handle = jobs.add_cancellable_job(async move {
            assert_eq!(tokio::task::try_id(), parent);
            send.send(()).unwrap();
        });
        runner.run_while(recv).await.unwrap();
        assert!(handle.is_finished());
    }

    #[tokio::test]
    async fn cancellation_releases_job_locks_while_foreground_work_waits() {
        let (jobs, mut runner) = BackgroundJobQueue::new();
        let lock = Arc::new(Mutex::new(()));
        let job_lock = lock.clone();
        let (send, recv) = oneshot::channel();
        let mut handle = jobs.add_cancellable_job(async move {
            let _guard = job_lock.lock().await;
            send.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        runner
            .run_while(async {
                recv.await.unwrap();
                handle.cancel_and_wait().await;
                let _guard = lock.lock().await;
                assert!(handle.is_finished());
            })
            .await;
    }
}
