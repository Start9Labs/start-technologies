pub mod git_hash;
pub mod init;
pub mod merkle_archive;
pub mod rpc;
pub mod v1;
pub mod v2;

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncSeek};
pub use v2::{S9pk, manifest};

use crate::prelude::*;
use crate::progress::FullProgressTracker;
use crate::s9pk::merkle_archive::source::{ArchiveSource, DynFileSource};
use crate::s9pk::v1::reader::S9pkReader;
use crate::s9pk::v2::compat::MAGIC_AND_VERSION;
use crate::sign::AnyVerifyingKey;
use crate::util::io::TmpDir;

pub async fn load<S, K>(
    source: S,
    key: K,
    progress: Option<&FullProgressTracker>,
) -> Result<S9pk<DynFileSource>, Error>
where
    S: ArchiveSource,
    S::FetchAllReader: AsyncSeek + Sync,
    K: FnOnce() -> Result<ed25519_dalek::SigningKey, Error>,
{
    load_with_signers(source, key, progress, None).await
}

pub async fn load_with_signers<S, K>(
    source: S,
    key: K,
    progress: Option<&FullProgressTracker>,
    accepted: Option<&[AnyVerifyingKey]>,
) -> Result<S9pk<DynFileSource>, Error>
where
    S: ArchiveSource,
    S::FetchAllReader: AsyncSeek + Sync,
    K: FnOnce() -> Result<ed25519_dalek::SigningKey, Error>,
{
    const MAGIC_LEN: usize = MAGIC_AND_VERSION.len();
    let mut magic = [0_u8; MAGIC_LEN];
    source.fetch(0, 3).await?.read_exact(&mut magic).await?;
    if magic == v2::compat::MAGIC_AND_VERSION {
        let phase = if let Some(progress) = progress {
            let mut phase = progress.add_phase(
                "Converting Package to V2".into(),
                Some(source.size().await.unwrap_or(60)),
            );
            phase.start();
            Some(phase)
        } else {
            None
        };
        tracing::info!("Converting package to v2 s9pk");
        let tmp_dir = TmpDir::new().await?;
        let reader = S9pkReader::from_reader(source.fetch_all().await?, true).await?;
        validate_signer(*reader.developer_key(), accepted)?;
        let s9pk = S9pk::from_v1(reader, Arc::new(tmp_dir), key()?).await?;
        tracing::info!("Converted s9pk successfully");
        if let Some(mut phase) = phase {
            phase.complete();
        }
        Ok(s9pk.into_dyn())
    } else {
        let s9pk = S9pk::deserialize(&Arc::new(source), None).await?;
        validate_signer(s9pk.as_archive().signer(), accepted)?;
        Ok(s9pk.into_dyn())
    }
}

fn validate_signer(
    signer: ed25519_dalek::VerifyingKey,
    accepted: Option<&[AnyVerifyingKey]>,
) -> Result<(), Error> {
    if accepted.is_some_and(|keys| !keys.contains(&AnyVerifyingKey::Ed25519(signer))) {
        return Err(Error::new(
            eyre!("{}", t!("registry.signer.not-accepted")),
            ErrorKind::InvalidSignature,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod trust_tests;
