use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::routing::get;
use imbl_value::json;
use patch_db::PatchDb;
use patch_db::json_ptr::ROOT;

use super::*;
use crate::registry::package::icon::icons_dir;
use crate::registry::tests::{fixture, rpc, serve, server};
use crate::sign::AnySigningKey;

fn clients() -> UpstreamClients {
    UpstreamClients::new("socks5h://127.0.0.1:9050".parse().unwrap()).unwrap()
}

async fn mirror_db() -> TypedPatchDb<RegistryDatabase> {
    let db = TypedPatchDb::<RegistryDatabase>::load_unchecked(PatchDb::in_memory());
    db.put(&ROOT, &RegistryDatabase::init()).await.unwrap();
    db
}

async fn wait_for(
    db: &TypedPatchDb<RegistryDatabase>,
    predicate: impl Fn(&FullIndex) -> bool,
) -> FullIndex {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let index = db.peek().await.into_index().de().unwrap();
            if predicate(&index) {
                return index;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap()
}

fn package_ids(index: &FullIndex) -> BTreeSet<String> {
    index
        .package
        .packages
        .keys()
        .map(|id| id.to_string())
        .collect()
}

fn spawn_follow(
    db: &TypedPatchDb<RegistryDatabase>,
    icons: &Path,
    signer: Signer,
    upstream: &Url,
) -> NonDetachingJoinHandle<Result<(), Error>> {
    let (db, icons, upstream) = (db.clone(), icons.to_owned(), upstream.clone());
    tokio::spawn(async move { follow(&db, &icons, &clients(), &signer, &upstream, &mut 300).await })
        .into()
}

#[tokio::test]
async fn mirror_follows_the_index_its_identity_can_read_and_copies_icons() {
    let (_upstream_temp, upstream, _admin, reader, icon) = fixture().await;
    let (url, _server) = server(upstream.clone()).await;

    let temp = tempfile::tempdir().unwrap();
    let icons = icons_dir(temp.path());
    let db = mirror_db().await;
    let _sync = spawn_follow(&db, &icons, reader, &url);
    let mirrored = wait_for(&db, |index| index.package.packages.len() == 2).await;
    assert_eq!(
        package_ids(&mirrored),
        BTreeSet::from(["public".into(), "secret".into()])
    );
    assert_eq!(
        tokio::fs::read(icons.join(&icon)).await.unwrap(),
        b"secret".to_vec()
    );

    upstream
        .db
        .mutate(|db| {
            db.as_index_mut()
                .as_name_mut()
                .ser(&Some("Upstream".into()))
        })
        .await
        .result
        .unwrap();
    wait_for(&db, |index| index.name.as_deref() == Some("Upstream")).await;

    let stranger = Signer::Key(AnySigningKey::Ed25519(
        ed25519_dalek::SigningKey::from_bytes(&[40; 32]),
    ));
    let public_db = mirror_db().await;
    let public_temp = tempfile::tempdir().unwrap();
    let _public_sync = spawn_follow(&public_db, &icons_dir(public_temp.path()), stranger, &url);
    let public = wait_for(&public_db, |index| index.name.is_some()).await;
    assert_eq!(package_ids(&public), BTreeSet::from(["public".into()]));
    assert!(
        !tokio::fs::try_exists(icons_dir(public_temp.path()).join(&icon))
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn copy_icon_rejects_bytes_that_do_not_match_the_path() {
    let (_upstream_temp, upstream, _admin, reader, icon) = fixture().await;
    let upstream_icon = icons_dir(&upstream.datadir).join(&icon);
    tokio::fs::write(&upstream_icon, b"tampered").await.unwrap();
    let (url, _server) = server(upstream).await;
    let temp = tempfile::tempdir().unwrap();
    let icons = icons_dir(temp.path());

    let error = copy_icon(&clients().http, &reader, &url, &icons, &icon)
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Incoherent);
    assert!(!tokio::fs::try_exists(icons.join(&icon)).await.unwrap());
}

#[tokio::test]
async fn icon_downloads_do_not_follow_redirects() {
    let (_upstream_temp, _upstream, _admin, reader, icon) = fixture().await;
    let requests = Arc::new(AtomicUsize::new(0));
    let (target, _target_server) = serve(Router::new().route(
        "/target",
        get({
            let requests = requests.clone();
            move || {
                requests.fetch_add(1, Ordering::SeqCst);
                async { "secret" }
            }
        }),
    ))
    .await;
    let target = target.join("target").unwrap().to_string();
    let (url, _origin_server) = serve(Router::new().route(
        "/icons/{*path}",
        get(move || {
            let target = target.clone();
            async move { axum::response::Redirect::temporary(&target) }
        }),
    ))
    .await;
    let temp = tempfile::tempdir().unwrap();
    let icons = icons_dir(temp.path());

    assert!(
        copy_icon(&clients().http, &reader, &url, &icons, &icon)
            .await
            .is_err()
    );
    assert_eq!(requests.load(Ordering::SeqCst), 0);
    assert!(!tokio::fs::try_exists(icons.join(&icon)).await.unwrap());
}

#[tokio::test]
async fn mirror_replaces_the_index_when_an_icon_body_stalls() {
    let (_upstream_temp, upstream, _admin, reader, icon) = fixture().await;
    let index = upstream.db.peek().await.into_index().de().unwrap();
    let value = to_value(&index).unwrap();
    let requested = Arc::new(tokio::sync::Notify::new());
    let (url, _server) = serve(Router::new().route(
        "/icons/{*path}",
        get({
            let requested = requested.clone();
            move || {
                requested.notify_one();
                async {
                    axum::response::Response::builder()
                        .header(http::header::CONTENT_LENGTH, "6")
                        .body(axum::body::Body::from_stream(futures::stream::pending::<
                            Result<bytes::Bytes, std::io::Error>,
                        >()))
                        .unwrap()
                }
            }
        }),
    ))
    .await;
    let temp = tempfile::tempdir().unwrap();
    let icons = icons_dir(temp.path());
    let db = mirror_db().await;
    let sync: NonDetachingJoinHandle<Result<(), Error>> = {
        let db = db.clone();
        let icons = icons.clone();
        let value = value.clone();
        tokio::spawn(async move {
            replace_index(&db, &icons, &clients().http, &reader, &url, &value).await
        })
        .into()
    };
    tokio::time::timeout(Duration::from_secs(5), requested.notified())
        .await
        .unwrap();
    tokio::time::pause();
    tokio::time::advance(ICON_DOWNLOAD_TIMEOUT + Duration::from_secs(1)).await;
    tokio::time::resume();
    tokio::time::timeout(Duration::from_secs(5), sync)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert_eq!(
        to_value(&db.peek().await.into_index().de().unwrap()).unwrap(),
        value
    );
    assert!(!tokio::fs::try_exists(icons.join(&icon)).await.unwrap());
}

#[tokio::test]
async fn mirror_rejects_index_writes_but_serves_reads_and_local_admin_changes() {
    let (_temp, ctx, admin, reader, _) = fixture().await;
    let ctx = ctx.with_upstream(Upstream::spawn(
        mirror_db().await,
        PathBuf::new(),
        clients(),
        AnySigningKey::generate().into(),
        "http://127.0.0.1:9/".parse().unwrap(),
    ));
    let before = ctx.db.peek().await.into_index().de().unwrap();
    let (url, _server) = server(ctx.clone()).await;

    for (method, params) in [
        ("info.set-name", json!({ "name": "Renamed" })),
        (
            "package.access.set-private",
            json!({ "id": "public", "private": true }),
        ),
        ("os.asset.sign.iso", json!({})),
        ("db.apply", json!({ "expr": "." })),
    ] {
        let response = rpc(&url, method, params, Some(&admin)).await;
        assert_eq!(
            response["error"]["code"],
            json!(ErrorKind::InvalidRequest as i32),
            "{method}: {response}"
        );
    }
    assert_eq!(
        to_value(&ctx.db.peek().await.into_index().de().unwrap()).unwrap(),
        to_value(&before).unwrap()
    );

    let index = rpc(&url, "index", json!({}), None).await;
    assert!(
        index["result"]["package"]["packages"]
            .get("public")
            .is_some(),
        "{index}"
    );

    let reader_id = ctx
        .db
        .peek()
        .await
        .as_index()
        .as_signers()
        .get_signer(&reader.verifying_key().await.unwrap())
        .unwrap();
    let response = rpc(
        &url,
        "admin.add",
        json!({ "signer": reader_id }),
        Some(&admin),
    )
    .await;
    assert!(response.get("error").is_none(), "{response}");
    assert!(
        ctx.db
            .peek()
            .await
            .as_admins()
            .de()
            .unwrap()
            .contains(&reader_id)
    );
}
