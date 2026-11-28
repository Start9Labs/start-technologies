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

mod default_locale;
mod error_details;
mod live;
mod live_legacy;
mod polling;
mod resume;

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

fn select(db: &DatabaseModel, registry: &Url, params: Value) -> Result<Value, Error> {
    let mut params: GetPackageParams = from_value(params)?;
    params.device_info = Some(device());
    synced_packages(db, registry, &params)
}

fn database() -> DatabaseModel {
    let account = AccountInfo::new("password", std::time::SystemTime::now(), None).unwrap();
    let mut public = Public::init(&account, false, None, None).unwrap();
    public.server_info.registries = from_value(json!({url().as_str(): "Test"})).unwrap();
    public.server_info.os_registry = url();
    public.server_info.version = device().os.version;
    Model::from_value(json!({
        "public": public,
        "private": {"notifications": {}, "osSecurityNotifications": {},
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
async fn reconciliation_restarts_every_registry_after_a_coalesced_language_change() {
    let mut tasks = RegistryTasks::new();
    let mut cancelled = Vec::new();
    for url in [url(), "https://other.test/".parse().unwrap()] {
        let (sender, receiver) = tokio::sync::oneshot::channel::<()>();
        cancelled.push(receiver);
        tasks.insert(
            BTreeSet::from([Subscription::Index(url)]),
            tokio::spawn(async move {
                let _sender = sender;
                std::future::pending::<()>().await;
            })
            .into(),
        );
    }
    let mut previous = Some(Some("en_US".into()));
    reconcile_connection_language(&mut tasks, &mut previous, &Some("en_US".into())).await;
    assert_eq!(tasks.len(), 2);
    reconcile_connection_language(&mut tasks, &mut previous, &Some("de_DE".into())).await;
    assert!(tasks.is_empty());
    assert_eq!(previous, Some(Some("de_DE".into())));
    for receiver in cancelled {
        assert!(receiver.await.is_err());
    }
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
        cache.selections["test"]
            .best
            .contains(&"2.0.0:0".parse::<crate::util::VersionString>().unwrap())
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
fn synced_package_queries_keep_target_and_source_selection_while_offline() {
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
    let result = select(
        &db,
        &url(),
        json!({"id":"test", "sourceVersion":"1.0.0:0", "otherVersions":"short"}),
    )
    .unwrap();
    assert_eq!(
        result["best"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        vec!["1.0.0:0"]
    );
    let result = select(
        &db,
        &url(),
        json!({"id":"test", "targetVersion":"=1.0.0:0"}),
    )
    .unwrap();
    assert!(result["best"]["1.0.0:0"].is_object());
    assert!(select(&db, &"https://unconnected.test".parse().unwrap(), json!({})).is_err());
}

#[test]
fn connecting_registries_select_nothing_and_first_failures_are_unreachable() {
    let mut db = database();
    for subscription in [Subscription::Index(url()), Subscription::Os(url())] {
        set_status(&mut db, &subscription, RegistryStatus::Connecting, None).unwrap();
    }
    assert_eq!(
        select(&db, &url(), json!({"id":"test"})).unwrap()["best"],
        json!({})
    );
    assert!(synced_os_versions(&db).unwrap().is_empty());
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
    assert!(select(&db, &url(), json!({"id":"test"})).is_err());
    assert!(synced_os_versions(&db).is_err());
    publish_snapshot(
        &mut db,
        &Subscription::Os(url()),
        &to_value(&OsIndex::default()).unwrap(),
        &device(),
    )
    .unwrap();
    assert!(synced_os_versions(&db).unwrap().is_empty());
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
    assert!(!cache.selections.is_empty());
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
        assert!(cache.selections.is_empty());
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
    assert!(cache.selections.is_empty());
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
        .selections;
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
            to_value(&cache.selections).unwrap(),
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
    db.as_private_mut()
        .as_value_mut()
        .as_object_mut()
        .unwrap()
        .insert("registryIndexes".into(), json!({ url().as_str(): {} }));
    let desired = subscriptions(&db.as_public().as_server_info().de().unwrap());
    reconcile(&mut db, &desired, false).unwrap();
    assert!(db.as_private().as_value().get("registryIndexes").is_none());
    assert!(db.as_public().as_registry_cache().de().unwrap().is_empty());
    assert!(
        db.as_public()
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
    assert_eq!(
        synced_os_versions(&db).unwrap().keys().collect::<Vec<_>>(),
        cache.versions.keys().collect::<Vec<_>>()
    );
    db.as_public_mut()
        .as_server_info_mut()
        .as_os_registry_mut()
        .ser(&"https://other.test".parse().unwrap())
        .unwrap();
    assert!(synced_os_versions(&db).unwrap().is_empty());
    db.as_public_mut()
        .as_server_info_mut()
        .as_os_registry_mut()
        .ser(&url())
        .unwrap();
    for status in [RegistryStatus::Connecting, RegistryStatus::Error] {
        set_status(&mut db, &Subscription::Os(url()), status, None).unwrap();
        assert_eq!(synced_os_versions(&db).unwrap().len(), 1);
    }
    let full = |os: OsIndex| FullIndex {
        os,
        ..super::tests::index()
    };
    for (os, publishes) in [(index, true), (OsIndex::default(), false)] {
        publish_snapshot(
            &mut db,
            &Subscription::Index(url()),
            &to_value(&full(os)).unwrap(),
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
        assert_eq!(cache.publishes_os_updates, publishes);
    }
}

fn revision(id: u64, patch: Value) -> patch_db::Revision {
    from_value(json!({ "id": id, "patch": patch })).unwrap()
}

#[test]
fn index_patches_name_the_packages_they_change() {
    let changed = |patch| changed_packages(&revision(1, patch).patch);
    assert_eq!(
        changed(json!([
            {"op": "replace", "path": "/package/packages/test/versions/1.0.0:0/releaseNotes", "value": "x"},
            {"op": "add", "path": "/os/versions/0.4.1", "value": {}},
        ])),
        Some(BTreeSet::from(["test".parse().unwrap()]))
    );
    assert_eq!(
        changed(json!([{"op": "add", "path": "/os/versions/0.4.1", "value": {}}])),
        Some(BTreeSet::new())
    );
    for path in ["/name", "/package/packages", "/package/categories/x", ""] {
        assert_eq!(
            changed(json!([{"op": "replace", "path": path, "value": null}])),
            None,
            "{path}"
        );
    }
}

#[test]
fn index_revisions_patch_the_saved_index_and_resume_only_from_the_same_view() {
    use connection::{SubscriptionEvent, SubscriptionStart};
    let mut db = database();
    let view = RegistryView::new(&device());
    let start = |epoch: Option<&str>, id, value, revisions| {
        SubscriptionEvent::Start(SubscriptionStart {
            epoch: epoch.map(Into::into),
            id,
            value,
            revisions,
        })
    };
    let rename = |id, name: &str| {
        revision(
            id,
            json!([{"op": "replace", "path": "/name", "value": name}]),
        )
    };
    let cursor = |db: &DatabaseModel| resumable_cursor(db, &url(), &view).unwrap();
    let name = |db: &DatabaseModel| {
        db.as_public()
            .as_registry_cache()
            .as_idx(&url())
            .unwrap()
            .de()
            .unwrap()
            .info
            .name
    };
    let ingest = |db: &mut DatabaseModel, event| ingest_event(db, &url(), &view, &event, &device());

    assert_eq!(cursor(&db), None);
    ingest(
        &mut db,
        start(Some("a"), 3, to_value(&index()).unwrap(), None),
    )
    .unwrap();
    assert_eq!(
        cursor(&db),
        Some(DbCursor {
            epoch: "a".into(),
            id: 3
        })
    );
    ingest(&mut db, SubscriptionEvent::Revision(rename(4, "Renamed"))).unwrap();
    assert_eq!(name(&db).as_deref(), Some("Renamed"));
    assert_eq!(cursor(&db).unwrap().id, 4);
    assert!(ingest(&mut db, SubscriptionEvent::Revision(rename(4, "Stale"))).is_err());

    ingest(
        &mut db,
        start(
            Some("a"),
            6,
            Value::Null,
            Some(vec![rename(5, "Resumed"), rename(6, "Again")]),
        ),
    )
    .unwrap();
    assert_eq!(name(&db).as_deref(), Some("Again"));
    assert_eq!(cursor(&db).unwrap().id, 6);
    assert!(
        ingest(
            &mut db,
            start(Some("b"), 7, Value::Null, Some(vec![rename(7, "Other")]))
        )
        .is_err()
    );
    assert_eq!(name(&db).as_deref(), Some("Again"));

    let mut localized = view.clone();
    localized.language = Some("de_DE".into());
    let mut upgraded = view.clone();
    upgraded.os_version = "0.4.0.6".parse().unwrap();
    for other in [localized, upgraded] {
        assert_eq!(resumable_cursor(&db, &url(), &other).unwrap(), None);
    }

    ingest(&mut db, start(None, 1, to_value(&index()).unwrap(), None)).unwrap();
    assert_eq!(cursor(&db), None);
    assert_eq!(name(&db).as_deref(), Some("Test Registry"));

    ingest(
        &mut db,
        start(Some("c"), 1, to_value(&index()).unwrap(), None),
    )
    .unwrap();
    db.as_public_mut()
        .as_server_info_mut()
        .mutate(|info| {
            info.registries.clear();
            Ok::<_, Error>(())
        })
        .unwrap();
    let desired = subscriptions(&db.as_public().as_server_info().de().unwrap());
    reconcile(&mut db, &desired, false).unwrap();
    assert!(
        db.as_private()
            .as_registry_cursors()
            .de()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn the_public_index_follows_live_replayed_and_snapshot_updates() {
    use connection::{SubscriptionEvent, SubscriptionStart};

    let mut db = database();
    let view = RegistryView::new(&device());
    let start = |id, value, revisions| {
        SubscriptionEvent::Start(SubscriptionStart {
            epoch: Some("details".into()),
            id,
            value,
            revisions,
        })
    };
    let ingest = |db: &mut DatabaseModel, event| ingest_event(db, &url(), &view, &event, &device());
    let long =
        |db: &DatabaseModel| {
            db.as_public().as_registry_indexes().as_idx(&url()).unwrap().as_value()["package"]
            ["packages"]["test"]["versions"]["2.0.0:0"]["description"]["long"]
            .clone()
        };
    let detail = |id, text| {
        revision(
            id,
            json!([{
                "op": "replace",
                "path": "/package/packages/test/versions/2.0.0:0/description/long",
                "value": text,
            }]),
        )
    };
    ingest(&mut db, start(1, to_value(&index()).unwrap(), None)).unwrap();
    assert_eq!(long(&db), json!("Test"));
    ingest(&mut db, SubscriptionEvent::Revision(detail(2, "Updated"))).unwrap();
    assert_eq!(long(&db), json!("Updated"));
    ingest(
        &mut db,
        start(3, Value::Null, Some(vec![detail(3, "Replayed")])),
    )
    .unwrap();
    assert_eq!(long(&db), json!("Replayed"));
    publish_snapshot(
        &mut db,
        &Subscription::Index(url()),
        &to_value(&index()).unwrap(),
        &device(),
    )
    .unwrap();
    assert_eq!(long(&db), json!("Test"));
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

fn install(db: &mut DatabaseModel, version: &str) {
    db.as_public_mut()
        .as_package_data_mut()
        .as_value_mut()
        .as_object_mut()
        .unwrap()
        .insert(
            "test".into(),
            json!({ "stateInfo": { "state": "installed", "manifest": {
                "id": "test", "title": "Test", "version": version
            }}}),
        );
}

fn update_target(db: &DatabaseModel) -> Option<String> {
    db.as_public()
        .as_registry_cache()
        .as_idx(&url())
        .unwrap()
        .de()
        .unwrap()
        .selections
        .get(&"test".parse::<crate::PackageId>().unwrap())
        .and_then(|package| package.update_target.as_ref())
        .map(ToString::to_string)
}

#[test]
fn update_targets_follow_the_installed_version_through_migration_and_os_ranges() {
    let mut db = database();
    let mut index = index();
    let mut newer_os: crate::registry::package::index::PackageVersionInfo = serde_json::from_value(
        serde_json::to_value(
            &index.package.packages[&"test".parse::<crate::PackageId>().unwrap()].versions
                [&"2.0.0:0".parse::<crate::util::VersionString>().unwrap()],
        )
        .unwrap(),
    )
    .unwrap();
    newer_os.metadata.os_version = "0.5.0".parse().unwrap();
    newer_os.source_version = None;
    index
        .package
        .packages
        .get_mut(&"test".parse::<crate::PackageId>().unwrap())
        .unwrap()
        .versions
        .insert("3.0.0:0".parse().unwrap(), newer_os);
    let mut device = device();
    device.os.compat = "<=0.4.0.5".parse().unwrap();
    publish_snapshot(
        &mut db,
        &Subscription::Index(url()),
        &to_value(&index).unwrap(),
        &device,
    )
    .unwrap();
    let cache = db
        .as_public()
        .as_registry_cache()
        .as_idx(&url())
        .unwrap()
        .de()
        .unwrap();
    let selected = &cache.selections["test"];
    let version = |v: &str| v.parse::<crate::util::VersionString>().unwrap();
    assert!(!selected.best.contains(&version("3.0.0:0")));
    assert!(selected.best.contains(&version("2.0.0:0")));
    assert!(!selected.versions[&version("3.0.0:0")].compatible);
    assert!(selected.versions[&version("2.0.0:0")].compatible);
    let mut strict: GetPackageParams =
        from_value(json!({"id":"test", "targetVersion":"=3.0.0:0"})).unwrap();
    strict.device_info = Some(device.clone());
    let listed = synced_packages(&db, &url(), &strict).unwrap();
    assert!(listed["best"]["3.0.0:0"].is_null());
    for (installed, target) in [
        ("0.9.0:0", Some("1.0.0:0")),
        ("1.0.0:0", None),
        ("1.6.0:0", Some("2.0.0:0")),
        ("#knots:1.6.0:0", None),
    ] {
        install(&mut db, installed);
        refresh_update_targets(&mut db, &device).unwrap();
        assert_eq!(update_target(&db).as_deref(), target, "{installed}");
    }
    db.as_public_mut()
        .as_package_data_mut()
        .ser(&Default::default())
        .unwrap();
    refresh_update_targets(&mut db, &device).unwrap();
    assert_eq!(update_target(&db), None);
}

mod vulnerabilities {
    use super::*;
    use crate::notifications::{NotificationType, VulnerabilityFound};
    use crate::registry::vulnerability::{SIG_CONTEXT, SignedVulnerability, Vulnerability};
    use crate::sign::{AnySigningKey, AnyVerifyingKey, Signer};

    fn key(seed: u8) -> (Signer, AnyVerifyingKey) {
        let key = ed25519_dalek::SigningKey::from_bytes(&[seed; 32]);
        let public = AnyVerifyingKey::Ed25519(key.verifying_key());
        (Signer::Key(AnySigningKey::Ed25519(key)), public)
    }

    async fn signed(seed: u8, subject: &str, id: &str, affected: &str) -> SignedVulnerability {
        let vulnerability = Vulnerability {
            affected: affected.parse().unwrap(),
            circumstances: "Exposed peer port".parse().unwrap(),
            impact: "Crash".parse().unwrap(),
            stop_recommended: true,
        };
        let (signer, public) = key(seed);
        let signature = signer
            .sign_commitment(&vulnerability.commitment(subject, id).unwrap(), SIG_CONTEXT)
            .await
            .unwrap();
        SignedVulnerability {
            vulnerability,
            published_at: chrono::Utc::now(),
            signatures: [(public, signature)].into(),
        }
    }

    fn trust(db: &mut DatabaseModel, seed: u8, marketplace: bool, os: bool) {
        let (_, public_key) = key(seed);
        db.as_public_mut()
            .as_server_info_mut()
            .as_trusted_keys_mut()
            .insert(
                &public_key.interned_der64(),
                &crate::trusted_keys::TrustedKey {
                    name: "Test".into(),
                    origin: "manual".into(),
                    origin_registry: None,
                    public_key,
                    gpg_exports: Default::default(),
                    contact: Default::default(),
                    added_at: chrono::Utc::now(),
                    removable: true,
                    marketplace,
                    os,
                },
            )
            .unwrap();
    }

    fn found(db: &DatabaseModel) -> Vec<VulnerabilityFound> {
        db.as_private()
            .as_notifications()
            .de()
            .unwrap()
            .0
            .into_values()
            .filter(|n| n.code == VulnerabilityFound::CODE)
            .map(|n| from_value(n.data).unwrap())
            .collect()
    }

    fn trusted_ids(db: &DatabaseModel) -> Vec<String> {
        db.as_public()
            .as_registry_cache()
            .as_idx(&url())
            .unwrap()
            .de()
            .unwrap()
            .selections[&"test".parse::<crate::PackageId>().unwrap()]
            .vulnerabilities
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[tokio::test]
    async fn only_trusted_package_vulnerabilities_are_selected_and_announced_once() {
        let mut db = database();
        let mut index = index();
        let package = index
            .package
            .packages
            .get_mut(&"test".parse::<crate::PackageId>().unwrap())
            .unwrap();
        package
            .vulnerabilities
            .insert("CVE-1".into(), signed(7, "test", "CVE-1", "<2.0.0:0").await);
        package
            .vulnerabilities
            .insert("CVE-2".into(), signed(8, "test", "CVE-2", "<2.0.0:0").await);
        package.vulnerabilities.insert(
            "CVE-3".into(),
            signed(7, "other", "CVE-3", "<2.0.0:0").await,
        );
        publish_snapshot(
            &mut db,
            &Subscription::Index(url()),
            &to_value(&index).unwrap(),
            &device(),
        )
        .unwrap();
        assert!(trusted_ids(&db).is_empty());
        install(&mut db, "1.0.0:0");
        trust(&mut db, 7, true, false);
        refresh_trusted_vulnerabilities(&mut db, &device()).unwrap();
        assert_eq!(trusted_ids(&db), ["CVE-1"]);
        let announced = found(&db);
        assert_eq!(announced.len(), 1);
        assert_eq!(announced[0].vulnerability_id, "CVE-1");
        assert_eq!(announced[0].package_id.as_deref(), Some("test"));
        assert!(announced[0].stop_recommended);
        refresh_trusted_vulnerabilities(&mut db, &device()).unwrap();
        assert_eq!(found(&db).len(), 1);
        trust(&mut db, 7, false, true);
        refresh_trusted_vulnerabilities(&mut db, &device()).unwrap();
        assert!(trusted_ids(&db).is_empty());
    }

    #[tokio::test]
    async fn trusted_advisories_survive_an_empty_release_selection() {
        let mut db = database();
        let mut index = index();
        let id: crate::PackageId = "test".parse().unwrap();
        let package = index.package.packages.get_mut(&id).unwrap();
        package.versions.clear();
        package
            .vulnerabilities
            .insert("CVE-1".into(), signed(7, "test", "CVE-1", "<2.0.0:0").await);
        publish_snapshot(
            &mut db,
            &Subscription::Index(url()),
            &to_value(&index).unwrap(),
            &device(),
        )
        .unwrap();
        install(&mut db, "1.0.0:0");
        trust(&mut db, 7, true, false);
        refresh_trusted_vulnerabilities(&mut db, &device()).unwrap();
        assert_eq!(trusted_ids(&db), ["CVE-1"]);
        let selection = db
            .as_public()
            .as_registry_cache()
            .as_idx(&url())
            .unwrap()
            .as_selections()
            .as_idx(&id)
            .unwrap()
            .de()
            .unwrap();
        assert!(selection.best.is_empty());
        assert!(selection.versions.is_empty());
        assert_eq!(found(&db).len(), 1);
        refresh_update_targets(&mut db, &device()).unwrap();
        assert_eq!(trusted_ids(&db), ["CVE-1"]);
        assert_eq!(found(&db).len(), 1);
        trust(&mut db, 7, false, true);
        refresh_trusted_vulnerabilities(&mut db, &device()).unwrap();
        assert!(
            db.as_public()
                .as_registry_cache()
                .as_idx(&url())
                .unwrap()
                .as_selections()
                .as_idx(&id)
                .is_none()
        );
    }

    #[tokio::test]
    async fn os_vulnerabilities_need_os_trust_and_announce_for_the_running_version() {
        let mut db = database();
        let mut index = OsIndex::default();
        index.vulnerabilities.insert(
            "OS-1".into(),
            signed(
                7,
                crate::registry::vulnerability::OS_SUBJECT,
                "OS-1",
                ">=0.4.0:0",
            )
            .await,
        );
        index.vulnerabilities.insert(
            "OS-2".into(),
            signed(
                7,
                crate::registry::vulnerability::OS_SUBJECT,
                "OS-2",
                "<0.4.0:0",
            )
            .await,
        );
        trust(&mut db, 7, true, false);
        publish_snapshot(
            &mut db,
            &Subscription::Os(url()),
            &to_value(&index).unwrap(),
            &device(),
        )
        .unwrap();
        let cache = db.as_public().as_os_update_cache().de().unwrap().unwrap();
        assert!(cache.trusted_vulnerabilities.is_empty());
        assert!(found(&db).is_empty());
        trust(&mut db, 7, false, true);
        refresh_trusted_vulnerabilities(&mut db, &device()).unwrap();
        let cache = db.as_public().as_os_update_cache().de().unwrap().unwrap();
        assert_eq!(cache.trusted_vulnerabilities.len(), 2);
        let announced = found(&db);
        assert_eq!(announced.len(), 1);
        assert_eq!(announced[0].vulnerability_id, "OS-1");
        assert!(announced[0].package_id.is_none());
    }
}
