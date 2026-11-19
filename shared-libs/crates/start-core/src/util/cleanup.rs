use std::future::Future;

use lazy_static::lazy_static;
use tokio::sync::oneshot;

use super::future::NonDetachingJoinHandle;
use super::sync::SyncMutex;
use crate::ResultExt;

#[cfg(test)]
mod tests;

pub(crate) type TaskHandles = Vec<NonDetachingJoinHandle<()>>;
/// Cleanup finishes after its producers stop; teardown does not abort it.
pub(crate) type CleanupTasks = SyncMutex<TaskHandles>;

lazy_static! {
    /// startd joins mount-retaining filesystem work and destructor cleanup before storage export.
    static ref FILESYSTEM_CLEANUP: CleanupTasks = SyncMutex::new(Vec::new());
}

pub(crate) fn spawn_filesystem_cleanup(future: impl Future<Output = ()> + Send + 'static) {
    let _ = spawn_cleanup(&FILESYSTEM_CLEANUP, future);
}

/// Caller cancellation cannot release mounts retained by an unfinished blocking operation.
pub(crate) async fn run_filesystem_blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, tokio::task::JoinError> {
    spawn_cleanup(&FILESYSTEM_CLEANUP, async move {
        tokio::task::spawn_blocking(work).await
    })
    .await
    .expect("filesystem task retained until cleanup drain")
}

pub(crate) async fn wait_for_filesystem_cleanup() {
    drain_cleanup(&FILESYSTEM_CLEANUP).await;
}

#[cfg(test)]
pub(crate) static FILESYSTEM_CLEANUP_TEST_LOCK: tokio::sync::Mutex<()> =
    tokio::sync::Mutex::const_new(());

pub(crate) fn retain_owned_task(tasks: &mut TaskHandles, task: NonDetachingJoinHandle<()>) {
    tasks.retain(|task| !task.is_finished());
    tasks.push(task);
}

/// Dropping the completion receiver leaves teardown responsible for the task.
pub(crate) fn spawn_cleanup<T: Send + 'static>(
    tasks: &CleanupTasks,
    future: impl Future<Output = T> + Send + 'static,
) -> oneshot::Receiver<T> {
    let (completed, completion) = oneshot::channel();
    // A closed runtime can synchronously drop the future during spawn.
    let task = tokio::spawn(async move {
        let _ = completed.send(future.await);
    })
    .into();
    tasks.mutate(|tasks| retain_owned_task(tasks, task));
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
