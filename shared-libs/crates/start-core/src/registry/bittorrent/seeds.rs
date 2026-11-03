use std::path::{Path, PathBuf};
use std::sync::Arc;

use librqbit::{AddTorrent, AddTorrentOptions, ListenerOptions};
use serde::{Deserialize, Serialize};

use super::{
    ContentCommitment, TorrentSession, file_name, session_options, torrent_error, validate_metadata,
};
use crate::prelude::*;
use crate::util::FileLock;
use crate::util::io::{TmpDir, copy_file};

/// Persistent seeding storage and an optional incoming peer listener.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TorrentSeedConfig {
    /// Dedicated cache for completed, verified torrents.
    pub directory: PathBuf,
    /// Unset accepts no incoming peer connections.
    pub listen: Option<std::net::SocketAddr>,
}

impl Default for TorrentSeedConfig {
    fn default() -> Self {
        Self {
            directory: Path::new(crate::PACKAGE_DATA).join("registry-seeds"),
            listen: None,
        }
    }
}

pub(super) struct SeedCache {
    directory: PathBuf,
    _lock: FileLock,
}

pub(super) struct SeedService {
    session: TorrentSession,
    cache: Arc<SeedCache>,
}

impl SeedService {
    pub(super) async fn start(
        cache: Arc<SeedCache>,
        listen: Option<std::net::SocketAddr>,
    ) -> Result<Self, Error> {
        let session = TorrentSession::new(
            cache.directory.clone(),
            librqbit::SessionOptions {
                listen: listen.map(|listen_addr| ListenerOptions {
                    listen_addr,
                    ..Default::default()
                }),
                ..session_options()
            },
        )
        .await?;
        Ok(Self { session, cache })
    }

    pub(super) async fn stop(&self) {
        self.session.0.stop().await;
    }

    pub(super) async fn refresh(&self) -> Result<(), Error> {
        let mut entries = tokio::fs::read_dir(&self.cache.directory).await?;
        while let Some(entry) = entries.next_entry().await? {
            let key = entry.file_name().to_string_lossy().into_owned();
            if key.len() != 40
                || !key.bytes().all(|b| b.is_ascii_hexdigit())
                || !entry.file_type().await?.is_dir()
            {
                continue;
            }
            let path = entry.path();
            if let Err(error) = self.resume(&path).await {
                tracing::warn!(%error, "Cannot seed retained registry asset");
            }
        }
        Ok(())
    }

    pub(super) async fn retain(
        &self,
        source: &Path,
        torrent: &[u8],
        commitment: &ContentCommitment,
    ) -> Result<(), Error> {
        let cancellation = self.session.0.cancellation_token();
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => Ok(()),
            result = async {
                let path = self.cache.retain(source, torrent, commitment).await?;
                self.resume(&path).await
            } => result,
        }
    }

    async fn resume(&self, path: &Path) -> Result<(), Error> {
        let commitment: ContentCommitment =
            serde_json::from_slice(&tokio::fs::read(path.join("commitment.json")).await?)
                .with_kind(ErrorKind::Deserialization)?;
        let bytes = tokio::fs::read(path.join("metadata.torrent")).await?;
        let meta = validate_metadata(&bytes, &commitment)?;
        if self.session.0.get(meta.info_hash.into()).is_some() {
            return Ok(());
        }
        let output = path.join("data");
        commitment.verify(&output.join(file_name(&meta)?)).await?;
        self.session
            .0
            .add_torrent(
                AddTorrent::from_bytes(bytes),
                Some(AddTorrentOptions {
                    overwrite: true,
                    output_folder: Some(output.to_string_lossy().into_owned()),
                    ..Default::default()
                }),
            )
            .await
            .map_err(torrent_error)?;
        Ok(())
    }
}

impl SeedCache {
    pub(super) async fn open(directory: PathBuf) -> Result<Self, Error> {
        tokio::fs::create_dir_all(&directory).await?;
        let lock = FileLock::new(directory.join("session.lock"), false).await?;
        Ok(Self {
            directory,
            _lock: lock,
        })
    }

    async fn retain(
        &self,
        source: &Path,
        torrent: &[u8],
        commitment: &ContentCommitment,
    ) -> Result<PathBuf, Error> {
        let meta = validate_metadata(torrent, commitment)?;
        let key = meta.info_hash.as_string();
        let _lock = FileLock::new(self.directory.join(format!("{key}.lock")), true).await?;
        let dest = self.directory.join(&key);
        if dest.exists() {
            return Ok(dest);
        }
        let temp = TmpDir::new_in(&self.directory).await?;
        tokio::fs::create_dir(temp.join("data")).await?;
        copy_file(source, temp.join("data").join(file_name(&meta)?)).await?;
        tokio::fs::write(temp.join("metadata.torrent"), torrent).await?;
        tokio::fs::write(
            temp.join("commitment.json"),
            serde_json::to_vec(commitment).with_kind(ErrorKind::Serialization)?,
        )
        .await?;
        tokio::fs::rename(&*temp, &dest).await?;
        Ok(dest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
    use crate::sign::commitment::Commitment;
    use crate::sign::commitment::blake3::Blake3Commitment;
    use crate::util::io::open_file;

    #[tokio::test]
    async fn verified_cache_resumes_and_corrupt_content_is_not_seeded() -> Result<(), Error> {
        let source = TmpDir::new().await?;
        let cache = TmpDir::new().await?;
        let path = source.join("payload.bin");
        tokio::fs::write(&path, b"hello world").await?;
        let session = TorrentSession::new(
            source.to_path_buf(),
            librqbit::SessionOptions {
                dht: None,
                ..session_options()
            },
        )
        .await?;
        let (torrent, _) = session
            .0
            .create_and_serve_torrent(&path, Default::default())
            .await
            .map_err(torrent_error)?;
        let bytes = torrent.as_bytes().map_err(torrent_error)?;
        let commitment: ContentCommitment =
            Blake3Commitment::create(&MultiCursorFile::from(open_file(&path).await?))
                .await?
                .into();
        let meta = validate_metadata(&bytes, &commitment)?;
        let storage = Arc::new(SeedCache::open(cache.to_path_buf()).await?);
        storage.retain(&path, &bytes, &commitment).await?;
        let mut listen = Some("127.0.0.1:0".parse()?);
        for _ in 0..2 {
            let service = SeedService::start(storage.clone(), listen).await?;
            listen = service.session.0.listen_addr();
            assert!(listen.is_some());
            service.refresh().await?;
            let handle = service.session.0.get(meta.info_hash.into()).unwrap();
            handle
                .wait_until_initialized()
                .await
                .map_err(torrent_error)?;
            assert!(handle.stats().finished);
            service.stop().await;
        }
        let data = cache
            .join(meta.info_hash.as_string())
            .join("data")
            .join(file_name(&meta)?);
        tokio::fs::write(data, b"wrong bytes").await?;
        let service = SeedService::start(storage, listen).await?;
        service.refresh().await?;
        assert!(service.session.0.get(meta.info_hash.into()).is_none());
        service.stop().await;
        Ok(())
    }
}
