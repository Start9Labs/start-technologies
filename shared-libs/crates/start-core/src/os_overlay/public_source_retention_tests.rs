use super::*;
use crate::rpc_continuations::Guid;

pub(super) async fn published_rootfs(configuration: &Path, image: &Path) -> PathBuf {
    let store = crate::fde::boot::create_generation_store(configuration).unwrap();
    let generation = store.join(format!("generation-{}", Guid::new()));
    tokio::fs::create_dir(&generation).await.unwrap();
    for name in [
        super::super::super::ROOTFS_FILE,
        super::super::super::OVERLAY_FILE,
    ] {
        crate::util::io::copy_file(image, generation.join(name))
            .await
            .unwrap();
    }
    generation
}

#[tokio::test]
async fn stored_repack_and_published_generation_independently_retain_public_sources() {
    let root = tempfile::tempdir().unwrap();
    let volume = root.path().join("volume");
    let cache = cache(&volume);
    let _lease = volume_lease(&volume).await;
    let key = SigningKey::from_bytes(&[41; 32]);
    let mut sources = std::collections::BTreeMap::new();
    for (commit, compression) in [
        ("running", "gzip"),
        ("pending", "zstd"),
        ("stored", "gzip"),
        ("published", "zstd"),
        ("obsolete", "gzip"),
    ] {
        let image = fixture(root.path(), commit, compression).await;
        let source = cache
            .retain(signed_asset(&image, &key).await, &image)
            .await
            .unwrap();
        sources.insert(commit, source.path);
    }
    tokio::fs::rename(root.path().join("running"), volume.join("current"))
        .await
        .unwrap();
    let pointer = volume.join("config/current.rootfs");
    tokio::fs::symlink(root.path().join("pending.zstd.squashfs"), &pointer)
        .await
        .unwrap();
    let stored = volume.join("images/recovery.rootfs");
    tokio::fs::write(root.path().join("stored/private-config"), b"private repack")
        .await
        .unwrap();
    Command::new("/usr/bin/mksquashfs")
        .arg(root.path().join("stored"))
        .arg(&stored)
        .args(["-noappend", "-comp", "gzip", "-no-progress"])
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    let generation = published_rootfs(&volume.join("config"), &sources["published"]).await;
    assert!(
        cache
            .retain(signed_asset(&sources["stored"], &key).await, &stored)
            .await
            .is_err()
    );
    cache.prune_boot_sources().await.unwrap();
    assert!(!sources["obsolete"].exists());
    for commit in ["running", "pending", "stored", "published"] {
        assert!(sources[commit].is_file(), "{commit}");
        assert!(
            cache
                .find_approved(&release(commit), policy(&key))
                .await
                .unwrap()
                .is_some()
        );
    }
    assert!(
        cache
            .find_approved(
                &release("stored"),
                policy(&SigningKey::from_bytes(&[42; 32]))
            )
            .await
            .unwrap()
            .is_none()
    );
    tokio::fs::remove_file(&pointer).await.unwrap();
    tokio::fs::symlink(root.path().join("running.gzip.squashfs"), &pointer)
        .await
        .unwrap();
    cache.prune_boot_sources().await.unwrap();
    assert!(!sources["pending"].exists());
    assert!(sources["stored"].is_file());
    assert!(sources["published"].is_file());
    tokio::fs::remove_file(stored).await.unwrap();
    cache.prune_boot_sources().await.unwrap();
    assert!(!sources["stored"].exists());
    assert!(sources["published"].is_file());
    crate::util::io::delete_dir(generation).await.unwrap();
    cache.prune_boot_sources().await.unwrap();
    assert!(!sources["published"].exists());
    assert!(sources["running"].is_file());
}

#[tokio::test]
async fn invalid_stored_or_published_inputs_preserve_every_cached_source() {
    let root = tempfile::tempdir().unwrap();
    let volume = root.path().join("volume");
    let cache = cache(&volume);
    let _lease = volume_lease(&volume).await;
    let key = SigningKey::from_bytes(&[43; 32]);
    let image = fixture(root.path(), "running", "gzip").await;
    let unused = fixture(root.path(), "unused", "zstd").await;
    let retained = cache
        .retain(signed_asset(&unused, &key).await, &unused)
        .await
        .unwrap();
    tokio::fs::rename(root.path().join("running"), volume.join("current"))
        .await
        .unwrap();
    tokio::fs::symlink(&image, volume.join("config/current.rootfs"))
        .await
        .unwrap();
    let bad = volume.join("images/bad.rootfs");
    tokio::fs::write(&bad, b"not SquashFS").await.unwrap();
    assert!(cache.prune_boot_sources().await.is_err());
    assert!(retained.path.is_file());
    tokio::fs::remove_file(&bad).await.unwrap();
    tokio::fs::symlink(&image, &bad).await.unwrap();
    assert!(cache.prune_boot_sources().await.is_err());
    assert!(retained.path.is_file());
    tokio::fs::remove_file(&bad).await.unwrap();
    let namespace = volume.join("config/fde");
    tokio::fs::symlink(root.path().join("absent"), &namespace)
        .await
        .unwrap();
    assert!(cache.prune_boot_sources().await.is_err());
    assert!(retained.path.is_file());
    tokio::fs::remove_file(&namespace).await.unwrap();
    let generation = published_rootfs(&volume.join("config"), &image).await;
    let rootfs = generation.join(super::super::super::ROOTFS_FILE);
    tokio::fs::remove_file(&rootfs).await.unwrap();
    assert!(cache.prune_boot_sources().await.is_err());
    assert!(retained.path.is_file());
    tokio::fs::symlink(&image, &rootfs).await.unwrap();
    assert!(cache.prune_boot_sources().await.is_err());
    assert!(retained.path.is_file());
    tokio::fs::remove_file(&rootfs).await.unwrap();
    tokio::fs::write(&rootfs, b"not SquashFS").await.unwrap();
    assert!(cache.prune_boot_sources().await.is_err());
    assert!(retained.path.is_file());
    crate::util::io::delete_dir(generation).await.unwrap();
    let unknown = volume.join("config/fde/generations/unknown");
    tokio::fs::create_dir(&unknown).await.unwrap();
    assert!(cache.prune_boot_sources().await.is_err());
    assert!(retained.path.is_file());
    tokio::fs::remove_dir(&unknown).await.unwrap();
    let staging = volume.join(format!("config/fde/generations/.staging-{}", Guid::new()));
    tokio::fs::write(&staging, b"wrong type").await.unwrap();
    assert!(cache.prune_boot_sources().await.is_err());
    assert!(retained.path.is_file());
    tokio::fs::remove_file(staging).await.unwrap();
    cache.prune_boot_sources().await.unwrap();
    assert!(!retained.path.exists());
}

#[tokio::test]
async fn active_unpublished_preparation_neither_blocks_pruning_nor_filters_published_inputs() {
    let root = tempfile::tempdir().unwrap();
    let volume = root.path().join("volume");
    let cache = cache(&volume);
    let _lease = volume_lease(&volume).await;
    let key = SigningKey::from_bytes(&[44; 32]);
    let running = fixture(root.path(), "running", "gzip").await;
    let image = fixture(root.path(), "published", "zstd").await;
    let retained = cache
        .retain(signed_asset(&image, &key).await, &image)
        .await
        .unwrap();
    tokio::fs::rename(root.path().join("running"), volume.join("current"))
        .await
        .unwrap();
    tokio::fs::symlink(&running, volume.join("config/current.rootfs"))
        .await
        .unwrap();
    let configuration = volume.join("config");
    let generation = published_rootfs(&configuration, &image).await;
    let store = crate::fde::boot::generation_store(&configuration);
    let staging = store.join(format!(".staging-{}", Guid::new()));
    tokio::fs::create_dir(&staging).await.unwrap();
    let observed = super::super::super::published_images(&store).await.unwrap();
    assert_eq!(observed.len(), 2);
    assert!(observed.iter().all(|image| image.starts_with(&generation)));
    cache.prune_boot_sources().await.unwrap();
    assert!(retained.path.is_file());
    assert!(staging.is_dir());
}

#[tokio::test]
#[ignore = "requires the actual built startbox in FDE_SOURCE_GC_STARTBOX"]
async fn shell_retires_public_alias_before_rechecking_capacity_after_image_deletion() {
    let binary = PathBuf::from(std::env::var_os("FDE_SOURCE_GC_STARTBOX").unwrap());
    let root = tempfile::tempdir().unwrap();
    let volume = root.path().join("volume");
    let cache = cache(&volume);
    let lease = volume_lease(&volume).await;
    let key = SigningKey::from_bytes(&[45; 32]);
    let running = fixture(root.path(), "running", "gzip").await;
    let old = fixture(root.path(), "old", "zstd").await;
    let retained = cache
        .retain(signed_asset(&old, &key).await, &old)
        .await
        .unwrap();
    tokio::fs::rename(root.path().join("running"), volume.join("current"))
        .await
        .unwrap();
    let selected = volume.join("images/current.rootfs");
    let unselected = volume.join("images/old.rootfs");
    tokio::fs::hard_link(&running, &selected).await.unwrap();
    tokio::fs::hard_link(&old, &unselected).await.unwrap();
    tokio::fs::remove_file(running).await.unwrap();
    tokio::fs::remove_file(old).await.unwrap();
    tokio::fs::symlink(&selected, volume.join("config/current.rootfs"))
        .await
        .unwrap();
    let (mut command, _executable) = shell_command(
        &lease,
        &volume,
        &binary,
        r#"source "$1/os-image-store"
        df() { if [[ -e "$FDE_TEST_CAPACITY_SOURCE" ]]; then printf '0\n'; else printf '1\n'; fi; }
        startos_prune_images "$2" 1 0"#,
    )
    .await
    .unwrap();
    command.env("FDE_TEST_CAPACITY_SOURCE", &retained.path);
    command.invoke(ErrorKind::Filesystem).await.unwrap();
    assert!(!unselected.exists());
    assert!(!retained.path.exists());
    assert!(selected.is_file());
}
