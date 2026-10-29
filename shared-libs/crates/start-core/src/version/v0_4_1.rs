use exver::VersionRange;
use imbl_value::json;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0_5};
use crate::prelude::*;
use crate::trusted_keys::{migrate_signers, os_signers};

lazy_static::lazy_static! {
    static ref V0_4_1: exver::Version = exver::Version::new([0, 4, 1], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0_5::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_1.clone()
    }
    fn migration_revision(self) -> usize {
        1
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        let server = db
            .get_mut("public")
            .and_then(|public| public.get_mut("serverInfo"))
            .and_then(Value::as_object_mut)
            .or_not_found("serverInfo")?;
        if !server.contains_key("trustedKeys") {
            let legacy = server
                .get("trustedOsUpdateSigners")
                .cloned()
                .map(from_value)
                .transpose()?
                .unwrap_or_default();
            server.insert("trustedKeys".into(), to_value(&migrate_signers(legacy)?)?);
        }
        server.remove("trustedOsUpdateSigners");
        let private = db["private"].as_object_mut().or_not_found("private")?;
        if !private.contains_key("sessions") {
            let pubkeys = private
                .remove("sessionPubkeys")
                .unwrap_or_else(|| json!({}));
            private.insert(
                "sessions".into(),
                json!({ "pubkeys": pubkeys, "tokens": {} }),
            );
        }
        Ok(Value::Null)
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
        let server = db
            .get_mut("public")
            .and_then(|public| public.get_mut("serverInfo"))
            .and_then(Value::as_object_mut)
            .or_not_found("serverInfo")?;
        if let Some(keys) = server.get("trustedKeys") {
            let signers = to_value(&os_signers(&from_value(keys.clone())?))?;
            server.remove("trustedKeys");
            server.insert("trustedOsUpdateSigners".into(), signers);
        }
        let private = db["private"].as_object_mut().or_not_found("private")?;
        let pubkeys = private
            .remove("sessions")
            .map(|sessions| sessions["pubkeys"].clone())
            .unwrap_or_else(|| json!({}));
        private.insert("sessionPubkeys".into(), pubkeys);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trusted_keys::TrustedKeys;
    use crate::update::signers::{TrustedOsUpdateSigners, initial_signers};
    use crate::version::{Current, Upgrade, v0_4_0_3};

    #[test]
    fn malformed_legacy_keys_fail_without_changing_saved_trust() {
        let mut db = json!({
            "private": { "sessionPubkeys": {} },
            "public": { "serverInfo": { "trustedOsUpdateSigners": {
                "not a public key": { "label": "custom" },
            } } },
        });
        let saved = db.clone();
        assert!(Version.up(&mut db, ()).is_err());
        assert_eq!(db, saved);
    }

    #[test]
    fn migration_is_idempotent_and_downgrade_preserves_os_policy() {
        let legacy = initial_signers();
        let mut db = json!({
            "private": { "sessionPubkeys": {} },
            "public": { "serverInfo": { "trustedOsUpdateSigners": legacy } },
        });
        Version.up(&mut db, ()).unwrap();
        let migrated = db.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(db, migrated);
        assert!(
            db["public"]["serverInfo"]
                .get("trustedOsUpdateSigners")
                .is_none()
        );
        Version.down(&mut db).unwrap();
        let restored: TrustedOsUpdateSigners =
            from_value(db["public"]["serverInfo"]["trustedOsUpdateSigners"].clone()).unwrap();
        assert_eq!(
            restored.into_keys().collect::<Vec<_>>(),
            legacy.into_keys().collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn upgrade_from_0_4_0_3_reruns_its_stale_migration() {
        let initial = to_value(&initial_signers()).unwrap();
        let mut custom = initial_signers();
        custom.pop_first();
        custom.values_mut().next().unwrap().label = "Owner-managed".to_owned();
        let custom = to_value(&custom).unwrap();
        for (revision, saved, expected) in [
            (2, None, initial),
            (3, Some(json!({})), json!({})),
            (3, Some(custom.clone()), custom),
        ] {
            let previous = v0_4_0_3::Version;
            let mut db = json!({
                "private": { "sessionPubkeys": {} },
                "public": { "serverInfo": {
                    "version": previous.semver(),
                    "packageVersionCompat": "*",
                    "latestMigrationRevision": revision,
                    "postInitMigrationTodos": {},
                } },
            });
            if let Some(saved) = saved {
                db["public"]["serverInfo"]["trustedOsUpdateSigners"] = saved;
            }
            Upgrade::load(&previous, &Version, revision)
                .await
                .unwrap()
                .apply(&previous, &Version, &mut db)
                .unwrap();
            let keyrings: TrustedKeys =
                from_value(db["public"]["serverInfo"]["trustedKeys"].clone()).unwrap();
            let actual = os_signers(&keyrings);
            let expected: TrustedOsUpdateSigners = from_value(expected).unwrap();
            assert_eq!(
                actual.keys().collect::<Vec<_>>(),
                expected.keys().collect::<Vec<_>>()
            );
            for (key, signer) in expected {
                if signer.label == "Owner-managed" {
                    assert_eq!(actual[&key].label, signer.label);
                }
            }
            assert!(
                db["public"]["serverInfo"]
                    .get("trustedOsUpdateSigners")
                    .is_none()
            );
            assert_eq!(db["public"]["serverInfo"]["version"], json!("0.4.1"));
        }
    }

    #[test]
    fn sessions_split_by_credential_and_roll_back() {
        let pubkeys = json!({ "-----BEGIN PUBLIC KEY-----\nkey\n-----END PUBLIC KEY-----\n": {
            "name": null,
            "loggedIn": "2026-01-01T00:00:00Z",
            "lastActive": "2026-01-02T00:00:00Z",
            "userAgent": "agent",
        } });
        let original = json!({
            "public": { "serverInfo": { "trustedOsUpdateSigners": {} } },
            "private": { "password": "hash", "sessionPubkeys": pubkeys.clone() },
        });
        let mut db = original.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(
            db["private"],
            json!({
                "password": "hash", "sessions": { "pubkeys": pubkeys, "tokens": {} },
            })
        );
        db["private"]["sessions"]["tokens"] = json!({ "hash": {} });
        let migrated = db.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(db, migrated);
        Version.down(&mut db).unwrap();
        assert_eq!(db, original);
    }

    #[tokio::test]
    async fn existing_0_4_1_tokens_and_custom_os_trust_survive_the_new_revision() {
        use patch_db::json_ptr::ROOT;

        let account =
            crate::account::AccountInfo::new("password", std::time::SystemTime::now(), None)
                .unwrap();
        let key = "MCowBQYDK2VwAyEAGaAYxsY7iO5GyQrCA5vcLWYIQyoKImp0du4WWvIzr2Y=";
        for legacy in [
            TrustedOsUpdateSigners::new(),
            from_value(json!({ key: { "label": "Owner-managed" } })).unwrap(),
        ] {
            let mut fixture =
                to_value(&crate::db::model::Database::init(&account, false, None, None).unwrap())
                    .unwrap();
            let server = fixture["public"]["serverInfo"].as_object_mut().unwrap();
            server.remove("trustedKeys");
            server.insert("trustedOsUpdateSigners".into(), to_value(&legacy).unwrap());
            server.insert("latestMigrationRevision".into(), json!(0));
            server.insert("postInitMigrationTodos".into(), json!({}));
            fixture["private"]["sessions"]["tokens"] =
                json!({ "a".repeat(64): crate::auth::Session::default() });
            let sessions = fixture["private"]["sessions"].clone();
            let db = PatchDb::in_memory();
            db.put(&ROOT, &fixture).await.unwrap();
            Current::default().pre_init(&db).await.unwrap();
            let migrated = db.dump(&ROOT).await.value;
            let keyrings: TrustedKeys =
                from_value(migrated["public"]["serverInfo"]["trustedKeys"].clone()).unwrap();
            assert_eq!(os_signers(&keyrings), legacy);
            assert_eq!(migrated["private"]["sessions"], sessions);
            assert_eq!(
                migrated["public"]["serverInfo"]["latestMigrationRevision"],
                json!(1)
            );
            assert_eq!(
                migrated["public"]["serverInfo"]["postInitMigrationTodos"],
                json!({ "0.4.1": null })
            );
            Current::default().pre_init(&db).await.unwrap();
            assert_eq!(db.dump(&ROOT).await.value, migrated);
        }
    }
}
