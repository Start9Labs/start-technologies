use super::*;
use crate::disk::mount::filesystem::bind::{Bind, FileType};
use crate::disk::mount::filesystem::idmapped::{IdMap, IdMapped};
use crate::disk::mount::filesystem::overlayfs::OverlayGuard;
use crate::disk::mount::filesystem::private_tmpfs::PrivateTmpfs;
use crate::disk::mount::filesystem::{FileSystem, ReadOnly, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, MountGuard};

#[tokio::test]
#[ignore = "requires exclusive managed guest with native mount/idmap privileges and STARTOS_TEST_USERNS_HELPER"]
async fn recursive_file_bind_retirement_preserves_source() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    let source_mount = MountGuard::mount(&PrivateTmpfs::new(), &source, ReadWrite)
        .await
        .unwrap()
        .with_strict_unmount();
    let file = source.join("file with space");
    std::fs::write(&file, b"source file").unwrap();
    let target = directory.path().join("file view");
    let filesystem = IdMapped::new(
        Bind::new(&file).with_type(FileType::File).recursive(true),
        vec![IdMap {
            from_id: 0,
            to_id: 0,
            range: 65536,
        }],
    );
    for mode in [ReadOnly, ReadWrite] {
        let mounted = MountGuard::mount(&filesystem, &target, mode)
            .await
            .unwrap()
            .with_strict_unmount();
        assert_eq!(std::fs::read(&target).unwrap(), b"source file");
        assert!(is_mountpoint(&target).await.unwrap());
        if mode == ReadOnly {
            assert_eq!(
                std::fs::write(&target, b"denied")
                    .unwrap_err()
                    .raw_os_error(),
                Some(libc::EROFS)
            );
        } else {
            std::fs::write(&target, b"source file").unwrap();
        }
        mounted.unmount(false).await.unwrap();
        assert!(!is_mountpoint(&target).await.unwrap());
        assert_eq!(std::fs::read(&file).unwrap(), b"source file");
    }
    source_mount.unmount(false).await.unwrap();
}

#[tokio::test]
#[ignore = "requires exclusive managed guest with shared mount/OverlayFS privileges"]
async fn lazy_attachment_and_overlay_root_retirement_preserve_shared_source_children() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    let source_mount = MountGuard::mount(&PrivateTmpfs::new(), &source, ReadWrite)
        .await
        .unwrap()
        .with_strict_unmount();
    tokio::process::Command::new("mount")
        .arg("--make-shared")
        .arg(&source)
        .invoke(crate::ErrorKind::Filesystem)
        .await
        .unwrap();
    let main = MountGuard::mount(&PrivateTmpfs::new(), source.join("main"), ReadWrite)
        .await
        .unwrap()
        .with_strict_unmount();
    std::fs::write(source.join("main/marker"), b"source marker").unwrap();
    let filesystem = Bind::new(&source).recursive(true);
    let live = MountGuard::mount(&filesystem, directory.path().join("live peer"), ReadWrite)
        .await
        .unwrap()
        .with_strict_unmount();
    let remount = directory.path().join("remount");
    filesystem.mount(&remount, ReadWrite).await.unwrap();
    let retained_file = std::fs::File::open(remount.join("main/marker")).unwrap();
    unmount(&remount, true).await.unwrap();
    assert!(is_mountpoint(source.join("main")).await.unwrap());
    assert_eq!(
        std::fs::read(live.path().join("main/marker")).unwrap(),
        b"source marker"
    );
    drop(retained_file);
    let lower = MountGuard::mount(
        &PrivateTmpfs::new(),
        directory.path().join("lower"),
        ReadWrite,
    )
    .await
    .unwrap();
    let rootfs = OverlayGuard::mount(lower, directory.path().join("rootfs"))
        .await
        .unwrap();
    filesystem
        .mount(rootfs.path().join("dependency"), ReadWrite)
        .await
        .unwrap();
    rootfs.unmount(true).await.unwrap();
    assert!(is_mountpoint(source.join("main")).await.unwrap());
    assert_eq!(
        std::fs::read(live.path().join("main/marker")).unwrap(),
        b"source marker"
    );
    live.unmount(false).await.unwrap();
    assert!(is_mountpoint(source.join("main")).await.unwrap());
    std::fs::write(source.join("main/marker"), b"still writable").unwrap();
    main.unmount(false).await.unwrap();
    source_mount.unmount(false).await.unwrap();
}
