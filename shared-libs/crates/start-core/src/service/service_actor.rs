use std::sync::Arc;
use std::time::Duration;

use patch_db::TypedDbWatch;

use super::ServiceActorSeed;
use crate::prelude::*;
use crate::service::SYNC_RETRY_COOLDOWN_SECONDS;
use crate::service::transition::{Transition, TransitionKind};
use crate::status::{DesiredStatus, StatusInfo};
use crate::util::actor::Actor;
use crate::util::actor::background::BackgroundJobQueue;

#[derive(Clone)]
pub(super) struct ServiceActor(pub(super) Arc<ServiceActorSeed>);

impl Actor for ServiceActor {
    fn init(&mut self, jobs: &BackgroundJobQueue) {
        let seed = self.0.clone();
        let mut state = seed.persistent_container.state.subscribe();
        let initialized = async move { state.wait_for(|s| s.rt_initialized).await.map(|_| ()) };

        jobs.add_job(async move {
            if initialized.await.is_err() {
                return;
            }
            let mut watch = seed
                .ctx
                .db
                .watch(
                    format!("/public/packageData/{}/statusInfo", seed.id)
                        .parse()
                        .unwrap(),
                ) // TODO: typed pointers
                .await
                .typed::<StatusInfo>();
            let mut transition: Option<Transition> = None;

            loop {
                let res = service_actor_loop(&mut watch, &seed, &mut transition).await;
                let wait = async {
                    if let Err(e) = async {
                        res?;
                        watch.changed().await?;
                        Ok::<_, Error>(())
                    }
                    .await
                    {
                        tracing::error!(
                            "{}",
                            t!("service.service-actor.error-synchronizing-state", error = e)
                        );
                        tracing::debug!("{e:?}");
                        tracing::error!(
                            "{}",
                            t!(
                                "service.service-actor.retrying-in-seconds",
                                seconds = SYNC_RETRY_COOLDOWN_SECONDS
                            )
                        );
                        tokio::time::timeout(
                            Duration::from_secs(SYNC_RETRY_COOLDOWN_SECONDS),
                            async {
                                watch.changed().await.log_err();
                            },
                        )
                        .await
                        .ok();
                    }
                };
                tokio::pin!(wait);
                let transition_handler = async {
                    match &mut transition {
                        Some(Transition { future, .. }) => {
                            let err = future.await.log_err().is_none(); // TODO: ideally this error should be sent to service logs
                            transition.take();
                            if err {
                                tokio::time::sleep(Duration::from_secs(
                                    SYNC_RETRY_COOLDOWN_SECONDS,
                                ))
                                .await;
                            } else {
                                futures::future::pending().await
                            }
                        }
                        _ => futures::future::pending().await,
                    }
                };
                tokio::pin!(transition_handler);
                futures::future::select(wait, transition_handler).await;
            }
        });
    }
}

async fn service_actor_loop<'a>(
    watch: &mut TypedDbWatch<StatusInfo>,
    seed: &'a Arc<ServiceActorSeed>,
    transition: &mut Option<Transition<'a>>,
) -> Result<(), Error> {
    let status_model = watch.peek_and_mark_seen()?;
    let status = status_model.de()?;

    match status {
        StatusInfo {
            desired: DesiredStatus::Running | DesiredStatus::Restarting { .. },
            started: None,
            ..
        } => {
            let task = transition
                .take()
                .filter(|task| task.kind == TransitionKind::Starting);
            *transition = task.or_else(|| Some(seed.start()));
        }
        StatusInfo {
            desired:
                DesiredStatus::Stopped
                | DesiredStatus::Restarting { .. }
                | DesiredStatus::BackingUp { .. }
                | DesiredStatus::Updating { .. },
            started: Some(_),
            ..
        } => {
            let task = transition
                .take()
                .filter(|task| task.kind == TransitionKind::Stopping);
            *transition = task.or_else(|| Some(seed.stop()));
        }
        StatusInfo {
            desired: DesiredStatus::BackingUp { .. },
            started: None,
            ..
        } => {
            let task = transition
                .take()
                .filter(|task| task.kind == TransitionKind::BackingUp);
            *transition = task.or_else(|| Some(seed.backup()));
        }
        _ => (),
    };
    Ok(())
}
