use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use patch_db::HasModel;
use serde::{Deserialize, Serialize};

use super::{RetentionPolicy, Schedule};
use crate::backup::PackageBackupReport;
use crate::backup::target::BackupTargetId;
use crate::prelude::{Model, VisitFields, VisitVariants};
use crate::rpc_continuations::Guid;
use crate::{PackageId, SYSTEM_PACKAGE_ID};

pub type BackupJobId = Guid;
pub type BackupRunId = Guid;
pub type BackupActivityId = Guid;
pub type ServiceSnapshotId = Guid;

pub(crate) const MAX_COMPLETED_HISTORY_ITEMS: usize = 1_000;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, VisitVariants)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum BackupServiceScope {
    All,
    AllExcept {
        #[serde(rename = "excludedPackageIds")]
        excluded_package_ids: BTreeSet<PackageId>,
    },
    Selected {
        #[serde(rename = "packageIds")]
        package_ids: BTreeSet<PackageId>,
        #[serde(
            rename = "includeSystem",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        include_system: Option<bool>,
    },
}

rpc_toolkit::reflect_ts!(BackupServiceScope);
rpc_toolkit::ts_export!(BackupServiceScope, namespaces = [""]);

impl BackupServiceScope {
    pub fn includes(&self, package_id: &PackageId) -> bool {
        if package_id == &*SYSTEM_PACKAGE_ID {
            return match self {
                Self::All => true,
                Self::AllExcept {
                    excluded_package_ids,
                } => !excluded_package_ids.contains(package_id),
                Self::Selected { include_system, .. } => include_system.unwrap_or(true),
            };
        }
        match self {
            Self::All => true,
            Self::AllExcept {
                excluded_package_ids,
            } => !excluded_package_ids.contains(package_id),
            Self::Selected { package_ids, .. } => package_ids.contains(package_id),
        }
    }

    pub(crate) fn configured_services(
        &self,
        installed: BTreeSet<PackageId>,
    ) -> BTreeSet<PackageId> {
        let mut selected = match self {
            Self::All => installed,
            Self::AllExcept {
                excluded_package_ids,
            } => installed
                .into_iter()
                .filter(|id| !excluded_package_ids.contains(id))
                .collect(),
            Self::Selected { package_ids, .. } => package_ids
                .iter()
                .filter(|id| *id != &*SYSTEM_PACKAGE_ID)
                .cloned()
                .collect(),
        };
        if self.includes(&SYSTEM_PACKAGE_ID) {
            selected.insert(SYSTEM_PACKAGE_ID.clone());
        }
        selected
    }

    pub(crate) fn runnable_services(
        &self,
        mut installed: BTreeSet<PackageId>,
    ) -> BTreeSet<PackageId> {
        installed.insert(SYSTEM_PACKAGE_ID.clone());
        self.configured_services(installed.clone())
            .intersection(&installed)
            .cloned()
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, VisitVariants)]
#[serde(rename_all = "camelCase", tag = "reason")]
pub enum BackupJobPause {
    User,
    TargetUnavailable { failures: u8 },
    TargetIdentityMismatch,
    TargetUnreadable,
    ReauthenticationRequired,
}

rpc_toolkit::reflect_ts!(BackupJobPause);
rpc_toolkit::ts_export!(BackupJobPause, namespaces = [""]);

impl BackupJobPause {
    pub(crate) fn requires_target_retry(&self) -> bool {
        !matches!(self, Self::User)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct BackupJobStatus {
    pub last_scheduled_at: Option<DateTime<Utc>>,

    pub last_attempted_at: Option<DateTime<Utc>>,

    pub last_succeeded_at: Option<DateTime<Utc>>,

    pub next_run_at: Option<DateTime<Utc>>,
    /// A durable one-shot request consumed once the backup coordinator is free.
    #[serde(default)]
    pub run_requested: bool,
    pub consecutive_failures: u8,
    pub last_result: Option<BackupRunState>,
}

rpc_toolkit::reflect_ts!(BackupJobStatus);
rpc_toolkit::ts_export!(BackupJobStatus, namespaces = [""]);

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct BackupJob {
    pub id: BackupJobId,
    pub name: String,
    pub enabled: bool,
    pub pause: Option<BackupJobPause>,
    pub target_id: BackupTargetId,
    pub target_instance_id: String,
    pub services: BackupServiceScope,
    pub schedule: Schedule,
    pub default_retention: RetentionPolicy,
    pub retention_overrides: BTreeMap<PackageId, RetentionPolicy>,
    pub status: BackupJobStatus,

    pub created_at: DateTime<Utc>,

    pub updated_at: DateTime<Utc>,
}

rpc_toolkit::reflect_ts!(BackupJob);
rpc_toolkit::ts_export!(BackupJob, namespaces = [""]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum BackupRunTrigger {
    Scheduled,
    CatchUp,
    RunNow,
}

rpc_toolkit::reflect_ts!(BackupRunTrigger);
rpc_toolkit::ts_export!(BackupRunTrigger, namespaces = [""]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum BackupRunState {
    Running,
    Succeeded,
    PartiallyFailed,
    Failed,
}

rpc_toolkit::reflect_ts!(BackupRunState);
rpc_toolkit::ts_export!(BackupRunState, namespaces = [""]);

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct BackupRun {
    pub id: BackupRunId,
    pub job_id: BackupJobId,
    pub job_name: String,
    pub target_id: BackupTargetId,
    pub trigger: BackupRunTrigger,
    pub state: BackupRunState,

    pub started_at: DateTime<Utc>,

    pub completed_at: Option<DateTime<Utc>>,
    pub intended_services: BTreeSet<PackageId>,
    pub services: BTreeMap<PackageId, PackageBackupReport>,
    pub error: Option<String>,
}

rpc_toolkit::reflect_ts!(BackupRun);
rpc_toolkit::ts_export!(BackupRun, namespaces = [""]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum BackupActivityKind {
    Manual,
    Automatic,
    Restore,
}

rpc_toolkit::reflect_ts!(BackupActivityKind);
rpc_toolkit::ts_export!(BackupActivityKind, namespaces = [""]);

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct BackupActivity {
    pub id: BackupActivityId,
    pub kind: BackupActivityKind,
    pub state: BackupRunState,
    pub target_id: BackupTargetId,
    pub source_server_id: Option<String>,
    pub job_id: Option<BackupJobId>,
    pub job_name: Option<String>,
    pub trigger: Option<BackupRunTrigger>,

    pub started_at: DateTime<Utc>,

    pub completed_at: Option<DateTime<Utc>>,
    pub intended_services: BTreeSet<PackageId>,
    pub services: BTreeMap<PackageId, PackageBackupReport>,
    pub error: Option<String>,
}

rpc_toolkit::reflect_ts!(BackupActivity);
rpc_toolkit::ts_export!(BackupActivity, namespaces = [""]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum BackupSource {
    Manual,
    Scheduled,
}

rpc_toolkit::reflect_ts!(BackupSource);
rpc_toolkit::ts_export!(BackupSource, namespaces = [""]);

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSnapshot {
    pub id: ServiceSnapshotId,
    pub package_id: PackageId,
    pub package_version: String,
    pub source: BackupSource,
    pub job_id: BackupJobId,
    pub job_name: String,
    pub run_id: BackupRunId,

    pub completed_at: DateTime<Utc>,

    pub logical_size: u64,

    pub physical_size: Option<u64>,

    pub changed_bytes: Option<u64>,

    pub measured_at: DateTime<Utc>,
    pub archived: bool,
}

rpc_toolkit::reflect_ts!(ServiceSnapshot);
rpc_toolkit::ts_export!(ServiceSnapshot, namespaces = [""]);

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct ServiceTargetRetentionPolicy {
    pub target_id: BackupTargetId,
    pub package_id: PackageId,
    /// Local timezone forming retention buckets.
    pub timezone: String,
    pub policy: RetentionPolicy,
    pub feeding_jobs: BTreeSet<BackupJobId>,
    pub archived: bool,
}

rpc_toolkit::reflect_ts!(ServiceTargetRetentionPolicy);
rpc_toolkit::ts_export!(ServiceTargetRetentionPolicy, namespaces = [""]);

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct ScheduledBackupState {
    pub jobs: BTreeMap<BackupJobId, BackupJob>,
    pub histories: BTreeMap<String, ServiceTargetHistory>,
    pub runs: BTreeMap<BackupRunId, BackupRun>,
    #[serde(default)]
    pub activities: BTreeMap<BackupActivityId, BackupActivity>,
    pub target_failures: BTreeMap<String, BackupTargetFailureState>,
    pub pending_service_reviews: BTreeMap<PackageId, NewServiceBackupReview>,
}

rpc_toolkit::reflect_ts!(ScheduledBackupState);
rpc_toolkit::ts_export!(ScheduledBackupState, namespaces = [""]);

impl ScheduledBackupState {
    pub(crate) fn prune_completed_history(&mut self) -> usize {
        let activity_ids = completed_activity_overflow(self.activities.values());
        let run_ids = completed_run_overflow(self.runs.values());
        let removed = activity_ids.len() + run_ids.len();

        for id in activity_ids {
            self.activities.remove(&id);
        }
        for id in run_ids {
            self.runs.remove(&id);
        }

        removed
    }
}

pub(crate) fn completed_activity_overflow<'a>(
    activities: impl IntoIterator<Item = &'a BackupActivity>,
) -> Vec<BackupActivityId> {
    completed_history_overflow(activities.into_iter().filter_map(|activity| {
        (activity.state != BackupRunState::Running)
            .then_some((activity.started_at, activity.id.clone()))
    }))
}

pub(crate) fn completed_run_overflow<'a>(
    runs: impl IntoIterator<Item = &'a BackupRun>,
) -> Vec<BackupRunId> {
    completed_history_overflow(runs.into_iter().filter_map(|run| {
        (run.state != BackupRunState::Running).then_some((run.started_at, run.id.clone()))
    }))
}

fn completed_history_overflow(
    completed: impl IntoIterator<Item = (DateTime<Utc>, Guid)>,
) -> Vec<Guid> {
    let mut completed = completed.into_iter().collect::<Vec<_>>();
    completed.sort();
    let overflow = completed.len().saturating_sub(MAX_COMPLETED_HISTORY_ITEMS);
    completed
        .into_iter()
        .take(overflow)
        .map(|(_, id)| id)
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct ServiceTargetHistory {
    pub target_id: BackupTargetId,
    pub target_instance_id: String,
    pub package_id: PackageId,
    pub timezone: String,
    pub policy: RetentionPolicy,
    pub feeding_jobs: BTreeSet<BackupJobId>,
    pub snapshots: Vec<ServiceSnapshot>,
    pub archived: bool,
}

rpc_toolkit::reflect_ts!(ServiceTargetHistory);
rpc_toolkit::ts_export!(ServiceTargetHistory, namespaces = [""]);

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct BackupTargetFailureState {
    pub consecutive_connectivity_failures: u8,
    pub jobs_paused: BTreeSet<BackupJobId>,
    pub notification_sent: bool,
}

rpc_toolkit::reflect_ts!(BackupTargetFailureState);
rpc_toolkit::ts_export!(BackupTargetFailureState, namespaces = [""]);

impl BackupTargetFailureState {
    /// Returns true when this failure first requires intervention.
    pub fn record_failure(&mut self, affected_jobs: impl IntoIterator<Item = BackupJobId>) -> bool {
        self.consecutive_connectivity_failures =
            self.consecutive_connectivity_failures.saturating_add(1);
        if self.consecutive_connectivity_failures < 3 {
            return false;
        }
        self.jobs_paused.extend(affected_jobs);
        if self.notification_sent {
            false
        } else {
            self.notification_sent = true;
            true
        }
    }

    pub fn reset(&mut self) {
        self.consecutive_connectivity_failures = 0;
        self.jobs_paused.clear();
        self.notification_sent = false;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct NewServiceBackupReview {
    pub package_id: PackageId,
    pub affected_jobs: BTreeSet<BackupJobId>,

    pub created_at: DateTime<Utc>,
}

rpc_toolkit::reflect_ts!(NewServiceBackupReview);
rpc_toolkit::ts_export!(NewServiceBackupReview, namespaces = [""]);

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct RetentionPolicyChangePreview {
    pub removed: Vec<ServiceSnapshot>,

    pub estimated_reclaimed_bytes: u64,
    pub affected_jobs: Vec<String>,
}

rpc_toolkit::reflect_ts!(RetentionPolicyChangePreview);
rpc_toolkit::ts_export!(RetentionPolicyChangePreview, namespaces = [""]);

/// Device-sealed target encryption key.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledBackupCredential {
    pub target_instance_id: String,
    pub sealed_key: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduled_state_without_activity_history_migrates_safely() {
        let state: ScheduledBackupState = serde_json::from_value(serde_json::json!({
            "jobs": {},
            "histories": {},
            "runs": {},
            "targetFailures": {},
            "pendingServiceReviews": {}
        }))
        .unwrap();
        assert!(state.activities.is_empty());
    }

    #[test]
    fn completed_history_is_bounded_without_removing_running_entries() {
        let mut state = ScheduledBackupState::default();
        let target_id = BackupTargetId::Cifs { id: 1 };
        let job_id = BackupJobId::new();
        let mut oldest_completed_ids = Vec::new();

        for offset in 0..(MAX_COMPLETED_HISTORY_ITEMS + 2) {
            let started_at = DateTime::from_timestamp(offset as i64, 0).unwrap();
            let run = BackupRun {
                id: BackupRunId::new(),
                job_id: job_id.clone(),
                job_name: "Nightly".to_owned(),
                target_id: target_id.clone(),
                trigger: BackupRunTrigger::Scheduled,
                state: BackupRunState::Succeeded,
                started_at,
                completed_at: Some(started_at),
                intended_services: BTreeSet::new(),
                services: BTreeMap::new(),
                error: None,
            };
            if offset < 2 {
                oldest_completed_ids.push(run.id.clone());
            }
            state.activities.insert(
                run.id.clone(),
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
                },
            );
            state.runs.insert(run.id.clone(), run);
        }

        let running = BackupRun {
            id: BackupRunId::new(),
            job_id,
            job_name: "Nightly".to_owned(),
            target_id,
            trigger: BackupRunTrigger::RunNow,
            state: BackupRunState::Running,
            started_at: DateTime::from_timestamp(-1, 0).unwrap(),
            completed_at: None,
            intended_services: BTreeSet::new(),
            services: BTreeMap::new(),
            error: None,
        };
        let running_id = running.id.clone();
        state.activities.insert(
            running.id.clone(),
            BackupActivity {
                id: running.id.clone(),
                kind: BackupActivityKind::Automatic,
                state: running.state,
                target_id: running.target_id.clone(),
                source_server_id: None,
                job_id: Some(running.job_id.clone()),
                job_name: Some(running.job_name.clone()),
                trigger: Some(running.trigger),
                started_at: running.started_at,
                completed_at: None,
                intended_services: BTreeSet::new(),
                services: BTreeMap::new(),
                error: None,
            },
        );
        state.runs.insert(running.id.clone(), running);

        assert_eq!(state.prune_completed_history(), 4);
        assert_eq!(state.activities.len(), MAX_COMPLETED_HISTORY_ITEMS + 1);
        assert_eq!(state.runs.len(), MAX_COMPLETED_HISTORY_ITEMS + 1);
        assert!(state.activities.contains_key(&running_id));
        assert!(state.runs.contains_key(&running_id));
        for id in oldest_completed_ids {
            assert!(!state.activities.contains_key(&id));
            assert!(!state.runs.contains_key(&id));
        }
    }

    #[test]
    fn service_scope_preserves_legacy_and_exclusion_shapes() {
        let all: BackupServiceScope =
            serde_json::from_value(serde_json::json!({ "type": "all" })).unwrap();
        assert!(all.includes(&"bitcoind".parse().unwrap()));

        let selected: BackupServiceScope = serde_json::from_value(serde_json::json!({
            "type": "selected",
            "packageIds": ["bitcoind"]
        }))
        .unwrap();
        assert!(selected.includes(&"bitcoind".parse().unwrap()));
        assert!(!selected.includes(&"lnd".parse().unwrap()));
        assert!(selected.includes(&*SYSTEM_PACKAGE_ID));

        let all_except: BackupServiceScope = serde_json::from_value(serde_json::json!({
            "type": "allExcept",
            "excludedPackageIds": ["lnd"]
        }))
        .unwrap();
        assert!(all_except.includes(&"bitcoind".parse().unwrap()));
        assert!(!all_except.includes(&"lnd".parse().unwrap()));
        assert!(all_except.includes(&*SYSTEM_PACKAGE_ID));
    }

    #[test]
    fn service_scope_with_system_id_round_trips() {
        let scope = BackupServiceScope::Selected {
            package_ids: BTreeSet::from([SYSTEM_PACKAGE_ID.clone()]),
            include_system: None,
        };
        let encoded = serde_json::to_value(&scope).unwrap();

        assert_eq!(
            serde_json::from_value::<BackupServiceScope>(encoded).unwrap(),
            scope
        );
    }

    #[test]
    fn service_scope_allows_system_data_to_be_excluded() {
        let installed = BTreeSet::from(["hello-world".parse().unwrap()]);
        let selected: BackupServiceScope = serde_json::from_value(serde_json::json!({
            "type": "selected",
            "packageIds": ["hello-world", "x_system"],
            "includeSystem": false
        }))
        .unwrap();
        let excluded: BackupServiceScope = serde_json::from_value(serde_json::json!({
            "type": "allExcept",
            "excludedPackageIds": ["x_system"]
        }))
        .unwrap();

        assert!(!selected.includes(&SYSTEM_PACKAGE_ID));
        assert!(
            !selected
                .configured_services(installed.clone())
                .contains(&*SYSTEM_PACKAGE_ID)
        );
        assert!(
            !excluded
                .runnable_services(installed)
                .contains(&*SYSTEM_PACKAGE_ID)
        );
    }

    #[test]
    fn target_failure_threshold_notifies_and_pauses_once() {
        let job = BackupJobId::new();
        let mut state = BackupTargetFailureState::default();
        assert!(!state.record_failure([job.clone()]));
        assert!(!state.record_failure([job.clone()]));
        assert!(state.record_failure([job.clone()]));
        assert!(!state.record_failure([job.clone()]));
        assert!(state.jobs_paused.contains(&job));
        state.reset();
        assert_eq!(state, BackupTargetFailureState::default());
    }
}
