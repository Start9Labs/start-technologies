use super::*;

#[tokio::test]
async fn content_inspection_validates_backup_metadata_before_legacy_edits_publish() {
    let root = tempfile::tempdir().unwrap();
    let (inspection, backups, legacy) = inspect_contents(root.path(), "server").await.unwrap();
    assert_eq!(
        inspection.availability,
        crate::shared_volumes::SharedVolumeAvailability::Available
    );
    assert!(inspection.checked_at.is_some());
    assert!(backups.is_empty());
    assert!(!legacy);
    let metadata = crate::disk::util::BackupUnencryptedMetadata::path(root.path(), "server");
    tokio::fs::create_dir_all(metadata.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(metadata, b"{broken").await.unwrap();
    let error = inspect_contents(root.path(), "server").await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::Deserialization);
}

fn database() -> DatabaseModel {
    DatabaseModel::from(to_value(&serde_json::json!({
        "public": {"backupTargets": {}},
        "private": {"backupTargetCredentials": {}, "cifs": {
            "7": {"hostname": "nas.local", "path": "/backups", "username": "family", "password": "private-password"}
        }}
    })).unwrap())
}

#[test]
fn migration_preserves_identity_intent_and_secrets_without_a_second_catalog() {
    let mut db = database();
    initialize(&mut db).unwrap();
    let id: BackupTargetId = "cifs-7".parse().unwrap();
    let target = db
        .as_public()
        .as_backup_targets()
        .as_idx(&id)
        .unwrap()
        .de()
        .unwrap();
    assert_eq!(target.name, "nas.local:/backups");
    assert_eq!(target.inspection, SharedVolumeInspection::default());
    let public = serde_json::to_value(&target).unwrap();
    assert_eq!(public["source"]["username"], "family");
    assert!(!public.to_string().contains("private-password"));
    assert!(db.as_private().as_cifs().de().unwrap().0.is_empty());
    let source = BackupTargetSource::read(&db, &id).unwrap();
    assert_eq!(
        source.definition.cifs().unwrap().password.as_deref(),
        Some("private-password")
    );
    db.as_public_mut()
        .as_backup_targets_mut()
        .as_idx_mut(&id)
        .unwrap()
        .as_name_mut()
        .ser(&"My NAS".to_owned())
        .unwrap();
    initialize(&mut db).unwrap();
    assert_eq!(db.as_public().as_backup_targets().de().unwrap().len(), 1);
    assert_eq!(BackupTargetSource::read(&db, &id).unwrap().name, "My NAS");
    assert_eq!(
        db.as_private()
            .as_backup_target_credentials()
            .de()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn invalid_historical_connections_migrate_as_intent_instead_of_blocking_boot() {
    let mut db = database();
    db.as_private_mut()
        .as_cifs_mut()
        .as_idx_mut(&7)
        .unwrap()
        .ser(&Cifs {
            hostname: "invalid/hostname".into(),
            path: "../share".into(),
            username: "family".into(),
            password: Some("private-password".into()),
        })
        .unwrap();
    initialize(&mut db).unwrap();
    let source = BackupTargetSource::read(&db, &"cifs-7".parse().unwrap()).unwrap();
    assert!(source.definition.cifs().unwrap().validate().is_err());
    assert!(db.as_private().as_cifs().de().unwrap().0.is_empty());
}

fn update(id: BackupTargetId, password: Option<String>) -> UpdateCifsTargetParams {
    UpdateCifsTargetParams {
        id,
        hostname: "new-nas.local".into(),
        path: "new-share".into(),
        username: "new-user".into(),
        password,
    }
}

#[test]
fn connection_edits_preserve_target_identity_and_name_and_reset_old_observations() {
    let mut db = database();
    initialize(&mut db).unwrap();
    let id: BackupTargetId = "cifs-7".parse().unwrap();
    db.as_public_mut()
        .as_backup_targets_mut()
        .as_idx_mut(&id)
        .unwrap()
        .mutate(|target| {
            target.inspection = SharedVolumeInspection::available(Some(4096));
            target.legacy_backup = true;
            Ok(())
        })
        .unwrap();
    update_cifs_definition(&mut db, update(id.clone(), None)).unwrap();
    let target = db
        .as_public()
        .as_backup_targets()
        .as_idx(&id)
        .unwrap()
        .de()
        .unwrap();
    assert_eq!(target.name, "nas.local:/backups");
    assert_eq!(target.inspection, SharedVolumeInspection::default());
    assert!(target.start_os.is_empty());
    assert!(!target.legacy_backup);
    let cifs = BackupTargetSource::read(&db, &id)
        .unwrap()
        .definition
        .cifs()
        .unwrap();
    assert_eq!(cifs.hostname, "new-nas.local");
    assert_eq!(cifs.path, Path::new("/new-share"));
    assert_eq!(cifs.username, "new-user");
    assert_eq!(cifs.password.as_deref(), Some("private-password"));
    update_cifs_definition(&mut db, update(id.clone(), Some(String::new()))).unwrap();
    assert_eq!(
        BackupTargetSource::read(&db, &id)
            .unwrap()
            .definition
            .cifs()
            .unwrap()
            .password,
        None
    );
}

#[test]
fn stale_and_removed_target_observations_cannot_replace_current_state() {
    let mut db = database();
    initialize(&mut db).unwrap();
    let id: BackupTargetId = "cifs-7".parse().unwrap();
    let before = BackupTargetSource::read(&db, &id).unwrap();
    update_cifs_definition(&mut db, update(id.clone(), Some("replacement".into()))).unwrap();
    write_inspection(
        &mut db,
        &id,
        &before.definition,
        None,
        SharedVolumeInspection::available(Some(123)),
        Some((BTreeMap::new(), true)),
    )
    .unwrap();
    assert_eq!(
        db.as_public()
            .as_backup_targets()
            .as_idx(&id)
            .unwrap()
            .as_inspection()
            .de()
            .unwrap(),
        SharedVolumeInspection::default()
    );
    let current = BackupTargetSource::read(&db, &id).unwrap();
    let inspection = SharedVolumeInspection::available(Some(456));
    write_inspection(
        &mut db,
        &id,
        &current.definition,
        None,
        inspection.clone(),
        Some((BTreeMap::new(), true)),
    )
    .unwrap();
    let target = db
        .as_public()
        .as_backup_targets()
        .as_idx(&id)
        .unwrap()
        .de()
        .unwrap();
    assert_eq!(target.inspection, inspection);
    assert_eq!(target.name, "nas.local:/backups");
    assert!(target.legacy_backup);
    assert!(
        serde_json::to_value(target)
            .unwrap()
            .get("inspection")
            .is_none()
    );
    db.as_public_mut()
        .as_backup_targets_mut()
        .remove(&id)
        .unwrap();
    write_inspection(
        &mut db,
        &id,
        &current.definition,
        None,
        SharedVolumeInspection::unavailable("old check".into()),
        None,
    )
    .unwrap();
    assert!(db.as_public().as_backup_targets().de().unwrap().is_empty());
}
