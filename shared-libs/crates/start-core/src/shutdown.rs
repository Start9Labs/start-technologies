use clap::Parser;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::PLATFORM;
use crate::context::RpcContext;
use crate::db::model::{Database, DatabaseModel};
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

#[derive(Debug, Clone, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ShutdownParams {
    /// Block until graceful teardown completes (default over the CLI; the
    /// frontend omits this and gets an immediate reply). Cleared with
    /// `--nowait`. The wait can't outlive the webserver teardown that follows
    /// container shutdown, so the connection drops once services are stopped.
    #[arg(long = "nowait", action = clap::ArgAction::SetFalse, help = "help.arg.nowait")]
    #[serde(default)]
    wait: bool,
}

rpc_toolkit::reflect_ts!(ShutdownParams);
rpc_toolkit::ts_export!(ShutdownParams, namespaces = [""]);

/// A new RPC context accepts a new shutdown request.
pub(crate) fn reset_shutdown(db: &mut DatabaseModel) -> Result<(), Error> {
    let status = db.as_public_mut().as_server_info_mut().as_status_info_mut();
    status.as_shutting_down_mut().ser(&false)?;
    status.as_restarting_mut().ser(&false)
}

/// The first request fixes the action; later requests leave it unchanged and fail.
/// `None` stops startd without powering off.
pub(crate) async fn request_shutdown(
    db: &TypedPatchDb<Database>,
    sender: &broadcast::Sender<Option<Shutdown>>,
    request: Option<Shutdown>,
) -> Result<(), Error> {
    let restart = request.as_ref().is_some_and(|request| request.restart);
    db.mutate(|db| {
        let status = db.as_public_mut().as_server_info_mut().as_status_info_mut();
        if status.as_shutting_down().de()? {
            return Err(Error::new(
                eyre!(t!("shutdown.already-shutting-down")),
                ErrorKind::InvalidRequest,
            ));
        }
        if status.as_restarting().de()? {
            return Err(Error::new(
                eyre!(t!("shutdown.already-restarting")),
                ErrorKind::InvalidRequest,
            ));
        }
        status.as_shutting_down_mut().ser(&!restart)?;
        status.as_restarting_mut().ser(&restart)
    })
    .await
    .result?;
    sender
        .send(request)
        .map_err(|_| eyre!("receiver dropped"))
        .log_err();
    Ok(())
}

async fn begin_shutdown(ctx: &RpcContext, restart: bool, wait: bool) -> Result<(), Error> {
    request_shutdown(
        &ctx.db,
        &ctx.shutdown,
        Some(Shutdown {
            disk_guid: Some(ctx.disk_guid.clone()),
            restart,
        }),
    )
    .await?;
    if wait {
        ctx.wait_closed().await;
    }
    Ok(())
}

pub async fn shutdown(
    ctx: RpcContext,
    ShutdownParams { wait }: ShutdownParams,
) -> Result<(), Error> {
    begin_shutdown(&ctx, false, wait).await
}

pub async fn restart(
    ctx: RpcContext,
    ShutdownParams { wait }: ShutdownParams,
) -> Result<(), Error> {
    begin_shutdown(&ctx, true, wait).await
}

pub async fn rebuild(ctx: RpcContext) -> Result<(), Error> {
    tokio::fs::write(SYSTEM_REBUILD_PATH, b"").await?;
    restart(ctx, ShutdownParams { wait: false }).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::model::public::ServerStatus;

    async fn database() -> TypedPatchDb<Database> {
        let db = TypedPatchDb::load_unchecked(PatchDb::in_memory());
        db.put(
            &patch_db::json_ptr::ROOT,
            &imbl_value::json!({
                "public": {"serverInfo": {"statusInfo": ServerStatus::default()}}
            }),
        )
        .await
        .unwrap();
        db
    }

    fn request(restart: bool) -> Option<Shutdown> {
        Some(Shutdown {
            disk_guid: None,
            restart,
        })
    }

    #[tokio::test]
    async fn shutdown_rejects_repeated_and_conflicting_requests_during_teardown() {
        for restart in [false, true] {
            let db = database().await;
            let (send, mut requests) = broadcast::channel(1);
            request_shutdown(&db, &send, request(restart))
                .await
                .unwrap();
            assert_eq!(requests.recv().await.unwrap().unwrap().restart, restart);
            drop(requests);
            let before = db.dump(&patch_db::json_ptr::ROOT).await;

            for later in [request(false), request(true), None] {
                let error = request_shutdown(&db, &send, later).await.unwrap_err();
                assert!(matches!(error.kind, ErrorKind::InvalidRequest));
                assert_eq!(
                    error.source.to_string(),
                    if restart {
                        t!("shutdown.already-restarting")
                    } else {
                        t!("shutdown.already-shutting-down")
                    }
                );
            }

            let after = db.dump(&patch_db::json_ptr::ROOT).await;
            assert_eq!(after.value, before.value);
            assert_eq!(after.id, before.id);
            let status = db
                .peek()
                .await
                .as_public()
                .as_server_info()
                .as_status_info()
                .de()
                .unwrap();
            assert_eq!(status.shutting_down, !restart);
            assert_eq!(status.restarting, restart);
        }
    }

    #[tokio::test]
    async fn signal_initiated_shutdown_rejects_later_power_requests() {
        let db = database().await;
        let (send, mut requests) = broadcast::channel(1);
        request_shutdown(&db, &send, None).await.unwrap();
        for restart in [false, true] {
            let error = request_shutdown(&db, &send, request(restart))
                .await
                .unwrap_err();
            assert_eq!(
                error.source.to_string(),
                t!("shutdown.already-shutting-down")
            );
        }
        assert!(requests.recv().await.unwrap().is_none());
        assert!(matches!(
            requests.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
    }

    #[tokio::test]
    async fn shutdown_admission_resets_with_the_rpc_context() {
        let db = database().await;
        let (send, requests) = broadcast::channel(1);
        request_shutdown(&db, &send, request(false)).await.unwrap();
        drop((send, requests));

        db.mutate(reset_shutdown).await.result.unwrap();
        let (send, mut requests) = broadcast::channel(1);
        request_shutdown(&db, &send, request(true)).await.unwrap();
        assert!(requests.recv().await.unwrap().unwrap().restart);
    }

    #[tokio::test]
    async fn simultaneous_shutdown_and_restart_publish_exactly_one_request() {
        let db = database().await;
        let (send, mut requests) = broadcast::channel(1);
        let (shutdown, restart) = tokio::join!(
            request_shutdown(&db, &send, request(false)),
            request_shutdown(&db, &send, request(true)),
        );
        assert_ne!(shutdown.is_ok(), restart.is_ok());
        let chosen = requests.recv().await.unwrap().unwrap();
        assert_eq!(chosen.restart, restart.is_ok());
        assert!(matches!(
            requests.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        let status = db
            .peek()
            .await
            .as_public()
            .as_server_info()
            .as_status_info()
            .de()
            .unwrap();
        assert_eq!(status.shutting_down, !chosen.restart);
        assert_eq!(status.restarting, chosen.restart);
    }
}
