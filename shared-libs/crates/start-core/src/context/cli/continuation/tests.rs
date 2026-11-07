use axum::response::IntoResponse;

use super::*;
use crate::context::config::ClientConfig;
use crate::util::future::NonDetachingJoinHandle;

#[tokio::test]
async fn noise_keeps_the_selected_identity_when_the_dial_host_is_pinned() {
    let (_temp, registry, admin, _, _) = crate::registry::tests::fixture().await;
    let app = axum::Router::new()
        .nest("/nested", crate::registry::registry_router(registry))
        .layer(axum::middleware::from_fn(
            |request: axum::extract::Request, next: axum::middleware::Next| async move {
                if request.uri().path().ends_with("/ws/rpc")
                    && !request
                        .uri()
                        .query()
                        .unwrap_or_default()
                        .contains("context=localhost")
                {
                    return http::StatusCode::FORBIDDEN.into_response();
                }
                next.run(request).await
            },
        ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dial: Url = format!("http://{}/nested/", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let mut selected = dial.clone();
    selected.set_host(Some("localhost")).unwrap();
    let ctx = CliContext::init(ClientConfig {
        host: Some(selected.as_str().parse().unwrap()),
        ..Default::default()
    })
    .unwrap();
    ctx.base_url.pinned.set(dial).unwrap();
    ctx.id_key.set(admin).unwrap();
    let _server: NonDetachingJoinHandle<()> =
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }).into();
    let dump = ctx
        .call_remote::<RpcContext>("db.dump", imbl_value::json!({}))
        .await
        .unwrap();
    assert!(
        dump["value"]["index"]["package"]["packages"]
            .get("secret")
            .is_some()
    );
}
