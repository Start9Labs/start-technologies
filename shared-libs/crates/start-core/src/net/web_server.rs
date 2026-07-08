use core::fmt;
use std::any::Any;
use std::collections::BTreeMap;
use std::future::Future;
use std::net::SocketAddr;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Poll, ready};
use std::time::Duration;

use axum::Router;
use futures::FutureExt;
use futures::future::Either;
use http::Extensions;
use hyper_util::rt::{TokioIo, TokioTimer};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use visit_rs::{Visit, VisitFields, Visitor};

use crate::net::static_server::{UiContext, ui_router};
use crate::net::utils::bind_tokio_listener;
use crate::prelude::*;
use crate::util::actor::background::BackgroundJobQueue;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::ReadWriter;
use crate::util::sync::{SyncRwLock, Watch};

pub type AcceptStream = Pin<Box<dyn ReadWriter + Send + 'static>>;

pub trait MetadataVisitor: Visitor<Result = ()> {
    fn visit<M: Clone + Send + Sync + 'static>(&mut self, metadata: &M) -> Self::Result;
}

pub struct ExtensionVisitor<'a>(&'a mut Extensions);
impl<'a> Visitor for ExtensionVisitor<'a> {
    type Result = ();
}
impl<'a> MetadataVisitor for ExtensionVisitor<'a> {
    fn visit<M: Clone + Send + Sync + 'static>(&mut self, metadata: &M) -> Self::Result {
        self.0.insert(metadata.clone());
    }
}
impl<'a> Visit<ExtensionVisitor<'a>>
    for Box<dyn for<'x> Visit<ExtensionVisitor<'x>> + Send + Sync + 'static>
{
    fn visit(
        &self,
        visitor: &mut ExtensionVisitor<'a>,
    ) -> <ExtensionVisitor<'a> as Visitor>::Result {
        (&**self).visit(visitor)
    }
}

pub struct ExtractVisitor<T>(Option<T>);
impl<T> Visitor for ExtractVisitor<T> {
    type Result = ();
}
impl<T: Clone + Send + Sync + 'static> MetadataVisitor for ExtractVisitor<T> {
    fn visit<M: Clone + Send + Sync + 'static>(&mut self, metadata: &M) -> Self::Result {
        if let Some(matching) = (metadata as &dyn Any).downcast_ref::<T>() {
            self.0 = Some(matching.clone());
        }
    }
}
pub fn extract<
    T: Clone + Send + Sync + 'static,
    M: Visit<ExtractVisitor<T>> + Clone + Send + Sync + 'static,
>(
    metadata: &M,
) -> Option<T> {
    let mut visitor = ExtractVisitor(None);
    metadata.visit(&mut visitor);
    visitor.0
}

#[derive(Clone, Copy, Debug)]
pub struct TcpMetadata {
    pub peer_addr: SocketAddr,
    pub local_addr: SocketAddr,
}
impl<V: MetadataVisitor> Visit<V> for TcpMetadata {
    fn visit(&self, visitor: &mut V) -> <V as visit_rs::Visitor>::Result {
        visitor.visit(self)
    }
}

pub trait Accept {
    type Metadata: fmt::Debug;
    fn poll_accept(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(Self::Metadata, AcceptStream), Error>>;
    fn into_dyn(self) -> DynAccept
    where
        Self: Sized + Send + Sync + 'static,
        for<'a> Self::Metadata: Visit<ExtensionVisitor<'a>> + Send + Sync + 'static,
    {
        DynAccept::new(self)
    }
}

impl Accept for TcpListener {
    type Metadata = TcpMetadata;
    fn poll_accept(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(Self::Metadata, AcceptStream), Error>> {
        if let Poll::Ready((stream, peer_addr)) = TcpListener::poll_accept(self, cx)? {
            if let Err(e) = socket2::SockRef::from(&stream)
                .set_tcp_keepalive(&crate::net::utils::default_keepalive())
            {
                tracing::error!("Failed to set tcp keepalive: {e}");
                tracing::debug!("{e:?}");
            }
            return Poll::Ready(Ok((
                TcpMetadata {
                    local_addr: self.local_addr()?,
                    peer_addr,
                },
                Box::pin(stream),
            )));
        }
        Poll::Pending
    }
}

impl<A> Accept for Vec<A>
where
    A: Accept,
{
    type Metadata = A::Metadata;
    fn poll_accept(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(Self::Metadata, AcceptStream), Error>> {
        for listener in self {
            if let Poll::Ready(accepted) = listener.poll_accept(cx)? {
                return Poll::Ready(Ok(accepted));
            }
        }
        Poll::Pending
    }
}

#[derive(Debug, Clone, VisitFields)]
pub struct MapListenerMetadata<K, M> {
    pub inner: M,
    pub key: K,
}
impl<K, M, V> Visit<V> for MapListenerMetadata<K, M>
where
    V: MetadataVisitor,
    K: Visit<V>,
    M: Visit<V>,
{
    fn visit(&self, visitor: &mut V) -> <V as Visitor>::Result {
        self.visit_fields(visitor).collect()
    }
}

impl<K, A> Accept for BTreeMap<K, A>
where
    K: Clone + fmt::Debug,
    A: Accept,
{
    type Metadata = MapListenerMetadata<K, A::Metadata>;
    fn poll_accept(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(Self::Metadata, AcceptStream), Error>> {
        for (key, listener) in self {
            if let Poll::Ready((metadata, stream)) = listener.poll_accept(cx)? {
                return Poll::Ready(Ok((
                    MapListenerMetadata {
                        inner: metadata,
                        key: key.clone(),
                    },
                    stream,
                )));
            }
        }
        Poll::Pending
    }
}

impl<A, B> Accept for Either<A, B>
where
    A: Accept,
    B: Accept<Metadata = A::Metadata>,
{
    type Metadata = A::Metadata;
    fn poll_accept(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(Self::Metadata, AcceptStream), Error>> {
        match self {
            Either::Left(a) => a.poll_accept(cx),
            Either::Right(b) => b.poll_accept(cx),
        }
    }
}
impl<A: Accept> Accept for Option<A> {
    type Metadata = A::Metadata;
    fn poll_accept(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(Self::Metadata, AcceptStream), Error>> {
        match self {
            None => Poll::Pending,
            Some(a) => a.poll_accept(cx),
        }
    }
}

trait DynAcceptT: Send + Sync {
    fn poll_accept(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(DynMetadata, AcceptStream), Error>>;
}
impl<A> DynAcceptT for A
where
    A: Accept + Send + Sync,
    <A as Accept>::Metadata: DynMetadataT + 'static,
{
    fn poll_accept(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(DynMetadata, AcceptStream), Error>> {
        let (metadata, stream) = ready!(Accept::poll_accept(self, cx)?);
        Poll::Ready(Ok((DynMetadata(Box::new(metadata)), stream)))
    }
}
pub struct DynAccept(Box<dyn DynAcceptT>);
trait DynMetadataT: for<'a> Visit<ExtensionVisitor<'a>> + fmt::Debug + Send + Sync {}
impl<T> DynMetadataT for T where for<'a> T: Visit<ExtensionVisitor<'a>> + fmt::Debug + Send + Sync {}

#[derive(Debug)]
pub struct DynMetadata(Box<dyn DynMetadataT>);
impl<'a> Visit<ExtensionVisitor<'a>> for DynMetadata {
    fn visit(
        &self,
        visitor: &mut ExtensionVisitor<'a>,
    ) -> <ExtensionVisitor<'a> as Visitor>::Result {
        self.0.visit(visitor)
    }
}

impl Accept for DynAccept {
    type Metadata = DynMetadata;
    fn poll_accept(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(Self::Metadata, AcceptStream), Error>> {
        DynAcceptT::poll_accept(&mut *self.0, cx)
    }
    fn into_dyn(self) -> DynAccept
    where
        Self: Sized,
        for<'a> Self::Metadata: Visit<ExtensionVisitor<'a>> + Send + Sync + 'static,
    {
        self
    }
}
impl DynAccept {
    pub fn new<A>(accept: A) -> Self
    where
        A: Accept + Send + Sync + 'static,
        for<'a> <A as Accept>::Metadata: Visit<ExtensionVisitor<'a>> + Send + Sync + 'static,
    {
        Self(Box::new(accept))
    }
}

#[pin_project::pin_project]
pub struct Acceptor<A: Accept> {
    acceptor: Watch<A>,
}
impl<A: Accept + Send + Sync + 'static> Acceptor<A> {
    pub fn new(acceptor: A) -> Self {
        Self {
            acceptor: Watch::new(acceptor),
        }
    }

    fn poll_changed(&mut self, cx: &mut std::task::Context<'_>) -> Poll<()> {
        self.acceptor.poll_changed(cx)
    }

    fn poll_accept(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(A::Metadata, AcceptStream), Error>> {
        while self.poll_changed(cx).is_ready() {}
        self.acceptor.peek_mut(|a| a.poll_accept(cx))
    }

    async fn accept(&mut self) -> Result<(A::Metadata, AcceptStream), Error> {
        std::future::poll_fn(|cx| self.poll_accept(cx)).await
    }
}
impl Acceptor<Vec<TcpListener>> {
    pub async fn bind(listen: impl IntoIterator<Item = SocketAddr>) -> Result<Self, Error> {
        Ok(Self::new(
            listen
                .into_iter()
                .map(|addr| bind_tokio_listener(addr).with_kind(ErrorKind::Network))
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }
}
impl Acceptor<Vec<DynAccept>> {
    pub async fn bind_dyn(listen: impl IntoIterator<Item = SocketAddr>) -> Result<Self, Error> {
        Ok(Self::new(
            listen
                .into_iter()
                .map(|addr| {
                    bind_tokio_listener(addr)
                        .with_kind(ErrorKind::Network)
                        .map(DynAccept::new)
                })
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }
}
impl<K> Acceptor<BTreeMap<K, TcpListener>>
where
    K: Ord + Clone + fmt::Debug + Send + Sync + 'static,
{
    pub async fn bind_map(
        listen: impl IntoIterator<Item = (K, SocketAddr)>,
    ) -> Result<Self, Error> {
        Ok(Self::new(
            listen
                .into_iter()
                .map(|(key, addr)| {
                    bind_tokio_listener(addr)
                        .with_kind(ErrorKind::Network)
                        .map(|l| (key, l))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?,
        ))
    }
}
impl<K> Acceptor<BTreeMap<K, DynAccept>>
where
    K: Ord + Clone + fmt::Debug + Send + Sync + 'static,
{
    pub async fn bind_map_dyn(
        listen: impl IntoIterator<Item = (K, SocketAddr)>,
    ) -> Result<Self, Error> {
        Ok(Self::new(
            listen
                .into_iter()
                .map(|(key, addr)| {
                    bind_tokio_listener(addr)
                        .with_kind(ErrorKind::Network)
                        .map(|l| (key, l.into_dyn()))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?,
        ))
    }
}

pub struct WebServerAcceptorSetter<A: Accept> {
    acceptor: Watch<A>,
}
impl<A: Accept> Deref for WebServerAcceptorSetter<A> {
    type Target = Watch<A>;
    fn deref(&self) -> &Self::Target {
        &self.acceptor
    }
}

pub struct WebServer<A: Accept> {
    shutdown: oneshot::Sender<()>,
    router: Watch<Router>,
    acceptor: Watch<A>,
    thread: NonDetachingJoinHandle<()>,
}
impl<A> WebServer<A>
where
    A: Accept + Send + Sync + 'static,
    for<'a> A::Metadata: Visit<ExtensionVisitor<'a>> + Send + Sync + 'static,
{
    pub fn acceptor_setter(&self) -> WebServerAcceptorSetter<A> {
        WebServerAcceptorSetter {
            acceptor: self.acceptor.clone(),
        }
    }

    pub fn new(mut acceptor: Acceptor<A>, router: Router) -> Self {
        let acceptor_send = acceptor.acceptor.clone();
        let router = Watch::new(router);
        let service = router.clone_unseen();
        let (shutdown, shutdown_recv) = oneshot::channel();
        let thread = NonDetachingJoinHandle::from(tokio::spawn(async move {
            #[derive(Clone)]
            struct QueueRunner {
                queue: Arc<SyncRwLock<Option<BackgroundJobQueue>>>,
            }
            impl<Fut> hyper::rt::Executor<Fut> for QueueRunner
            where
                Fut: Future + Send + 'static,
            {
                fn execute(&self, fut: Fut) {
                    // Run each connection (and hyper's per-stream futures) on its
                    // own task instead of serializing them all on the runner's
                    // single poll loop — h2's per-byte framing/flow-control work
                    // otherwise caps single-stream upload throughput. The handle
                    // stays in the runner wrapped in NonDetachingJoinHandle, which
                    // aborts the task on drop, preserving shutdown-cancel.
                    self.queue.peek(|q| {
                        if let Some(q) = q {
                            q.add_job(NonDetachingJoinHandle::from(tokio::spawn(async move {
                                fut.await;
                            })));
                        } else {
                            tracing::warn!("job queued after shutdown");
                        }
                    })
                }
            }

            struct SwappableRouter<M> {
                router: Watch<Router>,
                metadata: M,
            }
            impl<M: for<'a> Visit<ExtensionVisitor<'a>> + Send + Sync + 'static>
                hyper::service::Service<hyper::Request<hyper::body::Incoming>>
                for SwappableRouter<M>
            {
                type Response = <Router as tower_service::Service<
                    hyper::Request<hyper::body::Incoming>,
                >>::Response;
                type Error = <Router as tower_service::Service<
                    hyper::Request<hyper::body::Incoming>,
                >>::Error;
                type Future = <Router as tower_service::Service<
                    hyper::Request<hyper::body::Incoming>,
                >>::Future;

                fn call(&self, mut req: hyper::Request<hyper::body::Incoming>) -> Self::Future {
                    use tower_service::Service;

                    self.metadata
                        .visit(&mut ExtensionVisitor(req.extensions_mut()));

                    self.router.read().call(req)
                }
            }

            let queue_cell = Arc::new(SyncRwLock::new(None));
            let graceful = hyper_util::server::graceful::GracefulShutdown::new();
            let mut server = hyper_util::server::conn::auto::Builder::new(QueueRunner {
                queue: queue_cell.clone(),
            });
            server
                .http1()
                .timer(TokioTimer::new())
                .title_case_headers(true)
                .preserve_header_case(true)
                .http2()
                .timer(TokioTimer::new())
                .enable_connect_protocol()
                .adaptive_window(true)
                .keep_alive_interval(Duration::from_secs(25))
                .keep_alive_timeout(Duration::from_secs(300));
            let (queue, mut runner) = BackgroundJobQueue::new();
            queue_cell.replace(Some(queue.clone()));

            let handler = async {
                loop {
                    let mut err = None;
                    for _ in 0..5 {
                        if let Err(e) = async {
                            let (metadata, stream) = acceptor.accept().await?;
                            let conn = graceful.watch(
                                server
                                    .serve_connection_with_upgrades(
                                        TokioIo::new(stream),
                                        SwappableRouter {
                                            router: service.clone(),
                                            metadata,
                                        },
                                    )
                                    .into_owned(),
                            );
                            queue.add_job(NonDetachingJoinHandle::from(tokio::spawn(async move {
                                let _ = conn.await;
                            })));

                            Ok::<_, Error>(())
                        }
                        .await
                        {
                            err = Some(e);
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        } else {
                            break;
                        }
                    }
                    if let Some(e) = err {
                        tracing::error!("Error accepting HTTP connection: {e}");
                        tracing::debug!("{e:?}");
                    }
                }
            }
            .boxed();

            tokio::select! {
                _ = shutdown_recv => (),
                _ = handler => (),
                _ = &mut runner => (),
            }

            drop(queue);
            drop(queue_cell.replace(None));

            if !runner.is_empty() {
                tokio::time::timeout(Duration::from_millis(100), runner)
                    .await
                    .log_err();
            }
        }));
        Self {
            shutdown,
            router,
            thread,
            acceptor: acceptor_send,
        }
    }

    pub async fn shutdown(self) {
        self.shutdown.send(()).unwrap_or_default();
        self.thread.await.unwrap()
    }

    pub fn serve_router(&mut self, router: Router) {
        self.router.send(router)
    }

    pub fn serve_ui_for<C: UiContext>(&mut self, ctx: C) {
        self.serve_router(ui_router(ctx))
    }
}
