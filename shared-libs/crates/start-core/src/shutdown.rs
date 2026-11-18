use std::sync::Arc;

use clap::Parser;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::PLATFORM;
use crate::context::RpcContext;
use crate::db::model::Database;
use crate::disk::main::export;
use crate::init::STANDBY_MODE_PATH;
use crate::prelude::*;
use crate::sound::SHUTDOWN;
use crate::util::Invoke;
use crate::util::sync::SyncMutex;

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

/// The first request consumes the sender; the restart flag labels later errors.
pub type ShutdownSender = SyncMutex<(Option<oneshot::Sender<Option<Shutdown>>>, bool)>;

pub(crate) fn in_progress_error(restarting: bool) -> Error {
    let message = if restarting {
        t!("shutdown.already-restarting")
    } else {
        t!("shutdown.already-shutting-down")
    };
    Error::new(eyre!(message), ErrorKind::InvalidRequest)
}

/// `None` requests daemon exit without powering off. Admission never writes to the database.
pub fn send_shutdown(sender: &ShutdownSender, request: Option<Shutdown>) -> Result<(), Error> {
    sender.mutate(|(sender, restarting)| {
        let Some(sender) = sender.take() else {
            return Err(in_progress_error(*restarting));
        };
        let restart = request.as_ref().is_some_and(|request| request.restart);
        sender.send(request).map_err(|_| in_progress_error(false))?;
        *restarting = restart;
        Ok(())
    })
}

/// UI observations; shutdown admission belongs to the oneshot sender.
pub(crate) async fn publish_shutdown_status(db: &TypedPatchDb<Database>, sender: &ShutdownSender) {
    let (shutting_down, restarting) =
        sender.peek(|(sender, restarting)| (sender.is_none() && !*restarting, *restarting));
    db.mutate(|db| {
        let status = db.as_public_mut().as_server_info_mut().as_status_info_mut();
        status.as_shutting_down_mut().ser(&shutting_down)?;
        status.as_restarting_mut().ser(&restarting)
    })
    .await
    .result
    .log_err();
}

/// Admission completes before the returned status-publication future is polled.
pub(crate) fn request_shutdown(
    db: TypedPatchDb<Database>,
    sender: Arc<ShutdownSender>,
    request: Option<Shutdown>,
) -> Result<impl Future<Output = ()> + Send + 'static, Error> {
    send_shutdown(&sender, request)?;
    Ok(async move { publish_shutdown_status(&db, &sender).await })
}

async fn begin_shutdown(ctx: RpcContext, restart: bool, wait: bool) -> Result<(), Error> {
    let tasks = ctx.background_tasks.clone();
    let completion = ctx.wait_closed();
    let db = ctx.db.clone();
    let sender = ctx.shutdown.clone();
    let request = Some(Shutdown {
        disk_guid: Some(ctx.disk_guid.clone()),
        restart,
    });
    drop(ctx);
    let publication = request_shutdown(db, sender, request)?;
    crate::context::rpc::spawn_background(&tasks, publication).log_err();
    wait_for_services(completion, wait).await;
    Ok(())
}

/// Accepted replies remain connected through service teardown without retaining a producer scope.
pub(crate) async fn wait_for_services(completion: impl Future<Output = ()>, wait: bool) {
    rpc_toolkit::requests::preserve_request_reply();
    if wait {
        completion.await;
    }
}

pub async fn shutdown(
    ctx: RpcContext,
    ShutdownParams { wait }: ShutdownParams,
) -> Result<(), Error> {
    begin_shutdown(ctx, false, wait).await
}

pub async fn restart(
    ctx: RpcContext,
    ShutdownParams { wait }: ShutdownParams,
) -> Result<(), Error> {
    begin_shutdown(ctx, true, wait).await
}

#[cfg(test)]
mod tests {
    use futures::FutureExt;

    use super::*;
    use crate::db::model::public::ServerStatus;

    fn requests() -> (Arc<ShutdownSender>, oneshot::Receiver<Option<Shutdown>>) {
        let (sender, receiver) = oneshot::channel();
        (Arc::new(SyncMutex::new((Some(sender), false))), receiver)
    }

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

    #[test]
    fn cli_waits_by_default_while_rpc_wait_remains_opt_in() {
        assert!(ShutdownParams::try_parse_from(["shutdown"]).unwrap().wait);
        assert!(
            !ShutdownParams::try_parse_from(["shutdown", "--nowait"])
                .unwrap()
                .wait
        );
        assert!(!serde_json::from_str::<ShutdownParams>("{}").unwrap().wait);
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
            let (requests, receiver) = requests();
            request_shutdown(db.clone(), requests.clone(), request(restart))
                .unwrap()
                .await;
            assert_eq!(receiver.await.unwrap().unwrap().restart, restart);
            let before = db.dump(&patch_db::json_ptr::ROOT).await;

            for later in [request(false), request(true), None] {
                let error = request_shutdown(db.clone(), requests.clone(), later)
                    .err()
                    .unwrap();
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
    async fn a_closed_receiver_rejects_requests_without_publishing_status() {
        let db = database().await;
        let (requests, receiver) = requests();
        drop(receiver);
        let before = db.dump(&patch_db::json_ptr::ROOT).await;
        let error = request_shutdown(db.clone(), requests.clone(), request(true))
            .err()
            .unwrap();
        assert!(matches!(error.kind, ErrorKind::InvalidRequest));
        assert_eq!(
            error.source.to_string(),
            t!("shutdown.already-shutting-down")
        );
        let repeated = send_shutdown(&requests, request(false)).unwrap_err();
        assert_eq!(repeated.source.to_string(), error.source.to_string());
        assert_eq!(db.dump(&patch_db::json_ptr::ROOT).await.id, before.id);
    }

    #[tokio::test]
    async fn signal_exit_and_rejection_do_not_require_a_database() {
        let (requests, receiver) = requests();
        send_shutdown(&requests, None).unwrap();
        assert!(requests.peek(|(sender, _)| sender.is_none()));
        for restart in [false, true] {
            let error = send_shutdown(&requests, request(restart)).unwrap_err();
            assert_eq!(
                error.source.to_string(),
                t!("shutdown.already-shutting-down")
            );
        }
        assert!(receiver.await.unwrap().is_none());
    }

    #[tokio::test]
    async fn new_rpc_context_clears_observed_status_and_accepts_a_new_request() {
        let db = database().await;
        let (previous, receiver) = requests();
        request_shutdown(db.clone(), previous.clone(), request(false))
            .unwrap()
            .await;
        drop((previous, receiver));

        let (requests, receiver) = requests();
        publish_shutdown_status(&db, &requests).await;
        let status = db
            .peek()
            .await
            .as_public()
            .as_server_info()
            .as_status_info()
            .de()
            .unwrap();
        assert!(!status.shutting_down);
        assert!(!status.restarting);
        assert!(requests.peek(|(sender, _)| sender.is_some()));
        request_shutdown(db.clone(), requests.clone(), request(true))
            .unwrap()
            .await;
        assert!(receiver.await.unwrap().unwrap().restart);
    }

    #[tokio::test]
    async fn concurrent_shutdown_and_restart_publish_exactly_one_request() {
        let (requests, receiver) = requests();
        let (shutdown, restart) = std::thread::scope(|scope| {
            let shutdown = scope.spawn(|| send_shutdown(&requests, request(false)));
            let restart = scope.spawn(|| send_shutdown(&requests, request(true)));
            (shutdown.join().unwrap(), restart.join().unwrap())
        });
        assert_ne!(shutdown.is_ok(), restart.is_ok());
        let chosen = receiver.await.unwrap().unwrap();
        assert_eq!(chosen.restart, restart.is_ok());
    }

    #[tokio::test]
    async fn shutdown_notification_precedes_a_blocked_status_write() {
        let db = database().await;
        let (requests, mut receiver) = requests();
        let locked_db = db.clone();
        let runtime = tokio::runtime::Handle::current();
        let (locked_send, locked_recv) = std::sync::mpsc::channel();
        let (release_send, release_recv) = std::sync::mpsc::channel::<()>();
        let holder = std::thread::spawn(move || {
            runtime
                .block_on(locked_db.mutate(move |_| {
                    locked_send.send(()).unwrap();
                    let _ = release_recv.recv();
                    Ok::<_, Error>(())
                }))
                .result
                .unwrap();
        });
        locked_recv.recv().unwrap();

        let shutdown = request_shutdown(db.clone(), requests.clone(), request(false)).unwrap();
        tokio::pin!(shutdown);
        assert!(shutdown.as_mut().now_or_never().is_none());
        assert!(!receiver.try_recv().unwrap().unwrap().restart);
        assert!(requests.peek(|(sender, _)| sender.is_none()));
        assert!(send_shutdown(&requests, request(true)).is_err());

        drop(release_send);
        holder.join().unwrap();
        shutdown.await;
    }
}
