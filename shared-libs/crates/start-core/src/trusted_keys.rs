use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::context::{CliContext, RpcContext};
use crate::prelude::*;
use crate::registry::signer::{AcceptSigners, ContactInfo};
use crate::sign::AnyVerifyingKey;
use crate::sign::gpg::{GpgPublicKeyExport, ImportedPublicKeys, parse_public_keys};
use crate::update::signers::{TrustedOsUpdateSigner, TrustedOsUpdateSigners};
use crate::util::serde::HandlerExtSerde;
pub(crate) mod connection;
pub(crate) use connection::{ensure_builtin_registries, ui_registries};
pub use connection::{initial_os_registry, initial_registries};
pub(crate) mod legacy;

pub type TrustedKeys = BTreeMap<InternedString, TrustedKey>;
/// Connected registry URLs and their display names.
pub type Registries = BTreeMap<InternedString, Option<String>>;
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize, VisitVariants, clap::ValueEnum)]
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

pub(crate) fn initial_signer_certificates()
-> impl Iterator<Item = (&'static str, ImportedPublicKeys)> {
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
    protect_registry_keys(&mut keys, &BTreeMap::new(), &Registries::new());
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
    protect_registry_keys(
        &mut keys,
        &public.server_info.registry_keys,
        &public.server_info.registries,
    );
    public.server_info.trusted_keys = keys;
    Ok(res)
}

fn protected_keys(public: &crate::db::model::public::Public) -> TrustedKeys {
    let mut keys = public.server_info.trusted_keys.clone();
    protect_registry_keys(
        &mut keys,
        &public.server_info.registry_keys,
        &public.server_info.registries,
    );
    keys
}

pub(crate) fn protect_registry_keys(
    keys: &mut TrustedKeys,
    snapshots: &BTreeMap<InternedString, BTreeSet<AnyVerifyingKey>>,
    registries: &Registries,
) {
    let listed = snapshots
        .iter()
        .filter(|(url, _)| connection::is_connected(registries, url))
        .flat_map(|(_, keys)| keys.iter().cloned())
        .collect::<BTreeSet<_>>();
    for entry in keys.values_mut() {
        entry.removable = !listed.contains(&entry.public_key);
    }
}

pub fn api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async(list)
                .with_display_serializable()
                .with_about("about.list-trusted-keys")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "add",
            from_fn_async(add)
                .with_metadata("sync_db", Value::Bool(true))
                .with_display_serializable()
                .with_about("about.add-trusted-key")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "update",
            from_fn_async(update)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.update-trusted-key")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.remove-trusted-key")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove-many",
            from_fn_async(remove_many)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
}

#[derive(Deserialize, Serialize, clap::Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct AddTrustedKeyParams {
    /// Blank names each GPG key after its identity.
    #[arg(long, default_value = "", help = "help.arg.trusted-key-name")]
    pub name: String,
    /// Ed25519 public keys in PEM or armored GPG format.
    #[arg(required = true, help = "help.arg.trusted-public-keys")]
    pub public_keys: Vec<String>,
    /// Grants this permission to every imported key, including existing keys.
    #[serde(default)]
    #[arg(long, help = "help.arg.trust-for")]
    pub trust_for: Option<TrustContext>,
}

rpc_toolkit::reflect_ts!(AddTrustedKeyParams);

/// Changes a key's name and both trust selections atomically.
#[derive(Deserialize, Serialize, clap::Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct UpdateTrustedKeyParams {
    #[arg(help = "help.arg.trusted-key-id")]
    pub id: InternedString,
    #[arg(long, help = "help.arg.trusted-key-name")]
    pub name: String,
    #[arg(long, action = clap::ArgAction::Set, help = "help.arg.trust-marketplace")]
    pub marketplace: bool,
    #[arg(long, action = clap::ArgAction::Set, help = "help.arg.trust-os")]
    pub os: bool,
}

rpc_toolkit::reflect_ts!(UpdateTrustedKeyParams);

#[derive(Deserialize, Serialize, clap::Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct RemoveTrustedKeyParams {
    #[arg(help = "help.arg.trusted-key-id")]
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

async fn list(ctx: RpcContext) -> Result<TrustedKeys, Error> {
    ctx.db
        .peek()
        .await
        .into_public()
        .into_server_info()
        .into_trusted_keys()
        .de()
}

async fn add(ctx: RpcContext, params: AddTrustedKeyParams) -> Result<AddTrustedKeyRes, Error> {
    let public_keys = parse_public_keys(&params.public_keys)?;
    crate::update::signer_files::mutate(ctx.db.clone(), move |db| {
        db.as_public_mut().mutate(|public| {
            mutate_public_keys(public, |keys| {
                import_individual_keys(keys, params.name, public_keys, params.trust_for)
            })
        })
    })
    .await
}

async fn update(ctx: RpcContext, params: UpdateTrustedKeyParams) -> Result<(), Error> {
    crate::update::signer_files::mutate(ctx.db.clone(), move |db| {
        db.as_public_mut()
            .mutate(|public| mutate_public_keys(public, |keys| update_key(keys, params)))
    })
    .await
}

async fn remove(ctx: RpcContext, params: RemoveTrustedKeyParams) -> Result<(), Error> {
    crate::update::signer_files::mutate(ctx.db.clone(), move |db| {
        db.as_public_mut()
            .mutate(|public| mutate_public_keys(public, |keys| remove_key(keys, &params.id)))
    })
    .await
}

async fn remove_many(ctx: RpcContext, params: RemoveTrustedKeysParams) -> Result<(), Error> {
    crate::update::signer_files::mutate(ctx.db.clone(), move |db| {
        db.as_public_mut()
            .mutate(|public| mutate_public_keys(public, |keys| remove_keys(keys, params.ids)))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_trusted_key_commands() {
        use clap::Parser;
        let key = initial_signers().into_keys().next().unwrap().to_string();
        let add = AddTrustedKeyParams::try_parse_from([
            "add",
            key.as_str(),
            "--trust-for",
            "os",
            "--name",
            "Signer",
        ])
        .unwrap();
        assert_eq!(add.public_keys, [key.as_str()]);
        assert_eq!(add.name, "Signer");
        assert!(matches!(add.trust_for, Some(TrustContext::Os)));
        assert!(AddTrustedKeyParams::try_parse_from(["add"]).is_err());
        let update = UpdateTrustedKeyParams::try_parse_from([
            "update",
            "id",
            "--name",
            "Renamed",
            "--marketplace",
            "false",
            "--os",
            "true",
        ])
        .unwrap();
        assert_eq!(
            (
                update.id.as_ref(),
                update.name.as_str(),
                update.marketplace,
                update.os
            ),
            ("id", "Renamed", false, true)
        );
        assert!(
            UpdateTrustedKeyParams::try_parse_from(["update", "id", "--name", "Renamed"]).is_err()
        );
        assert_eq!(
            RemoveTrustedKeyParams::try_parse_from(["remove", "id"])
                .unwrap()
                .id,
            "id"
        );
    }

    #[test]
    fn cli_imports_armored_keys_after_the_option_delimiter() {
        use clap::Parser;
        let key = initial_signers().into_keys().next().unwrap();
        let pem = format!("-----BEGIN PUBLIC KEY-----\n{key}\n-----END PUBLIC KEY-----");
        let gpg = include_str!("sign/fixtures/ed25519-signers.asc");
        let add = AddTrustedKeyParams::try_parse_from([
            "add",
            "--name",
            "Signer",
            "--trust-for",
            "os",
            "--",
            pem.as_str(),
            gpg,
        ])
        .unwrap();
        assert_eq!(add.public_keys, [pem.as_str(), gpg]);
        let imported = parse_public_keys(&add.public_keys).unwrap();
        let mut keys = TrustedKeys::new();
        let result = import_individual_keys(&mut keys, add.name, imported, add.trust_for).unwrap();
        assert!(!result.added.is_empty());
        assert!(keys.values().all(|key| key.os && key.marketplace));
    }

    #[test]
    fn defaults_are_three_independent_start9_registry_keys() {
        let keys = initial_trusted_keys();
        assert_eq!(keys.len(), 3);
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
        remove_key(&mut keys, &removed.interned_der64()).unwrap();
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
