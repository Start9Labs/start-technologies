use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::TryStreamExt;
use pkcs8::EncodePrivateKey;
use rpc_toolkit::{HandlerExt, ParentHandler, Server, from_fn_async};
use tokio::io::AsyncWriteExt;

use super::AttachParams;
use crate::context::InitContext;
use crate::context::config::ServerConfig;
use crate::middleware::auth::session::SessionContext;
use crate::net::dns::DnsController;
use crate::net::static_server::{UiContext, local_hosts_only, session_rpc_router};
use crate::net::web_server::{Acceptor, WebServer};
use crate::prelude::*;
use crate::rpc_continuations::{Guid, RpcContinuation, WebSocketPolicy};
use crate::util::sync::Watch;

#[derive(Clone, Copy, Debug)]
enum Outcome {
    Exit(i32),
    NormalClose,
    ErrorClose,
    Lost,
    InvalidExit,
}

async fn serve_protocol_peer(
    outcome: Outcome,
) -> (
    String,
    WebServer<tokio::net::TcpListener>,
    Arc<Mutex<Vec<OsString>>>,
) {
    let dns = Arc::new(DnsController::new(Watch::new(Default::default())));
    let context = InitContext::init(&ServerConfig::default(), dns)
        .await
        .unwrap();
    let command = Arc::new(Mutex::new(Vec::new()));
    let observed = command.clone();
    let handler = from_fn_async(move |ctx: InitContext, params: AttachParams| {
        *observed.lock().unwrap() = params.command;
        async move {
            let guid = Guid::new();
            ctx.rpc_continuations
                .add(
                    guid.clone(),
                    RpcContinuation::ws(
                        WebSocketPolicy::Public,
                        ctx.secure_session().cloned(),
                        move |mut ws| async move {
                            use axum::extract::ws::{CloseFrame, Message};
                            let mut input = Vec::new();
                            while let Some(message) = ws.try_next().await.unwrap() {
                                if let Message::Binary(bytes) = message {
                                    input.extend_from_slice(&bytes);
                                    if input.len() >= b"probe".len() {
                                        break;
                                    }
                                }
                            }
                            assert_eq!(input, b"probe");
                            ws.send(Message::Text("stdout".into())).await.unwrap();
                            ws.send(Message::Binary(b"peer-out\0\xff".to_vec().into()))
                                .await
                                .unwrap();
                            ws.send(Message::Text("stderr".into())).await.unwrap();
                            ws.send(Message::Binary(b"peer-err\0\xfe".to_vec().into()))
                                .await
                                .unwrap();
                            match outcome {
                                Outcome::Exit(code) => {
                                    ws.send(Message::Text("exit".into())).await.unwrap();
                                    ws.send(Message::Binary(code.to_be_bytes().to_vec().into()))
                                        .await
                                        .unwrap();
                                    let _ = ws.normal_close("exit").await;
                                }
                                Outcome::NormalClose => ws.normal_close("exit").await.unwrap(),
                                Outcome::ErrorClose => ws
                                    .send(Message::Close(Some(CloseFrame {
                                        code: 1011,
                                        reason: "controlled attach failure".into(),
                                    })))
                                    .await
                                    .unwrap(),
                                Outcome::Lost => (),
                                Outcome::InvalidExit => {
                                    ws.send(Message::Text("exit".into())).await.unwrap();
                                    ws.send(Message::Binary(vec![1, 2, 3].into()))
                                        .await
                                        .unwrap();
                                }
                            }
                        },
                        Duration::from_secs(30),
                    ),
                )
                .await;
            Ok::<_, Error>(guid)
        }
    })
    .no_cli()
    .no_ts();
    let api = ParentHandler::<InitContext>::new().subcommand(
        "package",
        ParentHandler::<InitContext>::new().subcommand("attach", handler),
    );
    let factory = context.clone();
    let http = InitContext::middleware(Server::new(
        move || {
            let ctx = factory.clone();
            async move { Ok(ctx) }
        },
        api,
    ));
    let router = local_hosts_only(session_rpc_router(
        context,
        http,
        axum::routing::any(|| async { http::StatusCode::FORBIDDEN }),
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    (
        host,
        WebServer::new(Acceptor::new(listener), router),
        command,
    )
}

#[tokio::test]
#[ignore = "Requires START_CLI_BIN pointing to the freshly built CLI"]
async fn actual_attach_cli_requires_exit_and_cancels_open_stdin() {
    let binary = std::env::var("START_CLI_BIN").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let key = crate::sign::AnySigningKey::generate();
    let path = temp.path().join("id.pem");
    std::fs::write(
        &path,
        key.to_pkcs8_pem(Default::default()).unwrap().as_bytes(),
    )
    .unwrap();
    let argument = OsString::from_vec(b"native-argument-\xff".to_vec());
    for outcome in [
        Outcome::Exit(0),
        Outcome::Exit(7),
        Outcome::NormalClose,
        Outcome::ErrorClose,
        Outcome::Lost,
        Outcome::InvalidExit,
    ] {
        let (host, _peer, command) = serve_protocol_peer(outcome).await;
        let mut child = tokio::process::Command::new(&binary)
            .current_dir(temp.path())
            .env("HOME", temp.path())
            .args([
                "--host",
                &host,
                "--id-key-path",
                path.to_str().unwrap(),
                "package",
                "attach",
                "demo",
                "--name",
                "fixture",
                "--",
                "echo",
            ])
            .arg(&argument)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        input.write_all(b"probe").await.unwrap();
        let output = tokio::time::timeout(Duration::from_secs(15), child.wait_with_output()).await;
        drop(input);
        let output = output.expect("attach hung with open stdin").unwrap();
        assert_eq!(
            *command.lock().unwrap(),
            vec![OsString::from("echo"), argument.clone()]
        );
        let expected = match outcome {
            Outcome::Exit(code) => code,
            Outcome::InvalidExit => ErrorKind::InvalidRequest as i32,
            _ => ErrorKind::Network as i32,
        };
        assert_eq!(
            output.status.code(),
            Some(expected),
            "{outcome:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output
                .stdout
                .windows(b"peer-out\0\xff".len())
                .any(|bytes| bytes == b"peer-out\0\xff")
        );
        assert!(
            output
                .stderr
                .windows(b"peer-err\0\xfe".len())
                .any(|bytes| bytes == b"peer-err\0\xfe")
        );
        if matches!(outcome, Outcome::ErrorClose) {
            assert!(String::from_utf8_lossy(&output.stderr).contains("controlled attach failure"));
        }
    }
}
