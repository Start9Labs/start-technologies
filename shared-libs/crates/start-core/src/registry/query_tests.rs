use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::test_utils::{context, socks_client};
use super::*;
use crate::middleware::auth::session::RpcSessionUpgrader;
use crate::sign::{AnySigningKey, Signer};
use crate::util::future::NonDetachingJoinHandle;

#[tokio::test]
async fn backend_registry_queries_keep_private_bulk_headers_and_live_auth_over_socks() {
    let temp = tempfile::tempdir().unwrap();
    let signer: Signer = AnySigningKey::generate().into();
    let ctx = context(
        temp.path().to_owned(),
        signer.verifying_key().await.unwrap(),
    )
    .await;
    let name = "x".repeat(17 * 1024 * 1024);
    ctx.db
        .mutate(|db| db.as_index_mut().as_name_mut().ser(&Some(name.clone())))
        .await
        .result
        .unwrap();
    let upgrades = Arc::new(AtomicUsize::new(0));
    let http_calls = Arc::new(AtomicUsize::new(0));
    let seen_upgrades = upgrades.clone();
    let seen_http = http_calls.clone();
    let app = crate::registry::registry_router(ctx.clone()).layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let seen_upgrades = seen_upgrades.clone();
            let seen_http = seen_http.clone();
            async move {
                if request.uri().path().ends_with(RpcSessionUpgrader::PATH) {
                    assert_eq!(
                        request.headers()[crate::registry::device_info::DEVICE_INFO_HEADER],
                        "os.version=0.4.2&os.compat=0.4.0.5&os.platform=test"
                    );
                    assert!(
                        !request
                            .headers()
                            .contains_key(crate::middleware::auth::signature::AUTH_SIG_HEADER)
                    );
                    seen_upgrades.fetch_add(1, Ordering::Relaxed);
                } else {
                    seen_http.fetch_add(1, Ordering::Relaxed);
                }
                next.run(request).await
            }
        },
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url: Url = format!(
        "http://localhost:{}/catalog/",
        listener.local_addr().unwrap().port()
    )
    .parse()
    .unwrap();
    let _server: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        axum::serve(listener, axum::Router::new().nest("/catalog", app))
            .await
            .unwrap();
    })
    .into();
    let (client, _proxy) = socks_client(&url).await;
    let headers = || {
        let mut headers = HeaderMap::new();
        headers.insert(
            crate::registry::device_info::DEVICE_INFO_HEADER,
            "os.version=0.4.2&os.compat=0.4.0.5&os.platform=test"
                .parse()
                .unwrap(),
        );
        headers
    };
    let dump = call(
        &client,
        &signer,
        &url,
        headers(),
        "db.dump",
        imbl_value::json!({"pointer":"/index"}),
    )
    .await
    .unwrap();
    assert_eq!(dump["value"]["name"].as_str(), Some(name.as_str()));
    ctx.db
        .mutate(|db| {
            db.as_admins_mut()
                .ser(&BTreeSet::<crate::rpc_continuations::Guid>::new())
        })
        .await
        .result
        .unwrap();
    assert_eq!(
        call(
            &client,
            &signer,
            &url,
            headers(),
            "db.dump",
            imbl_value::json!({"pointer":"/index"})
        )
        .await
        .unwrap_err()
        .code,
        34
    );
    assert_eq!(upgrades.load(Ordering::Relaxed), 2);
    assert_eq!(http_calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn backend_registry_queries_never_follow_upgrade_redirects_or_retry_http() {
    let http_calls = Arc::new(AtomicUsize::new(0));
    let redirect_calls = Arc::new(AtomicUsize::new(0));
    let seen_http = http_calls.clone();
    let seen_redirect = redirect_calls.clone();
    let app = axum::Router::new()
        .route(
            "/catalog/rpc/v2",
            axum::routing::get(|| async { axum::response::Redirect::temporary("/redirected") }),
        )
        .route(
            "/redirected",
            axum::routing::get(move || {
                let seen_redirect = seen_redirect.clone();
                async move {
                    seen_redirect.fetch_add(1, Ordering::Relaxed);
                    "wrong"
                }
            }),
        )
        .route(
            "/catalog/rpc/v0",
            axum::routing::post(move || {
                let seen_http = seen_http.clone();
                async move {
                    seen_http.fetch_add(1, Ordering::Relaxed);
                    axum::Json(serde_json::json!({"jsonrpc":"2.0","id":0,"result":null}))
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url: Url = format!(
        "http://localhost:{}/catalog",
        listener.local_addr().unwrap().port()
    )
    .parse()
    .unwrap();
    let _server: NonDetachingJoinHandle<()> =
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }).into();
    let (client, _proxy) = socks_client(&url).await;
    let signer: Signer = AnySigningKey::generate().into();
    assert!(
        call(
            &client,
            &signer,
            &url,
            HeaderMap::new(),
            "info",
            imbl_value::json!({})
        )
        .await
        .is_err()
    );
    assert_eq!(http_calls.load(Ordering::Relaxed), 0);
    assert_eq!(redirect_calls.load(Ordering::Relaxed), 0);
}
