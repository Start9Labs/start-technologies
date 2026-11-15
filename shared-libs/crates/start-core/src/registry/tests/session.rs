use std::num::NonZeroUsize;
use std::time::Duration;

use axum::extract::ws::Message as LogicalMessage;
use axum::routing::get;

use super::*;
use crate::middleware::auth::session::RpcSessionUpgrader;
use crate::sign::session::{PreparedClient, Target};
use crate::util::net::secure::EncryptedSocket;
use crate::util::net::secure::tests::raw_socket;

#[path = "session_admission.rs"]
mod admission;
#[path = "session_browser.rs"]
mod browser;

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

pub(super) fn rpc_ws_url(url: &Url) -> String {
    ws_url(url, RpcSessionUpgrader::PATH.trim_start_matches('/'))
}

pub(super) async fn encrypted(url: &str, signer: &Signer) -> EncryptedSocket {
    let path = Url::parse(url).unwrap();
    let target = path
        .path()
        .rsplit('/')
        .next()
        .and_then(crate::rpc_continuations::Guid::from)
        .map(Target::Continuation)
        .unwrap_or(Target::Rpc);
    let prepared = PreparedClient::new(signer, "127.0.0.1", target)
        .await
        .unwrap();
    EncryptedSocket::initiate(
        raw_socket(url).await,
        prepared,
        NonZeroUsize::new(64 * 1024 * 1024).unwrap(),
    )
    .await
    .unwrap()
    .1
}

pub(super) async fn receive(socket: &mut EncryptedSocket) -> Value {
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
    ws_url(url, "secure")
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
async fn unknown_key_admission_observes_enrollment_and_revocation_on_the_same_connection() {
    use crate::middleware::auth::signature::SignatureAuthorizationContext;
    let (_temp, ctx, _admin, _reader, _) = fixture().await;
    let (url, _task) = session_server(ctx.clone()).await;
    let key: Signer = crate::sign::AnySigningKey::generate().into();
    let public = key.verifying_key().await.unwrap();
    let mut socket = encrypted(&rpc_ws_url(&url), &key).await;
    let info = call(&mut socket, json!({"jsonrpc":"2.0","id":1,"method":"info"})).await;
    assert!(info.get("result").is_some(), "{info}");
    let request = json!({"jsonrpc":"2.0","id":2,"method":"admin.list"});
    let denied = call(&mut socket, request.clone()).await;
    assert_eq!(
        denied["error"]["code"],
        json!(ErrorKind::Authorization as i32)
    );
    let signer = crate::registry::admin::add_signer(
        ctx.clone(),
        crate::registry::admin::AddSignerParams {
            name: "Admitted unknown key".into(),
            contact: vec![],
            keys: vec![public.interned_der64().to_string()],
            gpg_exports: Default::default(),
        },
    )
    .await
    .unwrap();
    crate::registry::admin::add_admin(
        ctx.clone(),
        crate::registry::admin::AddAdminParams { signer },
    )
    .await
    .unwrap();
    let accepted = call(&mut socket, request.clone()).await;
    assert!(accepted.get("result").is_some(), "{accepted}");
    ctx.unenroll([public.interned_der64()]).await.unwrap();
    let denied = call(&mut socket, request).await;
    assert_eq!(
        denied["error"]["code"],
        json!(ErrorKind::Authorization as i32)
    );
    assert!(
        call(&mut socket, json!({"jsonrpc":"2.0","id":3,"method":"info"}))
            .await
            .get("result")
            .is_some()
    );
}

#[tokio::test]
async fn bare_rpc_rejects_plaintext_without_a_negotiation_flag() {
    let (_temp, ctx, _admin, _reader, _) = fixture().await;
    let (url, _task) = session_server(ctx).await;
    let mut socket = raw_socket(&rpc_ws_url(&url)).await;
    socket
        .send(LogicalMessage::Text(
            json!({"jsonrpc":"2.0","id":1,"method":"info"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert!(!matches!(
        tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap(),
        Some(Ok(LogicalMessage::Text(_)))
    ));
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
    let forged = json!({"jsonrpc":"2.0","id":3,"method":"package.access.get","params":{"id":"secret","__Auth_signer":admin.verifying_key().await.unwrap()},"signature":"untrusted"});
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
async fn non_object_subscription_params_are_rejected_without_losing_the_encrypted_session() {
    let (_temp, ctx, _admin, reader, _) = fixture().await;
    let (url, _task) = session_server(ctx).await;
    let mut socket = encrypted(&secure_url(&url), &reader).await;
    for (id, params) in [(10, json!([])), (11, json!(["value"]))] {
        let reply = call(
            &mut socket,
            json!({"jsonrpc":"2.0","id":id,"method":"db.subscribe","params":params}),
        )
        .await;
        assert_eq!(reply["id"], json!(id));
        assert_eq!(
            reply["error"]["code"],
            json!(rpc_toolkit::yajrc::INVALID_PARAMS_ERROR.code)
        );
    }
    let reply = call(
        &mut socket,
        json!({"jsonrpc":"2.0","id":12,"method":"db.subscribe"}),
    )
    .await;
    assert!(reply.get("error").is_none(), "{reply}");
    assert!(reply["result"]["guid"].is_string(), "{reply}");
    let reply = call(
        &mut socket,
        json!({"jsonrpc":"2.0","id":13,"method":"index","params":{}}),
    )
    .await;
    assert!(reply.get("error").is_none(), "{reply}");
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
        .send(Message::Text("not an encrypted handshake".into()))
        .await
        .unwrap();
    assert!(!matches!(
        tokio::time::timeout(Duration::from_secs(3), plain.next())
            .await
            .unwrap(),
        Some(Ok(Message::Text(_)))
    ));
    let guid = subscribe(&mut socket).await;
    let prepared = PreparedClient::new(
        &signer(33),
        "127.0.0.1",
        Target::Continuation(guid.parse().unwrap()),
    )
    .await
    .unwrap();
    assert!(
        EncryptedSocket::initiate(
            raw_socket(&ws_url(&url, &format!("ws/rpc/{guid}"))).await,
            prepared,
            NonZeroUsize::new(1024).unwrap(),
        )
        .await
        .is_err()
    );
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
async fn handshake_selects_verified_ingress_identity_not_a_query_hint() {
    let (_temp, ctx, admin, _reader, _) = fixture().await;
    let (url, _task) = session_server(ctx).await;
    let mut socket = encrypted(
        &ws_url(&url, "secure?secure=1&context=unconfigured.test"),
        &admin,
    )
    .await;
    assert!(
        call(
            &mut socket,
            json!({"jsonrpc":"2.0", "id":1, "method":"info"})
        )
        .await
        .get("result")
        .is_some()
    );
    let prepared = PreparedClient::new(&admin, "wrong.test", Target::Rpc)
        .await
        .unwrap();
    assert!(
        EncryptedSocket::initiate(
            raw_socket(&secure_url(&url)).await,
            prepared,
            NonZeroUsize::new(1024).unwrap()
        )
        .await
        .is_err()
    );
}
