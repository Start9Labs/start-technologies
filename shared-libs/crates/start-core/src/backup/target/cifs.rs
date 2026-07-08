use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clap::Parser;
use color_eyre::eyre::eyre;
use imbl_value::InternedString;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{BackupTarget, BackupTargetId};
use crate::context::{CliContext, RpcContext};
use crate::db::model::DatabaseModel;
use crate::disk::mount::filesystem::ReadOnly;
use crate::disk::mount::filesystem::cifs::Cifs;
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
use crate::disk::util::{StartOsRecoveryInfo, get_available, has_legacy_backup, recovery_info};
use crate::prelude::*;
use crate::util::serde::KeyVal;

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CifsTargets(pub BTreeMap<u32, Cifs>);
impl CifsTargets {
    pub fn new() -> Self {
        Self(BTreeMap::new())
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

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CifsBackupTarget {
    hostname: String,
    path: PathBuf,
    username: String,
    mountable: bool,
    #[ts(type = "number | null")]
    available: Option<u64>,
    start_os: BTreeMap<String, StartOsRecoveryInfo>,
    legacy_backup: bool,
}

pub fn cifs<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "add",
            from_fn_async(add)
                .no_display()
                .with_about("about.add-new-backup-target")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "update",
            from_fn_async(update)
                .no_display()
                .with_about("about.update-existing-backup-target")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove)
                .no_display()
                .with_about("about.remove-existing-backup-target")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CifsAddParams {
    #[arg(help = "help.arg.cifs-hostname")]
    pub hostname: String,
    #[arg(help = "help.arg.cifs-path")]
    pub path: PathBuf,
    #[arg(help = "help.arg.cifs-username")]
    pub username: String,
    #[arg(help = "help.arg.cifs-password")]
    pub password: Option<String>,
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
        path: Path::new("/").join(path),
        username,
        password,
    };
    let server_id = ctx
        .db
        .peek()
        .await
        .as_public()
        .as_server_info()
        .as_id()
        .de()?;
    let guard = TmpMountGuard::mount(&cifs, ReadOnly).await?;
    let start_os = recovery_info(guard.path()).await?;
    let available = get_available(guard.path()).await.ok();
    let legacy_backup = has_legacy_backup(guard.path(), &server_id).await;
    guard.unmount().await?;
    let id = ctx
        .db
        .mutate(|db| {
            let id = db
                .as_private()
                .as_cifs()
                .keys()?
                .into_iter()
                .max()
                .map_or(0, |a| a + 1);
            db.as_private_mut().as_cifs_mut().insert(&id, &cifs)?;
            Ok(id)
        })
        .await
        .result?;
    Ok(KeyVal {
        key: BackupTargetId::Cifs { id },
        value: BackupTarget::Cifs(CifsBackupTarget {
            hostname: cifs.hostname,
            path: cifs.path,
            username: cifs.username,
            mountable: true,
            available,
            start_os,
            legacy_backup,
        }),
    })
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
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
    #[arg(help = "help.arg.cifs-password")]
    pub password: Option<String>,
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
    let id = if let BackupTargetId::Cifs { id } = id {
        id
    } else {
        return Err(Error::new(
            eyre!("{}", t!("backup.target.cifs.target-not-found", id = id)),
            ErrorKind::NotFound,
        ));
    };
    let cifs = Cifs {
        hostname,
        path: Path::new("/").join(path),
        username,
        password,
    };
    let server_id = ctx
        .db
        .peek()
        .await
        .as_public()
        .as_server_info()
        .as_id()
        .de()?;
    let guard = TmpMountGuard::mount(&cifs, ReadOnly).await?;
    let start_os = recovery_info(guard.path()).await?;
    let available = get_available(guard.path()).await.ok();
    let legacy_backup = has_legacy_backup(guard.path(), &server_id).await;
    guard.unmount().await?;
    ctx.db
        .mutate(|db| {
            db.as_private_mut()
                .as_cifs_mut()
                .as_idx_mut(&id)
                .ok_or_else(|| {
                    Error::new(
                        eyre!(
                            "{}",
                            t!(
                                "backup.target.cifs.target-not-found",
                                id = BackupTargetId::Cifs { id }
                            )
                        ),
                        ErrorKind::NotFound,
                    )
                })?
                .ser(&cifs)
        })
        .await
        .result?;
    Ok(KeyVal {
        key: BackupTargetId::Cifs { id },
        value: BackupTarget::Cifs(CifsBackupTarget {
            hostname: cifs.hostname,
            path: cifs.path,
            username: cifs.username,
            mountable: true,
            available,
            start_os,
            legacy_backup,
        }),
    })
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CifsRemoveParams {
    #[arg(help = "help.arg.backup-target-id")]
    pub id: BackupTargetId,
}

pub async fn remove(
    ctx: RpcContext,
    CifsRemoveParams { id }: CifsRemoveParams,
) -> Result<(), Error> {
    let id = if let BackupTargetId::Cifs { id } = id {
        id
    } else {
        return Err(Error::new(
            eyre!("{}", t!("backup.target.cifs.target-not-found", id = id)),
            ErrorKind::NotFound,
        ));
    };
    ctx.db
        .mutate(|db| db.as_private_mut().as_cifs_mut().remove(&id))
        .await
        .result?;
    Ok(())
}

pub fn load(db: &DatabaseModel, id: u32) -> Result<Cifs, Error> {
    db.as_private()
        .as_cifs()
        .as_idx(&id)
        .ok_or_else(|| {
            Error::new(
                eyre!("{}", t!("backup.target.cifs.target-not-found-id", id = id)),
                ErrorKind::NotFound,
            )
        })?
        .de()
}

pub async fn list(
    db: &DatabaseModel,
    server_id: &str,
) -> Result<Vec<(u32, CifsBackupTarget)>, Error> {
    let mut cifs = Vec::new();
    for (id, model) in db.as_private().as_cifs().as_entries()? {
        let mount_info = model.de()?;
        let info = async {
            let guard = TmpMountGuard::mount(&mount_info, ReadOnly).await?;
            let start_os = recovery_info(guard.path()).await?;
            let available = get_available(guard.path()).await.ok();
            let legacy_backup = has_legacy_backup(guard.path(), server_id).await;
            guard.unmount().await?;
            Ok::<_, Error>((start_os, available, legacy_backup))
        }
        .await;
        let mountable = info.is_ok();
        let (start_os, available, legacy_backup) = info.ok().unwrap_or_default();
        cifs.push((
            id,
            CifsBackupTarget {
                hostname: mount_info.hostname,
                path: mount_info.path,
                username: mount_info.username,
                mountable,
                available,
                start_os,
                legacy_backup,
            },
        ));
    }

    Ok(cifs)
}
