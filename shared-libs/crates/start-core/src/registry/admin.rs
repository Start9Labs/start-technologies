use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::Parser;
use exver::VersionRange;
use itertools::Itertools;
use rpc_toolkit::{Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::context::CliContext;
use crate::middleware::auth::signature::{HasUnenrolledKeys, SignatureAuthContext};
use crate::prelude::*;
use crate::registry::RegistryDatabase;
use crate::registry::context::RegistryContext;
use crate::registry::signer::{ContactInfo, SignerInfo};
use crate::rpc_continuations::Guid;
use crate::sign::AnyVerifyingKey;
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
            from_fn_async(cli_add_signer).with_about("about.add-signer"),
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
    pub fn get_signer(&self, key: &AnyVerifyingKey) -> Result<Guid, Error> {
        self.as_entries()?
            .into_iter()
            .map(|(guid, s)| Ok::<_, Error>((guid, s.as_keys().de()?)))
            .filter_ok(|(_, s)| s.contains(key))
            .next()
            .transpose()?
            .map(|(a, _)| a)
            .ok_or_else(|| {
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
    table.print_tty(false)?;
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
    table.print_tty(false)?;
    Ok(())
}

pub async fn add_signer(ctx: RegistryContext, signer: SignerInfo) -> Result<Guid, Error> {
    ctx.db
        .mutate(|db| db.as_index_mut().as_signers_mut().add_signer(&signer))
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
#[ts(export)]
pub struct EditSignerParams {
    #[arg(help = "help.arg.signer-id")]
    pub id: Guid,
    #[arg(short = 'n', long, help = "help.arg.set-signer-name")]
    pub set_name: Option<String>,
    #[arg(short = 'c', long, help = "help.arg.add-signer-contact")]
    pub add_contact: Vec<ContactInfo>,
    #[arg(short = 'k', long, help = "help.arg.add-signer-key")]
    pub add_key: Vec<AnyVerifyingKey>,
    #[arg(short = 'C', long, help = "help.arg.remove-signer-contact")]
    pub remove_contact: Vec<ContactInfo>,
    #[arg(short = 'K', long, help = "help.arg.remove-signer-key")]
    pub remove_key: Vec<AnyVerifyingKey>,
}

pub async fn edit_signer(
    ctx: RegistryContext,
    EditSignerParams {
        id,
        set_name,
        add_contact,
        add_key,
        remove_contact,
        remove_key,
    }: EditSignerParams,
) -> Result<(), Error> {
    let continuations = ctx.open_authed_continuations();
    let ephemeral = ctx.ephemeral_auth_keys();
    ctx.db
        .mutate(|db| {
            let mut removed = Vec::new();
            db.as_index_mut()
                .as_signers_mut()
                .as_idx_mut(&id)
                .or_not_found(&id)?
                .mutate(|s| {
                    if let Some(name) = set_name {
                        s.name = name;
                    }
                    s.contact.extend(add_contact);
                    for rm in remove_contact {
                        let Some((idx, _)) = s.contact.iter().enumerate().find(|(_, c)| *c == &rm)
                        else {
                            continue;
                        };
                        s.contact.remove(idx);
                    }

                    s.keys.extend(add_key);
                    for rm in remove_key {
                        if s.keys.remove(&rm) {
                            removed.push(rm.interned_pem());
                        }
                    }
                    Ok(())
                })?;
            HasUnenrolledKeys::unenroll::<RegistryContext>(continuations, ephemeral, db, removed)?;
            Ok(())
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
#[ts(export)]
pub struct RemoveSignerParams {
    #[arg(help = "help.arg.signer-id")]
    pub id: Guid,
}

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
                signer.de()?.keys.iter().map(|k| k.interned_pem()),
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
    #[arg(long = "key", help = "help.arg.signer-key")]
    pub keys: Vec<AnyVerifyingKey>,
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
    let signer = SignerInfo {
        name,
        contact,
        keys: keys.into_iter().collect(),
    };
    if let Some(database) = database {
        TypedPatchDb::<RegistryDatabase>::load(PatchDb::open(database).await?)
            .await?
            .mutate(|db| db.as_index_mut().as_signers_mut().add_signer(&signer))
            .await
            .result
    } else {
        from_value(
            ctx.call_remote::<RegistryContext>(
                &parent_method.into_iter().chain(method).join("."),
                to_value(&signer)?,
            )
            .await?,
        )
    }
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AddAdminParams {
    pub signer: Guid,
}

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

#[derive(Debug, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RemoveAdminParams {
    #[arg(help = "help.arg.signer-id")]
    pub signer: Guid,
}

// TODO: don't allow removing self?
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
