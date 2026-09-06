use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use clap::{ArgAction, Parser};
use color_eyre::eyre::{Result, eyre};
use exver::{Version, VersionRange};
use futures::TryStreamExt;
use imbl::OrdMap;
use imbl_value::json;
use itertools::Itertools;
use patch_db::json_ptr::JsonPointer;
use reqwest::{Client, Url};
use rpc_toolkit::HandlerArgs;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::instrument;
use ts_rs::TS;

pub mod diagnostic;

use crate::PLATFORM;
use crate::context::{CliContext, RpcContext};
use crate::db::model::public::RestartReason;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;
use crate::progress::{
    FullProgressTracker, PhaseProgressTrackerHandle, PhasedProgressBar, ProgressUnits,
};
use crate::registry::asset::RegistryAsset;
use crate::registry::context::{RegistryContext, RegistryUrlParams};
use crate::registry::os::SIG_CONTEXT;
use crate::registry::os::index::OsVersionInfo;
use crate::rpc_continuations::{Guid, RpcContinuation};
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::sign::commitment::Commitment;
use crate::sign::commitment::blake3::Blake3Commitment;
use crate::sound::{
    CIRCLE_OF_5THS_SHORT, UPDATE_FAILED_1, UPDATE_FAILED_2, UPDATE_FAILED_3, UPDATE_FAILED_4,
};
use crate::util::Invoke;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::AtomicFile;

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct UpdateSystemParams {
    #[arg(help = "help.arg.registry-url")]
    #[ts(type = "string")]
    registry: Url,
    #[ts(type = "string | null")]
    #[arg(long = "to", help = "help.arg.update-target-version")]
    target: Option<VersionRange>,
    #[arg(long = "no-progress", action = ArgAction::SetFalse, help = "help.arg.no-progress")]
    #[serde(default)]
    progress: bool,
}

#[derive(Deserialize, Serialize, TS)]
pub struct UpdateSystemRes {
    #[ts(type = "string | null")]
    target: Option<Version>,
    #[ts(type = "string | null")]
    progress: Option<Guid>,
}

/// An user/ daemon would call this to update the system to the latest version and do the updates available,
/// and this will return something if there is an update, and in that case there will need to be a restart.
#[instrument(skip_all)]
pub async fn update_system(
    ctx: RpcContext,
    UpdateSystemParams {
        target,
        registry,
        progress,
    }: UpdateSystemParams,
) -> Result<UpdateSystemRes, Error> {
    if ctx
        .db
        .peek()
        .await
        .into_public()
        .into_server_info()
        .into_status_info()
        .into_restart()
        .de()?
        == Some(RestartReason::Update)
    {
        return Err(Error::new(
            eyre!("{}", t!("update.already-updated-restart-required")),
            ErrorKind::InvalidRequest,
        ));
    }
    let target =
        maybe_do_update(ctx.clone(), registry, target.unwrap_or(VersionRange::Any)).await?;
    let progress = if progress && target.is_some() {
        let guid = Guid::new();
        ctx.clone()
            .rpc_continuations
            .add(
                guid.clone(),
                RpcContinuation::ws(
                    |mut ws| async move {
                        if let Err(e) = async {
                            let mut sub = ctx
                                .db
                                .subscribe(
                                    "/public/serverInfo/statusInfo/updateProgress"
                                        .parse::<JsonPointer>()
                                        .with_kind(ErrorKind::Database)?,
                                )
                                .await;
                            loop {
                                let progress = ctx
                                    .db
                                    .peek()
                                    .await
                                    .into_public()
                                    .into_server_info()
                                    .into_status_info()
                                    .into_update_progress()
                                    .de()?;
                                ws.send(axum::extract::ws::Message::Text(
                                    serde_json::to_string(&progress)
                                        .with_kind(ErrorKind::Serialization)?
                                        .into(),
                                ))
                                .await
                                .with_kind(ErrorKind::Network)?;
                                if progress.is_none() {
                                    return ws.normal_close("complete").await;
                                }
                                tokio::select! {
                                    _ = sub.recv() => (),
                                    res = async {
                                        loop {
                                            if ws.recv().await.transpose().with_kind(ErrorKind::Network)?.is_none() {
                                                return Ok(())
                                            }
                                        }
                                     } => {
                                        return res
                                    }
                                }
                            }
                        }
                        .await
                        {
                            tracing::error!("{}", t!("update.error-returning-progress", error = e.to_string()));
                            tracing::debug!("{e:?}")
                        }
                    },
                    Duration::from_secs(30),
                ),
            )
            .await;
        Some(guid)
    } else {
        None
    };
    Ok(UpdateSystemRes { target, progress })
}

pub async fn cli_update_system(
    HandlerArgs {
        context,
        parent_method,
        method,
        raw_params,
        ..
    }: HandlerArgs<CliContext, UpdateSystemParams>,
) -> Result<(), Error> {
    let res = from_value::<UpdateSystemRes>(
        context
            .call_remote::<RpcContext>(
                &parent_method.into_iter().chain(method).join("."),
                raw_params,
            )
            .await?,
    )?;
    match res.target {
        None => println!("{}", t!("update.no-updates-available")),
        Some(v) => {
            if let Some(progress) = res.progress {
                let mut ws = context.ws_continuation(progress).await?;
                let mut progress = PhasedProgressBar::new(&t!(
                    "update.updating-to-version",
                    version = v.to_string()
                ));
                let mut prev = None;
                while let Some(msg) = ws.try_next().await.with_kind(ErrorKind::Network)? {
                    if let tokio_tungstenite::tungstenite::Message::Text(msg) = msg {
                        if let Some(snap) =
                            serde_json::from_str(&msg).with_kind(ErrorKind::Deserialization)?
                        {
                            progress.update(&snap);
                            prev = Some(snap);
                        } else {
                            break;
                        }
                    }
                }
                if let Some(mut prev) = prev {
                    for phase in &mut prev.phases {
                        phase.progress.set_complete();
                    }
                    prev.overall.set_complete();
                    progress.update(&prev);
                }
                println!("{}", t!("update.complete-restart-to-apply"))
            } else {
                println!(
                    "{}",
                    t!("update.updating-to-version", version = v.to_string())
                )
            }
        }
    }
    Ok(())
}

/// What is the status of the updates?
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum UpdateResult {
    NoUpdates,
    Updating,
}

pub fn display_update_result(_: UpdateSystemParams, status: UpdateResult) {
    match status {
        UpdateResult::Updating => {
            println!("Updating...");
        }
        UpdateResult::NoUpdates => {
            println!("No updates available");
        }
    }
}

#[instrument(skip_all)]
async fn maybe_do_update(
    ctx: RpcContext,
    registry: Url,
    target: VersionRange,
) -> Result<Option<Version>, Error> {
    let peeked = ctx.db.peek().await;
    let current_version = peeked.as_public().as_server_info().as_version().de()?;
    let mut available = from_value::<BTreeMap<Version, OsVersionInfo>>(
        ctx.call_remote_with::<RegistryContext, _>(
            "os.version.get",
            OrdMap::new(),
            json!({
                "sourceVersion": current_version,
                "targetVersion": target,
            }),
            RegistryUrlParams { registry },
        )
        .await?,
    )?;
    let Some((target_version, asset)) = available
        .pop_last()
        .and_then(|(v, mut info)| info.squashfs.remove(&**PLATFORM).map(|a| (v, a)))
    else {
        return Ok(None);
    };
    if !target_version.satisfies(&target) {
        return Err(Error::new(
            eyre!("got back version from registry that does not satisfy {target}"),
            ErrorKind::Registry,
        ));
    }

    asset.validate(SIG_CONTEXT, asset.all_signers())?;

    let progress = FullProgressTracker::new();
    let prune_phase = progress.add_phase("Pruning Old OS Images".into(), Some(2));
    let mut download_phase = progress.add_phase("Downloading File".into(), Some(100));
    download_phase.set_total(asset.commitment.size);
    download_phase.set_units(Some(ProgressUnits::Bytes));
    let reverify_phase = progress.add_phase("Reverifying File".into(), Some(10));
    let finalize_phase = progress.add_phase("Finalizing Update".into(), Some(1));

    let start_progress = progress.snapshot();

    ctx.db
        .mutate(|db| {
            let server_info = db.as_public_mut().as_server_info_mut();

            if server_info.as_status_info().as_restart().de()?.is_some() {
                return Err(Error::new(
                    eyre!("{}", t!("update.already-updated-restart-required")),
                    crate::ErrorKind::InvalidRequest,
                ));
            }

            let mut status = server_info.as_status_info().de()?;
            if status.update_progress.is_some() {
                return Err(Error::new(
                    eyre!("{}", t!("update.already-updating")),
                    crate::ErrorKind::InvalidRequest,
                ));
            }

            status.update_progress = Some(start_progress);
            server_info.as_status_info_mut().ser(&status)?;
            Ok(())
        })
        .await
        .result?;

    let progress_task = NonDetachingJoinHandle::from(tokio::spawn(progress.clone().sync_to_db(
        ctx.db.clone(),
        |db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_status_info_mut()
                .as_update_progress_mut()
                .transpose_mut()
        },
        Some(Duration::from_millis(300)),
    )));

    tokio::spawn(async move {
        let res = do_update(
            ctx.http_client(),
            asset,
            UpdateProgressHandles {
                progress,
                prune_phase,
                download_phase,
                reverify_phase,
                finalize_phase,
            },
        )
        .await;
        match res {
            Ok(()) => {
                ctx.db
                    .mutate(|db| {
                        let server_info = db.as_public_mut().as_server_info_mut();
                        server_info
                            .as_status_info_mut()
                            .as_update_progress_mut()
                            .ser(&None)?;
                        server_info
                            .as_status_info_mut()
                            .as_restart_mut()
                            .ser(&Some(RestartReason::Update))
                    })
                    .await
                    .result?;
                progress_task.await.with_kind(ErrorKind::Unknown)??;
                CIRCLE_OF_5THS_SHORT.play().await.log_err();
            }
            Err(e) => {
                let err_string = t!("update.not-successful", error = e.to_string()).to_string();
                ctx.db
                    .mutate(|db| {
                        db.as_public_mut()
                            .as_server_info_mut()
                            .as_status_info_mut()
                            .as_update_progress_mut()
                            .ser(&None)?;
                        notify(
                            db,
                            None,
                            NotificationLevel::Error,
                            t!("update.failed-title").to_string(),
                            err_string,
                            (),
                        )
                    })
                    .await
                    .result
                    .log_err();
                // TODO: refactor sound lib to make compound tempos easier to deal with
                UPDATE_FAILED_1.play().await.log_err();
                UPDATE_FAILED_2.play().await.log_err();
                UPDATE_FAILED_3.play().await.log_err();
                UPDATE_FAILED_4.play().await.log_err();
            }
        }
        Ok::<(), Error>(())
    });
    Ok(Some(target_version))
}

pub(crate) struct UpdateProgressHandles {
    pub progress: FullProgressTracker,
    pub prune_phase: PhaseProgressTrackerHandle,
    pub download_phase: PhaseProgressTrackerHandle,
    pub reverify_phase: PhaseProgressTrackerHandle,
    pub finalize_phase: PhaseProgressTrackerHandle,
}

#[instrument(skip_all)]
pub(crate) async fn do_update(
    client: Client,
    asset: RegistryAsset<Blake3Commitment>,
    UpdateProgressHandles {
        progress,
        mut prune_phase,
        mut download_phase,
        mut reverify_phase,
        mut finalize_phase,
    }: UpdateProgressHandles,
) -> Result<(), Error> {
    prune_phase.start();
    Command::new("/usr/lib/startos/scripts/prune-images")
        .arg(asset.commitment.size.to_string())
        .invoke(ErrorKind::Filesystem)
        .await?;
    Command::new("/usr/lib/startos/scripts/prune-boot")
        .invoke(ErrorKind::Filesystem)
        .await?;
    prune_phase.complete();

    download_phase.start();
    let path = Path::new("/media/startos/images/next.squashfs");
    let mut dst = AtomicFile::new(&path, None::<&Path>).await?;
    let mut download_writer = download_phase.writer(&mut *dst);
    asset.download(client, &mut download_writer).await?;
    let (_, mut download_phase) = download_writer.into_inner();
    dst.sync_all().await?;
    download_phase.complete();

    reverify_phase.start();
    asset
        .commitment
        .check(&MultiCursorFile::open(&*dst).await?)
        .await?;
    dst.save().await?;
    reverify_phase.complete();

    finalize_phase.start();
    Command::new("unsquashfs")
        .arg("-n")
        .arg("-f")
        .arg("-d")
        .arg("/")
        .arg(&path)
        .arg("/usr/lib/startos/scripts/upgrade")
        .invoke(crate::ErrorKind::Filesystem)
        .await?;

    let checksum = hex::encode(&asset.commitment.hash[..16]);

    Command::new("/usr/lib/startos/scripts/upgrade")
        .env("CHECKSUM", &checksum)
        .arg(&path)
        .invoke(ErrorKind::Grub)
        .await?;
    finalize_phase.complete();

    progress.complete();

    Ok(())
}
