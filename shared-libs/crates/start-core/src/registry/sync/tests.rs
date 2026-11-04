use imbl_value::json;
use patch_db::json_ptr::ROOT;

use super::*;
use crate::account::AccountInfo;
use crate::db::model::public::Public;
use crate::registry::device_info::OsInfo;
use crate::registry::os::index::OsVersionInfo;
use crate::registry::signer::SignerInfo;
use crate::util::future::NonDetachingJoinHandle;

fn url() -> Url {
    registry_url("https://registry.test/catalog/").unwrap()
}

fn device() -> DeviceInfo {
    DeviceInfo {
        os: OsInfo {
            version: "0.4.0.5".parse().unwrap(),
            compat: "*".parse().unwrap(),
            platform: "x86_64".into(),
            language: None,
        },
        hardware: None,
    }
}

fn database() -> DatabaseModel {
    let account = AccountInfo::new("password", std::time::SystemTime::now(), None).unwrap();
    let mut public = Public::init(&account, false, None, None).unwrap();
    public.ui = json!({"registries": {url().as_str(): "Test"}, "startosRegistry": url()});
    public.server_info.version = device().os.version;
    Model::from_value(json!({
        "public": public,
        "private": {"registryIndexes": {}, "notifications": {}, "osSecurityNotifications": {},
            "push": crate::notifications::push::PushState::default()},
    }))
}

fn index() -> FullIndex {
    serde_json::from_str(r#"{
        "name": "Test Registry", "icon": null, "description": null,
        "package": {"categories": {}, "packages": {"test": {"authorized": {}, "categories": [], "versions": {
            "1.0.0:0": {"title":"Test", "description":{"short":"Test", "long":"Test"},
                "releaseNotes":"First", "gitHash":null, "license":"MIT", "upstreamRepo":"https://upstream.test",
                "marketingUrl":null, "donationUrl":null, "osVersion":"0.4.0", "sdkVersion":null,
                "sourceVersion":"*", "dependencyMetadata":{}, "s9pks":[]},
            "2.0.0:0": {"title":"Test", "description":{"short":"Test", "long":"Test"},
                "releaseNotes":"Second", "gitHash":null, "license":"MIT", "upstreamRepo":"https://upstream.test",
                "marketingUrl":null, "donationUrl":null, "osVersion":"0.4.0", "sdkVersion":null,
                "sourceVersion":">=1.5.0", "dependencyMetadata":{}, "s9pks":[]}
        }}}},
        "os": {"versions": {}}, "signers":{}, "trustSigners":[]
    }"#).unwrap()
}

#[test]
fn canonical_configuration_has_one_subscription_per_registry_plus_the_os_source() {
    let desired = subscriptions(&json!({
        "registries": {"https://REGISTRY.test/catalog/":null,"https://registry.test/catalog":null,
            "https://other.test":null}, "startosRegistry":"https://registry.test/catalog/"
    }))
    .unwrap();
    assert_eq!(desired.len(), 3);
    assert!(desired.contains(&Subscription::Index(url())));
    assert!(desired.contains(&Subscription::Os(url())));
    assert_eq!(Subscription::Index(url()).pointer(), "/index");
    assert_eq!(Subscription::Os(url()).pointer(), "/index/os");
}

#[test]
fn websocket_urls_preserve_the_registry_prefix_and_tls() {
    let guid = Guid::new();
    for (registry, expected) in [
        (
            "https://registry.test/catalog/",
            format!("wss://registry.test/catalog/ws/rpc/{guid}"),
        ),
        (
            "http://registry.test:5959/",
            format!("ws://registry.test:5959/ws/rpc/{guid}"),
        ),
    ] {
        assert_eq!(
            websocket_url(&registry.parse().unwrap(), &guid)
                .unwrap()
                .as_str(),
            expected
        );
    }
}

#[test]
fn package_and_signer_changes_publish_atomically_without_granting_trust() {
    let mut db = database();
    let mut index = index();
    let signer = Guid::new();
    let key = crate::sign::AnyVerifyingKey::Ed25519(
        ed25519_dalek::SigningKey::from_bytes(&[93; 32]).verifying_key(),
    );
    let id = key.interned_der64();
    index.trust_signers.insert(signer.clone());
    index.signers.insert(
        signer,
        SignerInfo {
            name: "Publisher".into(),
            contact: vec![],
            keys: [key].into(),
            gpg_exports: Default::default(),
        },
    );
    publish_snapshot(
        &mut db,
        &Subscription::Index(url()),
        &to_value(&index).unwrap(),
        &device(),
    )
    .unwrap();
    let cache = db
        .as_public()
        .as_registry_cache()
        .as_idx(&url())
        .unwrap()
        .de()
        .unwrap();
    assert!(cache.connected);
    assert_eq!(cache.info.name.as_deref(), Some("Test Registry"));
    assert!(
        cache.packages["test"]
            .best
            .contains_key(&"2.0.0:0".parse::<crate::util::VersionString>().unwrap())
    );
    let key = db
        .as_public()
        .as_server_info()
        .as_trusted_keys()
        .as_idx(&id)
        .unwrap()
        .de()
        .unwrap();
    assert!(!key.marketplace && !key.os && !key.removable);
    assert_eq!(db.as_private().as_notifications().de().unwrap().0.len(), 1);
    publish_snapshot(
        &mut db,
        &Subscription::Index(url()),
        &to_value(&index).unwrap(),
        &device(),
    )
    .unwrap();
    assert_eq!(db.as_private().as_notifications().de().unwrap().0.len(), 1);
    index.trust_signers.clear();
    publish_snapshot(
        &mut db,
        &Subscription::Index(url()),
        &to_value(&index).unwrap(),
        &device(),
    )
    .unwrap();
    assert_eq!(db.as_private().as_notifications().de().unwrap().0.len(), 2);
    assert!(
        db.as_public()
            .as_server_info()
            .as_trusted_keys()
            .as_idx(&id)
            .unwrap()
            .de()
            .unwrap()
            .removable
    );
}

#[test]
fn cached_package_queries_keep_target_and_source_selection_while_offline() {
    let mut db = database();
    publish_snapshot(
        &mut db,
        &Subscription::Index(url()),
        &to_value(&index()).unwrap(),
        &device(),
    )
    .unwrap();
    set_disconnected(&mut db, &Subscription::Index(url())).unwrap();
    let result = cached_response(
        &db,
        &url(),
        "package.get",
        &json!({"id":"test", "sourceVersion":"1.0.0:0", "otherVersions":"short"}),
        Some(&device()),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        result["best"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        vec!["1.0.0:0"]
    );
    let result = cached_response(
        &db,
        &url(),
        "package.get",
        &json!({"id":"test", "targetVersion":"=1.0.0:0"}),
        Some(&device()),
    )
    .unwrap()
    .unwrap();
    assert!(result["best"]["1.0.0:0"].is_object());
    assert!(
        cached_response(
            &db,
            &"https://unconnected.test".parse().unwrap(),
            "info",
            &json!({}),
            None
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn first_connection_failure_publishes_availability_and_does_not_fetch_on_cache_queries() {
    let mut db = database();
    set_disconnected(&mut db, &Subscription::Index(url())).unwrap();
    set_disconnected(&mut db, &Subscription::Os(url())).unwrap();
    assert!(
        !db.as_public()
            .as_registry_cache()
            .as_idx(&url())
            .unwrap()
            .de()
            .unwrap()
            .connected
    );
    assert!(
        !db.as_public()
            .as_os_update_cache()
            .de()
            .unwrap()
            .unwrap()
            .connected
    );
    let result = cached_response(
        &db,
        &url(),
        "package.get",
        &json!({"id":"test"}),
        Some(&device()),
    )
    .unwrap()
    .unwrap();
    assert_eq!(result["best"], json!({}));
    publish_snapshot(
        &mut db,
        &Subscription::Index(url()),
        &to_value(&index()).unwrap(),
        &device(),
    )
    .unwrap();
    reconcile(
        &mut db,
        &subscriptions(&json!({"registries":{url().as_str():null},"startosRegistry":url()}))
            .unwrap(),
        true,
    )
    .unwrap();
    let cache = db
        .as_public()
        .as_registry_cache()
        .as_idx(&url())
        .unwrap()
        .de()
        .unwrap();
    assert!(!cache.connected);
    assert_eq!(cache.info.name.as_deref(), Some("Test Registry"));
    assert!(!cache.packages.is_empty());
}

#[test]
fn removal_and_os_switch_reject_in_flight_snapshots_and_prune_old_caches() {
    let mut db = database();
    publish_snapshot(
        &mut db,
        &Subscription::Index(url()),
        &to_value(&index()).unwrap(),
        &device(),
    )
    .unwrap();
    db.as_public_mut()
        .as_ui_mut()
        .ser(&json!({"registries":{},"startosRegistry":"https://other.test"}))
        .unwrap();
    let before = db.as_value().clone();
    publish_snapshot(
        &mut db,
        &Subscription::Index(url()),
        &to_value(&index()).unwrap(),
        &device(),
    )
    .unwrap();
    publish_snapshot(
        &mut db,
        &Subscription::Os(url()),
        &to_value(&OsIndex::default()).unwrap(),
        &device(),
    )
    .unwrap();
    assert_eq!(db.as_value(), &before);
    let desired = subscriptions(&db.as_public().as_ui().de().unwrap()).unwrap();
    reconcile(&mut db, &desired, false).unwrap();
    assert!(db.as_public().as_registry_cache().de().unwrap().is_empty());
    assert!(
        db.as_private()
            .as_registry_indexes()
            .de()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn os_cache_excludes_other_platforms_and_incompatible_sources() {
    let mut db = database();
    let mut index = OsIndex::default();
    let asset = from_value(json!({"publishedAt":"2026-01-01T00:00:00Z", "urls":[], "commitment":{"hash":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","size":0}, "signatures":{}})).unwrap();
    let mut release = OsVersionInfo {
        source_version: "*".parse().unwrap(),
        squashfs: [("x86_64".into(), asset)].into(),
        ..Default::default()
    };
    index.versions.0.insert("0.4.0.6".parse().unwrap(), release);
    release = OsVersionInfo {
        source_version: ">=0.5.0".parse().unwrap(),
        ..Default::default()
    };
    index.versions.0.insert("0.5.0".parse().unwrap(), release);
    publish_snapshot(
        &mut db,
        &Subscription::Os(url()),
        &to_value(&index).unwrap(),
        &device(),
    )
    .unwrap();
    let cache = db.as_public().as_os_update_cache().de().unwrap().unwrap();
    assert!(cache.connected);
    assert_eq!(cache.versions.len(), 1);
    assert!(cache.versions.contains_key(&"0.4.0.6".parse().unwrap()));
    assert!(
        db.as_private()
            .as_notifications()
            .de()
            .unwrap()
            .0
            .is_empty()
    );
}

#[tokio::test]
async fn continuation_applies_scoped_revisions_and_answers_heartbeats_on_reconnect() {
    let temp = tempfile::tempdir().unwrap();
    let db = TypedPatchDb::<Database>::load_unchecked(
        PatchDb::open(temp.path().join("db")).await.unwrap(),
    );
    db.put(&ROOT, database().as_value()).await.unwrap();
    let source = PatchDb::in_memory();
    source.put(&ROOT, &index()).await.unwrap();
    for attempt in 0..2 {
        let dump = source.dump(&ROOT).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let socket_url = format!("ws://{}", listener.local_addr().unwrap());
        let source = source.clone();
        let server: NonDetachingJoinHandle<()> = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            if attempt == 0 {
                let mut next = index();
                next.name = Some("Renamed".into());
                let revision = source.put(&ROOT, &next).await.unwrap().unwrap();
                socket
                    .send(Message::Text(
                        serde_json::to_string(&revision).unwrap().into(),
                    ))
                    .await
                    .unwrap();
            }
            socket
                .send(Message::Ping(vec![1, 2, 3].into()))
                .await
                .unwrap();
            assert_eq!(
                socket.next().await.unwrap().unwrap(),
                Message::Pong(vec![1, 2, 3].into())
            );
            socket.send(Message::Close(None)).await.unwrap();
        })
        .into();
        let (socket, _) = tokio_tungstenite::connect_async(socket_url).await.unwrap();
        let mut backoff = 300;
        tokio::time::timeout(
            Duration::from_secs(5),
            receive_updates(
                socket,
                dump,
                &db,
                &Subscription::Index(url()),
                &device(),
                &mut backoff,
            ),
        )
        .await
        .unwrap()
        .unwrap();
        server.await.unwrap();
        assert_eq!(backoff, 30);
        assert_eq!(
            db.peek()
                .await
                .as_public()
                .as_registry_cache()
                .as_idx(&url())
                .unwrap()
                .de()
                .unwrap()
                .info
                .name
                .as_deref(),
            Some("Renamed")
        );
    }
}
