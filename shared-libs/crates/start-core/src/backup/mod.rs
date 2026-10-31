use std::collections::BTreeMap;

use itertools::Itertools;
use rpc_toolkit::{Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::PackageId;
use crate::context::{CliContext, RpcContext};
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
