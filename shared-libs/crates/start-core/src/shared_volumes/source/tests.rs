use super::*;

#[test]
fn observations_are_flattened_and_preserve_the_volume_definition() {
    let id = SharedVolumeId::new();
    let mut db = crate::db::model::DatabaseModel::from(
        to_value(&serde_json::json!({
            "public": { "sharedVolumes": {
                id.to_string(): {
                    "name": "Photos",
                    "source": {"kind": "local"},
                    "grants": {"consumer": {"readonly": true}},
                    "availability": "unknown"
                }
            }}
        }))
        .unwrap(),
    );
    let before = db
        .as_public()
        .as_shared_volumes()
        .as_idx(&id)
        .unwrap()
        .de()
        .unwrap();
    assert_eq!(before.inspection, SharedVolumeInspection::default());
    let inspection = SharedVolumeInspection::available(Some(1234));
    write_inspection(&mut db, &id, inspection.clone()).unwrap();
    let after = db
        .as_public()
        .as_shared_volumes()
        .as_idx(&id)
        .unwrap()
        .de()
        .unwrap();
    assert_eq!(after.name, before.name);
    assert_eq!(after.source, before.source);
    assert_eq!(after.grants, before.grants);
    assert_eq!(after.inspection, inspection);
    let encoded = serde_json::to_value(after).unwrap();
    assert_eq!(encoded["available"], 1234);
    assert!(encoded["error"].is_null());
    assert!(encoded["checkedAt"].is_string());
    assert!(encoded.get("inspection").is_none());
    let unavailable = SharedVolumeInspection::unavailable("source disconnected".into());
    write_inspection(&mut db, &id, unavailable.clone()).unwrap();
    assert_eq!(
        db.as_public()
            .as_shared_volumes()
            .as_idx(&id)
            .unwrap()
            .as_inspection()
            .de()
            .unwrap(),
        unavailable
    );
}

#[test]
fn a_finished_check_does_not_resurrect_a_removed_volume() {
    let mut db = crate::db::model::DatabaseModel::from(
        to_value(&serde_json::json!({
            "public": {"sharedVolumes": {}}
        }))
        .unwrap(),
    );
    write_inspection(
        &mut db,
        &SharedVolumeId::new(),
        SharedVolumeInspection::available(None),
    )
    .unwrap();
    assert!(db.as_public().as_shared_volumes().de().unwrap().is_empty());
}

#[test]
fn source_failures_keep_their_reason_kind_and_metadata() {
    for (kind, reason) in [
        (ErrorKind::Network, "DNS could not resolve the server"),
        (ErrorKind::Filesystem, "mount error(13): Permission denied"),
        (ErrorKind::Timeout, "mount deadline elapsed"),
    ] {
        let info = imbl_value::json!({ "operation": "source" });
        let error = source_result::<PreparedSource>(Err(
            Error::new(eyre!(reason), kind).with_info(info.clone())
        ))
        .unwrap_err();
        assert_eq!(error.kind, kind);
        assert_eq!(error.info, info);
        assert!(error.to_string().contains(reason));
        assert!(
            error
                .to_string()
                .contains(&*t!("shared-volumes.source-unavailable"))
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn stored_folder_failures_are_source_errors_without_recreating_paths() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("photos")).unwrap();
    std::fs::rename(root.path().join("photos"), root.path().join("renamed")).unwrap();
    std::fs::write(root.path().join("file"), b"not a directory").unwrap();
    symlink(outside.path(), root.path().join("escape")).unwrap();
    symlink("cycle", root.path().join("cycle")).unwrap();
    for path in ["photos", "file/child", "escape", "cycle"] {
        let error = open_beneath(root.path(), Path::new(path)).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidRequest);
        let cause = error.source.downcast_ref::<std::io::Error>().unwrap();
        let reason = cause.to_string();
        let errno = cause.raw_os_error();
        let error = source_result::<PreparedSource>(Err(error)).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Filesystem);
        assert!(error.to_string().contains(&reason));
        assert_eq!(
            error
                .source
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .raw_os_error(),
            errno
        );
    }
    assert!(!root.path().join("photos").exists());
}

#[cfg(target_os = "linux")]
#[test]
fn folder_creation_is_confined_and_preserves_existing_contents() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    create_directory_beneath(root.path(), Path::new("/library/photos")).unwrap();
    let sentinel = root.path().join("library/photos/sentinel");
    std::fs::write(&sentinel, b"preserved").unwrap();
    create_directory_beneath(root.path(), Path::new("library/photos")).unwrap();
    assert_eq!(std::fs::read(sentinel).unwrap(), b"preserved");
    symlink(outside.path(), root.path().join("escape")).unwrap();
    for path in ["../outside", "escape/new", "library/photos/sentinel/child"] {
        assert!(create_directory_beneath(root.path(), Path::new(path)).is_err());
    }
    assert!(!outside.path().join("new").exists());
    symlink("library", root.path().join("inside")).unwrap();
    create_directory_beneath(root.path(), Path::new("inside/allowed")).unwrap();
    assert!(root.path().join("library/allowed").is_dir());
}

#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "requires root, STARTOS_TEST_EXTERNAL_PARTITION and STARTOS_TEST_USERNS_HELPER"]
async fn external_folder_is_created_pinned_and_limits_consumer_subpaths() {
    use crate::disk::mount::filesystem::bind::Bind;
    use crate::disk::mount::guard::MountGuard;
    use crate::util::Invoke;
    let device = PathBuf::from(std::env::var_os("STARTOS_TEST_EXTERNAL_PARTITION").unwrap());
    let chosen = prepare_external_source(device.clone(), PathBuf::from("/created-folder/view"))
        .await
        .unwrap();
    let SharedVolumeSource::Disk { logicalname, path } = chosen else {
        panic!("disk source")
    };
    assert_eq!(path, Path::new("created-folder/view"));
    prepare_external_source(device.clone(), path.clone())
        .await
        .unwrap();
    let source = PreparedSource::open(
        open_external(&logicalname, ReadWrite).await.unwrap(),
        Some(path.clone()),
    )
    .await
    .unwrap();
    assert!(!source.path().join("external-sentinel").exists());
    let consumer_root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(consumer_root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let consumer_path = consumer_root.path().join("volume");
    let consumer = MountGuard::mount(&Bind::new(source.path()), &consumer_path, ReadWrite)
        .await
        .unwrap();
    let uid = crate::lxc::BASE_IDMAP.to_id.to_string();
    tokio::process::Command::new("setpriv")
        .args([
            "--reuid",
            &uid,
            "--regid",
            &uid,
            "--clear-groups",
            "sh",
            "-c",
        ])
        .arg("mkdir -p \"$1/child\"; printf 'selected-only\\n' > \"$1/child/file\"")
        .arg("_")
        .arg(&consumer_path)
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    consumer.unmount(true).await.unwrap();
    let raw = TmpMountGuard::mount(&BlockDev::new(&device), ReadWrite)
        .await
        .unwrap();
    std::fs::rename(
        raw.path().join(&path),
        raw.path().join("created-folder/renamed"),
    )
    .unwrap();
    assert_eq!(
        std::fs::read(source.path().join("child/file")).unwrap(),
        b"selected-only\n"
    );
    let child = source
        .into_subpath(Some(PathBuf::from("child")))
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(child.path().join("file")).unwrap(),
        b"selected-only\n"
    );
    assert!(
        child
            .into_subpath(Some(PathBuf::from("../..")))
            .await
            .is_err()
    );
    std::fs::rename(
        raw.path().join("created-folder/renamed"),
        raw.path().join(&path),
    )
    .unwrap();
    raw.unmount().await.unwrap();
    let readonly = PreparedSource::open(
        open_external(&logicalname, ReadOnly).await.unwrap(),
        Some(path),
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read(readonly.path().join("child/file")).unwrap(),
        b"selected-only\n"
    );
    assert_eq!(
        std::fs::write(readonly.path().join("forbidden"), b"no")
            .unwrap_err()
            .raw_os_error(),
        Some(libc::EROFS)
    );
    readonly.unmount().await.unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "requires root, STARTOS_TEST_EXTERNAL_PARTITION and STARTOS_TEST_USERNS_HELPER"]
async fn external_partition_maps_service_ownership_without_changing_disk_owners() {
    use std::os::unix::fs::MetadataExt;

    use crate::disk::mount::filesystem::bind::Bind;
    use crate::disk::mount::filesystem::{ReadOnly, ReadWrite};
    use crate::disk::mount::guard::{GenericMountGuard, MountGuard};
    use crate::util::Invoke;

    let device = PathBuf::from(std::env::var_os("STARTOS_TEST_EXTERNAL_PARTITION").unwrap());
    let persistent = BlockDev::new(&device).persistent_path().await.unwrap();
    assert_eq!(std::fs::canonicalize(&persistent).unwrap(), device);
    let mapped = open_external(&persistent, ReadWrite).await.unwrap();
    assert_eq!(
        std::fs::read(mapped.path().join("external-sentinel")).unwrap(),
        b"external-preserved\n"
    );
    let metadata = std::fs::metadata(mapped.path()).unwrap();
    assert_eq!(metadata.uid(), crate::lxc::BASE_IDMAP.to_id);
    assert_eq!(metadata.gid(), crate::lxc::BASE_IDMAP.to_id);
    let consumer_root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(consumer_root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let consumer_path = consumer_root.path().join("volume");
    let consumer = MountGuard::mount(&Bind::new(mapped.path()), &consumer_path, ReadWrite)
        .await
        .unwrap();
    let uid = crate::lxc::BASE_IDMAP.to_id.to_string();
    tokio::process::Command::new("setpriv")
        .args([
            "--reuid",
            &uid,
            "--regid",
            &uid,
            "--clear-groups",
            "sh",
            "-c",
        ])
        .arg("cat \"$1/external-sentinel\"; printf 'service-write\\n' > \"$1/service-created\"")
        .arg("_")
        .arg(&consumer_path)
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(mapped.path().join("service-created")).unwrap(),
        b"service-write\n"
    );
    consumer.unmount(true).await.unwrap();
    mapped.unmount().await.unwrap();

    let raw = TmpMountGuard::mount(&BlockDev::new(&persistent), ReadOnly)
        .await
        .unwrap();
    for name in ["external-sentinel", "service-created"] {
        let metadata = std::fs::metadata(raw.path().join(name)).unwrap();
        assert_eq!(metadata.uid(), 0);
        assert_eq!(metadata.gid(), 0);
    }
    raw.unmount().await.unwrap();

    let readonly = open_external(&persistent, ReadOnly).await.unwrap();
    assert_eq!(
        std::fs::read(readonly.path().join("service-created")).unwrap(),
        b"service-write\n"
    );
    assert_eq!(
        std::fs::write(readonly.path().join("forbidden"), b"no")
            .unwrap_err()
            .raw_os_error(),
        Some(libc::EROFS)
    );
    readonly.unmount().await.unwrap();
}

#[test]
fn only_partitions_from_backup_discovery_can_be_registered() {
    let disks = serde_json::from_value::<Vec<crate::disk::util::DiskInfo>>(serde_json::json!([{
        "logicalname": "/dev/sdz",
        "stablePath": "/dev/disk/by-path/external",
        "partitionTable": "gpt",
        "vendor": null,
        "model": null,
        "capacity": 4096,
        "guid": null,
        "filesystem": null,
        "partitions": [{
            "logicalname": "/dev/sdz1",
            "stablePath": "/dev/disk/by-path/external-part1",
            "label": "Photos",
            "capacity": 4096,
            "used": null,
            "available": null,
            "startOs": {},
            "legacyBackup": false,
            "guid": null,
            "filesystem": null
        }]
    }]))
    .unwrap();
    for path in ["/dev/sdz1", "/dev/disk/by-path/external-part1"] {
        assert_eq!(
            external_partition(&disks, Path::new(path)).unwrap(),
            Path::new("/dev/sdz1")
        );
    }
    for path in ["/", "/dev/sdz", "/dev/system-root", "/dev/missing"] {
        assert!(external_partition(&disks, Path::new(path)).is_err());
    }
}
