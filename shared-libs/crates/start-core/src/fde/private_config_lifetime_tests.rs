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
