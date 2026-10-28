use std::collections::BTreeSet;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::Poll;

use bytes::Bytes;
use futures::{Stream, TryStreamExt};
use reqwest::header::{CONTENT_LENGTH, RANGE};
use reqwest::{Client, Url};
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf, Take};
use tokio_util::io::StreamReader;

use crate::prelude::*;
use crate::s9pk::merkle_archive::source::ArchiveSource;
use crate::util::Apply;
use crate::util::io::TrackingIO;

pub struct HttpSource {
    url: Url,
    client: Client,
    size: Option<u64>,
    range_support: Result<(), Arc<Mutex<BTreeSet<TrackingIO<HttpBodyReader>>>>>,
}
impl HttpSource {
    pub async fn new(client: Client, url: Url) -> Result<Self, Error> {
        let head = client
            .head(url.clone())
            .send()
            .await
            .with_kind(ErrorKind::Network)?
            .error_for_status()
            .with_kind(ErrorKind::Network)?;
        // GitHub's release-asset CDN performs poorly under range requests.
        let range_support = false;
        let size = head
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|s| s.to_str().ok())
            .and_then(|s| s.parse().ok());
        Ok(Self {
            url,
            client,
            size,
            range_support: if range_support {
                Ok(())
            } else {
                Err(Arc::new(Mutex::new(BTreeSet::new())))
            },
        })
    }
}
impl ArchiveSource for HttpSource {
    type FetchReader = HttpReader;
    type FetchAllReader = StreamReader<BoxStream<'static, Result<Bytes, std::io::Error>>, Bytes>;
    async fn size(&self) -> Option<u64> {
        self.size
    }
    async fn fetch_all(&self) -> Result<Self::FetchAllReader, Error> {
        Ok(StreamReader::new(
            self.client
                .get(self.url.clone())
                .send()
                .await
                .with_kind(ErrorKind::Network)?
                .error_for_status()
                .with_kind(ErrorKind::Network)?
                .bytes_stream()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                .apply(boxed),
        ))
    }
    async fn fetch(&self, position: u64, size: u64) -> Result<Self::FetchReader, Error> {
        match &self.range_support {
            Ok(_) => Ok(HttpReader::Range(
                StreamReader::new(if size > 0 {
                    self.client
                        .get(self.url.clone())
                        .header(RANGE, format!("bytes={}-{}", position, position + size - 1))
                        .send()
                        .await
                        .with_kind(ErrorKind::Network)?
                        .error_for_status()
                        .with_kind(ErrorKind::Network)?
                        .bytes_stream()
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                        .apply(boxed)
                } else {
                    futures::stream::empty().apply(boxed)
                })
                .take(size),
            )),
            Err(pool) => {
                fn get_reader_for(
                    pool: &Arc<Mutex<BTreeSet<TrackingIO<HttpBodyReader>>>>,
                    position: u64,
                ) -> Option<TrackingIO<HttpBodyReader>> {
                    let mut lock = pool.lock().unwrap();
                    let pos = lock.range(..position).last()?.position();
                    lock.take(&pos)
                }
                let reader = get_reader_for(pool, position);
                let mut reader = if let Some(reader) = reader {
                    reader
                } else {
                    TrackingIO::new(
                        0,
                        StreamReader::new(
                            self.client
                                .get(self.url.clone())
                                .send()
                                .await
                                .with_kind(ErrorKind::Network)?
                                .error_for_status()
                                .with_kind(ErrorKind::Network)?
                                .bytes_stream()
                                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                                .apply(boxed),
                        ),
                    )
                };
                if reader.position() < position {
                    let to_skip = position - reader.position();
                    tokio::io::copy(&mut (&mut reader).take(to_skip), &mut tokio::io::sink())
                        .await?;
                }
                Ok(HttpReader::Rangeless {
                    pool: pool.clone(),
                    reader: Some(reader.take(size)),
                })
            }
        }
    }
}

type BoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + Sync + 'a>>;
fn boxed<'a, T>(stream: impl Stream<Item = T> + Send + Sync + 'a) -> BoxStream<'a, T> {
    Box::pin(stream)
}
type HttpBodyReader = StreamReader<BoxStream<'static, Result<Bytes, std::io::Error>>, Bytes>;

#[pin_project::pin_project(project = HttpReaderProj, PinnedDrop)]
pub enum HttpReader {
    Range(#[pin] Take<HttpBodyReader>),
    Rangeless {
        pool: Arc<Mutex<BTreeSet<TrackingIO<HttpBodyReader>>>>,
        #[pin]
        reader: Option<Take<TrackingIO<HttpBodyReader>>>,
    },
}
impl AsyncRead for HttpReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.project() {
            HttpReaderProj::Range(r) => r.poll_read(cx, buf),
            HttpReaderProj::Rangeless { mut reader, .. } => {
                let mut finished = false;
                if let Some(reader) = reader.as_mut().as_pin_mut() {
                    let start = buf.filled().len();
                    futures::ready!(reader.poll_read(cx, buf)?);
                    finished = start == buf.filled().len();
                }
                if finished {
                    reader.take();
                }
                Poll::Ready(Ok(()))
            }
        }
    }
}
#[pin_project::pinned_drop]
impl PinnedDrop for HttpReader {
    fn drop(self: Pin<&mut Self>) {
        match self.project() {
            HttpReaderProj::Range(_) => (),
            HttpReaderProj::Rangeless { pool, mut reader } => {
                if let Some(reader) = reader.take() {
                    pool.lock().unwrap().insert(reader.into_inner());
                }
            }
        }
    }
}

// type RangelessReader = StreamReader<BoxStream<'static, Bytes>, Bytes>;
