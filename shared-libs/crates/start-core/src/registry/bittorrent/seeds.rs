use std::path::{Path, PathBuf};

use librqbit::{AddTorrent, AddTorrentOptions, ListenerOptions};
use serde::{Deserialize, Serialize};

use super::{
    ContentCommitment, TorrentSession, file_name, session_options, torrent_error, validate_metadata,
};
use crate::prelude::*;
use crate::util::FileLock;
use crate::util::io::TmpDir;

/// Persistent seeding storage and an optional incoming peer listener.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TorrentSeedConfig {
    /// Dedicated cache for completed, verified torrents.
    pub directory: PathBuf,
    /// Unset accepts no incoming peer connections.
    #[serde(default)]
    pub listen: Option<std::net::SocketAddr>,
}

pub(super) struct SeedService {
    session: TorrentSession,
    directory: PathBuf,
    _lock: FileLock,
}

impl SeedService {
    pub(super) async fn start(config: TorrentSeedConfig) -> Result<Self, Error> {
        tokio::fs::create_dir_all(&config.directory).await?;
        let lock = FileLock::new(config.directory.join("session.lock"), false).await?;
        let session = TorrentSession::new(
            config.directory.clone(),
            librqbit::SessionOptions {
                listen: config.listen.map(|listen_addr| ListenerOptions {
                    listen_addr,
                    ..Default::default()
                }),
                ..session_options()
            },
        )
        .await?;
        Ok(Self {
            session,
            directory: config.directory,
            _lock: lock,
        })
    }

    pub(super) async fn stop(&self) {
        self.session.0.stop().await;
    }

    pub(super) async fn refresh(&self) -> Result<(), Error> {
        let mut entries = tokio::fs::read_dir(&self.directory).await?;
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

pub(super) async fn retain(
    directory: &Path,
    source: &Path,
    torrent: &[u8],
    commitment: &ContentCommitment,
) -> Result<(), Error> {
    let meta = validate_metadata(torrent, commitment)?;
    let key = meta.info_hash.as_string();
    tokio::fs::create_dir_all(directory).await?;
    let _lock = FileLock::new(directory.join(format!("{key}.lock")), true).await?;
    let dest = directory.join(&key);
    if dest.exists() {
        return Ok(());
    }
    let temp = TmpDir::new_in(directory).await?;
    tokio::fs::create_dir(temp.join("data")).await?;
    tokio::fs::copy(source, temp.join("data").join(file_name(&meta)?)).await?;
    tokio::fs::write(temp.join("metadata.torrent"), torrent).await?;
    tokio::fs::write(
        temp.join("commitment.json"),
        serde_json::to_vec(commitment).with_kind(ErrorKind::Serialization)?,
    )
    .await?;
    tokio::fs::rename(&*temp, dest).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
    use crate::sign::commitment::Commitment;
    use crate::sign::commitment::blake3::Blake3Commitment;
    use crate::util::io::open_file;

    async fn stop_and_unlock(service: SeedService) -> Result<(), Error> {
        let SeedService {
            session,
            _lock: lock,
            ..
        } = service;
        session.0.stop().await;
        drop(session);
        lock.unlock().await
    }

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
        retain(&cache, &path, &bytes, &commitment).await?;
        let config = TorrentSeedConfig {
            directory: cache.to_path_buf(),
            listen: None,
        };
        for _ in 0..2 {
            let service = SeedService::start(config.clone()).await?;
            service.refresh().await?;
            let handle = service.session.0.get(meta.info_hash.into()).unwrap();
            handle
                .wait_until_initialized()
                .await
                .map_err(torrent_error)?;
            assert!(handle.stats().finished);
            stop_and_unlock(service).await?;
        }
        let data = cache
            .join(meta.info_hash.as_string())
            .join("data")
            .join(file_name(&meta)?);
        tokio::fs::write(data, b"wrong bytes").await?;
        let service = SeedService::start(config).await?;
        service.refresh().await?;
        assert!(service.session.0.get(meta.info_hash.into()).is_none());
        stop_and_unlock(service).await?;
        Ok(())
    }
}
