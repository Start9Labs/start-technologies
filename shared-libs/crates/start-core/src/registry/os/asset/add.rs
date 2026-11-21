use std::collections::BTreeMap;
use std::panic::UnwindSafe;
use std::path::PathBuf;

use chrono::Utc;
use clap::Parser;
use exver::Version;
use imbl_value::InternedString;
use itertools::Itertools;
use rpc_toolkit::{Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::context::CliContext;
use crate::prelude::*;
use crate::progress::{FullProgressTracker, ProgressUnits};
use crate::registry::asset::RegistryAsset;
use crate::registry::context::RegistryContext;
use crate::registry::os::SIG_CONTEXT;
use crate::registry::os::asset::may_manage_assets;
use crate::registry::os::index::OsVersionInfo;
use crate::s9pk::merkle_archive::source::ArchiveSource;
use crate::s9pk::merkle_archive::source::http::HttpSource;
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::sign::commitment::blake3::Blake3Commitment;
use crate::sign::{AnySignature, AnyVerifyingKey, SignatureMap};
use crate::util::io::open_file;
use crate::util::serde::Base64;

pub fn add_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "iso",
            from_fn_async(add_iso)
                .with_metadata("get_signer", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "img",
            from_fn_async(add_img)
                .with_metadata("get_signer", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "squashfs",
            from_fn_async(add_squashfs)
                .with_metadata("get_signer", Value::Bool(true))
                .no_cli(),
        )
}

pub fn remove_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "iso",
            from_fn_async(remove_iso)
                .with_metadata("get_signer", Value::Bool(true))
                .with_custom_display_fn(|args, removed| warn_if_absent(&args.params, removed))
                .with_about("about.remove-iso-registry")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "img",
            from_fn_async(remove_img)
                .with_metadata("get_signer", Value::Bool(true))
                .with_custom_display_fn(|args, removed| warn_if_absent(&args.params, removed))
                .with_about("about.remove-img-registry")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "squashfs",
            from_fn_async(remove_squashfs)
                .with_metadata("get_signer", Value::Bool(true))
                .with_custom_display_fn(|args, removed| warn_if_absent(&args.params, removed))
                .with_about("about.remove-squashfs-registry")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct AddAssetParams {
    pub version: Version,
    pub platform: InternedString,
    /// @deprecated Use `urls`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<Url>,
    #[serde(default)]
    pub urls: Vec<Url>,
    #[serde(default)]
    pub torrent: Option<Url>,
    #[serde(rename = "__Auth_signer")]
    #[visit(ts(skip))]
    pub signer: AnyVerifyingKey,
    pub signature: AnySignature,
    #[serde(default)]
    pub signatures: SignatureMap,
    pub commitment: Blake3Commitment,
}

rpc_toolkit::reflect_ts!(AddAssetParams);
rpc_toolkit::ts_export!(AddAssetParams, namespaces = [""]);

fn publication_mirrors(urls: Vec<Url>, legacy: Option<Url>) -> Result<Vec<Url>, Error> {
    let urls: Vec<_> = urls.into_iter().chain(legacy).unique().collect();
    crate::registry::asset::require_http_mirrors(&urls)?;
    Ok(urls)
}

async fn add_asset(
    ctx: RegistryContext,
    AddAssetParams {
        version,
        platform,
        url,
        urls,
        torrent,
        signer,
        signature,
        signatures,
        commitment,
    }: AddAssetParams,
    accessor: impl FnOnce(
        &mut Model<OsVersionInfo>,
    ) -> &mut Model<BTreeMap<InternedString, RegistryAsset<Blake3Commitment>>>
    + UnwindSafe
    + Send,
) -> Result<(), Error> {
    let urls = publication_mirrors(urls, url)?;
    if let Some(torrent) = &torrent {
        crate::registry::bittorrent::validate_torrent_url(torrent)?;
    }
    let signatures = crate::sign::compat::verify_signatures(
        &commitment,
        SIG_CONTEXT,
        &signatures,
        &signer,
        &signature,
    )?;
    ctx.db
        .mutate(|db| {
            let signer_guid = db.as_index().as_signers().get_signer(&signer)?;
            if may_manage_assets(db, &version, &signer_guid)? {
                accessor(
                    db.as_index_mut()
                        .as_os_mut()
                        .as_versions_mut()
                        .as_idx_mut(&version)
                        .or_not_found(&version)?,
                )
                .upsert(&platform, || {
                    Ok(RegistryAsset {
                        published_at: Utc::now(),
                        urls: Vec::new(),
                        torrent: None,
                        commitment: commitment.clone(),
                        signatures: SignatureMap::new(),
                    })
                })?
                .mutate(|s| {
                    if s.commitment != commitment {
                        Err(Error::new(
                            eyre!("{}", t!("registry.os.asset.commitment-mismatch")),
                            ErrorKind::InvalidSignature,
                        ))
                    } else {
                        s.merge_signatures(&commitment, signatures);
                        s.merge_torrent(torrent);
                        for url in urls {
                            if !s.urls.contains(&url) {
                                s.urls.push(url);
                            }
                        }
                        Ok(())
                    }
                })?;
                Ok(())
            } else {
                Err(Error::new(
                    eyre!("{}", t!("registry.os.asset.unauthorized")),
                    ErrorKind::Authorization,
                ))
            }
        })
        .await
        .result?;

    Ok(())
}

pub async fn add_iso(ctx: RegistryContext, params: AddAssetParams) -> Result<(), Error> {
    add_asset(ctx, params, |m| m.as_iso_mut()).await
}

pub async fn add_img(ctx: RegistryContext, params: AddAssetParams) -> Result<(), Error> {
    add_asset(ctx, params, |m| m.as_img_mut()).await
}

pub async fn add_squashfs(ctx: RegistryContext, params: AddAssetParams) -> Result<(), Error> {
    add_asset(ctx, params, |m| m.as_squashfs_mut()).await
}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct CliAddAssetParams {
    #[arg(short = 'p', long = "platform", help = "help.arg.platform")]
    pub platform: InternedString,
    #[arg(short = 'v', long = "version", help = "help.arg.os-version")]
    pub version: Version,
    #[arg(help = "help.arg.asset-file-path")]
    pub file: PathBuf,
    #[arg(help = "help.arg.asset-url")]
    pub url: Url,
    #[arg(long, help = "help.arg.torrent")]
    pub torrent: Option<Url>,
}

pub async fn cli_add_asset(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        params:
            CliAddAssetParams {
                platform,
                version,
                file: path,
                url,
                torrent,
            },
        ..
    }: HandlerArgs<CliContext, CliAddAssetParams>,
) -> Result<(), Error> {
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some("iso") => "iso",
        Some("img") => "img",
        Some("squashfs") => "squashfs",
        _ => {
            return Err(Error::new(
                eyre!("{}", t!("registry.os.asset.unknown-extension")),
                ErrorKind::InvalidRequest,
            ));
        }
    };

    let file = MultiCursorFile::from(open_file(&path).await?);

    let progress = FullProgressTracker::new();
    let mut sign_phase = progress.add_phase(InternedString::intern("Signing File"), Some(10));
    let mut verify_phase = progress.add_phase(InternedString::intern("Verifying URL"), Some(100));
    let mut index_phase = progress.add_phase(
        InternedString::intern("Adding File to Registry Index"),
        Some(1),
    );

    let progress_task =
        progress.progress_bar_task(&format!("Adding {} to registry...", path.display()));

    sign_phase.start();
    let blake3 = file.blake3_mmap().await?;
    let size = file.size().await.ok_or_else(|| {
        Error::new(
            eyre!("{}", t!("registry.os.asset.failed-read-metadata")),
            ErrorKind::Filesystem,
        )
    })?;
    let commitment = Blake3Commitment {
        hash: Base64(*blake3.as_bytes()),
        size,
    };
    let signature = ctx
        .id_key()?
        .sign_commitment(&commitment, SIG_CONTEXT)
        .await?;
    sign_phase.complete();

    verify_phase.start();
    let src = HttpSource::new(ctx.client.clone(), url.clone()).await?;
    if let Some(size) = src.size().await {
        verify_phase.set_total(size);
    }
    verify_phase.set_units(Some(ProgressUnits::Bytes));
    let mut writer = verify_phase.writer(commitment.verifying_writer(tokio::io::sink()));
    src.copy_all_to(&mut writer).await?;
    let (verifier, mut verify_phase) = writer.into_inner();
    verifier.verify().await?;
    verify_phase.complete();

    index_phase.start();
    ctx.call_remote::<RegistryContext>(
        &parent_method
            .into_iter()
            .chain(method)
            .chain([ext])
            .join("."),
        imbl_value::json!({
            "platform": platform,
            "version": version,
            "urls": [&url],
            "torrent": torrent,
            "signature": signature,
            "commitment": commitment,
        }),
    )
    .await?;
    index_phase.complete();

    progress.complete();

    progress_task.await.with_kind(ErrorKind::Unknown)?;

    Ok(())
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct RemoveAssetParams {
    #[arg(help = "help.arg.os-version")]
    pub version: Version,
    #[arg(help = "help.arg.platform")]
    pub platform: InternedString,
    #[serde(rename = "__Auth_signer")]
    #[visit(ts(skip))]
    #[arg(skip)]
    pub signer: Option<AnyVerifyingKey>,
}

rpc_toolkit::reflect_ts!(RemoveAssetParams);
rpc_toolkit::ts_export!(RemoveAssetParams, namespaces = [""]);

fn warn_if_absent(params: &RemoveAssetParams, removed: bool) -> Result<(), Error> {
    if !removed {
        tracing::warn!(
            "{}",
            t!(
                "registry.os.asset.remove-not-exist",
                version = params.version,
                platform = params.platform
            )
        );
    }
    Ok(())
}

async fn remove_asset(
    ctx: RegistryContext,
    RemoveAssetParams {
        version,
        platform,
        signer,
    }: RemoveAssetParams,
    accessor: impl FnOnce(
        &mut Model<OsVersionInfo>,
    ) -> &mut Model<BTreeMap<InternedString, RegistryAsset<Blake3Commitment>>>
    + UnwindSafe
    + Send,
) -> Result<bool, Error> {
    let signer = signer.ok_or_else(|| {
        Error::new(
            eyre!("{}", t!("registry.os.asset.missing-signer")),
            ErrorKind::InvalidRequest,
        )
    })?;
    ctx.db
        .mutate(|db| {
            let signer_guid = db.as_index().as_signers().get_signer(&signer)?;
            if may_manage_assets(db, &version, &signer_guid)? {
                Ok(accessor(
                    db.as_index_mut()
                        .as_os_mut()
                        .as_versions_mut()
                        .as_idx_mut(&version)
                        .or_not_found(&version)?,
                )
                .remove(&platform)?
                .is_some())
            } else {
                Err(Error::new(
                    eyre!("{}", t!("registry.os.asset.unauthorized")),
                    ErrorKind::Authorization,
                ))
            }
        })
        .await
        .result
}

pub async fn remove_iso(ctx: RegistryContext, params: RemoveAssetParams) -> Result<bool, Error> {
    remove_asset(ctx, params, |m| m.as_iso_mut()).await
}

pub async fn remove_img(ctx: RegistryContext, params: RemoveAssetParams) -> Result<bool, Error> {
    remove_asset(ctx, params, |m| m.as_img_mut()).await
}

pub async fn remove_squashfs(
    ctx: RegistryContext,
    params: RemoveAssetParams,
) -> Result<bool, Error> {
    remove_asset(ctx, params, |m| m.as_squashfs_mut()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(mirrors: serde_json::Value) -> AddAssetParams {
        let key = ed25519_dalek::SigningKey::from_bytes(&[42; 32]);
        let mut value = serde_json::to_value(AddAssetParams {
            version: "0.4.2".parse().unwrap(),
            platform: "x86_64".into(),
            url: None,
            urls: Vec::new(),
            torrent: None,
            signer: AnyVerifyingKey::Ed25519(key.verifying_key()),
            signature: AnySignature::Ed25519ph(ed25519_dalek::Signature::from_bytes(&[0; 64])),
            signatures: Default::default(),
            commitment: Blake3Commitment {
                hash: Base64([7; 32]),
                size: 1,
            },
        })
        .unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("urls");
        object.remove("torrent");
        object.extend(mirrors.as_object().unwrap().clone());
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn url_array_publication_needs_no_legacy_url() {
        let params = request(
            serde_json::json!({"urls": ["https://one.example/asset", "https://two.example/asset"]}),
        );
        assert!(params.url.is_none());
        assert!(serde_json::to_value(&params).unwrap().get("url").is_none());
        assert_eq!(
            publication_mirrors(params.urls, params.url).unwrap(),
            vec![
                "https://one.example/asset".parse::<Url>().unwrap(),
                "https://two.example/asset".parse().unwrap(),
            ]
        );
    }

    #[test]
    fn legacy_url_publication_remains_accepted() {
        let params = request(serde_json::json!({"url": "https://legacy.example/asset"}));
        assert_eq!(
            publication_mirrors(params.urls, params.url).unwrap(),
            vec!["https://legacy.example/asset".parse::<Url>().unwrap()]
        );
    }

    #[test]
    fn canonical_mirrors_precede_distinct_legacy_mirrors() {
        let params = request(serde_json::json!({
            "urls": ["https://one.example/asset", "https://two.example/asset"],
            "url": "https://legacy.example/asset"
        }));
        assert_eq!(
            publication_mirrors(params.urls, params.url).unwrap(),
            vec![
                "https://one.example/asset".parse::<Url>().unwrap(),
                "https://two.example/asset".parse().unwrap(),
                "https://legacy.example/asset".parse().unwrap(),
            ]
        );
    }

    #[test]
    fn duplicate_and_missing_mirrors_follow_publication_rules() {
        let params = request(serde_json::json!({
            "urls": ["https://one.example/asset", "https://one.example/asset"],
            "url": "https://one.example/asset"
        }));
        assert_eq!(
            publication_mirrors(params.urls, params.url).unwrap(),
            vec!["https://one.example/asset".parse::<Url>().unwrap()]
        );
        let params = request(serde_json::json!({}));
        assert!(matches!(
            publication_mirrors(params.urls, params.url)
                .unwrap_err()
                .kind,
            ErrorKind::InvalidRequest
        ));
    }
}
