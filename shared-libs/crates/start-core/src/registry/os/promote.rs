use clap::Parser;
use exver::Version;
use imbl_value::InternedString;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::context::CliContext;
use crate::prelude::*;
use crate::registry::os::SIG_CONTEXT;
use crate::registry::os::index::OsIndex;
use crate::registry::os::version::AddVersionParams;
use crate::registry::package::promote::{call_registry, resolve_registry_url};
use crate::s9pk::git_hash::GitHash;
use crate::sign::commitment::blake3::Blake3Commitment;

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct CliOsPromoteParams {
    #[arg(long, help = "help.arg.from-registry-url")]
    pub from: Option<Url>,
    #[arg(long, help = "help.arg.to-registry-url")]
    pub to: Option<Url>,
    #[arg(help = "help.arg.os-version")]
    pub version: Version,
    #[arg(long, help = "help.arg.expected-os-git-hash")]
    pub expected_git_hash: Option<GitHash>,
}

pub async fn cli_os_promote(
    ctx: CliContext,
    CliOsPromoteParams {
        from,
        to,
        version,
        expected_git_hash,
    }: CliOsPromoteParams,
) -> Result<(), Error> {
    if from.is_none() && to.is_none() {
        return Err(Error::new(
            eyre!("{}", t!("registry.os.promote.need-from-or-to")),
            ErrorKind::InvalidRequest,
        ));
    }

    let from_url = resolve_registry_url(from.as_ref(), &ctx)?;
    let to_url = resolve_registry_url(to.as_ref(), &ctx)?;

    let res: Value = call_registry(&ctx, from_url, "os.index", imbl_value::json!({})).await?;
    let os_index: OsIndex = from_value(res)?;

    let version_info = os_index.versions.0.get(&version).ok_or_else(|| {
        Error::new(
            eyre!(
                "{}",
                t!("registry.os.promote.version-not-found", version = &version)
            ),
            ErrorKind::NotFound,
        )
    })?;

    if let Some(expected) = expected_git_hash {
        version_info.require_git_hash(&expected)?;
    }

    call_registry(
        &ctx,
        to_url.clone(),
        "os.version.add",
        to_value(&AddVersionParams::from_info(version.clone(), version_info))?,
    )
    .await?;

    promote_assets(
        &ctx,
        &to_url,
        &version,
        &version_info.iso,
        "os.asset.add.iso",
    )
    .await?;
    promote_assets(
        &ctx,
        &to_url,
        &version,
        &version_info.squashfs,
        "os.asset.add.squashfs",
    )
    .await?;
    promote_assets(
        &ctx,
        &to_url,
        &version,
        &version_info.img,
        "os.asset.add.img",
    )
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::os::index::OsVersionInfo;

    #[test]
    fn promotion_carries_release_metadata() {
        let info = OsVersionInfo {
            headline: "Security update".into(),
            release_notes: "Release notes".parse().unwrap(),
            source_version: ">=0.4.0".parse().unwrap(),
            git_hash: Some("0123456789abcdef".parse().unwrap()),
            security: true,
            ..Default::default()
        };
        let version: Version = "0.4.0.5".parse().unwrap();
        let params = AddVersionParams::from_info(version.clone(), &info);
        assert_eq!(params.version, version);
        assert_eq!(params.headline, info.headline);
        assert_eq!(params.release_notes, info.release_notes);
        assert_eq!(params.source_version, info.source_version);
        assert_eq!(params.git_hash, info.git_hash);
        assert!(params.security);
        assert!(params.signer.is_none());
    }
}

async fn promote_assets(
    ctx: &CliContext,
    to_url: &Url,
    version: &Version,
    assets: &std::collections::BTreeMap<
        InternedString,
        crate::registry::asset::RegistryAsset<Blake3Commitment>,
    >,
    method: &str,
) -> Result<(), Error> {
    for (platform, asset) in assets {
        let commitment = &asset.commitment;
        let signature = ctx
            .id_key()?
            .sign_commitment(commitment, SIG_CONTEXT)
            .await?;

        call_registry(
            ctx,
            to_url.clone(),
            method,
            imbl_value::json!({
                "version": version,
                "platform": platform,
                "url": &asset.urls[0],
                "signature": signature,
                "commitment": commitment,
            }),
        )
        .await?;
    }
    Ok(())
}
