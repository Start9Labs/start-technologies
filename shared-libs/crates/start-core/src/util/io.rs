use std::collections::VecDeque;
use std::future::Future;
use std::io::Cursor;
use std::mem::MaybeUninit;
use std::os::unix::prelude::MetadataExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::task::{Poll, Waker};
use std::time::Duration;

use bytes::{Buf, BytesMut};
use clap::builder::ValueParserFactory;
use futures::future::{BoxFuture, Fuse};
use futures::{FutureExt, Stream, TryStreamExt};
use inotify::{EventMask, EventStream, Inotify, WatchMask};
use nix::unistd::{Gid, Uid, fchown};
use serde::{Deserialize, Serialize};
use tokio::fs::{File, OpenOptions};
use tokio::io::{
    AsyncRead, AsyncReadExt, AsyncSeek, AsyncWrite, AsyncWriteExt, DuplexStream, ReadBuf,
    WriteHalf, duplex,
};
use tokio::net::TcpStream;
use tokio::sync::{Notify, OwnedMutexGuard};
use tokio::time::{Instant, Sleep};
use ts_rs::TS;

use crate::prelude::*;
use crate::util::FromStrParser;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::sync::SyncMutex;

pub trait AsyncReadSeek: AsyncRead + AsyncSeek {}
impl<T: AsyncRead + AsyncSeek> AsyncReadSeek for T {}

#[derive(Clone, Debug)]
pub struct AsyncCompat<T>(pub T);
impl<T> futures::io::AsyncRead for AsyncCompat<T>
where
    T: tokio::io::AsyncRead,
{
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let mut read_buf = ReadBuf::new(buf);
        tokio::io::AsyncRead::poll_read(
            unsafe { self.map_unchecked_mut(|a| &mut a.0) },
            cx,
            &mut read_buf,
        )
        .map(|res| res.map(|_| read_buf.filled().len()))
    }
}
impl<T> tokio::io::AsyncRead for AsyncCompat<T>
where
    T: futures::io::AsyncRead,
{
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf,
    ) -> std::task::Poll<std::io::Result<()>> {
        futures::io::AsyncRead::poll_read(
            unsafe { self.map_unchecked_mut(|a| &mut a.0) },
            cx,
            buf.initialize_unfilled(),
        )
        .map(|res| res.map(|len| buf.set_filled(len)))
    }
}
impl<T> futures::io::AsyncWrite for AsyncCompat<T>
where
    T: tokio::io::AsyncWrite,
{
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        tokio::io::AsyncWrite::poll_write(unsafe { self.map_unchecked_mut(|a| &mut a.0) }, cx, buf)
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        tokio::io::AsyncWrite::poll_flush(unsafe { self.map_unchecked_mut(|a| &mut a.0) }, cx)
    }
    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        tokio::io::AsyncWrite::poll_shutdown(unsafe { self.map_unchecked_mut(|a| &mut a.0) }, cx)
    }
}
impl<T> tokio::io::AsyncWrite for AsyncCompat<T>
where
    T: futures::io::AsyncWrite,
{
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        futures::io::AsyncWrite::poll_write(
            unsafe { self.map_unchecked_mut(|a| &mut a.0) },
            cx,
            buf,
        )
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        futures::io::AsyncWrite::poll_flush(unsafe { self.map_unchecked_mut(|a| &mut a.0) }, cx)
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        futures::io::AsyncWrite::poll_close(unsafe { self.map_unchecked_mut(|a| &mut a.0) }, cx)
    }
}

pub async fn from_yaml_async_reader<T, R>(mut reader: R) -> Result<T, crate::Error>
where
    T: for<'de> serde::Deserialize<'de>,
    R: AsyncRead + Unpin,
{
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer).await?;
    serde_yaml::from_slice(&buffer)
        .map_err(color_eyre::eyre::Error::from)
        .with_kind(crate::ErrorKind::Deserialization)
}

pub async fn to_yaml_async_writer<T, W>(mut writer: W, value: &T) -> Result<(), crate::Error>
where
    T: serde::Serialize,
    W: AsyncWrite + Unpin,
{
    let mut buffer = serde_yaml::to_string(value)
        .with_kind(crate::ErrorKind::Serialization)?
        .into_bytes();
    buffer.extend_from_slice(b"\n");
    writer.write_all(&buffer).await?;
    Ok(())
}

pub async fn from_toml_async_reader<T, R>(mut reader: R) -> Result<T, crate::Error>
where
    T: for<'de> serde::Deserialize<'de>,
    R: AsyncRead + Unpin,
{
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer).await?;
    serde_toml::from_str(std::str::from_utf8(&buffer)?)
        .map_err(color_eyre::eyre::Error::from)
        .with_kind(crate::ErrorKind::Deserialization)
}

pub async fn to_toml_async_writer<T, W>(mut writer: W, value: &T) -> Result<(), crate::Error>
where
    T: serde::Serialize,
    W: AsyncWrite + Unpin,
{
    let mut buffer = serde_toml::to_string(value)
        .with_kind(crate::ErrorKind::Serialization)?
        .into_bytes();
    buffer.extend_from_slice(b"\n");
    writer.write_all(&buffer).await?;
    Ok(())
}

pub async fn from_cbor_async_reader<T, R>(mut reader: R) -> Result<T, crate::Error>
where
    T: for<'de> serde::Deserialize<'de>,
    R: AsyncRead + Unpin,
{
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer).await?;
    serde_cbor::de::from_reader(buffer.as_slice())
        .map_err(color_eyre::eyre::Error::from)
        .with_kind(crate::ErrorKind::Deserialization)
}
pub async fn to_cbor_async_writer<T, W>(mut writer: W, value: &T) -> Result<(), crate::Error>
where
    T: serde::Serialize,
    W: AsyncWrite + Unpin,
{
    let mut buffer = Vec::new();
    serde_cbor::ser::into_writer(value, &mut buffer).with_kind(crate::ErrorKind::Serialization)?;
    buffer.extend_from_slice(b"\n");
    writer.write_all(&buffer).await?;
    Ok(())
}

pub async fn from_json_async_reader<T, R>(mut reader: R) -> Result<T, crate::Error>
where
    T: for<'de> serde::Deserialize<'de>,
    R: AsyncRead + Unpin,
{
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer).await?;
    serde_json::from_slice(&buffer)
        .map_err(color_eyre::eyre::Error::from)
        .with_kind(crate::ErrorKind::Deserialization)
}

pub async fn to_json_async_writer<T, W>(mut writer: W, value: &T) -> Result<(), crate::Error>
where
    T: serde::Serialize,
    W: AsyncWrite + Unpin,
{
    let buffer = serde_json::to_string(value).with_kind(crate::ErrorKind::Serialization)?;
    writer.write_all(&buffer.as_bytes()).await?;
    Ok(())
}

pub async fn to_json_pretty_async_writer<T, W>(mut writer: W, value: &T) -> Result<(), crate::Error>
where
    T: serde::Serialize,
    W: AsyncWrite + Unpin,
{
    let mut buffer =
        serde_json::to_string_pretty(value).with_kind(crate::ErrorKind::Serialization)?;
    buffer.push_str("\n");
    writer.write_all(&buffer.as_bytes()).await?;
    Ok(())
}

pub async fn copy_and_shutdown<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    r: &mut R,
    mut w: W,
) -> Result<(), std::io::Error> {
    tokio::io::copy(r, &mut w).await?;
    w.flush().await?;
    w.shutdown().await?;
    Ok(())
}

pub fn dir_size<'a, P: AsRef<Path> + 'a + Send + Sync>(
    path: P,
    ctr: Option<&'a Counter>,
) -> BoxFuture<'a, Result<u64, std::io::Error>> {
    async move {
        tokio_stream::wrappers::ReadDirStream::new(tokio::fs::read_dir(path.as_ref()).await?)
            .try_fold(0, |acc, e| async move {
                let m = e.metadata().await?;
                Ok(acc
                    + if m.is_file() {
                        if let Some(ctr) = ctr {
                            ctr.add(m.len());
                        }
                        m.len()
                    } else if m.is_dir() {
                        dir_size(e.path(), ctr).await?
                    } else {
                        0
                    })
            })
            .await
    }
    .boxed()
}

pub fn response_to_reader(response: reqwest::Response) -> impl AsyncRead + Unpin {
    tokio_util::io::StreamReader::new(response.bytes_stream().map_err(|e| {
        std::io::Error::new(
            if e.is_connect() {
                std::io::ErrorKind::ConnectionRefused
            } else if e.is_timeout() {
                std::io::ErrorKind::TimedOut
            } else {
                std::io::ErrorKind::Other
            },
            e,
        )
    }))
}

#[pin_project::pin_project]
pub struct IOHook<'a, T> {
    #[pin]
    pub io: T,
    pre_write: Option<Box<dyn FnMut(&[u8]) -> Result<(), std::io::Error> + Send + 'a>>,
    post_write: Option<Box<dyn FnMut(&[u8]) + Send + 'a>>,
    post_read: Option<Box<dyn FnMut(&[u8]) + Send + 'a>>,
}
impl<'a, T> IOHook<'a, T> {
    pub fn new(io: T) -> Self {
        Self {
            io,
            pre_write: None,
            post_write: None,
            post_read: None,
        }
    }
    pub fn into_inner(self) -> T {
        self.io
    }
    pub fn pre_write<F: FnMut(&[u8]) -> Result<(), std::io::Error> + Send + 'a>(&mut self, f: F) {
        self.pre_write = Some(Box::new(f))
    }
    pub fn post_write<F: FnMut(&[u8]) + Send + 'a>(&mut self, f: F) {
        self.post_write = Some(Box::new(f))
    }
    pub fn post_read<F: FnMut(&[u8]) + Send + 'a>(&mut self, f: F) {
        self.post_read = Some(Box::new(f))
    }
}
impl<'a, T: AsyncWrite> AsyncWrite for IOHook<'a, T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        let this = self.project();
        if let Some(pre_write) = this.pre_write {
            pre_write(buf)?;
        }
        let written = futures::ready!(this.io.poll_write(cx, buf)?);
        if let Some(post_write) = this.post_write {
            post_write(&buf[..written]);
        }
        Poll::Ready(Ok(written))
    }
    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        self.project().io.poll_flush(cx)
    }
    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        self.project().io.poll_shutdown(cx)
    }
}
impl<'a, T: AsyncRead> AsyncRead for IOHook<'a, T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.project();
        let start = buf.filled().len();
        futures::ready!(this.io.poll_read(cx, buf)?);
        if let Some(post_read) = this.post_read {
            post_read(&buf.filled()[start..]);
        }
        Poll::Ready(Ok(()))
    }
}

#[pin_project::pin_project]
pub struct BufferedWriteReader {
    #[pin]
    hdl: Fuse<NonDetachingJoinHandle<Result<(), std::io::Error>>>,
    #[pin]
    rdr: DuplexStream,
}
impl BufferedWriteReader {
    pub fn new<
        W: FnOnce(WriteHalf<DuplexStream>) -> Fut,
        Fut: Future<Output = Result<(), std::io::Error>> + Send + Sync + 'static,
    >(
        write_fn: W,
        max_buf_size: usize,
    ) -> Self {
        let (w, rdr) = duplex(max_buf_size);
        let (_, w) = tokio::io::split(w);
        BufferedWriteReader {
            hdl: NonDetachingJoinHandle::from(tokio::spawn(write_fn(w))).fuse(),
            rdr,
        }
    }
}
impl AsyncRead for BufferedWriteReader {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.project();
        let res = this.rdr.poll_read(cx, buf);
        match this.hdl.poll(cx) {
            Poll::Ready(Ok(Err(e))) => return Poll::Ready(Err(e)),
            Poll::Ready(Err(e)) => {
                return Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, e)));
            }
            _ => res,
        }
    }
}

pub trait CursorExt {
    fn pure_read(&mut self, buf: &mut ReadBuf<'_>);
    fn remaining_slice(&self) -> &[u8];
}

impl<T: AsRef<[u8]>> CursorExt for Cursor<T> {
    fn pure_read(&mut self, buf: &mut ReadBuf<'_>) {
        let end = self.position() as usize
            + std::cmp::min(
                buf.remaining(),
                self.get_ref().as_ref().len() - self.position() as usize,
            );
        buf.put_slice(&self.get_ref().as_ref()[self.position() as usize..end]);
        self.set_position(end as u64);
    }
    fn remaining_slice(&self) -> &[u8] {
        let len = self.position().min(self.get_ref().as_ref().len() as u64);
        &self.get_ref().as_ref()[(len as usize)..]
    }
}

#[derive(Debug)]
enum BTBuffer {
    NotBuffering,
    Buffering { read: Vec<u8>, write: Vec<u8> },
    Rewound { read: Cursor<Vec<u8>> },
}
impl Default for BTBuffer {
    fn default() -> Self {
        BTBuffer::NotBuffering
    }
}

#[pin_project::pin_project]
#[derive(Debug)]
pub struct BackTrackingIO<T> {
    #[pin]
    io: T,
    buffer: BTBuffer,
}
impl<T> BackTrackingIO<T> {
    pub fn new(io: T) -> Self {
        Self {
            io,
            buffer: BTBuffer::Buffering {
                read: Vec::new(),
                write: Vec::new(),
            },
        }
    }
    pub fn read_buffer(&self) -> &[u8] {
        match &self.buffer {
            BTBuffer::NotBuffering => &[],
            BTBuffer::Buffering { read, .. } => read,
            BTBuffer::Rewound { read } => read.remaining_slice(),
        }
    }
    #[must_use]
    pub fn stop_buffering(&mut self) -> Vec<u8> {
        match std::mem::take(&mut self.buffer) {
            BTBuffer::Buffering { write, .. } => write,
            BTBuffer::NotBuffering => Vec::new(),
            BTBuffer::Rewound { read } => {
                self.buffer = BTBuffer::Rewound { read };
                Vec::new()
            }
        }
    }
    pub fn rewind<'a>(&'a mut self) -> (Vec<u8>, &'a [u8]) {
        match std::mem::take(&mut self.buffer) {
            BTBuffer::Buffering { read, write } => {
                self.buffer = BTBuffer::Rewound {
                    read: Cursor::new(read),
                };
                (
                    write,
                    match &self.buffer {
                        BTBuffer::Rewound { read } => read.get_ref(),
                        _ => unreachable!(),
                    },
                )
            }
            BTBuffer::NotBuffering => (Vec::new(), &[]),
            BTBuffer::Rewound { read } => {
                self.buffer = BTBuffer::Rewound { read };
                (
                    Vec::new(),
                    match &self.buffer {
                        BTBuffer::Rewound { read } => read.get_ref(),
                        _ => unreachable!(),
                    },
                )
            }
        }
    }
    pub fn unwrap(self) -> T {
        self.io
    }
}

impl<T: AsyncRead> AsyncRead for BackTrackingIO<T> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.project();
        match this.buffer {
            BTBuffer::Buffering { read, .. } => {
                let filled = buf.filled().len();
                let res = this.io.poll_read(cx, buf);
                read.extend_from_slice(&buf.filled()[filled..]);
                res
            }
            BTBuffer::NotBuffering => this.io.poll_read(cx, buf),
            BTBuffer::Rewound { read } => {
                let mut ready = false;
                if (read.position() as usize) < read.get_ref().len() {
                    read.pure_read(buf);
                    ready = true;
                }
                if buf.remaining() > 0 {
                    match this.io.poll_read(cx, buf) {
                        Poll::Pending => {
                            if ready {
                                Poll::Ready(Ok(()))
                            } else {
                                Poll::Pending
                            }
                        }
                        a => a,
                    }
                } else {
                    Poll::Ready(Ok(()))
                }
            }
        }
    }
}
impl<T: std::io::Read> std::io::Read for BackTrackingIO<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match &mut self.buffer {
            BTBuffer::Buffering { read, .. } => {
                let n = self.io.read(buf)?;
                read.extend_from_slice(&buf[..n]);
                Ok(n)
            }
            BTBuffer::NotBuffering => self.io.read(buf),
            BTBuffer::Rewound { read } => {
                if (read.position() as usize) < read.get_ref().len() {
                    let n = std::io::Read::read(read, buf)?;
                    if n != 0 {
                        return Ok(n);
                    }
                }
                self.io.read(buf)
            }
        }
    }
}

impl<T: AsyncWrite> AsyncWrite for BackTrackingIO<T> {
    fn is_write_vectored(&self) -> bool {
        self.io.is_write_vectored()
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        self.project().io.poll_flush(cx)
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        self.project().io.poll_shutdown(cx)
    }
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        let this = self.project();
        if let BTBuffer::Buffering { write, .. } = this.buffer {
            write.extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        } else {
            this.io.poll_write(cx, buf)
        }
    }
    fn poll_write_vectored(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> Poll<Result<usize, std::io::Error>> {
        let this = self.project();
        if let BTBuffer::Buffering { write, .. } = this.buffer {
            let len = bufs.iter().map(|b| b.len()).sum();
            write.reserve(len);
            for buf in bufs {
                write.extend_from_slice(buf);
            }
            Poll::Ready(Ok(len))
        } else {
            this.io.poll_write_vectored(cx, bufs)
        }
    }
}

pub struct Counter {
    atomic: AtomicU64,
    ordering: std::sync::atomic::Ordering,
}
impl Counter {
    pub fn new(init: u64, ordering: std::sync::atomic::Ordering) -> Self {
        Self {
            atomic: AtomicU64::new(init),
            ordering,
        }
    }
    pub fn load(&self) -> u64 {
        self.atomic.load(self.ordering)
    }
    pub fn add(&self, value: u64) {
        self.atomic.fetch_add(value, self.ordering);
    }
}

#[pin_project::pin_project]
pub struct CountingReader<'a, R> {
    ctr: &'a Counter,
    #[pin]
    rdr: R,
}
impl<'a, R> CountingReader<'a, R> {
    pub fn new(rdr: R, ctr: &'a Counter) -> Self {
        Self { ctr, rdr }
    }
    pub fn into_inner(self) -> R {
        self.rdr
    }
}
impl<'a, R: AsyncRead> AsyncRead for CountingReader<'a, R> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.project();
        let start = buf.filled().len();
        let res = this.rdr.poll_read(cx, buf);
        let len = buf.filled().len() - start;
        if len > 0 {
            this.ctr.add(len as u64);
        }
        res
    }
}

pub fn dir_copy<'a, P0: AsRef<Path> + 'a + Send + Sync, P1: AsRef<Path> + 'a + Send + Sync>(
    src: P0,
    dst: P1,
    ctr: Option<&'a Counter>,
) -> BoxFuture<'a, Result<(), crate::Error>> {
    async move {
        let m = tokio::fs::metadata(&src).await?;
        let dst_path = dst.as_ref();
        tokio::fs::create_dir_all(&dst_path).await.with_ctx(|_| {
            (
                crate::ErrorKind::Filesystem,
                format!("mkdir {}", dst_path.display()),
            )
        })?;
        tokio::fs::set_permissions(&dst_path, m.permissions())
            .await
            .with_ctx(|_| {
                (
                    crate::ErrorKind::Filesystem,
                    format!("chmod {}", dst_path.display()),
                )
            })?;
        let tmp_dst_path = dst_path.to_owned();
        tokio::task::spawn_blocking(move || {
            nix::unistd::chown(
                &tmp_dst_path,
                Some(Uid::from_raw(m.uid())),
                Some(Gid::from_raw(m.gid())),
            )
        })
        .await
        .with_kind(crate::ErrorKind::Unknown)?
        .with_ctx(|_| {
            (
                crate::ErrorKind::Filesystem,
                format!("chown {}", dst_path.display()),
            )
        })?;
        tokio_stream::wrappers::ReadDirStream::new(tokio::fs::read_dir(src.as_ref()).await?)
            .map_err(|e| crate::Error::new(e, crate::ErrorKind::Filesystem))
            .try_for_each(|e| async move {
                let m = e.metadata().await?;
                let src_path = e.path();
                let dst_path = dst_path.join(e.file_name());
                if m.is_file() {
                    let mut dst_file = create_file(&dst_path).await.with_ctx(|_| {
                        (
                            crate::ErrorKind::Filesystem,
                            format!("create {}", dst_path.display()),
                        )
                    })?;
                    let mut rdr = open_file(&src_path).await.with_ctx(|_| {
                        (
                            crate::ErrorKind::Filesystem,
                            format!("open {}", src_path.display()),
                        )
                    })?;
                    if let Some(ctr) = ctr {
                        tokio::io::copy(&mut CountingReader::new(rdr, ctr), &mut dst_file).await
                    } else {
                        tokio::io::copy(&mut rdr, &mut dst_file).await
                    }
                    .with_ctx(|_| {
                        (
                            crate::ErrorKind::Filesystem,
                            format!("cp {} -> {}", src_path.display(), dst_path.display()),
                        )
                    })?;
                    dst_file.flush().await?;
                    dst_file.shutdown().await?;
                    dst_file.sync_all().await?;
                    drop(dst_file);
                    let tmp_dst_path = dst_path.clone();
                    tokio::task::spawn_blocking(move || {
                        nix::unistd::chown(
                            &tmp_dst_path,
                            Some(Uid::from_raw(m.uid())),
                            Some(Gid::from_raw(m.gid())),
                        )
                    })
                    .await
                    .with_kind(crate::ErrorKind::Unknown)?
                    .with_ctx(|_| {
                        (
                            crate::ErrorKind::Filesystem,
                            format!("chown {}", dst_path.display()),
                        )
                    })?;
                } else if m.is_dir() {
                    dir_copy(src_path, dst_path, ctr).await?;
                } else if m.file_type().is_symlink() {
                    tokio::fs::symlink(
                        tokio::fs::read_link(&src_path).await.with_ctx(|_| {
                            (
                                crate::ErrorKind::Filesystem,
                                format!("readlink {}", src_path.display()),
                            )
                        })?,
                        &dst_path,
                    )
                    .await
                    .with_ctx(|_| {
                        (
                            crate::ErrorKind::Filesystem,
                            format!("cp -P {} -> {}", src_path.display(), dst_path.display()),
                        )
                    })?;
                    // Do not set permissions (see https://unix.stackexchange.com/questions/87200/change-permissions-for-a-symbolic-link)
                }
                Ok(())
            })
            .await?;
        Ok(())
    }
    .boxed()
}

pub async fn copy_file(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<(), Error> {
    let src = src.as_ref();
    tokio::fs::metadata(src)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, src.display()))?;
    let dst = dst.as_ref();
    if let Some(parent) = dst.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("mkdir -p {parent:?}")))?;
    }
    tokio::fs::copy(src, dst)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("cp {src:?} -> {dst:?}")))?;
    Ok(())
}

#[pin_project::pin_project]
pub struct TimeoutStream<S: AsyncRead + AsyncWrite = TcpStream> {
    timeout: Duration,
    #[pin]
    sleep: Sleep,
    #[pin]
    stream: S,
}
impl<S: AsyncRead + AsyncWrite> TimeoutStream<S> {
    pub fn new(stream: S, timeout: Duration) -> Self {
        Self {
            timeout,
            sleep: tokio::time::sleep(timeout),
            stream,
        }
    }
}
impl<S: AsyncRead + AsyncWrite> AsyncRead for TimeoutStream<S> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let mut this = self.project();
        let timeout = this.sleep.as_mut().poll(cx);
        let res = this.stream.poll_read(cx, buf);
        if res.is_ready() {
            this.sleep.reset(Instant::now() + *this.timeout);
        } else if timeout.is_ready() {
            return std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "timed out",
            )));
        }
        res
    }
}
impl<S: AsyncRead + AsyncWrite> AsyncWrite for TimeoutStream<S> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let mut this = self.project();
        let timeout = this.sleep.as_mut().poll(cx);
        let res = this.stream.poll_write(cx, buf);
        if res.is_ready() {
            this.sleep.reset(Instant::now() + *this.timeout);
        } else if timeout.is_ready() {
            return std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "timed out",
            )));
        }
        res
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let mut this = self.project();
        let timeout = this.sleep.as_mut().poll(cx);
        let res = this.stream.poll_flush(cx);
        if res.is_ready() {
            this.sleep.reset(Instant::now() + *this.timeout);
        } else if timeout.is_ready() {
            return std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "timed out",
            )));
        }
        res
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let mut this = self.project();
        let timeout = this.sleep.as_mut().poll(cx);
        let res = this.stream.poll_shutdown(cx);
        if res.is_ready() {
            this.sleep.reset(Instant::now() + *this.timeout);
        } else if timeout.is_ready() {
            return std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "timed out",
            )));
        }
        res
    }
}

#[derive(Debug)]
pub struct TmpDir {
    path: PathBuf,
}
impl TmpDir {
    pub async fn new() -> Result<Self, Error> {
        let path = Path::new("/var/tmp/startos").join(base32::encode(
            base32::Alphabet::Rfc4648 { padding: false },
            &rand::random::<[u8; 8]>(),
        ));
        if tokio::fs::metadata(&path).await.is_ok() {
            return Err(Error::new(
                eyre!("{path:?} already exists"),
                ErrorKind::Filesystem,
            ));
        }
        tokio::fs::create_dir_all(&path).await?;
        crate::util::writeback::set_no_cow(&path).await;
        Ok(Self { path })
    }

    pub async fn delete(self) -> Result<(), Error> {
        tokio::fs::remove_dir_all(&self.path).await?;
        Ok(())
    }

    pub fn leak(mut self) {
        std::mem::take(&mut self.path);
    }

    pub async fn unmount_and_delete(self) -> Result<(), Error> {
        crate::disk::mount::util::unmount_all_under(&self.path, false).await?;
        tokio::fs::remove_dir_all(&self.path).await?;
        Ok(())
    }

    pub async fn gc(self: Arc<Self>) -> Result<(), Error> {
        if let Ok(dir) = Arc::try_unwrap(self) {
            dir.delete().await
        } else {
            Ok(())
        }
    }
}
impl std::ops::Deref for TmpDir {
    type Target = Path;
    fn deref(&self) -> &Self::Target {
        &self.path
    }
}
impl AsRef<Path> for TmpDir {
    fn as_ref(&self) -> &Path {
        &*self
    }
}
impl Drop for TmpDir {
    fn drop(&mut self) {
        if self.path != PathBuf::new() && self.path.exists() {
            let path = std::mem::take(&mut self.path);
            tokio::spawn(async move {
                tokio::fs::remove_dir_all(&path).await.log_err();
            });
        }
    }
}

#[instrument(skip_all)]
pub async fn maybe_open_file(path: impl AsRef<Path>) -> Result<Option<File>, Error> {
    let path = path.as_ref();
    match File::open(path).await {
        Ok(a) => Ok(Some(a)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
    .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("open {path:?}")))
}

#[instrument(skip_all)]
pub async fn open_file(path: impl AsRef<Path>) -> Result<File, Error> {
    maybe_open_file(path.as_ref())
        .await?
        .or_not_found(path.as_ref().display())
}

#[instrument(skip_all)]
pub async fn maybe_read_file_to_string(path: impl AsRef<Path>) -> Result<Option<String>, Error> {
    let Some(mut file) = maybe_open_file(path).await? else {
        return Ok(None);
    };
    let meta = file.metadata().await?;
    let mut res = String::with_capacity(meta.len() as usize);
    file.read_to_string(&mut res).await?;
    Ok(Some(res))
}

#[instrument(skip_all)]
pub async fn read_file_to_string(path: impl AsRef<Path>) -> Result<String, Error> {
    maybe_read_file_to_string(path.as_ref())
        .await?
        .or_not_found(path.as_ref().display())
}

#[instrument(skip_all)]
pub async fn create_file(path: impl AsRef<Path>) -> Result<File, Error> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("mkdir -p {parent:?}")))?;
    }
    File::create(path)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("create {path:?}")))
}

#[instrument(skip_all)]
pub async fn create_file_mod(path: impl AsRef<Path>, mode: u32) -> Result<File, Error> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("mkdir -p {parent:?}")))?;
    }
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(mode)
        .open(path)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("create {path:?}")))
}

#[instrument(skip_all)]
pub async fn append_file(path: impl AsRef<Path>) -> Result<File, Error> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("mkdir -p {parent:?}")))?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("create {path:?}")))
}

#[instrument(skip_all)]
pub async fn delete_file(path: impl AsRef<Path>) -> Result<(), Error> {
    let path = path.as_ref();
    tokio::fs::remove_file(path)
        .await
        .or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(e)
            }
        })
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("delete {path:?}")))
}

pub async fn delete_dir(path: impl AsRef<Path>) -> Result<(), Error> {
    let path = path.as_ref();
    tokio::fs::remove_dir_all(path)
        .await
        .or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(e)
            }
        })
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("delete dir {path:?}")))
}

#[instrument(skip_all)]
pub async fn rename(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<(), Error> {
    let src = src.as_ref();
    let dst = dst.as_ref();
    if let Some(parent) = dst.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("mkdir -p {parent:?}")))?;
    }
    tokio::fs::rename(src, dst)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("mv {src:?} -> {dst:?}")))
}

#[instrument(skip_all)]
pub async fn write_file_atomic(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
) -> Result<(), Error> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("mkdir -p {parent:?}")))?;
    }
    let mut file = AtomicFile::new(path, None::<&Path>)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("create {path:?}")))?;
    file.write_all(contents.as_ref())
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("write {path:?}")))?;
    file.save()
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("save {path:?}")))?;
    Ok(())
}

#[instrument(skip_all)]
pub async fn write_file_owned_atomic(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
    uid: u32,
    gid: u32,
) -> Result<(), Error> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("mkdir -p {parent:?}")))?;
    }
    let mut file = AtomicFile::new(path, None::<&Path>)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("create {path:?}")))?;
    fchown(&*file, Some(uid.into()), Some(gid.into())).with_kind(ErrorKind::Filesystem)?;
    file.write_all(contents.as_ref())
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("write {path:?}")))?;
    file.save()
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("save {path:?}")))?;
    Ok(())
}

fn poll_flush_prefix<W: AsyncWrite>(
    mut writer: Pin<&mut W>,
    cx: &mut std::task::Context<'_>,
    prefix: &mut VecDeque<Cursor<Vec<u8>>>,
    flush_writer: bool,
) -> Poll<Result<(), std::io::Error>> {
    while let Some(mut cur) = prefix.pop_front() {
        let buf = CursorExt::remaining_slice(&cur);
        if !buf.is_empty() {
            match writer.as_mut().poll_write(cx, buf)? {
                Poll::Ready(n) if n == buf.len() => (),
                Poll::Ready(n) => {
                    cur.advance(n);
                    prefix.push_front(cur);
                }
                Poll::Pending => {
                    prefix.push_front(cur);
                    return Poll::Pending;
                }
            }
        }
    }
    if flush_writer {
        writer.poll_flush(cx)
    } else {
        Poll::Ready(Ok(()))
    }
}

fn poll_write_prefix_buf<W: AsyncWrite>(
    mut writer: Pin<&mut W>,
    cx: &mut std::task::Context<'_>,
    prefix: &mut VecDeque<Cursor<Vec<u8>>>,
    buf: &[u8],
) -> Poll<Result<usize, std::io::Error>> {
    futures::ready!(poll_flush_prefix(writer.as_mut(), cx, prefix, false)?);
    writer.poll_write(cx, buf)
}

#[pin_project::pin_project]
pub struct TeeWriter<W1, W2> {
    capacity: usize,
    buffer1: VecDeque<Cursor<Vec<u8>>>,
    buffer2: VecDeque<Cursor<Vec<u8>>>,
    #[pin]
    writer1: W1,
    #[pin]
    writer2: W2,
}
impl<W1, W2> TeeWriter<W1, W2> {
    pub fn new(writer1: W1, writer2: W2, capacity: usize) -> Self {
        Self {
            capacity,
            buffer1: VecDeque::new(),
            buffer2: VecDeque::new(),
            writer1,
            writer2,
        }
    }
}
impl<W1: AsyncWrite + Unpin, W2: AsyncWrite + Unpin> TeeWriter<W1, W2> {
    pub async fn into_inner(mut self) -> Result<(W1, W2), Error> {
        self.flush().await?;

        Ok((self.writer1, self.writer2))
    }
}
impl<W1: AsyncWrite, W2: AsyncWrite> AsyncWrite for TeeWriter<W1, W2> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        mut buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        let mut this = self.project();
        let buffer_size = this
            .buffer1
            .iter()
            .chain(this.buffer2.iter())
            .map(|b| b.get_ref().len())
            .sum::<usize>();
        if buffer_size < *this.capacity {
            let to_write = std::cmp::min(*this.capacity - buffer_size, buf.len());
            buf = &buf[0..to_write];
        } else {
            match (
                poll_flush_prefix(this.writer1.as_mut(), cx, &mut this.buffer1, false)?,
                poll_flush_prefix(this.writer2.as_mut(), cx, &mut this.buffer2, false)?,
            ) {
                (Poll::Ready(()), Poll::Ready(())) => (),
                _ => return Poll::Pending,
            }
        }
        let (w1, w2) = match (
            poll_write_prefix_buf(this.writer1.as_mut(), cx, &mut this.buffer1, buf)?,
            poll_write_prefix_buf(this.writer2.as_mut(), cx, &mut this.buffer2, buf)?,
        ) {
            (Poll::Pending, Poll::Pending) => return Poll::Pending,
            (Poll::Ready(n), Poll::Pending) => (n, 0),
            (Poll::Pending, Poll::Ready(n)) => (0, n),
            (Poll::Ready(n1), Poll::Ready(n2)) => (n1, n2),
        };
        if w1 > w2 {
            this.buffer2.push_back(Cursor::new(buf[w2..w1].to_vec()));
        } else if w1 < w2 {
            this.buffer1.push_back(Cursor::new(buf[w1..w2].to_vec()));
        }
        Poll::Ready(Ok(std::cmp::max(w1, w2)))
    }
    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let mut this = self.project();
        match (
            poll_flush_prefix(this.writer1, cx, &mut this.buffer1, true)?,
            poll_flush_prefix(this.writer2, cx, &mut this.buffer2, true)?,
        ) {
            (Poll::Ready(()), Poll::Ready(())) => Poll::Ready(Ok(())),
            _ => Poll::Pending,
        }
    }
    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        self.poll_flush(cx)
    }
}

#[pin_project::pin_project]
pub struct ParallelBlake3Writer {
    #[pin]
    hasher: NonDetachingJoinHandle<blake3::Hash>,
    buffer: Arc<(std::sync::Mutex<(BytesMut, Vec<Waker>, bool)>, Notify)>,
    capacity: usize,
}
impl ParallelBlake3Writer {
    /// memory usage can be as much as 2x capacity
    pub fn new(capacity: usize) -> Self {
        let buffer = Arc::new((
            std::sync::Mutex::new((BytesMut::new(), Vec::<Waker>::new(), false)),
            Notify::new(),
        ));
        let hasher_buffer = buffer.clone();
        Self {
            hasher: tokio::spawn(async move {
                let mut hasher = blake3::Hasher::new();
                let mut to_hash = BytesMut::new();
                let mut notified;
                while {
                    let mut guard = hasher_buffer.0.lock().unwrap();
                    let (buffer, wakers, shutdown) = &mut *guard;
                    std::mem::swap(buffer, &mut to_hash);
                    let wakers = std::mem::take(wakers);
                    let shutdown = *shutdown;
                    notified = hasher_buffer.1.notified();
                    drop(guard);
                    if to_hash.len() > 128 * 1024
                    /* 128 KiB */
                    {
                        hasher.update_rayon(&to_hash);
                    } else {
                        hasher.update(&to_hash);
                    }
                    to_hash.truncate(0);
                    for waker in wakers {
                        waker.wake();
                    }
                    !shutdown && to_hash.len() == 0
                } {
                    notified.await;
                }
                hasher.finalize()
            })
            .into(),
            buffer,
            capacity,
        }
    }

    pub async fn finalize(mut self) -> Result<blake3::Hash, Error> {
        self.shutdown().await?;
        self.hasher.await.with_kind(ErrorKind::Unknown)
    }
}
impl AsyncWrite for ParallelBlake3Writer {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        let this = self.project();
        let mut guard = this.buffer.0.lock().map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::Other, eyre!("hashing thread panicked"))
        })?;
        let (buffer, wakers, shutdown) = &mut *guard;
        if !*shutdown {
            if buffer.len() < *this.capacity {
                let to_write = std::cmp::min(*this.capacity - buffer.len(), buf.len());
                buffer.extend_from_slice(&buf[0..to_write]);
                if buffer.len() >= *this.capacity / 2 {
                    this.buffer.1.notify_waiters();
                }
                Poll::Ready(Ok(to_write))
            } else {
                wakers.push(cx.waker().clone());
                Poll::Pending
            }
        } else {
            Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                eyre!("write after shutdown"),
            )))
        }
    }
    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let this = self.project();
        let mut guard = this.buffer.0.lock().map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::Other, eyre!("hashing thread panicked"))
        })?;
        let (buffer, wakers, _) = &mut *guard;
        if buffer.is_empty() {
            Poll::Ready(Ok(()))
        } else {
            wakers.push(cx.waker().clone());
            this.buffer.1.notify_waiters();
            Poll::Pending
        }
    }
    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        futures::ready!(self.as_mut().poll_flush(cx)?);
        let this = self.project();
        let mut guard = this.buffer.0.lock().map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::Other, eyre!("hashing thread panicked"))
        })?;
        let (buffer, wakers, shutdown) = &mut *guard;
        if *shutdown && buffer.len() == 0 {
            return Poll::Ready(Ok(()));
        }
        wakers.push(cx.waker().clone());
        *shutdown = true;
        this.buffer.1.notify_waiters();
        Poll::Pending
    }
}

#[pin_project::pin_project]
pub struct TrackingIO<T> {
    position: u64,
    #[pin]
    io: T,
}
impl<T> TrackingIO<T> {
    pub fn new(start: u64, io: T) -> Self {
        Self {
            position: start,
            io,
        }
    }
    pub fn position(&self) -> u64 {
        self.position
    }
    pub fn into_inner(self) -> T {
        self.io
    }
}
impl<W: AsyncWrite> AsyncWrite for TrackingIO<W> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let this = self.project();
        let written = futures::ready!(this.io.poll_write(cx, buf)?);
        *this.position += written as u64;
        Poll::Ready(Ok(written))
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        self.project().io.poll_flush(cx)
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        self.project().io.poll_shutdown(cx)
    }
}
impl<R: AsyncRead> AsyncRead for TrackingIO<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.project();
        let start = buf.filled().len();
        futures::ready!(this.io.poll_read(cx, buf)?);
        *this.position += (buf.filled().len() - start) as u64;
        Poll::Ready(Ok(()))
    }
}
impl<T> std::cmp::PartialEq for TrackingIO<T> {
    fn eq(&self, other: &Self) -> bool {
        self.position.eq(&other.position)
    }
}
impl<T> std::cmp::Eq for TrackingIO<T> {}
impl<T> std::cmp::PartialOrd for TrackingIO<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.position.partial_cmp(&other.position)
    }
}
impl<T> std::cmp::Ord for TrackingIO<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.position.cmp(&other.position)
    }
}
impl<T> std::borrow::Borrow<u64> for TrackingIO<T> {
    fn borrow(&self) -> &u64 {
        &self.position
    }
}

pub struct MutexIO<T>(OwnedMutexGuard<T>);
impl<R: AsyncRead + Unpin> AsyncRead for MutexIO<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut *self.get_mut().0).poll_read(cx, buf)
    }
}
impl<W: AsyncWrite + Unpin> AsyncWrite for MutexIO<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut *self.get_mut().0).poll_write(cx, buf)
    }
    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut *self.get_mut().0).poll_flush(cx)
    }
    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut *self.get_mut().0).poll_shutdown(cx)
    }
}

#[pin_project::pin_project]
pub struct AsyncReadStream<T> {
    buffer: Vec<MaybeUninit<u8>>,
    #[pin]
    pub io: T,
}
impl<T> AsyncReadStream<T> {
    pub fn new(io: T, buffer_size: usize) -> Self {
        Self {
            buffer: vec![MaybeUninit::uninit(); buffer_size],
            io,
        }
    }
}
impl<T: AsyncRead> Stream for AsyncReadStream<T> {
    type Item = Result<Vec<u8>, Error>;
    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.project();
        let mut buf = ReadBuf::uninit(this.buffer);
        match futures::ready!(this.io.poll_read(cx, &mut buf)) {
            Ok(()) if buf.filled().is_empty() => Poll::Ready(None),
            Ok(()) => Poll::Ready(Some(Ok(buf.filled().to_vec()))),
            Err(e) => Poll::Ready(Some(Err(e.into()))),
        }
    }
}

pub struct SharedIO<T>(pub Arc<SyncMutex<T>>);
impl<T> SharedIO<T> {
    pub fn new(t: T) -> Self {
        Self(Arc::new(SyncMutex::new(t)))
    }
}
impl<T> Clone for SharedIO<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: std::io::Write> std::io::Write for SharedIO<T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.mutate(|w| w.write(buf))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.mutate(|w| w.flush())
    }
}
impl<T: std::io::Read> std::io::Read for SharedIO<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.mutate(|r| r.read(buf))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TermSize {
    pub rows: u16,
    pub cols: u16,
    pub pixels: Option<(u16, u16)>, // x, y
}
impl TermSize {
    pub fn get_current() -> Option<Self> {
        if let Some((cols, rows)) = termion::terminal_size().log_err() {
            Some(Self {
                rows,
                cols,
                pixels: termion::terminal_size_pixels().ok(),
            })
        } else {
            None
        }
    }
}
impl FromStr for TermSize {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        (|| {
            let mut split = s.split(":");
            let rows: u16 = split.next()?.parse().ok()?;
            let cols: u16 = split.next()?.parse().ok()?;
            let pixels = if let Some(x) = split.next() {
                let x: u16 = x.parse().ok()?;
                let y: u16 = split.next()?.parse().ok()?;
                Some((x, y))
            } else {
                None
            };

            Some(Self { rows, cols, pixels }).filter(|_| split.next().is_none())
        })()
        .ok_or_else(|| Error::new(eyre!("invalid pty size"), ErrorKind::ParseNumber))
    }
}
impl std::fmt::Display for TermSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.rows, self.cols)?;
        if let Some(pixels) = self.pixels {
            write!(f, ":{}:{}", pixels.0, pixels.1)?;
        }
        Ok(())
    }
}
impl ValueParserFactory for TermSize {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        FromStrParser::new()
    }
}

pub trait ReadWriter: AsyncRead + AsyncWrite {}
impl<T: AsyncRead + AsyncWrite> ReadWriter for T {}

#[instrument(skip_all)]
fn wait_for_created<'a>(
    stream: &'a mut EventStream<[u8; 1024]>,
    path: &'a Path,
) -> BoxFuture<'a, Result<(), Error>> {
    async {
        let parent_path = path.parent().unwrap_or("/".as_ref());
        let parent;
        loop {
            match stream.watches().add(parent_path, WatchMask::CREATE) {
                Ok(desc) => {
                    parent = desc;
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    wait_for_created(stream, parent_path).await?;
                }
                Err(e) => Err(e)?,
            }
        }
        while let Some(e) = stream.try_next().await? {
            if e.mask & EventMask::CREATE != EventMask::empty()
                && e.name.as_deref() == path.file_name()
            {
                break;
            }
        }
        stream.watches().remove(parent)?;
        Ok(())
    }
    .boxed()
}

#[instrument(skip_all)]
pub fn file_string_stream(
    path: impl Into<PathBuf>,
) -> impl Stream<Item = Result<Option<String>, Error>> {
    let path = path.into();
    async_stream::try_stream! {
        let mut stream = Inotify::init()?.into_event_stream([0; 1024])?;
        loop {
            loop {
                match stream.watches().add(
                    &path,
                    WatchMask::MODIFY | WatchMask::CLOSE_WRITE | WatchMask::MOVE_SELF | WatchMask::MOVED_TO | WatchMask::DELETE_SELF,
                ) {
                    Ok(_) => break,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        yield None;
                        wait_for_created(&mut stream, &path).await?;
                    }
                    Err(e) => Err(e)?,
                }
            }
            yield maybe_read_file_to_string(&path).await?;
            while let Some(e) = stream.try_next().await? {
                if e.mask & EventMask::DELETE_SELF != EventMask::empty() {
                    break;
                }
                yield maybe_read_file_to_string(&path).await?;
            }
        }
    }
}

#[pin_project::pin_project]
pub struct ByteReplacementReader<R> {
    pub replace: u8,
    pub with: u8,
    #[pin]
    pub inner: R,
}
impl<R: AsyncRead> AsyncRead for ByteReplacementReader<R> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.project();
        match this.inner.poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                for idx in 0..buf.filled().len() {
                    if buf.filled()[idx] == *this.replace {
                        buf.filled_mut()[idx] = *this.with;
                    }
                }
                Poll::Ready(Ok(()))
            }
            a => a,
        }
    }
}

pub fn to_tmp_path(path: impl AsRef<Path>) -> Result<PathBuf, Error> {
    let path = path.as_ref();
    if let (Some(parent), Some(file_name)) =
        (path.parent(), path.file_name().and_then(|f| f.to_str()))
    {
        Ok(parent.join(format!(".{}.tmp", file_name)))
    } else {
        Err(Error::new(
            eyre!("invalid path: {}", path.display()),
            ErrorKind::Filesystem,
        ))
    }
}

pub async fn canonicalize(
    path: impl AsRef<Path> + Send + Sync,
    create_parent: bool,
) -> Result<PathBuf, Error> {
    canonicalize_rec(path.as_ref(), create_parent).await
}

fn canonicalize_rec(path: &Path, create_parent: bool) -> BoxFuture<'_, Result<PathBuf, Error>> {
    async move {
        match tokio::fs::canonicalize(path).await {
            Ok(canonical) => Ok(canonical),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let Some(file_name) = path.file_name() else {
                    return Err(e).with_ctx(|_| {
                        (ErrorKind::Filesystem, lazy_format!("canonicalize {path:?}"))
                    });
                };
                let parent = path.parent().unwrap_or(Path::new("."));
                if create_parent {
                    // short-circuit: create the whole missing chain at once
                    tokio::fs::create_dir_all(parent).await.with_ctx(|_| {
                        (ErrorKind::Filesystem, lazy_format!("mkdir -p {parent:?}"))
                    })?;
                }
                // resolve the first existing ancestor, then re-append the tail
                Ok(canonicalize_rec(parent, create_parent)
                    .await?
                    .join(file_name))
            }
            Err(e) => {
                Err(e).with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("canonicalize {path:?}")))
            }
        }
    }
    .boxed()
}

pub struct AtomicFile {
    tmp_path: PathBuf,
    path: PathBuf,
    file: Option<File>,
}
impl AtomicFile {
    pub async fn new(
        path: impl AsRef<Path> + Send + Sync,
        tmp_path: Option<impl AsRef<Path> + Send + Sync>,
    ) -> Result<Self, Error> {
        let path = canonicalize(&path, true).await?;
        let tmp_path = if let Some(tmp_path) = tmp_path {
            canonicalize(&tmp_path, true).await?
        } else {
            to_tmp_path(&path)?
        };
        let file = File::create(&tmp_path)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, format!("create {tmp_path:?}")))?;
        Ok(Self {
            tmp_path,
            path,
            file: Some(file),
        })
    }

    pub async fn rollback(mut self) -> Result<(), Error> {
        drop(self.file.take());
        tokio::fs::remove_file(&self.tmp_path)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, format!("rm {:?}", self.tmp_path)))?;
        Ok(())
    }

    pub async fn save(mut self) -> Result<(), Error> {
        use tokio::io::AsyncWriteExt;
        if let Some(file) = self.file.as_mut() {
            file.flush().await?;
            file.shutdown().await?;
            file.sync_all().await?;
        }
        drop(self.file.take());
        tokio::fs::rename(&self.tmp_path, &self.path)
            .await
            .with_ctx(|_| {
                (
                    ErrorKind::Filesystem,
                    format!("mv {} -> {}", self.tmp_path.display(), self.path.display()),
                )
            })?;
        Ok(())
    }
}
impl std::ops::Deref for AtomicFile {
    type Target = File;
    fn deref(&self) -> &Self::Target {
        self.file.as_ref().unwrap()
    }
}
impl std::ops::DerefMut for AtomicFile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.file.as_mut().unwrap()
    }
}
impl Drop for AtomicFile {
    fn drop(&mut self) {
        if let Some(file) = self.file.take() {
            drop(file);
            let path = std::mem::take(&mut self.tmp_path);
            tokio::spawn(async move { tokio::fs::remove_file(path).await.log_err() });
        }
    }
}
