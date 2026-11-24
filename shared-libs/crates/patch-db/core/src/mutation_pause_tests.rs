use std::num::NonZeroUsize;
use std::pin::pin;
use std::task::Poll;

use futures::poll;
use imbl_value::json;
use json_ptr::ROOT;

use super::{cleanup_db, unique_db_path};
use crate::patch::diff;
use crate::{PatchDb, Projection, Update};

#[tokio::test]
async fn pause_mutations_keeps_readers_and_subscriptions_live_behind_a_queued_writer() {
    let db = PatchDb::in_memory();
    db.put(&ROOT, &json!({"value": 1})).await.unwrap();
    let pause = db.pause_mutations().await;
    let clone = db.clone();
    let mut write = pin!(clone.put(&ROOT, &2));
    assert!(poll!(&mut write).is_pending());

    let mut read = pin!(async {
        assert_eq!(db.get_value(&ROOT).await, json!({"value": 1}));
        assert_eq!(db.dump(&ROOT).await.id, 1);
        db.dump_and_sub(ROOT.to_owned()).await;
        db.catch_up_and_sub(ROOT.to_owned(), None).await;
        db.dump_and_sub_json(ROOT.to_owned(), NonZeroUsize::new(1).unwrap())
            .await
            .unwrap();
        let (_, _, mut projected) = db
            .catch_up_and_sub_mutable(ROOT.to_owned(), Projection::default(), None)
            .await;
        assert!(db.reproject(projected.id, Projection::default()).await);
        let Update::Extend(extension) = projected.receiver.try_recv().unwrap() else {
            panic!("expected a projection acknowledgement")
        };
        assert_eq!(extension.id, 1);
        (db.subscribe(ROOT.to_owned()).await, projected)
    });
    let Poll::Ready((mut subscriber, mut projected)) = poll!(&mut read) else {
        panic!("queued mutations must not exclude readers or subscriptions");
    };
    assert!(subscriber.try_recv().is_err());
    drop(pause);
    write.await.unwrap();
    assert_eq!(subscriber.try_recv().unwrap().id, 2);
    let Update::Revision(revision) = projected.receiver.try_recv().unwrap() else {
        panic!("expected a resumed mutation")
    };
    assert_eq!(revision.id, 2);
    assert_eq!(db.get_value(&ROOT).await, json!(2));
}

#[tokio::test]
async fn pause_mutations_covers_patch_function_and_idempotent_commit_paths() {
    let db = PatchDb::in_memory();
    db.put(&ROOT, &1).await.unwrap();
    let pause = db.pause_mutations().await;
    let patch = diff(&json!(1), &json!(2));
    let mut apply = pin!(db.apply(patch));
    let mut function = pin!(db.apply_function(|_| Ok::<_, crate::Error>((json!(3), ()))));
    let mut idempotent =
        pin!(db.run_idempotent(|_| async { Ok::<_, crate::Error>((json!(4), ())) }));
    assert!(poll!(&mut apply).is_pending());
    assert!(poll!(&mut function).is_pending());
    assert!(poll!(&mut idempotent).is_pending());
    assert_eq!(db.get_value(&ROOT).await, json!(1));
    drop(pause);
    apply.await.unwrap();
    assert_eq!(db.get_value(&ROOT).await, json!(2));
    function.await.result.unwrap();
    assert_eq!(db.get_value(&ROOT).await, json!(3));
    idempotent.await.unwrap();
    assert_eq!(db.get_value(&ROOT).await, json!(4));
    assert_eq!(db.sequence().await, 4);
}

#[tokio::test]
async fn pause_mutations_preserves_file_contents_and_durable_resume() {
    let path = unique_db_path("paused-mutations");
    let db = PatchDb::open(&path).await.unwrap();
    db.put(&ROOT, &1).await.unwrap();
    let before = tokio::fs::read(&path).await.unwrap();
    let pause = db.pause_mutations().await;
    {
        let mut write = pin!(db.put(&ROOT, &2));
        assert!(poll!(&mut write).is_pending());
        assert_eq!(tokio::fs::read(&path).await.unwrap(), before);
        assert_eq!(db.get_value(&ROOT).await, json!(1));
        drop(pause);
        write.await.unwrap();
    }
    db.close().await;
    let reopened = PatchDb::open(&path).await.unwrap();
    assert_eq!(reopened.get_value(&ROOT).await, json!(2));
    assert_eq!(reopened.sequence().await, 2);
    reopened.close().await;
    cleanup_db(&path).await;
}

#[tokio::test]
async fn pause_mutations_cancellation_releases_waiters_without_changing_values() {
    let db = PatchDb::in_memory();
    db.put(&ROOT, &1).await.unwrap();
    let pause = db.pause_mutations().await;
    {
        let mut next_pause = pin!(db.pause_mutations());
        assert!(poll!(&mut next_pause).is_pending());
        let mut cancelled = pin!(db.put(&ROOT, &2));
        assert!(poll!(&mut cancelled).is_pending());
    }
    drop(pause);
    let mut next = pin!(db.put(&ROOT, &3));
    assert!(
        matches!(poll!(&mut next), Poll::Ready(Ok(_))),
        "cancelled waiters must not retain the mutation pause"
    );
    assert_eq!(db.get_value(&ROOT).await, json!(3));
    assert_eq!(db.sequence().await, 2);
}

#[tokio::test]
async fn pause_mutations_also_defers_last_handle_close() {
    let path = unique_db_path("paused-close");
    let db = PatchDb::open(&path).await.unwrap();
    db.put(&ROOT, &1).await.unwrap();
    let pause = db.pause_mutations().await;
    let before = tokio::fs::read(&path).await.unwrap();
    let mut close = pin!(db.close());
    assert!(poll!(&mut close).is_pending());
    assert_eq!(tokio::fs::read(&path).await.unwrap(), before);
    drop(pause);
    close.await;
    let reopened = PatchDb::open(&path).await.unwrap();
    assert_eq!(reopened.get_value(&ROOT).await, json!(1));
    reopened.close().await;
    cleanup_db(&path).await;
}
