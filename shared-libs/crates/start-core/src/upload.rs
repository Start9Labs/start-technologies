use std::future::Future;
use std::io::SeekFrom;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;

use axum::body::Body;
use axum::extract::Request;
use axum::response::Response;
use bytes::Bytes;
use futures::{Stream, StreamExt, ready};
use http::header::{CONTENT_LENGTH, CONTENT_RANGE};
use http::{HeaderMap, StatusCode};
use imbl_value::InternedString;
use tokio::io::{AsyncRead, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::watch;

use crate::context::RpcContext;
use crate::prelude::*;
use crate::progress::{PhaseProgressTrackerHandle, ProgressUnits};
use crate::rpc_continuations::{Guid, RpcContinuation};
use crate::s9pk::merkle_archive::source::ArchiveSource;
use crate::s9pk::merkle_archive::source::multi_cursor_file::{FileCursor, MultiCursorFile};
use crate::util::direct_io::DirectIoFile;
use crate::util::io::{TmpDir, create_file};

pub async fn upload(
    ctx: &RpcContext,
    session: Option<InternedString>,
    progress: PhaseProgressTrackerHandle,
) -> Result<(Guid, UploadingFile), Error> {
    let guid = Guid::new();
    let (mut handle, file) = UploadingFile::new(progress).await?;
    ctx.rpc_continuations
        .add(
            guid.clone(),
            RpcContinuation::rest_authed(
                ctx,
                session,
                |request| async move {
                    handle.upload(request).await;

                    Response::builder()
                        .status(StatusCode::NO_CONTENT)
                        .body(Body::empty())
                        .with_kind(ErrorKind::Network)
                },
                Duration::from_secs(30),
            ),
        )
        .await;
    Ok((guid, file))
}

struct Progress {
    tracker: PhaseProgressTrackerHandle,
    expected_size: Option<u64>,
    written: u64,
    complete: bool,
    error: Option<Error>,
}
impl Progress {
    fn handle_error(&mut self, e: &std::io::Error) -> bool {
        if self.error.is_none() {
            self.error = Some(Error::new(eyre!("{e}"), ErrorKind::Network));
            true
        } else {
            false
        }
    }
    async fn expected_size(watch: &mut watch::Receiver<Self>) -> Option<u64> {
        watch
            .wait_for(|progress| progress.error.is_some() || progress.expected_size.is_some())
            .await
            .ok()
            .and_then(|a| a.expected_size)
    }
    /// `None` = keep waiting; `Some(res)` = stop waiting with `res`.
    fn check(&self, target: WaitTarget) -> Option<Result<(), Error>> {
        if let Some(e) = &self.error {
            return Some(Err(e.clone_output()));
        }
        match target {
            WaitTarget::Complete => self.complete.then(|| Ok(())),
            WaitTarget::DataAt(position) => {
                (self.complete || self.written > position).then(|| Ok(()))
            }
            WaitTarget::SizeAtLeast(size) => {
                if self.written >= size {
                    Some(Ok(()))
                } else if self.expected_size.map_or(false, |e| e < size) {
                    Some(Err(Error::new(
                        eyre!("file size is less than requested"),
                        ErrorKind::Network,
                    )))
                } else {
                    None
                }
            }
        }
    }
    async fn wait(watch: &mut watch::Receiver<Self>, target: WaitTarget) -> Result<(), Error> {
        watch
            .wait_for(|progress| progress.check(target).is_some())
            .await
            .map_err(|_| {
                Error::new(
                    eyre!("failed to determine upload progress"),
                    ErrorKind::Network,
                )
            })?
            .check(target)
            .expect("wait_for predicate held")
    }
    async fn ready_for(watch: &mut watch::Receiver<Self>, size: u64) -> Result<(), Error> {
        Self::wait(watch, WaitTarget::SizeAtLeast(size)).await
    }
    async fn ready(watch: &mut watch::Receiver<Self>) -> Result<(), Error> {
        Self::wait(watch, WaitTarget::Complete).await
    }
    fn complete(&mut self) -> bool {
        let mut changed = !self.complete;
        self.tracker.complete();
        changed |= match self {
            Self {
                expected_size: Some(size),
                written,
                ..
            } if *written == *size => false,
            Self {
                expected_size: Some(size),
                written,
                error,
                ..
            } if *written > *size && error.is_none() => {
                *error = Some(Error::new(
                    eyre!("Too many bytes received"),
                    ErrorKind::Network,
                ));
                true
            }
            Self {
                error,
                expected_size: Some(_),
                ..
            } if error.is_none() => {
                *error = Some(Error::new(
                    eyre!("Connection closed or timed out before full file received"),
                    ErrorKind::Network,
                ));
                true
            }
            Self {
                expected_size,
                written,
                ..
            } if expected_size.is_none() => {
                *expected_size = Some(*written);
                true
            }
            _ => false,
        };
        self.complete = true;
        changed
    }
}

#[derive(Clone)]
pub struct UploadingFile {
    tmp_dir: Option<Arc<TmpDir>>,
    file: MultiCursorFile,
    progress: watch::Receiver<Progress>,
}
impl UploadingFile {
    pub async fn with_path(
        path: impl AsRef<Path>,
        mut progress: PhaseProgressTrackerHandle,
    ) -> Result<(UploadHandle, Self), Error> {
        progress.set_units(Some(ProgressUnits::Bytes));
        let progress = watch::channel(Progress {
            tracker: progress,
            expected_size: None,
            written: 0,
            error: None,
            complete: false,
        });
        let file = create_file(path).await?;
        let multi_cursor = MultiCursorFile::open(&file).await?;
        let direct_file = DirectIoFile::from_tokio_file(file).await?;
        let uploading = Self {
            tmp_dir: None,
            file: multi_cursor,
            progress: progress.1,
        };
        Ok((
            UploadHandle {
                tmp_dir: None,
                file: direct_file,
                progress: progress.0,
                last_synced: 0,
            },
            uploading,
        ))
    }
    pub async fn new(progress: PhaseProgressTrackerHandle) -> Result<(UploadHandle, Self), Error> {
        let tmp_dir = Arc::new(TmpDir::new().await?);
        let (mut handle, mut file) = Self::with_path(tmp_dir.join("upload.tmp"), progress).await?;
        handle.tmp_dir = Some(tmp_dir.clone());
        file.tmp_dir = Some(tmp_dir);
        Ok((handle, file))
    }
    pub async fn with_path_for_download(
        path: impl AsRef<Path>,
        mut progress: PhaseProgressTrackerHandle,
    ) -> Result<(DownloadHandle, Self), Error> {
        progress.set_units(Some(ProgressUnits::Bytes));
        let progress = watch::channel(Progress {
            tracker: progress,
            expected_size: None,
            written: 0,
            error: None,
            complete: false,
        });
        let file = create_file(path).await?;
        let multi_cursor = MultiCursorFile::open(&file).await?;
        let uploading = Self {
            tmp_dir: None,
            file: multi_cursor,
            progress: progress.1,
        };
        Ok((
            DownloadHandle {
                tmp_dir: None,
                file,
                progress: progress.0,
            },
            uploading,
        ))
    }
    pub async fn new_for_download(
        progress: PhaseProgressTrackerHandle,
    ) -> Result<(DownloadHandle, Self), Error> {
        let tmp_dir = Arc::new(TmpDir::new().await?);
        let (mut handle, mut file) =
            Self::with_path_for_download(tmp_dir.join("download.tmp"), progress).await?;
        handle.tmp_dir = Some(tmp_dir.clone());
        file.tmp_dir = Some(tmp_dir);
        Ok((handle, file))
    }
    pub async fn wait_for_complete(&self) -> Result<(), Error> {
        Progress::ready(&mut self.progress.clone()).await
    }
    pub async fn delete(self) -> Result<(), Error> {
        if let Some(Ok(tmp_dir)) = self.tmp_dir.map(Arc::try_unwrap) {
            tmp_dir.delete().await?;
        }
        Ok(())
    }
}
impl ArchiveSource for UploadingFile {
    type FetchReader = <MultiCursorFile as ArchiveSource>::FetchReader;
    type FetchAllReader = UploadingFileReader;
    async fn size(&self) -> Option<u64> {
        Progress::expected_size(&mut self.progress.clone()).await
    }
    async fn fetch_all(&self) -> Result<Self::FetchAllReader, Error> {
        let mut file = self.file.cursor().await?;
        file.seek(SeekFrom::Start(0)).await?;
        Ok(UploadingFileReader {
            tmp_dir: self.tmp_dir.clone(),
            file,
            position: 0,
            to_seek: None,
            progress: self.progress.clone(),
            wait: None,
        })
    }
    async fn fetch(&self, position: u64, size: u64) -> Result<Self::FetchReader, Error> {
        Progress::ready_for(&mut self.progress.clone(), position + size).await?;
        self.file.fetch(position, size).await
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WaitTarget {
    Complete,
    SizeAtLeast(u64),
    DataAt(u64),
}

#[pin_project::pin_project(project = UploadingFileReaderProjection)]
pub struct UploadingFileReader {
    tmp_dir: Option<Arc<TmpDir>>,
    position: u64,
    to_seek: Option<SeekFrom>,
    #[pin]
    file: FileCursor,
    progress: watch::Receiver<Progress>,
    wait: Option<(
        WaitTarget,
        Pin<Box<dyn Future<Output = Result<(), Error>> + Send + Sync>>,
    )>,
}
impl<'a> UploadingFileReaderProjection<'a> {
    // The wait future is kept across polls — dropping it would deregister its waker.
    fn poll_wait(
        &mut self,
        cx: &mut std::task::Context<'_>,
        target: WaitTarget,
    ) -> Result<bool, std::io::Error> {
        if self.wait.as_ref().map(|(t, _)| *t) != Some(target) {
            // fast path: already satisfied — skip allocating a wait future
            let ready = self.progress.borrow().check(target);
            if let Some(res) = ready {
                *self.wait = None;
                res.map_err(|e| std::io::Error::other(e.source))?;
                return Ok(true);
            }
            let mut progress = self.progress.clone();
            *self.wait = Some((
                target,
                Box::pin(async move { Progress::wait(&mut progress, target).await }),
            ));
        }
        let (_, wait) = self.wait.as_mut().expect("wait future was just created");
        match wait.as_mut().poll(cx) {
            Poll::Ready(res) => {
                *self.wait = None;
                res.map_err(|e| std::io::Error::other(e.source))?;
                Ok(true)
            }
            Poll::Pending => Ok(false),
        }
    }
}
impl AsyncRead for UploadingFileReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let mut this = self.project();

        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        let position = *this.position;
        if this.poll_wait(cx, WaitTarget::DataAt(position))? {
            let start = buf.filled().len();
            ready!(this.file.as_mut().poll_read(cx, buf))?;
            let read = (buf.filled().len() - start) as u64;
            *this.position += read;
            if read == 0 {
                let eof = {
                    let p = this.progress.borrow();
                    p.error.is_none() && p.complete && p.written <= *this.position
                };
                if !eof {
                    // A mirror retry truncated the file out from under us (or its bytes
                    // aren't visible yet) — retry rather than report a false EOF. The
                    // self-wake yields so the writer task can catch up.
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
            }
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}
impl AsyncSeek for UploadingFileReader {
    fn start_seek(self: Pin<&mut Self>, position: SeekFrom) -> std::io::Result<()> {
        let this = self.project();
        *this.to_seek = Some(position);
        Ok(())
    }
    fn poll_complete(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::io::Result<u64>> {
        let mut this = self.project();
        if let Some(to_seek) = *this.to_seek {
            let size = match to_seek {
                SeekFrom::Current(n) => (*this.position as i64 + n) as u64,
                SeekFrom::Start(n) => n,
                SeekFrom::End(n) => {
                    let expected_size = this.progress.borrow().expected_size;
                    match expected_size {
                        Some(end) => (end as i64 + n) as u64,
                        None => {
                            if !this.poll_wait(cx, WaitTarget::Complete)? {
                                return Poll::Pending;
                            }
                            (this.progress.borrow().expected_size.ok_or_else(|| {
                                std::io::Error::new(
                                    std::io::ErrorKind::Other,
                                    eyre!("upload maked complete without expected size"),
                                )
                            })? as i64
                                + n) as u64
                        }
                    }
                }
            };
            if !this.poll_wait(cx, WaitTarget::SizeAtLeast(size))? {
                return Poll::Pending;
            }
        }
        if let Some(seek) = this.to_seek.take() {
            this.file.as_mut().start_seek(seek)?;
        }
        *this.position = ready!(this.file.as_mut().poll_complete(cx)?);
        Poll::Ready(Ok(*this.position))
    }
}

#[pin_project::pin_project(PinnedDrop)]
pub struct UploadHandle {
    tmp_dir: Option<Arc<TmpDir>>,
    #[pin]
    file: DirectIoFile,
    progress: watch::Sender<Progress>,
    last_synced: u64,
}
impl UploadHandle {
    pub async fn upload(&mut self, request: Request) {
        self.process_headers(request.headers());
        self.process_body(request.into_body().into_data_stream())
            .await;
        self.progress.send_if_modified(|p| p.complete());
    }
    fn process_headers(&mut self, headers: &HeaderMap) {
        if let Some(content_length) = headers
            .get(CONTENT_LENGTH)
            .and_then(|a| a.to_str().log_err())
            .and_then(|a| a.parse::<u64>().log_err())
        {
            self.progress.send_modify(|p| {
                p.expected_size = Some(content_length);
                p.tracker.set_total(content_length);
            });
        }
    }
    async fn process_body<E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>>(
        &mut self,
        mut body: impl Stream<Item = Result<Bytes, E>> + Unpin,
    ) {
        while let Some(next) = body.next().await {
            if let Err(e) = async {
                self.write_all(&next.map_err(std::io::Error::other)?).await?;
                Ok(())
            }
            .await
            {
                self.progress.send_if_modified(|p| p.handle_error(&e));
                break;
            }
        }
        if let Err(e) = self.file.sync_all().await {
            self.progress.send_if_modified(|p| p.handle_error(&e));
        }
        self.update_sync_progress();
    }
    fn update_sync_progress(&mut self) {
        let synced = self.file.bytes_synced();
        let delta = synced - self.last_synced;
        if delta > 0 {
            self.last_synced = synced;
            self.progress.send_modify(|p| {
                p.written += delta;
                p.tracker += delta;
            });
        }
    }
}
#[pin_project::pinned_drop]
impl PinnedDrop for UploadHandle {
    fn drop(self: Pin<&mut Self>) {
        let this = self.project();
        this.progress.send_if_modified(|p| p.complete());
    }
}
impl AsyncWrite for UploadHandle {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        let this = self.project();
        // Update progress based on bytes actually flushed to disk
        let synced = this.file.bytes_synced();
        let delta = synced - *this.last_synced;
        if delta > 0 {
            *this.last_synced = synced;
            this.progress.send_modify(|p| {
                p.written += delta;
                p.tracker += delta;
            });
        }
        match this.file.poll_write(cx, buf) {
            Poll::Ready(Err(e)) => {
                this.progress
                    .send_if_modified(|progress| progress.handle_error(&e));
                Poll::Ready(Err(e))
            }
            a => a,
        }
    }
    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let this = self.project();
        match this.file.poll_flush(cx) {
            Poll::Ready(Err(e)) => {
                this.progress
                    .send_if_modified(|progress| progress.handle_error(&e));
                Poll::Ready(Err(e))
            }
            a => a,
        }
    }
    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let this = self.project();
        match this.file.poll_shutdown(cx) {
            Poll::Ready(Err(e)) => {
                this.progress
                    .send_if_modified(|progress| progress.handle_error(&e));
                Poll::Ready(Err(e))
            }
            a => a,
        }
    }
}

pub struct DownloadAttemptContext {
    pub bytes_written: u64,
    pub expected_size: Option<u64>,
    pub last_error: Option<Error>,
}

pub struct DownloadHandle {
    tmp_dir: Option<Arc<TmpDir>>,
    file: tokio::fs::File,
    progress: watch::Sender<Progress>,
}
impl DownloadHandle {
    pub async fn download_from<F, Fut>(&mut self, mut next_response: F)
    where
        F: FnMut(DownloadAttemptContext) -> Fut,
        Fut: Future<Output = Result<reqwest::Response, Error>>,
    {
        let mut last_error: Option<Error> = None;
        let outcome: Result<(), Error> = loop {
            let (bytes_written, expected_size) = {
                let p = self.progress.borrow();
                (p.written, p.expected_size)
            };
            let response = match next_response(DownloadAttemptContext {
                bytes_written,
                expected_size,
                last_error: last_error.as_ref().map(|e| e.clone_output()),
            })
            .await
            {
                Ok(r) => r,
                Err(e) => break Err(e),
            };
            let response = match response.error_for_status() {
                Ok(r) => r,
                Err(e) => {
                    last_error = Some(Error::new(e, ErrorKind::Network));
                    continue;
                }
            };

            if response.status() == StatusCode::PARTIAL_CONTENT {
                if let Some(total) = parse_content_range_total(response.headers()) {
                    self.progress.send_modify(|p| {
                        p.expected_size = Some(total);
                        p.tracker.set_total(total);
                    });
                }
            } else {
                if let Err(e) = self.file.set_len(0).await {
                    break Err(Error::new(e, ErrorKind::Filesystem));
                }
                if let Err(e) = self.file.seek(SeekFrom::Start(0)).await {
                    break Err(Error::new(e, ErrorKind::Filesystem));
                }
                self.progress.send_modify(|p| {
                    p.written = 0;
                    p.tracker.set_done(0);
                });
                if let Some(content_length) = response
                    .headers()
                    .get(CONTENT_LENGTH)
                    .and_then(|a| a.to_str().log_err())
                    .and_then(|a| a.parse::<u64>().log_err())
                {
                    self.progress.send_modify(|p| {
                        p.expected_size = Some(content_length);
                        p.tracker.set_total(content_length);
                    });
                }
            }

            let stream_result: Result<(), Error> = async {
                let mut stream = response.bytes_stream();
                while let Some(next) = stream.next().await {
                    let chunk = next.map_err(|e| Error::new(e, ErrorKind::Network))?;
                    self.file
                        .write_all(&chunk)
                        .await
                        .map_err(|e| Error::new(e, ErrorKind::Filesystem))?;
                    let len = chunk.len() as u64;
                    self.progress.send_modify(|p| {
                        p.written += len;
                        p.tracker += len;
                    });
                }
                Ok(())
            }
            .await;

            if let Err(e) = stream_result {
                last_error = Some(e);
                continue;
            }

            let (written, expected) = {
                let p = self.progress.borrow();
                (p.written, p.expected_size)
            };
            match expected {
                Some(total) if written < total => {
                    last_error = Some(Error::new(
                        eyre!("download ended after {written} bytes, expected {total}"),
                        ErrorKind::Network,
                    ));
                    continue;
                }
                Some(total) if written > total => {
                    break Err(Error::new(
                        eyre!("Too many bytes received"),
                        ErrorKind::Network,
                    ));
                }
                _ => break Ok(()),
            }
        };

        match outcome {
            Ok(()) => {
                if let Err(e) = self.file.sync_all().await {
                    self.progress
                        .send_if_modified(|p| p.handle_error(&e));
                }
            }
            Err(e) => {
                self.progress.send_if_modified(|p| {
                    if p.error.is_none() {
                        p.error = Some(e);
                        true
                    } else {
                        false
                    }
                });
            }
        }
        self.progress.send_if_modified(|p| p.complete());
    }
}
impl Drop for DownloadHandle {
    fn drop(&mut self) {
        self.progress.send_if_modified(|p| p.complete());
    }
}

fn parse_content_range_total(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_RANGE)?
        .to_str()
        .ok()?
        .rsplit_once('/')?
        .1
        .trim()
        .parse()
        .ok()
}
