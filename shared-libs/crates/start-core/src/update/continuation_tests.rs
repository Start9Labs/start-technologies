use futures::StreamExt;

use super::*;

#[tokio::test]
async fn completed_progress_waits_for_the_actual_operation_outcome() {
    let tracker = FullProgressTracker::new();
    let mut phase = tracker.add_phase("Work".into(), Some(100));
    phase.set_total(1);
    phase.complete();
    tracker.complete();
    let (complete, result) = tokio::sync::oneshot::channel();
    let mut source = Box::pin(progress_until_complete(tracker, async move {
        result.await.with_kind(ErrorKind::Cancelled)?
    }));
    assert!(source.next().await.unwrap().unwrap().overall.is_complete());
    assert!(futures::poll!(std::pin::pin!(source.next())).is_pending());
    complete
        .send(Err(Error::new(
            eyre!("staging failed"),
            ErrorKind::Filesystem,
        )))
        .unwrap();
    assert!(source.next().await.unwrap().is_ok());
    assert_eq!(
        source.next().await.unwrap().unwrap_err().kind,
        ErrorKind::Filesystem
    );
    assert!(source.next().await.is_none());
}

#[tokio::test]
async fn successful_completion_marks_the_final_snapshot_complete() {
    let tracker = FullProgressTracker::new();
    let mut phase = tracker.add_phase("Work".into(), Some(100));
    phase.set_total(10);
    phase.set_done(2);
    let mut source = Box::pin(progress_until_complete(tracker, async { Ok(()) }));
    let snapshot = source.next().await.unwrap().unwrap();
    assert!(snapshot.overall.is_complete());
    assert!(
        snapshot
            .phases
            .iter()
            .all(|phase| phase.progress.is_complete())
    );
    assert!(source.next().await.is_none());
}

#[tokio::test]
async fn dropping_the_observer_does_not_abort_the_accepted_operation() {
    let run = std::sync::Arc::new(tokio::sync::Notify::new());
    let resume = run.clone();
    let (completed, observed) = tokio::sync::oneshot::channel();
    let operation = tokio::spawn(async move {
        resume.notified().await;
        let _ = completed.send(());
        Ok(())
    });
    let source = progress_until_complete(FullProgressTracker::new(), async move {
        operation.await.with_kind(ErrorKind::Cancelled)?
    });
    drop(source);
    run.notify_one();
    tokio::time::timeout(Duration::from_secs(2), observed)
        .await
        .unwrap()
        .unwrap();
}
