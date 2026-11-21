use std::collections::BTreeSet;

use clap::Parser;
use http::HeaderMap;
use imbl_value::InternedString;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::PackageId;
use crate::context::CliContext;
use crate::context::config::resolve_profile;
use crate::prelude::*;
use crate::registry::admin::AddSignerParams;
use crate::registry::package::access::{
    PackageAccess, list_registry_signers, restrict_package_to_signers,
};
use crate::registry::package::get::{GetPackageResponseFull, PackageDetailLevel};
use crate::registry::package::index::PackageIndex;
use crate::rpc_continuations::Guid;
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
        from_url.clone(),
        "package.get",
        imbl_value::json!({
            "id": &id,
            "otherVersions": PackageDetailLevel::Full,
            "allRevisions": true,
        }),
    )
    .await?;

    let response: GetPackageResponseFull = from_value(res.clone())?;

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

    if source_package_is_private(&ctx, from_url.clone(), &id, &res).await? {
        carry_private_access(&ctx, from_url, to_url.clone(), &id).await?;
    }

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

async fn source_package_is_private(
    ctx: &CliContext,
    from: Url,
    id: &PackageId,
    response: &Value,
) -> Result<bool, Error> {
    if let Some(private) = response
        .as_object()
        .and_then(|response| response.get("private"))
    {
        return from_value(private.clone());
    }
    // Registries before 1.7.0 omit `private` from `package.get`.
    let index: PackageIndex =
        from_value(call_registry(ctx, from, "package.index", imbl_value::json!({})).await?)?;
    Ok(index.packages.get(id).or_not_found(id)?.access.private)
}

/// Existing reader signers match by shared keys; their records stay unchanged.
async fn carry_private_access(
    ctx: &CliContext,
    from: Url,
    to: Url,
    id: &PackageId,
) -> Result<(), Error> {
    let access: PackageAccess = from_value(
        call_registry(
            ctx,
            from,
            "package.access.get",
            imbl_value::json!({ "id": id }),
        )
        .await?,
    )?;
    let registered = list_registry_signers(ctx, to.clone()).await?;
    let mut readers = BTreeSet::new();
    for reader in access.readers.values() {
        let matching: BTreeSet<Guid> = registered
            .iter()
            .filter(|(_, signer)| !signer.keys.is_disjoint(&reader.keys))
            .map(|(id, _)| id.clone())
            .collect();
        if matching.is_empty() {
            readers.insert(from_value::<Guid>(
                call_registry(
                    ctx,
                    to.clone(),
                    "admin.signer.add",
                    to_value(&AddSignerParams {
                        name: reader.name.clone(),
                        contact: reader.contact.clone(),
                        keys: reader.keys.iter().map(ToString::to_string).collect(),
                        gpg_exports: reader.gpg_exports.clone(),
                    })?,
                )
                .await?,
            )?);
        } else {
            readers.extend(matching);
        }
    }
    restrict_package_to_signers(ctx, to, id, &readers).await
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

    #[tokio::test]
    async fn legacy_selection_privacy_comes_from_the_source_index() {
        use ed25519_dalek::SigningKey;

        use crate::developer::write_signing_key;
        use crate::registry::context::test_utils::{context as registry_context, registry_server};
        use crate::registry::package::access::{SetPackagePrivateParams, set_package_private};
        use crate::sign::AnyVerifyingKey;

        let temp = tempfile::tempdir().unwrap();
        let key = SigningKey::from_bytes(&[19; 32]);
        let registry = registry_context(
            temp.path().join("registry"),
            AnyVerifyingKey::Ed25519(key.verifying_key()),
        )
        .await;
        let id: PackageId = "private-fixture".parse().unwrap();
        let (url, task) = registry_server(registry.clone()).await;
        let key_path = temp.path().join("identity.pem");
        write_signing_key(&key, &key_path).await.unwrap();
        let cli = CliContext::init(ClientConfig {
            id_key_path: Some(key_path),
            ..Default::default()
        })
        .unwrap();
        let url = registry_rpc_url(&url).unwrap();
        let legacy = imbl_value::json!({
            "categories": [], "best": {}, "otherVersions": {},
        });
        for private in [true, false] {
            set_package_private(
                registry.clone(),
                SetPackagePrivateParams {
                    id: id.clone(),
                    private,
                },
            )
            .await
            .unwrap();
            assert_eq!(
                source_package_is_private(&cli, url.clone(), &id, &legacy)
                    .await
                    .unwrap(),
                private,
            );
        }
        assert_eq!(
            source_package_is_private(
                &cli,
                url.clone(),
                &"missing-fixture".parse().unwrap(),
                &legacy,
            )
            .await
            .unwrap_err()
            .kind,
            ErrorKind::NotFound,
        );
        task.abort();
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
