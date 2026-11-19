use super::*;

#[test]
fn failure_details_follow_attempts_without_discarding_cached_catalogs() {
    let mut db = database();
    let package = Subscription::Index(url());
    let os = Subscription::Os(url());
    publish_snapshot(&mut db, &package, &to_value(&index()).unwrap(), &device()).unwrap();
    publish_snapshot(
        &mut db,
        &os,
        &to_value(&OsIndex::default()).unwrap(),
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
    let versions = db
        .as_public()
        .as_os_update_cache()
        .de()
        .unwrap()
        .unwrap()
        .versions;
    assert!(!packages.is_empty());

    for message in ["TLS certificate verification failed", "DNS lookup failed"] {
        for subscription in [&package, &os] {
            set_status(&mut db, subscription, RegistryStatus::Error, Some(message)).unwrap();
        }
        let cache = db
            .as_public()
            .as_registry_cache()
            .as_idx(&url())
            .unwrap()
            .de()
            .unwrap();
        assert_eq!(cache.status, RegistryStatus::Error);
        assert_eq!(cache.error.as_deref(), Some(message));
        assert_eq!(
            to_value(&cache.packages).unwrap(),
            to_value(&packages).unwrap()
        );
        let os_cache = db.as_public().as_os_update_cache().de().unwrap().unwrap();
        assert_eq!(os_cache.status, RegistryStatus::Error);
        assert_eq!(os_cache.error.as_deref(), Some(message));
        assert_eq!(
            to_value(&os_cache.versions).unwrap(),
            to_value(&versions).unwrap()
        );

        for subscription in [&package, &os] {
            set_status(&mut db, subscription, RegistryStatus::Connecting, None).unwrap();
        }
        let cache = db
            .as_public()
            .as_registry_cache()
            .as_idx(&url())
            .unwrap()
            .de()
            .unwrap();
        assert_eq!(cache.status, RegistryStatus::Connecting);
        assert!(cache.error.is_none());
        assert_eq!(
            to_value(&cache.packages).unwrap(),
            to_value(&packages).unwrap()
        );
        assert!(
            db.as_public()
                .as_os_update_cache()
                .de()
                .unwrap()
                .unwrap()
                .error
                .is_none()
        );
    }
    for subscription in [&package, &os] {
        set_status(
            &mut db,
            subscription,
            RegistryStatus::Error,
            Some("remote failure"),
        )
        .unwrap();
    }
    publish_snapshot(&mut db, &package, &to_value(&index()).unwrap(), &device()).unwrap();
    publish_snapshot(
        &mut db,
        &os,
        &to_value(&OsIndex::default()).unwrap(),
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
    assert!(cache.error.is_none());
    let cache = db.as_public().as_os_update_cache().de().unwrap().unwrap();
    assert_eq!(cache.status, RegistryStatus::Connected);
    assert!(cache.error.is_none());
}

#[test]
fn first_failure_retains_details_and_legacy_snapshots_without_details_still_load() {
    let mut db = database();
    set_status(
        &mut db,
        &Subscription::Index(url()),
        RegistryStatus::Error,
        Some("Connection refused"),
    )
    .unwrap();
    let cache = db
        .as_public()
        .as_registry_cache()
        .as_idx(&url())
        .unwrap()
        .de()
        .unwrap();
    assert_eq!(cache.error.as_deref(), Some("Connection refused"));
    assert!(cache.packages.is_empty());
    let cache: RegistryCache =
        from_value(json!({"info": RegistryInfo::default(), "packages": {}, "status": "error"}))
            .unwrap();
    assert_eq!(cache.status, RegistryStatus::Error);
    assert!(cache.error.is_none());
    let cache: OsUpdateCache =
        from_value(json!({"registry": url(), "versions": {}, "status": "error"})).unwrap();
    assert!(cache.error.is_none());
}
