use std::collections::{BTreeMap, BTreeSet};

use chrono::Utc;
use clap::{Parser, ValueEnum};
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::retention::CAPACITY_MARGIN_PERCENT;
use super::{
    BackupJob, BackupJobId, BackupJobPause, BackupJobStatus, BackupRun, BackupRunTrigger,
    BackupServiceScope, CapacityEstimate, RetentionPolicy, RetentionPolicyChangePreview,
    RetentionTier, Schedule, ScheduledBackupCredential, ScheduledBackupMountGuard,
    ServiceSnapshotId, ServiceTargetHistory, associate_histories, associated_service_ids,
    disassociate_histories, history_key, refresh_archive_state, run_job,
};
use crate::auth::{LoginContext, PasswordType};
use crate::backup::target::BackupTargetId;
use crate::context::RpcContext;
use crate::db::model::DatabaseModel;
use crate::disk::mount::filesystem::ReadWrite;
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::util::serde::HandlerExtSerde;
use crate::{PackageId, SYSTEM_PACKAGE_ID};

pub fn job<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async(list)
                .with_display_serializable()
                .with_about("about.list-automatic-backup-jobs")
                .with_call_remote::<crate::context::CliContext>(),
        )
        .subcommand(
            "add",
            from_fn_async(add_cli)
                .with_display_serializable()
                .with_about("about.add-automatic-backup-job")
                .with_call_remote::<crate::context::CliContext>(),
        )
        .subcommand(
            "edit",
            from_fn_async(edit_cli)
                .with_display_serializable()
                .with_about("about.edit-automatic-backup-job")
                .with_call_remote::<crate::context::CliContext>(),
        )
        .subcommand("create", from_fn_async(create).no_cli())
        .subcommand("update", from_fn_async(update).no_cli())
        .subcommand("validate", from_fn_async(validate).no_cli())
        .subcommand(
            "delete-with-backups",
            from_fn_async(delete_with_backups).no_cli(),
        )
        .subcommand("set-enabled", from_fn_async(set_enabled).no_cli())
        .subcommand("set-enabled-bulk", from_fn_async(set_enabled_bulk).no_cli())
        .subcommand(
            "enable",
            from_fn_async(enable_cli)
                .with_display_serializable()
                .with_about("about.enable-automatic-backup-job")
                .with_call_remote::<crate::context::CliContext>(),
        )
        .subcommand(
            "disable",
            from_fn_async(disable_cli)
                .with_display_serializable()
                .with_about("about.disable-automatic-backup-job")
                .with_call_remote::<crate::context::CliContext>(),
        )
        .subcommand(
            "delete",
            from_fn_async(delete)
                .no_display()
                .with_about("about.delete-automatic-backup-job")
                .with_call_remote::<crate::context::CliContext>(),
        )
        .subcommand(
            "run-now",
            from_fn_async(run_now)
                .with_display_serializable()
                .with_about("about.run-automatic-backup-job-now")
                .with_call_remote::<crate::context::CliContext>(),
        )
        .subcommand(
            "retry-target",
            from_fn_async(retry_target)
                .with_display_serializable()
                .with_about("about.retry-automatic-backup-target")
                .with_call_remote::<crate::context::CliContext>(),
        )
        .subcommand(
            "reassign-target",
            from_fn_async(reassign_target)
                .with_display_serializable()
                .with_about("about.reassign-automatic-backup-target")
                .with_call_remote::<crate::context::CliContext>(),
        )
}

pub fn history<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async(list_histories)
                .with_display_serializable()
                .with_about("about.list-automatic-backup-history")
                .with_call_remote::<crate::context::CliContext>(),
        )
        .subcommand(
            "discover",
            from_fn_async(discover_histories)
                .with_display_serializable()
                .with_about("about.discover-automatic-backup-history")
                .with_call_remote::<crate::context::CliContext>(),
        )
        .subcommand("refresh", from_fn_async(refresh_histories).no_cli())
        .subcommand(
            "delete-archived-snapshots",
            from_fn_async(delete_archived_snapshots).no_cli(),
        )
        .subcommand(
            "delete-archived-snapshots-bulk",
            from_fn_async(delete_archived_snapshots_bulk).no_cli(),
        )
        .subcommand(
            "delete-archived",
            from_fn_async(delete_archived_snapshots_cli)
                .with_display_serializable()
                .with_about("about.delete-archived-backup-checkpoints")
                .with_call_remote::<crate::context::CliContext>(),
        )
}

pub fn policy<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("estimate", from_fn_async(estimate_capacity).no_cli())
        .subcommand("preview", from_fn_async(preview_policy_change).no_cli())
        .subcommand("update", from_fn_async(update_policy).no_cli())
        .subcommand(
            "preview-change",
            from_fn_async(preview_policy_change_cli)
                .with_display_serializable()
                .with_about("about.preview-backup-retention-change")
                .with_call_remote::<crate::context::CliContext>(),
        )
        .subcommand(
            "apply",
            from_fn_async(apply_retention_policy_cli)
                .with_display_serializable()
                .with_about("about.apply-backup-retention-policy")
                .with_call_remote::<crate::context::CliContext>(),
        )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "camelCase")]
pub enum SystemDataSelection {
    Include,
    Exclude,
}

fn service_scope_from_cli(
    package_ids: Vec<PackageId>,
    exclude_package_ids: Vec<PackageId>,
    system_data: Option<SystemDataSelection>,
) -> BackupServiceScope {
    if !package_ids.is_empty() {
        BackupServiceScope::Selected {
            package_ids: package_ids.into_iter().collect(),
            include_system: system_data.map(|value| value == SystemDataSelection::Include),
        }
    } else {
        let mut excluded_package_ids: BTreeSet<_> = exclude_package_ids.into_iter().collect();
        match system_data {
            Some(SystemDataSelection::Include) => {
                excluded_package_ids.remove(&*SYSTEM_PACKAGE_ID);
            }
            Some(SystemDataSelection::Exclude) => {
                excluded_package_ids.insert(SYSTEM_PACKAGE_ID.clone());
            }
            None => {}
        }
        BackupServiceScope::AllExcept {
            excluded_package_ids,
        }
    }
}

fn with_system_data_selection(
    scope: BackupServiceScope,
    system_data: SystemDataSelection,
) -> BackupServiceScope {
    let include_system = system_data == SystemDataSelection::Include;
    match scope {
        BackupServiceScope::All if include_system => BackupServiceScope::All,
        BackupServiceScope::All => BackupServiceScope::AllExcept {
            excluded_package_ids: BTreeSet::from([SYSTEM_PACKAGE_ID.clone()]),
        },
        BackupServiceScope::AllExcept {
            mut excluded_package_ids,
        } => {
            if include_system {
                excluded_package_ids.remove(&*SYSTEM_PACKAGE_ID);
            } else {
                excluded_package_ids.insert(SYSTEM_PACKAGE_ID.clone());
            }
            BackupServiceScope::AllExcept {
                excluded_package_ids,
            }
        }
        BackupServiceScope::Selected { package_ids, .. } => BackupServiceScope::Selected {
            package_ids,
            include_system: Some(include_system),
        },
    }
}

#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct EstimateBackupCapacityParams {
    pub target_id: BackupTargetId,
    pub services: BackupServiceScope,
    pub default_retention: RetentionPolicy,
    pub retention_overrides: BTreeMap<PackageId, RetentionPolicy>,
    /// Preserves established history policies when estimating a new schedule.
    #[serde(default)]
    pub preserve_existing_policies: bool,
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct EstimateBackupCapacityCliParams {
    #[arg(help = "help.arg.backup-target-id")]
    pub target_id: BackupTargetId,
    #[arg(
        long,
        value_delimiter = ',',
        conflicts_with = "exclude_package_ids",
        help = "help.arg.automatic-backup-package-ids"
    )]
    pub package_ids: Vec<PackageId>,
    #[arg(
        long,
        value_delimiter = ',',
        conflicts_with = "package_ids",
        help = "help.arg.automatic-backup-excluded-package-ids"
    )]
    pub exclude_package_ids: Vec<PackageId>,
    #[arg(long, value_enum, help = "help.arg.automatic-backup-system-data")]
    pub system_data: Option<SystemDataSelection>,
    #[arg(
        long = "keep-rule",
        alias = "keep-tier",
        value_name = "INTERVAL:COVERAGE",
        value_parser = parse_retention_tier,
        help = "help.arg.automatic-backup-retention-tier"
    )]
    pub retention_tiers: Vec<RetentionTier>,
    #[arg(
        long = "service-keep-rule",
        alias = "service-keep-tier",
        value_name = "PACKAGE_ID=INTERVAL:COVERAGE",
        value_parser = parse_retention_override_tier,
        help = "help.arg.automatic-backup-service-retention-tier"
    )]
    pub retention_override_tiers: Vec<(PackageId, RetentionTier)>,
    #[arg(
        long = "service-latest-only",
        value_name = "PACKAGE_ID",
        value_parser = parse_backup_item_id,
        value_delimiter = ',',
        help = "help.arg.automatic-backup-service-latest-only"
    )]
    pub latest_only_overrides: Vec<PackageId>,
}

#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct BackupServiceCapacityEstimate {
    pub package_id: PackageId,
    #[ts(type = "number")]
    pub live_logical_bytes: u64,
    pub retained_snapshot_count: usize,
    #[ts(type = "number")]
    pub maximum_projected_snapshot_count: u64,
    #[ts(type = "number")]
    pub scheduled_retained_bytes: u64,
    #[ts(type = "number | null")]
    pub manual_checkpoint_bytes: Option<u64>,
    #[ts(type = "number")]
    pub archived_bytes: u64,
    #[ts(type = "number")]
    pub staging_headroom_bytes: u64,
    #[ts(type = "number | null")]
    pub last_changed_bytes: Option<u64>,
    #[ts(type = "number")]
    pub conservative_peak_excluding_manual_bytes: u64,
}

pub async fn estimate_capacity(
    ctx: RpcContext,
    EstimateBackupCapacityParams {
        target_id,
        services,
        default_retention,
        retention_overrides,
        preserve_existing_policies,
    }: EstimateBackupCapacityParams,
) -> Result<Vec<BackupServiceCapacityEstimate>, Error> {
    validate_retention_policies(&default_retention, &retention_overrides)?;
    let system_logical_bytes = crate::backup::os::system_logical_size(&ctx).await?;
    let db = ctx.db.peek().await;
    let package_ids = selected_installed_services(&db, &services)?;
    let mut estimates = Vec::with_capacity(package_ids.len());
    for package_id in package_ids {
        let live_logical_bytes =
            super::runner::live_logical_size(&db, &package_id, system_logical_bytes).await?;
        let history: Option<ServiceTargetHistory> = db
            .as_public()
            .as_scheduled_backups()
            .as_histories()
            .as_idx(&history_key(&target_id, &package_id))
            .map(|history| history.de())
            .transpose()?;
        let policy = estimated_retention_policy(
            history.as_ref(),
            retention_overrides
                .get(&package_id)
                .unwrap_or(&default_retention),
            preserve_existing_policies,
        )
        .clone();
        let (active, archived): (Vec<_>, Vec<_>) = history
            .into_iter()
            .flat_map(|history| history.snapshots)
            .partition(|snapshot| !snapshot.archived);
        let archived_bytes = archived
            .iter()
            .map(|snapshot| snapshot.physical_size.unwrap_or(snapshot.logical_size))
            .sum::<u64>();
        let estimate = CapacityEstimate::calculate(
            &policy,
            &active,
            0,
            archived_bytes,
            live_logical_bytes,
            CAPACITY_MARGIN_PERCENT,
        )?;
        estimates.push(BackupServiceCapacityEstimate {
            package_id,
            live_logical_bytes,
            retained_snapshot_count: estimate.retained_snapshot_count,
            maximum_projected_snapshot_count: estimate.maximum_projected_snapshot_count,
            scheduled_retained_bytes: estimate.scheduled_retained_bytes,
            manual_checkpoint_bytes: None,
            archived_bytes: estimate.archived_bytes,
            staging_headroom_bytes: estimate.staging_headroom_bytes,
            last_changed_bytes: estimate.last_changed_bytes,
            conservative_peak_excluding_manual_bytes: estimate.conservative_peak_bytes,
        });
    }
    Ok(estimates)
}

fn estimated_retention_policy<'a>(
    history: Option<&'a ServiceTargetHistory>,
    proposed: &'a RetentionPolicy,
    preserve_existing: bool,
) -> &'a RetentionPolicy {
    history
        .filter(|history| {
            preserve_existing && super::association::history_owns_retention_settings(history)
        })
        .map(|history| &history.policy)
        .unwrap_or(proposed)
}

pub async fn estimate_capacity_cli(
    ctx: RpcContext,
    EstimateBackupCapacityCliParams {
        target_id,
        package_ids,
        exclude_package_ids,
        system_data,
        retention_tiers,
        retention_override_tiers,
        latest_only_overrides,
    }: EstimateBackupCapacityCliParams,
) -> Result<Vec<BackupServiceCapacityEstimate>, Error> {
    let services = service_scope_from_cli(package_ids, exclude_package_ids, system_data);
    estimate_capacity(
        ctx,
        EstimateBackupCapacityParams {
            target_id,
            services,
            default_retention: RetentionPolicy {
                tiers: retention_tiers,
            },
            retention_overrides: retention_overrides_from_cli(
                retention_override_tiers,
                latest_only_overrides,
            )?,
            preserve_existing_policies: false,
        },
    )
    .await
}

pub async fn list(ctx: RpcContext) -> Result<Vec<BackupJob>, Error> {
    Ok(ctx
        .db
        .peek()
        .await
        .as_public()
        .as_scheduled_backups()
        .as_jobs()
        .as_entries()?
        .into_iter()
        .map(|(_, job)| job.de())
        .collect::<Result<_, _>>()?)
}

pub async fn list_histories(ctx: RpcContext) -> Result<Vec<ServiceTargetHistory>, Error> {
    ctx.db
        .peek()
        .await
        .as_public()
        .as_scheduled_backups()
        .as_histories()
        .as_entries()?
        .into_iter()
        .map(|(_, history)| history.de())
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RefreshScheduledBackupHistoriesParams {
    target_id: BackupTargetId,
}

async fn refresh_histories(
    ctx: RpcContext,
    RefreshScheduledBackupHistoriesParams { target_id }: RefreshScheduledBackupHistoriesParams,
) -> Result<Vec<ServiceTargetHistory>, Error> {
    let _coordinator = crate::backup::try_backup_coordinator(ctx.backup_coordinator.clone())?;
    let db = ctx.db.peek().await;
    let credential: ScheduledBackupCredential = db
        .as_private()
        .as_scheduled_backup_credentials()
        .as_idx(&target_id.to_string())
        .or_not_found(target_id.to_string())?
        .de()?;
    validate_target_identity(&db, &target_id, &credential.target_instance_id)?;
    let encryption_key =
        credential.open(&db.as_private().as_scheduled_backup_device_key().de()?)?;
    let server_id = db.as_public().as_server_info().as_id().de()?;
    let target = target_id.clone().load(&db)?;
    let mut guard = ScheduledBackupMountGuard::mount_with_key(
        TmpMountGuard::mount(&target, ReadWrite).await?,
        &server_id,
        &credential.target_instance_id,
        &encryption_key,
    )
    .await?;
    reconcile_target_histories(&db, &target_id, &mut guard)?;
    drop(db);
    let target_instance_id = guard.recovery.target_instance_id.clone();
    let remote: BTreeMap<PackageId, ServiceTargetHistory> = guard
        .metadata
        .services
        .iter()
        .map(|(package_id, history)| {
            (
                package_id.clone(),
                service_target_history(
                    &target_id,
                    &target_instance_id,
                    package_id,
                    history,
                    BTreeSet::new(),
                ),
            )
        })
        .collect();
    guard.save_and_unmount().await?;

    let db = ctx.db.peek().await;
    let jobs = db
        .as_public()
        .as_scheduled_backups()
        .as_jobs()
        .as_entries()?
        .into_iter()
        .map(|(_, job)| job.de())
        .collect::<Result<Vec<BackupJob>, Error>>()?;
    let mut merged: BTreeMap<PackageId, ServiceTargetHistory> = db
        .as_public()
        .as_scheduled_backups()
        .as_histories()
        .as_entries()?
        .into_iter()
        .map(|(_, history)| history.de())
        .collect::<Result<Vec<ServiceTargetHistory>, Error>>()?
        .into_iter()
        .filter(|history| history.target_id == target_id)
        .map(|history| (history.package_id.clone(), history))
        .collect();
    for history in merged.values_mut() {
        if let Some(remote_history) = remote.get(&history.package_id) {
            *history = remote_history.clone();
        } else {
            history.snapshots.clear();
        }
    }
    for (package_id, history) in remote {
        merged.entry(package_id).or_insert(history);
    }
    for history in merged.values_mut() {
        history.feeding_jobs = current_feeding_jobs(
            &jobs,
            &target_id,
            &history.target_instance_id,
            &history.package_id,
        );
    }
    drop(db);
    let histories: Vec<_> = merged.into_values().collect();
    persist_histories(&ctx, &histories, None).await?;
    Ok(histories)
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct DiscoverScheduledBackupsParams {
    #[arg(help = "help.arg.backup-target-id")]
    pub target_id: BackupTargetId,
    #[arg(help = "help.arg.server-id")]
    pub server_id: String,
    #[arg(help = "help.arg.backup-password")]
    pub password: String,
}

pub async fn discover_histories(
    ctx: RpcContext,
    DiscoverScheduledBackupsParams {
        target_id,
        server_id,
        password,
    }: DiscoverScheduledBackupsParams,
) -> Result<Vec<ServiceTargetHistory>, Error> {
    let _coordinator = crate::backup::try_backup_coordinator(ctx.backup_coordinator.clone())?;
    let db = ctx.db.peek().await;
    let current_server_id = db.as_public().as_server_info().as_id().de()?;
    let local_server = server_id == current_server_id;
    let jobs = if local_server {
        db.as_public()
            .as_scheduled_backups()
            .as_jobs()
            .as_entries()?
            .into_iter()
            .map(|(_, job)| job.de())
            .collect::<Result<Vec<BackupJob>, Error>>()?
    } else {
        Vec::new()
    };
    let device_key = local_server
        .then(|| db.as_private().as_scheduled_backup_device_key().de())
        .transpose()?;
    let target = target_id.clone().load(&db)?;
    drop(db);
    let (mut guard, encryption_key) = ScheduledBackupMountGuard::discover_with_password(
        TmpMountGuard::mount(&target, ReadWrite).await?,
        &server_id,
        &password,
    )
    .await?;
    let target_instance_id = guard.recovery.target_instance_id.clone();
    if local_server {
        let db = ctx.db.peek().await;
        validate_target_identity(&db, &target_id, &target_instance_id)?;
        reconcile_target_histories(&db, &target_id, &mut guard)?;
    }
    let credential = device_key
        .map(|device_key| {
            ScheduledBackupCredential::seal(
                target_instance_id.clone(),
                &encryption_key,
                &device_key,
            )
        })
        .transpose()?;
    let histories: Vec<_> = guard
        .metadata
        .services
        .iter()
        .map(|(package_id, history)| {
            let feeding_jobs =
                current_feeding_jobs(&jobs, &target_id, &target_instance_id, package_id);
            service_target_history(
                &target_id,
                &target_instance_id,
                package_id,
                history,
                feeding_jobs,
            )
        })
        .collect();
    if local_server {
        guard.save_and_unmount().await?;
    } else {
        guard.unmount().await?;
    }
    if local_server {
        persist_histories(
            &ctx,
            &histories,
            credential
                .as_ref()
                .map(|credential| (&target_id, credential)),
        )
        .await?;
    }
    Ok(histories)
}

async fn persist_histories(
    ctx: &RpcContext,
    histories: &[ServiceTargetHistory],
    credential: Option<(&BackupTargetId, &ScheduledBackupCredential)>,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            if let Some((target_id, credential)) = credential {
                db.as_private_mut()
                    .as_scheduled_backup_credentials_mut()
                    .insert(&target_id.to_string(), credential)?;
            }
            let state = db.as_public_mut().as_scheduled_backups_mut();
            for history in histories {
                state.as_histories_mut().insert(
                    &history_key(&history.target_id, &history.package_id),
                    history,
                )?;
            }
            Ok(())
        })
        .await
        .result
}

fn current_feeding_jobs(
    jobs: &[BackupJob],
    target_id: &BackupTargetId,
    target_instance_id: &str,
    package_id: &PackageId,
) -> BTreeSet<BackupJobId> {
    jobs.iter()
        .filter(|job| {
            job.target_id == *target_id
                && job.target_instance_id == target_instance_id
                && job.services.includes(package_id)
        })
        .map(|job| job.id.clone())
        .collect()
}

fn service_target_history(
    target_id: &BackupTargetId,
    target_instance_id: &str,
    package_id: &PackageId,
    history: &super::OnTargetServiceHistory,
    feeding_jobs: BTreeSet<BackupJobId>,
) -> ServiceTargetHistory {
    ServiceTargetHistory {
        target_id: target_id.clone(),
        target_instance_id: target_instance_id.to_owned(),
        package_id: package_id.clone(),
        timezone: history.timezone.clone(),
        policy: history.policy.clone(),
        feeding_jobs,
        snapshots: history.snapshots.clone(),
        archived: history.archived,
    }
}

fn validate_target_identity(
    db: &DatabaseModel,
    target_id: &BackupTargetId,
    target_instance_id: &str,
) -> Result<(), Error> {
    let jobs = db
        .as_public()
        .as_scheduled_backups()
        .as_jobs()
        .as_entries()?
        .into_iter()
        .map(|(_, job)| job.de())
        .collect::<Result<Vec<BackupJob>, Error>>()?;
    let histories = db
        .as_public()
        .as_scheduled_backups()
        .as_histories()
        .as_entries()?
        .into_iter()
        .map(|(_, history)| history.de())
        .collect::<Result<Vec<ServiceTargetHistory>, Error>>()?;
    if jobs
        .iter()
        .any(|job| job.target_id == *target_id && job.target_instance_id != target_instance_id)
        || histories.iter().any(|history| {
            history.target_id == *target_id
                && history.target_instance_id != target_instance_id
                && super::association::history_owns_retention_settings(history)
        })
    {
        return Err(Error::new(
            eyre!(
                "{}",
                t!("backup.scheduled.target-location-identity-mismatch")
            ),
            ErrorKind::InvalidRequest,
        ));
    }
    let aliases = jobs
        .into_iter()
        .filter(|job| job.target_instance_id == target_instance_id && job.target_id != *target_id)
        .map(|job| job.name)
        .collect::<Vec<_>>();
    if !aliases.is_empty() {
        return Err(Error::new(
            eyre!(
                "{}",
                t!(
                    "backup.scheduled.target-already-configured",
                    jobs = aliases.join(", ")
                )
            ),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

fn import_target_histories(
    db: &mut DatabaseModel,
    target_id: &BackupTargetId,
    metadata: &super::ScheduledBackupOnTargetMetadata,
) -> Result<(), Error> {
    let histories = db
        .as_public_mut()
        .as_scheduled_backups_mut()
        .as_histories_mut();
    for (package_id, history) in &metadata.services {
        let key = history_key(target_id, package_id);
        let cached: Option<ServiceTargetHistory> = histories
            .as_idx(&key)
            .map(|history| history.de())
            .transpose()?;
        if cached.as_ref().is_some_and(|history| {
            !history.snapshots.is_empty() || !history.feeding_jobs.is_empty()
        }) {
            continue;
        }
        histories.insert(
            &key,
            &service_target_history(
                target_id,
                &metadata.target_instance_id,
                package_id,
                history,
                BTreeSet::new(),
            ),
        )?;
    }
    Ok(())
}

#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DeleteArchivedSnapshotsParams {
    pub target_id: BackupTargetId,
    pub package_id: PackageId,
    pub snapshot_ids: BTreeSet<ServiceSnapshotId>,
    pub password: String,
    /// Password that encrypted the existing backup location.
    #[serde(default)]
    #[ts(optional)]
    pub old_password: Option<String>,
}

#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedSnapshotSelection {
    pub package_id: PackageId,
    pub snapshot_ids: BTreeSet<ServiceSnapshotId>,
}

#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DeleteArchivedSnapshotsBulkParams {
    pub target_id: BackupTargetId,
    pub snapshots: Vec<ArchivedSnapshotSelection>,
    pub password: String,
    /// Password that encrypted the existing backup location.
    #[serde(default)]
    #[ts(optional)]
    pub old_password: Option<String>,
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct DeleteArchivedSnapshotsCliParams {
    #[arg(help = "help.arg.backup-target-id")]
    pub target_id: BackupTargetId,
    #[arg(value_parser = parse_backup_item_id, help = "help.arg.backup-item-id")]
    pub package_id: PackageId,
    #[arg(required = true, help = "help.arg.automatic-backup-snapshot-ids")]
    pub snapshot_ids: Vec<ServiceSnapshotId>,
    #[arg(long, help = "help.arg.backup-password")]
    pub password: String,
    #[arg(long, help = "help.arg.old-backup-password")]
    #[serde(default)]
    pub old_password: Option<String>,
}

pub async fn delete_archived_snapshots_cli(
    ctx: RpcContext,
    DeleteArchivedSnapshotsCliParams {
        target_id,
        package_id,
        snapshot_ids,
        password,
        old_password,
    }: DeleteArchivedSnapshotsCliParams,
) -> Result<ServiceTargetHistory, Error> {
    delete_archived_snapshots(
        ctx,
        DeleteArchivedSnapshotsParams {
            target_id,
            package_id,
            snapshot_ids: snapshot_ids.into_iter().collect(),
            password,
            old_password,
        },
    )
    .await
}

pub async fn delete_archived_snapshots(
    ctx: RpcContext,
    DeleteArchivedSnapshotsParams {
        target_id,
        package_id,
        snapshot_ids,
        password,
        old_password,
    }: DeleteArchivedSnapshotsParams,
) -> Result<ServiceTargetHistory, Error> {
    let mut histories = delete_archived_snapshots_bulk(
        ctx,
        DeleteArchivedSnapshotsBulkParams {
            target_id,
            snapshots: vec![ArchivedSnapshotSelection {
                package_id,
                snapshot_ids,
            }],
            password,
            old_password,
        },
    )
    .await?;
    Ok(histories
        .pop()
        .expect("one service selection produces one history"))
}

/// Deletes archived checkpoints for multiple services with one target mount.
pub async fn delete_archived_snapshots_bulk(
    ctx: RpcContext,
    DeleteArchivedSnapshotsBulkParams {
        target_id,
        snapshots,
        password,
        old_password,
    }: DeleteArchivedSnapshotsBulkParams,
) -> Result<Vec<ServiceTargetHistory>, Error> {
    if snapshots.is_empty() {
        return Ok(Vec::new());
    }
    let _coordinator = crate::backup::try_backup_coordinator(ctx.backup_coordinator.clone())?;

    let db = ctx.db.peek().await;
    RpcContext::check_password(&db, &password)?;
    let mut requested = BTreeMap::<PackageId, BTreeSet<ServiceSnapshotId>>::new();
    for selection in snapshots {
        requested
            .entry(selection.package_id)
            .or_default()
            .extend(selection.snapshot_ids);
    }
    let mut histories = Vec::with_capacity(requested.len());
    for (package_id, snapshot_ids) in &requested {
        let key = history_key(&target_id, package_id);
        let history: ServiceTargetHistory = db
            .as_public()
            .as_scheduled_backups()
            .as_histories()
            .as_idx(&key)
            .or_not_found(&key)?
            .de()?;
        validate_archived_snapshot_deletion(&history, snapshot_ids)?;
        histories.push(history);
    }
    let expected_target_instance_id = one_target_instance_id(
        histories
            .iter()
            .map(|history| history.target_instance_id.clone()),
    )?;
    let server_id = db.as_public().as_server_info().as_id().de()?;
    let (mut guard, credential) = mount_scheduled_target(
        &db,
        &target_id,
        &server_id,
        &expected_target_instance_id,
        Some(old_password.as_deref().unwrap_or(&password)),
    )
    .await
    .map_err(backup_password_mismatch)?;
    let deletion = guard.delete_archived_snapshots_bulk(&requested).await;
    finish_history_change(
        &ctx,
        guard,
        histories,
        Some((&target_id, &credential)),
        deletion,
    )
    .await
}

async fn finish_history_change(
    ctx: &RpcContext,
    mut guard: ScheduledBackupMountGuard<TmpMountGuard>,
    mut histories: Vec<ServiceTargetHistory>,
    credential: Option<(&BackupTargetId, &ScheduledBackupCredential)>,
    change: Result<(), Error>,
) -> Result<Vec<ServiceTargetHistory>, Error> {
    if change.is_err() {
        if let Err(error) = guard.reload_metadata().await {
            guard.unmount().await.log_err();
            change?;
            return Err(error);
        }
    }
    for history in &mut histories {
        guard.metadata.refresh_history(history);
    }
    let unmount = guard.unmount().await;
    persist_histories(ctx, &histories, credential).await?;
    change?;
    unmount?;
    Ok(histories)
}

fn validate_archived_snapshot_deletion(
    history: &ServiceTargetHistory,
    snapshot_ids: &BTreeSet<ServiceSnapshotId>,
) -> Result<(), Error> {
    let archived_ids: BTreeSet<_> = history
        .snapshots
        .iter()
        .filter(|snapshot| snapshot.archived)
        .map(|snapshot| snapshot.id.clone())
        .collect();
    if history.feeding_jobs.is_empty() && snapshot_ids.is_subset(&archived_ids) {
        return Ok(());
    }
    Err(Error::new(
        eyre!("{}", t!("backup.scheduled.delete-active-history")),
        ErrorKind::InvalidRequest,
    ))
}

fn backup_password_mismatch(error: Error) -> Error {
    if error.kind == ErrorKind::IncorrectPassword {
        Error::new(
            eyre!("{}", t!("backup.bulk.password-mismatch")),
            ErrorKind::BackupPasswordMismatch,
        )
    } else {
        error
    }
}

pub(crate) async fn mount_scheduled_target(
    db: &DatabaseModel,
    target_id: &BackupTargetId,
    server_id: &str,
    expected_target_instance_id: &str,
    password: Option<&str>,
) -> Result<
    (
        ScheduledBackupMountGuard<TmpMountGuard>,
        ScheduledBackupCredential,
    ),
    Error,
> {
    validate_target_identity(db, target_id, expected_target_instance_id)?;
    let target = target_id.clone().load(db)?;
    let device_key = db.as_private().as_scheduled_backup_device_key().de()?;
    let credential = db
        .as_private()
        .as_scheduled_backup_credentials()
        .as_idx(&target_id.to_string())
        .map(|credential| credential.de())
        .transpose()?;

    if let Some(credential) = credential {
        if credential.target_instance_id == expected_target_instance_id {
            if let Ok(encryption_key) = credential.open(&device_key) {
                let mut guard = ScheduledBackupMountGuard::mount_with_key(
                    TmpMountGuard::mount(&target, ReadWrite).await?,
                    server_id,
                    expected_target_instance_id,
                    &encryption_key,
                )
                .await?;
                reconcile_target_histories(db, target_id, &mut guard)?;
                return Ok((guard, credential));
            }
        }
    }

    let password = password.ok_or_else(|| {
        Error::new(
            eyre!("{}", t!("backup.scheduled.reauth-required")),
            ErrorKind::InvalidRequest,
        )
    })?;
    let (mut guard, encryption_key) = ScheduledBackupMountGuard::mount_with_password(
        TmpMountGuard::mount(&target, ReadWrite).await?,
        server_id,
        expected_target_instance_id,
        password,
    )
    .await?;
    let credential = ScheduledBackupCredential::seal(
        expected_target_instance_id.to_owned(),
        &encryption_key,
        &device_key,
    )?;
    reconcile_target_histories(db, target_id, &mut guard)?;
    Ok((guard, credential))
}

fn one_target_instance_id(
    target_instance_ids: impl IntoIterator<Item = String>,
) -> Result<String, Error> {
    let target_instance_ids: BTreeSet<_> = target_instance_ids.into_iter().collect();
    if target_instance_ids.len() != 1 {
        return Err(Error::new(
            eyre!("{}", t!("backup.scheduled.target-identity-mismatch")),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(target_instance_ids
        .first()
        .expect("one target instance ID exists")
        .clone())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct RetryBackupTargetParams {
    #[arg(help = "help.arg.backup-target-id")]
    pub target_id: BackupTargetId,
    #[arg(help = "help.arg.backup-password")]
    pub password: PasswordType,
    #[arg(long, help = "help.arg.old-backup-password")]
    #[serde(default)]
    #[ts(optional)]
    pub old_password: Option<PasswordType>,
}

/// Reconnects a failed automatic backup target and resumes affected jobs.
pub async fn retry_target(
    ctx: RpcContext,
    RetryBackupTargetParams {
        target_id,
        password,
        old_password,
    }: RetryBackupTargetParams,
) -> Result<Vec<BackupJob>, Error> {
    let password = password.decrypt(&ctx)?;
    let old_password = old_password
        .map(|password| password.decrypt(&ctx))
        .transpose()?;
    let coordinator = crate::backup::try_backup_coordinator(ctx.backup_coordinator.clone())?;
    let db = ctx.db.peek().await;
    RpcContext::check_password(&db, &password)?;
    let target_instance_id = target_instance_id_for_target(&db, &target_id)?;
    let server_id = db.as_public().as_server_info().as_id().de()?;
    let (guard, credential) = mount_scheduled_target(
        &db,
        &target_id,
        &server_id,
        &target_instance_id,
        Some(old_password.as_deref().unwrap_or(&password)),
    )
    .await
    .map_err(backup_password_mismatch)?;
    guard.save_and_unmount().await?;

    let jobs = ctx
        .db
        .mutate(|db| {
            db.as_private_mut()
                .as_scheduled_backup_credentials_mut()
                .insert(&target_id.to_string(), &credential)?;

            let state = db.as_public_mut().as_scheduled_backups_mut();
            let jobs: Vec<BackupJob> = state
                .as_jobs()
                .as_entries()?
                .into_iter()
                .map(|(_, job)| job.de())
                .collect::<Result<Vec<BackupJob>, Error>>()?;
            let mut resumed = Vec::new();
            for mut job in jobs.into_iter().filter(|job| job.target_id == target_id) {
                if job
                    .pause
                    .as_ref()
                    .is_some_and(BackupJobPause::requires_target_retry)
                {
                    job.pause = None;
                    job.status.consecutive_failures = 0;
                    let now = Utc::now();
                    job.updated_at = now;
                    reschedule_job(&mut job, now)?;
                    state.as_jobs_mut().insert(&job.id, &job)?;
                }
                resumed.push(job);
            }
            let mut failure = state
                .as_target_failures()
                .as_idx(&target_id.to_string())
                .map(|state| state.de())
                .transpose()?
                .unwrap_or_default();
            failure.reset();
            state
                .as_target_failures_mut()
                .insert(&target_id.to_string(), &failure)?;
            Ok(resumed)
        })
        .await
        .result?;
    sync_archive_states(&ctx, &target_id, &coordinator)
        .await
        .log_err();
    Ok(jobs)
}

fn target_instance_id_for_target(
    db: &DatabaseModel,
    target_id: &BackupTargetId,
) -> Result<String, Error> {
    let job_instance_ids = db
        .as_public()
        .as_scheduled_backups()
        .as_jobs()
        .as_entries()?
        .into_iter()
        .map(|(_, job)| job.de())
        .collect::<Result<Vec<BackupJob>, Error>>()?
        .into_iter()
        .filter(|job| job.target_id == *target_id)
        .map(|job| job.target_instance_id)
        .collect::<BTreeSet<_>>();
    if !job_instance_ids.is_empty() {
        return one_target_instance_id(job_instance_ids);
    }

    let history_instance_ids = db
        .as_public()
        .as_scheduled_backups()
        .as_histories()
        .as_entries()?
        .into_iter()
        .map(|(_, history)| history.de())
        .collect::<Result<Vec<ServiceTargetHistory>, Error>>()?
        .into_iter()
        .filter(|history| history.target_id == *target_id && !history.snapshots.is_empty())
        .map(|history| history.target_instance_id)
        .collect::<BTreeSet<_>>();
    if !history_instance_ids.is_empty() {
        return one_target_instance_id(history_instance_ids);
    }

    let credential: ScheduledBackupCredential = db
        .as_private()
        .as_scheduled_backup_credentials()
        .as_idx(&target_id.to_string())
        .or_not_found(target_id.to_string())?
        .de()?;
    Ok(credential.target_instance_id)
}

fn update_reassigned_job(
    job: &mut BackupJob,
    target_id: BackupTargetId,
    target_instance_id: String,
    wait_for_schedule: bool,
    now: chrono::DateTime<Utc>,
) -> Result<(), Error> {
    job.target_id = target_id;
    job.target_instance_id = target_instance_id;
    job.pause = None;
    job.updated_at = now;
    job.status.consecutive_failures = 0;
    job.status.run_requested = job.enabled && !wait_for_schedule;
    reschedule_job(job, now)
}

fn reschedule_job(job: &mut BackupJob, now: chrono::DateTime<Utc>) -> Result<(), Error> {
    job.status.next_run_at = (job.enabled && job.pause.is_none())
        .then(|| {
            job.schedule
                .next_after_cursor(now, job.status.last_scheduled_at)
        })
        .transpose()?
        .map(|occurrence| occurrence.utc);
    Ok(())
}

fn update_job_schedule(
    job: &mut BackupJob,
    schedule: Schedule,
    now: chrono::DateTime<Utc>,
) -> Result<(), Error> {
    if job.schedule != schedule {
        job.status.last_scheduled_at = None;
    }
    job.schedule = schedule;
    reschedule_job(job, now)
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ReassignBackupTargetParams {
    #[arg(help = "help.arg.automatic-backup-job-id")]
    pub id: BackupJobId,
    #[arg(help = "help.arg.backup-target-id")]
    pub target_id: BackupTargetId,
    #[arg(help = "help.arg.backup-password")]
    pub password: PasswordType,
    #[arg(long, help = "help.arg.old-backup-password")]
    #[serde(default)]
    #[ts(optional)]
    pub old_password: Option<PasswordType>,
    #[arg(long, help = "help.arg.automatic-backup-wait-for-schedule")]
    #[serde(default)]
    pub wait_for_schedule: bool,
}

pub async fn reassign_target(
    ctx: RpcContext,
    ReassignBackupTargetParams {
        id,
        target_id,
        password,
        old_password,
        wait_for_schedule,
    }: ReassignBackupTargetParams,
) -> Result<BackupJob, Error> {
    let password = password.decrypt(&ctx)?;
    let old_password = old_password
        .map(|password| password.decrypt(&ctx))
        .transpose()?;
    let coordinator = crate::backup::try_backup_coordinator(ctx.backup_coordinator.clone())?;
    let db = ctx.db.peek().await;
    RpcContext::check_password(&db, &password)?;
    let mut job: BackupJob = db
        .as_public()
        .as_scheduled_backups()
        .as_jobs()
        .as_idx(&id)
        .or_not_found(&id)?
        .de()?;
    validate_unique_job_name(&db, &job.name, Some(&id))?;
    let package_ids = selected_installed_services(&db, &job.services)?;
    let server_id = db.as_public().as_server_info().as_id().de()?;
    let hostname = ctx.account.peek(|account| account.hostname.clone());
    let target_guard = TmpMountGuard::mount(&target_id.clone().load(&db)?, ReadWrite).await?;
    let available = crate::disk::util::get_available(target_guard.path()).await?;
    super::runner::preflight_new_target_capacity(&ctx, &package_ids, available).await?;
    let (mut guard, encryption_key) = ScheduledBackupMountGuard::initialize(
        target_guard,
        &server_id,
        hostname,
        &password,
        old_password.as_deref(),
    )
    .await
    .map_err(backup_password_mismatch)?;
    let target_instance_id = guard.recovery.target_instance_id.clone();
    validate_target_identity(&db, &target_id, &target_instance_id)?;
    reconcile_target_histories(&db, &target_id, &mut guard)?;
    let target_metadata = guard.metadata.clone();
    guard.save_and_unmount().await?;

    let old_job = job.clone();
    update_reassigned_job(
        &mut job,
        target_id.clone(),
        target_instance_id.clone(),
        wait_for_schedule,
        Utc::now(),
    )?;
    ctx.db
        .mutate(|db| {
            validate_unique_job_name(db, &job.name, Some(&id))?;
            let old_package_ids = associated_service_ids(db, &old_job)?;
            let device_key = db.as_private().as_scheduled_backup_device_key().de()?;
            let credential =
                ScheduledBackupCredential::seal(target_instance_id, &encryption_key, &device_key)?;
            db.as_private_mut()
                .as_scheduled_backup_credentials_mut()
                .insert(&target_id.to_string(), &credential)?;
            disassociate_histories(db, &old_job, &old_package_ids)?;
            import_target_histories(db, &target_id, &target_metadata)?;
            associate_histories(db, &job, &package_ids)?;
            db.as_public_mut()
                .as_scheduled_backups_mut()
                .as_jobs_mut()
                .insert(&id, &job)?;
            refresh_archive_state(db, &old_job.target_id)?;
            refresh_archive_state(db, &target_id)?;
            Ok(())
        })
        .await
        .result?;
    sync_archive_states(&ctx, &old_job.target_id, &coordinator)
        .await
        .log_err();
    sync_archive_states(&ctx, &job.target_id, &coordinator)
        .await
        .log_err();
    Ok(job)
}

#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRetentionPolicyParams {
    pub target_id: BackupTargetId,
    pub package_id: PackageId,
    pub policy: RetentionPolicy,
}

pub async fn preview_policy_change(
    ctx: RpcContext,
    params: PreviewRetentionPolicyParams,
) -> Result<RetentionPolicyChangePreview, Error> {
    let db = ctx.db.peek().await;
    policy_preview(&db, &params)
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct PreviewRetentionPolicyCliParams {
    #[arg(help = "help.arg.backup-target-id")]
    pub target_id: BackupTargetId,
    #[arg(value_parser = parse_backup_item_id, help = "help.arg.backup-item-id")]
    pub package_id: PackageId,
    #[arg(
        long = "keep-rule",
        alias = "keep-tier",
        value_name = "INTERVAL:COVERAGE",
        value_parser = parse_retention_tier,
        required_unless_present = "latest_only",
        conflicts_with = "latest_only",
        help = "help.arg.automatic-backup-retention-tier"
    )]
    pub retention_tiers: Vec<RetentionTier>,
    #[arg(
        long,
        required_unless_present = "retention_tiers",
        help = "help.arg.automatic-backup-latest-only"
    )]
    pub latest_only: bool,
}

pub async fn preview_policy_change_cli(
    ctx: RpcContext,
    PreviewRetentionPolicyCliParams {
        target_id,
        package_id,
        retention_tiers,
        latest_only,
    }: PreviewRetentionPolicyCliParams,
) -> Result<RetentionPolicyChangePreview, Error> {
    preview_policy_change(
        ctx,
        PreviewRetentionPolicyParams {
            target_id,
            package_id,
            policy: retention_policy_from_cli(retention_tiers, latest_only)?,
        },
    )
    .await
}

#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRetentionPolicyParams {
    pub target_id: BackupTargetId,
    pub package_id: PackageId,
    pub policy: RetentionPolicy,
    pub confirmed_removals: BTreeSet<ServiceSnapshotId>,
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ApplyRetentionPolicyCliParams {
    #[arg(help = "help.arg.backup-target-id")]
    pub target_id: BackupTargetId,
    #[arg(value_parser = parse_backup_item_id, help = "help.arg.backup-item-id")]
    pub package_id: PackageId,
    #[arg(
        long = "keep-rule",
        alias = "keep-tier",
        value_name = "INTERVAL:COVERAGE",
        value_parser = parse_retention_tier,
        required_unless_present = "latest_only",
        conflicts_with = "latest_only",
        help = "help.arg.automatic-backup-retention-tier"
    )]
    pub retention_tiers: Vec<RetentionTier>,
    #[arg(
        long,
        required_unless_present = "retention_tiers",
        help = "help.arg.automatic-backup-latest-only"
    )]
    pub latest_only: bool,
    #[arg(
        long = "confirm-removal",
        value_name = "CHECKPOINT_ID",
        help = "help.arg.automatic-backup-confirm-removal"
    )]
    pub confirmed_removals: Vec<ServiceSnapshotId>,
}

pub async fn apply_retention_policy_cli(
    ctx: RpcContext,
    ApplyRetentionPolicyCliParams {
        target_id,
        package_id,
        retention_tiers,
        latest_only,
        confirmed_removals,
    }: ApplyRetentionPolicyCliParams,
) -> Result<ServiceTargetHistory, Error> {
    update_policy(
        ctx,
        UpdateRetentionPolicyParams {
            target_id,
            package_id,
            policy: retention_policy_from_cli(retention_tiers, latest_only)?,
            confirmed_removals: confirmed_removals.into_iter().collect(),
        },
    )
    .await
}

fn retention_policy_from_cli(
    retention_tiers: Vec<RetentionTier>,
    latest_only: bool,
) -> Result<RetentionPolicy, Error> {
    if latest_only != retention_tiers.is_empty() {
        return Err(Error::new(
            eyre!("{}", t!("backup.scheduled.invalid-retention-tiers")),
            ErrorKind::InvalidRequest,
        ));
    }
    let policy = RetentionPolicy {
        tiers: retention_tiers,
    };
    policy.validate()?;
    Ok(policy)
}

pub async fn update_policy(
    ctx: RpcContext,
    UpdateRetentionPolicyParams {
        target_id,
        package_id,
        policy,
        confirmed_removals,
    }: UpdateRetentionPolicyParams,
) -> Result<ServiceTargetHistory, Error> {
    policy.validate()?;
    let _coordinator = crate::backup::try_backup_coordinator(ctx.backup_coordinator.clone())?;
    let db = ctx.db.peek().await;
    let preview = policy_preview(
        &db,
        &PreviewRetentionPolicyParams {
            target_id: target_id.clone(),
            package_id: package_id.clone(),
            policy: policy.clone(),
        },
    )?;
    let exact_removals: BTreeSet<_> = preview.removed.iter().map(|s| s.id.clone()).collect();
    if exact_removals != confirmed_removals {
        return Err(Error::new(
            eyre!("{}", t!("backup.scheduled.prune-confirmation-stale")),
            ErrorKind::InvalidRequest,
        ));
    }
    let key = history_key(&target_id, &package_id);
    let mut history: ServiceTargetHistory = db
        .as_public()
        .as_scheduled_backups()
        .as_histories()
        .as_idx(&key)
        .or_not_found(&key)?
        .de()?;
    let credential: ScheduledBackupCredential = db
        .as_private()
        .as_scheduled_backup_credentials()
        .as_idx(&target_id.to_string())
        .or_not_found(target_id.to_string())?
        .de()?;
    validate_target_identity(&db, &target_id, &credential.target_instance_id)?;
    let encryption_key =
        credential.open(&db.as_private().as_scheduled_backup_device_key().de()?)?;
    let server_id = db.as_public().as_server_info().as_id().de()?;
    let mut guard = ScheduledBackupMountGuard::mount_with_key(
        TmpMountGuard::mount(&target_id.clone().load(&db)?, ReadWrite).await?,
        &server_id,
        &credential.target_instance_id,
        &encryption_key,
    )
    .await?;
    reconcile_target_histories(&db, &target_id, &mut guard)?;
    history.snapshots = guard
        .metadata
        .services
        .get(&package_id)
        .or_not_found(&package_id)?
        .snapshots
        .clone();
    persist_histories(&ctx, std::slice::from_ref(&history), None).await?;
    let change = guard
        .apply_policy(
            &package_id,
            history.timezone.clone(),
            policy,
            &confirmed_removals,
        )
        .await;
    let mut histories = finish_history_change(&ctx, guard, vec![history], None, change).await?;
    Ok(histories.pop().expect("one service history"))
}

fn policy_preview(
    db: &DatabaseModel,
    params: &PreviewRetentionPolicyParams,
) -> Result<RetentionPolicyChangePreview, Error> {
    params.policy.validate()?;
    let key = history_key(&params.target_id, &params.package_id);
    let history: ServiceTargetHistory = db
        .as_public()
        .as_scheduled_backups()
        .as_histories()
        .as_idx(&key)
        .or_not_found(&key)?
        .de()?;
    let timezone = history.timezone.parse().map_err(|_| {
        Error::new(
            eyre!("{}", t!("backup.scheduled.stored-timezone-invalid")),
            ErrorKind::Backup,
        )
    })?;
    let preview = params.policy.preview(&history.snapshots, timezone)?;
    let jobs = db.as_public().as_scheduled_backups().as_jobs();
    let affected_jobs = history
        .feeding_jobs
        .iter()
        .filter_map(|id| jobs.as_idx(id))
        .map(|job| job.de().map(|job: BackupJob| job.name))
        .collect::<Result<_, _>>()?;
    Ok(RetentionPolicyChangePreview {
        removed: preview.removed,
        estimated_reclaimed_bytes: preview.estimated_reclaimed_bytes,
        affected_jobs,
    })
}

#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CreateBackupJobParams {
    pub name: String,
    pub target_id: BackupTargetId,
    pub services: BackupServiceScope,
    pub schedule: Schedule,
    pub default_retention: RetentionPolicy,
    pub retention_overrides: BTreeMap<PackageId, RetentionPolicy>,
    pub password: PasswordType,
    /// Password that encrypted the existing backup location.
    #[serde(default)]
    #[ts(optional)]
    pub old_password: Option<PasswordType>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Queue the job's first run after creation.
    #[serde(default)]
    pub run_now: bool,
}

#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ValidateBackupJobParams {
    /// Existing job being replaced, or `None` for a new job.
    pub id: Option<BackupJobId>,
    pub services: BackupServiceScope,
    pub schedule: Schedule,
    pub default_retention: RetentionPolicy,
    pub retention_overrides: BTreeMap<PackageId, RetentionPolicy>,
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct AddBackupJobCliParams {
    #[arg(help = "help.arg.automatic-backup-job-name")]
    pub name: String,
    #[arg(help = "help.arg.backup-target-id")]
    pub target_id: BackupTargetId,
    #[arg(help = "help.arg.backup-password")]
    pub password: PasswordType,
    #[arg(long, help = "help.arg.old-backup-password")]
    #[serde(default)]
    pub old_password: Option<PasswordType>,
    #[arg(
        long,
        default_value = "0 3 * * *",
        help = "help.arg.automatic-backup-cron"
    )]
    pub cron: String,
    #[arg(
        long,
        default_value = "UTC",
        help = "help.arg.automatic-backup-timezone"
    )]
    pub timezone: String,
    #[arg(
        long,
        value_delimiter = ',',
        conflicts_with = "exclude_package_ids",
        help = "help.arg.automatic-backup-package-ids"
    )]
    pub package_ids: Vec<PackageId>,
    #[arg(
        long,
        value_delimiter = ',',
        conflicts_with = "package_ids",
        help = "help.arg.automatic-backup-excluded-package-ids"
    )]
    pub exclude_package_ids: Vec<PackageId>,
    #[arg(long, value_enum, help = "help.arg.automatic-backup-system-data")]
    pub system_data: Option<SystemDataSelection>,
    #[arg(
        long = "keep-rule",
        alias = "keep-tier",
        value_name = "INTERVAL:COVERAGE",
        value_parser = parse_retention_tier,
        help = "help.arg.automatic-backup-retention-tier"
    )]
    pub retention_tiers: Vec<RetentionTier>,
    #[arg(
        long = "service-keep-rule",
        alias = "service-keep-tier",
        value_name = "PACKAGE_ID=INTERVAL:COVERAGE",
        value_parser = parse_retention_override_tier,
        help = "help.arg.automatic-backup-service-retention-tier"
    )]
    pub retention_override_tiers: Vec<(PackageId, RetentionTier)>,
    #[arg(
        long = "service-latest-only",
        value_name = "PACKAGE_ID",
        value_parser = parse_backup_item_id,
        value_delimiter = ',',
        help = "help.arg.automatic-backup-service-latest-only"
    )]
    pub latest_only_overrides: Vec<PackageId>,
    #[arg(long, help = "help.arg.automatic-backup-disabled")]
    pub disabled: bool,
}

pub async fn add_cli(
    ctx: RpcContext,
    AddBackupJobCliParams {
        name,
        target_id,
        password,
        old_password,
        cron,
        timezone,
        package_ids,
        exclude_package_ids,
        system_data,
        retention_tiers,
        retention_override_tiers,
        latest_only_overrides,
        disabled,
    }: AddBackupJobCliParams,
) -> Result<BackupJob, Error> {
    let services = service_scope_from_cli(package_ids, exclude_package_ids, system_data);
    create(
        ctx,
        CreateBackupJobParams {
            name,
            target_id,
            services,
            schedule: Schedule::new(cron, timezone)?,
            default_retention: RetentionPolicy {
                tiers: retention_tiers,
            },
            retention_overrides: retention_overrides_from_cli(
                retention_override_tiers,
                latest_only_overrides,
            )?,
            password,
            old_password,
            enabled: !disabled,
            run_now: false,
        },
    )
    .await
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct EditBackupJobCliParams {
    #[arg(help = "help.arg.automatic-backup-job-id")]
    pub id: BackupJobId,
    #[arg(long, help = "help.arg.automatic-backup-job-name")]
    pub name: Option<String>,
    #[arg(long, help = "help.arg.automatic-backup-cron")]
    pub cron: Option<String>,
    #[arg(long, help = "help.arg.automatic-backup-timezone")]
    pub timezone: Option<String>,
    #[arg(
        long,
        conflicts_with_all = ["package_ids", "exclude_package_ids"],
        help = "help.arg.automatic-backup-all-services"
    )]
    pub all_services: bool,
    #[arg(
        long,
        value_delimiter = ',',
        conflicts_with_all = ["all_services", "exclude_package_ids"],
        help = "help.arg.automatic-backup-package-ids"
    )]
    pub package_ids: Vec<PackageId>,
    #[arg(
        long,
        value_delimiter = ',',
        conflicts_with_all = ["all_services", "package_ids"],
        help = "help.arg.automatic-backup-excluded-package-ids"
    )]
    pub exclude_package_ids: Vec<PackageId>,
    #[arg(long, value_enum, help = "help.arg.automatic-backup-system-data")]
    pub system_data: Option<SystemDataSelection>,
    #[arg(
        long = "keep-rule",
        alias = "keep-tier",
        value_name = "INTERVAL:COVERAGE",
        value_parser = parse_retention_tier,
        conflicts_with = "latest_only",
        help = "help.arg.automatic-backup-retention-tier"
    )]
    pub retention_tiers: Vec<RetentionTier>,
    #[arg(long, help = "help.arg.automatic-backup-latest-only")]
    pub latest_only: bool,
    #[arg(
        long = "service-keep-rule",
        alias = "service-keep-tier",
        value_name = "PACKAGE_ID=INTERVAL:COVERAGE",
        value_parser = parse_retention_override_tier,
        help = "help.arg.automatic-backup-service-retention-tier"
    )]
    pub retention_override_tiers: Vec<(PackageId, RetentionTier)>,
    #[arg(
        long = "service-latest-only",
        value_name = "PACKAGE_ID",
        value_parser = parse_backup_item_id,
        value_delimiter = ',',
        help = "help.arg.automatic-backup-service-latest-only"
    )]
    pub latest_only_overrides: Vec<PackageId>,
    #[arg(
        long = "use-default-retention",
        value_name = "PACKAGE_ID",
        value_parser = parse_backup_item_id,
        value_delimiter = ',',
        help = "help.arg.automatic-backup-use-default-retention"
    )]
    pub default_retention_packages: Vec<PackageId>,
}

pub async fn edit_cli(
    ctx: RpcContext,
    EditBackupJobCliParams {
        id,
        name,
        cron,
        timezone,
        all_services,
        package_ids,
        exclude_package_ids,
        system_data,
        retention_tiers,
        latest_only,
        retention_override_tiers,
        latest_only_overrides,
        default_retention_packages,
    }: EditBackupJobCliParams,
) -> Result<BackupJob, Error> {
    let job: BackupJob = ctx
        .db
        .peek()
        .await
        .as_public()
        .as_scheduled_backups()
        .as_jobs()
        .as_idx(&id)
        .or_not_found(&id)?
        .de()?;
    let mut services = if all_services {
        BackupServiceScope::All
    } else if !package_ids.is_empty() {
        BackupServiceScope::Selected {
            package_ids: package_ids.into_iter().collect(),
            include_system: Some(job.services.includes(&SYSTEM_PACKAGE_ID)),
        }
    } else if !exclude_package_ids.is_empty() {
        let mut excluded_package_ids: BTreeSet<_> = exclude_package_ids.into_iter().collect();
        if !job.services.includes(&SYSTEM_PACKAGE_ID) {
            excluded_package_ids.insert(SYSTEM_PACKAGE_ID.clone());
        }
        BackupServiceScope::AllExcept {
            excluded_package_ids,
        }
    } else {
        job.services.clone()
    };
    if let Some(system_data) = system_data {
        services = with_system_data_selection(services, system_data);
    }
    let schedule = if cron.is_some() || timezone.is_some() {
        Schedule::new(
            cron.unwrap_or_else(|| job.schedule.cron.clone()),
            timezone.unwrap_or_else(|| job.schedule.timezone.clone()),
        )?
    } else {
        job.schedule.clone()
    };
    let default_retention = if latest_only {
        RetentionPolicy::latest_only()
    } else if !retention_tiers.is_empty() {
        RetentionPolicy {
            tiers: retention_tiers,
        }
    } else {
        job.default_retention.clone()
    };

    let mut retention_overrides = job.retention_overrides;
    let override_updates =
        retention_overrides_from_cli(retention_override_tiers, latest_only_overrides)?;
    let default_retention_packages: BTreeSet<_> = default_retention_packages.into_iter().collect();
    if override_updates
        .keys()
        .any(|package_id| default_retention_packages.contains(package_id))
    {
        return Err(Error::new(
            eyre!("{}", t!("backup.scheduled.invalid-retention-tiers")),
            ErrorKind::InvalidRequest,
        ));
    }
    for package_id in default_retention_packages {
        retention_overrides.remove(&package_id);
    }
    retention_overrides.extend(override_updates);

    update(
        ctx,
        UpdateBackupJobParams {
            id,
            name: name.unwrap_or(job.name),
            services,
            schedule,
            default_retention,
            retention_overrides,
        },
    )
    .await
}

fn parse_retention_tier(value: &str) -> Result<RetentionTier, String> {
    let (interval, coverage) = value
        .split_once(':')
        .ok_or_else(|| t!("backup.scheduled.invalid-version-history-rule").to_string())?;
    let tier = RetentionTier {
        interval_seconds: parse_duration_seconds(interval)?,
        coverage_seconds: parse_duration_seconds(coverage)?,
    };
    RetentionPolicy {
        tiers: vec![tier.clone()],
    }
    .validate()
    .map_err(|error| error.to_string())?;
    Ok(tier)
}

fn parse_backup_item_id(value: &str) -> Result<PackageId, crate::id::InvalidId> {
    if value == &**SYSTEM_PACKAGE_ID {
        Ok(SYSTEM_PACKAGE_ID.clone())
    } else {
        value.parse()
    }
}

fn parse_retention_override_tier(value: &str) -> Result<(PackageId, RetentionTier), String> {
    let (package_id, tier) = value
        .split_once('=')
        .ok_or_else(|| t!("backup.scheduled.invalid-retention-override").to_string())?;
    Ok((
        parse_backup_item_id(package_id).map_err(|error| error.to_string())?,
        parse_retention_tier(tier)?,
    ))
}

fn retention_overrides_from_cli(
    retention_override_tiers: Vec<(PackageId, RetentionTier)>,
    latest_only_overrides: Vec<PackageId>,
) -> Result<BTreeMap<PackageId, RetentionPolicy>, Error> {
    let mut overrides: BTreeMap<PackageId, RetentionPolicy> = BTreeMap::new();
    for (package_id, tier) in retention_override_tiers {
        overrides.entry(package_id).or_default().tiers.push(tier);
    }
    for package_id in latest_only_overrides {
        if overrides.contains_key(&package_id) {
            return Err(Error::new(
                eyre!("{}", t!("backup.scheduled.invalid-retention-tiers")),
                ErrorKind::InvalidRequest,
            ));
        }
        overrides.insert(package_id, RetentionPolicy::latest_only());
    }
    for policy in overrides.values() {
        policy.validate()?;
    }
    Ok(overrides)
}

fn parse_duration_seconds(value: &str) -> Result<u64, String> {
    let value = value.trim();
    let split = value
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(value.len());
    let (amount, suffix) = value.split_at(split);
    let amount = amount
        .parse::<u64>()
        .map_err(|_| t!("backup.scheduled.invalid-duration", value = value).to_string())?;
    let multiplier = match suffix {
        "" | "s" => 1,
        "m" => 60,
        "h" => 60 * 60,
        "d" => 24 * 60 * 60,
        "w" => 7 * 24 * 60 * 60,
        _ => {
            return Err(t!("backup.scheduled.invalid-duration-unit", value = value).to_string());
        }
    };
    amount
        .checked_mul(multiplier)
        .filter(|seconds| *seconds > 0)
        .ok_or_else(|| t!("backup.scheduled.invalid-duration", value = value).to_string())
}

/// Parses a package and checkpoint selection from `PACKAGE_ID=SNAPSHOT_ID`.
pub fn parse_checkpoint_selection(value: &str) -> Result<(PackageId, ServiceSnapshotId), String> {
    let (package_id, snapshot_id) = value
        .split_once('=')
        .ok_or_else(|| t!("backup.scheduled.invalid-checkpoint-selection").to_string())?;
    Ok((
        package_id
            .parse()
            .map_err(|error: crate::id::InvalidId| error.to_string())?,
        snapshot_id
            .parse()
            .map_err(|error: Error| error.to_string())?,
    ))
}

/// Parses a new-service review decision from `JOB_ID=add|skip`.
pub fn parse_review_decision(value: &str) -> Result<(BackupJobId, bool), String> {
    let (job_id, decision) = value
        .split_once('=')
        .ok_or_else(|| t!("backup.scheduled.invalid-review-decision").to_string())?;
    let add = match decision {
        "add" => true,
        "skip" => false,
        _ => {
            return Err(t!("backup.scheduled.invalid-review-decision").to_string());
        }
    };
    Ok((
        job_id.parse().map_err(|error: Error| error.to_string())?,
        add,
    ))
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ResolveBackupReviewCliParams {
    #[arg(help = "help.arg.package-id")]
    pub package_id: PackageId,
    #[arg(
        long = "decision",
        value_parser = parse_review_decision,
        help = "help.arg.automatic-backup-review-decision"
    )]
    pub decisions: Vec<(BackupJobId, bool)>,
}

pub async fn resolve_review_cli(
    ctx: RpcContext,
    ResolveBackupReviewCliParams {
        package_id,
        decisions,
    }: ResolveBackupReviewCliParams,
) -> Result<(), Error> {
    super::review::resolve(
        ctx,
        super::review::ResolveNewServiceBackupReviewParams {
            package_id,
            decisions: decisions.into_iter().collect(),
        },
    )
    .await
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct RestoreAutomaticCheckpointCliParams {
    #[arg(help = "help.arg.backup-target-id")]
    pub target_id: BackupTargetId,
    #[arg(
        required = true,
        value_parser = parse_checkpoint_selection,
        help = "help.arg.automatic-backup-checkpoint-selection"
    )]
    pub checkpoints: Vec<(PackageId, ServiceSnapshotId)>,
    #[arg(long, help = "help.arg.server-id")]
    pub server_id: Option<String>,
    #[arg(long, help = "help.arg.backup-password")]
    pub password: Option<String>,
}

pub async fn restore_automatic_checkpoint_cli(
    ctx: RpcContext,
    RestoreAutomaticCheckpointCliParams {
        target_id,
        checkpoints,
        server_id,
        password,
    }: RestoreAutomaticCheckpointCliParams,
) -> Result<(), Error> {
    crate::backup::restore::restore_scheduled_packages_rpc(
        ctx,
        crate::backup::restore::RestoreScheduledPackagesParams {
            target_id,
            snapshots: checkpoints.into_iter().collect(),
            server_id,
            password,
        },
    )
    .await
}

pub async fn validate(
    ctx: RpcContext,
    ValidateBackupJobParams {
        id,
        services,
        schedule,
        default_retention,
        retention_overrides,
    }: ValidateBackupJobParams,
) -> Result<(), Error> {
    schedule.next_after(Utc::now(), None)?;
    validate_retention_policies(&default_retention, &retention_overrides)?;

    let db = ctx.db.peek().await;
    selected_installed_services(&db, &services)?;
    if let Some(id) = &id {
        let _: BackupJob = db
            .as_public()
            .as_scheduled_backups()
            .as_jobs()
            .as_idx(id)
            .or_not_found(id)?
            .de()?;
    }
    Ok(())
}

pub async fn create(
    ctx: RpcContext,
    CreateBackupJobParams {
        name,
        target_id,
        services,
        schedule,
        default_retention,
        retention_overrides,
        password,
        old_password,
        enabled,
        run_now,
    }: CreateBackupJobParams,
) -> Result<BackupJob, Error> {
    validate_job_input(&name, &schedule, &default_retention, &retention_overrides)?;
    let password = password.decrypt(&ctx)?;
    let old_password = old_password
        .map(|password| password.decrypt(&ctx))
        .transpose()?;
    let coordinator = crate::backup::try_backup_coordinator(ctx.backup_coordinator.clone())?;
    let db = ctx.db.peek().await;
    RpcContext::check_password(&db, &password)?;
    validate_unique_job_name(&db, &name, None)?;
    let package_ids = selected_installed_services(&db, &services)?;
    let server_id = db.as_public().as_server_info().as_id().de()?;
    let hostname = ctx.account.peek(|account| account.hostname.clone());
    let target_guard = TmpMountGuard::mount(&target_id.clone().load(&db)?, ReadWrite).await?;
    let (mut scheduled_guard, encryption_key) = ScheduledBackupMountGuard::initialize(
        target_guard,
        &server_id,
        hostname,
        &password,
        old_password.as_deref(),
    )
    .await
    .map_err(backup_password_mismatch)?;
    let target_instance_id = scheduled_guard.recovery.target_instance_id.clone();
    validate_target_identity(&db, &target_id, &target_instance_id)?;
    reconcile_target_histories(&db, &target_id, &mut scheduled_guard)?;
    let target_metadata = scheduled_guard.metadata.clone();
    scheduled_guard.save_and_unmount().await?;

    let id = Guid::new();
    let now = Utc::now();
    let next_run_at = enabled
        .then(|| schedule.next_after(now, None))
        .transpose()?
        .map(|x| x.utc);
    let job = BackupJob {
        id: id.clone(),
        name,
        enabled,
        pause: None,
        target_id: target_id.clone(),
        target_instance_id: target_instance_id.clone(),
        services,
        schedule,
        default_retention,
        retention_overrides,
        status: BackupJobStatus {
            next_run_at,
            run_requested: enabled && run_now,
            ..Default::default()
        },
        created_at: now,
        updated_at: now,
    };

    ctx.db
        .mutate(|db| {
            let device_key = db.as_private().as_scheduled_backup_device_key().de()?;
            let credential =
                ScheduledBackupCredential::seal(target_instance_id, &encryption_key, &device_key)?;
            db.as_private_mut()
                .as_scheduled_backup_credentials_mut()
                .insert(&target_id.to_string(), &credential)?;
            db.as_public_mut()
                .as_scheduled_backups_mut()
                .as_jobs_mut()
                .insert(&id, &job)?;
            import_target_histories(db, &target_id, &target_metadata)?;
            associate_histories(db, &job, &package_ids)?;
            refresh_archive_state(db, &job.target_id)?;
            Ok(())
        })
        .await
        .result?;
    sync_archive_states(&ctx, &job.target_id, &coordinator)
        .await
        .log_err();
    drop(coordinator);
    if job.status.run_requested {
        super::scheduler::dispatch_due_jobs(&ctx).await.log_err();
        let db = ctx.db.peek().await;
        let current: BackupJob = db
            .as_public()
            .as_scheduled_backups()
            .as_jobs()
            .as_idx(&id)
            .or_not_found(&id)?
            .de()?;
        return Ok(current);
    }
    Ok(job)
}

#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UpdateBackupJobParams {
    pub id: BackupJobId,
    pub name: String,
    pub services: BackupServiceScope,
    pub schedule: Schedule,
    pub default_retention: RetentionPolicy,
    pub retention_overrides: BTreeMap<PackageId, RetentionPolicy>,
}

pub async fn update(
    ctx: RpcContext,
    UpdateBackupJobParams {
        id,
        name,
        services,
        schedule,
        default_retention,
        retention_overrides,
    }: UpdateBackupJobParams,
) -> Result<BackupJob, Error> {
    validate_job_input(&name, &schedule, &default_retention, &retention_overrides)?;
    let coordinator = crate::backup::try_backup_coordinator(ctx.backup_coordinator.clone())?;
    let snapshot = ctx.db.peek().await;
    validate_unique_job_name(&snapshot, &name, Some(&id))?;
    let mut job: BackupJob = snapshot
        .as_public()
        .as_scheduled_backups()
        .as_jobs()
        .as_idx(&id)
        .or_not_found(&id)?
        .de()?;
    let server_id = snapshot.as_public().as_server_info().as_id().de()?;
    let (target_guard, _) = mount_scheduled_target(
        &snapshot,
        &job.target_id,
        &server_id,
        &job.target_instance_id,
        None,
    )
    .await?;
    target_guard.save_and_unmount().await?;
    let old_services = associated_service_ids(&snapshot, &job)?;
    let new_services = selected_installed_services(&snapshot, &services)?;
    let removed_services = old_services
        .into_iter()
        .filter(|package_id| !services.includes(package_id))
        .collect();
    job.name = name;
    job.services = services;
    job.default_retention = default_retention;
    job.retention_overrides = retention_overrides;
    job.updated_at = Utc::now();
    let now = job.updated_at;
    update_job_schedule(&mut job, schedule, now)?;

    ctx.db
        .mutate(|db| {
            validate_unique_job_name(db, &job.name, Some(&id))?;
            disassociate_histories(db, &job, &removed_services)?;
            associate_histories(db, &job, &new_services)?;
            db.as_public_mut()
                .as_scheduled_backups_mut()
                .as_jobs_mut()
                .insert(&id, &job)?;
            refresh_archive_state(db, &job.target_id)?;
            Ok(())
        })
        .await
        .result?;
    sync_archive_states(&ctx, &job.target_id, &coordinator)
        .await
        .log_err();
    Ok(job)
}

#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SetBackupJobEnabledParams {
    pub id: BackupJobId,
    pub enabled: bool,
}

#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SetBackupJobsEnabledParams {
    pub ids: Vec<BackupJobId>,
    pub enabled: bool,
}

pub async fn set_enabled(
    ctx: RpcContext,
    SetBackupJobEnabledParams { id, enabled }: SetBackupJobEnabledParams,
) -> Result<BackupJob, Error> {
    Ok(set_enabled_bulk(
        ctx,
        SetBackupJobsEnabledParams {
            ids: vec![id],
            enabled,
        },
    )
    .await?
    .pop()
    .expect("one backup job was enabled or disabled"))
}

/// Atomically enables or disables multiple automatic backup jobs.
pub async fn set_enabled_bulk(
    ctx: RpcContext,
    SetBackupJobsEnabledParams { ids, enabled }: SetBackupJobsEnabledParams,
) -> Result<Vec<BackupJob>, Error> {
    let ids = ids.into_iter().collect::<BTreeSet<_>>();
    let coordinator = crate::backup::try_backup_coordinator(ctx.backup_coordinator.clone())?;
    let (jobs, targets) = ctx
        .db
        .mutate(|db| {
            let mut jobs = ids
                .iter()
                .map(|id| {
                    let job: BackupJob = db
                        .as_public()
                        .as_scheduled_backups()
                        .as_jobs()
                        .as_idx(id)
                        .or_not_found(id)?
                        .de()?;
                    Ok(job)
                })
                .collect::<Result<Vec<_>, Error>>()?;
            if enabled
                && jobs.iter().any(|job| {
                    job.pause
                        .as_ref()
                        .is_some_and(BackupJobPause::requires_target_retry)
                })
            {
                return Err(Error::new(
                    eyre!("{}", t!("backup.scheduled.retry-before-resume")),
                    ErrorKind::InvalidRequest,
                ));
            }
            let now = Utc::now();
            let mut targets = BTreeSet::new();
            for job in &mut jobs {
                job.enabled = enabled;
                job.pause = match (&job.pause, enabled) {
                    (Some(BackupJobPause::User), true) => None,
                    (None, false) => Some(BackupJobPause::User),
                    (pause, _) => pause.clone(),
                };
                job.updated_at = now;
                reschedule_job(job, now)?;
                if !enabled {
                    job.status.run_requested = false;
                }
                targets.insert(job.target_id.clone());
                db.as_public_mut()
                    .as_scheduled_backups_mut()
                    .as_jobs_mut()
                    .insert(&job.id, job)?;
            }
            for target in &targets {
                refresh_archive_state(db, target)?;
            }
            Ok((jobs, targets))
        })
        .await
        .result?;
    for target in targets {
        sync_archive_states(&ctx, &target, &coordinator)
            .await
            .log_err();
    }
    Ok(jobs)
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct DeleteBackupJobParams {
    #[arg(help = "help.arg.automatic-backup-job-id")]
    pub id: BackupJobId,
}

#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DeleteBackupJobWithBackupsParams {
    pub id: BackupJobId,
    pub password: String,
    #[serde(default)]
    #[ts(optional)]
    pub old_password: Option<String>,
}

pub async fn delete_with_backups(
    ctx: RpcContext,
    DeleteBackupJobWithBackupsParams {
        id,
        password,
        old_password,
    }: DeleteBackupJobWithBackupsParams,
) -> Result<(), Error> {
    let _coordinator = crate::backup::try_backup_coordinator(ctx.backup_coordinator.clone())?;
    let db = ctx.db.peek().await;
    RpcContext::check_password(&db, &password)?;
    let job: BackupJob = db
        .as_public()
        .as_scheduled_backups()
        .as_jobs()
        .as_idx(&id)
        .or_not_found(&id)?
        .de()?;
    let server_id = db.as_public().as_server_info().as_id().de()?;
    let (mut guard, credential) = mount_scheduled_target(
        &db,
        &job.target_id,
        &server_id,
        &job.target_instance_id,
        Some(old_password.as_deref().unwrap_or(&password)),
    )
    .await
    .map_err(backup_password_mismatch)?;
    let mut histories = db
        .as_public()
        .as_scheduled_backups()
        .as_histories()
        .as_entries()?
        .into_iter()
        .map(|(_, history)| history.de())
        .collect::<Result<Vec<ServiceTargetHistory>, Error>>()?
        .into_iter()
        .filter(|history| {
            history.target_id == job.target_id
                && history.feeding_jobs.len() == 1
                && history.feeding_jobs.contains(&id)
        })
        .collect::<Vec<_>>();
    for history in &mut histories {
        guard.metadata.refresh_history(history);
    }
    let requested = histories
        .iter()
        .filter(|history| !history.snapshots.is_empty())
        .map(|history| {
            (
                history.package_id.clone(),
                history
                    .snapshots
                    .iter()
                    .map(|snapshot| snapshot.id.clone())
                    .collect(),
            )
        })
        .collect();
    drop(db);
    ctx.db.mutate(|db| remove_job(db, &id)).await.result?;
    for history in &mut histories {
        history.feeding_jobs.remove(&id);
        history.archived = true;
    }
    reconcile_target_histories(&ctx.db.peek().await, &job.target_id, &mut guard)?;
    let deletion = guard.delete_archived_snapshots_bulk(&requested).await;
    finish_history_change(
        &ctx,
        guard,
        histories,
        Some((&job.target_id, &credential)),
        deletion,
    )
    .await?;
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct RunBackupJobNowParams {
    #[arg(help = "help.arg.automatic-backup-job-id")]
    pub id: BackupJobId,
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct BackupJobIdCliParams {
    #[arg(help = "help.arg.automatic-backup-job-id")]
    pub id: BackupJobId,
}

pub async fn enable_cli(
    ctx: RpcContext,
    BackupJobIdCliParams { id }: BackupJobIdCliParams,
) -> Result<BackupJob, Error> {
    set_enabled(ctx, SetBackupJobEnabledParams { id, enabled: true }).await
}

pub async fn disable_cli(
    ctx: RpcContext,
    BackupJobIdCliParams { id }: BackupJobIdCliParams,
) -> Result<BackupJob, Error> {
    set_enabled(ctx, SetBackupJobEnabledParams { id, enabled: false }).await
}

pub async fn run_now(
    ctx: RpcContext,
    RunBackupJobNowParams { id }: RunBackupJobNowParams,
) -> Result<BackupRun, Error> {
    run_job(ctx, id, BackupRunTrigger::RunNow).await
}

/// Deletes one automatic backup job while preserving shared histories.
pub async fn delete(
    ctx: RpcContext,
    DeleteBackupJobParams { id }: DeleteBackupJobParams,
) -> Result<(), Error> {
    let coordinator = crate::backup::try_backup_coordinator(ctx.backup_coordinator.clone())?;
    let target_id = ctx.db.mutate(|db| remove_job(db, &id)).await.result?;
    sync_archive_states(&ctx, &target_id, &coordinator)
        .await
        .log_err();
    Ok(())
}

fn remove_job(db: &mut DatabaseModel, id: &BackupJobId) -> Result<BackupTargetId, Error> {
    let job: BackupJob = db
        .as_public()
        .as_scheduled_backups()
        .as_jobs()
        .as_idx(id)
        .or_not_found(id)?
        .de()?;
    let package_ids = associated_service_ids(db, &job)?;
    disassociate_histories(db, &job, &package_ids)?;
    db.as_public_mut()
        .as_scheduled_backups_mut()
        .as_jobs_mut()
        .remove(id)?;
    refresh_archive_state(db, &job.target_id)?;
    Ok(job.target_id)
}

fn validate_job_input(
    name: &str,
    schedule: &Schedule,
    default_retention: &RetentionPolicy,
    retention_overrides: &BTreeMap<PackageId, RetentionPolicy>,
) -> Result<(), Error> {
    if name.trim().is_empty() || name.len() > 80 {
        return Err(Error::new(
            eyre!("{}", t!("backup.scheduled.invalid-job-name")),
            ErrorKind::InvalidRequest,
        ));
    }
    schedule.next_after(Utc::now(), None)?;
    validate_retention_policies(default_retention, retention_overrides)
}

fn validate_retention_policies(
    default_retention: &RetentionPolicy,
    retention_overrides: &BTreeMap<PackageId, RetentionPolicy>,
) -> Result<(), Error> {
    default_retention.validate()?;
    for policy in retention_overrides.values() {
        policy.validate()?;
    }
    Ok(())
}

fn validate_unique_job_name(
    db: &DatabaseModel,
    name: &str,
    replacing: Option<&BackupJobId>,
) -> Result<(), Error> {
    let jobs = db
        .as_public()
        .as_scheduled_backups()
        .as_jobs()
        .as_entries()?
        .into_iter()
        .map(|(_, job)| job.de())
        .collect::<Result<Vec<BackupJob>, Error>>()?;
    if job_name_conflicts(&jobs, name, replacing) {
        return Err(Error::new(
            eyre!("{}", t!("backup.scheduled.duplicate-job-name")),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

fn job_names_match(left: &str, right: &str) -> bool {
    left.trim() == right.trim()
}

fn job_name_conflicts(jobs: &[BackupJob], name: &str, replacing: Option<&BackupJobId>) -> bool {
    jobs.iter()
        .any(|job| Some(&job.id) != replacing && job_names_match(&job.name, name))
}

fn selected_installed_services(
    db: &DatabaseModel,
    scope: &BackupServiceScope,
) -> Result<BTreeSet<PackageId>, Error> {
    let installed = db
        .as_public()
        .as_package_data()
        .as_entries()?
        .into_iter()
        .filter(|(_, package)| package.as_state_info().expect_installed().is_ok())
        .map(|(id, _)| id)
        .collect();
    Ok(scope.configured_services(installed))
}

pub(crate) async fn sync_archive_states(
    ctx: &RpcContext,
    target_id: &BackupTargetId,
    _coordinator: &tokio::sync::OwnedMutexGuard<()>,
) -> Result<(), Error> {
    let db = ctx.db.peek().await;
    let Some(credential) = db
        .as_private()
        .as_scheduled_backup_credentials()
        .as_idx(&target_id.to_string())
    else {
        return Ok(());
    };
    let credential: ScheduledBackupCredential = credential.de()?;
    let encryption_key =
        credential.open(&db.as_private().as_scheduled_backup_device_key().de()?)?;
    let server_id = db.as_public().as_server_info().as_id().de()?;
    let mut guard = ScheduledBackupMountGuard::mount_with_key(
        TmpMountGuard::mount(&target_id.clone().load(&db)?, ReadWrite).await?,
        &server_id,
        &credential.target_instance_id,
        &encryption_key,
    )
    .await?;
    reconcile_target_histories(&db, target_id, &mut guard)?;
    guard.save_and_unmount().await
}

pub(super) fn reconcile_target_histories(
    db: &DatabaseModel,
    target_id: &BackupTargetId,
    guard: &mut ScheduledBackupMountGuard<TmpMountGuard>,
) -> Result<(), Error> {
    let histories = db
        .as_public()
        .as_scheduled_backups()
        .as_histories()
        .as_entries()?
        .into_iter()
        .map(|(_, history)| history.de())
        .collect::<Result<Vec<ServiceTargetHistory>, Error>>()?;
    guard.metadata.reconcile_histories(
        histories
            .into_iter()
            .filter(|history| history.target_id == *target_id),
    );
    Ok(())
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod cli_tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn backup_password_error_preserves_other_failures() {
        let mismatch = backup_password_mismatch(Error::new(
            eyre!("wrong backup password"),
            ErrorKind::IncorrectPassword,
        ));
        assert_eq!(mismatch.kind, ErrorKind::BackupPasswordMismatch);
        let unreadable =
            backup_password_mismatch(Error::new(eyre!("metadata missing"), ErrorKind::Filesystem));
        assert_eq!(unreadable.kind, ErrorKind::Filesystem);
        assert_eq!(unreadable.source.to_string(), "metadata missing");
    }

    #[test]
    fn schedule_cli_accepts_separate_server_and_backup_passwords() {
        let add = AddBackupJobCliParams::try_parse_from([
            "test",
            "Daily",
            "cifs-0",
            "current",
            "--old-password",
            "original",
        ])
        .unwrap();
        assert!(add.old_password.is_some());
        let retry = RetryBackupTargetParams::try_parse_from([
            "test",
            "cifs-0",
            "current",
            "--old-password",
            "original",
        ])
        .unwrap();
        assert!(retry.old_password.is_some());
        let without_original =
            RetryBackupTargetParams::try_parse_from(["test", "cifs-0", "current"]).unwrap();
        assert!(without_original.old_password.is_none());
        let legacy: RetryBackupTargetParams = serde_json::from_value(serde_json::json!({
            "targetId": "cifs-0", "password": "current"
        }))
        .unwrap();
        assert!(legacy.old_password.is_none());
    }

    fn backup_job(
        id: BackupJobId,
        name: &str,
        target_id: &str,
        target_instance_id: &str,
        package_id: &str,
    ) -> BackupJob {
        let now = Utc::now();
        BackupJob {
            id,
            name: name.to_owned(),
            enabled: true,
            pause: None,
            target_id: target_id.parse().unwrap(),
            target_instance_id: target_instance_id.to_owned(),
            services: BackupServiceScope::Selected {
                package_ids: BTreeSet::from([package_id.parse().unwrap()]),
                include_system: Some(false),
            },
            schedule: Schedule::new("0 * * * *", "UTC").unwrap(),
            default_retention: RetentionPolicy::latest_only(),
            retention_overrides: BTreeMap::new(),
            status: BackupJobStatus::default(),
            created_at: now,
            updated_at: now,
        }
    }

    fn empty_history(feeding_jobs: BTreeSet<BackupJobId>) -> ServiceTargetHistory {
        ServiceTargetHistory {
            target_id: "cifs-0".parse().unwrap(),
            target_instance_id: "instance".to_owned(),
            package_id: "hello-world".parse().unwrap(),
            timezone: "UTC".to_owned(),
            policy: RetentionPolicy::latest_only(),
            feeding_jobs,
            snapshots: Vec::new(),
            archived: true,
        }
    }

    #[test]
    fn capacity_preview_uses_proposed_rules_and_preserves_established_history_on_create() {
        let proposed = RetentionPolicy {
            tiers: vec![RetentionTier {
                interval_seconds: 86400,
                coverage_seconds: 30 * 86400,
            }],
        };
        let mut history = empty_history(BTreeSet::from([BackupJobId::new()]));
        assert_eq!(
            estimated_retention_policy(Some(&history), &proposed, false),
            &proposed
        );
        assert_eq!(
            estimated_retention_policy(Some(&history), &proposed, true),
            &history.policy
        );
        history.feeding_jobs.clear();
        assert_eq!(
            estimated_retention_policy(Some(&history), &proposed, true),
            &proposed
        );
        assert_eq!(estimated_retention_policy(None, &proposed, true), &proposed);
    }

    fn backup_database() -> DatabaseModel {
        DatabaseModel::from(imbl_value::json!({
            "public": { "scheduledBackups": { "jobs": {}, "histories": {} } }
        }))
    }

    #[test]
    fn attaching_existing_target_preserves_uncached_retention_and_checkpoints() {
        let job = backup_job(
            BackupJobId::new(),
            "Latest only",
            "cifs-0",
            "instance",
            "hello-world",
        );
        let package_id: PackageId = "hello-world".parse().unwrap();
        let now = Utc::now();
        let checkpoint = super::super::ServiceSnapshot {
            id: ServiceSnapshotId::new(),
            package_id: package_id.clone(),
            package_version: "1.0.0".to_owned(),
            source: super::super::BackupSource::Scheduled,
            job_id: BackupJobId::new(),
            job_name: "Original schedule".to_owned(),
            run_id: Guid::new(),
            completed_at: now,
            logical_size: 1,
            physical_size: None,
            changed_bytes: None,
            measured_at: now,
            archived: false,
        };
        let policy = RetentionPolicy {
            tiers: vec![RetentionTier {
                interval_seconds: 86400,
                coverage_seconds: 7 * 86400,
            }],
        };
        let metadata = super::super::ScheduledBackupOnTargetMetadata {
            target_instance_id: "instance".to_owned(),
            services: BTreeMap::from([(
                package_id.clone(),
                super::super::OnTargetServiceHistory {
                    timezone: "America/New_York".to_owned(),
                    policy: policy.clone(),
                    archived: false,
                    snapshots: vec![checkpoint.clone()],
                },
            )]),
        };
        let key = history_key(&job.target_id, &package_id);
        for empty_cached_history in [false, true] {
            let mut db = backup_database();
            if empty_cached_history {
                db.as_public_mut()
                    .as_scheduled_backups_mut()
                    .as_histories_mut()
                    .insert(&key, &empty_history(BTreeSet::new()))
                    .unwrap();
            }

            import_target_histories(&mut db, &job.target_id, &metadata).unwrap();
            associate_histories(&mut db, &job, &BTreeSet::from([package_id.clone()])).unwrap();

            let history: ServiceTargetHistory = db
                .as_public()
                .as_scheduled_backups()
                .as_histories()
                .as_idx(&key)
                .unwrap()
                .de()
                .unwrap();
            assert_eq!(history.policy, policy);
            assert_eq!(history.timezone, "America/New_York");
            assert_eq!(history.snapshots, vec![checkpoint.clone()]);
            assert_eq!(history.feeding_jobs, BTreeSet::from([job.id.clone()]));
            assert!(!history.archived);
        }
    }

    #[test]
    fn target_instance_has_one_location_even_when_its_job_is_paused() {
        let mut db = backup_database();
        let mut job = backup_job(
            BackupJobId::new(),
            "Existing schedule",
            "cifs-0",
            "instance",
            "hello-world",
        );
        job.enabled = false;
        job.pause = Some(BackupJobPause::User);
        db.as_public_mut()
            .as_scheduled_backups_mut()
            .as_jobs_mut()
            .insert(&job.id, &job)
            .unwrap();

        assert!(validate_target_identity(&db, &job.target_id, "instance").is_ok());
        assert!(validate_target_identity(&db, &job.target_id, "replacement-instance").is_err());
        assert!(validate_target_identity(&db, &"cifs-1".parse().unwrap(), "instance").is_err());
        assert!(
            validate_target_identity(&db, &"cifs-1".parse().unwrap(), "other-instance").is_ok()
        );
    }

    #[test]
    fn archived_checkpoints_preserve_the_location_identity() {
        let mut db = backup_database();
        let mut history = empty_history(BTreeSet::new());
        let now = Utc::now();
        history.snapshots.push(super::super::ServiceSnapshot {
            id: ServiceSnapshotId::new(),
            package_id: history.package_id.clone(),
            package_version: "1.0.0".to_owned(),
            source: super::super::BackupSource::Scheduled,
            job_id: BackupJobId::new(),
            job_name: "Archived schedule".to_owned(),
            run_id: Guid::new(),
            completed_at: now,
            logical_size: 1,
            physical_size: None,
            changed_bytes: None,
            measured_at: now,
            archived: true,
        });
        let key = history_key(&history.target_id, &history.package_id);
        db.as_public_mut()
            .as_scheduled_backups_mut()
            .as_histories_mut()
            .insert(&key, &history)
            .unwrap();

        assert!(validate_target_identity(&db, &history.target_id, "instance").is_ok());
        assert!(validate_target_identity(&db, &history.target_id, "replacement-instance").is_err());

        history.snapshots.clear();
        db.as_public_mut()
            .as_scheduled_backups_mut()
            .as_histories_mut()
            .insert(&key, &history)
            .unwrap();
        assert!(validate_target_identity(&db, &history.target_id, "replacement-instance").is_ok());
    }

    #[test]
    fn archived_snapshot_deletion_requires_an_unreferenced_history() {
        let history = empty_history(BTreeSet::new());
        assert!(validate_archived_snapshot_deletion(&history, &BTreeSet::new()).is_ok());
        assert!(
            validate_archived_snapshot_deletion(
                &history,
                &BTreeSet::from([ServiceSnapshotId::new()]),
            )
            .is_err()
        );

        let referenced = empty_history(BTreeSet::from([BackupJobId::new()]));
        assert!(validate_archived_snapshot_deletion(&referenced, &BTreeSet::new()).is_err());
    }

    #[test]
    fn backup_job_names_are_unique_after_trimming() {
        assert!(job_names_match("Nightly", " Nightly "));
        assert!(!job_names_match("Nightly", "Weekly"));

        let id = BackupJobId::new();
        let job = backup_job(id.clone(), "Nightly", "cifs-0", "instance", "hello-world");
        assert!(!job_name_conflicts(&[job.clone()], "Nightly", Some(&id)));
        assert!(job_name_conflicts(&[job], "Nightly", None));
    }

    #[test]
    fn reassignment_preserves_disabled_state() {
        let mut job = backup_job(
            BackupJobId::new(),
            "Disabled",
            "cifs-0",
            "old-instance",
            "hello-world",
        );
        job.enabled = false;
        job.status.run_requested = true;
        job.status.next_run_at = Some(Utc::now());
        let now = Utc::now();

        update_reassigned_job(
            &mut job,
            "disk-/dev/sda1".parse().unwrap(),
            "new-instance".to_owned(),
            false,
            now,
        )
        .unwrap();

        assert!(!job.enabled);
        assert_eq!(job.target_id, "disk-/dev/sda1".parse().unwrap());
        assert_eq!(job.target_instance_id, "new-instance");
        assert_eq!(job.updated_at, now);
        assert!(job.status.next_run_at.is_none());
        assert!(!job.status.run_requested);
    }

    #[test]
    fn edits_and_resume_preserve_the_completed_fall_back_occurrence() {
        let mut job = backup_job(
            BackupJobId::new(),
            "Daily",
            "cifs-0",
            "instance",
            "hello-world",
        );
        job.schedule = Schedule::new("30 1 * * *", "America/New_York").unwrap();
        let completed = Utc.with_ymd_and_hms(2025, 11, 2, 5, 30, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2025, 11, 2, 5, 45, 0).unwrap();
        let next = Utc.with_ymd_and_hms(2025, 11, 3, 6, 30, 0).unwrap();
        job.status.last_scheduled_at = Some(completed);
        job.name = "Renamed".into();
        let schedule = job.schedule.clone();
        update_job_schedule(&mut job, schedule, now).unwrap();
        assert_eq!(job.status.next_run_at, Some(next));
        assert_eq!(job.status.last_scheduled_at, Some(completed));

        job.enabled = false;
        reschedule_job(&mut job, now).unwrap();
        assert_eq!(job.status.next_run_at, None);
        job.enabled = true;
        reschedule_job(&mut job, now).unwrap();
        assert_eq!(job.status.next_run_at, Some(next));

        job.pause = Some(BackupJobPause::TargetUnreadable);
        reschedule_job(&mut job, now).unwrap();
        assert_eq!(job.status.next_run_at, None);
    }

    #[test]
    fn changed_timing_starts_a_new_schedule_cursor() {
        for (schedule, expected) in [
            (
                Schedule::new("45 1 * * *", "America/New_York").unwrap(),
                Utc.with_ymd_and_hms(2025, 11, 2, 6, 45, 0).unwrap(),
            ),
            (
                Schedule::new("30 1 * * *", "America/Chicago").unwrap(),
                Utc.with_ymd_and_hms(2025, 11, 2, 6, 30, 0).unwrap(),
            ),
        ] {
            let mut job = backup_job(
                BackupJobId::new(),
                "Daily",
                "cifs-0",
                "instance",
                "hello-world",
            );
            job.schedule = Schedule::new("30 1 * * *", "America/New_York").unwrap();
            job.status.last_scheduled_at =
                Some(Utc.with_ymd_and_hms(2025, 11, 2, 5, 30, 0).unwrap());
            update_job_schedule(
                &mut job,
                schedule,
                Utc.with_ymd_and_hms(2025, 11, 2, 5, 45, 0).unwrap(),
            )
            .unwrap();
            assert_eq!(job.status.last_scheduled_at, None);
            assert_eq!(job.status.next_run_at, Some(expected));
        }
    }

    #[test]
    fn reassignment_keeps_immediate_runs_separate_from_the_schedule_cursor() {
        for wait_for_schedule in [false, true] {
            let mut job = backup_job(
                BackupJobId::new(),
                "Daily",
                "cifs-0",
                "instance",
                "hello-world",
            );
            job.schedule = Schedule::new("30 1 * * *", "America/New_York").unwrap();
            let completed = Utc.with_ymd_and_hms(2025, 11, 2, 5, 30, 0).unwrap();
            job.status.last_scheduled_at = Some(completed);
            update_reassigned_job(
                &mut job,
                "cifs-1".parse().unwrap(),
                "new-instance".into(),
                wait_for_schedule,
                Utc.with_ymd_and_hms(2025, 11, 2, 5, 45, 0).unwrap(),
            )
            .unwrap();
            assert_eq!(job.status.last_scheduled_at, Some(completed));
            assert_eq!(
                job.status.next_run_at,
                Some(Utc.with_ymd_and_hms(2025, 11, 3, 6, 30, 0).unwrap())
            );
            assert_eq!(job.status.run_requested, !wait_for_schedule);
        }
    }

    #[test]
    fn discovered_histories_only_report_current_feeding_jobs() {
        let package_id: PackageId = "hello-world".parse().unwrap();
        let matching = backup_job(
            BackupJobId::new(),
            "Matching",
            "cifs-0",
            "instance",
            "hello-world",
        );
        let wrong_target = backup_job(
            BackupJobId::new(),
            "Wrong target",
            "disk-/dev/sda1",
            "instance",
            "hello-world",
        );
        let wrong_instance = backup_job(
            BackupJobId::new(),
            "Wrong instance",
            "cifs-0",
            "other-instance",
            "hello-world",
        );
        let wrong_service = backup_job(
            BackupJobId::new(),
            "Wrong service",
            "cifs-0",
            "instance",
            "other-service",
        );
        let jobs = vec![
            matching.clone(),
            wrong_target,
            wrong_instance,
            wrong_service,
        ];

        assert_eq!(
            current_feeding_jobs(&jobs, &"cifs-0".parse().unwrap(), "instance", &package_id,),
            BTreeSet::from([matching.id]),
        );
        assert!(
            current_feeding_jobs(&[], &"cifs-0".parse().unwrap(), "instance", &package_id,)
                .is_empty()
        );
    }

    #[test]
    fn archive_operations_require_one_target_instance() {
        assert_eq!(
            one_target_instance_id(["instance".to_owned()]).unwrap(),
            "instance"
        );
        assert!(one_target_instance_id(Vec::new()).is_err());
        assert!(one_target_instance_id(["one".to_owned(), "two".to_owned()]).is_err());
    }

    #[test]
    fn backup_item_cli_accepts_system_data_without_relaxing_package_ids() {
        let job_id = BackupJobId::new().to_string();
        let snapshot_id = ServiceSnapshotId::new().to_string();
        for (item, valid) in [
            ("x_system", true),
            ("hello-world", true),
            ("x_other", false),
        ] {
            assert_eq!(
                DeleteArchivedSnapshotsCliParams::try_parse_from([
                    "test",
                    "cifs-0",
                    item,
                    &snapshot_id,
                    "--password",
                    "password",
                ])
                .is_ok(),
                valid,
            );
            assert_eq!(
                PreviewRetentionPolicyCliParams::try_parse_from([
                    "test",
                    "cifs-0",
                    item,
                    "--latest-only",
                ])
                .is_ok(),
                valid,
            );
            assert_eq!(
                ApplyRetentionPolicyCliParams::try_parse_from([
                    "test",
                    "cifs-0",
                    item,
                    "--latest-only",
                ])
                .is_ok(),
                valid,
            );
            let rule = format!("{item}=1h:1d");
            for (option, value) in [
                ("--service-latest-only", item),
                ("--service-keep-rule", rule.as_str()),
            ] {
                assert_eq!(
                    EstimateBackupCapacityCliParams::try_parse_from([
                        "test", "cifs-0", option, value,
                    ])
                    .is_ok(),
                    valid,
                );
                assert_eq!(
                    AddBackupJobCliParams::try_parse_from([
                        "test", "Daily", "cifs-0", "password", option, value,
                    ])
                    .is_ok(),
                    valid,
                );
                assert_eq!(
                    EditBackupJobCliParams::try_parse_from(["test", &job_id, option, value,])
                        .is_ok(),
                    valid,
                );
            }
            assert_eq!(
                EditBackupJobCliParams::try_parse_from([
                    "test",
                    &job_id,
                    "--use-default-retention",
                    item,
                ])
                .is_ok(),
                valid,
            );
        }
        assert_eq!(
            parse_backup_item_id("x_system").unwrap(),
            *SYSTEM_PACKAGE_ID
        );
        assert!("x_system".parse::<PackageId>().is_err());
        assert!(parse_checkpoint_selection(&format!("x_system={snapshot_id}")).is_err());
        assert!(
            AddBackupJobCliParams::try_parse_from([
                "test",
                "Daily",
                "cifs-0",
                "password",
                "--package-ids",
                "x_system",
            ])
            .is_err()
        );
    }

    #[test]
    fn retention_tier_accepts_human_duration_suffixes() {
        assert_eq!(
            parse_retention_tier("1d:2w").unwrap(),
            RetentionTier {
                interval_seconds: 24 * 60 * 60,
                coverage_seconds: 14 * 24 * 60 * 60,
            }
        );
    }

    #[test]
    fn retention_tier_rejects_invalid_or_inverted_ranges() {
        assert!(parse_retention_tier("1d").is_err());
        assert!(parse_retention_tier("1w:1d").is_err());
        assert!(parse_retention_tier("0d:1d").is_err());
    }

    #[test]
    fn edit_job_cli_distinguishes_omitted_and_explicit_settings() {
        let id = BackupJobId::new().to_string();
        let params = EditBackupJobCliParams::try_parse_from([
            "test",
            id.as_str(),
            "--cron",
            "15 * * * *",
            "--all-services",
            "--latest-only",
            "--service-keep-rule",
            "bitcoind=1h:1d",
            "--service-latest-only",
            "lnd",
            "--use-default-retention",
            "electrs",
        ])
        .unwrap();

        assert_eq!(params.id.to_string(), id);
        assert_eq!(params.cron.as_deref(), Some("15 * * * *"));
        assert!(params.all_services);
        assert!(params.latest_only);
        assert!(params.retention_tiers.is_empty());
        assert_eq!(params.retention_override_tiers.len(), 1);
        assert_eq!(params.latest_only_overrides.len(), 1);
        assert_eq!(params.default_retention_packages.len(), 1);
    }

    #[test]
    fn cli_service_scope_controls_system_data_independently() {
        let add = AddBackupJobCliParams::try_parse_from([
            "test",
            "Selected services",
            "cifs-0",
            "password",
            "--package-ids",
            "bitcoind",
            "--system-data",
            "exclude",
        ])
        .unwrap();
        let selected =
            service_scope_from_cli(add.package_ids, add.exclude_package_ids, add.system_data);

        assert!(!selected.includes(&SYSTEM_PACKAGE_ID));
        assert!(selected.includes(&"bitcoind".parse().unwrap()));
        assert!(!selected.includes(&"lnd".parse().unwrap()));

        let edit = EditBackupJobCliParams::try_parse_from([
            "test",
            BackupJobId::new().to_string().as_str(),
            "--system-data",
            "include",
        ])
        .unwrap();
        assert_eq!(edit.system_data, Some(SystemDataSelection::Include));

        let included = with_system_data_selection(selected, edit.system_data.unwrap());
        assert!(included.includes(&SYSTEM_PACKAGE_ID));
        assert!(!included.includes(&"lnd".parse().unwrap()));
    }

    #[test]
    fn policy_cli_requires_an_explicit_retention_policy() {
        let target = "cifs-0";
        let package = "bitcoind";
        assert!(
            PreviewRetentionPolicyCliParams::try_parse_from(["test", target, package]).is_err()
        );

        let params = PreviewRetentionPolicyCliParams::try_parse_from([
            "test",
            target,
            package,
            "--keep-rule",
            "1h:1d",
            "--keep-rule",
            "1d:1w",
        ])
        .unwrap();
        assert_eq!(params.retention_tiers.len(), 2);
        assert!(!params.latest_only);

        let params = ApplyRetentionPolicyCliParams::try_parse_from([
            "test",
            target,
            package,
            "--latest-only",
            "--confirm-removal",
            ServiceSnapshotId::new().to_string().as_str(),
        ])
        .unwrap();
        assert!(params.latest_only);
        assert_eq!(params.confirmed_removals.len(), 1);
    }

    #[test]
    fn capacity_estimate_cli_accepts_service_scope_and_retention() {
        let params = EstimateBackupCapacityCliParams::try_parse_from([
            "test",
            "cifs-0",
            "--package-ids",
            "bitcoind,lnd",
            "--keep-rule",
            "1h:1d",
            "--service-latest-only",
            "lnd",
        ])
        .unwrap();

        assert_eq!(params.package_ids.len(), 2);
        assert!(params.exclude_package_ids.is_empty());
        assert_eq!(params.retention_tiers.len(), 1);
        assert_eq!(params.latest_only_overrides.len(), 1);
    }

    #[test]
    fn retention_override_cli_groups_tiers_and_rejects_conflicts() {
        let first = parse_retention_override_tier("bitcoind=1h:1d").unwrap();
        let second = parse_retention_override_tier("bitcoind=1d:1w").unwrap();
        let lnd: PackageId = "lnd".parse().unwrap();
        let overrides =
            retention_overrides_from_cli(vec![first.clone(), second], vec![lnd.clone()]).unwrap();

        assert_eq!(overrides[&first.0].tiers.len(), 2);
        assert_eq!(overrides[&lnd], RetentionPolicy::latest_only());
        assert!(
            retention_overrides_from_cli(vec![first], vec!["bitcoind".parse().unwrap()]).is_err()
        );
    }

    #[test]
    fn checkpoint_selection_accepts_package_and_snapshot_ids() {
        let snapshot_id = ServiceSnapshotId::new();
        let (package_id, parsed_snapshot) =
            parse_checkpoint_selection(&format!("bitcoind={snapshot_id}")).unwrap();

        assert_eq!(package_id, "bitcoind".parse().unwrap());
        assert_eq!(parsed_snapshot, snapshot_id);
    }

    #[test]
    fn review_decision_accepts_add_and_skip_actions() {
        let job_id = BackupJobId::new();
        assert_eq!(
            parse_review_decision(&format!("{job_id}=add")).unwrap(),
            (job_id.clone(), true)
        );
        assert_eq!(
            parse_review_decision(&format!("{job_id}=skip")).unwrap(),
            (job_id, false)
        );
    }

    #[test]
    fn review_cli_allows_dismissal_when_no_jobs_exist() {
        let params = ResolveBackupReviewCliParams::try_parse_from(["test", "hello-world"]).unwrap();

        assert!(params.decisions.is_empty());
    }
}
