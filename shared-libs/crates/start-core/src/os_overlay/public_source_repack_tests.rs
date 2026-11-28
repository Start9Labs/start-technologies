use super::*;

#[tokio::test]
async fn pending_private_repack_contents_retain_every_public_source_until_retirement() {
    let root = tempfile::tempdir().unwrap();
    let volume = root.path().join("volume");
    let cache = cache(&volume);
    let _lease = volume_lease(&volume).await;
    let key = SigningKey::from_bytes(&[47; 32]);
    let running = fixture(root.path(), "running", "gzip").await;
    let unused = fixture(root.path(), "unselected", "zstd").await;
    let running_source = cache
        .retain(signed_asset(&running, &key).await, &running)
        .await
        .unwrap();
    tokio::fs::rename(root.path().join("running"), volume.join("current"))
        .await
        .unwrap();
    tokio::fs::symlink(&running, volume.join("config/current.rootfs"))
        .await
        .unwrap();
    for name in ["next", "upper"] {
        tokio::fs::create_dir(volume.join(name)).await.unwrap();
    }
    let obsolete = cache
        .retain(signed_asset(&unused, &key).await, &unused)
        .await
        .unwrap();
    cache.prune_boot_sources().await.unwrap();
    assert!(!obsolete.path.exists());
    for name in ["next", "upper"] {
        let contents = volume.join(name).join("private-unpublished-input");
        nix::unistd::mkfifo(&contents, nix::sys::stat::Mode::S_IRUSR).unwrap();
        let retained = cache
            .retain(signed_asset(&unused, &key).await, &unused)
            .await
            .unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            cache.prune_boot_sources(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(retained.path.is_file());
        assert!(running_source.path.is_file());
        assert!(
            cache
                .find_approved(
                    &release("unselected"),
                    policy(&SigningKey::from_bytes(&[48; 32]))
                )
                .await
                .unwrap()
                .is_none()
        );
        tokio::fs::remove_file(contents).await.unwrap();
        cache.prune_boot_sources().await.unwrap();
        assert!(!retained.path.exists());
        assert!(running_source.path.is_file());
    }
}
