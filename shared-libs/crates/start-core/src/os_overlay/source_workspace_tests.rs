use super::*;

#[tokio::test]
async fn workspace_file_cleanup_preserves_retained_image_aliases() {
    let root = tempfile::tempdir().unwrap();
    let cache = SourceCache::new(root.path());
    let workspace = cache.create_workspace().await.unwrap();
    std::fs::write(workspace.join("download.squashfs"), b"public image bytes").unwrap();
    std::fs::hard_link(workspace.join("download.squashfs"), workspace.join(IMAGE)).unwrap();
    let retained = root.path().join("retained.squashfs");
    std::fs::hard_link(workspace.join(IMAGE), &retained).unwrap();
    std::fs::write(workspace.join("image.verity"), b"public tree").unwrap();
    for name in ["source", "runtime", "rootfs"] {
        std::fs::create_dir(workspace.join(name)).unwrap();
    }
    cache.remove_workspace_files().await.unwrap();
    assert!(!workspace.exists());
    assert_eq!(std::fs::read(retained).unwrap(), b"public image bytes");
    assert!(cache.create_workspace().await.is_ok());
}

#[tokio::test]
async fn workspace_file_cleanup_never_recurses_into_remaining_targets() {
    let root = tempfile::tempdir().unwrap();
    let cache = SourceCache::new(root.path());
    let workspace = cache.create_workspace().await.unwrap();
    std::fs::create_dir(workspace.join("source")).unwrap();
    std::fs::write(workspace.join("source/retained"), b"keep").unwrap();
    assert!(cache.remove_workspace_files().await.is_err());
    assert_eq!(
        std::fs::read(workspace.join("source/retained")).unwrap(),
        b"keep"
    );
}

#[tokio::test]
async fn workspace_admission_rejects_cache_and_mount_target_redirects() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let cache = SourceCache::new(root.path());
    std::fs::create_dir(root.path().join("images")).unwrap();
    symlink(outside.path(), cache.directory()).unwrap();
    assert!(cache.create_workspace().await.is_err());
    assert!(cache.recover_workspace().await.is_err());
    assert!(std::fs::read_dir(outside.path()).unwrap().next().is_none());
    std::fs::remove_file(cache.directory()).unwrap();
    let workspace = cache.create_workspace().await.unwrap();
    symlink(outside.path(), workspace.join("source")).unwrap();
    assert!(cache.recover_workspace().await.is_err());
    assert!(outside.path().is_dir());
    assert!(
        std::fs::symlink_metadata(workspace.join("source"))
            .unwrap()
            .is_symlink()
    );
}

#[tokio::test]
#[ignore = "requires an exclusive managed guest and real device-mapper/mount privileges"]
async fn native_workspace_recovery_retires_stale_mounts_and_only_its_reserved_mapping() {
    use crate::disk::mount::guard::GenericMountGuard;
    use crate::os_overlay::VerityImage;

    let root =
        PathBuf::from(std::env::var_os("FDE_SOURCE_STAGE_ROOT").expect("owned guest fixture"));
    let lease = crate::os_overlay::reserve_image_store(true).await.unwrap();
    let cache = SourceCache::new(&root);
    let workspace = cache.create_workspace().await.unwrap();
    let image = crate::util::squashfs::Squashfs::<
        crate::s9pk::merkle_archive::source::DynFileSource,
    >::default();
    let mut output = crate::util::io::create_file_mod(workspace.join(IMAGE), 0o600)
        .await
        .unwrap();
    image.serialize(&mut output).await.unwrap();
    output.sync_all().await.unwrap();
    let data = workspace.join(IMAGE);
    let tree = workspace.join("image.verity");
    let verity = VerityImage::format(&data, &tree).await.unwrap();
    let unrelated_data = root.join("unrelated-data");
    let unrelated_tree = root.join("unrelated-tree");
    std::fs::write(&unrelated_data, vec![71; 4096]).unwrap();
    let unrelated_image = VerityImage::format(&unrelated_data, &unrelated_tree)
        .await
        .unwrap();
    let unrelated = unrelated_image
        .open(
            &unrelated_data,
            &unrelated_tree,
            &std::env::var("FDE_SOURCE_STAGE_CANARY").expect("owned canary mapping"),
        )
        .await
        .unwrap();
    let mounted = verity
        .mount(&data, &tree, SOURCE_MAPPING, &workspace.join("rootfs"))
        .await
        .unwrap();
    assert!(
        crate::disk::mount::util::is_mountpoint(mounted.path())
            .await
            .unwrap()
    );
    let abandoned = mounted.persist();
    cache.recover_workspace().await.unwrap();
    assert!(!workspace.exists());
    assert!(
        !crate::disk::mount::util::is_mountpoint(abandoned)
            .await
            .unwrap()
    );
    assert!(
        !std::path::Path::new("/dev/mapper")
            .join(SOURCE_MAPPING)
            .exists()
    );
    cache.recover_workspace().await.unwrap();
    let workspace = cache.create_workspace().await.unwrap();
    cache.recover_workspace().await.unwrap();
    assert!(!workspace.exists());
    assert_eq!(std::fs::read(unrelated.path()).unwrap(), vec![71; 4096]);
    unrelated.close().await.unwrap();
    drop(lease);
}
