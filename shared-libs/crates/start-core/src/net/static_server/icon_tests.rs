use axum::body::to_bytes;
use imbl_value::json;

use super::*;
use crate::auth::{LoginContext, Session};
use crate::middleware::auth::DbContext;
use crate::registry::package::icon::store_icon;
use crate::sign::AnySigningKey;
use crate::util::DataUrl;
use crate::util::future::NonDetachingJoinHandle;

fn signer(seed: u8) -> Signer {
    Signer::Key(AnySigningKey::Ed25519(
        ed25519_dalek::SigningKey::from_bytes(&[seed; 32]),
    ))
}

#[test]
fn registry_icon_credentials_follow_saved_origins_and_exact_content_addressed_paths() {
    let temp = tempfile::tempdir().unwrap();
    let path = store_icon(
        temp.path(),
        &DataUrl::from_vec("image/png", b"icon".to_vec()),
    )
    .unwrap();
    let ui = from_value(json!({
        "https://REGISTRY.test/catalog/": "Registry", "not a URL": null
    }))
    .unwrap();
    assert!(is_registry_icon(
        &ui,
        &format!("https://registry.test/catalog/icons/{path}")
            .parse()
            .unwrap()
    ));
    for target in [
        format!("http://registry.test/catalog/icons/{path}"),
        format!("https://registry.test:444/catalog/icons/{path}"),
        format!("https://registry.test.evil/catalog/icons/{path}"),
        format!("https://user@registry.test/catalog/icons/{path}"),
        format!("https://registry.test/catalog/icons/{path}?extra=true"),
        format!("https://registry.test/catalog/icons/{path}#fragment"),
        format!("https://registry.test/other/icons/{path}"),
        format!("https://registry.test/catalog/icons-other/{path}"),
        format!("https://registry.test/catalog/icons/{path}/extra"),
        "https://registry.test/catalog/icons/../../rpc/v0".to_owned(),
        "https://registry.test/catalog/icons/blake3/invalid.png".to_owned(),
        format!("https://os-only.test/icons/{path}"),
    ] {
        assert!(!is_registry_icon(&ui, &target.parse().unwrap()), "{target}");
    }
    assert!(!is_registry_icon(
        &crate::trusted_keys::Registries::new(),
        &format!("https://registry.test/catalog/icons/{path}")
            .parse()
            .unwrap()
    ));
}

#[tokio::test]
async fn signed_proxy_fetches_private_dependency_icons_and_observes_revocation() {
    let (_temp, registry, _admin, reader, path) = crate::registry::tests::fixture().await;
    let (url, _server) = crate::registry::tests::server(registry.clone()).await;
    let target = url.join(&format!("icons/{path}")).unwrap();
    let ui = from_value(json!({url.as_str():"Registry"})).unwrap();
    assert!(is_registry_icon(&ui, &target));
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let unsigned = registry_icon_response(client.clone(), target.clone(), None)
        .await
        .unwrap();
    assert_eq!(unsigned.status(), StatusCode::NOT_FOUND);
    let response = registry_icon_response(client.clone(), target.clone(), Some(&reader))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[CONTENT_TYPE], "image/png");
    assert_eq!(response.headers()[CACHE_CONTROL], "private, no-store");
    assert_eq!(
        to_bytes(response.into_body(), 1024).await.unwrap(),
        b"secret".as_slice()
    );
    registry
        .db
        .mutate(|db| {
            db.as_index_mut()
                .as_package_mut()
                .as_packages_mut()
                .as_idx_mut(&"secret".parse().unwrap())
                .unwrap()
                .as_access_mut()
                .as_readers_mut()
                .ser(&std::collections::BTreeSet::new())
        })
        .await
        .result
        .unwrap();
    let response = registry_icon_response(client, target, Some(&reader))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn signed_icon_redirects_are_not_forwarded_and_public_images_keep_redirects() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let requests = Arc::new(AtomicUsize::new(0));
    let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_url = format!("http://{}/target", target.local_addr().unwrap());
    let target_server: NonDetachingJoinHandle<()> = {
        let requests = requests.clone();
        tokio::spawn(async move {
            axum::serve(
                target,
                Router::new().route(
                    "/target",
                    get(move |headers: http::HeaderMap| {
                        assert!(!headers.contains_key(AUTH_SIG_HEADER));
                        requests.fetch_add(1, Ordering::SeqCst);
                        async { ([(CONTENT_TYPE, "image/png")], "public") }
                    }),
                ),
            )
            .await
            .unwrap();
        })
        .into()
    };
    let origin = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let registry = format!("http://{}/", origin.local_addr().unwrap());
    let origin_url: Url = format!("{registry}icons/blake3/{}.png", "a".repeat(52))
        .parse()
        .unwrap();
    let public_url: Url = format!("{registry}public-image").parse().unwrap();
    let ui = from_value(json!({registry:"Registry"})).unwrap();
    assert!(is_registry_icon(&ui, &origin_url));
    assert!(!is_registry_icon(&ui, &public_url));
    let origin_server: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        let redirect = move || async move { axum::response::Redirect::temporary(&target_url) };
        axum::serve(
            origin,
            Router::new()
                .route("/icons/{*path}", get(redirect.clone()))
                .route("/public-image", get(redirect)),
        )
        .await
        .unwrap();
    })
    .into();
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = registry_icon_response(client, origin_url.clone(), Some(&signer(17)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert!(!response.headers().contains_key(http::header::LOCATION));
    assert_eq!(requests.load(Ordering::SeqCst), 0);
    let response = registry_icon_response(reqwest::Client::new(), public_url, None)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(response.into_body(), 1024).await.unwrap(),
        "public"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    drop((origin_server, target_server));
}

#[tokio::test]
async fn local_resource_gate_requires_a_live_enrolled_signature_before_proxying() {
    let ctx = crate::middleware::auth::bearer::tests::context().await;
    let key = signer(18);
    let public = key.verifying_key().await.unwrap();
    let id = public.interned_der64();
    let request = |proof: Option<HeaderValue>| {
        let mut request = Request::builder().uri("/registry/icons/target");
        if let Some(proof) = proof {
            request = request.header(AUTH_SIG_HEADER, proof);
        }
        request.body(Body::empty()).unwrap()
    };
    let proof = || async {
        SignatureHeader::sign(&key, b"", "localhost")
            .await
            .unwrap()
            .to_header()
    };
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let fetch = |_| async {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(Response::new(Body::empty()))
    };
    assert_eq!(
        if_authorized(&ctx, request(None), fetch)
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        if_authorized(&ctx, request(Some(proof().await)), fetch)
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    ctx.db()
        .mutate(|db| {
            <crate::middleware::auth::bearer::tests::TestCtx as LoginContext>::access_auth_keys(db)
                .insert(
                    &id,
                    &Session {
                        name: None,
                        logged_in: chrono::Utc::now(),
                        last_active: chrono::Utc::now(),
                        user_agent: None,
                    },
                )
        })
        .await
        .result
        .unwrap();
    let valid = proof().await;
    assert_eq!(
        if_authorized(&ctx, request(Some(valid.clone())), fetch)
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        if_authorized(&ctx, request(Some(valid)), fetch)
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    ctx.unenroll([id]).await.unwrap();
    assert_eq!(
        if_authorized(&ctx, request(Some(proof().await)), fetch)
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}
