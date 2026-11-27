use std::borrow::Cow;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use axum::body::Body;
use axum::response::Response;
use futures::{FutureExt, StreamExt};
use http_body_util::{BodyStream, StreamBody};
use tokio_util::sync::CancellationToken;
use tokio_util::task::task_tracker::TaskTrackerToken;
use tokio_util::task::TaskTracker;

pub(crate) const REQUEST_CANCELLED_ERROR: yajrc::RpcError = yajrc::RpcError {
    code: -32800,
    message: Cow::Borrowed("Request canceled"),
    data: None,
};

#[derive(Default)]
struct Requests {
    // TaskTracker::close does not seal token admission.
    rejection: Mutex<Option<yajrc::RpcError>>,
    tracker: TaskTracker,
    cancellation: CancellationToken,
}

/// Cancels request producers independently of their HTTP/WebSocket transports.
#[derive(Clone, Default)]
pub struct RequestTasks(Arc<Requests>);

struct ScopeInner {
    owner: RequestTasks,
    token: Mutex<Option<TaskTrackerToken>>,
}
impl ScopeInner {
    fn release(&self) {
        let token = self.token.lock().unwrap().take();
        drop(token);
    }
    fn is_active(&self) -> bool {
        self.token.lock().unwrap().is_some()
    }
}

tokio::task_local! {
    static REQUEST: Arc<ScopeInner>;
}

/// The remaining future must only observe completion or assemble a transport reply.
/// Further service work requires a separately admitted child scope.
pub fn preserve_request_reply() {
    let _ = REQUEST.try_with(|scope| scope.release());
}

/// Running blocking callbacks outlive cancellation of their caller.
pub(crate) fn retain_current_request<F: FnOnce() -> T + Send, T>(
    call: F,
) -> impl FnOnce() -> T + Send {
    let retained = RequestOwned {
        value: call,
        scope: REQUEST.try_with(Arc::clone).ok(),
    };
    move || retained.call()
}

impl RequestTasks {
    pub(crate) fn current() -> Option<Self> {
        REQUEST.try_with(|scope| scope.owner.clone()).ok()
    }

    pub(crate) async fn run_current<F: Future>(future: F) -> Option<F::Output> {
        match Self::current() {
            Some(owner) => owner.run(future).await,
            None => Some(future.await),
        }
    }

    /// Registers before polling; refused futures are destroyed outside the admission lock.
    pub fn run<F: Future>(&self, future: F) -> impl Future<Output = Option<F::Output>> {
        let scope = {
            let rejection = self.0.rejection.lock().unwrap();
            rejection.is_none().then(|| {
                Arc::new(ScopeInner {
                    owner: self.clone(),
                    token: Mutex::new(Some(self.0.tracker.token())),
                })
            })
        };
        let future = if scope.is_some() {
            Some(future)
        } else {
            drop(future);
            None
        };
        track(scope, future)
    }

    /// Carries request ownership across a transport-owned task boundary.
    pub(crate) async fn transport<F: Future>(&self, future: F) -> F::Output {
        REQUEST
            .scope(
                Arc::new(ScopeInner {
                    owner: self.clone(),
                    token: Mutex::new(None),
                }),
                future,
            )
            .await
    }

    /// Closes admission with an application-owned error; repeated closure preserves it.
    pub fn close(&self, error: yajrc::RpcError) {
        let mut rejection = self.0.rejection.lock().unwrap();
        if rejection.is_none() {
            *rejection = Some(error);
            self.0.tracker.close();
            self.0.cancellation.cancel();
        }
    }

    /// The error supplied when admission closed.
    pub fn rejection(&self) -> Option<yajrc::RpcError> {
        self.0.rejection.lock().unwrap().clone()
    }

    /// Closes admission and waits for producer destruction; passive replies remain connected.
    pub async fn quiesce(&self) {
        self.close(REQUEST_CANCELLED_ERROR);
        self.0.tracker.wait().await;
    }
}

fn track<F: Future>(
    scope: Option<Arc<ScopeInner>>,
    future: Option<F>,
) -> impl Future<Output = Option<F::Output>> {
    let inner_scope = scope.clone();
    RequestOwned {
        value: Some(async move {
            let scope = inner_scope?;
            let cancelled = scope.owner.0.cancellation.clone().cancelled_owned();
            REQUEST
                .scope(scope.clone(), async move {
                    let mut future = Box::pin(future.expect("admitted request"));
                    tokio::select! {
                        biased;
                        result = &mut future => Some(result),
                        _ = cancelled => {
                            if scope.is_active() { None }
                            else { Some(future.await) }
                        }
                    }
                })
                .await
        }),
        scope,
    }
}

#[pin_project::pin_project]
struct RequestOwned<T> {
    // Resource destruction precedes quiescence acknowledgement, including unpolled work.
    #[pin]
    value: T,
    scope: Option<Arc<ScopeInner>>,
}
impl<F> RequestOwned<F> {
    fn call<T>(self) -> T
    where
        F: FnOnce() -> T,
    {
        match self.scope {
            Some(scope) => REQUEST.sync_scope(scope, self.value),
            None => (self.value)(),
        }
    }
}
impl<F: Future> Future for RequestOwned<Option<F>> {
    type Output = F::Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        let result = this.value.as_mut().as_pin_mut().unwrap().poll(cx);
        if result.is_ready() {
            this.value.set(None);
            this.scope.take();
        }
        result
    }
}

/// Body production must remain cancellable while the peer stops reading its transport.
/// The application retains and joins the submitted task.
pub fn retain_response_body(
    mut response: Response,
    spawn: impl FnOnce(futures::future::BoxFuture<'static, ()>),
) -> Response {
    if let Ok(scope) = REQUEST.try_with(Arc::clone) {
        let body = std::mem::replace(response.body_mut(), Body::empty());
        let (send, receive) = tokio::sync::mpsc::channel(1);
        let operation = track(
            Some(scope),
            Some(async move {
                let mut body = BodyStream::new(body);
                while let Some(frame) = body.next().await {
                    if send.send(frame).await.is_err() {
                        break;
                    }
                }
            }),
        );
        spawn(
            async move {
                let _ = operation.await;
            }
            .boxed(),
        );
        *response.body_mut() = Body::new(StreamBody::new(
            tokio_stream::wrappers::ReceiverStream::new(receive),
        ));
    }
    response
}

#[cfg(test)]
mod tests;
