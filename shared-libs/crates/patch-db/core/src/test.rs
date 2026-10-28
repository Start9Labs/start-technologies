use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use imbl_value::{json, Value};
use json_ptr::{JsonPointer, ROOT};
use patch_db::{DiffPatch, Error, PatchDb, Revision};
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
        &json!({
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
    let mut get_res: Value = db.get(&ptr).await.unwrap();
    assert_eq!(get_res.as_u64(), Some(1));
    db.put(&ptr, "hello").await.unwrap();
    get_res = db.get(&ptr).await.unwrap();
    assert_eq!(get_res.as_str(), Some("hello"));
    db.close().await;
    cleanup_db(&path).await;
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
    assert_eq!(dump.value, Value::Null);

    let other = PatchDb::in_memory();
    let clone = db.clone();
    let mut sub = clone.subscribe(ROOT.to_owned()).await;
    db.close().await;
    clone.put(&ROOT, &json!({"count": 1})).await.unwrap();
    assert_eq!(clone.get_value(&ROOT).await, json!({"count": 1}));
    assert_eq!(sub.try_recv().unwrap().id, 1);
    assert_eq!(other.sequence().await, 0);
    assert_eq!(other.get_value(&ROOT).await, Value::Null);
    clone.close().await;
    assert!(matches!(
        sub.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Disconnected)
    ));

    let fresh = PatchDb::in_memory();
    assert_eq!(fresh.sequence().await, 0);
    assert_eq!(fresh.get_value(&ROOT).await, Value::Null);
}

#[tokio::test]
async fn in_memory_subscriptions_watches_and_noops() {
    let db = PatchDb::in_memory();
    db.put(&ROOT, &json!({"count": 0, "other": true}))
        .await
        .unwrap();
    let ptr: JsonPointer = "/count".parse().unwrap();
    let (dump, mut sub) = db.dump_and_sub(ptr.clone()).await;
    assert_eq!(dump.id, 1);
    assert_eq!(dump.value, json!(0));
    let mut watch = db.watch(ptr.clone()).await;
    assert_eq!(watch.peek_and_mark_seen().unwrap(), json!(0));

    let revision = db.put(&ptr, &1).await.unwrap().unwrap();
    assert_eq!(revision.id, 2);
    let scoped = sub.try_recv().unwrap();
    assert_eq!(scoped.id, 2);
    let mut scoped_value = dump.value;
    json_patch::patch(&mut scoped_value, &scoped.patch).unwrap();
    assert_eq!(scoped_value, json!(1));
    assert_eq!(watch.peek().unwrap(), json!(1));

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
    assert_eq!(watch.peek().unwrap(), json!(1));
    assert!(db.exists(&ptr).await);
    assert_eq!(db.get::<u64, _, _>(&ptr).await.unwrap(), 1);
    assert_eq!(db.keys(&ROOT).await.len(), 2);
}

#[tokio::test]
async fn in_memory_failed_mutations_preserve_state_and_revision() {
    let db = PatchDb::in_memory();
    let initial = json!({"count": 1});
    db.put(&ROOT, &initial).await.unwrap();
    let mut sub = db.subscribe(ROOT.to_owned()).await;
    let patch: DiffPatch = imbl_value::from_value(json!([
        {"op": "replace", "path": "/count", "value": 2},
        {"op": "remove", "path": "/missing"}
    ]))
    .unwrap();
    assert!(db.apply(patch).await.is_err());

    let failed = db
        .apply_function(|_| Err::<(Value, ()), _>(Error::NodeDoesNotExist(ROOT.to_owned())))
        .await;
    assert!(failed.result.is_err());
    assert!(failed.revision.is_none());
    let panicked = db
        .apply_function(|_| -> Result<(Value, ()), Error> { panic!("mutation panic") })
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
        .apply_function(|_| Ok::<_, Error>((json!({"count": 1}), "applied")))
        .await;
    assert_eq!(applied.result.unwrap(), (json!({"count": 1}), "applied"));
    assert_eq!(applied.revision.unwrap().id, 1);
    let (value, result) = db
        .run_idempotent(|value| async move {
            assert_eq!(value, json!({"count": 1}));
            Ok::<_, Error>((json!({"count": 2}), "updated"))
        })
        .await
        .unwrap();
    assert_eq!(value, json!({"count": 2}));
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
    assert_eq!(db.get_value(&ROOT).await, json!(4097));
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
    assert_eq!(reopened.get_value(&ROOT).await, json!(4097));
    assert_eq!(reopened.put(&ROOT, &4098).await.unwrap().unwrap().id, 4098);
    reopened.close().await;
    cleanup_db(&path).await;
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
