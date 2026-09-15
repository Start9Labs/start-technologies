use std::cmp::max;
use std::ffi::OsString;
use std::time::Duration;

use clap::Parser;
use color_eyre::eyre::eyre;
use futures::FutureExt;
use rust_i18n::t;
use tokio::signal::unix::signal;
use tracing::instrument;

use crate::context::config::ServerConfig;
use crate::context::rpc::InitRpcContextPhases;
use crate::context::{DiagnosticContext, InitContext, RpcContext};
use crate::net::gateway::WildcardListener;
use crate::net::static_server::refresher;
use crate::net::web_server::{Acceptor, WebServer};
use crate::prelude::*;
use crate::shutdown::Shutdown;
use crate::system::launch_metrics_task;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::append_file;
use crate::util::logger::LOGGER;

fn startd_task_result(
    result: Result<(), tokio::task::JoinError>,
    cancellation_is_success: bool,
    panic_message: String,
) -> Result<(), Error> {
    match result {
        Ok(()) => Ok(()),
        Err(error) if cancellation_is_success && error.is_cancelled() => Ok(()),
        Err(error) => Err(Error::new(
            eyre!("{error}").wrap_err(panic_message),
            ErrorKind::Unknown,
        )),
    }
}

#[instrument(skip_all)]
async fn inner_main(
    server: &mut WebServer<WildcardListener>,
    config: &ServerConfig,
) -> Result<Option<Shutdown>, Error> {
    let rpc_ctx = if !tokio::fs::metadata("/run/startos/initialized")
        .await
        .is_ok()
    {
        LOGGER.set_logfile(Some(
            append_file("/run/startos/init.log").await?.into_std().await,
        ));
        let (ctx, handle) = match super::start_init::main(server, &config).await? {
            Err(s) => return Ok(Some(s)),
            Ok(ctx) => ctx,
        };
        if let Err(error) = tokio::fs::write("/run/startos/initialized", "").await {
            ctx.shutdown().await.log_err();
            return Err(error.into());
        }

        server.serve_ui_for(ctx.clone());
        LOGGER.set_logfile(None);
        handle.complete();

        ctx
    } else {
        let init_ctx = InitContext::init(config).await?;
        let handle = init_ctx.progress.clone();
        let rpc_ctx_phases = InitRpcContextPhases::new(&handle);
        server.serve_ui_for(init_ctx);

        let ctx = RpcContext::init(
            &server.acceptor_setter(),
            config,
            InternedString::intern(
                tokio::fs::read_to_string("/media/startos/config/disk.guid") // unique identifier for volume group - keeps track of the disk that goes with your embassy
                    .await?
                    .trim(),
            ),
            None,
            rpc_ctx_phases,
        )
        .await?;

        server.serve_ui_for(ctx.clone());
        handle.complete();

        ctx
    };

    let shutdown = async {
        crate::hostname::sync_hostname(&rpc_ctx.account.peek(|a| a.hostname.clone())).await?;

        let mut shutdown_recv = rpc_ctx.shutdown.subscribe();

        let sig_handler_ctx = rpc_ctx.clone();
        let mut sig_handler: Option<NonDetachingJoinHandle<()>> = Some(
            tokio::spawn(async move {
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
            .into(),
        );

        let metrics_ctx = rpc_ctx.clone();
        let mut metrics_task: Option<NonDetachingJoinHandle<()>> = Some(
            tokio::spawn(async move {
                launch_metrics_task(&metrics_ctx.metrics_cache, || {
                    metrics_ctx.shutdown.subscribe()
                })
                .await
            })
            .into(),
        );

        enum Event {
            Shutdown(
                Result<Option<crate::shutdown::Shutdown>, tokio::sync::broadcast::error::RecvError>,
            ),
            Metrics(Result<(), tokio::task::JoinError>),
            Signal(Result<(), tokio::task::JoinError>),
        }

        let event = tokio::select! {
            shutdown = shutdown_recv.recv() => Event::Shutdown(shutdown),
            result = metrics_task.as_mut().expect("metrics task") => {
                metrics_task.take();
                Event::Metrics(result)
            }
            result = sig_handler.as_mut().expect("signal task") => {
                sig_handler.take();
                Event::Signal(result)
            }
        };
        let result = match event {
            Event::Shutdown(shutdown) => shutdown.with_kind(crate::ErrorKind::Unknown),
            Event::Metrics(metrics) => match startd_task_result(
                metrics,
                false,
                t!("bins.startd.metrics-daemon-panicked").to_string(),
            ) {
                Ok(()) => {
                    tracing::debug!("{}", t!("bins.startd.metrics-daemon-shutdown"));
                    shutdown_recv
                        .recv()
                        .await
                        .with_kind(crate::ErrorKind::Unknown)
                }
                Err(error) => Err(error),
            },
            Event::Signal(signal) => match startd_task_result(
                signal,
                false,
                t!("bins.startd.signal-handler-panicked").to_string(),
            ) {
                Ok(()) => shutdown_recv
                    .recv()
                    .await
                    .with_kind(crate::ErrorKind::Unknown),
                Err(error) => Err(error),
            },
        };

        let (metrics_result, signal_result) = tokio::join!(
            async {
                match metrics_task {
                    Some(task) => startd_task_result(
                        task.wait_for_abort().await,
                        true,
                        t!("bins.startd.metrics-daemon-panicked").to_string(),
                    ),
                    None => Ok(()),
                }
            },
            async {
                match sig_handler {
                    Some(task) => startd_task_result(
                        task.wait_for_abort().await,
                        true,
                        t!("bins.startd.signal-handler-panicked").to_string(),
                    ),
                    None => Ok(()),
                }
            }
        );
        let tasks_result = match metrics_result {
            Ok(()) => signal_result,
            Err(error) => {
                signal_result.log_err();
                Err(error)
            }
        };
        match result {
            Ok(shutdown) => {
                tasks_result?;
                Ok(shutdown)
            }
            Err(error) => {
                tasks_result.log_err();
                Err(error)
            }
        }
    }
    .await;
    let cleanup = rpc_ctx.shutdown().await;
    match shutdown {
        Ok(shutdown) => {
            cleanup?;
            Ok(shutdown)
        }
        Err(error) => {
            cleanup.log_err();
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

            let mut server = WebServer::new(Acceptor::new(WildcardListener::new(80)?), refresher());
            match inner_main(&mut server, &config).await {
                Ok(a) => {
                    server.shutdown().await?;
                    Ok(a)
                }
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
                                    tokio::fs::read_to_string("/media/startos/config/disk.guid") // unique identifier for volume group - keeps track of the disk that goes with your embassy
                                        .await?
                                        .trim(),
                                ))
                            } else {
                                None
                            },
                            e,
                        )?;

                        server.serve_ui_for(ctx.clone());

                        let mut shutdown = ctx.shutdown.subscribe();

                        let shutdown =
                            shutdown.recv().await.with_kind(crate::ErrorKind::Unknown)?;

                        server.shutdown().await?;

                        Ok::<_, Error>(Some(shutdown))
                    }
                    .await
                }
            }
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
