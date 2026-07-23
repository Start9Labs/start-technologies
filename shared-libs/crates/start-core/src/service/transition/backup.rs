use std::path::PathBuf;

use futures::FutureExt;
use futures::future::BoxFuture;

use crate::disk::mount::filesystem::ReadWrite;
use crate::prelude::*;
use crate::progress::PhaseProgressTrackerHandle;
use crate::rpc_continuations::Guid;
use crate::service::action::GetActionInput;
use crate::service::start_stop::StartStop;
use crate::service::transition::{Transition, TransitionKind};
use crate::service::{ProcedureName, ServiceActor, ServiceActorSeed};
use crate::status::DesiredStatus;
use crate::util::actor::background::BackgroundJobQueue;
use crate::util::actor::{ConflictBuilder, Handler};
use crate::util::serde::NoOutput;

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
                // The backup future clears BackingUp itself when it finishes, so
                // here we just drive it to completion. If there's nothing to
                // resume, recover the state so it can't get stuck, then report.
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
    type Response = Result<BoxFuture<'static, Result<(), Error>>, Error>;
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

        // Split the backup into a driver (`remote`, stored for the actor to run
        // once the service has stopped) and a handle (returned to the caller).
        // Awaiting the handle only reads the result — it never drives the work —
        // so the backup can't start before the actor runs it, and the handle
        // doesn't resolve until the service has left the backing-up state.
        let (remote, handle) = async move {
            let res = async {
                let backup_guard = seed
                    .persistent_container
                    .mount_backup(path, ReadWrite)
                    .await?;
                seed.persistent_container
                    .execute::<NoOutput>(id, ProcedureName::CreateBackup, Value::Null, None)
                    .await?;
                backup_guard.unmount(true).await?;

                Ok::<_, Error>(())
            }
            .await;
            seed.leave_backing_up().await?;
            res
        }
        .remote_handle();

        self.0.backup.replace(Some(remote.boxed()));

        Ok(handle.boxed())
    }
}
