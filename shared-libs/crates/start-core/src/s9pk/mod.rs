pub mod git_hash;
pub mod init;
pub mod merkle_archive;
pub mod rpc;
pub mod v1;
pub mod v2;

use std::sync::Arc;

use futures::FutureExt;
use futures::future::BoxFuture;
use tokio::io::{AsyncReadExt, AsyncSeek};
pub use v2::{S9pk, manifest};

use crate::prelude::*;
use crate::progress::FullProgressTracker;
use crate::registry::signer::AcceptSigners;
use crate::s9pk::merkle_archive::source::{ArchiveSource, DynFileSource, FileSource, Section};
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

pub type SignerCheck = BoxFuture<'static, Result<(), Error>>;

pub async fn load_with_signers<S, K>(
    source: S,
    key: K,
    progress: Option<&FullProgressTracker>,
    mut accepted: Option<AcceptSigners>,
) -> Result<S9pk<DynFileSource>, Error>
where
    S: ArchiveSource,
    S::FetchAllReader: AsyncSeek + Sync,
    K: FnOnce() -> Result<ed25519_dalek::SigningKey, Error>,
{
    let source = Arc::new(source);
    match load_header(&source, key, progress, &mut accepted).await? {
        Loaded::V1(s9pk) => Ok(s9pk),
        Loaded::V2(mut s9pk) => {
            s9pk.read_trailer(&source).await?;
            if let Some(policy) = accepted {
                accept_signers(&s9pk, policy).await?;
            }
            Ok(s9pk.into_dyn())
        }
    }
}

/// Returns once the header is read; the check reads to the end of the source.
pub async fn load_deferring_signers<S, K>(
    source: S,
    key: K,
    progress: Option<&FullProgressTracker>,
    mut accepted: Option<AcceptSigners>,
) -> Result<(S9pk<DynFileSource>, Option<SignerCheck>), Error>
where
    S: ArchiveSource,
    S::FetchAllReader: AsyncSeek + Sync,
    K: FnOnce() -> Result<ed25519_dalek::SigningKey, Error>,
{
    let source = Arc::new(source);
    match load_header(&source, key, progress, &mut accepted).await? {
        Loaded::V1(s9pk) => Ok((s9pk, None)),
        Loaded::V2(s9pk) => {
            let check = accepted.map(|policy| {
                let mut s9pk = s9pk.clone();
                async move {
                    s9pk.read_trailer(&source).await?;
                    accept_signers(&s9pk, policy).await
                }
                .boxed()
            });
            Ok((s9pk.into_dyn(), check))
        }
    }
}

enum Loaded<S> {
    V1(S9pk<DynFileSource>),
    V2(S9pk<Section<Arc<S>>>),
}

async fn load_header<S, K>(
    source: &Arc<S>,
    key: K,
    progress: Option<&FullProgressTracker>,
    accepted: &mut Option<AcceptSigners>,
) -> Result<Loaded<S>, Error>
where
    S: ArchiveSource,
    S::FetchAllReader: AsyncSeek + Sync,
    K: FnOnce() -> Result<ed25519_dalek::SigningKey, Error>,
{
    const MAGIC_LEN: usize = MAGIC_AND_VERSION.len();
    let mut magic = [0_u8; MAGIC_LEN];
    source.fetch(0, 3).await?.read_exact(&mut magic).await?;
    if magic != v2::compat::MAGIC_AND_VERSION {
        return Ok(Loaded::V2(S9pk::deserialize(source, None).await?));
    }
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
    if let Some(mut policy) = accepted.take() {
        policy.process_verified_signer(&AnyVerifyingKey::Ed25519(*reader.developer_key()));
        policy.try_accept()?;
    }
    let s9pk = S9pk::from_v1(reader, Arc::new(tmp_dir), key()?).await?;
    tracing::info!("Converted s9pk successfully");
    if let Some(mut phase) = phase {
        phase.complete();
    }
    Ok(Loaded::V1(s9pk.into_dyn()))
}

async fn accept_signers<S: FileSource + Clone>(
    s9pk: &S9pk<S>,
    mut policy: AcceptSigners,
) -> Result<(), Error> {
    let commitment = s9pk.as_archive().commitment().await?;
    for (signer, signature) in s9pk.signatures().await? {
        policy.process_signature(&signer, &commitment, v2::SIG_CONTEXT, &signature)?;
    }
    policy.try_accept()
}

#[cfg(test)]
mod trust_tests;
