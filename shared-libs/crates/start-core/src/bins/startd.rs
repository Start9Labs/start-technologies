use std::cmp::max;
use std::ffi::OsString;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use color_eyre::eyre::eyre;
use futures::{FutureExt, TryFutureExt};
use rust_i18n::t;
use tokio::signal::unix::signal;
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
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::append_file;
use crate::util::logger::LOGGER;

#[instrument(skip_all)]
async fn inner_main(
    server: &mut WebServer<WildcardListener>,
    dns: &Arc<DnsController>,
    config: &ServerConfig,
) -> Result<(Option<Shutdown>, Option<PatchDb>), Error> {
    let rpc_ctx = if !tokio::fs::metadata("/run/startos/initialized")
        .await
        .is_ok()
    {
        LOGGER.set_logfile(Some(
            append_file("/run/startos/init.log").await?.into_std().await,
        ));
        let (ctx, handle) = match super::start_init::main(server, dns.clone(), &config).await? {
            Err(s) => return Ok((Some(s), None)),
            Ok(ctx) => ctx,
        };
        tokio::fs::write("/run/startos/initialized", "").await?;

        server.serve_ui_for(ctx.clone());
        LOGGER.set_logfile(None);
        handle.complete();

        ctx
    } else {
        let init_ctx = InitContext::init(
            config,
            dns.clone(),
            crate::context::startup::startup_identity().await?,
        )
        .await?;
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

    let (rpc_ctx, shutdown) = async {
        crate::hostname::sync_hostname(&rpc_ctx.account.peek(|a| a.hostname.clone())).await?;

        let mut shutdown_recv = rpc_ctx.shutdown.subscribe();

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
            shutdown_send
                .send(None)
                .map_err(|_| ())
                .expect("send shutdown signal");
        }));

        let metrics_ctx = rpc_ctx.clone();
        let metrics_task = tokio::spawn(async move {
            launch_metrics_task(&metrics_ctx.metrics_cache, &metrics_ctx.db, || {
                metrics_ctx.shutdown.subscribe()
            })
            .await
        });

        metrics_task
            .map_err(|e| {
                Error::new(
                    eyre!("{}", e).wrap_err(t!("bins.startd.metrics-daemon-panicked").to_string()),
                    ErrorKind::Unknown,
                )
            })
            .map_ok(|_| tracing::debug!("{}", t!("bins.startd.metrics-daemon-shutdown")))
            .await?;

        let shutdown = shutdown_recv
            .recv()
            .await
            .with_kind(crate::ErrorKind::Unknown)?;

        drop(sig_handler);

        Ok::<_, Error>((rpc_ctx, shutdown))
    }
    .await?;
    let db = (*rpc_ctx.db).clone();
    rpc_ctx.shutdown().await;

    Ok((shutdown, Some(db)))
}

const DB_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DbReleaseOutcome {
    Closed,
    TimedOut,
}

fn gate_shutdown_export(shutdown: Option<Shutdown>, outcome: DbReleaseOutcome) -> Option<Shutdown> {
    let mut shutdown = shutdown?;
    if outcome == DbReleaseOutcome::TimedOut && shutdown.disk_guid.take().is_some() {
        tracing::error!(
            "Skipping in-process data volume export because database storage did not close in time"
        );
    }
    Some(shutdown)
}

async fn release_db_handles(dns: &DnsController, db: Option<PatchDb>) -> DbReleaseOutcome {
    release_db_handles_with_timeout(
        dns,
        db,
        std::path::Path::new(crate::MAIN_DATA).join("embassy.db"),
        DB_CLOSE_TIMEOUT,
    )
    .await
}

async fn release_db_handles_with_timeout(
    dns: &DnsController,
    db: Option<PatchDb>,
    db_path: impl AsRef<std::path::Path>,
    timeout: Duration,
) -> DbReleaseOutcome {
    let net_iface = dns.shutdown().await;
    let db = db.or_else(|| net_iface.as_ref().map(|net| (*net.db).clone()));
    drop(net_iface);
    let release = async {
        if let Some(db) = db {
            if let Err(e) = db.close_storage().await {
                tracing::error!("Error closing database storage: {e}");
                tracing::debug!("{e:?}");
            }
        }
        PatchDb::wait_until_closed(db_path).await;
    };
    if tokio::time::timeout(timeout, release).await.is_err() {
        tracing::error!("Timed out releasing database storage before export");
        DbReleaseOutcome::TimedOut
    } else {
        DbReleaseOutcome::Closed
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
            match inner_main(&mut server, &dns, &config).await {
                Ok((shutdown, db)) => {
                    server.shutdown().await;
                    let released = release_db_handles(&dns, db).await;
                    Ok(gate_shutdown_export(shutdown, released))
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
                                    tokio::fs::read_to_string("/media/startos/config/disk.guid")
                                        .await?
                                        .trim(),
                                ))
                            } else {
                                None
                            },
                            e,
                            dns.clone(),
                            crate::context::startup::diagnostic_identity().await,
                        )
                        .await?;

                        server.serve_ui_for(ctx.clone());

                        let mut shutdown = ctx.shutdown.subscribe();

                        let shutdown =
                            shutdown.recv().await.with_kind(crate::ErrorKind::Unknown)?;

                        server.shutdown().await;
                        let released = release_db_handles(&dns, None).await;

                        Ok::<_, Error>(gate_shutdown_export(Some(shutdown), released))
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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Weak};
    use std::time::Duration;

    use imbl::OrdMap;
    use imbl_value::InternedString;
    use patch_db::PatchDb;

    use super::{
        DbReleaseOutcome, gate_shutdown_export, release_db_handles, release_db_handles_with_timeout,
    };
    use crate::db::model::Database;
    use crate::net::dns::DnsController;
    use crate::net::gateway::NetworkInterfaceController;
    use crate::prelude::TypedPatchDb;
    use crate::shutdown::Shutdown;
    use crate::util::sync::Watch;

    #[cfg(target_os = "linux")]
    fn open_fds_for(path: &Path) -> Vec<PathBuf> {
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        std::fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| std::fs::read_link(entry.path()).ok())
            .filter(|target| *target == canonical)
            .collect()
    }

    #[cfg(not(target_os = "linux"))]
    fn open_fds_for(_path: &Path) -> Vec<PathBuf> {
        Vec::new()
    }

    fn try_reopen_db(path: &Path) -> Result<(), patch_db::Error> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let reopened = PatchDb::open(path).await?;
                reopened.close().await;
                Ok(())
            })
    }

    #[test]
    fn shutdown_releases_embassy_db_fd_before_export_with_lingering_holders() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("embassy.db");
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let (blocker_started_tx, blocker_started_rx) = std::sync::mpsc::channel::<()>();

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();

        let (net_weak, _held_db_clone, shutdown) = rt.block_on(async {
            let db =
                TypedPatchDb::<Database>::load_unchecked(PatchDb::open(&db_path).await.unwrap());
            let dns = Arc::new(DnsController::new(Watch::new(OrdMap::new())));
            let net_iface = Arc::new(NetworkInterfaceController::with_test_interfaces(
                db.clone(),
                Watch::new(OrdMap::new()),
            ));
            let net_weak: Weak<NetworkInterfaceController> = Arc::downgrade(&net_iface);
            dns.activate(net_iface);

            let late_db = db.clone();
            tokio::task::spawn_blocking(move || {
                let _ = blocker_started_tx.send(());
                let _ = release_rx.recv_timeout(Duration::from_secs(5));
                drop(late_db);
            });
            blocker_started_rx.recv().unwrap();

            let extra_clone = db.clone();
            let raw_db = (*db).clone();
            drop(db);

            let outcome = release_db_handles(&dns, Some(raw_db)).await;
            assert_eq!(outcome, DbReleaseOutcome::Closed);
            let shutdown = gate_shutdown_export(
                Some(Shutdown {
                    disk_guid: Some(InternedString::intern("test-disk-guid")),
                    restart: true,
                }),
                outcome,
            );
            drop(dns);
            (net_weak, extra_clone, shutdown)
        });

        rt.shutdown_timeout(Duration::from_millis(100));

        let open_fds = open_fds_for(&db_path);
        let reopen_res = try_reopen_db(&db_path);
        let _ = release_tx.send(());
        let shutdown = shutdown.unwrap();
        assert_eq!(
            shutdown.disk_guid.as_deref(),
            Some("test-disk-guid"),
            "confirmed close must preserve disk_guid for export"
        );
        assert!(shutdown.restart);
        assert_eq!(
            net_weak.strong_count(),
            0,
            "NetworkInterfaceController still held alive after shutdown"
        );
        assert!(
            open_fds.is_empty(),
            "startd still holds embassy.db open when export runs: {open_fds:?}"
        );
        reopen_res.unwrap();
    }

    #[test]
    fn diagnostic_shutdown_releases_db_held_by_dns_controller() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("embassy.db");
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let (blocker_started_tx, blocker_started_rx) = std::sync::mpsc::channel::<()>();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();

        let (net_weak, _held_db_clone) = rt.block_on(async {
            let db =
                TypedPatchDb::<Database>::load_unchecked(PatchDb::open(&db_path).await.unwrap());
            let late_db = db.clone();
            let extra_clone = db.clone();
            let dns = Arc::new(DnsController::new(Watch::new(OrdMap::new())));
            let net_iface = Arc::new(NetworkInterfaceController::with_test_interfaces(
                db,
                Watch::new(OrdMap::new()),
            ));
            let net_weak = Arc::downgrade(&net_iface);
            dns.activate(net_iface);

            tokio::task::spawn_blocking(move || {
                let _ = blocker_started_tx.send(());
                let _ = release_rx.recv_timeout(Duration::from_secs(5));
                drop(late_db);
            });
            blocker_started_rx.recv().unwrap();

            assert_eq!(
                release_db_handles(&dns, None).await,
                DbReleaseOutcome::Closed
            );
            assert_eq!(net_weak.strong_count(), 0);
            drop(dns);
            (net_weak, extra_clone)
        });

        rt.shutdown_timeout(Duration::from_millis(100));

        let open_fds = open_fds_for(&db_path);
        let reopen_res = try_reopen_db(&db_path);
        let _ = release_tx.send(());
        assert_eq!(net_weak.strong_count(), 0);
        assert!(
            open_fds.is_empty(),
            "diagnostic shutdown left embassy.db open: {open_fds:?}"
        );
        reopen_res.unwrap();
    }

    #[test]
    fn release_db_handles_bounds_stalled_close_storage_and_wait_until_closed() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("embassy.db");
        let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(3)
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let db = PatchDb::open(&db_path).await.unwrap();
            let dns = Arc::new(DnsController::new(Watch::new(OrdMap::new())));

            let blocker_db = db.clone();
            let channels = std::panic::AssertUnwindSafe((started_tx, release_rx));
            let blocker = tokio::spawn(async move {
                let _ = blocker_db
                    .apply_function(move |v| {
                        let std::panic::AssertUnwindSafe((started_tx, release_rx)) = channels;
                        let _ = started_tx.send(());
                        let _ = release_rx.recv_timeout(Duration::from_secs(5));
                        Ok::<_, patch_db::Error>((v, ()))
                    })
                    .await;
            });
            started_rx.recv().unwrap();

            let outcome = tokio::time::timeout(
                Duration::from_secs(1),
                release_db_handles_with_timeout(
                    &dns,
                    Some(db.clone()),
                    &db_path,
                    Duration::from_millis(50),
                ),
            )
            .await
            .expect("release_db_handles hung on stalled close_storage");
            assert_eq!(outcome, DbReleaseOutcome::TimedOut);

            let _ = release_tx.send(());
            blocker.await.unwrap();

            tokio::time::timeout(Duration::from_secs(2), PatchDb::wait_until_closed(&db_path))
                .await
                .expect("detached close_storage did not release lock");
            assert!(open_fds_for(&db_path).is_empty());

            let reopened = PatchDb::open(&db_path).await.unwrap();
            let wait_outcome = tokio::time::timeout(
                Duration::from_secs(1),
                release_db_handles_with_timeout(&dns, None, &db_path, Duration::from_millis(50)),
            )
            .await
            .expect("release_db_handles hung on wait_until_closed");
            assert_eq!(wait_outcome, DbReleaseOutcome::TimedOut);

            reopened.close().await;
            drop(db);
        });
    }

    #[test]
    fn timeout_releasing_db_skips_in_process_export_and_preserves_power_action() {
        for restart in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let db_path = dir.path().join("embassy.db");
            let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
            let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
            let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();

            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(3)
                .enable_all()
                .build()
                .unwrap();

            let (outcome, shutdown) = rt.block_on(async {
                let db = PatchDb::open(&db_path).await.unwrap();
                let dns = Arc::new(DnsController::new(Watch::new(OrdMap::new())));

                let blocker_db = db.clone();
                let channels = std::panic::AssertUnwindSafe((started_tx, release_rx));
                tokio::spawn(async move {
                    let _done = done_tx;
                    let _ = blocker_db
                        .apply_function(move |v| {
                            let std::panic::AssertUnwindSafe((started_tx, release_rx)) = channels;
                            let _ = started_tx.send(());
                            let _ = release_rx.recv_timeout(Duration::from_secs(10));
                            Ok::<_, patch_db::Error>((v, ()))
                        })
                        .await;
                    drop(blocker_db);
                });
                started_rx.recv().unwrap();

                let outcome = tokio::time::timeout(
                    Duration::from_secs(1),
                    release_db_handles_with_timeout(
                        &dns,
                        Some(db),
                        &db_path,
                        Duration::from_millis(50),
                    ),
                )
                .await
                .expect("release_db_handles hung on stalled writer");

                let requested = Some(Shutdown {
                    disk_guid: Some(InternedString::intern("test-disk-guid")),
                    restart,
                });
                (outcome, gate_shutdown_export(requested, outcome))
            });

            rt.shutdown_timeout(Duration::from_millis(100));

            #[cfg(target_os = "linux")]
            let open_fds_after_rt_shutdown = open_fds_for(&db_path);

            let _ = release_tx.send(());
            let _ = done_rx.recv_timeout(Duration::from_secs(2));

            let shutdown = shutdown.expect("shutdown action dropped on close timeout");
            assert_eq!(outcome, DbReleaseOutcome::TimedOut);
            #[cfg(target_os = "linux")]
            assert!(
                !open_fds_after_rt_shutdown.is_empty(),
                "test did not hold embassy.db open across rt.shutdown_timeout"
            );
            assert_eq!(
                shutdown.disk_guid, None,
                "in-process export must be skipped when database closure timed out"
            );
            assert_eq!(
                shutdown.restart, restart,
                "requested power action must be preserved on close timeout"
            );
        }
    }
}
