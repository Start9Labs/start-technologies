use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_1_1};
use crate::prelude::*;
use crate::sign::AnyVerifyingKey;
use crate::trusted_keys::legacy::{decode_keys, grouped_keys};
use crate::trusted_keys::{TrustedKeys, initial_signer_certificates, protect_registry_keys};

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

pub(crate) fn retired_alpha_keys() -> BTreeSet<AnyVerifyingKey> {
    BTreeSet::from([
        "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAGaAYxsY7iO5GyQrCA5vcLWYIQyoKImp0du4WWvIzr2Y=\n-----END PUBLIC KEY-----\n".parse().unwrap(),
        "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA4P+LwrLkZfhgcM3MNz6g8ihBvNnIfGSu+ojr+lFoDxU=\n-----END PUBLIC KEY-----\n".parse().unwrap(),
    ])
}

fn remove_seeded_alpha_keys(keys: &mut TrustedKeys) {
    for key in retired_alpha_keys() {
        keys.remove(&key.interned_der64());
    }
}

fn replace_stored_drbonez_certificates(keys: &mut TrustedKeys) {
    let seeded = initial_signer_certificates()
        .find(|(label, _)| *label == "drbonez")
        .map(|(_, imported)| imported.gpg_exports)
        .unwrap_or_default();
    for key in keys.values_mut() {
        let stored = key.gpg_exports.len();
        key.gpg_exports.retain(|export| {
            !seeded
                .iter()
                .any(|seed| seed.fingerprints == export.fingerprints)
        });
        if key.gpg_exports.len() != stored {
            key.gpg_exports.extend(seeded.iter().cloned());
        }
    }
}

fn stored_keys(db: &mut Value) -> Result<&mut Value, Error> {
    db.get_mut("public")
        .and_then(|public| public.get_mut("serverInfo"))
        .and_then(|server| server.get_mut("trustedKeys"))
        .or_not_found("trustedKeys")
}

fn migrate(db: &mut Value, now: DateTime<Utc>) -> Result<(), Error> {
    let snapshots = db
        .get("public")
        .and_then(|public| public.get("serverInfo"))
        .and_then(|server| server.get("registryKeys"))
        .cloned()
        .map(from_value)
        .transpose()?
        .unwrap_or_default();
    let ui = db
        .get("public")
        .and_then(|public| public.get("ui"))
        .cloned()
        .unwrap_or_default();
    let stored = stored_keys(db)?;
    let mut keys = decode_keys(stored.clone(), now)?;
    replace_stored_drbonez_certificates(&mut keys);
    remove_seeded_alpha_keys(&mut keys);
    protect_registry_keys(&mut keys, &snapshots, &ui);
    *stored = to_value(&keys)?;
    Ok(())
}

impl VersionT for Version {
    type Previous = v0_4_1_1::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        exver::Version::new([0, 4, 1, 2], [])
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn migration_revision(self) -> usize {
        2
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        migrate(db, Utc::now())?;
        Ok(Value::Null)
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
        let stored = stored_keys(db)?;
        *stored = to_value(&grouped_keys(decode_keys(stored.clone(), Utc::now())?))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trusted_keys::{TrustContext, TrustedKeys, enabled_keys, legacy};

    #[test]
    fn grouped_trust_migration_preserves_both_policies_and_rolls_back_compatibly() {
        let groups = legacy::historical_keyrings();
        let mut historical = to_value(&groups).unwrap();
        for (_, group) in historical.as_object_mut().unwrap().iter_mut() {
            group.as_object_mut().unwrap().remove("origin");
            group.as_object_mut().unwrap().remove("originRegistry");
            for export in group["gpgExports"].as_array_mut().unwrap() {
                export.as_object_mut().unwrap().remove("identities");
                export.as_object_mut().unwrap().remove("keyFingerprints");
            }
        }
        let mut db =
            imbl_value::json!({ "public": { "serverInfo": { "trustedKeys": historical } } });
        let upgraded_at = Utc::now();
        migrate(&mut db, upgraded_at).unwrap();
        let migrated: TrustedKeys =
            from_value(db["public"]["serverInfo"]["trustedKeys"].clone()).unwrap();
        let mut expected = legacy::individual_keys(&groups, upgraded_at).unwrap();
        remove_seeded_alpha_keys(&mut expected);
        for key in expected.values_mut() {
            key.removable = true;
        }
        assert_eq!(migrated, expected);
        assert!(migrated.values().all(|key| key.removable));
        for key in migrated.values() {
            assert_eq!(key.added_at, upgraded_at);
            assert_eq!(key.origin, crate::trusted_keys::START9_REGISTRY);
        }
        for (id, key) in &migrated {
            assert_eq!(*id, key.public_key.interned_der64());
            let wire = to_value(key).unwrap();
            assert!(wire.get("publicKey").is_some());
            assert!(wire.get("publicKeys").is_none());
        }
        let first = db.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(db, first);
        Version.down(&mut db).unwrap();
        let restored: legacy::TrustedKeyrings =
            from_value(db["public"]["serverInfo"]["trustedKeys"].clone()).unwrap();
        assert_eq!(
            legacy::os_signers(&restored),
            crate::trusted_keys::os_signers(&migrated)
        );
        let rolled_back = db.clone();
        Version.down(&mut db).unwrap();
        assert_eq!(db, rolled_back);
        Version.up(&mut db, ()).unwrap();
        assert_eq!(db, first);
        for context in [TrustContext::Marketplace, TrustContext::Os] {
            assert_eq!(
                enabled_keys(&migrated, context),
                enabled_keys(&expected, context)
            );
        }
    }

    #[test]
    fn stored_drbonez_certificates_collapse_to_the_seed() {
        let seeded = crate::trusted_keys::initial_trusted_keys();
        let (id, _) = crate::update::signers::initial_signers()
            .into_iter()
            .find(|(_, signer)| signer.label == "drbonez")
            .unwrap();
        let mut stale = seeded.clone();
        let key = stale.get_mut(&id).unwrap();
        let mut certificate = key.gpg_exports.pop_first().unwrap();
        certificate.armored = certificate.armored.replacen('\n', "\nComment: stale\n", 1);
        key.gpg_exports.insert(certificate);
        for trusted_keys in [
            to_value(&stale).unwrap(),
            to_value(&legacy::grouped_keys(stale.clone())).unwrap(),
        ] {
            let mut db =
                imbl_value::json!({ "public": { "serverInfo": { "trustedKeys": trusted_keys } } });
            Version.up(&mut db, ()).unwrap();
            let migrated: TrustedKeys =
                from_value(db["public"]["serverInfo"]["trustedKeys"].clone()).unwrap();
            for (id, key) in &seeded {
                assert_eq!(migrated[id].gpg_exports, key.gpg_exports);
            }
        }
    }

    #[test]
    fn migration_derives_protection_from_connected_observed_listings() {
        let now = Utc::now();
        let groups = legacy::initial_keyrings();
        let mut keys = legacy::individual_keys(&groups, now).unwrap();
        remove_seeded_alpha_keys(&mut keys);
        let listed = keys.values().next().unwrap().public_key.clone();
        for connected in [false, true] {
            let mut db = imbl_value::json!({ "public": {
                "serverInfo": {
                    "trustedKeys": groups,
                    "registryKeys": { "https://registry.example": [listed] }
                },
                "ui": { "registries": {} }
            } });
            if connected {
                db["public"]["ui"]["registries"]["https://registry.example/"] = Value::Null;
            }
            migrate(&mut db, now).unwrap();
            let migrated: TrustedKeys =
                from_value(db["public"]["serverInfo"]["trustedKeys"].clone()).unwrap();
            for (id, key) in &keys {
                assert_eq!(
                    migrated[id],
                    crate::trusted_keys::TrustedKey {
                        removable: !(connected && key.public_key == listed),
                        ..key.clone()
                    }
                );
            }
        }
    }

    #[test]
    fn migration_removes_seeded_alpha_keys_from_grouped_and_individual_trust() {
        let now = Utc::now();
        let mut historical = legacy::historical_keyrings();
        let alpha = historical.get_mut("start9-alpha").unwrap();
        alpha.name = "Owner label".into();
        alpha.marketplace = true;
        alpha.os = true;
        let individual = legacy::individual_keys(&historical, now).unwrap();
        for stored in [
            to_value(&historical).unwrap(),
            to_value(&individual).unwrap(),
        ] {
            let snapshots =
                imbl_value::json!({"https://alpha-registry.start9.com": retired_alpha_keys()});
            let mut db = imbl_value::json!({"public": {"serverInfo": {"trustedKeys": stored, "registryKeys": snapshots}, "ui": {"registries": {"https://alpha-registry.start9.com": "Alpha"}}}});
            migrate(&mut db, now).unwrap();
            let migrated: TrustedKeys =
                from_value(db["public"]["serverInfo"]["trustedKeys"].clone()).unwrap();
            for key in retired_alpha_keys() {
                assert!(!migrated.contains_key(&key.interned_der64()));
            }
            assert_eq!(db["public"]["serverInfo"]["registryKeys"], snapshots);
            assert_eq!(
                migrated.len(),
                individual.len() - retired_alpha_keys().len()
            );
            let first = db.clone();
            migrate(&mut db, now).unwrap();
            assert_eq!(db, first);
        }
    }

    #[tokio::test]
    async fn existing_0_4_1_2_installations_rerun_the_alpha_key_cleanup() {
        let account =
            crate::account::AccountInfo::new("password", std::time::SystemTime::now(), None)
                .unwrap();
        let mut fixture =
            to_value(&crate::db::model::Database::init(&account, false, None, None).unwrap())
                .unwrap();
        let historical =
            legacy::individual_keys(&legacy::historical_keyrings(), Utc::now()).unwrap();
        fixture["public"]["serverInfo"]["version"] = to_value(&Version.semver()).unwrap();
        fixture["public"]["serverInfo"]["trustedKeys"] = to_value(&historical).unwrap();
        fixture["public"]["serverInfo"]["latestMigrationRevision"] = imbl_value::json!(1);
        let db = patch_db::PatchDb::in_memory();
        db.put(&patch_db::json_ptr::ROOT, &fixture).await.unwrap();
        let current = super::super::Current::default();
        current.pre_init(&db).await.unwrap();
        let migrated = db.dump(&patch_db::json_ptr::ROOT).await.value;
        let keys: TrustedKeys =
            from_value(migrated["public"]["serverInfo"]["trustedKeys"].clone()).unwrap();
        assert_eq!(keys.len(), 3);
        for key in retired_alpha_keys() {
            assert!(!keys.contains_key(&key.interned_der64()));
        }
        assert_eq!(
            migrated["public"]["serverInfo"]["version"],
            to_value(&current.semver()).unwrap()
        );
        assert_eq!(
            migrated["public"]["serverInfo"]["latestMigrationRevision"],
            imbl_value::json!(current.migration_revision())
        );
    }

    #[test]
    fn malformed_historical_keys_leave_persisted_trust_unchanged() {
        let mut db = imbl_value::json!({ "public": { "serverInfo": { "trustedKeys": {
            "legacy": { "name": "Publisher", "publicKeys": ["invalid"], "removable": true, "marketplace": true, "os": false }
        } } } });
        let before = db.clone();
        assert!(Version.up(&mut db, ()).is_err());
        assert_eq!(db, before);
    }
}
