mod session;
mod versioned;

use std::collections::BTreeMap;

use futures::{SinkExt, StreamExt};
use imbl_value::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use url::Url;

use super::*;
use crate::middleware::auth::signature::{AUTH_SIG_HEADER, SignatureHeader};
use crate::registry::asset::RegistryAsset;
use crate::registry::context::test_context;
use crate::registry::package::icon::{icons_dir, store_icon};
use crate::registry::reader::ReaderKey;
use crate::s9pk::merkle_archive::source::encrypted::KeyStreamInfo;
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::sign::{AnySigningKey, Signer};
use crate::util::future::NonDetachingJoinHandle;

fn signer(seed: u8) -> Signer {
    Signer::Key(AnySigningKey::Ed25519(
        ed25519_dalek::SigningKey::from_bytes(&[seed; 32]),
    ))
}

/// A registry with a public package and a package whose only s9pk is private to the reader key.
pub(crate) async fn fixture() -> (tempfile::TempDir, RegistryContext, Signer, Signer, String) {
    let temp = tempfile::tempdir().unwrap();
    let admin = signer(31);
    let reader = signer(32);
    let ctx = test_context(temp.path().into(), admin.verifying_key().await.unwrap()).await;
    let reader_key = reader.verifying_key().await.unwrap();
    let path = store_icon(
        &icons_dir(temp.path()),
        &DataUrl::from_vec("image/png", b"secret".to_vec()),
    )
    .unwrap();
    let (_, info) = crate::registry::package::index::tests::version_info_signed_by(1);
    let mut version = to_value(&info).unwrap();
    version["title"] = json!("Secret");
    version["sourceVersion"] = json!("*");
    version["dependencyMetadata"] = json!({
        "dependency": {"title": null, "description": null, "optional": false, "icon": path}
    });
    version["s9pks"][0][1]["private"] = to_value(&secret_keystream()).unwrap();
    let reader_id = ctx
        .db
        .mutate(|db| {
            db.as_index_mut().as_readers_mut().add_reader(&ReaderKey {
                label: "Reader".into(),
                key: reader_key.clone(),
            })
        })
        .await
        .result
        .unwrap();
    version["s9pks"][0][1]["readers"] = json!([reader_id]);
    ctx.db
        .mutate(|db| {
            db.as_index_mut().as_package_mut().ser(&from_value(json!({
                "categories": {},
                "packages": {
                    "public": { "authorized": {}, "categories": [], "versions": {} },
                    "secret": {
                        "authorized": {}, "categories": ["hidden"],
                        "versions": { "1.0.0:0": version }
                    }
                }
            }))?)
        })
        .await
        .result
        .unwrap();
    (temp, ctx, admin, reader, path.to_string())
}

pub(crate) fn secret_keystream() -> KeyStreamInfo {
    KeyStreamInfo {
        key: crate::util::serde::Base64([3; 32]),
        iv: crate::util::serde::Base64([4; 16]),
    }
}

async fn mutate_secret_s9pk(
    ctx: &RegistryContext,
    f: impl FnOnce(&mut RegistryAsset<MerkleArchiveCommitment>) + std::panic::UnwindSafe + Send,
) {
    ctx.db
        .mutate(|db| {
            db.as_index_mut()
                .as_package_mut()
                .as_packages_mut()
                .as_idx_mut(&"secret".parse().unwrap())
                .unwrap()
                .as_versions_mut()
                .as_idx_mut(&"1.0.0:0".parse().unwrap())
                .unwrap()
                .as_s9pks_mut()
                .mutate(|s9pks| Ok(f(&mut s9pks[0].1)))
        })
        .await
        .result
        .unwrap();
}

pub(crate) async fn secret_readers(ctx: &RegistryContext) -> BTreeSet<Guid> {
    ctx.db
        .peek()
        .await
        .into_index()
        .into_package()
        .de()
        .unwrap()
        .packages["secret"]
        .versions[&"1.0.0:0".parse().unwrap()]
        .s9pks[0]
        .1
        .readers
        .clone()
}

pub(crate) async fn set_secret_readers(ctx: &RegistryContext, readers: &BTreeSet<Guid>) {
    let readers = readers.clone();
    mutate_secret_s9pk(ctx, move |asset| asset.readers = readers).await
}

/// Removes the reader from the fixture's private s9pk.
pub(crate) async fn revoke_secret_reader(ctx: &RegistryContext) {
    set_secret_readers(ctx, &BTreeSet::new()).await
}

/// Makes the fixture's private s9pk public.
pub(crate) async fn publish_secret(ctx: &RegistryContext) {
    mutate_secret_s9pk(ctx, |asset| asset.private = None).await
}

pub(crate) async fn server(ctx: RegistryContext) -> (Url, NonDetachingJoinHandle<()>) {
    serve(registry_router(ctx)).await
}

pub(crate) async fn serve(router: Router) -> (Url, NonDetachingJoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    })
    .into();
    (url, task)
}

pub(crate) async fn rpc(url: &Url, method: &str, params: Value, signer: Option<&Signer>) -> Value {
    let body =
        serde_json::to_vec(&json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
            .unwrap();
    let mut request = reqwest::Client::new()
        .post(url.join("rpc/v0").unwrap())
        .header(http::header::CONTENT_TYPE, "application/json");
    if let Some(signer) = signer {
        request = request.header(
            AUTH_SIG_HEADER,
            SignatureHeader::sign(signer, &body, "127.0.0.1")
                .await
                .unwrap()
                .to_header(),
        );
    }
    request
        .body(body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

async fn registry_socket(url: &Url, signer: &Signer) -> crate::util::net::secure::EncryptedSocket {
    session::encrypted(&session::rpc_ws_url(url), signer).await
}

async fn post_json_message(url: &Url, message: &Value) -> Value {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap()
        .post(url.join("rpc/v0").unwrap())
        .json(message)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn root_signature_fields_neither_authenticate_nor_override_http_headers() {
    let (_temp, ctx, admin, _reader, _) = fixture().await;
    let (url, _task) = server(ctx).await;
    let message =
        json!({"id":u64::MAX,"method":"admin.reader.list","params":{},"signature":"untrusted"});
    let response = post_json_message(&url, &message).await;
    assert_eq!(response["id"], json!(u64::MAX));
    assert_eq!(
        response["error"]["code"],
        json!(ErrorKind::Authorization as i32)
    );
    let bytes = serde_json::to_vec(&message).unwrap();
    let legacy = SignatureHeader::sign(&admin, &bytes, "127.0.0.1")
        .await
        .unwrap()
        .to_header();
    let response: Value = reqwest::Client::new()
        .post(url.join("rpc/v0").unwrap())
        .header(AUTH_SIG_HEADER, legacy)
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(bytes)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(response["id"], json!(u64::MAX));
    assert_eq!(
        response["result"].as_object().unwrap().len(),
        1,
        "{response}"
    );
}

#[tokio::test]
async fn websocket_upgrade_headers_and_root_fields_cannot_override_an_unknown_encrypted_peer() {
    use axum::extract::ws::Message as LogicalMessage;
    let (_temp, ctx, admin, _reader, _) = fixture().await;
    let (url, _task) = server(ctx).await;
    let mut request = session::rpc_ws_url(&url).into_client_request().unwrap();
    request.headers_mut().insert(
        AUTH_SIG_HEADER,
        SignatureHeader::sign(&admin, b"", "127.0.0.1")
            .await
            .unwrap()
            .to_header(),
    );
    let (socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let key: Signer = crate::sign::AnySigningKey::generate().into();
    let prepared = crate::sign::session::PreparedClient::new(
        &key,
        "127.0.0.1",
        crate::sign::session::Target::Rpc,
    )
    .await
    .unwrap();
    let (_, mut socket) = crate::util::net::secure::EncryptedSocket::initiate(
        crate::util::net::tungstenite::transport(socket),
        prepared,
        std::num::NonZeroUsize::new(64 * 1024 * 1024).unwrap(),
    )
    .await
    .unwrap();
    socket
        .send(LogicalMessage::Text(
            json!({"jsonrpc":"2.0","id":1,"method":"admin.reader.list","params":{}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let response: Value = serde_json::from_str(
        tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .to_text()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        response["error"]["code"],
        json!(ErrorKind::Authorization as i32)
    );
    let batch = json!([
        json!({"jsonrpc":"2.0","id":2,"method":"index","signature":"untrusted"}),
        json!({"jsonrpc":"2.0","id":3,"method":"index"}),
    ]);
    socket
        .send(LogicalMessage::Text(batch.to_string().into()))
        .await
        .unwrap();
    let response: Value = serde_json::from_str(
        tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .to_text()
            .unwrap(),
    )
    .unwrap();
    let responses = response.as_array().unwrap();
    let signed = responses
        .iter()
        .find(|response| response["id"] == json!(2))
        .unwrap();
    let unsigned = responses
        .iter()
        .find(|response| response["id"] == json!(3))
        .unwrap();
    assert!(
        signed["result"]["package"]["packages"]
            .get("secret")
            .is_none(),
        "{signed}"
    );
    assert!(
        unsigned["result"]["package"]["packages"]
            .get("secret")
            .is_none(),
        "{unsigned}"
    );
    futures::SinkExt::close(&mut socket).await.unwrap();
}

#[tokio::test]
async fn legacy_signature_body_limits_apply_before_reads_and_honor_transport_configuration() {
    use std::num::NonZeroUsize;

    use crate::middleware::auth::signature::verify_request_signature;

    let temp = tempfile::tempdir().unwrap();
    let key = signer(73);
    let expected = key.verifying_key().await.unwrap();
    let ctx = test_context(temp.path().into(), expected.clone()).await;
    let payload = vec![b'x'; rpc_toolkit::RequestBodyLimit::default().0.get() + 1];
    let signature = SignatureHeader::sign(&key, &payload, "127.0.0.1")
        .await
        .unwrap()
        .to_header();
    let body = axum::body::Body::from_stream(futures::stream::poll_fn(
        |_| -> std::task::Poll<Option<Result<axum::body::Bytes, std::io::Error>>> {
            panic!("oversized declared bodies must be rejected before polling")
        },
    ));
    let mut request = http::Request::builder()
        .header(AUTH_SIG_HEADER, signature.clone())
        .body(body)
        .unwrap();
    let error = verify_request_signature(&ctx, &mut request)
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidRequest);

    let mut request = http::Request::builder()
        .header(AUTH_SIG_HEADER, signature)
        .body(axum::body::Body::from(payload.clone()))
        .unwrap();
    request
        .extensions_mut()
        .insert(rpc_toolkit::RequestBodyLimit(
            NonZeroUsize::new(payload.len()).unwrap(),
        ));
    assert_eq!(
        verify_request_signature(&ctx, &mut request).await.unwrap(),
        expected
    );
    assert_eq!(
        axum::body::to_bytes(request.into_body(), payload.len())
            .await
            .unwrap(),
        payload
    );
}

#[tokio::test]
async fn browsing_and_access_commands_use_verified_visibility() {
    let (_temp, ctx, admin, reader, _) = fixture().await;
    let (url, _task) = server(ctx.clone()).await;
    let forged = json!({"__Auth_signer": admin.verifying_key().await.unwrap()});
    let stranger = signer(33);
    let s9pk = |package: &Value| package["versions"]["1.0.0:0"]["s9pks"][0][1].clone();
    for (key, visible, is_admin) in [
        (None, false, false),
        (Some(&stranger), false, false),
        (Some(&reader), true, false),
        (Some(&admin), true, true),
    ] {
        let index = rpc(&url, "index", forged.clone(), key).await;
        assert!(index.get("error").is_none(), "{index}");
        assert_eq!(
            index["result"]["readers"].as_object().unwrap().len(),
            usize::from(is_admin),
            "{index}"
        );
        let secret = &index["result"]["package"]["packages"]["secret"];
        assert_eq!(!secret.is_null(), visible);
        if visible {
            assert_eq!(
                s9pk(secret)["private"],
                to_value(&secret_keystream()).unwrap()
            );
            assert_eq!(
                s9pk(secret)["readers"].as_array().map_or(0, |r| r.len()),
                usize::from(is_admin)
            );
        }
        let index = rpc(&url, "package.index", forged.clone(), key).await;
        let secret = &index["result"]["packages"]["secret"];
        assert_eq!(!secret.is_null(), visible, "{index}");
        if visible {
            assert_eq!(
                s9pk(secret)["readers"].as_array().map_or(0, |r| r.len()),
                usize::from(is_admin)
            );
        }
        let packages = rpc(&url, "package.get", forged.clone(), key).await;
        assert_eq!(
            packages["result"].get("secret").is_some(),
            visible,
            "{packages}"
        );
        let package = rpc(&url, "package.get", json!({"id": "secret"}), key).await;
        assert_eq!(
            package["result"]["best"].as_object().unwrap().is_empty(),
            !visible,
            "{package}"
        );
        if !visible {
            assert_eq!(package["result"]["categories"], json!([]));
        }
        let signers = rpc(&url, "package.signer.list", json!({"id": "secret"}), key).await;
        assert_eq!(signers.get("error").is_some(), !visible, "{signers}");
    }
    assert!(!ctx.datadir.join("admin.log").exists());
    let stranger_key = stranger.verifying_key().await.unwrap();
    let add = json!({"label": "Stranger", "key": stranger_key});
    for method in ["admin.reader.list", "admin.reader.add"] {
        let denied = rpc(&url, method, add.clone(), Some(&reader)).await;
        assert_eq!(
            denied["error"]["code"],
            json!(ErrorKind::Authorization as i32),
            "{denied}"
        );
    }
    let added = rpc(&url, "admin.reader.add", add.clone(), Some(&admin)).await;
    let stranger_id: Guid = from_value(added["result"].clone()).unwrap();
    let duplicate = rpc(&url, "admin.reader.add", add, Some(&admin)).await;
    assert!(duplicate.get("error").is_some(), "{duplicate}");
    let edited = rpc(
        &url,
        "admin.reader.edit",
        json!({"id": stranger_id, "label": "Friend"}),
        Some(&admin),
    )
    .await;
    assert!(edited.get("error").is_none(), "{edited}");
    let listed = rpc(&url, "admin.reader.list", json!({}), Some(&admin)).await;
    assert_eq!(
        listed["result"][stranger_id.as_ref()]["label"],
        json!("Friend")
    );
    let reader_id = ctx
        .db
        .peek()
        .await
        .as_index()
        .as_readers()
        .ids_for(&reader.verifying_key().await.unwrap())
        .unwrap()
        .pop_first()
        .unwrap();
    for id in [&stranger_id, &reader_id] {
        let removed = rpc(&url, "admin.reader.remove", json!({"id": id}), Some(&admin)).await;
        assert!(removed.get("error").is_none(), "{removed}");
    }
    let db = ctx.db.peek().await;
    assert!(db.as_index().as_readers().de().unwrap().is_empty());
    let secret = db
        .into_index()
        .into_package()
        .de()
        .unwrap()
        .packages
        .remove("secret")
        .unwrap();
    assert!(
        secret.versions[&"1.0.0:0".parse().unwrap()].s9pks[0]
            .1
            .readers
            .is_empty()
    );
    assert!(
        secret.versions[&"1.0.0:0".parse().unwrap()].s9pks[0]
            .1
            .private
            .is_some()
    );
    let package = rpc(&url, "package.get", json!({"id": "secret"}), Some(&reader)).await;
    assert!(
        package["result"]["best"].as_object().unwrap().is_empty(),
        "{package}"
    );
    assert!(ctx.datadir.join("admin.log").exists());
}

#[tokio::test]
async fn legacy_subscriptions_filter_initial_dumps_and_live_revisions() {
    let (_temp, ctx, _admin, reader, _) = fixture().await;
    let (url, _task) = server(ctx.clone()).await;
    let public = rpc(&url, "db.subscribe", json!({}), None).await;
    assert!(
        public["result"]["dump"]["value"]["package"]["packages"]
            .get("secret")
            .is_none(),
        "{public}"
    );
    let public_guid: Guid = from_value(public["result"]["guid"].clone()).unwrap();
    drop(ctx.rpc_continuations.get_ws_handler(&public_guid).await);
    let private = rpc(&url, "db.subscribe", json!({}), Some(&reader)).await;
    let mut view = private["result"]["dump"]["value"].clone();
    assert!(
        view["package"]["packages"].get("secret").is_some(),
        "{private}"
    );
    assert!(
        view["package"]["packages"]["secret"]["versions"]["1.0.0:0"]["s9pks"][0][1]
            .get("readers")
            .is_none()
    );
    let guid: Guid = from_value(private["result"]["guid"].clone()).unwrap();
    let websocket = url
        .join(&format!("ws/rpc/{guid}"))
        .unwrap()
        .to_string()
        .replacen("http:", "ws:", 1);
    let (mut socket, _) = tokio_tungstenite::connect_async(websocket).await.unwrap();
    revoke_secret_reader(&ctx).await;
    let message = tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let revision: patch_db::Revision = serde_json::from_str(message.to_text().unwrap()).unwrap();
    patch_db::json_patch::patch(&mut view, &revision.patch).unwrap();
    assert!(view["package"]["packages"].get("secret").is_none());
    socket.close(None).await.unwrap();
}

#[tokio::test]
async fn private_dependency_icons_require_a_current_reader_and_disable_caching() {
    let (_temp, ctx, _admin, reader, path) = fixture().await;
    let (url, _task) = server(ctx.clone()).await;
    let icon = url.join(&format!("icons/{path}")).unwrap();
    let client = reqwest::Client::new();
    assert_eq!(client.get(icon.clone()).send().await.unwrap().status(), 404);
    let signed = || async {
        SignatureHeader::sign(&reader, b"", "127.0.0.1")
            .await
            .unwrap()
            .to_header()
    };
    let response = client
        .get(icon.clone())
        .header(AUTH_SIG_HEADER, signed().await)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()[http::header::CACHE_CONTROL],
        "private, no-store"
    );
    assert_eq!(response.bytes().await.unwrap(), &b"secret"[..]);
    revoke_secret_reader(&ctx).await;
    assert_eq!(
        client
            .get(icon.clone())
            .header(AUTH_SIG_HEADER, signed().await)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    publish_secret(&ctx).await;
    let response = client.get(icon).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()[http::header::CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );
}

#[tokio::test]
async fn rejected_icon_proofs_fall_back_to_anonymous_access() {
    let (_temp, ctx, _admin, reader, path) = fixture().await;
    let (url, _task) = server(ctx.clone()).await;
    let icon = url.join(&format!("icons/{path}")).unwrap();
    let client = reqwest::Client::new();
    let wrong_identity = || async {
        SignatureHeader::sign(&reader, b"", "other.test")
            .await
            .unwrap()
            .to_header()
    };
    let response = client
        .get(icon.clone())
        .header(AUTH_SIG_HEADER, wrong_identity().await)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    publish_secret(&ctx).await;
    let response = client
        .get(icon)
        .header(AUTH_SIG_HEADER, wrong_identity().await)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn watches_multiplex_with_ordinary_calls_and_revoke_private_packages() {
    use axum::extract::ws::Message;
    let (_temp, ctx, _admin, reader, _) = fixture().await;
    let (url, _task) = server(ctx.clone()).await;
    let mut socket = registry_socket(&url, &reader).await;
    socket.send(Message::Text(json!({"jsonrpc":"2.0", "id":40, "method":"db.subscribe", "params":{"osVersion":"0.4.1.3"}}).to_string().into())).await.unwrap();
    let legacy = session::receive(&mut socket).await;
    assert_eq!(legacy["id"], json!(40));
    assert!(
        legacy["result"]["dump"]["value"]["package"]["packages"]
            .get("secret")
            .is_some(),
        "{legacy}"
    );
    assert!(legacy["result"].get("value").is_none());
    let guid: Guid = from_value(legacy["result"]["guid"].clone()).unwrap();
    drop(ctx.rpc_continuations.get_ws_handler(&guid).await);
    for (id, pointer) in [(41, "/index"), (42, "/index/os")] {
        socket.send(Message::Text(json!({"jsonrpc":"2.0", "rpcstream":"1.0", "id":id, "method":"db.subscribe", "params":{"pointer":pointer, "osVersion":"0.4.1.3"}}).to_string().into())).await.unwrap();
    }
    let mut dumps = BTreeMap::new();
    for _ in 0..2 {
        let response = session::receive(&mut socket).await;
        assert!(response.get("error").is_none(), "{response}");
        dumps.insert(
            response["id"].as_u64().unwrap(),
            response["result"]["value"].clone(),
        );
    }
    let mut view = dumps.remove(&41).unwrap();
    assert!(view["package"]["packages"].get("secret").is_some());
    assert!(
        view["package"]["packages"]["secret"]["versions"]["1.0.0:0"]["s9pks"][0][1]
            .get("readers")
            .is_none()
    );
    assert_eq!(dumps[&42], json!({"versions": {}}));
    revoke_secret_reader(&ctx).await;
    socket
        .send(Message::Text(
            json!({"jsonrpc":"2.0", "id":43, "method":"index", "params":{}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let mut saw_revision = false;
    let mut saw_reply = false;
    while !saw_revision || !saw_reply {
        let response = session::receive(&mut socket).await;
        match response["id"].as_u64().unwrap() {
            41 => {
                assert_eq!(response["end"], json!(false));
                let revision: patch_db::Revision = from_value(response["result"].clone()).unwrap();
                patch_db::json_patch::patch(&mut view, &revision.patch).unwrap();
                assert!(view["package"]["packages"].get("secret").is_none());
                saw_revision = true;
            }
            43 => {
                assert!(
                    response["result"]["package"]["packages"]
                        .get("secret")
                        .is_none(),
                    "{response}"
                );
                saw_reply = true;
            }
            _ => panic!("unexpected response: {response}"),
        }
    }
    for id in [41, 42] {
        socket
            .send(Message::Text(
                json!({"rpcstream":"1.0", "close":id}).to_string().into(),
            ))
            .await
            .unwrap();
    }
    socket.close().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if ctx
                .index_mirrors
                .lock()
                .await
                .values()
                .all(|mirror| mirror.strong_count() == 0)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
