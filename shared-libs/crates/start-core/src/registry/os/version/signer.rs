use std::collections::BTreeMap;

use clap::Parser;
use exver::Version;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use visit_rs::ts::TS;

use crate::context::CliContext;
use crate::prelude::*;
use crate::registry::admin::display_signers;
use crate::registry::context::RegistryContext;
use crate::registry::signer::SignerInfo;
use crate::rpc_continuations::Guid;
use crate::util::serde::HandlerExtSerde;

pub fn signer_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "add",
            from_fn_async(add_version_signer)
                .with_metadata("admin", Value::Bool(true))
                .no_display()
                .with_about("about.add-version-signer")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove_version_signer)
                .with_metadata("admin", Value::Bool(true))
                .no_display()
                .with_about("about.remove-version-signer")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "list",
            from_fn_async(list_version_signers)
                .with_metadata("authenticated", Value::Bool(false))
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| display_signers(handle.params, result))
                .with_about("about.list-version-signers")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Debug, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct VersionSignerParams {
    pub version: Version,
    pub signer: Guid,
}

pub async fn add_version_signer(
    ctx: RegistryContext,
    VersionSignerParams { version, signer }: VersionSignerParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            ensure_code!(
                db.as_index().as_signers().contains_key(&signer)?,
                ErrorKind::InvalidRequest,
                "unknown signer {signer}"
            );

            db.as_index_mut()
                .as_os_mut()
                .as_versions_mut()
                .as_idx_mut(&version)
                .or_not_found(&version)?
                .as_authorized_mut()
                .mutate(|s| Ok(s.insert(signer)))?;

            Ok(())
        })
        .await
        .result
}

pub async fn remove_version_signer(
    ctx: RegistryContext,
    VersionSignerParams { version, signer }: VersionSignerParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            if !db
                .as_index_mut()
                .as_os_mut()
                .as_versions_mut()
                .as_idx_mut(&version)
                .or_not_found(&version)?
                .as_authorized_mut()
                .mutate(|s| Ok(s.remove(&signer)))?
            {
                return Err(Error::new(
                    eyre!(
                        "{}",
                        t!(
                            "registry.os.version.signer-not-authorized",
                            signer = signer,
                            version = version
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

#[derive(Debug, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ListVersionSignersParams {
    pub version: Version,
}

pub async fn list_version_signers(
    ctx: RegistryContext,
    ListVersionSignersParams { version }: ListVersionSignersParams,
) -> Result<BTreeMap<Guid, SignerInfo>, Error> {
    let db = ctx.db.peek().await;
    db.as_index()
        .as_os()
        .as_versions()
        .as_idx(&version)
        .or_not_found(&version)?
        .as_authorized()
        .de()?
        .into_iter()
        .filter_map(|guid| {
            db.as_index()
                .as_signers()
                .as_idx(&guid)
                .map(|s| s.de().map(|s| (guid, s)))
        })
        .collect()
}
