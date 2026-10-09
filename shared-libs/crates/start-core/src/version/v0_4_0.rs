use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0_alpha_0, v0_4_0_beta_10};
use crate::context::RpcContext;
use crate::db::model::Database;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;

lazy_static::lazy_static! {
    static ref V0_4_0: exver::Version = exver::Version::new([0, 4, 0], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0_beta_10::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_0.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    /// Whether this run arrived from a 0.4.0 pre-release below beta.10.
    #[instrument(skip_all)]
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        Ok(Value::Bool(
            !migrated_from_pre_0_4_0(db) && migrated_through_beta_10(db),
        ))
    }
    async fn post_up(self, ctx: &RpcContext, input: Value) -> Result<(), Error> {
        welcome_beta_arrival(&ctx.db, input).await
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        Ok(())
    }
}

async fn welcome_beta_arrival(db: &TypedPatchDb<Database>, input: Value) -> Result<(), Error> {
    if input.as_bool().unwrap_or(false) {
        let highlights = include_str!("update_details/v0_4_0_highlights.md").to_string();
        db.mutate(|db| {
            notify(
                db,
                None,
                NotificationLevel::Success,
                "Welcome to stable StartOS 0.4.0!".to_string(),
                "Click \"View Details\" for the highlights — including important changes to backups and sign-in.".to_string(),
                highlights,
            )?;
            Ok(())
        })
        .await
        .result?;
    }
    Ok(())
}

/// Must run before `post_init` drains `postInitMigrationTodos`.
fn migrated_from_pre_0_4_0(db: &Value) -> bool {
    let floor = v0_4_0_alpha_0::Version.semver();
    db["public"]["serverInfo"]["postInitMigrationTodos"]
        .as_object()
        .into_iter()
        .flat_map(|todos| todos.iter())
        .filter_map(|(k, _)| (&**k).parse::<exver::Version>().ok())
        .any(|v| v <= floor)
}

/// Must run before `post_init` drains `postInitMigrationTodos`.
fn migrated_through_beta_10(db: &Value) -> bool {
    let beta_10 = v0_4_0_beta_10::Version.semver();
    db["public"]["serverInfo"]["postInitMigrationTodos"]
        .as_object()
        .into_iter()
        .flat_map(|todos| todos.iter())
        .filter_map(|(k, _)| (&**k).parse::<exver::Version>().ok())
        .any(|v| v == beta_10)
}

#[cfg(test)]
mod test {
    use imbl_value::json;

    use super::*;

    #[tokio::test]
    async fn welcome_notification_survives_migration_to_0_4_1() {
        use super::super::{Current, PreUps, migrate_from_unchecked};

        for (from, todos, eligible) in [
            (
                super::super::v0_4_0_beta_9::Version.semver(),
                json!({}),
                true,
            ),
            (v0_4_0_beta_10::Version.semver(), json!({}), false),
            (
                super::super::v0_4_0_beta_9::Version.semver(),
                json!({ "0.4.0-alpha.0": null }),
                false,
            ),
        ] {
            let previous = super::super::Version::from_exver_version(from.clone())
                .as_version_t()
                .unwrap();
            let current = Current::default();
            let mut db = json!({ "public": { "serverInfo": {
                "version": from.to_string(),
                "packageVersionCompat": ">=0.3.0 <0.5.0",
                "caFingerprint": "AB:CD",
                "postInitMigrationTodos": todos,
                "unreadNotificationCount": 0,
            } }, "private": { "notifications": {} } });
            let pre_ups = PreUps::load(&previous, &current).await.unwrap();
            migrate_from_unchecked(&previous, &current, pre_ups, &mut db).unwrap();
            assert_eq!(db["public"]["serverInfo"]["version"], json!("0.4.1"));
            assert_eq!(
                db["public"]["serverInfo"]["postInitMigrationTodos"]["0.4.0"],
                json!(eligible),
                "arrival from {from} with pending {todos}",
            );
            let input = db["public"]["serverInfo"]["postInitMigrationTodos"]["0.4.0"].clone();
            let temp = tempfile::tempdir().unwrap();
            let store = PatchDb::open(temp.path().join("db")).await.unwrap();
            store
                .apply_function(|_| Ok::<_, Error>((db.clone(), ())))
                .await
                .result
                .unwrap();
            let store = TypedPatchDb::<Database>::load_unchecked(store);
            welcome_beta_arrival(&store, input).await.unwrap();
            let snapshot = store.peek().await;
            let notifications = snapshot.as_private().as_notifications().de().unwrap();
            assert_eq!(notifications.0.len(), usize::from(eligible));
            assert_eq!(
                snapshot
                    .as_public()
                    .as_server_info()
                    .as_unread_notification_count()
                    .de()
                    .unwrap(),
                u64::from(eligible),
            );
            if eligible {
                let notification = notifications.0.get(&0).unwrap();
                assert_eq!(notification.package_id, None);
                assert_eq!(notification.level, NotificationLevel::Success);
                assert_eq!(notification.title, "Welcome to stable StartOS 0.4.0!");
                assert_eq!(
                    notification.message,
                    "Click \"View Details\" for the highlights — including important changes to backups and sign-in."
                );
                assert_eq!(notification.code, 2);
                assert_eq!(
                    notification.data,
                    json!(include_str!("update_details/v0_4_0_highlights.md"))
                );
                assert!(!notification.seen);
            } else {
                assert_eq!(to_value(&snapshot).unwrap(), db);
            }
        }
    }

    #[test]
    fn welcome_routing() {
        let todos = |v| json!({ "public": { "serverInfo": { "postInitMigrationTodos": v } } });

        assert!(!migrated_from_pre_0_4_0(&todos(json!({}))));
        assert!(!migrated_from_pre_0_4_0(
            &json!({ "public": { "serverInfo": {} } })
        ));
        assert!(!migrated_from_pre_0_4_0(&todos(
            json!({ "0.4.0-alpha.6": null, "0.4.0-beta.9": null })
        )));
        assert!(migrated_from_pre_0_4_0(&todos(
            json!({ "0.3.5.2": null, "0.4.0-alpha.0": null })
        )));
        assert!(migrated_from_pre_0_4_0(&todos(
            json!({ "0.4.0-alpha.0": null, "0.4.0-alpha.1": null })
        )));

        assert!(!migrated_through_beta_10(&todos(json!({}))));
        assert!(!migrated_through_beta_10(
            &json!({ "public": { "serverInfo": {} } })
        ));
        assert!(migrated_through_beta_10(&todos(
            json!({ "0.4.0-beta.10": null })
        )));

        let from_beta_9 = todos(json!({ "0.4.0-beta.10": null }));
        assert!(!migrated_from_pre_0_4_0(&from_beta_9) && migrated_through_beta_10(&from_beta_9));
        let from_0_3_x = todos(
            json!({ "0.3.5.2": null, "0.4.0-alpha.0": null, "0.4.0-beta.9": null, "0.4.0-beta.10": null }),
        );
        assert!(migrated_from_pre_0_4_0(&from_0_3_x));
    }
}
