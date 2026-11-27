use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use exver::VersionRange;
use serde::Deserialize;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_1_1};
use crate::prelude::*;
use crate::sign::AnyVerifyingKey;
use crate::trusted_keys::legacy::{TrustedKeyring, TrustedKeyrings, individual_keys};
use crate::trusted_keys::{
    Registries, TrustedKey, TrustedKeys, initial_signer_certificates, protect_registry_keys,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

pub(crate) fn retired_alpha_keys() -> BTreeSet<AnyVerifyingKey> {
    BTreeSet::from([
        "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAGaAYxsY7iO5GyQrCA5vcLWYIQyoKImp0du4WWvIzr2Y=\n-----END PUBLIC KEY-----\n".parse().unwrap(),
        "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA4P+LwrLkZfhgcM3MNz6g8ihBvNnIfGSu+ojr+lFoDxU=\n-----END PUBLIC KEY-----\n".parse().unwrap(),
    ])
}

/// Reads the `registries` map StartOS kept in `public.ui` before 0.4.2.1.
pub(super) fn ui_registries(ui: &Value) -> Registries {
    ui.get("registries")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(url, name)| (url.clone(), name.as_str().map(str::to_owned)))
        .collect()
}

impl From<TrustedKey> for TrustedKeyring {
    fn from(key: TrustedKey) -> Self {
        Self {
            name: key.name,
            origin: key.origin,
            origin_registry: key.origin_registry,
            public_keys: BTreeSet::from([key.public_key]),
            gpg_exports: key.gpg_exports,
            contact: key.contact,
            added_at: Some(key.added_at),
            removable: key.removable,
            marketplace: key.marketplace,
            os: key.os,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StoredKey {
    Individual(TrustedKey),
    Grouped(TrustedKeyring),
}

pub(super) fn decode_keys(value: Value, now: DateTime<Utc>) -> Result<TrustedKeys, Error> {
    let stored: BTreeMap<InternedString, StoredKey> = from_value(value)?;
    let groups = stored
        .into_iter()
        .map(|(id, key)| {
            (
                id,
                match key {
                    StoredKey::Individual(key) => key.into(),
                    StoredKey::Grouped(group) => group,
                },
            )
        })
        .collect();
    individual_keys(&groups, now)
}

pub(super) fn grouped_keys(keys: TrustedKeys) -> TrustedKeyrings {
    keys.into_iter().map(|(id, key)| (id, key.into())).collect()
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
    let registries = db
        .get("public")
        .and_then(|public| public.get("ui"))
        .map(ui_registries)
        .unwrap_or_default();
    let stored = stored_keys(db)?;
    let mut keys = decode_keys(stored.clone(), now)?;
    replace_stored_drbonez_certificates(&mut keys);
    remove_seeded_alpha_keys(&mut keys);
    protect_registry_keys(&mut keys, &snapshots, &registries);
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
    use crate::sign::gpg::parse_public_keys;
    use crate::trusted_keys::legacy::start9_registry;
    use crate::trusted_keys::{
        START9, START9_REGISTRY, TrustContext, enabled_keys, initial_signer_labels,
        initial_trusted_keys, legacy, remove_key,
    };
    use crate::version::v0_4_1::{os_signers, trusted_key_os_signers};

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
        assert_eq!(os_signers(&restored), trusted_key_os_signers(&migrated));
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
        let (id, _) = initial_signer_labels()
            .into_iter()
            .find(|(_, label)| *label == "drbonez")
            .unwrap();
        let mut stale = seeded.clone();
        let key = stale.get_mut(&id).unwrap();
        let mut certificate = key.gpg_exports.pop_first().unwrap();
        certificate.armored = certificate.armored.replacen('\n', "\nComment: stale\n", 1);
        key.gpg_exports.insert(certificate);
        for trusted_keys in [
            to_value(&stale).unwrap(),
            to_value(&grouped_keys(stale.clone())).unwrap(),
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
        fixture["public"]["serverInfo"]["trustedKeys"] = to_value(&historical).unwrap();
        fixture["public"]["serverInfo"]["version"] = to_value(&Version.semver()).unwrap();
        fixture["public"]["serverInfo"]["latestMigrationRevision"] = imbl_value::json!(1);
        let db = patch_db::PatchDb::in_memory();
        db.put(&patch_db::json_ptr::ROOT, &fixture).await.unwrap();
        super::super::Current::default()
            .pre_init(&db)
            .await
            .unwrap();
        let migrated = db.dump(&patch_db::json_ptr::ROOT).await.value;
        let keys: TrustedKeys =
            from_value(migrated["public"]["serverInfo"]["trustedKeys"].clone()).unwrap();
        assert_eq!(keys.len(), 3);
        for key in retired_alpha_keys() {
            assert!(!keys.contains_key(&key.interned_der64()));
        }
        assert_eq!(
            migrated["public"]["serverInfo"]["latestMigrationRevision"],
            imbl_value::json!(super::super::Current::default().migration_revision())
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

    #[test]
    fn current_decoder_requires_a_singular_key_and_migration_accepts_historical_groups() {
        let key = initial_signer_labels().into_keys().next().unwrap();
        let historical = imbl_value::json!({ "name": "Publisher", "publicKeys": [key], "removable": true, "marketplace": true, "os": false });
        assert!(from_value::<TrustedKey>(historical.clone()).is_err());
        let migrated =
            decode_keys(imbl_value::json!({ "legacy": historical }), Utc::now()).unwrap();
        assert_eq!(migrated.len(), 1);
        let current = &migrated[&key];
        assert_eq!(current.public_key.interned_der64(), key);
        assert_eq!(current.origin, "manual");
        assert!(current.origin_registry.is_none());
    }

    #[test]
    fn decoding_a_retained_full_certificate_does_not_restore_a_removed_signing_key() {
        let imported =
            parse_public_keys(&[include_str!("../sign/fixtures/ed25519-signers.asc").into()])
                .unwrap();
        let removed = imported.keys.first().unwrap().clone();
        let groups = TrustedKeyrings::from([(
            "publisher".into(),
            TrustedKeyring {
                name: "Publisher".into(),
                origin: "manual".into(),
                origin_registry: None,
                public_keys: imported.keys,
                gpg_exports: imported.gpg_exports,
                contact: BTreeSet::new(),
                added_at: None,
                removable: true,
                marketplace: true,
                os: true,
            },
        )]);
        let mut keys = individual_keys(&groups, Utc::now()).unwrap();
        remove_key(&mut keys, &removed.interned_der64()).unwrap();
        let wire = to_value(&keys).unwrap();
        let restored: TrustedKeys = from_value(wire.clone()).unwrap();
        assert_eq!(restored.len(), 1);
        assert!(!restored.contains_key(&removed.interned_der64()));
        assert_eq!(decode_keys(wire, Utc::now()).unwrap(), restored);
    }

    #[test]
    fn grouped_duplicates_preserve_effective_trust_system_protection_and_identity() {
        let mut groups = legacy::historical_keyrings();
        groups.get_mut(START9).unwrap().os = false;
        let public_key = groups[START9].public_keys.first().unwrap().clone();
        let mut overlap = groups[START9].clone();
        overlap.name = "Owner label".into();
        overlap.public_keys = BTreeSet::from([public_key.clone()]);
        overlap.origin = "manual".into();
        overlap.origin_registry = Some(crate::trusted_keys::RegistryIdentity {
            name: Some("Registry".into()),
            icon: None,
        });
        overlap.removable = true;
        overlap.os = true;
        groups.insert("overlap".into(), overlap);
        let keys = individual_keys(&groups, Utc::now()).unwrap();
        assert_eq!(keys.len(), 5);
        assert_eq!(
            trusted_key_os_signers(&keys)
                .into_keys()
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([public_key.interned_der64()])
        );
        assert_eq!(enabled_keys(&keys, TrustContext::Marketplace).len(), 3);
        for (id, key) in &keys {
            assert_eq!(*id, key.public_key.interned_der64());
            assert_eq!(key.origin, START9_REGISTRY);
            assert_eq!(key.origin_registry, Some(start9_registry()));
            assert!(!key.removable);
            assert!(
                key.gpg_exports
                    .iter()
                    .all(|export| export.identities.contains_key(id))
            );
        }
        assert_eq!(keys[&public_key.interned_der64()].name, "Owner label");
        assert_eq!(
            decode_keys(to_value(&keys).unwrap(), Utc::now()).unwrap(),
            keys
        );
    }

    #[test]
    fn historical_arrays_canonicalize_and_deduplicate_without_losing_keys() {
        let first = "MCowBQYDK2VwAyEAGaAYxsY7iO5GyQrCA5vcLWYIQyoKImp0du4WWvIzr2Y=";
        let second = "MCowBQYDK2VwAyEA4P+LwrLkZfhgcM3MNz6g8ihBvNnIfGSu+ojr+lFoDxU=";
        let value = imbl_value::json!({ "historical": { "name": "Publisher", "publicKeys": [second, first, second], "removable": true, "marketplace": false, "os": true } });
        let keys = decode_keys(value, Utc::now()).unwrap();
        assert_eq!(keys.len(), 2);
        assert_eq!(
            keys.keys().map(|key| &**key).collect::<BTreeSet<_>>(),
            BTreeSet::from([first, second])
        );
        assert!(keys.values().all(|key| key.os && !key.marketplace));
        assert_eq!(
            decode_keys(to_value(&keys).unwrap(), Utc::now()).unwrap(),
            keys
        );
    }

    #[test]
    fn mixed_current_and_historical_records_merge_by_canonical_identity() {
        let mut initial = initial_trusted_keys();
        let id = initial.keys().next().unwrap().clone();
        initial.get_mut(&id).unwrap().removable = false;
        let mut wire = to_value(&initial).unwrap();
        let mut group: TrustedKeyring = initial[&id].clone().into();
        group.origin = "manual".into();
        group.name = "Alias".into();
        group.removable = true;
        group.os = true;
        wire.as_object_mut()
            .unwrap()
            .insert("legacy".into(), to_value(&group).unwrap());
        let keys = decode_keys(wire, Utc::now()).unwrap();
        assert_eq!(keys.len(), initial.len());
        assert!(keys[&id].os);
        assert_eq!(keys[&id].origin, START9_REGISTRY);
        assert!(!keys[&id].removable);
    }
}
