use std::collections::BTreeSet;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::Poll;

use bytes::Bytes;
use futures::{Stream, TryStreamExt};
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, RANGE};
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
    // GitHub's release-asset CDN performs poorly with repeated range requests.
    pool: Arc<Mutex<BTreeSet<TrackingIO<HttpBodyReader>>>>,
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
        let size = match head
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|s| s.to_str().ok())
            .and_then(|s| s.parse().ok())
        {
            Some(size) => Some(size),
            None => ranged_size(&client, &url).await?,
        };
        Ok(Self {
            url,
            client,
            size,
            pool: Arc::new(Mutex::new(BTreeSet::new())),
        })
    }
}
/// The total length a one-byte range request reports. None without a `Content-Range` total.
async fn ranged_size(client: &Client, url: &Url) -> Result<Option<u64>, Error> {
    let response = client
        .get(url.clone())
        .header(RANGE, "bytes=0-0")
        .send()
        .await
        .with_kind(ErrorKind::Network)?
        .error_for_status()
        .with_kind(ErrorKind::Network)?;
    Ok(response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.rsplit_once('/'))
        .and_then(|(_, total)| total.parse().ok()))
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
        fn get_reader_for(
            pool: &Arc<Mutex<BTreeSet<TrackingIO<HttpBodyReader>>>>,
            position: u64,
        ) -> Option<TrackingIO<HttpBodyReader>> {
            let mut lock = pool.lock().unwrap();
            let pos = lock.range(..=position).last()?.position();
            lock.take(&pos)
        }
        let reader = get_reader_for(&self.pool, position);
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
            tokio::io::copy(&mut (&mut reader).take(to_skip), &mut tokio::io::sink()).await?;
        }
        Ok(HttpReader {
            pool: self.pool.clone(),
            reader: Some(reader.take(size)),
        })
    }
}
type BoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + Sync + 'a>>;
fn boxed<'a, T>(stream: impl Stream<Item = T> + Send + Sync + 'a) -> BoxStream<'a, T> {
    Box::pin(stream)
}
type HttpBodyReader = StreamReader<BoxStream<'static, Result<Bytes, std::io::Error>>, Bytes>;

#[pin_project::pin_project(PinnedDrop)]
pub struct HttpReader {
    pool: Arc<Mutex<BTreeSet<TrackingIO<HttpBodyReader>>>>,
    #[pin]
    reader: Option<Take<TrackingIO<HttpBodyReader>>>,
}
impl AsyncRead for HttpReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let mut reader = self.project().reader;
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
#[pin_project::pinned_drop]
impl PinnedDrop for HttpReader {
    fn drop(self: Pin<&mut Self>) {
        let mut this = self.project();
        if let Some(reader) = this.reader.take() {
            this.pool.lock().unwrap().insert(reader.into_inner());
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn size_falls_back_to_the_range_total_when_head_omits_it() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url: Url = format!("http://{}/file", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = vec![0; 4096];
                let len = stream.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..len]).to_lowercase();
                let response = if request.starts_with("head") {
                    "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_owned()
                } else {
                    assert!(request.contains("range: bytes=0-0"));
                    "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/1234\r\nContent-Length: 1\r\nConnection: close\r\n\r\nx"
                        .to_owned()
                };
                stream.write_all(response.as_bytes()).await.unwrap();
                stream.shutdown().await.unwrap();
            }
        });
        let source = HttpSource::new(Client::new(), url).await.unwrap();
        assert_eq!(source.size().await, Some(1234));
        server.await.unwrap();
    }
}
