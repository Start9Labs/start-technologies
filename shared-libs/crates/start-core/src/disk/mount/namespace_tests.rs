use super::*;
use crate::disk::mount::filesystem::bind::Bind;
use crate::disk::mount::filesystem::overlayfs::OverlayGuard;
use crate::disk::mount::filesystem::private_tmpfs::PrivateTmpfs;
use crate::disk::mount::filesystem::{FileSystem, ReadOnly, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, MountGuard, TmpMountGuard};

fn peer_has(path: &Path) -> bool {
    let pid = std::env::var("FDE_NAMESPACE_PEER_PID").expect("real systemd sandbox PID");
    let mounts = std::fs::read_to_string(format!("/proc/{pid}/mountinfo")).unwrap();
    mounts
        .lines()
        .any(|line| line.split_whitespace().nth(4) == path.to_str())
}

#[tokio::test]
#[ignore = "requires exclusive managed guest, shared non-/tmp fixture root and pre-existing native systemd slave namespace"]
async fn native_slave_namespace_releases_rootfs_and_nested_staging_copies() {
    let root = std::path::PathBuf::from(
        std::env::var_os("FDE_NAMESPACE_TEST_ROOT").expect("owned non-/tmp root"),
    );
    let source = root.join("source");
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
    std::fs::write(source.join("main/marker"), b"data").unwrap();
    assert!(peer_has(&source));
    assert!(peer_has(&source.join("main")));
    for mode in [ReadOnly, ReadWrite] {
        let lower = MountGuard::mount(&PrivateTmpfs::new(), root.join("lower"), ReadWrite)
            .await
            .unwrap();
        let root_path = root.join("rootfs");
        let rootfs = OverlayGuard::mount(lower, &root_path).await.unwrap();
        Bind::new(&source)
            .recursive(true)
            .mount(rootfs.path().join("dependency"), mode)
            .await
            .unwrap();
        assert!(peer_has(&root_path));
        assert!(peer_has(&root_path.join("dependency/main")));
        rootfs.unmount(true).await.unwrap();
        assert!(
            !peer_has(&root_path),
            "{mode:?}: rootfs copy stranded in systemd namespace"
        );
        assert!(
            !peer_has(&root_path.join("dependency/main")),
            "{mode:?}: dependency copy stranded in systemd namespace"
        );
        assert_eq!(std::fs::read(source.join("main/marker")).unwrap(), b"data");
        let staging = TmpMountGuard::mount(&Bind::new(&source).recursive(true), mode)
            .await
            .unwrap();
        let staged = staging.path().to_owned();
        std::fs::write(
            root.join("staging-path"),
            staged.as_os_str().as_encoded_bytes(),
        )
        .unwrap();
        assert!(peer_has(&staged));
        assert!(peer_has(&staged.join("main")));
        staging.unmount().await.unwrap();
        assert!(
            !peer_has(&staged),
            "{mode:?}: nested staging copy stranded in systemd namespace"
        );
        assert!(
            !peer_has(&staged.join("main")),
            "{mode:?}: nested data copy stranded in systemd namespace"
        );
        assert_eq!(std::fs::read(source.join("main/marker")).unwrap(), b"data");
    }
    main.unmount(false).await.unwrap();
    source_mount.unmount(false).await.unwrap();
    assert!(!peer_has(&source));
    assert!(!peer_has(&source.join("main")));
}
