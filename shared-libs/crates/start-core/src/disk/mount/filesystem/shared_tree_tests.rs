use super::*;
use crate::disk::mount::filesystem::idmapped::{IdMap, IdMapped};
use crate::disk::mount::filesystem::private_tmpfs::PrivateTmpfs;
use crate::disk::mount::filesystem::{ReadOnly, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, MountGuard, TmpMountGuard};
use crate::util::Invoke;

#[tokio::test]
#[ignore = "requires exclusive managed guest with mount/idmap privileges and native STARTOS_TEST_USERNS_HELPER"]
async fn shared_staging_retirement_preserves_source_and_live_peer_mounts() {
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
    std::fs::create_dir(source.join("late")).unwrap();
    let main = MountGuard::mount(&PrivateTmpfs::new(), source.join("main"), ReadWrite)
        .await
        .unwrap()
        .with_strict_unmount();
    std::fs::write(source.join("main/marker"), b"original main").unwrap();
    let filesystem = Bind::new(&source).recursive(true);
    let live = MountGuard::mount(&filesystem, directory.path().join("live"), ReadWrite)
        .await
        .unwrap()
        .with_strict_unmount();
    let readonly = MountGuard::mount(&filesystem, directory.path().join("readonly"), ReadOnly)
        .await
        .unwrap()
        .with_strict_unmount();
    let unmapped = MountGuard::mount(
        &IdMapped::new(Bind::new(&source).recursive(true), Vec::new()),
        directory.path().join("unmapped"),
        ReadWrite,
    )
    .await
    .unwrap()
    .with_strict_unmount();
    let idmapped = IdMapped::new(
        Bind::new(&source).recursive(true),
        vec![IdMap {
            from_id: 0,
            to_id: 0,
            range: 65536,
        }],
    );
    let mapped = MountGuard::mount(&idmapped, directory.path().join("mapped"), ReadWrite)
        .await
        .unwrap()
        .with_strict_unmount();
    let mapped_readonly = MountGuard::mount(
        &idmapped,
        directory.path().join("mapped-readonly"),
        ReadOnly,
    )
    .await
    .unwrap()
    .with_strict_unmount();
    for path in [
        source.join("main"),
        live.path().join("main"),
        mapped.path().join("main"),
        mapped_readonly.path().join("main"),
    ] {
        assert!(
            crate::disk::mount::util::is_mountpoint(&path)
                .await
                .unwrap()
        );
        assert_eq!(
            std::fs::read(path.join("marker")).unwrap(),
            b"original main"
        );
    }
    let staging = TmpMountGuard::mount(&filesystem, ReadWrite).await.unwrap();
    let second = MountGuard::mount(&idmapped, directory.path().join("second"), ReadWrite)
        .await
        .unwrap()
        .with_strict_unmount();
    let late = MountGuard::mount(&PrivateTmpfs::new(), source.join("late"), ReadWrite)
        .await
        .unwrap()
        .with_strict_unmount();
    std::fs::write(source.join("late/marker"), b"late child").unwrap();
    for path in [live.path(), staging.path(), unmapped.path()] {
        assert_eq!(
            std::fs::read(path.join("late/marker")).unwrap(),
            b"late child"
        );
    }
    assert!(
        !crate::disk::mount::util::is_mountpoint(readonly.path().join("late"))
            .await
            .unwrap()
    );
    assert!(!readonly.path().join("late/marker").exists());
    for path in [mapped.path(), mapped_readonly.path(), second.path()] {
        assert!(
            !crate::disk::mount::util::is_mountpoint(path.join("late"))
                .await
                .unwrap()
        );
        assert!(!path.join("late/marker").exists());
    }
    assert_eq!(
        std::fs::write(mapped_readonly.path().join("main/marker"), b"rejected")
            .unwrap_err()
            .raw_os_error(),
        Some(libc::EROFS)
    );
    staging.unmount().await.unwrap();
    for path in [
        source.join("main"),
        live.path().join("main"),
        mapped.path().join("main"),
        mapped_readonly.path().join("main"),
        second.path().join("main"),
    ] {
        assert_eq!(
            std::fs::read(path.join("marker")).unwrap(),
            b"original main"
        );
    }
    second.unmount(false).await.unwrap();
    mapped.unmount(false).await.unwrap();
    mapped_readonly.unmount(false).await.unwrap();
    unmapped.unmount(false).await.unwrap();
    live.unmount(false).await.unwrap();
    readonly.unmount(false).await.unwrap();
    assert!(
        crate::disk::mount::util::is_mountpoint(source.join("main"))
            .await
            .unwrap()
    );
    assert!(
        crate::disk::mount::util::is_mountpoint(source.join("late"))
            .await
            .unwrap()
    );
    std::fs::write(source.join("main/marker"), b"source still writable").unwrap();
    late.unmount(false).await.unwrap();
    main.unmount(false).await.unwrap();
    source_mount.unmount(false).await.unwrap();
}
