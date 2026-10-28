use std::collections::{BTreeMap, BTreeSet};

use base64::Engine;
use oci_spec::distribution::Reference;
use oci_spec::image::{
    Descriptor, DigestAlgorithm, ImageIndex, ImageManifest, MediaType, OciLayout, ToDockerV2S2,
};
use sha2::{Digest, Sha256, Sha384, Sha512};

use super::*;

#[cfg(test)]
pub(super) mod tests;

#[derive(Deserialize)]
#[serde(untagged)]
enum Manifest {
    Image(ImageManifest),
    Index(ImageIndex),
}

fn media_matches(actual: &MediaType, expected: MediaType) -> bool {
    actual == &expected
        || expected
            .to_docker_v2s2()
            .is_ok_and(|media| actual.as_ref() == media)
}

fn blob_path(descriptor: &Descriptor) -> PathBuf {
    Path::new("blobs")
        .join(descriptor.digest().algorithm().as_ref())
        .join(descriptor.digest().digest())
}

fn blob_limit(descriptor: &Descriptor) -> Result<u64, Error> {
    Ok(descriptor
        .size()
        .checked_add(1)
        .ok_or_else(|| invalid("OCI blob size overflow"))?)
}

fn verify_blob(descriptor: &Descriptor, size: u64, digest: impl AsRef<[u8]>) -> Result<(), Error> {
    if size != descriptor.size() {
        return Err(invalid("OCI blob does not match its descriptor size").into());
    }
    if hex::encode(digest) != descriptor.digest().digest() {
        return Err(invalid("OCI blob does not match its descriptor digest").into());
    }
    Ok(())
}

fn unsupported_digest() -> Error {
    invalid("unsupported OCI digest algorithm").into()
}

fn blob_source<S: FileSource + Clone>(
    archive: &Squashfs<S>,
    descriptor: &Descriptor,
) -> Result<DynFileSource, Error> {
    Ok(if let Some(data) = descriptor.data() {
        DynFileSource::new(std::sync::Arc::<[u8]>::from(
            base64::prelude::BASE64_STANDARD
                .decode(data)
                .map_err(|_| invalid("invalid OCI embedded blob"))?,
        ))
    } else {
        DynFileSource::new(archive_file(archive, &blob_path(descriptor))?.clone())
    })
}

async fn checked_blob<S: FileSource + Clone>(
    archive: &Squashfs<S>,
    descriptor: &Descriptor,
) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    blob_source(archive, descriptor)?
        .reader()
        .await?
        .take(blob_limit(descriptor)?)
        .read_to_end(&mut bytes)
        .await?;
    let digest = match descriptor.digest().algorithm() {
        DigestAlgorithm::Sha256 => Sha256::digest(&bytes).to_vec(),
        DigestAlgorithm::Sha384 => Sha384::digest(&bytes).to_vec(),
        DigestAlgorithm::Sha512 => Sha512::digest(&bytes).to_vec(),
        _ => return Err(unsupported_digest()),
    };
    verify_blob(descriptor, bytes.len() as u64, digest)?;
    Ok(bytes)
}

async fn apply_verified<H: Digest + Unpin + Send>(
    filesystem: &mut LayerFilesystem,
    reader: impl AsyncRead + Unpin + Send,
    diff_id: &str,
    descriptor: &Descriptor,
) -> Result<(), Error> {
    let mut raw = DigestReader::new(reader.take(blob_limit(descriptor)?), H::new());
    let applied = apply_layer(filesystem, &mut raw, diff_id).await;
    tokio::io::copy(&mut raw, &mut tokio::io::sink()).await?;
    verify_blob(descriptor, raw.size, raw.hash.finalize())?;
    applied
}

pub(super) async fn apply_blob(
    filesystem: &mut LayerFilesystem,
    reader: impl AsyncRead + Unpin + Send,
    diff_id: &str,
    descriptor: &Descriptor,
) -> Result<(), Error> {
    match descriptor.digest().algorithm() {
        DigestAlgorithm::Sha256 => {
            apply_verified::<Sha256>(filesystem, reader, diff_id, descriptor).await
        }
        DigestAlgorithm::Sha384 => {
            apply_verified::<Sha384>(filesystem, reader, diff_id, descriptor).await
        }
        DigestAlgorithm::Sha512 => {
            apply_verified::<Sha512>(filesystem, reader, diff_id, descriptor).await
        }
        _ => Err(unsupported_digest()),
    }
}

fn parse_manifest(bytes: &[u8], expected: &MediaType) -> Result<Manifest, Error> {
    let manifest: Manifest = serde_json::from_slice(bytes).with_kind(ErrorKind::Deserialization)?;
    let (schema, media_type, kind) = match &manifest {
        Manifest::Image(image) => (
            image.schema_version(),
            image.media_type(),
            MediaType::ImageManifest,
        ),
        Manifest::Index(index) => (
            index.schema_version(),
            index.media_type(),
            MediaType::ImageIndex,
        ),
    };
    if schema != 2
        || !media_matches(expected, kind.clone())
        || media_type
            .as_ref()
            .is_some_and(|media| !media_matches(media, kind))
    {
        return Err(invalid("unsupported OCI manifest schema or media type").into());
    }
    Ok(manifest)
}

fn reference_matches(entry: &Descriptor, reference: &str) -> bool {
    let annotation = |key| entry.annotations().as_ref().and_then(|a| a.get(key));
    entry.digest().as_ref() == reference
        || annotation("org.opencontainers.image.ref.name").is_some_and(|name| name == reference)
        || annotation("io.containerd.image.name").is_some_and(|name| {
            match (name.parse::<Reference>(), reference.parse::<Reference>()) {
                (Ok(name), Ok(reference)) => name == reference,
                _ => false,
            }
        })
}

fn layer_media_type(media_type: &MediaType) -> bool {
    matches!(
        media_type,
        MediaType::ImageLayer
            | MediaType::ImageLayerGzip
            | MediaType::ImageLayerZstd
            | MediaType::ImageLayerNonDistributable
            | MediaType::ImageLayerNonDistributableGzip
            | MediaType::ImageLayerNonDistributableZstd
    ) || media_matches(media_type, MediaType::ImageLayerGzip)
        || matches!(
            media_type.as_ref(),
            "application/vnd.docker.image.rootfs.diff.tar"
                | "application/vnd.docker.image.rootfs.foreign.diff.tar.gzip"
        )
}

pub(super) async fn from_layout<S: FileSource + Clone>(
    archive: &Squashfs<S>,
    platform: &str,
    reference: Option<&str>,
) -> Result<(Squashfs<DynFileSource>, DockerImageConfig), Error> {
    let layout: OciLayout = serde_json::from_slice(
        &archive_file(archive, Path::new("oci-layout"))?
            .to_vec(None)
            .await?,
    )
    .with_kind(ErrorKind::Deserialization)?;
    if layout.image_layout_version() != "1.0.0" {
        return Err(invalid("unsupported OCI image layout version").into());
    }
    let root = parse_manifest(
        &archive_file(archive, Path::new("index.json"))?
            .to_vec(None)
            .await?,
        &MediaType::ImageIndex,
    )?;
    let Manifest::Index(mut index) = root else {
        unreachable!()
    };
    if let Some(reference) = reference {
        index.set_manifests(
            index
                .manifests()
                .iter()
                .filter(|entry| reference_matches(entry, reference))
                .cloned()
                .collect(),
        );
    }
    let mut pending = vec![Manifest::Index(index)];
    let mut visited = BTreeSet::new();
    let mut images = BTreeMap::new();
    while let Some(next) = pending.pop() {
        match next {
            Manifest::Index(index) => {
                for entry in index.manifests() {
                    if entry.artifact_type().is_some()
                        || !(media_matches(entry.media_type(), MediaType::ImageManifest)
                            || media_matches(entry.media_type(), MediaType::ImageIndex))
                    {
                        continue;
                    }
                    if let Some(p) = entry.platform() {
                        if !(ImagePlatform {
                            os: p.os().to_string(),
                            architecture: p.architecture().to_string(),
                        })
                        .matches(platform)
                        {
                            continue;
                        }
                    }
                    if !visited.insert(entry.digest().to_string()) {
                        continue;
                    }
                    pending.push(parse_manifest(
                        &checked_blob(archive, entry).await?,
                        entry.media_type(),
                    )?);
                }
            }
            Manifest::Image(image) => {
                if image.artifact_type().is_some()
                    || !media_matches(image.config().media_type(), MediaType::ImageConfig)
                {
                    continue;
                }
                let config: ArchiveConfig =
                    serde_json::from_slice(&checked_blob(archive, image.config()).await?)
                        .with_kind(ErrorKind::Deserialization)?;
                if config.platform.matches(platform) {
                    images
                        .entry(image.config().digest().to_string())
                        .or_insert((config, image.layers().clone()));
                }
            }
        }
    }
    if images.is_empty() {
        return Err(Error::new(MissingImagePlatform, ErrorKind::ParseS9pk));
    }
    if images.len() != 1 {
        return Err(invalid("OCI input selects several images for the requested platform").into());
    }
    let (config, descriptors) = images.into_values().next().unwrap();
    let mut layers = Vec::new();
    for descriptor in descriptors {
        if !layer_media_type(descriptor.media_type()) {
            return Err(invalid("unsupported OCI layer media type").into());
        }
        layers.push((blob_source(archive, &descriptor)?, Some(descriptor)));
    }
    from_layers(config, layers, platform).await
}
