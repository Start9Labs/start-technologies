use super::*;

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
