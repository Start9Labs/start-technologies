use std::collections::BTreeMap;

use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::PackageId;
use crate::context::CliContext;
#[allow(unused_imports)]
use crate::prelude::*;
use crate::util::serde::HandlerExtSerde;

pub mod backup_bulk;
pub mod os;
pub mod restore;
pub mod scheduled;
pub mod target;
pub mod trash;

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct BackupReport {
    server: ServerBackupReport,
    packages: BTreeMap<PackageId, PackageBackupReport>,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct ServerBackupReport {
    attempted: bool,
    error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct PackageBackupReport {
    pub error: Option<String>,
    #[ts(type = "number")]
    pub duration_ms: u64,
    #[ts(type = "number | null")]
    pub logical_size: Option<u64>,
    #[ts(type = "number | null")]
    pub physical_size: Option<u64>,
    #[ts(type = "number | null")]
    pub changed_bytes: Option<u64>,
    #[ts(type = "string | null")]
    pub measured_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PackageBackupOutput {
    #[ts(type = "number | null")]
    pub changed_bytes: Option<u64>,
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

// #[command(subcommands(backup_bulk::backup_all, target::target))]
pub fn backup<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
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
                .with_display_serializable()
                .with_about("about.estimate-backup-capacity")
                .with_call_remote::<CliContext>(),
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
            "history",
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
            from_fn_async(restore::restore_packages_rpc)
                .no_display()
                .with_about("about.restore-packages-from-backup")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "restore-checkpoint",
            from_fn_async(scheduled::restore_automatic_checkpoint_cli)
                .no_display()
                .with_about("about.restore-automatic-backup-checkpoints")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "restore-mixed",
            from_fn_async(restore::restore_selection_cli)
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
mod tests {
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
