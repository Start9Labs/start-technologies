use chrono::{DateTime, Utc};
use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_1_1};
use crate::prelude::*;
use crate::trusted_keys::legacy::{decode_keys, grouped_keys};
use crate::trusted_keys::{protect_registry_keys, replace_stored_drbonez_certificates};

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

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
        1
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
        let groups = legacy::initial_keyrings();
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
        let keys = legacy::individual_keys(&groups, now).unwrap();
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
    fn malformed_historical_keys_leave_persisted_trust_unchanged() {
        let mut db = imbl_value::json!({ "public": { "serverInfo": { "trustedKeys": {
            "legacy": { "name": "Publisher", "publicKeys": ["invalid"], "removable": true, "marketplace": true, "os": false }
        } } } });
        let before = db.clone();
        assert!(Version.up(&mut db, ()).is_err());
        assert_eq!(db, before);
    }
}
