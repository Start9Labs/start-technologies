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
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt};
use tokio::sync::OnceCell;
use tokio::time::{Instant, timeout};
use tokio_util::sync::{CancellationToken, DropGuard};
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
use crate::util::sync::{SyncMutex, SyncRwLock};

mod preferences;
mod seeds;
mod stored_file;
pub use preferences::TorrentPreferences;
pub use seeds::TorrentSeedConfig;
pub(crate) mod discovery;
#[cfg(test)]
mod tests;

const METADATA_LIMIT: u64 = 4 * 1024 * 1024;
pub(crate) const METADATA_TIMEOUT: Duration = Duration::from_secs(30);
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
    /// Overrides local download opt-in or the server's administrative transport cap.
    #[arg(long, help = "help.arg.bittorrent", action = clap::ArgAction::Set)]
    pub bittorrent: Option<bool>,
}

impl TorrentDownloadConfig {
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

pub(crate) struct TorrentAttempt<T = (Arc<TmpDir>, PathBuf)> {
    pub(crate) metadata: SharingMetadata,
    pub(crate) source: Option<T>,
    _cancel: Option<DropGuard>,
}

pub(crate) struct TorrentTransfer {
    workspace: TorrentWorkspace,
    torrent: Arc<librqbit::ManagedTorrent>,
    path: PathBuf,
    length: u64,
    commitment: ContentCommitment,
}

impl TorrentAttempt<Arc<TorrentTransfer>> {
    pub(crate) async fn complete(
        &self,
        progress: &mut PhaseProgressTrackerHandle,
    ) -> TorrentAttempt {
        let source = match &self.source {
            Some(transfer) => match transfer.complete(progress).await {
                Ok(source) => Some(source),
                Err(error) => {
                    tracing::warn!(%error, "BitTorrent payload failed; falling back to HTTP mirrors");
                    None
                }
            },
            None => None,
        };
        TorrentAttempt {
            metadata: self.metadata.clone(),
            source,
            _cancel: None,
        }
    }
}

impl TorrentTransfer {
    fn cancellation_guard(&self) -> DropGuard {
        self.workspace
            .session
            .0
            .cancellation_token()
            .clone()
            .drop_guard()
    }

    async fn complete(
        &self,
        progress: &mut PhaseProgressTrackerHandle,
    ) -> Result<(Arc<TmpDir>, PathBuf), Error> {
        let _cancel = self.cancellation_guard();
        progress.start();
        progress.set_units(Some(ProgressUnits::Bytes));
        progress.set_total(self.length);
        let mut last_progress = 0;
        let mut last_change = Instant::now();
        loop {
            ensure_code!(
                !self.workspace.session.0.cancellation_token().is_cancelled(),
                ErrorKind::Network,
                "{}",
                t!("registry.bittorrent.cancelled")
            );
            let stats = self.torrent.stats();
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
        self.workspace.session.0.stop().await;
        self.commitment.verify(&self.path).await?;
        progress.complete();
        Ok((self.workspace.directory.clone(), self.path.clone()))
    }
}

impl ArchiveSource for TorrentTransfer {
    type FetchReader = Box<dyn AsyncRead + Unpin + Send>;
    type FetchAllReader = Box<dyn AsyncRead + Unpin + Send>;

    async fn size(&self) -> Option<u64> {
        Some(self.length)
    }

    async fn fetch_all(&self) -> Result<Self::FetchAllReader, Error> {
        Ok(Box::new(
            self.torrent
                .clone()
                .stream(0)
                .await
                .map_err(torrent_error)?,
        ))
    }

    async fn fetch(&self, position: u64, size: u64) -> Result<Self::FetchReader, Error> {
        let mut stream = self
            .torrent
            .clone()
            .stream(0)
            .await
            .map_err(torrent_error)?;
        stream.seek(std::io::SeekFrom::Start(position)).await?;
        Ok(Box::new(stream.take(size)))
    }
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
        let token = options.cancellation_token.take().unwrap_or_default();
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
    available: bool,
    downloads: SyncMutex<Option<CancellationToken>>,
    disable_upload: bool,
    seed: Option<TorrentSeedConfig>,
    catalog: OnceCell<Arc<seeds::SeedCatalog>>,
    seeds: SyncRwLock<Option<Arc<seeds::SeedService>>>,
    closed: CancellationToken,
}

impl std::fmt::Debug for TorrentClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TorrentClient")
            .field("available", &self.available)
            .field("disable_upload", &self.disable_upload)
            .field("seed", &self.seed)
            .finish()
    }
}

impl TorrentClient {
    /// Never uploads asset data or retains completed torrents.
    pub fn download_only(config: TorrentDownloadConfig, proxy: bool) -> Self {
        let closed = CancellationToken::new();
        Self {
            available: config
                .bittorrent
                .unwrap_or(TorrentPreferences::default().downloads)
                && !proxy,
            downloads: SyncMutex::new(Some(closed.child_token())),
            disable_upload: true,
            seed: None,
            catalog: OnceCell::new(),
            seeds: SyncRwLock::new(None),
            closed,
        }
    }

    /// Keeps preference controls available unless administratively disabled; peer transfers require opt-in.
    pub fn for_server(config: TorrentDownloadConfig, seed: Option<TorrentSeedConfig>) -> Self {
        let available = config.bittorrent.unwrap_or(true);
        let closed = CancellationToken::new();
        Self {
            available,
            downloads: SyncMutex::new(
                TorrentPreferences::default()
                    .downloads
                    .then(|| closed.child_token()),
            ),
            disable_upload: false,
            seed: available.then(|| seed.unwrap_or_default()),
            catalog: OnceCell::new(),
            seeds: SyncRwLock::new(None),
            closed,
        }
    }

    /// None for `.onion` sources or disabled BitTorrent.
    pub fn for_source(&self, source: &Url) -> Option<&Self> {
        (self.downloads_enabled()
            && !source
                .host_str()
                .is_some_and(|host| host.ends_with(".onion")))
        .then_some(self)
    }

    pub(crate) fn available(&self) -> bool {
        self.available && !self.closed.is_cancelled()
    }

    pub(crate) fn downloads_enabled(&self) -> bool {
        self.available() && self.downloads.peek(Option::is_some)
    }

    pub(crate) fn set_downloads_enabled(&self, enabled: bool) {
        self.downloads.mutate(|current| {
            if enabled && self.available() {
                current.get_or_insert_with(|| self.closed.child_token());
            } else if let Some(token) = current.take() {
                token.cancel();
            }
        });
    }

    fn download_token(&self) -> Option<CancellationToken> {
        self.available()
            .then(|| {
                self.downloads
                    .peek(|current| current.as_ref().map(CancellationToken::child_token))
            })
            .flatten()
    }

    pub(crate) async fn maintain_preferences<F, Fut, G, Incoming>(
        &self,
        mut preferences: patch_db::TypedDbWatch<TorrentPreferences>,
        mut bindings: patch_db::DbWatch,
        configure_incoming: G,
        discover: F,
    ) -> Result<(), Error>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<(), Error>> + Send,
        G: Fn() -> Incoming + Send + Sync,
        Incoming:
            std::future::Future<Output = Result<Option<librqbit::ListenerOptions>, Error>> + Send,
    {
        loop {
            if self.closed.is_cancelled() {
                return Ok(());
            }
            let current: TorrentPreferences =
                from_value(preferences.as_mut().peek_and_mark_seen()?)?;
            self.set_downloads_enabled(current.downloads);
            let should_seed = self.available() && current.seeding;
            let changed = tokio::select! {
                biased;
                _ = self.closed.cancelled() => return Ok(()),
                result = preferences.changed() => result.map_err(Error::from),
                result = async {
                    let maintenance = async {
                        loop {
                            let result = async {
                                bindings.peek_and_mark_seen()?;
                                let listener = configure_incoming().await?;
                                if should_seed {
                                    let service = self.seeds.peek(Clone::clone);
                                    match service {
                                        Some(service) if service.matches_listener(listener.as_ref()) => {
                                            service.refresh().await?;
                                        }
                                        _ => {
                                            self.stop_seeding().await;
                                            self.resume_seeding(listener).await?;
                                        }
                                    }
                                }
                                Ok::<_, Error>(())
                            }.await;
                            if let Err(error) = result {
                                self.stop_seeding().await;
                                tracing::warn!(%error, "Cannot refresh stored-artifact sharing");
                            }
                            let changed = tokio::select! {
                                result = bindings.changed() => result,
                                _ = tokio::time::sleep(Duration::from_secs(30)) => Ok(()),
                            };
                            if let Err(error) = changed {
                                break Err::<(), Error>(error.into());
                            }
                        }
                    };
                    let discovery = async {
                        if should_seed {
                            loop {
                                if let Err(error) = discover().await {
                                    tracing::warn!(%error, "Cannot discover stored software advertisements");
                                }
                                tokio::time::sleep(DISCOVERY_INTERVAL).await;
                            }
                        }
                        std::future::pending::<Result<(), Error>>().await
                    };
                    futures::future::try_join(maintenance, discovery).await?;
                    Ok::<_, Error>(())
                } => result,
            };
            changed?;
            let next: TorrentPreferences = from_value(preferences.as_mut().peek()?)?;
            self.set_downloads_enabled(next.downloads);
            self.stop_seeding().await;
        }
    }

    async fn catalog(&self) -> Result<Option<&Arc<seeds::SeedCatalog>>, Error> {
        let Some(config) = self.seed.as_ref().filter(|_| self.available()) else {
            return Ok(None);
        };
        tokio::select! {
            biased;
            _ = self.closed.cancelled() => Ok(None),
            catalog = self.catalog.get_or_try_init(|| async {
                seeds::SeedCatalog::open(config.directory.clone())
                    .await
                    .map(Arc::new)
            }) => catalog.map(Some),
        }
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

    async fn resume_seeding(
        &self,
        listener: Option<librqbit::ListenerOptions>,
    ) -> Result<(), Error> {
        if let Some(catalog) = self.catalog().await? {
            let service = tokio::select! {
                biased;
                _ = self.closed.cancelled() => return Ok(()),
                service = seeds::SeedService::start(
                    catalog.clone(), listener, self.closed.child_token(),
                ) => Arc::new(service?),
            };
            let published = self.seeds.mutate(|current| {
                if self.closed.is_cancelled() {
                    return false;
                }
                *current = Some(service.clone());
                true
            });
            if published {
                service.refresh().await?;
            }
        }
        Ok(())
    }

    /// Cancels peer work permanently; preference updates cannot reopen this client.
    pub fn shutdown(&self) {
        self.closed.cancel();
        self.set_downloads_enabled(false);
        self.seeds.replace(None);
    }

    async fn stop_seeding(&self) {
        let service = self.seeds.replace(None);
        if let Some(service) = service {
            service.stop().await;
        }
    }

    async fn resolve(
        &self,
        url: &Url,
        commitment: &ContentCommitment,
        client: Client,
        cancellation: Option<CancellationToken>,
    ) -> Result<Option<ResolvedTorrent>, Error> {
        if self.for_source(url).is_none() {
            return Ok(None);
        }
        validate_torrent_url(url)?;
        let mut workspace = None;
        let bytes = match url.scheme() {
            "http" | "https" => metadata(client, url).await?,
            "magnet" => {
                let resolver = self.workspace(cancellation).await?;
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
            .resolve(url, &commitment, client, None)
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
        let Some(pending) = self.begin(url, commitment, client).await? else {
            return Ok(None);
        };
        Ok(Some(pending.complete(progress).await))
    }

    pub(crate) async fn begin(
        &self,
        url: &Url,
        commitment: ContentCommitment,
        client: Client,
    ) -> Result<Option<TorrentAttempt<Arc<TorrentTransfer>>>, Error> {
        let Some(token) = self.download_token() else {
            return Ok(None);
        };
        let Some(resolved) = self
            .resolve(url, &commitment, client, Some(token.clone()))
            .await?
        else {
            return Ok(None);
        };
        let metadata = resolved.sharing_metadata(commitment.clone());
        let source = match self.start_transfer(resolved, commitment, token).await {
            Ok(transfer) => Some(Arc::new(transfer)),
            Err(error) => {
                tracing::warn!(%error, "Cannot start BitTorrent payload; falling back to HTTP mirrors");
                None
            }
        };
        let cancel = source
            .as_ref()
            .map(|transfer| transfer.cancellation_guard());
        Ok(Some(TorrentAttempt {
            metadata,
            source,
            _cancel: cancel,
        }))
    }

    async fn workspace(
        &self,
        cancellation_token: Option<CancellationToken>,
    ) -> Result<TorrentWorkspace, Error> {
        let directory = Arc::new(TmpDir::new().await?);
        let output = directory.join("data");
        tokio::fs::create_dir(&output).await?;
        let session = TorrentSession::new(
            output.clone(),
            SessionOptions {
                disable_upload: self.disable_upload,
                cancellation_token: Some(
                    cancellation_token.unwrap_or_else(|| self.closed.child_token()),
                ),
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

    async fn start_transfer(
        &self,
        mut resolved: ResolvedTorrent,
        commitment: ContentCommitment,
        cancellation: CancellationToken,
    ) -> Result<TorrentTransfer, Error> {
        let workspace = match resolved.workspace.take() {
            Some(workspace) => workspace,
            None => self.workspace(Some(cancellation)).await?,
        };
        let path = workspace.output.join(file_name(&resolved.meta)?);
        let length = resolved.meta.info.data.length.unwrap();
        let torrent = workspace
            .session
            .0
            .add_torrent(AddTorrent::from_bytes(resolved.bytes), None)
            .await
            .map_err(torrent_error)?
            .into_handle()
            .or_not_found("torrent handle")?;
        Ok(TorrentTransfer {
            workspace,
            torrent,
            path,
            length,
            commitment,
        })
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
