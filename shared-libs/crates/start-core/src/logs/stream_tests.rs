use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use clap::Parser;
use http::HeaderMap;
use rpc_toolkit::{HandlerExt, ParentHandler, Server};

use super::result_tests::{ENTRY, child};
use super::*;
use crate::context::InitContext;
use crate::context::config::{ClientConfig, ServerConfig};
use crate::net::dns::DnsController;
use crate::net::static_server::{UiContext, local_hosts_only, session_rpc_router};
use crate::net::web_server::{Acceptor, WebServer};
use crate::util::sync::Watch;

async fn serve(script: String) -> (String, WebServer<tokio::net::TcpListener>, Arc<AtomicU32>) {
    let dns = Arc::new(DnsController::new(Watch::new(Default::default())));
    let context = InitContext::init(&ServerConfig::default(), dns)
        .await
        .unwrap();
    let pid = Arc::new(AtomicU32::new(0));
    let process_id = pid.clone();
    let handler = from_fn_async(
        move |args: HandlerArgs<InitContext, Empty, Empty, LogEntry>| {
            let script = script.clone();
            let pid = process_id.clone();
            async move {
                let child = child(&script);
                pid.store(child.id().unwrap(), Ordering::SeqCst);
                let source = LogStream::follow(child, vec![])?;
                PreparedLogFollow::new(source)
                    .await?
                    .subscribe(args.context, None, WebSocketPolicy::Public, args.rpc_stream)
                    .await
            }
        },
    )
    .no_cli()
    .no_ts();
    let api = ParentHandler::<InitContext>::new().subcommand(
        "init",
        ParentHandler::<InitContext>::new().subcommand(
            "logs",
            ParentHandler::<InitContext>::new().subcommand("follow", handler),
        ),
    );
    let factory = context.clone();
    let http = InitContext::middleware(Server::new(
        move || {
            let context = factory.clone();
            async move { Ok(context) }
        },
        api,
    ));
    let router = local_hosts_only(session_rpc_router(
        context,
        http.clone(),
        axum::routing::any(http),
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    (host, WebServer::new(Acceptor::new(listener), router), pid)
}

fn script(failed: bool) -> String {
    format!(
        "printf '%s\\n' '{ENTRY}' '{}'; exit {}",
        ENTRY
            .replace("last entry", "next entry")
            .replace("cursor-a", "cursor-b"),
        if failed { 7 } else { 0 }
    )
}

#[tokio::test]
async fn encrypted_typed_logs_preserve_initial_cursor_first_entry_and_actual_terminal() {
    for failed in [false, true] {
        let (host, _server, _) = serve(script(failed)).await;
        let cli =
            CliContext::init(ClientConfig::try_parse_from(["test", "-H", &host]).unwrap()).unwrap();
        let mut response = cli
            .call_startup_stream::<LogEntry, ()>(
                "init.logs.follow",
                imbl_value::json!({}),
                HeaderMap::new(),
            )
            .await
            .unwrap();
        assert_eq!(response.initial["startCursor"].as_str(), Some("cursor-a"));
        assert!(response.initial["guid"].is_null());
        assert_eq!(
            response.items.next().await.unwrap().unwrap().message,
            "last entry"
        );
        assert_eq!(
            response.items.next().await.unwrap().unwrap().message,
            "next entry"
        );
        let terminal = response.items.finish().await;
        if failed {
            assert_eq!(terminal.unwrap_err().code, ErrorKind::Journald as i32)
        } else {
            terminal.unwrap()
        }
    }
}

#[tokio::test]
async fn encrypted_ordinary_logs_keep_a_guid_and_report_the_same_process_result() {
    for failed in [false, true] {
        let (host, _server, _) = serve(script(failed)).await;
        let cli =
            CliContext::init(ClientConfig::try_parse_from(["test", "-H", &host]).unwrap()).unwrap();
        let value = cli
            .call_remote::<InitContext>("init.logs.follow", imbl_value::json!({}))
            .await
            .unwrap();
        let response: LogFollowResponse = from_value(value).unwrap();
        assert_eq!(response.start_cursor.as_deref(), Some("cursor-a"));
        let mut socket = cli
            .ws_continuation::<InitContext>(response.guid.unwrap())
            .await
            .unwrap();
        for message in ["last entry", "next entry"] {
            let text = crate::util::net::tungstenite::next_text(&mut socket)
                .await
                .unwrap()
                .unwrap();
            let entry: LogEntry = serde_json::from_str(&text).unwrap();
            assert_eq!(entry.message, message);
        }
        let terminal = crate::util::net::tungstenite::next_text(&mut socket).await;
        if failed {
            assert!(terminal.is_err())
        } else {
            assert!(terminal.unwrap().is_none())
        }
    }
}

#[tokio::test]
async fn encrypted_log_observer_disposal_stops_its_process() {
    let (host, _server, pid) = serve(format!("printf '%s\\n' '{ENTRY}'; exec sleep 300")).await;
    let cli =
        CliContext::init(ClientConfig::try_parse_from(["test", "-H", &host]).unwrap()).unwrap();
    let mut response = cli
        .call_startup_stream::<LogEntry, ()>(
            "init.logs.follow",
            imbl_value::json!({}),
            HeaderMap::new(),
        )
        .await
        .unwrap();
    assert!(response.items.next().await.unwrap().is_some());
    drop(response);
    let pid = nix::unistd::Pid::from_raw(pid.load(Ordering::SeqCst) as i32);
    tokio::time::timeout(Duration::from_secs(5), async {
        while nix::sys::signal::kill(pid, None).is_ok() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "Requires START_CLI_BIN pointing to the built CLI"]
async fn actual_log_cli_uses_startup_admission_and_the_process_terminal() {
    let binary = std::env::var("START_CLI_BIN").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let key = temp.path().join("malformed.pem");
    tokio::fs::write(&key, "not a key").await.unwrap();
    let backlog = (0..64)
        .map(|n| ENTRY.replace("last entry", &format!("backlog-{n:03}")))
        .map(|entry| format!("'{entry}'"))
        .collect::<Vec<_>>()
        .join(" ");
    for failed in [false, true] {
        let (host, _server, _) = serve(format!(
            "printf '%s\\n' {backlog}; exit {}",
            if failed { 7 } else { 0 }
        ))
        .await;
        let output = tokio::time::timeout(
            Duration::from_secs(15),
            tokio::process::Command::new(&binary)
                .current_dir(temp.path())
                .env("HOME", temp.path())
                .args([
                    "--host",
                    &host,
                    "--id-key-path",
                    key.to_str().unwrap(),
                    "--id-key-gpg",
                    "unavailable-log-fixture",
                    "init",
                    "logs",
                    "--follow",
                ])
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            output.status.success(),
            !failed,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for n in 0..64 {
            assert!(String::from_utf8_lossy(&output.stdout).contains(&format!("backlog-{n:03}")));
        }
        if failed {
            assert_eq!(output.status.code(), Some(ErrorKind::Journald as i32));
        }
    }
    assert_eq!(tokio::fs::read_to_string(key).await.unwrap(), "not a key");
}
