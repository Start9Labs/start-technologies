use std::collections::BTreeMap;
use std::ffi::OsString;
use std::net::SocketAddr;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use futures::FutureExt;
use patch_db::json_ptr::ROOT;
use rpc_toolkit::CliApp;
use rust_i18n::t;
use tokio::net::TcpListener;
use tokio::signal::unix::signal;
use tracing::instrument;
use visit_rs::Visit;

use crate::context::CliContext;
use crate::context::config::ClientConfig;
use crate::net::tls::TlsListener;
use crate::net::web_server::{Accept, Acceptor, MetadataVisitor, WebServer};
use crate::prelude::*;
use crate::tunnel::context::{TunnelConfig, TunnelContext};
use crate::tunnel::tunnel_router;
use crate::tunnel::web::TunnelCertHandler;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::logger::LOGGER;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum WebserverListener {
    Http,
    Https(SocketAddr),
}
impl<V: MetadataVisitor> Visit<V> for WebserverListener {
    fn visit(&self, visitor: &mut V) -> <V as visit_rs::Visitor>::Result {
        visitor.visit(self)
    }
}

const FORWARDING_TASK_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(15);

fn log_task_result(name: &str, result: Result<(), tokio::task::JoinError>) {
    if let Err(error) = result {
        if !error.is_cancelled() {
            tracing::error!("{name} task failed: {error}");
        }
    }
}

async fn await_task(name: &str, task: NonDetachingJoinHandle<()>) {
    log_task_result(name, task.await);
}

async fn stop_forwarding_tasks(tasks: [NonDetachingJoinHandle<()>; 3]) {
    let [mut pcp, mut igd, mut lease] = tasks;
    let mut pcp_done = false;
    let mut igd_done = false;
    let mut lease_done = false;
    let timeout = tokio::time::sleep(FORWARDING_TASK_SHUTDOWN_TIMEOUT);
    tokio::pin!(timeout);

    while !(pcp_done && igd_done && lease_done) {
        tokio::select! {
            biased;
            _ = &mut timeout => break,
            result = &mut pcp, if !pcp_done => {
                log_task_result("PCP", result);
                pcp_done = true;
            }
            result = &mut igd, if !igd_done => {
                log_task_result("IGD", result);
                igd_done = true;
            }
            result = &mut lease, if !lease_done => {
                log_task_result("lease", result);
                lease_done = true;
            }
        }
    }

    if pcp_done && igd_done && lease_done {
        return;
    }
    tracing::warn!(
        "forwarding servers did not stop within {FORWARDING_TASK_SHUTDOWN_TIMEOUT:?}; aborting"
    );
    if !pcp_done {
        pcp.abort();
    }
    if !igd_done {
        igd.abort();
    }
    if !lease_done {
        lease.abort();
    }
    if !pcp_done {
        log_task_result("PCP", pcp.await);
    }
    if !igd_done {
        log_task_result("IGD", igd.await);
    }
    if !lease_done {
        log_task_result("lease", lease.await);
    }
}

#[instrument(skip_all)]
async fn inner_main(config: &TunnelConfig) -> Result<Option<bool>, Error> {
    let listen = config
        .tunnel_listen
        .unwrap_or(crate::tunnel::TUNNEL_DEFAULT_LISTEN);
    let http_acceptor = Acceptor::bind_map_dyn([(WebserverListener::Http, listen)]).await?;
    let ctx = TunnelContext::init(config).await?;
    let mut shutdown_recv = ctx.shutdown.subscribe();
    let forwarding_threads = ctx.spawn_forwarding_servers();
    let server = WebServer::new(http_acceptor, tunnel_router(ctx.clone()));

    let shutdown = async {
        let acceptor_setter = server.acceptor_setter();
        let https_db = ctx.db.clone();
        let https_thread: NonDetachingJoinHandle<()> = tokio::spawn(async move {
            let mut sub = https_db.subscribe("/webserver".parse().unwrap()).await;
            while {
                while let Err(e) = async {
                    let webserver = https_db.peek().await.into_webserver();
                    if webserver.as_enabled().de()? {
                        let addr = webserver.as_listen().de()?.or_not_found("listen address")?;
                        acceptor_setter.send_if_modified(|a| {
                            let key = WebserverListener::Https(addr);
                            if !a.contains_key(&key) {
                                match (|| {
                                    Ok::<_, Error>(TlsListener::new(
                                        TcpListener::from_std(
                                            mio::net::TcpListener::bind(addr)
                                                .with_kind(ErrorKind::Network)?
                                                .into(),
                                        )
                                        .with_kind(ErrorKind::Network)?,
                                        TunnelCertHandler {
                                            db: https_db.clone(),
                                            crypto_provider: Arc::new(tokio_rustls::rustls::crypto::ring::default_provider()),
                                        },
                                    ))
                                })() {
                                    Ok(l) => {
                                        a.retain(|k, _| *k == WebserverListener::Http);
                                        a.insert(key, l.into_dyn());

                                        true
                                    }
                                    Err(e) => {
                                        tracing::error!("{}", t!("bins.tunnel.error-adding-ssl-listener", error = e.to_string()));
                                        tracing::debug!("{e:?}");

                                        false
                                    }
                                }
                            } else {
                                false
                            }
                        });
                    } else {
                        acceptor_setter.send_if_modified(|a| {
                            let before = a.len();
                            a.retain(|k, _| *k == WebserverListener::Http);
                            a.len() != before
                        });
                    }

                    Ok::<_, Error>(())
                }
                .await
                {
                    tracing::error!("{}", t!("bins.tunnel.error-updating-webserver-bind", error = e.to_string()));
                    tracing::debug!("{e:?}");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                sub.recv().await.is_some()
            } {}
        })
        .into();

        // Reconcile the per-IPv4 HTTP→HTTPS redirect listeners on every db
        // revision: the reactive analogue of the `/webserver` HTTPS block above.
        let redirect_ctx = ctx.clone();
        let redirect_thread: NonDetachingJoinHandle<()> = tokio::spawn(async move {
            let mut listeners: BTreeMap<SocketAddr, NonDetachingJoinHandle<()>> = BTreeMap::new();
            let mut sub = redirect_ctx.db.subscribe(ROOT.to_owned()).await;
            loop {
                if let Err(e) =
                    crate::tunnel::redirect::reconcile(&redirect_ctx, &mut listeners).await
                {
                    tracing::error!("error reconciling http redirects: {e}");
                    tracing::debug!("{e:?}");
                }
                if sub.recv().await.is_none() {
                    break;
                }
            }
        })
        .into();

        let sig_handler_ctx = ctx.clone();
        let sig_handler: NonDetachingJoinHandle<()> = tokio::spawn(async move {
            use tokio::signal::unix::SignalKind;
            futures::future::select_all(
                [
                    SignalKind::interrupt(),
                    SignalKind::quit(),
                    SignalKind::terminate(),
                ]
                .iter()
                .map(|s| {
                    async move {
                        signal(*s)
                            .unwrap_or_else(|_| panic!("register {:?} handler", s))
                            .recv()
                            .await
                    }
                    .boxed()
                }),
            )
            .await;
            sig_handler_ctx
                .shutdown
                .send(None)
                .map_err(|_| ())
                .expect("send shutdown signal");
        })
        .into();

        let shutdown = loop {
            match shutdown_recv.recv().await {
                Ok(shutdown) => break Ok(shutdown),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(error) => break Err(error).with_kind(crate::ErrorKind::Unknown),
            }
        };

        sig_handler.abort();
        https_thread.abort();
        redirect_thread.abort();

        await_task("signal", sig_handler).await;
        await_task("HTTPS", https_thread).await;
        await_task("redirect", redirect_thread).await;

        shutdown
    }
    .await;
    let server_result = server.shutdown().await;

    if let Err(error) = crate::net::forward::timeout_forwarding_drain(async {
        stop_forwarding_tasks(forwarding_threads).await;
        ctx.drain_forwarding().await
    })
    .await
    {
        tracing::error!("forwarding cleanup failed: {error}");
        tracing::debug!("{error:?}");
    }

    server_result?;
    shutdown
}

pub fn main(args: impl IntoIterator<Item = OsString>) {
    LOGGER.enable();

    let config = TunnelConfig::parse_from(args).load().unwrap();

    let res = {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect(&t!("bins.tunnel.failed-to-initialize-runtime"));
        rt.block_on(inner_main(&config))
    };

    match res {
        Ok(None) => (),
        Ok(Some(true)) => {
            Command::new("reboot").status().unwrap();
        }
        Ok(Some(false)) => {
            Command::new("poweroff").status().unwrap();
        }
        Err(e) => {
            eprintln!("{}", e.source);
            tracing::debug!("{:?}", e.source);
            drop(e.source);
            std::process::exit(e.kind as i32)
        }
    }
}

fn app() -> CliApp<CliContext, ClientConfig> {
    CliApp::new(
        |cfg: ClientConfig| Ok(CliContext::init(cfg.load()?)?),
        crate::tunnel::api::tunnel_api(),
    )
    .mutate_command(super::translate_cli)
    .mutate_command(|cmd| cmd.name("start-tunnel").version(super::product_version()))
}

pub fn cli(args: impl IntoIterator<Item = OsString>) {
    LOGGER.enable();

    if let Err(e) = app().run(args) {
        match e.data {
            Some(serde_json::Value::String(s)) => eprintln!("{}: {}", e.message, s),
            Some(serde_json::Value::Object(o)) => {
                if let Some(serde_json::Value::String(s)) = o.get("details") {
                    eprintln!("{}: {}", e.message, s);
                    if let Some(serde_json::Value::String(s)) = o.get("debug") {
                        tracing::debug!("{}", s)
                    }
                }
            }
            Some(a) => eprintln!("{}: {}", e.message, a),
            None => eprintln!("{}", e.message),
        }

        std::process::exit(e.code);
    }
}

#[test]
fn no_shadowed_args_start_tunnel() {
    super::assert_no_shadowed_args(app().into_command());
}

#[test]
fn export_manpage_start_tunnel() {
    // Pages live with the start-tunnel product; anchored to start-core's crate dir.
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../projects/start-tunnel/man"
    );
    std::fs::create_dir_all(dir).unwrap();
    clap_mangen::generate_to(app().into_command(), dir).unwrap();
}
