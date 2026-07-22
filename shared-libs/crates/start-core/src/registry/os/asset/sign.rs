use std::collections::BTreeMap;
use std::panic::UnwindSafe;
use std::path::PathBuf;

use clap::Parser;
use exver::Version;
use imbl_value::InternedString;
use itertools::Itertools;
use rpc_toolkit::{Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::context::CliContext;
use crate::prelude::*;
use crate::progress::FullProgressTracker;
use crate::registry::asset::RegistryAsset;
use crate::registry::context::RegistryContext;
use crate::registry::os::SIG_CONTEXT;
use crate::registry::os::index::OsVersionInfo;
use crate::s9pk::merkle_archive::source::ArchiveSource;
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::sign::commitment::blake3::Blake3Commitment;
use crate::sign::ed25519::Ed25519;
use crate::sign::{AnySignature, AnyVerifyingKey, SignatureScheme};
use crate::util::io::open_file;
use crate::util::serde::Base64;

pub fn sign_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "iso",
            from_fn_async(sign_iso)
                .with_metadata("get_signer", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "img",
            from_fn_async(sign_img)
                .with_metadata("get_signer", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "squashfs",
            from_fn_async(sign_squashfs)
                .with_metadata("get_signer", Value::Bool(true))
                .no_cli(),
        )
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SignAssetParams {
    #[ts(type = "string")]
    version: Version,
    #[ts(type = "string")]
    platform: InternedString,
    #[ts(skip)]
    #[serde(rename = "__Auth_signer")]
    signer: AnyVerifyingKey,
    signature: AnySignature,
}

async fn sign_asset(
    ctx: RegistryContext,
    SignAssetParams {
        version,
        platform,
        signer,
        signature,
    }: SignAssetParams,
    accessor: impl FnOnce(
        &mut Model<OsVersionInfo>,
    ) -> &mut Model<BTreeMap<InternedString, RegistryAsset<Blake3Commitment>>>
    + UnwindSafe
    + Send,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            let guid = db.as_index().as_signers().get_signer(&signer)?;
            if !db
                .as_index()
                .as_os()
                .as_versions()
                .as_idx(&version)
                .or_not_found(&version)?
                .as_authorized()
                .de()?
                .contains(&guid)
            {
                return Err(Error::new(
                    eyre!(
                        "{}",
                        t!("registry.os.asset.signer-not-authorized", guid = guid)
                    ),
                    ErrorKind::Authorization,
                ));
            }

            accessor(
                db.as_index_mut()
                    .as_os_mut()
                    .as_versions_mut()
                    .as_idx_mut(&version)
                    .or_not_found(&version)?,
            )
            .as_idx_mut(&platform)
            .or_not_found(&platform)?
            .mutate(|s| {
                signer.scheme().verify_commitment(
                    &signer,
                    &s.commitment,
                    SIG_CONTEXT,
                    &signature,
                )?;
                s.signatures.insert(signer, signature);
                Ok(())
            })?;

            Ok(())
        })
        .await
        .result
}

pub async fn sign_iso(ctx: RegistryContext, params: SignAssetParams) -> Result<(), Error> {
    sign_asset(ctx, params, |m| m.as_iso_mut()).await
}

pub async fn sign_img(ctx: RegistryContext, params: SignAssetParams) -> Result<(), Error> {
    sign_asset(ctx, params, |m| m.as_img_mut()).await
}

pub async fn sign_squashfs(ctx: RegistryContext, params: SignAssetParams) -> Result<(), Error> {
    sign_asset(ctx, params, |m| m.as_squashfs_mut()).await
}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct CliSignAssetParams {
    #[arg(short = 'p', long = "platform", help = "help.arg.platform")]
    pub platform: InternedString,
    #[arg(short = 'v', long = "version", help = "help.arg.os-version")]
    pub version: Version,
    #[arg(help = "help.arg.asset-file-path")]
    pub file: PathBuf,
}

pub async fn cli_sign_asset(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        params:
            CliSignAssetParams {
                platform,
                version,
                file: path,
            },
        ..
    }: HandlerArgs<CliContext, CliSignAssetParams>,
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
    let mut index_phase = progress.add_phase(
        InternedString::intern("Adding Signature to Registry Index"),
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
    let signature =
        AnySignature::Ed25519(Ed25519.sign_commitment(ctx.id_key()?, &commitment, SIG_CONTEXT)?);
    sign_phase.complete();

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
            "signature": signature,
        }),
    )
    .await?;
    index_phase.complete();

    progress.complete();

    progress_task.await.with_kind(ErrorKind::Unknown)?;

    Ok(())
}
