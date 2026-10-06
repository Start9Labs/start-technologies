use std::sync::Arc;
use std::time::Duration;

use patch_db::TypedDbWatch;
use patch_db::json_ptr::JsonPointer;

use super::ServiceActorSeed;
use crate::PackageId;
use crate::db::model::package::PackageDataEntry;
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
                .watch(service_watch_path(&seed.id))
                .await
                .typed::<PackageDataEntry>();
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
                let transition_handler = finish_transition(&mut transition);
                tokio::pin!(transition_handler);
                futures::future::select(wait, transition_handler).await;
            }
        });
    }
}

async fn finish_transition(transition: &mut Option<Transition<'_>>) {
    match transition {
        Some(Transition { future, .. }) => {
            let err = future.await.log_err().is_none();
            transition.take();
            if err {
                tokio::time::sleep(Duration::from_secs(SYNC_RETRY_COOLDOWN_SECONDS)).await;
            }
        }
        None => futures::future::pending().await,
    }
}

fn service_watch_path(id: &PackageId) -> JsonPointer {
    format!("/public/packageData/{id}").parse().unwrap()
}

async fn service_actor_loop<'a>(
    watch: &mut TypedDbWatch<PackageDataEntry>,
    seed: &'a Arc<ServiceActorSeed>,
    transition: &mut Option<Transition<'a>>,
) -> Result<(), Error> {
    let entry = watch.peek_and_mark_seen()?;
    let kind = required_transition(&entry, &seed.id, transition.as_ref().map(|task| task.kind))?;
    let task = transition.take().filter(|task| Some(task.kind) == kind);
    *transition = task.or_else(|| match kind {
        Some(TransitionKind::Starting) => Some(seed.start()),
        Some(TransitionKind::Stopping) => Some(seed.stop()),
        Some(TransitionKind::BackingUp) => Some(seed.backup()),
        None => None,
    });
    Ok(())
}

fn required_transition(
    entry: &Model<PackageDataEntry>,
    id: &PackageId,
    current: Option<TransitionKind>,
) -> Result<Option<TransitionKind>, Error> {
    if current.is_some() {
        return Ok(current);
    }

    let status = entry.as_status_info().de()?;
    let blocked = entry.has_blocking_task(id)?;

    if blocked && status.started.is_some() {
        return Ok(Some(TransitionKind::Stopping));
    }

    Ok(match status {
        StatusInfo {
            desired: DesiredStatus::Running | DesiredStatus::Restarting { .. },
            started: None,
            ..
        } if !blocked => Some(TransitionKind::Starting),
        StatusInfo {
            desired:
                DesiredStatus::Stopped
                | DesiredStatus::Restarting { .. }
                | DesiredStatus::BackingUp { .. }
                | DesiredStatus::Updating { .. },
            started: Some(_),
            ..
        } => Some(TransitionKind::Stopping),
        StatusInfo {
            desired: DesiredStatus::BackingUp { .. },
            started: None,
            ..
        } => Some(TransitionKind::BackingUp),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::db::model::package::{TaskEntry, TaskSeverity};
    use crate::service::action::update_tasks;
    use crate::service::start_stop::StartStop;

    fn entry(desired: DesiredStatus, started: bool) -> Model<PackageDataEntry> {
        Model::from(
            crate::db::prelude::to_value(&json!({
                "statusInfo": StatusInfo {
                    desired,
                    started: started.then(chrono::Utc::now),
                    ..Default::default()
                },
                "currentDependencies": {},
                "tasks": {},
            }))
            .unwrap(),
        )
    }

    fn add_task(
        entry: &mut Model<PackageDataEntry>,
        name: &str,
        target: &str,
        active: bool,
        severity: TaskSeverity,
    ) {
        let task: TaskEntry = serde_json::from_value(json!({
            "active": active,
            "task": {
                "packageId": target,
                "actionId": "configure",
                "severity": severity,
            },
        }))
        .unwrap();
        entry
            .as_tasks_mut()
            .insert(&name.parse().unwrap(), &task)
            .unwrap();
    }

    #[test]
    fn blocking_tasks_suspend_runtime_without_changing_intent() {
        let id = "service".parse().unwrap();
        for desired in [DesiredStatus::Running, DesiredStatus::Stopped] {
            let running = desired == DesiredStatus::Running;
            let mut entry = entry(desired, running);
            add_task(&mut entry, "first", "service", true, TaskSeverity::Critical);
            add_task(&mut entry, "last", "service", true, TaskSeverity::Critical);
            assert_eq!(
                required_transition(&entry, &id, None).unwrap(),
                running.then_some(TransitionKind::Stopping)
            );
            entry.as_status_info_mut().stopped().unwrap();
            assert_eq!(entry.as_status_info().de().unwrap().desired, desired);
            assert_eq!(required_transition(&entry, &id, None).unwrap(), None);
            entry
                .as_tasks_mut()
                .remove(&"first".parse().unwrap())
                .unwrap();
            assert_eq!(required_transition(&entry, &id, None).unwrap(), None);
            entry
                .as_tasks_mut()
                .remove(&"last".parse().unwrap())
                .unwrap();
            assert_eq!(
                required_transition(&entry, &id, None).unwrap(),
                running.then_some(TransitionKind::Starting)
            );
            assert_eq!(entry.as_status_info().de().unwrap().desired, desired);
        }
    }

    #[test]
    fn task_activation_and_deactivation_reconcile_runtime() {
        let id = "service".parse().unwrap();
        let action = "configure".parse().unwrap();
        let mut entry = entry(DesiredStatus::Running, true);
        let task: TaskEntry = serde_json::from_value(json!({
            "active": false,
            "task": {
                "packageId": "service",
                "actionId": "configure",
                "severity": "critical",
                "when": { "once": false, "condition": "input-not-matches" },
                "input": { "kind": "partial", "accept": [{ "enabled": true }], "set": { "enabled": true } },
            },
        })).unwrap();
        entry
            .as_tasks_mut()
            .insert(&"configure".parse().unwrap(), &task)
            .unwrap();
        assert_eq!(required_transition(&entry, &id, None).unwrap(), None);
        for (input, expected) in [
            (json!({"enabled": false}), Some(TransitionKind::Stopping)),
            (json!({"enabled": true}), Some(TransitionKind::Starting)),
        ] {
            let input = crate::db::prelude::to_value(&input).unwrap();
            entry
                .as_tasks_mut()
                .mutate(|tasks| {
                    update_tasks(tasks, &id, &action, &input, true);
                    Ok(())
                })
                .unwrap();
            assert_eq!(required_transition(&entry, &id, None).unwrap(), expected);
            entry.as_status_info_mut().stopped().unwrap();
            assert_eq!(
                entry.as_status_info().de().unwrap().desired,
                DesiredStatus::Running
            );
        }
    }

    #[test]
    fn nonblocking_tasks_do_not_gate_start() {
        let id = "service".parse().unwrap();
        let mut entry = entry(DesiredStatus::Running, false);
        for (name, target, active, severity) in [
            ("inactive", "service", false, TaskSeverity::Critical),
            ("important", "service", true, TaskSeverity::Important),
            ("optional", "service", true, TaskSeverity::Optional),
            ("unrelated", "other", true, TaskSeverity::Critical),
        ] {
            add_task(&mut entry, name, target, active, severity);
            assert_eq!(
                required_transition(&entry, &id, None).unwrap(),
                Some(TransitionKind::Starting)
            );
        }
    }

    #[test]
    fn blocking_tasks_preserve_backup_update_and_restart() {
        let id = "service".parse().unwrap();
        for desired in [
            DesiredStatus::Restarting {
                restart_again: false,
            },
            DesiredStatus::Restarting {
                restart_again: true,
            },
            DesiredStatus::BackingUp {
                on_complete: StartStop::Start,
            },
            DesiredStatus::BackingUp {
                on_complete: StartStop::Stop,
            },
            DesiredStatus::Updating {
                on_complete: StartStop::Start,
            },
            DesiredStatus::Updating {
                on_complete: StartStop::Stop,
            },
        ] {
            let mut entry = entry(desired, true);
            add_task(
                &mut entry,
                "blocker",
                "service",
                true,
                TaskSeverity::Critical,
            );
            assert_eq!(
                required_transition(&entry, &id, None).unwrap(),
                Some(TransitionKind::Stopping)
            );
            entry.as_status_info_mut().stopped().unwrap();
            let backing_up = matches!(desired, DesiredStatus::BackingUp { .. });
            assert_eq!(
                required_transition(&entry, &id, None).unwrap(),
                backing_up.then_some(TransitionKind::BackingUp)
            );
            assert_eq!(entry.as_status_info().de().unwrap().desired, desired);
            entry
                .as_tasks_mut()
                .remove(&"blocker".parse().unwrap())
                .unwrap();
            let expected = match desired {
                DesiredStatus::Restarting { .. } => Some(TransitionKind::Starting),
                DesiredStatus::BackingUp { .. } => Some(TransitionKind::BackingUp),
                _ => None,
            };
            assert_eq!(required_transition(&entry, &id, None).unwrap(), expected);
        }
    }

    #[test]
    fn blocking_task_drains_in_flight_start_then_stops() {
        let id = "service".parse().unwrap();
        let mut entry = entry(DesiredStatus::Running, false);
        add_task(
            &mut entry,
            "blocker",
            "service",
            true,
            TaskSeverity::Critical,
        );
        assert_eq!(
            required_transition(&entry, &id, Some(TransitionKind::Starting)).unwrap(),
            Some(TransitionKind::Starting)
        );
        assert_eq!(required_transition(&entry, &id, None).unwrap(), None);
        entry.as_status_info_mut().started().unwrap();
        assert_eq!(
            required_transition(&entry, &id, Some(TransitionKind::Starting)).unwrap(),
            Some(TransitionKind::Starting)
        );
        assert_eq!(
            required_transition(&entry, &id, None).unwrap(),
            Some(TransitionKind::Stopping)
        );
    }

    #[test]
    fn explicit_stop_drains_in_flight_start_then_stops() {
        let id = "service".parse().unwrap();
        let mut entry = entry(DesiredStatus::Running, false);
        entry.as_status_info_mut().stop().unwrap();
        assert_eq!(
            required_transition(&entry, &id, Some(TransitionKind::Starting)).unwrap(),
            Some(TransitionKind::Starting)
        );
        entry.as_status_info_mut().started().unwrap();
        assert_eq!(
            entry.as_status_info().de().unwrap().desired,
            DesiredStatus::Stopped
        );
        assert_eq!(
            required_transition(&entry, &id, None).unwrap(),
            Some(TransitionKind::Stopping)
        );
    }

    #[tokio::test]
    async fn completed_transition_reconciles_without_database_wakeup() {
        let id = "service".parse().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let db = patch_db::PatchDb::open(dir.path().join("db"))
            .await
            .unwrap();
        db.put(
            &patch_db::json_ptr::ROOT,
            &json!({ "public": { "packageData": {} } }),
        )
        .await
        .unwrap();
        let path = service_watch_path(&id);
        for (kind, started, blocked, next) in [
            (
                TransitionKind::Starting,
                true,
                true,
                TransitionKind::Stopping,
            ),
            (
                TransitionKind::Stopping,
                false,
                false,
                TransitionKind::Starting,
            ),
            (
                TransitionKind::BackingUp,
                false,
                false,
                TransitionKind::Starting,
            ),
        ] {
            let mut entry = entry(DesiredStatus::Running, started);
            if blocked {
                add_task(
                    &mut entry,
                    "blocker",
                    "service",
                    true,
                    TaskSeverity::Critical,
                );
            }
            db.put(&path, &entry).await.unwrap();
            let mut watch = db.watch(path.clone()).await.typed::<PackageDataEntry>();
            let observed = watch.peek_and_mark_seen().unwrap();
            let (complete, completed) = tokio::sync::oneshot::channel();
            let mut transition = Some(Transition {
                kind,
                future: Box::pin(async move {
                    completed.await.unwrap();
                    Ok(())
                }),
            });
            assert_eq!(
                required_transition(&observed, &id, Some(kind)).unwrap(),
                Some(kind)
            );
            complete.send(()).unwrap();
            {
                let wait = watch.changed();
                let finished = finish_transition(&mut transition);
                tokio::pin!(wait, finished);
                let result = tokio::time::timeout(
                    Duration::from_secs(1),
                    futures::future::select(wait, finished),
                )
                .await
                .unwrap();
                assert!(matches!(result, futures::future::Either::Right(_)));
            }
            assert!(transition.is_none());
            assert_eq!(
                required_transition(&watch.peek_and_mark_seen().unwrap(), &id, None).unwrap(),
                Some(next)
            );
        }
    }

    #[test]
    fn clearing_blocker_retains_stop_until_completion() {
        let id = "service".parse().unwrap();
        for started in [false, true] {
            let mut entry = entry(DesiredStatus::Running, started);
            add_task(
                &mut entry,
                "blocker",
                "service",
                true,
                TaskSeverity::Critical,
            );
            assert_eq!(
                required_transition(&entry, &id, Some(TransitionKind::Stopping)).unwrap(),
                Some(TransitionKind::Stopping)
            );
            entry
                .as_tasks_mut()
                .remove(&"blocker".parse().unwrap())
                .unwrap();
            assert_eq!(
                required_transition(&entry, &id, Some(TransitionKind::Stopping)).unwrap(),
                Some(TransitionKind::Stopping)
            );
            entry.as_status_info_mut().stopped().unwrap();
            assert_eq!(
                required_transition(&entry, &id, None).unwrap(),
                Some(TransitionKind::Starting)
            );
        }
    }

    #[test]
    fn explicit_stop_retains_in_flight_backup() {
        let id = "service".parse().unwrap();
        let mut entry = entry(
            DesiredStatus::BackingUp {
                on_complete: StartStop::Start,
            },
            false,
        );
        entry.as_status_info_mut().stop().unwrap();
        assert_eq!(
            entry.as_status_info().de().unwrap().desired,
            DesiredStatus::BackingUp {
                on_complete: StartStop::Stop,
            }
        );
        assert_eq!(
            required_transition(&entry, &id, Some(TransitionKind::BackingUp)).unwrap(),
            Some(TransitionKind::BackingUp)
        );
    }

    #[test]
    fn no_op_reconciliation_retains_in_flight_start() {
        let id = "service".parse().unwrap();
        for (desired, started) in [
            (DesiredStatus::Stopped, false),
            (DesiredStatus::Running, true),
        ] {
            let entry = entry(desired, started);
            assert_eq!(
                required_transition(&entry, &id, Some(TransitionKind::Starting)).unwrap(),
                Some(TransitionKind::Starting)
            );
            assert_eq!(required_transition(&entry, &id, None).unwrap(), None);
        }
    }

    #[test]
    fn restart_stops_then_starts_after_stop_completes() {
        let id = "service".parse().unwrap();
        for restart_again in [false, true] {
            let mut entry = entry(DesiredStatus::Restarting { restart_again }, true);
            assert_eq!(
                required_transition(&entry, &id, Some(TransitionKind::Starting)).unwrap(),
                Some(TransitionKind::Starting)
            );
            assert_eq!(
                required_transition(&entry, &id, None).unwrap(),
                Some(TransitionKind::Stopping)
            );
            entry.as_status_info_mut().stopped().unwrap();
            assert_eq!(
                required_transition(&entry, &id, Some(TransitionKind::Stopping)).unwrap(),
                Some(TransitionKind::Stopping)
            );
            assert_eq!(
                required_transition(&entry, &id, None).unwrap(),
                Some(TransitionKind::Starting)
            );
            entry.as_status_info_mut().started().unwrap();
            assert_eq!(
                required_transition(&entry, &id, None).unwrap(),
                restart_again.then_some(TransitionKind::Stopping)
            );
        }
    }

    #[tokio::test]
    async fn service_watch_reconciles_task_and_dependency_changes() {
        let id = "service".parse().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let db = patch_db::PatchDb::open(dir.path().join("db"))
            .await
            .unwrap();
        let path = service_watch_path(&id);
        let mut entry = entry(DesiredStatus::Running, false);
        db.put(
            &patch_db::json_ptr::ROOT,
            &json!({ "public": { "packageData": { "service": entry } } }),
        )
        .await
        .unwrap();
        let mut watch = db.watch(path.clone()).await.typed::<PackageDataEntry>();
        assert_eq!(
            required_transition(&watch.peek_and_mark_seen().unwrap(), &id, None).unwrap(),
            Some(TransitionKind::Starting)
        );
        add_task(
            &mut entry,
            "dependency",
            "other",
            true,
            TaskSeverity::Critical,
        );
        db.put(&path, &entry).await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), watch.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            required_transition(&watch.peek_and_mark_seen().unwrap(), &id, None).unwrap(),
            Some(TransitionKind::Starting)
        );
        entry
            .as_current_dependencies_mut()
            .ser(
                &serde_json::from_value(json!({
                    "other": { "kind": "exists", "versionRange": "*", "title": null, "icon": null },
                }))
                .unwrap(),
            )
            .unwrap();
        db.put(&path, &entry).await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), watch.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            required_transition(&watch.peek_and_mark_seen().unwrap(), &id, None).unwrap(),
            None
        );
        entry
            .as_current_dependencies_mut()
            .ser(&Default::default())
            .unwrap();
        db.put(&path, &entry).await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), watch.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            required_transition(&watch.peek_and_mark_seen().unwrap(), &id, None).unwrap(),
            Some(TransitionKind::Starting)
        );
        assert_eq!(
            watch
                .peek_and_mark_seen()
                .unwrap()
                .as_status_info()
                .de()
                .unwrap()
                .desired,
            DesiredStatus::Running
        );
    }
}
