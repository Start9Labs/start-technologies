use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};

use async_compression::tokio::bufread::{GzipDecoder, ZstdDecoder};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, BufReader, ReadBuf};

use super::pack::DockerImageConfig;
use crate::prelude::*;
use crate::s9pk::merkle_archive::source::{DynFileSource, FileSource};
use crate::util::squashfs::{Entry, LayerFilesystem, Squashfs, resolve_hardlink};

mod oci;

#[cfg(test)]
pub(crate) use oci::tests::{legacy_docker_save_fixture, simple_archive as oci_test_archive};

const ARCHITECTURES: &[(&str, &str)] = &[("x86_64", "amd64"), ("aarch64", "arm64")];

pub(super) fn oci_architecture(arch: &str) -> &str {
    ARCHITECTURES
        .iter()
        .find(|(package, _)| *package == arch)
        .map_or(arch, |(_, oci)| *oci)
}

pub(super) fn package_architecture(arch: &str) -> &str {
    ARCHITECTURES
        .iter()
        .find(|(_, oci)| *oci == arch)
        .map_or(arch, |(package, _)| *package)
}

#[derive(Debug, thiserror::Error)]
#[error("image archive has no image for the requested platform")]
pub(super) struct MissingImagePlatform;

fn invalid(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}

#[derive(Deserialize)]
pub(super) struct ImagePlatform {
    #[serde(alias = "Os")]
    os: String,
    #[serde(alias = "Architecture")]
    architecture: String,
}

impl ImagePlatform {
    pub(super) fn matches(&self, platform: &str) -> bool {
        platform.strip_prefix("--platform=")
            == Some(format!("{}/{}", self.os, self.architecture).as_str())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ArchiveManifest {
    config: PathBuf,
    layers: Option<Vec<PathBuf>>,
    repo_tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct RootFs {
    #[serde(rename = "type")]
    kind: String,
    diff_ids: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ArchiveConfig {
    #[serde(flatten)]
    platform: ImagePlatform,
    config: Option<DockerImageConfig>,
    rootfs: RootFs,
}

fn archive_file<'a, S>(archive: &'a Squashfs<S>, path: &Path) -> Result<&'a S, Error> {
    let node = resolve_hardlink(path, |target| {
        archive
            .contents()
            .get_path(target)
            .map(|entry| match entry {
                Entry::Node(node) => Ok(node),
                Entry::Hardlink(target) => Err(target.clone()),
            })
    })?;
    match &node.contents {
        crate::util::squashfs::NodeContents::File(source) => Ok(source),
        _ => Err(invalid("image archive member is not a regular file").into()),
    }
}

fn digest_matches(expected: &str, digest: impl AsRef<[u8]>) -> bool {
    expected.strip_prefix("sha256:") == Some(hex::encode(digest).as_str())
}

struct DigestReader<R, H> {
    reader: R,
    hash: H,
    size: u64,
}

impl<R, H> DigestReader<R, H> {
    fn new(reader: R, hash: H) -> Self {
        Self {
            reader,
            hash,
            size: 0,
        }
    }
}

impl<R: AsyncRead + Unpin, H: Digest + Unpin> AsyncRead for DigestReader<R, H> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let start = buf.filled().len();
        match Pin::new(&mut this.reader).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                let read = &buf.filled()[start..];
                this.hash.update(read);
                this.size += read.len() as u64;
                Poll::Ready(Ok(()))
            }
            result => result,
        }
    }
}

async fn layer_reader<'a>(
    mut reader: impl AsyncRead + Unpin + Send + 'a,
) -> Result<Box<dyn AsyncRead + Unpin + Send + 'a>, Error> {
    let mut magic = [0; 4];
    reader.read_exact(&mut magic).await?;
    let reader = BufReader::new(Cursor::new(magic).chain(reader));
    if magic.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = GzipDecoder::new(reader);
        decoder.multiple_members(true);
        Ok(Box::new(decoder))
    } else if magic == [0x28, 0xb5, 0x2f, 0xfd]
        || (magic[0] & 0xf0 == 0x50 && magic[1..] == [0x2a, 0x4d, 0x18])
    {
        let mut decoder = ZstdDecoder::new(reader);
        decoder.multiple_members(true);
        Ok(Box::new(decoder))
    } else {
        Ok(Box::new(reader))
    }
}

pub(super) async fn from_image_archive<R: AsyncRead + Unpin + Send>(
    reader: R,
    platform: &str,
) -> Result<(Squashfs<DynFileSource>, DockerImageConfig), Error> {
    let archive = Squashfs::from_tar(reader).await?;
    from_archive(&archive, platform, None).await
}

pub(super) async fn from_path(
    path: &Path,
    platform: &str,
    reference: Option<&str>,
) -> Result<(Squashfs<DynFileSource>, DockerImageConfig), Error> {
    if tokio::fs::metadata(path).await?.is_dir() {
        from_archive(&Squashfs::from_directory(path).await?, platform, reference).await
    } else {
        from_archive(
            &Squashfs::from_tar(crate::util::io::open_file(path).await?).await?,
            platform,
            reference,
        )
        .await
    }
}

pub(super) async fn from_archive<S: FileSource + Clone>(
    archive: &Squashfs<S>,
    platform: &str,
    reference: Option<&str>,
) -> Result<(Squashfs<DynFileSource>, DockerImageConfig), Error> {
    if archive.contents().get("oci-layout").is_some() {
        return oci::from_layout(archive, platform, reference).await;
    }
    let manifests: Vec<ArchiveManifest> = serde_json::from_slice(
        &archive_file(&archive, Path::new("manifest.json"))?
            .to_vec(None)
            .await?,
    )
    .with_kind(ErrorKind::Deserialization)?;
    let mut selected = None;
    for manifest in manifests {
        if reference.is_some_and(|reference| {
            !manifest
                .repo_tags
                .as_ref()
                .is_some_and(|tags| tags.iter().any(|tag| tag == reference))
        }) {
            continue;
        }
        let config: ArchiveConfig = serde_json::from_slice(
            &archive_file(&archive, &manifest.config)?
                .to_vec(None)
                .await?,
        )
        .with_kind(ErrorKind::Deserialization)?;
        if !config.platform.matches(platform) {
            continue;
        }
        if selected.replace((manifest, config)).is_some() {
            return Err(
                invalid("image archive has several images for the requested platform").into(),
            );
        }
    }
    let (manifest, config) =
        selected.ok_or_else(|| Error::new(MissingImagePlatform, ErrorKind::ParseS9pk))?;
    let layers = manifest
        .layers
        .unwrap_or_default()
        .iter()
        .map(|path| {
            archive_file(&archive, path)
                .cloned()
                .map(|source| (DynFileSource::new(source), None))
        })
        .collect::<Result<Vec<_>, _>>()?;
    from_layers(config, layers, platform).await
}

async fn apply_layer(
    filesystem: &mut LayerFilesystem,
    reader: impl AsyncRead + Unpin + Send,
    diff_id: &str,
) -> Result<(), Error> {
    let mut reader = DigestReader::new(layer_reader(reader).await?, Sha256::new());
    filesystem.apply_tar(&mut reader).await?;
    if !digest_matches(diff_id, reader.hash.finalize()) {
        return Err(invalid("image layer does not match its configured digest").into());
    }
    Ok(())
}

async fn from_layers(
    config: ArchiveConfig,
    layers: Vec<(DynFileSource, Option<oci_spec::image::Descriptor>)>,
    platform: &str,
) -> Result<(Squashfs<DynFileSource>, DockerImageConfig), Error> {
    if !config.platform.matches(platform) {
        return Err(invalid("image configuration does not match the requested platform").into());
    }
    if config.rootfs.kind != "layers" {
        return Err(invalid("unsupported image rootfs type").into());
    }
    let diff_ids = config.rootfs.diff_ids.unwrap_or_default();
    if layers.len() != diff_ids.len() {
        return Err(invalid("image archive layer count does not match its configuration").into());
    }
    let mut filesystem = LayerFilesystem::new();
    for ((source, blob), diff_id) in layers.into_iter().zip(diff_ids) {
        let reader = source.reader().await?;
        match blob {
            Some(descriptor) => {
                oci::apply_blob(&mut filesystem, reader, &diff_id, &descriptor).await?
            }
            None => apply_layer(&mut filesystem, reader, &diff_id).await?,
        }
    }
    Ok((
        filesystem.into_squashfs()?,
        config.config.unwrap_or_default(),
    ))
}

#[cfg(test)]
pub(super) mod tests;
