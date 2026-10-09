use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;
use futures::{FutureExt, StreamExt, stream};
use patch_db::json_ptr::ROOT;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, OwnedMutexGuard};
use tracing::instrument;
use ts_rs::TS;

use super::PackageBackupReport;
use super::scheduled::{
    BackupActivityKind, BackupRunState, ScheduledBackupMountGuard, ServiceSnapshotId,
    complete_activity, history_key, insert_activity, mount_scheduled_target, running_activity,
};
use super::target::BackupTargetId;
use crate::backup::os::OsBackup;
use crate::context::rpc::InitRpcContextPhases;
use crate::context::setup::SetupResult;
use crate::context::{RpcContext, SetupContext};
use crate::db::model::Database;
use crate::disk::mount::backup::BackupMountGuard;
use crate::disk::mount::filesystem::ReadWrite;
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
use crate::hostname::{ServerHostname, repair_hostname};
use crate::init::{InitPhases, init};
use crate::prelude::*;
use crate::progress::{PhaseProgressTrackerHandle, ProgressUnits};
use crate::s9pk::S9pk;
use crate::service::service_map::DownloadInstallFuture;
use crate::setup::SetupExecuteProgress;
use crate::system::{save_language, sync_kiosk};
use crate::util::serde::{IoFormat, Pem};
use crate::{PackageId, SYSTEM_PACKAGE_ID};

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
#[ts(export)]
pub struct RestorePackageParams {
    #[arg(help = "help.arg.backup-target-id")]
    pub target_id: BackupTargetId,
    #[arg(help = "help.arg.backup-password")]
    pub password: String,
    #[arg(help = "help.arg.package-ids")]
    pub ids: Vec<PackageId>,
    #[arg(long, help = "help.arg.server-id")]
    pub server_id: Option<String>,
}

#[instrument(skip(ctx, password))]
pub async fn restore_packages_rpc(
    ctx: RpcContext,
    RestorePackageParams {
        ids,
        target_id,
        password,
        server_id,
    }: RestorePackageParams,
) -> Result<(), Error> {
    restore_selection_rpc(
        ctx,
        RestoreSelectionParams {
            target_id,
            manual_ids: ids,
            snapshots: BTreeMap::new(),
            server_id,
            password: Some(password),
        },
    )
    .await
}

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestoreSelectionParams {
    pub target_id: BackupTargetId,
    #[serde(default)]
    pub manual_ids: Vec<PackageId>,
    #[serde(default)]
    pub snapshots: BTreeMap<PackageId, ServiceSnapshotId>,
    #[serde(default)]
    #[ts(optional)]
    pub server_id: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub password: Option<String>,
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct RestoreSelectionCliParams {
    #[arg(help = "help.arg.backup-target-id")]
    pub target_id: BackupTargetId,
    #[arg(long, value_delimiter = ',', help = "help.arg.package-ids")]
    pub manual_ids: Vec<PackageId>,
    #[arg(
        long = "checkpoint",
        value_name = "PACKAGE_ID=SNAPSHOT_ID",
        value_parser = super::scheduled::parse_checkpoint_selection,
        help = "help.arg.automatic-backup-checkpoint-selection"
    )]
    pub checkpoints: Vec<(PackageId, ServiceSnapshotId)>,
    #[arg(long, help = "help.arg.server-id")]
    pub server_id: Option<String>,
    #[arg(long, help = "help.arg.backup-password")]
    pub password: Option<String>,
}

pub async fn restore_selection_cli(
    ctx: RpcContext,
    RestoreSelectionCliParams {
        target_id,
        manual_ids,
        checkpoints,
        server_id,
        password,
    }: RestoreSelectionCliParams,
) -> Result<(), Error> {
    restore_selection_rpc(
        ctx,
        RestoreSelectionParams {
            target_id,
            manual_ids,
            snapshots: checkpoints.into_iter().collect(),
            server_id,
            password,
        },
    )
    .await
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    #[test]
    fn mixed_restore_cli_accepts_manual_and_automatic_selections() {
        let snapshot_id = ServiceSnapshotId::new();
        let params = RestoreSelectionCliParams::try_parse_from(vec![
            "restore-mixed".to_owned(),
            "cifs-7".to_owned(),
            "--manual-ids".to_owned(),
            "bitcoind,lnd".to_owned(),
            "--checkpoint".to_owned(),
            format!("core-lightning={snapshot_id}"),
            "--password".to_owned(),
            "secret".to_owned(),
        ])
        .unwrap();

        assert_eq!(params.target_id, BackupTargetId::Cifs { id: 7 });
        assert_eq!(params.manual_ids.len(), 2);
        assert_eq!(
            params.checkpoints,
            vec![("core-lightning".parse().unwrap(), snapshot_id)]
        );
        assert_eq!(params.password.as_deref(), Some("secret"));
    }
}

pub async fn restore_selection_rpc(
    ctx: RpcContext,
    RestoreSelectionParams {
        target_id,
        manual_ids,
        snapshots,
        server_id,
        password,
    }: RestoreSelectionParams,
) -> Result<(), Error> {
    if manual_ids.iter().any(|id| snapshots.contains_key(id)) {
        return Err(Error::new(
            eyre!("{}", t!("backup.restore.duplicate-checkpoint")),
            ErrorKind::InvalidRequest,
        ));
    }
    if manual_ids.is_empty() && snapshots.is_empty() {
        return Err(Error::new(
            eyre!("{}", t!("backup.restore.select-service")),
            ErrorKind::InvalidRequest,
        ));
    }

    let operation_coordinator =
        crate::backup::try_backup_coordinator(ctx.backup_coordinator.clone())?;
    crate::backup::scheduled::reconcile_interrupted_backup_state(&ctx).await?;

    let db = ctx.db.peek().await;
    let current_server_id = db.as_public().as_server_info().as_id().de()?;
    let server_id = server_id.unwrap_or_else(|| current_server_id.clone());
    let mut tasks = BTreeMap::new();
    let mut refreshed_credential = None;
    let mut scheduled_guard = None;
    let mut manual_guard = None;

    if !snapshots.is_empty() {
        let guard = if server_id == current_server_id {
            let target_instance_ids = snapshots
                .iter()
                .map(|(package_id, snapshot_id)| {
                    let key = history_key(&target_id, package_id);
                    let history: super::scheduled::ServiceTargetHistory = db
                        .as_public()
                        .as_scheduled_backups()
                        .as_histories()
                        .as_idx(&key)
                        .or_not_found(&key)?
                        .de()?;
                    if !history
                        .snapshots
                        .iter()
                        .any(|snapshot| &snapshot.id == snapshot_id)
                    {
                        return Err(Error::new(
                            eyre!("{}", t!("backup.scheduled.snapshot-not-found")),
                            ErrorKind::NotFound,
                        ));
                    }
                    Ok(history.target_instance_id)
                })
                .collect::<Result<std::collections::BTreeSet<_>, Error>>()?;
            if target_instance_ids.len() != 1 {
                return Err(Error::new(
                    eyre!("{}", t!("backup.scheduled.target-identity-mismatch")),
                    ErrorKind::InvalidRequest,
                ));
            }
            let target_instance_id = target_instance_ids
                .first()
                .expect("one target instance ID exists");
            let (guard, credential) = mount_scheduled_target(
                &db,
                &target_id,
                &server_id,
                target_instance_id,
                password.as_deref(),
            )
            .await?;
            refreshed_credential = Some(credential);
            guard
        } else {
            let password = password.as_deref().ok_or_else(|| {
                Error::new(
                    eyre!("{}", t!("backup.scheduled.reauth-required")),
                    ErrorKind::InvalidRequest,
                )
            })?;
            let target = target_id.clone().load(&db)?;
            ScheduledBackupMountGuard::discover_with_password(
                TmpMountGuard::mount(&target, ReadWrite).await?,
                &server_id,
                password,
            )
            .await?
            .0
        };
        validate_scheduled_snapshots(&guard, &snapshots).await?;
        scheduled_guard = Some(guard);
    }

    if !manual_ids.is_empty() {
        let password = password.as_deref().ok_or_else(|| {
            Error::new(
                eyre!("{}", t!("backup.scheduled.reauth-required")),
                ErrorKind::InvalidRequest,
            )
        })?;
        let target = target_id.clone().load(&db)?;
        let guard = BackupMountGuard::mount(
            TmpMountGuard::mount(&target, ReadWrite).await?,
            &server_id,
            password,
        )
        .await?;
        manual_guard = Some(guard);
    }
    drop(db);

    if let Some(credential) = refreshed_credential {
        ctx.db
            .mutate(|db| {
                db.as_private_mut()
                    .as_scheduled_backup_credentials_mut()
                    .insert(&target_id.to_string(), &credential)?;
                Ok(())
            })
            .await
            .result?;
    }
    if let Some(guard) = scheduled_guard {
        tasks.extend(restore_scheduled_packages(&ctx, guard, snapshots));
    }
    if let Some(guard) = manual_guard {
        tasks.extend(restore_packages(&ctx, guard, manual_ids));
    }

    let intended_services = tasks.keys().cloned().collect();
    let activity = running_activity(
        BackupActivityKind::Restore,
        target_id,
        Some(server_id),
        None,
        None,
        intended_services,
    );
    ctx.db
        .mutate(|db| insert_activity(db, &activity))
        .await
        .result?;
    spawn_restore_activity(ctx, activity.id, tasks, operation_coordinator);
    Ok(())
}

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestoreScheduledPackagesParams {
    pub target_id: BackupTargetId,
    pub snapshots: BTreeMap<PackageId, ServiceSnapshotId>,
    #[serde(default)]
    #[ts(optional)]
    pub server_id: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub password: Option<String>,
}

pub async fn restore_scheduled_packages_rpc(
    ctx: RpcContext,
    RestoreScheduledPackagesParams {
        target_id,
        snapshots,
        server_id,
        password,
    }: RestoreScheduledPackagesParams,
) -> Result<(), Error> {
    restore_selection_rpc(
        ctx,
        RestoreSelectionParams {
            target_id,
            manual_ids: Vec::new(),
            snapshots,
            server_id,
            password,
        },
    )
    .await
}

fn restore_scheduled_packages(
    ctx: &RpcContext,
    guard: ScheduledBackupMountGuard<TmpMountGuard>,
    snapshots: BTreeMap<PackageId, ServiceSnapshotId>,
) -> BTreeMap<PackageId, DownloadInstallFuture> {
    let guard = Arc::new(guard);
    let mut tasks = BTreeMap::new();
    for (package_id, snapshot_id) in snapshots {
        let snapshot = guard.snapshot(&package_id, &snapshot_id);
        let s9pk_path = snapshot.path().join(&package_id).with_extension("s9pk");
        let ctx = ctx.clone();
        let id = package_id.clone();
        let task = defer_restore_preparation(async move {
            ctx.services
                .install(
                    ctx.clone(),
                    || S9pk::open(s9pk_path, Some(&id)),
                    None,
                    Some(snapshot),
                    None,
                )
                .await
        });
        tasks.insert(package_id, task);
    }

    tasks
}

fn defer_restore_preparation(
    preparation: impl std::future::Future<Output = Result<DownloadInstallFuture, Error>>
    + Send
    + 'static,
) -> DownloadInstallFuture {
    async move { preparation.await?.await }.boxed()
}

async fn validate_scheduled_snapshots(
    guard: &ScheduledBackupMountGuard<TmpMountGuard>,
    snapshots: &BTreeMap<PackageId, ServiceSnapshotId>,
) -> Result<(), Error> {
    validate_snapshot_selection(&guard.metadata, snapshots)?;
    for (package_id, snapshot_id) in snapshots {
        if tokio::fs::metadata(guard.snapshot_path(package_id, snapshot_id))
            .await
            .is_err()
        {
            return Err(Error::new(
                eyre!("{}", t!("backup.scheduled.snapshot-not-found")),
                ErrorKind::NotFound,
            ));
        }
    }
    Ok(())
}

fn validate_snapshot_selection(
    metadata: &super::scheduled::ScheduledBackupOnTargetMetadata,
    snapshots: &BTreeMap<PackageId, ServiceSnapshotId>,
) -> Result<(), Error> {
    for (package_id, snapshot_id) in snapshots {
        if !metadata.services.get(package_id).is_some_and(|history| {
            history
                .snapshots
                .iter()
                .any(|snapshot| &snapshot.id == snapshot_id)
        }) {
            return Err(Error::new(
                eyre!("{}", t!("backup.scheduled.snapshot-not-found")),
                ErrorKind::NotFound,
            ));
        }
    }
    Ok(())
}

fn spawn_restore_activity(
    ctx: RpcContext,
    activity_id: super::scheduled::BackupActivityId,
    tasks: BTreeMap<PackageId, DownloadInstallFuture>,
    operation_coordinator: OwnedMutexGuard<()>,
) {
    tokio::spawn(async move {
        let _operation_coordinator = operation_coordinator;
        let reports = Arc::new(Mutex::new(BTreeMap::new()));
        stream::iter(tasks)
            .for_each_concurrent(5, |(id, result)| {
                let reports = reports.clone();
                async move {
                    let started = Instant::now();
                    let error = async { result.await?.await }.await.err();
                    if let Some(error) = &error {
                        tracing::error!(
                            "{}",
                            t!("backup.restore.package-error", id = id, error = error)
                        );
                        tracing::debug!("{error:?}");
                    }
                    reports.lock().await.insert(
                        id,
                        PackageBackupReport {
                            error: error.map(|error| error.to_string()),
                            duration_ms: started.elapsed().as_millis() as u64,
                            logical_size: None,
                            physical_size: None,
                            changed_bytes: None,
                            measured_at: None,
                        },
                    );
                }
            })
            .await;
        let reports = Arc::try_unwrap(reports).unwrap().into_inner();
        let failures = reports
            .values()
            .filter(|report| report.error.is_some())
            .count();
        let state = if failures == 0 {
            BackupRunState::Succeeded
        } else if failures == reports.len() {
            BackupRunState::Failed
        } else {
            BackupRunState::PartiallyFailed
        };
        ctx.db
            .mutate(|db| complete_activity(db, &activity_id, state, reports, None))
            .await
            .result
            .log_err();
    });
}

fn restored_hostname(
    backup_hostname: ServerHostname,
    requested_hostname: Option<ServerHostname>,
) -> ServerHostname {
    requested_hostname.unwrap_or_else(|| repair_hostname(backup_hostname.as_ref()))
}

#[instrument(skip_all)]
pub async fn recover_full_server(
    ctx: &SetupContext,
    disk_guid: InternedString,
    password: Option<String>,
    recovery_source: TmpMountGuard,
    server_id: &str,
    recovery_password: &str,
    kiosk: bool,
    hostname: Option<ServerHostname>,
    SetupExecuteProgress {
        init_phases,
        restore_phase,
        rpc_ctx_phases,
    }: SetupExecuteProgress,
) -> Result<(SetupResult, RpcContext), Error> {
    let backup_guard =
        BackupMountGuard::mount(recovery_source, server_id, recovery_password).await?;
    let os_backup_path = backup_guard.path().join("os-backup.json");
    let os_backup = read_os_backup(&os_backup_path).await?;
    let ids = backup_guard
        .metadata
        .package_backups
        .keys()
        .cloned()
        .collect();
    let (result, rpc_ctx, restore_phase) = initialize_recovered_server(
        ctx,
        disk_guid,
        password,
        os_backup,
        kiosk,
        hostname,
        init_phases,
        restore_phase,
        rpc_ctx_phases,
    )
    .await?;
    let tasks = restore_packages(&rpc_ctx, backup_guard, ids);
    restore_setup_services(tasks, restore_phase).await;
    Ok((result, rpc_ctx))
}

#[instrument(skip_all)]
pub async fn recover_full_server_from_scheduled(
    ctx: &SetupContext,
    disk_guid: InternedString,
    password: Option<String>,
    recovery_source: TmpMountGuard,
    server_id: &str,
    recovery_password: &str,
    kiosk: bool,
    hostname: Option<ServerHostname>,
    SetupExecuteProgress {
        init_phases,
        restore_phase,
        rpc_ctx_phases,
    }: SetupExecuteProgress,
) -> Result<(SetupResult, RpcContext), Error> {
    let (backup_guard, _) = ScheduledBackupMountGuard::discover_with_password(
        recovery_source,
        server_id,
        recovery_password,
    )
    .await?;
    let mut snapshots = latest_scheduled_snapshots(&backup_guard.metadata);
    let system_snapshot = snapshots.remove(&*SYSTEM_PACKAGE_ID).ok_or_else(|| {
        Error::new(
            eyre!("{}", t!("backup.scheduled.system-snapshot-not-found")),
            ErrorKind::NotFound,
        )
    })?;
    let os_backup_path = backup_guard
        .snapshot_path(&*SYSTEM_PACKAGE_ID, &system_snapshot)
        .join("os-backup.json");
    let os_backup = read_os_backup(&os_backup_path).await?;
    let (result, rpc_ctx, restore_phase) = initialize_recovered_server(
        ctx,
        disk_guid,
        password,
        os_backup,
        kiosk,
        hostname,
        init_phases,
        restore_phase,
        rpc_ctx_phases,
    )
    .await?;
    let tasks = restore_scheduled_packages(&rpc_ctx, backup_guard, snapshots);
    restore_setup_services(tasks, restore_phase).await;
    Ok((result, rpc_ctx))
}

fn latest_scheduled_snapshots(
    metadata: &super::scheduled::ScheduledBackupOnTargetMetadata,
) -> BTreeMap<PackageId, ServiceSnapshotId> {
    metadata
        .services
        .iter()
        .filter_map(|(package_id, history)| {
            history
                .snapshots
                .iter()
                .max_by_key(|snapshot| snapshot.completed_at)
                .map(|snapshot| (package_id.clone(), snapshot.id.clone()))
        })
        .collect()
}

#[cfg(test)]
mod scheduled_recovery_tests {
    use chrono::{DateTime, Utc};

    use super::*;
    use crate::backup::scheduled::{
        BackupJobId, BackupRunId, BackupSource, OnTargetServiceHistory, RetentionPolicy,
        ScheduledBackupOnTargetMetadata, ServiceSnapshot,
    };

    fn snapshot(package_id: &PackageId, completed_at: i64) -> ServiceSnapshot {
        let completed_at = DateTime::<Utc>::from_timestamp(completed_at, 0).unwrap();
        ServiceSnapshot {
            id: ServiceSnapshotId::new(),
            package_id: package_id.clone(),
            package_version: "1.0.0".to_owned(),
            source: BackupSource::Scheduled,
            job_id: BackupJobId::new(),
            job_name: "Daily".to_owned(),
            run_id: BackupRunId::new(),
            completed_at,
            logical_size: 1,
            physical_size: Some(1),
            changed_bytes: Some(1),
            measured_at: completed_at,
            archived: false,
        }
    }

    #[test]
    fn restore_selection_requires_a_checkpoint_owned_by_the_source_service() {
        let package_id: PackageId = "bitcoind".parse().unwrap();
        let checkpoint = snapshot(&package_id, 1);
        let metadata = ScheduledBackupOnTargetMetadata {
            target_instance_id: "foreign-target".to_owned(),
            services: BTreeMap::from([(
                package_id.clone(),
                OnTargetServiceHistory {
                    timezone: "UTC".to_owned(),
                    policy: RetentionPolicy::latest_only(),
                    archived: false,
                    snapshots: vec![checkpoint.clone()],
                },
            )]),
        };

        assert!(
            validate_snapshot_selection(
                &metadata,
                &BTreeMap::from([(package_id.clone(), checkpoint.id.clone())]),
            )
            .is_ok()
        );
        assert!(
            validate_snapshot_selection(
                &metadata,
                &BTreeMap::from([(package_id, ServiceSnapshotId::new())]),
            )
            .is_err()
        );
        assert!(
            validate_snapshot_selection(
                &metadata,
                &BTreeMap::from([("lnd".parse().unwrap(), checkpoint.id)]),
            )
            .is_err()
        );
    }

    #[test]
    fn full_recovery_selects_the_newest_checkpoint_for_each_package() {
        let package_id: PackageId = "bitcoind".parse().unwrap();
        let older = snapshot(&package_id, 1);
        let newer = snapshot(&package_id, 2);
        let metadata = ScheduledBackupOnTargetMetadata {
            target_instance_id: "target".to_owned(),
            services: BTreeMap::from([(
                package_id.clone(),
                OnTargetServiceHistory {
                    timezone: "UTC".to_owned(),
                    policy: RetentionPolicy::latest_only(),
                    archived: false,
                    snapshots: vec![newer.clone(), older],
                },
            )]),
        };

        assert_eq!(
            latest_scheduled_snapshots(&metadata).get(&package_id),
            Some(&newer.id)
        );
    }
}

async fn read_os_backup(path: &std::path::Path) -> Result<OsBackup, Error> {
    IoFormat::Json.from_slice(
        &tokio::fs::read(path)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, path.display().to_string()))?,
    )
}

async fn initialize_recovered_server(
    ctx: &SetupContext,
    disk_guid: InternedString,
    password: Option<String>,
    mut os_backup: OsBackup,
    kiosk: bool,
    hostname: Option<ServerHostname>,
    init_phases: InitPhases,
    restore_phase: Option<PhaseProgressTrackerHandle>,
    rpc_ctx_phases: InitRpcContextPhases,
) -> Result<(SetupResult, RpcContext, PhaseProgressTrackerHandle), Error> {
    let restore_phase = restore_phase.or_not_found("restore progress")?;

    if let Some(password) = password {
        os_backup.account.password = argon2::hash_encoded(
            password.as_bytes(),
            &rand::random::<[u8; 16]>()[..],
            &argon2::Config::rfc9106_low_mem(),
        )
        .with_kind(ErrorKind::PasswordHashGeneration)?;
    }

    os_backup.account.hostname = restored_hostname(os_backup.account.hostname, hostname);

    sync_kiosk(kiosk).await?;

    let language = ctx.language.peek(|a| a.clone());
    let keyboard = ctx.keyboard.peek(|a| a.clone());

    if let Some(language) = &language {
        save_language(&**language).await?;
    }

    if let Some(keyboard) = &keyboard {
        keyboard.save().await?;
    }

    let db = ctx.db().await?;
    db.put(
        &ROOT,
        &Database::init(&os_backup.account, kiosk, language, keyboard)?,
    )
    .await?;
    drop(db);

    let config = ctx.config.peek(|c| c.clone());

    let init_result = init(&ctx.webserver, &config, init_phases).await?;

    let rpc_ctx = RpcContext::init(
        &ctx.webserver,
        &config,
        disk_guid.clone(),
        Some(init_result),
        rpc_ctx_phases,
    )
    .await?;

    let result = SetupResult {
        hostname: os_backup.account.hostname,
        root_ca: Pem(os_backup.account.root_ca_cert),
        needs_restart: ctx.install_rootfs.peek(|a| a.is_some()),
    };
    Ok((result, rpc_ctx, restore_phase))
}

async fn restore_setup_services(
    tasks: BTreeMap<PackageId, DownloadInstallFuture>,
    mut restore_phase: PhaseProgressTrackerHandle,
) {
    restore_phase.start();
    restore_phase.set_total(tasks.len() as u64);
    restore_phase.set_units(Some(ProgressUnits::Steps));
    let restore_phase = Arc::new(Mutex::new(restore_phase));
    stream::iter(tasks)
        .for_each_concurrent(5, |(id, res)| {
            let restore_phase = restore_phase.clone();
            async move {
                match async { res.await?.await }.await {
                    Ok(_) => (),
                    Err(err) => {
                        tracing::error!(
                            "{}",
                            t!("backup.restore.package-error", id = id, error = err)
                        );
                        tracing::debug!("{:?}", err);
                    }
                }
                *restore_phase.lock().await += 1;
            }
        })
        .await;
    restore_phase.lock().await.complete();
}

#[instrument(skip(ctx, backup_guard))]
fn restore_packages(
    ctx: &RpcContext,
    backup_guard: BackupMountGuard<TmpMountGuard>,
    ids: Vec<PackageId>,
) -> BTreeMap<PackageId, DownloadInstallFuture> {
    let backup_guard = Arc::new(backup_guard);
    let mut tasks = BTreeMap::new();
    for id in ids {
        let ctx = ctx.clone();
        let backup_guard = backup_guard.clone();
        let package_id = id.clone();
        let task = defer_restore_preparation(async move {
            let backup_dir = backup_guard.package_backup(&package_id).await?;
            let s9pk_path = backup_dir.path().join(&package_id).with_extension("s9pk");
            ctx.services
                .install(
                    ctx.clone(),
                    || S9pk::open(s9pk_path, Some(&package_id)),
                    None,
                    Some(backup_dir),
                    None,
                )
                .await
        });
        tasks.insert(id, task);
    }

    tasks
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn restore_preparation_is_deferred_and_failures_leave_siblings_running() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let completed = Arc::new(AtomicUsize::new(0));
        let failed = defer_restore_preparation({
            let completed = completed.clone();
            async move {
                completed.fetch_add(1, Ordering::SeqCst);
                Err(Error::new(eyre!("Unreadable archive"), ErrorKind::Backup))
            }
        });
        let succeeded = defer_restore_preparation({
            let completed = completed.clone();
            async move {
                completed.fetch_add(1, Ordering::SeqCst);
                Ok(async move {
                    Ok(async move {
                        completed.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    }
                    .boxed())
                }
                .boxed())
            }
        });

        assert_eq!(completed.load(Ordering::SeqCst), 0);
        let (failed, succeeded) = tokio::join!(async { failed.await?.await }, async {
            succeeded.await?.await
        });
        let failed = failed.unwrap_err();
        assert_eq!(failed.kind, ErrorKind::Backup);
        assert_eq!(failed.source.to_string(), "Unreadable archive");
        succeeded.unwrap();
        assert_eq!(completed.load(Ordering::SeqCst), 3);
    }

    fn hostname(value: &str) -> ServerHostname {
        ServerHostname::new(InternedString::intern(value)).unwrap()
    }

    #[test]
    fn restore_preserves_the_backup_hostname() {
        assert_eq!(
            restored_hostname(hostname("preserved-host"), None).as_ref(),
            "preserved-host"
        );
    }

    #[test]
    fn restore_repairs_a_backup_hostname_over_the_limit() {
        assert_eq!(
            restored_hostname(hostname(&"a".repeat(50)), None).as_ref(),
            "a".repeat(32)
        );
    }

    #[test]
    fn restore_uses_an_explicit_replacement_hostname() {
        assert_eq!(
            restored_hostname(
                hostname("preserved-host"),
                Some(hostname("replacement-host"))
            )
            .as_ref(),
            "replacement-host"
        );
    }
}
