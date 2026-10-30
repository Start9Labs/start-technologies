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
pub mod os;
pub mod restore;
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

#[derive(Debug, Deserialize, Serialize, VisitFields)]
pub struct PackageBackupReport {
    pub error: Option<String>,
    pub duration_ms: u64,
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
        .subcommand(
            "create",
            from_fn_async(backup_bulk::backup_all)
                .no_display()
                .with_about("about.create-backup-all-packages")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "volume",
            ParentHandler::<C>::new()
                .subcommand(
                    "restore",
                    from_fn_async(volume::restore_volumes_rpc)
                        .with_metadata("sync_db", Value::Bool(true))
                        .with_display_serializable()
                        .with_about("about.restore-volumes-from-backup")
                        .with_call_remote::<CliContext>(),
                )
                .with_about("about.commands-volume-backups"),
        )
        .subcommand(
            "target",
            target::target::<C>().with_about("about.commands-backup-target"),
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
