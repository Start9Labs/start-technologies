use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use color_eyre::eyre::eyre;
use exver::{Version, VersionRange};
use futures::{StreamExt, TryStreamExt};
use http::HeaderMap;
use imbl_value::json;
use itertools::Itertools;
use reqwest::{Client, Url};
use rpc_toolkit::HandlerArgs;
use tokio::sync::oneshot;
use tracing::instrument;

use crate::PLATFORM;
use crate::context::{CliContext, DiagnosticContext};
use crate::middleware::auth::signature::{self, SigningContext};
use crate::prelude::*;
use crate::progress::{FullProgress, FullProgressTracker, PhasedProgressBar, ProgressUnits};
use crate::registry::os::SIG_CONTEXT;
use crate::registry::os::index::OsVersionInfo;
use crate::rpc_continuations::{Guid, RpcContinuation};
use crate::sign::AnySigningKey;
use crate::update::{UpdateProgressHandles, UpdateSystemParams, UpdateSystemRes, do_update};
use crate::version::{Current, VersionT};

/// `SigningContext` for unauthenticated registry calls in diagnostic mode.
struct UnsignedRegistry(Client);
impl AsRef<Client> for UnsignedRegistry {
    fn as_ref(&self) -> &Client {
        &self.0
    }
}
impl SigningContext for UnsignedRegistry {
    fn signing_key(&self) -> Result<AnySigningKey, Error> {
        Err(Error::new(
            eyre!("diagnostic mode has no signing key"),
            ErrorKind::Authorization,
        ))
    }
}

#[instrument(skip_all)]
pub async fn update_system(
    ctx: DiagnosticContext,
    UpdateSystemParams {
        target,
        registry,
        progress: report_progress,
    }: UpdateSystemParams,
) -> Result<UpdateSystemRes, Error> {
    let guard = ctx.update_in_progress.mutate(|slot| {
        if Arc::strong_count(slot) > 1 {
            None
        } else {
            Some(slot.clone())
        }
    });
    let Some(guard) = guard else {
        return Err(Error::new(
            eyre!("{}", t!("update.already-updating")),
            ErrorKind::InvalidRequest,
        ));
    };

    run_update(ctx, target, registry, report_progress, guard).await
}

async fn run_update(
    ctx: DiagnosticContext,
    target: Option<VersionRange>,
    registry: Url,
    report_progress: bool,
    guard: Arc<()>,
) -> Result<UpdateSystemRes, Error> {
    let target_range = target.unwrap_or(VersionRange::Any);
    let client = Client::new();

    let current_version = Current::default().semver();
    let mut available = get_versions(&client, registry, &current_version, &target_range).await?;
    let Some((target_version, asset)) = available
        .pop_last()
        .and_then(|(v, mut info)| info.squashfs.remove(&**PLATFORM).map(|a| (v, a)))
    else {
        return Ok(UpdateSystemRes {
            target: None,
            progress: None,
        });
    };
    if !target_version.satisfies(&target_range) {
        return Err(Error::new(
            eyre!("got back version from registry that does not satisfy {target_range}"),
            ErrorKind::Registry,
        ));
    }
    asset.validate(SIG_CONTEXT, asset.all_signers())?;

    let tracker = FullProgressTracker::new();
    let prune_phase = tracker.add_phase("Pruning Old OS Images".into(), Some(2));
    let mut download_phase = tracker.add_phase("Downloading File".into(), Some(100));
    download_phase.set_total(asset.commitment.size);
    download_phase.set_units(Some(ProgressUnits::Bytes));
    let reverify_phase = tracker.add_phase("Reverifying File".into(), Some(10));
    let finalize_phase = tracker.add_phase("Finalizing Update".into(), Some(1));

    let (done_tx, done_rx) = oneshot::channel();
    let tracker_for_task = tracker.clone();
    tokio::spawn(async move {
        let _guard = guard;
        let res = do_update(
            client,
            asset,
            UpdateProgressHandles {
                progress: tracker_for_task,
                prune_phase,
                download_phase,
                reverify_phase,
                finalize_phase,
            },
        )
        .await;
        if let Err(e) = &res {
            tracing::error!("{}", t!("update.not-successful", error = e.to_string()));
            tracing::debug!("{e:?}");
        }
        let _ = done_tx.send(res);
    });

    let progress = if report_progress {
        let guid = Guid::new();
        let tracker = tracker.clone();
        ctx.rpc_continuations
            .add(
                guid.clone(),
                RpcContinuation::ws(
                    move |ws| async move {
                        if let Err(e) = stream_progress(ws, tracker, done_rx).await {
                            tracing::error!(
                                "{}",
                                t!("update.error-returning-progress", error = e.to_string())
                            );
                            tracing::debug!("{e:?}");
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
        target: Some(target_version),
        progress,
    })
}

async fn stream_progress(
    mut ws: crate::util::net::WebSocket,
    tracker: FullProgressTracker,
    mut done_rx: oneshot::Receiver<Result<(), Error>>,
) -> Result<(), Error> {
    use axum::extract::ws::Message;

    let mut stream = tracker.stream(Some(Duration::from_millis(300)));
    let result: Result<(), Error> = loop {
        tokio::select! {
            snap = stream.next() => {
                let Some(snap) = snap else { break Ok(()); };
                ws.send(Message::Text(
                    serde_json::to_string(&Some(&snap))
                        .with_kind(ErrorKind::Serialization)?
                        .into(),
                ))
                .await
                .with_kind(ErrorKind::Network)?;
            }
            res = &mut done_rx => {
                break res.unwrap_or_else(|_| Err(Error::new(eyre!("update task aborted"), ErrorKind::Cancelled)));
            }
        }
    };
    let mut final_snap = tracker.snapshot();
    if result.is_ok() {
        for phase in &mut final_snap.phases {
            phase.progress.set_complete();
        }
        final_snap.overall.set_complete();
    }
    ws.send(Message::Text(
        serde_json::to_string(&Some(&final_snap))
            .with_kind(ErrorKind::Serialization)?
            .into(),
    ))
    .await
    .with_kind(ErrorKind::Network)?;
    ws.send(Message::Text(
        serde_json::to_string::<Option<&FullProgress>>(&None)
            .with_kind(ErrorKind::Serialization)?
            .into(),
    ))
    .await
    .with_kind(ErrorKind::Network)?;
    let close_msg: Result<&str, String> = match &result {
        Ok(()) => Ok("complete"),
        Err(e) => Err(e.to_string()),
    };
    ws.close_result(close_msg).await.log_err();
    result
}

async fn get_versions(
    client: &Client,
    mut registry: Url,
    source: &Version,
    target: &VersionRange,
) -> Result<BTreeMap<Version, OsVersionInfo>, Error> {
    registry
        .path_segments_mut()
        .map_err(|_| Error::new(eyre!("cannot extend URL path"), ErrorKind::ParseUrl))?
        .push("rpc")
        .push("v0");
    let sig_context = registry.host_str().map(|s| s.to_string());
    let ctx = UnsignedRegistry(client.clone());
    let res = signature::call_remote(
        &ctx,
        registry,
        HeaderMap::new(),
        sig_context.as_deref(),
        "os.version.get",
        json!({
            "sourceVersion": source,
            "targetVersion": target,
            "platform": &**PLATFORM,
        }),
    )
    .await
    .map_err(Error::from)?;
    from_value::<BTreeMap<Version, OsVersionInfo>>(res)
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
            .call_remote::<DiagnosticContext>(
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
                let mut bar = PhasedProgressBar::new(&t!(
                    "update.updating-to-version",
                    version = v.to_string()
                ));
                let mut prev = None;
                while let Some(msg) = ws.try_next().await.with_kind(ErrorKind::Network)? {
                    if let tokio_tungstenite::tungstenite::Message::Text(msg) = msg {
                        if let Some(snap) = serde_json::from_str::<Option<FullProgress>>(&msg)
                            .with_kind(ErrorKind::Deserialization)?
                        {
                            bar.update(&snap);
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
                    bar.update(&prev);
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
