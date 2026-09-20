use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::Weak;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::future::{BoxFuture, FusedFuture, abortable, pending};
use futures::stream::{AbortHandle, Abortable, BoxStream};
use futures::{Future, FutureExt, Stream, StreamExt};
use tokio::sync::{oneshot, watch};
use tokio::task::{JoinError, JoinHandle, LocalSet};

use crate::prelude::*;

#[pin_project::pin_project(PinnedDrop)]
pub struct NonDetachingJoinHandle<T> {
    #[pin]
    handle: JoinHandle<T>,
    abort_on_drop: bool,
}
impl<T> NonDetachingJoinHandle<T> {
    pub async fn wait_for_abort(self) -> Result<T, JoinError> {
        self.abort();
        self.await
    }

    pub(crate) fn detach(mut self) {
        self.abort_on_drop = false;
    }
}
impl<T> From<JoinHandle<T>> for NonDetachingJoinHandle<T> {
    fn from(handle: JoinHandle<T>) -> Self {
        Self {
            handle,
            abort_on_drop: true,
        }
    }
}

impl<T> Deref for NonDetachingJoinHandle<T> {
    type Target = JoinHandle<T>;
    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}
impl<T> DerefMut for NonDetachingJoinHandle<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.handle
    }
}
#[pin_project::pinned_drop]
impl<T> PinnedDrop for NonDetachingJoinHandle<T> {
    fn drop(self: std::pin::Pin<&mut Self>) {
        let this = self.project();
        if *this.abort_on_drop {
            this.handle.into_ref().get_ref().abort();
        }
    }
}
impl<T> Future for NonDetachingJoinHandle<T> {
    type Output = Result<T, JoinError>;
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.project().handle.poll(cx)
    }
}

#[tokio::test]
async fn test_detach_inside_own_task() {
    async fn survives(detach: bool) -> bool {
        let (handle_send, handle_recv) = oneshot::channel::<NonDetachingJoinHandle<()>>();
        let (done_send, done_recv) = oneshot::channel();
        let task = tokio::spawn(async move {
            let handle = handle_recv.await.unwrap();
            if detach {
                handle.detach();
            } else {
                drop(handle);
            }
            tokio::task::yield_now().await;
            let _ = done_send.send(());
        });
        assert!(handle_send.send(task.into()).is_ok());
        done_recv.await.is_ok()
    }

    assert!(survives(true).await);
    assert!(!survives(false).await);
}

#[pin_project::pin_project(PinnedDrop)]
pub struct DropSignaling<F> {
    #[pin]
    fut: F,
    on_drop: watch::Sender<bool>,
}
impl<F> DropSignaling<F> {
    pub fn new(fut: F) -> Self {
        Self {
            fut,
            on_drop: watch::channel(false).0,
        }
    }
    pub fn subscribe(&self) -> DropHandle {
        DropHandle(self.on_drop.subscribe())
    }
}
impl<F> Future for DropSignaling<F>
where
    F: Future,
{
    type Output = F::Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        this.fut.poll(cx)
    }
}
#[pin_project::pinned_drop]
impl<F> PinnedDrop for DropSignaling<F> {
    fn drop(self: Pin<&mut Self>) {
        let _ = self.on_drop.send(true);
    }
}

#[derive(Clone)]
pub struct DropHandle(watch::Receiver<bool>);
impl DropHandle {
    pub async fn wait(&mut self) {
        let _ = self.0.wait_for(|a| *a).await;
    }
}

#[pin_project::pin_project]
pub struct RemoteCancellable<F> {
    #[pin]
    fut: Abortable<DropSignaling<F>>,
    on_drop: DropHandle,
    handle: AbortHandle,
}
impl<F: Future> RemoteCancellable<F> {
    pub fn new(fut: F) -> Self {
        let sig_fut = DropSignaling::new(fut);
        let on_drop = sig_fut.subscribe();
        let (fut, handle) = abortable(sig_fut);
        Self {
            fut,
            on_drop,
            handle,
        }
    }
}
impl<F> RemoteCancellable<F> {
    pub fn cancellation_handle(&self) -> CancellationHandle {
        CancellationHandle {
            on_drop: self.on_drop.clone(),
            handle: self.handle.clone(),
        }
    }
}
impl<F> Future for RemoteCancellable<F>
where
    F: Future,
{
    type Output = Option<F::Output>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        this.fut.poll(cx).map(|a| a.ok())
    }
}

#[derive(Clone)]
pub struct CancellationHandle {
    on_drop: DropHandle,
    handle: AbortHandle,
}
impl CancellationHandle {
    pub fn cancel(&mut self) {
        self.handle.abort();
    }

    pub async fn cancel_and_wait(&mut self) {
        self.handle.abort();
        self.on_drop.wait().await
    }
}

#[derive(Default)]
pub struct Until<'a> {
    streams: Vec<BoxStream<'a, Result<(), Error>>>,
    async_fns: Vec<Box<dyn FnMut() -> BoxFuture<'a, Result<(), Error>> + Send + 'a>>,
}
impl<'a> Until<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_stream(
        mut self,
        stream: impl Stream<Item = Result<(), Error>> + Send + 'a,
    ) -> Self {
        self.streams.push(stream.boxed());
        self
    }

    pub fn with_async_fn<F, Fut>(mut self, mut f: F) -> Self
    where
        F: FnMut() -> Fut + Send + 'a,
        Fut: Future<Output = Result<(), Error>> + FusedFuture + Send + 'a,
    {
        self.async_fns.push(Box::new(move || f().boxed()));
        self
    }

    pub async fn run<Fut: Future<Output = Result<(), Error>> + Send>(
        &mut self,
        fut: Fut,
    ) -> Result<(), Error> {
        let (res, _, _) = futures::future::select_all(
            self.streams
                .iter_mut()
                .map(|s| {
                    async {
                        s.next().await.transpose()?.ok_or_else(|| {
                            Error::new(eyre!("stream is empty"), ErrorKind::Cancelled)
                        })
                    }
                    .boxed()
                })
                .chain(self.async_fns.iter_mut().map(|f| f()))
                .chain([async {
                    fut.await?;
                    pending().await
                }
                .boxed()]),
        )
        .await;
        res
    }
}

pub async fn make_send<F, Fut, T>(f: F) -> Result<T, Error>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, Error>> + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let local = LocalSet::new();

        local.block_on(&rt, async move { f().await })
    })
    .await
    .map_err(|e| {
        Error::new(
            eyre!("Task running non-Send future panicked: {}", e),
            ErrorKind::Unknown,
        )
    })?
}

#[tokio::test]
async fn test_cancellable() {
    use std::sync::Arc;

    let arc = Arc::new(());
    let weak = Arc::downgrade(&arc);
    let cancellable = RemoteCancellable::new(async move {
        futures::future::pending::<()>().await;
        drop(arc)
    });
    let mut handle = cancellable.cancellation_handle();
    tokio::spawn(cancellable);
    handle.cancel_and_wait().await;
    assert!(weak.strong_count() == 0);
}

#[pin_project::pin_project]
pub struct WeakFuture<Fut> {
    rc: Weak<()>,
    #[pin]
    fut: Fut,
}
impl<Fut> WeakFuture<Fut> {
    pub fn new(rc: Weak<()>, fut: Fut) -> Self {
        Self { rc, fut }
    }
}
impl<Fut: Future> Future for WeakFuture<Fut> {
    type Output = Option<Fut::Output>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        if this.rc.strong_count() > 0 {
            this.fut.poll(cx).map(Some)
        } else {
            Poll::Ready(None)
        }
    }
}

pub struct TimedResource<T: 'static + Send> {
    handle: NonDetachingJoinHandle<Option<T>>,
    ready: oneshot::Sender<()>,
}
impl<T: 'static + Send> TimedResource<T> {
    pub fn new(resource: T, timer: Duration) -> Self {
        let (send, recv) = oneshot::channel();
        let handle = tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(timer) => {
                    drop(resource);
                    None
                },
                _ = recv => Some(resource),
            }
        });
        Self {
            handle: handle.into(),
            ready: send,
        }
    }

    pub fn new_with_destructor<
        Fn: FnOnce(T) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send,
    >(
        resource: T,
        timer: Duration,
        destructor: Fn,
    ) -> Self {
        let (send, recv) = oneshot::channel();
        let handle = tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(timer) => {
                    destructor(resource).await;
                    None
                },
                _ = recv => Some(resource),
            }
        });
        Self {
            handle: handle.into(),
            ready: send,
        }
    }

    pub async fn get(self) -> Option<T> {
        let _ = self.ready.send(());
        self.handle.await.unwrap()
    }

    pub fn is_timed_out(&self) -> bool {
        self.ready.is_closed()
    }
}
