use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use imbl_value::InternedString;
use itertools::Itertools;
use rpc_toolkit::HandlerArgs;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use url::Url;

use crate::PackageId;
use crate::context::CliContext;
use crate::prelude::*;
use crate::progress::FullProgressTracker;
use crate::registry::asset::BufferedHttpSource;
use crate::registry::context::RegistryContext;
use crate::registry::package::index::PackageVersionInfo;
use crate::s9pk::S9pk;
use crate::s9pk::merkle_archive::source::http::HttpSource;
use crate::s9pk::v2::SIG_CONTEXT;
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::sign::ed25519::Ed25519;
use crate::sign::{AnySignature, AnyVerifyingKey, SignatureScheme};
use crate::util::VersionString;
use crate::util::io::TrackingIO;
use crate::util::serde::Base64;

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AddPackageParams {
    #[ts(type = "string[]")]
    pub urls: Vec<Url>,
    #[ts(skip)]
    #[serde(rename = "__Auth_signer")]
    pub uploader: AnyVerifyingKey,
    pub commitment: MerkleArchiveCommitment,
    pub signature: AnySignature,
}

pub async fn add_package(
    ctx: RegistryContext,
    AddPackageParams {
        urls,
        uploader,
        commitment,
        signature,
    }: AddPackageParams,
) -> Result<(), Error> {
    uploader
        .scheme()
        .verify_commitment(&uploader, &commitment, SIG_CONTEXT, &signature)?;
    let peek = ctx.db.peek().await;
    let uploader_guid = peek.as_index().as_signers().get_signer(&uploader)?;

    let Some(([url], rest)) = urls.split_at_checked(1) else {
        return Err(Error::new(
            eyre!("{}", t!("registry.package.add.must-specify-url")),
            ErrorKind::InvalidRequest,
        ));
    };

    let s9pk = S9pk::deserialize(
        &Arc::new(HttpSource::new(ctx.client.clone(), url.clone()).await?),
        Some(&commitment),
    )
    .await?;

    for url in rest {
        S9pk::deserialize(
            &Arc::new(HttpSource::new(ctx.client.clone(), url.clone()).await?),
            Some(&commitment),
        )
        .await?;
    }

    let manifest = s9pk.as_manifest();

    let mut info = PackageVersionInfo::from_s9pk(&s9pk, urls).await?;
    for (_, s9pk) in &mut info.s9pks {
        if !s9pk.signatures.contains_key(&uploader) && s9pk.commitment == commitment {
            s9pk.signatures.insert(uploader.clone(), signature.clone());
        }
    }

    ctx.db
        .mutate(|db| {
            if db.as_admins().de()?.contains(&uploader_guid)
                || db
                    .as_index()
                    .as_package()
                    .as_packages()
                    .as_idx(&manifest.id)
                    .or_not_found(&manifest.id)?
                    .as_authorized()
                    .de()?
                    .get(&uploader_guid)
                    .map_or(false, |v| manifest.version.satisfies(v))
            {
                let package = db
                    .as_index_mut()
                    .as_package_mut()
                    .as_packages_mut()
                    .upsert(&manifest.id, || Ok(Default::default()))?;
                let v = package.as_versions_mut();
                if let Some(prev) = v.as_idx_mut(&manifest.version) {
                    prev.mutate(|p| p.merge_with(info, true))?;
                } else {
                    v.insert(&manifest.version, &info)?;
                }

                Ok(())
            } else {
                Err(Error::new(
                    eyre!("{}", t!("registry.package.add.unauthorized")),
                    ErrorKind::Authorization,
                ))
            }
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct CliAddPackageParams {
    #[arg(help = "help.arg.s9pk-file-path")]
    pub file: PathBuf,
    #[arg(long, help = "help.arg.package-url")]
    pub url: Vec<Url>,
    #[arg(long, help = "help.arg.no-verify")]
    pub no_verify: bool,
}

pub async fn cli_add_package(
    ctx: CliContext,
    CliAddPackageParams {
        file,
        url,
        no_verify,
    }: CliAddPackageParams,
) -> Result<(), Error> {
    let s9pk = S9pk::open(&file, None).await?;
    cli_add_package_impl(ctx, s9pk, url, no_verify).await
}

pub async fn cli_add_package_impl(
    ctx: CliContext,
    s9pk: S9pk,
    url: Vec<Url>,
    no_verify: bool,
) -> Result<(), Error> {
    let manifest = s9pk.as_manifest();

    let progress = FullProgressTracker::new();
    let mut sign_phase = progress.add_phase(InternedString::intern("Signing File"), Some(1));
    let verify = if !no_verify {
        url.iter()
            .map(|url| {
                let phase = progress.add_phase(
                    InternedString::from_display(&lazy_format!("Verifying {url}")),
                    Some(100),
                );
                (url.clone(), phase)
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut index_phase = progress.add_phase(
        InternedString::intern("Adding File to Registry Index"),
        Some(1),
    );

    let progress_task = progress.progress_bar_task(&format!(
        "Adding {}@{}{} to registry...",
        manifest.id,
        manifest.version,
        manifest
            .hardware_requirements
            .arch
            .as_ref()
            .map_or(String::new(), |a| format!(" ({})", a.iter().join("/")))
    ));

    sign_phase.start();
    let commitment = s9pk.as_archive().commitment().await?;
    let signature = Ed25519.sign_commitment(ctx.id_key()?, &commitment, SIG_CONTEXT)?;
    sign_phase.complete();

    for (url, mut phase) in verify {
        phase.start();
        let source = BufferedHttpSource::new(ctx.client.clone(), url, phase).await?;
        let mut src = S9pk::deserialize(&Arc::new(source), Some(&commitment)).await?;
        src.serialize(&mut TrackingIO::new(0, &mut tokio::io::sink()), true)
            .await?;
    }

    index_phase.start();
    ctx.call_remote::<RegistryContext>(
        "package.add",
        imbl_value::json!({
            "urls": &url,
            "signature": AnySignature::Ed25519(signature),
            "commitment": commitment,
        }),
    )
    .await?;
    index_phase.complete();

    progress.complete();

    progress_task.await.with_kind(ErrorKind::Unknown)?;

    Ok(())
}

#[derive(Debug, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RemovePackageParams {
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
    #[arg(help = "help.arg.package-version")]
    pub version: Option<VersionString>,
    #[arg(long, help = "help.arg.signature-hash")]
    pub sighash: Option<Base64<[u8; 32]>>,
    #[arg(long, help = "help.arg.force-remove-package")]
    #[serde(default)]
    pub force: bool,
    #[ts(skip)]
    #[arg(skip)]
    #[serde(rename = "__Auth_signer")]
    pub signer: Option<AnyVerifyingKey>,
}

pub async fn remove_package(
    ctx: RegistryContext,
    RemovePackageParams {
        id,
        version,
        sighash,
        force,
        signer,
    }: RemovePackageParams,
) -> Result<bool, Error> {
    let peek = ctx.db.peek().await;
    let signer = signer.ok_or_else(|| {
        Error::new(
            eyre!("{}", t!("registry.package.missing-signer")),
            ErrorKind::InvalidRequest,
        )
    })?;
    let signer_guid = peek.as_index().as_signers().get_signer(&signer)?;

    let rev = ctx
        .db
        .mutate(|db| {
            let is_admin = db.as_admins().de()?.contains(&signer_guid);
            let is_authorized = is_admin
                || db
                    .as_index()
                    .as_package()
                    .as_packages()
                    .as_idx(&id)
                    .or_not_found(&id)?
                    .as_authorized()
                    .de()?
                    .get(&signer_guid)
                    .map_or(false, |v| {
                        version
                            .as_ref()
                            .map_or(true, |version| version.satisfies(v))
                    });
            if is_authorized {
                if let Some(version) = &version {
                    if let Some(package) = db
                        .as_index_mut()
                        .as_package_mut()
                        .as_packages_mut()
                        .as_idx_mut(&id)
                    {
                        if let Some(sighash) = sighash {
                            if if let Some(package) = package.as_versions_mut().as_idx_mut(version)
                            {
                                package.as_s9pks_mut().mutate(|s| {
                                    s.retain(|(_, asset)| asset.commitment.root_sighash != sighash);
                                    Ok(s.is_empty())
                                })?
                            } else {
                                false
                            } {
                                package.as_versions_mut().remove(version)?;
                            }
                        } else {
                            package.as_versions_mut().remove(version)?;
                        }
                    }
                } else {
                    let has_versions = db
                        .as_index()
                        .as_package()
                        .as_packages()
                        .as_idx(&id)
                        .map(|p| {
                            p.as_versions()
                                .as_entries()
                                .map(|e| !e.is_empty())
                                .unwrap_or(false)
                        })
                        .unwrap_or(false);
                    if has_versions && !force {
                        return Err(Error::new(
                            eyre!("{}", t!("registry.package.remove-has-versions", id = id)),
                            ErrorKind::InvalidRequest,
                        ));
                    }
                    db.as_index_mut()
                        .as_package_mut()
                        .as_packages_mut()
                        .remove(&id)?;
                }
                Ok(())
            } else {
                Err(Error::new(
                    eyre!("{}", t!("registry.package.unauthorized")),
                    ErrorKind::Authorization,
                ))
            }
        })
        .await;
    rev.result.map(|_| rev.revision.is_some())
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AddMirrorParams {
    #[ts(type = "string")]
    pub url: Url,
    #[ts(skip)]
    #[serde(rename = "__Auth_signer")]
    pub uploader: AnyVerifyingKey,
    pub commitment: MerkleArchiveCommitment,
    pub signature: AnySignature,
}

pub async fn add_mirror(
    ctx: RegistryContext,
    AddMirrorParams {
        url,
        uploader,
        commitment,
        signature,
    }: AddMirrorParams,
) -> Result<(), Error> {
    uploader
        .scheme()
        .verify_commitment(&uploader, &commitment, SIG_CONTEXT, &signature)?;
    let peek = ctx.db.peek().await;
    let uploader_guid = peek.as_index().as_signers().get_signer(&uploader)?;

    let s9pk = S9pk::deserialize(
        &Arc::new(HttpSource::new(ctx.client.clone(), url.clone()).await?),
        Some(&commitment),
    )
    .await?;

    let manifest = s9pk.as_manifest();

    let mut info = PackageVersionInfo::from_s9pk(&s9pk, vec![url]).await?;
    for (_, s9pk) in &mut info.s9pks {
        if !s9pk.signatures.contains_key(&uploader) && s9pk.commitment == commitment {
            s9pk.signatures.insert(uploader.clone(), signature.clone());
        }
    }

    ctx.db
        .mutate(|db| {
            if db.as_admins().de()?.contains(&uploader_guid)
                || db
                    .as_index()
                    .as_package()
                    .as_packages()
                    .as_idx(&manifest.id)
                    .or_not_found(&manifest.id)?
                    .as_authorized()
                    .de()?
                    .get(&uploader_guid)
                    .map_or(false, |v| manifest.version.satisfies(v))
            {
                let package = db
                    .as_index_mut()
                    .as_package_mut()
                    .as_packages_mut()
                    .as_idx_mut(&manifest.id)
                    .and_then(|p| p.as_versions_mut().as_idx_mut(&manifest.version))
                    .or_not_found(&lazy_format!("{}@{}", &manifest.id, &manifest.version))?;
                package.mutate(|p| p.merge_with(info, false))?;

                Ok(())
            } else {
                Err(Error::new(
                    eyre!("{}", t!("registry.package.add-mirror.unauthorized")),
                    ErrorKind::Authorization,
                ))
            }
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct CliAddMirrorParams {
    #[arg(help = "help.arg.s9pk-file-path")]
    pub file: PathBuf,
    #[arg(help = "help.arg.mirror-url")]
    pub url: Url,
    #[arg(long, help = "help.arg.no-verify")]
    pub no_verify: bool,
}

pub async fn cli_add_mirror(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        params:
            CliAddMirrorParams {
                file,
                url,
                no_verify,
            },
        ..
    }: HandlerArgs<CliContext, CliAddMirrorParams>,
) -> Result<(), Error> {
    let s9pk = S9pk::open(&file, None).await?;

    let progress = FullProgressTracker::new();
    let mut sign_phase = progress.add_phase(InternedString::intern("Signing File"), Some(1));
    let verify = if !no_verify {
        let url = &url;
        vec![(
            url.clone(),
            progress.add_phase(
                InternedString::from_display(&lazy_format!("Verifying {url}")),
                Some(100),
            ),
        )]
    } else {
        Vec::new()
    };
    let mut index_phase = progress.add_phase(
        InternedString::intern("Adding File to Registry Index"),
        Some(1),
    );

    let progress_task =
        progress.progress_bar_task(&format!("Adding {} to registry...", file.display()));

    sign_phase.start();
    let commitment = s9pk.as_archive().commitment().await?;
    let signature = Ed25519.sign_commitment(ctx.id_key()?, &commitment, SIG_CONTEXT)?;
    sign_phase.complete();

    for (url, mut phase) in verify {
        phase.start();
        let source = BufferedHttpSource::new(ctx.client.clone(), url, phase).await?;
        let mut src = S9pk::deserialize(&Arc::new(source), Some(&commitment)).await?;
        src.serialize(&mut TrackingIO::new(0, &mut tokio::io::sink()), true)
            .await?;
    }

    index_phase.start();
    ctx.call_remote::<RegistryContext>(
        &parent_method.into_iter().chain(method).join("."),
        imbl_value::json!({
            "url": &url,
            "signature": AnySignature::Ed25519(signature),
            "commitment": commitment,
        }),
    )
    .await?;
    index_phase.complete();

    progress.complete();

    progress_task.await.with_kind(ErrorKind::Unknown)?;

    Ok(())
}

#[derive(Debug, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RemoveMirrorParams {
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
    #[arg(help = "help.arg.package-version")]
    pub version: VersionString,
    #[arg(long, help = "help.arg.mirror-url")]
    #[ts(type = "string")]
    pub url: Url,
    #[ts(skip)]
    #[arg(skip)]
    #[serde(rename = "__Auth_signer")]
    pub signer: Option<AnyVerifyingKey>,
}

pub async fn remove_mirror(
    ctx: RegistryContext,
    RemoveMirrorParams {
        id,
        version,
        url,
        signer,
    }: RemoveMirrorParams,
) -> Result<(), Error> {
    let peek = ctx.db.peek().await;
    let signer = signer.ok_or_else(|| {
        Error::new(
            eyre!("{}", t!("registry.package.missing-signer")),
            ErrorKind::InvalidRequest,
        )
    })?;
    let signer_guid = peek.as_index().as_signers().get_signer(&signer)?;

    ctx.db
        .mutate(|db| {
            if db.as_admins().de()?.contains(&signer_guid)
                || db
                    .as_index()
                    .as_package()
                    .as_packages()
                    .as_idx(&id)
                    .or_not_found(&id)?
                    .as_authorized()
                    .de()?
                    .get(&signer_guid)
                    .map_or(false, |v| version.satisfies(v))
            {
                if let Some(package) = db
                    .as_index_mut()
                    .as_package_mut()
                    .as_packages_mut()
                    .as_idx_mut(&id)
                    .and_then(|p| p.as_versions_mut().as_idx_mut(&version))
                {
                    package.as_s9pks_mut().mutate(|s| {
                        s.iter_mut()
                            .for_each(|(_, asset)| asset.urls.retain(|u| u != &url));
                        if s.iter().any(|(_, asset)| asset.urls.is_empty()) {
                            Err(Error::new(
                                eyre!("{}", t!("registry.package.cannot-remove-last-mirror")),
                                ErrorKind::InvalidRequest,
                            ))
                        } else {
                            Ok(())
                        }
                    })?;
                }
                Ok(())
            } else {
                Err(Error::new(
                    eyre!("{}", t!("registry.package.remove-mirror.unauthorized")),
                    ErrorKind::Authorization,
                ))
            }
        })
        .await
        .result
}
