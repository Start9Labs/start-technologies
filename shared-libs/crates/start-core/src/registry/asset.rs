use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use reqwest::header::RANGE;
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWrite;
use ts_rs::TS;
use url::Url;

use crate::prelude::*;
use crate::progress::PhaseProgressTrackerHandle;
use crate::registry::signer::AcceptSigners;
use crate::s9pk::S9pk;
use crate::s9pk::merkle_archive::source::http::HttpSource;
use crate::s9pk::merkle_archive::source::{ArchiveSource, Section};
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::sign::commitment::{Commitment, Digestable};
use crate::sign::{AnySignature, AnyVerifyingKey};
use crate::upload::{DownloadAttemptContext, DownloadHandle, UploadingFile};
use crate::util::future::NonDetachingJoinHandle;
#[cfg(test)]
use crate::util::io::TmpDir;

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RegistryAsset<Commitment> {
    #[ts(type = "string")]
    pub published_at: DateTime<Utc>,
    #[ts(type = "string[]")]
    pub urls: Vec<Url>,
    pub commitment: Commitment,
    pub signatures: HashMap<AnyVerifyingKey, AnySignature>,
}
impl<Commitment> RegistryAsset<Commitment> {
    pub fn all_signers(&self) -> AcceptSigners {
        AcceptSigners::All(
            self.signatures
                .keys()
                .cloned()
                .map(AcceptSigners::Signer)
                .collect(),
        )
    }
    pub async fn load_http_source(&self, client: Client) -> Result<HttpSource, Error> {
        for url in &self.urls {
            if let Ok(source) = HttpSource::new(client.clone(), url.clone()).await {
                return Ok(source);
            }
        }
        Err(Error::new(
            eyre!("{}", t!("registry.asset.failed-to-load-http-url")),
            ErrorKind::Network,
        ))
    }
    pub async fn load_buffered_http_source(
        &self,
        client: Client,
        progress: PhaseProgressTrackerHandle,
    ) -> Result<BufferedHttpSource, Error> {
        BufferedHttpSource::from_urls(&self.urls, client, progress).await
    }
    pub async fn load_buffered_http_source_with_path(
        &self,
        path: impl AsRef<Path>,
        client: Client,
        progress: PhaseProgressTrackerHandle,
    ) -> Result<BufferedHttpSource, Error> {
        BufferedHttpSource::from_urls_with_path(path, &self.urls, client, progress).await
    }
}
impl<Commitment: Digestable> RegistryAsset<Commitment> {
    pub fn validate(&self, context: &str, mut accept: AcceptSigners) -> Result<&Commitment, Error> {
        for (signer, signature) in &self.signatures {
            accept.process_signature(signer, &self.commitment, context, signature)?;
        }
        accept.try_accept()?;
        Ok(&self.commitment)
    }
}
impl<C: for<'a> Commitment<&'a HttpSource>> RegistryAsset<C> {
    pub async fn download(
        &self,
        client: Client,
        dst: &mut (impl AsyncWrite + Unpin + Send + ?Sized),
    ) -> Result<(), Error> {
        self.commitment
            .copy_to(&self.load_http_source(client).await?, dst)
            .await
    }
}
impl RegistryAsset<MerkleArchiveCommitment> {
    pub async fn deserialize_s9pk(
        &self,
        client: Client,
    ) -> Result<S9pk<Section<Arc<HttpSource>>>, Error> {
        S9pk::deserialize(
            &Arc::new(self.load_http_source(client).await?),
            Some(&self.commitment),
        )
        .await
    }
    pub async fn deserialize_s9pk_buffered(
        &self,
        client: Client,
        progress: PhaseProgressTrackerHandle,
    ) -> Result<S9pk<Section<Arc<BufferedHttpSource>>>, Error> {
        S9pk::deserialize(
            &Arc::new(self.load_buffered_http_source(client, progress).await?),
            Some(&self.commitment),
        )
        .await
    }
    pub async fn download_to(
        &self,
        path: impl AsRef<Path>,
        client: Client,
        progress: PhaseProgressTrackerHandle,
    ) -> Result<
        (
            S9pk<Section<Arc<BufferedHttpSource>>>,
            Arc<BufferedHttpSource>,
        ),
        Error,
    > {
        let source = Arc::new(
            self.load_buffered_http_source_with_path(path, client, progress)
                .await?,
        );
        Ok((
            S9pk::deserialize(&source, Some(&self.commitment)).await?,
            source,
        ))
    }
}

pub struct BufferedHttpSource {
    _download: NonDetachingJoinHandle<()>,
    file: UploadingFile,
}

impl BufferedHttpSource {
    pub async fn new(
        client: Client,
        url: Url,
        progress: PhaseProgressTrackerHandle,
    ) -> Result<Self, Error> {
        Self::from_urls(std::slice::from_ref(&url), client, progress).await
    }
    pub async fn from_response(
        response: Response,
        progress: PhaseProgressTrackerHandle,
    ) -> Result<Self, Error> {
        let (mut handle, file) = UploadingFile::new_for_download(progress).await?;
        let mut response = Some(response);
        Ok(Self {
            _download: tokio::spawn(async move {
                handle
                    .download_from(|_| {
                        let next = response.take();
                        async move {
                            next.ok_or_else(|| {
                                Error::new(eyre!("download failed"), ErrorKind::Network)
                            })
                        }
                    })
                    .await
            })
            .into(),
            file,
        })
    }
    pub async fn from_response_with_path(
        path: impl AsRef<Path>,
        response: Response,
        progress: PhaseProgressTrackerHandle,
    ) -> Result<Self, Error> {
        let (mut handle, file) = UploadingFile::with_path_for_download(path, progress).await?;
        let mut response = Some(response);
        Ok(Self {
            _download: tokio::spawn(async move {
                handle
                    .download_from(|_| {
                        let next = response.take();
                        async move {
                            next.ok_or_else(|| {
                                Error::new(eyre!("download failed"), ErrorKind::Network)
                            })
                        }
                    })
                    .await
            })
            .into(),
            file,
        })
    }
    async fn from_urls(
        urls: &[Url],
        client: Client,
        progress: PhaseProgressTrackerHandle,
    ) -> Result<Self, Error> {
        let (handle, file) = UploadingFile::new_for_download(progress).await?;
        Ok(Self::spawn_mirror_download(handle, file, urls, client))
    }
    async fn from_urls_with_path(
        path: impl AsRef<Path>,
        urls: &[Url],
        client: Client,
        progress: PhaseProgressTrackerHandle,
    ) -> Result<Self, Error> {
        let (handle, file) = UploadingFile::with_path_for_download(path, progress).await?;
        Ok(Self::spawn_mirror_download(handle, file, urls, client))
    }
    // Must stay in the background — reads gate on Progress, which also surfaces download errors.
    fn spawn_mirror_download(
        mut handle: DownloadHandle,
        file: UploadingFile,
        urls: &[Url],
        client: Client,
    ) -> Self {
        let next_response = MirrorRetry::new(urls.to_vec(), client).next_response();
        Self {
            _download: tokio::spawn(async move { handle.download_from(next_response).await })
                .into(),
            file,
        }
    }
    pub async fn wait_for_buffered(&self) -> Result<(), Error> {
        self.file.wait_for_complete().await
    }
}
impl ArchiveSource for BufferedHttpSource {
    type FetchReader = <UploadingFile as ArchiveSource>::FetchReader;
    type FetchAllReader = <UploadingFile as ArchiveSource>::FetchAllReader;
    async fn size(&self) -> Option<u64> {
        self.file.size().await
    }
    async fn fetch_all(&self) -> Result<Self::FetchAllReader, Error> {
        self.file.fetch_all().await
    }
    async fn fetch(&self, position: u64, size: u64) -> Result<Self::FetchReader, Error> {
        self.file.fetch(position, size).await
    }
}

struct MirrorRetry {
    urls: Vec<Url>,
    client: Client,
    state: MirrorState,
    errors: Vec<String>,
}

#[derive(Clone, Copy)]
enum MirrorState {
    NextFull(usize),
    TryResume(usize),
    RetryFull(usize),
}

impl MirrorRetry {
    fn new(urls: Vec<Url>, client: Client) -> Self {
        Self {
            urls,
            client,
            state: MirrorState::NextFull(0),
            errors: Vec::new(),
        }
    }
    fn next_response(
        mut self,
    ) -> impl FnMut(
        DownloadAttemptContext,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Response, Error>> + Send>,
    > {
        move |ctx| {
            if let Some(err) = &ctx.last_error {
                let url_idx = match self.state {
                    MirrorState::NextFull(i) => i.saturating_sub(1),
                    MirrorState::TryResume(i) | MirrorState::RetryFull(i) => i,
                };
                if let Some(url) = self.urls.get(url_idx) {
                    self.errors.push(format!("{url}: {err}"));
                    tracing::warn!("Failed to download {url}: {err}");
                }
            }
            let next = self.advance(&ctx);
            let exhausted = (next.is_none()).then(|| self.errors.join("; "));
            Box::pin(async move {
                match next {
                    Some(req) => req
                        .send()
                        .await
                        .map_err(|e| Error::new(e, ErrorKind::Network)),
                    None => Err(Error::new(
                        eyre!(
                            "failed to download package from any mirror: {}",
                            exhausted.unwrap_or_default()
                        ),
                        ErrorKind::Network,
                    )),
                }
            })
        }
    }
    fn advance(&mut self, ctx: &DownloadAttemptContext) -> Option<reqwest::RequestBuilder> {
        loop {
            match self.state {
                MirrorState::NextFull(idx) => {
                    let url = self.urls.get(idx)?;
                    let req = self.client.get(url.clone());
                    self.state = MirrorState::TryResume(idx);
                    return Some(req);
                }
                MirrorState::TryResume(idx) => {
                    let url = self.urls.get(idx)?;
                    self.state = MirrorState::RetryFull(idx);
                    if let (Some(total), written) = (ctx.expected_size, ctx.bytes_written) {
                        if written > 0 && written < total {
                            return Some(
                                self.client
                                    .get(url.clone())
                                    .header(RANGE, format!("bytes={written}-")),
                            );
                        }
                    }
                    continue;
                }
                MirrorState::RetryFull(idx) => {
                    let url = self.urls.get(idx)?;
                    self.state = MirrorState::NextFull(idx + 1);
                    return Some(self.client.get(url.clone()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;
    use crate::progress::FullProgressTracker;
    use crate::s9pk::merkle_archive::source::ArchiveSource;

    struct TestServer {
        url: Url,
        complete_hits: Arc<AtomicUsize>,
        changes_full_hits: Arc<AtomicUsize>,
    }

    async fn spawn_test_server() -> Result<TestServer, Error> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let complete_hits = Arc::new(AtomicUsize::new(0));
        let changes_full_hits = Arc::new(AtomicUsize::new(0));
        let server_complete_hits = complete_hits.clone();
        let server_changes_full_hits = changes_full_hits.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let complete_hits = server_complete_hits.clone();
                let changes_full_hits = server_changes_full_hits.clone();
                tokio::spawn(async move {
                    let mut buf = [0; 1024];
                    let Ok(n) = stream.read(&mut buf).await else {
                        return;
                    };
                    let request = String::from_utf8_lossy(&buf[..n]);
                    let request_lower = request.to_ascii_lowercase();
                    let path = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("/");
                    match path {
                        "/resume" if request_lower.contains("range: bytes=5-") => {
                            let _ = stream
                                .write_all(
                                    b"HTTP/1.1 206 Partial Content\r\nContent-Length: 6\r\nContent-Range: bytes 5-10/11\r\n\r\n world",
                                )
                                .await;
                        }
                        "/resume" => {
                            let _ = stream
                                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nhello")
                                .await;
                        }
                        "/range-ignored" if request_lower.contains("range: bytes=5-") => {
                            let _ = stream
                                .write_all(
                                    b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nhello world",
                                )
                                .await;
                        }
                        "/range-ignored" => {
                            let _ = stream
                                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nhello")
                                .await;
                        }
                        "/changes" if request_lower.contains("range: bytes=5-") => {
                            // The file was replaced mid-download: the resume total
                            // (13) no longer matches the first response's (11).
                            let _ = stream
                                .write_all(
                                    b"HTTP/1.1 206 Partial Content\r\nContent-Length: 8\r\nContent-Range: bytes 5-12/13\r\n\r\nED WORLD",
                                )
                                .await;
                        }
                        "/changes" => {
                            if changes_full_hits.fetch_add(1, Ordering::SeqCst) == 0 {
                                let _ = stream
                                    .write_all(
                                        b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nhello",
                                    )
                                    .await;
                            } else {
                                let _ = stream
                                    .write_all(
                                        b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\nCHANGED WORLD",
                                    )
                                    .await;
                            }
                        }
                        "/always-truncated" => {
                            let _ = stream
                                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nhello")
                                .await;
                        }
                        "/complete" => {
                            complete_hits.fetch_add(1, Ordering::SeqCst);
                            let _ = stream
                                .write_all(
                                    b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nhello world",
                                )
                                .await;
                        }
                        "/slow-tail" => {
                            let _ = stream
                                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nhello")
                                .await;
                            let _ = stream.flush().await;
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            let _ = stream.write_all(b" world").await;
                        }
                        _ => {
                            let _ = stream
                                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                                .await;
                        }
                    }
                });
            }
        });
        Ok(TestServer {
            url: Url::parse(&format!("http://{addr}")).with_kind(ErrorKind::ParseUrl)?,
            complete_hits,
            changes_full_hits,
        })
    }

    async fn buffered_contents(asset: RegistryAsset<()>) -> Result<Vec<u8>, Error> {
        let progress = FullProgressTracker::new().add_phase("Downloading".into(), Some(100));
        let tmp_dir = TmpDir::new().await?;
        let source = asset
            .load_buffered_http_source_with_path(
                tmp_dir.join("package.s9pk"),
                Client::new(),
                progress,
            )
            .await?;
        let mut contents = Vec::new();
        source.fetch_all().await?.read_to_end(&mut contents).await?;
        Ok(contents)
    }

    #[tokio::test]
    async fn buffered_download_resumes_same_url_after_truncated_response() -> Result<(), Error> {
        let server = spawn_test_server().await?;
        let asset = RegistryAsset {
            published_at: Utc::now(),
            urls: vec![server.url.join("resume")?, server.url.join("complete")?],
            commitment: (),
            signatures: HashMap::new(),
        };

        assert_eq!(buffered_contents(asset).await?, b"hello world");
        assert_eq!(server.complete_hits.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[tokio::test]
    async fn buffered_download_retries_same_url_when_range_is_ignored() -> Result<(), Error> {
        let server = spawn_test_server().await?;
        let asset = RegistryAsset {
            published_at: Utc::now(),
            urls: vec![
                server.url.join("range-ignored")?,
                server.url.join("complete")?,
            ],
            commitment: (),
            signatures: HashMap::new(),
        };

        assert_eq!(buffered_contents(asset).await?, b"hello world");
        assert_eq!(server.complete_hits.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[tokio::test]
    async fn buffered_download_falls_back_to_next_mirror_after_retry_fails() -> Result<(), Error> {
        let server = spawn_test_server().await?;
        let asset = RegistryAsset {
            published_at: Utc::now(),
            urls: vec![
                server.url.join("always-truncated")?,
                server.url.join("complete")?,
            ],
            commitment: (),
            signatures: HashMap::new(),
        };

        assert_eq!(buffered_contents(asset).await?, b"hello world");
        assert_eq!(server.complete_hits.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[tokio::test]
    async fn buffered_download_fails_when_mirror_file_changes() -> Result<(), Error> {
        let server = spawn_test_server().await?;
        let asset = RegistryAsset {
            published_at: Utc::now(),
            urls: vec![server.url.join("changes")?, server.url.join("complete")?],
            commitment: (),
            signatures: HashMap::new(),
        };

        // Splicing the two generations would corrupt the archive, so the
        // download fails rather than recover.
        assert!(buffered_contents(asset).await.is_err());
        assert_eq!(server.complete_hits.load(Ordering::SeqCst), 0);
        assert_eq!(server.changes_full_hits.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[tokio::test]
    async fn buffered_download_fails_after_all_mirrors_fail() -> Result<(), Error> {
        let server = spawn_test_server().await?;
        let asset = RegistryAsset {
            published_at: Utc::now(),
            urls: vec![server.url.join("always-truncated")?],
            commitment: (),
            signatures: HashMap::new(),
        };

        assert!(buffered_contents(asset).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn buffered_download_serves_reads_before_download_completes() -> Result<(), Error> {
        let server = spawn_test_server().await?;
        let asset = RegistryAsset {
            published_at: Utc::now(),
            urls: vec![server.url.join("slow-tail")?],
            commitment: (),
            signatures: HashMap::new(),
        };

        let progress = FullProgressTracker::new().add_phase("Downloading".into(), Some(100));
        let tmp_dir = TmpDir::new().await?;
        // Opening the source and reading the head must not wait out the 5s tail.
        let source = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            let source = asset
                .load_buffered_http_source_with_path(
                    tmp_dir.join("package.s9pk"),
                    Client::new(),
                    progress,
                )
                .await?;
            let mut head = Vec::new();
            source.fetch(0, 5).await?.read_to_end(&mut head).await?;
            assert_eq!(head, b"hello");
            Ok::<_, Error>(source)
        })
        .await
        .with_kind(ErrorKind::Timeout)??;

        let mut contents = Vec::new();
        source.fetch_all().await?.read_to_end(&mut contents).await?;
        assert_eq!(contents, b"hello world");
        Ok(())
    }
}
