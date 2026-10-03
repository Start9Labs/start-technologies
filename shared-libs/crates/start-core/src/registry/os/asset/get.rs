use std::collections::BTreeMap;
use std::panic::UnwindSafe;
use std::path::{Path, PathBuf};

use clap::Parser;
use exver::Version;
use imbl_value::{InternedString, json};
use itertools::Itertools;
use rpc_toolkit::{Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use visit_rs::ts::TS;

use crate::context::CliContext;
use crate::prelude::*;
use crate::progress::{FullProgressTracker, ProgressUnits};
use crate::registry::asset::RegistryAsset;
use crate::registry::context::RegistryContext;
use crate::registry::os::SIG_CONTEXT;
use crate::registry::os::index::OsVersionInfo;
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::sign::commitment::Commitment;
use crate::sign::commitment::blake3::Blake3Commitment;
use crate::util::io::{AtomicFile, open_file};

pub fn get_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "iso",
            from_fn_async(get_iso)
                .with_metadata("authenticated", Value::Bool(false))
                .no_cli(),
        )
        .subcommand(
            "iso",
            from_fn_async(cli_get_os_asset)
                .no_ts()
                .no_display()
                .with_about("about.download-iso"),
        )
        .subcommand(
            "img",
            from_fn_async(get_img)
                .with_metadata("authenticated", Value::Bool(false))
                .no_cli(),
        )
        .subcommand(
            "img",
            from_fn_async(cli_get_os_asset)
                .no_ts()
                .no_display()
                .with_about("about.download-img"),
        )
        .subcommand(
            "squashfs",
            from_fn_async(get_squashfs)
                .with_metadata("authenticated", Value::Bool(false))
                .no_cli(),
        )
        .subcommand(
            "squashfs",
            from_fn_async(cli_get_os_asset)
                .no_ts()
                .no_display()
                .with_about("about.download-squashfs"),
        )
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GetOsAssetParams {
    pub version: Version,
    pub platform: InternedString,
}

async fn get_os_asset(
    ctx: RegistryContext,
    GetOsAssetParams { version, platform }: GetOsAssetParams,
    accessor: impl FnOnce(
        &Model<OsVersionInfo>,
    ) -> &Model<BTreeMap<InternedString, RegistryAsset<Blake3Commitment>>>
    + UnwindSafe
    + Send,
) -> Result<RegistryAsset<Blake3Commitment>, Error> {
    accessor(
        ctx.db
            .peek()
            .await
            .as_index()
            .as_os()
            .as_versions()
            .as_idx(&version)
            .or_not_found(&version)?,
    )
    .as_idx(&platform)
    .or_not_found(&platform)?
    .de()
}

pub async fn get_iso(
    ctx: RegistryContext,
    params: GetOsAssetParams,
) -> Result<RegistryAsset<Blake3Commitment>, Error> {
    get_os_asset(ctx, params, |info| info.as_iso()).await
}

pub async fn get_img(
    ctx: RegistryContext,
    params: GetOsAssetParams,
) -> Result<RegistryAsset<Blake3Commitment>, Error> {
    get_os_asset(ctx, params, |info| info.as_img()).await
}

pub async fn get_squashfs(
    ctx: RegistryContext,
    params: GetOsAssetParams,
) -> Result<RegistryAsset<Blake3Commitment>, Error> {
    get_os_asset(ctx, params, |info| info.as_squashfs()).await
}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct CliGetOsAssetParams {
    #[arg(help = "help.arg.os-version")]
    pub version: Version,
    #[arg(help = "help.arg.platform")]
    pub platform: InternedString,
    #[arg(
        long = "download",
        short = 'd',
        help = "help.arg.download-directory",
        default_value = "."
    )]
    pub download: PathBuf,
    #[arg(long = "reverify", short = 'r', help = "help.arg.reverify-hash")]
    pub reverify: bool,
}

async fn cli_get_os_asset(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        params:
            CliGetOsAssetParams {
                version,
                platform,
                download,
                reverify,
            },
        ..
    }: HandlerArgs<CliContext, CliGetOsAssetParams>,
) -> Result<RegistryAsset<Blake3Commitment>, Error> {
    let ext = method
        .iter()
        .last()
        .or_else(|| parent_method.iter().last())
        .unwrap_or(&"bin");

    let res = from_value::<RegistryAsset<Blake3Commitment>>(
        ctx.call_remote::<RegistryContext>(
            &parent_method.iter().chain(&method).join("."),
            json!({
                "version": version,
                "platform": platform,
            }),
        )
        .await?,
    )?;

    res.validate(SIG_CONTEXT, res.all_signers())?;

    let download = download.join(format!("startos-{version}_{platform}.{ext}"));
    let mut file = AtomicFile::new(&download, None::<&Path>).await?;

    let progress = FullProgressTracker::new();
    let mut download_phase =
        progress.add_phase(InternedString::intern("Downloading File"), Some(100));
    download_phase.set_total(res.commitment.size);
    download_phase.set_units(Some(ProgressUnits::Bytes));
    let reverify_phase = if reverify {
        Some(progress.add_phase(InternedString::intern("Reverifying File"), Some(10)))
    } else {
        None
    };

    let progress_task = progress.progress_bar_task("Downloading...");

    download_phase.start();
    let mut download_writer = download_phase.writer(&mut *file);
    res.download(ctx.client.clone(), &mut download_writer)
        .await?;
    let (_, mut download_phase) = download_writer.into_inner();
    file.save().await?;
    download_phase.complete();

    if let Some(mut reverify_phase) = reverify_phase {
        reverify_phase.start();
        res.commitment
            .check(&MultiCursorFile::from(open_file(download).await?))
            .await?;
        reverify_phase.complete();
    }

    progress.complete();

    progress_task.await.with_kind(ErrorKind::Unknown)?;

    Ok(res)
}
