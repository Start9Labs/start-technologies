use std::fs::{File, Metadata};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Result, ensure};
use librqbit::storage::{BoxStorageFactory, StorageFactory, StorageFactoryExt, TorrentStorage};
use librqbit::{ManagedTorrentShared, TorrentMetadata};

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
    pub(super) fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.to_owned(),
            identity: FileIdentity::read(File::open(path)?.metadata()?)?,
        })
    }

    fn reader(&self) -> Result<File> {
        let file = File::open(&self.path)?;
        ensure!(
            FileIdentity::read(file.metadata()?)? == self.identity,
            "Stored artifact changed"
        );
        Ok(file)
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
            "Stored artifact length differs"
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

    #[test]
    fn reads_existing_bytes_and_rejects_every_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("installed.s9pk");
        std::fs::write(&path, b"retained package").unwrap();
        let source = StoredFile::open(&path).unwrap();
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

    #[test]
    fn deletion_and_same_path_replacement_invalidate_the_source() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("installed.s9pk");
        std::fs::write(&path, b"original").unwrap();
        let source = StoredFile::open(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(source.pread_exact(0, 0, &mut [0; 1]).is_err());
        std::fs::write(&path, b"replacement").unwrap();
        assert!(source.pread_exact(0, 0, &mut [0; 1]).is_err());
    }

    #[test]
    fn changing_the_existing_inode_invalidates_the_source() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.rootfs");
        std::fs::write(&path, b"original").unwrap();
        let source = StoredFile::open(&path).unwrap();
        std::fs::write(&path, b"changed contents").unwrap();
        assert!(source.pread_exact(0, 0, &mut [0; 1]).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"changed contents");
    }
}
