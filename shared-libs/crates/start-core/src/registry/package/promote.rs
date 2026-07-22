use clap::Parser;
use http::HeaderMap;
use imbl_value::InternedString;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::PackageId;
use crate::context::CliContext;
use crate::prelude::*;
use crate::registry::package::get::{GetPackageResponseFull, PackageDetailLevel};
use crate::s9pk::v2::SIG_CONTEXT;
use crate::sign::ed25519::Ed25519;
use crate::sign::{AnySignature, SignatureScheme};
use crate::util::VersionString;

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct CliPromoteParams {
    #[arg(long, help = "help.arg.from-registry-url")]
    pub from: Option<Url>,
    #[arg(long, help = "help.arg.to-registry-url")]
    pub to: Option<Url>,
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
    #[arg(help = "help.arg.package-version")]
    pub version: VersionString,
}

pub fn registry_rpc_url(url: &Url) -> Result<Url, Error> {
    let mut url = url.clone();
    url.path_segments_mut()
        .map_err(|_| eyre!("Url cannot be base"))
        .with_kind(ErrorKind::ParseUrl)?
        .push("rpc")
        .push("v0");
    Ok(url)
}

pub fn resolve_registry_url(explicit: Option<&Url>, ctx: &CliContext) -> Result<Url, Error> {
    if let Some(url) = explicit {
        registry_rpc_url(url)
    } else if let Some(url) = &ctx.registry_url {
        Ok(url.clone())
    } else {
        Err(Error::new(
            eyre!("{}", t!("registry.context.registry-required")),
            ErrorKind::InvalidRequest,
        ))
    }
}

pub async fn call_registry(
    ctx: &CliContext,
    url: Url,
    method: &str,
    params: Value,
) -> Result<Value, Error> {
    let sig_context = url.host().as_ref().map(InternedString::from_display);
    crate::middleware::auth::signature::call_remote(
        ctx,
        url,
        HeaderMap::new(),
        sig_context.as_deref(),
        method,
        params,
    )
    .await
    .map_err(Error::from)
}

pub async fn cli_promote(
    ctx: CliContext,
    CliPromoteParams {
        from,
        to,
        id,
        version,
    }: CliPromoteParams,
) -> Result<(), Error> {
    if from.is_none() && to.is_none() {
        return Err(Error::new(
            eyre!("{}", t!("registry.package.promote.need-from-or-to")),
            ErrorKind::InvalidRequest,
        ));
    }

    let from_url = resolve_registry_url(from.as_ref(), &ctx)?;
    let to_url = resolve_registry_url(to.as_ref(), &ctx)?;

    // Fetch package info from source registry
    let res: Value = call_registry(
        &ctx,
        from_url,
        "package.get",
        imbl_value::json!({
            "id": &id,
            "otherVersions": PackageDetailLevel::Full,
        }),
    )
    .await?;

    let response: GetPackageResponseFull = from_value(res)?;

    // Find the target version
    let version_info = response
        .best
        .get(&version)
        .or_else(|| response.other_versions.get(&version))
        .ok_or_else(|| {
            Error::new(
                eyre!(
                    "{}",
                    t!(
                        "registry.package.promote.version-not-found",
                        id = &id,
                        version = &version
                    )
                ),
                ErrorKind::NotFound,
            )
        })?;

    // Promote each s9pk variant to the target registry
    for (_, asset) in &version_info.s9pks {
        let commitment = &asset.commitment;
        let signature = Ed25519.sign_commitment(ctx.id_key()?, commitment, SIG_CONTEXT)?;

        call_registry(
            &ctx,
            to_url.clone(),
            "package.add",
            imbl_value::json!({
                "urls": &asset.urls,
                "signature": AnySignature::Ed25519(signature),
                "commitment": commitment,
            }),
        )
        .await?;
    }

    Ok(())
}
