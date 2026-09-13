use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::{Future, FutureExt, StreamExt};
use tokio::sync::mpsc;

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
