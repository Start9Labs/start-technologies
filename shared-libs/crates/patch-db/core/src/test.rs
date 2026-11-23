use std::future::Future;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use imbl_value::{ord_json, OrdValue};
use json_ptr::{JsonPointer, ROOT};
use patch_db::{Catchup, DiffPatch, Error, JsonCatchup, PatchDb, Revision};
use proptest::prelude::*;
use tokio::fs;
use tokio::runtime::Builder;

use crate::{self as patch_db};

static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn unique_db_path(prefix: &str) -> String {
    let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("test-{}-{}.db", prefix, id)
}

async fn init_db(db_name: String) -> PatchDb {
    cleanup_db(&db_name).await;
    let db = PatchDb::open(db_name).await.unwrap();
    db.put(
        &JsonPointer::<&'static str>::default(),
        &ord_json!({
            "a": "test1",
            "b": {
                "a": "test2",
                "b": 1,
                "c": null,
            },
        }),
    )
    .await
    .unwrap();
    db
}

async fn cleanup_db(db_name: &str) {
    let path = Path::new(db_name);
    fs::remove_file(path).await.ok();
    fs::remove_file(path.with_extension("bak")).await.ok();
    fs::remove_file(path.with_extension("bak.tmp")).await.ok();
    fs::remove_file(path.with_extension("failed")).await.ok();
}

async fn put_string_into_root(db: &PatchDb, s: String) -> Arc<Revision> {
    db.put(&JsonPointer::<&'static str>::default(), &s)
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn basic() {
    let path = unique_db_path("basic");
    let db = init_db(path.clone()).await;
    let ptr: JsonPointer = "/b/b".parse().unwrap();
    let mut get_res: OrdValue = db.get(&ptr).await.unwrap();
    assert_eq!(get_res.as_u64(), Some(1));
    db.put(&ptr, "hello").await.unwrap();
    get_res = db.get(&ptr).await.unwrap();
    assert_eq!(get_res.as_str(), Some("hello"));
    db.close().await;
    cleanup_db(&path).await;
}

#[tokio::test]
async fn catch_up_replays_scoped_revisions_within_its_epoch() {
    let db = PatchDb::in_memory();
    db.put(
        &ROOT,
        &ord_json!({ "public": { "a": 1 }, "private": { "b": 1 } }),
    )
    .await
    .unwrap();
    let public: JsonPointer = "/public".parse().unwrap();
    let private: JsonPointer = "/private/b".parse().unwrap();
    let (epoch, catchup, _) = db.catch_up_and_sub(public.clone(), None).await;
    let Catchup::Dump(dump) = catchup else {
        panic!("expected a dump")
    };
    assert_eq!(dump.id, 1);
    db.put(&private, &2).await.unwrap();
    db.put(&"/public/a".parse::<JsonPointer>().unwrap(), &2)
        .await
        .unwrap();
    db.put(&private, &3).await.unwrap();

    let (_, catchup, _) = db
        .catch_up_and_sub(public.clone(), Some((&epoch, dump.id)))
        .await;
    let Catchup::Replay { id, revisions } = catchup else {
        panic!("expected a replay")
    };
    assert_eq!(id, 4);
    assert_eq!(revisions.iter().map(|rev| rev.id).collect::<Vec<_>>(), [3]);
    let mut value = dump.value;
    json_patch::patch(&mut value, &revisions[0].patch).unwrap();
    assert_eq!(value, ord_json!({ "a": 2 }));

    let (_, catchup, _) = db.catch_up_and_sub(public.clone(), Some((&epoch, 4))).await;
    assert!(matches!(catchup, Catchup::Replay { id: 4, revisions } if revisions.is_empty()));
    let other = PatchDb::in_memory()
        .catch_up_and_sub(ROOT.to_owned(), None)
        .await
        .0;
    assert_ne!(other, epoch);
    for since in [(&*other, 1), (&*epoch, 5)] {
        let (_, catchup, _) = db.catch_up_and_sub(public.clone(), Some(since)).await;
        assert!(matches!(catchup, Catchup::Dump(dump) if dump.id == 4));
    }
}

#[tokio::test]
async fn json_catch_up_replays_within_its_epoch_and_streams_later_revisions() {
    let db = PatchDb::in_memory();
    db.put(
        &ROOT,
        &ord_json!({ "index": { "package": {} }, "other": 0 }),
    )
    .await
    .unwrap();
    let index: JsonPointer = "/index".parse().unwrap();
    let capacity = NonZeroUsize::new(8).unwrap();
    let (epoch, catchup, _) = db
        .catch_up_and_sub_json(index.clone(), None, capacity)
        .await
        .unwrap();
    let JsonCatchup::Dump(json) = catchup else {
        panic!("expected a dump")
    };
    let dump: patch_db::Dump = serde_json::from_slice(&json).unwrap();
    db.put(&"/other".parse::<JsonPointer>().unwrap(), &1)
        .await
        .unwrap();
    db.put(&"/index/package/a".parse::<JsonPointer>().unwrap(), &1)
        .await
        .unwrap();

    let (_, catchup, mut sub) = db
        .catch_up_and_sub_json(index.clone(), Some((&epoch, dump.id)), capacity)
        .await
        .unwrap();
    let JsonCatchup::Replay { id, revisions } = catchup else {
        panic!("expected a replay")
    };
    assert_eq!(id, dump.id + 2);
    assert_eq!(revisions.len(), 1);
    assert_eq!(
        revisions[0]
            .patch
            .paths()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["/package/a"]
    );
    db.put(&"/index/package/b".parse::<JsonPointer>().unwrap(), &1)
        .await
        .unwrap();
    let next: Revision = serde_json::from_slice(&sub.recv().await.unwrap().unwrap()).unwrap();
    assert_eq!(next.id, id + 1);

    let (_, catchup, _) = db
        .catch_up_and_sub_json(index, Some(("other", dump.id)), capacity)
        .await
        .unwrap();
    assert!(matches!(catchup, JsonCatchup::Dump(_)));
}

#[tokio::test]
async fn catch_up_dumps_once_history_no_longer_reaches_back() {
    let db = PatchDb::in_memory();
    for i in 0..1100 {
        db.put(&ROOT, &i).await.unwrap();
    }
    let (epoch, _, _) = db.catch_up_and_sub(ROOT.to_owned(), None).await;
    let (_, oldest, _) = db
        .catch_up_and_sub(ROOT.to_owned(), Some((&epoch, 76)))
        .await;
    assert!(matches!(oldest, Catchup::Replay { revisions, .. } if revisions.len() == 1024));
    let (_, older, _) = db
        .catch_up_and_sub(ROOT.to_owned(), Some((&epoch, 75)))
        .await;
    assert!(matches!(older, Catchup::Dump(_)));
}

#[test]
fn in_memory_construction_needs_no_runtime() {
    drop(PatchDb::in_memory());
}

#[tokio::test]
async fn in_memory_handles_share_state_and_independent_databases_do_not() {
    let db = PatchDb::in_memory();
    let dump = db.dump(&ROOT).await;
    assert_eq!(dump.id, 0);
    assert_eq!(dump.value, OrdValue::Null);

    let other = PatchDb::in_memory();
    let clone = db.clone();
    let mut sub = clone.subscribe(ROOT.to_owned()).await;
    db.close().await;
    clone.put(&ROOT, &ord_json!({"count": 1})).await.unwrap();
    assert_eq!(clone.get_value(&ROOT).await, ord_json!({"count": 1}));
    assert_eq!(sub.try_recv().unwrap().id, 1);
    assert_eq!(other.sequence().await, 0);
    assert_eq!(other.get_value(&ROOT).await, OrdValue::Null);
    clone.close().await;
    assert!(matches!(
        sub.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Disconnected)
    ));

    let fresh = PatchDb::in_memory();
    assert_eq!(fresh.sequence().await, 0);
    assert_eq!(fresh.get_value(&ROOT).await, OrdValue::Null);
}

#[tokio::test]
async fn in_memory_subscriptions_watches_and_noops() {
    let db = PatchDb::in_memory();
    db.put(&ROOT, &ord_json!({"count": 0, "other": true}))
        .await
        .unwrap();
    let ptr: JsonPointer = "/count".parse().unwrap();
    let (dump, mut sub) = db.dump_and_sub(ptr.clone()).await;
    assert_eq!(dump.id, 1);
    assert_eq!(dump.value, ord_json!(0));
    let mut watch = db.watch(ptr.clone()).await;
    assert_eq!(watch.peek_and_mark_seen().unwrap(), ord_json!(0));

    let revision = db.put(&ptr, &1).await.unwrap().unwrap();
    assert_eq!(revision.id, 2);
    let scoped = sub.try_recv().unwrap();
    assert_eq!(scoped.id, 2);
    let mut scoped_value = dump.value;
    json_patch::patch(&mut scoped_value, &scoped.patch).unwrap();
    assert_eq!(scoped_value, ord_json!(1));
    assert_eq!(watch.peek().unwrap(), ord_json!(1));

    assert!(db.put(&ptr, &1).await.unwrap().is_none());
    assert_eq!(db.sequence().await, 2);
    assert!(matches!(
        sub.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
    db.put(&"/other".parse::<JsonPointer>().unwrap(), &false)
        .await
        .unwrap();
    assert!(matches!(
        sub.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
    assert_eq!(watch.peek().unwrap(), ord_json!(1));
    assert!(db.exists(&ptr).await);
    assert_eq!(db.get::<u64, _, _>(&ptr).await.unwrap(), 1);
    assert_eq!(db.keys(&ROOT).await.len(), 2);
}

#[tokio::test]
async fn in_memory_failed_mutations_preserve_state_and_revision() {
    let db = PatchDb::in_memory();
    let initial = ord_json!({"count": 1});
    db.put(&ROOT, &initial).await.unwrap();
    let mut sub = db.subscribe(ROOT.to_owned()).await;
    let patch: DiffPatch = imbl_value::from_value(ord_json!([
        {"op": "replace", "path": "/count", "value": 2},
        {"op": "remove", "path": "/missing"}
    ]))
    .unwrap();
    assert!(db.apply(patch).await.is_err());

    let failed = db
        .apply_function(|_| Err::<(OrdValue, ()), _>(Error::NodeDoesNotExist(ROOT.to_owned())))
        .await;
    assert!(failed.result.is_err());
    assert!(failed.revision.is_none());
    let panicked = db
        .apply_function(|_| -> Result<(OrdValue, ()), Error> { panic!("mutation panic") })
        .await;
    assert!(matches!(panicked.result, Err(Error::Panic(_))));
    assert!(panicked.revision.is_none());
    assert_eq!(db.sequence().await, 1);
    assert_eq!(db.get_value(&ROOT).await, initial);
    assert!(matches!(
        sub.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn in_memory_function_mutations() {
    let db = PatchDb::in_memory();
    let applied = db
        .apply_function(|_| Ok::<_, Error>((ord_json!({"count": 1}), "applied")))
        .await;
    assert_eq!(
        applied.result.unwrap(),
        (ord_json!({"count": 1}), "applied")
    );
    assert_eq!(applied.revision.unwrap().id, 1);
    let (value, result) = db
        .run_idempotent(|value| async move {
            assert_eq!(value, ord_json!({"count": 1}));
            Ok::<_, Error>((ord_json!({"count": 2}), "updated"))
        })
        .await
        .unwrap();
    assert_eq!(value, ord_json!({"count": 2}));
    assert_eq!(result, "updated");
    assert_eq!(db.sequence().await, 2);
    assert_eq!(db.get_value(&ROOT).await, value);
}

async fn write_past_compaction_boundary(db: &PatchDb) {
    for revision in 1..=4097u64 {
        assert_eq!(
            db.put(&ROOT, &revision).await.unwrap().unwrap().id,
            revision
        );
    }
    assert_eq!(db.sequence().await, 4097);
    assert_eq!(db.get_value(&ROOT).await, ord_json!(4097));
}

#[tokio::test]
async fn in_memory_writes_past_compaction_boundary() {
    write_past_compaction_boundary(&PatchDb::in_memory()).await;
}

#[tokio::test]
async fn file_storage_reopens_after_compaction() {
    let path = unique_db_path("compaction");
    cleanup_db(&path).await;
    let db = PatchDb::open(&path).await.unwrap();
    write_past_compaction_boundary(&db).await;
    db.close().await;
    let reopened = PatchDb::open(&path).await.unwrap();
    assert_eq!(reopened.sequence().await, 4097);
    assert_eq!(reopened.get_value(&ROOT).await, ord_json!(4097));
    assert_eq!(reopened.put(&ROOT, &4098).await.unwrap().unwrap().id, 4098);
    reopened.close().await;
    cleanup_db(&path).await;
}

#[tokio::test]
async fn store_file_is_private() {
    use std::os::unix::fs::PermissionsExt;

    let path = unique_db_path("private");
    cleanup_db(&path).await;
    let db = PatchDb::open(&path).await.unwrap();
    write_past_compaction_boundary(&db).await;
    let mode = fs::metadata(&path).await.unwrap().permissions().mode();
    db.close().await;
    cleanup_db(&path).await;
    assert_eq!(mode & 0o777, 0o600);
}

fn run_future<S: Into<String>, Fut: Future<Output = ()>>(name: S, fut: Fut) {
    Builder::new_multi_thread()
        .thread_name(name)
        .build()
        .unwrap()
        .block_on(fut)
}

proptest! {
    #[test]
    fn doesnt_crash(s in "\\PC*") {
        run_future("test-doesnt-crash", async {
            let path = unique_db_path("proptest");
            let db = init_db(path.clone()).await;
            put_string_into_root(&db, s).await;
            db.close().await;
            cleanup_db(&path).await;
        });
    }
}
