use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use ed25519_dalek::SigningKey;
use tower_service::Service;

use super::test_utils::context;
use crate::registry::registry_router;
use crate::sign::AnyVerifyingKey;

#[tokio::test]
async fn registry_root_redirect_uses_the_requested_origin() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(
        temp.path().join("registry"),
        AnyVerifyingKey::Ed25519(SigningKey::from_bytes(&[1; 32]).verifying_key()),
    )
    .await;
    let mut router = registry_router(ctx);
    for (uri, host, scheme, expected) in [
        (
            "/",
            Some("registry.example.com"),
            Some("https"),
            "https%3A%2F%2Fregistry.example.com%2F",
        ),
        (
            "/?ignored=value",
            Some("community.example.com:8443"),
            Some("https"),
            "https%3A%2F%2Fcommunity.example.com%3A8443%2F",
        ),
        (
            "/",
            Some("localhost:5959"),
            None,
            "http%3A%2F%2Flocalhost%3A5959%2F",
        ),
        (
            "/",
            Some("registry.onion"),
            Some("http"),
            "http%3A%2F%2Fregistry.onion%2F",
        ),
        (
            "/",
            Some("[::1]:5959"),
            None,
            "http%3A%2F%2F%5B%3A%3A1%5D%3A5959%2F",
        ),
        (
            "https://registry.example.com/",
            None,
            None,
            "https%3A%2F%2Fregistry.example.com%2F",
        ),
    ] {
        let mut request = Request::builder().uri(uri);
        if let Some(host) = host {
            request = request.header(header::HOST, host);
        }
        if let Some(scheme) = scheme {
            request = request.header("x-forwarded-proto", scheme);
        }
        let response = router
            .call(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            response.headers()[header::LOCATION],
            format!("https://marketplace.start9.com/?registry={expected}")
        );
    }

    for (method, path, expected) in [
        ("POST", "/", StatusCode::METHOD_NOT_ALLOWED),
        ("GET", "/missing", StatusCode::NOT_FOUND),
        ("GET", "/rpc/v0/info", StatusCode::OK),
        ("GET", "/icons/missing", StatusCode::BAD_REQUEST),
    ] {
        let response = router
            .call(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header(header::HOST, "registry.example.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "{method} {path}");
        assert!(!response.headers().contains_key(header::LOCATION));
    }
}

#[tokio::test]
async fn registry_root_redirect_rejects_invalid_origins() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(
        temp.path().join("registry"),
        AnyVerifyingKey::Ed25519(SigningKey::from_bytes(&[1; 32]).verifying_key()),
    )
    .await;
    let mut router = registry_router(ctx);
    for (host, scheme) in [
        (None, None),
        (Some("registry.example.com/other?registry=forged"), None),
        (Some("registry.example.com"), Some("javascript")),
    ] {
        let mut request = Request::builder().uri("/");
        if let Some(host) = host {
            request = request.header(header::HOST, host);
        }
        if let Some(scheme) = scheme {
            request = request.header("x-forwarded-proto", scheme);
        }
        let response = router
            .call(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(!response.headers().contains_key(header::LOCATION));
    }
}
