use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::Parser;
use imbl_value::InternedString;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use super::{BackupTarget, BackupTargetId, catalog};
use crate::context::{CliContext, RpcContext};
use crate::disk::mount::filesystem::cifs::Cifs;
use crate::prelude::*;
use crate::shared_volumes::{CreateSharedVolumeParams, CreateSharedVolumeSource};
use crate::util::serde::KeyVal;

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CifsTargets(pub BTreeMap<u32, Cifs>);
impl CifsTargets {
    pub fn new() -> Self {
        Self::default()
    }
}
impl Map for CifsTargets {
    type Key = u32;
    type Value = Cifs;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Self::key_string(key)
    }
    fn key_string(key: &Self::Key) -> Result<InternedString, Error> {
        Ok(InternedString::from_display(key))
    }
}

pub fn cifs<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("add", from_fn_async(add).no_cli())
        .subcommand(
            "add",
            from_fn_async(cli_add)
                .no_ts()
                .no_display()
                .with_about("about.add-new-backup-target"),
        )
        .subcommand("update", from_fn_async(update).no_cli())
        .subcommand(
            "update",
            from_fn_async(cli_update)
                .no_ts()
                .no_display()
                .with_about("about.update-existing-backup-target"),
        )
        .subcommand(
            "remove",
            from_fn_async(remove)
                .no_display()
                .with_about("about.remove-existing-backup-target")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CifsAddParams {
    #[arg(help = "help.arg.cifs-hostname")]
    pub hostname: String,
    #[arg(help = "help.arg.cifs-path")]
    pub path: PathBuf,
    #[arg(help = "help.arg.cifs-username")]
    pub username: String,
    #[arg(skip)]
    pub password: Option<String>,
}
rpc_toolkit::reflect_ts!(CifsAddParams);
rpc_toolkit::ts_export!(CifsAddParams, namespaces = [""]);

fn read_password() -> Result<Option<String>, Error> {
    let password = crate::util::tui::read_password("CIFS_PASSWORD", &t!("help.arg.cifs-password"))?;
    Ok((!password.is_empty()).then_some(password))
}

async fn cli_add(
    mut args: rpc_toolkit::HandlerArgs<CliContext, CifsAddParams>,
) -> Result<(), Error> {
    args.params.password = read_password()?;
    crate::backup::call_remote(args).await?;
    Ok(())
}

pub async fn add(
    ctx: RpcContext,
    CifsAddParams {
        hostname,
        path,
        username,
        password,
    }: CifsAddParams,
) -> Result<KeyVal<BackupTargetId, BackupTarget>, Error> {
    let cifs = Cifs {
        hostname,
        path,
        username,
        password,
    };
    cifs.mountable().await?;
    let name = catalog::cifs_name(&cifs);
    let id = catalog::create(
        ctx.clone(),
        CreateSharedVolumeParams {
            name,
            source: CreateSharedVolumeSource::Cifs(cifs),
        },
    )
    .await?;
    catalog::inspect(ctx.clone(), catalog::TargetParams { id: id.clone() }).await?;
    let value = ctx
        .db
        .peek()
        .await
        .as_public()
        .as_backup_targets()
        .as_idx(&id)
        .or_not_found(&id)?
        .de()?;
    Ok(KeyVal { key: id, value })
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CifsUpdateParams {
    #[arg(help = "help.arg.backup-target-id")]
    pub id: BackupTargetId,
    #[arg(help = "help.arg.cifs-hostname")]
    pub hostname: String,
    #[arg(help = "help.arg.cifs-path")]
    pub path: PathBuf,
    #[arg(help = "help.arg.cifs-username")]
    pub username: String,
    #[arg(skip)]
    pub password: Option<String>,
}
rpc_toolkit::reflect_ts!(CifsUpdateParams);
rpc_toolkit::ts_export!(CifsUpdateParams, namespaces = [""]);

async fn cli_update(
    mut args: rpc_toolkit::HandlerArgs<CliContext, CifsUpdateParams>,
) -> Result<(), Error> {
    args.params.password = read_password()?;
    crate::backup::call_remote(args).await?;
    Ok(())
}

pub async fn update(
    ctx: RpcContext,
    CifsUpdateParams {
        id,
        hostname,
        path,
        username,
        password,
    }: CifsUpdateParams,
) -> Result<KeyVal<BackupTargetId, BackupTarget>, Error> {
    catalog::ensure_cifs(&ctx.db.peek().await, &id)?;
    let cifs = Cifs {
        hostname,
        path,
        username,
        password,
    };
    cifs.mountable().await?;
    catalog::update_cifs(ctx.clone(), legacy_replacement(id.clone(), cifs)).await?;
    catalog::inspect(ctx.clone(), catalog::TargetParams { id: id.clone() }).await?;
    let value = ctx
        .db
        .peek()
        .await
        .as_public()
        .as_backup_targets()
        .as_idx(&id)
        .or_not_found(&id)?
        .de()?;
    Ok(KeyVal { key: id, value })
}

fn legacy_replacement(id: BackupTargetId, cifs: Cifs) -> catalog::UpdateCifsTargetParams {
    catalog::UpdateCifsTargetParams {
        id,
        hostname: cifs.hostname,
        path: cifs.path,
        username: cifs.username,
        password: Some(cifs.password.unwrap_or_default()),
    }
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CifsRemoveParams {
    #[arg(help = "help.arg.backup-target-id")]
    pub id: BackupTargetId,
}
rpc_toolkit::reflect_ts!(CifsRemoveParams);
rpc_toolkit::ts_export!(CifsRemoveParams, namespaces = [""]);

pub async fn remove(
    ctx: RpcContext,
    CifsRemoveParams { id }: CifsRemoveParams,
) -> Result<(), Error> {
    catalog::ensure_cifs(&ctx.db.peek().await, &id)?;
    catalog::remove(ctx, catalog::TargetParams { id }).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_updates_replace_passwords_instead_of_using_the_catalog_omission_rule() {
        for (password, expected) in [
            (None, ""),
            (Some(""), ""),
            (Some("replacement"), "replacement"),
        ] {
            let id: BackupTargetId = "cifs-7".parse().unwrap();
            let params = legacy_replacement(
                id.clone(),
                Cifs {
                    hostname: "nas.local".into(),
                    path: "/share".into(),
                    username: "owner".into(),
                    password: password.map(str::to_owned),
                },
            );
            assert_eq!(params.id, id);
            assert_eq!(params.password.as_deref(), Some(expected));
        }
    }

    #[test]
    fn cifs_password_is_not_a_cli_argument_but_remains_in_rpc_params() {
        let mut add = CifsAddParams::try_parse_from(["add", "nas", "share", "user"]).unwrap();
        assert!(add.password.is_none());
        assert!(CifsAddParams::try_parse_from(["add", "nas", "share", "user", "secret"]).is_err());
        add.password = Some("provided out of band".into());
        assert_eq!(
            serde_json::to_value(&add).unwrap()["password"],
            "provided out of band"
        );
        let update =
            CifsUpdateParams::try_parse_from(["update", "cifs-0", "nas", "share", "user"]).unwrap();
        assert!(update.password.is_none());
        assert!(
            CifsUpdateParams::try_parse_from([
                "update", "cifs-0", "nas", "share", "user", "secret"
            ])
            .is_err()
        );
    }
}
