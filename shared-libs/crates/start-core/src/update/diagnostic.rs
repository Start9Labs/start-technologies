use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use clap::{ArgAction, Parser};
use color_eyre::eyre::eyre;
use exver::{Version, VersionRange};
use http::HeaderMap;
use imbl_value::json;
use itertools::Itertools;
use reqwest::{Client, Url};
use rpc_toolkit::HandlerArgs;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tracing::instrument;

use crate::PLATFORM;
use crate::context::{CliContext, DiagnosticContext};
use crate::middleware::auth::signature;
use crate::prelude::*;
use crate::progress::{FullProgress, FullProgressTracker, ProgressUnits};
use crate::registry::os::index::OsVersionInfo;
use crate::rpc_continuations::{Guid, RpcContinuation, WebSocketPolicy};
use crate::update::{UpdateProgressHandles, UpdateSystemRes, do_update, target_asset};
use crate::version::{Current, VersionT};

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct DiagnosticUpdateSystemParams {
    /// Accepts an untrusted signer without skipping signature or integrity verification.
    #[arg(long, help = "help.arg.trust")]
    #[serde(default)]
    trust: bool,
    #[arg(skip)]
    #[visit(ts(skip))]
    #[serde(rename = "__Auth_local", default, skip_serializing)]
    local_auth: bool,
    #[arg(
        long = "to",
        value_name = "TARGET",
        help = "help.arg.update-target-version"
    )]
    target_version: Option<VersionRange>,
    #[arg(long = "no-progress", action = ArgAction::SetFalse, help = "help.arg.no-progress")]
    #[serde(default)]
    progress: bool,
}
rpc_toolkit::reflect_ts!(DiagnosticUpdateSystemParams);

impl DiagnosticUpdateSystemParams {
    fn authorize(&self) -> Result<(), Error> {
        if self.trust && !self.local_auth {
            return Err(crate::middleware::auth::local::unauthorized());
        }
        Ok(())
    }
}

#[instrument(skip_all)]
/// Observation ends with the independently owned update task's actual result.
pub async fn update_system(
    HandlerArgs {
        context: ctx,
        params,
        rpc_stream,
        ..
    }: HandlerArgs<
        DiagnosticContext,
        DiagnosticUpdateSystemParams,
        rpc_toolkit::Empty,
        FullProgress,
    >,
) -> Result<UpdateSystemRes, Error> {
    params.authorize()?;
    let DiagnosticUpdateSystemParams {
        target_version: target,
        progress: report_progress,
        trust,
        ..
    } = params;
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

    run_update(
        ctx,
        target,
        crate::trusted_keys::initial_os_registry(),
        report_progress,
        guard,
        trust,
        rpc_stream,
    )
    .await
}

async fn run_update(
    ctx: DiagnosticContext,
    target: Option<VersionRange>,
    registry: Url,
    report_progress: bool,
    guard: Arc<()>,
    trust: bool,
    rpc_stream: Option<rpc_toolkit::RpcStream<FullProgress>>,
) -> Result<UpdateSystemRes, Error> {
    let target_range = target.unwrap_or(VersionRange::Any);
    let client = Client::new();

    let current_version = Current::default().semver();
    let mut available = get_versions(&client, registry, &current_version, &target_range).await?;
    let Some((target_version, asset, notes)) = available.pop_last().and_then(target_asset) else {
        if let Some(stream) = rpc_stream {
            let _ = stream.close(Ok::<_, Error>(()));
        }
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
    let policy = if trust {
        None
    } else {
        Some(super::signer_files::policy().await?)
    };
    super::validate_update_signatures(&asset, policy)?;

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
            notes,
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

    let progress = if let Some(stream) = rpc_stream {
        let source = crate::progress::progress_until_complete(tracker, async move {
            done_rx.await.with_kind(ErrorKind::Cancelled)?
        });
        tokio::spawn(async move {
            let result = stream.forward(source).await;
            let _ = stream.close(result);
        });
        None
    } else if report_progress {
        let guid = Guid::new();
        let tracker = tracker.clone();
        ctx.rpc_continuations
            .add(
                guid.clone(),
                RpcContinuation::ws(
                    WebSocketPolicy::Public,
                    ctx.secure_session().cloned(),
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
    ws: crate::rpc_continuations::WebSocket,
    tracker: FullProgressTracker,
    done_rx: oneshot::Receiver<Result<(), Error>>,
) -> Result<(), Error> {
    crate::progress::forward_progress(
        ws,
        crate::progress::progress_with_null(crate::progress::progress_until_complete(
            tracker,
            async move { done_rx.await.with_kind(ErrorKind::Cancelled)? },
        )),
    )
    .await
}

async fn get_versions(
    client: &Client,
    registry: Url,
    source: &Version,
    target: &VersionRange,
) -> Result<BTreeMap<Version, OsVersionInfo>, Error> {
    let registry = crate::registry::package::promote::registry_rpc_url(&registry)?;
    let res = signature::call_http(
        client.clone(),
        registry,
        HeaderMap::new(),
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
        params: DiagnosticUpdateSystemParams { progress, .. },
        raw_params,
        ..
    }: HandlerArgs<CliContext, DiagnosticUpdateSystemParams>,
) -> Result<(), Error> {
    let method = parent_method.into_iter().chain(method).join(".");
    if !progress {
        super::display_update_started(from_value(
            context
                .call_remote::<DiagnosticContext>(&method, raw_params)
                .await?,
        )?);
        return Ok(());
    }
    let response = context
        .call_diagnostic_stream::<FullProgress, ()>(&method, raw_params)
        .await?;
    super::render_update_progress(response).await
}

#[cfg(test)]
mod tests {
    use rpc_toolkit::HandlerFor;

    use super::*;
    use crate::net::static_server::UiContext;

    #[tokio::test]
    async fn version_queries_keep_public_http_and_deployment_prefixes() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let registry = format!("http://{}/prefix/", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let app = axum::Router::new().route(
            "/prefix/rpc/v0",
            axum::routing::post(
                |headers: HeaderMap, axum::Json(body): axum::Json<serde_json::Value>| async move {
                    assert!(headers.get(signature::AUTH_SIG_HEADER).is_none());
                    assert_eq!(body["method"], "os.version.get");
                    assert_eq!(body["params"]["sourceVersion"], "0.4.2");
                    axum::Json(serde_json::json!({"jsonrpc":"2.0","id":0,"result":{}}))
                },
            ),
        );
        let _server: crate::util::future::NonDetachingJoinHandle<()> =
            tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }).into();
        let versions = get_versions(
            &Client::new(),
            registry,
            &"0.4.2".parse().unwrap(),
            &"*".parse().unwrap(),
        )
        .await
        .unwrap();
        assert!(versions.is_empty());
    }

    #[test]
    fn trust_override_requires_injected_local_authentication() {
        for trust in [false, true] {
            for local_auth in [false, true] {
                let params: DiagnosticUpdateSystemParams = serde_json::from_value(
                    serde_json::json!({"trust": trust, "__Auth_local": local_auth}),
                )
                .unwrap();
                assert_eq!(params.authorize().is_ok(), !trust || local_auth);
                assert!(
                    serde_json::to_value(&params)
                        .unwrap()
                        .get("__Auth_local")
                        .is_none()
                );
            }
        }
        let params: DiagnosticUpdateSystemParams =
            serde_json::from_value(serde_json::json!({"trust": true})).unwrap();
        assert!(params.authorize().is_err());
    }

    #[test]
    fn diagnostic_ui_keeps_signed_updates_public() {
        let metadata =
            DiagnosticContext::api().metadata(["diagnostic", "update"].into_iter().collect());
        assert_eq!(metadata.get("authenticated"), Some(&Value::Bool(false)));
        assert_eq!(metadata.get("get_local_auth"), Some(&Value::Bool(true)));
    }
}
