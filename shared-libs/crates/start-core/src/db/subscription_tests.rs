use super::*;

#[tokio::test]
async fn subscription_emits_sync_fences_before_following_revisions() {
    let (send, sub) = mpsc::unbounded_channel();
    let (sync, sync_db) = watch::channel(0);
    let mut subscriber = DbSubscriber {
        rev: 0,
        sub,
        sync_db,
    };
    sync.send(4).unwrap();
    let fence = subscriber.recv().await.unwrap();
    assert_eq!(fence.id, 4);
    assert_eq!(
        serde_json::to_value(fence.patch).unwrap(),
        serde_json::json!([])
    );
    send.send(Revision {
        id: 6,
        patch: DiffPatch::default(),
    })
    .unwrap();
    assert_eq!(subscriber.recv().await.unwrap().id, 6);
}

#[tokio::test]
async fn closed_sync_owner_does_not_spin_or_discard_remaining_revisions() {
    let (send, sub) = mpsc::unbounded_channel();
    let (sync, sync_db) = watch::channel(0);
    let mut subscriber = DbSubscriber {
        rev: 0,
        sub,
        sync_db,
    };
    drop(sync);
    let mut pending = Box::pin(subscriber.recv());
    assert!(futures::poll!(pending.as_mut()).is_pending());
    send.send(Revision {
        id: 1,
        patch: DiffPatch::default(),
    })
    .unwrap();
    assert_eq!(pending.await.unwrap().id, 1);
    drop(send);
    assert!(subscriber.recv().await.is_none());
}

#[tokio::test]
async fn stream_start_dumps_without_a_cursor_and_replays_with_one() {
    let db = PatchDb::in_memory();
    db.put(
        &ROOT,
        &serde_json::json!({ "public": { "a": 1 }, "private": {} }),
    )
    .await
    .unwrap();
    let (start, _) = subscribe_start(&db, PUBLIC.clone(), Default::default(), None).await;
    assert_eq!(
        serde_json::to_value(&start).unwrap(),
        serde_json::json!({
            "epoch": &*start.epoch,
            "id": 1,
            "value": { "a": 1 },
            "revisions": null,
        })
    );
    db.put(&"/private/b".parse::<JsonPointer>().unwrap(), &1)
        .await
        .unwrap();
    db.put(&"/public/a".parse::<JsonPointer>().unwrap(), &2)
        .await
        .unwrap();
    let cursor = DbCursor {
        epoch: start.epoch.clone(),
        id: start.id,
    };
    let (resumed, _) =
        subscribe_start(&db, PUBLIC.clone(), Default::default(), Some(&cursor)).await;
    assert_eq!(
        serde_json::to_value(&resumed).unwrap(),
        serde_json::json!({
            "epoch": &*start.epoch,
            "id": 3,
            "value": null,
            "revisions": [{
                "id": 3,
                "patch": [{ "op": "replace", "path": "/a", "value": 2 }],
            }],
        })
    );
}

#[tokio::test]
async fn projected_stream_start_dumps_and_replays_only_the_projection() {
    let db = PatchDb::in_memory();
    db.put(
        &ROOT,
        &serde_json::json!({
            "public": { "packageData": { "x": { "state": 1, "icon": "big" } } },
        }),
    )
    .await
    .unwrap();
    let params: SubscribeParams = serde_json::from_value(serde_json::json!({
        "pointer": "/public/packageData",
        "exclude": ["/*/icon"],
    }))
    .unwrap();
    let pointer = params.pointer.unwrap();
    let (start, _) = subscribe_start(
        &db,
        pointer.clone(),
        params.projection.clone().try_into().unwrap(),
        None,
    )
    .await;
    assert_eq!(
        start.value,
        Some(imbl_value::json!({ "x": { "state": 1 } }))
    );
    db.put(
        &"/public/packageData/x/icon".parse::<JsonPointer>().unwrap(),
        &"bigger",
    )
    .await
    .unwrap();
    db.put(
        &"/public/packageData/x/state"
            .parse::<JsonPointer>()
            .unwrap(),
        &2,
    )
    .await
    .unwrap();
    let cursor = DbCursor {
        epoch: start.epoch.clone(),
        id: start.id,
    };
    let (resumed, _) = subscribe_start(
        &db,
        pointer,
        params.projection.try_into().unwrap(),
        Some(&cursor),
    )
    .await;
    assert_eq!(
        serde_json::to_value(&resumed.revisions).unwrap(),
        serde_json::json!([{
            "id": 3,
            "patch": [{ "op": "replace", "path": "/x/state", "value": 2 }],
        }])
    );
}

#[test]
fn projection_params_reject_array_positions() {
    let params: ProjectionParams =
        serde_json::from_value(serde_json::json!({ "include": ["/arr/1"] })).unwrap();
    let error = Projection::try_from(params).unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidRequest);
}
