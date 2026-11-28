use super::*;

#[tokio::test]
async fn reused_backings_admit_concurrent_projections_and_live_credential_writes() {
    if crate::util::file_lock_tests::isolated_test("fde::private_config::tests::lifetimes::reused_backings_admit_concurrent_projections_and_live_credential_writes").await {
        return;
    }
    let (_root, raw, data) = fixture().await;
    let initial = project(&raw, &data, &[]).await;
    let backing = initial.new_backing.clone().unwrap();
    let credential = backing_file(&data, target(&initial, "etc/shadow"))
        .await
        .unwrap();
    let resumed = ProjectedConfig::stage(initial.snapshot.clone(), &data, &[])
        .await
        .unwrap();
    assert!(resumed.new_backing.is_none());
    assert!(
        reserve_backing(&backing, FileLockAccess::Exclusive)
            .await
            .is_err()
    );
    tokio::fs::write(&credential, b"changed-while-projected")
        .await
        .unwrap();
    assert_eq!(
        tokio::fs::read(&credential).await.unwrap(),
        b"changed-while-projected"
    );
    drop(initial);
    assert!(
        reserve_backing(&backing, FileLockAccess::Exclusive)
            .await
            .is_err()
    );
    let snapshot = resumed.snapshot.clone();
    resumed.discard().await.unwrap();
    let retirement = reserve_backing(&backing, FileLockAccess::Exclusive)
        .await
        .unwrap();
    assert!(
        ProjectedConfig::stage(snapshot.clone(), &data, &[])
            .await
            .is_err()
    );
    drop(retirement);
    ProjectedConfig::stage(snapshot, &data, &[]).await.unwrap();
}

#[tokio::test]
async fn prepared_generation_retains_backing_uses_after_projection_drop() {
    if crate::util::file_lock_tests::isolated_test("fde::private_config::tests::lifetimes::prepared_generation_retains_backing_uses_after_projection_drop").await {
        return;
    }
    let (root, raw, data) = fixture().await;
    let projected = project(&raw, &data, &[]).await;
    let backing = projected.new_backing.clone().unwrap();
    let prepared = reclamation::prepare(&projected, root.path()).await;
    drop(projected);
    assert!(
        reserve_backing(&backing, FileLockAccess::Exclusive)
            .await
            .is_err()
    );
    prepared.discard().await.unwrap();
    reserve_backing(&backing, FileLockAccess::Exclusive)
        .await
        .unwrap();
}

#[tokio::test]
async fn pending_legacy_replacement_retains_fresh_backing_during_disposal() {
    if crate::util::file_lock_tests::isolated_test("fde::private_config::tests::lifetimes::pending_legacy_replacement_retains_fresh_backing_during_disposal").await {
        return;
    }
    let (_root, raw, data) = fixture().await;
    let projected = project(&raw, &data, &[]).await;
    let backing = projected.new_backing.clone().unwrap();
    let credential = backing_file(&data, target(&projected, "etc/shadow"))
        .await
        .unwrap();
    let legacy = projected.legacy_replacements();
    assert!(projected.discard().await.is_err());
    assert_eq!(
        tokio::fs::read(credential).await.unwrap(),
        b"private-shadow"
    );
    assert!(
        reserve_backing(&backing, FileLockAccess::Exclusive)
            .await
            .is_err()
    );
    drop(legacy);
    reserve_backing(&backing, FileLockAccess::Exclusive)
        .await
        .unwrap();
}

#[tokio::test]
async fn whole_private_ancestor_uses_include_unreferenced_backings_and_keep_writes_live() {
    if crate::util::file_lock_tests::isolated_test("fde::private_config::tests::lifetimes::whole_private_ancestor_uses_include_unreferenced_backings_and_keep_writes_live").await {
        return;
    }
    let (_root, raw, data) = fixture().await;
    let projected = project(&raw, &data, &[]).await;
    let backing = projected.new_backing.clone().unwrap();
    let orphan = data.join(DIRECTORY).join(Guid::new().to_string());
    tokio::fs::create_dir(&orphan).await.unwrap();
    let credential = orphan.join("not-in-the-snapshot");
    tokio::fs::write(&credential, b"before query")
        .await
        .unwrap();
    let uses = reserve_visible_backings(&data).await.unwrap();
    assert_eq!(uses.len(), 2);
    assert!(projected.discard().await.is_err());
    assert!(backing.is_dir());
    assert!(
        reserve_backing(&orphan, FileLockAccess::Exclusive)
            .await
            .is_err()
    );
    tokio::fs::write(&credential, b"live credential write")
        .await
        .unwrap();
    assert_eq!(
        tokio::fs::read(credential).await.unwrap(),
        b"live credential write"
    );
    drop(uses);
    reserve_backing(&backing, FileLockAccess::Exclusive)
        .await
        .unwrap();
    reserve_backing(&orphan, FileLockAccess::Exclusive)
        .await
        .unwrap();
}

#[tokio::test]
async fn visible_backing_uses_reject_unknown_redirected_and_special_entries_without_leaks() {
    if crate::util::file_lock_tests::isolated_test("fde::private_config::tests::lifetimes::visible_backing_uses_reject_unknown_redirected_and_special_entries_without_leaks").await {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let data = root.path();
    assert!(reserve_visible_backings(data).await.unwrap().is_empty());
    let namespace = data.join(DIRECTORY);
    tokio::fs::create_dir(&namespace).await.unwrap();
    assert!(reserve_visible_backings(data).await.unwrap().is_empty());
    let valid = namespace.join(Guid::new().to_string());
    tokio::fs::create_dir(&valid).await.unwrap();
    let unknown = namespace.join("not-a-guid");
    tokio::fs::create_dir(&unknown).await.unwrap();
    assert!(reserve_visible_backings(data).await.is_err());
    tokio::fs::remove_dir(unknown).await.unwrap();
    let bad = namespace.join(Guid::new().to_string());
    for kind in ["symlink", "file", "fifo"] {
        match kind {
            "symlink" => tokio::fs::symlink(&valid, &bad).await.unwrap(),
            "file" => tokio::fs::write(&bad, b"not a directory").await.unwrap(),
            _ => nix::unistd::mkfifo(&bad, nix::sys::stat::Mode::S_IRUSR).unwrap(),
        }
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            assert!(reserve_visible_backings(data).await.is_err());
            reserve_backing(&valid, FileLockAccess::Exclusive)
                .await
                .unwrap();
        })
        .await
        .unwrap();
        tokio::fs::remove_file(&bad).await.unwrap();
    }
    tokio::fs::remove_dir(&valid).await.unwrap();
    tokio::fs::remove_dir(&namespace).await.unwrap();
    tokio::fs::symlink(data, &namespace).await.unwrap();
    assert!(reserve_visible_backings(data).await.is_err());
}
