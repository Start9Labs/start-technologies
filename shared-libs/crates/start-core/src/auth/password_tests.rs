use std::sync::Arc;

use axum::Router;
use josekit::jwk::Jwk;
use rpc_toolkit::{HandlerExt, ParentHandler, Server, from_fn_async};

use super::PasswordType;
use crate::Error;
use crate::middleware::auth::session::SessionContext;
use crate::middleware::auth::session::client::SessionClient;
use crate::net::static_server::session_rpc_router;
use crate::registry::context::RegistryContext;
use crate::sign::{AnySigningKey, Signer};
use crate::util::crypto::EncryptedWire;
use crate::util::future::NonDetachingJoinHandle;

#[derive(serde::Deserialize)]
struct SecretParams {
    password: PasswordType,
}

async fn server() -> (String, Jwk, NonDetachingJoinHandle<()>, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let admin: Signer = AnySigningKey::generate().into();
    let context = crate::registry::context::test_context(
        directory.path().into(),
        admin.verifying_key().await.unwrap(),
    )
    .await;
    let secret = Jwk::generate_ec_key(josekit::jwk::alg::ec::EcCurve::P256).unwrap();
    let key = Arc::new(secret.clone());
    let api = ParentHandler::new().subcommand(
        "decode",
        from_fn_async(move |context: RegistryContext, params: SecretParams| {
            let key = key.clone();
            async move { params.password.decrypt_in_session(&context, key) }
        })
        .no_cli()
        .no_ts(),
    );
    let factory = context.clone();
    let rpc = Server::new(
        move || {
            let context = factory.clone();
            async move { Ok(context) }
        },
        api,
    )
    .for_http();
    let router: Router = session_rpc_router(context, rpc.clone(), axum::routing::any(rpc));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://localhost:{}", listener.local_addr().unwrap().port());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() }).into();
    (address, secret, task, directory)
}

#[tokio::test]
async fn unwrapped_passwords_require_actual_admission_but_legacy_jwe_and_auth_policy_remain() {
    let (address, key, _server, _directory) = server().await;
    let network = reqwest::Client::builder().http1_only().build().unwrap();
    let client = SessionClient::new(network.clone());
    let signer: Signer = AnySigningKey::generate().into();
    let url = format!("{address}/rpc/v2").parse().unwrap();
    let raw = "  keep whitespace\n";
    let result = client
        .call(
            &signer,
            url,
            "localhost",
            Default::default(),
            "decode",
            imbl_value::json!({"password":raw}),
        )
        .await
        .unwrap();
    assert_eq!(result.as_str(), Some(raw));
    let request = |password: serde_json::Value| {
        network.post(format!("{address}/rpc/v1")).json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"decode","params":{"password":password}}))
    };
    let rejected: serde_json::Value = request(serde_json::json!(raw))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        rejected["error"]["code"],
        crate::ErrorKind::InvalidRequest as i32
    );
    let wire = EncryptedWire::encrypt(raw, &key.to_public_key().unwrap()).unwrap();
    let accepted: serde_json::Value = request(serde_json::to_value(&wire).unwrap())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(accepted["result"], raw);
    let other = Jwk::generate_ec_key(josekit::jwk::alg::ec::EcCurve::P256).unwrap();
    let bad = EncryptedWire::encrypt(raw, &other.to_public_key().unwrap()).unwrap();
    let rejected: serde_json::Value = request(serde_json::to_value(bad).unwrap())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(rejected.get("error").is_some());
    assert_eq!(
        PasswordType::String(raw.into())
            .decrypt(Arc::new(key))
            .unwrap(),
        raw
    );
}

#[test]
fn password_debug_redacts_both_wire_shapes() {
    let key = Jwk::generate_ec_key(josekit::jwk::alg::ec::EcCurve::P256).unwrap();
    for password in [
        PasswordType::String("sensitive password".into()),
        PasswordType::EncryptedWire(
            EncryptedWire::encrypt("sensitive password", &key.to_public_key().unwrap()).unwrap(),
        ),
    ] {
        assert_eq!(format!("{password:?}"), "<REDACTED_PASSWORD>");
    }
}

#[derive(serde::Deserialize)]
struct CliSecretParams {
    password: Option<PasswordType>,
}

#[tokio::test]
#[ignore = "requires START_CLI_TEST_BINARY built from this tree"]
async fn actual_setup_cli_sends_secrets_only_in_admitted_rpc_without_fetching_a_jwe_key() {
    let directory = tempfile::tempdir().unwrap();
    let admin: Signer = AnySigningKey::generate().into();
    let context = crate::registry::context::test_context(
        directory.path().join("registry"),
        admin.verifying_key().await.unwrap(),
    )
    .await;
    let handler = from_fn_async(|context: RegistryContext, params: CliSecretParams| async move {
        assert!(context.secure_session().is_some());
        let result = params.password.map(|password| {
            assert!(matches!(password, PasswordType::String(ref value) if value == "fixture password"));
            true
        });
        Ok::<_, Error>(result)
    }).no_cli().no_ts();
    let api = ParentHandler::<RegistryContext>::new().subcommand(
        "setup",
        ParentHandler::<RegistryContext>::new()
            .subcommand("execute", handler.clone())
            .subcommand("attach", handler),
    );
    let factory = context.clone();
    let rpc = Server::new(
        move || {
            let context = factory.clone();
            async move { Ok(context) }
        },
        api,
    )
    .for_http();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://localhost:{}", listener.local_addr().unwrap().port());
    let _server: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        axum::serve(
            listener,
            session_rpc_router(context, rpc.clone(), axum::routing::any(rpc)),
        )
        .await
        .unwrap()
    })
    .into();
    let key = directory.path().join("malformed-key");
    tokio::fs::write(&key, "not a private key").await.unwrap();
    for (method, no_password, expected) in [
        ("execute", false, "true"),
        ("attach", false, "true"),
        ("attach", true, "null"),
    ] {
        let mut command =
            tokio::process::Command::new(std::env::var_os("START_CLI_TEST_BINARY").unwrap());
        command.args([
            "--host",
            &address,
            "--id-key-path",
            key.to_str().unwrap(),
            "setup",
            method,
            "--guid",
            "fixture_UNENC",
        ]);
        if no_password {
            command.arg("--no-password").env_remove("PASSWORD");
        } else {
            command.env("PASSWORD", "fixture password");
        }
        let output = command.kill_on_drop(true).output().await.unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stderr).contains("fixture password"));
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), expected);
    }
    assert_eq!(
        tokio::fs::read_to_string(key).await.unwrap(),
        "not a private key"
    );
}
