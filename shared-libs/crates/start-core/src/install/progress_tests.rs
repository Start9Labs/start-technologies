use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::routing::{get, post};
use futures::{SinkExt, StreamExt};
use tokio::sync::Notify;

use super::*;
use crate::context::config::ClientConfig;
use crate::middleware::auth::session::RpcSessionUpgrader;
use crate::sign::session::Target;
use crate::sign::{AnySigningKey, Signer};
use crate::util::future::NonDetachingJoinHandle;
use crate::util::net::secure::EncryptedSocket;

fn cli_args(ctx: CliContext, path: PathBuf) -> HandlerArgs<CliContext, CliInstallParams> {
    HandlerArgs {
        context: ctx,
        parent_method: ["package"].into(),
        method: ["install"].into(),
        params: CliInstallParams::Sideload { path, trust: false },
        inherited_params: rpc_toolkit::Empty {},
        raw_params: json!({}),
        rpc_stream: None,
    }
}

#[tokio::test]
async fn cli_sideload_waits_for_finalization_and_cancels_observation_when_upload_fails() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/s9pk-trailer/endorsed.s9pk");
    let expected = tokio::fs::read(&path).await.unwrap();
    let uploads = Arc::new(AtomicUsize::new(0));
    let admissions = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(AtomicUsize::new(0));
    let uploaded = Arc::new(Notify::new());
    let fully_counted = Arc::new(Notify::new());
    let finalize = Arc::new(Notify::new());
    let canceled = Arc::new(Notify::new());
    let guids = Arc::new([Guid::new(), Guid::new()]);
    let key: Signer = AnySigningKey::generate().into();
    let rpc = {
        let admissions = admissions.clone();
        let requests = requests.clone();
        let uploaded = uploaded.clone();
        let fully_counted = fully_counted.clone();
        let finalize = finalize.clone();
        let canceled = canceled.clone();
        let guids = guids.clone();
        get(move |upgrade: WebSocketUpgrade| {
            admissions.fetch_add(1, Ordering::SeqCst);
            let key = key.clone();
            let requests = requests.clone();
            let uploaded = uploaded.clone();
            let fully_counted = fully_counted.clone();
            let finalize = finalize.clone();
            let canceled = canceled.clone();
            let guids = guids.clone();
            async move {
                upgrade.on_upgrade(move |socket| async move {
                let (_, _, mut socket) = EncryptedSocket::accept(Box::new(socket), &key, &["127.0.0.1".into()], NonZeroUsize::new(1024 * 1024).unwrap(), &Target::Rpc, None, |_| async { Ok(()) }).await.unwrap();
                for step in 0..2 {
                    let text = socket.next().await.unwrap().unwrap().into_text().unwrap();
                    let request: serde_json::Value = serde_json::from_str(&text).unwrap();
                    assert_eq!(request["method"], "package.sideload");
                    assert_eq!(request["rpcstream"], rpc_toolkit::RPCSTREAM_VERSION);
                    assert!(!request["params"]["signatures"].as_object().unwrap().is_empty());
                    requests.fetch_add(1, Ordering::SeqCst);
                    let id = &request["id"];
                    socket.send(Message::Text(serde_json::json!({"jsonrpc":"2.0","id":id,"result":{"upload":guids[step],"progress":null}}).to_string().into())).await.unwrap();
                    if step == 0 {
                        uploaded.notified().await;
                        let mut snapshot = FullProgress::new();
                        snapshot.set_complete();
                        socket.send(Message::Text(serde_json::json!({"rpcstream":rpc_toolkit::RPCSTREAM_VERSION,"id":id,"result":snapshot,"end":false}).to_string().into())).await.unwrap();
                        fully_counted.notify_one();
                        finalize.notified().await;
                        socket.send(Message::Text(serde_json::json!({"rpcstream":rpc_toolkit::RPCSTREAM_VERSION,"id":id,"error":{"code":ErrorKind::Filesystem as i32,"message":"finalization failed"},"end":true}).to_string().into())).await.unwrap();
                    } else {
                        let text = socket.next().await.unwrap().unwrap().into_text().unwrap();
                        let close: serde_json::Value = serde_json::from_str(&text).unwrap();
                        assert_eq!(close["close"], *id);
                        canceled.notify_one();
                    }
                }
                while socket.next().await.is_some() {}
            })
            }
        })
    };
    let upload = {
        let uploads = uploads.clone();
        let uploaded = uploaded.clone();
        post(
            move |axum::extract::Path(guid): axum::extract::Path<String>,
                  headers: HeaderMap,
                  body: bytes::Bytes| {
                let step = uploads.fetch_add(1, Ordering::SeqCst);
                assert_eq!(guid, guids[step].as_ref());
                assert_eq!(
                    headers[CONTENT_LENGTH]
                        .to_str()
                        .unwrap()
                        .parse::<usize>()
                        .unwrap(),
                    expected.len()
                );
                assert_eq!(body.as_ref(), expected.as_slice());
                uploaded.notify_one();
                async move {
                    if step == 0 {
                        StatusCode::NO_CONTENT
                    } else {
                        StatusCode::SERVICE_UNAVAILABLE
                    }
                }
            },
        )
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let _server: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new()
                .route(RpcSessionUpgrader::PATH, rpc)
                .route("/rest/rpc/{guid}", upload),
        )
        .await
        .unwrap();
    })
    .into();
    let ctx =
        CliContext::init(ClientConfig::try_parse_from(["test", "-H", &host]).unwrap()).unwrap();
    assert!(ctx.id_key.set(AnySigningKey::generate().into()).is_ok());
    let mut operation = Box::pin(cli_install(cli_args(ctx.clone(), path.clone())));
    tokio::time::timeout(Duration::from_secs(3), async {
        tokio::select! {
            result = &mut operation => panic!("operation ended before its final result: {result:?}"),
            _ = fully_counted.notified() => (),
        }
    }).await.unwrap();
    assert!(futures::poll!(operation.as_mut()).is_pending());
    finalize.notify_one();
    let error = tokio::time::timeout(Duration::from_secs(3), operation)
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, ErrorKind::Filesystem as i32);
    let error = tokio::time::timeout(
        Duration::from_secs(3),
        cli_install(cli_args(ctx.clone(), path)),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert_eq!(error.code, ErrorKind::Network as i32);
    tokio::time::timeout(Duration::from_secs(3), canceled.notified())
        .await
        .unwrap();
    assert_eq!(admissions.load(Ordering::SeqCst), 1);
    assert_eq!(requests.load(Ordering::SeqCst), 2);
    assert_eq!(uploads.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn legacy_progress_protocols_keep_their_distinct_end_markers_and_late_errors() {
    for update in [false, true] {
        let app = axum::Router::new().route(
            "/progress",
            get(move |upgrade: WebSocketUpgrade| async move {
                upgrade.on_upgrade(move |socket| async move {
                    let tracker = FullProgressTracker::new();
                    tracker.complete();
                    let source = progress_until_complete(tracker, async {
                        Err(Error::new(eyre!("cleanup failed"), ErrorKind::Filesystem))
                    });
                    let socket = crate::util::net::WebSocket::new(
                        Box::new(socket) as rpc_toolkit::BoxWebSocket
                    );
                    if update {
                        let _ =
                            forward_progress(socket, crate::progress::progress_with_null(source))
                                .await;
                    } else {
                        let _ = forward_progress(socket, source).await;
                    }
                })
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/progress", listener.local_addr().unwrap());
        let _server: NonDetachingJoinHandle<()> =
            tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }).into();
        let (socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let mut socket = socket.map(|message| message.with_kind(ErrorKind::Network));
        let snapshot: FullProgress = crate::util::net::tungstenite::next_json(&mut socket)
            .await
            .unwrap();
        assert!(snapshot.overall.is_complete());
        if update {
            let sentinel: Option<FullProgress> =
                crate::util::net::tungstenite::next_json(&mut socket)
                    .await
                    .unwrap();
            assert!(sentinel.is_none());
        }
        let error = crate::util::net::tungstenite::next_text(&mut socket)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("cleanup failed"));
    }
}

#[test]
fn legacy_sideload_reply_preserves_the_progress_guid() {
    let reply = SideloadResponse {
        upload: Guid::new(),
        progress: Some(Guid::new()),
    };
    let wire = serde_json::to_value(&reply).unwrap();
    assert_eq!(wire["progress"], reply.progress.unwrap().as_ref());
    let typed = serde_json::to_value(SideloadResponse {
        upload: reply.upload,
        progress: None,
    })
    .unwrap();
    assert!(typed["progress"].is_null());
}
