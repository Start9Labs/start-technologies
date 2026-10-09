use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use rpc_toolkit::{Empty, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use url::Url;

use crate::ImageId;
use crate::context::{CliContext, RpcContext};
use crate::prelude::*;
use crate::registry::device_info::DeviceInfo;
use crate::s9pk::manifest::{HardwareRequirements, Manifest};
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::s9pk::v2::SIG_CONTEXT;
use crate::s9pk::v2::pack::ImageConfig;
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::util::io::{TmpDir, create_file, open_file};
use crate::util::serde::{HandlerExtSerde, apply_expr};
use crate::util::{Apply, Invoke};

pub const SKIP_ENV: &[&str] = &["TERM", "container", "HOME", "HOSTNAME"];

pub fn s9pk() -> ParentHandler<CliContext> {
    ParentHandler::new()
        .subcommand(
            "pack",
            from_fn_async(super::v2::pack::pack)
                .no_ts()
                .no_display()
                .with_about("about.package-s9pk-input-files-into-valid-s9pk"),
        )
        .subcommand(
            "list-ingredients",
            from_fn_async(super::v2::pack::list_ingredients)
                .no_ts()
                .with_custom_display_fn(|_, ingredients| {
                    ingredients
                        .into_iter()
                        .map(Some)
                        .apply(|i| itertools::intersperse(i, None))
                        .for_each(|i| {
                            if let Some(p) = i {
                                print!("{}", p.display())
                            } else {
                                print!(" ")
                            }
                        });
                    println!();
                    Ok(())
                })
                .with_about("about.list-paths-of-package-ingredients"),
        )
        .subcommand(
            "init-workspace",
            from_fn_async(super::init::init_workspace)
                .no_ts()
                .no_display()
                .with_about("about.initialize-a-packaging-workspace"),
        )
        .subcommand(
            "init-package",
            from_fn_async(super::init::init_package)
                .no_ts()
                .no_display()
                .with_about("about.scaffold-a-new-package-from-template"),
        )
        .subcommand(
            "edit",
            edit().with_about("about.commands-add-image-or-edit-manifest"),
        )
        .subcommand(
            "inspect",
            inspect().with_about("about.commands-display-file-paths-contents-manifest"),
        )
        .subcommand(
            "convert",
            from_fn_async(convert)
                .no_ts()
                .no_display()
                .with_about("about.convert-s9pk-v1-to-v2"),
        )
        .subcommand(
            "publish",
            from_fn_async(publish)
                .no_ts()
                .no_display()
                .with_about("about.publish-s9pk"),
        )
        .subcommand(
            "select",
            from_fn_async(select)
                .no_ts()
                .with_custom_display_fn(|_, path: PathBuf| {
                    println!("{}", path.display());
                    Ok(())
                })
                .with_about("about.select-s9pk-for-device"),
        )
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
struct S9pkPath {
    #[arg(help = "help.arg.s9pk-file-path")]
    s9pk: PathBuf,
}

rpc_toolkit::reflect_ts!(S9pkPath);

fn edit() -> ParentHandler<CliContext, S9pkPath> {
    let only_parent = |a, _| a;
    ParentHandler::new()
        .subcommand(
            "add-image",
            from_fn_async(add_image)
                .with_inherited(only_parent)
                .no_display()
                .with_about("about.add-image-to-s9pk"),
        )
        .subcommand(
            "manifest",
            from_fn_async(edit_manifest)
                .with_inherited(only_parent)
                .with_display_serializable()
                .with_about("about.edit-s9pk-manifest"),
        )
}

fn inspect() -> ParentHandler<CliContext, S9pkPath> {
    let only_parent = |a, _| a;
    ParentHandler::new()
        .subcommand(
            "file-tree",
            from_fn_async(file_tree)
                .with_inherited(only_parent)
                .with_display_serializable()
                .with_about("about.display-list-of-paths"),
        )
        .subcommand(
            "cat",
            from_fn_async(cat)
                .with_inherited(only_parent)
                .no_display()
                .with_about("about.display-file-contents"),
        )
        .subcommand(
            "manifest",
            from_fn_async(inspect_manifest)
                .with_inherited(only_parent)
                .with_display_serializable()
                .with_about("about.display-s9pk-manifest"),
        )
        .subcommand(
            "commitment",
            from_fn_async(inspect_commitment)
                .with_inherited(only_parent)
                .with_display_serializable()
                .with_about("about.display-s9pk-root-sighash-and-maxsize"),
        )
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
struct AddImageParams {
    #[arg(help = "help.arg.image-id")]
    id: ImageId,
    #[command(flatten)]
    config: ImageConfig,
}

rpc_toolkit::reflect_ts!(AddImageParams);
async fn add_image(
    ctx: CliContext,
    AddImageParams { id, config }: AddImageParams,
    S9pkPath { s9pk: s9pk_path }: S9pkPath,
) -> Result<(), Error> {
    let mut s9pk = super::load(
        MultiCursorFile::from(open_file(&s9pk_path).await?),
        || ctx.build_key(),
        None,
    )
    .await?;
    s9pk.as_manifest_mut().images.insert(id, config);
    let tmp_dir = Arc::new(TmpDir::new().await?);
    s9pk.load_images(tmp_dir.clone()).await?;
    s9pk.validate_and_filter(None)?;
    s9pk.as_archive_mut()
        .set_signer(ctx.build_key()?, SIG_CONTEXT);
    let tmp_path = s9pk_path.with_extension("s9pk.tmp");
    let mut tmp_file = create_file(&tmp_path).await?;
    s9pk.serialize(&mut tmp_file, true).await?;
    drop(s9pk);
    tmp_file.sync_all().await?;
    tokio::fs::rename(&tmp_path, &s9pk_path).await?;

    tmp_dir.gc().await?;

    Ok(())
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
struct EditManifestParams {
    #[arg(help = "help.arg.db-apply-expr")]
    expression: String,
}

rpc_toolkit::reflect_ts!(EditManifestParams);
async fn edit_manifest(
    ctx: CliContext,
    EditManifestParams { expression }: EditManifestParams,
    S9pkPath { s9pk: s9pk_path }: S9pkPath,
) -> Result<Manifest, Error> {
    let mut s9pk = super::load(
        MultiCursorFile::from(open_file(&s9pk_path).await?),
        || ctx.build_key(),
        None,
    )
    .await?;
    let old = serde_json::to_value(s9pk.as_manifest()).with_kind(ErrorKind::Serialization)?;
    *s9pk.as_manifest_mut() = serde_json::from_value(apply_expr(old.into(), &expression)?.into())
        .with_kind(ErrorKind::Serialization)?;
    let manifest = s9pk.as_manifest().clone();
    let tmp_path = s9pk_path.with_extension("s9pk.tmp");
    let mut tmp_file = create_file(&tmp_path).await?;
    s9pk.as_archive_mut()
        .set_signer(ctx.build_key()?, SIG_CONTEXT);
    s9pk.serialize(&mut tmp_file, true).await?;
    tmp_file.sync_all().await?;
    tokio::fs::rename(&tmp_path, &s9pk_path).await?;

    Ok(manifest)
}

async fn file_tree(
    ctx: CliContext,
    _: Empty,
    S9pkPath { s9pk: s9pk_path }: S9pkPath,
) -> Result<Vec<PathBuf>, Error> {
    let s9pk = super::load(
        MultiCursorFile::from(open_file(&s9pk_path).await?),
        || ctx.build_key(),
        None,
    )
    .await?;
    Ok(s9pk.as_archive().contents().file_paths(""))
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
struct CatParams {
    #[arg(help = "help.arg.file-path")]
    file_path: PathBuf,
}

rpc_toolkit::reflect_ts!(CatParams);
async fn cat(
    ctx: CliContext,
    CatParams { file_path }: CatParams,
    S9pkPath { s9pk: s9pk_path }: S9pkPath,
) -> Result<(), Error> {
    use crate::s9pk::merkle_archive::source::FileSource;

    let s9pk = super::load(
        MultiCursorFile::from(open_file(&s9pk_path).await?),
        || ctx.build_key(),
        None,
    )
    .await?;
    tokio::io::copy(
        &mut s9pk
            .as_archive()
            .contents()
            .get_path(&file_path)
            .or_not_found(&file_path.display())?
            .as_file()
            .or_not_found(&file_path.display())?
            .reader()
            .await?,
        &mut tokio::io::stdout(),
    )
    .await?;
    Ok(())
}

async fn inspect_manifest(
    ctx: CliContext,
    _: Empty,
    S9pkPath { s9pk: s9pk_path }: S9pkPath,
) -> Result<Manifest, Error> {
    let s9pk = super::load(
        MultiCursorFile::from(open_file(&s9pk_path).await?),
        || ctx.build_key(),
        None,
    )
    .await?;
    Ok(s9pk.as_manifest().clone())
}

async fn inspect_commitment(
    _: CliContext,
    _: Empty,
    S9pkPath { s9pk: s9pk_path }: S9pkPath,
) -> Result<MerkleArchiveCommitment, Error> {
    let s9pk = super::S9pk::open(&s9pk_path, None).await?;
    s9pk.as_archive().commitment().await
}

async fn convert(ctx: CliContext, S9pkPath { s9pk: s9pk_path }: S9pkPath) -> Result<(), Error> {
    let mut s9pk = super::load(
        MultiCursorFile::from(open_file(&s9pk_path).await?),
        || ctx.build_key(),
        None,
    )
    .await?;
    let tmp_path = s9pk_path.with_extension("s9pk.tmp");
    s9pk.serialize(&mut create_file(&tmp_path).await?, true)
        .await?;
    tokio::fs::rename(tmp_path, s9pk_path).await?;
    Ok(())
}

async fn publish(ctx: CliContext, S9pkPath { s9pk: s9pk_path }: S9pkPath) -> Result<(), Error> {
    let filename = s9pk_path.file_name().unwrap().to_string_lossy();
    let s9pk = super::S9pk::open(&s9pk_path, None).await?;
    let manifest = s9pk.as_manifest();
    let path = [
        manifest.id.deref(),
        manifest.version.as_str().trim_start_matches('#'),
        filename.deref(),
    ];
    let mut s3url = ctx
        .s9pk_s3base
        .as_ref()
        .ok_or_else(|| Error::new(eyre!("--s9pk-s3base required"), ErrorKind::InvalidRequest))?
        .clone();
    s3url
        .path_segments_mut()
        .map_err(|_| {
            Error::new(
                eyre!("s9pk-s3base is invalid (missing protocol?)"),
                ErrorKind::ParseUrl,
            )
        })?
        .pop_if_empty()
        .extend(path);

    let mut s3dest = format!(
        "s3://{}",
        ctx.s9pk_s3bucket
            .as_deref()
            .or_else(|| s3url
                .host_str()
                .and_then(|h| h.split_once(".").map(|h| h.0)))
            .ok_or_else(|| {
                Error::new(eyre!("--s9pk-s3bucket required"), ErrorKind::InvalidRequest)
            })?,
    )
    .parse::<Url>()?;
    s3dest
        .path_segments_mut()
        .map_err(|_| {
            Error::new(
                eyre!("s9pk-s3base is invalid (missing protocol?)"),
                ErrorKind::ParseUrl,
            )
        })?
        .pop_if_empty()
        .extend(path);
    Command::new("s3cmd")
        .arg("put")
        .arg("-P")
        .arg(s9pk_path)
        .arg(s3dest.as_str())
        .capture(false)
        .invoke(ErrorKind::Network)
        .await?;
    crate::registry::package::add::cli_add_package_impl(ctx, s9pk, vec![s3url], false).await
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
struct SelectParams {
    #[arg(help = "help.arg.s9pk-file-paths")]
    s9pks: Vec<PathBuf>,
}

async fn select(
    HandlerArgs {
        context,
        params: SelectParams { s9pks },
        ..
    }: HandlerArgs<CliContext, SelectParams>,
) -> Result<PathBuf, Error> {
    // Resolve file list: use provided paths or scan cwd for *.s9pk
    let paths = if s9pks.is_empty() {
        let mut found = Vec::new();
        let mut entries = tokio::fs::read_dir(".").await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("s9pk") {
                found.push(path);
            }
        }
        if found.is_empty() {
            return Err(Error::new(
                eyre!("no .s9pk files found in current directory"),
                ErrorKind::NotFound,
            ));
        }
        found
    } else {
        s9pks
    };

    // Fetch DeviceInfo from the target server
    let device_info: DeviceInfo = from_value(
        context
            .call_remote::<RpcContext>("server.device-info", imbl_value::json!({}))
            .await?,
    )?;

    // Filter and rank s9pk files by compatibility
    let mut compatible: Vec<(PathBuf, HardwareRequirements)> = Vec::new();
    for path in &paths {
        let s9pk = match super::S9pk::open(path, None).await {
            Ok(s9pk) => s9pk,
            Err(e) => {
                tracing::warn!("skipping {}: {e}", path.display());
                continue;
            }
        };
        let manifest = s9pk.as_manifest();

        // OS version check: package's required OS version must be in server's compat range
        if !manifest
            .metadata
            .os_version
            .satisfies(&device_info.os.compat)
        {
            continue;
        }

        let hw_req = &manifest.hardware_requirements;

        if let Some(hw) = &device_info.hardware {
            if !hw_req.is_compatible(hw) {
                continue;
            }
        }

        compatible.push((path.clone(), hw_req.clone()));
    }

    if compatible.is_empty() {
        return Err(Error::new(
            eyre!(
                "no compatible s9pk found for device (arch: {}, os: {})",
                device_info
                    .hardware
                    .as_ref()
                    .map(|h| h.arch.to_string())
                    .unwrap_or_else(|| "unknown".into()),
                device_info.os.version,
            ),
            ErrorKind::NotFound,
        ));
    }

    // Sort by specificity (most specific first)
    compatible.sort_by_key(|(_, req)| req.specificity_desc());

    Ok(compatible.into_iter().next().unwrap().0)
}
