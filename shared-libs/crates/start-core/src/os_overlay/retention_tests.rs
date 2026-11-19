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
        let image = generation.join(OVERLAY_FILE);
        tokio::fs::write(&image, b"metadata input, not authenticated boot")
            .await
            .unwrap();
        expected.push(image);
    }
    expected.sort();
    assert_eq!(
        published_overlays(directory.path()).await.unwrap(),
        expected
    );
}

#[tokio::test]
async fn published_reference_inputs_reject_unknown_redirected_and_incomplete_roots() {
    let directory = tempfile::tempdir().unwrap();
    let generation = directory
        .path()
        .join(format!("{GENERATION_PREFIX}{}", Guid::new()));
    tokio::fs::create_dir(&generation).await.unwrap();
    assert!(published_overlays(directory.path()).await.is_err());
    let image = generation.join(OVERLAY_FILE);
    tokio::fs::write(&image, b"preserved").await.unwrap();
    let outside = tempfile::tempdir().unwrap();
    let unknown = directory.path().join("unknown");
    tokio::fs::create_dir(&unknown).await.unwrap();
    assert!(published_overlays(directory.path()).await.is_err());
    tokio::fs::remove_dir(&unknown).await.unwrap();
    let staging = directory
        .path()
        .join(format!("{STAGING_PREFIX}{}", Guid::new()));
    tokio::fs::create_dir(&staging).await.unwrap();
    assert!(published_overlays(directory.path()).await.is_err());
    tokio::fs::remove_dir(staging).await.unwrap();
    let redirected = directory
        .path()
        .join(format!("{GENERATION_PREFIX}{}", Guid::new()));
    tokio::fs::symlink(outside.path(), &redirected)
        .await
        .unwrap();
    assert!(published_overlays(directory.path()).await.is_err());
    tokio::fs::remove_file(redirected).await.unwrap();
    tokio::fs::rename(&image, generation.join("saved"))
        .await
        .unwrap();
    tokio::fs::symlink("saved", &image).await.unwrap();
    assert!(published_overlays(directory.path()).await.is_err());
    assert_eq!(
        tokio::fs::read(generation.join("saved")).await.unwrap(),
        b"preserved"
    );
}

#[tokio::test]
async fn published_reference_inputs_distinguish_empty_and_uninspectable_stores() {
    let directory = tempfile::tempdir().unwrap();
    assert!(
        published_overlays(directory.path())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        published_overlays(&directory.path().join("absent"))
            .await
            .is_err()
    );
    let alias = directory.path().join("alias");
    tokio::fs::symlink(directory.path(), &alias).await.unwrap();
    assert!(published_overlays(&alias).await.is_err());
}
