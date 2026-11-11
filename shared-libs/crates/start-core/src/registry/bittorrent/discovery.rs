use std::path::{Path, PathBuf};

use super::stored_file::StoredFile;
use super::{ContentCommitment, METADATA_TIMEOUT};
use crate::context::RpcContext;
use crate::db::model::package::{ManifestPreference, PackageState};
use crate::prelude::*;
use crate::registry::asset::RegistryAsset;
use crate::registry::context::{RegistryContext, RegistryUrlParams};
use crate::registry::os::index::OsIndex;
use crate::s9pk::S9pk;
use crate::sign::commitment::blake3::Blake3Commitment;
use crate::sign::commitment::{Commitment, SignedCommitment};

pub(crate) async fn register_existing(ctx: &RpcContext) -> Result<(), Error> {
    if !ctx.torrents.downloads_enabled() {
        return Ok(());
    }
    let (packages, os_registry) = {
        let peek = ctx.db.peek().await;
        (
            peek.as_public().as_package_data().de()?,
            peek.as_public().as_server_info().as_os_registry().de()?,
        )
    };
    for (id, entry) in packages.0 {
        if !matches!(
            entry.state_info,
            PackageState::Installed(_) | PackageState::Updating(_)
        ) {
            continue;
        }
        let Some(registry) = &entry.registry else {
            continue;
        };
        if ctx.torrents.for_source(registry).is_none() {
            continue;
        }
        let manifest = entry.state_info.as_manifest(ManifestPreference::Old);
        let discovered = async {
            let source = StoredFile::open(&entry.s9pk).map_err(super::torrent_error)?;
            let package = S9pk::deserialize_local(&source, None).await?;
            if package.as_manifest().id != id || package.as_manifest().version != manifest.version {
                return Ok::<_, Error>(());
            }
            let local = package.as_archive().commitment().await?;
            let asset = tokio::time::timeout(
                METADATA_TIMEOUT,
                crate::registry::package::get::advertised_asset(
                    ctx,
                    registry,
                    &id,
                    &manifest.version,
                ),
            )
            .await
            .with_kind(ErrorKind::Timeout)??;
            if let Some(asset) = asset {
                if asset.commitment == local {
                    register_advertisement(ctx, &entry.s9pk, &asset, crate::s9pk::v2::SIG_CONTEXT)
                        .await?;
                }
            }
            Ok(())
        }
        .await;
        if let Err(error) = discovered {
            tracing::warn!(%error, package = %id, "Cannot discover stored S9PK torrent metadata");
        }
    }
    if ctx.torrents.for_source(&os_registry).is_some() {
        discover_images(ctx, os_registry).await?;
    }
    Ok(())
}

async fn register_advertisement<C>(
    ctx: &RpcContext,
    path: &Path,
    asset: &RegistryAsset<C>,
    signature_context: &str,
) -> Result<(), Error>
where
    C: SignedCommitment + Clone + Into<ContentCommitment>,
{
    asset.validate_all(signature_context, None)?;
    if let Some(metadata) = asset
        .sharing_metadata(ctx.client.get(), Some(&ctx.torrents))
        .await?
    {
        ctx.torrents.register_stored(path, &metadata).await?;
    }
    Ok(())
}

fn advertised_images(index: &OsIndex) -> impl Iterator<Item = &RegistryAsset<Blake3Commitment>> {
    index
        .versions
        .0
        .values()
        .flat_map(|version| version.squashfs.values())
        .filter(|asset| asset.torrent.is_some())
}

fn image_for_name<'a>(
    index: &'a OsIndex,
    path: &Path,
) -> Option<&'a RegistryAsset<Blake3Commitment>> {
    let name = path.file_stem()?.to_str()?;
    advertised_images(index).find(|asset| crate::update::image_checksum(&asset.commitment) == name)
}

async fn discover_images(ctx: &RpcContext, registry: url::Url) -> Result<(), Error> {
    let mut entries = match tokio::fs::read_dir(crate::update::IMAGES_DIRECTORY).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let mut images = Vec::<PathBuf>::new();
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "rootfs" || extension == "squashfs")
        {
            images.push(entry.path());
        }
    }
    if images.is_empty() {
        return Ok(());
    }
    let index: OsIndex = from_value(
        tokio::time::timeout(
            METADATA_TIMEOUT,
            ctx.call_remote_with::<RegistryContext, _>(
                "os.index",
                Default::default(),
                imbl_value::json!({}),
                RegistryUrlParams { registry },
            ),
        )
        .await
        .with_kind(ErrorKind::Timeout)??,
    )?;
    for path in images {
        let discovered = async {
            let asset = match image_for_name(&index, &path) {
                Some(asset) => Some(asset),
                None if path.file_name().is_some_and(|name| name == "next.squashfs") => {
                    let source = StoredFile::open(&path).map_err(super::torrent_error)?;
                    let commitment = Blake3Commitment::create(&source).await?;
                    advertised_images(&index).find(|asset| asset.commitment == commitment)
                }
                None => None,
            };
            if let Some(asset) = asset {
                register_advertisement(ctx, &path, asset, crate::registry::os::SIG_CONTEXT).await?;
            }
            Ok::<_, Error>(())
        }
        .await;
        if let Err(error) = discovered {
            tracing::warn!(%error, "Cannot discover stored OS image torrent metadata");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::os::index::OsVersionInfo;
    use crate::util::serde::Base64;

    #[test]
    fn native_image_names_select_only_advertised_squashfs_assets() {
        let mut index = OsIndex::default();
        let asset = RegistryAsset {
            published_at: chrono::Utc::now(),
            urls: vec!["https://example.com/image.squashfs".parse().unwrap()],
            torrent: Some("https://example.com/image.torrent".parse().unwrap()),
            commitment: Blake3Commitment {
                hash: Base64([0xab; 32]),
                size: 1024,
            },
            signatures: Default::default(),
        };
        let mut version = OsVersionInfo::default();
        version.iso.insert("x86_64".into(), asset.clone());
        index.versions.0.insert("0.4.2".parse().unwrap(), version);
        let path = Path::new("/images/abababababababababababababababab.rootfs");
        assert!(image_for_name(&index, path).is_none());
        let version = index.versions.0.values_mut().next().unwrap();
        version.squashfs.insert("x86_64".into(), asset);
        assert!(image_for_name(&index, path).is_some());
        assert!(image_for_name(&index, Path::new("/images/other.rootfs")).is_none());
        index
            .versions
            .0
            .values_mut()
            .next()
            .unwrap()
            .squashfs
            .values_mut()
            .next()
            .unwrap()
            .torrent = None;
        assert!(image_for_name(&index, path).is_none());
    }
}
