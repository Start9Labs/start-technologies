use std::sync::Arc;

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
                assert!(
                    request.headers()[http::header::HOST]
                        .to_str()
                        .unwrap()
                        .starts_with("selected.local:")
                );
                if request
                    .uri()
                    .path()
                    .ends_with(crate::middleware::auth::session::RpcSessionUpgrader::PATH)
                    && request
                        .uri()
                        .query()
                        .unwrap_or_default()
                        .contains("context=")
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

#[tokio::test]
async fn startup_rpc_and_guid_keep_logical_authority_after_dial_pinning() {
    let dns = Arc::new(crate::net::dns::DnsController::new(
        crate::util::sync::Watch::new(Default::default()),
    ));
    let startup = InitContext::init(&crate::context::config::ServerConfig::default(), dns)
        .await
        .unwrap();
    let app = axum::Router::new().nest(
        "/nested",
        crate::net::static_server::ui_router(startup.clone()),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dial: Url = format!("http://{}/nested/", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let _server: NonDetachingJoinHandle<()> =
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }).into();
    let temp = tempfile::tempdir().unwrap();
    for name in ["bootstrap.local", "[::1]"] {
        let mut selected = dial.clone();
        selected.set_host(Some(name)).unwrap();
        let ctx = CliContext::init(ClientConfig {
            host: Some(selected.as_str().parse().unwrap()),
            id_key_path: Some(temp.path().join("unused-key")),
            id_key_gpg: Some("unavailable-startup-fixture".into()),
            ..Default::default()
        })
        .unwrap();
        ctx.base_url.pinned.set(dial.clone()).unwrap();
        assert_eq!(
            ctx.call_remote::<InitContext>("state", imbl_value::json!({}))
                .await
                .unwrap()
                .as_str(),
            Some("initializing")
        );
        let subscription: crate::init::InitProgressRes = imbl_value::from_value(
            ctx.call_remote::<InitContext>("init.subscribe", imbl_value::json!({}))
                .await
                .unwrap(),
        )
        .unwrap();
        let mut socket = ctx
            .ws_continuation::<InitContext>(subscription.guid.unwrap())
            .await
            .unwrap();
        let snapshot: crate::progress::FullProgress =
            crate::util::net::tungstenite::next_json(&mut socket)
                .await
                .unwrap();
        assert!(!snapshot.overall.is_complete());
        assert!(ctx.id_key.get().is_none());
        assert!(!temp.path().join("unused-key").exists());
    }
    startup.progress.complete();
}
