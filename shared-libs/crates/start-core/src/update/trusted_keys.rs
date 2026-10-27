use std::collections::BTreeMap;

use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::context::RpcContext;
use crate::prelude::*;
use crate::registry::signer::AcceptSigners;
use crate::sign::AnyVerifyingKey;
use crate::update::signers::{TrustedOsUpdateSigner, TrustedOsUpdateSigners, initial_signers};

pub type TrustedKeys = BTreeMap<InternedString, TrustedKey>;
pub const START9: &str = "start9";

#[derive(Clone, Debug, Deserialize, Serialize, HasModel, VisitFields, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct TrustedKey {
    pub name: String,
    pub public_keys: Vec<AnyVerifyingKey>,
    pub removable: bool,
    pub marketplace: bool,
    pub os: bool,
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
            removable: true,
            public_keys: vec!["-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAGaAYxsY7iO5GyQrCA5vcLWYIQyoKImp0du4WWvIzr2Y=\n-----END PUBLIC KEY-----\n".parse().unwrap()],
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
                    key.interned_pem(),
                    TrustedOsUpdateSigner {
                        label: group.name.clone(),
                    },
                )
            })
        })
        .collect()
}

pub fn migrate_signers(legacy: TrustedOsUpdateSigners) -> TrustedKeys {
    let mut keys = initial_trusted_keys();
    let start9 = keys.get_mut(START9).unwrap();
    start9.os = start9
        .public_keys
        .iter()
        .all(|key| legacy.contains_key(&key.interned_pem()));
    let grouped = if start9.os {
        start9.public_keys.clone()
    } else {
        Vec::new()
    };
    for (key, signer) in legacy {
        let public_key: AnyVerifyingKey = key.parse().unwrap();
        if !grouped.contains(&public_key) {
            keys.insert(
                key,
                TrustedKey {
                    name: signer.label,
                    public_keys: vec![public_key],
                    removable: true,
                    marketplace: true,
                    os: true,
                },
            );
        }
    }
    keys
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
    pub public_keys: Vec<AnyVerifyingKey>,
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
        eyre!("{}", t!("update.trusted-keys.not-found")),
        ErrorKind::NotFound,
    )
}

fn invalid_group() -> Error {
    Error::new(
        eyre!("{}", t!("update.trusted-keys.invalid")),
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
    params: AddTrustedKeyParams,
) -> Result<(), Error> {
    let name = group_name(params.name)?;
    if params.public_keys.is_empty() {
        return Err(invalid_group());
    }
    let mut public_keys = Vec::new();
    for key in params.public_keys {
        if !public_keys.contains(&key) {
            public_keys.push(key);
        }
    }
    keys.insert(
        id,
        TrustedKey {
            name,
            public_keys,
            removable: true,
            marketplace: true,
            os: matches!(params.context, TrustContext::Os),
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
            eyre!("{}", t!("update.trusted-keys.protected")),
            ErrorKind::InvalidRequest,
        ));
    }
    keys.remove(id).ok_or_else(not_found)?;
    Ok(())
}

async fn add(ctx: RpcContext, params: AddTrustedKeyParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_trusted_keys_mut()
                .mutate(|keys| add_group(keys, uuid::Uuid::new_v4().to_string().into(), params))
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
    if keys.values().any(|group| group.public_keys.contains(&key)) {
        return Err(Error::new(
            eyre!("{}", t!("update.signers.already-exists")),
            ErrorKind::InvalidRequest,
        ));
    }
    add_group(
        keys,
        key.interned_pem(),
        AddTrustedKeyParams {
            name: label,
            public_keys: vec![key],
            context: TrustContext::Os,
        },
    )
}

pub fn legacy_update(
    keys: &mut TrustedKeys,
    key: &AnyVerifyingKey,
    label: String,
) -> Result<(), Error> {
    let group = keys
        .values_mut()
        .find(|group| group.os && group.public_keys.contains(key))
        .ok_or_else(not_found)?;
    group.name = group_name(label)?;
    Ok(())
}

pub fn legacy_remove(keys: &mut TrustedKeys, key: &AnyVerifyingKey) -> Result<(), Error> {
    let id = keys
        .iter()
        .find(|(_, group)| group.os && group.public_keys.contains(key))
        .map(|(id, _)| id.clone())
        .ok_or_else(not_found)?;
    if !keys[&id].removable {
        return remove_group(keys, &id);
    }
    let group = keys.get_mut(&id).unwrap();
    group.public_keys.retain(|k| k != key);
    if group.public_keys.is_empty() {
        remove_group(keys, &id)?;
    }
    Ok(())
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
        assert!(keys[START9].marketplace && keys[START9].os);
        assert!(!keys["start9-alpha"].marketplace && !keys["start9-alpha"].os);
        assert!(remove_group(&mut keys, START9).is_err());
        toggle_group(
            &mut keys,
            ToggleTrustedKeyParams {
                id: START9.into(),
                context: TrustContext::Os,
                enabled: false,
            },
        )
        .unwrap();
        assert!(keys[START9].marketplace);
        assert!(os_signers(&keys).is_empty());
        remove_group(&mut keys, "start9-alpha").unwrap();
        assert!(!keys.contains_key("start9-alpha"));
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
                AddTrustedKeyParams {
                    name: " ".into(),
                    public_keys: vec![public_key.clone()],
                    context: TrustContext::Os
                }
            )
            .is_err()
        );
        assert!(
            add_group(
                &mut keys,
                "invalid".into(),
                AddTrustedKeyParams {
                    name: "empty".into(),
                    public_keys: vec![],
                    context: TrustContext::Os
                }
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
                AddTrustedKeyParams {
                    name: " custom ".into(),
                    public_keys: vec![public_key.clone(), public_key.clone()],
                    context,
                },
            )
            .unwrap();
            assert_eq!(keys[id].name, "custom");
            assert_eq!(keys[id].public_keys, [public_key.clone()]);
            assert!(keys[id].marketplace);
            assert_eq!(keys[id].os, os);
        }
        assert!(
            serde_json::from_value::<AddTrustedKeyParams>(
                serde_json::json!({"name":"bad", "publicKeys":["not pem"], "context":"os"})
            )
            .is_err()
        );
    }

    #[test]
    fn migration_preserves_custom_and_removed_os_trust() {
        let legacy = initial_signers();
        let keys = migrate_signers(legacy.clone());
        assert_eq!(keys.len(), 2);
        assert_eq!(
            os_signers(&keys).into_keys().collect::<Vec<_>>(),
            legacy.into_keys().collect::<Vec<_>>()
        );
        let mut legacy = initial_signers();
        let removed = legacy.pop_first().unwrap().0;
        let keys = migrate_signers(legacy.clone());
        assert!(!keys[START9].os);
        assert!(!os_signers(&keys).contains_key(&removed));
        assert_eq!(os_signers(&keys), legacy);
        assert!(os_signers(&migrate_signers(TrustedOsUpdateSigners::new())).is_empty());
    }
}
