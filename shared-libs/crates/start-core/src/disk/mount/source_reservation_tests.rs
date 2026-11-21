use super::*;
use crate::disk::mount::filesystem::bind::Bind;

#[tokio::test]
#[ignore = "requires an exclusive managed guest and native kernel bind mounts"]
async fn native_source_removal_rejects_residual_cache_mounts_and_uninspectable_subtrees() {
    use std::os::unix::ffi::OsStringExt;

    assert_eq!(std::env::var("STARTOS_TEST_MANAGED_GUEST").unwrap(), "1");
    let root = tempfile::tempdir_in(std::env::var_os("FDE_ROOT_VIEW_TEST_ROOT").unwrap()).unwrap();
    let source = root.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("sentinel"), b"retained source").unwrap();
    let filesystem = Bind::new(&source);
    let cached = tmp_mountpoint(&filesystem, ReadOnly).await.unwrap();
    let orphan = MountGuard::mount(&filesystem, &cached, ReadOnly)
        .await
        .unwrap()
        .with_strict_unmount();
    assert!(
        TmpMountGuard::reserve_unmounted(&filesystem, ReadOnly)
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read(orphan.path().join("sentinel")).unwrap(),
        b"retained source"
    );
    orphan.unmount(false).await.unwrap();
    let residual = MountGuard::mount(&filesystem, cached.join("residual"), ReadOnly)
        .await
        .unwrap()
        .with_strict_unmount();
    assert!(
        TmpMountGuard::reserve_unmounted(&filesystem, ReadOnly)
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read(residual.path().join("sentinel")).unwrap(),
        b"retained source"
    );
    residual.unmount(false).await.unwrap();
    let invalid = cached.join(std::ffi::OsString::from_vec(b"native-\xff".to_vec()));
    let uninspectable = MountGuard::mount(&filesystem, &invalid, ReadOnly)
        .await
        .unwrap()
        .with_strict_unmount();
    assert!(
        TmpMountGuard::reserve_unmounted(&filesystem, ReadOnly)
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read(uninspectable.path().join("sentinel")).unwrap(),
        b"retained source"
    );
    uninspectable.unmount(false).await.unwrap();
    let unused = TmpMountGuard::reserve_unmounted(&filesystem, ReadOnly)
        .await
        .unwrap();
    {
        let mount = TmpMountGuard::mount(&filesystem, ReadOnly);
        tokio::pin!(mount);
        assert!(futures::poll!(&mut mount).is_pending());
    }
    drop(unused);
    let view = TmpMountGuard::mount(&filesystem, ReadOnly).await.unwrap();
    assert_eq!(
        std::fs::read(view.path().join("sentinel")).unwrap(),
        b"retained source"
    );
    view.unmount().await.unwrap();
    crate::util::io::delete_dir(&cached).await.unwrap();
    assert_eq!(
        std::fs::read(source.join("sentinel")).unwrap(),
        b"retained source"
    );
}
