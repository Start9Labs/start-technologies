use std::cmp::min;
use std::io::SeekFrom;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use blake3::Hash;
use futures::future::BoxFuture;
use futures::{Future, FutureExt};
use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite, Take};

use crate::prelude::*;
use crate::s9pk::merkle_archive::hash::VerifyingWriter;
use crate::util::io::{TmpDir, open_file};

pub mod http;
pub mod multi_cursor_file;

pub trait FileSource: Send + Sync + Sized + 'static {
    type Reader: AsyncRead + Unpin + Send;
    type SliceReader: AsyncRead + Unpin + Send;
    fn size(&self) -> impl Future<Output = Result<u64, Error>> + Send;
    fn reader(&self) -> impl Future<Output = Result<Self::Reader, Error>> + Send;
    fn slice(
        &self,
        position: u64,
        size: u64,
    ) -> impl Future<Output = Result<Self::SliceReader, Error>> + Send;
    fn copy<W: AsyncWrite + Unpin + Send + ?Sized>(
        &self,
        w: &mut W,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move {
            tokio::io::copy(&mut self.reader().await?, w).await?;
            Ok(())
        }
    }
    fn copy_verify<W: AsyncWrite + Unpin + Send + ?Sized>(
        &self,
        w: &mut W,
        verify: Option<(Hash, u64)>,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move {
            let mut w = VerifyingWriter::new(w, verify);
            tokio::io::copy(&mut self.reader().await?, &mut w).await?;
            w.verify().await?;
            Ok(())
        }
    }
    fn to_vec(
        &self,
        verify: Option<(Hash, u64)>,
    ) -> impl Future<Output = Result<Vec<u8>, Error>> + Send {
        fn to_vec(
            src: &impl FileSource,
            verify: Option<(Hash, u64)>,
        ) -> BoxFuture<'_, Result<Vec<u8>, Error>> {
            async move {
                let mut vec = Vec::with_capacity(if let Some((_, size)) = &verify {
                    *size
                } else {
                    src.size().await?
                } as usize);
                src.copy_verify(&mut vec, verify).await?;
                Ok(vec)
            }
            .boxed()
        }
        to_vec(self, verify)
    }
}

impl<T: FileSource> FileSource for Arc<T> {
    type Reader = T::Reader;
    type SliceReader = T::SliceReader;
    async fn size(&self) -> Result<u64, Error> {
        self.deref().size().await
    }
    async fn reader(&self) -> Result<Self::Reader, Error> {
        self.deref().reader().await
    }
    async fn slice(&self, position: u64, size: u64) -> Result<Self::SliceReader, Error> {
        self.deref().slice(position, size).await
    }
    async fn copy<W: AsyncWrite + Unpin + Send + ?Sized>(&self, w: &mut W) -> Result<(), Error> {
        self.deref().copy(w).await
    }
    async fn copy_verify<W: AsyncWrite + Unpin + Send + ?Sized>(
        &self,
        w: &mut W,
        verify: Option<(Hash, u64)>,
    ) -> Result<(), Error> {
        self.deref().copy_verify(w, verify).await
    }
    async fn to_vec(&self, verify: Option<(Hash, u64)>) -> Result<Vec<u8>, Error> {
        self.deref().to_vec(verify).await
    }
}

#[derive(Clone)]
pub struct DynFileSource(Arc<dyn DynableFileSource>);
impl DynFileSource {
    pub fn new<T: FileSource>(source: T) -> Self {
        Self(Arc::new(source))
    }
}
impl FileSource for DynFileSource {
    type Reader = Box<dyn AsyncRead + Unpin + Send>;
    type SliceReader = Box<dyn AsyncRead + Unpin + Send>;
    async fn size(&self) -> Result<u64, Error> {
        self.0.size().await
    }
    async fn reader(&self) -> Result<Self::Reader, Error> {
        self.0.reader().await
    }
    async fn slice(&self, position: u64, size: u64) -> Result<Self::SliceReader, Error> {
        self.0.slice(position, size).await
    }
    async fn copy<W: AsyncWrite + Unpin + Send + ?Sized>(
        &self,
        mut w: &mut W,
    ) -> Result<(), Error> {
        self.0.copy(&mut w).await
    }
    async fn copy_verify<W: AsyncWrite + Unpin + Send + ?Sized>(
        &self,
        mut w: &mut W,
        verify: Option<(Hash, u64)>,
    ) -> Result<(), Error> {
        self.0.copy_verify(&mut w, verify).await
    }
    async fn to_vec(&self, verify: Option<(Hash, u64)>) -> Result<Vec<u8>, Error> {
        self.0.to_vec(verify).await
    }
}

#[async_trait::async_trait]
trait DynableFileSource: Send + Sync + 'static {
    async fn size(&self) -> Result<u64, Error>;
    async fn reader(&self) -> Result<Box<dyn AsyncRead + Unpin + Send>, Error>;
    async fn slice(
        &self,
        position: u64,
        size: u64,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>, Error>;
    async fn copy(&self, w: &mut (dyn AsyncWrite + Unpin + Send)) -> Result<(), Error>;
    async fn copy_verify(
        &self,
        w: &mut (dyn AsyncWrite + Unpin + Send),
        verify: Option<(Hash, u64)>,
    ) -> Result<(), Error>;
    async fn to_vec(&self, verify: Option<(Hash, u64)>) -> Result<Vec<u8>, Error>;
}
#[async_trait::async_trait]
impl<T: FileSource> DynableFileSource for T {
    async fn size(&self) -> Result<u64, Error> {
        FileSource::size(self).await
    }
    async fn reader(&self) -> Result<Box<dyn AsyncRead + Unpin + Send>, Error> {
        Ok(Box::new(FileSource::reader(self).await?))
    }
    async fn slice(
        &self,
        position: u64,
        size: u64,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>, Error> {
        Ok(Box::new(FileSource::slice(self, position, size).await?))
    }
    async fn copy(&self, w: &mut (dyn AsyncWrite + Unpin + Send)) -> Result<(), Error> {
        FileSource::copy(self, w).await
    }
    async fn copy_verify(
        &self,
        w: &mut (dyn AsyncWrite + Unpin + Send),
        verify: Option<(Hash, u64)>,
    ) -> Result<(), Error> {
        FileSource::copy_verify(self, w, verify).await
    }
    async fn to_vec(&self, verify: Option<(Hash, u64)>) -> Result<Vec<u8>, Error> {
        FileSource::to_vec(self, verify).await
    }
}

impl FileSource for PathBuf {
    type Reader = File;
    type SliceReader = Take<Self::Reader>;
    async fn size(&self) -> Result<u64, Error> {
        Ok(tokio::fs::metadata(self).await?.len())
    }
    async fn reader(&self) -> Result<Self::Reader, Error> {
        Ok(open_file(self).await?)
    }
    async fn slice(&self, position: u64, size: u64) -> Result<Self::SliceReader, Error> {
        let mut r = FileSource::reader(self).await?;
        r.seek(SeekFrom::Start(position)).await?;
        Ok(r.take(size))
    }
}

impl FileSource for Arc<[u8]> {
    type Reader = std::io::Cursor<Self>;
    type SliceReader = Take<Self::Reader>;
    async fn size(&self) -> Result<u64, Error> {
        Ok(self.len() as u64)
    }
    async fn reader(&self) -> Result<Self::Reader, Error> {
        Ok(std::io::Cursor::new(self.clone()))
    }
    async fn slice(&self, position: u64, size: u64) -> Result<Self::SliceReader, Error> {
        let mut r = FileSource::reader(self).await?;
        r.seek(SeekFrom::Start(position)).await?;
        Ok(r.take(size))
    }
    async fn copy<W: AsyncWrite + Unpin + Send + ?Sized>(&self, w: &mut W) -> Result<(), Error> {
        use tokio::io::AsyncWriteExt;

        w.write_all(&*self).await?;
        Ok(())
    }
}

pub trait ArchiveSource: Send + Sync + Sized + 'static {
    type FetchReader: AsyncRead + Unpin + Send;
    type FetchAllReader: AsyncRead + Unpin + Send;
    fn size(&self) -> impl Future<Output = Option<u64>> + Send {
        async { None }
    }
    fn fetch_all(&self) -> impl Future<Output = Result<Self::FetchAllReader, Error>> + Send;
    fn fetch(
        &self,
        position: u64,
        size: u64,
    ) -> impl Future<Output = Result<Self::FetchReader, Error>> + Send;
    fn copy_all_to<W: AsyncWrite + Unpin + Send + ?Sized>(
        &self,
        w: &mut W,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move {
            tokio::io::copy(&mut self.fetch_all().await?, w).await?;
            Ok(())
        }
    }
    fn copy_to<W: AsyncWrite + Unpin + Send + ?Sized>(
        &self,
        position: u64,
        size: u64,
        w: &mut W,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move {
            tokio::io::copy(&mut self.fetch(position, size).await?, w).await?;
            Ok(())
        }
    }
    fn section(self, position: u64, size: u64) -> Section<Self> {
        Section {
            source: self,
            position,
            size,
        }
    }
}

impl<T: ArchiveSource> ArchiveSource for Arc<T> {
    type FetchReader = T::FetchReader;
    type FetchAllReader = T::FetchAllReader;
    async fn size(&self) -> Option<u64> {
        self.deref().size().await
    }
    async fn fetch_all(&self) -> Result<Self::FetchAllReader, Error> {
        self.deref().fetch_all().await
    }
    async fn fetch(&self, position: u64, size: u64) -> Result<Self::FetchReader, Error> {
        self.deref().fetch(position, size).await
    }
    async fn copy_all_to<W: AsyncWrite + Unpin + Send + ?Sized>(
        &self,
        w: &mut W,
    ) -> Result<(), Error> {
        self.deref().copy_all_to(w).await
    }
    async fn copy_to<W: AsyncWrite + Unpin + Send + ?Sized>(
        &self,
        position: u64,
        size: u64,
        w: &mut W,
    ) -> Result<(), Error> {
        self.deref().copy_to(position, size, w).await
    }
}

impl ArchiveSource for Arc<[u8]> {
    type FetchReader = tokio::io::Take<std::io::Cursor<Self>>;
    type FetchAllReader = std::io::Cursor<Self>;
    async fn fetch_all(&self) -> Result<Self::FetchAllReader, Error> {
        Ok(std::io::Cursor::new(self.clone()))
    }
    async fn fetch(&self, position: u64, size: u64) -> Result<Self::FetchReader, Error> {
        use tokio::io::AsyncReadExt;

        let mut cur = std::io::Cursor::new(self.clone());
        cur.set_position(position);
        Ok(cur.take(size))
    }
}

#[derive(Debug, Clone)]
pub struct Section<S> {
    source: S,
    position: u64,
    size: u64,
}
impl<S> Section<S> {
    pub fn source(&self) -> &S {
        &self.source
    }
    pub fn null(source: S) -> Self {
        Self {
            source,
            position: 0,
            size: 0,
        }
    }
}
impl<S: ArchiveSource> FileSource for Section<S> {
    type Reader = S::FetchReader;
    type SliceReader = S::FetchReader;
    async fn size(&self) -> Result<u64, Error> {
        Ok(self.size)
    }
    async fn reader(&self) -> Result<Self::Reader, Error> {
        self.source.fetch(self.position, self.size).await
    }
    async fn slice(&self, position: u64, size: u64) -> Result<Self::SliceReader, Error> {
        let position = min(position, self.size);
        let offset = self.position.checked_add(position).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "archive section offset overflow",
            )
        })?;
        self.source
            .fetch(offset, min(size, self.size - position))
            .await
    }
    async fn copy<W: AsyncWrite + Unpin + Send + ?Sized>(&self, w: &mut W) -> Result<(), Error> {
        self.source.copy_to(self.position, self.size, w).await
    }
}

pub type DynRead = Box<dyn AsyncRead + Unpin + Send + Sync + 'static>;
pub fn into_dyn_read<R: AsyncRead + Unpin + Send + Sync + 'static>(r: R) -> DynRead {
    Box::new(r)
}

#[derive(Clone)]
pub struct TmpSource<S> {
    tmp_dir: Arc<TmpDir>,
    source: S,
}
impl<S> TmpSource<S> {
    pub fn new(tmp_dir: Arc<TmpDir>, source: S) -> Self {
        Self { tmp_dir, source }
    }
    pub async fn gc(self) -> Result<(), Error> {
        self.tmp_dir.gc().await
    }
}
impl<S> std::ops::Deref for TmpSource<S> {
    type Target = S;
    fn deref(&self) -> &Self::Target {
        &self.source
    }
}
impl<S: ArchiveSource> ArchiveSource for TmpSource<S> {
    type FetchReader = <S as ArchiveSource>::FetchReader;
    type FetchAllReader = <S as ArchiveSource>::FetchAllReader;
    async fn size(&self) -> Option<u64> {
        self.source.size().await
    }
    async fn fetch_all(&self) -> Result<Self::FetchAllReader, Error> {
        self.source.fetch_all().await
    }
    async fn fetch(&self, position: u64, size: u64) -> Result<Self::FetchReader, Error> {
        self.source.fetch(position, size).await
    }
    async fn copy_all_to<W: AsyncWrite + Unpin + Send + ?Sized>(
        &self,
        w: &mut W,
    ) -> Result<(), Error> {
        self.source.copy_all_to(w).await
    }
    async fn copy_to<W: AsyncWrite + Unpin + Send + ?Sized>(
        &self,
        position: u64,
        size: u64,
        w: &mut W,
    ) -> Result<(), Error> {
        self.source.copy_to(position, size, w).await
    }
}
impl<S: FileSource> From<TmpSource<S>> for DynFileSource {
    fn from(value: TmpSource<S>) -> Self {
        DynFileSource::new(value)
    }
}

impl<S: FileSource> FileSource for TmpSource<S> {
    type Reader = <S as FileSource>::Reader;
    type SliceReader = <S as FileSource>::SliceReader;
    async fn size(&self) -> Result<u64, Error> {
        self.source.size().await
    }
    async fn reader(&self) -> Result<Self::Reader, Error> {
        self.source.reader().await
    }
    async fn slice(&self, position: u64, size: u64) -> Result<Self::SliceReader, Error> {
        self.source.slice(position, size).await
    }
    async fn copy<W: AsyncWrite + Unpin + Send + ?Sized>(
        &self,
        mut w: &mut W,
    ) -> Result<(), Error> {
        self.source.copy(&mut w).await
    }
    async fn copy_verify<W: AsyncWrite + Unpin + Send + ?Sized>(
        &self,
        mut w: &mut W,
        verify: Option<(Hash, u64)>,
    ) -> Result<(), Error> {
        self.source.copy_verify(&mut w, verify).await
    }
    async fn to_vec(&self, verify: Option<(Hash, u64)>) -> Result<Vec<u8>, Error> {
        self.source.to_vec(verify).await
    }
}

#[cfg(test)]
mod tests;
