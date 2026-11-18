use std::sync::atomic::{AtomicBool, Ordering};

use futures::FutureExt;
use tokio::sync::oneshot;

use super::*;

struct Probe(RequestTasks, Arc<AtomicBool>);
impl Drop for Probe {
    fn drop(&mut self) {
        let owner = &self.0;
        assert!(owner.0.tracker.len() > 0);
        assert!(owner.rejection().is_some());
        self.1.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn quiescence_acknowledges_destruction_not_cancellation() {
    let tasks = RequestTasks::default();
    let destroyed = Arc::new(AtomicBool::new(false));
    let probe = Probe(tasks.clone(), destroyed.clone());
    let (started, running) = oneshot::channel();
    let operation = tasks.run(async move {
        let _probe = probe;
        started.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    let job = tokio::spawn(operation);
    running.await.unwrap();
    tasks.quiesce().await;
    assert!(destroyed.load(Ordering::SeqCst));
    assert!(job.await.unwrap().is_none());
    assert!(tasks
        .run(async { panic!("late producer ran") })
        .await
        .is_none());
}

#[tokio::test]
async fn an_unpolled_request_releases_resources_before_its_permit() {
    let tasks = RequestTasks::default();
    let destroyed = Arc::new(AtomicBool::new(false));
    let probe = Probe(tasks.clone(), destroyed.clone());
    let operation = tasks.run(async move {
        drop(probe);
    });
    let quiesce = tasks.quiesce();
    tokio::pin!(quiesce);
    assert!(futures::poll!(&mut quiesce).is_pending());
    drop(operation);
    quiesce.await;
    assert!(destroyed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn refused_work_is_destroyed_before_the_returned_future_is_polled() {
    let tasks = RequestTasks::default();
    let owner = tasks.clone();
    let destroyed = Arc::new(AtomicBool::new(false));
    let probe = Probe(tasks.clone(), destroyed.clone());
    let observed = destroyed.clone();
    tasks
        .run(async move {
            owner.close(REQUEST_CANCELLED_ERROR);
            let refused = owner.run(async move {
                drop(probe);
            });
            assert!(observed.load(Ordering::SeqCst));
            drop(refused);
        })
        .await
        .unwrap();
    tasks.quiesce().await;
}

#[tokio::test]
async fn an_unstarted_blocking_callback_drops_inputs_before_acknowledging_quiescence() {
    let tasks = RequestTasks::default();
    let destroyed = Arc::new(AtomicBool::new(false));
    let probe = Probe(tasks.clone(), destroyed.clone());
    let (send, receive) = oneshot::channel();
    let operation = tasks.run(async move {
        let callback = retain_current_request(move || drop(probe));
        assert!(send.send(callback).is_ok());
        std::future::pending::<()>().await;
    });
    let job = tokio::spawn(operation);
    let callback = receive.await.unwrap();
    let quiesce = tasks.quiesce();
    tokio::pin!(quiesce);
    assert!(futures::poll!(&mut quiesce).is_pending());
    assert!(job.await.unwrap().is_none());
    assert!(futures::poll!(&mut quiesce).is_pending());
    drop(callback);
    quiesce.await;
    assert!(destroyed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn cancelling_a_caller_still_joins_its_running_blocking_callback() {
    let tasks = RequestTasks::default();
    let destroyed = Arc::new(AtomicBool::new(false));
    let probe = Probe(tasks.clone(), destroyed.clone());
    let (started, running) = oneshot::channel();
    let (release, completion) = std::sync::mpsc::channel();
    use crate::HandlerExt;

    struct BlockingContext;
    impl crate::Context for BlockingContext {}
    let arguments = Arc::new(std::sync::Mutex::new(Some((probe, started, completion))));
    let handler = crate::from_fn_blocking(move |_: crate::HandlerArgs<BlockingContext>| {
        let (_probe, started, completion) = arguments.lock().unwrap().take().unwrap();
        started.send(()).unwrap();
        completion.recv().unwrap();
        Ok::<(), yajrc::RpcError>(())
    })
    .no_cli()
    .no_ts();
    let server = crate::Server::new(
        || async { Ok(BlockingContext) },
        crate::ParentHandler::new().subcommand("block", handler),
    );
    let operation = tasks.run(server.handle_command("block", imbl_value::json!({})));
    let job = tokio::spawn(operation);
    running.await.unwrap();
    let quiesce = tasks.quiesce();
    tokio::pin!(quiesce);
    assert!(futures::poll!(&mut quiesce).is_pending());
    assert!(job.await.unwrap().is_none());
    assert!(futures::poll!(&mut quiesce).is_pending());
    assert!(!destroyed.load(Ordering::SeqCst));
    release.send(()).unwrap();
    quiesce.await;
    assert!(destroyed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn passive_replies_do_not_exempt_other_requests() {
    let tasks = RequestTasks::default();
    let (started, waiting) = oneshot::channel();
    let (release, completion) = oneshot::channel();
    let reply = tasks.run(async move {
        preserve_request_reply();
        started.send(()).unwrap();
        completion.await.unwrap()
    });
    let mut reply = tokio::spawn(reply);
    waiting.await.unwrap();
    let producer = tasks.run(std::future::pending::<()>());
    let producer = tokio::spawn(producer);
    tasks.quiesce().await;
    assert!(producer.await.unwrap().is_none());
    assert!((&mut reply).now_or_never().is_none());
    assert!(tasks.run(async { 99 }).await.is_none());
    release.send(42).unwrap();
    assert_eq!(reply.await.unwrap(), Some(42));
}

#[tokio::test]
async fn quiescence_preserves_the_application_rejection_error() {
    let tasks = RequestTasks::default();
    tasks.close(yajrc::METHOD_NOT_FOUND_ERROR);
    tasks.quiesce().await;
    assert!(tasks
        .run(async { panic!("closed admission ran a handler") })
        .await
        .is_none());
    assert_eq!(
        tasks.rejection().unwrap().code,
        yajrc::METHOD_NOT_FOUND_ERROR.code
    );
}

#[tokio::test]
async fn response_bodies_retain_producer_resources() {
    let tasks = RequestTasks::default();
    let response = tasks
        .run(async {
            retain_response_body(Response::new(Body::from("unfinished response")), |future| {
                tokio::spawn(future);
            })
        })
        .await
        .unwrap();
    let quiesce = tasks.quiesce();
    tokio::pin!(quiesce);
    assert!(futures::poll!(&mut quiesce).is_pending());
    drop(response);
    quiesce.await;
}

#[tokio::test]
async fn a_stalled_transport_cannot_retain_body_producer_resources() {
    let tasks = RequestTasks::default();
    let destroyed = Arc::new(AtomicBool::new(false));
    let probe = Probe(tasks.clone(), destroyed.clone());
    let response = tasks
        .run(async move {
            let source = async_stream::stream! {
                let _probe = probe;
                loop { yield Ok::<_, std::convert::Infallible>("frame"); }
            };
            retain_response_body(Response::new(Body::from_stream(source)), |future| {
                tokio::spawn(future);
            })
        })
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), tasks.quiesce())
        .await
        .unwrap();
    assert!(destroyed.load(Ordering::SeqCst));
    drop(response);
}
