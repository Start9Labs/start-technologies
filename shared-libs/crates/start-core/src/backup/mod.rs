use std::collections::BTreeMap;

use itertools::Itertools;
use rpc_toolkit::{Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::PackageId;
use crate::context::{CliContext, RpcContext};
#[allow(unused_imports)]
use crate::prelude::*;
use crate::util::serde::HandlerExtSerde;

pub mod backup_bulk;
pub mod history;
pub mod os;
pub mod restore;
pub mod scheduled;
pub mod target;
pub mod trash;
pub mod volume;

#[derive(Debug, Deserialize, Serialize, VisitFields)]
pub struct BackupReport {
    server: ServerBackupReport,
    packages: BTreeMap<PackageId, PackageBackupReport>,
    #[serde(default, rename = "sharedVolumes")]
    shared_volumes: BTreeMap<crate::shared_volumes::SharedVolumeId, VolumeBackupReport>,
}

impl BackupReport {
    fn has_failures(&self) -> bool {
        self.server.error.is_some()
            || self.packages.values().any(|item| item.error.is_some())
            || self
                .shared_volumes
                .values()
                .any(|item| item.result.error.is_some())
    }
}

#[derive(Debug, Deserialize, Serialize, VisitFields)]
pub struct VolumeBackupReport {
    pub name: String,
    #[serde(flatten)]
    pub result: PackageBackupReport,
}

rpc_toolkit::reflect_ts!(VolumeBackupReport);

rpc_toolkit::reflect_ts!(BackupReport);
rpc_toolkit::ts_export!(BackupReport, namespaces = [""]);

#[derive(Debug, Deserialize, Serialize, VisitFields)]
pub struct ServerBackupReport {
    attempted: bool,
    error: Option<String>,
}

rpc_toolkit::reflect_ts!(ServerBackupReport);
rpc_toolkit::ts_export!(ServerBackupReport, namespaces = [""]);

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, VisitFields)]
pub struct PackageBackupReport {
    pub error: Option<String>,
    pub duration_ms: u64,

    #[serde(default)]
    pub logical_size: Option<u64>,

    #[serde(default)]
    pub physical_size: Option<u64>,

    #[serde(default)]
    pub changed_bytes: Option<u64>,

    #[serde(default)]
    pub measured_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct PackageBackupOutput {
    #[serde(default)]
    pub changed_bytes: Option<u64>,
}

rpc_toolkit::reflect_ts!(PackageBackupOutput);
rpc_toolkit::ts_export!(PackageBackupOutput, namespaces = [""]);

fn record_package_backup(
    db: &mut crate::db::model::DatabaseModel,
    id: &PackageId,
    timestamp: chrono::DateTime<chrono::Utc>,
) -> Result<(), Error> {
    if let Some(package) = db.as_public_mut().as_package_data_mut().as_idx_mut(id) {
        package.as_last_backup_mut().ser(&Some(timestamp))?;
    }
    Ok(())
}

pub(crate) fn try_backup_coordinator(
    coordinator: std::sync::Arc<tokio::sync::Mutex<()>>,
) -> Result<tokio::sync::OwnedMutexGuard<()>, Error> {
    coordinator
        .try_lock_owned()
        .map_err(|_| backup_in_progress_error())
}

pub(crate) fn backup_in_progress_error() -> Error {
    Error::new(
        eyre!("{}", t!("backup.bulk.already-backing-up")),
        ErrorKind::InvalidRequest,
    )
}

rpc_toolkit::reflect_ts!(PackageBackupReport);
rpc_toolkit::ts_export!(PackageBackupReport, namespaces = [""]);

async fn call_remote<P: Serialize + Send + Sync>(
    args: HandlerArgs<CliContext, P>,
) -> Result<Value, Error> {
    let method = args.parent_method.into_iter().chain(args.method).join(".");
    args.context
        .call_remote::<RpcContext>(&method, to_value(&args.params)?)
        .await
}

fn read_password() -> Result<String, Error> {
    crate::util::tui::read_password("BACKUP_PASSWORD", &t!("help.arg.backup-password"))
}

pub fn backup<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("history", history::handler::<C>().no_cli())
        .subcommand(
            "create",
            from_fn_async(backup_bulk::backup_all)
                .no_display()
                .with_about("about.create-backup-all-packages")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "estimate-capacity",
            from_fn_async(scheduled::estimate_capacity_cli)
                .no_ts()
                .with_display_serializable()
                .with_about("about.estimate-backup-capacity")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "volume",
            ParentHandler::<C>::new()
                .subcommand(
                    "restore",
                    from_fn_async(volume::restore_volumes_rpc)
                        .with_metadata("sync_db", Value::Bool(true))
                        .with_about("about.restore-volumes-from-backup")
                        .no_cli(),
                )
                .subcommand(
                    "restore",
                    from_fn_async(volume::cli_restore_volumes)
                        .no_ts()
                        .with_display_serializable()
                        .with_about("about.restore-volumes-from-backup"),
                )
                .with_about("about.commands-volume-backups"),
        )
        .subcommand(
            "target",
            target::target::<C>().with_about("about.commands-backup-target"),
        )
        .subcommand(
            "targets",
            from_fn_async(target::list)
                .with_display_serializable()
                .with_about("about.list-existing-backup-targets")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "job",
            scheduled::job::<C>().with_about("about.commands-automatic-backup-jobs"),
        )
        .subcommand(
            "activity",
            scheduled::activity::<C>().with_about("about.commands-backup-activity"),
        )
        .subcommand(
            "checkpoint",
            scheduled::history::<C>().with_about("about.commands-backup-history"),
        )
        .subcommand(
            "policy",
            scheduled::policy::<C>().with_about("about.commands-backup-policy"),
        )
        .subcommand(
            "review",
            scheduled::review::<C>().with_about("about.commands-backup-review"),
        )
}

pub fn package_backup<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "restore",
            from_fn_async(restore::restore_packages_rpc).no_cli(),
        )
        .subcommand(
            "restore",
            from_fn_async(restore::cli_restore_packages)
                .no_ts()
                .no_display()
                .with_about("about.restore-packages-from-backup"),
        )
        .subcommand(
            "restore-checkpoint",
            from_fn_async(scheduled::restore_automatic_checkpoint_cli)
                .no_ts()
                .no_display()
                .with_about("about.restore-automatic-backup-checkpoints")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "restore-mixed",
            from_fn_async(restore::restore_selection_cli)
                .no_ts()
                .no_display()
                .with_about("about.restore-mixed-backup-checkpoints")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "restore-scheduled",
            from_fn_async(restore::restore_scheduled_packages_rpc).no_cli(),
        )
        .subcommand(
            "restore-selection",
            from_fn_async(restore::restore_selection_rpc).no_cli(),
        )
}

#[cfg(test)]
mod coordinator_tests {
    use super::*;

    #[test]
    fn backup_coordinator_rejects_a_second_request() {
        let coordinator = std::sync::Arc::new(tokio::sync::Mutex::new(()));
        let first = try_backup_coordinator(coordinator.clone()).unwrap();

        let error = try_backup_coordinator(coordinator.clone()).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidRequest);

        drop(first);
        assert!(try_backup_coordinator(coordinator).is_ok());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_reports_default_to_no_volume_items() {
        let report: BackupReport = serde_json::from_value(serde_json::json!({
            "server": { "attempted": true, "error": null }, "packages": {}
        }))
        .unwrap();
        assert!(report.shared_volumes.is_empty());
        assert!(!report.has_failures());
    }

    #[test]
    fn volume_errors_make_the_bulk_report_unsuccessful() {
        let id = crate::shared_volumes::SharedVolumeId::new();
        let mut report = BackupReport {
            server: ServerBackupReport {
                attempted: true,
                error: None,
            },
            packages: BTreeMap::new(),
            shared_volumes: BTreeMap::from([(
                id.clone(),
                VolumeBackupReport {
                    name: "Photos".into(),
                    result: PackageBackupReport {
                        error: Some("offline".into()),
                        duration_ms: 12,
                        ..Default::default()
                    },
                },
            )]),
        };
        assert!(report.has_failures());
        let wire = serde_json::to_value(&report).unwrap();
        assert_eq!(wire["sharedVolumes"][id.as_ref()]["error"], "offline");
        assert_eq!(wire["sharedVolumes"][id.as_ref()]["duration_ms"], 12);
        assert!(
            serde_json::from_value::<BackupReport>(wire)
                .unwrap()
                .has_failures()
        );
        report.shared_volumes.get_mut(&id).unwrap().result.error = None;
        assert!(!report.has_failures());
    }
}
