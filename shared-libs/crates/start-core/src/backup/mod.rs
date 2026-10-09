use std::collections::BTreeMap;

use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::PackageId;
use crate::context::CliContext;
#[allow(unused_imports)]
use crate::prelude::*;

pub mod backup_bulk;
pub mod os;
pub mod restore;
pub mod target;
pub mod trash;

#[derive(Debug, Deserialize, Serialize, VisitFields)]
pub struct BackupReport {
    server: ServerBackupReport,
    packages: BTreeMap<PackageId, PackageBackupReport>,
}

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
    #[visit(ts(type = "number"), wire = "rpc_toolkit::ts::Unknown")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub duration_ms: u64,
}

rpc_toolkit::reflect_ts!(PackageBackupReport);
rpc_toolkit::ts_export!(PackageBackupReport, namespaces = [""]);

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
            "target",
            target::target::<C>().with_about("about.commands-backup-target"),
        )
}

pub fn package_backup<C: Context>() -> ParentHandler<C> {
    ParentHandler::new().subcommand(
        "restore",
        from_fn_async(restore::restore_packages_rpc)
            .no_display()
            .with_about("about.restore-packages-from-backup")
            .with_call_remote::<CliContext>(),
    )
}
