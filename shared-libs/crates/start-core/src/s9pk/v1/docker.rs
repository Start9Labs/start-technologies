use std::collections::BTreeSet;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};

use color_eyre::eyre::eyre;
use futures::{FutureExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncSeek, AsyncSeekExt};
use tokio_tar::{Archive, Entry};

use crate::util::io::{from_cbor_async_reader, from_json_async_reader};
use crate::{Error, ErrorKind};

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerMultiArch {
    pub default: String,
    pub available: BTreeSet<String>,
}

#[pin_project::pin_project(project = DockerReaderProject)]
#[derive(Debug)]
pub enum DockerReader<R: AsyncRead + Unpin> {
    SingleArch(#[pin] R),
    MultiArch(#[pin] Entry<Archive<R>>),
}
impl<R: AsyncRead + AsyncSeek + Unpin + Send + Sync> DockerReader<R> {
    pub async fn list_arches(rdr: &mut R) -> Result<BTreeSet<String>, Error> {
        if let Some(multiarch) = tokio_tar::Archive::new(&mut *rdr)
            .entries()?
            .try_filter_map(|e| {
                async move {
                    Ok(if &*e.path()? == Path::new("multiarch.cbor") {
                        Some(e)
                    } else {
                        None
                    })
                }
                .boxed()
            })
            .try_next()
            .await?
        {
            let multiarch: DockerMultiArch = from_cbor_async_reader(multiarch).await?;
            return Ok(multiarch.available);
        }

        rdr.seek(SeekFrom::Start(0)).await?;
        let Some(manifest) = tokio_tar::Archive::new(&mut *rdr)
            .entries()?
            .try_filter_map(|e| {
                async move {
                    Ok(if &*e.path()? == Path::new("manifest.json") {
                        Some(e)
                    } else {
                        None
                    })
                }
                .boxed()
            })
            .try_next()
            .await?
        else {
            return Err(Error::new(
                eyre!("Single arch legacy s9pk is malformed"),
                ErrorKind::ParseS9pk,
            ));
        };

        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Manifest {
            config: PathBuf,
        }
        let manifests: Vec<Manifest> = from_json_async_reader(manifest).await?;
        let Manifest { config } = manifests.into_iter().next().ok_or_else(|| {
            Error::new(
                eyre!("Single arch legacy s9pk has no images"),
                ErrorKind::ParseS9pk,
            )
        })?;
        rdr.seek(SeekFrom::Start(0)).await?;
        let Some(config) = tokio_tar::Archive::new(rdr)
            .entries()?
            .try_filter_map(|e| {
                let config = config.clone();
                async move { Ok(if &*e.path()? == config { Some(e) } else { None }) }.boxed()
            })
            .try_next()
            .await?
        else {
            return Err(Error::new(
                eyre!("Single arch legacy s9pk is malformed"),
                ErrorKind::ParseS9pk,
            ));
        };

        #[derive(Deserialize)]
        struct Config {
            architecture: String,
        }
        let Config { architecture } = from_json_async_reader(config).await?;

        let mut arches = BTreeSet::new();
        arches.insert(architecture);

        Ok(arches)
    }
    pub async fn new(mut rdr: R, arch: &str) -> Result<Self, Error> {
        rdr.seek(SeekFrom::Start(0)).await?;
        if tokio_tar::Archive::new(&mut rdr)
            .entries()?
            .try_filter_map(|e| {
                async move {
                    Ok(if &*e.path()? == Path::new("multiarch.cbor") {
                        Some(e)
                    } else {
                        None
                    })
                }
                .boxed()
            })
            .try_next()
            .await?
            .is_some()
        {
            rdr.seek(SeekFrom::Start(0)).await?;
            if let Some(image) = tokio_tar::Archive::new(rdr)
                .entries()?
                .try_filter_map(|e| {
                    async move {
                        Ok(if &*e.path()? == Path::new(&format!("{}.tar", arch)) {
                            Some(e)
                        } else {
                            None
                        })
                    }
                    .boxed()
                })
                .try_next()
                .await?
            {
                Ok(Self::MultiArch(image))
            } else {
                Err(Error::new(
                    eyre!("Docker image section does not contain tarball for architecture"),
                    ErrorKind::ParseS9pk,
                ))
            }
        } else {
            rdr.seek(SeekFrom::Start(0)).await?;
            Ok(Self::SingleArch(rdr))
        }
    }
}
impl<R: AsyncRead + Unpin + Send + Sync> AsyncRead for DockerReader<R> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.project() {
            DockerReaderProject::SingleArch(r) => r.poll_read(cx, buf),
            DockerReaderProject::MultiArch(r) => r.poll_read(cx, buf),
        }
    }
}
