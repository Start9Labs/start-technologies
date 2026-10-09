use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use clap::Parser;
use clap::builder::ValueParserFactory;
use color_eyre::eyre::eyre;
use exver::Version;
use imbl_value::InternedString;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tokio::sync::Mutex;
use tracing::instrument;
use ts_rs::TS;

use self::cifs::CifsBackupTarget;
use crate::PackageId;
use crate::backup::trash;
use crate::context::{CliContext, RpcContext};
use crate::db::model::DatabaseModel;
use crate::disk::mount::backup::BackupMountGuard;
use crate::disk::mount::filesystem::block_dev::BlockDev;
use crate::disk::mount::filesystem::cifs::Cifs;
use crate::disk::mount::filesystem::{FileSystem, MountType, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
use crate::disk::util::PartitionInfo;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;
use crate::util::serde::{
    HandlerExtSerde, WithIoFormat, deserialize_from_str, display_serializable, serialize_display,
};
use crate::util::{FromStrParser, VersionString};

pub mod cifs;

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum BackupTarget {
    #[serde(rename_all = "camelCase")]
    Disk {
        vendor: Option<String>,
        model: Option<String>,
        #[serde(flatten)]
        partition_info: PartitionInfo,
    },
    Cifs(CifsBackupTarget),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, TS)]
#[ts(export, type = "string")]
pub enum BackupTargetId {
    Disk { logicalname: PathBuf },
    Cifs { id: u32 },
}
impl BackupTargetId {
    pub fn load(self, db: &DatabaseModel) -> Result<BackupTargetFS, Error> {
        Ok(match self {
            BackupTargetId::Disk { logicalname } => {
                BackupTargetFS::Disk(BlockDev::new(logicalname))
            }
            BackupTargetId::Cifs { id } => BackupTargetFS::Cifs(cifs::load(db, id)?),
        })
    }

    pub fn user_facing_name(&self, db: &DatabaseModel) -> String {
        match self {
            BackupTargetId::Disk { logicalname } => logicalname.display().to_string(),
            BackupTargetId::Cifs { id } => cifs::load(db, *id)
                .map(|target| format!("//{}{}", target.hostname, target.path.display()))
                .unwrap_or_else(|_| self.to_string()),
        }
    }
}
impl std::fmt::Display for BackupTargetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackupTargetId::Disk { logicalname } => write!(f, "disk-{}", logicalname.display()),
            BackupTargetId::Cifs { id } => write!(f, "cifs-{}", id),
        }
    }
}
impl std::str::FromStr for BackupTargetId {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once('-') {
            Some(("disk", logicalname)) => Ok(BackupTargetId::Disk {
                logicalname: Path::new(logicalname).to_owned(),
            }),
            Some(("cifs", id)) => Ok(BackupTargetId::Cifs { id: id.parse()? }),
            _ => Err(Error::new(
                eyre!("Invalid Backup Target ID"),
                ErrorKind::InvalidBackupTargetId,
            )),
        }
    }
}
impl ValueParserFactory for BackupTargetId {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        FromStrParser::new()
    }
}
impl<'de> Deserialize<'de> for BackupTargetId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_from_str(deserializer)
    }
}
impl Serialize for BackupTargetId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_display(self, serializer)
    }
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum BackupTargetFS {
    Disk(BlockDev<PathBuf>),
    Cifs(Cifs),
}
impl FileSystem for BackupTargetFS {
    async fn mount<P: AsRef<Path> + Send>(
        &self,
        mountpoint: P,
        mount_type: MountType,
    ) -> Result<(), Error> {
        match self {
            BackupTargetFS::Disk(a) => a.mount(mountpoint, mount_type).await,
            BackupTargetFS::Cifs(a) => a.mount(mountpoint, mount_type).await,
        }
    }
    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        match self {
            BackupTargetFS::Disk(a) => a.source_hash().await,
            BackupTargetFS::Cifs(a) => a.source_hash().await,
        }
    }
}

// #[command(subcommands(cifs::cifs, list, info, mount, umount))]
pub fn target<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "cifs",
            cifs::cifs::<C>().with_about("about.add-remove-update-backup-target"),
        )
        .subcommand(
            "list",
            from_fn_async(list)
                .with_display_serializable()
                .with_about("about.list-existing-backup-targets")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "info",
            from_fn_async(info)
                .with_display_serializable()
                .with_custom_display_fn::<CliContext, _>(|params, info| {
                    display_backup_info(params.params, info)
                })
                .with_about("about.display-package-backup-information")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "mount",
            from_fn_async(mount)
                .with_about("about.mount-backup-target")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "umount",
            from_fn_async(umount)
                .no_display()
                .with_about("about.unmount-backup-target")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "delete-legacy",
            from_fn_async(delete_legacy)
                .no_display()
                .with_about("about.delete-legacy-backup")
                .with_call_remote::<CliContext>(),
        )
}

// #[command(display(display_serializable))]
pub async fn list(ctx: RpcContext) -> Result<BTreeMap<BackupTargetId, BackupTarget>, Error> {
    let peek = ctx.db.peek().await;
    let server_id = peek.as_public().as_server_info().as_id().de()?;
    let (disks_res, cifs) = tokio::try_join!(
        crate::disk::util::list(&ctx.os_partitions, Some(server_id.as_str())),
        cifs::list(&peek, &server_id),
    )?;
    Ok(disks_res
        .into_iter()
        .flat_map(|mut disk| {
            std::mem::take(&mut disk.partitions)
                .into_iter()
                .map(|part| {
                    (
                        BackupTargetId::Disk {
                            logicalname: part.logicalname.clone(),
                        },
                        BackupTarget::Disk {
                            vendor: disk.vendor.clone(),
                            model: disk.model.clone(),
                            partition_info: part,
                        },
                    )
                })
                .collect::<Vec<_>>()
        })
        .chain(
            cifs.into_iter()
                .map(|(id, cifs)| (BackupTargetId::Cifs { id }, BackupTarget::Cifs(cifs))),
        )
        .collect())
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    #[ts(type = "string")]
    pub version: Version,
    #[ts(type = "string | null")]
    pub timestamp: Option<DateTime<Utc>>,
    pub package_backups: BTreeMap<PackageId, PackageBackupInfo>,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PackageBackupInfo {
    pub title: InternedString,
    pub version: VersionString,
    #[ts(type = "string")]
    pub os_version: Version,
    #[ts(type = "string")]
    pub timestamp: DateTime<Utc>,
}

fn display_backup_info(params: WithIoFormat<InfoParams>, info: BackupInfo) -> Result<(), Error> {
    use prettytable::*;

    if let Some(format) = params.format {
        return display_serializable(format, info);
    }

    let mut table = Table::new();
    table.add_row(row![bc =>
        "ID",
        "VERSION",
        "OS VERSION",
        "TIMESTAMP",
    ]);
    table.add_row(row![
        "StartOS",
        &info.version.to_string(),
        &info.version.to_string(),
        &if let Some(ts) = &info.timestamp {
            ts.to_string()
        } else {
            "N/A".to_owned()
        },
    ]);
    for (id, info) in info.package_backups {
        let row = row![
            &*id,
            info.version.as_str(),
            &info.os_version.to_string(),
            &info.timestamp.to_string(),
        ];
        table.add_row(row);
    }
    table.print_tty(false)?;
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct InfoParams {
    #[arg(help = "help.arg.backup-target-id")]
    target_id: BackupTargetId,
    #[arg(help = "help.arg.server-id")]
    server_id: String,
    #[arg(help = "help.arg.backup-password")]
    password: String,
}

#[instrument(skip(ctx, password))]
pub async fn info(
    ctx: RpcContext,
    InfoParams {
        target_id,
        server_id,
        password,
    }: InfoParams,
) -> Result<BackupInfo, Error> {
    let guard = BackupMountGuard::mount(
        TmpMountGuard::mount(&target_id.load(&ctx.db.peek().await)?, ReadWrite).await?,
        &server_id,
        &password,
    )
    .await?;

    let res = guard.metadata.clone();

    guard.unmount().await?;

    Ok(res)
}

lazy_static::lazy_static! {
    static ref USER_MOUNTS: Mutex<BTreeMap<BackupTargetId, Result<BackupMountGuard<TmpMountGuard>, TmpMountGuard>>> =
        Mutex::new(BTreeMap::new());
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct MountParams {
    #[arg(help = "help.arg.backup-target-id")]
    target_id: BackupTargetId,
    #[arg(long, help = "help.arg.server-id")]
    server_id: Option<String>,
    #[arg(help = "help.arg.backup-password")]
    password: String, // TODO: rpassword
    #[arg(long, help = "help.arg.allow-partial-backup")]
    allow_partial: bool,
}

#[instrument(skip_all)]
pub async fn mount(
    ctx: RpcContext,
    MountParams {
        target_id,
        server_id,
        password,
        allow_partial,
    }: MountParams,
) -> Result<String, Error> {
    let server_id = if let Some(server_id) = server_id {
        server_id
    } else {
        ctx.db
            .peek()
            .await
            .into_public()
            .into_server_info()
            .into_id()
            .de()?
    };

    let mut mounts = USER_MOUNTS.lock().await;

    let existing = mounts.get(&target_id);

    let base = match existing {
        Some(Ok(a)) => return Ok(a.path().display().to_string()),
        Some(Err(e)) => e.clone(),
        None => {
            TmpMountGuard::mount(&target_id.clone().load(&ctx.db.peek().await)?, ReadWrite).await?
        }
    };

    let guard = match BackupMountGuard::mount(base.clone(), &server_id, &password).await {
        Ok(a) => a,
        Err(e) => {
            if allow_partial {
                mounts.insert(target_id, Err(base.clone()));
                let enc_key = BackupMountGuard::<TmpMountGuard>::load_metadata(
                    base.path(),
                    &server_id,
                    &password,
                )
                .await
                .map(|(_, k)| k);
                return Err(e)
                    .with_ctx(|e| (
                        e.kind,
                        format!(
                            "\nThe base filesystem did successfully mount at {:?}\nWrapped Key: {:?}",
                            base.path(),
                            enc_key
                        )
                    ));
            } else {
                return Err(e);
            }
        }
    };

    let res = guard.path().display().to_string();

    mounts.insert(target_id, Ok(guard));

    Ok(res)
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct UmountParams {
    #[arg(help = "help.arg.backup-target-id")]
    target_id: Option<BackupTargetId>,
}

#[instrument(skip_all)]
pub async fn umount(_: RpcContext, UmountParams { target_id }: UmountParams) -> Result<(), Error> {
    let mut mounts = USER_MOUNTS.lock().await; // TODO: move to context
    if let Some(target_id) = target_id {
        if let Some(existing) = mounts.remove(&target_id) {
            match existing {
                Ok(e) => e.unmount().await?,
                Err(e) => e.unmount().await?,
            }
        }
    } else {
        for (_, existing) in std::mem::take(&mut *mounts) {
            match existing {
                Ok(e) => e.unmount().await?,
                Err(e) => e.unmount().await?,
            }
        }
    }

    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct DeleteLegacyParams {
    #[arg(help = "help.arg.backup-target-id")]
    target_id: BackupTargetId,
}

/// Delete this server's pre-V2 `StartOSBackups/<server_id>` backup from a target,
/// freeing the space it occupied. Other servers' legacy backups and the current
/// `StartOSBackupsV2` data are untouched. No-op if absent.
///
/// Unlinking a large backup can take hours, so the backup is only atomically
/// renamed into the target's trash before this returns — gone from the target
/// as far as detection is concerned — and a background sweep reclaims the
/// space, posting a notification when it finishes.
#[instrument(skip_all)]
pub async fn delete_legacy(
    ctx: RpcContext,
    DeleteLegacyParams { target_id }: DeleteLegacyParams,
) -> Result<(), Error> {
    let peek = ctx.db.peek().await;
    let server_id = peek.as_public().as_server_info().as_id().de()?;
    let target = target_id.to_string();
    let guard = TmpMountGuard::mount(&target_id.load(&peek)?, ReadWrite).await?;
    let legacy_dir = guard
        .path()
        .join(crate::disk::LEGACY_BACKUP_DIR_NAME)
        .join(&server_id);
    if tokio::fs::metadata(&legacy_dir).await.is_ok() {
        trash::move_to_trash(guard.path(), &legacy_dir).await?;
    }
    if !trash::has_trash(guard.path()).await {
        return guard.unmount().await;
    }
    let db = ctx.db.clone();
    tokio::task::spawn(async move {
        let result = trash::sweep_until_clear(&guard).await;
        guard.unmount().await.log_err();
        // `mutate` needs an unwind-safe closure, which eyre errors are not —
        // render the notification before it, not inside
        let (level, title, message) = match &result {
            Ok(()) => (
                NotificationLevel::Success,
                t!("backup.trash.reclaimed-title").to_string(),
                t!("backup.trash.reclaimed-message", target = target).to_string(),
            ),
            Err(e) => (
                NotificationLevel::Warning,
                t!("backup.trash.reclaim-failed-title").to_string(),
                t!(
                    "backup.trash.reclaim-failed-message",
                    target = target,
                    error = e
                )
                .to_string(),
            ),
        };
        db.mutate(|db| notify(db, None, level, title, message, ()))
            .await
            .result
            .log_err();
    });
    Ok(())
}
