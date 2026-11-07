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

use crate::PackageId;
use crate::backup::trash;
use crate::context::{CliContext, RpcContext};
use crate::db::model::DatabaseModel;
use crate::disk::mount::backup::BackupMountGuard;
use crate::disk::mount::filesystem::block_dev::BlockDev;
use crate::disk::mount::filesystem::cifs::Cifs;
use crate::disk::mount::filesystem::{FileSystem, MountType, ReadWrite};
use crate::disk::mount::guard::GenericMountGuard;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;
use crate::util::serde::{
    HandlerExtSerde, WithIoFormat, deserialize_from_str, display_serializable, serialize_display,
};
use crate::util::{FromStrParser, VersionString};

pub(crate) mod catalog;
pub mod cifs;
pub mod conversion;
pub use catalog::BackupTarget;
pub(crate) use catalog::{BackupTargetSource, TargetMount};

#[cfg(test)]
mod password_cli_tests {
    use super::*;

    #[test]
    fn backup_password_is_not_a_cli_argument_but_remains_in_rpc_params() {
        let mut info = InfoParams::try_parse_from(["info", "cifs-0", "server-id"]).unwrap();
        assert!(info.password.is_empty());
        assert!(InfoParams::try_parse_from(["info", "cifs-0", "server-id", "secret"]).is_err());
        info.password = "provided out of band".into();
        assert_eq!(
            serde_json::to_value(&info).unwrap()["password"],
            "provided out of band"
        );

        let mount = MountParams::try_parse_from(["mount", "cifs-0", "--allow-partial"]).unwrap();
        assert!(mount.password.is_empty());
        assert!(mount.allow_partial);
        assert!(MountParams::try_parse_from(["mount", "cifs-0", "secret"]).is_err());
    }
}

/// A registered-target identity; migrated CIFS targets retain their existing IDs.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, VisitFields)]
#[visit(ts(type = "z.string()"))]
pub struct BackupTargetId(String);

rpc_toolkit::reflect_ts!(BackupTargetId);
rpc_toolkit::ts_export!(BackupTargetId, namespaces = [""]);
impl BackupTargetId {
    pub(crate) fn refresh_disks_on_drop(
        &self,
        updates: tokio::sync::watch::Sender<()>,
    ) -> crate::util::GeneralBoxedGuard {
        crate::util::GeneralBoxedGuard::new(move || {
            updates.send_replace(());
        })
    }

    pub fn new() -> Self {
        Self(crate::rpc_continuations::Guid::new().to_string())
    }

    pub(crate) fn load(&self, db: &DatabaseModel) -> Result<BackupTargetSource, Error> {
        BackupTargetSource::read(db, self)
    }
}
impl std::fmt::Display for BackupTargetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::str::FromStr for BackupTargetId {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.parse::<crate::rpc_continuations::Guid>().is_ok()
            || s.strip_prefix("cifs-")
                .is_some_and(|id| id.parse::<u32>().is_ok_and(|id| format!("cifs-{id}") == s))
        {
            Ok(Self(s.to_owned()))
        } else {
            Err(Error::new(
                eyre!("Invalid Backup Target ID"),
                ErrorKind::InvalidBackupTargetId,
            ))
        }
    }
}
impl AsRef<str> for BackupTargetId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
impl Default for BackupTargetId {
    fn default() -> Self {
        Self::new()
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

#[derive(Debug, Deserialize, Serialize, VisitVariants)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum BackupTargetFS {
    Disk(BlockDev<PathBuf>),
    Cifs(Cifs),
}

rpc_toolkit::reflect_ts!(BackupTargetFS);
rpc_toolkit::ts_export!(BackupTargetFS, namespaces = [""]);
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
    async fn device(&self) -> Result<Option<PathBuf>, Error> {
        match self {
            Self::Disk(disk) => disk.device().await,
            Self::Cifs(_) => Ok(None),
        }
    }
    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        match self {
            BackupTargetFS::Disk(a) => a.source_hash().await,
            BackupTargetFS::Cifs(a) => a.source_hash().await,
        }
    }
}

pub fn target<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("create", from_fn_async(catalog::create).no_cli())
        .subcommand("rename", from_fn_async(catalog::rename).no_cli())
        .subcommand("update-cifs", from_fn_async(catalog::update_cifs).no_cli())
        .subcommand("remove", from_fn_async(catalog::remove).no_cli())
        .subcommand("inspect", from_fn_async(catalog::inspect).no_cli())
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
            from_fn_async(info).with_display_serializable().no_cli(),
        )
        .subcommand(
            "info",
            from_fn_async(cli_info)
                .no_ts()
                .with_display_serializable()
                .with_custom_display_fn::<CliContext, _>(|params, info| {
                    display_backup_info(params.params, info)
                })
                .with_about("about.display-package-backup-information"),
        )
        .subcommand("convert", from_fn_async(conversion::start).no_cli())
        .subcommand("mount", from_fn_async(mount).no_cli())
        .subcommand(
            "mount",
            from_fn_async(cli_mount)
                .no_ts()
                .with_about("about.mount-backup-target"),
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

pub async fn list(ctx: RpcContext) -> Result<BTreeMap<BackupTargetId, BackupTarget>, Error> {
    ctx.db.peek().await.as_public().as_backup_targets().de()
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub version: Version,
    pub timestamp: Option<DateTime<Utc>>,
    pub package_backups: BTreeMap<PackageId, PackageBackupInfo>,
    #[serde(default)]
    pub shared_volume_backups:
        BTreeMap<crate::shared_volumes::SharedVolumeId, SharedVolumeBackupInfo>,
}

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SharedVolumeBackupInfo {
    pub name: String,
    pub timestamp: DateTime<Utc>,
    /// Identifies the completed copy beneath the volume's encrypted `data` directory.
    pub generation: String,
}

rpc_toolkit::reflect_ts!(SharedVolumeBackupInfo);

rpc_toolkit::reflect_ts!(BackupInfo);
rpc_toolkit::ts_export!(BackupInfo, namespaces = [""]);

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct PackageBackupInfo {
    pub title: InternedString,
    pub version: VersionString,
    pub os_version: Version,
    pub timestamp: DateTime<Utc>,
}

rpc_toolkit::reflect_ts!(PackageBackupInfo);
rpc_toolkit::ts_export!(PackageBackupInfo, namespaces = [""]);

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
    print_table(&table)?;
    if !info.shared_volume_backups.is_empty() {
        let mut volumes = Table::new();
        volumes.add_row(row![bc => "ID", t!("backup.volume.volumes"), "TIMESTAMP"]);
        for (id, info) in info.shared_volume_backups {
            volumes.add_row(row![id.to_string(), info.name, info.timestamp.to_string()]);
        }
        print_table(&volumes)?;
    }
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct InfoParams {
    #[arg(help = "help.arg.backup-target-id")]
    target_id: BackupTargetId,
    #[arg(help = "help.arg.server-id")]
    server_id: String,
    #[arg(skip)]
    password: String,
}

rpc_toolkit::reflect_ts!(InfoParams);
rpc_toolkit::ts_export!(InfoParams, namespaces = [""]);

#[instrument(skip_all)]
async fn cli_info(
    mut args: rpc_toolkit::HandlerArgs<CliContext, InfoParams>,
) -> Result<BackupInfo, Error> {
    args.params.password = crate::backup::read_password()?;
    Ok(from_value(crate::backup::call_remote(args).await?)?)
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
        target_id
            .load(&ctx.db.peek().await)?
            .mount(ReadWrite)
            .await?,
        &server_id,
        &password,
    )
    .await?;

    let res = guard.metadata.clone();

    guard.unmount().await?;

    Ok(res)
}

lazy_static::lazy_static! {
    static ref USER_MOUNTS: Mutex<BTreeMap<BackupTargetId, Result<BackupMountGuard<TargetMount>, TargetMount>>> =
        Mutex::new(BTreeMap::new());
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct MountParams {
    #[arg(help = "help.arg.backup-target-id")]
    target_id: BackupTargetId,
    #[arg(long, help = "help.arg.server-id")]
    server_id: Option<String>,
    #[arg(skip)]
    password: String,
    #[arg(long, help = "help.arg.allow-partial-backup")]
    allow_partial: bool,
}

rpc_toolkit::reflect_ts!(MountParams);

#[instrument(skip_all)]
async fn cli_mount(
    mut args: rpc_toolkit::HandlerArgs<CliContext, MountParams>,
) -> Result<String, Error> {
    args.params.password = crate::backup::read_password()?;
    Ok(from_value(crate::backup::call_remote(args).await?)?)
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
            target_id
                .load(&ctx.db.peek().await)?
                .mount(ReadWrite)
                .await?
        }
    };

    let guard = match BackupMountGuard::mount(base.clone(), &server_id, &password).await {
        Ok(a) => a,
        Err(e) => {
            if allow_partial {
                mounts.insert(target_id, Err(base.clone()));
                return Err(partial_mount_error(e, base.path()));
            } else {
                return Err(e);
            }
        }
    };

    let res = guard.path().display().to_string();

    mounts.insert(target_id, Ok(guard));

    Ok(res)
}

fn partial_mount_error(error: Error, path: &Path) -> Error {
    Err::<(), _>(error)
        .with_ctx(|error| {
            (
                error.kind,
                t!("backup.target.partial-mount", path = path.display()).to_string(),
            )
        })
        .unwrap_err()
}

#[test]
fn partial_mount_diagnostic_contains_path_without_key_material() {
    let error = partial_mount_error(
        Error::new(eyre!("encrypted mount failed"), ErrorKind::Backup),
        Path::new("/media/backup"),
    );
    assert!(error.to_string().contains("encrypted mount failed"));
    for message in [format!("{error}"), format!("{error:?}")] {
        assert!(message.contains("/media/backup"));
        assert!(!message.contains("Wrapped Key"));
        assert!(!message.contains(crate::util::crypto::backup_key_tests::KEY));
    }
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct UmountParams {
    #[arg(help = "help.arg.backup-target-id")]
    target_id: Option<BackupTargetId>,
}

rpc_toolkit::reflect_ts!(UmountParams);
rpc_toolkit::ts_export!(UmountParams, namespaces = [""]);

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

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct DeleteLegacyParams {
    #[arg(help = "help.arg.backup-target-id")]
    target_id: BackupTargetId,
}

rpc_toolkit::reflect_ts!(DeleteLegacyParams);
rpc_toolkit::ts_export!(DeleteLegacyParams, namespaces = [""]);

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
    let guard = target_id.load(&peek)?.mount(ReadWrite).await?;
    let _disk_refresh = target_id.refresh_disks_on_drop(ctx.disk_updates.clone());
    let legacy_dir = guard
        .path()
        .join(crate::disk::LEGACY_BACKUP_DIR_NAME)
        .join(&server_id);
    if tokio::fs::metadata(&legacy_dir).await.is_ok() {
        trash::move_to_trash(guard.path(), &legacy_dir).await?;
    }
    catalog::refresh(&ctx.db, &target_id).await.log_err();
    if !trash::has_trash(guard.path()).await {
        return guard.unmount().await;
    }
    let db = ctx.db.clone();
    let disk_refresh = target_id.refresh_disks_on_drop(ctx.disk_updates.clone());
    tokio::task::spawn(async move {
        let _disk_refresh = disk_refresh;
        let result = trash::sweep_until_clear(&guard).await;
        guard.unmount().await.log_err();
        catalog::refresh(&db, &target_id).await.log_err();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_mutations_request_discovery_after_the_guard_releases() {
        let (updates, mut observed) = tokio::sync::watch::channel(());
        let target = BackupTargetId::new();
        let guard = target.refresh_disks_on_drop(updates.clone());
        assert!(!observed.has_changed().unwrap());
        drop(guard);
        assert!(observed.has_changed().unwrap());
        observed.borrow_and_update();
        drop(
            "cifs-1"
                .parse::<BackupTargetId>()
                .unwrap()
                .refresh_disks_on_drop(updates.clone()),
        );
        assert!(observed.has_changed().unwrap());
    }

    #[test]
    fn legacy_metadata_defaults_to_no_volume_backups() {
        let mut wire = serde_json::to_value(BackupInfo::default()).unwrap();
        wire.as_object_mut().unwrap().remove("sharedVolumeBackups");
        let info: BackupInfo = serde_json::from_value(wire).unwrap();
        assert!(info.shared_volume_backups.is_empty());
    }
}
