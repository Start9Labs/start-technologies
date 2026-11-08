use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::Instant;

use super::Event;
use super::registered::{DirectoryContents, MountedSource, RegisteredSource, SourceId};
use crate::context::RpcContext;
use crate::disk::mount::filesystem::cifs::health::KernelState;
use crate::disk::mount::guard::WeakMountGuard;
use crate::prelude::*;
use crate::shared_volumes::{SharedVolumeInspection, SharedVolumeSource, source};
use crate::util::future::NonDetachingJoinHandle;

const HEALTH_INTERVAL: Duration = Duration::from_secs(30);
const CAPACITY_INTERVAL: Duration = Duration::from_secs(180);

#[derive(Debug)]
enum Observation<T> {
    TimedOut,
    Finished(Result<T, String>),
}

#[derive(Debug)]
enum ProbeObservation {
    Space(Observation<u64>),
    Directory(PathBuf, Observation<DirectoryContents>),
}

#[derive(Debug)]
pub(super) struct ProbeUpdate {
    mount: WeakMountGuard,
    observation: ProbeObservation,
}

struct Group {
    mount: WeakMountGuard,
    path: PathBuf,
    cifs: bool,
    sources: Vec<MountedSource>,
}

struct Query<T> {
    result: Option<Result<T, String>>,
    task: Option<NonDetachingJoinHandle<()>>,
}

impl<T> Default for Query<T> {
    fn default() -> Self {
        Self {
            result: None,
            task: None,
        }
    }
}

impl<T> Query<T> {
    fn update(&mut self, observation: Observation<T>) {
        if matches!(observation, Observation::Finished(_)) {
            self.task = None;
        }
        self.result = Some(match observation {
            Observation::TimedOut => Err(format!(
                "{}: {}",
                t!("shared-volumes.source-unavailable"),
                ErrorKind::Timeout
            )),
            Observation::Finished(result) => result,
        });
    }
}

impl Query<DirectoryContents> {
    fn take_contents(&mut self) -> Option<DirectoryContents> {
        match self.result.as_mut() {
            Some(Ok(contents @ DirectoryContents::Target(_))) => {
                Some(std::mem::replace(contents, DirectoryContents::Folder))
            }
            _ => None,
        }
    }
}

struct Probe {
    mount: WeakMountGuard,
    connection: Option<bool>,
    sources: Vec<MountedSource>,
    space: Query<u64>,
    // Folder recovery remains independent of unfinished capacity queries.
    directories: BTreeMap<PathBuf, Query<DirectoryContents>>,
}

impl Probe {
    async fn publish(&mut self, ctx: &RpcContext) {
        let paths: BTreeSet<_> = self
            .sources
            .iter()
            .map(MountedSource::relative_path)
            .collect();
        for path in paths {
            self.publish_directory(ctx, &path).await;
        }
    }

    async fn publish_directory(&mut self, ctx: &RpcContext, path: &Path) {
        let directory = self
            .directories
            .get(path)
            .and_then(|probe| probe.result.as_ref());
        let Some(inspection) =
            source_inspection(self.connection, directory, self.space.result.as_ref())
        else {
            return;
        };
        // Capacity and kernel updates cannot replay old content validation.
        let contents = self
            .directories
            .get_mut(path)
            .and_then(Query::take_contents);
        for source in self
            .sources
            .iter()
            .filter(|source| source.relative_path() == path)
        {
            match &contents {
                Some(contents) => source
                    .record_directory(ctx, inspection.clone(), contents)
                    .await
                    .log_err(),
                None => source.record(ctx, inspection.clone()).await.log_err(),
            };
        }
    }
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
        let Some(probe) = self.probes.get_mut(&update.mount.key()) else {
            return;
        };
        match update.observation {
            ProbeObservation::Space(observation) => {
                probe.space.update(observation);
                probe.publish(ctx).await;
            }
            ProbeObservation::Directory(path, observation) => {
                probe
                    .directories
                    .get_mut(&path)
                    .unwrap()
                    .update(observation);
                probe.publish_directory(ctx, &path).await;
            }
        }
    }

    pub(super) async fn tick(
        &mut self,
        ctx: &RpcContext,
        sources: &BTreeMap<SourceId, RegisteredSource>,
        server_id: &str,
    ) {
        self.probes
            .retain(|_, probe| probe.mount.active_path().is_some());
        for probe in self.probes.values_mut() {
            probe.sources.retain(|mounted| {
                sources
                    .get(&mounted.source.id)
                    .is_some_and(|current| current.definition == mounted.source.definition)
            });
        }
        let now = Instant::now();
        if now < self.next_health {
            return;
        }
        self.next_health = now + HEALTH_INTERVAL;
        let capacity_due = now >= self.next_capacity;
        if capacity_due {
            self.next_capacity = now + CAPACITY_INTERVAL;
        }
        let kernel = KernelState::read().await;
        let mut groups = BTreeMap::<usize, Group>::new();
        for registration in sources.values() {
            let mounts = match registration.existing().await {
                Ok(mounts) => mounts,
                Err(error) => {
                    registration
                        .record(
                            ctx,
                            None,
                            SharedVolumeInspection::unavailable(source::public_error_message(
                                error,
                            )),
                        )
                        .await
                        .log_err();
                    continue;
                }
            };
            for source in mounts {
                let Some(path) = source.mount.active_path() else {
                    continue;
                };
                groups
                    .entry(source.mount.key())
                    .or_insert_with(|| Group {
                        mount: source.mount.clone(),
                        path,
                        cifs: matches!(
                            registration.definition.source(),
                            SharedVolumeSource::Cifs { .. }
                        ),
                        sources: Vec::new(),
                    })
                    .sources
                    .push(source);
            }
        }
        for (key, group) in groups {
            let connection = if group.cifs {
                kernel.health(&group.path)
            } else {
                Some(true)
            };
            let capacity = group.sources.iter().any(|source| {
                should_measure_capacity(&source.source.inspection, connection, capacity_due)
            });
            let probe = self.probes.entry(key).or_insert_with(|| Probe {
                mount: group.mount,
                connection,
                sources: Vec::new(),
                space: Query::default(),
                directories: BTreeMap::new(),
            });
            probe.connection = connection;
            probe.sources = group.sources;
            if connection == Some(false) {
                probe.publish(ctx).await;
                continue;
            }
            let paths: BTreeSet<_> = probe
                .sources
                .iter()
                .map(MountedSource::relative_path)
                .collect();
            // Unfinished directory checks retain their notification keys.
            probe
                .directories
                .retain(|path, directory| directory.task.is_some() || paths.contains(path));
            for relative in paths {
                let directory = probe.directories.entry(relative.clone()).or_default();
                if directory.task.is_some() {
                    continue;
                }
                directory.result = None;
                let sender = self.send.clone();
                let mount = probe.mount.clone();
                let root = group.path.clone();
                let registration = super::registered::directory_inspector(
                    probe
                        .sources
                        .iter()
                        .filter(|source| source.relative_path() == relative)
                        .map(|source| &source.source),
                )
                .unwrap()
                .clone();
                let server_id = server_id.to_owned();
                directory.task = Some(
                    tokio::spawn(async move {
                        let operation = async {
                            registration
                                .check_directory(root, server_id)
                                .await
                                .map_err(source::public_error_message)
                        };
                        observe_query(operation, source::SPACE_QUERY_TIMEOUT, move |observation| {
                            let sender = sender.clone();
                            let mount = mount.clone();
                            let relative = relative.clone();
                            async move {
                                sender
                                    .send(Event::Probe(ProbeUpdate {
                                        mount,
                                        observation: ProbeObservation::Directory(
                                            relative,
                                            observation,
                                        ),
                                    }))
                                    .await
                                    .is_ok()
                            }
                        })
                        .await;
                    })
                    .into(),
                );
            }
            if probe.space.task.is_none()
                && (capacity
                    || (group.cifs && connection.is_none())
                    || probe.space.result.is_none())
            {
                let sender = self.send.clone();
                let mount = probe.mount.clone();
                let path = group.path;
                probe.space.task = Some(
                    tokio::spawn(async move {
                        let operation = async {
                            crate::disk::util::get_available(&path)
                                .await
                                .map_err(source::public_error_message)
                        };
                        observe_query(operation, source::SPACE_QUERY_TIMEOUT, move |observation| {
                            let sender = sender.clone();
                            let mount = mount.clone();
                            async move {
                                sender
                                    .send(Event::Probe(ProbeUpdate {
                                        mount,
                                        observation: ProbeObservation::Space(observation),
                                    }))
                                    .await
                                    .is_ok()
                            }
                        })
                        .await;
                    })
                    .into(),
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
    connection != Some(false) && (deadline_elapsed || inspection.checked_at.is_none())
}

#[cfg(test)]
fn check_directory(root: &Path, path: &Path) -> Result<(), String> {
    crate::shared_volumes::mounts::PinnedDirectory::open(root, path)
        .map(drop)
        .map_err(source::public_error_message)
}

// Parent-filesystem health cannot establish selected-folder availability.
fn source_inspection<T>(
    connection: Option<bool>,
    directory: Option<&Result<T, String>>,
    capacity: Option<&Result<u64, String>>,
) -> Option<SharedVolumeInspection> {
    if connection == Some(false) {
        return Some(SharedVolumeInspection::unavailable(
            t!("shared-volumes.source-unavailable").to_string(),
        ));
    }
    match directory? {
        Err(error) => Some(SharedVolumeInspection::unavailable(error.clone())),
        Ok(_) => match (connection, capacity) {
            (Some(true), Some(result)) => {
                Some(SharedVolumeInspection::available_capacity(result.clone()))
            }
            (Some(true), None) => Some(SharedVolumeInspection::available(None)),
            (None, Some(Ok(bytes))) => Some(SharedVolumeInspection::available(Some(*bytes))),
            (None, Some(Err(error))) => Some(SharedVolumeInspection::unavailable(error.clone())),
            (None, None) => None,
            (Some(false), _) => unreachable!(),
        },
    }
}

async fn observe_query<T, F, N, NF>(operation: F, deadline: Duration, mut notify: N)
where
    F: Future<Output = Result<T, String>>,
    N: FnMut(Observation<T>) -> NF,
    NF: Future<Output = bool>,
{
    tokio::pin!(operation);
    tokio::select! {
        result = &mut operation => { notify(Observation::Finished(result)).await; }
        _ = tokio::time::sleep(deadline) => {
            if notify(Observation::TimedOut).await {
                notify(Observation::Finished(operation.await)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_volumes::SharedVolumeAvailability;

    #[test]
    fn capacity_scheduling_respects_health_freshness_and_slow_deadlines() {
        let available = SharedVolumeInspection::available(Some(1));
        assert!(!should_measure_capacity(&available, None, false));
        assert!(should_measure_capacity(&available, None, true));
        assert!(!should_measure_capacity(&available, Some(false), true));
        assert!(should_measure_capacity(
            &SharedVolumeInspection::default(),
            None,
            false
        ));
    }

    #[test]
    fn capacity_failure_does_not_override_known_kernel_health() {
        let open = Ok(());
        let error = Err("capacity query failed".into());
        let healthy = source_inspection(Some(true), Some(&open), Some(&error)).unwrap();
        assert_eq!(healthy.availability, SharedVolumeAvailability::Available);
        assert_eq!(healthy.error.as_deref(), Some("capacity query failed"));
        assert!(healthy.checked_at.is_some());
        assert_eq!(
            source_inspection(None, Some(&open), Some(&error))
                .unwrap()
                .availability,
            SharedVolumeAvailability::Unavailable
        );
        assert_eq!(
            source_inspection(Some(false), Some(&open), Some(&Ok(123)))
                .unwrap()
                .availability,
            SharedVolumeAvailability::Unavailable
        );
        assert_eq!(
            source_inspection(None, Some(&open), Some(&Ok(0)))
                .unwrap()
                .available,
            Some(0)
        );
    }

    #[test]
    fn healthy_parent_and_capacity_do_not_erase_missing_selected_folder() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("selected")).unwrap();
        assert!(check_directory(root.path(), Path::new("selected")).is_ok());
        std::fs::remove_dir(root.path().join("selected")).unwrap();
        let missing = check_directory(root.path(), Path::new("selected"));
        assert!(missing.is_err());
        let inspection = source_inspection(Some(true), Some(&missing), Some(&Ok(1024))).unwrap();
        assert_eq!(
            inspection.availability,
            SharedVolumeAvailability::Unavailable
        );
        assert_eq!(inspection.available, None);
        assert_eq!(inspection.error, missing.err());
        assert!(source_inspection::<()>(Some(true), None, Some(&Ok(1024))).is_none());
        std::fs::create_dir(root.path().join("selected")).unwrap();
        let recovered = check_directory(root.path(), Path::new("selected"));
        let inspection = source_inspection(
            Some(true),
            Some(&recovered),
            Some(&Err("slow capacity".into())),
        )
        .unwrap();
        assert_eq!(inspection.availability, SharedVolumeAvailability::Available);
        assert_eq!(inspection.error.as_deref(), Some("slow capacity"));
    }

    #[tokio::test]
    async fn selected_folder_can_fail_and_recover_while_capacity_remains_unfinished() {
        let (complete, pending) = tokio::sync::oneshot::channel();
        let (send, mut receive) = mpsc::unbounded_channel();
        let mut space = Query::<u64> {
            result: None,
            task: Some(
                tokio::spawn(observe_query(
                    async { pending.await.unwrap() },
                    Duration::from_millis(1),
                    move |observation| {
                        let send = send.clone();
                        async move { send.send(observation).is_ok() }
                    },
                ))
                .into(),
            ),
        };
        space.update(receive.recv().await.unwrap());
        assert!(space.result.as_ref().unwrap().is_err());
        assert!(!space.task.as_ref().unwrap().is_finished());
        let root = tempfile::tempdir().unwrap();
        let selected = Path::new("selected");
        let mut directory = Query::<()>::default();
        directory.update(Observation::Finished(check_directory(
            root.path(),
            selected,
        )));
        assert_eq!(
            source_inspection(Some(true), directory.result.as_ref(), space.result.as_ref())
                .unwrap()
                .availability,
            SharedVolumeAvailability::Unavailable
        );
        std::fs::create_dir(root.path().join(selected)).unwrap();
        directory.update(Observation::Finished(check_directory(
            root.path(),
            selected,
        )));
        let recovered =
            source_inspection(Some(true), directory.result.as_ref(), space.result.as_ref())
                .unwrap();
        assert_eq!(recovered.availability, SharedVolumeAvailability::Available);
        assert!(recovered.error.is_some());
        assert!(!space.task.as_ref().unwrap().is_finished());
        complete.send(Ok(42)).unwrap();
        space.update(receive.recv().await.unwrap());
        assert!(space.task.is_none());
        assert_eq!(
            source_inspection(Some(true), directory.result.as_ref(), space.result.as_ref())
                .unwrap()
                .available,
            Some(42)
        );
    }

    #[tokio::test]
    async fn target_contents_wait_for_combined_observation_and_cannot_be_replayed() {
        let root = tempfile::tempdir().unwrap();
        let contents = crate::backup::target::catalog::read_contents(root.path(), "server")
            .await
            .unwrap();
        let mut query = Query::<DirectoryContents>::default();
        query.update(Observation::Finished(Ok(DirectoryContents::Target(
            contents,
        ))));
        assert!(source_inspection(None, query.result.as_ref(), None).is_none());
        assert!(matches!(
            query.result,
            Some(Ok(DirectoryContents::Target(_)))
        ));
        assert!(source_inspection(None, query.result.as_ref(), Some(&Ok(123))).is_some());
        assert!(matches!(
            query.take_contents(),
            Some(DirectoryContents::Target(_))
        ));
        assert!(query.take_contents().is_none());
        assert!(matches!(query.result, Some(Ok(DirectoryContents::Folder))));
    }

    #[tokio::test]
    async fn deadline_reports_failure_without_finishing_the_pending_operation() {
        let (complete, pending) = tokio::sync::oneshot::channel();
        let (send, mut receive) = mpsc::unbounded_channel();
        let task = tokio::spawn(observe_query(
            async { pending.await.unwrap() },
            Duration::from_millis(1),
            move |observation| {
                let send = send.clone();
                async move { send.send(observation).is_ok() }
            },
        ));
        assert!(matches!(receive.recv().await, Some(Observation::TimedOut)));
        assert!(!task.is_finished());
        complete.send(Ok(42)).unwrap();
        assert!(matches!(
            receive.recv().await,
            Some(Observation::Finished(Ok(42)))
        ));
        task.await.unwrap();
        assert!(receive.recv().await.is_none());
    }

    #[tokio::test]
    async fn closed_observer_drops_a_pending_query_after_the_deadline() {
        let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        struct Signal(std::sync::Arc<std::sync::atomic::AtomicBool>);
        impl Drop for Signal {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::Release);
            }
        }
        let signal = Signal(dropped.clone());
        observe_query(
            async move {
                let _signal = signal;
                std::future::pending::<Result<(), String>>().await
            },
            Duration::from_millis(1),
            |observation| async move {
                assert!(matches!(observation, Observation::TimedOut));
                false
            },
        )
        .await;
        assert!(dropped.load(std::sync::atomic::Ordering::Acquire));
    }

    #[tokio::test]
    async fn completed_query_reports_once_without_a_deadline_error() {
        let mut results = Vec::new();
        observe_query(async { Ok(7) }, Duration::from_secs(1), |observation| {
            results.push(observation);
            std::future::ready(true)
        })
        .await;
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0], Observation::Finished(Ok(7))));
    }
}
