use super::*;

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
            public_keys: crate::version::retired_alpha_keys(),
            gpg_exports: BTreeSet::new(),
            marketplace: false,
            os: false,
        },
    );
    keys
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
