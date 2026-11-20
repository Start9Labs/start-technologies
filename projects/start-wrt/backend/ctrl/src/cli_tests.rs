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
        root_ca: Vec::new(),
    })
    .unwrap();
    Arc::get_mut(&mut ctx.0).unwrap().id_key_path = key_path;
    ctx
}

#[tokio::test]
async fn configured_root_ca_applies_to_http_and_websocket_clients() {
    use std::io::{Read, Write};

    use openssl::asn1::Asn1Time;
    use openssl::hash::MessageDigest;
    use openssl::pkey::PKey;
    use openssl::ssl::{SslAcceptor, SslMethod};
    use openssl::x509::extension::{BasicConstraints, SubjectAlternativeName};
    use openssl::x509::{X509NameBuilder, X509};

    let key = PKey::from_rsa(openssl::rsa::Rsa::generate(2048).unwrap()).unwrap();
    let mut name = X509NameBuilder::new().unwrap();
    name.append_entry_by_text("CN", "WRT CLI integration fixture")
        .unwrap();
    let name = name.build();
    let mut cert = X509::builder().unwrap();
    cert.set_version(2).unwrap();
    cert.set_subject_name(&name).unwrap();
    cert.set_issuer_name(&name).unwrap();
    cert.set_pubkey(&key).unwrap();
    cert.set_not_before(&Asn1Time::days_from_now(0).unwrap())
        .unwrap();
    cert.set_not_after(&Asn1Time::days_from_now(1).unwrap())
        .unwrap();
    cert.append_extension(BasicConstraints::new().critical().ca().build().unwrap())
        .unwrap();
    let san = SubjectAlternativeName::new()
        .ip("127.0.0.1")
        .build(&cert.x509v3_context(None, None))
        .unwrap();
    cert.append_extension(san).unwrap();
    cert.sign(&key, MessageDigest::sha256()).unwrap();
    let cert = cert.build();
    let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    acceptor.set_private_key(&key).unwrap();
    acceptor.set_certificate(&cert).unwrap();
    let acceptor = acceptor.build();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let host: Url = format!("https://{}/rpc/v1", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let server = tokio::task::spawn_blocking(move || {
        for _ in 0..4 {
            let (socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            socket
                .set_write_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            if let Ok(mut socket) = acceptor.accept(socket) {
                let mut request = [0; 4096];
                socket.read(&mut request).unwrap();
                socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK",
                    )
                    .unwrap();
            }
        }
    });
    let temp = tempfile::tempdir().unwrap();
    let untrusted = client(host.clone(), temp.path().join("identity.pem"));
    for client in [&untrusted.client, &untrusted.websocket_client] {
        assert!(client.get(host.clone()).send().await.is_err());
    }
    let ca = temp.path().join("ca.pem");
    std::fs::write(&ca, cert.to_pem().unwrap()).unwrap();
    let trusted = CliContext::init(CliArgs {
        config_root: temp.path().to_owned(),
        configs_only: true,
        host: Some(host.clone()),
        root_ca: vec![ca],
    })
    .unwrap();
    for client in [&trusted.client, &trusted.websocket_client] {
        assert_eq!(
            client
                .get(host.clone())
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            "OK"
        );
    }
    server.await.unwrap();
}

#[tokio::test]
async fn keyed_cli_uses_production_encrypted_admission_and_live_authorization() {
    let temp = tempfile::tempdir().unwrap();
    let (url, server, _task, http_calls, upgrades) = crate::rpc::tests::server().await;
    let ctx = client(
        url.join("rpc/v1").unwrap(),
        temp.path().join("identity.pem"),
    );
    let signer: Signer = AnySigningKey::generate().into();
    ctx.id_key.set(signer.clone()).unwrap();
    let key = crate::rpc::tests::enroll(&server, &signer).await;
    assert_eq!(upgrades.load(Ordering::Relaxed), 0);
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
    use startos::middleware::auth::signature::SignatureAuthorizationContext;
    server.unenroll([key]).await.unwrap();
    assert_eq!(
        ctx.call("auth.session.list", imbl_value::json!({}))
            .await
            .unwrap_err()
            .code,
        34
    );
    assert_eq!(http_calls.load(Ordering::Relaxed), 0);
    assert_eq!(upgrades.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn absent_identity_uses_unenrolled_admission_but_malformed_identity_never_downgrades() {
    let temp = tempfile::tempdir().unwrap();
    let (url, server, _task, http_calls, upgrades) = crate::rpc::tests::server().await;
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
    assert_eq!(http_calls.load(Ordering::Relaxed), 0);
    assert_eq!(upgrades.load(Ordering::Relaxed), 1);
    assert!(!path.exists());
    use startos::middleware::auth::DbContext;
    assert!(server
        .db()
        .peek()
        .await
        .de()
        .unwrap()
        .session_pubkeys
        .0
        .is_empty());
    std::fs::write(path, "not a private key").unwrap();
    assert!(ctx
        .call("auth.session.list", imbl_value::json!({}))
        .await
        .is_err());
    assert_eq!(http_calls.load(Ordering::Relaxed), 0);
    assert_eq!(upgrades.load(Ordering::Relaxed), 1);
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, "Bearer local-fixture".parse().unwrap());
    assert!(ctx.rpc_signer(&headers).unwrap().is_none());
}

#[tokio::test]
async fn refused_admission_never_retries_plaintext_rpc() {
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
    let (url, server, _task, http_calls, _) = crate::rpc::tests::server().await;
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
