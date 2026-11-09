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
use crate::registry::signer::AcceptSigners;
use crate::s9pk::merkle_archive::source::{ArchiveSource, DynFileSource};
use crate::s9pk::v1::reader::S9pkReader;
use crate::s9pk::v2::compat::MAGIC_AND_VERSION;
use crate::sign::{AnyVerifyingKey, SignatureMap};
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
    if is_v1(&source).await? {
        return convert_v1(source, key, progress, None).await;
    }
    Ok(S9pk::deserialize_local(&Arc::new(source), None)
        .await?
        .into_dyn())
}

/// Verifies request-supplied endorsements without reading an upload's trailer.
pub async fn load_for_sideload<S, K>(
    source: S,
    key: K,
    progress: Option<&FullProgressTracker>,
    signatures: &SignatureMap,
    accepted: Option<AcceptSigners>,
) -> Result<S9pk<DynFileSource>, Error>
where
    S: ArchiveSource,
    S::FetchAllReader: AsyncSeek + Sync,
    K: FnOnce() -> Result<ed25519_dalek::SigningKey, Error>,
{
    if is_v1(&source).await? {
        return convert_v1(source, key, progress, accepted).await;
    }
    let s9pk = S9pk::deserialize(&Arc::new(source), None).await?;
    let mut policy = accepted.unwrap_or(AcceptSigners::Accepted);
    policy.process_verified_signer(&AnyVerifyingKey::Ed25519(s9pk.as_archive().signer()));
    let commitment = s9pk.as_archive().commitment().await?;
    for (signer, signature) in signatures {
        policy.process_signature(signer, &commitment, v2::SIG_CONTEXT, signature)?;
    }
    policy.try_accept()?;
    Ok(s9pk.into_dyn())
}

pub(crate) async fn is_v1(source: &impl ArchiveSource) -> Result<bool, Error> {
    let mut magic = [0_u8; MAGIC_AND_VERSION.len()];
    source
        .fetch(0, magic.len() as u64)
        .await?
        .read_exact(&mut magic)
        .await?;
    Ok(magic == MAGIC_AND_VERSION)
}

async fn convert_v1<S, K>(
    source: S,
    key: K,
    progress: Option<&FullProgressTracker>,
    accepted: Option<AcceptSigners>,
) -> Result<S9pk<DynFileSource>, Error>
where
    S: ArchiveSource,
    S::FetchAllReader: AsyncSeek + Sync,
    K: FnOnce() -> Result<ed25519_dalek::SigningKey, Error>,
{
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
    if let Some(mut policy) = accepted {
        policy.process_verified_signer(&AnyVerifyingKey::Ed25519(*reader.developer_key()));
        policy.try_accept()?;
    }
    let s9pk = S9pk::from_v1(reader, Arc::new(tmp_dir), key()?).await?;
    tracing::info!("Converted s9pk successfully");
    if let Some(mut phase) = phase {
        phase.complete();
    }
    Ok(s9pk.into_dyn())
}

#[cfg(test)]
mod trust_tests;
