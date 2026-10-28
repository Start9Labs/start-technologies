use std::collections::{BTreeMap, HashSet};

use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize, Serializer};

use crate::context::RpcContext;
use crate::prelude::*;
use crate::registry::signer::AcceptSigners;
use crate::sign::gpg::parse_public_keys;
use crate::sign::{AnyVerifyingKey, canonicalize_verifying_keys};
use crate::update::signers::{TrustedOsUpdateSigner, TrustedOsUpdateSigners, initial_signers};
use crate::util::new_guid;

pub type TrustedKeys = BTreeMap<InternedString, TrustedKey>;
pub const START9: &str = "start9";

/// A named keyring with independent service-package and OS-update trust.
#[derive(Clone, Debug, Deserialize, Serialize, HasModel, VisitFields, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct TrustedKey {
    pub name: String,
    /// Ed25519 public keys encoded as padded standard Base64 DER in canonical order.
    #[serde(serialize_with = "serialize_public_keys")]
    #[visit(
        output_wire = "Vec<AnyVerifyingKey>",
        type_attributes(visit::output_wire)
    )]
    pub public_keys: HashSet<AnyVerifyingKey>,
    /// False for built-in keyrings.
    pub removable: bool,
    pub marketplace: bool,
    pub os: bool,
}

impl TrustedKey {
    fn trusts_os_key(&self, key: &AnyVerifyingKey) -> bool {
        self.os && self.public_keys.contains(key)
    }
}

fn serialize_public_keys<S: Serializer>(
    keys: &HashSet<AnyVerifyingKey>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut keys = keys.iter().collect::<Vec<_>>();
    keys.sort_unstable_by_key(|key| key.interned_der64());
    keys.serialize(serializer)
}

rpc_toolkit::reflect_ts!(TrustedKey);
rpc_toolkit::ts_export!(TrustedKey, namespaces = [""]);

#[derive(Clone, Copy, Debug, Deserialize, Serialize, VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum TrustContext {
    Marketplace,
    Os,
}

rpc_toolkit::reflect_ts!(TrustContext);

pub fn enabled_keys(keys: &TrustedKeys, context: TrustContext) -> Vec<AnyVerifyingKey> {
    keys.values()
        .filter(|group| match context {
            TrustContext::Marketplace => group.marketplace,
            TrustContext::Os => group.os,
        })
        .flat_map(|group| group.public_keys.iter().cloned())
        .collect()
}

pub fn signer_policy(keys: Option<Vec<AnyVerifyingKey>>) -> Option<AcceptSigners> {
    keys.map(|keys| AcceptSigners::Any(keys.into_iter().map(AcceptSigners::Signer).collect()))
}

fn required_keys(
    keys: &TrustedKeys,
    context: TrustContext,
    trust: bool,
) -> Option<Vec<AnyVerifyingKey>> {
    (!trust).then(|| enabled_keys(keys, context))
}

pub async fn required(
    ctx: &RpcContext,
    context: TrustContext,
    trust: bool,
) -> Result<Option<Vec<AnyVerifyingKey>>, Error> {
    Ok(required_keys(
        &ctx.db
            .peek()
            .await
            .as_public()
            .as_server_info()
            .as_trusted_keys()
            .de()?,
        context,
        trust,
    ))
}

pub fn initial_trusted_keys() -> TrustedKeys {
    BTreeMap::from([
        (START9.into(), TrustedKey {
            name: "Start9".into(),
            removable: false,
            public_keys: initial_signers().into_keys().map(|key| key.parse().unwrap()).collect(),
            marketplace: true,
            os: true,
        }),
        ("start9-alpha".into(), TrustedKey {
            name: "Start9 Alpha".into(),
            removable: false,
            public_keys: HashSet::from([
                "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAGaAYxsY7iO5GyQrCA5vcLWYIQyoKImp0du4WWvIzr2Y=\n-----END PUBLIC KEY-----\n".parse().unwrap(),
                "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA4P+LwrLkZfhgcM3MNz6g8ihBvNnIfGSu+ojr+lFoDxU=\n-----END PUBLIC KEY-----\n".parse().unwrap(),
            ]),
            marketplace: false,
            os: false,
        }),
    ])
}

pub fn os_signers(keys: &TrustedKeys) -> TrustedOsUpdateSigners {
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

pub fn migrate_signers(legacy: TrustedOsUpdateSigners) -> Result<TrustedKeys, Error> {
    let legacy = canonicalize_verifying_keys(legacy)?;
    let mut keys = initial_trusted_keys();
    let start9 = keys.get_mut(START9).unwrap();
    start9.os = start9
        .public_keys
        .iter()
        .all(|key| legacy.contains_key(&key.interned_der64()));
    let grouped = if start9.os {
        start9.public_keys.clone()
    } else {
        HashSet::new()
    };
    for (key, signer) in legacy {
        let public_key: AnyVerifyingKey = key.parse()?;
        if !grouped.contains(&public_key) {
            keys.insert(
                key,
                TrustedKey {
                    name: signer.label,
                    public_keys: HashSet::from([public_key]),
                    removable: true,
                    marketplace: true,
                    os: true,
                },
            );
        }
    }
    Ok(keys)
}

pub fn api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "add",
            from_fn_async(add)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "toggle",
            from_fn_async(toggle)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
}

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct AddTrustedKeyParams {
    pub name: String,
    /// Ed25519 public keys in PEM or armored GPG format.
    pub public_keys: Vec<String>,
    /// New entries are enabled in this context and in the Marketplace.
    pub context: TrustContext,
}

rpc_toolkit::reflect_ts!(AddTrustedKeyParams);

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct ToggleTrustedKeyParams {
    pub id: InternedString,
    pub context: TrustContext,
    pub enabled: bool,
}

rpc_toolkit::reflect_ts!(ToggleTrustedKeyParams);

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct RemoveTrustedKeyParams {
    pub id: InternedString,
}

rpc_toolkit::reflect_ts!(RemoveTrustedKeyParams);

fn not_found() -> Error {
    Error::new(
        eyre!("{}", t!("trusted-keys.not-found")),
        ErrorKind::NotFound,
    )
}

fn invalid_group() -> Error {
    Error::new(
        eyre!("{}", t!("trusted-keys.invalid")),
        ErrorKind::InvalidRequest,
    )
}

fn group_name(name: String) -> Result<String, Error> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(invalid_group());
    }
    Ok(name)
}

pub fn add_group(
    keys: &mut TrustedKeys,
    id: InternedString,
    name: String,
    public_keys: Vec<AnyVerifyingKey>,
    context: TrustContext,
) -> Result<(), Error> {
    let name = group_name(name)?;
    if public_keys.is_empty() {
        return Err(invalid_group());
    }
    keys.insert(
        id,
        TrustedKey {
            name,
            public_keys: public_keys.into_iter().collect(),
            removable: true,
            marketplace: true,
            os: matches!(context, TrustContext::Os),
        },
    );
    Ok(())
}

fn toggle_group(keys: &mut TrustedKeys, params: ToggleTrustedKeyParams) -> Result<(), Error> {
    let group = keys.get_mut(&params.id).ok_or_else(not_found)?;
    match params.context {
        TrustContext::Marketplace => group.marketplace = params.enabled,
        TrustContext::Os => group.os = params.enabled,
    }
    Ok(())
}

pub fn remove_group(keys: &mut TrustedKeys, id: &str) -> Result<(), Error> {
    if !keys.get(id).ok_or_else(not_found)?.removable {
        return Err(Error::new(
            eyre!("{}", t!("trusted-keys.protected")),
            ErrorKind::InvalidRequest,
        ));
    }
    keys.remove(id).ok_or_else(not_found)?;
    Ok(())
}

async fn add(ctx: RpcContext, params: AddTrustedKeyParams) -> Result<(), Error> {
    let public_keys = parse_public_keys(&params.public_keys)?;
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_trusted_keys_mut()
                .mutate(|keys| {
                    add_group(keys, new_guid(), params.name, public_keys, params.context)
                })
        })
        .await
        .result
}

async fn toggle(ctx: RpcContext, params: ToggleTrustedKeyParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_trusted_keys_mut()
                .mutate(|keys| toggle_group(keys, params))
        })
        .await
        .result
}

async fn remove(ctx: RpcContext, params: RemoveTrustedKeyParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_trusted_keys_mut()
                .mutate(|keys| remove_group(keys, &params.id))
        })
        .await
        .result
}

pub fn legacy_add(
    keys: &mut TrustedKeys,
    key: AnyVerifyingKey,
    label: String,
) -> Result<(), Error> {
    let mut disabled = None;
    for group in keys
        .values_mut()
        .filter(|group| group.public_keys.contains(&key))
    {
        if group.os {
            return Err(Error::new(
                eyre!("{}", t!("update.signers.already-exists")),
                ErrorKind::InvalidRequest,
            ));
        }
        if disabled.is_none() {
            disabled = Some(group);
        }
    }
    if let Some(group) = disabled {
        group.os = true;
        return Ok(());
    }
    add_group(
        keys,
        key.interned_der64(),
        label,
        vec![key],
        TrustContext::Os,
    )
}

pub fn legacy_update(
    keys: &mut TrustedKeys,
    key: &AnyVerifyingKey,
    label: String,
) -> Result<(), Error> {
    let mut groups = keys
        .values_mut()
        .filter(|group| group.trusts_os_key(key))
        .peekable();
    groups.peek().ok_or_else(not_found)?;
    let label = group_name(label)?;
    for group in groups {
        group.name = label.clone();
    }
    Ok(())
}

pub fn legacy_remove(keys: &mut TrustedKeys, key: &AnyVerifyingKey) -> Result<(), Error> {
    let mut found = false;
    keys.retain(|_, group| {
        if !group.trusts_os_key(key) {
            return true;
        }
        found = true;
        if !group.removable {
            group.os = false;
            return true;
        }
        group.public_keys.retain(|candidate| candidate != key);
        !group.public_keys.is_empty()
    });
    found.then_some(()).ok_or_else(not_found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_override_is_explicit_and_empty_trust_does_not_bypass_policy() {
        let keys = TrustedKeys::new();
        assert!(required_keys(&keys, TrustContext::Os, true).is_none());
        assert!(required_keys(&keys, TrustContext::Marketplace, true).is_none());
        for context in [TrustContext::Os, TrustContext::Marketplace] {
            let policy = signer_policy(required_keys(&keys, context, false)).unwrap();
            assert!(policy.try_accept().is_err());
        }
    }

    #[test]
    fn defaults_and_context_independence() {
        let mut keys = initial_trusted_keys();
        assert_eq!(keys[START9].public_keys.len(), 3);
        assert_eq!(keys["start9-alpha"].public_keys.len(), 2);
        assert!(keys[START9].marketplace && keys[START9].os);
        assert!(!keys["start9-alpha"].marketplace && !keys["start9-alpha"].os);
        for id in [START9, "start9-alpha"] {
            let saved = keys[id].clone();
            assert!(!saved.removable);
            assert!(remove_group(&mut keys, id).is_err());
            assert_eq!(keys[id], saved);
            toggle_group(
                &mut keys,
                ToggleTrustedKeyParams {
                    id: id.into(),
                    context: TrustContext::Os,
                    enabled: !saved.os,
                },
            )
            .unwrap();
            assert_eq!(keys[id].os, !saved.os);
            assert_eq!(keys[id].marketplace, saved.marketplace);
        }
    }

    #[test]
    fn stored_arrays_deserialize_as_unique_key_sets() {
        let key = "MCowBQYDK2VwAyEAJdDg62o3oHIh4lXECn9gqORe+cCBbH/67MWjMxIMzDg=";
        let group: TrustedKey = from_value(imbl_value::json!({
            "name": "Publisher", "publicKeys": [key, key],
            "removable": true, "marketplace": true, "os": false,
        }))
        .unwrap();
        assert_eq!(group.public_keys, HashSet::from([key.parse().unwrap()]));
        assert_eq!(
            to_value(&group).unwrap()["publicKeys"],
            imbl_value::json!([key])
        );
    }

    #[test]
    fn public_key_set_serialization_is_canonical_across_reads() {
        let first = "MCowBQYDK2VwAyEA4P+LwrLkZfhgcM3MNz6g8ihBvNnIfGSu+ojr+lFoDxU=";
        let second = "MCowBQYDK2VwAyEAGaAYxsY7iO5GyQrCA5vcLWYIQyoKImp0du4WWvIzr2Y=";
        let group: TrustedKey = from_value(imbl_value::json!({
            "name": "Publisher", "publicKeys": [second, first, second],
            "removable": true, "marketplace": true, "os": false,
        }))
        .unwrap();
        assert_eq!(group.public_keys.len(), 2);
        let saved = to_value(&group).unwrap();
        assert_eq!(saved["publicKeys"], imbl_value::json!([first, second]));
        for _ in 0..8 {
            let reloaded: TrustedKey = from_value(saved.clone()).unwrap();
            assert_eq!(to_value(&reloaded).unwrap(), saved);
        }
    }

    #[test]
    fn pem_and_gpg_imports_persist_der64_keys() {
        let key = "MCowBQYDK2VwAyEAJdDg62o3oHIh4lXECn9gqORe+cCBbH/67MWjMxIMzDg=";
        let params: AddTrustedKeyParams = serde_json::from_value(serde_json::json!({
            "name": "Imported publisher",
            "context": "marketplace",
            "publicKeys": [
                format!("-----BEGIN PUBLIC KEY-----\n{key}\n-----END PUBLIC KEY-----\n"),
                include_str!("sign/fixtures/start9-public.asc"),
            ],
        }))
        .unwrap();
        let imported = parse_public_keys(&params.public_keys).unwrap();
        let mut keys = TrustedKeys::new();
        add_group(
            &mut keys,
            "imported".into(),
            params.name,
            imported,
            params.context,
        )
        .unwrap();
        let saved = to_value(&keys).unwrap();
        assert_eq!(saved["imported"]["publicKeys"], imbl_value::json!([key]));
        assert_eq!(from_value::<TrustedKeys>(saved).unwrap(), keys);
        assert!(keys["imported"].marketplace);
        assert!(!keys["imported"].os);
    }

    #[test]
    fn legacy_commands_use_der64_identity_for_pem_and_der64_inputs() {
        let der64 = "MCowBQYDK2VwAyEAGaAYxsY7iO5GyQrCA5vcLWYIQyoKImp0du4WWvIzr2Y=";
        let pem = format!("-----BEGIN PUBLIC KEY-----\n{der64}\n-----END PUBLIC KEY-----\n");
        let mut keys = TrustedKeys::new();
        legacy_add(&mut keys, pem.parse().unwrap(), "Publisher".into()).unwrap();
        assert_eq!(keys.keys().map(|id| &**id).collect::<Vec<_>>(), [der64]);
        assert_eq!(
            os_signers(&keys).keys().map(|id| &**id).collect::<Vec<_>>(),
            [der64]
        );
        legacy_update(&mut keys, &der64.parse().unwrap(), "Renamed".into()).unwrap();
        assert_eq!(keys[der64].name, "Renamed");
        legacy_remove(&mut keys, &pem.parse().unwrap()).unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn additions_are_atomic_and_have_context_specific_defaults() {
        let public_key = initial_signers()
            .into_keys()
            .next()
            .unwrap()
            .parse::<AnyVerifyingKey>()
            .unwrap();
        let mut keys = TrustedKeys::new();
        assert!(
            add_group(
                &mut keys,
                "invalid".into(),
                " ".into(),
                vec![public_key.clone()],
                TrustContext::Os
            )
            .is_err()
        );
        assert!(
            add_group(
                &mut keys,
                "invalid".into(),
                "empty".into(),
                vec![],
                TrustContext::Os
            )
            .is_err()
        );
        assert!(keys.is_empty());
        for (id, context, os) in [
            ("market", TrustContext::Marketplace, false),
            ("os", TrustContext::Os, true),
        ] {
            add_group(
                &mut keys,
                id.into(),
                " custom ".into(),
                vec![public_key.clone(), public_key.clone()],
                context,
            )
            .unwrap();
            assert_eq!(keys[id].name, "custom");
            assert_eq!(keys[id].public_keys, HashSet::from([public_key.clone()]));
            assert!(keys[id].marketplace);
            assert_eq!(keys[id].os, os);
        }
        remove_group(&mut keys, "market").unwrap();
        assert!(!keys.contains_key("market"));
        assert!(keys.contains_key("os"));
    }

    #[test]
    fn legacy_commands_disable_and_reenable_protected_os_trust() {
        let mut keys = initial_trusted_keys();
        let start9 = keys[START9].clone();
        let key = start9.public_keys.iter().next().unwrap().clone();
        legacy_remove(&mut keys, &key).unwrap();
        assert!(!keys[START9].os);
        assert!(keys[START9].marketplace);
        assert_eq!(keys[START9].public_keys, start9.public_keys);
        assert!(!keys[START9].removable);
        legacy_add(&mut keys, key.clone(), "ignored label".into()).unwrap();
        assert_eq!(keys[START9], start9);
        assert_eq!(keys.len(), 2);
        assert!(legacy_add(&mut keys, key, "duplicate".into()).is_err());
    }

    #[test]
    fn legacy_add_enables_existing_os_disabled_entry_without_changing_marketplace_trust() {
        let mut keys = initial_trusted_keys();
        let alpha = keys["start9-alpha"].clone();
        let key = alpha.public_keys.iter().next().unwrap().clone();
        legacy_add(&mut keys, key.clone(), "ignored label".into()).unwrap();
        assert!(keys["start9-alpha"].os);
        assert!(!keys["start9-alpha"].marketplace);
        assert_eq!(keys["start9-alpha"].name, alpha.name);
        assert_eq!(keys.len(), 2);
        legacy_remove(&mut keys, &key).unwrap();
        assert_eq!(keys["start9-alpha"], alpha);
        assert!(legacy_remove(&mut keys, &key).is_err());
    }

    #[test]
    fn legacy_add_does_not_mutate_an_overlapping_disabled_entry_when_key_is_already_os_trusted() {
        let mut keys = initial_trusted_keys();
        let key = keys["start9-alpha"]
            .public_keys
            .iter()
            .next()
            .unwrap()
            .clone();
        add_group(
            &mut keys,
            "zz-overlap".into(),
            "Overlapping publisher".into(),
            vec![key.clone()],
            TrustContext::Os,
        )
        .unwrap();
        let saved = keys.clone();
        assert!(legacy_add(&mut keys, key, "duplicate".into()).is_err());
        assert_eq!(keys, saved);
    }

    #[test]
    fn legacy_key_commands_update_every_os_enabled_entry_containing_the_key() {
        let mut keys = initial_trusted_keys();
        let key = keys[START9].public_keys.iter().next().unwrap().clone();
        add_group(
            &mut keys,
            "overlap".into(),
            "Overlapping publisher".into(),
            vec![key.clone()],
            TrustContext::Os,
        )
        .unwrap();
        legacy_update(&mut keys, &key, "renamed".into()).unwrap();
        assert_eq!(keys[START9].name, "renamed");
        assert_eq!(keys["overlap"].name, "renamed");
        legacy_remove(&mut keys, &key).unwrap();
        assert!(!keys[START9].os);
        assert!(keys[START9].marketplace);
        assert!(keys[START9].public_keys.contains(&key));
        assert!(!keys.contains_key("overlap"));
        assert!(!os_signers(&keys).contains_key(&key.interned_der64()));
        assert!(legacy_remove(&mut keys, &key).is_err());
    }

    #[test]
    fn migration_preserves_pem_and_der64_trust_without_enabling_removed_keys() {
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
            let keys = migrate_signers(legacy.clone()).unwrap();
            assert_eq!(keys.len(), 2);
            assert!(keys[START9].os);
            assert_eq!(
                os_signers(&keys).into_keys().collect::<Vec<_>>(),
                canonical.keys().cloned().collect::<Vec<_>>()
            );
            let removed = legacy
                .pop_first()
                .unwrap()
                .0
                .parse::<AnyVerifyingKey>()
                .unwrap();
            let keys = migrate_signers(legacy.clone()).unwrap();
            assert!(!keys[START9].os);
            assert!(!os_signers(&keys).contains_key(&removed.interned_der64()));
            let mut remaining = canonical.clone();
            remaining.remove(&removed.interned_der64());
            assert_eq!(os_signers(&keys), remaining);
            for id in remaining.keys() {
                assert!(keys[id].os);
            }
        }
        assert!(os_signers(&migrate_signers(TrustedOsUpdateSigners::new()).unwrap()).is_empty());
    }
}
