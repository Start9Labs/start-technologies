use super::*;
use crate::sign::canonicalize_verifying_keys;

pub(crate) type TrustedKeyrings = BTreeMap<InternedString, TrustedKeyring>;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrustedKeyring {
    pub name: String,

    #[serde(default = "manual_origin")]
    pub origin: String,

    #[serde(default)]
    pub origin_registry: Option<RegistryIdentity>,
    pub public_keys: BTreeSet<AnyVerifyingKey>,

    #[serde(default)]
    pub gpg_exports: BTreeSet<GpgPublicKeyExport>,

    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub contact: BTreeSet<ContactInfo>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_at: Option<DateTime<Utc>>,

    pub removable: bool,
    pub marketplace: bool,
    pub os: bool,
}

pub(crate) fn start9_registry() -> RegistryIdentity {
    RegistryIdentity {
        name: Some("Start9 Registry".into()),
        icon: None,
    }
}

pub(crate) fn retired_alpha_keys() -> BTreeSet<AnyVerifyingKey> {
    BTreeSet::from([
        "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAGaAYxsY7iO5GyQrCA5vcLWYIQyoKImp0du4WWvIzr2Y=\n-----END PUBLIC KEY-----\n".parse().unwrap(),
        "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA4P+LwrLkZfhgcM3MNz6g8ihBvNnIfGSu+ojr+lFoDxU=\n-----END PUBLIC KEY-----\n".parse().unwrap(),
    ])
}

pub(crate) fn initial_keyrings() -> TrustedKeyrings {
    let mut start9 = ImportedPublicKeys::default();
    for (_, imported) in initial_signer_certificates() {
        start9.keys.extend(imported.keys);
        start9.gpg_exports.extend(imported.gpg_exports);
    }
    BTreeMap::from([(
        START9.into(),
        TrustedKeyring {
            name: "Start9".into(),
            origin: START9_REGISTRY.into(),
            origin_registry: Some(start9_registry()),
            removable: false,
            public_keys: start9.keys,
            gpg_exports: start9.gpg_exports,
            contact: BTreeSet::new(),
            added_at: None,
            marketplace: true,
            os: true,
        },
    )])
}

#[cfg(test)]
pub(crate) fn historical_keyrings() -> TrustedKeyrings {
    let mut keys = initial_keyrings();
    keys.insert(
        "start9-alpha".into(),
        TrustedKeyring {
            name: "Start9 Alpha".into(),
            origin: START9_REGISTRY.into(),
            origin_registry: Some(start9_registry()),
            removable: false,
            contact: BTreeSet::new(),
            added_at: None,
            public_keys: retired_alpha_keys(),
            gpg_exports: BTreeSet::new(),
            marketplace: false,
            os: false,
        },
    );
    keys
}

pub(crate) fn os_signers(keys: &TrustedKeyrings) -> TrustedOsUpdateSigners {
    keys.values()
        .filter(|group| group.os)
        .flat_map(|group| {
            group.public_keys.iter().map(|key| {
                (
                    key.interned_der64(),
                    TrustedOsUpdateSigner {
                        label: group.name.clone(),
                    },
                )
            })
        })
        .collect()
}

pub(crate) fn migrate_signers(legacy: TrustedOsUpdateSigners) -> Result<TrustedKeyrings, Error> {
    let legacy = canonicalize_verifying_keys(legacy)?;
    let mut keys = initial_keyrings();
    let start9 = keys.get_mut(START9).unwrap();
    start9.os = start9
        .public_keys
        .iter()
        .all(|key| legacy.contains_key(&key.interned_der64()));
    let grouped = if start9.os {
        start9.public_keys.clone()
    } else {
        BTreeSet::new()
    };
    for (key, signer) in legacy {
        let public_key: AnyVerifyingKey = key.parse()?;
        if !grouped.contains(&public_key) {
            keys.insert(
                key,
                TrustedKeyring {
                    name: signer.label,
                    origin: manual_origin(),
                    origin_registry: None,
                    public_keys: BTreeSet::from([public_key]),
                    gpg_exports: BTreeSet::new(),
                    contact: BTreeSet::new(),
                    added_at: None,
                    removable: true,
                    marketplace: true,
                    os: true,
                },
            );
        }
    }
    Ok(keys)
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

pub(crate) fn decode_keys(value: Value, now: DateTime<Utc>) -> Result<TrustedKeys, Error> {
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

pub(crate) fn grouped_keys(keys: TrustedKeys) -> TrustedKeyrings {
    keys.into_iter().map(|(id, key)| (id, key.into())).collect()
}

pub(crate) fn individual_keys(
    groups: &TrustedKeyrings,
    now: DateTime<Utc>,
) -> Result<TrustedKeys, Error> {
    let mut keys = TrustedKeys::new();
    for (group_id, group) in groups {
        let imported = ImportedPublicKeys {
            keys: group.public_keys.clone(),
            gpg_exports: group.gpg_exports.clone(),
        };
        let builtin = group.origin == "system"
            || (!group.removable && matches!(group_id.as_ref(), START9 | "start9-alpha"));
        let added_at = group.added_at.unwrap_or(now);
        for (id, mut individual) in split_import(group.name.clone(), imported, added_at)? {
            individual.origin = group.origin.clone();
            individual.origin_registry = group.origin_registry.clone();
            individual.contact = group.contact.clone();
            individual.removable = group.removable;
            individual.marketplace = group.marketplace;
            individual.os = group.os;
            if builtin {
                individual.origin = START9_REGISTRY.into();
                individual.origin_registry = Some(start9_registry());
            }
            match keys.entry(id) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(individual);
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    let existing = entry.get_mut();
                    if builtin {
                        if individual.os || !existing.os {
                            existing.name = individual.name;
                        }
                        existing.origin = individual.origin;
                        existing.origin_registry = individual.origin_registry;
                        existing.removable = false;
                    }
                    existing.marketplace |= individual.marketplace;
                    existing.os |= individual.os;
                    existing.gpg_exports.extend(individual.gpg_exports);
                    existing.contact.extend(individual.contact);
                    existing.added_at = existing.added_at.min(individual.added_at);
                }
            }
        }
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouped_duplicates_preserve_effective_trust_system_protection_and_identity() {
        let mut groups = historical_keyrings();
        groups.get_mut(START9).unwrap().os = false;
        let public_key = groups[START9].public_keys.first().unwrap().clone();
        let mut overlap = groups[START9].clone();
        overlap.name = "Owner label".into();
        overlap.public_keys = BTreeSet::from([public_key.clone()]);
        overlap.origin = "manual".into();
        overlap.origin_registry = Some(RegistryIdentity {
            name: Some("Registry".into()),
            icon: None,
        });
        overlap.removable = true;
        overlap.os = true;
        groups.insert("overlap".into(), overlap);
        let keys = individual_keys(&groups, Utc::now()).unwrap();
        assert_eq!(keys.len(), 5);
        assert_eq!(
            crate::trusted_keys::os_signers(&keys)
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
    fn migration_preserves_legacy_os_policy_without_reenabling_removed_keys() {
        let canonical = initial_signers();
        let pem = canonical
            .iter()
            .map(|(key, signer)| {
                (
                    key.parse::<AnyVerifyingKey>().unwrap().to_string().into(),
                    signer.clone(),
                )
            })
            .collect::<TrustedOsUpdateSigners>();
        for mut legacy in [canonical.clone(), pem] {
            let groups = migrate_signers(legacy.clone()).unwrap();
            assert_eq!(
                os_signers(&groups).into_keys().collect::<Vec<_>>(),
                canonical.keys().cloned().collect::<Vec<_>>()
            );
            let removed = legacy
                .pop_first()
                .unwrap()
                .0
                .parse::<AnyVerifyingKey>()
                .unwrap();
            let keys = individual_keys(&migrate_signers(legacy).unwrap(), Utc::now()).unwrap();
            assert!(
                !crate::trusted_keys::os_signers(&keys).contains_key(&removed.interned_der64())
            );
            assert!(!keys[&removed.interned_der64()].os);
        }
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
