use std::error::Error;
use std::num::NonZeroUsize;

use axum::body::{Body, Bytes, HttpBody};
use axum::extract::Request;
use axum::handler::Handler;
use axum::response::Response;
use futures::future::{join_all, BoxFuture};
use futures::{Future, FutureExt};
use http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use http::StatusCode;
use http_body_util::LengthLimitError;
use imbl_value::imbl::Vector;
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::Serialize;
use yajrc::{RpcError, RpcMethod};

use crate::server::{RpcRequest, RpcResponse, SingleOrBatchRpcRequest};
use crate::util::{internal_error, parse_error};
use crate::{Context, HandleAny, Server};

const FALLBACK_ERROR: &str = "{\"error\":{\"code\":-32603,\"message\":\"Internal error\",\"data\":\"Failed to serialize rpc response\"}}";

/// Maximum request-body bytes, installed in HTTP request extensions before middleware runs.
#[derive(Clone, Copy, Debug)]
pub struct RequestBodyLimit(pub NonZeroUsize);

impl Default for RequestBodyLimit {
    fn default() -> Self {
        Self(NonZeroUsize::new(1024 * 1024).unwrap())
    }
}

fn payload_too_large() -> Response {
    Response::builder()
        .status(StatusCode::PAYLOAD_TOO_LARGE)
        .body(Body::empty())
        .unwrap()
}

async fn buffer_request_body(
    req: &mut Request,
    limit: RequestBodyLimit,
) -> Result<Bytes, Response> {
    if req.body().size_hint().lower() > limit.0.get() as u64 {
        return Err(payload_too_large());
    }
    let body = axum::body::to_bytes(
        std::mem::replace(req.body_mut(), Body::empty()),
        limit.0.get(),
    )
    .await
    .map_err(|error| {
        if error
            .source()
            .is_some_and(|cause| cause.is::<LengthLimitError>())
        {
            payload_too_large()
        } else {
            json_http_response(&RpcResponse::from(internal_error(error)))
        }
    })?;
    *req.body_mut() = Body::from(body.clone());
    Ok(body)
}

pub fn fallback_rpc_error_response() -> Response {
    Response::builder()
        .header(CONTENT_TYPE, "application/json")
        .header(CONTENT_LENGTH, FALLBACK_ERROR.len())
        .body(Body::from(FALLBACK_ERROR.as_bytes()))
        .unwrap()
}

pub fn json_http_response<T: Serialize>(t: &T) -> Response {
    let body = match serde_json::to_vec(t) {
        Ok(a) => a,
        Err(_) => return fallback_rpc_error_response(),
    };
    Response::builder()
        .header(CONTENT_TYPE, "application/json")
        .header(CONTENT_LENGTH, body.len())
        .body(Body::from(body))
        .unwrap_or_else(|_| fallback_rpc_error_response())
}

pub trait Middleware<Context: Send + 'static>: Clone + Send + Sync + 'static {
    type Metadata: DeserializeOwned + Send + 'static;
    #[allow(unused_variables)]
    fn process_http_request(
        &mut self,
        context: &Context,
        request: &mut Request,
    ) -> impl Future<Output = Result<(), Response>> + Send {
        async { Ok(()) }
    }
    #[allow(unused_variables)]
    fn process_rpc_request(
        &mut self,
        context: &Context,
        metadata: Self::Metadata,
        request: &mut RpcRequest,
    ) -> impl Future<Output = Result<(), RpcResponse>> + Send {
        async { Ok(()) }
    }
    #[allow(unused_variables)]
    fn process_rpc_response(
        &mut self,
        context: &Context,
        response: &mut RpcResponse,
    ) -> impl Future<Output = ()> + Send {
        async { () }
    }
    #[allow(unused_variables)]
    fn process_http_response(
        &mut self,
        context: &Context,
        response: &mut Response,
    ) -> impl Future<Output = ()> + Send {
        async { () }
    }
}

#[allow(private_bounds)]
trait _Middleware<Context>: Send + Sync {
    fn dyn_clone(&self) -> DynMiddleware<Context>;
    fn process_http_request<'a>(
        &'a mut self,
        context: &'a Context,
        request: &'a mut Request,
    ) -> BoxFuture<'a, Result<(), Response>>;
    fn process_rpc_request<'a>(
        &'a mut self,
        context: &'a Context,
        metadata: Value,
        request: &'a mut RpcRequest,
    ) -> BoxFuture<'a, Result<(), RpcResponse>>;
    fn process_rpc_response<'a>(
        &'a mut self,

        context: &'a Context,
        response: &'a mut RpcResponse,
    ) -> BoxFuture<'a, ()>;
    fn process_http_response<'a>(
        &'a mut self,
        context: &'a Context,
        response: &'a mut Response,
    ) -> BoxFuture<'a, ()>;
}
impl<Context: Send + 'static, T: Middleware<Context> + Send + Sync> _Middleware<Context> for T {
    fn dyn_clone(&self) -> DynMiddleware<Context> {
        DynMiddleware(Box::new(<Self as Clone>::clone(&self)))
    }
    fn process_http_request<'a>(
        &'a mut self,
        context: &'a Context,
        request: &'a mut Request,
    ) -> BoxFuture<'a, Result<(), Response>> {
        <Self as Middleware<Context>>::process_http_request(self, context, request).boxed()
    }
    fn process_rpc_request<'a>(
        &'a mut self,
        context: &'a Context,
        metadata: Value,
        request: &'a mut RpcRequest,
    ) -> BoxFuture<'a, Result<(), RpcResponse>> {
        <Self as Middleware<Context>>::process_rpc_request(
            self,
            context,
            match imbl_value::from_value(metadata) {
                Ok(a) => a,
                Err(e) => return async { Err(internal_error(e).into()) }.boxed(),
            },
            request,
        )
        .boxed()
    }
    fn process_rpc_response<'a>(
        &'a mut self,
        context: &'a Context,
        response: &'a mut RpcResponse,
    ) -> BoxFuture<'a, ()> {
        <Self as Middleware<Context>>::process_rpc_response(self, context, response).boxed()
    }
    fn process_http_response<'a>(
        &'a mut self,
        context: &'a Context,
        response: &'a mut Response,
    ) -> BoxFuture<'a, ()> {
        <Self as Middleware<Context>>::process_http_response(self, context, response).boxed()
    }
}

pub struct DynMiddleware<Context>(Box<dyn _Middleware<Context>>);
impl<C: Context> DynMiddleware<C> {
    pub fn new<M: Middleware<C>>(middleware: M) -> Self {
        Self(Box::new(middleware))
    }
    pub fn process_http_request<'a>(
        &'a mut self,
        context: &'a C,
        request: &'a mut Request,
    ) -> BoxFuture<'a, Result<(), Response>> {
        self.0.process_http_request(context, request)
    }
    pub fn process_rpc_request<'a>(
        &'a mut self,
        context: &'a C,
        metadata: Value,
        request: &'a mut RpcRequest,
    ) -> BoxFuture<'a, Result<(), RpcResponse>> {
        self.0.process_rpc_request(context, metadata, request)
    }
    pub fn process_rpc_response<'a>(
        &'a mut self,
        context: &'a C,
        response: &'a mut RpcResponse,
    ) -> BoxFuture<'a, ()> {
        self.0.process_rpc_response(context, response)
    }
    pub fn process_http_response<'a>(
        &'a mut self,
        context: &'a C,
        response: &'a mut Response,
    ) -> BoxFuture<'a, ()> {
        self.0.process_http_response(context, response)
    }
}
impl<Context> Clone for DynMiddleware<Context> {
    fn clone(&self) -> Self {
        self.0.dyn_clone()
    }
}

pub struct HttpServer<Context: crate::Context> {
    pub(crate) inner: Server<Context>,
    middleware: Vector<DynMiddleware<Context>>,
    request_body_limit: RequestBodyLimit,
}
impl<Context: crate::Context> Clone for HttpServer<Context> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            middleware: self.middleware.clone(),
            request_body_limit: self.request_body_limit,
        }
    }
}
impl<Context: crate::Context> Server<Context> {
    pub fn for_http(self) -> HttpServer<Context> {
        HttpServer {
            inner: self,
            middleware: Vector::new(),
            request_body_limit: RequestBodyLimit::default(),
        }
    }
    pub fn middleware<T: Middleware<Context>>(self, middleware: T) -> HttpServer<Context> {
        self.for_http().middleware(middleware)
    }
}
impl<Context: crate::Context> HttpServer<Context> {
    pub fn middleware<T: Middleware<Context>>(mut self, middleware: T) -> Self {
        self.middleware.push_back(DynMiddleware::new(middleware));
        self
    }
    /// Caps the HTTP body before context creation, middleware, and JSON parsing.
    /// Applies to JSON-RPC, MCP, and WebSocket upgrade requests. Defaults to 1 MiB.
    pub fn with_max_request_body_size(mut self, max_bytes: NonZeroUsize) -> Self {
        self.request_body_limit = RequestBodyLimit(max_bytes);
        self
    }

    /// Bounds the request body, builds its context, and runs HTTP request middleware.
    pub(crate) async fn begin_http_request(
        &self,
        req: &mut Request,
    ) -> Result<(Context, Vector<DynMiddleware<Context>>, Bytes), Response> {
        let limit = self.request_body_limit;
        req.extensions_mut().insert(limit);
        buffer_request_body(req, limit).await?;
        let ctx = (self.inner.make_ctx)()
            .await
            .map_err(|e| json_http_response(&RpcResponse::from(e)))?;
        let mut mid = self.middleware.clone();
        for middleware in mid.iter_mut().rev() {
            middleware.process_http_request(&ctx, req).await?;
        }
        let body = buffer_request_body(req, limit).await?;
        Ok((ctx, mid, body))
    }
    pub(crate) async fn finish_http_response(
        ctx: &Context,
        mid: &mut Vector<DynMiddleware<Context>>,
        res: &mut Response,
    ) {
        for middleware in mid.iter_mut() {
            middleware.process_http_response(ctx, res).await;
        }
    }
    async fn process_http_request(&self, mut req: Request) -> Response {
        let (ctx, mut mid, body) = match self.begin_http_request(&mut req).await {
            Ok(a) => a,
            Err(res) => return res,
        };
        drop(req);
        let mid = &mut mid;
        match async {
            let request =
                serde_json::from_slice::<SingleOrBatchRpcRequest>(&body).map_err(parse_error)?;
            drop(body);
            match request {
                SingleOrBatchRpcRequest::Single(rpc_req) => {
                    let mut res = json_http_response(
                        &self
                            .process_rpc_request_with_stream(&ctx, mid, rpc_req, None)
                            .await,
                    );
                    Self::finish_http_response(&ctx, mid, &mut res).await;
                    Ok::<_, RpcError>(res)
                }
                SingleOrBatchRpcRequest::Batch(rpc_reqs) => {
                    let (mids, rpc_res): (Vec<_>, Vec<_>) =
                        join_all(rpc_reqs.into_iter().map(|rpc_req| {
                            let mut mid = mid.clone();
                            let ctx = &ctx;
                            async move {
                                let res = self
                                    .process_rpc_request_with_stream(ctx, &mut mid, rpc_req, None)
                                    .await;
                                (mid, res)
                            }
                        }))
                        .await
                        .into_iter()
                        .unzip();
                    let mut res = json_http_response(&rpc_res);
                    for mut batch_mid in mids.into_iter().fold(
                        vec![Vec::with_capacity(rpc_res.len()); mid.len()],
                        |mut acc, x| {
                            for (idx, middleware) in x.into_iter().enumerate() {
                                acc[idx].push(middleware);
                            }
                            acc
                        },
                    ) {
                        for middleware in batch_mid.iter_mut() {
                            middleware.process_http_response(&ctx, &mut res).await;
                        }
                    }
                    Ok(res)
                }
            }
        }
        .await
        {
            Ok(a) => a,
            Err(e) => json_http_response(&RpcResponse::from(e)),
        }
    }
    pub(crate) async fn process_rpc_request_with_stream(
        &self,
        ctx: &Context,
        mid: &mut Vector<DynMiddleware<Context>>,
        mut req: RpcRequest,
        rpc_stream: Option<crate::RpcStream>,
    ) -> RpcResponse {
        let metadata = Value::Object(
            self.inner
                .root_handler
                .metadata(
                    match self
                        .inner
                        .root_handler
                        .method_from_dots(req.method.as_str())
                    {
                        Some(a) => a,
                        None => {
                            return RpcResponse::new(req.id, Err(yajrc::METHOD_NOT_FOUND_ERROR));
                        }
                    },
                )
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        );
        let id = req.id.clone();
        let mut res = async {
            for middleware in mid.iter_mut().rev() {
                if let Err(res) = middleware
                    .process_rpc_request(ctx, metadata.clone(), &mut req)
                    .await
                {
                    return res;
                }
            }
            self.inner
                .handle_single_request_with_stream(req, rpc_stream)
                .await
        }
        .await;
        for middleware in mid.iter_mut() {
            middleware.process_rpc_response(ctx, &mut res).await;
        }
        res.id = id;
        res
    }
    pub fn handle(&self, req: Request) -> BoxFuture<'static, Response> {
        let server = self.clone();
        async move { server.process_http_request(req).await }.boxed()
    }
}

impl<Context: crate::Context> Handler<(), ()> for HttpServer<Context> {
    type Future = BoxFuture<'static, Response>;
    fn call(self, req: Request, _: ()) -> Self::Future {
        self.handle(req)
    }
}
