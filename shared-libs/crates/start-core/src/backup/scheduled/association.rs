use std::collections::BTreeSet;

use super::{BackupJob, BackupJobId, RetentionPolicy, ServiceSnapshot, ServiceTargetHistory};
use crate::PackageId;
use crate::backup::target::BackupTargetId;
use crate::db::model::DatabaseModel;
use crate::prelude::*;

pub(super) fn history_owns_retention_settings(history: &ServiceTargetHistory) -> bool {
    !history.snapshots.is_empty() || !history.feeding_jobs.is_empty()
}

fn history_is_archived(feeding_jobs: &BTreeSet<BackupJobId>) -> bool {
    feeding_jobs.is_empty()
}

fn adopt_new_job_settings(
    history: &mut ServiceTargetHistory,
    job: &BackupJob,
    policy: &RetentionPolicy,
) {
    if history_owns_retention_settings(history) {
        return;
    }
    history.target_instance_id = job.target_instance_id.clone();
    history.timezone = job.schedule.timezone.clone();
    history.policy = policy.clone();
}

pub(crate) fn associated_service_ids(
    db: &DatabaseModel,
    job: &BackupJob,
) -> Result<BTreeSet<PackageId>, Error> {
    db.as_public()
        .as_scheduled_backups()
        .as_histories()
        .as_entries()?
        .into_iter()
        .map(|(_, history)| history.de())
        .collect::<Result<Vec<ServiceTargetHistory>, Error>>()
        .map(|histories| associated_service_ids_from_histories(job, histories))
}

fn associated_service_ids_from_histories(
    job: &BackupJob,
    histories: impl IntoIterator<Item = ServiceTargetHistory>,
) -> BTreeSet<PackageId> {
    histories
        .into_iter()
        .filter(|history| {
            history.target_id == job.target_id && history.feeding_jobs.contains(&job.id)
        })
        .map(|history| history.package_id)
        .collect()
}

pub(crate) fn associate_histories(
    db: &mut DatabaseModel,
    job: &BackupJob,
    package_ids: &BTreeSet<PackageId>,
) -> Result<(), Error> {
    let histories = db
        .as_public_mut()
        .as_scheduled_backups_mut()
        .as_histories_mut();
    for package_id in package_ids {
        let key = history_key(&job.target_id, package_id);
        let policy = job
            .retention_overrides
            .get(package_id)
            .unwrap_or(&job.default_retention)
            .clone();
        if let Some(history) = histories.as_idx(&key) {
            let mut history: ServiceTargetHistory = history.de()?;
            adopt_new_job_settings(&mut history, job, &policy);
            history.feeding_jobs.insert(job.id.clone());
            history.archived = false;
            histories.insert(&key, &history)?;
        } else {
            histories.insert(
                &key,
                &ServiceTargetHistory {
                    target_id: job.target_id.clone(),
                    target_instance_id: job.target_instance_id.clone(),
                    package_id: package_id.clone(),
                    timezone: job.schedule.timezone.clone(),
                    policy,
                    feeding_jobs: BTreeSet::from([job.id.clone()]),
                    snapshots: Vec::new(),
                    archived: false,
                },
            )?;
        }
    }
    Ok(())
}

pub(crate) fn disassociate_histories(
    db: &mut DatabaseModel,
    job: &BackupJob,
    package_ids: &BTreeSet<PackageId>,
) -> Result<(), Error> {
    let histories = db
        .as_public_mut()
        .as_scheduled_backups_mut()
        .as_histories_mut();
    for package_id in package_ids {
        if let Some(history) = histories.as_idx_mut(&history_key(&job.target_id, package_id)) {
            history
                .as_feeding_jobs_mut()
                .mutate(|jobs| Ok(jobs.remove(&job.id)))?;
            if history.as_feeding_jobs().de()?.is_empty() {
                history.as_archived_mut().ser(&true)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn refresh_archive_state(
    db: &mut DatabaseModel,
    target_id: &BackupTargetId,
) -> Result<(), Error> {
    let histories = db
        .as_public_mut()
        .as_scheduled_backups_mut()
        .as_histories_mut();
    for key in histories.keys()? {
        let history = histories.as_idx_mut(&key).expect("history key exists");
        if history.as_target_id().de()? != *target_id {
            continue;
        }
        let feeding_jobs: BTreeSet<BackupJobId> = history.as_feeding_jobs().de()?;
        let archived = history_is_archived(&feeding_jobs);
        history.as_archived_mut().ser(&archived)?;
        if archived {
            let mut snapshots: Vec<ServiceSnapshot> = history.as_snapshots().de()?;
            for snapshot in &mut snapshots {
                snapshot.archived = true;
            }
            history.as_snapshots_mut().ser(&snapshots)?;
        }
    }
    Ok(())
}

pub fn history_key(target_id: &BackupTargetId, package_id: &PackageId) -> String {
    format!("{target_id}::{package_id}")
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::backup::scheduled::{BackupJobStatus, BackupServiceScope, Schedule};

    fn backup_job(id: BackupJobId) -> BackupJob {
        let now = Utc::now();
        BackupJob {
            id,
            name: "Daily".to_owned(),
            enabled: true,
            pause: None,
            target_id: "cifs-0".parse().unwrap(),
            target_instance_id: "instance".to_owned(),
            services: BackupServiceScope::All,
            schedule: Schedule::new("0 3 * * *", "UTC").unwrap(),
            default_retention: RetentionPolicy::latest_only(),
            retention_overrides: Default::default(),
            status: BackupJobStatus::default(),
            created_at: now,
            updated_at: now,
        }
    }

    fn service_history(
        package_id: &str,
        feeding_jobs: BTreeSet<BackupJobId>,
    ) -> ServiceTargetHistory {
        ServiceTargetHistory {
            target_id: "cifs-0".parse().unwrap(),
            target_instance_id: "instance".to_owned(),
            package_id: package_id.parse().unwrap(),
            timezone: "UTC".to_owned(),
            policy: RetentionPolicy::latest_only(),
            feeding_jobs,
            snapshots: Vec::new(),
            archived: false,
        }
    }

    #[test]
    fn associated_histories_include_services_absent_from_package_data() {
        let id = BackupJobId::new();
        let job = backup_job(id.clone());
        let histories = [
            service_history("installed", BTreeSet::from([id.clone()])),
            service_history("uninstalled", BTreeSet::from([id.clone()])),
            service_history("other-job", BTreeSet::from([BackupJobId::new()])),
        ];

        let package_ids = associated_service_ids_from_histories(&job, histories);

        assert_eq!(
            package_ids,
            BTreeSet::from(["installed".parse().unwrap(), "uninstalled".parse().unwrap(),])
        );
    }

    #[test]
    fn orphaned_empty_history_adopts_new_retention_settings() {
        let new_policy = RetentionPolicy {
            tiers: vec![super::super::RetentionTier {
                interval_seconds: 24 * 60 * 60,
                coverage_seconds: 7 * 24 * 60 * 60,
            }],
        };
        let mut history = service_history("docuseal", BTreeSet::new());
        history.target_instance_id = "old-instance".to_owned();
        history.timezone = "America/New_York".to_owned();
        let mut job = backup_job(BackupJobId::new());
        job.target_instance_id = "new-instance".to_owned();

        adopt_new_job_settings(&mut history, &job, &new_policy);

        assert_eq!(history.target_instance_id, "new-instance");
        assert_eq!(history.timezone, "UTC");
        assert_eq!(history.policy, new_policy);
    }

    #[test]
    fn associated_history_preserves_retention_settings() {
        let old_policy = RetentionPolicy {
            tiers: vec![super::super::RetentionTier {
                interval_seconds: 60 * 60,
                coverage_seconds: 7 * 24 * 60 * 60,
            }],
        };
        let new_policy = RetentionPolicy {
            tiers: vec![super::super::RetentionTier {
                interval_seconds: 24 * 60 * 60,
                coverage_seconds: 7 * 24 * 60 * 60,
            }],
        };
        let mut history = service_history("docuseal", BTreeSet::from([BackupJobId::new()]));
        history.target_instance_id = "old-instance".to_owned();
        history.timezone = "America/New_York".to_owned();
        history.policy = old_policy.clone();
        let mut job = backup_job(BackupJobId::new());
        job.target_instance_id = "new-instance".to_owned();

        adopt_new_job_settings(&mut history, &job, &new_policy);

        assert_eq!(history.target_instance_id, "old-instance");
        assert_eq!(history.timezone, "America/New_York");
        assert_eq!(history.policy, old_policy);
    }

    #[test]
    fn referenced_histories_remain_active_while_jobs_are_paused() {
        assert!(!history_is_archived(&BTreeSet::from([BackupJobId::new()])));
        assert!(history_is_archived(&BTreeSet::new()));
    }
}
