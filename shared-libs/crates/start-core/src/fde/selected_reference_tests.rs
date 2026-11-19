use super::*;

#[tokio::test]
async fn selection_requires_a_regular_file_on_an_epoch_filesystem() {
    let root = tempfile::tempdir().unwrap();
    let image = root.path().join("selected.private");
    tokio::fs::write(&image, b"metadata-only fixture")
        .await
        .unwrap();
    let mut epoch: ReadEpoch = serde_json::from_value(serde_json::json!({
        "baseRoot": "/base", "dataRoot": "/data", "networkProfiles": "/profiles",
        "source": {"kind": "legacy", "path": "/working"},
        "frozenFilesystems": [root.path()], "readonlyFilesystems": []
    }))
    .unwrap();
    assert!(epoch.selected_image().await.is_err());
    epoch.selected_image = Some(image.clone());
    assert_eq!(epoch.selected_image().await.unwrap(), image);
    epoch.frozen_filesystems = vec!["/proc".into()];
    assert!(epoch.selected_image().await.is_err());
    epoch.frozen_filesystems = vec![root.path().to_owned()];
    tokio::fs::remove_file(&image).await.unwrap();
    tokio::fs::symlink(root.path().join("missing"), &image)
        .await
        .unwrap();
    assert!(epoch.selected_image().await.is_err());
}
