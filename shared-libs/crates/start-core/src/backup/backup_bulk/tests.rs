use super::*;
use crate::backup::target::conversion::{ConversionJob, ConversionJobs, ConversionState};

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
