use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::Instant;

use super::Event;
use super::cifs::DebugData;
use crate::context::RpcContext;
use crate::disk::mount::guard::WeakMountGuard;
use crate::prelude::*;
use crate::shared_volumes::source::{self, MountedSource};
use crate::shared_volumes::{
    SharedVolume, SharedVolumeAvailability, SharedVolumeId, SharedVolumeInspection,
    SharedVolumeSource,
};
use crate::util::future::NonDetachingJoinHandle;

const HEALTH_INTERVAL: Duration = Duration::from_secs(30);
const CAPACITY_INTERVAL: Duration = Duration::from_secs(180);

#[derive(Debug)]
pub(super) enum SpaceObservation {
    TimedOut,
    Finished(Result<u64, String>),
}

#[derive(Debug)]
pub(super) struct ProbeUpdate {
    mount: WeakMountGuard,
    observation: SpaceObservation,
}

struct Group {
    mount: WeakMountGuard,
    path: PathBuf,
    cifs: bool,
    sources: Vec<MountedSource>,
}

struct Probe {
    mount: WeakMountGuard,
    connection: Option<bool>,
    sources: Vec<MountedSource>,
    _task: NonDetachingJoinHandle<()>,
}

pub(super) struct MountedChecks {
    next_health: Instant,
    next_capacity: Instant,
    probes: BTreeMap<usize, Probe>,
    send: mpsc::Sender<Event>,
}

impl MountedChecks {
    pub(super) fn new(send: mpsc::Sender<Event>) -> Self {
        Self {
            next_health: Instant::now(),
            next_capacity: Instant::now(),
            probes: BTreeMap::new(),
            send,
        }
    }

    pub(super) fn invalidate(&mut self) {
        self.next_health = Instant::now();
    }

    pub(super) async fn publish(&mut self, ctx: &RpcContext, update: ProbeUpdate) {
        let key = update.mount.key();
        let finished = matches!(&update.observation, SpaceObservation::Finished(_));
        if let Some(probe) = self.probes.get(&key) {
            let result = match update.observation {
                SpaceObservation::TimedOut => Err(format!(
                    "{}: {}",
                    t!("shared-volumes.source-unavailable"),
                    ErrorKind::Timeout
                )),
                SpaceObservation::Finished(result) => result,
            };
            let inspection = space_inspection(probe.connection, result);
            for source in &probe.sources {
                source.record(ctx, inspection.clone()).await.log_err();
            }
        }
        if finished {
            self.probes.remove(&key);
        }
    }

    pub(super) async fn tick(
        &mut self,
        ctx: &RpcContext,
        volumes: &BTreeMap<SharedVolumeId, SharedVolume>,
    ) {
        // Definition changes must not reuse an unfinished probe's notification key.
        self.probes
            .retain(|_, probe| probe.mount.active_path().is_some());
        let now = Instant::now();
        if now < self.next_health {
            return;
        }
        self.next_health = now + HEALTH_INTERVAL;
        let capacity_due = now >= self.next_capacity;
        if capacity_due {
            self.next_capacity = now + CAPACITY_INTERVAL;
        }

        let debug = tokio::fs::read_to_string("/proc/fs/cifs/DebugData")
            .await
            .ok()
            .and_then(|input| DebugData::parse(&input));
        let mounts = procfs::process::Process::myself()
            .and_then(|process| process.mountinfo())
            .ok();
        let mut groups = BTreeMap::<usize, Group>::new();
        for (id, volume) in volumes {
            let Some(source) = source::existing_source(ctx, id).await.log_err().flatten() else {
                continue;
            };
            let Some(path) = source.mount.active_path() else {
                continue;
            };
            groups
                .entry(source.mount.key())
                .or_insert_with(|| Group {
                    mount: source.mount.clone(),
                    path,
                    cifs: matches!(volume.source, SharedVolumeSource::Cifs { .. }),
                    sources: Vec::new(),
                })
                .sources
                .push(source);
        }
        for (key, group) in groups {
            let connection = if group.cifs {
                debug
                    .as_ref()
                    .zip(mounts.as_ref())
                    .and_then(|(debug, mounts)| {
                        let mount = mounts
                            .0
                            .iter()
                            .find(|mount| mount.mount_point == group.path)?;
                        debug.health(mount.mount_source.as_deref()?)
                    })
            } else {
                None
            };
            if group.cifs {
                if let Some(healthy) = connection {
                    for source in &group.sources {
                        source.record_connection(ctx, healthy).await.log_err();
                    }
                }
            }
            if let Some(probe) = self.probes.get_mut(&key) {
                probe.connection = connection;
                probe.sources = group.sources;
                continue;
            }
            let fallback = group.cifs && connection.is_none();
            let capacity = group.sources.iter().any(|source| {
                should_measure_capacity(&volumes[source.id()].inspection, connection, capacity_due)
            });
            if fallback || capacity {
                let sender = self.send.clone();
                let mount = group.mount.clone();
                let path = group.path;
                let task = tokio::spawn(async move {
                    let operation = async {
                        crate::disk::util::get_available(&path)
                            .await
                            .map_err(source::public_error_message)
                    };
                    observe_space(operation, source::SPACE_QUERY_TIMEOUT, move |observation| {
                        let sender = sender.clone();
                        let mount = mount.clone();
                        async move {
                            sender
                                .send(Event::Probe(ProbeUpdate { mount, observation }))
                                .await
                                .is_ok()
                        }
                    })
                    .await;
                })
                .into();
                self.probes.insert(
                    key,
                    Probe {
                        mount: group.mount,
                        connection,
                        sources: group.sources,
                        _task: task,
                    },
                );
            }
        }
    }
}

fn should_measure_capacity(
    inspection: &SharedVolumeInspection,
    connection: Option<bool>,
    deadline_elapsed: bool,
) -> bool {
    connection != Some(false)
        && (deadline_elapsed || inspection.checked_at.is_none())
        && (connection == Some(true)
            || inspection.availability == SharedVolumeAvailability::Available)
}

fn space_inspection(
    connection: Option<bool>,
    result: Result<u64, String>,
) -> SharedVolumeInspection {
    if connection == Some(false) {
        return SharedVolumeInspection::unavailable(
            t!("shared-volumes.source-unavailable").to_string(),
        );
    }
    match result {
        Ok(bytes) => SharedVolumeInspection::available(Some(bytes)),
        Err(error) => SharedVolumeInspection::observed(
            if connection == Some(true) {
                SharedVolumeAvailability::Available
            } else {
                SharedVolumeAvailability::Unavailable
            },
            None,
            Some(error),
        ),
    }
}

async fn observe_space<F, N, NF>(operation: F, deadline: Duration, mut notify: N)
where
    F: Future<Output = Result<u64, String>>,
    N: FnMut(SpaceObservation) -> NF,
    NF: Future<Output = bool>,
{
    tokio::pin!(operation);
    tokio::select! {
        result = &mut operation => { notify(SpaceObservation::Finished(result)).await; }
        _ = tokio::time::sleep(deadline) => {
            if notify(SpaceObservation::TimedOut).await {
                notify(SpaceObservation::Finished(operation.await)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_scheduling_respects_health_freshness_and_slow_deadlines() {
        let available = SharedVolumeInspection::available(Some(1));
        assert!(!should_measure_capacity(&available, None, false));
        assert!(should_measure_capacity(&available, None, true));
        assert!(!should_measure_capacity(&available, Some(false), true));
        let unknown = SharedVolumeInspection::default();
        assert!(!should_measure_capacity(&unknown, None, true));
        assert!(should_measure_capacity(&unknown, Some(true), false));
        let unavailable = SharedVolumeInspection::unavailable("offline".into());
        assert!(!should_measure_capacity(&unavailable, None, true));
        assert!(should_measure_capacity(&unavailable, Some(true), true));
    }

    #[test]
    fn capacity_failure_does_not_override_known_kernel_health() {
        let healthy = space_inspection(Some(true), Err("capacity query failed".into()));
        assert_eq!(healthy.availability, SharedVolumeAvailability::Available);
        assert_eq!(healthy.available, None);
        assert_eq!(healthy.error.as_deref(), Some("capacity query failed"));
        assert!(healthy.checked_at.is_some());
        let unavailable = space_inspection(None, Err("query timed out".into()));
        assert_eq!(
            unavailable.availability,
            SharedVolumeAvailability::Unavailable
        );
        assert_eq!(unavailable.available, None);
        let down = space_inspection(Some(false), Ok(123));
        assert_eq!(down.availability, SharedVolumeAvailability::Unavailable);
        assert_eq!(down.available, None);
        let zero = space_inspection(None, Ok(0));
        assert_eq!(zero.availability, SharedVolumeAvailability::Available);
        assert_eq!(zero.available, Some(0));
        assert_eq!(zero.error, None);
    }

    #[tokio::test]
    async fn deadline_reports_failure_without_finishing_the_pending_operation() {
        let (complete, pending) = tokio::sync::oneshot::channel();
        let (send, mut receive) = mpsc::unbounded_channel();
        let task = tokio::spawn(observe_space(
            async { pending.await.unwrap() },
            Duration::from_millis(1),
            move |observation| std::future::ready(send.send(observation).is_ok()),
        ));
        assert!(matches!(
            receive.recv().await,
            Some(SpaceObservation::TimedOut)
        ));
        assert!(!task.is_finished());
        complete.send(Ok(42)).unwrap();
        assert!(matches!(
            receive.recv().await,
            Some(SpaceObservation::Finished(Ok(42)))
        ));
        task.await.unwrap();
        assert!(receive.recv().await.is_none());
    }

    #[tokio::test]
    async fn closed_observer_drops_a_pending_query_after_the_deadline() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Dropped(Arc<AtomicBool>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let dropped = Arc::new(AtomicBool::new(false));
        let guard = Dropped(dropped.clone());
        observe_space(
            async move {
                let _guard = guard;
                std::future::pending().await
            },
            Duration::from_millis(1),
            |observation| {
                assert!(matches!(observation, SpaceObservation::TimedOut));
                std::future::ready(false)
            },
        )
        .await;
        assert!(dropped.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn completed_query_reports_once_without_a_deadline_error() {
        let (send, mut receive) = mpsc::unbounded_channel();
        observe_space(
            std::future::ready(Ok(7)),
            Duration::from_secs(60),
            move |observation| std::future::ready(send.send(observation).is_ok()),
        )
        .await;
        assert!(matches!(
            receive.recv().await,
            Some(SpaceObservation::Finished(Ok(7)))
        ));
        assert!(receive.recv().await.is_none());
    }
}
