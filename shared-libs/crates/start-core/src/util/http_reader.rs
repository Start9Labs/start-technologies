use std::cmp::min;
use std::convert::TryFrom;
use std::fmt::Display;
use std::future::Future;
use std::io::Error as StdIOError;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use color_eyre::eyre::eyre;
use futures::Stream;
use pin_project::pin_project;
use reqwest::header::{ACCEPT_RANGES, CONTENT_LENGTH, RANGE};
use reqwest::{Client, Url};
use tokio::io::{AsyncRead, AsyncSeek};

use crate::{Error, ResultExt};

#[pin_project]
pub struct HttpReader {
    http_url: Url,
    cursor_pos: usize,
    http_client: Client,
    total_bytes: usize,
    range_unit: Option<RangeUnit>,
    read_in_progress: ReadInProgress,
}

type InProgress = Pin<
    Box<
        dyn Future<
                Output = Result<
                    Pin<
                        Box<
                            dyn Stream<Item = Result<Bytes, reqwest::Error>>
                                + Send
                                + Sync
                                + 'static,
                        >,
                    >,
                    Error,
                >,
            > + Send
            + Sync
            + 'static,
    >,
>;

enum ReadInProgress {
    None,
    InProgress(InProgress),
    Complete(Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send + Sync + 'static>>),
}
impl ReadInProgress {
    fn take(&mut self) -> Self {
        std::mem::replace(self, Self::None)
    }
}

// If we want to add support for units other than Accept-Ranges: bytes, we can use this enum
#[derive(Clone, Copy)]
enum RangeUnit {
    Bytes,
}
impl Default for RangeUnit {
    fn default() -> Self {
        RangeUnit::Bytes
    }
}

impl Display for RangeUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RangeUnit::Bytes => write!(f, "bytes"),
        }
    }
}

impl HttpReader {
    pub async fn new(http_url: Url) -> Result<Self, Error> {
        let http_client = Client::builder()
            // .proxy(reqwest::Proxy::all("socks5h://127.0.0.1:9050").unwrap())
            .build()
            .with_kind(crate::ErrorKind::TLSInit)?;

        // Make a head request so that we can get the file size and check for http range support.
        let head_request = http_client
            .head(http_url.clone())
            .send()
            .await
            .with_kind(crate::ErrorKind::InvalidRequest)?;

        let accept_ranges = head_request.headers().get(ACCEPT_RANGES);

        let range_unit = match accept_ranges {
            Some(range_type) => {
                // as per rfc, header will contain data but not always UTF8 characters.

                let value = range_type
                    .to_str()
                    .map_err(|err| Error::new(err, crate::ErrorKind::Utf8))?;

                match value {
                    "bytes" => Some(RangeUnit::Bytes),
                    _ => {
                        return Err(Error::new(
                            eyre!(
                                "{} HTTP range downloading not supported with this unit {value}",
                                http_url
                            ),
                            crate::ErrorKind::MissingHeader,
                        ));
                    }
                }
            }

            // None can mean just get entire contents, but we currently error out.
            None => {
                return Err(Error::new(
                    eyre!(
                        "{} HTTP range downloading not supported with this url",
                        http_url
                    ),
                    crate::ErrorKind::MissingHeader,
                ));
            }
        };

        let total_bytes_option = head_request.headers().get(CONTENT_LENGTH);

        let total_bytes = match total_bytes_option {
            Some(bytes) => bytes
                .to_str()
                .map_err(|err| Error::new(err, crate::ErrorKind::Utf8))?
                .parse::<usize>()?,
            None => {
                return Err(Error::new(
                    eyre!("No content length headers for {}", http_url),
                    crate::ErrorKind::MissingHeader,
                ));
            }
        };

        Ok(HttpReader {
            http_url,
            cursor_pos: 0,
            http_client,
            total_bytes,
            range_unit,
            read_in_progress: ReadInProgress::None,
        })
    }

    // https://developer.mozilla.org/en-US/docs/Web/HTTP/Range_requests
    async fn get_range(
        range_unit: Option<RangeUnit>,
        http_client: Client,
        http_url: Url,
        start: usize,
        len: usize,
        total_bytes: usize,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send + Sync + 'static>>,
        Error,
    > {
        let end = min(start + len, total_bytes) - 1;

        if start > end {
            return Ok(Box::pin(futures::stream::empty()));
        }

        let data_range = format!("{}={}-{} ", range_unit.unwrap_or_default(), start, end);

        let data_resp = http_client
            .get(http_url)
            .header(RANGE, data_range)
            .send()
            .await
            .with_kind(crate::ErrorKind::Network)?
            .error_for_status()
            .with_kind(crate::ErrorKind::Network)?;

        Ok(Box::pin(data_resp.bytes_stream()))
    }
}

impl AsyncRead for HttpReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        fn poll_complete(
            body: &mut Pin<
                Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send + Sync + 'static>,
            >,
            cx: &mut Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<Option<std::io::Result<usize>>> {
            Poll::Ready(match futures::ready!(body.as_mut().poll_next(cx)) {
                Some(Ok(bytes)) => {
                    if buf.remaining() < bytes.len() {
                        Some(Err(StdIOError::new(
                            std::io::ErrorKind::InvalidInput,
                            format!("more bytes returned than expected"),
                        )))
                    } else {
                        buf.put_slice(&*bytes);
                        Some(Ok(bytes.len()))
                    }
                }
                Some(Err(e)) => Some(Err(StdIOError::new(std::io::ErrorKind::Interrupted, e))),
                None => None,
            })
        }
        let this = self.project();

        loop {
            let mut in_progress = match this.read_in_progress.take() {
                ReadInProgress::Complete(mut body) => match poll_complete(&mut body, cx, buf) {
                    Poll::Pending => {
                        *this.read_in_progress = ReadInProgress::Complete(body);
                        return Poll::Pending;
                    }
                    Poll::Ready(Some(Ok(len))) => {
                        *this.read_in_progress = ReadInProgress::Complete(body);
                        *this.cursor_pos += len;

                        return Poll::Ready(Ok(()));
                    }
                    Poll::Ready(res) => {
                        if let Some(Err(e)) = res {
                            tracing::error!(
                                "Error reading bytes from {}: {}, attempting to resume download",
                                this.http_url,
                                e
                            );
                            tracing::debug!("{:?}", e);
                        }
                        if *this.cursor_pos == *this.total_bytes {
                            return Poll::Ready(Ok(()));
                        }
                        continue;
                    }
                },
                ReadInProgress::None => Box::pin(HttpReader::get_range(
                    *this.range_unit,
                    this.http_client.clone(),
                    this.http_url.clone(),
                    *this.cursor_pos,
                    buf.remaining(),
                    *this.total_bytes,
                )),
                ReadInProgress::InProgress(fut) => fut,
            };

            let res_poll = in_progress.as_mut().poll(cx);

            match res_poll {
                Poll::Ready(result) => match result {
                    Ok(body) => {
                        *this.read_in_progress = ReadInProgress::Complete(body);
                    }
                    Err(err) => {
                        break Poll::Ready(Err(StdIOError::new(
                            std::io::ErrorKind::Interrupted,
                            Box::<dyn std::error::Error + Send + Sync>::from(err.source),
                        )));
                    }
                },
                Poll::Pending => {
                    *this.read_in_progress = ReadInProgress::InProgress(in_progress);

                    break Poll::Pending;
                }
            }
        }
    }
}

impl AsyncSeek for HttpReader {
    fn start_seek(self: Pin<&mut Self>, position: std::io::SeekFrom) -> std::io::Result<()> {
        let this = self.project();

        this.read_in_progress.take(); // invalidate any existing reads

        match position {
            std::io::SeekFrom::Start(offset) => {
                let pos_res = usize::try_from(offset);

                match pos_res {
                    Ok(pos) => {
                        if pos > *this.total_bytes {
                            StdIOError::new(
                                std::io::ErrorKind::InvalidInput,
                                format!(
                                    "The offset: {} cannot be greater than {} bytes",
                                    pos, *this.total_bytes
                                ),
                            );
                        }
                        *this.cursor_pos = pos;
                    }
                    Err(err) => return Err(StdIOError::new(std::io::ErrorKind::InvalidInput, err)),
                }
                Ok(())
            }
            std::io::SeekFrom::Current(offset) => {
                // We explicitly check if we read before byte 0.
                let new_pos = i64::try_from(*this.cursor_pos)
                    .map_err(|err| StdIOError::new(std::io::ErrorKind::InvalidInput, err))?
                    + offset;

                if new_pos < 0 {
                    return Err(StdIOError::new(
                        std::io::ErrorKind::InvalidInput,
                        "Can't read before byte 0",
                    ));
                }

                *this.cursor_pos = new_pos as usize;
                Ok(())
            }

            std::io::SeekFrom::End(offset) => {
                // We explicitly check if we read before byte 0.
                let new_pos = i64::try_from(*this.total_bytes)
                    .map_err(|err| StdIOError::new(std::io::ErrorKind::InvalidInput, err))?
                    + offset;

                if new_pos < 0 {
                    return Err(StdIOError::new(
                        std::io::ErrorKind::InvalidInput,
                        "Can't read before byte 0",
                    ));
                }

                *this.cursor_pos = new_pos as usize;
                Ok(())
            }
        }
    }

    fn poll_complete(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<u64>> {
        Poll::Ready(Ok(self.cursor_pos as u64))
    }
}

#[tokio::test]
async fn main_test() {
    let http_url = Url::parse("https://docs.start9.com/llms.txt").unwrap();

    println!("Getting this resource: {}", http_url);
    let mut test_reader = HttpReader::new(http_url).await.unwrap();

    let mut buf = Vec::new();

    tokio::io::copy(&mut test_reader, &mut buf).await.unwrap();

    assert_eq!(buf.len(), test_reader.total_bytes)
}
