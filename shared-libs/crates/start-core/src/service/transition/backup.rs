use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures::FutureExt;
use futures::future::BoxFuture;

use crate::backup::PackageBackupOutput;
use crate::disk::mount::filesystem::ReadWrite;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;
use crate::progress::PhaseProgressTrackerHandle;
use crate::rpc_continuations::Guid;
use crate::service::action::GetActionInput;
use crate::service::start_stop::StartStop;
use crate::service::transition::{Transition, TransitionKind};
use crate::service::{ServiceActor, ServiceActorSeed};
use crate::status::DesiredStatus;
use crate::util::actor::background::BackgroundJobQueue;
use crate::util::actor::{ConflictBuilder, Handler};

/// Maximum wall-clock time a service backup procedure may run.
const PACKAGE_BACKUP_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);

async fn run_backup_procedure<T, Stop, StopFuture>(
    execute: impl Future<Output = Result<T, Error>>,
    mut stop_runtime: Stop,
    unmount: impl Future<Output = Result<(), Error>>,
    restart_runtime: impl Future<Output = Result<(), Error>>,
    timeout: Duration,
) -> Result<T, Error>
where
    Stop: FnMut() -> StopFuture,
    StopFuture: Future<Output = Result<(), Error>>,
{
    let (execute_result, timed_out) = match tokio::time::timeout(timeout, execute).await {
        Ok(result) => (result, false),
        Err(error) => (Err(error).with_kind(ErrorKind::Timeout), true),
    };
    // Cleanup waits for confirmed runtime shutdown.
    if timed_out {
        while let Err(error) = stop_runtime().await {
            tracing::error!(%error, "failed to stop package runtime after backup timeout; retrying");
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    }
    let unmount_result = unmount.await;
    let restart_result = if timed_out && unmount_result.is_ok() {
        Some(restart_runtime.await)
    } else {
        None
    };

    if let Err(error) = &unmount_result {
        tracing::error!(%error, "failed to unmount package backup");
    }
    if let Some(Err(error)) = &restart_result {
        tracing::error!(%error, "failed to restart package runtime after backup timeout");
    }

    match execute_result {
        Ok(output) => {
            unmount_result?;
            Ok(output)
        }
        Err(error) => {
            unmount_result?;
            if let Some(restarted) = restart_result {
                restarted?;
            }
            Err(error)
        }
    }
}

impl ServiceActorSeed {
    async fn leave_backing_up(&self) -> Result<(), Error> {
        let id = &self.id;
        self.ctx
            .db
            .mutate(|db| {
                db.as_public_mut()
                    .as_package_data_mut()
                    .as_idx_mut(id)
                    .or_not_found(id)?
                    .as_status_info_mut()
                    .as_desired_mut()
                    .map_mutate(|s| {
                        Ok(match s {
                            DesiredStatus::BackingUp {
                                on_complete: StartStop::Start,
                            } => DesiredStatus::Running,
                            DesiredStatus::BackingUp {
                                on_complete: StartStop::Stop,
                            } => DesiredStatus::Stopped,
                            x => x,
                        })
                    })?;
                Ok(())
            })
            .await
            .result
    }

    pub fn backup(&self) -> Transition<'_> {
        Transition {
            kind: TransitionKind::BackingUp,
            future: async {
                // The actor drives the stored backup future.
                if let Some(backup) = self.backup.replace(None) {
                    backup.await;
                    Ok(())
                } else {
                    self.leave_backing_up().await?;
                    Err(Error::new(
                        eyre!("{}", t!("service.transition.backup.no-backup-to-resume")),
                        ErrorKind::Cancelled,
                    ))
                }
            }
            .boxed(),
        }
    }
}

pub(in crate::service) struct Backup {
    pub path: PathBuf,
    pub progress: PhaseProgressTrackerHandle,
}
impl Handler<Backup> for ServiceActor {
    type Response = Result<BoxFuture<'static, Result<PackageBackupOutput, Error>>, Error>;
    fn conflicts_with(_: &Backup) -> ConflictBuilder<Self> {
        ConflictBuilder::everything().except::<GetActionInput>()
    }
    async fn handle(
        &mut self,
        id: Guid,
        Backup { path, progress }: Backup,
        _: &BackgroundJobQueue,
    ) -> Self::Response {
        let seed = self.0.clone();
        seed.backup_phase.replace(Some(progress));

        // Dropping the result handle cancels cleanup.
        let (remote, handle) = async move {
            let runtime_stopped = AtomicBool::new(false);
            let res = async {
                let backup_guard = seed
                    .persistent_container
                    .mount_backup(path, ReadWrite)
                    .await?;
                let restart_id = id.clone();
                let cleanup_notified = AtomicBool::new(false);
                let output = run_backup_procedure(
                    seed.persistent_container
                        .execute_backup::<Option<PackageBackupOutput>>(
                            id,
                            Value::Null,
                            PACKAGE_BACKUP_TIMEOUT,
                        ),
                    || async {
                        let result = seed
                            .persistent_container
                            .stop_runtime_after_backup_timeout()
                            .await;
                        if result.is_ok() {
                            runtime_stopped.store(true, Ordering::Relaxed);
                        }
                        if result.is_err() && !cleanup_notified.swap(true, Ordering::Relaxed) {
                            let package_id = seed.id.clone();
                            seed.ctx
                                .db
                                .mutate(|db| {
                                    notify(
                                        db,
                                        Some(package_id.clone()),
                                        NotificationLevel::Error,
                                        t!("service.transition.backup.cleanup-blocked-title")
                                            .to_string(),
                                        t!(
                                            "service.transition.backup.cleanup-blocked-message",
                                            service = package_id
                                        )
                                        .to_string(),
                                        (),
                                    )
                                })
                                .await
                                .result
                                .log_err();
                        }
                        result
                    },
                    backup_guard.unmount(true),
                    async {
                        seed.persistent_container
                            .restart_runtime_after_backup_timeout(restart_id)
                            .await?;
                        runtime_stopped.store(false, Ordering::Relaxed);
                        Ok(())
                    },
                    PACKAGE_BACKUP_TIMEOUT,
                )
                .await?;
                Ok::<_, Error>(output.unwrap_or_default())
            }
            .await;
            if runtime_stopped.load(Ordering::Relaxed) {
                let package_id = seed.id.clone();
                let message = t!(
                    "service.transition.backup.runtime-recovery-failed",
                    service = package_id
                )
                .to_string();
                let error = crate::error::ErrorData {
                    details: message.clone(),
                    debug: res
                        .as_ref()
                        .err()
                        .map(|error| format!("{error:?}"))
                        .unwrap_or_default(),
                    info: Value::Null,
                };
                seed.ctx
                    .db
                    .mutate(|db| {
                        let status = db
                            .as_public_mut()
                            .as_package_data_mut()
                            .as_idx_mut(&package_id)
                            .or_not_found(&package_id)?
                            .as_status_info_mut();
                        status.as_desired_mut().ser(&DesiredStatus::Stopped)?;
                        status.as_error_mut().ser(&Some(error))?;
                        notify(
                            db,
                            Some(package_id),
                            NotificationLevel::Error,
                            t!("service.transition.backup.cleanup-blocked-title").to_string(),
                            message,
                            (),
                        )
                    })
                    .await
                    .result?;
            } else {
                seed.leave_backing_up().await?;
            }
            res
        }
        .remote_handle();

        self.0.backup.replace(Some(remote.boxed()));

        Ok(handle.boxed())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;

    #[tokio::test]
    async fn package_backup_timeout_stops_unmounts_and_restarts_in_order() {
        let step = Arc::new(AtomicUsize::new(0));
        let stopped = step.clone();
        let unmounted = step.clone();
        let restarted = step.clone();
        let execute = std::future::pending::<Result<PackageBackupOutput, Error>>();
        let stop_runtime = || async {
            assert_eq!(stopped.fetch_add(1, Ordering::SeqCst), 0);
            Ok(())
        };
        let unmount = async move {
            assert_eq!(unmounted.fetch_add(1, Ordering::SeqCst), 1);
            Ok(())
        };
        let restart_runtime = async move {
            assert_eq!(restarted.fetch_add(1, Ordering::SeqCst), 2);
            Ok(())
        };

        let error = run_backup_procedure(
            execute,
            stop_runtime,
            unmount,
            restart_runtime,
            Duration::ZERO,
        )
        .await
        .unwrap_err();

        assert_eq!(error.kind, ErrorKind::Timeout);
        assert_eq!(step.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn package_backup_success_preserves_output_and_unmounts() {
        let unmounted = Arc::new(AtomicBool::new(false));
        let unmounted_after = unmounted.clone();
        let stopped = Arc::new(AtomicBool::new(false));
        let stopped_after = stopped.clone();
        let restarted = Arc::new(AtomicBool::new(false));
        let restarted_after = restarted.clone();
        let execute = async {
            Ok(PackageBackupOutput {
                changed_bytes: Some(42),
            })
        };
        let unmount = async move {
            unmounted_after.store(true, Ordering::SeqCst);
            Ok(())
        };
        let stop_runtime = || async {
            stopped_after.store(true, Ordering::SeqCst);
            Ok(())
        };
        let restart_runtime = async move {
            restarted_after.store(true, Ordering::SeqCst);
            Ok(())
        };

        let output = run_backup_procedure(
            execute,
            stop_runtime,
            unmount,
            restart_runtime,
            Duration::from_secs(1),
        )
        .await
        .unwrap();

        assert_eq!(output.changed_bytes, Some(42));
        assert!(unmounted.load(Ordering::SeqCst));
        assert!(!stopped.load(Ordering::SeqCst));
        assert!(!restarted.load(Ordering::SeqCst));
    }

    #[tokio::test(start_paused = true)]
    async fn package_backup_cleanup_waits_for_successful_cancellation() {
        let attempts = AtomicUsize::new(0);
        let unmounted = AtomicBool::new(false);
        let error = run_backup_procedure(
            std::future::pending::<Result<(), Error>>(),
            || async {
                if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    assert!(!unmounted.load(Ordering::SeqCst));
                    Err(Error::new(
                        eyre!("runtime shutdown failed"),
                        ErrorKind::Docker,
                    ))
                } else {
                    Ok(())
                }
            },
            async {
                assert_eq!(attempts.load(Ordering::SeqCst), 2);
                unmounted.store(true, Ordering::SeqCst);
                Ok(())
            },
            async { Ok(()) },
            Duration::ZERO,
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Timeout);
        assert!(unmounted.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn package_backup_restart_failure_is_reported() {
        let unmounted = AtomicBool::new(false);
        let error = run_backup_procedure(
            std::future::pending::<Result<(), Error>>(),
            || async { Ok(()) },
            async {
                unmounted.store(true, Ordering::SeqCst);
                Ok(())
            },
            async {
                assert!(unmounted.load(Ordering::SeqCst));
                Err(Error::new(
                    eyre!("runtime restart failed"),
                    ErrorKind::Docker,
                ))
            },
            Duration::ZERO,
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Docker);
    }
}
