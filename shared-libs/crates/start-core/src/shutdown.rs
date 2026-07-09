use clap::Parser;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::PLATFORM;
use crate::context::RpcContext;
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

#[derive(Debug, Clone, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
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

pub async fn shutdown(
    ctx: RpcContext,
    ShutdownParams { wait }: ShutdownParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_status_info_mut()
                .as_shutting_down_mut()
                .ser(&true)
        })
        .await
        .result?;
    begin_shutdown(&ctx, false, wait).await;
    Ok(())
}

pub async fn restart(
    ctx: RpcContext,
    ShutdownParams { wait }: ShutdownParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_status_info_mut()
                .as_restarting_mut()
                .ser(&true)
        })
        .await
        .result?;
    begin_shutdown(&ctx, true, wait).await;
    Ok(())
}

pub async fn rebuild(ctx: RpcContext) -> Result<(), Error> {
    tokio::fs::write(SYSTEM_REBUILD_PATH, b"").await?;
    restart(ctx, ShutdownParams { wait: false }).await
}
