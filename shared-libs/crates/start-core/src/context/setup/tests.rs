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
