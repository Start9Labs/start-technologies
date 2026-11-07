use clap::Parser;
use http::HeaderMap;
use imbl_value::InternedString;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::PackageId;
use crate::context::CliContext;
use crate::context::config::resolve_profile;
use crate::prelude::*;
use crate::registry::package::get::{GetPackageResponseFull, PackageDetailLevel};
use crate::s9pk::v2::SIG_CONTEXT;
use crate::util::VersionString;

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct CliPromoteParams {
    #[arg(long, help = "help.arg.from-registry-url")]
    pub from: Option<String>,
    #[arg(long, help = "help.arg.to-registry-url")]
    pub to: Option<String>,
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
    #[arg(help = "help.arg.package-version")]
    pub version: VersionString,
}

pub fn registry_rpc_url(url: &Url) -> Result<Url, Error> {
    crate::util::net::rpc_url(url, 0)
}

pub fn resolve_registry_url(explicit: Option<&str>, ctx: &CliContext) -> Result<Url, Error> {
    if let Some(target) = explicit {
        registry_rpc_url(&resolve_profile(ctx.registry_profiles.as_ref(), target)?)
    } else if let Some(url) = &ctx.registry_url {
        Ok(url.get()?.clone())
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
    crate::registry::context::call_cli(
        ctx,
        url,
        HeaderMap::new(),
        sig_context.as_deref().or_not_found("registry hostname")?,
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

    let from_url = resolve_registry_url(from.as_deref(), &ctx)?;
    let to_url = resolve_registry_url(to.as_deref(), &ctx)?;

    let res: Value = call_registry(
        &ctx,
        from_url,
        "package.get",
        imbl_value::json!({
            "id": &id,
            "otherVersions": PackageDetailLevel::Full,
            "allRevisions": true,
        }),
    )
    .await?;

    let response: GetPackageResponseFull = from_value(res)?;

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

    for (_, asset) in &version_info.s9pks {
        let commitment = &asset.commitment;
        let signature = ctx
            .id_key()?
            .sign_commitment(commitment, SIG_CONTEXT)
            .await?;

        call_registry(
            &ctx,
            to_url.clone(),
            "package.add",
            imbl_value::json!({
                "urls": &asset.urls,
                "signature": signature,
                "signatures": &asset.signatures,
                "commitment": commitment,
            }),
        )
        .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::config::ClientConfig;
    use crate::registry::os::promote::CliOsPromoteParams;
    use crate::util::serde::IoFormat;

    fn context(yaml: &str) -> CliContext {
        CliContext::init(
            IoFormat::Yaml
                .from_reader::<_, ClientConfig>(yaml.as_bytes())
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn both_promote_commands_accept_registry_profiles() {
        let ctx = context(
            "host:\n  alpha: https://server/\nregistry:\n  default: https://fallback/\n  alpha: source\n  source: https://alpha/registry\n  production: https://production/registry",
        );
        let package = CliPromoteParams::try_parse_from([
            "promote",
            "--from",
            "alpha",
            "--to",
            "production",
            "example",
            "1.0.0:0",
        ])
        .unwrap();
        let os = CliOsPromoteParams::try_parse_from([
            "promote",
            "--from",
            "alpha",
            "--to",
            "production",
            "0.4.1",
        ])
        .unwrap();
        for (from, to) in [(package.from, package.to), (os.from, os.to)] {
            assert_eq!(
                resolve_registry_url(from.as_deref(), &ctx)
                    .unwrap()
                    .as_str(),
                "https://alpha/registry/rpc/v0",
            );
            assert_eq!(
                resolve_registry_url(to.as_deref(), &ctx).unwrap().as_str(),
                "https://production/registry/rpc/v0",
            );
        }
        assert_eq!(
            resolve_registry_url(None, &ctx).unwrap().as_str(),
            "https://fallback/rpc/v0",
        );
    }

    #[test]
    fn explicit_registry_targets_work_without_a_default() {
        let ctx = context("registry:\n  alpha: https://alpha/");
        assert_eq!(
            resolve_registry_url(Some("alpha"), &ctx).unwrap().as_str(),
            "https://alpha/rpc/v0",
        );
        assert_eq!(
            resolve_registry_url(Some("https://literal/base"), &ctx)
                .unwrap()
                .as_str(),
            "https://literal/base/rpc/v0",
        );
        assert!(resolve_registry_url(None, &ctx).is_err());
        assert_eq!(
            resolve_registry_url(Some("https://literal/"), &context("{}"))
                .unwrap()
                .as_str(),
            "https://literal/rpc/v0",
        );
    }

    #[test]
    fn invalid_explicit_registry_targets_do_not_fall_back() {
        let ctx = context(
            "registry:\n  default: https://fallback/\n  broken: unknown\n  cycle: other\n  other: cycle",
        );
        for target in ["missing", "broken", "cycle", "not a url"] {
            let error = resolve_registry_url(Some(target), &ctx).unwrap_err();
            assert_eq!(error.kind, ErrorKind::InvalidRequest);
        }
    }
}
