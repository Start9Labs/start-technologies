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

#[instrument(skip_all)]
async fn inner_main(config: &TunnelConfig) -> Result<Option<bool>, Error> {
    let mut shutdown = None;

    let server = async {
        let ctx = TunnelContext::init(config).await?;
        let listen = ctx.listen;
        let server = WebServer::new(
            Acceptor::bind_map_dyn([(WebserverListener::Http, listen)]).await?,
            tunnel_router(ctx.clone()),
        );
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

        let mut shutdown_recv = ctx.shutdown.subscribe();

        let sig_handler_ctx = ctx;
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

        shutdown = shutdown_recv
            .recv()
            .await
            .with_kind(crate::ErrorKind::Unknown)?;

        sig_handler.wait_for_abort().await.with_kind(ErrorKind::Unknown)?;
        https_thread.wait_for_abort().await.with_kind(ErrorKind::Unknown)?;
        redirect_thread.wait_for_abort().await.with_kind(ErrorKind::Unknown)?;

        Ok::<_, Error>(server)
    }
    .await?;
    server.shutdown().await;

    Ok(shutdown)
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
fn export_manpage_start_tunnel() {
    // Pages live with the start-tunnel product; anchored to start-core's crate dir.
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../projects/start-tunnel/man"
    );
    std::fs::create_dir_all(dir).unwrap();
    clap_mangen::generate_to(app().into_command(), dir).unwrap();
}
