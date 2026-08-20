use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;

use imbl_value::InternedString;
use tokio::fs::File;

use crate::PackageId;
use crate::dependencies::DependencyMetadata;
use crate::prelude::*;
use crate::s9pk::manifest::Manifest;
use crate::s9pk::merkle_archive::sink::Sink;
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::s9pk::merkle_archive::source::{
    ArchiveSource, DynFileSource, FileSource, Section, TmpSource,
};
use crate::s9pk::merkle_archive::{Entry, MerkleArchive};
use crate::s9pk::v2::pack::{ImageSource, PackSource};
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::util::io::{TmpDir, open_file};
use crate::util::serde::IoFormat;
use crate::util::{DataUrl, mime};

const MAGIC_AND_VERSION: &[u8] = &[0x3b, 0x3b, 0x02];

pub const SIG_CONTEXT: &str = "s9pk";

pub mod compat;
pub mod manifest;
pub mod pack;
pub mod recipe;

/**
    /
    ├── manifest.json
    ├── icon.<ext>
    ├── LICENSE.md
    ├── instructions.md
    ├── README.md (optional)
    ├── dependencies
    │   └── <id>
    │       ├── metadata.json
    │       └── icon.<ext>
    ├── javascript.squashfs
    ├── assets
    │   └── <id>.squashfs (xN)
    └── images
        └── <arch>
            ├── <id>.json (xN)
            ├── <id>.env (xN)
            └── <id>.squashfs (xN)
*/

// this sorts the s9pk to optimize such that the parts that are used first appear earlier in the s9pk
// this is useful for manipulating an s9pk while partially downloaded on a source that does not support
// random access
fn priority(s: &str) -> Option<usize> {
    match s {
        "manifest.json" => Some(0),
        a if Path::new(a).file_stem() == Some(OsStr::new("icon")) => Some(1),
        "LICENSE.md" => Some(2),
        "instructions.md" => Some(3),
        "README.md" => Some(4),
        "dependencies" => Some(5),
        "javascript.squashfs" => Some(6),
        "assets.squashfs" => Some(7),
        "images" => Some(8),
        _ => None,
    }
}

#[derive(Clone)]
pub struct S9pk<S = Section<MultiCursorFile>> {
    pub manifest: Manifest,
    manifest_dirty: bool,
    archive: MerkleArchive<S>,
    size: Option<u64>,
}
impl<S> S9pk<S> {
    pub fn as_manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn as_manifest_mut(&mut self) -> &mut Manifest {
        self.manifest_dirty = true;
        &mut self.manifest
    }
    pub fn as_archive(&self) -> &MerkleArchive<S> {
        &self.archive
    }
    pub fn as_archive_mut(&mut self) -> &mut MerkleArchive<S> {
        &mut self.archive
    }
    pub fn size(&self) -> Option<u64> {
        self.size
    }
}

impl<S: FileSource + Clone> S9pk<S> {
    pub async fn new(archive: MerkleArchive<S>, size: Option<u64>) -> Result<Self, Error> {
        let manifest = extract_manifest(&archive).await?;
        Ok(Self {
            manifest,
            manifest_dirty: false,
            archive,
            size,
        })
    }

    pub fn new_with_manifest(
        archive: MerkleArchive<S>,
        size: Option<u64>,
        manifest: Manifest,
    ) -> Self {
        Self {
            manifest,
            manifest_dirty: true,
            archive,
            size,
        }
    }

    pub fn validate_and_filter(&mut self, arch: Option<&str>) -> Result<(), Error> {
        let filter = self.manifest.validate_for(arch, self.archive.contents())?;
        filter.keep_checked(self.archive.contents_mut())
    }

    pub async fn icon(&self) -> Result<(InternedString, Entry<S>), Error> {
        let mut best_icon = None;
        for (path, icon) in self.archive.contents().with_stem("icon").filter(|(p, v)| {
            Path::new(&*p)
                .extension()
                .and_then(|e| e.to_str())
                .and_then(mime)
                .map_or(false, |e| e.starts_with("image/") && v.as_file().is_some())
        }) {
            let size = icon.expect_file()?.size().await?;
            best_icon = match best_icon {
                Some((s, a)) if s >= size => Some((s, a)),
                _ => Some((size, (path, icon))),
            };
        }
        best_icon
            .map(|(_, a)| a)
            .ok_or_else(|| Error::new(eyre!("no icon found in archive"), ErrorKind::ParseS9pk))
    }

    pub async fn icon_data_url(&self) -> Result<DataUrl<'static>, Error> {
        let (name, contents) = self.icon().await?;
        let mime = Path::new(&*name)
            .extension()
            .and_then(|e| e.to_str())
            .and_then(mime)
            .unwrap_or("image/png");
        Ok(DataUrl::from_vec(
            mime,
            contents.expect_file()?.to_vec(contents.hash()).await?,
        ))
    }

    pub async fn dependency_icon(
        &self,
        id: &PackageId,
    ) -> Result<Option<(InternedString, Entry<S>)>, Error> {
        let mut best_icon = None;
        for (path, icon) in self
            .archive
            .contents()
            .get_path(Path::new("dependencies").join(id))
            .and_then(|p| p.as_directory())
            .into_iter()
            .flat_map(|d| {
                d.with_stem("icon").filter(|(p, v)| {
                    Path::new(&*p)
                        .extension()
                        .and_then(|e| e.to_str())
                        .and_then(mime)
                        .map_or(false, |e| e.starts_with("image/") && v.as_file().is_some())
                })
            })
        {
            let size = icon.expect_file()?.size().await?;
            best_icon = match best_icon {
                Some((s, a)) if s >= size => Some((s, a)),
                _ => Some((size, (path, icon))),
            };
        }
        Ok(best_icon.map(|(_, a)| a))
    }

    pub async fn dependency_icon_data_url(
        &self,
        id: &PackageId,
    ) -> Result<Option<DataUrl<'static>>, Error> {
        let Some((name, contents)) = self.dependency_icon(id).await? else {
            return Ok(None);
        };
        let mime = Path::new(&*name)
            .extension()
            .and_then(|e| e.to_str())
            .and_then(mime)
            .unwrap_or("image/png");
        Ok(Some(DataUrl::from_vec(
            mime,
            contents.expect_file()?.to_vec(contents.hash()).await?,
        )))
    }

    pub async fn instructions(&self) -> Result<Option<String>, Error> {
        let Some(entry) = self.archive.contents().get_path("instructions.md") else {
            return Ok(None);
        };
        let bytes = entry.expect_file()?.to_vec(entry.hash()).await?;
        Ok(Some(String::from_utf8(bytes)?))
    }

    /// The package's technical reference, as packed at build time. `None` for a package
    /// built before READMEs were packed, or one whose repository has no README — callers
    /// must handle its absence rather than assume it.
    pub async fn readme(&self) -> Result<Option<String>, Error> {
        let Some(entry) = self.archive.contents().get_path("README.md") else {
            return Ok(None);
        };
        let bytes = entry.expect_file()?.to_vec(entry.hash()).await?;
        Ok(Some(String::from_utf8(bytes)?))
    }

    pub async fn dependency_metadata(
        &self,
        id: &PackageId,
    ) -> Result<Option<DependencyMetadata>, Error> {
        if let Some(entry) = self
            .archive
            .contents()
            .get_path(Path::new("dependencies").join(id).join("metadata.json"))
        {
            Ok(Some(IoFormat::Json.from_slice(
                &entry.expect_file()?.to_vec(entry.hash()).await?,
            )?))
        } else {
            Ok(None)
        }
    }

    pub async fn serialize<W: Sink>(&mut self, w: &mut W, verify: bool) -> Result<(), Error> {
        use tokio::io::AsyncWriteExt;

        w.write_all(MAGIC_AND_VERSION).await?;
        if !self.manifest_dirty {
            self.archive.serialize(w, verify).await?;
        } else {
            let mut dyn_s9pk = self.clone().into_dyn();
            dyn_s9pk.as_archive_mut().contents_mut().insert_path(
                "manifest.json",
                Entry::file(DynFileSource::new(Arc::<[u8]>::from(
                    serde_json::to_vec(&self.manifest).with_kind(ErrorKind::Serialization)?,
                ))),
            )?;
            dyn_s9pk.archive.serialize(w, verify).await?;
        }

        Ok(())
    }

    pub fn into_dyn(self) -> S9pk<DynFileSource> {
        S9pk {
            manifest: self.manifest,
            manifest_dirty: self.manifest_dirty,
            archive: self.archive.into_dyn(),
            size: self.size,
        }
    }
}

impl<S: From<TmpSource<PackSource>> + FileSource + Clone> S9pk<S> {
    pub async fn load_images(&mut self, tmp_dir: Arc<TmpDir>) -> Result<(), Error> {
        let id = &self.manifest.id;
        let version = &self.manifest.version;
        for (image_id, image_config) in &mut self.manifest.images {
            self.manifest_dirty = true;
            for arch in &image_config.arch {
                image_config
                    .source
                    .load(
                        tmp_dir.clone(),
                        id,
                        version,
                        image_id,
                        arch,
                        self.archive.contents_mut(),
                    )
                    .await?;
            }
            image_config.source = ImageSource::Packed;
        }

        Ok(())
    }
}

impl<S: ArchiveSource + Clone> S9pk<Section<S>> {
    #[instrument(skip_all)]
    pub async fn archive(
        source: &S,
        commitment: Option<&MerkleArchiveCommitment>,
    ) -> Result<MerkleArchive<Section<S>>, Error> {
        use tokio::io::AsyncReadExt;

        let mut header = source
            .fetch(
                0,
                MAGIC_AND_VERSION.len() as u64 + MerkleArchive::<Section<S>>::header_size(),
            )
            .await?;

        let mut magic_version = [0u8; MAGIC_AND_VERSION.len()];
        header.read_exact(&mut magic_version).await?;
        ensure_code!(
            &magic_version == MAGIC_AND_VERSION,
            ErrorKind::ParseS9pk,
            "Invalid Magic or Unexpected Version"
        );
        MerkleArchive::deserialize(source, SIG_CONTEXT, &mut header, commitment).await
    }
    #[instrument(skip_all)]
    pub async fn deserialize(
        source: &S,
        commitment: Option<&MerkleArchiveCommitment>,
    ) -> Result<Self, Error> {
        let mut archive = Self::archive(source, commitment).await?;

        archive.sort_by(|a, b| match (priority(a), priority(b)) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });

        Self::new(archive, source.size().await).await
    }
}
impl S9pk {
    pub async fn from_file(file: File) -> Result<Self, Error> {
        Self::deserialize(&MultiCursorFile::from(file), None).await
    }
    pub async fn open(path: impl AsRef<Path>, id: Option<&PackageId>) -> Result<Self, Error> {
        let res = Self::from_file(open_file(path).await?).await?;
        if let Some(id) = id {
            ensure_code!(
                &res.as_manifest().id == id,
                ErrorKind::ValidateS9pk,
                "manifest.id does not match expected"
            );
        }
        Ok(res)
    }
}

async fn extract_manifest<S: FileSource>(archive: &MerkleArchive<S>) -> Result<Manifest, Error> {
    let manifest = serde_json::from_slice(
        &archive
            .contents()
            .get_path("manifest.json")
            .or_not_found("manifest.json")?
            .read_file_to_vec()
            .await?,
    )
    .with_kind(ErrorKind::Deserialization)?;
    Ok(manifest)
}
