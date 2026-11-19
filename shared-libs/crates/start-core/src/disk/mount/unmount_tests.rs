use super::*;
use crate::disk::mount::filesystem::bind::{Bind, FileType};
use crate::disk::mount::filesystem::idmapped::{IdMap, IdMapped};
use crate::disk::mount::filesystem::overlayfs::OverlayGuard;
use crate::disk::mount::filesystem::private_tmpfs::PrivateTmpfs;
use crate::disk::mount::filesystem::{FileSystem, ReadOnly, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, MountGuard};

#[tokio::test]
async fn native_mount_inspection_decodes_paths_and_rejects_uninspectable_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let table = directory.path().join("mountinfo");
    tokio::fs::write(&table, b"19 1 0:1 / /owned/with\\040space rw - tmpfs tmpfs rw\n20 1 0:2 / /owned/line\\012break\\134slash rw - tmpfs tmpfs rw\n")
        .await.unwrap();
    assert_eq!(
        read_mountpoints(&table).await.unwrap(),
        vec![
            PathBuf::from("/owned/with space"),
            PathBuf::from("/owned/line\nbreak\\slash"),
        ]
    );
    tokio::fs::write(
        &table,
        b"19 1 0:1 / /owned/invalid-\xff rw - tmpfs tmpfs rw\n",
    )
    .await
    .unwrap();
    assert!(read_mountpoints(&table).await.is_err());
    tokio::fs::write(&table, b"not a kernel mount table\n")
        .await
        .unwrap();
    assert!(read_mountpoints(&table).await.is_err());
}

#[tokio::test]
#[ignore = "requires exclusive managed guest and mount privileges"]
async fn native_failed_temporary_teardown_preserves_mounted_contents() {
    assert_eq!(
        std::env::var("STARTOS_TEST_MANAGED_GUEST").as_deref(),
        Ok("1")
    );
    use futures::StreamExt;
    use inotify::{Inotify, WatchMask};

    use crate::util::io::TmpDir;

    let directory = tempfile::tempdir().unwrap();
    let temporary = TmpDir::new_in(directory.path()).await.unwrap();
    let temporary_path = temporary.to_path_buf();
    let mounted = temporary_path.join("busy child\nwith space");
    PrivateTmpfs::new()
        .mount(&mounted, ReadWrite)
        .await
        .unwrap();
    std::fs::write(mounted.join("witness"), b"mounted contents").unwrap();
    let notify = Inotify::init().unwrap();
    notify.watches().add(&mounted, WatchMask::DELETE).unwrap();
    let mut events = notify.into_event_stream([0u8; 1024]).unwrap();
    let mut child = std::process::Command::new("/bin/sleep")
        .arg("60")
        .current_dir(&mounted)
        .spawn()
        .unwrap();
    let cleanup_mount = mounted.clone();
    let cleanup = crate::util::GeneralBoxedGuard::new(move || {
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::process::Command::new("/usr/bin/umount")
            .arg("--")
            .arg(cleanup_mount)
            .output();
    });
    assert!(temporary.unmount_and_delete().await.is_err());
    assert!(
        tokio::time::timeout(Duration::from_millis(300), events.next())
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read(mounted.join("witness")).unwrap(),
        b"mounted contents"
    );
    assert!(temporary_path.is_dir());
    drop(cleanup);
    assert!(!is_mountpoint(&mounted).await.unwrap());
    crate::util::io::delete_dir(&temporary_path).await.unwrap();
}

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
