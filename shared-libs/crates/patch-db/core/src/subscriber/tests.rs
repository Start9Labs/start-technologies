use std::num::NonZeroUsize;

use imbl_value::json;
use json_ptr::ROOT;

use crate::{Dump, PatchDb};

#[test]
fn idle_subscriptions_reclaim_dropped_listeners_without_a_revision() {
    let mut broadcast = super::Broadcast::new();
    for i in 0..100 {
        drop(broadcast.subscribe(format!("/dropped/{i}").parse().unwrap()));
    }
    let _live = broadcast.subscribe(ROOT.to_owned());
    assert_eq!(broadcast.listeners.len(), 1);
    assert_eq!(broadcast.listeners[&ROOT.to_owned()].unbounded.len(), 1);

    let capacity = NonZeroUsize::new(1).unwrap();
    let dump = Dump {
        id: 0,
        value: json!(null),
    };
    for _ in 0..100 {
        drop(
            broadcast
                .subscribe_json(ROOT.to_owned(), dump.clone(), capacity)
                .unwrap(),
        );
    }
    let (_, _live_json) = broadcast
        .subscribe_json(ROOT.to_owned(), dump, capacity)
        .unwrap();
    let listeners = &broadcast.listeners[&ROOT.to_owned()];
    assert_eq!(listeners.unbounded.len(), 1);
    assert_eq!(listeners.json.len(), 1);
    assert_eq!(listeners.json[&capacity].receiver_count(), 1);
}

#[tokio::test]
async fn json_subscribers_share_dumps_and_scoped_revisions_across_capacities() {
    let db = PatchDb::in_memory();
    db.put(&ROOT, &json!({ "visible": 1, "secret": 7 }))
        .await
        .unwrap();
    let pointer: json_ptr::JsonPointer = "/visible".parse().unwrap();
    let (first_dump, mut first) = db
        .dump_and_sub_json(pointer.clone(), NonZeroUsize::new(1).unwrap())
        .await
        .unwrap();
    let (second_dump, mut second) = db
        .dump_and_sub_json(pointer.clone(), NonZeroUsize::new(2).unwrap())
        .await
        .unwrap();
    assert_eq!(first_dump.as_ptr(), second_dump.as_ptr());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&first_dump).unwrap(),
        serde_json::json!({ "id": 1, "value": 1 })
    );
    let (_, mut raw) = db.dump_and_sub(pointer.clone()).await;

    db.put(&pointer, &2).await.unwrap();
    let first_json = first.recv().await.unwrap().unwrap();
    let second_json = second.recv().await.unwrap().unwrap();
    assert_eq!(first_json.as_ptr(), second_json.as_ptr());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&first_json).unwrap(),
        serde_json::json!({
            "id": 2, "patch": [{ "op": "replace", "path": "", "value": 2 }]
        })
    );
    assert_eq!(
        first_json.as_ref(),
        serde_json::to_vec(&raw.recv().await.unwrap())
            .unwrap()
            .as_slice()
    );

    db.put(&"/secret".parse::<json_ptr::JsonPointer>().unwrap(), &8)
        .await
        .unwrap();
    assert!(futures::poll!(std::pin::pin!(first.recv())).is_pending());
    assert!(futures::poll!(std::pin::pin!(second.recv())).is_pending());
    let (fresh, _) = db
        .dump_and_sub_json(pointer, NonZeroUsize::new(1).unwrap())
        .await
        .unwrap();
    let dump: Dump = serde_json::from_slice(&fresh).unwrap();
    assert_eq!(dump.id, 3);
    assert_eq!(dump.value, json!(2));
}

#[tokio::test]
async fn a_lagging_json_subscriber_closes_and_recovers_only_from_a_fresh_dump() {
    let db = PatchDb::in_memory();
    db.put(&ROOT, &0).await.unwrap();
    let capacity = NonZeroUsize::new(1).unwrap();
    let (initial, mut slow) = db
        .dump_and_sub_json(ROOT.to_owned(), capacity)
        .await
        .unwrap();
    let (_, mut fast) = db
        .dump_and_sub_json(ROOT.to_owned(), capacity)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Dump>(&initial).unwrap().value,
        json!(0)
    );
    for value in 1..=2 {
        db.put(&ROOT, &value).await.unwrap();
        let json = fast.recv().await.unwrap().unwrap();
        let revision: crate::Revision = serde_json::from_slice(&json).unwrap();
        assert_eq!(revision.id, value + 1);
    }
    assert!(slow.recv().await.unwrap().is_none());
    let (dump, mut reconnected) = db
        .dump_and_sub_json(ROOT.to_owned(), capacity)
        .await
        .unwrap();
    let dump: Dump = serde_json::from_slice(&dump).unwrap();
    assert_eq!(dump.id, 3);
    assert_eq!(dump.value, json!(2));
    db.put(&ROOT, &3).await.unwrap();
    assert!(slow.recv().await.unwrap().is_none());
    let next = reconnected.recv().await.unwrap().unwrap();
    assert_eq!(next.as_ptr(), fast.recv().await.unwrap().unwrap().as_ptr());
}

#[tokio::test]
async fn json_dump_and_subscription_cover_changes_before_the_dump_is_read() {
    let db = PatchDb::in_memory();
    db.put(&ROOT, &json!({ "count": 0 })).await.unwrap();
    let (dump, mut revisions) = db
        .dump_and_sub_json(ROOT.to_owned(), NonZeroUsize::new(4).unwrap())
        .await
        .unwrap();
    db.put(&"/count".parse::<json_ptr::JsonPointer>().unwrap(), &1)
        .await
        .unwrap();
    let mut dump: Dump = serde_json::from_slice(&dump).unwrap();
    assert_eq!(dump.value["count"], json!(0));
    let json = revisions.recv().await.unwrap().unwrap();
    let revision: crate::Revision = serde_json::from_slice(&json).unwrap();
    assert_eq!(revision.id, dump.id + 1);
    crate::json_patch::patch(&mut dump.value, &revision.patch).unwrap();
    assert_eq!(dump.value, db.dump(&ROOT).await.value);
    assert!(db.put(&ROOT, &dump.value).await.unwrap().is_none());
    assert!(futures::poll!(std::pin::pin!(revisions.recv())).is_pending());
}

#[tokio::test]
async fn dropping_the_database_closes_json_subscriptions() {
    let db = PatchDb::in_memory();
    let (_, mut revisions) = db
        .dump_and_sub_json(ROOT.to_owned(), NonZeroUsize::new(1).unwrap())
        .await
        .unwrap();
    drop(db);
    assert!(revisions.recv().await.unwrap().is_none());
    assert!(revisions.recv().await.unwrap().is_none());
}
