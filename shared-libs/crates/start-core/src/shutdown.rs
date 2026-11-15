use clap::Parser;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

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

/// Receives the next shutdown request. A request superseded before it was read
/// is skipped.
pub async fn recv_request(
    requests: &mut broadcast::Receiver<Option<Shutdown>>,
) -> Result<Option<Shutdown>, Error> {
    loop {
        match requests.recv().await {
            Err(broadcast::error::RecvError::Lagged(_)) => (),
            res => return res.with_kind(ErrorKind::Unknown),
        }
    }
}

/// The power action to execute once nothing can submit requests.
/// A later explicit request supersedes the received one.
pub fn final_request(
    mut request: Option<Shutdown>,
    mut requests: broadcast::Receiver<Option<Shutdown>>,
) -> Option<Shutdown> {
    use broadcast::error::TryRecvError;
    loop {
        match requests.try_recv() {
            Ok(Some(later)) => request = Some(later),
            Ok(None) | Err(TryRecvError::Lagged(_)) => (),
            Err(TryRecvError::Empty | TryRecvError::Closed) => return request,
        }
    }
}

async fn begin_shutdown(ctx: &RpcContext, restart: bool, wait: bool) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            let status = db.as_public_mut().as_server_info_mut().as_status_info_mut();
            status.as_shutting_down_mut().ser(&!restart)?;
            status.as_restarting_mut().ser(&restart)
        })
        .await
        .result?;
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
    use tokio::sync::broadcast;

    use super::{Shutdown, final_request, recv_request};

    fn request(restart: bool) -> Option<Shutdown> {
        Some(Shutdown {
            disk_guid: None,
            restart,
        })
    }

    fn restart_of(request: Option<Shutdown>) -> Option<bool> {
        request.map(|s| s.restart)
    }

    #[tokio::test]
    async fn recv_request_returns_the_latest_of_unread_requests() {
        let (send, mut requests) = broadcast::channel(1);
        send.send(request(false)).unwrap();
        send.send(request(true)).unwrap();

        let received = recv_request(&mut requests).await.unwrap();

        assert_eq!(restart_of(received), Some(true));
    }

    #[test]
    fn final_request_keeps_the_received_request_without_later_ones() {
        let (_send, requests) = broadcast::channel(1);

        assert_eq!(
            restart_of(final_request(request(false), requests)),
            Some(false)
        );
    }

    #[test]
    fn final_request_honors_a_request_accepted_during_teardown() {
        let (send, requests) = broadcast::channel(1);
        send.send(request(true)).unwrap();

        assert_eq!(
            restart_of(final_request(request(false), requests)),
            Some(true)
        );
    }

    #[test]
    fn final_request_honors_an_explicit_request_after_a_signal() {
        let (send, requests) = broadcast::channel(1);
        send.send(request(false)).unwrap();

        assert_eq!(restart_of(final_request(None, requests)), Some(false));
    }

    #[test]
    fn final_request_keeps_the_latest_of_several_later_requests() {
        let (send, requests) = broadcast::channel(1);
        send.send(request(true)).unwrap();
        send.send(request(false)).unwrap();

        assert_eq!(
            restart_of(final_request(request(true), requests)),
            Some(false)
        );
    }

    #[test]
    fn final_request_ignores_a_later_signal() {
        let (send, requests) = broadcast::channel(1);
        send.send(None).unwrap();

        assert_eq!(
            restart_of(final_request(request(true), requests)),
            Some(true)
        );
    }
}
