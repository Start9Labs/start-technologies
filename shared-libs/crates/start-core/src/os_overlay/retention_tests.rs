use super::*;

#[tokio::test]
async fn published_reference_inputs_include_every_generation_without_boot_selection() {
    let directory = tempfile::tempdir().unwrap();
    let mut expected = Vec::new();
    for _ in 0..3 {
        let generation = directory
            .path()
            .join(format!("{GENERATION_PREFIX}{}", Guid::new()));
        tokio::fs::create_dir(&generation).await.unwrap();
        for file in [ROOTFS_FILE, OVERLAY_FILE] {
            let image = generation.join(file);
            tokio::fs::write(&image, b"metadata input, not authenticated boot")
                .await
                .unwrap();
            expected.push(image);
        }
    }
    expected.sort();
    assert_eq!(published_images(directory.path()).await.unwrap(), expected);
}

#[tokio::test]
async fn published_reference_inputs_reject_unknown_redirected_and_incomplete_roots() {
    let directory = tempfile::tempdir().unwrap();
    let generation = directory
        .path()
        .join(format!("{GENERATION_PREFIX}{}", Guid::new()));
    tokio::fs::create_dir(&generation).await.unwrap();
    assert!(published_images(directory.path()).await.is_err());
    let image = generation.join(OVERLAY_FILE);
    tokio::fs::write(&image, b"preserved").await.unwrap();
    assert!(published_images(directory.path()).await.is_err());
    let rootfs = generation.join(ROOTFS_FILE);
    tokio::fs::write(&rootfs, b"rootfs retained").await.unwrap();
    let outside = tempfile::tempdir().unwrap();
    let unknown = directory.path().join("unknown");
    tokio::fs::create_dir(&unknown).await.unwrap();
    assert!(published_images(directory.path()).await.is_err());
    tokio::fs::remove_dir(&unknown).await.unwrap();
    let staging = directory
        .path()
        .join(format!("{STAGING_PREFIX}{}", Guid::new()));
    tokio::fs::create_dir(&staging).await.unwrap();
    let inputs = published_images(directory.path()).await.unwrap();
    assert_eq!(inputs.len(), 2);
    assert!(inputs.contains(&image));
    assert!(inputs.contains(&rootfs));
    tokio::fs::remove_dir(&staging).await.unwrap();
    tokio::fs::symlink(outside.path(), &staging).await.unwrap();
    assert!(published_images(directory.path()).await.is_err());
    tokio::fs::remove_file(staging).await.unwrap();
    let redirected = directory
        .path()
        .join(format!("{GENERATION_PREFIX}{}", Guid::new()));
    tokio::fs::symlink(outside.path(), &redirected)
        .await
        .unwrap();
    assert!(published_images(directory.path()).await.is_err());
    tokio::fs::remove_file(redirected).await.unwrap();
    tokio::fs::rename(&rootfs, generation.join("saved-rootfs"))
        .await
        .unwrap();
    tokio::fs::symlink("saved-rootfs", &rootfs).await.unwrap();
    assert!(published_images(directory.path()).await.is_err());
    tokio::fs::remove_file(&rootfs).await.unwrap();
    tokio::fs::rename(generation.join("saved-rootfs"), &rootfs)
        .await
        .unwrap();
    tokio::fs::rename(&image, generation.join("saved"))
        .await
        .unwrap();
    tokio::fs::symlink("saved", &image).await.unwrap();
    assert!(published_images(directory.path()).await.is_err());
    assert_eq!(
        tokio::fs::read(generation.join("saved")).await.unwrap(),
        b"preserved"
    );
}

#[tokio::test]
async fn published_reference_inputs_distinguish_empty_and_uninspectable_stores() {
    let directory = tempfile::tempdir().unwrap();
    assert!(published_images(directory.path()).await.unwrap().is_empty());
    assert!(
        published_images(&directory.path().join("absent"))
            .await
            .is_err()
    );
    let alias = directory.path().join("alias");
    tokio::fs::symlink(directory.path(), &alias).await.unwrap();
    assert!(published_images(&alias).await.is_err());
}
