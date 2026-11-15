use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::builder::ValueParserFactory;
use clap::{CommandFactory, FromArgMatches, Parser, value_parser};
use color_eyre::eyre::eyre;
use exver::VersionRange;
use imbl_value::{InternedString, json};
use itertools::Itertools;
use reqwest::Url;
use reqwest::header::{CONTENT_LENGTH, HeaderMap};
use rpc_toolkit::HandlerArgs;
use rpc_toolkit::yajrc::RpcError;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::context::{CliContext, RpcContext};
use crate::db::model::package::{ManifestPreference, PackageStateMatchModelRef};
use crate::prelude::*;
use crate::progress::{
    FullProgress, FullProgressTracker, forward_progress, progress_until_complete, with_cli_progress,
};
use crate::registry::context::{RegistryContext, RegistryUrlParams};
use crate::registry::package::get::GetPackageResponse;
use crate::rpc_continuations::{Guid, RpcContinuation, WebSocketPolicy};
use crate::s9pk::manifest::PackageId;
use crate::s9pk::v2::SIG_CONTEXT;
use crate::sign::SignatureMap;
use crate::trusted_keys::{self, TrustContext};
use crate::upload::upload;
use crate::util::io::open_file;
use crate::util::tui::choose;
use crate::util::{FromStrParser, Never, VersionString};

#[cfg(test)]
mod progress_tests;
#[cfg(test)]
mod trust_tests;

pub const PKG_ARCHIVE_DIR: &str = "package-data/archive";
pub const PKG_PUBLIC_DIR: &str = "package-data/public";
pub const PKG_WASM_DIR: &str = "package-data/wasm";

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

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize, VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum MinMax {
    Min,
    Max,
}

rpc_toolkit::reflect_ts!(MinMax);
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
                eyre!("{}", t!("install.invalid-min-max")),
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

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct InstallParams {
    pub accept_signers: Option<crate::registry::signer::AcceptSigners>,
    registry: Url,
    id: PackageId,
    version: VersionString,
    /// Accepts an untrusted signer without skipping signature or integrity verification.
    #[serde(default)]
    pub trust: bool,
}

rpc_toolkit::reflect_ts!(InstallParams);
rpc_toolkit::ts_export!(InstallParams, namespaces = [""]);

#[instrument(skip_all)]
pub async fn install(
    ctx: RpcContext,
    InstallParams {
        registry,
        id,
        version,
        trust,
        accept_signers,
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
                eyre!(
                    "{}",
                    t!(
                        "install.version-not-found-on-registry",
                        id = id,
                        version = version,
                        registry = registry
                    )
                ),
                ErrorKind::NotFound,
            )
        })?;

    let policy = trusted_keys::signer_policy(
        trusted_keys::required(&ctx, TrustContext::Marketplace, trust).await?,
    );
    asset.validate_all(SIG_CONTEXT, policy)?;
    if let Some(policy) = accept_signers {
        asset.validate(SIG_CONTEXT, policy)?;
    }

    let progress_tracker = FullProgressTracker::new();
    let download_progress = progress_tracker.add_phase("Downloading".into(), Some(100));
    let download = ctx
        .services
        .install(
            ctx.clone(),
            || asset.deserialize_s9pk_buffered(ctx.client.get(), download_progress),
            Some(registry),
            None::<Never>,
            Some(progress_tracker),
        )
        .await?;
    tokio::spawn(async move { download.await?.await });

    Ok(())
}

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SideloadParams {
    pub accept_signers: Option<crate::registry::signer::AcceptSigners>,
    /// DER64 supplemental signatures for the uploaded archive.
    #[serde(default)]
    pub signatures: SignatureMap,
    /// Accepts an untrusted signer without skipping signature or integrity verification.
    #[serde(default)]
    trust: bool,
    #[visit(ts(skip))]
    #[serde(rename = "__Auth_signer")]
    signer: Option<InternedString>,
}

rpc_toolkit::reflect_ts!(SideloadParams);
rpc_toolkit::ts_export!(SideloadParams, namespaces = [""]);

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SideloadResponse {
    pub upload: Guid,
    /// Legacy progress continuation, absent for rpcstream observers.
    pub progress: Option<Guid>,
}

rpc_toolkit::reflect_ts!(SideloadResponse);
rpc_toolkit::ts_export!(SideloadResponse, namespaces = [""]);

/// Completion includes installation finalization and uploaded-archive cleanup.
/// Dropping progress does not cancel an accepted sideload.
#[instrument(skip_all)]
pub async fn sideload(
    HandlerArgs {
        context: ctx,
        params:
            SideloadParams {
                signer,
                trust,
                accept_signers,
                signatures,
            },
        rpc_stream,
        ..
    }: HandlerArgs<RpcContext, SideloadParams, rpc_toolkit::Empty, FullProgress>,
) -> Result<SideloadResponse, Error> {
    let progress_tracker = FullProgressTracker::new();
    let (upload, file) = upload(
        &ctx,
        signer.clone(),
        progress_tracker.add_phase("Uploading".into(), Some(100)),
    )
    .await?;
    let done = tokio::spawn({
        let ctx = ctx.clone();
        let progress_tracker = progress_tracker.clone();
        async move {
            let result: Result<(), Error> = async {
                let key = ctx
                    .db
                    .peek()
                    .await
                    .into_private()
                    .into_server_identity_key();
                let server_policy = trusted_keys::signer_policy(
                    trusted_keys::required(&ctx, TrustContext::Marketplace, trust).await?,
                );
                let accepted = match (server_policy, accept_signers) {
                    (Some(server), Some(client)) => {
                        Some(crate::registry::signer::AcceptSigners::All(vec![
                            server, client,
                        ]))
                    }
                    (server, client) => server.or(client),
                };

                ctx.services
                    .install(
                        ctx.clone(),
                        || {
                            crate::s9pk::load_for_sideload(
                                file.clone(),
                                || Ok(key.de()?.0),
                                Some(&progress_tracker),
                                &signatures,
                                accepted,
                            )
                        },
                        None,
                        None::<Never>,
                        Some(progress_tracker.clone()),
                    )
                    .await?
                    .await?
                    .await?;
                file.delete().await
            }
            .await;
            if let Err(error) = &result {
                tracing::error!("Error sideloading package: {error}");
                tracing::debug!("{error:?}");
            }
            result
        }
    });
    let source = progress_until_complete(progress_tracker, async move {
        done.await.with_kind(ErrorKind::Cancelled)?
    });
    let progress = if let Some(stream) = rpc_stream {
        crate::auth::continuation_subscription(&ctx, signer)
            .await?
            .forward_stream(stream, source);
        None
    } else {
        let progress = Guid::new();
        ctx.rpc_continuations
            .add(
                progress.clone(),
                RpcContinuation::ws_authed(
                    crate::auth::continuation_subscription(&ctx, signer).await?,
                    WebSocketPolicy::Authenticated,
                    ctx.secure_session().cloned(),
                    move |socket| async move {
                        if let Err(error) = forward_progress(socket, source).await {
                            tracing::error!("Error tracking sideload progress: {error}");
                        }
                    },
                    Duration::from_secs(600),
                ),
            )
            .await;
        Some(progress)
    };
    Ok(SideloadResponse { upload, progress })
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CancelInstallParams {
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
}

rpc_toolkit::reflect_ts!(CancelInstallParams);
rpc_toolkit::ts_export!(CancelInstallParams, namespaces = [""]);

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
    Marketplace {
        package: QueryPackageParams,
        trust: bool,
    },
    Sideload {
        path: PathBuf,
        trust: bool,
    },
}
impl CommandFactory for CliInstallParams {
    fn command() -> clap::Command {
        use clap::{Arg, Command};
        Command::new("install")
            .arg(
                Arg::new("trust")
                    .long("trust")
                    .action(clap::ArgAction::SetTrue)
                    .help("help.arg.trust"),
            )
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
        let trust = matches.get_flag("trust");
        if let Some(sideload) = matches.get_one::<PathBuf>("sideload") {
            Ok(Self::Sideload {
                path: sideload.clone(),
                trust,
            })
        } else {
            Ok(Self::Marketplace {
                package: QueryPackageParams::from_arg_matches(matches)?,
                trust,
            })
        }
    }
    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
pub struct InstalledVersionParams {
    #[arg(help = "help.arg.package-id")]
    id: PackageId,
}

rpc_toolkit::reflect_ts!(InstalledVersionParams);
rpc_toolkit::ts_export!(InstalledVersionParams, namespaces = [""]);

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
        CliInstallParams::Sideload { path, trust } => {
            let params = local_sideload_params(&path, trust).await?;
            let file = open_file(path).await?;

            let response = ctx
                .call_remote_stream::<FullProgress, ()>(
                    &method[..method.len() - 1]
                        .iter()
                        .chain(std::iter::once(&"sideload"))
                        .join("."),
                    to_value(&params)?,
                )
                .await?;
            let SideloadResponse { upload, .. } = from_value(response.initial)?;

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

            let label = t!("install.sideloading");
            tokio::try_join!(upload, with_cli_progress(response.items, &label))?;
        }
        CliInstallParams::Marketplace {
            package: QueryPackageParams { id, version },
            trust,
        } => {
            let registry = ctx
                .registry_url
                .as_ref()
                .or_not_found("--registry")?
                .get()?
                .clone();
            let source_version: Option<VersionString> = from_value(
                ctx.call_remote::<RpcContext>("package.installed-version", json!({ "id": &id }))
                    .await?,
            )?;
            let mut packages: GetPackageResponse = from_value(
                ctx.call_remote::<RegistryContext>(
                    "package.get",
                    json!({ "id": &id, "targetVersion": &version, "sourceVersion": source_version, "otherVersions": "none" }),
                )
                .await?,
            )?;
            let version = match packages.best.len() {
                0 => {
                    return Err(Error::new(
                        eyre!(
                            "{}",
                            t!(
                                "install.version-not-found-on-registry",
                                id = id,
                                version = version.unwrap_or(VersionRange::Any),
                                registry = registry
                            )
                        ),
                        ErrorKind::NotFound,
                    )
                    .into());
                }
                1 => packages.best.pop_first().map(|(k, _)| k).unwrap(),
                _ => {
                    let versions = packages.best.keys().collect::<Vec<_>>();
                    let version = choose(&t!("install.select-version", id = id), &versions).await?;
                    (*version).clone()
                }
            };
            ctx.call_remote::<RpcContext>(
                &method.join("."),
                to_value(&InstallParams {
                    id,
                    registry,
                    version,
                    trust,
                    accept_signers: None,
                })?,
            )
            .await?;
        }
    }
    Ok(())
}

async fn local_sideload_params(path: &PathBuf, trust: bool) -> Result<SideloadParams, Error> {
    use crate::registry::signer::AcceptSigners;
    use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;

    let mut params = SideloadParams {
        accept_signers: None,
        signatures: SignatureMap::new(),
        trust,
        signer: None,
    };
    let source = Arc::new(MultiCursorFile::from(open_file(path).await?));
    if crate::s9pk::is_v1(&source).await? {
        return Ok(params);
    }
    let package = crate::s9pk::S9pk::deserialize_local(&source, None).await?;
    let signatures = package.signatures().await?;
    params.accept_signers = Some(AcceptSigners::Any(
        signatures
            .keys()
            .cloned()
            .map(AcceptSigners::Signer)
            .collect(),
    ));
    params.signatures = signatures;
    Ok(params)
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
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

rpc_toolkit::reflect_ts!(UninstallParams);
rpc_toolkit::ts_export!(UninstallParams, namespaces = [""]);

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
