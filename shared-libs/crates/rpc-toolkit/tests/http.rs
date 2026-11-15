use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::extract::Request;
use futures::stream;
use http::StatusCode;
use http_body_util::BodyExt;
use rpc_toolkit::{
    from_fn_async, Context, HandlerExt, HttpServer, Middleware, ParentHandler, RequestBodyLimit,
    RpcResponse, Server,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Clone, Default)]
struct TestContext {
    created: Arc<AtomicUsize>,
    middleware: Arc<AtomicUsize>,
    dispatched: Arc<AtomicUsize>,
}
impl Context for TestContext {}

#[derive(Deserialize)]
struct EchoParams {
    text: String,
}

#[derive(Clone)]
struct CollectBody;

impl Middleware<TestContext> for CollectBody {
    type Metadata = rpc_toolkit::Empty;

    async fn process_http_request(
        &mut self,
        context: &TestContext,
        request: &mut Request,
    ) -> Result<(), axum::response::Response> {
        context.middleware.fetch_add(1, Ordering::SeqCst);
        let limit = request
            .extensions()
            .get::<RequestBodyLimit>()
            .unwrap()
            .0
            .get();
        let body = std::mem::replace(request.body_mut(), Body::empty())
            .collect()
            .await
            .unwrap()
            .to_bytes();
        assert!(body.len() <= limit);
        *request.body_mut() = Body::from(body);
        Ok(())
    }
}

fn bare_server(context: TestContext) -> Server<TestContext> {
    Server::new(
        move || {
            let context = context.clone();
            async move {
                context.created.fetch_add(1, Ordering::SeqCst);
                Ok(context)
            }
        },
        ParentHandler::new().subcommand(
            "echo",
            from_fn_async(|context: TestContext, params: EchoParams| async move {
                context.dispatched.fetch_add(1, Ordering::SeqCst);
                Ok::<_, yajrc::RpcError>(params.text.len())
            })
            .no_cli()
            .no_ts(),
        ),
    )
}

fn server(context: TestContext) -> HttpServer<TestContext> {
    bare_server(context).middleware(CollectBody)
}

fn body(text: &str) -> Vec<u8> {
    serde_json::to_vec(
        &json!({"jsonrpc": "2.0", "id": 1, "method": "echo", "params": {"text": text}}),
    )
    .unwrap()
}

fn request(body: Body) -> Request {
    Request::builder()
        .method("POST")
        .uri("/rpc")
        .body(body)
        .unwrap()
}

fn rejected_before_dispatch(context: &TestContext) {
    assert_eq!(context.created.load(Ordering::SeqCst), 0);
    assert_eq!(context.middleware.load(Ordering::SeqCst), 0);
    assert_eq!(context.dispatched.load(Ordering::SeqCst), 0);
}

#[derive(Clone)]
struct RejectRpc;
impl Middleware<TestContext> for RejectRpc {
    type Metadata = rpc_toolkit::Empty;
    async fn process_rpc_request(
        &mut self,
        _: &TestContext,
        _: Self::Metadata,
        _: &mut rpc_toolkit::RpcRequest,
    ) -> Result<(), RpcResponse> {
        Err(RpcResponse::from(yajrc::INVALID_PARAMS_ERROR))
    }
    async fn process_rpc_response(&mut self, context: &TestContext, response: &mut RpcResponse) {
        context.middleware.fetch_add(1, Ordering::SeqCst);
        response.id = Some(yajrc::Id::String("not-the-request".into()));
    }
}

#[tokio::test]
async fn middleware_errors_keep_the_original_full_integer_id_after_response_hooks() {
    let context = TestContext::default();
    let server = bare_server(context.clone()).middleware(RejectRpc);
    let response = server
        .handle(request(Body::from(
            r#"{"id":18446744073709551615,"method":"echo","params":{"text":"x"}}"#,
        )))
        .await;
    let response: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(response["id"].as_u64(), Some(u64::MAX));
    assert_eq!(response["error"]["code"], yajrc::INVALID_PARAMS_ERROR.code);
    assert_eq!(context.middleware.load(Ordering::SeqCst), 1);
    assert_eq!(context.dispatched.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn floating_point_parameters_retain_binary64_roundtrip_precision() {
    let server = Server::new(
        || async { Ok(TestContext::default()) },
        ParentHandler::new().subcommand(
            "bits",
            from_fn_async(|_: TestContext, params: imbl_value::Value| async move {
                Ok::<_, yajrc::RpcError>(params["number"].as_f64().unwrap().to_bits())
            })
            .no_cli()
            .no_ts(),
        ),
    )
    .for_http();
    let response = server
        .handle(request(Body::from(
            r#"{"id":1,"method":"bits","params":{"number":51.248178375505404}}"#,
        )))
        .await;
    let response: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(
        response["result"].as_u64(),
        Some(51.248178375505404_f64.to_bits())
    );
}

#[tokio::test]
async fn exact_limit_preserves_bytes_through_body_reading_middleware() {
    let context = TestContext::default();
    let bytes = body("boundary");
    let server =
        server(context.clone()).with_max_request_body_size(NonZeroUsize::new(bytes.len()).unwrap());
    let response = server.handle(request(Body::from(bytes))).await;
    assert_eq!(response.status(), StatusCode::OK);
    let response: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(response["result"], 8);
    assert_eq!(context.middleware.load(Ordering::SeqCst), 1);
    assert_eq!(context.dispatched.load(Ordering::SeqCst), 1);
}

#[derive(Clone)]
struct ReplaceBody(Vec<u8>);

impl Middleware<TestContext> for ReplaceBody {
    type Metadata = rpc_toolkit::Empty;

    async fn process_http_request(
        &mut self,
        context: &TestContext,
        request: &mut Request,
    ) -> Result<(), axum::response::Response> {
        context.middleware.fetch_add(1, Ordering::SeqCst);
        *request.body_mut() = Body::from(self.0.clone());
        Ok(())
    }
}

#[tokio::test]
async fn middleware_body_replacements_are_bounded_before_json_decoding() {
    let context = TestContext::default();
    let original = body("small");
    let response = bare_server(context.clone())
        .middleware(ReplaceBody(body(&"x".repeat(1024))))
        .with_max_request_body_size(NonZeroUsize::new(original.len()).unwrap())
        .handle(request(Body::from(original)))
        .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(context.created.load(Ordering::SeqCst), 1);
    assert_eq!(context.middleware.load(Ordering::SeqCst), 1);
    assert_eq!(context.dispatched.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn rejects_known_oversized_bodies_before_context_or_middleware() {
    let context = TestContext::default();
    let bytes = body("boundary");
    let server = server(context.clone())
        .with_max_request_body_size(NonZeroUsize::new(bytes.len() - 1).unwrap());
    let response = server.handle(request(Body::from(bytes))).await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    rejected_before_dispatch(&context);
}

#[tokio::test]
async fn streamed_body_limit_ignores_missing_or_false_content_lengths_and_stops_reading() {
    for content_length in [None, Some("1")] {
        let context = TestContext::default();
        let chunks = stream::iter((0..4).map(|index| {
            assert!(
                index < 3,
                "must stop reading after exceeding the byte limit"
            );
            Ok::<_, std::io::Error>(Bytes::from_static(b"[[[[[[[[[[[[[[[["))
        }));
        let mut request = request(Body::from_stream(chunks));
        if let Some(content_length) = content_length {
            request.headers_mut().insert(
                http::header::CONTENT_LENGTH,
                content_length.parse().unwrap(),
            );
        }
        let response = server(context.clone())
            .with_max_request_body_size(NonZeroUsize::new(32).unwrap())
            .handle(request)
            .await;
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        rejected_before_dispatch(&context);
    }
}

#[tokio::test]
async fn default_limit_is_enforced_and_explicit_larger_limits_are_preserved_by_clone() {
    let context = TestContext::default();
    let text = "x".repeat(RequestBodyLimit::default().0.get());
    let bytes = body(&text);
    let response = server(context.clone())
        .handle(request(Body::from(bytes.clone())))
        .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    rejected_before_dispatch(&context);

    let configured =
        server(context.clone()).with_max_request_body_size(NonZeroUsize::new(bytes.len()).unwrap());
    let response = configured.clone().handle(request(Body::from(bytes))).await;
    assert_eq!(response.status(), StatusCode::OK);
    let response: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(response["result"], text.len());
    assert_eq!(context.dispatched.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn body_read_errors_remain_rpc_errors_rather_than_size_rejections() {
    let context = TestContext::default();
    let stream = stream::once(async { Err::<Bytes, _>(std::io::Error::other("read failed")) });
    let response = server(context.clone())
        .handle(request(Body::from_stream(stream)))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response: RpcResponse =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(response.result.unwrap_err().code, -32603);
    rejected_before_dispatch(&context);
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn upgrade_body_uses_the_http_limit_before_upgrade_middleware() {
    let context = TestContext::default();
    let response = server(context.clone())
        .with_max_request_body_size(NonZeroUsize::new(32).unwrap())
        .for_websocket()
        .handle(request(Body::from(vec![b'x'; 33])))
        .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    rejected_before_dispatch(&context);
}
