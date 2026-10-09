use std::collections::BTreeMap;

use chrono::Utc;
use clap::Parser;
use exver::{Version, VersionRange};
use imbl_value::InternedString;
use itertools::Itertools;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::context::CliContext;
use crate::prelude::*;
use crate::registry::context::RegistryContext;
use crate::registry::device_info::DeviceInfo;
use crate::registry::os::index::OsVersionInfo;
use crate::sign::AnyVerifyingKey;
use crate::util::serde::{HandlerExtSerde, WithIoFormat, display_serializable};

pub mod signer;

pub fn version_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "add",
            from_fn_async(add_version)
                .with_metadata("admin", Value::Bool(true))
                .with_metadata("get_signer", Value::Bool(true))
                .no_display()
                .with_about("about.add-os-version")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove_version)
                .with_metadata("admin", Value::Bool(true))
                .no_display()
                .with_about("about.remove-os-version")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "signer",
            signer::signer_api::<C>().with_about("about.add-remove-list-version-signers"),
        )
        .subcommand(
            "get",
            from_fn_async(get_version)
                .with_metadata("authenticated", Value::Bool(false))
                .with_metadata("get_device_info", Value::Bool(true))
                .override_return_ts_as::<super::index::OsVersionInfoMap>()
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| {
                    display_version_info(handle.params, result)
                })
                .with_about("about.get-os-versions-info")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct AddVersionParams {
    #[visit(ts(type = "string"), wire = "rpc_toolkit::ts::Unknown")]
    #[arg(help = "help.arg.os-version")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub version: Version,
    #[arg(help = "help.arg.version-headline")]
    pub headline: String,
    #[arg(help = "help.arg.release-notes")]
    pub release_notes: String,
    #[visit(ts(type = "string"), wire = "rpc_toolkit::ts::Unknown")]
    #[arg(help = "help.arg.source-version-range")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub source_version: VersionRange,
    #[arg(skip)]
    #[visit(ts(skip), wire = "rpc_toolkit::ts::Unknown")]
    #[serde(rename = "__Auth_signer")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub signer: Option<AnyVerifyingKey>,
}

rpc_toolkit::reflect_ts!(AddVersionParams);
rpc_toolkit::ts_export!(AddVersionParams, namespaces = [""]);

pub async fn add_version(
    ctx: RegistryContext,
    AddVersionParams {
        version,
        headline,
        release_notes,
        source_version,
        signer,
    }: AddVersionParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            let signer = signer
                .map(|s| db.as_index().as_signers().get_signer(&s))
                .transpose()?;
            db.as_index_mut()
                .as_os_mut()
                .as_versions_mut()
                .upsert(&version, || Ok(OsVersionInfo::default()))?
                .mutate(|i| {
                    i.headline = headline;
                    i.release_notes = release_notes;
                    i.source_version = source_version;
                    i.authorized.extend(signer);
                    Ok(())
                })
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct RemoveVersionParams {
    #[visit(ts(type = "string"), wire = "rpc_toolkit::ts::Unknown")]
    #[arg(help = "help.arg.os-version")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub version: Version,
}

rpc_toolkit::reflect_ts!(RemoveVersionParams);
rpc_toolkit::ts_export!(RemoveVersionParams, namespaces = [""]);

pub async fn remove_version(
    ctx: RegistryContext,
    RemoveVersionParams { version }: RemoveVersionParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_index_mut()
                .as_os_mut()
                .as_versions_mut()
                .remove(&version)?;
            Ok(())
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct GetOsVersionParams {
    #[visit(ts(type = "string | null"), wire = "Option<rpc_toolkit::ts::Unknown>")]
    #[arg(long = "src", help = "help.arg.source-version")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub source_version: Option<Version>,
    #[visit(ts(type = "string | null"), wire = "Option<rpc_toolkit::ts::Unknown>")]
    #[arg(long, help = "help.arg.target-version-range")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub target_version: Option<VersionRange>,
    #[arg(long = "id", help = "help.arg.server-id")]
    server_id: Option<String>,
    #[visit(ts(type = "string | null"), wire = "Option<rpc_toolkit::ts::Unknown>")]
    #[arg(long, help = "help.arg.platform")]
    #[visit(opaque, type_attributes(visit::wire))]
    platform: Option<InternedString>,
    #[visit(ts(skip), wire = "rpc_toolkit::ts::Unknown")]
    #[arg(skip)]
    #[serde(rename = "__DeviceInfo_device_info")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub device_info: Option<DeviceInfo>,
}

rpc_toolkit::reflect_ts!(GetOsVersionParams);
rpc_toolkit::ts_export!(GetOsVersionParams, namespaces = [""]);

pub async fn get_version(
    ctx: RegistryContext,
    GetOsVersionParams {
        source_version: source,
        target_version: target,
        server_id,
        platform,
        device_info,
    }: GetOsVersionParams,
) -> Result<Value, Error> // BTreeMap<Version, OsVersionInfo>
{
    let source = source.or_else(|| device_info.as_ref().map(|d| d.os.version.clone()));
    let platform = platform.or_else(|| device_info.as_ref().map(|d| d.os.platform.clone()));
    if let (Some(server_id), Some(arch)) = (server_id, &platform) {
        const MAX_SERVER_ID_LEN: usize = 256;
        if server_id.len() <= MAX_SERVER_ID_LEN {
            let created_at = Utc::now().to_rfc3339();
            let arch = arch.to_string();
            let os_version = source.as_ref().map(|v| v.to_string());
            let ctx = ctx.clone();
            tokio::task::spawn_blocking(move || {
                ctx.metrics_db.mutate(|conn| {
                    if let Err(e) = conn.execute(
                        concat!(
                            "INSERT INTO user_activity ",
                            "(created_at, server_id, arch, os_version) ",
                            "VALUES (?1, ?2, ?3, ?4)"
                        ),
                        params![created_at, server_id, arch, os_version],
                    ) {
                        warn!("failed to record user activity metric: {e}");
                    }
                });
            });
        }
    }
    let target = target.unwrap_or(VersionRange::Any);
    let res = to_value::<BTreeMap<Version, OsVersionInfo>>(
        &ctx.db
            .peek()
            .await
            .into_index()
            .into_os()
            .into_versions()
            .into_entries()?
            .into_iter()
            .map(|(v, i)| i.de().map(|i| (v, i)))
            .filter_ok(|(version, info)| {
                platform
                    .as_ref()
                    .map_or(true, |p| info.squashfs.contains_key(p))
                    && version.satisfies(&target)
                    && source
                        .as_ref()
                        .map_or(true, |s| s.satisfies(&info.source_version))
            })
            .collect::<Result<_, _>>()?,
    )?;
    Ok(res)
}

pub fn display_version_info<T>(
    params: WithIoFormat<T>,
    info: Value, // BTreeMap<Version, OsVersionInfo>,
) -> Result<(), Error> {
    use prettytable::*;

    let info = from_value::<BTreeMap<Version, OsVersionInfo>>(info)?;

    if let Some(format) = params.format {
        return display_serializable(format, info);
    }

    let mut table = Table::new();
    table.add_row(row![bc =>
        "VERSION",
        "HEADLINE",
        "RELEASE NOTES",
        "ISO PLATFORMS",
        "IMG PLATFORMS",
        "SQUASHFS PLATFORMS",
    ]);
    for (version, info) in &info {
        table.add_row(row![
            &version.to_string(),
            &info.headline,
            &info.release_notes,
            &info.iso.keys().into_iter().join(", "),
            &info.img.keys().into_iter().join(", "),
            &info.squashfs.keys().into_iter().join(", "),
        ]);
    }
    table.print_tty(false)?;
    Ok(())
}
