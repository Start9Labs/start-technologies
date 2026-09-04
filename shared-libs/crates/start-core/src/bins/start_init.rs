use tokio::process::Command;
use tracing::instrument;

use crate::context::config::ServerConfig;
use crate::context::rpc::InitRpcContextPhases;
use crate::context::{DiagnosticContext, InitContext, RpcContext, SetupContext};
use crate::disk::REPAIR_DISK_PATH;
use crate::disk::fsck::RepairStrategy;
use crate::disk::main::{DEFAULT_PASSWORD, ImportMode};
use crate::firmware::{check_for_firmware_update, update_firmware};
use crate::init::{InitPhases, STANDBY_MODE_PATH};
use crate::net::gateway::WildcardListener;
use crate::net::web_server::WebServer;
use crate::prelude::*;
use crate::progress::FullProgressTracker;
use crate::shutdown::Shutdown;
use crate::util::Invoke;
use crate::{DATA_DIR, PLATFORM};

#[instrument(skip_all)]
async fn setup_or_init(
    server: &mut WebServer<WildcardListener>,
    config: &ServerConfig,
) -> Result<Result<(RpcContext, FullProgressTracker), Shutdown>, Error> {
    if let Some(firmware) = check_for_firmware_update()
        .await
        .map_err(|e| {
            tracing::warn!(
                "{}",
                t!(
                    "bins.start-init.error-checking-firmware",
                    error = e.to_string()
                )
            );
            tracing::debug!("{e:?}");
        })
        .ok()
        .and_then(|a| a)
    {
        let init_ctx = InitContext::init(config).await?;
        let handle = &init_ctx.progress;
        let mut update_phase =
            handle.add_phase(t!("bins.start-init.updating-firmware").into(), Some(10));
        let mut reboot_phase = handle.add_phase(t!("bins.start-init.rebooting").into(), Some(1));

        server.serve_ui_for(init_ctx);

        update_phase.start();
        if let Err(e) = update_firmware(firmware).await {
            tracing::warn!(
                "{}",
                t!(
                    "bins.start-init.error-firmware-update",
                    error = e.to_string()
                )
            );
            tracing::debug!("{e:?}");
        } else {
            update_phase.complete();
            reboot_phase.start();
            return Ok(Err(Shutdown {
                disk_guid: None,
                restart: true,
            }));
        }
    }

    Command::new("ln")
        .arg("-sf")
        .arg("/usr/lib/startos/scripts/fake-apt")
        .arg("/usr/local/bin/apt")
        .invoke(crate::ErrorKind::Filesystem)
        .await?;
    Command::new("ln")
        .arg("-sf")
        .arg("/usr/lib/startos/scripts/fake-apt")
        .arg("/usr/local/bin/apt-get")
        .invoke(crate::ErrorKind::Filesystem)
        .await?;
    Command::new("ln")
        .arg("-sf")
        .arg("/usr/lib/startos/scripts/fake-apt")
        .arg("/usr/local/bin/aptitude")
        .invoke(crate::ErrorKind::Filesystem)
        .await?;

    Command::new("make-ssl-cert")
        .arg("generate-default-snakeoil")
        .arg("--force-overwrite")
        .invoke(crate::ErrorKind::OpenSsl)
        .await?;

    if tokio::fs::metadata("/media/startos/config/disk.guid")
        .await
        .is_err()
    {
        let ctx = SetupContext::init(server, config.clone())?;

        server.serve_ui_for(ctx.clone());

        let mut shutdown = ctx.shutdown.subscribe();
        if let Some(shutdown) = shutdown.recv().await.expect("context dropped") {
            return Ok(Err(shutdown));
        }

        tokio::task::yield_now().await;
        if let Err(e) = Command::new("killall")
            .arg("firefox-esr")
            .invoke(ErrorKind::NotFound)
            .await
        {
            tracing::error!(
                "{}",
                t!(
                    "bins.start-init.failed-to-kill-kiosk",
                    error = e.to_string()
                )
            );
            tracing::debug!("{:?}", e);
        }

        Ok(Ok(match ctx.result.get() {
            Some(Ok((_, rpc_ctx))) => (rpc_ctx.clone(), ctx.progress.clone()),
            Some(Err(e)) => return Err(e.clone_output()),
            None => {
                return Err(Error::new(
                    eyre!("{}", t!("bins.start-init.setup-mode-exited")),
                    ErrorKind::Unknown,
                ));
            }
        }))
    } else {
        let init_ctx = InitContext::init(config).await?;
        let handle = init_ctx.progress.clone();
        let err_channel = init_ctx.error.clone();

        let mut disk_phase =
            handle.add_phase(t!("bins.start-init.opening-data-drive").into(), Some(10));
        let init_phases = InitPhases::new(&handle);
        let rpc_ctx_phases = InitRpcContextPhases::new(&handle);

        server.serve_ui_for(init_ctx);

        async {
            disk_phase.start();
            let guid_string = tokio::fs::read_to_string("/media/startos/config/disk.guid") // unique identifier for volume group - keeps track of the disk that goes with your embassy
                .await?;
            let disk_guid = InternedString::intern(guid_string.trim());
            let requires_reboot = crate::disk::main::import(
                &*disk_guid,
                DATA_DIR,
                ImportMode::ReadWrite(if tokio::fs::metadata(REPAIR_DISK_PATH).await.is_ok() {
                    RepairStrategy::Aggressive
                } else {
                    RepairStrategy::Preen
                }),
                if disk_guid.ends_with("_UNENC") {
                    None
                } else {
                    Some(DEFAULT_PASSWORD)
                },
                Some(&handle),
            )
            .await?;
            if tokio::fs::metadata(REPAIR_DISK_PATH).await.is_ok() {
                tokio::fs::remove_file(REPAIR_DISK_PATH)
                    .await
                    .with_ctx(|_| (crate::ErrorKind::Filesystem, REPAIR_DISK_PATH))?;
            }
            disk_phase.complete();
            tracing::info!("{}", t!("bins.start-init.loaded-disk"));

            if requires_reboot.0 {
                tracing::info!("{}", t!("bins.start-init.rebooting"));
                let mut reboot_phase =
                    handle.add_phase(t!("bins.start-init.rebooting").into(), Some(1));
                reboot_phase.start();
                return Ok(Err(Shutdown {
                    disk_guid: Some(disk_guid),
                    restart: true,
                }));
            }

            let init_result =
                crate::init::init(&server.acceptor_setter(), config, init_phases).await?;

            let rpc_ctx = RpcContext::init(
                &server.acceptor_setter(),
                config,
                disk_guid,
                Some(init_result),
                rpc_ctx_phases,
            )
            .await?;

            Ok::<_, Error>(Ok((rpc_ctx, handle)))
        }
        .await
        .map_err(|e| {
            err_channel.send_replace(Some(e.clone_output()));
            e
        })
    }
}

#[instrument(skip_all)]
pub async fn main(
    server: &mut WebServer<WildcardListener>,
    config: &ServerConfig,
) -> Result<Result<(RpcContext, FullProgressTracker), Shutdown>, Error> {
    if &*PLATFORM == "raspberrypi" && tokio::fs::metadata(STANDBY_MODE_PATH).await.is_ok() {
        tokio::fs::remove_file(STANDBY_MODE_PATH).await?;
        Command::new("sync").invoke(ErrorKind::Filesystem).await?;
        crate::sound::SHUTDOWN.play().await?;
        futures::future::pending::<()>().await;
    }

    let res = match setup_or_init(server, config).await {
        Err(e) => {
            async move {
                tracing::error!("{e}");
                tracing::debug!("{e:?}");

                let ctx = DiagnosticContext::init(
                    config,
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

                let shutdown = ctx.shutdown.subscribe().recv().await.unwrap();

                Ok(Err(shutdown))
            }
            .await
        }
        Ok(s) => Ok(s),
    };

    res
}
