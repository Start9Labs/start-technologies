use super::*;

#[tokio::test]
async fn recovery_discovery_reads_partition_roots_not_nested_backup_folders() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let nested = root.join("nested");
    let metadata = BackupUnencryptedMetadata {
        hostname: ServerHostname::new("backup-fixture".into()).unwrap(),
        version: "0.4.2".parse().unwrap(),
        timestamp: Utc::now(),
        password_hash: Some("private-hash".into()),
        wrapped_key: Some("private-key".into()),
    };
    for (base, server) in [(root, "root-server"), (nested.as_path(), "nested-server")] {
        let path = BackupUnencryptedMetadata::path(base, server);
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(path, serde_json::to_vec(&metadata).unwrap())
            .await
            .unwrap();
    }
    let found = recovery_info(root).await.unwrap();
    assert_eq!(
        found.keys().map(String::as_str).collect::<Vec<_>>(),
        ["root-server"]
    );
    let public = serde_json::to_string(&found).unwrap();
    assert!(!public.contains("private-hash"));
    assert!(!public.contains("private-key"));
    tokio::fs::remove_dir_all(root.join(crate::disk::BACKUP_DIR_NAME))
        .await
        .unwrap();
    assert!(recovery_info(root).await.unwrap().is_empty());
}
