use std::path::PathBuf;

use futures::future::BoxFuture;
use futures::{FutureExt, TryFutureExt};
use rpc_toolkit::yajrc::RpcError;

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
    pub fn backup(&self) -> Transition<'_> {
        Transition {
            kind: TransitionKind::BackingUp,
            future: async {
                let res = if let Some(fut) = self.backup.replace(None) {
                    fut.await.map_err(Error::from)
                } else {
                    Err(Error::new(
                        eyre!("{}", t!("service.transition.backup.no-backup-to-resume")),
                        ErrorKind::Cancelled,
                    ))
                };
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
                    .result?;
                res
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

        let transition = async move {
            async {
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
            .await
            .map_err(RpcError::from)
        }
        .shared();

        self.0.backup.replace(Some(transition.clone().boxed()));

        Ok(transition.map_err(Error::from).boxed())
    }
}
