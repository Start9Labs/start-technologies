use super::*;
use crate::backup::target::conversion::{ConversionJob, ConversionJobs, ConversionState};

#[tokio::test]
async fn backup_outcomes_persist_in_history_without_notifications() {
    for (package_error, volume_error, fatal_error) in [
        (None, None, None),
        (Some("service stopped"), None, None),
        (None, Some("volume unavailable"), None),
        (None, None, Some("disk disconnected")),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let db = TypedPatchDb::load_unchecked(PatchDb::open(temp.path().join("db")).await.unwrap());
        db.put(&patch_db::json_ptr::ROOT, &imbl_value::json!({
            "private": {"backupHistory": {}, "notifications": {}},
            "public": {"serverInfo": {"unreadNotificationCount": 0, "statusInfo": {"backupProgress": FullProgress::new()}}}
        })).await.unwrap();
        db.put(
            &"/private/push".parse().unwrap(),
            &crate::notifications::push::PushState::default(),
        )
        .await
        .unwrap();
        db.put(
            &"/public/scheduledBackups".parse().unwrap(),
            &crate::backup::scheduled::ScheduledBackupState::default(),
        )
        .await
        .unwrap();
        let activity = running_activity(
            BackupActivityKind::Manual,
            "cifs-1".parse().unwrap(),
            None,
            None,
            None,
            ["test".parse().unwrap()].into_iter().collect(),
        );
        db.mutate(|db| insert_activity(db, &activity))
            .await
            .result
            .unwrap();
        let guard = BackupStatusGuard::new(
            db.clone(),
            "cifs-1".parse().unwrap(),
            "Backup Drive".into(),
            activity.id.clone(),
        );
        let started_at = guard.started_at;
        let volume_id = SharedVolumeId::new();
        let report: BackupReport = serde_json::from_value(serde_json::json!({
            "server": {"attempted": true, "error": null},
            "packages": {"test": {"error": package_error, "duration_ms": 10}},
            "sharedVolumes": {(volume_id.to_string()): {"name": "Documents", "error": volume_error, "duration_ms": 20}}
        })).unwrap();
        let expected_report = if fatal_error.is_some() {
            serde_json::json!({
                "server": {"attempted": true, "error": "Filesystem I/O Error: disk disconnected"},
                "packages": {},
                "sharedVolumes": {}
            })
        } else {
            serde_json::to_value(&report).unwrap()
        };
        let result = if let Some(error) = fatal_error {
            Err(Error::new(eyre!(error), ErrorKind::Filesystem))
        } else {
            Ok(report)
        };
        guard.handle_result(result).await.unwrap();

        let snapshot = db.peek().await;
        let runs = snapshot.as_private().as_backup_history().de().unwrap();
        assert_eq!(runs.len(), 1);
        let run = runs.into_values().next().unwrap();
        assert_eq!(run.target_id.to_string(), "cifs-1");
        assert_eq!(run.target_name, "Backup Drive");
        assert_eq!(run.started_at, started_at);
        assert!(run.completed_at >= started_at);
        assert_eq!(serde_json::to_value(&run.report).unwrap(), expected_report);
        let completed = snapshot
            .as_public()
            .as_scheduled_backups()
            .as_activities()
            .as_idx(&activity.id)
            .unwrap()
            .de()
            .unwrap();
        assert_eq!(
            completed.state,
            if fatal_error.is_some() {
                BackupRunState::Failed
            } else if package_error.is_some() || volume_error.is_some() {
                BackupRunState::PartiallyFailed
            } else {
                BackupRunState::Succeeded
            }
        );
        assert!(completed.completed_at.is_some());
        assert!(
            snapshot
                .as_public()
                .as_server_info()
                .as_status_info()
                .as_backup_progress()
                .de()
                .unwrap()
                .is_none()
        );
        assert!(
            snapshot
                .as_private()
                .as_notifications()
                .as_entries()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            snapshot
                .as_public()
                .as_server_info()
                .as_unread_notification_count()
                .de()
                .unwrap(),
            0
        );
        assert_eq!(
            serde_json::to_value(snapshot.as_private().as_push().de().unwrap()).unwrap(),
            serde_json::to_value(crate::notifications::push::PushState::default()).unwrap()
        );
    }
}

fn database(state: ConversionState) -> DatabaseModel {
    let mut db = DatabaseModel::from(imbl_value::json!({
        "public": {"serverInfo": {"statusInfo": {"backupProgress": null, "backupConversions": {}}}}
    }));
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
