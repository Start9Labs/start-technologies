use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use exver::{ExtendedVersion, VersionRange};
use tokio::io::{AsyncRead, AsyncSeek, AsyncWriteExt};
use tokio::process::Command;

use crate::dependencies::{DepInfo, Dependencies};
use crate::prelude::*;
use crate::registry::package::index::PackageMetadata;
use crate::s9pk::manifest::{DeviceFilter, LocaleString, Manifest};
use crate::s9pk::merkle_archive::directory_contents::DirectoryContents;
use crate::s9pk::merkle_archive::source::TmpSource;
use crate::s9pk::merkle_archive::{Entry, MerkleArchive};
use crate::s9pk::v1::manifest::{Manifest as ManifestV1, PackageProcedure};
use crate::s9pk::v1::reader::S9pkReader;
use crate::s9pk::v2::pack::{CONTAINER_TOOL, ImageSource, PackSource};
use crate::s9pk::v2::{S9pk, SIG_CONTEXT};
use crate::util::Invoke;
use crate::util::io::{TmpDir, create_file};
use crate::{ImageId, VolumeId};

pub const MAGIC_AND_VERSION: &[u8] = &[0x3b, 0x3b, 0x01];

const STUB_INSTRUCTIONS: &[u8] = b"# Instructions\n\nThis package was migrated from an earlier version of StartOS and did not include instructions. See the service's website or upstream documentation for setup and usage.\n";

impl S9pk<TmpSource<PackSource>> {
    #[instrument(skip_all)]
    pub async fn from_v1<R: AsyncRead + AsyncSeek + Unpin + Send + Sync>(
        mut reader: S9pkReader<R>,
        tmp_dir: Arc<TmpDir>,
        signer: ed25519_dalek::SigningKey,
    ) -> Result<Self, Error> {
        Command::new(*CONTAINER_TOOL)
            .arg("run")
            .arg("--rm")
            .arg("--privileged")
            .arg("tonistiigi/binfmt")
            .arg("--install")
            .arg("all")
            .invoke(ErrorKind::Docker)
            .await?;

        let mut archive = DirectoryContents::<TmpSource<PackSource>>::new();

        // manifest.json
        let manifest_raw = reader.manifest().await?;
        let manifest = from_value::<ManifestV1>(manifest_raw.clone())?;
        let mut new_manifest = Manifest::try_from(manifest.clone())?;

        let images: BTreeSet<(ImageId, bool)> = manifest
            .package_procedures()
            .filter_map(|p| {
                if let PackageProcedure::Docker(p) = p {
                    Some((p.image.clone(), p.system))
                } else {
                    None
                }
            })
            .collect();

        // LICENSE.md
        let license: Arc<[u8]> = reader.license().await?.to_vec().await?.into();
        archive.insert_path(
            "LICENSE.md",
            Entry::file(TmpSource::new(
                tmp_dir.clone(),
                PackSource::Buffered(license.into()),
            )),
        )?;

        // instructions.md — v1 packages may lack instructions; stub a placeholder so a
        // missing file never breaks migration.
        let instructions = match reader.instructions().await {
            Ok(handle) => handle.to_vec().await.unwrap_or_default(),
            Err(e) => {
                tracing::warn!("could not read instructions from v1 s9pk, using placeholder: {e}");
                Vec::new()
            }
        };
        let instructions: Arc<[u8]> = if instructions.is_empty() {
            Arc::from(STUB_INSTRUCTIONS)
        } else {
            Arc::from(instructions)
        };
        archive.insert_path(
            "instructions.md",
            Entry::file(TmpSource::new(
                tmp_dir.clone(),
                PackSource::Buffered(instructions),
            )),
        )?;

        // icon.*
        let icon: Arc<[u8]> = reader.icon().await?.to_vec().await?.into();
        archive.insert_path(
            format!("icon.{}", manifest.assets.icon_type()),
            Entry::file(TmpSource::new(
                tmp_dir.clone(),
                PackSource::Buffered(icon.into()),
            )),
        )?;

        // images
        for arch in reader.docker_arches().await? {
            Command::new(*CONTAINER_TOOL)
                .arg("load")
                .input(Some(&mut reader.docker_images(&arch).await?))
                .invoke(ErrorKind::Docker)
                .await?;
            for (image, system) in &images {
                let mut image_config = new_manifest.images.remove(image).unwrap_or_default();
                image_config.arch.insert(arch.as_str().into());
                new_manifest.images.insert(image.clone(), image_config);
                let image_name = if *system {
                    format!("start9/{}:latest", image)
                } else {
                    format!("start9/{}/{}:{}", manifest.id, image, manifest.version)
                };
                ImageSource::DockerTag(image_name.clone())
                    .load(
                        tmp_dir.clone(),
                        &new_manifest.id,
                        &new_manifest.version,
                        image,
                        &arch,
                        &mut archive,
                    )
                    .await?;
                Command::new(*CONTAINER_TOOL)
                    .arg("rmi")
                    .arg("-f")
                    .arg(&image_name)
                    .invoke(ErrorKind::Docker)
                    .await?;
            }
        }

        // assets
        let asset_dir = tmp_dir.join("assets");
        tokio::fs::create_dir_all(&asset_dir).await?;
        tokio_tar::Archive::new(reader.assets().await?)
            .unpack(&asset_dir)
            .await?;
        let sqfs_path = asset_dir.with_extension("squashfs");
        Command::new("mksquashfs")
            .arg(&asset_dir)
            .arg(&sqfs_path)
            .invoke(ErrorKind::Filesystem)
            .await?;
        archive.insert_path(
            "assets.squashfs",
            Entry::file(TmpSource::new(tmp_dir.clone(), PackSource::File(sqfs_path))),
        )?;

        // javascript
        let js_dir = tmp_dir.join("javascript");
        let sqfs_path = js_dir.with_extension("squashfs");
        tokio::fs::create_dir_all(&js_dir).await?;
        if let Some(mut scripts) = reader.scripts().await? {
            let mut js_file = create_file(js_dir.join("embassy.js")).await?;
            tokio::io::copy(&mut scripts, &mut js_file).await?;
            js_file.sync_all().await?;
        }
        {
            let mut js_file = create_file(js_dir.join("embassyManifest.json")).await?;
            js_file
                .write_all(&serde_json::to_vec(&manifest_raw).with_kind(ErrorKind::Serialization)?)
                .await?;
            js_file.sync_all().await?;
        }
        Command::new("mksquashfs")
            .arg(&js_dir)
            .arg(&sqfs_path)
            .invoke(ErrorKind::Filesystem)
            .await?;
        archive.insert_path(
            Path::new("javascript.squashfs"),
            Entry::file(TmpSource::new(tmp_dir.clone(), PackSource::File(sqfs_path))),
        )?;

        archive.insert_path(
            "manifest.json",
            Entry::file(TmpSource::new(
                tmp_dir.clone(),
                PackSource::Buffered(
                    serde_json::to_vec::<Manifest>(&new_manifest)
                        .with_kind(ErrorKind::Serialization)?
                        .into(),
                ),
            )),
        )?;

        let mut res = S9pk::new(MerkleArchive::new(archive, signer, SIG_CONTEXT), None).await?;
        res.as_archive_mut().update_hashes(true).await?;
        Ok(res)
    }
}

impl TryFrom<ManifestV1> for Manifest {
    type Error = Error;
    fn try_from(mut value: ManifestV1) -> Result<Self, Self::Error> {
        let default_url = value.upstream_repo.clone();
        let mut version = ExtendedVersion::from(
            exver::emver::Version::from_str(&value.version)
                .with_kind(ErrorKind::Deserialization)?,
        );
        if &*value.id == "bitcoind" && value.title.to_ascii_lowercase().contains("knots") {
            version = version.with_flavor("knots");
        } else if &*value.id == "lnd" || &*value.id == "ride-the-lightning" || &*value.id == "datum"
        {
            version = version.map_upstream(|v| v.with_prerelease(["beta".into()]));
        } else if &*value.id == "lightning-terminal" || &*value.id == "robosats" {
            version = version.map_upstream(|v| v.with_prerelease(["alpha".into()]));
        }
        if &*value.id == "nostr" {
            value.id = "nostr-rs-relay".parse()?;
        }
        if &*value.id == "ghost" {
            value.id = "ghost-legacy".parse()?;
        }
        if &*value.id == "synapse" {
            value.id = "synapse-legacy".parse()?;
        }
        if &*value.id == "monerod" {
            value.id = "monerod-legacy".parse()?;
        }
        if &*value.id == "fedimintd" {
            value.id = "fedimint-guardian".parse()?;
        }
        Ok(Self {
            id: value.id,
            version: version.into(),
            can_migrate_from: VersionRange::any(),
            can_migrate_to: VersionRange::none(),
            metadata: PackageMetadata {
                title: format!("{} (Legacy)", value.title).into(),
                release_notes: LocaleString::Translated(value.release_notes),
                license: value.license.into(),
                package_repo: value.wrapper_repo,
                upstream_repo: value.upstream_repo,
                marketing_url: Some(value.marketing_site.unwrap_or_else(|| default_url.clone())),
                donation_url: value.donation_url,
                description: value.description,
                git_hash: value.git_hash,
                os_version: value.eos_version,
                sdk_version: None,
                hardware_acceleration: match value.main {
                    PackageProcedure::Docker(d) => d.gpu_acceleration,
                    PackageProcedure::Script(_) => false,
                },
                userspace_filesystems: false,
                virtual_networking: false,
                plugins: BTreeSet::new(),
                satisfies: BTreeSet::new(),
            },
            images: BTreeMap::new(),
            volumes: value
                .volumes
                .iter()
                .filter(|(_, v)| v.get("type").and_then(|v| v.as_str()) == Some("data"))
                .map(|(id, _)| id.clone())
                .chain([VolumeId::from_str("embassy").unwrap()])
                .collect(),
            dependencies: Dependencies(
                value
                    .dependencies
                    .into_iter()
                    .map(|(id, value)| {
                        (
                            id,
                            DepInfo {
                                description: value.description.map(LocaleString::Translated),
                                optional: !value.requirement.required(),
                                metadata: None,
                            },
                        )
                    })
                    .collect(),
            ),
            hardware_requirements: super::manifest::HardwareRequirements {
                arch: value.hardware_requirements.arch,
                ram: value.hardware_requirements.ram,
                device: value
                    .hardware_requirements
                    .device
                    .into_iter()
                    .map(|(class, product)| DeviceFilter {
                        description: format!(
                            "a {class} device matching the expression {}",
                            product.as_ref()
                        ),
                        class,
                        product: Some(product),
                        ..Default::default()
                    })
                    .collect(),
            },
        })
    }
}
