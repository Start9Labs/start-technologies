use std::cmp::max;
use std::ffi::OsString;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use color_eyre::eyre::eyre;
use futures::FutureExt;
use rust_i18n::t;
use tokio::signal::unix::signal;
use tokio::sync::oneshot;
use tracing::instrument;

use crate::context::config::ServerConfig;
use crate::context::rpc::InitRpcContextPhases;
use crate::context::{DiagnosticContext, InitContext, RpcContext};
use crate::net::dns::DnsController;
use crate::net::gateway::WildcardListener;
use crate::net::static_server::refresher;
use crate::net::web_server::{Acceptor, WebServer};
use crate::prelude::*;
use crate::shutdown::Shutdown;
use crate::system::launch_metrics_task;
use crate::util::cleanup::wait_for_filesystem_cleanup;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::append_file;
use crate::util::logger::LOGGER;

/// Joins metrics cancellation before background and service teardown can unmount storage.
async fn wait_for_shutdown(
    mut requests: oneshot::Receiver<Option<Shutdown>>,
    mut metrics_task: NonDetachingJoinHandle<()>,
) -> Result<Option<Shutdown>, Error> {
    tokio::select! {
        shutdown = &mut requests => {
            let _ = metrics_task.wait_for_abort().await;
            shutdown
        }
        result = &mut metrics_task => {
            result.map_err(|e| Error::new(
                eyre!("{}", e).wrap_err(t!("bins.startd.metrics-daemon-panicked").to_string()),
                ErrorKind::Unknown,
            ))?;
            tracing::debug!("{}", t!("bins.startd.metrics-daemon-shutdown"));
            requests.await
        }
    }
    .with_kind(ErrorKind::Unknown)
}

#[instrument(skip_all)]
async fn inner_main(
    server: &mut WebServer<WildcardListener>,
    dns: &Arc<DnsController>,
    config: &ServerConfig,
) -> Result<Option<Shutdown>, Error> {
    let rpc_ctx = if !tokio::fs::metadata("/run/startos/initialized")
        .await
        .is_ok()
    {
        LOGGER.set_logfile(Some(
            append_file("/run/startos/init.log").await?.into_std().await,
        ));
        let (ctx, handle) = match super::start_init::main(server, dns.clone(), &config).await? {
            Err(s) => return Ok(Some(s)),
            Ok(ctx) => ctx,
        };
        tokio::fs::write("/run/startos/initialized", "").await?;

        server.serve_ui_for(ctx.clone());
        LOGGER.set_logfile(None);
        handle.complete();

        ctx
    } else {
        let init_ctx = InitContext::init(config, dns.clone()).await?;
        let handle = init_ctx.progress.clone();
        let rpc_ctx_phases = InitRpcContextPhases::new(&handle);
        server.serve_ui_for(init_ctx.clone());

        let result = async {
            RpcContext::init(
                &server.acceptor_setter(),
                dns.clone(),
                config,
                InternedString::intern(
                    tokio::fs::read_to_string("/media/startos/config/disk.guid")
                        .await?
                        .trim(),
                ),
                None,
                rpc_ctx_phases,
            )
            .await
        }
        .await;
        init_ctx
            .complete_and_retire(result.as_ref().map(|_| ()).map_err(Error::clone_output))
            .await;
        let ctx = result?;

        server.serve_ui_for(ctx.clone());

        ctx
    };

    let requests = rpc_ctx
        .shutdown_recv
        .replace(None)
        .expect("startd owns the shutdown receiver");
    let result = async {
        crate::hostname::sync_hostname(&rpc_ctx.account.peek(|a| a.hostname.clone())).await?;

        let shutdown_send = rpc_ctx.shutdown.clone();
        let sig_handler = NonDetachingJoinHandle::from(tokio::spawn(async move {
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
            crate::shutdown::send_shutdown(&shutdown_send, None).log_err();
        }));

        let metrics_ctx = rpc_ctx.clone();
        let metrics_task = NonDetachingJoinHandle::from(tokio::spawn(async move {
            launch_metrics_task(&metrics_ctx.metrics_cache, &metrics_ctx.db).await
        }));

        let result = wait_for_shutdown(requests, metrics_task).await;
        let _ = sig_handler.wait_for_abort().await;
        result
    }
    .await;
    server.reset_router(refresher()).await;
    let teardown = rpc_ctx.shutdown().await;
    match result {
        Ok(shutdown) => {
            teardown?;
            Ok(shutdown)
        }
        Err(error) => {
            teardown.log_err();
            Err(error)
        }
    }
}

pub fn main(args: impl IntoIterator<Item = OsString>) {
    LOGGER.enable();

    let config = ServerConfig::parse_from(args).load().unwrap();

    let res = {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(max(1, num_cpus::get()))
            .enable_all()
            .build()
            .expect(&t!("bins.startd.failed-to-initialize-runtime"));
        let res = rt.block_on(async {
            // Periodically wake a worker thread from a non-tokio OS thread to
            // prevent tokio I/O driver starvation (all workers parked on
            // condvar with no driver).  See tokio-rs/tokio#4730.
            let rt_handle = tokio::runtime::Handle::current();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_secs(30));
                    rt_handle.spawn(async {});
                }
            });

            let listener = WildcardListener::new(80)?;
            let dns = Arc::new(DnsController::new(listener.subscribe()));
            let mut server = WebServer::new(Acceptor::new(listener), refresher());
            let result = match inner_main(&mut server, &dns, &config).await {
                Ok(shutdown) => Ok(shutdown),
                Err(e) => {
                    async {
                        tracing::error!("{e}");
                        tracing::debug!("{e:?}");
                        let ctx = DiagnosticContext::init(
                            &config,
                            if tokio::fs::metadata("/media/startos/config/disk.guid")
                                .await
                                .is_ok()
                            {
                                Some(InternedString::intern(
                                    tokio::fs::read_to_string("/media/startos/config/disk.guid")
                                        .await?
                                        .trim(),
                                ))
                            } else {
                                None
                            },
                            e,
                            dns.clone(),
                        )
                        .await?;

                        server.serve_ui_for(ctx.clone());

                        let mut shutdown = ctx.shutdown.subscribe();

                        let shutdown =
                            shutdown.recv().await.with_kind(crate::ErrorKind::Unknown)?;

                        Ok::<_, Error>(Some(shutdown))
                    }
                    .await
                }
            };
            server.shutdown().await;
            wait_for_filesystem_cleanup().await;
            result
        });
        rt.shutdown_timeout(Duration::from_millis(100));
        res
    };

    match res {
        Ok(None) => (),
        Ok(Some(s)) => s.execute(),
        Err(e) => {
            eprintln!("{}", e.source);
            tracing::debug!("{:?}", e.source);
            drop(e.source);
            std::process::exit(e.kind as i32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::future::DropSignaling;

    #[tokio::test]
    async fn shutdown_does_not_wait_for_metrics_to_observe_the_request() {
        let (send, requests) = oneshot::channel();
        send.send(Some(Shutdown {
            disk_guid: None,
            restart: true,
        }))
        .unwrap();
        let metrics = DropSignaling::new(std::future::pending::<()>());
        let mut metrics_stopped = metrics.subscribe();
        let metrics_task = NonDetachingJoinHandle::from(tokio::spawn(metrics));

        let shutdown = tokio::time::timeout(
            Duration::from_secs(1),
            wait_for_shutdown(requests, metrics_task),
        )
        .await
        .expect("shutdown waited for metrics polling")
        .unwrap()
        .unwrap();

        assert!(shutdown.restart);
        assert!(metrics_stopped.wait().now_or_never().is_some());
    }

    #[tokio::test]
    async fn metrics_panics_still_fail_the_shutdown_wait() {
        let (_send, requests) = oneshot::channel();
        let metrics_task = NonDetachingJoinHandle::from(tokio::spawn(async {
            panic!("metrics task failed");
        }));

        let error = wait_for_shutdown(requests, metrics_task).await.unwrap_err();

        assert!(matches!(error.kind, ErrorKind::Unknown));
    }
}
