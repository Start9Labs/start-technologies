use axum::extract::ws::Message;
use futures::{SinkExt, StreamExt};
use imbl_value::json;
use patch_db::json_ptr::ROOT;

use super::connection::{receive_updates, subscription_request, websocket_url};
use super::*;
use crate::account::AccountInfo;
use crate::db::model::public::Public;
use crate::registry::device_info::OsInfo;
use crate::registry::os::index::OsVersionInfo;
use crate::registry::signer::SignerInfo;
use crate::rpc_continuations::Guid;
use crate::util::future::NonDetachingJoinHandle;

mod error_details;
mod live;
mod live_legacy;
mod polling;

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
    public.server_info.registries = from_value(json!({url().as_str(): "Test"})).unwrap();
    public.server_info.os_registry = url();
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

#[tokio::test]
async fn refreshing_a_registry_cancels_its_group_without_cancelling_other_registries() {
    let registry = url();
    let sibling = Url::parse("https://other.test/").unwrap();
    let waiting = || {
        let (sender, receiver) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            let _sender = sender;
            std::future::pending::<()>().await;
        });
        (NonDetachingJoinHandle::from(task), receiver)
    };
    let (task, cancelled) = waiting();
    let (other, mut retained) = waiting();
    let group = BTreeSet::from([
        Subscription::Index(registry.clone()),
        Subscription::Os(registry.clone()),
    ]);
    let other_group = BTreeSet::from([Subscription::Index(sibling)]);
    let mut tasks = RegistryTasks::from([(group, task), (other_group.clone(), other)]);

    cancel_registry_connection(&mut tasks, &registry).await;
    assert_eq!(tasks.keys().collect::<Vec<_>>(), vec![&other_group]);
    assert!(
        tokio::time::timeout(Duration::from_secs(1), cancelled)
            .await
            .unwrap()
            .is_err()
    );
    assert!(matches!(
        retained.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ));
    drop(tasks);
    assert!(
        tokio::time::timeout(Duration::from_secs(1), retained)
            .await
            .unwrap()
            .is_err()
    );
}

#[tokio::test]
async fn registry_settings_watch_ignores_unrelated_writes_and_reads_coherent_settings() {
    let temp = tempfile::tempdir().unwrap();
    let db = TypedPatchDb::<Database>::load_unchecked(
        patch_db::PatchDb::open(temp.path().join("db"))
            .await
            .unwrap(),
    );
    db.put(&ROOT, database().as_value()).await.unwrap();
    let mut watch = RegistrySettingsWatch::new(&db).await;
    watch.read(&db).await.unwrap();
    db.mutate(|db| {
        db.as_public_mut().as_server_info_mut().mutate(|info| {
            info.unread_notification_count += 1;
            info.registry_keys
                .insert(url().as_str().into(), BTreeSet::new());
            Ok::<_, Error>(())
        })
    })
    .await
    .result
    .unwrap();
    assert!(futures::poll!(std::pin::pin!(watch.changed())).is_pending());
    db.mutate(|db| {
        db.as_public_mut().as_server_info_mut().mutate(|info| {
            info.registries
                .insert(url().as_str().into(), Some("Renamed".into()));
            Ok::<_, Error>(())
        })
    })
    .await
    .result
    .unwrap();
    watch.changed().await.unwrap();
    assert_eq!(
        watch.read(&db).await.unwrap().registries[url().as_str()].as_deref(),
        Some("Renamed")
    );
    let other: Url = "https://other.test/".parse().unwrap();
    db.mutate(|db| {
        db.as_public_mut().as_server_info_mut().mutate(|info| {
            info.registries
                .insert(other.as_str().into(), Some("Other".into()));
            info.os_registry = other.clone();
            Ok::<_, Error>(())
        })
    })
    .await
    .result
    .unwrap();
    watch.changed().await.unwrap();
    let observed = watch.read(&db).await.unwrap();
    assert_eq!(observed.os_registry, other);
    assert!(observed.registries.contains_key(other.as_str()));
    assert!(futures::poll!(std::pin::pin!(watch.changed())).is_pending());
}

#[tokio::test]
async fn registry_manager_follows_server_info_and_ignores_opaque_ui_settings() {
    let temp = tempfile::tempdir().unwrap();
    let db = TypedPatchDb::<Database>::load_unchecked(
        patch_db::PatchDb::open(temp.path().join("db"))
            .await
            .unwrap(),
    );
    let mut model = database();
    model
        .as_public_mut()
        .as_ui_mut()
        .ser(&json!({
            "registries": {"https://obsolete.test": "Obsolete"},
            "startosRegistry": "https://obsolete.test"
        }))
        .unwrap();
    db.put(&ROOT, model.as_value()).await.unwrap();
    let mut cache = db.watch("/public/registryCache".parse().unwrap()).await;
    let (refresh, refreshes) = tokio::sync::mpsc::channel(1);
    let (started, mut starts) = tokio::sync::mpsc::unbounded_channel();
    let observed = db.clone();
    let _manager: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        watch_registries(observed, refreshes, move || {
            let _ = started.send(());
            None
        })
        .await
    })
    .into();
    tokio::time::timeout(Duration::from_secs(2), async {
        while cache
            .peek_and_mark_seen()
            .unwrap()
            .get(url().as_str())
            .is_none()
        {
            cache.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(
        cache
            .peek_and_mark_seen()
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        1
    );
    tokio::time::timeout(Duration::from_secs(2), starts.recv())
        .await
        .unwrap()
        .unwrap();
    refresh.send(url()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), starts.recv())
        .await
        .unwrap()
        .unwrap();
    db.mutate(|db| {
        db.as_public_mut()
            .as_server_info_mut()
            .as_registries_mut()
            .ser(&crate::trusted_keys::Registries::new())
    })
    .await
    .result
    .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !cache
            .peek_and_mark_seen()
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty()
        {
            cache.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(
        db.peek()
            .await
            .as_public()
            .as_os_update_cache()
            .de()
            .unwrap()
            .unwrap()
            .registry,
        url()
    );
}

#[test]
fn invalid_saved_registry_does_not_block_the_others() {
    let mut info = database().as_public().as_server_info().de().unwrap();
    info.registries =
        from_value(json!({"https://user@invalid.test":null, "https://registry.test/catalog":null}))
            .unwrap();
    let desired = subscriptions(&info);
    assert_eq!(
        desired,
        BTreeSet::from([Subscription::Index(url()), Subscription::Os(url())])
    );
}

#[test]
fn invalid_os_source_does_not_stop_package_subscriptions() {
    let mut info = database().as_public().as_server_info().de().unwrap();
    info.os_registry = "https://user@invalid.test".parse().unwrap();
    let desired = subscriptions(&info);
    assert_eq!(desired, BTreeSet::from([Subscription::Index(url())]));
}

#[test]
fn canonical_configuration_has_one_subscription_per_registry_plus_the_os_source() {
    let mut info = database().as_public().as_server_info().de().unwrap();
    info.registries = from_value(json!({"https://REGISTRY.test/catalog/":null,"https://registry.test/catalog":null,"https://other.test":null})).unwrap();
    let desired = subscriptions(&info);
    assert_eq!(desired.len(), 3);
    let connections = connections(&desired);
    assert_eq!(connections.len(), 2);
    assert!(connections.contains(&BTreeSet::from([
        Subscription::Index(url()),
        Subscription::Os(url())
    ])));
    assert!(desired.contains(&Subscription::Index(url())));
    assert!(desired.contains(&Subscription::Os(url())));
    assert_eq!(Subscription::Index(url()).pointer(), "/index");
    assert_eq!(Subscription::Os(url()).pointer(), "/index/os");
}

#[test]
fn websocket_urls_preserve_the_registry_prefix_and_tls() {
    for (registry, expected) in [
        (
            "https://registry.test/catalog/",
            "wss://registry.test/catalog/rpc/v2".to_owned(),
        ),
        (
            "http://registry.test:5959/",
            "ws://registry.test:5959/rpc/v2".to_owned(),
        ),
    ] {
        assert_eq!(
            websocket_url(&registry.parse().unwrap()).unwrap().as_str(),
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
    assert_eq!(cache.status, RegistryStatus::Connected);
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
    set_status(
        &mut db,
        &Subscription::Index(url()),
        RegistryStatus::Error,
        None,
    )
    .unwrap();
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
    set_status(
        &mut db,
        &Subscription::Index(url()),
        RegistryStatus::Error,
        None,
    )
    .unwrap();
    set_status(
        &mut db,
        &Subscription::Os(url()),
        RegistryStatus::Error,
        None,
    )
    .unwrap();
    assert_eq!(
        db.as_public()
            .as_registry_cache()
            .as_idx(&url())
            .unwrap()
            .de()
            .unwrap()
            .status,
        RegistryStatus::Error
    );
    assert_eq!(
        db.as_public()
            .as_os_update_cache()
            .de()
            .unwrap()
            .unwrap()
            .status,
        RegistryStatus::Error
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
    let desired = subscriptions(&db.as_public().as_server_info().de().unwrap());
    reconcile(&mut db, &desired, true).unwrap();
    let cache = db
        .as_public()
        .as_registry_cache()
        .as_idx(&url())
        .unwrap()
        .de()
        .unwrap();
    assert_eq!(cache.status, RegistryStatus::Connecting);
    assert_eq!(cache.info.name.as_deref(), Some("Test Registry"));
    assert!(!cache.packages.is_empty());
}

#[test]
fn registry_status_distinguishes_loading_empty_success_error_and_retry() {
    let mut db = database();
    let subscription = Subscription::Index(url());
    for status in [
        RegistryStatus::Connecting,
        RegistryStatus::Error,
        RegistryStatus::Connecting,
    ] {
        set_status(&mut db, &subscription, status, None).unwrap();
        let cache = db
            .as_public()
            .as_registry_cache()
            .as_idx(&url())
            .unwrap()
            .de()
            .unwrap();
        assert_eq!(cache.status, status);
        assert!(cache.packages.is_empty());
    }
    publish_snapshot(
        &mut db,
        &subscription,
        &to_value(&FullIndex::default()).unwrap(),
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
    assert_eq!(cache.status, RegistryStatus::Connected);
    assert!(cache.packages.is_empty());
    publish_snapshot(
        &mut db,
        &subscription,
        &to_value(&index()).unwrap(),
        &device(),
    )
    .unwrap();
    let packages = db
        .as_public()
        .as_registry_cache()
        .as_idx(&url())
        .unwrap()
        .de()
        .unwrap()
        .packages;
    assert!(!packages.is_empty());
    for status in [RegistryStatus::Error, RegistryStatus::Connecting] {
        set_status(&mut db, &subscription, status, None).unwrap();
        let cache = db
            .as_public()
            .as_registry_cache()
            .as_idx(&url())
            .unwrap()
            .de()
            .unwrap();
        assert_eq!(cache.status, status);
        assert_eq!(
            to_value(&cache.packages).unwrap(),
            to_value(&packages).unwrap()
        );
    }
}

#[test]
fn os_catalog_status_distinguishes_initial_attempt_failure_and_success() {
    let mut db = database();
    let subscription = Subscription::Os(url());
    for status in [
        RegistryStatus::Connecting,
        RegistryStatus::Error,
        RegistryStatus::Connecting,
    ] {
        set_status(&mut db, &subscription, status, None).unwrap();
        let cache = db.as_public().as_os_update_cache().de().unwrap().unwrap();
        assert_eq!(cache.status, status);
        assert!(cache.versions.is_empty());
    }
    publish_snapshot(
        &mut db,
        &subscription,
        &to_value(&OsIndex::default()).unwrap(),
        &device(),
    )
    .unwrap();
    assert_eq!(
        db.as_public()
            .as_os_update_cache()
            .de()
            .unwrap()
            .unwrap()
            .status,
        RegistryStatus::Connected
    );
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
        .as_server_info_mut()
        .mutate(|info| {
            info.registries.clear();
            info.os_registry = "https://other.test".parse().unwrap();
            Ok::<_, Error>(())
        })
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
    let desired = subscriptions(&db.as_public().as_server_info().de().unwrap());
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
    assert_eq!(cache.status, RegistryStatus::Connected);
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
async fn watch_applies_scoped_revisions_and_answers_heartbeats_on_reconnect() {
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
        let registry_url = format!(
            "http://registry.test:{}/catalog",
            listener.local_addr().unwrap().port()
        )
        .parse()
        .unwrap();
        let source = source.clone();
        let server: NonDetachingJoinHandle<()> = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let mut socket = live::encrypted_server(socket).await;
            let Message::Text(request) = socket.next().await.unwrap().unwrap() else {
                panic!("expected encrypted request")
            };
            let request: Value = serde_json::from_str(&request).unwrap();
            assert_eq!(request["method"].as_str(), Some("db.subscribe"));
            assert_eq!(request["params"]["osVersion"], json!(device().os.version));
            assert_eq!(request["params"]["pointer"].as_str(), Some("/index"));
            assert_eq!(
                request["rpcstream"].as_str(),
                Some(rpc_toolkit::RPCSTREAM_VERSION)
            );
            let id = request["id"].clone();
            socket
                .send(Message::Text(
                    json!({"jsonrpc": "2.0", "id": id, "result": dump})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            if attempt == 0 {
                let mut next = index();
                next.name = Some("Renamed".into());
                let revision = source.put(&ROOT, &next).await.unwrap().unwrap();
                socket
                    .send(Message::Text(
                        json!({"rpcstream":"1.0", "id":id, "result":revision, "end":false})
                            .to_string()
                            .into(),
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
        let signer = crate::sign::Signer::Key(crate::sign::AnySigningKey::Ed25519(
            ed25519_dalek::SigningKey::from_bytes(&[71; 32]),
        ));
        let (socket, _proxy) = live::native_connection(&registry_url, &signer).await;
        let socket = socket.unwrap();
        let mut backoff = 300;
        tokio::time::timeout(
            Duration::from_secs(5),
            receive_updates(
                socket,
                &db,
                &BTreeSet::from([Subscription::Index(url())]),
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
