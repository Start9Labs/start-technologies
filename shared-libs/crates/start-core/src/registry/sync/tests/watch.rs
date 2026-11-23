use std::time::Duration;

use futures::StreamExt;
use patch_db::json_ptr::JsonPointer;

use super::*;

async fn watched_db() -> TypedPatchDb<Database> {
    let mut model = database();
    publish_snapshot(
        &mut model,
        &Subscription::Index(url()),
        &to_value(&index()).unwrap(),
        &device(),
    )
    .unwrap();
    let db = TypedPatchDb::<Database>::load_unchecked(PatchDb::in_memory());
    db.put(&ROOT, model.as_value()).await.unwrap();
    db
}

fn pointer(segments: &[&str]) -> JsonPointer {
    let mut pointer = JsonPointer::default();
    for segment in segments {
        pointer.push_end(segment);
    }
    pointer
}

fn release_notes(details: &GetPackageResponse, version: &str) -> Value {
    let version: VersionString = version.parse().unwrap();
    to_value(&details.other_versions.as_ref().unwrap()[&version].release_notes).unwrap()
}

#[tokio::test]
async fn watched_details_follow_index_changes_and_skip_unrelated_writes() {
    let db = watched_db().await;
    let (current, changes) =
        watch_synced_package(db.clone(), url(), "test".parse().unwrap(), None, device())
            .await
            .unwrap();
    let mut changes = Box::pin(changes);
    assert!(current.best.contains_key(&"2.0.0:0".parse().unwrap()));
    assert_eq!(release_notes(&current, "1.0.0:0"), json!("First"));

    db.put(&pointer(&["public", "serverInfo", "hostname"]), &"renamed")
        .await
        .unwrap();
    db.put(
        &pointer(&[
            "private",
            "registryIndexes",
            url().as_str(),
            "package",
            "packages",
            "test",
            "versions",
            "1.0.0:0",
            "releaseNotes",
        ]),
        &"Edited",
    )
    .await
    .unwrap();

    let next = tokio::time::timeout(Duration::from_secs(5), changes.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(release_notes(&next, "1.0.0:0"), json!("Edited"));
}

#[tokio::test]
async fn watched_details_end_with_an_error_when_the_registry_is_removed() {
    let db = watched_db().await;
    let (_, changes) =
        watch_synced_package(db.clone(), url(), "test".parse().unwrap(), None, device())
            .await
            .unwrap();
    let mut changes = Box::pin(changes);
    db.mutate(|db| {
        db.as_public_mut()
            .as_server_info_mut()
            .as_registries_mut()
            .ser(&Default::default())?;
        db.as_private_mut()
            .as_registry_indexes_mut()
            .remove(&url())
            .map(|_| ())
    })
    .await
    .result
    .unwrap();
    let ended = tokio::time::timeout(Duration::from_secs(5), changes.next())
        .await
        .unwrap()
        .unwrap();
    assert!(ended.is_err());
    assert!(
        tokio::time::timeout(Duration::from_secs(1), changes.next())
            .await
            .unwrap()
            .is_none()
    );
}
