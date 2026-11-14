use super::*;

#[tokio::test]
async fn background_acquisition_skips_busy_services_without_blocking_readers() {
    let services = ServiceMap::default();
    let id = "backup-in-progress".parse().unwrap();
    let backup = services.get(&id).await;
    assert!(services.try_get_mut(&id).is_none());
    let reader = tokio::time::timeout(Duration::from_secs(1), services.get(&id))
        .await
        .expect("background acquisition queued a writer ahead of this reader");
    drop(reader);
    drop(backup);
    assert!(services.try_get_mut(&id).is_some());
}
