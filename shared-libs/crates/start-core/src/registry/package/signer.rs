use std::collections::BTreeMap;

use clap::Parser;
use exver::VersionRange;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::PackageId;
use crate::context::CliContext;
use crate::prelude::*;
use crate::registry::admin::display_package_signers;
use crate::registry::context::RegistryContext;
use crate::registry::signer::SignerInfo;
use crate::rpc_continuations::Guid;
use crate::util::serde::HandlerExtSerde;

pub fn signer_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "add",
            from_fn_async(add_package_signer)
                .with_metadata("admin", Value::Bool(true))
                .no_display()
                .with_about("about.add-package-signer")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove_package_signer)
                .with_metadata("admin", Value::Bool(true))
                .no_display()
                .with_about("about.remove-package-signer")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "list",
            from_fn_async(list_package_signers)
                .with_metadata("authenticated", Value::Bool(false))
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| {
                    display_package_signers(handle.params, result)
                })
                .with_about("about.list-package-signers")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct AddPackageSignerParams {
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
    #[arg(help = "help.arg.signer-id")]
    pub signer: Guid,
    #[arg(long, help = "help.arg.version-range")]
    #[visit(ts(type = "string | null"), wire = "Option<rpc_toolkit::ts::Unknown>")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub versions: Option<VersionRange>,
    #[arg(long, help = "help.arg.merge")]
    pub merge: bool,
}

rpc_toolkit::reflect_ts!(AddPackageSignerParams);
rpc_toolkit::ts_export!(AddPackageSignerParams, namespaces = [""]);

pub async fn add_package_signer(
    ctx: RegistryContext,
    AddPackageSignerParams {
        id,
        signer,
        versions,
        merge,
    }: AddPackageSignerParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            ensure_code!(
                db.as_index().as_signers().contains_key(&signer)?,
                ErrorKind::InvalidRequest,
                "unknown signer {signer}"
            );

            let versions = versions.unwrap_or_default();
            db.as_index_mut()
                .as_package_mut()
                .as_packages_mut()
                .as_idx_mut(&id)
                .or_not_found(&id)?
                .as_authorized_mut()
                .upsert(&signer, || Ok(VersionRange::None))?
                .mutate(|existing| {
                    *existing = if merge {
                        VersionRange::or(existing.clone(), versions)
                    } else {
                        versions
                    };
                    Ok(())
                })?;

            Ok(())
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct RemovePackageSignerParams {
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
    #[arg(help = "help.arg.signer-id")]
    pub signer: Guid,
}

rpc_toolkit::reflect_ts!(RemovePackageSignerParams);
rpc_toolkit::ts_export!(RemovePackageSignerParams, namespaces = [""]);

pub async fn remove_package_signer(
    ctx: RegistryContext,
    RemovePackageSignerParams { id, signer }: RemovePackageSignerParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            if db
                .as_index_mut()
                .as_package_mut()
                .as_packages_mut()
                .as_idx_mut(&id)
                .or_not_found(&id)?
                .as_authorized_mut()
                .remove(&signer)?
                .is_none()
            {
                return Err(Error::new(
                    eyre!(
                        "{}",
                        t!(
                            "registry.package.signer.not-authorized",
                            signer = signer,
                            id = id
                        )
                    ),
                    ErrorKind::NotFound,
                ));
            }

            Ok(())
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct ListPackageSignersParams {
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
}

rpc_toolkit::reflect_ts!(ListPackageSignersParams);
rpc_toolkit::ts_export!(ListPackageSignersParams, namespaces = [""]);

pub async fn list_package_signers(
    ctx: RegistryContext,
    ListPackageSignersParams { id }: ListPackageSignersParams,
) -> Result<BTreeMap<Guid, (SignerInfo, VersionRange)>, Error> {
    let db = ctx.db.peek().await;
    db.as_index()
        .as_package()
        .as_packages()
        .as_idx(&id)
        .or_not_found(&id)?
        .as_authorized()
        .de()?
        .into_iter()
        .filter_map(|(guid, versions)| {
            db.as_index()
                .as_signers()
                .as_idx(&guid)
                .map(|s| s.de().map(|s| (guid, (s, versions))))
        })
        .collect()
}
