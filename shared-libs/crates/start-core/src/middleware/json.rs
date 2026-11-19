use axum::extract::Request;
use axum::response::{IntoResponse, Response};
use http::header::{CONTENT_TYPE, ORIGIN};
use http::{HeaderMap, StatusCode};
use rpc_toolkit::{Empty, Middleware};

/// Restricts browser RPC to same-origin WebSockets or JSON HTTP bodies.
#[derive(Clone)]
pub struct RequireJson;

impl<Context: Send + Sync + 'static> Middleware<Context> for RequireJson {
    type Metadata = Empty;
    async fn process_http_request(
        &mut self,
        _: &Context,
        request: &mut Request,
    ) -> Result<(), Response> {
        if request
            .extensions()
            .get::<rpc_toolkit::WebSocketRequest>()
            .is_some()
        {
            return if same_origin(request) {
                Ok(())
            } else {
                Err(StatusCode::FORBIDDEN.into_response())
            };
        }
        if is_json(request.headers()) {
            Ok(())
        } else {
            Err(StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response())
        }
    }
}

fn same_origin(request: &Request) -> bool {
    let Some(origin) = request.headers().get(ORIGIN) else {
        return true;
    };
    let Some(authority) = crate::net::http::request_authority(request) else {
        return false;
    };
    let scheme = if request
        .extensions()
        .get::<crate::net::tls::TlsHandshakeInfo>()
        .is_some()
    {
        "https"
    } else {
        request.uri().scheme_str().unwrap_or("http")
    };
    let expected = url::Url::parse(&format!("{scheme}://{authority}"));
    let supplied = origin
        .to_str()
        .ok()
        .and_then(|value| url::Url::parse(value).ok());
    matches!((expected, supplied), (Ok(expected), Some(supplied)) if expected.origin() == supplied.origin())
}

fn is_json(headers: &HeaderMap) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use rpc_toolkit::{Context, HandlerExt, HttpServer, ParentHandler, Server, from_fn_async};

    use super::*;
    use crate::prelude::*;

    #[derive(Clone)]
    struct TestCtx;
    impl Context for TestCtx {}

    fn server() -> HttpServer<TestCtx> {
        Server::new(
            || async { Ok(TestCtx) },
            ParentHandler::new().subcommand(
                "ping",
                from_fn_async(|_: TestCtx| async { Ok::<_, Error>(String::from("pong")) })
                    .no_cli()
                    .no_ts(),
            ),
        )
        .middleware(RequireJson)
    }

    async fn call(content_type: Option<&str>) -> (StatusCode, Vec<u8>) {
        let mut request = http::Request::builder().method("POST").uri("/rpc/v1");
        if let Some(content_type) = content_type {
            request = request.header(CONTENT_TYPE, content_type);
        }
        let request = request
            .body(Body::from(
                r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#,
            ))
            .unwrap();
        let response = server().handle(request).await;
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, body.to_vec())
    }

    #[test]
    fn browser_origins_match_scheme_host_and_effective_port() {
        for (origin, allowed) in [
            ("http://server.local", true),
            ("http://server.local:80", true),
            ("https://server.local", false),
            ("http://server.local:8080", false),
            ("http://other.local", false),
            ("null", false),
        ] {
            let request = Request::builder()
                .uri("/rpc/v2")
                .header(http::header::HOST, "server.local")
                .header(ORIGIN, origin)
                .body(Body::empty())
                .unwrap();
            assert_eq!(same_origin(&request), allowed, "{origin}");
        }
        let mut request = Request::builder()
            .uri("/rpc/v2")
            .header(http::header::HOST, "server.local")
            .header(ORIGIN, "https://server.local")
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(crate::net::tls::TlsHandshakeInfo {
                sni: None,
                alpn: None,
            });
        assert!(same_origin(&request));
    }

    #[tokio::test]
    async fn simple_request_content_types_are_rejected_unprocessed() {
        for content_type in [
            None,
            Some("text/plain"),
            Some("text/plain;charset=UTF-8"),
            Some("application/x-www-form-urlencoded"),
            Some("multipart/form-data; boundary=x"),
        ] {
            let (status, body) = call(content_type).await;
            assert_eq!(
                status,
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "{content_type:?}"
            );
            assert!(body.is_empty(), "{content_type:?}");
        }
    }

    #[tokio::test]
    async fn json_requests_are_handled() {
        for content_type in ["application/json", "Application/JSON; charset=utf-8"] {
            let (status, body) = call(Some(content_type)).await;
            assert_eq!(status, StatusCode::OK);
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(body["result"], "pong", "{content_type}");
        }
    }
}
