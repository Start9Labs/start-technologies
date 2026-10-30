use backup_fs_protocol::{ConversionPhase, ConversionProgress, MAX_PROGRESS_LINE};
use tokio::io::AsyncWriteExt;

use super::ProgressSink;
use crate::progress::{FullProgressTracker, Progress, ProgressUnits};

async fn report(sink: &mut ProgressSink, event: ConversionProgress) {
    let mut bytes = serde_json::to_vec(&event).unwrap();
    bytes.push(b'\n');
    for chunk in bytes.chunks(3) {
        sink.write_all(chunk).await.unwrap();
    }
}

#[tokio::test]
async fn conversion_updates_existing_phased_progress_before_the_command_finishes() {
    let tracker = FullProgressTracker::new();
    let mut sink = ProgressSink::new(&tracker);
    report(
        &mut sink,
        ConversionProgress {
            phase: ConversionPhase::Inspecting,
            done: 0,
            total: None,
        },
    )
    .await;
    report(
        &mut sink,
        ConversionProgress {
            phase: ConversionPhase::Records,
            done: 50,
            total: Some(100),
        },
    )
    .await;
    let snapshot = tracker.snapshot();
    assert_eq!(snapshot.phases.len(), 2);
    assert!(matches!(
        snapshot.phases[0].progress,
        Progress::Complete(true)
    ));
    assert!(matches!(
        snapshot.phases[1].progress,
        Progress::Progress {
            done: 50,
            total: Some(100),
            units: Some(ProgressUnits::Bytes),
        }
    ));
    assert!(!snapshot.overall.is_complete());
    report(
        &mut sink,
        ConversionProgress {
            phase: ConversionPhase::Complete,
            done: 0,
            total: None,
        },
    )
    .await;
    sink.flush().await.unwrap();
    assert!(tracker.snapshot().overall.is_complete());
}

#[tokio::test]
async fn progress_requires_a_complete_message_and_a_complete_json_line() {
    for data in [
        b"{\"phase\":\"inspecting\",\"done\":0,\"total\":null}\n".as_slice(),
        b"{\"phase\":\"complete\",\"done\":0,\"total\":null}",
    ] {
        let tracker = FullProgressTracker::new();
        let mut sink = ProgressSink::new(&tracker);
        sink.write_all(data).await.unwrap();
        assert!(sink.flush().await.is_err());
        assert!(!tracker.snapshot().overall.is_complete());
    }
}

#[tokio::test]
async fn malformed_or_oversized_progress_is_rejected_with_bounded_buffering() {
    for bytes in [b"not JSON\n".to_vec(), vec![b'a'; MAX_PROGRESS_LINE + 1]] {
        let tracker = FullProgressTracker::new();
        let mut sink = ProgressSink::new(&tracker);
        assert!(sink.write_all(&bytes).await.is_err());
        assert!(sink.buffer.len() <= MAX_PROGRESS_LINE);
        assert!(!tracker.snapshot().overall.is_complete());
    }
}
