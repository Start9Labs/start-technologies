use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::body::{Body, Bytes, to_bytes};
use axum::extract::Request;
use axum::response::Response;
use axum::routing::{any, get};
use futures::future::ready;
use http::{HeaderMap, Method, StatusCode};
use rpc_toolkit::{Context, Empty, HandlerExt, Middleware, ParentHandler, Server, from_fn};

use crate::net::static_server::rpc_router;
use crate::rpc_continuations::RpcContinuations;
use crate::util::future::NonDetachingJoinHandle;

const ORIGIN: &str = "https://console.test";

#[derive(Clone)]
struct TestContext(Arc<RpcContinuations>);
impl Context for TestContext {}
impl AsRef<RpcContinuations> for TestContext {
    fn as_ref(&self) -> &RpcContinuations {
        &self.0
    }
}

#[derive(Clone)]
struct BodyReader(Arc<AtomicUsize>);
impl Middleware<TestContext> for BodyReader {
    type Metadata = Empty;

    async fn process_http_request(
        &mut self,
        _: &TestContext,
        request: &mut Request,
    ) -> Result<(), Response> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let body = std::mem::replace(request.body_mut(), Body::empty());
        *request.body_mut() = Body::from(to_bytes(body, usize::MAX).await.unwrap());
        Ok(())
    }
}

async fn serve(router: Router) -> (String, NonDetachingJoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    (
        url,
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() }).into(),
    )
}

async fn fixture() -> (
    String,
    NonDetachingJoinHandle<()>,
    Arc<AtomicUsize>,
    Arc<AtomicUsize>,
) {
    let contexts = Arc::new(AtomicUsize::new(0));
    let readers = Arc::new(AtomicUsize::new(0));
    let context = TestContext(Arc::new(RpcContinuations::new()));
    let server = Server::new(
        {
            let contexts = contexts.clone();
            let context = context.clone();
            move || {
                contexts.fetch_add(1, Ordering::SeqCst);
                ready(Ok(context.clone()))
            }
        },
        ParentHandler::new().subcommand(
            "call",
            from_fn(|_: TestContext, _: Empty| {
                Ok::<_, rpc_toolkit::yajrc::RpcError>("ok".to_owned())
            })
            .no_cli(),
        ),
    )
    .middleware(BodyReader(readers.clone()))
    .with_max_request_body_size(NonZeroUsize::new(64).unwrap());
    let router = rpc_router(context, any(server).layer(super::layer()))
        .route("/asset", get(|| async { "asset" }));
    let (url, server) = serve(router).await;
    (url, server, contexts, readers)
}

fn assert_origin(headers: &HeaderMap) {
    assert_eq!(headers["access-control-allow-origin"], ORIGIN);
    assert_eq!(headers["access-control-allow-credentials"], "true");
    assert!(
        headers
            .get_all("vary")
            .iter()
            .any(|value| value.to_str().unwrap().contains("origin"))
    );
}

#[tokio::test]
async fn oversized_declared_and_chunked_bodies_retain_cors_without_context_or_auth_body_reads() {
    let (url, _server, contexts, readers) = fixture().await;
    for body in [
        reqwest::Body::from("x".repeat(65)),
        reqwest::Body::wrap_stream(futures::stream::iter([
            Ok::<_, std::io::Error>(Bytes::from(vec![b'x'; 40])),
            Ok(Bytes::from(vec![b'x'; 40])),
        ])),
    ] {
        let response = reqwest::Client::new()
            .post(format!("{url}/rpc/v0"))
            .header("origin", ORIGIN)
            .body(body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_origin(response.headers());
    }
    assert_eq!(contexts.load(Ordering::SeqCst), 0);
    assert_eq!(readers.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn preflight_mirrors_requested_permissions_without_entering_rpc() {
    let (url, _server, contexts, readers) = fixture().await;
    let response = reqwest::Client::new()
        .request(Method::OPTIONS, format!("{url}/rpc/v0"))
        .header("origin", ORIGIN)
        .header("access-control-request-method", "POST")
        .header(
            "access-control-request-headers",
            "content-type, x-start-auth-sig",
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_origin(response.headers());
    assert_eq!(response.headers()["access-control-allow-methods"], "POST");
    assert_eq!(
        response.headers()["access-control-allow-headers"],
        "content-type, x-start-auth-sig"
    );
    assert_eq!(contexts.load(Ordering::SeqCst), 0);
    assert_eq!(readers.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn rpc_results_and_parse_errors_retain_cors_without_exposing_resources() {
    let (url, _server, contexts, readers) = fixture().await;
    let client = reqwest::Client::new();
    for (body, error) in [
        (
            r#"{"jsonrpc":"2.0","id":1,"method":"call","params":{}}"#,
            false,
        ),
        ("invalid", true),
    ] {
        let response = client
            .post(format!("{url}/rpc/v0"))
            .header("origin", ORIGIN)
            .body(body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_origin(response.headers());
        let body: serde_json::Value = response.json().await.unwrap();
        if error {
            assert_eq!(body["error"]["code"], -32700);
        } else {
            assert_eq!(body["result"], "ok");
        }
    }
    let cli = client
        .post(format!("{url}/rpc/v0"))
        .body(r#"{"jsonrpc":"2.0","id":2,"method":"call","params":{}}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(cli.status(), StatusCode::OK);
    assert!(!cli.headers().contains_key("access-control-allow-origin"));
    assert_eq!(
        cli.json::<serde_json::Value>().await.unwrap()["result"],
        "ok"
    );
    let contexts_before_resource = contexts.load(Ordering::SeqCst);
    assert!(contexts_before_resource > 0);
    let resource = client
        .get(format!("{url}/asset"))
        .header("origin", ORIGIN)
        .send()
        .await
        .unwrap();
    assert_eq!(resource.status(), StatusCode::OK);
    assert!(
        !resource
            .headers()
            .contains_key("access-control-allow-origin")
    );
    assert_eq!(contexts.load(Ordering::SeqCst), contexts_before_resource);
    assert_eq!(readers.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn registry_router_adds_cors_before_its_request_limit_and_authentication() {
    let (_temp, registry, _admin, _reader, _icon) = crate::registry::tests::fixture().await;
    let (url, _server) = crate::registry::tests::server(registry).await;
    let response = reqwest::Client::new()
        .post(url.join("rpc/v0").unwrap())
        .header("origin", ORIGIN)
        .header("x-start-auth-sig", "invalid-proof")
        .body(vec![b'x'; 1024 * 1024 + 1])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_origin(response.headers());
}
