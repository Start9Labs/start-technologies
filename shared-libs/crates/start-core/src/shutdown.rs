use std::time::Duration;

use clap::Parser;
use patch_db::json_ptr::JsonPointer;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::PLATFORM;
use crate::context::RpcContext;
use crate::db::model::DatabaseModel;
use crate::db::model::public::{PowerAction, ServerStatus};
use crate::disk::main::export;
use crate::init::{STANDBY_MODE_PATH, SYSTEM_REBUILD_PATH};
use crate::prelude::*;
use crate::sound::SHUTDOWN;
use crate::util::Invoke;

#[derive(Debug, Clone)]
pub struct Shutdown {
    pub disk_guid: Option<InternedString>,
    pub restart: bool,
}
impl Shutdown {
    /// BLOCKING
    pub fn execute(&self) {
        use std::process::Command;

        if self.restart {
            tracing::info!("{}", t!("shutdown.beginning-restart"));
        } else {
            tracing::info!("{}", t!("shutdown.beginning-shutdown"));
        }

        // When systemd is already driving the shutdown it stops journald, unmounts,
        // and issues the power action itself. Doing that work here — the
        // `systemctl stop systemd-journald` and volume-group export below — blocks
        // mid-transaction and hangs startd until its ~90s stop timeout, so exit now
        // and leave the rest to systemd. Exception: rpi poweroff must still
        // self-drive its standby reboot below (systemd can't power the Pi off).
        if systemd_is_stopping() && !(&*PLATFORM == "raspberrypi" && !self.restart) {
            tracing::info!(
                "systemd is already shutting down; exiting and leaving the power action to it"
            );
            std::process::exit(0);
        }

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            use tokio::process::Command;

            if let Err(e) = Command::new("systemctl")
                .arg("stop")
                .arg("systemd-journald")
                .invoke(crate::ErrorKind::Journald)
                .await
            {
                tracing::error!(
                    "{}",
                    t!("shutdown.error-stopping-journald", error = e.to_string())
                );
                tracing::debug!("{:?}", e);
            }
            if let Some(guid) = &self.disk_guid {
                if let Err(e) = export(guid, crate::DATA_DIR).await {
                    tracing::error!(
                        "{}",
                        t!(
                            "shutdown.error-exporting-volume-group",
                            error = e.to_string()
                        )
                    );
                    tracing::debug!("{:?}", e);
                }
            }
            if &*PLATFORM != "raspberrypi" || self.restart {
                if let Err(e) = SHUTDOWN.play().await {
                    tracing::error!(
                        "{}",
                        t!(
                            "shutdown.error-playing-shutdown-song",
                            error = e.to_string()
                        )
                    );
                    tracing::debug!("{:?}", e);
                }
            }
        });
        drop(rt);
        if &*PLATFORM == "raspberrypi" {
            // rpi has no real power-off: "off" is emulated as a standby reboot
            // (start_init parks on STANDBY_MODE_PATH on the way back up). A poweroff
            // self-drives the marker + reboot even under systemd, since systemd's own
            // poweroff would only halt the still-powered board. (A systemd-initiated
            // reboot already exited at the top.)
            if !self.restart {
                std::fs::write(STANDBY_MODE_PATH, "").unwrap();
                Command::new("sync").spawn().unwrap().wait().unwrap();
            }
            Command::new("reboot").spawn().unwrap().wait().unwrap();
        } else if self.restart {
            Command::new("reboot").spawn().unwrap().wait().unwrap();
        } else {
            Command::new("shutdown")
                .arg("now")
                .spawn()
                .unwrap()
                .wait()
                .unwrap();
        }
    }
}

/// True once systemd has entered its own shutdown transaction (`systemctl
/// is-system-running` == `stopping`). In that state the final reboot/poweroff is
/// systemd's to issue; [`Shutdown::execute`] must not race it with its own.
fn systemd_is_stopping() -> bool {
    std::process::Command::new("systemctl")
        .arg("is-system-running")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "stopping")
        .unwrap_or(false)
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ShutdownParams {
    /// Block until graceful teardown completes (default over the CLI; the
    /// frontend omits this and gets an immediate reply). Cleared with
    /// `--nowait`. The wait can't outlive the webserver teardown that follows
    /// container shutdown, so the connection drops once services are stopped.
    /// A deferred action replies immediately, before teardown begins.
    #[arg(long = "nowait", action = clap::ArgAction::SetFalse, help = "help.arg.nowait")]
    #[serde(default)]
    wait: bool,
    /// Interrupt a running backup instead of waiting for it to finish.
    #[arg(long, help = "help.arg.force-power")]
    #[serde(default)]
    force: bool,
}

pub(crate) const STATUS_INFO_PTR: &str = "/public/serverInfo/statusInfo";
/// How long to leave a failing patch-db alone before trying to take the
/// deferred action again.
const TAKE_RETRY: Duration = Duration::from_secs(30);

async fn begin_shutdown(ctx: &RpcContext, restart: bool, wait: bool) {
    ctx.shutdown
        .send(Some(Shutdown {
            disk_guid: Some(ctx.disk_guid.clone()),
            restart,
        }))
        .map_err(|_| eyre!("receiver dropped"))
        .log_err();
    if wait {
        ctx.wait_closed().await;
    }
}

/// Records `action` as the deferred power action if a backup is underway, and
/// reports whether it did. Unlike [`defer_or_begin`] it never performs the
/// action, which is what the power key needs: with no backup to wait for, the
/// press is logind's to act on.
pub async fn defer_until_backup_complete(
    ctx: &RpcContext,
    action: PowerAction,
) -> Result<bool, Error> {
    ctx.db
        .mutate(|db| defer_if_backing_up(db, action))
        .await
        .result
}

fn defer_if_backing_up(db: &mut DatabaseModel, action: PowerAction) -> Result<bool, Error> {
    let status = db.as_public_mut().as_server_info_mut().as_status_info_mut();
    if status.as_backup_progress().transpose_ref().is_none() {
        return Ok(false);
    }
    status.as_deferred_power_action_mut().ser(&Some(action))?;
    Ok(true)
}

/// Either records `action` for after the backup, or commits to performing it
/// now — in one mutation, so a backup cannot start in the window between
/// deciding and acting. Returns whether it was deferred.
async fn defer_or_begin(ctx: &RpcContext, action: PowerAction, force: bool) -> Result<bool, Error> {
    ctx.db
        .mutate(|db| defer_or_begin_in(db, action, force))
        .await
        .result
}

fn defer_or_begin_in(
    db: &mut DatabaseModel,
    action: PowerAction,
    force: bool,
) -> Result<bool, Error> {
    let status = db.as_public_mut().as_server_info_mut().as_status_info_mut();
    if !force && status.as_backup_progress().transpose_ref().is_some() {
        status.as_deferred_power_action_mut().ser(&Some(action))?;
        return Ok(true);
    }
    status.as_deferred_power_action_mut().ser(&None)?;
    match action {
        PowerAction::Restart => status.as_restarting_mut().ser(&true)?,
        PowerAction::Shutdown => status.as_shutting_down_mut().ser(&true)?,
    }
    Ok(false)
}

/// Reads the deferred action and clears it in one breath, so a cancellation that
/// lands first wins and the caller performs nothing.
fn take_deferred(db: &mut DatabaseModel) -> Result<Option<PowerAction>, Error> {
    let status = db.as_public_mut().as_server_info_mut().as_status_info_mut();
    let action = status.as_deferred_power_action().de()?;
    status.as_deferred_power_action_mut().ser(&None)?;
    Ok(action)
}

/// Carries out each deferred power action once the backup it was waiting on
/// finishes. Runs for the lifetime of startd: an action can be recorded at any
/// point during any backup — from the web UI, the CLI, or the power button — so
/// this must survive one having failed.
pub async fn run_deferred_power_actions(ctx: RpcContext) {
    let mut watch = ctx
        .db
        .watch(STATUS_INFO_PTR.parse::<JsonPointer>().unwrap())
        .await
        .typed::<ServerStatus>();
    loop {
        if let Err(e) = watch
            .wait_for(|status| {
                status.deferred_power_action.is_some() && status.backup_progress.is_none()
            })
            .await
        {
            // The db is gone, so there is nothing left to retry against.
            tracing::error!("stopped watching for deferred power actions: {e}");
            tracing::debug!("{e:?}");
            return;
        }
        let taken = ctx.db.mutate(take_deferred).await.result;
        let action = match taken {
            Ok(action) => action,
            Err(e) => {
                // A failed mutation leaves the db untouched, so retrying
                // immediately would spin against whatever is failing.
                tracing::error!("could not take the deferred power action: {e}");
                tracing::debug!("{e:?}");
                tokio::time::sleep(TAKE_RETRY).await;
                continue;
            }
        };
        // A backup that started since the take is waited for in turn.
        let params = ShutdownParams::default();
        let performed = match action {
            Some(PowerAction::Restart) => {
                tracing::info!("backup finished; carrying out the deferred restart");
                restart(ctx.clone(), params).await
            }
            Some(PowerAction::Shutdown) => {
                tracing::info!("backup finished; carrying out the deferred shutdown");
                shutdown(ctx.clone(), params).await
            }
            None => continue,
        };
        if let Err(e) = performed {
            tracing::error!("deferred power action failed: {e}");
            tracing::debug!("{e:?}");
            // Put it back rather than losing it, and give whatever failed room
            // to recover before trying again. Not via `defer_or_begin`: with the
            // backup already over it would commit to performing the action
            // instead of recording it.
            if let Some(action) = action {
                ctx.db
                    .mutate(|db| {
                        db.as_public_mut()
                            .as_server_info_mut()
                            .as_status_info_mut()
                            .as_deferred_power_action_mut()
                            .ser(&Some(action))
                    })
                    .await
                    .result
                    .log_err();
            }
            tokio::time::sleep(TAKE_RETRY).await;
        }
    }
}

pub async fn shutdown(
    ctx: RpcContext,
    ShutdownParams { wait, force }: ShutdownParams,
) -> Result<(), Error> {
    if defer_or_begin(&ctx, PowerAction::Shutdown, force).await? {
        return Ok(());
    }
    begin_shutdown(&ctx, false, wait).await;
    Ok(())
}

pub async fn restart(
    ctx: RpcContext,
    ShutdownParams { wait, force }: ShutdownParams,
) -> Result<(), Error> {
    if defer_or_begin(&ctx, PowerAction::Restart, force).await? {
        return Ok(());
    }
    begin_shutdown(&ctx, true, wait).await;
    Ok(())
}

pub async fn cancel_deferred_power(ctx: RpcContext) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_status_info_mut()
                .as_deferred_power_action_mut()
                .ser(&None)
        })
        .await
        .result
}

pub async fn rebuild(ctx: RpcContext) -> Result<(), Error> {
    tokio::fs::write(SYSTEM_REBUILD_PATH, b"").await?;
    restart(ctx, ShutdownParams::default()).await
}

#[cfg(test)]
mod test {
    use imbl_value::json;
    use patch_db::ModelExt;

    use super::*;

    fn db_with(backup_progress: Value, deferred: Value) -> DatabaseModel {
        DatabaseModel::from_value(json!({
            "public": { "serverInfo": { "statusInfo": {
                "backupProgress": backup_progress,
                "updateProgress": null,
                "shuttingDown": false,
                "restarting": false,
                "restart": null,
                "deferredPowerAction": deferred,
            } } }
        }))
    }

    fn backing_up() -> Value {
        json!({ "overall": { "done": 0, "total": 2, "units": null }, "phases": [] })
    }

    /// `(deferred action, shutting down, restarting)`.
    fn status(db: &DatabaseModel) -> (Option<PowerAction>, bool, bool) {
        let status = db.as_public().as_server_info().as_status_info();
        (
            status.as_deferred_power_action().de().unwrap(),
            status.as_shutting_down().de().unwrap(),
            status.as_restarting().de().unwrap(),
        )
    }

    #[test]
    fn json_and_rust_defaults_protect_running_backups() {
        let params: ShutdownParams = serde_json::from_str("{}").unwrap();
        assert!(!params.force);
        assert!(!params.wait);
        assert!(!ShutdownParams::default().force);
        assert!(!ShutdownParams::default().wait);
        let forced: ShutdownParams = serde_json::from_str(r#"{"force":true}"#).unwrap();
        assert!(forced.force);
        assert!(!forced.wait);
        let omitted: ShutdownParams = imbl_value::from_value(json!({})).unwrap();
        assert!(!omitted.force);
        assert!(!omitted.wait);
        for force in [false, true] {
            let params: ShutdownParams = imbl_value::from_value(json!({ "force": force })).unwrap();
            assert_eq!(params.force, force);
        }
    }

    #[test]
    fn cli_force_and_nowait_are_independent() {
        for action in ["restart", "shutdown"] {
            for (flags, force, wait) in [
                (vec![], false, true),
                (vec!["--force"], true, true),
                (vec!["--nowait"], false, false),
                (vec!["--force", "--nowait"], true, false),
            ] {
                let command = rpc_toolkit::CliApp::new(
                    |_: crate::context::config::ClientConfig| -> Result<crate::context::CliContext, crate::RpcError> {
                        unreachable!()
                    },
                    crate::main_api(),
                )
                .into_command();
                let matches = command
                    .try_get_matches_from(["start-cli", "server", action].into_iter().chain(flags))
                    .unwrap();
                let params = <ShutdownParams as clap::FromArgMatches>::from_arg_matches(
                    matches
                        .subcommand_matches("server")
                        .unwrap()
                        .subcommand_matches(action)
                        .unwrap(),
                )
                .unwrap();
                assert_eq!((params.force, params.wait), (force, wait));
                let serialized = imbl_value::to_value(&params).unwrap();
                assert_eq!(serialized, json!({ "wait": wait, "force": force }));
                let rpc: ShutdownParams = imbl_value::from_value(serialized).unwrap();
                assert_eq!((rpc.force, rpc.wait), (force, wait));
            }
            assert!(ShutdownParams::try_parse_from([action, "--after-backup"]).is_err());
        }
    }

    #[test]
    fn power_actions_defer_by_default_and_force_interrupts_backups() {
        for action in [PowerAction::Restart, PowerAction::Shutdown] {
            for backup in [false, true] {
                for params in [
                    serde_json::from_str::<ShutdownParams>("{}").unwrap(),
                    ShutdownParams::default(),
                    serde_json::from_str(r#"{"force":true}"#).unwrap(),
                ] {
                    let mut db = db_with(
                        if backup { backing_up() } else { json!(null) },
                        json!("restart"),
                    );
                    let deferred = backup && !params.force;
                    assert_eq!(
                        defer_or_begin_in(&mut db, action, params.force).unwrap(),
                        deferred
                    );
                    assert_eq!(
                        status(&db),
                        if deferred {
                            (Some(action), false, false)
                        } else {
                            (
                                None,
                                action == PowerAction::Shutdown,
                                action == PowerAction::Restart,
                            )
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn systemd_teardown_forces_both_power_actions() {
        for (unit, action) in [
            (
                include_str!("../../../../projects/start-os/startos-restart.service"),
                "restart",
            ),
            (
                include_str!("../../../../projects/start-os/startos-shutdown.service"),
                "shutdown",
            ),
        ] {
            let command = unit
                .lines()
                .find_map(|line| line.strip_prefix("ExecStop="))
                .unwrap();
            let args: Vec<_> = command.split_whitespace().collect();
            assert_eq!(&args[..3], &["/usr/bin/start-cli", "server", action]);
            let params = ShutdownParams::try_parse_from(
                std::iter::once("power").chain(args[3..].iter().copied()),
            )
            .unwrap();
            assert!(params.wait);
            assert!(params.force);
        }
    }

    /// Why [`run_deferred_power_actions`] cannot re-arm through this function:
    /// with the backup over it takes the other branch and commits to the action,
    /// which as a re-arm would leave the server flagged as powering down with
    /// nothing left to do it.
    #[test]
    fn beginning_an_action_clears_any_pending_one() {
        let mut db = db_with(json!(null), json!("restart"));
        assert!(!defer_or_begin_in(&mut db, PowerAction::Shutdown, false).unwrap());
        assert_eq!(status(&db), (None, true, false));
    }

    #[test]
    fn the_power_key_records_but_never_begins() {
        let mut db = db_with(backing_up(), json!(null));
        assert!(defer_if_backing_up(&mut db, PowerAction::Shutdown).unwrap());
        assert_eq!(status(&db), (Some(PowerAction::Shutdown), false, false));

        let mut db = db_with(json!(null), json!(null));
        assert!(!defer_if_backing_up(&mut db, PowerAction::Shutdown).unwrap());
        assert_eq!(
            status(&db),
            (None, false, false),
            "no backup to protect, so the press is logind's to act on"
        );
    }

    #[test]
    fn taking_the_action_clears_it_so_only_one_pass_performs_it() {
        let mut db = db_with(json!(null), json!("restart"));
        assert_eq!(take_deferred(&mut db).unwrap(), Some(PowerAction::Restart));
        assert_eq!(take_deferred(&mut db).unwrap(), None);
    }

    /// A cancellation that lands before the take wins outright.
    #[test]
    fn taking_a_cancelled_action_yields_nothing() {
        let mut db = db_with(json!(null), json!(null));
        assert_eq!(take_deferred(&mut db).unwrap(), None);
    }
}
