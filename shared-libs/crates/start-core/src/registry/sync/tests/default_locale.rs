use super::*;

fn cached_locale(db: &DatabaseModel) -> Option<InternedString> {
    db.as_public()
        .as_os_update_cache()
        .de()
        .unwrap()
        .unwrap()
        .default_locale
}

fn publish_index(db: &mut DatabaseModel, index: &FullIndex) {
    publish_snapshot(
        db,
        &Subscription::Index(url()),
        &to_ord_value(index).unwrap(),
        &device(),
    )
    .unwrap();
}

#[test]
fn os_default_locale_follows_registry_only_changes_in_either_snapshot_order() {
    for order in [
        [Subscription::Index(url()), Subscription::Os(url())],
        [Subscription::Os(url()), Subscription::Index(url())],
    ] {
        let mut db = database();
        let mut index = index();
        index.default_locale = Some("pl_PL".into());
        let full = to_ord_value(&index).unwrap();
        let os = to_ord_value(&index.os).unwrap();
        for subscription in order {
            let value = match &subscription {
                Subscription::Index(_) => &full,
                Subscription::Os(_) => &os,
            };
            publish_snapshot(&mut db, &subscription, value, &device()).unwrap();
        }
        assert_eq!(cached_locale(&db).as_deref(), Some("pl_PL"));

        for locale in [Some("de_DE"), None] {
            index.default_locale = locale.map(Into::into);
            publish_index(&mut db, &index);
            assert_eq!(cached_locale(&db).as_deref(), locale);
        }
    }
}

#[test]
fn disconnecting_the_package_registry_clears_the_os_default_locale() {
    let mut db = database();
    let mut index = index();
    index.default_locale = Some("pl_PL".into());
    publish_index(&mut db, &index);
    publish_snapshot(
        &mut db,
        &Subscription::Os(url()),
        &to_ord_value(&index.os).unwrap(),
        &device(),
    )
    .unwrap();
    assert_eq!(cached_locale(&db).as_deref(), Some("pl_PL"));

    db.as_public_mut()
        .as_server_info_mut()
        .as_registries_mut()
        .ser(&crate::trusted_keys::Registries::new())
        .unwrap();
    let desired = subscriptions(&db.as_public().as_server_info().de().unwrap());
    reconcile(&mut db, &desired, false).unwrap();
    assert_eq!(cached_locale(&db), None);
}

#[test]
fn connecting_os_cache_uses_already_cached_registry_metadata() {
    let mut db = database();
    let mut index = index();
    index.default_locale = Some("pl_PL".into());
    publish_index(&mut db, &index);
    set_status(
        &mut db,
        &Subscription::Os(url()),
        RegistryStatus::Connecting,
        None,
    )
    .unwrap();
    assert_eq!(cached_locale(&db).as_deref(), Some("pl_PL"));
}
