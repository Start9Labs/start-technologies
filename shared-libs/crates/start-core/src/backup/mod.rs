use std::collections::BTreeMap;

use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::PackageId;
use crate::context::CliContext;
#[allow(unused_imports)]
use crate::prelude::*;

pub mod backup_bulk;
pub mod os;
pub mod restore;
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

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct PackageBackupReport {
    pub error: Option<String>,
    #[ts(type = "number")]
    pub duration_ms: u64,
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
