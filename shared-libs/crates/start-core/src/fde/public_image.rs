//! Public build inputs use existing OS-update signing trust, never a captured repack.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use futures::StreamExt;

use super::namespace::invalid_source;
use super::source::BuildSource;
use crate::context::RpcContext;
use crate::middleware::auth::signature::SigningContext;
use crate::os_overlay::public_source::{Release, SourceCache, invalid_source as unavailable};
use crate::os_overlay::{PreparedGeneration, VerityImage};
use crate::prelude::*;
use crate::registry::asset::RegistryAsset;
use crate::registry::os::SIG_CONTEXT;
use crate::registry::os::asset::get::GetOsAssetParams;
use crate::sign::commitment::blake3::Blake3Commitment;
use crate::trusted_keys::{self, TrustContext};
use crate::util::io::create_file_mod;
use crate::util::{FileLock, Invoke};

const FILES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../projects/start-os/build/lib/scripts/fde-boot-input-files"
));
const ASSEMBLER: &str = "/usr/lib/startos/scripts/fde-assemble-boot-inputs-v2.mjs";
pub(crate) const SOURCE_APPLET: &str = "start-fde-source";

/// The caller retains the image-store lease and fresh configuration authorization.
/// The descriptor is public boot data, not authority to execute source providers.
pub(super) async fn stage(
    ctx: &RpcContext,
    generation: &PreparedGeneration,
    descriptor: &super::boot::Descriptor,
    lease: &FileLock,
) -> Result<(), Error> {
    let expected = Release::read(Path::new("/media/startos/current")).await?;
    let policy =
        trusted_keys::signer_policy(trusted_keys::required(ctx, TrustContext::Os, false).await?)
            .expect("required OS signer policy");
    let cache = SourceCache::system();
    // Native space admission repairs legacy pointers before retention pruning.
    cache.recover_workspace().await?;
    let cached = cache.find_approved(&expected, policy.clone()).await?;
    let asset: Option<RegistryAsset<Blake3Commitment>> = if cached.is_none() {
        let registry = ctx
            .db
            .peek()
            .await
            .as_public()
            .as_server_info()
            .as_os_registry()
            .de()?;
        let asset: RegistryAsset<Blake3Commitment> = from_value(
            crate::registry::context::call(
                &ctx.client,
                &ctx.signing_key()?,
                &registry,
                reqwest::header::HeaderMap::new(),
                "os.asset.get.squashfs",
                to_value(&GetOsAssetParams {
                    version: expected.version.clone(),
                    platform: expected.platform.clone().into(),
                })?,
            )
            .await?,
        )?;
        asset.validate_all(SIG_CONTEXT, Some(policy))?;
        Some(asset)
    } else {
        None
    };
    crate::os_overlay::admit_image_space(
        lease,
        asset.as_ref().map_or(0, |asset| asset.commitment.size),
    )
    .await?;
    let directory = cache.create_workspace().await?;
    let result = async {
        let image = match asset {
            Some(asset) => {
                let path = directory.join("download.squashfs");
                let mut output = create_file_mod(&path, 0o600).await?;
                asset.download(ctx.client.get(), &mut output).await?;
                output.sync_all().await?;
                drop(output);
                cache.retain(asset, &path).await?
            }
            None => cached.expect("cached source or admitted asset"),
        };
        if image.release != expected {
            return Err(unavailable());
        }
        tokio::fs::hard_link(&image.path, directory.join("image.squashfs")).await?;
        let mut input = std::io::Cursor::new(
            serde_json::to_vec(descriptor).with_kind(ErrorKind::Serialization)?,
        );
        let bundle = source_command(lease, &std::env::current_exe()?, &directory)?
            .input(Some(&mut input))
            .invoke(ErrorKind::Filesystem)
            .await?;
        stage_bundle(&bundle, generation.staging_path()).await
    }
    .await;
    let cleanup = cache.recover_workspace().await;
    match result {
        Ok(()) => cleanup,
        Err(error) => {
            cleanup.log_err();
            Err(error)
        }
    }
}

fn source_command(
    lease: &FileLock,
    executable: &Path,
    directory: &Path,
) -> Result<tokio::process::Command, Error> {
    let mut command = crate::os_overlay::image_store_command(lease, "/usr/bin/unshare")?;
    command
        .args([
            "--mount",
            "--propagation=private",
            "--pid",
            "--fork",
            "--kill-child=SIGKILL",
            "--",
        ])
        .arg(executable)
        .arg(SOURCE_APPLET)
        .arg(directory);
    Ok(command)
}

/// The caller admitted these stable public bytes under current OS signer policy.
/// Runs in a private namespace with the inherited image-store lease retained through teardown.
pub(crate) async fn assemble_source(directory: &Path, input: Vec<u8>) -> Result<Vec<u8>, Error> {
    let path = directory.join("image.squashfs");
    let expected = Release::read_image(&path).await?;
    let verity = VerityImage::format(&path, &directory.join("image.verity")).await?;
    let mapping = verity
        .mount(
            &path,
            &directory.join("image.verity"),
            crate::os_overlay::public_source::SOURCE_MAPPING,
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
    source.run_script(ASSEMBLER.into(), Vec::new(), input).await
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
