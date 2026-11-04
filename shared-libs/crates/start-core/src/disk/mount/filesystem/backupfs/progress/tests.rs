use backup_fs_protocol::{ConversionPhase, ConversionProgress, MAX_PROGRESS_LINE};
use tokio::io::AsyncWriteExt;

use super::ProgressSink;
use crate::progress::{Progress, ProgressUnits};

async fn report(sink: &mut ProgressSink, phase: ConversionPhase, done: u64, total: Option<u64>) {
    let mut bytes = serde_json::to_vec(&ConversionProgress { phase, done, total }).unwrap();
    bytes.push(b'\n');
    for chunk in bytes.chunks(3) {
        sink.write_all(chunk).await.unwrap();
    }
}

#[tokio::test]
async fn conversion_lists_all_phases_before_start_and_reports_weighted_overall_progress() {
    let mut sink = ProgressSink::new();
    let initial = sink.tracker().snapshot();
    assert_eq!(initial.phases.len(), 5);
    assert!(
        initial
            .phases
            .iter()
            .all(|phase| matches!(phase.progress, Progress::NotStarted(())))
    );
    assert!(matches!(
        initial.overall,
        Progress::Progress {
            done: 0,
            total: Some(100),
            ..
        }
    ));

    report(&mut sink, ConversionPhase::Inspecting, 0, None).await;
    report(&mut sink, ConversionPhase::Records, 50, Some(100)).await;
    let snapshot = sink.tracker().snapshot();
    assert_eq!(snapshot.phases.len(), 5);
    assert!(snapshot.phases[0].progress.is_complete());
    assert!(matches!(
        snapshot.phases[1].progress,
        Progress::Progress {
            done: 50,
            total: Some(100),
            units: Some(ProgressUnits::Bytes),
        }
    ));
    assert!(
        snapshot.phases[2..]
            .iter()
            .all(|phase| matches!(phase.progress, Progress::NotStarted(())))
    );
    assert!(matches!(
        snapshot.overall,
        Progress::Progress {
            done: 15,
            total: Some(100),
            ..
        }
    ));

    report(&mut sink, ConversionPhase::Objects, 50, Some(100)).await;
    assert!(matches!(
        sink.tracker().snapshot().overall,
        Progress::Progress {
            done: 65,
            total: Some(100),
            ..
        }
    ));
    report(&mut sink, ConversionPhase::Complete, 0, None).await;
    sink.flush().await.unwrap();
    let complete = sink.tracker().snapshot();
    assert!(complete.overall.is_complete());
    assert!(
        complete
            .phases
            .iter()
            .all(|phase| phase.progress.is_complete())
    );
}

#[tokio::test]
async fn resumption_marks_skipped_phases_complete_without_adding_duplicates() {
    let mut sink = ProgressSink::new();
    report(&mut sink, ConversionPhase::Objects, 1, Some(2)).await;
    report(&mut sink, ConversionPhase::Objects, 1, Some(2)).await;
    let snapshot = sink.tracker().snapshot();
    assert_eq!(snapshot.phases.len(), 5);
    assert!(
        snapshot.phases[..3]
            .iter()
            .all(|phase| phase.progress.is_complete())
    );
    assert!(matches!(
        snapshot.overall,
        Progress::Progress {
            done: 65,
            total: Some(100),
            ..
        }
    ));
    assert!(matches!(
        snapshot.phases[4].progress,
        Progress::NotStarted(())
    ));
    assert!(sink.flush().await.is_err());
}

#[tokio::test]
async fn object_retry_reuses_phases_and_preserves_monotonic_overall_progress() {
    let mut sink = ProgressSink::new();
    report(&mut sink, ConversionPhase::Objects, 1, Some(2)).await;
    report(&mut sink, ConversionPhase::ScanningObjects, 0, None).await;
    report(&mut sink, ConversionPhase::Objects, 0, Some(1)).await;
    let snapshot = sink.tracker().snapshot();
    assert_eq!(snapshot.phases.len(), 5);
    assert!(matches!(
        snapshot.overall,
        Progress::Progress {
            done: 65,
            total: Some(100),
            ..
        }
    ));
    report(&mut sink, ConversionPhase::Objects, 1, Some(1)).await;
    assert!(matches!(
        sink.tracker().snapshot().overall,
        Progress::Progress {
            done: 95,
            total: Some(100),
            ..
        }
    ));
}

#[tokio::test]
async fn already_current_backup_completes_every_predeclared_phase() {
    let mut sink = ProgressSink::new();
    report(&mut sink, ConversionPhase::Inspecting, 0, None).await;
    report(&mut sink, ConversionPhase::Complete, 0, None).await;
    sink.flush().await.unwrap();
    assert!(
        sink.tracker()
            .snapshot()
            .phases
            .iter()
            .all(|phase| phase.progress.is_complete())
    );
}

#[tokio::test]
async fn progress_requires_a_complete_message_and_a_complete_json_line() {
    for data in [
        b"{\"phase\":\"inspecting\",\"done\":0,\"total\":null}\n".as_slice(),
        b"{\"phase\":\"complete\",\"done\":0,\"total\":null}",
    ] {
        let mut sink = ProgressSink::new();
        sink.write_all(data).await.unwrap();
        assert!(sink.flush().await.is_err());
        assert!(!sink.tracker().snapshot().overall.is_complete());
    }
}

#[tokio::test]
async fn malformed_or_oversized_progress_is_rejected_with_bounded_buffering() {
    for bytes in [b"not JSON\n".to_vec(), vec![b'a'; MAX_PROGRESS_LINE + 1]] {
        let mut sink = ProgressSink::new();
        assert!(sink.write_all(&bytes).await.is_err());
        assert!(sink.buffer.len() <= MAX_PROGRESS_LINE);
        assert!(!sink.tracker().snapshot().overall.is_complete());
    }
}
