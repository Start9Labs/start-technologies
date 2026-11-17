//! Public build inputs use existing OS-update signing trust, never a captured repack.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use futures::StreamExt;
use imbl::OrdMap;

use super::namespace::invalid_source;
use super::source::BuildSource;
use crate::context::RpcContext;
use crate::os_overlay::public_source::{Release, SourceCache, invalid_source as unavailable};
use crate::os_overlay::{PreparedGeneration, VerityImage};
use crate::prelude::*;
use crate::registry::asset::RegistryAsset;
use crate::registry::context::{RegistryContext, RegistryUrlParams};
use crate::registry::os::SIG_CONTEXT;
use crate::registry::os::asset::get::GetOsAssetParams;
use crate::sign::commitment::blake3::Blake3Commitment;
use crate::trusted_keys::{self, TrustContext};
use crate::util::io::{TmpDir, create_file_mod};

const FILES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../projects/start-os/build/lib/scripts/fde-boot-input-files"
));
const ASSEMBLER: &str = "/usr/lib/startos/scripts/fde-assemble-boot-inputs.mjs";

/// The caller retains the image-store lease and fresh configuration authorization.
/// Cached and downloaded sources require existing accepted OS signers.
/// This stages public boot inputs only; closure, bootstrap and UKI activation remain separate.
pub(super) async fn stage(ctx: &RpcContext, generation: &PreparedGeneration) -> Result<(), Error> {
    let expected = Release::read(Path::new("/media/startos/current")).await?;
    let policy =
        trusted_keys::signer_policy(trusted_keys::required(ctx, TrustContext::Os, false).await?)
            .expect("required OS signer policy");
    let cache = SourceCache::system();
    cache.prune_boot_sources().await.log_err();
    let cached = cache.find_approved(&expected, policy.clone()).await?;
    let workspace = TmpDir::new_in("/media/startos/images").await?;
    let directory = workspace.to_path_buf();
    workspace.leak();
    let result = async {
        let image = match cached {
            Some(image) => image,
            None => {
                let registry = ctx
                    .db
                    .peek()
                    .await
                    .as_public()
                    .as_server_info()
                    .as_os_registry()
                    .de()?;
                let asset: RegistryAsset<Blake3Commitment> = from_value(
                    ctx.call_remote_with::<RegistryContext, _>(
                        "os.asset.get.squashfs",
                        OrdMap::new(),
                        to_value(&GetOsAssetParams {
                            version: expected.version.clone(),
                            platform: expected.platform.clone().into(),
                        })?,
                        RegistryUrlParams { registry },
                    )
                    .await?,
                )?;
                asset.validate_all(SIG_CONTEXT, Some(policy))?;
                let path = directory.join("image.squashfs");
                let mut output = create_file_mod(&path, 0o600).await?;
                asset.download(ctx.client.get(), &mut output).await?;
                output.sync_all().await?;
                drop(output);
                cache.retain(asset, &path).await?
            }
        };
        if image.release != expected {
            return Err(unavailable());
        }
        let path = image.path;
        let verity = VerityImage::format(&path, &directory.join("image.verity")).await?;
        let mapping = verity
            .mount(
                &path,
                &directory.join("image.verity"),
                &format!(
                    "startos-fde-source-{}",
                    crate::rpc_continuations::Guid::new()
                ),
                &directory.join("rootfs"),
            )
            .await?;
        let source = BuildSource::mount(
            mapping,
            &directory.join("runtime"),
            &directory.join("source"),
        )
        .await?;
        let identity = Release::read(source.path()).await.and_then(|actual| {
            if actual == expected {
                Ok(())
            } else {
                Err(unavailable())
            }
        });
        if let Err(error) = identity {
            source.unmount().await.log_err();
            return Err(error);
        }
        let bundle = source
            .run_script(ASSEMBLER.into(), Vec::new(), Vec::new())
            .await?;
        stage_bundle(&bundle, generation.staging_path()).await
    }
    .await;
    let cleanup = clean_workspace(&directory).await;
    match result {
        Ok(()) => cleanup,
        Err(error) => {
            cleanup.log_err();
            Err(error)
        }
    }
}

async fn clean_workspace(directory: &Path) -> Result<(), Error> {
    for file in ["image.squashfs", "image.verity"] {
        crate::util::io::delete_file(directory.join(file)).await?;
    }
    for mount in ["source", "runtime", "rootfs"] {
        match tokio::fs::remove_dir(directory.join(mount)).await {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    tokio::fs::remove_dir(directory).await?;
    Ok(())
}

async fn stage_bundle(bytes: &[u8], destination: &Path) -> Result<(), Error> {
    let expected: BTreeSet<_> = FILES.lines().map(PathBuf::from).collect();
    let source = std::io::Cursor::new(bytes);
    let mut archive = tokio_tar::ArchiveBuilder::new(source)
        .set_ignore_zeros(true)
        .build();
    let mut entries = archive.entries()?;
    let mut received = BTreeSet::new();
    while let Some(entry) = entries.next().await {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if !entry.header().entry_type().is_file()
            || !expected.contains(&path)
            || !received.insert(path.clone())
            || entry.header().size()? == 0
        {
            return Err(invalid_source());
        }
        let mut output = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(destination.join(path))
            .await?;
        tokio::io::copy(&mut entry, &mut output).await?;
        output.sync_all().await?;
    }
    if received != expected {
        return Err(invalid_source());
    }
    crate::util::io::open_file(destination)
        .await?
        .sync_all()
        .await?;
    Ok(())
}

#[cfg(test)]
#[path = "public_image_tests.rs"]
mod tests;
