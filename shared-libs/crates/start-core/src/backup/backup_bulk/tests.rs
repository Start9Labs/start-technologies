use imbl_value::json;
use patch_db::ModelExt;

use super::*;
use crate::backup::target::conversion::{ConversionJob, ConversionJobs, ConversionState};
use crate::db::model::public::PowerAction;

async fn active_backup(action: PowerAction) -> (tempfile::TempDir, TypedPatchDb<Database>) {
    let mut model = db_with(false, false);
    model
        .as_public_mut()
        .as_server_info_mut()
        .as_unread_notification_count_mut()
        .ser(&0)
        .unwrap();
    model
        .as_public_mut()
        .as_server_info_mut()
        .as_status_info_mut()
        .as_deferred_power_action_mut()
        .ser(&Some(action))
        .unwrap();
    model
        .as_private_mut()
        .as_notifications_mut()
        .ser(&crate::notifications::Notifications::new())
        .unwrap();
    model
        .as_private_mut()
        .as_push_mut()
        .ser(&crate::notifications::push::PushState::default())
        .unwrap();
    model
        .as_private_mut()
        .as_backup_history_mut()
        .ser(&BTreeMap::<String, crate::backup::history::BackupRun>::new())
        .unwrap();
    assure_backing_up(
        &mut model,
        &initial_progress(),
        &"cifs-1".parse().unwrap(),
        "server-a",
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let db = TypedPatchDb::<Database>::load_unchecked(
        patch_db::PatchDb::open(directory.path().join("state.db"))
            .await
            .unwrap(),
    );
    db.put(&patch_db::json_ptr::ROOT, &model).await.unwrap();
    (directory, db)
}

#[tokio::test]
async fn volume_only_reports_release_progress_without_cancelling_deferred_power() {
    for action in [PowerAction::Restart, PowerAction::Shutdown] {
        for (error, level) in [
            (None, NotificationLevel::Success),
            (Some("source disconnected"), NotificationLevel::Warning),
        ] {
            let (_directory, db) = active_backup(action).await;
            let volume = SharedVolumeId::new();
            let report = BackupReport {
                server: ServerBackupReport {
                    attempted: true,
                    error: None,
                },
                packages: BTreeMap::new(),
                shared_volumes: BTreeMap::from([(
                    volume.clone(),
                    crate::backup::VolumeBackupReport {
                        name: "Photos".into(),
                        result: PackageBackupReport {
                            error: error.map(str::to_owned),
                            duration_ms: 1,
                        },
                    },
                )]),
            };
            BackupStatusGuard::new(db.clone(), "cifs-1".parse().unwrap(), "Archive".into())
                .handle_result(false, Ok(report))
                .await
                .unwrap();
            let model = db.peek().await;
            let status = model.as_public().as_server_info().as_status_info();
            assert!(status.as_backup_progress().de().unwrap().is_none());
            assert_eq!(
                status.as_deferred_power_action().de().unwrap(),
                Some(action)
            );
            let history = model.as_private().as_backup_history().de().unwrap();
            let run = history.values().next().unwrap();
            assert!(run.report.packages.is_empty());
            assert_eq!(
                run.report.shared_volumes[&volume].result.error.as_deref(),
                error
            );
            let notifications = model.as_private().as_notifications().de().unwrap();
            assert_eq!(notifications.0.values().next().unwrap().level, level);
        }
    }
}

#[tokio::test]
async fn dropped_backup_releases_progress_without_cancelling_deferred_power() {
    for action in [PowerAction::Restart, PowerAction::Shutdown] {
        let (_directory, db) = active_backup(action).await;
        let mut watch = db
            .watch(
                "/public/serverInfo/statusInfo/backupProgress"
                    .parse()
                    .unwrap(),
            )
            .await
            .typed::<Option<FullProgress>>();
        drop(BackupStatusGuard::new(
            db.clone(),
            "cifs-1".parse().unwrap(),
            "Archive".into(),
        ));
        tokio::time::timeout(Duration::from_secs(5), watch.wait_for(Option::is_none))
            .await
            .unwrap()
            .unwrap();
        let model = db.peek().await;
        let status = model.as_public().as_server_info().as_status_info();
        assert_eq!(
            status.as_deferred_power_action().de().unwrap(),
            Some(action)
        );
        assert!(!status.as_shutting_down().de().unwrap());
        assert!(!status.as_restarting().de().unwrap());
    }
}

fn database(state: ConversionState) -> DatabaseModel {
    let mut db = db_with(false, false);
    let active = ConversionJobs::from([(
        "cifs-1".into(),
        BTreeMap::from([(
            "server-a".into(),
            ConversionJob {
                state,
                progress: FullProgress::new(),
                error: (state == ConversionState::Failed).then(|| "previous failure".into()),
            },
        )]),
    )]);
    db.as_public_mut()
        .as_server_info_mut()
        .as_status_info_mut()
        .as_backup_conversions_mut()
        .ser(&active)
        .unwrap();
    db
}

#[test]
fn backup_acceptance_rejects_a_running_conversion_before_publishing_progress() {
    for state in [
        ConversionState::Running,
        ConversionState::Complete,
        ConversionState::Failed,
    ] {
        let mut db = database(state);
        let result = assure_backing_up(
            &mut db,
            &FullProgress::new(),
            &"cifs-1".parse().unwrap(),
            "server-a",
        );
        assert_eq!(result.is_err(), state == ConversionState::Running);
        assert_eq!(
            db.as_public()
                .as_server_info()
                .as_status_info()
                .as_backup_progress()
                .transpose_ref()
                .is_some(),
            state != ConversionState::Running
        );
    }
}

#[test]
fn conversion_on_another_target_or_server_does_not_block_backup_acceptance() {
    for (target, server) in [
        ("cifs-2".parse::<BackupTargetId>().unwrap(), "server-a"),
        ("cifs-1".parse::<BackupTargetId>().unwrap(), "server-b"),
    ] {
        let mut db = database(ConversionState::Running);
        assure_backing_up(&mut db, &FullProgress::new(), &target, server).unwrap();
    }
}

fn db_with(shutting_down: bool, restarting: bool) -> DatabaseModel {
    DatabaseModel::from_value(json!({
        "public": { "serverInfo": { "statusInfo": {
            "backupProgress": null,
            "backupConversions": ConversionJobs::default(),
            "updateProgress": null,
            "shuttingDown": shutting_down,
            "restarting": restarting,
            "restart": null,
            "deferredPowerAction": null,
        } } }
    }))
}

fn initial_progress() -> FullProgress {
    let progress = FullProgressTracker::new();
    progress.add_phase("Initializing".into(), None).start();
    progress.snapshot()
}

#[test]
fn power_commitment_rejects_backups_even_when_conversion_is_running() {
    for (shutting_down, restarting) in [(true, false), (false, true)] {
        let mut db = database(ConversionState::Running);
        let status = db.as_public_mut().as_server_info_mut().as_status_info_mut();
        status.as_shutting_down_mut().ser(&shutting_down).unwrap();
        status.as_restarting_mut().ser(&restarting).unwrap();
        let before = db.as_value().clone();
        let error = assure_backing_up(
            &mut db,
            &initial_progress(),
            &"cifs-1".parse().unwrap(),
            "server-a",
        )
        .unwrap_err();
        assert_eq!(
            error.display_src().to_string(),
            t!("backup.bulk.powering-down")
        );
        assert_eq!(db.as_value(), &before);
    }
}

#[test]
fn starts_a_backup_on_an_idle_server() {
    let mut db = db_with(false, false);
    let initial = initial_progress();
    let mut expected = db.clone();
    expected
        .as_public_mut()
        .as_server_info_mut()
        .as_status_info_mut()
        .as_backup_progress_mut()
        .ser(&Some(initial.clone()))
        .unwrap();

    assure_backing_up(&mut db, &initial, &"cifs-1".parse().unwrap(), "server-a").unwrap();

    assert_eq!(db.as_value(), expected.as_value());
}

#[test]
fn refuses_an_existing_backup_without_mutating_the_database() {
    let mut db = db_with(false, false);
    assure_backing_up(
        &mut db,
        &initial_progress(),
        &"cifs-1".parse().unwrap(),
        "server-a",
    )
    .unwrap();
    let before = db.as_value().clone();

    let error = assure_backing_up(
        &mut db,
        &FullProgress::new(),
        &"cifs-1".parse().unwrap(),
        "server-a",
    )
    .unwrap_err();

    assert_eq!(error.kind, ErrorKind::InvalidRequest);
    assert_eq!(
        error.display_src().to_string(),
        t!("backup.bulk.already-backing-up")
    );
    assert_eq!(db.as_value(), &before);
}

#[test]
fn refuses_a_backup_once_a_power_action_has_begun() {
    for (shutting_down, restarting) in [(true, false), (false, true)] {
        for existing_backup in [None, Some(initial_progress())] {
            let mut db = db_with(shutting_down, restarting);
            db.as_public_mut()
                .as_server_info_mut()
                .as_status_info_mut()
                .as_backup_progress_mut()
                .ser(&existing_backup)
                .unwrap();
            let before = db.as_value().clone();

            let error = assure_backing_up(
                &mut db,
                &initial_progress(),
                &"cifs-1".parse().unwrap(),
                "server-a",
            )
            .unwrap_err();

            assert_eq!(error.kind, ErrorKind::InvalidRequest);
            assert_eq!(
                error.display_src().to_string(),
                t!("backup.bulk.powering-down")
            );
            assert_eq!(db.as_value(), &before);
        }
    }
}

#[test]
fn admits_a_backup_while_a_power_action_is_only_deferred() {
    for action in [PowerAction::Shutdown, PowerAction::Restart] {
        let mut db = db_with(false, false);
        db.as_public_mut()
            .as_server_info_mut()
            .as_status_info_mut()
            .as_deferred_power_action_mut()
            .ser(&Some(action))
            .unwrap();
        let initial = initial_progress();

        assure_backing_up(&mut db, &initial, &"cifs-1".parse().unwrap(), "server-a").unwrap();

        let status = db.as_public().as_server_info().as_status_info();
        assert_eq!(
            status.as_deferred_power_action().de().unwrap(),
            Some(action)
        );
        assert!(status.as_backup_progress().de().unwrap().is_some());
        assert!(!status.as_shutting_down().de().unwrap());
        assert!(!status.as_restarting().de().unwrap());
        let before = db.as_value().clone();
        let error = assure_backing_up(&mut db, &initial, &"cifs-1".parse().unwrap(), "server-a")
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidRequest);
        assert_eq!(
            error.display_src().to_string(),
            t!("backup.bulk.already-backing-up")
        );
        assert_eq!(db.as_value(), &before);
    }
}
