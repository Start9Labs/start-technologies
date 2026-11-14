use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use clap::Parser;
use exver::VersionRange;
use itertools::Itertools;
use rpc_toolkit::{Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::context::CliContext;
use crate::middleware::auth::signature::{HasUnenrolledKeys, SignatureAuthorizationContext};
use crate::prelude::*;
use crate::registry::RegistryDatabase;
use crate::registry::context::RegistryContext;
use crate::registry::signer::{ContactInfo, SignerInfo};
use crate::rpc_continuations::Guid;
use crate::sign::AnyVerifyingKey;
use crate::sign::gpg::{GpgPublicKeyExport, parse_public_keys};
use crate::util::serde::{HandlerExtSerde, WithIoFormat, display_serializable};

pub fn admin_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "signer",
            signers_api::<C>().with_about("about.commands-add-list-signers"),
        )
        .subcommand(
            "add",
            from_fn_async(add_admin)
                .with_metadata("admin", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "add",
            from_fn_async(cli_add_admin)
                .no_ts()
                .no_display()
                .with_about("about.add-admin-signer"),
        )
        .subcommand(
            "remove",
            from_fn_async(remove_admin)
                .with_metadata("admin", Value::Bool(true))
                .no_display()
                .with_about("about.remove-admin-signer")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "list",
            from_fn_async(list_admins)
                .with_metadata("admin", Value::Bool(true))
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| display_signers(handle.params, result))
                .with_about("about.list-admin-signers")
                .with_call_remote::<CliContext>(),
        )
}

fn signers_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async(list_signers)
                .with_metadata("admin", Value::Bool(true))
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| display_signers(handle.params, result))
                .with_about("about.list-signers")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "add",
            from_fn_async(add_signer)
                .with_metadata("admin", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "add",
            from_fn_async(cli_add_signer)
                .no_ts()
                .with_about("about.add-signer"),
        )
        .subcommand(
            "edit",
            from_fn_async(edit_signer)
                .with_metadata("admin", Value::Bool(true))
                .no_display()
                .with_about("about.edit-signer")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove_signer)
                .with_metadata("admin", Value::Bool(true))
                .no_display()
                .with_about("about.remove-signer")
                .with_call_remote::<CliContext>(),
        )
}

impl Model<BTreeMap<Guid, SignerInfo>> {
    pub fn find_signer(&self, key: &AnyVerifyingKey) -> Result<Option<Guid>, Error> {
        Ok(self
            .as_entries()?
            .into_iter()
            .map(|(guid, s)| Ok::<_, Error>((guid, s.as_keys().de()?)))
            .filter_ok(|(_, s)| s.contains(key))
            .next()
            .transpose()?
            .map(|(a, _)| a))
    }

    pub fn get_signer(&self, key: &AnyVerifyingKey) -> Result<Guid, Error> {
        self.find_signer(key)?.ok_or_else(|| {
            Error::new(
                eyre!("{}", t!("registry.admin.unknown-signer")),
                ErrorKind::Authorization,
            )
        })
    }

    pub fn get_signer_info(&self, key: &AnyVerifyingKey) -> Result<(Guid, SignerInfo), Error> {
        self.as_entries()?
            .into_iter()
            .map(|(guid, s)| Ok::<_, Error>((guid, s.de()?)))
            .filter_ok(|(_, s)| s.keys.contains(key))
            .next()
            .transpose()?
            .ok_or_else(|| {
                Error::new(
                    eyre!("{}", t!("registry.admin.unknown-signer")),
                    ErrorKind::Authorization,
                )
            })
    }

    pub fn add_signer(&mut self, signer: &SignerInfo) -> Result<Guid, Error> {
        if let Some((guid, s)) = self
            .as_entries()?
            .into_iter()
            .map(|(guid, s)| Ok::<_, Error>((guid, s.de()?)))
            .filter_ok(|(_, s)| !s.keys.is_disjoint(&signer.keys))
            .next()
            .transpose()?
        {
            return Err(Error::new(
                eyre!(
                    "{}",
                    t!(
                        "registry.admin.signer-already-exists",
                        guid = guid,
                        name = s.name
                    )
                ),
                ErrorKind::InvalidRequest,
            ));
        }
        let id = Guid::new();
        self.insert(&id, signer)?;
        Ok(id)
    }
}

pub async fn list_signers(ctx: RegistryContext) -> Result<BTreeMap<Guid, SignerInfo>, Error> {
    ctx.db.peek().await.into_index().into_signers().de()
}

pub fn display_signers<T>(
    params: WithIoFormat<T>,
    signers: BTreeMap<Guid, SignerInfo>,
) -> Result<(), Error> {
    use prettytable::*;

    if let Some(format) = params.format {
        return display_serializable(format, signers);
    }

    let mut table = Table::new();
    table.add_row(row![bc =>
        "ID",
        "NAME",
        "CONTACT",
        "KEYS",
    ]);
    for (id, info) in signers {
        table.add_row(row![
            id.as_ref(),
            &info.name,
            &info.contact.into_iter().join("\n"),
            &info.keys.into_iter().join("\n"),
        ]);
    }
    print_table(&table)?;
    Ok(())
}

pub fn display_package_signers<T>(
    params: WithIoFormat<T>,
    signers: BTreeMap<Guid, (SignerInfo, VersionRange)>,
) -> Result<(), Error> {
    use prettytable::*;

    if let Some(format) = params.format {
        return display_serializable(format, signers);
    }

    let mut table = Table::new();
    table.add_row(row![bc =>
        "ID",
        "NAME",
        "CONTACT",
        "KEYS",
        "AUTHORIZED VERSIONS"
    ]);
    for (id, (info, versions)) in signers {
        table.add_row(row![
            id.as_ref(),
            &info.name,
            &info.contact.into_iter().join("\n"),
            &info.keys.into_iter().join("\n"),
            &versions.to_string()
        ]);
    }
    print_table(&table)?;
    Ok(())
}

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct AddSignerParams {
    pub name: String,
    pub contact: Vec<ContactInfo>,
    /// Ed25519 public keys in PEM, canonical DER Base64, or armored OpenPGP format.
    pub keys: Vec<String>,
    /// Retained certificates; `keys` determines the selected signing keys.
    #[serde(default)]
    pub gpg_exports: BTreeSet<GpgPublicKeyExport>,
}

rpc_toolkit::reflect_ts!(AddSignerParams);

impl AddSignerParams {
    fn into_signer(self) -> Result<SignerInfo, Error> {
        let mut signer = SignerInfo {
            name: self.name,
            contact: self.contact,
            keys: Default::default(),
            gpg_exports: Default::default(),
        };
        signer.import_keys(&self.keys)?;
        signer.gpg_exports.extend(
            parse_public_keys(
                &self
                    .gpg_exports
                    .into_iter()
                    .map(|export| export.armored)
                    .collect::<Vec<_>>(),
            )?
            .gpg_exports,
        );
        Ok(signer)
    }
}

pub async fn add_signer(ctx: RegistryContext, params: AddSignerParams) -> Result<Guid, Error> {
    let signer = params.into_signer()?;
    ctx.db
        .mutate(|db| db.as_index_mut().as_signers_mut().add_signer(&signer))
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct EditSignerParams {
    #[arg(help = "help.arg.signer-id")]
    pub id: Guid,
    #[arg(short = 'n', long, help = "help.arg.set-signer-name")]
    pub set_name: Option<String>,
    #[arg(short = 'c', long, help = "help.arg.add-signer-contact")]
    pub add_contact: Vec<ContactInfo>,
    #[arg(
        short = 'k',
        long,
        help = "help.arg.add-signer-key",
        allow_hyphen_values = true
    )]
    pub add_key: Vec<String>,
    #[arg(short = 'C', long, help = "help.arg.remove-signer-contact")]
    pub remove_contact: Vec<ContactInfo>,
    #[arg(short = 'K', long, help = "help.arg.remove-signer-key")]
    pub remove_key: Vec<AnyVerifyingKey>,
    /// Removes whole exports containing a matching primary-key fingerprint, preserving signing keys.
    #[serde(default)]
    #[arg(
        long,
        value_name = "PRIMARY_FINGERPRINT",
        help = "help.arg.remove-signer-gpg-export"
    )]
    pub remove_gpg_export: Vec<String>,
}

rpc_toolkit::reflect_ts!(EditSignerParams);
rpc_toolkit::ts_export!(EditSignerParams, namespaces = [""]);

impl EditSignerParams {
    fn apply(self, signer: &mut SignerInfo) -> Result<Vec<InternedString>, Error> {
        signer.import_keys(&self.add_key)?;
        if let Some(name) = self.set_name {
            signer.name = name;
        }
        signer.contact.extend(self.add_contact);
        for rm in self.remove_contact {
            if let Some(idx) = signer.contact.iter().position(|contact| contact == &rm) {
                signer.contact.remove(idx);
            }
        }
        signer.gpg_exports.retain(|export| {
            !export.fingerprints.iter().any(|fingerprint| {
                self.remove_gpg_export
                    .iter()
                    .any(|remove| fingerprint.eq_ignore_ascii_case(remove))
            })
        });
        Ok(self
            .remove_key
            .into_iter()
            .filter_map(|key| signer.keys.remove(&key).then(|| key.interned_der64()))
            .collect())
    }
}

pub async fn edit_signer(ctx: RegistryContext, params: EditSignerParams) -> Result<(), Error> {
    let continuations = ctx.open_authed_continuations();
    let ephemeral = ctx.ephemeral_auth_keys();
    ctx.db
        .mutate(|db| {
            let removed = db
                .as_index_mut()
                .as_signers_mut()
                .as_idx_mut(&params.id)
                .or_not_found(&params.id)?
                .mutate(|signer| params.apply(signer))?;
            HasUnenrolledKeys::unenroll::<RegistryContext>(continuations, ephemeral, db, removed)?;
            Ok(())
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct RemoveSignerParams {
    #[arg(help = "help.arg.signer-id")]
    pub id: Guid,
}

rpc_toolkit::reflect_ts!(RemoveSignerParams);
rpc_toolkit::ts_export!(RemoveSignerParams, namespaces = [""]);

pub async fn remove_signer(
    ctx: RegistryContext,
    RemoveSignerParams { id }: RemoveSignerParams,
) -> Result<(), Error> {
    let continuations = ctx.open_authed_continuations();
    let ephemeral = ctx.ephemeral_auth_keys();
    ctx.db
        .mutate(|db| {
            let Some(signer) = db.as_index_mut().as_signers_mut().remove(&id)? else {
                return Err(Error::new(
                    eyre!("{}", t!("registry.admin.unknown-signer")),
                    ErrorKind::NotFound,
                ));
            };
            HasUnenrolledKeys::unenroll::<RegistryContext>(
                continuations,
                ephemeral,
                db,
                signer.de()?.keys.iter().map(|k| k.interned_der64()),
            )?;

            for (_, pkg) in db
                .as_index_mut()
                .as_package_mut()
                .as_packages_mut()
                .as_entries_mut()?
            {
                pkg.as_authorized_mut().remove(&id)?;
            }

            for (_, version) in db
                .as_index_mut()
                .as_os_mut()
                .as_versions_mut()
                .as_entries_mut()?
            {
                version.as_authorized_mut().mutate(|s| Ok(s.remove(&id)))?;
            }

            db.as_admins_mut().mutate(|a| Ok(a.remove(&id)))?;

            Ok(())
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct CliAddSignerParams {
    #[arg(long = "name", short = 'n', help = "help.arg.signer-name")]
    pub name: String,
    #[arg(long = "contact", short = 'c', help = "help.arg.signer-contact")]
    pub contact: Vec<ContactInfo>,
    #[arg(long = "key", help = "help.arg.signer-key", allow_hyphen_values = true)]
    pub keys: Vec<String>,
    #[arg(help = "help.arg.database-path")]
    pub database: Option<PathBuf>,
}

pub async fn cli_add_signer(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        params:
            CliAddSignerParams {
                name,
                contact,
                keys,
                database,
            },
        ..
    }: HandlerArgs<CliContext, CliAddSignerParams>,
) -> Result<Guid, Error> {
    let params = AddSignerParams {
        name,
        contact,
        keys,
        gpg_exports: BTreeSet::new(),
    };
    if let Some(database) = database {
        let signer = params.into_signer()?;
        TypedPatchDb::<RegistryDatabase>::load(PatchDb::open(database).await?)
            .await?
            .mutate(|db| db.as_index_mut().as_signers_mut().add_signer(&signer))
            .await
            .result
    } else {
        from_value(
            ctx.call_remote::<RegistryContext>(
                &parent_method.into_iter().chain(method).join("."),
                to_value(&params)?,
            )
            .await?,
        )
    }
}

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct AddAdminParams {
    pub signer: Guid,
}

rpc_toolkit::reflect_ts!(AddAdminParams);
rpc_toolkit::ts_export!(AddAdminParams, namespaces = [""]);

pub async fn add_admin(
    ctx: RegistryContext,
    AddAdminParams { signer }: AddAdminParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            ensure_code!(
                db.as_index().as_signers().contains_key(&signer)?,
                ErrorKind::InvalidRequest,
                "unknown signer {signer}"
            );
            db.as_admins_mut().mutate(|a| Ok(a.insert(signer)))?;
            Ok(())
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct RemoveAdminParams {
    #[arg(help = "help.arg.signer-id")]
    pub signer: Guid,
}

rpc_toolkit::reflect_ts!(RemoveAdminParams);
rpc_toolkit::ts_export!(RemoveAdminParams, namespaces = [""]);

pub async fn remove_admin(
    ctx: RegistryContext,
    RemoveAdminParams { signer }: RemoveAdminParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_admins_mut().mutate(|a| Ok(a.remove(&signer)))?;
            Ok(())
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct CliAddAdminParams {
    #[arg(help = "help.arg.signer-id")]
    pub signer: Guid,
    #[arg(help = "help.arg.database-path")]
    pub database: Option<PathBuf>,
}

pub async fn cli_add_admin(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        params: CliAddAdminParams { signer, database },
        ..
    }: HandlerArgs<CliContext, CliAddAdminParams>,
) -> Result<(), Error> {
    if let Some(database) = database {
        TypedPatchDb::<RegistryDatabase>::load(PatchDb::open(database).await?)
            .await?
            .mutate(|db| {
                ensure_code!(
                    db.as_index().as_signers().contains_key(&signer)?,
                    ErrorKind::InvalidRequest,
                    "unknown signer {signer}"
                );
                db.as_admins_mut().mutate(|a| Ok(a.insert(signer)))?;
                Ok(())
            })
            .await
            .result?;
    } else {
        ctx.call_remote::<RegistryContext>(
            &parent_method.into_iter().chain(method).join("."),
            to_value(&AddAdminParams { signer })?,
        )
        .await?;
    }
    Ok(())
}

pub async fn list_admins(ctx: RegistryContext) -> Result<BTreeMap<Guid, SignerInfo>, Error> {
    let db = ctx.db.peek().await;
    let admins = db.as_admins().de()?;
    Ok(db
        .into_index()
        .into_signers()
        .de()?
        .into_iter()
        .filter(|(id, _)| admins.contains(id))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_and_rpc_signer_add_and_edit_retain_full_gpg_exports() {
        let armor = include_str!("../sign/fixtures/ed25519-signers.asc");
        let parsed = parse_public_keys(&[armor.into()]).unwrap();
        let cli =
            CliAddSignerParams::try_parse_from(["add", "--name", "Publisher", "--key", armor])
                .unwrap();
        let params = AddSignerParams {
            name: cli.name,
            contact: cli.contact,
            keys: cli.keys,
            gpg_exports: BTreeSet::new(),
        };
        let wire = to_value(&params).unwrap();
        let mut signer = from_value::<AddSignerParams>(wire)
            .unwrap()
            .into_signer()
            .unwrap();
        assert_eq!(signer.keys, parsed.keys);
        assert_eq!(signer.gpg_exports, parsed.gpg_exports);
        assert!(
            signer
                .gpg_exports
                .iter()
                .all(|export| !export.identities.is_empty() && !export.key_fingerprints.is_empty())
        );
        let saved: SignerInfo = from_value(to_value(&signer).unwrap()).unwrap();
        assert_eq!(saved, signer);

        let removed_key = parsed.keys.first().unwrap();
        let id = Guid::new();
        let edit = EditSignerParams::try_parse_from([
            "edit",
            id.as_ref(),
            "--add-key",
            armor,
            "--remove-key",
            removed_key.interned_der64().as_ref(),
        ])
        .unwrap();
        let wire = to_value(&edit).unwrap();
        let removed = from_value::<EditSignerParams>(wire)
            .unwrap()
            .apply(&mut signer)
            .unwrap();
        assert_eq!(removed, vec![removed_key.interned_der64()]);
        assert!(!signer.keys.contains(removed_key));
        assert_eq!(signer.gpg_exports, parsed.gpg_exports);
        let wire = to_value(&signer).unwrap();
        let saved: SignerInfo = from_value(wire.clone()).unwrap();
        assert_eq!(saved, signer);
        let readded = from_value::<AddSignerParams>(wire)
            .unwrap()
            .into_signer()
            .unwrap();
        assert_eq!(readded, signer);
    }

    #[test]
    fn cli_and_rpc_signer_edit_remove_selected_gpg_exports_without_removing_keys() {
        let armor = include_str!("../sign/fixtures/ed25519-signers.asc");
        let other_armor = include_str!("../sign/fixtures/start9-public.asc");
        let mut signer = AddSignerParams {
            name: "Publisher".into(),
            contact: vec![],
            keys: vec![armor.into(), other_armor.into()],
            gpg_exports: BTreeSet::new(),
        }
        .into_signer()
        .unwrap();
        let original_keys = signer.keys.clone();
        let selected = parse_public_keys(&[armor.into()])
            .unwrap()
            .gpg_exports
            .pop_first()
            .unwrap();
        let retained = parse_public_keys(&[other_armor.into()])
            .unwrap()
            .gpg_exports;
        let fingerprint = selected.fingerprints.last().unwrap().to_ascii_uppercase();
        let id = Guid::new();
        let edit = EditSignerParams::try_parse_from([
            "edit",
            id.as_ref(),
            "--add-key",
            armor,
            "--remove-gpg-export",
            &fingerprint,
            "--remove-gpg-export",
            "unknown",
        ])
        .unwrap();
        let wire = to_value(&edit).unwrap();
        assert_eq!(
            wire["removeGpgExport"],
            imbl_value::json!([fingerprint, "unknown"])
        );
        let removed = from_value::<EditSignerParams>(wire)
            .unwrap()
            .apply(&mut signer)
            .unwrap();
        assert!(removed.is_empty());
        assert_eq!(signer.keys, original_keys);
        assert_eq!(signer.gpg_exports, retained);
        let saved: SignerInfo = from_value(to_value(&signer).unwrap()).unwrap();
        assert_eq!(saved, signer);
    }

    #[test]
    fn signer_edit_without_export_removal_preserves_exports() {
        let mut signer = AddSignerParams {
            name: "Publisher".into(),
            contact: vec![],
            keys: vec![include_str!("../sign/fixtures/ed25519-signers.asc").into()],
            gpg_exports: BTreeSet::new(),
        }
        .into_signer()
        .unwrap();
        let original = signer.clone();
        let edit: EditSignerParams = from_value(imbl_value::json!({
            "id": Guid::new(),
            "addContact": [],
            "addKey": [],
            "removeContact": [],
            "removeKey": []
        }))
        .unwrap();
        assert!(edit.apply(&mut signer).unwrap().is_empty());
        assert_eq!(signer, original);
    }

    #[test]
    fn historical_signers_default_to_no_exports_and_invalid_imports_are_atomic() {
        let key = crate::sign::gpg::parse_public_keys(&[include_str!(
            "../sign/fixtures/start9-public.asc"
        )
        .into()])
        .unwrap()
        .keys
        .pop_first()
        .unwrap();
        let mut signer: SignerInfo =
            from_value(imbl_value::json!({"name": "Historical", "contact": [], "keys": [key]}))
                .unwrap();
        assert!(signer.gpg_exports.is_empty());
        let saved = signer.clone();
        assert!(
            signer
                .import_keys(&[
                    include_str!("../sign/fixtures/start9-public.asc").into(),
                    "invalid".into()
                ])
                .is_err()
        );
        assert_eq!(signer, saved);
        let params: AddSignerParams =
            from_value(imbl_value::json!({"name": "Invalid", "contact": [], "keys": ["invalid"]}))
                .unwrap();
        assert!(params.into_signer().is_err());
    }
}
