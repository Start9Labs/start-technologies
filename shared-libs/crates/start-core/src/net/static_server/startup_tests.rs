use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use clap::Parser;
use futures::StreamExt;
use rpc_toolkit::{Empty, HandlerArgs, HandlerExt, Server, from_fn_async};
use tokio::sync::Notify;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use super::*;
use crate::context::CliContext;
use crate::context::config::{ClientConfig, ServerConfig};
use crate::middleware::auth::signature::SignatureAuthContext;
use crate::net::dns::DnsController;
use crate::net::web_server::{Acceptor, WebServer};
use crate::sign::AnySigningKey;
use crate::util::GeneralGuard;
use crate::util::sync::Watch;

async fn context() -> InitContext {
    let dns = Arc::new(DnsController::new(Watch::new(Default::default())));
    InitContext::init(&ServerConfig::default(), dns)
        .await
        .unwrap()
}

async fn server(router: Router) -> (String, WebServer<tokio::net::TcpListener>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    (host, WebServer::new(Acceptor::new(listener), router))
}

#[tokio::test]
async fn initialization_owner_loss_is_cancellation_not_success() {
    let ctx = context().await;
    let source = ctx.progress_stream();
    drop(ctx);
    tokio::pin!(source);
    assert!(!source.next().await.unwrap().unwrap().overall.is_complete());
    assert_eq!(
        source.next().await.unwrap().unwrap_err().kind,
        ErrorKind::Cancelled
    );
}

#[tokio::test]
#[ignore = "Requires START_CLI_BIN pointing to the built CLI"]
async fn actual_init_cli_uses_startup_possession_and_the_actual_terminal() {
    let binary = std::env::var("START_CLI_BIN").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let key = temp.path().join("malformed.pem");
    tokio::fs::write(&key, "not a key").await.unwrap();
    for failed in [false, true] {
        let ctx = context().await;
        let (host, mut server) = server(ui_router(ctx.clone())).await;
        let child = tokio::process::Command::new(&binary)
            .current_dir(temp.path())
            .env("HOME", temp.path())
            .args([
                "--host",
                &host,
                "--id-key-path",
                key.to_str().unwrap(),
                "--id-key-gpg",
                "unavailable-init-fixture",
                "init",
                "subscribe",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !ctx.observers.has_observers() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        ctx.complete_and_retire(if failed {
            Err(Error::new(
                eyre!("actual init failure"),
                ErrorKind::Filesystem,
            ))
        } else {
            Ok(())
        })
        .await;
        server.serve_router(Router::new());
        let output = tokio::time::timeout(Duration::from_secs(15), child.wait_with_output())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            output.status.success(),
            !failed,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if failed {
            assert_eq!(output.status.code(), Some(ErrorKind::Filesystem as i32));
        }
        assert_eq!(tokio::fs::read_to_string(&key).await.unwrap(), "not a key");
    }
}

#[tokio::test]
async fn initialization_terminal_is_written_before_router_replacement() {
    for failed in [false, true] {
        for _ in 0..8 {
            let ctx = context().await;
            let (host, mut server) = server(ui_router(ctx.clone())).await;
            let cli =
                CliContext::init(ClientConfig::try_parse_from(["test", "-H", &host]).unwrap())
                    .unwrap();
            let mut response = cli
                .call_startup_stream::<crate::progress::FullProgress, ()>(
                    "init.subscribe",
                    imbl_value::json!({}),
                    http::HeaderMap::new(),
                )
                .await
                .unwrap();
            assert!(
                !response
                    .items
                    .next()
                    .await
                    .unwrap()
                    .unwrap()
                    .overall
                    .is_complete()
            );
            let result = if failed {
                Err(Error::new(
                    eyre!("actual init failure"),
                    ErrorKind::Filesystem,
                ))
            } else {
                Ok(())
            };
            ctx.complete_and_retire(result).await;
            server.serve_router(Router::new());
            let outcome = async {
                while response.items.next().await?.is_some() {}
                response.items.finish().await
            }
            .await;
            if failed {
                assert_eq!(outcome.unwrap_err().code, ErrorKind::Filesystem as i32);
            } else {
                outcome.unwrap();
            }
        }
    }
}

#[tokio::test]
async fn unredeemed_initialization_guid_does_not_delay_retirement_or_allow_new_observers() {
    let ctx = context().await;
    let (host, _server) = server(ui_router(ctx.clone())).await;
    let cli =
        CliContext::init(ClientConfig::try_parse_from(["test", "-H", &host]).unwrap()).unwrap();
    let initial: crate::init::InitProgressRes = from_value(
        cli.call_remote::<InitContext>("init.subscribe", imbl_value::json!({}))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(initial.guid.is_some());
    let retirement = ctx.complete_and_retire(Ok(()));
    tokio::pin!(retirement);
    assert!(futures::poll!(retirement.as_mut()).is_ready());
    let error = cli
        .call_startup_stream::<crate::progress::FullProgress, ()>(
            "init.subscribe",
            imbl_value::json!({}),
            http::HeaderMap::new(),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, ErrorKind::Cancelled as i32);
    assert_eq!(
        cli.call_remote::<InitContext>("state", imbl_value::json!({}))
            .await
            .unwrap()
            .as_str(),
        Some("initializing")
    );
}

#[tokio::test]
async fn canceled_initialization_observer_releases_retirement() {
    let ctx = context().await;
    let (host, _server) = server(ui_router(ctx.clone())).await;
    let cli =
        CliContext::init(ClientConfig::try_parse_from(["test", "-H", &host]).unwrap()).unwrap();
    let response = cli
        .call_startup_stream::<crate::progress::FullProgress, ()>(
            "init.subscribe",
            imbl_value::json!({}),
            http::HeaderMap::new(),
        )
        .await
        .unwrap();
    drop(response);
    cli.call_remote::<InitContext>("state", imbl_value::json!({}))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), ctx.complete_and_retire(Ok(())))
        .await
        .unwrap();
}

#[tokio::test]
async fn redeemed_initialization_guid_drains_its_actual_close_before_retirement() {
    for failed in [false, true] {
        let ctx = context().await;
        let (host, mut server) = server(ui_router(ctx.clone())).await;
        let cli =
            CliContext::init(ClientConfig::try_parse_from(["test", "-H", &host]).unwrap()).unwrap();
        let initial: crate::init::InitProgressRes = from_value(
            cli.call_remote::<InitContext>("init.subscribe", imbl_value::json!({}))
                .await
                .unwrap(),
        )
        .unwrap();
        let mut socket = cli
            .ws_continuation::<InitContext>(initial.guid.unwrap())
            .await
            .unwrap();
        crate::util::net::tungstenite::next_json::<crate::progress::FullProgress, _>(&mut socket)
            .await
            .unwrap();
        let result = if failed {
            Err(Error::new(
                eyre!("actual legacy init failure"),
                ErrorKind::Filesystem,
            ))
        } else {
            Ok(())
        };
        let read_terminal = async {
            loop {
                match socket.next().await.unwrap().unwrap() {
                    tokio_tungstenite::tungstenite::Message::Text(_) => (),
                    tokio_tungstenite::tungstenite::Message::Close(Some(frame)) => {
                        assert_eq!(u16::from(frame.code), if failed { 1011 } else { 1000 });
                        let _ = futures::SinkExt::send(
                            &mut socket,
                            tokio_tungstenite::tungstenite::Message::Close(None),
                        )
                        .await;
                        break;
                    }
                    other => panic!("expected authenticated init close, got {other:?}"),
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(ctx.complete_and_retire(result), read_terminal);
        })
        .await
        .unwrap();
        server.serve_router(Router::new());
    }
}

#[tokio::test]
async fn initialization_percentage_does_not_hide_a_late_typed_error() {
    let ctx = context().await;
    let (host, _server) = server(ui_router(ctx.clone())).await;
    let cli =
        CliContext::init(ClientConfig::try_parse_from(["test", "-H", &host]).unwrap()).unwrap();
    let mut response = cli
        .call_startup_stream::<crate::progress::FullProgress, ()>(
            "init.subscribe",
            imbl_value::json!({}),
            http::HeaderMap::new(),
        )
        .await
        .unwrap();
    assert!(response.initial["guid"].is_null());
    ctx.progress.complete();
    while !response
        .items
        .next()
        .await
        .unwrap()
        .unwrap()
        .overall
        .is_complete()
    {}
    {
        let next = response.items.next();
        tokio::pin!(next);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut next)
                .await
                .is_err()
        );
    }
    ctx.complete(Err(Error::new(
        eyre!("late init failure"),
        ErrorKind::Filesystem,
    )));
    let error = loop {
        match response.items.next().await {
            Ok(Some(_)) => (),
            Err(error) => break error,
            Ok(None) => panic!("percentage completed initialization"),
        }
    };
    assert_eq!(error.code, ErrorKind::Filesystem as i32);
    assert_eq!(error.data.unwrap()["details"], "late init failure");
    assert_eq!(
        cli.call_remote::<InitContext>("state", imbl_value::json!({}))
            .await
            .unwrap()
            .as_str(),
        Some("initializing")
    );
}

#[tokio::test]
async fn initialization_success_survives_observer_drop_and_is_replayed_to_new_observers() {
    let ctx = context().await;
    let dropped = ctx.progress_stream();
    drop(dropped);
    ctx.complete(Ok(()));
    assert!(ctx.progress.snapshot().overall.is_complete());
    for _ in 0..2 {
        let source = ctx.progress_stream();
        tokio::pin!(source);
        assert!(source.next().await.unwrap().unwrap().overall.is_complete());
        assert!(source.next().await.is_none());
    }
    let (host, _server) = server(ui_router(ctx)).await;
    let cli =
        CliContext::init(ClientConfig::try_parse_from(["test", "-H", &host]).unwrap()).unwrap();
    let mut response = cli
        .call_startup_stream::<crate::progress::FullProgress, ()>(
            "init.subscribe",
            imbl_value::json!({}),
            http::HeaderMap::new(),
        )
        .await
        .unwrap();
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
    assert!(response.items.next().await.unwrap().is_none());
    response.items.finish().await.unwrap();
    let response: crate::init::InitProgressRes = from_value(
        cli.call_remote::<InitContext>("init.subscribe", imbl_value::json!({}))
            .await
            .unwrap(),
    )
    .unwrap();
    let mut continuation = cli
        .ws_continuation::<InitContext>(response.guid.unwrap())
        .await
        .unwrap();
    assert!(
        crate::util::net::tungstenite::next_json::<crate::progress::FullProgress, _>(
            &mut continuation
        )
        .await
        .unwrap()
        .overall
        .is_complete()
    );
    assert!(
        crate::util::net::tungstenite::next_text(&mut continuation)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn initialization_error_remains_an_error_for_late_legacy_observers() {
    let ctx = context().await;
    ctx.complete(Err(Error::new(
        eyre!("cached init failure"),
        ErrorKind::Filesystem,
    )));
    assert!(!ctx.progress.snapshot().overall.is_complete());
    let (host, _server) = server(ui_router(ctx)).await;
    let cli =
        CliContext::init(ClientConfig::try_parse_from(["test", "-H", &host]).unwrap()).unwrap();
    let response: crate::init::InitProgressRes = from_value(
        cli.call_remote::<InitContext>("init.subscribe", imbl_value::json!({}))
            .await
            .unwrap(),
    )
    .unwrap();
    let mut continuation = cli
        .ws_continuation::<InitContext>(response.guid.unwrap())
        .await
        .unwrap();
    crate::util::net::tungstenite::next_json::<crate::progress::FullProgress, _>(&mut continuation)
        .await
        .unwrap();
    assert!(
        crate::util::net::tungstenite::next_text(&mut continuation)
            .await
            .unwrap_err()
            .to_string()
            .contains("cached init failure")
    );
}

#[tokio::test]
async fn actual_startup_context_admits_without_a_database_and_preserves_http_and_host_policy() {
    let ctx = context().await;
    let (host, _server) = server(ui_router(ctx.clone())).await;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("malformed-key");
    tokio::fs::write(&path, "not a key").await.unwrap();
    let cli = CliContext::init(
        ClientConfig::try_parse_from([
            "test",
            "-H",
            &host,
            "--id-key-path",
            path.to_str().unwrap(),
        ])
        .unwrap(),
    )
    .unwrap();
    for _ in 0..2 {
        assert_eq!(
            cli.call_remote::<InitContext>("state", imbl_value::json!({}))
                .await
                .unwrap()
                .as_str(),
            Some("initializing")
        );
    }
    assert!(cli.id_key.get().is_none());
    let mut headers = http::HeaderMap::new();
    headers.insert(http::header::ORIGIN, host.parse().unwrap());
    assert_eq!(
        cli.call_startup_rpc("state", imbl_value::json!({}), headers)
            .await
            .unwrap()
            .as_str(),
        Some("initializing")
    );
    let preflight = reqwest::Client::new()
        .request(http::Method::OPTIONS, format!("{host}/rpc/v1"))
        .header(http::header::ORIGIN, "https://unrelated.example")
        .header(http::header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .header(http::header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
        .send()
        .await
        .unwrap();
    assert!(
        !preflight
            .headers()
            .contains_key(http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );
    let mut cross_origin = format!("{}/rpc/v2", host.replace("http:", "ws:"))
        .into_client_request()
        .unwrap();
    cross_origin.headers_mut().insert(
        http::header::ORIGIN,
        "https://unrelated.example".parse().unwrap(),
    );
    let rejected = tokio_tungstenite::connect_async(cross_origin)
        .await
        .unwrap_err();
    assert!(
        matches!(rejected, tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == StatusCode::FORBIDDEN)
    );
    let response: serde_json::Value = reqwest::Client::new()
        .post(format!("{host}/rpc/v1"))
        .json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"state","params":{}}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(response["result"], "initializing", "{response}");
    let progress: crate::init::InitProgressRes = from_value(
        cli.call_remote::<InitContext>("init.subscribe", imbl_value::json!({}))
            .await
            .unwrap(),
    )
    .unwrap();
    let mut continuation = cli
        .ws_continuation::<InitContext>(progress.guid.unwrap())
        .await
        .unwrap();
    let snapshot: crate::progress::FullProgress =
        crate::util::net::tungstenite::next_json(&mut continuation)
            .await
            .unwrap();
    assert!(!snapshot.overall.is_complete());
    drop(continuation);
    let mut request = format!("{}/rpc/v2", host.replace("http:", "ws:"))
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert(http::header::HOST, "unapproved.example".parse().unwrap());
    let rejected = tokio_tungstenite::connect_async(request).await.unwrap_err();
    assert!(
        matches!(rejected, tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == StatusCode::FORBIDDEN)
    );
    assert!(
        ctx.sig_context(&http::Extensions::new())
            .await
            .into_iter()
            .next()
            .unwrap()
            .is_err()
    );
}

#[tokio::test]
async fn diagnostic_cli_waits_for_the_actual_terminal_without_loading_a_device_key() {
    let ctx = context().await;
    let initialized = Arc::new(Notify::new());
    let finalize = Arc::new(Notify::new());
    let api = rpc_toolkit::ParentHandler::<InitContext>::new().subcommand(
        "diagnostic",
        rpc_toolkit::ParentHandler::<InitContext>::new().subcommand(
            "update",
            from_fn_async({
                let initialized = initialized.clone();
                let finalize = finalize.clone();
                move |args: HandlerArgs<
                    InitContext,
                    Empty,
                    Empty,
                    crate::progress::FullProgress,
                >| {
                    let initialized = initialized.clone();
                    let finalize = finalize.clone();
                    async move {
                        let tracker = crate::progress::FullProgressTracker::new();
                        tracker.complete();
                        let stream = args.rpc_stream.unwrap();
                        let source =
                            crate::progress::progress_until_complete(tracker, async move {
                                finalize.notified().await;
                                Err(Error::new(
                                    eyre!("finalization failed"),
                                    ErrorKind::Filesystem,
                                ))
                            });
                        tokio::spawn(async move {
                            let result = stream.forward(source).await;
                            let _ = stream.close(result);
                        });
                        initialized.notify_one();
                        Ok::<_, Error>(serde_json::json!({"target":"0.4.3","progress":null}))
                    }
                }
            })
            .no_cli()
            .no_ts(),
        ),
    );
    let factory = ctx.clone();
    let http = InitContext::middleware(Server::new(
        move || {
            let ctx = factory.clone();
            async move { Ok(ctx) }
        },
        api,
    ));
    let router = session_rpc_router(ctx, http.clone(), axum::routing::any(http));
    let (host, _server) = server(local_hosts_only(router)).await;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("malformed-key");
    tokio::fs::write(&path, "not a key").await.unwrap();
    let cli = CliContext::init(
        ClientConfig::try_parse_from([
            "test",
            "-H",
            &host,
            "--id-key-path",
            path.to_str().unwrap(),
        ])
        .unwrap(),
    )
    .unwrap();
    let args = HandlerArgs {
        context: cli.clone(),
        parent_method: ["diagnostic"].into(),
        method: ["update"].into(),
        params: serde_json::from_value::<crate::update::diagnostic::DiagnosticUpdateSystemParams>(
            serde_json::json!({"progress":true}),
        )
        .unwrap(),
        inherited_params: Empty {},
        raw_params: imbl_value::json!({"progress":true}),
        rpc_stream: None,
    };
    let mut operation = Box::pin(crate::update::diagnostic::cli_update_system(args));
    tokio::time::timeout(Duration::from_secs(3), async {
        tokio::select! { result = &mut operation => panic!("completed without its terminal: {:?}", result), _ = initialized.notified() => () }
    }).await.unwrap();
    assert!(futures::poll!(operation.as_mut()).is_pending());
    finalize.notify_one();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), operation)
            .await
            .unwrap()
            .unwrap_err()
            .kind,
        ErrorKind::Filesystem
    );
    assert!(cli.id_key.get().is_none());
}

#[tokio::test]
async fn startup_replay_capability_does_not_keep_its_context_alive() {
    let ctx = context().await;
    let replay = ctx.replay_check();
    drop(ctx);
    assert!(
        replay
            .check(crate::sign::commitment::request::RequestCommitment::for_bytes(b"hello"))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn router_replacement_cancels_two_active_streams_pending_initialization_and_pending_admission()
 {
    let ctx = context().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let changed = Arc::new(Notify::new());
    let initializing = Arc::new(Notify::new());
    let api = crate::main_api::<InitContext>()
        .subcommand(
            "watch",
            from_fn_async({
                let calls = calls.clone();
                let drops = drops.clone();
                let changed = changed.clone();
                move |args: HandlerArgs<InitContext, Empty, Empty, u64>| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    let drops = drops.clone();
                    let changed = changed.clone();
                    async move {
                        let dropped = GeneralGuard::new(move || {
                            drops.fetch_add(1, Ordering::SeqCst);
                            changed.notify_one();
                        });
                        let stream = args.rpc_stream.unwrap();
                        tokio::spawn(async move {
                            let _dropped = dropped;
                            let result = stream
                                .forward(futures::stream::pending::<Result<u64, Error>>())
                                .await;
                            let _ = stream.close(result);
                        });
                        Ok::<_, Error>(1u64)
                    }
                }
            })
            .no_cli()
            .no_ts(),
        )
        .subcommand(
            "wait",
            from_fn_async({
                let calls = calls.clone();
                let drops = drops.clone();
                let changed = changed.clone();
                let initializing = initializing.clone();
                move |_args: HandlerArgs<InitContext, Empty, Empty, u64>| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    let drops = drops.clone();
                    let changed = changed.clone();
                    let initializing = initializing.clone();
                    async move {
                        let _dropped = GeneralGuard::new(move || {
                            drops.fetch_add(1, Ordering::SeqCst);
                            changed.notify_one();
                        });
                        initializing.notify_one();
                        futures::future::pending::<()>().await;
                        Ok::<_, Error>(())
                    }
                }
            })
            .no_cli()
            .no_ts(),
        );
    let factory = ctx.clone();
    let http = InitContext::middleware(Server::new(
        move || {
            let ctx = factory.clone();
            async move { Ok(ctx) }
        },
        api,
    ));
    let router = session_rpc_router(ctx, http.clone(), axum::routing::any(http));
    let (host, mut server) = server(local_hosts_only(router)).await;
    let url: url::Url = format!("{}/rpc/v2", host.replace("http:", "ws:"))
        .parse()
        .unwrap();
    let (mut admission, _) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .unwrap();
    let client = crate::middleware::auth::session::client::SessionClient::new(
        reqwest::Client::builder().http1_only().build().unwrap(),
    );
    let signer = AnySigningKey::generate().into();
    let mut first = client
        .stream::<u64, ()>(
            &signer,
            url.clone(),
            "127.0.0.1",
            http::HeaderMap::new(),
            "watch",
            imbl_value::json!({}),
        )
        .await
        .unwrap();
    let mut second = client
        .stream::<u64, ()>(
            &signer,
            url.clone(),
            "127.0.0.1",
            http::HeaderMap::new(),
            "watch",
            imbl_value::json!({}),
        )
        .await
        .unwrap();
    let mut pending = Box::pin(client.stream::<u64, ()>(
        &signer,
        url.clone(),
        "127.0.0.1",
        http::HeaderMap::new(),
        "wait",
        imbl_value::json!({}),
    ));
    tokio::time::timeout(Duration::from_secs(3), async {
        tokio::select! { result = &mut pending => panic!("initialization unexpectedly ended: {:?}", result.err()), _ = initializing.notified() => () }
    }).await.unwrap();
    server.serve_router(ui_router(context().await));
    assert!(
        tokio::time::timeout(Duration::from_secs(3), first.items.next())
            .await
            .unwrap()
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(3), second.items.next())
            .await
            .unwrap()
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(3), pending)
            .await
            .unwrap()
            .is_err()
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        while drops.load(Ordering::SeqCst) != 3 {
            changed.notified().await;
        }
    })
    .await
    .unwrap();
    let admission = tokio::time::timeout(Duration::from_secs(3), admission.next())
        .await
        .unwrap();
    assert!(!matches!(
        admission,
        Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(_)))
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(
        client
            .call(
                &signer,
                url,
                "127.0.0.1",
                http::HeaderMap::new(),
                "state",
                imbl_value::json!({})
            )
            .await
            .unwrap()
            .as_str(),
        Some("initializing")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}
