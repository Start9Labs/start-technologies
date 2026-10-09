use std::collections::VecDeque;
use std::sync::Arc;

use futures::future::{join_all, BoxFuture};
use futures::{Future, FutureExt, Stream, StreamExt};
use imbl_value::{InternedString, Value};
use yajrc::{RpcError, RpcMethod};

use crate::util::{invalid_request, JobRunner};
use crate::{AnyHandler, Empty, HandleAny, HandleAnyArgs, ParentHandler};

pub type GenericRpcMethod = yajrc::GenericRpcMethod<InternedString, Value, Value>;
pub type RpcRequest = yajrc::RpcRequest<GenericRpcMethod>;
pub type RpcResponse = yajrc::RpcResponse<GenericRpcMethod>;
pub type SingleOrBatchRpcRequest = yajrc::SingleOrBatchRpcRequest<GenericRpcMethod>;

pub mod http;
pub mod socket;

pub use http::*;
pub use socket::*;

pub struct Server<Context: crate::Context> {
    make_ctx: Arc<dyn Fn() -> BoxFuture<'static, Result<Context, RpcError>> + Send + Sync>,
    root_handler: Arc<AnyHandler<Context, Empty, ParentHandler<Context>>>,
}
impl<Context: crate::Context> Clone for Server<Context> {
    fn clone(&self) -> Self {
        Self {
            make_ctx: self.make_ctx.clone(),
            root_handler: self.root_handler.clone(),
        }
    }
}
impl<Context: crate::Context> Server<Context> {
    pub fn new<
        MakeCtx: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Context, RpcError>> + Send + 'static,
    >(
        make_ctx: MakeCtx,
        root_handler: ParentHandler<Context>,
    ) -> Self {
        Server {
            make_ctx: Arc::new(move || make_ctx().boxed()),
            root_handler: Arc::new(AnyHandler::new(root_handler)),
        }
    }

    pub fn handle_command(
        &self,
        method: &str,
        params: Value,
    ) -> impl Future<Output = Result<Value, RpcError>> + Send + 'static + use<Context> {
        let (make_ctx, root_handler, method) = (
            self.make_ctx.clone(),
            self.root_handler.clone(),
            self.root_handler.method_from_dots(method.as_ref()),
        );

        async move {
            root_handler
                .handle_async(HandleAnyArgs {
                    context: make_ctx().await?,
                    parent_method: VecDeque::new(),
                    method: method.ok_or_else(|| yajrc::METHOD_NOT_FOUND_ERROR)?,
                    params,
                    inherited: crate::Empty {},
                })
                .await
        }
    }

    fn handle_single_request(
        &self,
        RpcRequest { id, method, params }: RpcRequest,
    ) -> impl Future<Output = RpcResponse> + Send + 'static {
        let handle = self.handle_command(method.as_str(), params);
        async move {
            RpcResponse {
                id,
                result: handle.await,
            }
        }
    }

    pub fn handle(
        &self,
        request: Result<Value, RpcError>,
    ) -> BoxFuture<'static, Result<Value, imbl_value::Error>> {
        match request.and_then(|request| {
            imbl_value::from_value::<SingleOrBatchRpcRequest>(request).map_err(invalid_request)
        }) {
            Ok(SingleOrBatchRpcRequest::Single(req)) => {
                let fut = self.handle_single_request(req);
                async { imbl_value::to_value(&fut.await) }.boxed()
            }
            Ok(SingleOrBatchRpcRequest::Batch(reqs)) => {
                let futs: Vec<_> = reqs
                    .into_iter()
                    .map(|req| self.handle_single_request(req))
                    .collect();
                async { imbl_value::to_value(&join_all(futs).await) }.boxed()
            }
            Err(e) => async {
                imbl_value::to_value(&RpcResponse {
                    id: None,
                    result: Err(e),
                })
            }
            .boxed(),
        }
    }

    pub fn stream<'a>(
        &'a self,
        requests: impl Stream<Item = Result<Value, RpcError>> + Send + 'a,
    ) -> impl Stream<Item = Result<Value, imbl_value::Error>> + 'a {
        async_stream::try_stream! {
            let mut runner = JobRunner::new();
            let requests = requests.fuse().map(|req| self.handle(req));
            tokio::pin!(requests);

            while let Some(res) = runner.next_result(&mut requests).await.transpose()? {
                yield res;
            }
        }
    }
}
