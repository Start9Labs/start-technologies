use futures::FutureExt;
use futures::future::BoxFuture;
use rpc_toolkit::{
    HandlerArgs, HandlerExt, ParentHandler, Server, WebSocketOptions, WebSocketUpgrader,
};
use tokio::sync::Notify;

use super::*;

#[derive(Clone, Default)]
struct CancelContext {
    started: Arc<Notify>,
    dropped: Arc<Notify>,
    completed: Arc<AtomicUsize>,
    release: Arc<Notify>,
}
impl rpc_toolkit::Context for CancelContext {}

struct PendingHandler(Arc<Notify>);
impl Drop for PendingHandler {
    fn drop(&mut self) {
        self.0.notify_one();
    }
}

struct EncryptedUpgrader {
    signer: Signer,
    connections: Arc<AtomicUsize>,
}
impl WebSocketUpgrader<CancelContext> for EncryptedUpgrader {
    fn max_wire_message_size(&self, logical: NonZeroUsize) -> NonZeroUsize {
        crate::util::net::secure::wire_message_size(logical)
    }
    fn request_context(&self, context: &CancelContext) -> CancelContext {
        context.clone()
    }
    fn upgrade(
        &self,
        context: Arc<CancelContext>,
        _: http::request::Parts,
        socket: axum::extract::ws::WebSocket,
        options: WebSocketOptions,
    ) -> BoxFuture<'static, Result<rpc_toolkit::UpgradedWebSocket<CancelContext>, axum::Error>>
    {
        let signer = self.signer.clone();
        let connections = self.connections.clone();
        async move {
            let (_, _, socket) = EncryptedSocket::accept(
                Box::new(socket),
                &signer,
                &["server.test".into()],
                options.max_message_size,
                &Target::Rpc,
                None,
                |_| async { Ok(()) },
            )
            .await
            .unwrap();
            connections.fetch_add(1, Ordering::SeqCst);
            Ok(rpc_toolkit::UpgradedWebSocket {
                context,
                socket: Box::new(socket),
            })
        }
        .boxed()
    }
}

#[tokio::test]
async fn ordinary_encrypted_call_drop_and_per_request_deadline_release_backend_futures_on_a_reused_live_connection()
 {
    let context = CancelContext::default();
    let base = context.clone();
    let connections = Arc::new(AtomicUsize::new(0));
    let tree = ParentHandler::new()
        .subcommand(
            "blocked",
            rpc_toolkit::from_fn_async(|args: HandlerArgs<CancelContext>| async move {
                let _guard = PendingHandler(args.context.dropped.clone());
                args.context.started.notify_one();
                args.context.release.notified().await;
                args.context.completed.fetch_add(1, Ordering::SeqCst);
                Ok::<_, RpcError>(())
            })
            .no_cli()
            .no_ts(),
        )
        .subcommand(
            "echo",
            rpc_toolkit::from_fn_async(|_: HandlerArgs<CancelContext>| async {
                Ok::<_, RpcError>("kept".to_owned())
            })
            .no_cli()
            .no_ts(),
        );
    let server = Server::new(
        move || {
            let context = base.clone();
            async move { Ok(context) }
        },
        tree,
    )
    .for_websocket()
    .with_upgrader(EncryptedUpgrader {
        signer: crate::sign::AnySigningKey::generate().into(),
        connections: connections.clone(),
    });
    let app = Router::new().route(RpcSessionUpgrader::PATH, get(server));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url: Url = format!(
        "ws://{}{}",
        listener.local_addr().unwrap(),
        RpcSessionUpgrader::PATH
    )
    .parse()
    .unwrap();
    let _server: NonDetachingJoinHandle<()> =
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }).into();
    let client = Client::new(reqwest::Client::builder().http1_only().build().unwrap());
    let signer: Signer = crate::sign::AnySigningKey::generate().into();
    for deadline in [None, Some(Duration::from_millis(10))] {
        let mut request = Box::pin(client.call(
            &signer,
            url.clone(),
            "server.test",
            HeaderMap::new(),
            "blocked",
            imbl_value::json!({}),
        ));
        tokio::time::timeout(Duration::from_secs(3), async {
            tokio::select! {
                _ = context.started.notified() => (),
                result = &mut request => panic!("unexpected reply: {result:?}"),
            }
        })
        .await
        .unwrap();
        assert_eq!(
            client
                .call(
                    &signer,
                    url.clone(),
                    "server.test",
                    HeaderMap::new(),
                    "echo",
                    imbl_value::json!({})
                )
                .await
                .unwrap()
                .as_str(),
            Some("kept")
        );
        if let Some(deadline) = deadline {
            assert!(tokio::time::timeout(deadline, request).await.is_err());
        } else {
            drop(request);
        }
        tokio::time::timeout(Duration::from_secs(3), context.dropped.notified())
            .await
            .unwrap();
        assert_eq!(context.completed.load(Ordering::SeqCst), 0);
        assert_eq!(
            client
                .call(
                    &signer,
                    url.clone(),
                    "server.test",
                    HeaderMap::new(),
                    "echo",
                    imbl_value::json!({})
                )
                .await
                .unwrap()
                .as_str(),
            Some("kept")
        );
    }
    assert_eq!(connections.load(Ordering::SeqCst), 1);
}
