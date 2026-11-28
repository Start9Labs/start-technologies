use super::*;

#[path = "image_store_repack_tests.rs"]
mod repack;

#[tokio::test]
async fn native_namespace_and_oldest_selection_preserve_byte_paths() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("images [one]");
    std::fs::create_dir(&directory).unwrap();
    assert!(image_paths(&directory).await.unwrap().is_empty());
    let oldest = directory.join(std::ffi::OsString::from_vec(
        b"old\ncurrent-\xff.rootfs".to_vec(),
    ));
    let current = directory.join(std::ffi::OsString::from_vec(
        b"current\nimage-\xfe.rootfs".to_vec(),
    ));
    let newest = directory.join("download.squashfs");
    for (index, path) in [&oldest, &current, &newest].into_iter().enumerate() {
        let file = std::fs::File::create(path).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(100 + index as u64),
        ))
        .unwrap();
    }
    std::fs::create_dir(directory.join("fde-sources")).unwrap();
    std::fs::write(directory.join("ignored.raw"), b"outside native namespace").unwrap();
    assert_eq!(
        image_paths(&directory).await.unwrap(),
        [newest, current.clone(), oldest.clone()]
    );
    let bytes = Command::new("/bin/bash")
        .args([
            "-euc",
            &format!("{SCRIPT}\nstartos_oldest_unselected_image \"$1\" \"$2\""),
            "oldest-image",
        ])
        .arg(&directory)
        .arg(&current)
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    assert_eq!(std::ffi::OsString::from_vec(bytes), oldest.into_os_string());
}

#[tokio::test]
async fn native_namespace_rejects_uninspectable_and_nonregular_inputs() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("images");
    assert!(image_paths(&directory).await.is_err());
    assert!(
        Command::new("/bin/bash")
            .args([
                "-euc",
                &format!("{SCRIPT}\nstartos_oldest_unselected_image \"$1\" \"$2\""),
                "oldest-image-error"
            ])
            .arg(&directory)
            .arg("/unselected")
            .invoke(ErrorKind::Filesystem)
            .await
            .is_err()
    );
    std::fs::create_dir(&directory).unwrap();
    let alias = root.path().join("alias");
    std::os::unix::fs::symlink(&directory, &alias).unwrap();
    assert!(image_paths(&alias).await.is_err());
    let candidate = directory.join("invalid.rootfs");
    std::fs::create_dir(&candidate).unwrap();
    assert!(image_paths(&directory).await.is_err());
    std::fs::remove_dir(&candidate).unwrap();
    std::os::unix::fs::symlink(root.path().join("absent"), &candidate).unwrap();
    assert!(image_paths(&directory).await.is_err());
    std::fs::remove_file(&candidate).unwrap();
    nix::unistd::mkfifo(&candidate, nix::sys::stat::Mode::S_IRUSR).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        assert!(image_paths(&directory).await.is_err());
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn selection_repair_preserves_native_bytes_and_independent_existing_target() {
    let root = tempfile::tempdir().unwrap();
    let configuration = root.path().join("config");
    let directory = root.path().join("images");
    std::fs::create_dir(&configuration).unwrap();
    std::fs::create_dir(&directory).unwrap();
    let lease = crate::util::FileLock::new(configuration.join("fixture-lease"), true)
        .await
        .unwrap();
    let image = directory.join(std::ffi::OsString::from_vec(
        b"root\nimage-\xff.rootfs".to_vec(),
    ));
    std::fs::write(&image, b"selection metadata fixture").unwrap();
    std::fs::write(directory.join("newer.squashfs"), b"not legacy fallback").unwrap();
    assert!(selected_image(&configuration).await.is_err());
    repair_selection(&lease, root.path()).await.unwrap();
    assert_eq!(selected_image(&configuration).await.unwrap(), image);
    let target = root.path().join(std::ffi::OsString::from_vec(
        b"outside\nimage-\xfe.private".to_vec(),
    ));
    std::fs::write(&target, b"independent selected fixture").unwrap();
    let pointer = configuration.join("current.rootfs");
    std::fs::remove_file(&pointer).unwrap();
    std::os::unix::fs::symlink(&target, &pointer).unwrap();
    let metadata = std::fs::symlink_metadata(&pointer).unwrap();
    repair_selection(&lease, root.path()).await.unwrap();
    assert_eq!(selected_image(&configuration).await.unwrap(), target);
    use std::os::unix::fs::MetadataExt;
    assert_eq!(
        std::fs::symlink_metadata(pointer).unwrap().ino(),
        metadata.ino()
    );
}

#[tokio::test]
async fn selection_rejects_missing_and_nonregular_targets_without_guessing() {
    let root = tempfile::tempdir().unwrap();
    let configuration = root.path().join("config");
    let images = root.path().join("images");
    std::fs::create_dir(&configuration).unwrap();
    std::fs::create_dir(&images).unwrap();
    std::fs::write(images.join("download.squashfs"), b"not fallback eligible").unwrap();
    let lease = crate::util::FileLock::new(configuration.join("fixture-lease"), true)
        .await
        .unwrap();
    repair_selection(&lease, root.path()).await.unwrap();
    assert!(selected_image(&configuration).await.is_err());
    let pointer = configuration.join("current.rootfs");
    std::fs::write(&pointer, b"not a pointer").unwrap();
    assert!(selected_image(&configuration).await.is_err());
    std::fs::remove_file(&pointer).unwrap();
    let target = root.path().join("target");
    std::os::unix::fs::symlink(&target, &pointer).unwrap();
    assert!(selected_image(&configuration).await.is_err());
    std::fs::create_dir(&target).unwrap();
    assert!(selected_image(&configuration).await.is_err());
    std::fs::remove_dir(&target).unwrap();
    nix::unistd::mkfifo(&target, nix::sys::stat::Mode::S_IRUSR).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        assert!(selected_image(&configuration).await.is_err());
    })
    .await
    .unwrap();
}
