use blake3::Hash;
use tokio::io::AsyncRead;

use crate::CAP_10_MiB;
use crate::prelude::*;
use crate::s9pk::merkle_archive::sink::Sink;
use crate::s9pk::merkle_archive::source::{ArchiveSource, DynFileSource, FileSource, Section};
use crate::util::io::{ParallelBlake3Writer, TrackingIO};

#[derive(Debug, Clone)]
pub struct FileContents<S>(S);
impl<S> FileContents<S> {
    pub fn new(source: S) -> Self {
        Self(source)
    }
    pub const fn header_size() -> u64 {
        8 // position: u64 BE
    }
}
impl<S: ArchiveSource> FileContents<Section<S>> {
    #[instrument(skip_all)]
    pub async fn deserialize(
        source: S,
        header: &mut (impl AsyncRead + Unpin + Send),
        size: u64,
    ) -> Result<Self, Error> {
        use tokio::io::AsyncReadExt;

        let mut position = [0u8; 8];
        header.read_exact(&mut position).await?;
        let position = u64::from_be_bytes(position);

        super::source::checked_end(&source, position, size).await?;
        Ok(Self(source.section(position, size)))
    }
}
impl<S: FileSource> FileContents<S> {
    pub async fn hash(&self) -> Result<(Hash, u64), Error> {
        let mut hasher = TrackingIO::new(0, ParallelBlake3Writer::new(CAP_10_MiB));
        self.serialize_body(&mut hasher, None).await?;
        let size = hasher.position();
        let hash = hasher.into_inner().finalize().await?;
        Ok((hash, size))
    }
    #[instrument(skip_all)]
    pub async fn serialize_header<W: Sink>(&self, position: u64, w: &mut W) -> Result<u64, Error> {
        use tokio::io::AsyncWriteExt;

        w.write_all(&position.to_be_bytes()).await?;

        Ok(position)
    }
    #[instrument(skip_all)]
    pub async fn serialize_body<W: Sink>(
        &self,
        w: &mut W,
        verify: Option<(Hash, u64)>,
    ) -> Result<(), Error> {
        self.0.copy_verify(w, verify).await?;
        Ok(())
    }
    pub fn into_dyn(self) -> FileContents<DynFileSource> {
        FileContents(DynFileSource::new(self.0))
    }
}
impl<S> std::ops::Deref for FileContents<S> {
    type Target = S;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
