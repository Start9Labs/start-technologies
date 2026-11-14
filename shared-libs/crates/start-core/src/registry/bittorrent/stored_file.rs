use std::fs::{File, Metadata};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Result, ensure};
use librqbit::storage::{BoxStorageFactory, StorageFactory, StorageFactoryExt, TorrentStorage};
use librqbit::{ManagedTorrentShared, TorrentMetadata};
use tokio::io::AsyncRead;

use crate::prelude::t;
use crate::s9pk::merkle_archive::source::{ArchiveSource, checked_end};
use crate::util::io::maybe_open_file;

const VERIFY_CHUNK_SIZE: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    length: u64,
    modified: SystemTime,
}

impl FileIdentity {
    fn read(metadata: Metadata) -> Result<Self> {
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            length: metadata.len(),
            modified: metadata.modified()?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StoredFile {
    path: PathBuf,
    identity: FileIdentity,
}

impl StoredFile {
    pub(super) async fn open(path: &Path) -> Result<Self> {
        let file = maybe_open_file(path)
            .await
            .map_err(|error| anyhow::anyhow!("{error:#}"))?
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        Ok(Self {
            path: path.to_owned(),
            identity: FileIdentity::read(file.metadata().await?)?,
        })
    }

    fn reader(&self) -> Result<File> {
        let file = File::open(&self.path)?;
        ensure!(
            FileIdentity::read(file.metadata()?)? == self.identity,
            "{}",
            t!("registry.bittorrent.stored-artifact-changed")
        );
        Ok(file)
    }
}

impl ArchiveSource for StoredFile {
    type FetchReader = Box<dyn AsyncRead + Unpin + Send>;
    type FetchAllReader = Self::FetchReader;

    async fn size(&self) -> Option<u64> {
        Some(self.identity.length)
    }

    async fn fetch_all(&self) -> Result<Self::FetchAllReader, crate::prelude::Error> {
        self.fetch(0, self.identity.length).await
    }

    async fn fetch(
        &self,
        position: u64,
        size: u64,
    ) -> Result<Self::FetchReader, crate::prelude::Error> {
        checked_end(self, position, size).await?;
        let chunks = futures::stream::try_unfold(
            (self.clone(), position, size),
            |(source, position, remaining)| async move {
                if remaining == 0 {
                    return Ok::<_, std::io::Error>(None);
                }
                let (bytes, source) = tokio::task::spawn_blocking(move || {
                    let mut buffer = vec![0; remaining.min(VERIFY_CHUNK_SIZE) as usize];
                    source
                        .pread_exact(0, position, &mut buffer)
                        .map_err(std::io::Error::other)?;
                    Ok::<_, std::io::Error>((bytes::Bytes::from(buffer), source))
                })
                .await
                .map_err(std::io::Error::other)??;
                let length = bytes.len() as u64;
                Ok(Some((
                    bytes,
                    (source, position + length, remaining - length),
                )))
            },
        );
        Ok(Box::new(tokio_util::io::StreamReader::new(Box::pin(
            chunks,
        ))))
    }
}

impl StorageFactory for StoredFile {
    type Storage = Self;

    fn create(&self, _: &ManagedTorrentShared, _: &TorrentMetadata) -> Result<Self> {
        Ok(self.clone())
    }

    fn clone_box(&self) -> BoxStorageFactory {
        self.clone().boxed()
    }
}

fn read_only() -> anyhow::Error {
    std::io::Error::from(std::io::ErrorKind::PermissionDenied).into()
}

// Callers must run storage callbacks in a blocking context.
impl TorrentStorage for StoredFile {
    fn init(&mut self, _: &ManagedTorrentShared, _: &TorrentMetadata) -> Result<()> {
        Ok(())
    }

    fn pread_exact(&self, _: usize, offset: u64, buf: &mut [u8]) -> Result<()> {
        self.reader()?.read_exact_at(buf, offset)?;
        Ok(())
    }

    fn pwrite_all(&self, _: usize, _: u64, _: &[u8]) -> Result<()> {
        Err(read_only())
    }

    fn remove_file(&self, _: usize, _: &Path) -> Result<()> {
        Err(read_only())
    }

    fn remove_directory_if_empty(&self, _: &Path) -> Result<()> {
        Err(read_only())
    }

    fn ensure_file_length(&self, _: usize, length: u64) -> Result<()> {
        ensure!(
            self.reader()?.metadata()?.len() == length,
            "{}",
            t!("registry.bittorrent.stored-artifact-length-differs")
        );
        Ok(())
    }

    fn take(&self) -> Result<Box<dyn TorrentStorage>> {
        Ok(Box::new(self.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn verification_reader_stops_after_the_owner_deletes_the_file() {
        use tokio::io::AsyncReadExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.rootfs");
        std::fs::write(&path, vec![7; VERIFY_CHUNK_SIZE as usize * 2]).unwrap();
        let source = StoredFile::open(&path).await.unwrap();
        let mut reader = source.fetch_all().await.unwrap();
        let mut first = vec![0; VERIFY_CHUNK_SIZE as usize];
        reader.read_exact(&mut first).await.unwrap();
        assert!(first.iter().all(|byte| *byte == 7));
        std::fs::remove_file(&path).unwrap();
        assert!(reader.read_to_end(&mut Vec::new()).await.is_err());
    }

    #[tokio::test]
    async fn verification_source_preserves_ranges_and_signed_content_checks() {
        use tokio::io::AsyncReadExt;

        use crate::registry::bittorrent::ContentCommitment;
        use crate::sign::commitment::Commitment;
        use crate::sign::commitment::blake3::Blake3Commitment;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.rootfs");
        let bytes: Vec<_> = (0..VERIFY_CHUNK_SIZE as usize * 2 + 13)
            .map(|index| (index % 251) as u8)
            .collect();
        let expected = std::sync::Arc::<[u8]>::from(bytes.clone());
        let commitment =
            ContentCommitment::from(Blake3Commitment::create(&expected).await.unwrap());
        std::fs::write(&path, &bytes).unwrap();
        let source = StoredFile::open(&path).await.unwrap();
        commitment.verify_source(&source).await.unwrap();
        let mut range = source.fetch(VERIFY_CHUNK_SIZE - 5, 19).await.unwrap();
        let mut result = Vec::new();
        range.read_to_end(&mut result).await.unwrap();
        assert_eq!(
            result,
            bytes[VERIFY_CHUNK_SIZE as usize - 5..VERIFY_CHUNK_SIZE as usize + 14]
        );
        assert!(source.fetch(bytes.len() as u64, 1).await.is_err());
    }

    #[tokio::test]
    async fn reads_existing_bytes_and_rejects_every_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("installed.s9pk");
        std::fs::write(&path, b"retained package").unwrap();
        let source = StoredFile::open(&path).await.unwrap();
        let mut bytes = [0; 7];
        source.pread_exact(0, 9, &mut bytes).unwrap();
        assert_eq!(&bytes, b"package");
        assert!(source.pwrite_all(0, 0, b"overwrite").is_err());
        assert!(source.remove_file(0, &path).is_err());
        assert!(source.remove_directory_if_empty(directory.path()).is_err());
        assert!(source.ensure_file_length(0, 3).is_err());
        source.ensure_file_length(0, 16).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"retained package");
    }

    #[tokio::test]
    async fn deletion_and_same_path_replacement_invalidate_the_source() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("installed.s9pk");
        std::fs::write(&path, b"original").unwrap();
        let source = StoredFile::open(&path).await.unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(source.pread_exact(0, 0, &mut [0; 1]).is_err());
        std::fs::write(&path, b"replacement").unwrap();
        assert!(source.pread_exact(0, 0, &mut [0; 1]).is_err());
    }

    #[tokio::test]
    async fn changing_the_existing_inode_invalidates_the_source() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.rootfs");
        std::fs::write(&path, b"original").unwrap();
        let source = StoredFile::open(&path).await.unwrap();
        std::fs::write(&path, b"changed contents").unwrap();
        assert!(source.pread_exact(0, 0, &mut [0; 1]).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"changed contents");
    }
}
