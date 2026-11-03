use super::*;

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
