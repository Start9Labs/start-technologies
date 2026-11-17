use std::future::Future;

use tokio::sync::oneshot;

use super::future::NonDetachingJoinHandle;
use super::sync::SyncMutex;
use crate::ResultExt;

pub(crate) type TaskHandles = Vec<NonDetachingJoinHandle<()>>;
/// Cleanup finishes after its producers stop; teardown does not abort it.
pub(crate) type CleanupTasks = SyncMutex<TaskHandles>;

pub(crate) fn spawn_owned<F: Future<Output = ()> + Send + 'static>(
    tasks: &mut TaskHandles,
    future: F,
) {
    tasks.retain(|task| !task.is_finished());
    tasks.push(tokio::spawn(future).into());
}

/// Dropping the completion receiver leaves teardown responsible for the task.
pub(crate) fn spawn_cleanup<F: Future<Output = ()> + Send + 'static>(
    tasks: &CleanupTasks,
    future: F,
) -> oneshot::Receiver<()> {
    let (completed, completion) = oneshot::channel();
    tasks.mutate(|tasks| {
        spawn_owned(tasks, async move {
            future.await;
            let _ = completed.send(());
        });
    });
    completion
}

/// Includes cleanup initiated by completed tasks' destructors.
/// The caller must stop external producers before draining.
pub(crate) async fn drain_cleanup(tasks: &CleanupTasks) {
    loop {
        let pending = tasks.replace(Vec::new());
        if pending.is_empty() {
            break;
        }
        for task in pending {
            task.await.log_err();
        }
    }
}
