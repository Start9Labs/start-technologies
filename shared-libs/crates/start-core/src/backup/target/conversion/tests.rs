use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::StreamExt;
use patch_db::PatchDb;

use super::*;
use crate::util::Invoke;

fn target(id: u32) -> BackupTargetId {
    BackupTargetId::Cifs { id }
}

#[test]
fn notifications_name_the_target_and_report_success_or_the_actual_failure() {
    rust_i18n::set_locale("en_US");
    let success = ConversionJob::finished(FullProgress::new(), &Ok(()));
    let (level, title, message) = success.notification("Backup Drive");
    assert!(matches!(level, NotificationLevel::Success));
    assert_eq!(title, "Backup conversion completed");
    assert_eq!(
        message,
        "Your backup on Backup Drive has been successfully converted to the new format."
    );
    let failure = ConversionJob::finished(
        FullProgress::new(),
        &Err(Error::new(eyre!("disk disconnected"), ErrorKind::Backup)),
    );
    let (level, title, message) = failure.notification("nas:/backups");
    assert!(matches!(level, NotificationLevel::Error));
    assert_eq!(title, "Backup conversion failed");
    assert_eq!(
        message,
        format!(
            "Your backup on nas:/backups failed to convert to the new format: {}",
            failure.error.unwrap()
        )
    );
    assert!(message.contains("disk disconnected"));
}

#[test]
fn conversion_reservations_exclude_the_same_backup_but_not_other_backups() {
    let mut jobs = ConversionJobs::new();
    reserve(&mut jobs, target(1), "server-a".into(), FullProgress::new()).unwrap();
    assert!(reserve(&mut jobs, target(1), "server-a".into(), FullProgress::new()).is_err());
    reserve(&mut jobs, target(1), "server-b".into(), FullProgress::new()).unwrap();
    reserve(&mut jobs, target(2), "server-a".into(), FullProgress::new()).unwrap();
    assert_eq!(jobs["cifs-1"].len(), 2);
    assert_eq!(jobs["cifs-2"].len(), 1);
}

#[test]
fn failed_and_completed_attempts_allow_an_explicit_retry() {
    for result in [
        Ok(()),
        Err(Error::new(eyre!("injected failure"), ErrorKind::Backup)),
    ] {
        let mut jobs = ConversionJobs::new();
        reserve(&mut jobs, target(1), "server-a".into(), FullProgress::new()).unwrap();
        let status = ConversionJob::finished(FullProgress::new(), &result);
        assert_eq!(status.error.is_some(), result.is_err());
        assert_eq!(status.state == ConversionState::Failed, result.is_err());
        jobs.get_mut("cifs-1")
            .unwrap()
            .insert("server-a".into(), status);
        reserve(&mut jobs, target(1), "server-a".into(), FullProgress::new()).unwrap();
        assert_eq!(jobs["cifs-1"]["server-a"].state, ConversionState::Running);
        assert!(jobs["cifs-1"]["server-a"].error.is_none());
    }
}

#[derive(Debug)]
struct TestMount {
    path: PathBuf,
    unmounted: Arc<AtomicBool>,
    fail: bool,
}

impl GenericMountGuard for TestMount {
    fn path(&self) -> &Path {
        &self.path
    }

    async fn unmount(self) -> Result<(), Error> {
        assert!(!self.unmounted.swap(true, Ordering::SeqCst));
        if self.fail {
            Err(Error::new(
                eyre!("injected unmount failure"),
                ErrorKind::Filesystem,
            ))
        } else {
            Ok(())
        }
    }
}

async fn worker_fixture(
    mode: &str,
    fail_unmount: bool,
) -> (
    tempfile::TempDir,
    TypedPatchDb<Database>,
    PreparedConversion<TestMount>,
) {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("mode"), mode).unwrap();
    let db = TypedPatchDb::load_unchecked(PatchDb::open(temp.path().join("db")).await.unwrap());
    db.put(&patch_db::json_ptr::ROOT, &imbl_value::json!({
        "private": {"notifications": {}},
        "public": {"serverInfo": {"unreadNotificationCount": 0, "statusInfo": {"backupConversions": {}}}}
    })).await.unwrap();
    db.put(
        &"/private/push".parse().unwrap(),
        &crate::notifications::push::PushState::default(),
    )
    .await
    .unwrap();
    db.mutate(|db| {
        let mut active = jobs(db)?;
        for (id, server) in [(1, "server-a"), (1, "server-b"), (2, "server-a")] {
            reserve(&mut active, target(id), server.into(), FullProgress::new())?;
        }
        db.as_public_mut()
            .as_server_info_mut()
            .as_status_info_mut()
            .as_backup_conversions_mut()
            .ser(&active)?;
        Ok::<_, Error>(())
    })
    .await
    .result
    .unwrap();
    let conversion = PreparedConversion {
        target_id: target(1),
        server_id: "server-a".into(),
        label: "Backup Drive".into(),
        data: temp.path().to_owned(),
        key: Zeroizing::new("test-key".into()),
        guard: TestMount {
            path: temp.path().to_owned(),
            unmounted: Arc::new(AtomicBool::new(false)),
            fail: fail_unmount,
        },
    };
    (temp, db, conversion)
}

async fn observed_jobs(db: &TypedPatchDb<Database>) -> ConversionJobs {
    jobs(&db.peek().await).unwrap()
}

#[tokio::test]
#[ignore = "requires testdata/backup-conversion helper on PATH"]
async fn worker_publishes_result_after_cleanup_for_success_and_failures() {
    for (mode, unmount_error, state) in [
        ("complete", false, ConversionState::Complete),
        ("fail", false, ConversionState::Failed),
        ("missing-completion", false, ConversionState::Failed),
        ("complete", true, ConversionState::Failed),
    ] {
        let (_temp, db, conversion) = worker_fixture(mode, unmount_error).await;
        let unmounted = conversion.guard.unmounted.clone();
        let (_shutdown, receiver) = broadcast::channel(1);
        conversion
            .run(
                db.clone(),
                backupfs::ConversionProgressSink::new(),
                receiver,
            )
            .await
            .unwrap();
        assert!(unmounted.load(Ordering::SeqCst));
        let active = observed_jobs(&db).await;
        let job = &active["cifs-1"]["server-a"];
        assert_eq!(job.state, state, "{mode}, unmount error {unmount_error}");
        assert_eq!(job.error.is_some(), state == ConversionState::Failed);
        assert_eq!(job.progress.phases.len(), 5);
        assert_eq!(active["cifs-1"]["server-b"].state, ConversionState::Running);
        assert_eq!(active["cifs-2"]["server-a"].state, ConversionState::Running);
        assert_eq!(
            db.peek()
                .await
                .as_private()
                .as_notifications()
                .as_entries()
                .unwrap()
                .len(),
            1
        );
    }
}

async fn live_progress(db: &TypedPatchDb<Database>) {
    let mut updates = db
        .watch(
            "/public/serverInfo/statusInfo/backupConversions/cifs-1/server-a"
                .parse()
                .unwrap(),
        )
        .await
        .typed::<ConversionJob>();
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(update) = updates.next().await {
            let job = update.unwrap().de().unwrap();
            if matches!(
                job.progress.phases.get(1).map(|phase| &phase.progress),
                Some(crate::progress::Progress::Progress {
                    done: 1,
                    total: Some(2),
                    ..
                })
            ) {
                assert_eq!(job.state, ConversionState::Running);
                assert_eq!(job.progress.phases.len(), 5);
                assert!(matches!(
                    job.progress.overall,
                    crate::progress::Progress::Progress {
                        done: 15,
                        total: Some(100),
                        ..
                    }
                ));
                return;
            }
        }
        panic!("conversion progress stream closed");
    })
    .await
    .unwrap();
}

fn helper_pid(temp: &tempfile::TempDir) -> nix::unistd::Pid {
    nix::unistd::Pid::from_raw(
        std::fs::read_to_string(temp.path().join("pid"))
            .unwrap()
            .parse()
            .unwrap(),
    )
}

#[tokio::test]
#[ignore = "requires testdata/backup-conversion helper on PATH"]
async fn worker_streams_progress_and_cancels_on_shutdown() {
    let (temp, db, conversion) = worker_fixture("hold", false).await;
    let unmounted = conversion.guard.unmounted.clone();
    let (shutdown, receiver) = broadcast::channel(1);
    let running = tokio::spawn(conversion.run(
        db.clone(),
        backupfs::ConversionProgressSink::new(),
        receiver,
    ));
    live_progress(&db).await;
    let pid = helper_pid(&temp);
    shutdown.send(None).unwrap();
    tokio::time::timeout(Duration::from_secs(10), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(unmounted.load(Ordering::SeqCst));
    let active = observed_jobs(&db).await;
    assert_eq!(active["cifs-1"]["server-a"].state, ConversionState::Failed);
    assert!(!active["cifs-1"]["server-a"].progress.overall.is_complete());
    tokio::time::timeout(Duration::from_secs(5), async {
        while nix::sys::signal::kill(pid, None).is_ok() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let before = serde_json::to_value(active).unwrap();
    tokio::time::sleep(Duration::from_millis(650)).await;
    assert_eq!(
        serde_json::to_value(observed_jobs(&db).await).unwrap(),
        before
    );
}

#[tokio::test]
#[ignore = "requires testdata/backup-conversion helper on PATH"]
async fn worker_progress_and_completion_are_scoped_to_one_backup() {
    let (temp, db, conversion) = worker_fixture("hold", false).await;
    let (_shutdown, receiver) = broadcast::channel(1);
    let running = tokio::spawn(conversion.run(
        db.clone(),
        backupfs::ConversionProgressSink::new(),
        receiver,
    ));
    live_progress(&db).await;
    nix::sys::signal::kill(helper_pid(&temp), nix::sys::signal::Signal::SIGUSR2).unwrap();
    tokio::time::timeout(Duration::from_secs(10), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let active = observed_jobs(&db).await;
    assert_eq!(
        active["cifs-1"]["server-a"].state,
        ConversionState::Complete
    );
    assert!(active["cifs-1"]["server-a"].progress.overall.is_complete());
    assert_eq!(active["cifs-1"]["server-b"].state, ConversionState::Running);
    assert_eq!(active["cifs-2"]["server-a"].state, ConversionState::Running);
}

#[tokio::test]
#[ignore = "requires the real startos-backup-fs helper on PATH"]
async fn real_helper_converts_the_frozen_backup_through_the_background_worker() {
    let (temp, db, mut conversion) = worker_fixture("unused", false).await;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../projects/start-os/backup-fs/testdata/legacy-format1.tar.gz");
    tokio::process::Command::new("tar")
        .arg("-xzf")
        .arg(fixture)
        .arg("-C")
        .arg(temp.path())
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    conversion.key = Zeroizing::new("password".into());
    assert_eq!(
        backupfs::inspect(temp.path()).await.unwrap(),
        backup_fs_protocol::FormatStatus::Legacy
    );
    let unmounted = conversion.guard.unmounted.clone();
    let (_shutdown, receiver) = broadcast::channel(1);
    conversion
        .run(
            db.clone(),
            backupfs::ConversionProgressSink::new(),
            receiver,
        )
        .await
        .unwrap();
    assert!(unmounted.load(Ordering::SeqCst));
    assert_eq!(
        backupfs::inspect(temp.path()).await.unwrap(),
        backup_fs_protocol::FormatStatus::Current
    );
    let active = observed_jobs(&db).await;
    let job = &active["cifs-1"]["server-a"];
    assert_eq!(job.state, ConversionState::Complete);
    assert!(job.progress.overall.is_complete());
    assert_eq!(job.progress.phases.len(), 5);
    assert!(
        job.progress
            .phases
            .iter()
            .all(|phase| phase.progress.is_complete())
    );
    assert_eq!(active["cifs-1"]["server-b"].state, ConversionState::Running);
}
