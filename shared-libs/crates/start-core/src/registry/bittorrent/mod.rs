use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use futures::TryStreamExt;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, CloneToOwned, DhtSessionConfig, Session,
    SessionOptions, TorrentMetaV1Owned,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::sync::{OnceCell, RwLock};
use tokio::time::{Instant, timeout};
use url::Url;

use crate::prelude::*;
use crate::progress::{PhaseProgressTrackerHandle, ProgressUnits};
use crate::s9pk::S9pk;
use crate::s9pk::merkle_archive::source::ArchiveSource;
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::sign::commitment::Commitment;
use crate::sign::commitment::blake3::Blake3Commitment;
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::util::io::{TmpDir, open_file};

mod seeds;
mod stored_file;
pub use seeds::TorrentSeedConfig;
pub(crate) mod discovery;
#[cfg(test)]
mod tests;

const METADATA_LIMIT: u64 = 4 * 1024 * 1024;
const METADATA_TIMEOUT: Duration = Duration::from_secs(30);
const STALL_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const DISCOVERY_INTERVAL: Duration = Duration::from_secs(300);

fn torrent_error(error: impl std::fmt::Display) -> Error {
    Error::new(eyre!("{error:#}"), ErrorKind::Network)
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "kebab-case")]
pub struct TorrentDownloadConfig {
    /// Enables peer downloads when unset.
    #[arg(long, help = "help.arg.bittorrent", action = clap::ArgAction::Set)]
    pub bittorrent: Option<bool>,
}

impl TorrentDownloadConfig {
    fn enabled(&self) -> bool {
        self.bittorrent.unwrap_or(true)
    }

    /// Keeps explicit settings ahead of lower-priority configuration.
    pub fn merge_with(&mut self, other: Self) {
        self.bittorrent = self.bittorrent.or(other.bittorrent);
    }
}

/// The registry's content commitment, independent of its download transport.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ContentCommitment {
    Blake3(Blake3Commitment),
    MerkleArchive(MerkleArchiveCommitment),
}

impl From<Blake3Commitment> for ContentCommitment {
    fn from(value: Blake3Commitment) -> Self {
        Self::Blake3(value)
    }
}
impl From<MerkleArchiveCommitment> for ContentCommitment {
    fn from(value: MerkleArchiveCommitment) -> Self {
        Self::MerkleArchive(value)
    }
}
impl ContentCommitment {
    async fn verify(&self, path: &Path) -> Result<(), Error> {
        self.verify_source(&MultiCursorFile::from(open_file(path).await?))
            .await
    }

    async fn verify_source<S: ArchiveSource + Clone>(&self, source: &S) -> Result<(), Error> {
        match self {
            Self::Blake3(commitment) => commitment.check(source).await,
            Self::MerkleArchive(commitment) => {
                let s9pk = S9pk::deserialize_local(source, None).await?;
                commitment.copy_to(&s9pk, tokio::io::sink()).await
            }
        }
    }

    fn accepts_size(&self, size: u64) -> bool {
        match self {
            Self::Blake3(commitment) => commitment.size == size,
            Self::MerkleArchive(_) => size > 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct SharingMetadata {
    pub(crate) torrent: crate::util::serde::Base64<Vec<u8>>,
    pub(crate) commitment: ContentCommitment,
}

pub(crate) struct TorrentAttempt {
    pub(crate) metadata: SharingMetadata,
    pub(crate) source: Option<(Arc<TmpDir>, PathBuf)>,
}

struct TorrentWorkspace {
    session: TorrentSession,
    directory: Arc<TmpDir>,
    output: PathBuf,
}

struct ResolvedTorrent {
    workspace: Option<TorrentWorkspace>,
    meta: TorrentMetaV1Owned,
    bytes: bytes::Bytes,
}

impl ResolvedTorrent {
    fn sharing_metadata(&self, commitment: ContentCommitment) -> SharingMetadata {
        SharingMetadata {
            torrent: crate::util::serde::Base64(self.bytes.to_vec()),
            commitment,
        }
    }
}

struct TorrentSession(Arc<Session>);
impl TorrentSession {
    async fn new(directory: PathBuf, mut options: SessionOptions) -> Result<Self, Error> {
        let token = tokio_util::sync::CancellationToken::new();
        let guard = token.clone().drop_guard();
        options.cancellation_token = Some(token);
        let session = Self(
            Session::new_with_opts(directory, options)
                .await
                .map_err(torrent_error)?,
        );
        guard.disarm();
        Ok(session)
    }
}
impl Drop for TorrentSession {
    fn drop(&mut self) {
        self.0.cancellation_token().cancel();
    }
}

fn session_options() -> SessionOptions {
    SessionOptions {
        dht: Some(DhtSessionConfig {
            persistence: None,
            ..Default::default()
        }),
        listen: None,
        disable_local_service_discovery: true,
        ..Default::default()
    }
}

/// Registry downloads and optional sharing of stored artifacts.
pub struct TorrentClient {
    enabled: bool,
    disable_upload: bool,
    seed: Option<TorrentSeedConfig>,
    catalog: OnceCell<Arc<seeds::SeedCatalog>>,
    seeds: RwLock<Option<Arc<seeds::SeedService>>>,
}

impl std::fmt::Debug for TorrentClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TorrentClient")
            .field("enabled", &self.enabled)
            .field("disable_upload", &self.disable_upload)
            .field("seed", &self.seed)
            .finish()
    }
}

impl TorrentClient {
    /// Never uploads asset data or retains completed torrents.
    pub fn download_only(config: TorrentDownloadConfig, proxy: bool) -> Self {
        Self {
            enabled: config.enabled() && !proxy,
            disable_upload: true,
            seed: None,
            catalog: OnceCell::new(),
            seeds: RwLock::new(None),
        }
    }

    /// Allows uploads during downloads; stored-artifact sharing starts disabled.
    pub fn for_server(config: TorrentDownloadConfig, seed: Option<TorrentSeedConfig>) -> Self {
        let enabled = config.enabled();
        Self {
            enabled,
            disable_upload: false,
            seed: enabled.then(|| seed.unwrap_or_default()),
            catalog: OnceCell::new(),
            seeds: RwLock::new(None),
        }
    }

    /// None for `.onion` sources or disabled BitTorrent.
    pub fn for_source(&self, source: &Url) -> Option<&Self> {
        (self.enabled
            && !source
                .host_str()
                .is_some_and(|host| host.ends_with(".onion")))
        .then_some(self)
    }

    pub(crate) fn downloads_enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) async fn maintain_seeding<F, Fut>(
        &self,
        mut enabled: patch_db::TypedDbWatch<bool>,
        discover: F,
    ) -> Result<(), Error>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<(), Error>> + Send,
    {
        loop {
            let should_seed: bool = from_value(enabled.as_mut().peek_and_mark_seen()?)?;
            let changed = tokio::select! {
                biased;
                result = enabled.changed() => result,
                result = async {
                    if should_seed {
                        let maintenance = async {
                            loop {
                                let service = self.seeds.read().await.clone();
                                let result = match service {
                                    Some(service) => service.refresh().await,
                                    None => self.resume_seeding().await,
                                };
                                if let Err(error) = result {
                                    tracing::warn!(%error, "Cannot refresh stored-artifact sharing");
                                }
                                tokio::time::sleep(Duration::from_secs(30)).await;
                            }
                        };
                        let discovery = async {
                            loop {
                                if let Err(error) = discover().await {
                                    tracing::warn!(%error, "Cannot discover stored software advertisements");
                                }
                                tokio::time::sleep(DISCOVERY_INTERVAL).await;
                            }
                        };
                        futures::future::join(maintenance, discovery).await;
                    }
                    std::future::pending::<Result<(), patch_db::Error>>().await
                } => result,
            };
            self.shutdown().await;
            changed?;
        }
    }

    async fn catalog(&self) -> Result<Option<&Arc<seeds::SeedCatalog>>, Error> {
        let Some(config) = &self.seed else {
            return Ok(None);
        };
        self.catalog
            .get_or_try_init(|| async {
                seeds::SeedCatalog::open(config.directory.clone())
                    .await
                    .map(Arc::new)
            })
            .await
            .map(Some)
    }

    pub(crate) async fn register_stored(
        &self,
        source: &Path,
        metadata: &SharingMetadata,
    ) -> Result<(), Error> {
        if let Some(catalog) = self.catalog().await? {
            catalog.register(source, metadata).await?;
        }
        Ok(())
    }

    async fn resume_seeding(&self) -> Result<(), Error> {
        if let (Some(config), Some(catalog)) = (&self.seed, self.catalog().await?) {
            let service =
                Arc::new(seeds::SeedService::start(catalog.clone(), config.listen).await?);
            *self.seeds.write().await = Some(service.clone());
            service.refresh().await?;
        }
        Ok(())
    }

    /// Stops sharing without deleting stored artifacts or their metadata.
    pub async fn shutdown(&self) {
        let service = self.seeds.write().await.take();
        if let Some(service) = service {
            service.stop().await;
        }
    }

    async fn resolve(
        &self,
        url: &Url,
        commitment: &ContentCommitment,
        client: Client,
    ) -> Result<Option<ResolvedTorrent>, Error> {
        if self.for_source(url).is_none() {
            return Ok(None);
        }
        validate_torrent_url(url)?;
        let mut workspace = None;
        let bytes = match url.scheme() {
            "http" | "https" => metadata(client, url).await?,
            "magnet" => {
                let resolver = self.workspace().await?;
                let response = timeout(
                    METADATA_TIMEOUT,
                    resolver.session.0.add_torrent(
                        AddTorrent::from_url(url.as_str()),
                        Some(AddTorrentOptions {
                            list_only: true,
                            ..Default::default()
                        }),
                    ),
                )
                .await
                .with_kind(ErrorKind::Timeout)?
                .map_err(torrent_error)?;
                let AddTorrentResponse::ListOnly(metadata) = response else {
                    return Err(Error::new(
                        eyre!("{}", t!("registry.bittorrent.invalid-metadata")),
                        ErrorKind::InvalidRequest,
                    ));
                };
                workspace = Some(resolver);
                metadata.torrent_bytes
            }
            _ => return Ok(None),
        };
        let meta = validate_metadata(&bytes, commitment)?;
        Ok(Some(ResolvedTorrent {
            workspace,
            meta,
            bytes,
        }))
    }

    pub(crate) async fn metadata_for(
        &self,
        url: &Url,
        commitment: ContentCommitment,
        client: Client,
    ) -> Result<Option<SharingMetadata>, Error> {
        Ok(self
            .resolve(url, &commitment, client)
            .await?
            .map(|resolved| resolved.sharing_metadata(commitment)))
    }

    pub(crate) async fn download(
        &self,
        url: &Url,
        commitment: ContentCommitment,
        client: Client,
        progress: &mut PhaseProgressTrackerHandle,
    ) -> Result<Option<TorrentAttempt>, Error> {
        let Some(resolved) = self.resolve(url, &commitment, client).await? else {
            return Ok(None);
        };
        let metadata = resolved.sharing_metadata(commitment.clone());
        let result = self
            .download_resolved(resolved, &commitment, progress)
            .await;
        let source = match result {
            Ok(source) => Some(source),
            Err(error) => {
                tracing::warn!(%error, "BitTorrent payload failed; falling back to HTTP mirrors");
                None
            }
        };
        Ok(Some(TorrentAttempt { metadata, source }))
    }

    async fn workspace(&self) -> Result<TorrentWorkspace, Error> {
        let directory = Arc::new(TmpDir::new().await?);
        let output = directory.join("data");
        tokio::fs::create_dir(&output).await?;
        let session = TorrentSession::new(
            output.clone(),
            SessionOptions {
                disable_upload: self.disable_upload,
                ..session_options()
            },
        )
        .await?;
        Ok(TorrentWorkspace {
            session,
            directory,
            output,
        })
    }

    async fn download_resolved(
        &self,
        mut resolved: ResolvedTorrent,
        commitment: &ContentCommitment,
        progress: &mut PhaseProgressTrackerHandle,
    ) -> Result<(Arc<TmpDir>, PathBuf), Error> {
        let workspace = match resolved.workspace.take() {
            Some(workspace) => workspace,
            None => self.workspace().await?,
        };
        let path = workspace.output.join(file_name(&resolved.meta)?);
        Self::download_payload(
            &workspace.session,
            &resolved.bytes,
            &path,
            &resolved.meta,
            commitment,
            progress,
        )
        .await?;
        Ok((workspace.directory, path))
    }

    async fn download_payload(
        session: &TorrentSession,
        bytes: &bytes::Bytes,
        path: &Path,
        meta: &TorrentMetaV1Owned,
        commitment: &ContentCommitment,
        progress: &mut PhaseProgressTrackerHandle,
    ) -> Result<(), Error> {
        let torrent = session
            .0
            .add_torrent(AddTorrent::from_bytes(bytes.clone()), None)
            .await
            .map_err(torrent_error)?
            .into_handle()
            .or_not_found("torrent handle")?;
        progress.start();
        progress.set_units(Some(ProgressUnits::Bytes));
        progress.set_total(meta.info.data.length.unwrap());
        let mut last_progress = 0;
        let mut last_change = Instant::now();
        loop {
            let stats = torrent.stats();
            if let Some(error) = stats.error {
                return Err(Error::new(eyre!("{error}"), ErrorKind::Network));
            }
            if stats.progress_bytes > last_progress {
                progress.set_done(stats.progress_bytes);
                last_progress = stats.progress_bytes;
                last_change = Instant::now();
            }
            if stats.finished {
                break;
            }
            ensure_code!(
                last_change.elapsed() < STALL_TIMEOUT,
                ErrorKind::Timeout,
                "{}",
                t!("registry.bittorrent.stalled")
            );
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        session.0.stop().await;
        commitment.verify(path).await?;
        progress.complete();
        Ok(())
    }
}

async fn metadata(client: Client, url: &Url) -> Result<bytes::Bytes, Error> {
    timeout(METADATA_TIMEOUT, async {
        let response = client
            .get(url.clone())
            .send()
            .await
            .with_kind(ErrorKind::Network)?
            .error_for_status()
            .with_kind(ErrorKind::Network)?;
        let stream = response.bytes_stream().map_err(std::io::Error::other);
        let mut reader = tokio_util::io::StreamReader::new(stream).take(METADATA_LIMIT + 1);
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        ensure_code!(
            bytes.len() as u64 <= METADATA_LIMIT,
            ErrorKind::InvalidRequest,
            "{}",
            t!("registry.bittorrent.invalid-metadata")
        );
        Ok(bytes::Bytes::from(bytes))
    })
    .await
    .with_kind(ErrorKind::Timeout)?
}

fn file_name(meta: &TorrentMetaV1Owned) -> Result<&str, Error> {
    let name = meta
        .info
        .data
        .name
        .as_ref()
        .and_then(|name| std::str::from_utf8(name.as_ref()).ok())
        .unwrap_or("");
    let mut components = Path::new(name).components();
    ensure_code!(
        matches!(components.next(), Some(Component::Normal(_)))
            && components.next().is_none()
            && !name.contains(['/', '\\']),
        ErrorKind::InvalidRequest,
        "{}",
        t!("registry.bittorrent.invalid-metadata")
    );
    Ok(name)
}

pub(crate) fn validate_torrent_url(url: &Url) -> Result<(), Error> {
    let valid = match url.scheme() {
        "http" | "https" => url.host_str().is_some(),
        "magnet" => {
            librqbit::Magnet::parse(url.as_str()).is_ok_and(|magnet| magnet.as_id20().is_some())
        }
        _ => false,
    };
    ensure_code!(
        valid,
        ErrorKind::InvalidRequest,
        "{}",
        t!("registry.bittorrent.invalid-metadata")
    );
    Ok(())
}

fn validate_metadata(
    bytes: &[u8],
    commitment: &ContentCommitment,
) -> Result<TorrentMetaV1Owned, Error> {
    ensure_code!(
        bytes.len() as u64 <= METADATA_LIMIT,
        ErrorKind::InvalidRequest,
        "{}",
        t!("registry.bittorrent.invalid-metadata")
    );
    let meta = librqbit::torrent_from_bytes(bytes)
        .map_err(torrent_error)?
        .clone_to_owned(None);
    ensure_code!(
        meta.info.data.files.is_none()
            && meta.info.data.symlink_path.is_none()
            && meta
                .info
                .data
                .length
                .is_some_and(|size| commitment.accepts_size(size)),
        ErrorKind::InvalidRequest,
        "{}",
        t!("registry.bittorrent.invalid-metadata")
    );
    file_name(&meta)?;
    Ok(meta)
}
