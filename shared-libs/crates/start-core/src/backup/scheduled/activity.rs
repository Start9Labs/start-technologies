use std::collections::{BTreeMap, BTreeSet};

use chrono::Utc;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};

use super::{
    BackupActivity, BackupActivityId, BackupActivityKind, BackupJob, BackupRun, BackupRunState,
    BackupRunTrigger, completed_activity_overflow, completed_run_overflow,
};
use crate::PackageId;
use crate::backup::PackageBackupReport;
use crate::backup::target::BackupTargetId;
use crate::context::RpcContext;
use crate::db::model::DatabaseModel;
use crate::prelude::*;
use crate::util::serde::HandlerExtSerde;

pub fn activity<C: Context>() -> ParentHandler<C> {
    ParentHandler::new().subcommand(
        "list",
        from_fn_async(list)
            .with_display_serializable()
            .with_about("about.list-backup-activity")
            .with_call_remote::<crate::context::CliContext>(),
    )
}

/// Newest first.
pub async fn list(ctx: RpcContext) -> Result<Vec<BackupActivity>, Error> {
    let mut activities = ctx
        .db
        .peek()
        .await
        .as_public()
        .as_scheduled_backups()
        .as_activities()
        .as_entries()?
        .into_iter()
        .map(|(_, activity)| activity.de())
        .collect::<Result<Vec<BackupActivity>, Error>>()?;
    activities.sort_by_key(|activity| std::cmp::Reverse(activity.started_at));
    Ok(activities)
}

pub(crate) fn running(
    kind: BackupActivityKind,
    target_id: BackupTargetId,
    source_server_id: Option<String>,
    job: Option<&BackupJob>,
    trigger: Option<BackupRunTrigger>,
    intended_services: BTreeSet<PackageId>,
) -> BackupActivity {
    BackupActivity {
        id: BackupActivityId::new(),
        kind,
        state: BackupRunState::Running,
        target_id,
        source_server_id,
        job_id: job.map(|job| job.id.clone()),
        job_name: job.map(|job| job.name.clone()),
        trigger,
        started_at: Utc::now(),
        completed_at: None,
        intended_services,
        services: BTreeMap::new(),
        error: None,
    }
}

pub(crate) fn from_run(run: &BackupRun) -> BackupActivity {
    BackupActivity {
        id: run.id.clone(),
        kind: BackupActivityKind::Automatic,
        state: run.state,
        target_id: run.target_id.clone(),
        source_server_id: None,
        job_id: Some(run.job_id.clone()),
        job_name: Some(run.job_name.clone()),
        trigger: Some(run.trigger),
        started_at: run.started_at,
        completed_at: run.completed_at,
        intended_services: run.intended_services.clone(),
        services: run.services.clone(),
        error: run.error.clone(),
    }
}

pub(crate) fn insert(db: &mut DatabaseModel, activity: &BackupActivity) -> Result<(), Error> {
    db.as_public_mut()
        .as_scheduled_backups_mut()
        .as_activities_mut()
        .insert(&activity.id, activity)
        .map(|_| ())
}

pub(crate) fn prune_completed_history(db: &mut DatabaseModel) -> Result<usize, Error> {
    let scheduled = db.as_public_mut().as_scheduled_backups_mut();
    let activities = scheduled
        .as_activities()
        .as_entries()?
        .into_iter()
        .map(|(_, activity)| activity.de())
        .collect::<Result<Vec<BackupActivity>, Error>>()?;
    let runs = scheduled
        .as_runs()
        .as_entries()?
        .into_iter()
        .map(|(_, run)| run.de())
        .collect::<Result<Vec<BackupRun>, Error>>()?;
    let activity_ids = completed_activity_overflow(&activities);
    let run_ids = completed_run_overflow(&runs);
    let removed = activity_ids.len() + run_ids.len();

    for id in activity_ids {
        scheduled.as_activities_mut().remove(&id)?;
    }
    for id in run_ids {
        scheduled.as_runs_mut().remove(&id)?;
    }

    Ok(removed)
}

pub(crate) fn complete(
    db: &mut DatabaseModel,
    id: &BackupActivityId,
    state: BackupRunState,
    services: BTreeMap<PackageId, PackageBackupReport>,
    error: Option<String>,
) -> Result<(), Error> {
    let activity = db
        .as_public_mut()
        .as_scheduled_backups_mut()
        .as_activities_mut()
        .as_idx_mut(id)
        .or_not_found(id)?;
    activity.as_state_mut().ser(&state)?;
    activity.as_services_mut().ser(&services)?;
    activity.as_error_mut().ser(&error)?;
    activity.as_completed_at_mut().ser(&Some(Utc::now()))?;
    prune_completed_history(db)?;
    Ok(())
}
