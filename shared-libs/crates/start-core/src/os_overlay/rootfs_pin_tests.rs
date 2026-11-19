use super::*;

#[tokio::test]
async fn published_rootfs_bytes_survive_original_replacement_and_unlink() {
    use std::os::unix::fs::MetadataExt;
    let root = tempfile::tempdir().unwrap();
    let raw = root.path().join("raw");
    let store = root.path().join("store");
    tokio::fs::create_dir(&raw).await.unwrap();
    tokio::fs::create_dir(&store).await.unwrap();
    let source = root.path().join("outside.private");
    let bytes = vec![42; VERITY_BLOCK_SIZE as usize * 2];
    tokio::fs::write(&source, &bytes).await.unwrap();
    let key = generate_signing_key().unwrap();
    let prepared = PreparedGeneration::import_raw(&raw, &source, &store, &key)
        .await
        .unwrap();
    let retained = prepared.staging_path().join(ROOTFS_FILE);
    assert!(
        !crate::util::io::same_file(&source, &retained)
            .await
            .unwrap()
    );
    assert_eq!(
        tokio::fs::metadata(&retained).await.unwrap().mode() & 0o777,
        0o600
    );
    let published = prepared.publish().await.unwrap();
    tokio::fs::write(&source, vec![21; bytes.len()])
        .await
        .unwrap();
    let retained = published.join(ROOTFS_FILE);
    assert_eq!(tokio::fs::read(&retained).await.unwrap(), bytes);
    tokio::fs::remove_file(source).await.unwrap();
    let public = PKey::public_key_from_der(&key.public_key_to_der().unwrap()).unwrap();
    assert_eq!(
        select_latest(&store, &retained, &public)
            .await
            .unwrap()
            .unwrap()
            .path,
        published
    );
}

#[tokio::test]
async fn publication_and_selection_reject_missing_redirected_or_changed_rootfs_bytes() {
    let root = tempfile::tempdir().unwrap();
    let raw = root.path().join("raw");
    let store = root.path().join("store");
    tokio::fs::create_dir(&raw).await.unwrap();
    tokio::fs::create_dir(&store).await.unwrap();
    let source = root.path().join("source");
    let bytes = vec![42; VERITY_BLOCK_SIZE as usize];
    tokio::fs::write(&source, &bytes).await.unwrap();
    let key = generate_signing_key().unwrap();
    let public = PKey::public_key_from_der(&key.public_key_to_der().unwrap()).unwrap();
    for variant in 0..3 {
        let prepared = PreparedGeneration::import_raw(&raw, &source, &store, &key)
            .await
            .unwrap();
        let retained = prepared.staging_path().join(ROOTFS_FILE);
        tokio::fs::remove_file(&retained).await.unwrap();
        match variant {
            0 => (),
            1 => tokio::fs::symlink(&source, &retained).await.unwrap(),
            _ => tokio::fs::write(&retained, vec![21; bytes.len()])
                .await
                .unwrap(),
        }
        assert!(prepared.publish().await.is_err());
        recover_unpublished(&store).await.unwrap();
        assert_eq!(tokio::fs::read(&source).await.unwrap(), bytes);
    }
    let published = PreparedGeneration::import_raw(&raw, &source, &store, &key)
        .await
        .unwrap()
        .publish()
        .await
        .unwrap();
    tokio::fs::write(published.join(ROOTFS_FILE), vec![21; bytes.len()])
        .await
        .unwrap();
    assert!(
        select_latest(&store, &source, &public)
            .await
            .unwrap()
            .is_none()
    );
}

#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "requires exclusive managed Btrfs guest and native bind mounts; never on build host"]
async fn native_rootfs_retention_crosses_readonly_image_and_writable_config_bind_views() {
    use crate::disk::mount::filesystem::ReadWrite;
    use crate::disk::mount::filesystem::bind::Bind;
    let root = PathBuf::from(std::env::var_os("FDE_ROOT_VIEW_TEST_ROOT").unwrap());
    assert_eq!(
        nix::sys::statfs::statfs(&root).unwrap().filesystem_type(),
        nix::sys::statfs::BTRFS_SUPER_MAGIC
    );
    let images = root.join("images");
    let config = root.join("config");
    let raw = root.join("raw");
    for path in [&images, &config, &raw] {
        tokio::fs::create_dir(path).await.unwrap();
    }
    let source = images.join("source.rootfs");
    let bytes = vec![42; VERITY_BLOCK_SIZE as usize * 16];
    tokio::fs::write(&source, &bytes).await.unwrap();
    let image_view = MountGuard::mount(&Bind::new(&images), root.join("images-view"), ReadOnly)
        .await
        .unwrap()
        .with_strict_unmount();
    let config_view = MountGuard::mount(&Bind::new(&config), root.join("config-view"), ReadWrite)
        .await
        .unwrap()
        .with_strict_unmount();
    assert_eq!(
        tokio::fs::hard_link(
            image_view.path().join("source.rootfs"),
            config_view.path().join("hardlink")
        )
        .await
        .unwrap_err()
        .raw_os_error(),
        Some(libc::EXDEV)
    );
    let key = generate_signing_key().unwrap();
    let prepared = PreparedGeneration::import_raw(
        &raw,
        &image_view.path().join("source.rootfs"),
        config_view.path(),
        &key,
    )
    .await
    .unwrap();
    let published = prepared.publish().await.unwrap();
    image_view.unmount(false).await.unwrap();
    tokio::fs::remove_file(&source).await.unwrap();
    assert_eq!(
        tokio::fs::read(published.join(ROOTFS_FILE)).await.unwrap(),
        bytes
    );
    config_view.unmount(false).await.unwrap();
}
