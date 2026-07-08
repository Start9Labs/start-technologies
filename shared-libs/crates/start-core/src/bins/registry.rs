use std::ffi::OsString;

use clap::Parser;
use futures::FutureExt;
use rpc_toolkit::CliApp;
use rust_i18n::t;
use tokio::signal::unix::signal;
use tracing::instrument;

use crate::context::CliContext;
use crate::context::config::ClientConfig;
use crate::net::web_server::{Acceptor, WebServer};
use crate::prelude::*;
use crate::registry::context::{RegistryConfig, RegistryContext};
use crate::registry::registry_router;
use crate::util::logger::LOGGER;

#[instrument(skip_all)]
async fn inner_main(config: &RegistryConfig) -> Result<(), Error> {
    let server = async {
        let ctx = RegistryContext::init(config).await?;
        let server = WebServer::new(
            Acceptor::bind([ctx.listen]).await?,
            registry_router(ctx.clone()),
        );

        let mut shutdown_recv = ctx.shutdown.subscribe();

        let sig_handler_ctx = ctx;
        let sig_handler = tokio::spawn(async move {
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
                .send(())
                .map_err(|_| ())
                .expect("send shutdown signal");
        });

        shutdown_recv
            .recv()
            .await
            .with_kind(crate::ErrorKind::Unknown)?;

        sig_handler.abort();

        Ok::<_, Error>(server)
    }
    .await?;
    server.shutdown().await;

    Ok(())
}

pub fn main(args: impl IntoIterator<Item = OsString>) {
    LOGGER.enable();

    let config = RegistryConfig::parse_from(args).load().unwrap();

    let res = {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect(&t!("bins.registry.failed-to-initialize-runtime"));
        rt.block_on(inner_main(&config))
    };

    match res {
        Ok(()) => (),
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
        crate::registry::registry_api(),
    )
    .mutate_command(super::translate_cli)
    .mutate_command(|cmd| cmd.name("start-registry").version(super::product_version()))
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
fn export_manpage_start_registry() {
    // Pages live with the start-registry product; anchored to start-core's crate dir.
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../projects/start-registry/man");
    std::fs::create_dir_all(dir).unwrap();
    clap_mangen::generate_to(app().into_command(), dir).unwrap();
}
