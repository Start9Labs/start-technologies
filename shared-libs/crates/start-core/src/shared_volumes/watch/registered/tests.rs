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
    assert!(!sources[&SourceId::Volume(volume_id.clone())].retryable);
    assert!(sources[&SourceId::Target(target_id.clone())].retryable);
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
    assert!(snapshot(&db).unwrap()[&SourceId::Volume(volume_id.clone())].retryable);
    db.as_public_mut()
        .as_backup_targets_mut()
        .as_idx_mut(&target_id)
        .unwrap()
        .mutate(|target| {
            target.inspection = SharedVolumeInspection::available(Some(123));
            Ok(())
        })
        .unwrap();
    assert!(!snapshot(&db).unwrap()[&SourceId::Target(target_id)].retryable);
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
async fn existing_registered_sources_do_not_cold_mount() {
    let (db, _, _) = database();
    for source in snapshot(&db).unwrap().into_values() {
        assert!(source.existing().await.unwrap().is_empty());
    }
}
