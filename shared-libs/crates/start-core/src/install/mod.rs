use std::ops::Deref;
use std::path::PathBuf;
use std::time::Duration;

use axum::extract::ws;
use clap::builder::ValueParserFactory;
use clap::{CommandFactory, FromArgMatches, Parser, value_parser};
use color_eyre::eyre::eyre;
use exver::VersionRange;
use futures::StreamExt;
use imbl_value::{InternedString, json};
use itertools::Itertools;
use reqwest::Url;
use reqwest::header::{CONTENT_LENGTH, HeaderMap};
use rpc_toolkit::HandlerArgs;
use rpc_toolkit::yajrc::RpcError;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tracing::instrument;
use ts_rs::TS;

use crate::context::{CliContext, RpcContext};
use crate::registry::asset::BufferedHttpSource;
use crate::db::model::package::{ManifestPreference, PackageStateMatchModelRef};
use crate::prelude::*;
use crate::progress::{FullProgress, FullProgressTracker, PhasedProgressBar};
use crate::registry::context::{RegistryContext, RegistryUrlParams};
use crate::registry::package::get::GetPackageResponse;
use crate::rpc_continuations::{Guid, RpcContinuation};
use crate::s9pk::manifest::PackageId;
use crate::s9pk::v2::SIG_CONTEXT;
use crate::upload::upload;
use crate::util::io::open_file;
use crate::util::tui::choose;
use crate::util::{FromStrParser, Never, VersionString};

pub const PKG_ARCHIVE_DIR: &str = "package-data/archive";
pub const PKG_PUBLIC_DIR: &str = "package-data/public";
pub const PKG_WASM_DIR: &str = "package-data/wasm";

// #[command(display(display_serializable))]
pub async fn list(ctx: RpcContext) -> Result<Vec<Value>, Error> {
    Ok(ctx
        .db
        .peek()
        .await
        .as_public()
        .as_package_data()
        .as_entries()?
        .iter()
        .filter_map(|(id, pde)| {
            let status = match pde.as_state_info().as_match() {
                PackageStateMatchModelRef::Installed(_) => "installed",
                PackageStateMatchModelRef::Installing(_) => "installing",
                PackageStateMatchModelRef::Updating(_) => "updating",
                PackageStateMatchModelRef::Restoring(_) => "restoring",
                PackageStateMatchModelRef::Removing(_) => "removing",
                PackageStateMatchModelRef::Error(_) => "error",
            };
            Some(json!({
                "status": status,
                "id": id.clone(),
                "version": pde.as_state_info()
                    .as_manifest(ManifestPreference::Old)
                    .as_version()
                    .de()
                    .ok()?
            }))
        })
        .collect())
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum MinMax {
    Min,
    Max,
}
impl Default for MinMax {
    fn default() -> Self {
        MinMax::Max
    }
}
impl std::str::FromStr for MinMax {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "min" => Ok(MinMax::Min),
            "max" => Ok(MinMax::Max),
            _ => Err(Error::new(
                eyre!("Must be one of \"min\", \"max\"."),
                crate::ErrorKind::ParseVersion,
            )),
        }
    }
}
impl ValueParserFactory for MinMax {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        FromStrParser::new()
    }
}
impl std::fmt::Display for MinMax {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MinMax::Min => write!(f, "min"),
            MinMax::Max => write!(f, "max"),
        }
    }
}

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InstallParams {
    #[ts(type = "string")]
    registry: Url,
    id: PackageId,
    version: VersionString,
}

#[instrument(skip_all)]
pub async fn install(
    ctx: RpcContext,
    InstallParams {
        registry,
        id,
        version,
    }: InstallParams,
) -> Result<(), Error> {
    let package: GetPackageResponse = from_value(
        ctx.call_remote_with::<RegistryContext, _>(
            "package.get",
            [("get_device_info", Value::Bool(true))]
                .into_iter()
                .collect(),
            json!({
                "id": id,
                "targetVersion": VersionRange::exactly(version.deref().clone()),
                "otherVersions": "none",
            }),
            RegistryUrlParams {
                registry: registry.clone(),
            },
        )
        .await?,
    )?;

    let (_, asset) = package
        .best
        .get(&version)
        .and_then(|i| i.s9pks.first())
        .ok_or_else(|| {
            Error::new(
                eyre!("{id}@{version} not found on {registry}"),
                ErrorKind::NotFound,
            )
        })?;

    asset.validate(SIG_CONTEXT, asset.all_signers())?;

    let progress_tracker = FullProgressTracker::new();
    let download_progress = progress_tracker.add_phase("Downloading".into(), Some(100));
    let download = ctx
        .services
        .install(
            ctx.clone(),
            || asset.deserialize_s9pk_buffered(ctx.client.clone(), download_progress),
            Some(registry),
            None::<Never>,
            Some(progress_tracker),
        )
        .await?;
    tokio::spawn(async move { download.await?.await });

    Ok(())
}

#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SideloadParams {
    #[ts(skip)]
    #[serde(rename = "__Auth_session")]
    session: Option<InternedString>,
}

#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SideloadResponse {
    pub upload: Guid,
    pub progress: Guid,
}

#[instrument(skip_all)]
pub async fn sideload(
    ctx: RpcContext,
    SideloadParams { session }: SideloadParams,
) -> Result<SideloadResponse, Error> {
    let (err_send, mut err_recv) = oneshot::channel::<Error>();
    let progress = Guid::new();
    let progress_tracker = FullProgressTracker::new();
    let (upload, file) = upload(
        &ctx,
        session.clone(),
        progress_tracker.add_phase("Uploading".into(), Some(100)),
    )
    .await?;
    let mut progress_listener = progress_tracker.stream(Some(Duration::from_millis(200)));
    ctx.rpc_continuations
        .add(
            progress.clone(),
            RpcContinuation::ws_authed(
                &ctx,
                session,
                |mut ws| async move {
                    if let Err(e) = async {
                        loop {
                            tokio::select! {
                                progress = progress_listener.next() => {
                                    if let Some(progress) = progress {
                                        ws.send(ws::Message::Text(
                                            serde_json::to_string(&progress)
                                                .with_kind(ErrorKind::Serialization)?
                                                .into(),
                                        ))
                                        .await
                                        .with_kind(ErrorKind::Network)?;
                                        if progress.overall.is_complete() {
                                            return ws.normal_close("complete").await;
                                        }
                                    } else {
                                        return ws.normal_close("complete").await;
                                    }
                                }
                                msg = ws.recv() => {
                                    if msg.transpose().with_kind(ErrorKind::Network)?.is_none() {
                                        return Ok(())
                                    }
                                }
                                err = (&mut err_recv) => {
                                    if let Ok(e) = err {
                                        ws.close_result(Err::<&str, _>(e.clone_output())).await?;
                                        return Err(e)
                                    }
                                }
                            }
                        }
                    }
                    .await
                    {
                        tracing::error!("Error tracking sideload progress: {e}");
                        tracing::debug!("{e:?}");
                    }
                },
                Duration::from_secs(600),
            ),
        )
        .await;
    tokio::spawn(async move {
        if let Err(e) = async {
            let key = ctx.db.peek().await.into_private().into_developer_key();

            ctx.services
                .install(
                    ctx.clone(),
                    || crate::s9pk::load(file.clone(), || Ok(key.de()?.0), Some(&progress_tracker)),
                    None,
                    None::<Never>,
                    Some(progress_tracker.clone()),
                )
                .await?
                .await?
                .await?;
            file.delete().await
        }
        .await
        {
            tracing::error!("Error sideloading package: {e}");
            tracing::debug!("{e:?}");
            let _ = err_send.send(e);
        }
    });
    Ok(SideloadResponse { upload, progress })
}

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SideloadUrlParams {
    #[ts(type = "string")]
    url: Url,
}

#[instrument(skip_all)]
pub async fn sideload_url(
    ctx: RpcContext,
    SideloadUrlParams { url }: SideloadUrlParams,
) -> Result<(), Error> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Error::new(
            eyre!("URL scheme must be http or https, got: {}", url.scheme()),
            ErrorKind::InvalidRequest,
        ));
    }

    let progress_tracker = FullProgressTracker::new();
    let download_progress = progress_tracker.add_phase("Downloading".into(), Some(100));
    let client = ctx.client.clone();
    let db = ctx.db.clone();
    let pt_ref = progress_tracker.clone();

    let download = ctx
        .services
        .install(
            ctx.clone(),
            || async move {
                let source = BufferedHttpSource::new(client, url, download_progress).await?;
                let key = db.peek().await.into_private().into_developer_key();
                crate::s9pk::load(source, || Ok(key.de()?.0), Some(&pt_ref)).await
            },
            None,
            None::<Never>,
            Some(progress_tracker),
        )
        .await?;

    tokio::spawn(async move {
        if let Err(e) = async { download.await?.await }.await {
            tracing::error!("Error sideloading package from URL: {e}");
            tracing::debug!("{e:?}");
        }
    });

    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CancelInstallParams {
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
}

#[instrument(skip_all)]
pub fn cancel_install(
    ctx: RpcContext,
    CancelInstallParams { id }: CancelInstallParams,
) -> Result<(), Error> {
    if let Some(cancel) = ctx.cancellable_installs.mutate(|c| c.remove(&id)) {
        cancel.send(()).ok();
    }
    Ok(())
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
pub struct QueryPackageParams {
    #[arg(help = "help.arg.package-id")]
    id: PackageId,
    #[arg(help = "help.arg.version-range")]
    version: Option<VersionRange>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CliInstallParams {
    Marketplace(QueryPackageParams),
    Sideload(PathBuf),
}
impl CommandFactory for CliInstallParams {
    fn command() -> clap::Command {
        use clap::{Arg, Command};
        Command::new("install")
            .arg(
                Arg::new("sideload")
                    .long("sideload")
                    .short('s')
                    .required_unless_present("id")
                    .value_parser(value_parser!(PathBuf)),
            )
            .args(
                QueryPackageParams::command()
                    .get_arguments()
                    .cloned()
                    .map(|a| {
                        if a.get_id() == "id" {
                            a.required(false).required_unless_present("sideload")
                        } else {
                            a
                        }
                        .conflicts_with("sideload")
                    }),
            )
    }
    fn command_for_update() -> clap::Command {
        Self::command()
    }
}
impl FromArgMatches for CliInstallParams {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        if let Some(sideload) = matches.get_one::<PathBuf>("sideload") {
            Ok(Self::Sideload(sideload.clone()))
        } else {
            Ok(Self::Marketplace(QueryPackageParams::from_arg_matches(
                matches,
            )?))
        }
    }
    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
pub struct InstalledVersionParams {
    #[arg(help = "help.arg.package-id")]
    id: PackageId,
}

pub async fn installed_version(
    ctx: RpcContext,
    InstalledVersionParams { id }: InstalledVersionParams,
) -> Result<Option<VersionString>, Error> {
    if let Some(pde) = ctx
        .db
        .peek()
        .await
        .into_public()
        .into_package_data()
        .into_idx(&id)
    {
        Ok(Some(
            pde.into_state_info()
                .as_manifest(ManifestPreference::Old)
                .as_version()
                .de()?,
        ))
    } else {
        Ok(None)
    }
}

#[instrument(skip_all)]
pub async fn cli_install(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        params,
        ..
    }: HandlerArgs<CliContext, CliInstallParams>,
) -> Result<(), RpcError> {
    let method = parent_method.into_iter().chain(method).collect_vec();
    match params {
        CliInstallParams::Sideload(path) => {
            let file = open_file(path).await?;

            // rpc call remote sideload
            let SideloadResponse { upload, progress } = from_value::<SideloadResponse>(
                ctx.call_remote::<RpcContext>(
                    &method[..method.len() - 1]
                        .into_iter()
                        .chain(std::iter::once(&"sideload"))
                        .join("."),
                    imbl_value::json!({}),
                )
                .await?,
            )?;

            let upload = async {
                let content_length = file.metadata().await?.len();
                ctx.rest_continuation(
                    upload,
                    reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(file)),
                    {
                        let mut map = HeaderMap::new();
                        map.insert(CONTENT_LENGTH, content_length.into());
                        map
                    },
                )
                .await?
                .error_for_status()
                .with_kind(ErrorKind::Network)?;
                Ok::<_, Error>(())
            };

            let progress = async {
                use tokio_tungstenite::tungstenite::Message;

                let mut bar = PhasedProgressBar::new("Sideloading");

                let mut ws = ctx.ws_continuation(progress).await?;

                let mut progress = FullProgress::new();

                loop {
                    tokio::select! {
                        msg = ws.next() => {
                            if let Some(msg) = msg {
                                match msg.with_kind(ErrorKind::Network)? {
                                    Message::Text(t) => {
                                        progress =
                                            serde_json::from_str::<FullProgress>(&t)
                                                .with_kind(ErrorKind::Deserialization)?;
                                        bar.update(&progress);
                                    }
                                    Message::Close(Some(c)) if c.code != CloseCode::Normal => {
                                        return Err(Error::new(eyre!("{}", c.reason), ErrorKind::Network))
                                    }
                                    _ => (),
                                }
                            } else {
                                break;
                            }
                        }
                        _ = tokio::time::sleep(Duration::from_millis(100)) => {
                            bar.update(&progress);
                        },
                    }
                }

                Ok::<_, Error>(())
            };

            let (upload, progress) = tokio::join!(upload, progress);
            progress?;
            upload?;
        }
        CliInstallParams::Marketplace(QueryPackageParams { id, version }) => {
            let source_version: Option<VersionString> = from_value(
                ctx.call_remote::<RpcContext>("package.installed-version", json!({ "id": &id }))
                    .await?,
            )?;
            let mut packages: GetPackageResponse = from_value(
                ctx.call_remote::<RegistryContext>(
                    "package.get",
                    json!({ "id": &id, "targetVersion": version, "sourceVersion": source_version, "otherVersions": "none" }),
                )
                .await?,
            )?;
            let version = if packages.best.len() == 1 {
                packages.best.pop_first().map(|(k, _)| k).unwrap()
            } else {
                let versions = packages.best.keys().collect::<Vec<_>>();
                let version = choose(
                    &format!(
                        concat!(
                            "Multiple flavors of {id} found. ",
                            "Please select one of the following versions to install:"
                        ),
                        id = id
                    ),
                    &versions,
                )
                .await?;
                (*version).clone()
            };
            ctx.call_remote::<RpcContext>(
                &method.join("."),
                to_value(&InstallParams {
                    id,
                    registry: ctx.registry_url.clone().or_not_found("--registry")?,
                    version,
                })?,
            )
            .await?;
        }
    }
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct UninstallParams {
    #[arg(help = "help.arg.package-id")]
    id: PackageId,
    #[arg(long, help = "help.arg.soft-uninstall")]
    #[serde(default)]
    soft: bool,
    #[arg(long, help = "help.arg.force-uninstall")]
    #[serde(default)]
    force: bool,
}

pub async fn uninstall(
    ctx: RpcContext,
    UninstallParams { id, soft, force }: UninstallParams,
) -> Result<(), Error> {
    let fut = ctx
        .services
        .uninstall(ctx.clone(), id.clone(), soft, force)
        .await?;

    tokio::spawn(async move {
        if let Err(e) = fut.await {
            tracing::error!("Error uninstalling service {id}: {e}");
            tracing::debug!("{e:?}");
        }
    });

    Ok(())
}
