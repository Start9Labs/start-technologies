use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rpc_toolkit::requests::RequestTasks;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, Server, from_fn, from_fn_async, yajrc};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{Notify, oneshot};

use super::{ErrorCollection, shutdown_rpc_server};
use crate::Error;
use crate::context::rpc::{BackgroundTasks, spawn_background};
use crate::util::GeneralBoxedGuard;

#[derive(Clone)]
struct EffectProbe {
    background: Arc<BackgroundTasks>,
    started: Arc<Notify>,
    finish: Arc<Notify>,
    completed: Arc<AtomicBool>,
    dropped: Arc<AtomicBool>,
}
impl Context for EffectProbe {}

#[tokio::test]
async fn uninit_effects_outlive_public_admission_and_finish_before_resource_teardown() {
    let public = RequestTasks::default();
    public.close(yajrc::INTERNAL_ERROR);
    assert!(public.run(async {}).await.is_none());
    let probe = EffectProbe {
        background: Arc::new(BackgroundTasks::new(None)),
        started: Arc::new(Notify::new()),
        finish: Arc::new(Notify::new()),
        completed: Arc::new(AtomicBool::new(false)),
        dropped: Arc::new(AtomicBool::new(false)),
    };
    let handlers = ParentHandler::new()
        .subcommand(
            "read",
            from_fn(|ctx: EffectProbe| {
                assert!(spawn_background(&ctx.background, async {}).is_err());
                Ok::<_, Error>(7_u64)
            })
            .no_cli()
            .no_ts(),
        )
        .subcommand(
            "hold",
            from_fn_async(|ctx: EffectProbe| async move {
                let dropped = ctx.dropped.clone();
                let _resource = GeneralBoxedGuard::new(move || {
                    dropped.store(true, Ordering::SeqCst);
                });
                ctx.started.notify_one();
                ctx.finish.notified().await;
                ctx.completed.store(true, Ordering::SeqCst);
                Ok::<_, Error>(())
            })
            .no_cli()
            .no_ts(),
        );
    let server_probe = probe.clone();
    let server = Server::new(
        move || std::future::ready(Ok(server_probe.clone())),
        handlers,
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("effects.sock");
    let socket_path = path.clone();
    let (ready, listening) = oneshot::channel();
    let server_task = tokio::spawn(async move {
        let (shutdown, serve) = server.run_unix(&socket_path, |_| {}).unwrap();
        ready.send(shutdown).ok().unwrap();
        serve.await;
    });
    let shutdown = listening.await.unwrap();
    let effect_probe = probe.clone();
    let uninit = async move {
        let mut client = BufReader::new(UnixStream::connect(path).await.unwrap());
        client
            .get_mut()
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"read\",\"params\":{}}\n")
            .await
            .unwrap();
        let mut reply = String::new();
        client.read_line(&mut reply).await.unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&reply).unwrap()["result"],
            7
        );
        client
            .get_mut()
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"hold\",\"params\":{}}\n")
            .await
            .unwrap();
        effect_probe.started.notified().await;
        drop(client);
        Ok::<_, Error>(())
    };
    let retired = Notify::new();
    let teardown = async {
        let mut errors = ErrorCollection::new();
        shutdown_rpc_server(
            uninit,
            || {
                shutdown.shutdown();
                retired.notify_one();
            },
            server_task,
            None,
            &mut errors,
        )
        .await;
        errors.into_result().unwrap();
        assert!(probe.completed.load(Ordering::SeqCst));
        assert!(probe.dropped.load(Ordering::SeqCst));
    };
    tokio::pin!(teardown);
    tokio::time::timeout(Duration::from_secs(3), async {
        tokio::select! {
            _ = retired.notified() => {}
            _ = &mut teardown => panic!("resource teardown preceded effect completion"),
        }
    })
    .await
    .expect("uninit could not call its effects after public admission closed");
    assert!(!probe.completed.load(Ordering::SeqCst));
    assert!(!probe.dropped.load(Ordering::SeqCst));
    assert!(futures::poll!(&mut teardown).is_pending());
    probe.finish.notify_one();
    tokio::time::timeout(Duration::from_secs(3), &mut teardown)
        .await
        .expect("effect server failed to drain accepted calls");
}
