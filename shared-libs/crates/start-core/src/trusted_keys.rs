use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::context::RpcContext;
use crate::prelude::*;
use crate::registry::signer::{AcceptSigners, ContactInfo};
use crate::sign::AnyVerifyingKey;
use crate::sign::gpg::{GpgPublicKeyExport, ImportedPublicKeys, parse_public_keys};
use crate::update::signers::{TrustedOsUpdateSigner, TrustedOsUpdateSigners};
mod connection;
pub use connection::check_registry;
pub(crate) mod legacy;

pub type TrustedKeys = BTreeMap<InternedString, TrustedKey>;
pub const START9: &str = "start9";
pub const START9_REGISTRY: &str = "https://registry.start9.com";
const MANUAL: &str = "manual";

#[derive(Clone, Debug, Deserialize, Serialize, HasModel, VisitFields, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct RegistryIdentity {
    pub name: Option<String>,
    pub icon: Option<crate::util::DataUrl<'static>>,
}

rpc_toolkit::reflect_ts!(RegistryIdentity);
rpc_toolkit::ts_export!(RegistryIdentity, namespaces = [""]);

/// Signing-key material and independent package and OS-update trust.
#[derive(Clone, Debug, Deserialize, Serialize, HasModel, VisitFields, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct TrustedKey {
    pub name: String,
    /// `manual`, or the base URL of the registry that first offered the key.
    #[serde(default = "manual_origin")]
    pub origin: String,
    /// Registry identity recorded when this key was first imported.
    #[serde(default)]
    pub origin_registry: Option<RegistryIdentity>,
    pub public_key: AnyVerifyingKey,
    /// Original GPG exports; signing trust is determined by `public_key`.
    #[serde(default)]
    pub gpg_exports: BTreeSet<GpgPublicKeyExport>,
    /// Contact details registries publish for the key's signer.
    #[serde(default)]
    pub contact: BTreeSet<ContactInfo>,
    pub added_at: DateTime<Utc>,
    /// False while a connected registry offers the key.
    pub removable: bool,
    pub marketplace: bool,
    pub os: bool,
}

fn manual_origin() -> String {
    MANUAL.into()
}

impl TrustedKey {
    fn trusts_os_key(&self, key: &AnyVerifyingKey) -> bool {
        self.os && &self.public_key == key
    }

    /// The name in the key's first GPG user ID.
    fn identity_name(&self) -> Option<String> {
        let id = self.public_key.interned_der64();
        let user_id: &str = self
            .gpg_exports
            .iter()
            .filter_map(|export| export.identities.get(&id))
            .flatten()
            .next()?;
        let name = user_id
            .split_once('<')
            .map_or(user_id, |(name, _)| name)
            .trim();
        Some(
            if name.is_empty() {
                user_id.trim()
            } else {
                name
            }
            .to_owned(),
        )
    }
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
rpc_toolkit::ts_export!(TrustContext, namespaces = [""]);

pub fn enabled_keys(keys: &TrustedKeys, context: TrustContext) -> Vec<AnyVerifyingKey> {
    keys.values()
        .filter(|key| match context {
            TrustContext::Marketplace => key.marketplace,
            TrustContext::Os => key.os,
        })
        .map(|key| key.public_key.clone())
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

fn initial_signer_certificates() -> impl Iterator<Item = (&'static str, ImportedPublicKeys)> {
    [
        ("drbonez", include_str!("trusted_keys/initial/aiden.asc")),
        (
            "matthill",
            include_str!("trusted_keys/initial/matthill.asc"),
        ),
        ("Start9", include_str!("trusted_keys/initial/dev.asc")),
    ]
    .into_iter()
    .map(|(label, armored)| (label, parse_public_keys(&[armored.into()]).unwrap()))
}

pub fn initial_signers() -> TrustedOsUpdateSigners {
    initial_signer_certificates()
        .flat_map(|(label, imported)| {
            imported.keys.into_iter().map(move |key| {
                (
                    key.interned_der64(),
                    TrustedOsUpdateSigner {
                        label: label.to_owned(),
                    },
                )
            })
        })
        .collect()
}

pub fn initial_trusted_keys() -> TrustedKeys {
    let mut keys = legacy::individual_keys(&legacy::initial_keyrings(), Utc::now()).unwrap();
    protect_registry_keys(&mut keys, &BTreeMap::new(), &Value::Null);
    keys
}

pub fn os_signers(keys: &TrustedKeys) -> TrustedOsUpdateSigners {
    keys.values()
        .filter(|key| key.os)
        .map(|key| {
            (
                key.public_key.interned_der64(),
                TrustedOsUpdateSigner {
                    label: key.name.clone(),
                },
            )
        })
        .collect()
}

fn split_import(
    name: String,
    imported: ImportedPublicKeys,
    added_at: DateTime<Utc>,
) -> Result<TrustedKeys, Error> {
    let exports = parse_public_keys(
        &imported
            .gpg_exports
            .into_iter()
            .map(|export| export.armored)
            .collect::<Vec<_>>(),
    )?
    .gpg_exports;
    Ok(imported
        .keys
        .into_iter()
        .map(|public_key| {
            let id = public_key.interned_der64();
            let gpg_exports = exports
                .iter()
                .filter(|export| export.identities.contains_key(&id))
                .cloned()
                .collect();
            (
                id,
                TrustedKey {
                    name: name.clone(),
                    origin: manual_origin(),
                    origin_registry: None,
                    public_key,
                    gpg_exports,
                    contact: BTreeSet::new(),
                    added_at,
                    removable: true,
                    marketplace: false,
                    os: false,
                },
            )
        })
        .collect())
}

/// Keys an import added, and keys it found already present.
#[derive(Debug, Default, Deserialize, Serialize, VisitFields, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddTrustedKeyRes {
    /// Trusted for packages.
    pub added: BTreeSet<InternedString>,
    /// GPG identities merged; explicit `trustFor` grants the requested permission.
    pub existing: BTreeSet<InternedString>,
}

rpc_toolkit::reflect_ts!(AddTrustedKeyRes);
rpc_toolkit::ts_export!(AddTrustedKeyRes, namespaces = [""]);

fn import_individual_keys(
    keys: &mut TrustedKeys,
    name: String,
    imported: ImportedPublicKeys,
    trust_for: Option<TrustContext>,
) -> Result<AddTrustedKeyRes, Error> {
    if imported.keys.is_empty() {
        return Err(invalid_key());
    }
    let mut imported = split_import(name.trim().to_owned(), imported, Utc::now())?;
    for key in imported.values_mut() {
        if key.name.is_empty() {
            key.name = key.identity_name().ok_or_else(invalid_key)?;
        }
        key.marketplace = true;
    }
    let (existing, added): (BTreeSet<_>, BTreeSet<_>) = imported
        .keys()
        .cloned()
        .partition(|id| keys.contains_key(id));
    merge_individual_keys(keys, imported);
    if let Some(context) = trust_for {
        for id in added.iter().chain(&existing) {
            let key = keys.get_mut(id).unwrap();
            match context {
                TrustContext::Marketplace => key.marketplace = true,
                TrustContext::Os => key.os = true,
            }
        }
    }
    Ok(AddTrustedKeyRes { added, existing })
}

fn merge_individual_keys(keys: &mut TrustedKeys, imported: TrustedKeys) {
    for (id, key) in imported {
        if let Some(existing) = keys.get_mut(&id) {
            existing.gpg_exports.extend(key.gpg_exports);
            existing.contact.extend(key.contact);
        } else {
            keys.insert(id, key);
        }
    }
}

pub(crate) fn mutate_public_keys<T>(
    public: &mut crate::db::model::public::Public,
    mutate: impl FnOnce(&mut TrustedKeys) -> Result<T, Error>,
) -> Result<T, Error> {
    let mut keys = protected_keys(public);
    let res = mutate(&mut keys)?;
    if !keys.values().any(|key| key.os)
        && public.server_info.trusted_keys.values().any(|key| key.os)
    {
        return Err(Error::new(
            eyre!("{}", t!("trusted-keys.os-key-required")),
            ErrorKind::InvalidRequest,
        ));
    }
    protect_registry_keys(&mut keys, &public.server_info.registry_keys, &public.ui);
    public.server_info.trusted_keys = keys;
    Ok(res)
}

fn protected_keys(public: &crate::db::model::public::Public) -> TrustedKeys {
    let mut keys = public.server_info.trusted_keys.clone();
    protect_registry_keys(&mut keys, &public.server_info.registry_keys, &public.ui);
    keys
}

pub(crate) fn protect_registry_keys(
    keys: &mut TrustedKeys,
    registries: &BTreeMap<InternedString, BTreeSet<AnyVerifyingKey>>,
    ui: &Value,
) {
    let listed = registries
        .iter()
        .filter(|(url, _)| connection::is_connected(ui, url))
        .flat_map(|(_, keys)| keys.iter().cloned())
        .collect::<BTreeSet<_>>();
    for entry in keys.values_mut() {
        entry.removable = !listed.contains(&entry.public_key);
    }
}

pub fn api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "remove-registry",
            from_fn_async(connection::remove)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "connect-registry",
            from_fn_async(connection::connect)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "set-os-registry",
            from_fn_async(connection::set_os_registry)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "add",
            from_fn_async(add)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "update",
            from_fn_async(update)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "remove-many",
            from_fn_async(remove_many)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
}

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct AddTrustedKeyParams {
    /// Blank names each GPG key after its identity.
    pub name: String,
    /// Ed25519 public keys in PEM or armored GPG format.
    pub public_keys: Vec<String>,
    /// Grants this permission to every imported key, including existing keys.
    #[serde(default)]
    pub trust_for: Option<TrustContext>,
}

rpc_toolkit::reflect_ts!(AddTrustedKeyParams);

/// Changes a key's name and both trust selections atomically.
#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTrustedKeyParams {
    pub id: InternedString,
    pub name: String,
    pub marketplace: bool,
    pub os: bool,
}

rpc_toolkit::reflect_ts!(UpdateTrustedKeyParams);

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct RemoveTrustedKeyParams {
    pub id: InternedString,
}

rpc_toolkit::reflect_ts!(RemoveTrustedKeyParams);

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct RemoveTrustedKeysParams {
    pub ids: BTreeSet<InternedString>,
}

rpc_toolkit::reflect_ts!(RemoveTrustedKeysParams);

fn not_found() -> Error {
    Error::new(
        eyre!("{}", t!("trusted-keys.not-found")),
        ErrorKind::NotFound,
    )
}

fn invalid_key() -> Error {
    Error::new(
        eyre!("{}", t!("trusted-keys.invalid")),
        ErrorKind::InvalidRequest,
    )
}

fn key_name(name: String) -> Result<String, Error> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(invalid_key());
    }
    Ok(name)
}

fn update_key(keys: &mut TrustedKeys, params: UpdateTrustedKeyParams) -> Result<(), Error> {
    let name = key_name(params.name)?;
    let key = keys.get_mut(&params.id).ok_or_else(not_found)?;
    key.name = name;
    key.marketplace = params.marketplace;
    key.os = params.os;
    Ok(())
}

pub fn remove_key(keys: &mut TrustedKeys, id: &str) -> Result<(), Error> {
    if !keys.get(id).ok_or_else(not_found)?.removable {
        return Err(Error::new(
            eyre!("{}", t!("trusted-keys.protected")),
            ErrorKind::InvalidRequest,
        ));
    }
    keys.remove(id).ok_or_else(not_found)?;
    Ok(())
}

fn remove_keys(keys: &mut TrustedKeys, ids: BTreeSet<InternedString>) -> Result<(), Error> {
    for id in ids {
        remove_key(keys, &id)?;
    }
    Ok(())
}

async fn add(ctx: RpcContext, params: AddTrustedKeyParams) -> Result<AddTrustedKeyRes, Error> {
    let public_keys = parse_public_keys(&params.public_keys)?;
    ctx.db
        .mutate(|db| {
            db.as_public_mut().mutate(|public| {
                mutate_public_keys(public, |keys| {
                    import_individual_keys(keys, params.name, public_keys, params.trust_for)
                })
            })
        })
        .await
        .result
}

async fn update(ctx: RpcContext, params: UpdateTrustedKeyParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .mutate(|public| mutate_public_keys(public, |keys| update_key(keys, params)))
        })
        .await
        .result
}

async fn remove(ctx: RpcContext, params: RemoveTrustedKeyParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .mutate(|public| mutate_public_keys(public, |keys| remove_key(keys, &params.id)))
        })
        .await
        .result
}

async fn remove_many(ctx: RpcContext, params: RemoveTrustedKeysParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .mutate(|public| mutate_public_keys(public, |keys| remove_keys(keys, params.ids)))
        })
        .await
        .result
}

pub fn legacy_add(
    keys: &mut TrustedKeys,
    key: AnyVerifyingKey,
    label: String,
) -> Result<(), Error> {
    let id = key.interned_der64();
    if let Some(existing) = keys.get_mut(&id) {
        if existing.os {
            return Err(Error::new(
                eyre!("{}", t!("update.signers.already-exists")),
                ErrorKind::InvalidRequest,
            ));
        }
        existing.os = true;
        return Ok(());
    }
    import_individual_keys(keys, label, vec![key].into(), None)?;
    if let Some(added) = keys.get_mut(&id) {
        added.os = true;
    }
    Ok(())
}

pub fn legacy_update(
    keys: &mut TrustedKeys,
    key: &AnyVerifyingKey,
    label: String,
) -> Result<(), Error> {
    let entry = keys
        .get_mut(&key.interned_der64())
        .filter(|entry| entry.trusts_os_key(key))
        .ok_or_else(not_found)?;
    entry.name = key_name(label)?;
    Ok(())
}

pub fn legacy_remove(keys: &mut TrustedKeys, key: &AnyVerifyingKey) -> Result<(), Error> {
    let id = key.interned_der64();
    let entry = keys
        .get_mut(&id)
        .filter(|entry| entry.trusts_os_key(key))
        .ok_or_else(not_found)?;
    if entry.removable {
        keys.remove(&id);
    } else {
        entry.os = false;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_five_independent_start9_registry_keys() {
        let keys = initial_trusted_keys();
        assert_eq!(keys.len(), 5);
        assert_eq!(enabled_keys(&keys, TrustContext::Marketplace).len(), 3);
        assert_eq!(enabled_keys(&keys, TrustContext::Os).len(), 3);
        assert_eq!(
            os_signers(&keys).into_keys().collect::<BTreeSet<_>>(),
            initial_signers().into_keys().collect()
        );
        for (id, key) in &keys {
            assert_eq!(*id, key.public_key.interned_der64());
            assert_eq!(key.origin, START9_REGISTRY);
            assert!(key.removable);
            assert_eq!(key.marketplace, key.os);
            for export in &key.gpg_exports {
                assert!(export.identities.contains_key(id));
                assert!(export.key_fingerprints.contains_key(id));
            }
        }
        let identities = keys
            .values()
            .flat_map(|key| key.gpg_exports.iter())
            .flat_map(|export| export.user_ids.iter().map(String::as_str))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            identities,
            BTreeSet::from([
                "Matt Hill <matthill@start9.com>",
                "Aiden McClelland <me@drbonez.dev>",
                "Aiden McClelland <aiden@start9.com>",
                "Start9 <security@start9.com>",
                "Start9 <dev@start9.com>",
            ])
        );
    }

    #[test]
    fn duplicate_import_enriches_metadata_without_reenabling_or_relabeling() {
        let imported =
            parse_public_keys(&[include_str!("trusted_keys/initial/matthill.asc").into()]).unwrap();
        let public_key = imported.keys.first().unwrap().clone();
        let id = public_key.interned_der64();
        let mut keys = TrustedKeys::new();
        let res =
            import_individual_keys(&mut keys, "Original".into(), vec![public_key].into(), None)
                .unwrap();
        assert_eq!(res.added, BTreeSet::from([id.clone()]));
        let entry = keys.get_mut(&id).unwrap();
        entry.marketplace = false;
        entry.origin = "https://registry.example".into();
        let added_at = entry.added_at;
        let res = import_individual_keys(&mut keys, "Replacement".into(), imported, None).unwrap();
        assert_eq!(res.existing, BTreeSet::from([id.clone()]));
        assert!(res.added.is_empty());
        let key = &keys[&id];
        assert!(!key.marketplace && !key.os);
        assert_eq!(key.name, "Original");
        assert_eq!(key.origin, "https://registry.example");
        assert_eq!(key.added_at, added_at);
        assert_eq!(key.gpg_exports.len(), 1);
        remove_key(&mut keys, &id).unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn import_grant_is_optional_on_the_wire() {
        let params: AddTrustedKeyParams = from_value(imbl_value::json!({
            "name": "Publisher", "publicKeys": []
        }))
        .unwrap();
        assert!(params.trust_for.is_none());
        let params: AddTrustedKeyParams = from_value(imbl_value::json!({
            "name": "Publisher", "publicKeys": [], "trustFor": "os"
        }))
        .unwrap();
        assert!(matches!(params.trust_for, Some(TrustContext::Os)));
    }

    #[test]
    fn editing_changes_one_keys_name_and_contexts_without_changing_material() {
        let mut keys = initial_trusted_keys();
        let id = keys.keys().next().unwrap().clone();
        keys.get_mut(&id).unwrap().removable = false;
        let saved = keys[&id].clone();
        let mut neighbors = keys.clone();
        neighbors.remove(&id);
        update_key(
            &mut keys,
            UpdateTrustedKeyParams {
                id: id.clone(),
                name: " Renamed ".into(),
                marketplace: false,
                os: true,
            },
        )
        .unwrap();
        assert_eq!(
            keys[&id],
            TrustedKey {
                name: "Renamed".into(),
                marketplace: false,
                os: true,
                ..saved
            }
        );
        assert!(remove_key(&mut keys, &id).is_err());
        keys.remove(&id);
        assert_eq!(keys, neighbors);
    }

    #[test]
    fn rejected_imports_and_edits_leave_trust_unchanged() {
        let mut keys = initial_trusted_keys();
        let saved = keys.clone();
        let public_key = keys.values().next().unwrap().public_key.clone();
        for (name, imported) in [(" ", vec![public_key]), ("Empty", vec![])] {
            assert!(import_individual_keys(&mut keys, name.into(), imported.into(), None).is_err());
            assert_eq!(keys, saved);
        }
        for (id, name) in [
            (keys.keys().next().unwrap().clone(), " "),
            ("missing".into(), "Name"),
        ] {
            assert!(
                update_key(
                    &mut keys,
                    UpdateTrustedKeyParams {
                        id,
                        name: name.into(),
                        marketplace: false,
                        os: false
                    }
                )
                .is_err()
            );
            assert_eq!(keys, saved);
        }
    }

    #[test]
    fn blank_import_names_take_the_gpg_identity_name() {
        let imported =
            parse_public_keys(&[include_str!("sign/fixtures/ed25519-signers.asc").into()]).unwrap();
        let mut keys = TrustedKeys::new();
        import_individual_keys(&mut keys, " ".into(), imported, None).unwrap();
        assert!(!keys.is_empty());
        for key in keys.values() {
            assert!(!key.name.is_empty());
            assert_eq!(Some(key.name.clone()), key.identity_name());
        }
    }

    #[test]
    fn imports_trust_packages_and_keep_singular_wire_material() {
        let imported =
            parse_public_keys(&[include_str!("sign/fixtures/ed25519-signers.asc").into()]).unwrap();
        let expected = imported.keys.clone();
        let mut keys = TrustedKeys::new();
        let res = import_individual_keys(&mut keys, " Publisher ".into(), imported, None).unwrap();
        assert_eq!(keys.len(), 2);
        assert_eq!(res.added, keys.keys().cloned().collect());
        for (id, key) in &keys {
            assert_eq!(key.name, "Publisher");
            assert!(expected.contains(&key.public_key));
            assert!(key.marketplace && !key.os && key.removable);
            let wire = to_value(key).unwrap();
            assert_eq!(wire["publicKey"], to_value(&key.public_key).unwrap());
            assert!(wire.get("publicKeys").is_none());
            assert_eq!(*id, key.public_key.interned_der64());
        }
        assert_eq!(
            from_value::<TrustedKeys>(to_value(&keys).unwrap()).unwrap(),
            keys
        );
    }

    #[test]
    fn retaining_a_full_certificate_does_not_restore_a_removed_signing_key() {
        let source = include_str!("sign/fixtures/ed25519-signers.asc");
        let imported = parse_public_keys(&[source.into()]).unwrap();
        let removed = imported.keys.first().unwrap().clone();
        let mut keys = TrustedKeys::new();
        import_individual_keys(&mut keys, "Publisher".into(), imported, None).unwrap();
        for key in keys.values_mut() {
            key.os = true;
        }
        legacy_remove(&mut keys, &removed).unwrap();
        let wire = to_value(&keys).unwrap();
        let restored: TrustedKeys = from_value(wire.clone()).unwrap();
        assert_eq!(restored.len(), 1);
        for context in [TrustContext::Marketplace, TrustContext::Os] {
            assert!(!enabled_keys(&restored, context).contains(&removed));
        }
        assert_eq!(
            restored
                .values()
                .next()
                .unwrap()
                .gpg_exports
                .first()
                .unwrap()
                .armored,
            source
        );
        assert_eq!(legacy::decode_keys(wire, Utc::now()).unwrap(), restored);
    }

    #[test]
    fn legacy_commands_use_single_key_identity_and_preserve_other_keys() {
        let mut keys = initial_trusted_keys();
        let id = keys.iter().find(|(_, key)| key.os).unwrap().0.clone();
        keys.get_mut(&id).unwrap().removable = false;
        let original = keys.clone();
        let key = keys[&id].public_key.clone();
        legacy_remove(&mut keys, &key).unwrap();
        assert!(!keys[&id].os && keys[&id].marketplace);
        assert!(legacy_remove(&mut keys, &key).is_err());
        legacy_add(&mut keys, key.clone(), "Ignored".into()).unwrap();
        assert_eq!(keys, original);
        assert!(legacy_add(&mut keys, key.clone(), "Duplicate".into()).is_err());
        legacy_update(&mut keys, &key, "Renamed".into()).unwrap();
        assert_eq!(keys.remove(&id).unwrap().name, "Renamed");
        let mut neighbors = original;
        neighbors.remove(&id);
        assert_eq!(keys, neighbors);
    }

    #[test]
    fn current_decoder_requires_a_singular_key_and_migration_accepts_historical_groups() {
        let key = initial_signers().into_keys().next().unwrap();
        let historical = imbl_value::json!({ "name": "Publisher", "publicKeys": [key], "removable": true, "marketplace": true, "os": false });
        assert!(from_value::<TrustedKey>(historical.clone()).is_err());
        let migrated =
            legacy::decode_keys(imbl_value::json!({ "legacy": historical }), Utc::now()).unwrap();
        assert_eq!(migrated.len(), 1);
        let current = &migrated[&key];
        assert_eq!(current.public_key.interned_der64(), key);
        assert_eq!(current.origin, "manual");
        assert!(current.origin_registry.is_none());
    }

    #[test]
    fn trust_override_is_explicit_and_empty_trust_does_not_bypass_policy() {
        let keys = TrustedKeys::new();
        for context in [TrustContext::Marketplace, TrustContext::Os] {
            assert!(required_keys(&keys, context, true).is_none());
            assert!(
                signer_policy(required_keys(&keys, context, false))
                    .unwrap()
                    .try_accept()
                    .is_err()
            );
        }
    }

    #[cfg(feature = "ts")]
    #[test]
    fn current_schema_exports_singular_key_without_historical_keyrings() {
        let mut visitor = rpc_toolkit::ts::TSVisitor::new();
        visitor.append_type::<TrustedKey>();
        let schema = visitor.into_bindings(None, "./schemas").unwrap().schemas;
        assert!(
            schema.contains("\"publicKey\": AnyVerifyingKey"),
            "{schema}"
        );
        assert!(!schema.contains("\"publicKeys\""), "{schema}");
        assert!(!schema.contains("TrustedKeyring"), "{schema}");
    }
}
