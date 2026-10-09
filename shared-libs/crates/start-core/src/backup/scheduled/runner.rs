use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use color_eyre::eyre::eyre;
use imbl_value::InternedString;
use tokio::sync::OwnedMutexGuard;

use super::retention::{CAPACITY_MARGIN_PERCENT, with_margin};
use super::{
    BackupJob, BackupJobId, BackupJobPause, BackupRun, BackupRunState, BackupRunTrigger,
    BackupServiceScope, BackupTargetFailureState, ScheduledBackupCredential,
    ScheduledBackupMountGuard, ServiceSnapshot, ServiceSnapshotId, activity_from_run,
    insert_activity, prune_completed_history,
};
use crate::backup::PackageBackupReport;
use crate::backup::scheduled::history_key;
use crate::backup::target::{BackupTargetFS, BackupTargetId};
use crate::context::RpcContext;
use crate::disk::mount::filesystem::ReadWrite;
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;
use crate::progress::{FullProgress, FullProgressTracker};
use crate::rpc_continuations::Guid;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::{delete_dir, dir_size};
use crate::version::VersionT;
use crate::volume::PKG_VOLUME_DIR;
use crate::{DATA_DIR, PackageId, SYSTEM_PACKAGE_ID};

const PREFLIGHT_METADATA_BYTES: u64 = 1024 * 1024;
const TARGET_MOUNT_RETRY_DELAYS: [Duration; 2] =
    [Duration::from_millis(500), Duration::from_millis(1500)];

async fn retry_mount<T, F, Fut>(mut mount: F, retry_delays: &[Duration]) -> Result<T, Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, Error>>,
{
    let mut result = mount().await;
    for delay in retry_delays {
        if result.is_ok() {
            return result;
        }
        tokio::time::sleep(*delay).await;
        result = mount().await;
    }
    result
}

async fn mount_target(target: &BackupTargetFS) -> Result<TmpMountGuard, Error> {
    retry_mount(
        || TmpMountGuard::mount(target, ReadWrite),
        &TARGET_MOUNT_RETRY_DELAYS,
    )
    .await
}

/// Runs one automatic backup job while holding the global backup coordinator.
pub async fn run_job(
    ctx: RpcContext,
    job_id: BackupJobId,
    trigger: BackupRunTrigger,
) -> Result<BackupRun, Error> {
    let coordinator = crate::backup::try_backup_coordinator(ctx.backup_coordinator.clone())?;
    // Client disconnects must not release active backup ownership.
    tokio::spawn(run_job_with_coordinator(ctx, job_id, trigger, coordinator))
        .await
        .map_err(|error| Error::new(error, ErrorKind::Unknown))?
}

#[tracing::instrument(skip(ctx, _coordinator), err)]
pub(super) async fn run_job_with_coordinator(
    ctx: RpcContext,
    job_id: BackupJobId,
    trigger: BackupRunTrigger,
    _coordinator: OwnedMutexGuard<()>,
) -> Result<BackupRun, Error> {
    super::reconcile_interrupted_backup_state(&ctx).await?;
    let result = run_job_inner(&ctx, &job_id, trigger).await;
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_status_info_mut()
                .as_backup_progress_mut()
                .ser(&None)
        })
        .await
        .result?;
    result
}

async fn run_job_inner(
    ctx: &RpcContext,
    job_id: &BackupJobId,
    trigger: BackupRunTrigger,
) -> Result<BackupRun, Error> {
    let db = ctx.db.peek().await;
    let job: BackupJob = db
        .as_public()
        .as_scheduled_backups()
        .as_jobs()
        .as_idx(job_id)
        .or_not_found(job_id)?
        .de()?;
    if !job.enabled || job.pause.is_some() {
        return Err(Error::new(
            eyre!("{}", t!("backup.scheduled.job-paused")),
            ErrorKind::InvalidRequest,
        ));
    }
    let target_name = job.target_id.user_facing_name(&db);
    let package_ids = selected_services(&db, &job.services)?;
    if package_ids.is_empty() {
        let error = Error::new(
            eyre!("{}", t!("backup.scheduled.no-installed-services")),
            ErrorKind::InvalidRequest,
        );
        ctx.db
            .mutate(|db| super::pause_job_without_services(db, &job.id))
            .await
            .result?;
        record_failed_run(ctx, &job, &package_ids, trigger, error.to_string()).await?;
        return Err(error);
    }
    drop(db);
    if let Err(error) = ctx
        .db
        .mutate(|db| super::associate_histories(db, &job, &package_ids))
        .await
        .result
    {
        record_failed_run_and_notify(ctx, &job, &package_ids, trigger, error.to_string()).await?;
        return Err(error);
    }
    let system_logical_bytes = match crate::backup::os::system_logical_size(ctx).await {
        Ok(size) => size,
        Err(error) => {
            record_failed_run_and_notify(ctx, &job, &package_ids, trigger, error.to_string())
                .await?;
            return Err(error);
        }
    };
    let db = ctx.db.peek().await;
    tracing::info!(
        job_id = %job.id,
        job_name = %job.name,
        target = %target_name,
        ?trigger,
        service_count = package_ids.len(),
        "automatic backup started"
    );
    let encryption_key = (|| {
        let credential: ScheduledBackupCredential = db
            .as_private()
            .as_scheduled_backup_credentials()
            .as_idx(&job.target_id.to_string())
            .or_not_found(job.target_id.to_string())?
            .de()?;
        let device_key = db.as_private().as_scheduled_backup_device_key().de()?;
        credential.open(&device_key)
    })();
    let encryption_key = match encryption_key {
        Ok(key) => key,
        Err(error) => {
            let message = error.to_string();
            pause_for_intervention(
                ctx,
                &job,
                BackupJobPause::ReauthenticationRequired,
                t!("backup.scheduled.reauth-title").to_string(),
                t!(
                    "backup.scheduled.reauth-message",
                    job = job.name,
                    target = target_name.as_str()
                )
                .to_string(),
            )
            .await?;
            record_failed_run(ctx, &job, &package_ids, trigger, message).await?;
            return Err(error);
        }
    };
    let server_id = db.as_public().as_server_info().as_id().de()?;
    let target_fs = match job.target_id.clone().load(&db) {
        Ok(target) => target,
        Err(error) => {
            record_target_failure(ctx, &job, &package_ids, trigger, &error).await?;
            return Err(error);
        }
    };
    let target_guard = match mount_target(&target_fs).await {
        Ok(guard) => guard,
        Err(error) => {
            record_target_failure(ctx, &job, &package_ids, trigger, &error).await?;
            return Err(error);
        }
    };

    let mut scheduled_guard = match ScheduledBackupMountGuard::mount_with_key(
        target_guard,
        &server_id,
        &job.target_instance_id,
        &encryption_key,
    )
    .await
    {
        Ok(guard) => guard,
        Err(error) => {
            let message = error.to_string();
            let (pause, title, notification) = target_read_failure(&error, &job.name, &target_name);
            pause_for_intervention(ctx, &job, pause, title, notification).await?;
            record_failed_run(ctx, &job, &package_ids, trigger, message).await?;
            return Err(error);
        }
    };
    super::rpc::reconcile_target_histories(&db, &job.target_id, &mut scheduled_guard)?;
    mark_target_connected(ctx, &job.target_id).await?;
    let cleanup = async {
        super::storage::remove_abandoned_staging(scheduled_guard.path()).await?;
        super::storage::remove_unreferenced_snapshots(
            scheduled_guard.path(),
            &scheduled_guard.metadata,
        )
        .await
    };
    if let Err(error) = cleanup.await {
        record_failed_run_and_notify(ctx, &job, &package_ids, trigger, error.to_string()).await?;
        return Err(error);
    }
    let target_available =
        match crate::disk::util::get_available(scheduled_guard.target_path()).await {
            Ok(available) => available,
            Err(error) => {
                record_target_failure(ctx, &job, &package_ids, trigger, &error).await?;
                return Err(error);
            }
        };
    if let Err(error) = preflight_capacity(
        &db,
        &package_ids,
        &scheduled_guard,
        target_available,
        system_logical_bytes,
    )
    .await
    {
        let message = error.to_string();
        record_failed_run(ctx, &job, &package_ids, trigger, message).await?;
        ctx.db
            .mutate(|db| {
                notify(
                    db,
                    None,
                    NotificationLevel::Error,
                    t!("backup.scheduled.capacity-title").to_string(),
                    t!(
                        "backup.scheduled.capacity-message",
                        job = job.name,
                        target = target_name.as_str()
                    )
                    .to_string(),
                    (),
                )
            })
            .await
            .result?;
        return Err(error);
    }
    let mut scheduled_guard = Arc::new(scheduled_guard);

    let now = Utc::now();
    let mut run = BackupRun {
        id: Guid::new(),
        job_id: job.id.clone(),
        job_name: job.name.clone(),
        target_id: job.target_id.clone(),
        trigger,
        state: BackupRunState::Running,
        started_at: now,
        completed_at: None,
        intended_services: package_ids.clone(),
        services: BTreeMap::new(),
        error: None,
    };

    if let Err(error) = scheduled_guard.save_run(&run).await {
        let message = error.to_string();
        record_failed_run_and_notify(ctx, &job, &package_ids, trigger, message).await?;
        return Err(error);
    }

    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_status_info_mut()
                .as_backup_progress_mut()
                .ser(&Some(FullProgress::new()))?;
            db.as_public_mut()
                .as_scheduled_backups_mut()
                .as_runs_mut()
                .insert(&run.id, &run)?;
            insert_activity(db, &activity_from_run(&run))?;
            Ok(())
        })
        .await
        .result?;

    let progress = FullProgressTracker::new();
    let mut phases: BTreeMap<PackageId, _> = package_ids
        .iter()
        .map(|id| {
            (
                id.clone(),
                progress.add_phase(InternedString::intern(&backup_item_name(id)), Some(1)),
            )
        })
        .collect();
    let _progress_sync = NonDetachingJoinHandle::from(tokio::spawn(progress.clone().sync_to_db(
        ctx.db.clone(),
        |db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_status_info_mut()
                .as_backup_progress_mut()
                .transpose_mut()
        },
        Some(std::time::Duration::from_millis(300)),
    )));

    for package_id in &package_ids {
        let started = Instant::now();
        let available_before = crate::disk::util::get_available(scheduled_guard.target_path())
            .await
            .ok();
        let mut phase = phases.remove(package_id).expect("backup phase exists");
        phase.start();
        tracing::info!(
            job_id = %job.id,
            run_id = %run.id,
            service = %package_id,
            "automatic backup service started"
        );
        let report = if package_id == &*SYSTEM_PACKAGE_ID {
            match scheduled_guard.staging(&run.id, package_id).await {
                Ok(staging) => {
                    let staging_path = staging.path().to_owned();
                    drop(staging);
                    let backup_result = crate::backup::os::backup_system(ctx, &staging_path).await;
                    phase.complete();
                    match backup_result {
                        Ok(()) => {
                            let physical_size = consumed_capacity(
                                available_before,
                                crate::disk::util::get_available(scheduled_guard.target_path())
                                    .await
                                    .ok(),
                            );
                            let (guard, report) = promote_staging(
                                scheduled_guard,
                                &db,
                                &job,
                                &run,
                                package_id,
                                crate::version::Current::default().semver().to_string(),
                                physical_size,
                                None,
                                started,
                            )
                            .await?;
                            scheduled_guard = guard;
                            report
                        }
                        Err(error) => {
                            delete_dir(
                                &scheduled_guard
                                    .path()
                                    .join("staging")
                                    .join(run.id.as_ref())
                                    .join(&**package_id),
                            )
                            .await
                            .log_err();
                            failed_report(started, error)
                        }
                    }
                }
                Err(error) => {
                    phase.complete();
                    failed_report(started, error)
                }
            }
        } else if let Some(service) = &*ctx.services.get(package_id).await {
            match scheduled_guard.staging(&run.id, package_id).await {
                Ok(staging) => match service.backup(staging, phase).await {
                    Ok(output) => {
                        let physical_size = consumed_capacity(
                            available_before,
                            crate::disk::util::get_available(scheduled_guard.target_path())
                                .await
                                .ok(),
                        );
                        let manifest = db
                            .as_public()
                            .as_package_data()
                            .as_idx(package_id)
                            .or_not_found(package_id)?
                            .as_state_info()
                            .expect_installed()?
                            .as_manifest();
                        let package_version = manifest.as_version().de()?.to_string();
                        let (guard, report) = promote_staging(
                            scheduled_guard,
                            &db,
                            &job,
                            &run,
                            package_id,
                            package_version,
                            physical_size,
                            output.changed_bytes,
                            started,
                        )
                        .await?;
                        scheduled_guard = guard;
                        report
                    }
                    Err(error) => {
                        delete_dir(
                            &scheduled_guard
                                .path()
                                .join("staging")
                                .join(run.id.as_ref())
                                .join(&**package_id),
                        )
                        .await
                        .log_err();
                        failed_report(started, error)
                    }
                },
                Err(error) => {
                    delete_dir(
                        &scheduled_guard
                            .path()
                            .join("staging")
                            .join(run.id.as_ref())
                            .join(&**package_id),
                    )
                    .await
                    .log_err();
                    phase.complete();
                    failed_report(started, error)
                }
            }
        } else {
            phase.complete();
            PackageBackupReport {
                error: Some(t!("backup.scheduled.service-not-ready").to_string()),
                duration_ms: started.elapsed().as_millis() as u64,
                logical_size: None,
                physical_size: None,
                changed_bytes: None,
                measured_at: None,
            }
        };
        if let Some(error) = report.error.as_deref() {
            tracing::warn!(
                job_id = %job.id,
                run_id = %run.id,
                service = %package_id,
                duration_ms = report.duration_ms,
                error,
                "automatic backup service failed"
            );
        } else {
            tracing::info!(
                job_id = %job.id,
                run_id = %run.id,
                service = %package_id,
                duration_ms = report.duration_ms,
                logical_size = ?report.logical_size,
                physical_size = ?report.physical_size,
                changed_bytes = ?report.changed_bytes,
                "automatic backup service completed"
            );
        }
        run.services.insert(package_id.clone(), report);
    }
    progress.complete();

    let failed = run
        .services
        .values()
        .filter(|report| report.error.is_some())
        .count();
    run.state = if failed == 0 {
        BackupRunState::Succeeded
    } else if failed == run.services.len() {
        BackupRunState::Failed
    } else {
        BackupRunState::PartiallyFailed
    };
    run.completed_at = Some(Utc::now());

    delete_dir(&scheduled_guard.path().join("staging").join(run.id.as_ref()))
        .await
        .log_err();

    let owned = Arc::try_unwrap(scheduled_guard).map_err(|_| {
        Error::new(
            eyre!("{}", t!("backup.scheduled.leaked-reference")),
            ErrorKind::Incoherent,
        )
    })?;
    let target_metadata = owned.metadata.clone();
    let run_save_error = owned.save_run(&run).await.err();
    let unmount_error = owned.save_and_unmount().await.err();
    if let Some(error) = run_save_error.or(unmount_error) {
        run.error = Some(error.to_string());
        run.state = if run.services.values().any(|report| report.error.is_none()) {
            BackupRunState::PartiallyFailed
        } else {
            BackupRunState::Failed
        };
    }

    ctx.db
        .mutate(|db| {
            let state = db.as_public_mut().as_scheduled_backups_mut();
            state.as_runs_mut().insert(&run.id, &run)?;
            state
                .as_activities_mut()
                .insert(&run.id, &activity_from_run(&run))?;
            for (package_id, history) in target_metadata.services {
                let key = history_key(&job.target_id, &package_id);
                if let Some(public_history) = state.as_histories_mut().as_idx_mut(&key) {
                    public_history.as_snapshots_mut().ser(&history.snapshots)?;
                    public_history.as_archived_mut().ser(&history.archived)?;
                }
            }
            let mut persisted_job: BackupJob = state
                .as_jobs()
                .as_idx(&job.id)
                .or_not_found(&job.id)?
                .de()?;
            persisted_job.status.last_attempted_at = Some(run.started_at);
            if run.state == BackupRunState::Succeeded {
                persisted_job.status.last_succeeded_at = run.completed_at;
                persisted_job.status.consecutive_failures = 0;
            } else {
                persisted_job.status.consecutive_failures =
                    persisted_job.status.consecutive_failures.saturating_add(1);
            }
            persisted_job.status.last_result = Some(run.state);
            state.as_jobs_mut().insert(&job.id, &persisted_job)?;
            prune_completed_history(db)?;
            Ok(())
        })
        .await
        .result?;
    if run.state != BackupRunState::Succeeded {
        let failed_packages = run
            .services
            .iter()
            .filter(|(_, report)| report.error.is_some())
            .map(|(package, _)| package.clone())
            .collect::<BTreeSet<_>>();
        let affected = if failed_packages.is_empty() {
            &package_ids
        } else {
            &failed_packages
        };
        ctx.db
            .mutate(|db| notify_run_failure(db, &job.name, &job.target_id, affected))
            .await
            .result?;
    }
    tracing::info!(
        job_id = %job.id,
        job_name = %job.name,
        target = %target_name,
        run_id = %run.id,
        ?trigger,
        state = ?run.state,
        service_count = run.services.len(),
        failed_service_count = run.services.values().filter(|report| report.error.is_some()).count(),
        "automatic backup completed"
    );
    Ok(run)
}

fn failed_report(started: Instant, error: Error) -> PackageBackupReport {
    PackageBackupReport {
        error: Some(error.to_string()),
        duration_ms: started.elapsed().as_millis() as u64,
        logical_size: None,
        physical_size: None,
        changed_bytes: None,
        measured_at: None,
    }
}

fn backup_item_name(package_id: &PackageId) -> String {
    if package_id == &*SYSTEM_PACKAGE_ID {
        t!("backup.scheduled.system").to_string()
    } else {
        package_id.to_string()
    }
}

async fn promote_staging(
    scheduled_guard: Arc<ScheduledBackupMountGuard<TmpMountGuard>>,
    db: &crate::db::model::DatabaseModel,
    job: &BackupJob,
    run: &BackupRun,
    package_id: &PackageId,
    package_version: String,
    physical_size: Option<u64>,
    changed_bytes: Option<u64>,
    started: Instant,
) -> Result<
    (
        Arc<ScheduledBackupMountGuard<TmpMountGuard>>,
        PackageBackupReport,
    ),
    Error,
> {
    let completed_at = Utc::now();
    let snapshot = ServiceSnapshot {
        id: ServiceSnapshotId::new(),
        package_id: package_id.clone(),
        package_version,
        source: super::BackupSource::Scheduled,
        job_id: job.id.clone(),
        job_name: job.name.clone(),
        run_id: run.id.clone(),
        completed_at,
        logical_size: 0,
        physical_size,
        changed_bytes,
        measured_at: completed_at,
        archived: false,
    };
    let history: super::ServiceTargetHistory = db
        .as_public()
        .as_scheduled_backups()
        .as_histories()
        .as_idx(&history_key(&job.target_id, package_id))
        .or_not_found(package_id)?
        .de()?;
    let mut owned = Arc::try_unwrap(scheduled_guard).map_err(|_| {
        Error::new(
            eyre!("{}", t!("backup.scheduled.leaked-reference")),
            ErrorKind::Incoherent,
        )
    })?;
    tracing::info!(
        job_id = %job.id,
        run_id = %run.id,
        service = %package_id,
        "automatic backup service snapshot promotion started"
    );
    let promotion = owned
        .promote(&run.id, snapshot, history.timezone, history.policy)
        .await;
    let report = match promotion {
        Ok(snapshot) => PackageBackupReport {
            error: None,
            duration_ms: started.elapsed().as_millis() as u64,
            logical_size: Some(snapshot.logical_size),
            physical_size: snapshot.physical_size,
            changed_bytes: snapshot.changed_bytes,
            measured_at: Some(snapshot.measured_at),
        },
        Err(error) => failed_report(started, error),
    };
    Ok((Arc::new(owned), report))
}

pub(super) fn notify_run_failure(
    db: &mut crate::db::model::DatabaseModel,
    job_name: &str,
    target_id: &BackupTargetId,
    package_ids: &BTreeSet<PackageId>,
) -> Result<(), Error> {
    let services = package_ids
        .iter()
        .map(backup_item_name)
        .collect::<Vec<_>>()
        .join(", ");
    let target_name = target_id.user_facing_name(db);
    notify(
        db,
        None,
        NotificationLevel::Warning,
        t!("backup.scheduled.run-failed-title").to_string(),
        t!(
            "backup.scheduled.run-failed-message",
            job = job_name,
            target = target_name.as_str(),
            services = services
        )
        .to_string(),
        (),
    )
}

fn selected_services(
    db: &crate::db::model::DatabaseModel,
    scope: &BackupServiceScope,
) -> Result<BTreeSet<PackageId>, Error> {
    let installed: BTreeSet<_> = db
        .as_public()
        .as_package_data()
        .as_entries()?
        .into_iter()
        .filter(|(_, package)| package.as_state_info().expect_installed().is_ok())
        .map(|(id, _)| id)
        .collect();
    Ok(scope.runnable_services(installed))
}

pub(crate) async fn preflight_new_target_capacity(
    ctx: &RpcContext,
    package_ids: &BTreeSet<PackageId>,
    available: u64,
) -> Result<(), Error> {
    let system_logical_bytes = crate::backup::os::system_logical_size(ctx).await?;
    let db = ctx.db.peek().await;
    let mut requirements = Vec::with_capacity(package_ids.len());
    for package_id in package_ids {
        requirements.push(live_logical_size(&db, package_id, system_logical_bytes).await?);
    }
    let required = complete_run_required_capacity(requirements)?;
    if required > available {
        return Err(Error::new(
            eyre!(
                "{}",
                t!(
                    "backup.scheduled.insufficient-capacity",
                    required = required,
                    available = available
                )
            ),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

async fn preflight_capacity<G: GenericMountGuard>(
    db: &crate::db::model::DatabaseModel,
    package_ids: &BTreeSet<PackageId>,
    guard: &ScheduledBackupMountGuard<G>,
    available: u64,
    system_logical_bytes: u64,
) -> Result<(), Error> {
    let mut requirements = Vec::with_capacity(package_ids.len());

    for package_id in package_ids {
        let live_logical = live_logical_size(db, package_id, system_logical_bytes).await?;
        let on_target = guard.metadata.services.get(package_id);
        let latest = on_target
            .into_iter()
            .flat_map(|history| history.snapshots.iter())
            .filter(|snapshot| !snapshot.archived)
            .max_by_key(|snapshot| snapshot.completed_at);
        let copy_bytes = projected_copy_bytes(
            live_logical,
            latest.map(|snapshot| snapshot.logical_size),
            latest.and_then(|snapshot| snapshot.physical_size),
        );
        requirements.push(copy_bytes);
    }

    let required = complete_run_required_capacity(requirements)?;
    if required > available {
        return Err(Error::new(
            eyre!(
                "{}",
                t!(
                    "backup.scheduled.insufficient-capacity",
                    required = required,
                    available = available
                )
            ),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

pub(super) async fn live_logical_size(
    db: &crate::db::model::DatabaseModel,
    package_id: &PackageId,
    system_logical_bytes: u64,
) -> Result<u64, Error> {
    if package_id == &*SYSTEM_PACKAGE_ID {
        return Ok(system_logical_bytes);
    }
    let Some(package) = db.as_public().as_package_data().as_idx(package_id) else {
        return Ok(0);
    };
    if package.as_state_info().expect_installed().is_err() {
        return Ok(0);
    }
    let path = std::path::Path::new(DATA_DIR)
        .join(PKG_VOLUME_DIR)
        .join(package_id);
    let archive: std::path::PathBuf = package.as_s9pk().de()?;
    service_backup_logical_size(&path, &archive).await
}

async fn service_backup_logical_size(
    volume: &std::path::Path,
    archive: &std::path::Path,
) -> Result<u64, Error> {
    let data_bytes = if tokio::fs::metadata(volume).await.is_ok() {
        dir_size(volume, None).await?
    } else {
        0
    };
    let archive_bytes = tokio::fs::metadata(&archive)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, archive.display()))?
        .len();
    data_bytes
        .checked_add(archive_bytes)
        .ok_or_else(capacity_overflow)
}

fn complete_run_required_capacity(
    requirements: impl IntoIterator<Item = u64>,
) -> Result<u64, Error> {
    let mut required = PREFLIGHT_METADATA_BYTES;
    for copy_bytes in requirements {
        let staging =
            with_margin(copy_bytes, CAPACITY_MARGIN_PERCENT).ok_or_else(capacity_overflow)?;
        required = required
            .checked_add(staging)
            .ok_or_else(capacity_overflow)?;
    }
    Ok(required)
}

fn consumed_capacity(before: Option<u64>, after: Option<u64>) -> Option<u64> {
    before
        .zip(after)
        .and_then(|(before, after)| before.checked_sub(after))
        .filter(|consumed| *consumed > 0)
}

fn projected_copy_bytes(
    live_logical: u64,
    latest_logical: Option<u64>,
    latest_physical: Option<u64>,
) -> u64 {
    latest_physical
        .unwrap_or_else(|| latest_logical.unwrap_or(0))
        .max(live_logical)
}

fn capacity_overflow() -> Error {
    Error::new(
        eyre!("{}", t!("backup.scheduled.capacity-overflow")),
        ErrorKind::InvalidRequest,
    )
}

async fn record_target_failure(
    ctx: &RpcContext,
    job: &BackupJob,
    package_ids: &BTreeSet<PackageId>,
    trigger: BackupRunTrigger,
    error: &Error,
) -> Result<(), Error> {
    let message = error.to_string();
    if record_connectivity_failure(ctx, job).await? {
        record_failed_run(ctx, job, package_ids, trigger, message).await?;
    } else {
        record_failed_run_and_notify(ctx, job, package_ids, trigger, message).await?;
    }
    Ok(())
}

async fn record_failed_run(
    ctx: &RpcContext,
    job: &BackupJob,
    package_ids: &BTreeSet<PackageId>,
    trigger: BackupRunTrigger,
    error: String,
) -> Result<BackupRun, Error> {
    let now = Utc::now();
    let run = BackupRun {
        id: Guid::new(),
        job_id: job.id.clone(),
        job_name: job.name.clone(),
        target_id: job.target_id.clone(),
        trigger,
        state: BackupRunState::Failed,
        started_at: now,
        completed_at: Some(now),
        intended_services: package_ids.clone(),
        services: BTreeMap::new(),
        error: Some(error),
    };
    ctx.db
        .mutate(|db| {
            let state = db.as_public_mut().as_scheduled_backups_mut();
            state.as_runs_mut().insert(&run.id, &run)?;
            state
                .as_activities_mut()
                .insert(&run.id, &activity_from_run(&run))?;
            let mut persisted: BackupJob = state
                .as_jobs()
                .as_idx(&job.id)
                .or_not_found(&job.id)?
                .de()?;
            persisted.status.last_attempted_at = Some(now);
            persisted.status.consecutive_failures =
                persisted.status.consecutive_failures.saturating_add(1);
            persisted.status.last_result = Some(BackupRunState::Failed);
            state.as_jobs_mut().insert(&job.id, &persisted)?;
            prune_completed_history(db)?;
            Ok(())
        })
        .await
        .result?;
    let target_name = job.target_id.user_facing_name(&ctx.db.peek().await);
    tracing::warn!(
        job_id = %job.id,
        job_name = %job.name,
        target = %target_name,
        run_id = %run.id,
        ?trigger,
        service_count = package_ids.len(),
        error = run.error.as_deref().unwrap_or_default(),
        "automatic backup failed before copying services"
    );
    Ok(run)
}

async fn record_failed_run_and_notify(
    ctx: &RpcContext,
    job: &BackupJob,
    package_ids: &BTreeSet<PackageId>,
    trigger: BackupRunTrigger,
    error: String,
) -> Result<BackupRun, Error> {
    let run = record_failed_run(ctx, job, package_ids, trigger, error).await?;
    ctx.db
        .mutate(|db| notify_run_failure(db, &job.name, &job.target_id, package_ids))
        .await
        .result?;
    Ok(run)
}

async fn record_connectivity_failure(ctx: &RpcContext, job: &BackupJob) -> Result<bool, Error> {
    let target_key = job.target_id.to_string();
    let notified = ctx
        .db
        .mutate(|db| {
            let target_name = job.target_id.user_facing_name(db);
            let state = db.as_public_mut().as_scheduled_backups_mut();
            let affected: Vec<BackupJob> = state
                .as_jobs()
                .as_entries()?
                .into_iter()
                .map(|(_, job)| job.de())
                .collect::<Result<Vec<BackupJob>, Error>>()?
                .into_iter()
                .filter(|candidate| {
                    candidate.target_id == job.target_id
                        && candidate.enabled
                        && !matches!(candidate.pause, Some(super::BackupJobPause::User))
                })
                .collect();
            let mut failure: BackupTargetFailureState = state
                .as_target_failures()
                .as_idx(&target_key)
                .map(|failure| failure.de())
                .transpose()?
                .unwrap_or_default();
            let notify_user = failure.record_failure(affected.iter().map(|job| job.id.clone()));
            if failure.consecutive_connectivity_failures >= 3 {
                for mut affected_job in affected {
                    affected_job.pause = Some(super::BackupJobPause::TargetUnavailable {
                        failures: failure.consecutive_connectivity_failures,
                    });
                    affected_job.status.next_run_at = None;
                    affected_job.updated_at = Utc::now();
                    state
                        .as_jobs_mut()
                        .insert(&affected_job.id, &affected_job)?;
                }
            }
            state
                .as_target_failures_mut()
                .insert(&target_key, &failure)?;
            if notify_user {
                notify(
                    db,
                    None,
                    NotificationLevel::Error,
                    t!("backup.scheduled.target-unavailable-title").to_string(),
                    t!(
                        "backup.scheduled.target-unavailable-message",
                        target = target_name.as_str()
                    )
                    .to_string(),
                    (),
                )?;
            }
            Ok(notify_user)
        })
        .await
        .result?;
    Ok(notified)
}

async fn mark_target_connected(ctx: &RpcContext, target_id: &BackupTargetId) -> Result<(), Error> {
    let key = target_id.to_string();
    ctx.db
        .mutate(|db| {
            let failures = db
                .as_public_mut()
                .as_scheduled_backups_mut()
                .as_target_failures_mut();
            let Some(existing) = failures.as_idx(&key) else {
                return Ok(());
            };
            let mut state: BackupTargetFailureState = existing.de()?;
            // Paused jobs require an explicit retry.
            if state.jobs_paused.is_empty() {
                state.reset();
                failures.insert(&key, &state)?;
            }
            Ok(())
        })
        .await
        .result
}

async fn pause_for_intervention(
    ctx: &RpcContext,
    job: &BackupJob,
    reason: super::BackupJobPause,
    title: String,
    message: String,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            let state = db.as_public_mut().as_scheduled_backups_mut();
            let affected: Vec<BackupJob> = state
                .as_jobs()
                .as_entries()?
                .into_iter()
                .map(|(_, job)| job.de())
                .collect::<Result<Vec<BackupJob>, Error>>()?
                .into_iter()
                .filter(|candidate| {
                    candidate.target_id == job.target_id
                        && candidate.enabled
                        && !matches!(candidate.pause, Some(super::BackupJobPause::User))
                })
                .collect();
            let should_notify = affected
                .iter()
                .any(|candidate| candidate.pause.as_ref() != Some(&reason));
            for mut affected_job in affected {
                affected_job.pause = Some(reason.clone());
                affected_job.status.next_run_at = None;
                affected_job.updated_at = Utc::now();
                state
                    .as_jobs_mut()
                    .insert(&affected_job.id, &affected_job)?;
            }
            if should_notify {
                notify(db, None, NotificationLevel::Error, title, message, ())?;
            }
            Ok(())
        })
        .await
        .result
}

fn target_read_failure(error: &Error, job: &str, target: &str) -> (BackupJobPause, String, String) {
    if error
        .source
        .downcast_ref::<super::storage::TargetIdentityMismatch>()
        .is_some()
    {
        (
            BackupJobPause::TargetIdentityMismatch,
            t!("backup.scheduled.identity-title").to_string(),
            t!(
                "backup.scheduled.identity-message",
                job = job,
                target = target
            )
            .to_string(),
        )
    } else {
        (
            BackupJobPause::TargetUnreadable,
            t!("backup.scheduled.unreadable-title").to_string(),
            t!(
                "backup.scheduled.unreadable-message",
                job = job,
                target = target
            )
            .to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn target_read_failure_distinguishes_identity_from_unreadable_metadata() {
        let mismatch = Error::new(
            super::super::storage::TargetIdentityMismatch("changed identity".into()),
            ErrorKind::InvalidRequest,
        );
        assert_eq!(
            target_read_failure(&mismatch, "Daily", "Drive").0,
            BackupJobPause::TargetIdentityMismatch,
        );
        for kind in [
            ErrorKind::Filesystem,
            ErrorKind::InvalidRequest,
            ErrorKind::Backup,
        ] {
            let unreadable = Error::new(eyre!("unreadable metadata"), kind);
            assert_eq!(
                target_read_failure(&unreadable, "Daily", "Drive").0,
                BackupJobPause::TargetUnreadable,
            );
        }
    }

    #[test]
    fn preflight_reserves_accumulated_growth_when_replacing_checkpoints() {
        let copy_bytes = 100 * 1024 * 1024;
        let required = complete_run_required_capacity([copy_bytes, copy_bytes]).unwrap();
        assert_eq!(required, PREFLIGHT_METADATA_BYTES + 220 * 1024 * 1024);
    }

    #[test]
    fn complete_preflight_is_order_independent_and_uses_full_copies() {
        let first = complete_run_required_capacity([100, 200]).unwrap();
        let reversed = complete_run_required_capacity([200, 100]).unwrap();
        assert_eq!(first, reversed);
        assert_eq!(first, PREFLIGHT_METADATA_BYTES + 110 + 220);
    }

    #[test]
    fn subsequent_preflight_uses_measured_target_consumption() {
        let physical_size = consumed_capacity(Some(1_000), Some(960)).unwrap();
        assert_eq!(physical_size, 40);
        let copy_bytes = projected_copy_bytes(30, Some(900), Some(physical_size));
        assert_eq!(copy_bytes, 40);
        assert_eq!(
            complete_run_required_capacity([copy_bytes]).unwrap(),
            PREFLIGHT_METADATA_BYTES + 44,
        );
        assert_eq!(projected_copy_bytes(1_000, Some(900), None), 1_000);
        assert_eq!(consumed_capacity(Some(960), Some(1_000)), None);
    }

    #[test]
    fn preflight_accounts_for_growth_since_the_measured_checkpoint() {
        let copy_bytes = projected_copy_bytes(10_000, Some(1_000), Some(100));
        assert_eq!(copy_bytes, 10_000);
        assert_eq!(
            complete_run_required_capacity([copy_bytes]).unwrap(),
            PREFLIGHT_METADATA_BYTES + 11_000
        );
        assert_eq!(projected_copy_bytes(900, Some(1_000), Some(100)), 900);
        assert_eq!(projected_copy_bytes(10_000, None, Some(100)), 10_000);
        assert_eq!(projected_copy_bytes(u64::MAX, Some(1), Some(100)), u64::MAX);
        assert!(complete_run_required_capacity([u64::MAX]).is_err());
    }

    #[tokio::test]
    async fn preflight_counts_current_data_and_the_package_archive_together() {
        let root = tempfile::tempdir().unwrap();
        let volume = root.path().join("volume");
        let archive = root.path().join("service.s9pk");
        tokio::fs::create_dir(&volume).await.unwrap();
        tokio::fs::write(volume.join("data"), vec![0; 800])
            .await
            .unwrap();
        tokio::fs::write(&archive, vec![0; 900]).await.unwrap();
        let current = service_backup_logical_size(&volume, &archive)
            .await
            .unwrap();
        assert_eq!(current, 1_700);
        let copy_bytes = projected_copy_bytes(current, Some(1_000), Some(1_000));
        assert_eq!(copy_bytes, 1_700);
        assert_eq!(
            complete_run_required_capacity([copy_bytes]).unwrap(),
            PREFLIGHT_METADATA_BYTES + 1_870
        );
        tokio::fs::remove_file(volume.join("data")).await.unwrap();
        tokio::fs::remove_dir(&volume).await.unwrap();
        assert_eq!(
            service_backup_logical_size(&volume, &archive)
                .await
                .unwrap(),
            900
        );
        tokio::fs::remove_file(&archive).await.unwrap();
        assert!(
            service_backup_logical_size(&volume, &archive)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn capacity_keeps_system_selection_when_a_selected_service_is_uninstalled() {
        let db = crate::db::model::DatabaseModel::from(imbl_value::json!({
            "public": { "packageData": {} }
        }));
        assert_eq!(
            live_logical_size(&db, &"removed-service".parse().unwrap(), 123)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            live_logical_size(&db, &SYSTEM_PACKAGE_ID, 123)
                .await
                .unwrap(),
            123
        );
    }

    #[tokio::test]
    async fn transient_mount_failure_is_retried() {
        let attempts = AtomicUsize::new(0);
        let result = retry_mount(
            || {
                let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                async move {
                    if attempt == 0 {
                        Err(Error::new(
                            eyre!("transient mount failure"),
                            ErrorKind::Filesystem,
                        ))
                    } else {
                        Ok(())
                    }
                }
            },
            &[Duration::ZERO],
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn persistent_mount_failure_stops_after_bounded_retries() {
        let attempts = AtomicUsize::new(0);
        let result: Result<(), Error> = retry_mount(
            || {
                attempts.fetch_add(1, Ordering::SeqCst);
                async {
                    Err(Error::new(
                        eyre!("persistent mount failure"),
                        ErrorKind::Filesystem,
                    ))
                }
            },
            &[Duration::ZERO, Duration::ZERO],
        )
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }
}
