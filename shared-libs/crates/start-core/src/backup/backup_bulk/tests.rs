use imbl_value::json;
use patch_db::ModelExt;

use super::*;
use crate::backup::target::conversion::{ConversionJob, ConversionJobs, ConversionState};
use crate::db::model::public::PowerAction;

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
            &BackupTargetId::Cifs { id: 1 },
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
        (BackupTargetId::Cifs { id: 2 }, "server-a"),
        (BackupTargetId::Cifs { id: 1 }, "server-b"),
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
            &BackupTargetId::Cifs { id: 1 },
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

    assure_backing_up(
        &mut db,
        &initial,
        &BackupTargetId::Cifs { id: 1 },
        "server-a",
    )
    .unwrap();

    assert_eq!(db.as_value(), expected.as_value());
}

#[test]
fn refuses_an_existing_backup_without_mutating_the_database() {
    let mut db = db_with(false, false);
    assure_backing_up(
        &mut db,
        &initial_progress(),
        &BackupTargetId::Cifs { id: 1 },
        "server-a",
    )
    .unwrap();
    let before = db.as_value().clone();

    let error = assure_backing_up(
        &mut db,
        &FullProgress::new(),
        &BackupTargetId::Cifs { id: 1 },
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
                &BackupTargetId::Cifs { id: 1 },
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

        assure_backing_up(
            &mut db,
            &initial,
            &BackupTargetId::Cifs { id: 1 },
            "server-a",
        )
        .unwrap();

        let status = db.as_public().as_server_info().as_status_info();
        assert_eq!(
            status.as_deferred_power_action().de().unwrap(),
            Some(action)
        );
        assert!(status.as_backup_progress().de().unwrap().is_some());
        assert!(!status.as_shutting_down().de().unwrap());
        assert!(!status.as_restarting().de().unwrap());
        let before = db.as_value().clone();
        let error = assure_backing_up(
            &mut db,
            &initial,
            &BackupTargetId::Cifs { id: 1 },
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
}
