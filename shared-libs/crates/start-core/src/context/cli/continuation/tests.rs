use axum::response::IntoResponse;

use super::*;
use crate::context::config::ClientConfig;
use crate::util::future::NonDetachingJoinHandle;

#[cfg(target_os = "linux")]
#[tokio::test]
async fn admission_keeps_the_selected_identity_when_the_dial_host_is_pinned() {
    let temp = tempfile::tempdir().unwrap();
    let admin: crate::sign::Signer = crate::sign::AnySigningKey::generate().into();
    let registry = crate::registry::context::test_context_for_hosts(
        temp.path().join("registry"),
        admin.verifying_key().await.unwrap(),
        vec!["selected.local".into()],
    )
    .await;
    let app = axum::Router::new()
        .nest("/nested", crate::registry::registry_router(registry))
        .layer(axum::middleware::from_fn(
            |request: axum::extract::Request, next: axum::middleware::Next| async move {
                if request.uri().path().ends_with("/ws/rpc")
                    && !request
                        .uri()
                        .query()
                        .unwrap_or_default()
                        .contains("context=selected.local")
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
    selected.set_host(Some("selected.local")).unwrap();
    let ctx = CliContext::init(ClientConfig {
        host: Some(selected.as_str().parse().unwrap()),
        registry: Some(selected.as_str().parse().unwrap()),
        id_key_path: Some(temp.path().join("id.pem")),
        ..Default::default()
    })
    .unwrap();
    ctx.base_url.pinned.set(dial.clone()).unwrap();
    ctx.registry_url
        .as_ref()
        .unwrap()
        .pinned
        .set(crate::registry::package::promote::registry_rpc_url(&dial).unwrap())
        .unwrap();
    ctx.id_key.set(admin).unwrap();
    let _server: NonDetachingJoinHandle<()> =
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }).into();
    let dump = ctx
        .call_remote::<RpcContext>("db.dump", imbl_value::json!({}))
        .await
        .unwrap();
    assert_eq!(
        dump["value"]["index"]["signers"].as_object().unwrap().len(),
        1
    );
    let directory = ctx.id_key_path.with_file_name("registry-identities");
    assert!(!directory.exists());
    let admins = ctx
        .call_remote::<crate::registry::context::RegistryContext>(
            "registry.admin.list",
            imbl_value::json!({}),
        )
        .await
        .unwrap();
    assert_eq!(admins.as_object().unwrap().len(), 1);
    assert_eq!(std::fs::read_dir(directory).unwrap().count(), 1);
}
