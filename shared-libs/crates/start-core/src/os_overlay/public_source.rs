//! Retained public OS assets remain subject to current OS-signing trust on every use.
//! Cache contents are untrusted, including temporary aliases on the OS volume.
//! Callers hold the image-store lease through retention, selection, pruning and use.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use exver::Version;
use tokio::process::Command;

use crate::fde::namespace::confined_path;
use crate::prelude::*;
use crate::registry::asset::RegistryAsset;
use crate::registry::os::SIG_CONTEXT;
use crate::registry::signer::AcceptSigners;
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::sign::commitment::Commitment;
use crate::sign::commitment::blake3::Blake3Commitment;
use crate::util::Invoke;
use crate::util::io::{open_file, read_secret_file, write_file_private_atomic};

const METADATA: [&str; 3] = ["VERSION.txt", "GIT_HASH.txt", "PLATFORM.txt"];
const IMAGE: &str = "image.squashfs";
const RECORD: &str = "asset.json";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub(crate) struct Release {
    pub version: Version,
    pub commit: String,
    pub platform: String,
}

impl Release {
    fn parse([version, commit, platform]: [Vec<u8>; 3]) -> Result<Self, Error> {
        fn text(bytes: Vec<u8>) -> Result<String, Error> {
            Ok(String::from_utf8(bytes)
                .with_kind(ErrorKind::Deserialization)?
                .trim()
                .to_owned())
        }
        Ok(Self {
            version: text(version)?
                .parse::<Version>()
                .with_kind(ErrorKind::Deserialization)?,
            commit: text(commit)?,
            platform: text(platform)?,
        })
    }

    pub(crate) async fn read(root: &Path) -> Result<Self, Error> {
        let mut fields = [Vec::new(), Vec::new(), Vec::new()];
        for (field, name) in fields.iter_mut().zip(METADATA) {
            let path = confined_path(root, &Path::new("usr/lib/startos").join(name)).await?;
            *field = read_secret_file(super::regular_file(&path).await?)
                .await?
                .to_vec();
        }
        Self::parse(fields)
    }

    async fn read_image(path: &Path) -> Result<Self, Error> {
        let mut fields = [Vec::new(), Vec::new(), Vec::new()];
        for (field, name) in fields.iter_mut().zip(METADATA) {
            *field = Command::new("/usr/bin/unsquashfs")
                .env_clear()
                .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
                .env("LC_ALL", "C")
                .arg("-cat")
                .arg(path)
                .arg(Path::new("usr/lib/startos").join(name))
                .invoke(ErrorKind::Filesystem)
                .await?;
        }
        Self::parse(fields)
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Record {
    release: Release,
    asset: RegistryAsset<Blake3Commitment>,
}

impl Record {
    async fn write(&self, directory: &Path) -> Result<(), Error> {
        let path = directory.join(RECORD);
        crate::util::io::delete_file(crate::util::io::to_tmp_path(&path)?).await?;
        write_file_private_atomic(
            &path,
            serde_json::to_vec(self).with_kind(ErrorKind::Serialization)?,
        )
        .await?;
        open_file(directory).await?.sync_all().await?;
        Ok(())
    }
}

pub(crate) struct SourceImage {
    pub path: PathBuf,
    pub release: Release,
}

pub(crate) struct SourceCache {
    directory: PathBuf,
}

impl SourceCache {
    pub(crate) fn system() -> Self {
        Self {
            directory: "/media/startos/images/fde-sources".into(),
        }
    }

    async fn entries(&self) -> Result<Vec<PathBuf>, Error> {
        match tokio::fs::symlink_metadata(&self.directory).await {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Ok(metadata) if metadata.is_dir() => (),
            Ok(_) => return Err(invalid_source()),
            Err(error) => return Err(error.into()),
        }
        let mut entries = tokio::fs::read_dir(&self.directory).await?;
        let mut paths = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() && is_digest(&entry.file_name()) {
                paths.push(entry.path());
            }
        }
        paths.sort();
        Ok(paths)
    }

    async fn record(path: &Path) -> Result<Record, Error> {
        let file = super::regular_file(&path.join(RECORD)).await?;
        serde_json::from_slice(&read_secret_file(file).await?).with_kind(ErrorKind::Deserialization)
    }

    async fn check(path: &Path, record: &Record) -> Result<(), Error> {
        if path.file_name() != Some(std::ffi::OsStr::new(&asset_key(&record.asset))) {
            return Err(invalid_source());
        }
        let image = path.join(IMAGE);
        let file = super::regular_file(&image).await?;
        record
            .asset
            .commitment
            .check(&MultiCursorFile::open(&file).await?)
            .await?;
        if Release::read_image(&image).await? != record.release {
            return Err(invalid_source());
        }
        Ok(())
    }

    /// A matching identity selects a candidate; signatures and complete bytes admit it.
    pub(crate) async fn find_approved(
        &self,
        expected: &Release,
        policy: AcceptSigners,
    ) -> Result<Option<SourceImage>, Error> {
        for path in self.entries().await? {
            let result = async {
                let record = Self::record(&path).await?;
                if &record.release != expected {
                    return Ok(None);
                }
                record
                    .asset
                    .validate_all(SIG_CONTEXT, Some(policy.clone()))?;
                Self::check(&path, &record).await?;
                Ok::<_, Error>(Some(SourceImage {
                    path: path.join(IMAGE),
                    release: record.release,
                }))
            }
            .await;
            match result {
                Ok(Some(image)) => return Ok(Some(image)),
                Ok(None) => (),
                Err(error) => tracing::warn!("Public OS source cache entry rejected: {error}"),
            }
        }
        Ok(None)
    }

    /// Retains signed public bytes, not an approval under the device's signer policy.
    pub(crate) async fn retain(
        &self,
        mut asset: RegistryAsset<Blake3Commitment>,
        image: &Path,
    ) -> Result<SourceImage, Error> {
        asset.validate_all(SIG_CONTEXT, None)?;
        let file = super::regular_file(image).await?;
        asset
            .commitment
            .check(&MultiCursorFile::open(&file).await?)
            .await?;
        let release = Release::read_image(image).await?;
        tokio::fs::create_dir_all(&self.directory).await?;
        if !tokio::fs::symlink_metadata(&self.directory).await?.is_dir() {
            return Err(invalid_source());
        }
        let path = self.directory.join(asset_key(&asset));
        let exists = match tokio::fs::symlink_metadata(&path).await {
            Ok(metadata) if metadata.is_dir() => true,
            Ok(_) => return Err(invalid_source()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        if let Ok(old) = Self::record(&path).await {
            if old.asset.validate_all(SIG_CONTEXT, None).is_ok()
                && Self::check(&path, &old).await.is_ok()
            {
                asset.merge_signatures(&old.asset.commitment, old.asset.signatures);
                let record = Record {
                    release: release.clone(),
                    asset,
                };
                record.write(&path).await?;
                self.sync().await?;
                return Ok(SourceImage {
                    path: path.join(IMAGE),
                    release,
                });
            }
        }
        if exists {
            remove_entry(&path).await?;
        }
        tokio::fs::create_dir(&path).await?;
        tokio::fs::hard_link(image, path.join(IMAGE)).await?;
        file.sync_all().await?;
        let record = Record {
            release: release.clone(),
            asset,
        };
        record.write(&path).await?;
        self.sync().await?;
        Ok(SourceImage {
            path: path.join(IMAGE),
            release,
        })
    }

    async fn sync(&self) -> Result<(), Error> {
        open_file(&self.directory).await?.sync_all().await?;
        open_file(
            self.directory
                .parent()
                .expect("cache directory has a parent"),
        )
        .await?
        .sync_all()
        .await?;
        Ok(())
    }

    pub(crate) async fn prune_boot_sources(&self) -> Result<(), Error> {
        if self.entries().await?.is_empty() {
            return Ok(());
        }
        self.prune(
            &boot_releases(
                Path::new("/media/startos/current"),
                Path::new("/media/startos/config/current.rootfs"),
            )
            .await?,
        )
        .await
    }

    async fn prune(&self, retained: &BTreeSet<Release>) -> Result<(), Error> {
        for path in self.entries().await? {
            match Self::record(&path).await {
                Ok(record) if retained.contains(&record.release) => (),
                _ => remove_entry(&path).await?,
            }
        }
        if self.directory.exists() {
            open_file(&self.directory).await?.sync_all().await?;
        }
        Ok(())
    }
}

pub(crate) fn invalid_source() -> Error {
    Error::new(
        eyre!(t!("os-overlay.public-source.unavailable")),
        ErrorKind::InvalidSignature,
    )
}

fn asset_key(asset: &RegistryAsset<Blake3Commitment>) -> String {
    hex::encode(&asset.commitment.hash[..])
}

fn is_digest(name: &std::ffi::OsStr) -> bool {
    name.to_str().is_some_and(|name| {
        name.len() == 64
            && name
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    })
}

async fn remove_entry(path: &Path) -> Result<(), Error> {
    for file in [
        path.join(IMAGE),
        path.join(RECORD),
        crate::util::io::to_tmp_path(path.join(RECORD))?,
    ] {
        crate::util::io::delete_file(file).await?;
    }
    tokio::fs::remove_dir(path).await?;
    Ok(())
}

/// Running and next-selected images are independent retention roots, including private repacks.
/// Their metadata selects public sources; it never authenticates the repacks for public execution.
async fn boot_releases(current: &Path, next: &Path) -> Result<BTreeSet<Release>, Error> {
    let current = Release::read(current).await?;
    let pending = if tokio::fs::metadata(next).await?.is_dir() {
        Release::read(next).await?
    } else {
        Release::read_image(next).await?
    };
    Ok(BTreeSet::from([current, pending]))
}

#[cfg(test)]
#[path = "public_source_tests.rs"]
mod tests;
