use std::collections::BTreeMap;

use clap::Parser;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::PackageId;
use crate::context::CliContext;
use crate::prelude::*;
use crate::registry::context::RegistryContext;
use crate::registry::package::index::{PackageVersionInfo, ReaderParams};
use crate::registry::reader::ReaderKey;
use crate::rpc_continuations::Guid;
use crate::util::VersionString;
use crate::util::serde::HandlerExtSerde;

pub fn version_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new().subcommand(
        "export",
        from_fn_async(export_package_version)
            .with_metadata("authenticated", Value::Bool(false))
            .with_metadata("get_signer", Value::Bool(true))
            .with_display_serializable()
            .with_about("about.export-package-version")
            .with_call_remote::<CliContext>(),
    )
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct ExportPackageVersionParams {
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
    #[arg(help = "help.arg.package-version")]
    pub version: VersionString,
    #[command(flatten)]
    #[serde(flatten)]
    pub reader: ReaderParams,
}

rpc_toolkit::reflect_ts!(ExportPackageVersionParams);
rpc_toolkit::ts_export!(ExportPackageVersionParams, namespaces = [""]);

/// A package version as stored, for promotion to another registry.
#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct PackageVersionExport {
    /// Unprojected: every s9pk with its keystream and reader ids.
    pub info: PackageVersionInfo,
    /// The reader directory entries the s9pks list.
    pub readers: BTreeMap<Guid, ReaderKey>,
}

rpc_toolkit::reflect_ts!(PackageVersionExport);
rpc_toolkit::ts_export!(PackageVersionExport, namespaces = [""]);

/// Restricted to admins and the package's publishers.
pub async fn export_package_version(
    ctx: RegistryContext,
    ExportPackageVersionParams {
        id,
        version,
        reader,
    }: ExportPackageVersionParams,
) -> Result<PackageVersionExport, Error> {
    let db = ctx.db.peek().await;
    let reader = reader.reader(&db)?;
    let package = db
        .as_index()
        .as_package()
        .visible_package(&id, reader.as_ref())?
        .or_not_found(&id)?;
    if !package.is_managed_by(reader.as_ref())? {
        return Err(Error::new(
            eyre!(
                "{}",
                t!("registry.package.version.export-unauthorized", id = id)
            ),
            ErrorKind::Authorization,
        ));
    }
    let info = package
        .as_versions()
        .as_idx(&version)
        .or_not_found(&lazy_format!("{id}@{version}"))?
        .de()?;
    let directory = db.as_index().as_readers();
    let mut readers = BTreeMap::new();
    for (_, asset) in &info.s9pks {
        for reader_id in &asset.readers {
            if let Some(reader) = directory.as_idx(reader_id) {
                readers.insert(reader_id.clone(), reader.de()?);
            }
        }
    }
    Ok(PackageVersionExport { info, readers })
}
