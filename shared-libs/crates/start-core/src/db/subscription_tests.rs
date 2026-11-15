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
