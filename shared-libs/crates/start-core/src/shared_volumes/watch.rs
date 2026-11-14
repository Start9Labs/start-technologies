//! Observer futures share the RPC task rather than spawning tasks per source.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use futures::{FutureExt, StreamExt, TryStreamExt, stream};
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::{SharedVolumeAvailability, SharedVolumeSource, source};
use crate::context::RpcContext;
use crate::prelude::*;
use crate::util::actor::background::BackgroundJobQueue;
use crate::util::future::{CancellationHandle, Until};

mod events;
mod mounted;
mod registered;

#[derive(Debug)]
enum Event {
    Disks,
    RescanDisks,
    Network,
    Probe(mounted::ProbeUpdate),
    Inspected(Result<(), Error>),
}

#[derive(Debug, PartialEq, Eq)]
enum InspectionEvent {
    None,
    Restart,
}

fn inspection_event(event: Option<&Event>, source: &SharedVolumeSource) -> InspectionEvent {
    match (event, source) {
        (Some(Event::Disks), SharedVolumeSource::Disk { .. }) => InspectionEvent::Restart,
        _ => InspectionEvent::None,
    }
}

struct Retry {
    next: Instant,
    delay: Duration,
}

impl Retry {
    fn new(now: Instant) -> Self {
        Self {
            next: now,
            delay: Duration::from_secs(2),
        }
    }

    fn take(&mut self, now: Instant) -> bool {
        if now < self.next {
            return false;
        }
        self.next = now + self.delay;
        self.delay = (self.delay * 2).min(Duration::from_secs(300));
        true
    }
}

struct Observer {
    mounted: mounted::MountedChecks,
    running: BTreeMap<registered::SourceId, (source::SourceDefinition, CancellationHandle)>,
    retries: BTreeMap<registered::SourceId, Retry>,
    disk_scan: Option<CancellationHandle>,
    retire_at: Option<Instant>,
    startup: bool,
}

impl Observer {
    /// The caller has already observed a live mount.
    fn observe_existing_source(&mut self, source: &registered::RegisteredSource) {
        if source.inspection.availability == SharedVolumeAvailability::Unknown {
            self.mounted.invalidate();
        }
    }

    fn cancel_inspection(&mut self, id: &registered::SourceId) {
        if let Some((_, mut job)) = self.running.remove(id) {
            job.cancel();
        }
    }

    fn reconcile_sources(
        &mut self,
        sources: &BTreeMap<registered::SourceId, registered::RegisteredSource>,
    ) {
        self.running.retain(|id, (definition, job)| {
            let keep = !job.is_finished()
                && sources
                    .get(id)
                    .is_some_and(|source| source.definition == *definition);
            if !keep {
                job.cancel();
            }
            keep
        });
        self.retries
            .retain(|id, _| sources.get(id).is_some_and(|source| source.retryable()));
    }

    fn new(send: mpsc::Sender<Event>, jobs: BackgroundJobQueue) -> Self {
        Self {
            mounted: mounted::MountedChecks::new(send, jobs),
            running: BTreeMap::new(),
            retries: BTreeMap::new(),
            disk_scan: None,
            retire_at: None,
            startup: true,
        }
    }
}

pub(crate) async fn run(context: impl Fn() -> Option<RpcContext> + Send + Sync) {
    let (jobs, mut runner) = BackgroundJobQueue::new();
    let (send, mut recv) = mpsc::channel(64);
    jobs.add_job(events::watch(send.clone()));
    let mut observer = Observer::new(send.clone(), jobs.clone());
    runner
        .run_while(async {
            loop {
                match watch(&context, &jobs, &send, &mut recv, &mut observer).await {
                    Ok(()) => break,
                    Err(error) => {
                        tracing::error!(
                            "{}",
                            t!("shared-volumes.availability-watch-failed", error = error)
                        );
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        })
        .await;
}

async fn watch(
    context: &(impl Fn() -> Option<RpcContext> + Send + Sync),
    jobs: &BackgroundJobQueue,
    send: &mpsc::Sender<Event>,
    recv: &mut mpsc::Receiver<Event>,
    observer: &mut Observer,
) -> Result<(), Error> {
    let Some(ctx) = context() else { return Ok(()) };
    let db = ctx.db.clone();
    let disks = ctx.disk_updates.subscribe();
    drop(ctx);
    let catalog = db.watch(patch_db::json_ptr::ROOT.to_owned()).await;
    let mut changes = stream::select_all(vec![
        registered::changes(catalog.map_err(Error::from))
            .map_ok(|_| None)
            .boxed(),
        stream::unfold(disks, |mut disks| async move {
            let result = disks
                .changed()
                .await
                .with_kind(ErrorKind::Unknown)
                .map(|_| Some(Event::RescanDisks));
            Some((result, disks))
        })
        .boxed(),
        stream::unfold(recv, |recv| async {
            recv.recv().await.map(|event| (Ok(Some(event)), recv))
        })
        .boxed(),
    ]);
    let mut next_event = None;
    loop {
        let Some(ctx) = context() else { break };
        let event = match next_event.take() {
            Some(Event::Probe(update)) => {
                observer.mounted.publish(&ctx, update).await;
                None
            }
            Some(Event::Inspected(result)) => {
                result?;
                None
            }
            event => event,
        };
        let snapshot = db.peek().await;
        let sources = registered::snapshot(&snapshot)?;
        observer.reconcile_sources(&sources);
        if matches!(event, Some(Event::Disks))
            || observer
                .retire_at
                .is_some_and(|deadline| deadline <= Instant::now())
        {
            let (retired, pending) =
                crate::disk::mount::guard::TmpMountGuard::retire_changed_devices().await;
            observer.retire_at = pending.then(|| Instant::now() + Duration::from_secs(1));
            let mut affected = BTreeSet::new();
            for (id, source) in &sources {
                if let SharedVolumeSource::Disk { logicalname, .. } = source.definition.source() {
                    if retired.contains(logicalname) {
                        if let registered::SourceId::Volume(id) = id {
                            affected.extend(
                                snapshot
                                    .as_public()
                                    .as_shared_volumes()
                                    .as_idx(id)
                                    .or_not_found(id)?
                                    .as_grants()
                                    .de()?
                                    .into_keys(),
                            );
                        }
                        observer.cancel_inspection(id);
                        source
                            .record(
                                &ctx,
                                None,
                                super::SharedVolumeInspection::unavailable(
                                    t!("shared-volumes.source-unavailable").to_string(),
                                ),
                            )
                            .await?;
                    }
                }
            }
            for package in affected {
                ctx.callbacks
                    .invalidate_shared_volumes(&package)
                    .await
                    .log_err();
            }
        }
        let server_id = snapshot.as_public().as_server_info().as_id().de()?;
        observer.mounted.tick(&ctx, &sources, &server_id).await;
        if observer.startup || matches!(event, Some(Event::Disks | Event::RescanDisks)) {
            if let Some(mut scan) = observer.disk_scan.take() {
                scan.cancel_and_wait().await;
            }
            if matches!(event, Some(Event::Disks)) {
                crate::disk::discovery::remove_missing(&db).await.log_err();
            }
            observer.disk_scan = Some(jobs.add_cancellable_job(crate::disk::discovery::refresh(
                db.clone(),
                ctx.os_partitions.clone(),
            )));
        }
        let mut deadline = observer.mounted.deadline();
        if let Some(retire_at) = observer.retire_at {
            deadline = deadline.min(retire_at);
        }
        for (id, source) in sources {
            let trigger = inspection_event(event.as_ref(), source.definition.source());
            let full =
                source.needs_inspection(observer.startup, trigger == InspectionEvent::Restart);
            if matches!(source.definition.source(), SharedVolumeSource::Cifs { .. })
                && (full || source.retryable())
                && source
                    .existing()
                    .await
                    .log_err()
                    .is_some_and(|mounts| !mounts.is_empty())
            {
                observer.observe_existing_source(&source);
                continue;
            }
            if trigger == InspectionEvent::Restart {
                observer.cancel_inspection(&id);
            }
            if full {
                if let SharedVolumeSource::Disk { logicalname, .. } = source.definition.source() {
                    if !tokio::fs::try_exists(logicalname).await? {
                        observer.cancel_inspection(&id);
                        source
                            .record(
                                &ctx,
                                None,
                                super::SharedVolumeInspection::unavailable(
                                    t!("shared-volumes.source-unavailable").to_string(),
                                ),
                            )
                            .await?;
                        continue;
                    }
                }
            }
            if observer.running.contains_key(&id) {
                continue;
            }
            let tcp = if source.retryable() {
                let retry = observer
                    .retries
                    .entry(id.clone())
                    .or_insert_with(|| Retry::new(Instant::now()));
                let due = retry.take(Instant::now());
                if !due {
                    deadline = deadline.min(retry.next);
                }
                due
            } else {
                false
            };
            if full || tcp {
                let ctx = ctx.clone();
                let send = send.clone();
                observer.running.insert(
                    id,
                    (
                        source.definition.clone(),
                        jobs.add_cancellable_job(async move {
                            let result = if full {
                                source.inspect(ctx).await
                            } else {
                                source.retry(ctx).await
                            };
                            let _ = send.send(Event::Inspected(result)).await;
                        }),
                    ),
                );
            }
        }
        observer.startup = false;
        deadline = deadline.min(observer.mounted.deadline());
        drop(ctx);
        next_event = wait_for_change(&mut changes, deadline).await?;
    }
    Ok(())
}

async fn wait_for_change(
    changes: &mut (impl futures::Stream<Item = Result<Option<Event>, Error>> + Unpin + Send),
    deadline: Instant,
) -> Result<Option<Event>, Error> {
    let mut next = None;
    {
        let mut until = Until::new()
            .with_stream(changes.by_ref().map(|change| {
                next = change?;
                Ok(())
            }))
            .with_async_fn(move || {
                async move {
                    tokio::time::sleep_until(deadline).await;
                    Ok(())
                }
                .fuse()
            });
        until.run(std::future::pending()).await?;
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_activity_reuses_pending_checks_but_device_replacement_restarts_them() {
        let cifs = SharedVolumeSource::Cifs {
            hostname: "nas".into(),
            path: "/share".into(),
            username: "user".into(),
        };
        let disk = SharedVolumeSource::Disk {
            logicalname: "/dev/disk/by-uuid/data".into(),
            path: "media".into(),
            drive: None,
        };
        assert_eq!(
            inspection_event(Some(&Event::Network), &cifs),
            InspectionEvent::None
        );
        assert_eq!(
            inspection_event(Some(&Event::Disks), &disk),
            InspectionEvent::Restart
        );
        assert_eq!(
            inspection_event(Some(&Event::Network), &disk),
            InspectionEvent::None
        );
        assert_eq!(
            inspection_event(Some(&Event::Disks), &cifs),
            InspectionEvent::None
        );
        assert_eq!(inspection_event(None, &cifs), InspectionEvent::None);
    }

    #[tokio::test]
    async fn source_refresh_preserves_unrelated_work_and_backoff() {
        use crate::shared_volumes::{
            SharedVolumeCredentials, SharedVolumeId, SharedVolumeInspection,
        };
        let (jobs, mut runner) = BackgroundJobQueue::new();
        let (send, _) = mpsc::channel(64);
        let mut observer = Observer::new(send, jobs.clone());
        let (started, mut starts) = mpsc::channel(2);
        let mut sources = BTreeMap::new();
        let mut handles = Vec::new();
        for _ in 0..2 {
            let id = registered::SourceId::Volume(SharedVolumeId::new());
            let definition = source::SourceDefinition::new(
                SharedVolumeSource::Cifs {
                    hostname: "nas".into(),
                    path: "/share".into(),
                    username: "user".into(),
                },
                Some(SharedVolumeCredentials {
                    password: Some("original".into()),
                }),
            );
            let started = started.clone();
            let handle = jobs.add_cancellable_job(async move {
                started.send(()).await.unwrap();
                std::future::pending::<()>().await;
            });
            handles.push((id.clone(), handle.clone()));
            observer
                .running
                .insert(id.clone(), (definition.clone(), handle));
            sources.insert(
                id.clone(),
                registered::RegisteredSource {
                    id: id.clone(),
                    definition,
                    inspection: SharedVolumeInspection::unavailable("offline".into()),
                    check_unmounted: true,
                },
            );
            let mut retry = Retry::new(Instant::now());
            retry.take(Instant::now());
            observer.retries.insert(id, retry);
        }
        runner
            .run_while(async {
                starts.recv().await.unwrap();
                starts.recv().await.unwrap();
                let (first, rest) = handles.split_at_mut(1);
                let (unchanged, _) = &first[0];
                let next = observer.retries[unchanged].next;
                sources.get_mut(unchanged).unwrap().inspection =
                    SharedVolumeInspection::unavailable("still offline".into());
                observer.reconcile_sources(&sources);
                assert_eq!(observer.running.len(), 2);
                assert_eq!(observer.retries[unchanged].next, next);
                let (changed, cancelled) = &mut rest[0];
                let source = sources.get_mut(changed).unwrap();
                source.definition = source::SourceDefinition::new(
                    source.definition.source().clone(),
                    Some(SharedVolumeCredentials {
                        password: Some("rotated".into()),
                    }),
                );
                observer.reconcile_sources(&sources);
                cancelled.cancel_and_wait().await;
                assert!(cancelled.is_finished());
                assert_eq!(observer.running.len(), 1);
                assert!(observer.running.contains_key(unchanged));
                assert_eq!(observer.retries[unchanged].next, next);
                let mut remaining = first[0].1.clone();
                observer.cancel_inspection(unchanged);
                remaining.cancel_and_wait().await;
                assert!(observer.running.is_empty());
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn unknown_existing_cifs_with_pending_probe_waits_between_health_scans() {
        use crate::shared_volumes::{SharedVolumeId, SharedVolumeInspection};
        let (jobs, mut runner) = BackgroundJobQueue::new();
        let (send, _) = mpsc::channel(64);
        let mut observer = Observer::new(send, jobs.clone());
        let source = registered::RegisteredSource {
            id: registered::SourceId::Volume(SharedVolumeId::new()),
            definition: source::SourceDefinition::new(
                SharedVolumeSource::Cifs {
                    hostname: "nas".into(),
                    path: "/share".into(),
                    username: "user".into(),
                },
                None,
            ),
            inspection: SharedVolumeInspection::default(),
            check_unmounted: true,
        };
        assert!(source.needs_inspection(false, false));
        let (started, ready) = tokio::sync::oneshot::channel();
        let mut pending_probe = jobs.add_cancellable_job(async move {
            started.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        runner
            .run_while(async {
                ready.await.unwrap();
                let mut quiet = stream::pending();
                for _ in 0..4 {
                    assert!(observer.mounted.take_due_tick().is_some());
                    let scanned = Instant::now();
                    observer.observe_existing_source(&source);
                    assert!(
                        wait_for_change(&mut quiet, observer.mounted.deadline())
                            .await
                            .unwrap()
                            .is_none()
                    );
                    assert!(Instant::now() - scanned >= Duration::from_secs(1));
                    assert!(!pending_probe.is_finished());
                    assert_eq!(
                        source.inspection.availability,
                        SharedVolumeAvailability::Unknown
                    );
                }
                pending_probe.cancel_and_wait().await;
            })
            .await;
    }

    #[tokio::test]
    async fn control_scope_errors_events_and_deadlines_leave_pending_work_owned() {
        let (jobs, mut runner) = BackgroundJobQueue::new();
        let (send, recv) = tokio::sync::oneshot::channel();
        let mut job = jobs.add_cancellable_job(async move {
            send.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        runner
            .run_while(async {
                recv.await.unwrap();
                let mut broken = stream::iter([Err(Error::new(
                    eyre!("fixture disconnected"),
                    ErrorKind::Network,
                ))]);
                assert!(
                    wait_for_change(&mut broken, Instant::now() + Duration::from_secs(1))
                        .await
                        .is_err()
                );
                assert!(!job.is_finished());
                let mut network = stream::iter([Ok(Some(Event::Network))]);
                assert!(matches!(
                    wait_for_change(&mut network, Instant::now() + Duration::from_secs(1))
                        .await
                        .unwrap(),
                    Some(Event::Network)
                ));
                assert!(!job.is_finished());
                let mut quiet = stream::pending();
                assert!(
                    wait_for_change(&mut quiet, Instant::now() + Duration::from_millis(1))
                        .await
                        .unwrap()
                        .is_none()
                );
                assert!(!job.is_finished());
                job.cancel_and_wait().await;
            })
            .await;
    }

    #[test]
    fn retries_back_off_to_five_minutes() {
        let now = Instant::now();
        let mut retry = Retry::new(now);
        assert!(retry.take(now));
        assert!(!retry.take(now + Duration::from_secs(1)));
        assert!(retry.take(now + Duration::from_secs(2)));
        assert_eq!(retry.next, now + Duration::from_secs(6));
        for _ in 0..20 {
            assert!(retry.take(retry.next));
        }
        assert_eq!(retry.delay, Duration::from_secs(300));
        let next = retry.next;
        assert!(retry.take(next));
        assert_eq!(retry.next - next, Duration::from_secs(300));
    }
}
