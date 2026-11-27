use std::sync::atomic::{AtomicUsize, Ordering};

use futures::StreamExt;
use tokio::sync::{Barrier, Notify, oneshot};

use super::*;
use crate::setup::test::context;

async fn observe(context: &SetupContext) -> Result<(), Error> {
    let source = context.progress_stream();
    tokio::pin!(source);
    while let Some(snapshot) = source.next().await {
        snapshot?;
    }
    Ok(())
}

#[tokio::test]
async fn failed_setup_retains_its_error_without_marking_progress_complete() {
    let (context, _server) = context();
    context
        .run_setup(|| async {
            Err(Error::new(
                eyre!("setup fixture failure"),
                ErrorKind::Filesystem,
            ))
        })
        .unwrap();
    assert_eq!(
        observe(&context).await.unwrap_err().kind,
        ErrorKind::Filesystem
    );
    assert!(!context.progress.snapshot().overall.is_complete());
    assert_eq!(
        context.result.get().unwrap().as_ref().err().unwrap().kind,
        ErrorKind::Filesystem
    );
    assert_eq!(
        crate::setup::status(rpc_toolkit::HandlerArgs {
            context: context.clone(),
            parent_method: Default::default(),
            method: Default::default(),
            params: rpc_toolkit::Empty {},
            inherited_params: rpc_toolkit::Empty {},
            raw_params: imbl_value::json!({}),
            rpc_stream: None,
        })
        .await
        .unwrap_err()
        .kind,
        ErrorKind::Filesystem
    );
    assert_eq!(
        crate::setup::complete(context.clone())
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Filesystem
    );
    assert_eq!(
        observe(&context).await.unwrap_err().kind,
        ErrorKind::Filesystem
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn setup_reservation_runs_the_accepted_closure_and_never_a_rejected_contender() {
    let (context, _server) = context();
    let barrier = Arc::new(Barrier::new(32));
    let finish = Arc::new(Notify::new());
    let constructors = Arc::new(AtomicUsize::new(0));
    let (started, mut runs) = tokio::sync::mpsc::unbounded_channel();
    let requests = (0..32).map(|id| {
        let context = context.clone();
        let barrier = barrier.clone();
        let finish = finish.clone();
        let constructors = constructors.clone();
        let started = started.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            let accepted = context.run_setup(move || {
                constructors.fetch_add(1, Ordering::SeqCst);
                async move {
                    started.send(id).unwrap();
                    finish.notified().await;
                    Err(Error::new(
                        eyre!("accepted fixture result"),
                        ErrorKind::InvalidRequest,
                    ))
                }
            });
            (id, accepted)
        })
    });
    let outcomes = futures::future::join_all(requests).await;
    let accepted = outcomes
        .into_iter()
        .map(Result::unwrap)
        .filter_map(|(id, result)| result.ok().map(|_| id))
        .collect::<Vec<_>>();
    assert_eq!(accepted.len(), 1);
    assert_eq!(runs.recv().await.unwrap(), accepted[0]);
    finish.notify_one();
    assert_eq!(
        observe(&context).await.unwrap_err().kind,
        ErrorKind::InvalidRequest
    );
    assert_eq!(constructors.load(Ordering::SeqCst), 1);
    assert!(runs.try_recv().is_err());
}

#[tokio::test]
async fn percentage_completion_waits_for_the_actual_setup_failure() {
    let (context, _server) = context();
    let (finish, pending) = oneshot::channel();
    context
        .run_setup(move || async move {
            pending.await.unwrap();
            Err(Error::new(
                eyre!("late setup failure"),
                ErrorKind::Filesystem,
            ))
        })
        .unwrap();
    context.progress.complete();
    let source = context.progress_stream();
    tokio::pin!(source);
    assert!(source.next().await.unwrap().unwrap().overall.is_complete());
    assert!(futures::poll!(std::pin::pin!(source.next())).is_pending());
    finish.send(()).unwrap();
    let mut failed = false;
    while let Some(snapshot) = source.next().await {
        if let Err(error) = snapshot {
            assert_eq!(error.kind, ErrorKind::Filesystem);
            assert!(error.to_string().contains("late setup failure"));
            failed = true;
        }
    }
    assert!(failed);
}

#[tokio::test]
async fn observer_drop_and_unopened_guid_do_not_abort_work_or_retain_its_context_afterward() {
    let (context, _server) = context();
    let (finish, pending) = oneshot::channel();
    let (completed, observed) = oneshot::channel();
    context
        .run_setup(move || async move {
            pending.await.unwrap();
            completed.send(()).unwrap();
            Err(Error::new(
                eyre!("unobserved fixture result"),
                ErrorKind::Filesystem,
            ))
        })
        .unwrap();
    let source = context.progress_stream();
    drop(source);
    let _unopened = context.progress(None).await;
    let owner = Arc::downgrade(&context.0);
    drop(context);
    finish.send(()).unwrap();
    observed.await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while owner.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn worker_failure_is_an_error_not_completion_or_an_indefinite_progress_stream() {
    let (context, _server) = context();
    context
        .run_setup(|| async { panic!("setup fixture worker panic") })
        .unwrap();
    let error = tokio::time::timeout(Duration::from_secs(2), observe(&context))
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Unknown);
    assert!(!context.progress.snapshot().overall.is_complete());
}

#[tokio::test]
async fn typed_setup_status_reports_late_failure_and_cached_initiating_error() {
    use crate::context::CliContext;
    use crate::context::config::ClientConfig;
    use crate::net::web_server::{Acceptor, WebServer};
    let (context, _unused_server) = context();
    let (finish, pending) = oneshot::channel();
    context
        .run_setup(move || async move {
            pending.await.unwrap();
            Err(Error::new(
                eyre!("typed setup failure"),
                ErrorKind::Filesystem,
            ))
        })
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let _server = WebServer::new(
        Acceptor::new(listener),
        crate::net::static_server::ui_router(context.clone()),
    );
    let cli = CliContext::init(ClientConfig {
        host: Some(host.parse().unwrap()),
        ..Default::default()
    })
    .unwrap();
    context.progress.complete();
    let mut response = cli
        .call_startup_stream::<crate::progress::FullProgress, ()>(
            "setup.status",
            imbl_value::json!({}),
            http::HeaderMap::new(),
        )
        .await
        .unwrap();
    assert_eq!(response.initial["status"].as_str(), Some("running"));
    assert!(response.initial["guid"].is_null());
    assert!(
        response
            .items
            .next()
            .await
            .unwrap()
            .unwrap()
            .overall
            .is_complete()
    );
    assert!(futures::poll!(std::pin::pin!(response.items.next())).is_pending());
    finish.send(()).unwrap();
    let error = loop {
        match response.items.next().await {
            Ok(Some(_)) => (),
            Err(error) => break error,
            Ok(None) => panic!("percentage completed setup"),
        }
    };
    assert_eq!(error.code, ErrorKind::Filesystem as i32);
    assert_eq!(error.data.unwrap()["details"], "typed setup failure");
    let error = cli
        .call_startup_stream::<crate::progress::FullProgress, ()>(
            "setup.status",
            imbl_value::json!({}),
            http::HeaderMap::new(),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, ErrorKind::Filesystem as i32);
}

#[tokio::test]
async fn idle_setup_status_has_a_successful_empty_typed_observation() {
    use crate::context::CliContext;
    use crate::context::config::ClientConfig;
    use crate::net::web_server::{Acceptor, WebServer};
    let (context, _unused_server) = context();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let _server = WebServer::new(
        Acceptor::new(listener),
        crate::net::static_server::ui_router(context),
    );
    let cli = CliContext::init(ClientConfig {
        host: Some(host.parse().unwrap()),
        ..Default::default()
    })
    .unwrap();
    let mut response = cli
        .call_startup_stream::<crate::progress::FullProgress, ()>(
            "setup.status",
            imbl_value::json!({}),
            http::HeaderMap::new(),
        )
        .await
        .unwrap();
    assert!(matches!(
        response.initial["status"].as_str(),
        Some("needs-install" | "incomplete")
    ));
    assert!(response.items.next().await.unwrap().is_none());
    response.items.finish().await.unwrap();
}

#[tokio::test]
async fn encrypted_legacy_setup_guid_reports_the_actual_late_failure() {
    use crate::context::CliContext;
    use crate::context::config::ClientConfig;
    use crate::net::web_server::{Acceptor, WebServer};

    let (context, _unused_server) = context();
    let (finish, pending) = oneshot::channel();
    context
        .run_setup(move || async move {
            pending.await.unwrap();
            Err(Error::new(
                eyre!("legacy setup failure"),
                ErrorKind::Filesystem,
            ))
        })
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let _server = WebServer::new(
        Acceptor::new(listener),
        crate::net::static_server::ui_router(context.clone()),
    );
    let cli = CliContext::init(ClientConfig {
        host: Some(host.parse().unwrap()),
        ..Default::default()
    })
    .unwrap();
    let response = cli
        .call_remote::<SetupContext>("setup.status", imbl_value::json!({}))
        .await
        .unwrap();
    assert_eq!(response["status"].as_str(), Some("running"));
    let guid: Guid = from_value(response["guid"].clone()).unwrap();
    let mut socket = cli.ws_continuation::<SetupContext>(guid).await.unwrap();
    let _: crate::progress::FullProgress = crate::util::net::tungstenite::next_json(&mut socket)
        .await
        .unwrap();
    finish.send(()).unwrap();
    let error = loop {
        match crate::util::net::tungstenite::next_text(&mut socket).await {
            Ok(Some(_)) => (),
            Err(error) => break error,
            Ok(None) => panic!("failed setup closed successfully"),
        }
    };
    assert!(error.to_string().contains("legacy setup failure"));
}

#[cfg(target_os = "linux")]
fn open_fds_for(path: &Path) -> Vec<std::path::PathBuf> {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    std::fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| std::fs::read_link(entry.path()).ok())
        .filter(|target| *target == canonical)
        .collect()
}

#[cfg(not(target_os = "linux"))]
fn open_fds_for(_path: &Path) -> Vec<std::path::PathBuf> {
    Vec::new()
}

#[tokio::test]
async fn shutdown_aborts_running_setup_breaks_context_cycle_and_closes_db() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("embassy.db");
    let (context, _server) = context();

    let initial_db = context.open_db(&db_path).await.unwrap();
    drop(initial_db);
    PatchDb::open(&db_path).await.unwrap().close().await;

    let (started_tx, started_rx) = oneshot::channel();
    let setup_ctx = context.clone();
    let task_db_path = db_path.clone();
    context
        .run_setup(move || async move {
            let db = setup_ctx.open_db(&task_db_path).await.unwrap();
            let _ = started_tx.send(db.clone());
            std::future::pending::<()>().await;
            unreachable!()
        })
        .unwrap();

    let lingering = started_rx.await.unwrap();
    let owner = Arc::downgrade(&context.0);

    assert!(context.shutdown().await);
    assert_eq!(
        context
            .run_setup(|| async { unreachable!() })
            .unwrap_err()
            .kind,
        ErrorKind::Cancelled
    );
    assert_eq!(
        observe(&context).await.unwrap_err().kind,
        ErrorKind::Cancelled
    );
    drop(context);

    assert_eq!(owner.strong_count(), 0);
    assert!(
        lingering
            .put(
                &patch_db::json_ptr::ROOT,
                &imbl_value::json!({"closed": true})
            )
            .await
            .is_err()
    );
    let open_fds = open_fds_for(&db_path);
    assert!(
        open_fds.is_empty(),
        "setup shutdown left embassy.db open: {open_fds:?}"
    );
    PatchDb::open(&db_path).await.unwrap().close().await;
    drop(lingering);
}

#[tokio::test]
async fn shutdown_during_in_flight_open_db_waits_and_closes_storage() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("embassy.db");
    let seed_db = PatchDb::open(&db_path).await.unwrap();
    for i in 0..128u64 {
        seed_db
            .put(&patch_db::json_ptr::ROOT, &imbl_value::json!({ "step": i }))
            .await
            .unwrap();
    }
    seed_db.close().await;

    let (context, _server) = context();
    let (opening_tx, opening_rx) = oneshot::channel();
    let setup_ctx = context.clone();
    let task_db_path = db_path.clone();
    context
        .run_setup(move || async move {
            let _ = opening_tx.send(());
            let _db = setup_ctx.open_db(&task_db_path).await?;
            std::future::pending::<()>().await;
            unreachable!()
        })
        .unwrap();

    opening_rx.await.unwrap();
    let owner = Arc::downgrade(&context.0);
    assert!(context.shutdown().await);
    assert_eq!(
        context.open_db(&db_path).await.err().unwrap().kind,
        ErrorKind::Cancelled
    );
    drop(context);

    assert_eq!(owner.strong_count(), 0);
    let open_fds = open_fds_for(&db_path);
    assert!(
        open_fds.is_empty(),
        "setup shutdown left embassy.db open during open_db abort: {open_fds:?}"
    );
    PatchDb::open(&db_path).await.unwrap().close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_during_second_open_before_dns_activation_waits_and_closes_storage() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("embassy.db");
    let (context, _server) = context();

    let (second_open_entered_tx, second_open_entered_rx) = oneshot::channel();
    let (release_second_open_tx, release_second_open_rx) = oneshot::channel();
    let setup_ctx = context.clone();
    let task_db_path = db_path.clone();
    context
        .run_setup(move || async move {
            let first_db = setup_ctx.open_db(&task_db_path).await?;
            for i in 0..32u64 {
                first_db
                    .put(&patch_db::json_ptr::ROOT, &imbl_value::json!({ "step": i }))
                    .await
                    .unwrap();
            }
            drop(first_db);

            let _second_db = setup_ctx
                .open_db_with(&task_db_path, move |path| async move {
                    let _ = second_open_entered_tx.send(());
                    release_second_open_rx.await.unwrap();
                    PatchDb::open(path).await
                })
                .await?;
            std::future::pending::<()>().await;
            unreachable!()
        })
        .unwrap();

    second_open_entered_rx.await.unwrap();

    let owner = Arc::downgrade(&context.0);
    let shutdown_ctx = context.clone();
    let mut shutdown_task = tokio::spawn(async move { shutdown_ctx.shutdown().await });

    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut shutdown_task)
            .await
            .is_err(),
        "setup shutdown returned before the in-flight second open finished"
    );

    let _ = release_second_open_tx.send(());

    let closed = tokio::time::timeout(Duration::from_secs(5), shutdown_task)
        .await
        .expect("setup shutdown hung after second open unblocked")
        .unwrap();
    assert!(closed);
    assert!(context.dns.shutdown().await.is_none());
    drop(context);

    assert_eq!(owner.strong_count(), 0);
    let open_fds = open_fds_for(&db_path);
    assert!(
        open_fds.is_empty(),
        "setup shutdown left embassy.db open after second open abort: {open_fds:?}"
    );
    let reopened = PatchDb::open(&db_path).await.unwrap();
    assert_eq!(
        reopened.get_value(&patch_db::json_ptr::ROOT).await,
        imbl_value::json!({ "step": 31 })
    );
    reopened.close().await;

    let payload = "x".repeat(64 * 1024);
    for yields in 0..4usize {
        let iter_dir = tempfile::tempdir().unwrap();
        let iter_db_path = iter_dir.path().join("embassy.db");
        let (ctx, _srv) = crate::setup::test::context();
        let (reopening_tx, reopening_rx) = oneshot::channel();
        let setup_ctx = ctx.clone();
        let task_db_path = iter_db_path.clone();
        let iter_payload = payload.clone();
        ctx.run_setup(move || async move {
            let first_db = setup_ctx.open_db(&task_db_path).await?;
            first_db
                .put(
                    &patch_db::json_ptr::ROOT,
                    &imbl_value::json!({ "payload": iter_payload }),
                )
                .await
                .unwrap();
            drop(first_db);
            let _ = reopening_tx.send(());
            let _second_db = setup_ctx.open_db(&task_db_path).await?;
            std::future::pending::<()>().await;
            unreachable!()
        })
        .unwrap();

        reopening_rx.await.unwrap();
        for _ in 0..yields {
            tokio::task::yield_now().await;
        }
        assert!(ctx.shutdown().await);
        assert!(ctx.dns.shutdown().await.is_none());
        drop(ctx);

        let open_fds = open_fds_for(&iter_db_path);
        assert!(
            open_fds.is_empty(),
            "setup shutdown left embassy.db open during second open sweep {yields}: {open_fds:?}"
        );
        PatchDb::open(&iter_db_path).await.unwrap().close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn shutdown_bounds_stalled_close_storage() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("embassy.db");
    let (context, _server) = context();
    let db = context.open_db(&db_path).await.unwrap();

    let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let blocker_db = db.clone();
    let channels = std::panic::AssertUnwindSafe((started_tx, release_rx));
    let blocker = tokio::spawn(async move {
        let _ = blocker_db
            .apply_function(move |v| {
                let std::panic::AssertUnwindSafe((started_tx, release_rx)) = channels;
                let _ = started_tx.send(());
                let _ = release_rx.recv_timeout(Duration::from_secs(5));
                Ok::<_, patch_db::Error>((v, ()))
            })
            .await;
    });
    started_rx.recv().unwrap();

    let closed = tokio::time::timeout(
        Duration::from_secs(1),
        context.shutdown_with_timeout(Duration::from_millis(50)),
    )
    .await
    .expect("SetupContext::shutdown hung on stalled close_storage");
    assert!(!closed);

    let _ = release_tx.send(());
    blocker.await.unwrap();

    tokio::time::timeout(Duration::from_secs(2), PatchDb::wait_until_closed(&db_path))
        .await
        .expect("detached close_storage did not release lock");
    assert!(open_fds_for(&db_path).is_empty());
    PatchDb::open(&db_path).await.unwrap().close().await;
    drop(db);
}
