use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use librqbit::storage::StorageFactoryExt;
use librqbit::{AddTorrent, AddTorrentOptions, ListenerOptions};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::stored_file::StoredFile;
use super::{SharingMetadata, TorrentSession, session_options, torrent_error, validate_metadata};
use crate::prelude::*;
use crate::util::FileLock;
use crate::util::io::AtomicFile;

const METADATA_EXTENSION: &str = "json";

/// Torrent metadata storage; native interfaces own incoming peer connections.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TorrentSeedConfig {
    /// Stores torrent descriptors, never artifact payloads.
    pub directory: PathBuf,
}

impl Default for TorrentSeedConfig {
    fn default() -> Self {
        Self {
            directory: Path::new(crate::PACKAGE_DATA).join("registry-torrents"),
        }
    }
}

#[derive(Deserialize, Serialize)]
struct StoredTorrent {
    source: PathBuf,
    metadata: SharingMetadata,
}

pub(super) struct SeedCatalog {
    directory: PathBuf,
    _lock: FileLock,
}

impl SeedCatalog {
    pub(super) async fn open(directory: PathBuf) -> Result<Self, Error> {
        tokio::fs::create_dir_all(&directory).await?;
        let lock = FileLock::new(directory.join("session.lock"), false).await?;
        Ok(Self {
            directory,
            _lock: lock,
        })
    }

    pub(super) async fn register(
        &self,
        source: &Path,
        metadata: &SharingMetadata,
    ) -> Result<(), Error> {
        let meta = validate_metadata(&metadata.torrent.0, &metadata.commitment)?;
        let path = self
            .directory
            .join(meta.info_hash.as_string())
            .with_extension(METADATA_EXTENSION);
        let _lock = Self::lock_record(&path).await?;
        let record = StoredTorrent {
            source: tokio::fs::canonicalize(source).await?,
            metadata: metadata.clone(),
        };
        let bytes = serde_json::to_vec(&record).with_kind(ErrorKind::Serialization)?;
        match tokio::fs::read(&path).await {
            Ok(saved) if saved == bytes => return Ok(()),
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        let mut out = AtomicFile::new(&path, None::<&Path>).await?;
        out.write_all(&bytes).await?;
        out.sync_all().await?;
        out.save().await?;
        Ok(())
    }

    async fn lock_record(path: &Path) -> Result<FileLock, Error> {
        FileLock::new(path.with_extension("lock"), true).await
    }
}

#[derive(PartialEq, Eq)]
struct CheckedArtifact {
    source: StoredFile,
    metadata: StoredFile,
}

pub(super) struct SeedService {
    session: TorrentSession,
    catalog: Arc<SeedCatalog>,
    checked: Mutex<HashMap<librqbit::dht::Id20, CheckedArtifact>>,
}

impl SeedService {
    pub(super) async fn start(
        catalog: Arc<SeedCatalog>,
        listen: Option<ListenerOptions>,
        cancellation: CancellationToken,
    ) -> Result<Self, Error> {
        let session = TorrentSession::new(
            catalog.directory.clone(),
            librqbit::SessionOptions {
                listen,
                cancellation_token: Some(cancellation),
                ..session_options()
            },
        )
        .await?;
        Ok(Self {
            session,
            catalog,
            checked: Mutex::new(HashMap::new()),
        })
    }

    pub(super) fn matches_listener(&self, listener: Option<&ListenerOptions>) -> bool {
        match listener {
            Some(listener) => {
                self.session.0.listen_addr() == Some(listener.listen_addr)
                    && self.session.0.announce_port() == listener.announce_port
            }
            None => self.session.0.listen_addr().is_none(),
        }
    }

    pub(super) async fn stop(&self) {
        self.session.0.stop().await;
    }

    pub(super) async fn refresh(&self) -> Result<(), Error> {
        let cancellation = self.session.0.cancellation_token();
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => Ok(()),
            result = self.refresh_live() => result,
        }
    }

    async fn refresh_live(&self) -> Result<(), Error> {
        let mut checked = self.checked.lock().await;
        let mut present = HashSet::new();
        let mut entries = tokio::fs::read_dir(&self.catalog.directory).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry
                .path()
                .extension()
                .is_none_or(|extension| extension != METADATA_EXTENSION)
                || !entry.file_type().await?.is_file()
            {
                continue;
            }
            let record_lock = SeedCatalog::lock_record(&entry.path()).await?;
            let record: StoredTorrent = match async {
                serde_json::from_slice(&tokio::fs::read(entry.path()).await?)
                    .with_kind(ErrorKind::Deserialization)
            }
            .await
            {
                Ok(record) => record,
                Err(error) => {
                    tracing::warn!(%error, "Cannot read stored torrent metadata");
                    continue;
                }
            };
            let meta =
                match validate_metadata(&record.metadata.torrent.0, &record.metadata.commitment) {
                    Ok(meta) => meta,
                    Err(error) => {
                        tracing::warn!(%error, "Invalid stored torrent metadata");
                        continue;
                    }
                };
            let key = meta.info_hash;
            let source = match StoredFile::open(&record.source).await {
                Ok(source) => source,
                Err(error) => {
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                    {
                        tokio::fs::remove_file(entry.path()).await?;
                    } else {
                        tracing::warn!(%error, "Cannot read stored artifact");
                    }
                    continue;
                }
            };
            let candidate = CheckedArtifact {
                source: source.clone(),
                metadata: StoredFile::open(&entry.path())
                    .await
                    .map_err(torrent_error)?,
            };
            drop(record_lock);
            present.insert(key);
            if checked.get(&key) == Some(&candidate) {
                continue;
            }
            if self.session.0.get(meta.info_hash.into()).is_some() {
                self.session
                    .0
                    .delete(meta.info_hash.into(), false)
                    .await
                    .map_err(torrent_error)?;
            }
            checked.insert(key, candidate);
            if let Err(error) = self.resume(&record, source).await {
                tracing::warn!(%error, "Stored artifact cannot join advertised torrent");
            }
        }
        let removed = checked
            .keys()
            .filter(|key| !present.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        for key in removed {
            if self.session.0.get(key.into()).is_some() {
                self.session
                    .0
                    .delete(key.into(), false)
                    .await
                    .map_err(torrent_error)?;
            }
            checked.remove(&key);
        }
        Ok(())
    }

    async fn resume(&self, record: &StoredTorrent, source: StoredFile) -> Result<(), Error> {
        record.metadata.commitment.verify_source(&source).await?;
        let torrent = self
            .session
            .0
            .add_torrent(
                AddTorrent::from_bytes(record.metadata.torrent.0.clone()),
                Some(AddTorrentOptions {
                    paused: true,
                    storage_factory: Some(source.boxed()),
                    ..Default::default()
                }),
            )
            .await
            .map_err(torrent_error)?
            .into_handle()
            .or_not_found("torrent handle")?;
        let initialized = torrent.wait_until_initialized().await;
        if initialized.is_err() || !torrent.stats().finished {
            self.session
                .0
                .delete(torrent.info_hash().into(), false)
                .await
                .map_err(torrent_error)?;
            initialized.map_err(torrent_error)?;
            return Err(Error::new(
                eyre!("{}", t!("registry.bittorrent.stored-artifact-differs")),
                ErrorKind::InvalidRequest,
            ));
        }
        self.session
            .0
            .unpause(&torrent)
            .await
            .map_err(torrent_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::bittorrent::{ContentCommitment, file_name};
    use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
    use crate::sign::commitment::Commitment;
    use crate::sign::commitment::blake3::Blake3Commitment;
    use crate::util::io::{TmpDir, open_file};
    use crate::util::serde::Base64;

    async fn advertisement(path: &Path) -> Result<SharingMetadata, Error> {
        let bytes = librqbit::create_torrent(
            path,
            Default::default(),
            &librqbit::spawn_utils::BlockingSpawner::new(num_cpus::get()),
        )
        .await
        .map_err(torrent_error)?
        .as_bytes()
        .map_err(torrent_error)?;
        Ok(SharingMetadata {
            torrent: Base64(bytes.to_vec()),
            commitment: Blake3Commitment::create(&MultiCursorFile::from(open_file(path).await?))
                .await?
                .into(),
        })
    }

    #[tokio::test]
    async fn cancellation_stops_sharing_while_service_references_survive() -> Result<(), Error> {
        let directory = TmpDir::new().await?;
        let catalog = Arc::new(SeedCatalog::open(directory.to_path_buf()).await?);
        let owner = CancellationToken::new();
        let service = Arc::new(SeedService::start(catalog, None, owner.child_token()).await?);
        let observer = service.clone();
        owner.cancel();
        drop(service);
        assert!(observer.session.0.cancellation_token().is_cancelled());
        observer.refresh().await?;
        Ok(())
    }

    #[tokio::test]
    async fn listener_matching_tracks_the_assigned_announce_port() -> Result<(), Error> {
        let directory = TmpDir::new().await?;
        let catalog = Arc::new(SeedCatalog::open(directory.to_path_buf()).await?);
        let mut listener = ListenerOptions {
            listen_addr: "127.0.0.1:0".parse()?,
            announce_port: Some(45678),
            ..Default::default()
        };
        let incoming = SeedService::start(
            catalog.clone(),
            Some(listener.clone()),
            CancellationToken::new(),
        )
        .await?;
        listener.listen_addr = incoming.session.0.listen_addr().unwrap();
        assert!(incoming.matches_listener(Some(&listener)));
        assert!(!incoming.matches_listener(None));
        listener.announce_port = Some(45679);
        assert!(!incoming.matches_listener(Some(&listener)));
        incoming.stop().await;
        let outgoing = SeedService::start(catalog, None, CancellationToken::new()).await?;
        assert!(outgoing.matches_listener(None));
        assert!(!outgoing.matches_listener(Some(&listener)));
        outgoing.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn stored_artifact_resumes_without_payload_copy_and_follows_deletion() -> Result<(), Error>
    {
        let source = TmpDir::new().await?;
        let catalog_dir = TmpDir::new().await?;
        let path = source.join("installed.s9pk");
        tokio::fs::write(&path, b"already installed").await?;
        let metadata = advertisement(&path).await?;
        let stored = source.join("retained.rootfs");
        tokio::fs::rename(&path, &stored).await?;
        let path = stored;
        let meta = validate_metadata(&metadata.torrent.0, &metadata.commitment)?;
        let catalog = Arc::new(SeedCatalog::open(catalog_dir.to_path_buf()).await?);
        catalog.register(&path, &metadata).await?;
        let mut listen = Some("127.0.0.1:0".parse()?);
        for _ in 0..2 {
            let service = SeedService::start(
                catalog.clone(),
                listen.map(|listen_addr| ListenerOptions {
                    listen_addr,
                    ..Default::default()
                }),
                CancellationToken::new(),
            )
            .await?;
            listen = service.session.0.listen_addr();
            assert!(listen.is_some());
            service.refresh().await?;
            assert!(
                service
                    .session
                    .0
                    .get(meta.info_hash.into())
                    .unwrap()
                    .stats()
                    .finished
            );
            service.stop().await;
        }
        let mut entries = tokio::fs::read_dir(&*catalog_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            assert!(entry.file_type().await?.is_file());
            assert!(
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "json" || extension == "lock")
            );
        }
        let service = SeedService::start(
            catalog,
            listen.map(|listen_addr| ListenerOptions {
                listen_addr,
                ..Default::default()
            }),
            CancellationToken::new(),
        )
        .await?;
        service.refresh().await?;
        let before = service.session.0.get(meta.info_hash.into()).unwrap();
        service.catalog.register(&path, &metadata).await?;
        service.refresh().await?;
        assert!(Arc::ptr_eq(
            &before,
            &service.session.0.get(meta.info_hash.into()).unwrap()
        ));
        let destination = TmpDir::new().await?;
        let receiver = TorrentSession::new(
            destination.to_path_buf(),
            librqbit::SessionOptions {
                dht: None,
                disable_upload: true,
                ..session_options()
            },
        )
        .await?;
        let received = receiver
            .0
            .add_torrent(
                AddTorrent::from_bytes(metadata.torrent.0.clone()),
                Some(AddTorrentOptions {
                    initial_peers: Some(vec![listen.unwrap()]),
                    disable_trackers: true,
                    ..Default::default()
                }),
            )
            .await
            .map_err(torrent_error)?
            .into_handle()
            .unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            received.wait_until_completed(),
        )
        .await
        .unwrap()
        .map_err(torrent_error)?;
        assert_eq!(
            tokio::fs::read(destination.join(file_name(&meta)?)).await?,
            b"already installed"
        );
        receiver.0.stop().await;
        tokio::fs::remove_file(&path).await?;
        service.refresh().await?;
        assert!(service.session.0.get(meta.info_hash.into()).is_none());
        assert!(
            !catalog_dir
                .join(meta.info_hash.as_string())
                .with_extension("json")
                .exists()
        );
        service.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn changed_artifact_is_rechecked_and_never_overwritten() -> Result<(), Error> {
        let source = TmpDir::new().await?;
        let catalog_dir = TmpDir::new().await?;
        let path = source.join("installed.s9pk");
        tokio::fs::write(&path, b"first bytes").await?;
        let metadata = advertisement(&path).await?;
        let meta = validate_metadata(&metadata.torrent.0, &metadata.commitment)?;
        let catalog = Arc::new(SeedCatalog::open(catalog_dir.to_path_buf()).await?);
        catalog.register(&path, &metadata).await?;
        let service = SeedService::start(catalog, None, CancellationToken::new()).await?;
        service.refresh().await?;
        assert!(service.session.0.get(meta.info_hash.into()).is_some());
        tokio::fs::write(&path, b"other bytes").await?;
        service.refresh().await?;
        assert!(service.session.0.get(meta.info_hash.into()).is_none());
        assert_eq!(tokio::fs::read(&path).await?, b"other bytes");
        tokio::fs::write(&path, b"first bytes").await?;
        service.refresh().await?;
        assert!(
            service
                .session
                .0
                .get(meta.info_hash.into())
                .unwrap()
                .stats()
                .finished
        );
        service.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn installed_archive_without_original_trailer_does_not_join_that_swarm()
    -> Result<(), Error> {
        let source = TmpDir::new().await?;
        let catalog_dir = TmpDir::new().await?;
        let original = source.join("original.s9pk");
        tokio::fs::write(
            &original,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/testdata/s9pk-trailer/endorsed.s9pk"
            )),
        )
        .await?;
        let mut metadata = advertisement(&original).await?;
        let mut package = crate::s9pk::S9pk::open(&original, None).await?;
        metadata.commitment = package.as_archive().commitment().await?.into();
        let stored = source.join("installed.s9pk");
        let mut file = crate::util::io::create_file(&stored).await?;
        package.serialize(&mut file, true).await?;
        drop(file);
        let installed_bytes = tokio::fs::read(&stored).await?;
        assert_ne!(installed_bytes, tokio::fs::read(&original).await?);
        metadata.commitment.verify(&stored).await?;
        let meta = validate_metadata(&metadata.torrent.0, &metadata.commitment)?;
        let catalog = Arc::new(SeedCatalog::open(catalog_dir.to_path_buf()).await?);
        catalog.register(&stored, &metadata).await?;
        let service = SeedService::start(catalog, None, CancellationToken::new()).await?;
        service.refresh().await?;
        assert!(service.session.0.get(meta.info_hash.into()).is_none());
        assert_eq!(tokio::fs::read(&stored).await?, installed_bytes);
        service.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn incompatible_torrent_never_repairs_the_stored_file() -> Result<(), Error> {
        let source = TmpDir::new().await?;
        let catalog_dir = TmpDir::new().await?;
        let path = source.join("payload.bin");
        tokio::fs::write(&path, b"first bytes").await?;
        let mut metadata = advertisement(&path).await?;
        tokio::fs::write(&path, b"other bytes").await?;
        metadata.commitment = ContentCommitment::from(
            Blake3Commitment::create(&MultiCursorFile::from(open_file(&path).await?)).await?,
        );
        let meta = validate_metadata(&metadata.torrent.0, &metadata.commitment)?;
        let catalog = Arc::new(SeedCatalog::open(catalog_dir.to_path_buf()).await?);
        catalog.register(&path, &metadata).await?;
        let service = SeedService::start(catalog, None, CancellationToken::new()).await?;
        service.refresh().await?;
        assert!(service.session.0.get(meta.info_hash.into()).is_none());
        assert_eq!(tokio::fs::read(&path).await?, b"other bytes");
        service.stop().await;
        Ok(())
    }
}
