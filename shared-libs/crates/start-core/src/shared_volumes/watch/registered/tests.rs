use super::*;
use crate::shared_volumes::{SharedVolume, SharedVolumeCredentials, SharedVolumeGrant};

fn database() -> (DatabaseModel, SharedVolumeId, BackupTargetId) {
    let mut db = DatabaseModel::from(
        to_value(&serde_json::json!({
            "public": {"sharedVolumes": {}, "backupTargets": {}},
            "private": {"sharedVolumeCredentials": {}, "backupTargetCredentials": {}}
        }))
        .unwrap(),
    );
    let volume_id = SharedVolumeId::new();
    let target_id = volume_id.as_ref().parse().unwrap();
    let source = SharedVolumeSource::Cifs {
        hostname: format!("fixture-{}", volume_id),
        path: "/share".into(),
        username: "user".into(),
    };
    let inspection = SharedVolumeInspection::unavailable("offline".into());
    db.as_public_mut()
        .as_shared_volumes_mut()
        .insert(
            &volume_id,
            &SharedVolume {
                name: "Volume".into(),
                source: source.clone(),
                inspection: inspection.clone(),
                grants: BTreeMap::new(),
            },
        )
        .unwrap();
    db.as_public_mut()
        .as_backup_targets_mut()
        .insert(
            &target_id,
            &catalog::BackupTarget {
                name: "Target".into(),
                source,
                inspection,
                start_os: BTreeMap::new(),
                legacy_backup: false,
            },
        )
        .unwrap();
    let credentials = SharedVolumeCredentials {
        password: Some("private".into()),
    };
    db.as_private_mut()
        .as_shared_volume_credentials_mut()
        .insert(&volume_id, &credentials)
        .unwrap();
    db.as_private_mut()
        .as_backup_target_credentials_mut()
        .insert(&target_id, &credentials)
        .unwrap();
    (db, volume_id, target_id)
}

#[test]
fn snapshot_keeps_collection_ids_distinct_and_retries_only_required_unavailable_cifs() {
    let (mut db, volume_id, target_id) = database();
    let sources = snapshot(&db).unwrap();
    assert_eq!(sources.len(), 2);
    assert!(!sources[&SourceId::Volume(volume_id.clone())].retryable());
    assert!(sources[&SourceId::Target(target_id.clone())].retryable());
    db.as_public_mut()
        .as_shared_volumes_mut()
        .as_idx_mut(&volume_id)
        .unwrap()
        .as_grants_mut()
        .insert(
            &"consumer".parse().unwrap(),
            &SharedVolumeGrant { readonly: true },
        )
        .unwrap();
    assert!(snapshot(&db).unwrap()[&SourceId::Volume(volume_id.clone())].retryable());
    db.as_public_mut()
        .as_backup_targets_mut()
        .as_idx_mut(&target_id)
        .unwrap()
        .mutate(|target| {
            target.inspection = SharedVolumeInspection::available(Some(123));
            Ok(())
        })
        .unwrap();
    assert!(!snapshot(&db).unwrap()[&SourceId::Target(target_id)].retryable());
}

#[test]
fn cold_cifs_checks_require_intent_and_available_sources_stay_passive() {
    let (mut db, volume_id, target_id) = database();
    let sources = snapshot(&db).unwrap();
    let mut ungranted = sources[&SourceId::Volume(volume_id.clone())].clone();
    ungranted.inspection = Default::default();
    assert!(!ungranted.needs_inspection(true, false));
    assert!(!ungranted.needs_inspection(false, false));
    db.as_public_mut()
        .as_shared_volumes_mut()
        .as_idx_mut(&volume_id)
        .unwrap()
        .as_grants_mut()
        .insert(
            &"consumer".parse().unwrap(),
            &SharedVolumeGrant { readonly: true },
        )
        .unwrap();
    let sources = snapshot(&db).unwrap();
    for id in [SourceId::Volume(volume_id), SourceId::Target(target_id)] {
        let mut source = sources[&id].clone();
        source.inspection = Default::default();
        assert!(source.needs_inspection(true, false));
        assert!(source.needs_inspection(false, false));
        source.inspection = SharedVolumeInspection::unavailable("offline".into());
        assert!(!source.needs_inspection(false, false));
        assert!(source.retryable());
        source.inspection = SharedVolumeInspection::available(Some(123));
        assert!(!source.needs_inspection(false, false));
        assert!(!source.retryable());
    }
}

#[test]
fn target_definition_tracks_private_password_but_not_names_or_observations() {
    let (mut db, _, target_id) = database();
    let key = SourceId::Target(target_id.clone());
    let before = snapshot(&db).unwrap()[&key].definition.clone();
    db.as_public_mut()
        .as_backup_targets_mut()
        .as_idx_mut(&target_id)
        .unwrap()
        .mutate(|target| {
            target.name = "Renamed".into();
            target.inspection = SharedVolumeInspection::available(Some(55));
            Ok(())
        })
        .unwrap();
    assert_eq!(snapshot(&db).unwrap()[&key].definition, before);
    db.as_private_mut()
        .as_backup_target_credentials_mut()
        .insert(
            &target_id,
            &SharedVolumeCredentials {
                password: Some("replacement".into()),
            },
        )
        .unwrap();
    assert_ne!(snapshot(&db).unwrap()[&key].definition, before);
    db.as_public_mut()
        .as_backup_targets_mut()
        .remove(&target_id)
        .unwrap();
    assert!(!snapshot(&db).unwrap().contains_key(&key));
}

#[tokio::test]
async fn mounted_directory_checks_initialize_and_revalidate_target_contents_without_mounting() {
    let (db, volume_id, target_id) = database();
    let sources = snapshot(&db).unwrap();
    let target = sources[&SourceId::Target(target_id)].clone();
    let volume = &sources[&SourceId::Volume(volume_id)];
    let mut available_alias = target.clone();
    available_alias.inspection = SharedVolumeInspection::available(Some(123));
    assert!(!available_alias.needs_contents());
    assert!(!volume.needs_contents());
    let inspector = directory_inspector([&available_alias, volume, &target].into_iter()).unwrap();
    assert!(inspector.needs_contents());
    let root = tempfile::tempdir().unwrap();
    assert!(matches!(
        inspector
            .check_directory(root.path().to_owned(), "server".into())
            .await
            .unwrap(),
        DirectoryContents::Target(_)
    ));
    let metadata = crate::disk::util::BackupUnencryptedMetadata::path(root.path(), "server");
    tokio::fs::create_dir_all(metadata.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&metadata, b"{broken").await.unwrap();
    assert_eq!(
        inspector
            .check_directory(root.path().to_owned(), "server".into())
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Deserialization
    );
    assert!(matches!(
        volume
            .check_directory(root.path().to_owned(), "server".into())
            .await
            .unwrap(),
        DirectoryContents::Folder
    ));
    assert!(matches!(
        available_alias
            .check_directory(root.path().to_owned(), "server".into())
            .await
            .unwrap(),
        DirectoryContents::Folder
    ));
    tokio::fs::remove_file(metadata).await.unwrap();
    assert!(matches!(
        inspector
            .check_directory(root.path().to_owned(), "server".into())
            .await
            .unwrap(),
        DirectoryContents::Target(_)
    ));
}

#[tokio::test]
async fn change_stream_ignores_unrelated_state_but_observes_private_credentials() {
    use futures::StreamExt;
    use patch_db::ModelExt;

    let (mut db, volume_id, _) = database();
    let initial = db.as_value().clone();
    db.as_value_mut()["public"]["ui"] = imbl_value::json!({ "snakeHighScore": 42 });
    let unrelated = db.as_value().clone();
    db.as_private_mut()
        .as_shared_volume_credentials_mut()
        .insert(
            &volume_id,
            &SharedVolumeCredentials {
                password: Some("rotated".into()),
            },
        )
        .unwrap();
    let rotated = db.as_value().clone();
    db.as_public_mut()
        .as_shared_volumes_mut()
        .as_idx_mut(&volume_id)
        .unwrap()
        .mutate(|volume| {
            volume.inspection = SharedVolumeInspection::available(Some(42));
            Ok(())
        })
        .unwrap();
    let observations = changes(futures::stream::iter([
        Ok(initial),
        Ok(unrelated),
        Ok(rotated),
        Ok(db.into_value()),
    ]))
    .collect::<Vec<_>>()
    .await;
    assert_eq!(observations.len(), 3);
    for observation in observations {
        observation.unwrap();
    }
}

#[tokio::test]
async fn existing_registered_sources_do_not_cold_mount() {
    let (db, _, _) = database();
    for source in snapshot(&db).unwrap().into_values() {
        assert!(source.existing().await.unwrap().is_empty());
    }
}
