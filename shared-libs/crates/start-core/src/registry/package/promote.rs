use std::collections::BTreeMap;

use clap::Parser;
use http::HeaderMap;
use imbl_value::InternedString;
use rpc_toolkit::yajrc::{METHOD_NOT_FOUND_ERROR, RpcError};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::PackageId;
use crate::context::CliContext;
use crate::context::config::resolve_profile;
use crate::prelude::*;
use crate::registry::package::add::AddPackageRequest;
use crate::registry::package::get::{GetPackageResponseFull, PackageDetailLevel};
use crate::registry::package::index::PackageVersionInfo;
use crate::registry::package::version::PackageVersionExport;
use crate::registry::reader::ReaderKey;
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
    call_registry_rpc(ctx, url, method, params)
        .await
        .map_err(Error::from)
}

async fn call_registry_rpc(
    ctx: &CliContext,
    url: Url,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
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
}

fn is_method_not_found(error: &RpcError) -> bool {
    error.code == METHOD_NOT_FOUND_ERROR.code
}

/// None when the source registry predates `package.version.export`.
async fn export_version(
    ctx: &CliContext,
    url: Url,
    id: &PackageId,
    version: &VersionString,
) -> Result<Option<PackageVersionExport>, Error> {
    match call_registry_rpc(
        ctx,
        url,
        "package.version.export",
        imbl_value::json!({ "id": id, "version": version }),
    )
    .await
    {
        Ok(export) => Ok(Some(from_value(export)?)),
        Err(e) if is_method_not_found(&e) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

async fn listed_version(
    ctx: &CliContext,
    url: Url,
    id: &PackageId,
    version: &VersionString,
) -> Result<PackageVersionInfo, Error> {
    let mut response: GetPackageResponseFull = from_value(
        call_registry(
            ctx,
            url,
            "package.get",
            imbl_value::json!({
                "id": id,
                "otherVersions": PackageDetailLevel::Full,
                "allRevisions": true,
            }),
        )
        .await?,
    )?;
    response
        .best
        .remove(version)
        .or_else(|| response.other_versions.remove(version))
        .ok_or_else(|| {
            Error::new(
                eyre!(
                    "{}",
                    t!(
                        "registry.package.promote.version-not-found",
                        id = id,
                        version = version
                    )
                ),
                ErrorKind::NotFound,
            )
        })
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

    let (s9pks, source_readers) =
        match export_version(&ctx, from_url.clone(), &id, &version).await? {
            Some(export) => (export.info.s9pks, export.readers),
            None => {
                let info = listed_version(&ctx, from_url, &id, &version).await?;
                if info.s9pks.iter().any(|(_, asset)| asset.private.is_some()) {
                    return Err(Error::new(
                        eyre!(
                            "{}",
                            t!(
                                "registry.package.promote.export-required",
                                id = &id,
                                version = &version
                            )
                        ),
                        ErrorKind::InvalidRequest,
                    ));
                }
                (info.s9pks, BTreeMap::new())
            }
        };
    let mut destination_readers = None;

    for (_, asset) in &s9pks {
        let mut readers = Vec::new();
        for id in &asset.readers {
            readers.push(source_readers.get(id).or_not_found(id)?.clone());
        }
        if !readers.is_empty() {
            let mut registered = match destination_readers.take() {
                Some(registered) => registered,
                None => list_readers(&ctx, to_url.clone()).await.map_err(|e| {
                    Error::new(
                        eyre!(
                            "{}",
                            t!(
                                "registry.package.promote.destination-admin-required",
                                error = e
                            )
                        ),
                        ErrorKind::Authorization,
                    )
                })?,
            };
            for reader in &readers {
                if !registered.values().any(|r| r.key == reader.key) {
                    let id: Guid = from_value(
                        call_registry(&ctx, to_url.clone(), "admin.reader.add", to_value(reader)?)
                            .await?,
                    )?;
                    registered.insert(id, reader.clone());
                }
            }
            destination_readers = Some(registered);
        }
        let reader_keys: Vec<_> = readers.into_iter().map(|r| r.key).collect();
        let commitment = &asset.commitment;
        let signature = ctx
            .id_key()?
            .sign_commitment(commitment, SIG_CONTEXT)
            .await?;

        call_registry(
            &ctx,
            to_url.clone(),
            "package.add",
            to_value(&AddPackageRequest {
                urls: &asset.urls,
                signature,
                signatures: &asset.signatures,
                commitment,
                private: asset.private.as_ref(),
                readers: &reader_keys,
            })?,
        )
        .await?;
    }

    Ok(())
}

async fn list_readers(ctx: &CliContext, url: Url) -> Result<BTreeMap<Guid, ReaderKey>, Error> {
    from_value(call_registry(ctx, url, "admin.reader.list", imbl_value::json!({})).await?)
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
    async fn only_a_missing_export_method_selects_the_listing_fallback() {
        use crate::registry::context::test_utils::{context as registry_context, registry_server};
        use crate::sign::{AnySigningKey, Signer};

        let temp = tempfile::tempdir().unwrap();
        let key = AnySigningKey::generate();
        let registry = registry_context(temp.path().join("registry"), key.verifying_key()).await;
        let (url, _task) = registry_server(registry).await;
        let cli = CliContext::init(ClientConfig {
            id_key_path: Some(temp.path().join("identity.pem")),
            ..Default::default()
        })
        .unwrap();
        cli.id_key.set(Signer::from(key)).unwrap();
        let url = registry_rpc_url(&url).unwrap();
        let missing_method = call_registry_rpc(
            &cli,
            url.clone(),
            "package.version.absent",
            imbl_value::json!({}),
        )
        .await
        .unwrap_err();
        assert!(is_method_not_found(&missing_method), "{missing_method:?}");
        let id: PackageId = "absent".parse().unwrap();
        let version: VersionString = "1.0.0:0".parse().unwrap();
        let missing_package = export_version(&cli, url, &id, &version).await.unwrap_err();
        assert_eq!(missing_package.kind, ErrorKind::NotFound);
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
