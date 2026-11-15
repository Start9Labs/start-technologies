use std::collections::{BTreeMap, BTreeSet};

use exver::{Version, VersionRange};
use imbl_value::InternedString;
use url::Url;

use crate::db::model::DatabaseModel;
use crate::notifications::{NotificationLevel, OsUpdateAvailable, notify};
use crate::prelude::*;
use crate::registry::os::index::OsIndex;
use crate::s9pk::git_hash::GitHash;

pub type SecurityNotifications = BTreeMap<Url, BTreeMap<Version, Option<GitHash>>>;

#[derive(Default)]
struct SecurityFeedState {
    name: Option<String>,
    updates: BTreeMap<Version, SecurityUpdate>,
}

struct SecurityUpdate {
    git_hash: Option<GitHash>,
    source_version: VersionRange,
    platforms: BTreeSet<InternedString>,
}

pub(crate) fn notify_updates(
    db: &mut DatabaseModel,
    registry: &Url,
    name: Option<String>,
    index: OsIndex,
) -> Result<(), Error> {
    let feed = SecurityFeedState {
        name,
        updates: index
            .versions
            .0
            .into_iter()
            .filter(|(_, info)| info.security)
            .map(|(version, info)| {
                (
                    version,
                    SecurityUpdate {
                        git_hash: info.git_hash,
                        source_version: info.source_version,
                        platforms: info.squashfs.into_keys().collect(),
                    },
                )
            })
            .collect(),
    };
    notify_security_updates(
        db,
        registry,
        &feed,
        crate::version::COMMIT_HASH.trim(),
        &crate::PLATFORM,
    )
}

fn notify_security_updates(
    db: &mut DatabaseModel,
    registry: &Url,
    feed: &SecurityFeedState,
    installed_hash: &str,
    platform: &str,
) -> Result<(), Error> {
    let installed = db.as_public().as_server_info().as_version().de()?;
    let mut notified = db.as_private().as_os_security_notifications().de()?;
    let releases = notified.entry(registry.clone()).or_default();
    let registry_name = feed
        .name
        .clone()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| t!("update.security.selected-registry").to_string());
    for (version, update) in &feed.updates {
        if !update.platforms.contains(platform)
            || !installed.satisfies(&update.source_version)
            || !super::is_os_update(
                version,
                update.git_hash.as_ref(),
                &installed,
                installed_hash,
            )
            || releases.get(version) == Some(&update.git_hash)
        {
            continue;
        }
        notify(
            db,
            None,
            NotificationLevel::Warning,
            t!("update.security.available-title").to_string(),
            t!(
                "update.security.available-message",
                version = version.to_string(),
                registry = registry_name.as_str()
            )
            .to_string(),
            OsUpdateAvailable {},
        )?;
        releases.insert(version.clone(), update.git_hash.clone());
    }
    db.as_private_mut()
        .as_os_security_notifications_mut()
        .ser(&notified)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use imbl_value::json;
    use patch_db::ModelExt;

    use super::*;

    fn database() -> DatabaseModel {
        let mut db = DatabaseModel::from_value(json!({
            "public": { "serverInfo": { "version": "0.4.0.5", "unreadNotificationCount": 0 } },
            "private": { "notifications": {}, "osSecurityNotifications": {} }
        }));
        db.as_private_mut()
            .as_push_mut()
            .ser(&crate::notifications::push::PushState::default())
            .unwrap();
        db
    }

    fn catalog(
        version: &str,
        hash: Option<&str>,
        source: &str,
        platform: &str,
    ) -> SecurityFeedState {
        SecurityFeedState {
            name: Some("Test Registry".into()),
            updates: [(
                version.parse().unwrap(),
                SecurityUpdate {
                    git_hash: hash.map(|h| h.parse().unwrap()),
                    source_version: source.parse().unwrap(),
                    platforms: BTreeSet::from([platform.into()]),
                },
            )]
            .into_iter()
            .collect(),
        }
    }

    fn count(db: &DatabaseModel) -> usize {
        db.as_private().as_notifications().de().unwrap().0.len()
    }

    #[test]
    fn registry_names_are_display_only_and_renames_do_not_repeat_notifications() {
        let mut db = database();
        let registry: Url = "https://registry.test/".parse().unwrap();
        let other: Url = "https://other-registry.test/".parse().unwrap();
        let mut feed = catalog("0.4.0.6", Some("new-build"), "*", "x86_64");
        notify_security_updates(&mut db, &registry, &feed, "installed-build", "x86_64").unwrap();
        let notifications = db.as_private().as_notifications().de().unwrap();
        let message = &notifications.0.values().next().unwrap().message;
        assert!(message.contains("Test Registry"));
        assert!(!message.contains(registry.as_str()));
        notify_security_updates(&mut db, &other, &feed, "installed-build", "x86_64").unwrap();
        assert_eq!(count(&db), 2);
        feed.name = Some("Renamed Registry".into());
        notify_security_updates(&mut db, &registry, &feed, "installed-build", "x86_64").unwrap();
        assert_eq!(count(&db), 2);
        feed.updates.values_mut().next().unwrap().git_hash =
            Some("replacement-build".parse().unwrap());
        notify_security_updates(&mut db, &registry, &feed, "installed-build", "x86_64").unwrap();
        let notifications = db.as_private().as_notifications().de().unwrap();
        assert_eq!(notifications.0.len(), 3);
        assert!(
            notifications
                .0
                .values()
                .last()
                .unwrap()
                .message
                .contains("Renamed Registry")
        );
    }

    #[test]
    fn unnamed_registries_use_a_localized_label_without_exposing_the_url() {
        let registry: Url = "https://registry.test/".parse().unwrap();
        for name in [None, Some(String::new())] {
            let mut db = database();
            let mut feed = catalog("0.4.0.6", Some("new-build"), "*", "x86_64");
            feed.name = name;
            notify_security_updates(&mut db, &registry, &feed, "installed-build", "x86_64")
                .unwrap();
            let notifications = db.as_private().as_notifications().de().unwrap();
            let message = &notifications.0.values().next().unwrap().message;
            assert!(message.contains(&t!("update.security.selected-registry").to_string()));
            assert!(!message.contains(registry.as_str()));
        }
    }

    #[test]
    fn notifies_once_across_reconnect_restart_and_notification_deletion() {
        let mut db = database();
        let registry: Url = "https://registry.test".parse().unwrap();
        let update = catalog("0.4.0.6", Some("new-build"), ">=0.4.0", "x86_64");
        notify_security_updates(&mut db, &registry, &update, "installed-build", "x86_64").unwrap();
        assert_eq!(count(&db), 1);
        assert_eq!(
            db.as_public()
                .as_server_info()
                .as_unread_notification_count()
                .de()
                .unwrap(),
            1
        );
        let notifications = db.as_private().as_notifications().de().unwrap();
        let notification = notifications.0.values().next().unwrap();
        assert_eq!(
            notification.code,
            <OsUpdateAvailable as crate::notifications::NotificationType>::CODE
        );
        assert_eq!(notification.data, json!({ "kind": "os-update-available" }));
        db = Model::from_value(db.as_value().clone());
        notify_security_updates(&mut db, &registry, &update, "installed-build", "x86_64").unwrap();
        assert_eq!(count(&db), 1);
        db.as_private_mut()
            .as_notifications_mut()
            .ser(&crate::notifications::Notifications::new())
            .unwrap();
        notify_security_updates(&mut db, &registry, &update, "installed-build", "x86_64").unwrap();
        assert_eq!(count(&db), 0);
    }

    #[test]
    fn security_notifications_enqueue_browser_push_once() {
        use base64::Engine;
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use web_push_native::p256;
        use web_push_native::p256::elliptic_curve::sec1::ToEncodedPoint;

        use crate::notifications::push::{PushKeys, PushState, PushSubscription};

        let mut db = database();
        let key = p256::SecretKey::from_slice(&[19; 32]).unwrap();
        let subscription = PushSubscription {
            endpoint: "https://push.example.com/subscription".into(),
            keys: PushKeys {
                p256dh: URL_SAFE_NO_PAD.encode(key.public_key().to_encoded_point(false).as_bytes()),
                auth: URL_SAFE_NO_PAD.encode([29u8; 16]),
            },
        };
        let mut state = to_value(&PushState::default()).unwrap();
        state["vapidPrivateKey"] =
            to_value(&josekit::jwk::Jwk::generate_ec_key(josekit::jwk::P_256).unwrap()).unwrap();
        state["subscriptions"] =
            json!([{ "id": crate::util::new_guid().to_string(), "subscription": subscription }]);
        db.as_private_mut()
            .as_push_mut()
            .ser(&from_value::<PushState>(state).unwrap())
            .unwrap();
        let registry: Url = "https://registry.test/".parse().unwrap();
        let update = catalog("0.4.0.6", Some("new-build"), "*", "x86_64");
        notify_security_updates(&mut db, &registry, &update, "installed-build", "x86_64").unwrap();
        db = Model::from_value(db.as_value().clone());
        notify_security_updates(&mut db, &registry, &update, "installed-build", "x86_64").unwrap();
        assert_eq!(count(&db), 1);
        let pending = db.as_private().as_push().as_value()["pending"]
            .as_array()
            .unwrap();
        assert_eq!(pending.len(), 1);
        let payload: serde_json::Value =
            serde_json::from_str(pending[0]["payload"].as_str().unwrap()).unwrap();
        let notification = db
            .as_private()
            .as_notifications()
            .de()
            .unwrap()
            .0
            .into_values()
            .next()
            .unwrap();
        assert_eq!(
            payload["notification"]["title"].as_str(),
            Some(notification.title.as_str())
        );
        assert_eq!(
            payload["notification"]["body"].as_str(),
            Some(notification.message.as_str())
        );
    }

    #[test]
    fn filters_installed_older_incompatible_and_other_platform_updates() {
        let registry: Url = "https://registry.test".parse().unwrap();
        for update in [
            catalog("0.4.0.4", Some("older-build"), "*", "x86_64"),
            catalog("0.4.0.5", Some("installed-build"), "*", "x86_64"),
            catalog("0.4.0.5", None, "*", "x86_64"),
            catalog("0.4.0.6", Some("new-build"), ">=0.5.0", "x86_64"),
            catalog("0.4.0.6", Some("new-build"), "*", "aarch64"),
        ] {
            let mut db = database();
            notify_security_updates(&mut db, &registry, &update, "installed-build", "x86_64")
                .unwrap();
            assert_eq!(count(&db), 0);
        }
    }

    #[test]
    fn replacement_builds_and_new_registry_subscriptions_get_notified() {
        let mut db = database();
        let registry: Url = "https://registry.test".parse().unwrap();
        let first = catalog("0.4.0.5", Some("rc-one"), "*", "x86_64");
        let second = catalog("0.4.0.5", Some("rc-two"), "*", "x86_64");
        notify_security_updates(&mut db, &registry, &first, "installed-build", "x86_64").unwrap();
        notify_security_updates(&mut db, &registry, &second, "installed-build", "x86_64").unwrap();
        notify_security_updates(&mut db, &registry, &second, "installed-build", "x86_64").unwrap();
        assert_eq!(count(&db), 2);
        let other: Url = "https://other.test".parse().unwrap();
        notify_security_updates(&mut db, &other, &second, "installed-build", "x86_64").unwrap();
        assert_eq!(count(&db), 3);
    }
}
