use std::collections::BTreeSet;
use std::io::SeekFrom;
use std::ops::Range;
use std::path::Path;
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};

use color_eyre::eyre::eyre;
use digest::Output;
use ed25519_dalek::VerifyingKey;
use sha2::{Digest, Sha512};
use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, BufReader, ReadBuf};
use tracing::instrument;

use super::SIG_CONTEXT;
use super::header::{FileSection, Header, TableOfContents};
use crate::prelude::*;
use crate::s9pk::v1::docker::DockerReader;
use crate::util::VersionString;
use crate::util::io::open_file;
use crate::{ImageId, PackageId};

#[pin_project::pin_project]
#[derive(Debug)]
pub struct ReadHandle<'a, R = File> {
    pos: &'a mut u64,
    range: Range<u64>,
    #[pin]
    rdr: &'a mut R,
}
impl<'a, R: AsyncRead + Unpin> ReadHandle<'a, R> {
    pub async fn to_vec(mut self) -> std::io::Result<Vec<u8>> {
        let mut buf = vec![0; (self.range.end - self.range.start) as usize];
        self.read_exact(&mut buf).await?;
        Ok(buf)
    }
}
impl<'a, R: AsyncRead + Unpin> AsyncRead for ReadHandle<'a, R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.project();
        let mut take_buf = buf.take(this.range.end.saturating_sub(**this.pos) as usize);
        let res = AsyncRead::poll_read(this.rdr, cx, &mut take_buf);
        let n = take_buf.filled().len();
        unsafe { buf.assume_init(n) };
        buf.advance(n);
        **this.pos += n as u64;
        res
    }
}
impl<'a, R: AsyncSeek + Unpin> AsyncSeek for ReadHandle<'a, R> {
    fn start_seek(self: Pin<&mut Self>, position: SeekFrom) -> std::io::Result<()> {
        let this = self.project();
        AsyncSeek::start_seek(
            this.rdr,
            match position {
                SeekFrom::Current(n) => SeekFrom::Current(n),
                SeekFrom::End(n) => SeekFrom::Start((this.range.end as i64 + n) as u64),
                SeekFrom::Start(n) => SeekFrom::Start(this.range.start + n),
            },
        )
    }
    fn poll_complete(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<u64>> {
        let this = self.project();
        match AsyncSeek::poll_complete(this.rdr, cx) {
            Poll::Ready(Ok(n)) => {
                let res = n.saturating_sub(this.range.start);
                **this.pos = this.range.start + res;
                Poll::Ready(Ok(res))
            }
            a => a,
        }
    }
}

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub struct ImageTag {
    pub package_id: PackageId,
    pub image_id: ImageId,
    pub version: VersionString,
}
impl ImageTag {
    #[instrument(skip_all)]
    pub fn validate(&self, id: &PackageId, version: &VersionString) -> Result<(), Error> {
        if id != &self.package_id {
            return Err(Error::new(
                eyre!(
                    "Contains image for incorrect package: id {}",
                    self.package_id,
                ),
                crate::ErrorKind::ValidateS9pk,
            ));
        }
        if version != &self.version {
            return Err(Error::new(
                eyre!(
                    "Contains image with incorrect version: expected {} received {}",
                    version,
                    self.version,
                ),
                crate::ErrorKind::ValidateS9pk,
            ));
        }
        Ok(())
    }
}
impl FromStr for ImageTag {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let rest = s.strip_prefix("start9/").ok_or_else(|| {
            Error::new(
                eyre!("Invalid image tag prefix: expected start9/"),
                crate::ErrorKind::ValidateS9pk,
            )
        })?;
        let (package, rest) = rest.split_once("/").ok_or_else(|| {
            Error::new(
                eyre!("Image tag missing image id"),
                crate::ErrorKind::ValidateS9pk,
            )
        })?;
        let (image, version) = rest.split_once(":").ok_or_else(|| {
            Error::new(
                eyre!("Image tag missing version"),
                crate::ErrorKind::ValidateS9pk,
            )
        })?;
        Ok(ImageTag {
            package_id: package.parse()?,
            image_id: image.parse()?,
            version: version.parse()?,
        })
    }
}

pub struct S9pkReader<R: AsyncRead + AsyncSeek + Unpin + Send + Sync = BufReader<File>> {
    hash: Option<Output<Sha512>>,
    hash_string: Option<String>,
    developer_key: VerifyingKey,
    toc: TableOfContents,
    pos: u64,
    rdr: R,
}
impl S9pkReader {
    pub async fn open<P: AsRef<Path>>(path: P, check_sig: bool) -> Result<Self, Error> {
        let p = path.as_ref();
        let rdr = open_file(p).await?;

        Self::from_reader(BufReader::new(rdr), check_sig).await
    }
}
impl<R: AsyncRead + AsyncSeek + Unpin + Send + Sync> S9pkReader<R> {
    #[instrument(skip_all)]
    pub async fn from_reader(mut rdr: R, check_sig: bool) -> Result<Self, Error> {
        let header = Header::deserialize(&mut rdr).await?;

        let (hash, hash_string) = if check_sig {
            let mut hasher = Sha512::new();
            let mut buf = [0; 1024];
            let mut read;
            while {
                read = rdr.read(&mut buf).await?;
                read != 0
            } {
                hasher.update(&buf[0..read]);
            }
            let hash = hasher.clone().finalize();
            header
                .pubkey
                .verify_prehashed(hasher, Some(SIG_CONTEXT), &header.signature)?;
            (
                Some(hash),
                Some(base32::encode(
                    base32::Alphabet::Rfc4648 { padding: false },
                    hash.as_slice(),
                )),
            )
        } else {
            (None, None)
        };

        let pos = rdr.stream_position().await?;

        Ok(S9pkReader {
            hash_string,
            hash,
            developer_key: header.pubkey,
            toc: header.table_of_contents,
            pos,
            rdr,
        })
    }

    pub fn hash(&self) -> Option<&Output<Sha512>> {
        self.hash.as_ref()
    }

    pub fn hash_str(&self) -> Option<&str> {
        self.hash_string.as_ref().map(|s| s.as_str())
    }

    pub fn developer_key(&self) -> &VerifyingKey {
        &self.developer_key
    }

    pub async fn reset(&mut self) -> Result<(), Error> {
        self.rdr.seek(SeekFrom::Start(0)).await?;
        Ok(())
    }

    async fn read_handle<'a>(
        &'a mut self,
        section: FileSection,
    ) -> Result<ReadHandle<'a, R>, Error> {
        if self.pos != section.position {
            self.rdr.seek(SeekFrom::Start(section.position)).await?;
            self.pos = section.position;
        }
        Ok(ReadHandle {
            range: self.pos..(self.pos + section.length),
            pos: &mut self.pos,
            rdr: &mut self.rdr,
        })
    }

    pub async fn manifest_raw(&mut self) -> Result<ReadHandle<'_, R>, Error> {
        self.read_handle(self.toc.manifest).await
    }

    pub async fn manifest(&mut self) -> Result<Value, Error> {
        let slice = self.manifest_raw().await?.to_vec().await?;
        serde_cbor::de::from_reader(slice.as_slice())
            .with_ctx(|_| (crate::ErrorKind::ParseS9pk, "Deserializing Manifest (CBOR)"))
    }

    pub async fn license(&mut self) -> Result<ReadHandle<'_, R>, Error> {
        self.read_handle(self.toc.license).await
    }

    pub async fn instructions(&mut self) -> Result<ReadHandle<'_, R>, Error> {
        self.read_handle(self.toc.instructions).await
    }

    pub async fn icon(&mut self) -> Result<ReadHandle<'_, R>, Error> {
        self.read_handle(self.toc.icon).await
    }

    pub async fn docker_arches(&mut self) -> Result<BTreeSet<String>, Error> {
        DockerReader::list_arches(&mut self.read_handle(self.toc.docker_images).await?).await
    }

    pub async fn docker_images(
        &mut self,
        arch: &str,
    ) -> Result<DockerReader<ReadHandle<'_, R>>, Error> {
        DockerReader::new(self.read_handle(self.toc.docker_images).await?, arch).await
    }

    pub async fn assets(&mut self) -> Result<ReadHandle<'_, R>, Error> {
        self.read_handle(self.toc.assets).await
    }

    pub async fn scripts(&mut self) -> Result<Option<ReadHandle<'_, R>>, Error> {
        Ok(match self.toc.scripts {
            None => None,
            Some(a) => Some(self.read_handle(a).await?),
        })
    }
}
