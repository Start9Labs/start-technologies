use std::io::Write;
use std::os::fd::AsRawFd;
use std::task::{Context, Poll};

use super::*;
use crate::util::cleanup::{FILESYSTEM_CLEANUP_TEST_LOCK, wait_for_filesystem_cleanup};

#[test]
fn duplicated_descriptor_retains_the_file_after_its_original_handle_closes() {
    let directory = tempfile::tempdir().unwrap();
    let original_path = directory.path().join("original");
    let original = std::fs::File::create(&original_path).unwrap();
    let retained = duplicate_descriptor(original.as_raw_fd()).unwrap();
    drop(original);
    let replacement_path = directory.path().join("replacement");
    let replacement = std::fs::File::create(&replacement_path).unwrap();
    let mut retained = std::fs::File::from(retained);
    retained.write_all(b"retained").unwrap();
    assert_eq!(std::fs::read(original_path).unwrap(), b"retained");
    assert_eq!(replacement.metadata().unwrap().len(), 0);
}

#[test]
fn dropped_pacer_leaves_queued_writeback_in_the_filesystem_drain() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let _serial = FILESYSTEM_CLEANUP_TEST_LOCK.lock().await;
        wait_for_filesystem_cleanup().await;
        let file = tempfile::tempfile().unwrap();
        file.set_len(KICK_EVERY).unwrap();
        let (started, running) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let blocker = tokio::task::spawn_blocking(move || {
            started.send(()).unwrap();
            released.recv().unwrap();
        });
        running.await.unwrap();
        let mut pacer = WritebackPacer::new(file.as_raw_fd());
        let mut context = Context::from_waker(futures::task::noop_waker_ref());
        assert!(matches!(
            pacer.poll_pace(&mut context, KICK_EVERY),
            Poll::Ready(Ok(()))
        ));
        drop(file);
        drop(pacer);
        let drain = wait_for_filesystem_cleanup();
        tokio::pin!(drain);
        assert!(futures::poll!(&mut drain).is_pending());
        release.send(()).unwrap();
        blocker.await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(3), &mut drain)
            .await
            .expect("queued writeback escaped the cleanup drain");
    });
}

#[tokio::test]
async fn invalid_descriptors_fail_without_starting_reservation_work() {
    assert_eq!(
        preallocate(-1, 1).await.unwrap_err().raw_os_error(),
        Some(libc::EBADF)
    );
    assert_eq!(
        release_beyond(-1, 0, 1).await.unwrap_err().raw_os_error(),
        Some(libc::EBADF)
    );
    preallocate(-1, u64::MAX).await.unwrap();
    release_beyond(-1, 1, 1).await.unwrap();
}
