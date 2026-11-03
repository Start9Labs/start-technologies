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
use tokio::sync::OnceCell;
use tokio::time::{Instant, timeout};
use url::Url;

use crate::prelude::*;
use crate::progress::{PhaseProgressTrackerHandle, ProgressUnits};
use crate::s9pk::S9pk;
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::sign::commitment::Commitment;
use crate::sign::commitment::blake3::Blake3Commitment;
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::util::io::{TmpDir, open_file};

mod seeds;
pub use seeds::TorrentSeedConfig;
#[cfg(test)]
mod tests;

const METADATA_LIMIT: u64 = 4 * 1024 * 1024;
const METADATA_TIMEOUT: Duration = Duration::from_secs(30);
const STALL_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(250);

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
        match self {
            Self::Blake3(commitment) => {
                commitment
                    .check(&MultiCursorFile::from(open_file(path).await?))
                    .await
            }
            Self::MerkleArchive(commitment) => {
                let s9pk = S9pk::open(path, None).await?;
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

/// Registry downloads and optional retained seeding.
pub struct TorrentClient {
    enabled: bool,
    disable_upload: bool,
    seed: Option<TorrentSeedConfig>,
    seeds: OnceCell<seeds::SeedService>,
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
            seeds: OnceCell::new(),
        }
    }

    /// Allows uploads during downloads; retained seeding requires configured storage.
    pub fn for_server(config: TorrentDownloadConfig, seed: Option<TorrentSeedConfig>) -> Self {
        let enabled = config.enabled();
        Self {
            enabled,
            disable_upload: false,
            seed: seed.filter(|_| enabled),
            seeds: OnceCell::new(),
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

    pub(crate) fn persistent_seeding_enabled(&self) -> bool {
        self.seed.is_some()
    }

    /// Validates retained files before adding them to the seeding session.
    pub async fn resume_seeding(&self) -> Result<(), Error> {
        if let Some(config) = &self.seed {
            let service = self
                .seeds
                .get_or_try_init(|| seeds::SeedService::start(config.clone()))
                .await?;
            service.refresh().await?;
        }
        Ok(())
    }

    /// Stops retained seeding without deleting the cache.
    pub async fn shutdown(&self) {
        if let Some(service) = self.seeds.get() {
            service.stop().await;
        }
    }

    pub(crate) async fn download(
        &self,
        url: &Url,
        commitment: ContentCommitment,
        client: Client,
        progress: &mut PhaseProgressTrackerHandle,
    ) -> Result<Option<(Arc<TmpDir>, PathBuf)>, Error> {
        if self.for_source(url).is_none() {
            return Ok(None);
        }
        validate_torrent_url(url)?;
        let tmp = Arc::new(TmpDir::new().await?);
        let output = tmp.join("data");
        tokio::fs::create_dir(&output).await?;
        let session = TorrentSession::new(
            output.clone(),
            SessionOptions {
                disable_upload: self.disable_upload,
                ..session_options()
            },
        )
        .await?;
        let bytes = match url.scheme() {
            "http" | "https" => metadata(client, url).await?,
            "magnet" => {
                let response = timeout(
                    METADATA_TIMEOUT,
                    session.0.add_torrent(
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
                metadata.torrent_bytes
            }
            _ => return Ok(None),
        };
        let meta = validate_metadata(&bytes, &commitment)?;
        let path = output.join(file_name(&meta)?);
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
        commitment.verify(&path).await?;
        if let Some(config) = &self.seed {
            let retained = seeds::retain(&config.directory, &path, &bytes, &commitment).await;
            if let Err(error) = retained {
                tracing::warn!(%error, "Cannot retain registry asset for seeding");
            } else if let Err(error) = self.resume_seeding().await {
                tracing::warn!(%error, "Cannot start registry asset seeding");
            }
        }
        progress.complete();
        Ok(Some((tmp, path)))
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
