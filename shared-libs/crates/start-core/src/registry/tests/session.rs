use std::num::NonZeroUsize;
use std::time::Duration;

use axum::extract::ws::Message as LogicalMessage;
use axum::routing::get;

use super::*;
use crate::middleware::auth::session::{RpcSessionUpgrader, prologue};
use crate::sign::noise::NoiseIdentity;
use crate::util::net::secure::EncryptedSocket;
use crate::util::net::secure::tests::raw_socket;

async fn session_server(ctx: RegistryContext) -> (Url, NonDetachingJoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let server = registry_server(ctx.clone())
        .for_websocket()
        .with_upgrader(RpcSessionUpgrader);
    let app = registry_router(ctx).route("/secure", get(server));
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }).into();
    (url, task)
}

fn ws_url(url: &Url, path: &str) -> String {
    url.join(path)
        .unwrap()
        .to_string()
        .replacen("http:", "ws:", 1)
}

async fn encrypted(url: &str, signer: &Signer) -> EncryptedSocket {
    let identity = NoiseIdentity::new(signer).await.unwrap();
    EncryptedSocket::initiate(
        raw_socket(url).await,
        identity,
        &prologue("127.0.0.1"),
        NonZeroUsize::new(64 * 1024 * 1024).unwrap(),
    )
    .await
    .unwrap()
    .1
}

async fn receive(socket: &mut EncryptedSocket) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match socket.next().await.unwrap().unwrap() {
                LogicalMessage::Text(text) => return serde_json::from_str(&text).unwrap(),
                LogicalMessage::Ping(_) => socket.flush().await.unwrap(),
                other => panic!("unexpected message: {other:?}"),
            }
        }
    })
    .await
    .unwrap()
}

async fn call(socket: &mut EncryptedSocket, request: Value) -> Value {
    socket
        .send(LogicalMessage::Text(request.to_string().into()))
        .await
        .unwrap();
    receive(socket).await
}

fn secure_url(url: &Url) -> String {
    ws_url(url, "secure?noise=1&context=127.0.0.1")
}

async fn subscribe(socket: &mut EncryptedSocket) -> String {
    let result = call(socket, json!({"jsonrpc":"2.0","id":11,"method":"db.subscribe","params":{"pointer":"/index","osVersion":"0.4.1.3"}})).await;
    assert!(result.get("error").is_none(), "{result}");
    assert!(
        result["result"]["dump"]["value"]["package"]["packages"]
            .get("secret")
            .is_some()
    );
    result["result"]["guid"].as_str().unwrap().to_string()
}

async fn revoke_private_reader(ctx: &RegistryContext) {
    ctx.db
        .mutate(|db| {
            db.as_index_mut()
                .as_package_mut()
                .as_packages_mut()
                .as_idx_mut(&"secret".parse().unwrap())
                .unwrap()
                .as_access_mut()
                .as_readers_mut()
                .ser(&BTreeSet::new())
        })
        .await
        .result
        .unwrap();
}

#[tokio::test]
async fn encrypted_identity_reaches_live_auth_without_per_message_proofs() {
    let (_temp, ctx, admin, reader, _) = fixture().await;
    let (url, _task) = session_server(ctx.clone()).await;
    let mut admin_socket = encrypted(&secure_url(&url), &admin).await;
    for id in [u64::MAX, 2] {
        let reply = call(&mut admin_socket, json!({"jsonrpc":"2.0","id":id,"method":"package.access.get","params":{"id":"secret"},"signature":"invalid ignored legacy proof"})).await;
        assert_eq!(reply["id"], json!(id));
        assert_eq!(reply["result"]["private"], json!(true));
    }
    let mut reader_socket = encrypted(&secure_url(&url), &reader).await;
    let mut forged = signed_message(&admin, json!({"id":3,"method":"package.access.get","params":{"id":"secret","__Auth_signer":admin.verifying_key().await.unwrap()}})).await;
    forged["jsonrpc"] = "2.0".into();
    assert_eq!(
        call(&mut reader_socket, forged).await["error"]["code"],
        json!(ErrorKind::Authorization as i32)
    );
    assert!(ctx.secure_session().is_none());
    assert_eq!(
        rpc(&url, "package.access.get", json!({"id":"secret"}), None).await["error"]["code"],
        json!(ErrorKind::Authorization as i32)
    );
}

#[tokio::test]
async fn membership_and_package_access_remain_live_on_an_admitted_socket() {
    let (_temp, ctx, admin, reader, _) = fixture().await;
    let (url, _task) = session_server(ctx.clone()).await;
    let mut socket = encrypted(&secure_url(&url), &reader).await;
    let before = call(
        &mut socket,
        json!({"jsonrpc":"2.0","id":1,"method":"index","params":{}}),
    )
    .await;
    assert!(
        before["result"]["package"]["packages"]
            .get("secret")
            .is_some(),
        "{before}"
    );
    revoke_private_reader(&ctx).await;
    let after = call(
        &mut socket,
        json!({"jsonrpc":"2.0","id":2,"method":"index","params":{}}),
    )
    .await;
    assert!(
        after["result"]["package"]["packages"]
            .get("secret")
            .is_none(),
        "{after}"
    );
    let mut admin_socket = encrypted(&secure_url(&url), &admin).await;
    let request =
        json!({"jsonrpc":"2.0","id":9,"method":"package.access.get","params":{"id":"secret"}});
    assert_eq!(
        call(&mut admin_socket, request.clone()).await["result"]["private"],
        json!(true)
    );
    ctx.db
        .mutate(|db| db.as_admins_mut().ser(&BTreeSet::<Guid>::new()))
        .await
        .result
        .unwrap();
    assert_eq!(
        call(&mut admin_socket, request).await["error"]["code"],
        json!(ErrorKind::Authorization as i32)
    );
}

#[tokio::test]
async fn secure_origin_guid_rejects_plaintext_and_different_authenticated_peer() {
    let (_temp, ctx, _admin, reader, _) = fixture().await;
    let (url, _task) = session_server(ctx.clone()).await;
    let mut socket = encrypted(&secure_url(&url), &reader).await;
    let guid = subscribe(&mut socket).await;
    let (mut plain, _) = tokio_tungstenite::connect_async(ws_url(&url, &format!("ws/rpc/{guid}")))
        .await
        .unwrap();
    plain
        .send(Message::Text("not a Noise handshake".into()))
        .await
        .unwrap();
    assert!(!matches!(
        tokio::time::timeout(Duration::from_secs(3), plain.next())
            .await
            .unwrap(),
        Some(Ok(Message::Text(_)))
    ));
    let guid = subscribe(&mut socket).await;
    let mut other = encrypted(&ws_url(&url, &format!("ws/rpc/{guid}")), &signer(33)).await;
    let _ = other.send(LogicalMessage::Text("probe".into())).await;
    assert!(!matches!(
        tokio::time::timeout(Duration::from_secs(3), other.next())
            .await
            .unwrap(),
        Some(Ok(LogicalMessage::Text(_)))
    ));
    let guid = subscribe(&mut socket).await;
    let mut same = encrypted(&ws_url(&url, &format!("ws/rpc/{guid}")), &reader).await;
    revoke_private_reader(&ctx).await;
    let revision = receive(&mut same).await;
    assert!(revision.get("patch").is_some(), "{revision}");
}

#[tokio::test]
async fn legacy_created_guid_remains_plain_for_the_same_key_used_securely() {
    let (_temp, ctx, _admin, reader, _) = fixture().await;
    let (url, _task) = session_server(ctx.clone()).await;
    let _secure = encrypted(&secure_url(&url), &reader).await;
    let legacy = rpc(
        &url,
        "db.subscribe",
        json!({"pointer":"/index","osVersion":"0.4.1.3"}),
        Some(&reader),
    )
    .await;
    let guid = legacy["result"]["guid"].as_str().unwrap();
    let (mut plain, _) = tokio_tungstenite::connect_async(ws_url(&url, &format!("ws/rpc/{guid}")))
        .await
        .unwrap();
    revoke_private_reader(&ctx).await;
    let frame = tokio::time::timeout(Duration::from_secs(5), plain.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        serde_json::from_str::<Value>(frame.to_text().unwrap())
            .unwrap()
            .get("patch")
            .is_some()
    );
}

#[tokio::test]
async fn handshake_rejects_unconfigured_name_and_mismatched_transcript_context() {
    let (_temp, ctx, admin, _reader, _) = fixture().await;
    let (url, _task) = session_server(ctx).await;
    let mut raw = raw_socket(&ws_url(&url, "secure?noise=1&context=unconfigured.test")).await;
    assert!(!matches!(
        tokio::time::timeout(Duration::from_secs(3), raw.next())
            .await
            .unwrap(),
        Some(Ok(LogicalMessage::Text(_)))
    ));
    let identity = NoiseIdentity::new(&admin).await.unwrap();
    assert!(
        EncryptedSocket::initiate(
            raw_socket(&secure_url(&url)).await,
            identity,
            &prologue("wrong.test"),
            NonZeroUsize::new(1024).unwrap()
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn missing_crypto_opt_in_preserves_legacy_per_call_authentication() {
    let (_temp, ctx, admin, _reader, _) = fixture().await;
    let (url, _task) = session_server(ctx).await;
    let (mut socket, _) = tokio_tungstenite::connect_async(ws_url(&url, "secure"))
        .await
        .unwrap();
    let request =
        json!({"jsonrpc":"2.0","id":1,"method":"package.access.get","params":{"id":"secret"}});
    socket
        .send(Message::Text(request.clone().to_string().into()))
        .await
        .unwrap();
    let response: Value =
        serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(
        response["error"]["code"],
        json!(ErrorKind::Authorization as i32)
    );
    socket
        .send(Message::Text(
            signed_message(&admin, request).await.to_string().into(),
        ))
        .await
        .unwrap();
    let response: Value =
        serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(response["result"]["private"], json!(true));
}
