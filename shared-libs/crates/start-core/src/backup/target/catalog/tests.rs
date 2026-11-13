use patch_db::json_ptr::ROOT;

use super::*;

#[test]
fn local_filesystem_uses_persisted_package_data_partition() {
    let id = BackupTargetId::new();
    let definition = SourceDefinition::new(SharedVolumeSource::Local, None);
    let PreparedFilesystem::Local(path) = filesystem(&id, &definition) else {
        panic!("local filesystem")
    };
    assert!(path.starts_with(crate::PACKAGE_DATA));
    assert!(path.ends_with(Path::new("backup-targets").join(id.as_ref())));
}

#[tokio::test]
async fn failed_local_deletion_stays_registered_and_can_be_retried() {
    let directory = tempfile::tempdir().unwrap();
    let id = BackupTargetId::new();
    let mut model = database();
    model
        .as_public_mut()
        .as_backup_targets_mut()
        .insert(
            &id,
            &BackupTarget {
                name: "Local".into(),
                source: SharedVolumeSource::Local,
                inspection: Default::default(),
                start_os: BTreeMap::new(),
                legacy_backup: false,
            },
        )
        .unwrap();
    let database = TypedPatchDb::<crate::db::model::Database>::load_unchecked(
        patch_db::PatchDb::open(directory.path().join("state.db"))
            .await
            .unwrap(),
    );
    database.put(&ROOT, &model).await.unwrap();
    let path = local_path(directory.path(), &id);
    tokio::fs::create_dir_all(path.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&path, b"not a directory").await.unwrap();
    assert!(
        remove_target(&database, &id, directory.path())
            .await
            .is_err()
    );
    assert!(
        database
            .peek()
            .await
            .as_public()
            .as_backup_targets()
            .as_idx(&id)
            .is_some()
    );
    tokio::fs::remove_file(&path).await.unwrap();
    remove_target(&database, &id, directory.path())
        .await
        .unwrap();
    assert!(
        database
            .peek()
            .await
            .as_public()
            .as_backup_targets()
            .as_idx(&id)
            .is_none()
    );
    database.put(&ROOT, &model).await.unwrap();
    tokio::fs::create_dir(&path).await.unwrap();
    tokio::fs::write(path.join("backup"), b"data")
        .await
        .unwrap();
    remove_target(&database, &id, directory.path())
        .await
        .unwrap();
    assert!(!path.exists());
    assert!(
        database
            .peek()
            .await
            .as_public()
            .as_backup_targets()
            .as_idx(&id)
            .is_none()
    );
}

#[tokio::test]
#[ignore = "requires BTRFS_TEST_VOLUMES_DIR and Btrfs subvolume permissions"]
async fn local_targets_are_private_subvolumes_and_delete_before_unregistering() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = crate::util::btrfs::tests::directory();
    let id = BackupTargetId::new();
    let path = local_path(fixture.path(), &id);
    create_local_target(&path).await.unwrap();
    assert!(crate::util::btrfs::is_subvolume(&path).await);
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let mut model = database();
    model
        .as_public_mut()
        .as_backup_targets_mut()
        .insert(
            &id,
            &BackupTarget::new("Local".into(), SharedVolumeSource::Local),
        )
        .unwrap();
    let database = TypedPatchDb::<crate::db::model::Database>::load_unchecked(
        patch_db::PatchDb::open(fixture.path().join("state.db"))
            .await
            .unwrap(),
    );
    database.put(&ROOT, &model).await.unwrap();
    let held = TmpMountGuard::mount(&Bind::new(&path), ReadOnly)
        .await
        .unwrap();
    tokio::fs::write(path.join("sentinel"), b"target data")
        .await
        .unwrap();
    assert!(remove_target(&database, &id, fixture.path()).await.is_err());
    assert!(
        database
            .peek()
            .await
            .as_public()
            .as_backup_targets()
            .as_idx(&id)
            .is_some()
    );
    assert_eq!(
        tokio::fs::read(path.join("sentinel")).await.unwrap(),
        b"target data"
    );
    held.unmount().await.unwrap();
    remove_target(&database, &id, fixture.path()).await.unwrap();
    assert!(!path.exists());
    assert!(
        database
            .peek()
            .await
            .as_public()
            .as_backup_targets()
            .as_idx(&id)
            .is_none()
    );
}

#[tokio::test]
async fn content_inspection_validates_backup_metadata_before_legacy_edits_publish() {
    let root = tempfile::tempdir().unwrap();
    let (inspection, contents) = inspect_contents(root.path(), "server").await.unwrap();
    assert_eq!(
        inspection.availability,
        crate::shared_volumes::SharedVolumeAvailability::Available
    );
    assert!(inspection.checked_at.is_some());
    assert!(contents.start_os.is_empty());
    assert!(!contents.legacy_backup);
    let metadata = crate::disk::util::BackupUnencryptedMetadata::path(root.path(), "server");
    tokio::fs::create_dir_all(metadata.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(metadata, b"{broken").await.unwrap();
    let error = inspect_contents(root.path(), "server").await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::Deserialization);
}

#[tokio::test]
async fn target_initialization_and_content_recovery_require_validated_contents() {
    let mut db = database();
    initialize(&mut db).unwrap();
    let id = "cifs-7".parse().unwrap();
    let source = BackupTargetSource::read(&db, &id).unwrap();
    let available = SharedVolumeInspection::available(Some(123));
    write_inspection(
        &mut db,
        &id,
        &source.definition,
        None,
        available.clone(),
        None,
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

    let root = tempfile::tempdir().unwrap();
    let contents = read_contents(root.path(), "server").await.unwrap();
    write_inspection(
        &mut db,
        &id,
        &source.definition,
        None,
        available.clone(),
        Some(&contents),
    )
    .unwrap();
    let metadata_path = crate::disk::util::BackupUnencryptedMetadata::path(root.path(), "server");
    tokio::fs::create_dir_all(metadata_path.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&metadata_path, b"{broken").await.unwrap();
    let error = read_contents(root.path(), "server").await.unwrap_err();
    let failed = SharedVolumeInspection::unavailable(source::public_error_message(error));
    write_inspection(&mut db, &id, &source.definition, None, failed.clone(), None).unwrap();
    write_inspection(&mut db, &id, &source.definition, None, available, None).unwrap();
    assert_eq!(
        db.as_public()
            .as_backup_targets()
            .as_idx(&id)
            .unwrap()
            .as_inspection()
            .de()
            .unwrap(),
        failed
    );

    let metadata = crate::disk::util::BackupUnencryptedMetadata {
        hostname: crate::hostname::ServerHostname::new("fixture".into()).unwrap(),
        version: "0.4.2".parse().unwrap(),
        timestamp: chrono::Utc::now(),
        password_hash: Some("private-hash".into()),
        wrapped_key: Some("private-key".into()),
    };
    tokio::fs::write(&metadata_path, serde_json::to_vec(&metadata).unwrap())
        .await
        .unwrap();
    let contents = read_contents(root.path(), "server").await.unwrap();
    let inspection = SharedVolumeInspection::available_capacity(Err("capacity diagnostic".into()));
    write_inspection(
        &mut db,
        &id,
        &source.definition,
        None,
        inspection.clone(),
        Some(&contents),
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
    assert_eq!(target.start_os["server"].hostname.to_string(), "fixture");
    let public = serde_json::to_string(&target).unwrap();
    assert!(!public.contains("private-hash"));
    assert!(!public.contains("private-key"));
}

fn database() -> DatabaseModel {
    DatabaseModel::from(to_value(&serde_json::json!({
        "public": {"backupTargets": {}, "scheduledBackups": crate::backup::scheduled::ScheduledBackupState::default()},
        "private": {"backupTargetCredentials": {}, "scheduledBackupCredentials": {}, "cifs": {
            "7": {"hostname": "nas.local", "path": "/backups", "username": "family", "password": "private-password"}
        }}
    })).unwrap())
}

#[tokio::test]
async fn registered_filesystem_preserves_inline_backup_cifs_policy_identity() {
    use crate::backup::target::BackupTargetFS;
    use crate::disk::mount::filesystem::{BackupWrite, FileSystem, ReadWrite};
    let mut db = database();
    initialize(&mut db).unwrap();
    let id = "cifs-7".parse().unwrap();
    let target = BackupTargetSource::read(&db, &id).unwrap();
    let filesystem = filesystem(&id, &target.definition);
    let inline = BackupTargetFS::Cifs(target.definition.cifs().unwrap());
    for mode in [ReadOnly, ReadWrite, BackupWrite] {
        assert_eq!(
            filesystem.mount_hash(mode).await.unwrap(),
            inline.mount_hash(mode).await.unwrap()
        );
    }
}

fn backup_drive() -> crate::disk::util::DiskInfo {
    use crate::disk::util::{DiskInfo, PartitionInfo, PartitionTable};
    DiskInfo {
        logicalname: "/dev/sdb".into(),
        stable_path: "/dev/disk/by-path/backup-drive".into(),
        partition_table: Some(PartitionTable::Gpt),
        vendor: None,
        model: None,
        capacity: 1 << 30,
        guid: None,
        filesystem: None,
        partitions: vec![PartitionInfo {
            logicalname: "/dev/sdb1".into(),
            stable_path: "/dev/disk/by-path/backup-drive-part1".into(),
            label: Some("Backups".into()),
            capacity: 1 << 30,
            used: None,
            available: None,
            start_os: BTreeMap::from([("server".to_owned(), StartOsRecoveryInfo::default())]),
            legacy_backup: true,
            guid: None,
            filesystem: None,
        }],
    }
}

#[test]
fn boot_keeps_attached_backups_unregistered_until_explicit_discovery() {
    let mut db = database();
    let disks = vec![backup_drive()];
    db.as_public_mut().as_disks_mut().ser(&disks).unwrap();
    for _ in 0..2 {
        initialize(&mut db).unwrap();
        let targets = db.as_public().as_backup_targets().de().unwrap();
        assert_eq!(targets.len(), 1);
        assert!(targets.contains_key(&"cifs-7".parse().unwrap()));
        assert_eq!(
            serde_json::to_value(db.as_public().as_disks().de().unwrap()).unwrap(),
            serde_json::to_value(&disks).unwrap()
        );
    }
}

#[test]
fn find_selects_v2_backup_roots_but_not_empty_or_legacy_only_partitions() {
    let mut drive = backup_drive();
    drive.partitions[0].legacy_backup = false;
    let mut legacy = drive.partitions[0].clone();
    legacy.logicalname = "/dev/sdb2".into();
    legacy.start_os.clear();
    legacy.legacy_backup = true;
    let mut empty = legacy.clone();
    empty.logicalname = "/dev/sdb3".into();
    empty.legacy_backup = false;
    drive.partitions.extend([legacy, empty]);
    let disks = [drive];
    let found = backup_partitions(&disks)
        .map(|partition| partition.logicalname.as_path())
        .collect::<Vec<_>>();
    assert_eq!(found, [Path::new("/dev/sdb1")]);
}

#[test]
fn find_registration_preserves_existing_targets_and_can_rediscover_removed_backups() {
    let mut db = database();
    initialize(&mut db).unwrap();
    let root = SharedVolumeSource::Disk {
        logicalname: "/dev/disk/by-uuid/backup-filesystem".into(),
        path: PathBuf::new(),
        drive: Some(SharedVolumeDrive {
            label: Some("Backup drive".into()),
            ..Default::default()
        }),
    };
    let mut nested = root.clone();
    if let SharedVolumeSource::Disk { path, .. } = &mut nested {
        *path = "nested".into();
    }
    let nested_id = BackupTargetId::new();
    db.as_public_mut()
        .as_backup_targets_mut()
        .insert(&nested_id, &BackupTarget::new("Nested".into(), nested))
        .unwrap();
    let found = register_found(
        &mut db,
        vec![
            ("Found drive".into(), root.clone()),
            ("Duplicate".into(), root.clone()),
        ],
    )
    .unwrap();
    assert_eq!(found.len(), 1);
    let id = &found[0];
    let target = db
        .as_public()
        .as_backup_targets()
        .as_idx(id)
        .unwrap()
        .de()
        .unwrap();
    assert_eq!(target.name, "Found drive");
    assert_eq!(target.inspection, SharedVolumeInspection::default());
    assert!(target.start_os.is_empty());
    assert!(!target.legacy_backup);
    db.as_public_mut()
        .as_backup_targets_mut()
        .as_idx_mut(id)
        .unwrap()
        .as_name_mut()
        .ser(&"Renamed".to_owned())
        .unwrap();
    let mut relabeled = root.clone();
    if let SharedVolumeSource::Disk { drive, .. } = &mut relabeled {
        drive.get_or_insert_default().label = Some("New label".into());
    }
    assert!(
        register_found(&mut db, vec![("New label".into(), relabeled)])
            .unwrap()
            .is_empty()
    );
    assert_eq!(BackupTargetSource::read(&db, id).unwrap().name, "Renamed");
    assert_eq!(db.as_public().as_backup_targets().de().unwrap().len(), 3);
    assert_eq!(
        BackupTargetSource::read(&db, &"cifs-7".parse().unwrap())
            .unwrap()
            .definition
            .cifs()
            .unwrap()
            .password
            .as_deref(),
        Some("private-password")
    );
    db.as_public_mut()
        .as_backup_targets_mut()
        .remove(id)
        .unwrap();
    let restored = register_found(&mut db, vec![("Found again".into(), root)]).unwrap();
    assert_eq!(restored.len(), 1);
    assert_ne!(restored[0], *id);
    assert!(
        db.as_public()
            .as_backup_targets()
            .as_idx(&nested_id)
            .is_some()
    );
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
        Some(&TargetContents {
            start_os: BTreeMap::new(),
            legacy_backup: true,
        }),
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
        Some(&TargetContents {
            start_os: BTreeMap::new(),
            legacy_backup: true,
        }),
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

#[tokio::test]
async fn a_schedule_prevents_target_removal_before_local_data_is_deleted() {
    use crate::backup::scheduled::{
        BackupJob, BackupJobId, BackupServiceScope, RetentionPolicy, Schedule,
    };
    let fixture = tempfile::tempdir().unwrap();
    let id = BackupTargetId::new();
    let mut model = database();
    model
        .as_public_mut()
        .as_backup_targets_mut()
        .insert(
            &id,
            &BackupTarget::new("Scheduled".into(), SharedVolumeSource::Local),
        )
        .unwrap();
    let now = chrono::Utc::now();
    let job = BackupJob {
        id: BackupJobId::new(),
        name: "Nightly".into(),
        enabled: true,
        pause: None,
        target_id: id.clone(),
        target_instance_id: "original".into(),
        services: BackupServiceScope::AllExcept {
            excluded_package_ids: Default::default(),
        },
        schedule: Schedule::new("0 3 * * *", "UTC").unwrap(),
        default_retention: RetentionPolicy::latest_only(),
        retention_overrides: Default::default(),
        status: Default::default(),
        created_at: now,
        updated_at: now,
    };
    model
        .as_public_mut()
        .as_scheduled_backups_mut()
        .as_jobs_mut()
        .insert(&job.id, &job)
        .unwrap();
    let database = TypedPatchDb::<crate::db::model::Database>::load_unchecked(
        patch_db::PatchDb::open(fixture.path().join("state.db"))
            .await
            .unwrap(),
    );
    database.put(&ROOT, &model).await.unwrap();
    let path = local_path(fixture.path(), &id);
    tokio::fs::create_dir_all(&path).await.unwrap();
    tokio::fs::write(path.join("checkpoint"), b"retained")
        .await
        .unwrap();
    let error = remove_target(&database, &id, fixture.path())
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidRequest);
    assert_eq!(
        tokio::fs::read(path.join("checkpoint")).await.unwrap(),
        b"retained"
    );
    assert!(
        database
            .peek()
            .await
            .as_public()
            .as_backup_targets()
            .as_idx(&id)
            .is_some()
    );
}
