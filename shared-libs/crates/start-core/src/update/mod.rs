use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use clap::{ArgAction, Parser};
use color_eyre::eyre::{Result, eyre};
use exver::{Version, VersionRange};
use imbl::OrdMap;
use imbl_value::json;
use itertools::Itertools;
use reqwest::{Client, Url};
use rpc_toolkit::HandlerArgs;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::instrument;

pub mod diagnostic;
pub mod security;
pub(crate) mod signer_files;
pub mod signers;

#[cfg(test)]
mod signature_tests;

use crate::PLATFORM;
use crate::context::{CliContext, RpcContext};
use crate::db::model::public::RestartReason;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;
use crate::progress::{
    FullProgress, FullProgressTracker, PhaseProgressTrackerHandle, ProgressUnits, forward_progress,
    progress_until_complete, with_cli_progress,
};
use crate::registry::asset::RegistryAsset;
use crate::registry::context::{RegistryContext, RegistryUrlParams};
use crate::registry::os::SIG_CONTEXT;
use crate::registry::os::index::OsVersionInfo;
use crate::registry::signer::AcceptSigners;
use crate::rpc_continuations::{Guid, RpcContinuation, WebSocketPolicy};
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::sign::commitment::Commitment;
use crate::sign::commitment::blake3::Blake3Commitment;
use crate::sound::{
    CIRCLE_OF_5THS_SHORT, UPDATE_FAILED_1, UPDATE_FAILED_2, UPDATE_FAILED_3, UPDATE_FAILED_4,
};
use crate::trusted_keys::{self, TrustContext};
use crate::util::Invoke;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::AtomicFile;
use crate::version::release_notes::{UpdateNotes, save_update_notes};

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct UpdateSystemParams {
    #[arg(help = "help.arg.registry-url")]
    registry: Url,
    /// Additional signer requirements checked before starting the update.
    #[arg(skip)]
    pub accept_signers: Option<AcceptSigners>,
    /// Accepts an untrusted signer without skipping signature or integrity verification.
    #[arg(long, help = "help.arg.trust")]
    #[serde(default)]
    pub trust: bool,
    #[arg(
        long = "to",
        value_name = "TARGET",
        help = "help.arg.update-target-version"
    )]
    target_version: Option<VersionRange>,
    /// Requests a legacy progress GUID; rpcstream observes completion independently.
    #[arg(long = "no-progress", action = ArgAction::SetFalse, help = "help.arg.no-progress")]
    #[serde(default)]
    progress: bool,
    #[arg(skip)]
    #[visit(ts(skip))]
    #[serde(rename = "__Auth_signer")]
    signer: Option<InternedString>,
}

rpc_toolkit::reflect_ts!(UpdateSystemParams);
rpc_toolkit::ts_export!(UpdateSystemParams, namespaces = [""]);

#[derive(Deserialize, Serialize, VisitFields)]
pub struct UpdateSystemRes {
    target: Option<Version>,
    /// Legacy progress continuation, absent for rpcstream observers.
    progress: Option<Guid>,
}

rpc_toolkit::reflect_ts!(UpdateSystemRes);
rpc_toolkit::ts_export!(UpdateSystemRes, namespaces = [""]);

#[instrument(skip_all)]
/// Canceling observation does not cancel an accepted update.
pub async fn update_system(
    HandlerArgs {
        context: ctx,
        params:
            UpdateSystemParams {
                target_version: target,
                registry,
                progress,
                accept_signers,
                trust,
                signer,
            },
        rpc_stream,
        ..
    }: HandlerArgs<RpcContext, UpdateSystemParams, rpc_toolkit::Empty, FullProgress>,
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
    let started = maybe_do_update(
        ctx.clone(),
        registry,
        target.unwrap_or(VersionRange::Any),
        accept_signers,
        trust,
    )
    .await?;
    let Some(StartedUpdate {
        target,
        progress: tracker,
        done,
    }) = started
    else {
        if let Some(stream) = rpc_stream {
            let _ = stream.close(Ok::<_, Error>(()));
        }
        return Ok(UpdateSystemRes {
            target: None,
            progress: None,
        });
    };
    let source = progress_until_complete(tracker, async move {
        done.await.with_kind(ErrorKind::Cancelled)?
    });
    let progress = if let Some(stream) = rpc_stream {
        crate::auth::continuation_subscription(&ctx, signer)
            .await?
            .forward_stream(stream, source);
        None
    } else if progress {
        let guid = Guid::new();
        ctx.rpc_continuations
            .add(
                guid.clone(),
                RpcContinuation::ws_authed(
                    crate::auth::continuation_subscription(&ctx, signer).await?,
                    WebSocketPolicy::Authenticated,
                    ctx.secure_session().cloned(),
                    move |socket| async move {
                        if let Err(error) =
                            forward_progress(socket, crate::progress::progress_with_null(source))
                                .await
                        {
                            tracing::error!(
                                "{}",
                                t!("update.error-returning-progress", error = error.to_string())
                            );
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
    Ok(UpdateSystemRes {
        target: Some(target),
        progress,
    })
}

pub async fn cli_update_system(
    HandlerArgs {
        context,
        parent_method,
        method,
        params: UpdateSystemParams { progress, .. },
        raw_params,
        ..
    }: HandlerArgs<CliContext, UpdateSystemParams>,
) -> Result<(), Error> {
    let method = parent_method.into_iter().chain(method).join(".");
    if !progress {
        let result: UpdateSystemRes = from_value(
            context
                .call_remote::<RpcContext>(&method, raw_params)
                .await?,
        )?;
        display_update_started(result);
        return Ok(());
    }
    let response = context
        .call_remote_stream::<FullProgress, ()>(&method, raw_params)
        .await?;
    render_update_progress(response).await
}

fn display_update_started(result: UpdateSystemRes) {
    match result.target {
        None => println!("{}", t!("update.no-updates-available")),
        Some(version) => println!(
            "{}",
            t!("update.updating-to-version", version = version.to_string())
        ),
    }
}

async fn render_update_progress(
    response: crate::util::rpc_client::RpcResponseStream<Value, FullProgress, ()>,
) -> Result<(), Error> {
    let result: UpdateSystemRes = from_value(response.initial)?;
    match result.target {
        None => {
            response.items.finish().await?;
            println!("{}", t!("update.no-updates-available"));
        }
        Some(version) => {
            with_cli_progress(
                response.items,
                &t!("update.updating-to-version", version = version.to_string()),
            )
            .await?;
            println!("{}", t!("update.complete-restart-to-apply"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod continuation_tests;

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

pub(crate) fn is_os_update(
    version: &Version,
    git_hash: Option<&crate::s9pk::git_hash::GitHash>,
    installed: &Version,
    installed_hash: &str,
) -> bool {
    version > installed
        || (version == installed && git_hash.is_some_and(|hash| hash.as_ref() != installed_hash))
}

fn validate_update_signatures(
    asset: &RegistryAsset<Blake3Commitment>,
    accept_signers: Option<AcceptSigners>,
) -> Result<(), Error> {
    asset.validate_all(SIG_CONTEXT, accept_signers)
}

struct StartedUpdate {
    target: Version,
    progress: FullProgressTracker,
    done: tokio::task::JoinHandle<Result<(), Error>>,
}

#[instrument(skip_all)]
async fn maybe_do_update(
    ctx: RpcContext,
    registry: Url,
    target: VersionRange,
    accept_signers: Option<AcceptSigners>,
    trust: bool,
) -> Result<Option<StartedUpdate>, Error> {
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
    let Some((target_version, asset, notes)) = available.pop_last().and_then(target_asset) else {
        return Ok(None);
    };
    if !target_version.satisfies(&target) {
        return Err(Error::new(
            eyre!("got back version from registry that does not satisfy {target}"),
            ErrorKind::Registry,
        ));
    }

    let policy =
        trusted_keys::signer_policy(trusted_keys::required(&ctx, TrustContext::Os, trust).await?);
    validate_update_signatures(&asset, policy)?;
    if let Some(policy) = accept_signers {
        asset.validate(SIG_CONTEXT, policy)?;
    }

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

    let observer = progress.clone();
    let done = tokio::spawn(async move {
        let res = do_update(
            ctx.client.get(),
            asset,
            notes,
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
                UPDATE_FAILED_1.play().await.log_err();
                UPDATE_FAILED_2.play().await.log_err();
                UPDATE_FAILED_3.play().await.log_err();
                UPDATE_FAILED_4.play().await.log_err();
                return Err(e);
            }
        }
        Ok::<(), Error>(())
    });
    Ok(Some(StartedUpdate {
        target: target_version,
        progress: observer,
        done,
    }))
}

pub(crate) struct UpdateProgressHandles {
    pub progress: FullProgressTracker,
    pub prune_phase: PhaseProgressTrackerHandle,
    pub download_phase: PhaseProgressTrackerHandle,
    pub reverify_phase: PhaseProgressTrackerHandle,
    pub finalize_phase: PhaseProgressTrackerHandle,
}

pub(crate) fn target_asset(
    (version, mut info): (Version, OsVersionInfo),
) -> Option<(Version, RegistryAsset<Blake3Commitment>, UpdateNotes)> {
    let asset = info.squashfs.remove(&**PLATFORM)?;
    let notes = UpdateNotes {
        version: version.clone(),
        release_notes: info.release_notes,
    };
    Some((version, asset, notes))
}

#[instrument(skip_all)]
pub(crate) async fn do_update(
    client: Client,
    asset: RegistryAsset<Blake3Commitment>,
    notes: UpdateNotes,
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
    save_update_notes(&notes).await.log_err();
    finalize_phase.complete();

    progress.complete();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_params_use_target_version_with_cli_to_flag() {
        let params = UpdateSystemParams::try_parse_from([
            "update",
            "https://registry.start9.com",
            "--to",
            "=0.4.0.2",
            "--trust",
        ])
        .unwrap();
        let value = serde_json::to_value(params).unwrap();
        assert_eq!(value["targetVersion"], "=0.4.0.2:0");
        assert!(value.get("target").is_none());
        assert_eq!(value["trust"], true);
        let default =
            UpdateSystemParams::try_parse_from(["update", "https://registry.start9.com"]).unwrap();
        assert!(!default.trust);
    }
}
