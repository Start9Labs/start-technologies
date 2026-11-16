use std::ffi::OsStr;
use std::path::PathBuf;

use futures::FutureExt;
use tokio::process::Command;

use super::*;

async fn native(tool: &str, args: &[&OsStr]) -> String {
    String::from_utf8(
        Command::new(tool)
            .args(args)
            .invoke(ErrorKind::DiskManagement)
            .await
            .unwrap(),
    )
    .unwrap()
    .trim()
    .to_owned()
}

#[tokio::test]
async fn unknown_block_holders_reject_pool_creation_before_writing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("not-a-block-device");
    let marker = vec![0x5a; 4096];
    std::fs::write(&path, &marker).unwrap();
    assert!(create_pool(&[&path], &BTreeMap::new(), true).await.is_err());
    assert_eq!(std::fs::read(path).unwrap(), marker);
}

#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "requires exclusive managed guest, partitioned loop/LVM privileges; never run on build host"]
async fn real_partitioned_pool_refuses_busy_holders_before_wipe_and_reclaims_only_target() {
    use std::os::unix::fs::FileExt;

    let directory = tempfile::tempdir().unwrap();
    let image = directory.path().join("partitioned");
    std::fs::File::create(&image)
        .unwrap()
        .set_len(512 << 20)
        .unwrap();
    native(
        "sgdisk",
        &[
            OsStr::new("--clear"),
            OsStr::new("--new=1:2048:0"),
            OsStr::new("--typecode=1:8e00"),
            image.as_os_str(),
        ],
    )
    .await;
    let device = PathBuf::from(
        native(
            "losetup",
            &[
                OsStr::new("--find"),
                OsStr::new("--show"),
                OsStr::new("--partscan"),
                image.as_os_str(),
            ],
        )
        .await,
    );
    let mut groups = Vec::new();
    let mut mappings = Vec::new();
    let result = std::panic::AssertUnwindSafe(async {
        crate::disk::util::settle_udev().await.unwrap();
        let partitions = crate::os_install::quiesce::list_partitions(&device)
            .await
            .unwrap();
        assert_eq!(partitions.len(), 1);
        let partition = &partitions[0];
        let unrelated = format!("fde-partition-unrelated-{}", crate::util::new_guid());
        native(
            "dmsetup",
            &[
                OsStr::new("create"),
                OsStr::new(&unrelated),
                OsStr::new("--table"),
                OsStr::new("0 4096 zero"),
            ],
        )
        .await;
        mappings.push(unrelated.clone());
        let old = format!("STARTOS_{}", crate::util::new_guid());
        native("pvcreate", &[partition.as_os_str()]).await;
        native("vgcreate", &[OsStr::new(&old), partition.as_os_str()]).await;
        groups.push(old.clone());
        native(
            "lvcreate",
            &[
                OsStr::new("-L"),
                OsStr::new("64M"),
                OsStr::new("-n"),
                OsStr::new("main"),
                OsStr::new(&old),
            ],
        )
        .await;
        let holder = format!("fde-partition-held-{}", crate::util::new_guid());
        native(
            "dmsetup",
            &[
                OsStr::new("create"),
                OsStr::new(&holder),
                OsStr::new("--table"),
                OsStr::new(&format!("0 4096 linear /dev/{old}/main 0")),
            ],
        )
        .await;
        mappings.push(holder.clone());
        let held = std::fs::File::open(Path::new("/dev/mapper").join(&holder)).unwrap();
        let mut original = [0; 4096];
        std::fs::File::open(&device)
            .unwrap()
            .read_exact_at(&mut original, 0)
            .unwrap();
        assert!(
            create_pool(&[&device], &pvscan().await.unwrap(), true)
                .await
                .is_err()
        );
        let mut after = [0; 4096];
        std::fs::File::open(&device)
            .unwrap()
            .read_exact_at(&mut after, 0)
            .unwrap();
        assert_eq!(after, original);
        assert_eq!(
            pvscan().await.unwrap()[partition].as_deref(),
            Some(old.as_str())
        );
        drop(held);
        let guid = create_pool(&[&device], &pvscan().await.unwrap(), true)
            .await
            .unwrap();
        groups.push(guid.to_string());
        assert_eq!(pvscan().await.unwrap()[&device].as_ref(), Some(&guid));
        assert_eq!(
            native(
                "dmsetup",
                &[
                    OsStr::new("info"),
                    OsStr::new("--columns"),
                    OsStr::new("--noheadings"),
                    OsStr::new("-o"),
                    OsStr::new("name"),
                    OsStr::new(&unrelated)
                ]
            )
            .await,
            unrelated
        );
    })
    .catch_unwind()
    .await;
    for mapping in mappings.into_iter().rev() {
        let _ = Command::new("dmsetup")
            .args(["remove", &mapping])
            .output()
            .await;
    }
    for group in groups.into_iter().rev() {
        let _ = Command::new("vgremove")
            .args(["--force", "--yes", &group])
            .output()
            .await;
    }
    let _ = Command::new("losetup")
        .arg("--detach")
        .arg(&device)
        .output()
        .await;
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "requires exclusive managed guest, loop/device-mapper/LVM privileges; never run on build host"]
async fn real_pool_creation_and_import_retry_preserve_unrelated_mappings() {
    let directory = tempfile::tempdir().unwrap();
    let mut loops = Vec::<PathBuf>::new();
    let mut groups = Vec::<String>::new();
    let mut mappings = Vec::<String>::new();
    let result = std::panic::AssertUnwindSafe(async {
        for name in ["selected", "unrelated"] {
            let path = directory.path().join(name);
            std::fs::File::create(&path)
                .unwrap()
                .set_len(512 << 20)
                .unwrap();
            let device = native(
                "losetup",
                &[OsStr::new("--find"), OsStr::new("--show"), path.as_os_str()],
            )
            .await;
            loops.push(device.into());
        }
        let unrelated = format!("fde-unrelated-{}", crate::util::new_guid());
        let table = format!("0 4096 linear {} 0", loops[1].display());
        native(
            "dmsetup",
            &[
                OsStr::new("create"),
                OsStr::new(&unrelated),
                OsStr::new("--table"),
                OsStr::new(&table),
            ],
        )
        .await;
        mappings.push(unrelated.clone());
        let unrelated_path = Path::new("/dev/mapper").join(&unrelated);
        let marker = b"unrelated native mapper remains readable";
        {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .write(true)
                .open(&unrelated_path)
                .unwrap()
                .write_all(marker)
                .unwrap();
        }
        let old = format!("STARTOS_{}", crate::util::new_guid());
        native("pvcreate", &[loops[0].as_os_str()]).await;
        native("vgcreate", &[OsStr::new(&old), loops[0].as_os_str()]).await;
        groups.push(old.clone());
        native(
            "lvcreate",
            &[
                OsStr::new("-L"),
                OsStr::new("64M"),
                OsStr::new("-n"),
                OsStr::new("main"),
                OsStr::new(&old),
            ],
        )
        .await;
        let holder = format!("fde-held-{}", crate::util::new_guid());
        let table = format!("0 4096 linear /dev/{old}/main 0");
        native(
            "dmsetup",
            &[
                OsStr::new("create"),
                OsStr::new(&holder),
                OsStr::new("--table"),
                OsStr::new(&table),
            ],
        )
        .await;
        mappings.push(holder.clone());
        let held = std::fs::File::open(Path::new("/dev/mapper").join(&holder)).unwrap();
        assert!(
            create_pool(&[&loops[0]], &pvscan().await.unwrap(), true)
                .await
                .is_err()
        );
        assert_eq!(
            pvscan().await.unwrap()[&loops[0]].as_deref(),
            Some(old.as_str())
        );
        drop(held);
        let guid = create_pool(&[&loops[0]], &pvscan().await.unwrap(), true)
            .await
            .unwrap();
        groups.push(guid.to_string());
        assert_eq!(pvscan().await.unwrap()[&loops[0]].as_ref(), Some(&guid));
        let password = "native-import-retry-password";
        let pool = crate::disk::fde::LuksPool::new(&guid, directory.path()).unwrap();
        for name in POOL_FILESYSTEMS {
            native(
                "lvcreate",
                &[
                    OsStr::new("-L"),
                    OsStr::new("160M"),
                    OsStr::new("-n"),
                    OsStr::new(name),
                    OsStr::new(&guid),
                ],
            )
            .await;
            let mut child = Command::new("cryptsetup")
                .args([
                    "luksFormat",
                    "--batch-mode",
                    "--pbkdf",
                    "pbkdf2",
                    "--iter-time",
                    "10",
                    "--key-file=-",
                ])
                .arg(format!("/dev/{guid}/{name}"))
                .stdin(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            use tokio::io::AsyncWriteExt;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(password.as_bytes())
                .await
                .unwrap();
            assert!(child.wait().await.unwrap().success());
            let mapper = format!("{guid}_{name}");
            let opened = pool
                .open_volume(
                    &mapper,
                    crate::disk::fde::KeyCredential::Password(password),
                    false,
                )
                .await
                .unwrap();
            mappings.push(mapper.clone());
            native("mkfs.btrfs", &[OsStr::new("-f"), opened.as_os_str()]).await;
            if name == "package-data" {
                close_luks(&mapper).await.unwrap();
            }
        }
        let data = directory.path().join("data");
        std::fs::create_dir(&data).unwrap();
        let _ = import(&guid, &data, ImportMode::ReadOnly, Some(password), None)
            .await
            .unwrap();
        for name in POOL_FILESYSTEMS {
            let source = native(
                "findmnt",
                &[
                    OsStr::new("--noheadings"),
                    OsStr::new("--output"),
                    OsStr::new("SOURCE"),
                    OsStr::new("--target"),
                    data.join(name).as_os_str(),
                ],
            )
            .await;
            assert_eq!(source, format!("/dev/mapper/{guid}_{name}"));
        }
        let output = native(
            "dmsetup",
            &[
                OsStr::new("info"),
                OsStr::new("--columns"),
                OsStr::new("--noheadings"),
                OsStr::new("-o"),
                OsStr::new("name"),
                unrelated_path.as_os_str(),
            ],
        )
        .await;
        assert_eq!(output, unrelated);
        use std::io::Read;
        let mut actual = vec![0; marker.len()];
        std::fs::File::open(&unrelated_path)
            .unwrap()
            .read_exact(&mut actual)
            .unwrap();
        assert_eq!(actual, marker);
    })
    .catch_unwind()
    .await;
    for name in POOL_FILESYSTEMS {
        let _ = Command::new("umount")
            .arg(directory.path().join("data").join(name))
            .output()
            .await;
    }
    for mapper in mappings.into_iter().rev() {
        let _ = Command::new("cryptsetup")
            .arg("close")
            .arg(&mapper)
            .output()
            .await;
        let _ = Command::new("dmsetup")
            .arg("remove")
            .arg(&mapper)
            .output()
            .await;
    }
    for group in groups.into_iter().rev() {
        let _ = Command::new("vgremove")
            .args(["--force", "--yes", &group])
            .output()
            .await;
    }
    for device in loops.into_iter().rev() {
        let _ = Command::new("losetup")
            .arg("--detach")
            .arg(device)
            .output()
            .await;
    }
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
