use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use rpc_toolkit::CallRemote;
use startos::sign::{AnySigningKey, Signer};
use startos::util::future::NonDetachingJoinHandle;

use super::*;

fn client(host: Url, key_path: PathBuf) -> CliContext {
    let mut ctx = CliContext::init(CliArgs {
        config_root: key_path.parent().unwrap().to_owned(),
        configs_only: true,
        host: Some(host),
    })
    .unwrap();
    Arc::get_mut(&mut ctx.0).unwrap().id_key_path = key_path;
    ctx
}

#[tokio::test]
async fn keyed_cli_uses_the_production_noise_route_and_live_authorization() {
    let temp = tempfile::tempdir().unwrap();
    let (url, server, _task, http_calls) = crate::rpc::tests::server().await;
    let ctx = client(
        url.join("rpc/v1").unwrap(),
        temp.path().join("identity.pem"),
    );
    let signer: Signer = AnySigningKey::generate().into();
    ctx.id_key.set(signer.clone()).unwrap();
    let key = crate::rpc::tests::enroll(&server, &signer).await;
    let response = <CliContext as CallRemote<ServerContext>>::call_remote(
        &ctx,
        "auth.session.list",
        Default::default(),
        imbl_value::json!({}),
        rpc_toolkit::Empty {},
    )
    .await
    .unwrap();
    assert_eq!(response["current"].as_str(), Some(&*key));
    use startos::middleware::auth::signature::SignatureAuthContext;
    server.unenroll([key]).await.unwrap();
    assert_eq!(
        ctx.call("auth.session.list", imbl_value::json!({}))
            .await
            .unwrap_err()
            .code,
        34
    );
    assert_eq!(http_calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn absent_identity_keeps_http_but_malformed_identity_never_downgrades() {
    let temp = tempfile::tempdir().unwrap();
    let (url, _, _task, http_calls) = crate::rpc::tests::server().await;
    let path = temp.path().join("identity.pem");
    let ctx = client(url.join("rpc/v1").unwrap(), path.clone());
    assert_eq!(
        ctx.signing_key().unwrap_err().kind,
        startos::ErrorKind::Uninitialized
    );
    assert_eq!(
        ctx.call("auth.session.list", imbl_value::json!({}))
            .await
            .unwrap_err()
            .code,
        34
    );
    assert_eq!(http_calls.load(Ordering::Relaxed), 1);
    std::fs::write(path, "not a private key").unwrap();
    assert!(ctx
        .call("auth.session.list", imbl_value::json!({}))
        .await
        .is_err());
    assert_eq!(http_calls.load(Ordering::Relaxed), 1);
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, "Bearer local-fixture".parse().unwrap());
    assert!(ctx.rpc_signer(&headers).unwrap().is_none());
}

#[tokio::test]
async fn refused_noise_never_retries_plaintext_rpc() {
    let temp = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = format!("http://{}/rpc/v1", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));
    let observed = attempts.clone();
    let app = axum::Router::new().route(
        "/rpc/v1",
        axum::routing::post(move || {
            let observed = observed.clone();
            async move {
                observed.fetch_add(1, Ordering::Relaxed);
                axum::Json(serde_json::json!({"jsonrpc":"2.0","id":0,"result":null}))
            }
        }),
    );
    let _task: NonDetachingJoinHandle<()> =
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }).into();
    let ctx = client(host, temp.path().join("identity.pem"));
    ctx.id_key.set(AnySigningKey::generate().into()).unwrap();
    assert!(ctx
        .call("auth.session.list", imbl_value::json!({}))
        .await
        .is_err());
    assert_eq!(attempts.load(Ordering::Relaxed), 0);
}

#[tokio::test]
#[ignore = "Requires STARTWRT_BIN pointing to the built CLI"]
async fn built_wrt_cli_uses_encrypted_admission_without_stdout_changes() {
    use ed25519_dalek::pkcs8::EncodePrivateKey;
    let binary = std::env::var("STARTWRT_BIN").expect("build startwrt and set STARTWRT_BIN");
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(".startwrt/id.key.pem");
    std::fs::create_dir(path.parent().unwrap()).unwrap();
    let key = ed25519_dalek::SigningKey::from_bytes(&[84; 32]);
    std::fs::write(
        &path,
        key.to_pkcs8_pem(Default::default()).unwrap().as_bytes(),
    )
    .unwrap();
    let signer: Signer =
        AnySigningKey::Ed25519(startos::developer::load_signing_key(path).unwrap()).into();
    let (url, server, _task, http_calls) = crate::rpc::tests::server().await;
    let key = crate::rpc::tests::enroll(&server, &signer).await;
    let output = tokio::process::Command::new(binary)
        .env("HOME", temp.path())
        .args([
            "startwrt-cli",
            "--host",
            url.join("rpc/v1").unwrap().as_str(),
            "auth",
            "session",
            "list",
            "--format",
            "json",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["current"].as_str(), Some(&*key));
    assert_eq!(http_calls.load(Ordering::Relaxed), 0);
}
