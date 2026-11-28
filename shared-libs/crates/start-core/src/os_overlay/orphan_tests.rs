use std::os::unix::fs::symlink;

use super::*;

#[tokio::test]
async fn orphan_recovery_preserves_published_and_unrecognized_entries_and_private_backings() {
    let directory = tempfile::tempdir().unwrap();
    let store = directory.path().join("store");
    let raw = directory.path().join("raw");
    let rootfs = directory.path().join("rootfs");
    let private = directory.path().join("private-backing");
    for path in [&store, &raw, &private] {
        tokio::fs::create_dir(path).await.unwrap();
    }
    tokio::fs::write(raw.join("config"), b"configuration")
        .await
        .unwrap();
    tokio::fs::write(private.join("keep"), b"private backing")
        .await
        .unwrap();
    tokio::fs::write(&rootfs, vec![42; VERITY_BLOCK_SIZE as usize])
        .await
        .unwrap();
    let key = generate_signing_key().unwrap();
    let abandoned = PreparedGeneration::import_raw(&raw, &rootfs, &store, &key)
        .await
        .unwrap();
    let staging = abandoned.staging_path().to_owned();
    symlink(&private, staging.join("outside-reference")).unwrap();
    drop(abandoned);
    let published = PreparedGeneration::import_raw(&raw, &rootfs, &store, &key)
        .await
        .unwrap()
        .publish()
        .await
        .unwrap();
    let unknown = store.join(".staging-unrecognized");
    tokio::fs::create_dir(&unknown).await.unwrap();
    tokio::fs::write(unknown.join("keep"), b"unrecognized")
        .await
        .unwrap();
    let _lease = crate::util::FileLock::new(directory.path().join("images.lock"), false)
        .await
        .unwrap();
    recover_unpublished(&store).await.unwrap();
    assert!(!staging.exists());
    assert!(published.join(OVERLAY_FILE).is_file());
    assert_eq!(
        tokio::fs::read(private.join("keep")).await.unwrap(),
        b"private backing"
    );
    assert_eq!(
        tokio::fs::read(unknown.join("keep")).await.unwrap(),
        b"unrecognized"
    );
    recover_unpublished(&store).await.unwrap();
}

#[tokio::test]
async fn recovery_admits_every_staging_leaf_before_deleting_any_candidate() {
    let directory = tempfile::tempdir().unwrap();
    let store = directory.path().join("store");
    tokio::fs::create_dir(&store).await.unwrap();
    let abandoned = store.join(format!("{STAGING_PREFIX}{}", Guid::new()));
    tokio::fs::create_dir(&abandoned).await.unwrap();
    tokio::fs::write(abandoned.join("keep"), b"candidate")
        .await
        .unwrap();
    let outside = directory.path().join("outside");
    tokio::fs::create_dir(&outside).await.unwrap();
    tokio::fs::write(outside.join("keep"), b"outside")
        .await
        .unwrap();
    let invalid = store.join(format!("{STAGING_PREFIX}{}", Guid::new()));
    symlink(&outside, &invalid).unwrap();
    assert!(recover_unpublished(&store).await.is_err());
    assert_eq!(
        tokio::fs::read(abandoned.join("keep")).await.unwrap(),
        b"candidate"
    );
    assert_eq!(
        tokio::fs::read(outside.join("keep")).await.unwrap(),
        b"outside"
    );
    tokio::fs::remove_file(&invalid).await.unwrap();
    tokio::fs::write(&invalid, b"not a directory")
        .await
        .unwrap();
    assert!(recover_unpublished(&store).await.is_err());
    assert!(abandoned.join("keep").is_file());
    assert_eq!(tokio::fs::read(invalid).await.unwrap(), b"not a directory");
}

#[tokio::test]
async fn recovery_preserves_absent_and_redirected_store_roots() {
    let directory = tempfile::tempdir().unwrap();
    let store = directory.path().join("absent");
    recover_unpublished(&store).await.unwrap();
    assert!(!store.exists());
    let outside = directory.path().join("outside");
    tokio::fs::create_dir(&outside).await.unwrap();
    let candidate = outside.join(format!("{STAGING_PREFIX}{}", Guid::new()));
    tokio::fs::create_dir(&candidate).await.unwrap();
    symlink(&outside, &store).unwrap();
    assert!(recover_unpublished(&store).await.is_err());
    assert!(candidate.is_dir());
}

#[tokio::test]
#[ignore = "requires exclusive managed guest and native mount privileges"]
async fn native_orphan_recovery_rejects_mounted_and_uninspectable_candidates() {
    assert_eq!(
        std::env::var("STARTOS_TEST_MANAGED_GUEST").as_deref(),
        Ok("1")
    );
    use std::os::unix::ffi::OsStringExt;

    use crate::disk::mount::filesystem::ReadWrite;
    use crate::disk::mount::filesystem::private_tmpfs::PrivateTmpfs;
    use crate::disk::mount::guard::MountGuard;

    let directory = tempfile::tempdir().unwrap();
    let store = directory.path().join("store");
    tokio::fs::create_dir(&store).await.unwrap();
    let first = store.join(format!("{STAGING_PREFIX}{}", Guid::new()));
    let second = store.join(format!("{STAGING_PREFIX}{}", Guid::new()));
    for path in [&first, &second] {
        tokio::fs::create_dir(path).await.unwrap();
        tokio::fs::write(path.join("candidate"), b"candidate")
            .await
            .unwrap();
    }
    let published = store.join(format!("{GENERATION_PREFIX}{}", Guid::new()));
    let published_mount = MountGuard::mount(&PrivateTmpfs::new(), &published, ReadWrite)
        .await
        .unwrap()
        .with_strict_unmount();
    tokio::fs::write(published.join("keep"), b"published mount")
        .await
        .unwrap();
    let target = second.join("mounted child\nwith space");
    let mounted = MountGuard::mount(&PrivateTmpfs::new(), &target, ReadWrite)
        .await
        .unwrap()
        .with_strict_unmount();
    tokio::fs::write(target.join("keep"), b"mounted witness")
        .await
        .unwrap();
    let _lease = crate::util::FileLock::new(directory.path().join("images.lock"), false)
        .await
        .unwrap();
    let mounts = crate::disk::mount::util::mountpoints_under(&store)
        .await
        .unwrap();
    assert!(mounts.contains(&target));
    assert!(recover_unpublished(&store).await.is_err());
    assert!(first.join("candidate").is_file());
    assert_eq!(
        tokio::fs::read(target.join("keep")).await.unwrap(),
        b"mounted witness"
    );
    crate::disk::mount::util::unmount_all_under(&second, false)
        .await
        .unwrap();
    mounted.unmount(false).await.unwrap();
    let invalid_target = second.join(std::ffi::OsString::from_vec(b"mounted-\xff".to_vec()));
    let invalid_mount = MountGuard::mount(&PrivateTmpfs::new(), &invalid_target, ReadWrite)
        .await
        .unwrap()
        .with_strict_unmount();
    assert!(
        crate::disk::mount::util::mountpoints_under(&store)
            .await
            .is_err()
    );
    assert!(recover_unpublished(&store).await.is_err());
    assert!(first.join("candidate").is_file());
    invalid_mount.unmount(false).await.unwrap();
    recover_unpublished(&store).await.unwrap();
    assert!(!first.exists());
    assert!(!second.exists());
    assert!(
        crate::disk::mount::util::is_mountpoint(&published)
            .await
            .unwrap()
    );
    assert_eq!(
        tokio::fs::read(published.join("keep")).await.unwrap(),
        b"published mount"
    );
    published_mount.unmount(false).await.unwrap();
}
