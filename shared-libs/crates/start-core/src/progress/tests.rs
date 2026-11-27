use super::*;
use crate::util::future::DropSignaling;

#[tokio::test]
async fn progress_persistence_stops_with_the_operation() {
    let sync = DropSignaling::new(pending::<Result<(), Error>>());
    let mut stopped = sync.subscribe();
    assert_eq!(run_with_progress_sync(async { 42 }, sync).await, 42);
    assert!(stopped.wait().now_or_never().is_some());
}

#[tokio::test]
async fn progress_failure_does_not_cancel_the_operation() {
    let (reply, result) = tokio::sync::oneshot::channel();
    let sync = async {
        reply.send(42).unwrap();
        Err(Error::new(
            eyre!("status writer failed"),
            ErrorKind::Database,
        ))
    };
    assert_eq!(
        run_with_progress_sync(async { result.await.unwrap() }, sync).await,
        42
    );
}

#[tokio::test]
async fn cancelling_the_owner_drops_work_and_progress() {
    let work = DropSignaling::new(pending::<()>());
    let mut work_stopped = work.subscribe();
    let sync = DropSignaling::new(pending::<Result<(), Error>>());
    let mut sync_stopped = sync.subscribe();
    let task = NonDetachingJoinHandle::from(tokio::spawn(run_with_progress_sync(work, sync)));
    assert!(task.wait_for_abort().await.unwrap_err().is_cancelled());
    assert!(work_stopped.wait().now_or_never().is_some());
    assert!(sync_stopped.wait().now_or_never().is_some());
}
