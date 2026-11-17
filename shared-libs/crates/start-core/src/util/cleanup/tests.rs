use tokio::io::AsyncWriteExt;

use super::*;
use crate::util::io::{AtomicFile, TmpDir};

#[test]
fn closed_runtime_cleanup_registration_drops_work_outside_the_registry_lock() {
    let (finished, completed) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let handle = runtime.handle().clone();
        drop(runtime);
        let _entered = handle.enter();
        let tasks = std::sync::Arc::new(CleanupTasks::new(Vec::new()));
        let cleanup_owner = tasks.clone();
        let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let dropped_in_callback = dropped.clone();
        let guard = crate::util::GeneralBoxedGuard::new(move || {
            let _ = spawn_cleanup(&cleanup_owner, async {});
            dropped_in_callback.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        let _ = spawn_cleanup(&tasks, async move {
            let _guard = guard;
            std::future::pending::<()>().await;
        });
        assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
        finished.send(()).unwrap();
    });
    completed
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("cleanup destructor re-entered a held registry lock");
    worker.join().unwrap();
}

#[tokio::test]
async fn temporary_directory_drop_cleanup_finishes_before_teardown_returns() {
    let _serial = FILESYSTEM_CLEANUP_TEST_LOCK.lock().await;
    wait_for_filesystem_cleanup().await;
    let directory = TmpDir::new().await.unwrap();
    let path = directory.as_ref().to_owned();
    tokio::fs::write(path.join("upload"), b"unfinished upload")
        .await
        .unwrap();

    drop(directory);
    wait_for_filesystem_cleanup().await;

    assert!(!path.exists());
}

#[tokio::test]
async fn atomic_file_drop_cleanup_preserves_published_contents() {
    let _serial = FILESYSTEM_CLEANUP_TEST_LOCK.lock().await;
    wait_for_filesystem_cleanup().await;
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("destination");
    let temporary = directory.path().join("temporary");
    tokio::fs::write(&destination, b"published").await.unwrap();
    let mut file = AtomicFile::new(&destination, Some(&temporary))
        .await
        .unwrap();
    file.write_all(b"unfinished replacement").await.unwrap();

    drop(file);
    wait_for_filesystem_cleanup().await;

    assert!(!temporary.exists());
    assert_eq!(std::fs::read(&destination).unwrap(), b"published");
}
