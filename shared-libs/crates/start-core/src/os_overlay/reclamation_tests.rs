use super::*;

#[tokio::test]
async fn failed_generation_construction_reclaims_only_its_unpublished_directory() {
    let root = tempfile::tempdir().unwrap();
    let raw = root.path().join("raw");
    let store = root.path().join("store");
    let rootfs = root.path().join("rootfs");
    tokio::fs::create_dir(&raw).await.unwrap();
    tokio::fs::create_dir(&store).await.unwrap();
    tokio::fs::write(raw.join("config"), b"public configuration")
        .await
        .unwrap();
    tokio::fs::write(&rootfs, b"unaligned").await.unwrap();
    let retained = store.join("unrelated");
    tokio::fs::create_dir(&retained).await.unwrap();
    tokio::fs::write(retained.join("keep"), b"retained")
        .await
        .unwrap();
    let key = generate_signing_key().unwrap();
    let result = PreparedGeneration::import_raw(&raw, &rootfs, &store, &key).await;
    assert!(result.is_err());
    assert_eq!(
        tokio::fs::read_dir(&store)
            .await
            .unwrap()
            .next_entry()
            .await
            .unwrap()
            .unwrap()
            .path(),
        retained
    );
    let mut entries = tokio::fs::read_dir(&store).await.unwrap();
    assert!(entries.next_entry().await.unwrap().is_some());
    assert!(entries.next_entry().await.unwrap().is_none());
    assert_eq!(
        tokio::fs::read(retained.join("keep")).await.unwrap(),
        b"retained"
    );
}

#[tokio::test]
async fn explicit_discard_reclaims_staging_without_pruning_published_generations() {
    let root = tempfile::tempdir().unwrap();
    let raw = root.path().join("raw");
    let store = root.path().join("store");
    let rootfs = root.path().join("rootfs");
    tokio::fs::create_dir(&raw).await.unwrap();
    tokio::fs::create_dir(&store).await.unwrap();
    tokio::fs::write(raw.join("config"), b"public configuration")
        .await
        .unwrap();
    tokio::fs::write(&rootfs, vec![42; VERITY_BLOCK_SIZE as usize])
        .await
        .unwrap();
    let key = generate_signing_key().unwrap();
    let public = PKey::public_key_from_der(&key.public_key_to_der().unwrap()).unwrap();
    let published = PreparedGeneration::import_raw(&raw, &rootfs, &store, &key)
        .await
        .unwrap()
        .publish()
        .await
        .unwrap();
    let prepared = PreparedGeneration::import_raw(&raw, &rootfs, &store, &key)
        .await
        .unwrap();
    let staging = prepared.staging_path().to_owned();
    prepared.discard().await.unwrap();
    assert!(!staging.exists());
    assert_eq!(
        select_latest(&store, &rootfs, &public)
            .await
            .unwrap()
            .unwrap()
            .path,
        published
    );
}
