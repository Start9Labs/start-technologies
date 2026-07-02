use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::sign::commitment::Blake3Commitment;
use crate::sign::{AnySignature, AnyVerifyingKey};
use crate::Error;

use super::signer::AcceptSigners;

/// A downloadable, signed asset from the registry.
/// Wire-compatible with start-os's `RegistryAsset<Commitment>`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryAsset<Commitment> {
    pub published_at: DateTime<Utc>,
    pub urls: Vec<Url>,
    pub commitment: Commitment,
    pub signatures: HashMap<AnyVerifyingKey, AnySignature>,
}

impl<C> RegistryAsset<C> {
    /// Build an `AcceptSigners::All` requiring every signer on this asset.
    pub fn all_signers(&self) -> AcceptSigners {
        AcceptSigners::All(
            self.signatures
                .keys()
                .cloned()
                .map(AcceptSigners::Signer)
                .collect(),
        )
    }
}

impl RegistryAsset<Blake3Commitment> {
    /// Validate all signatures on this asset.
    pub fn validate(
        &self,
        context: &str,
        mut accept: AcceptSigners,
    ) -> Result<&Blake3Commitment, Error> {
        for (signer, signature) in &self.signatures {
            accept.process_signature(signer, &self.commitment, context, signature)?;
        }
        accept.try_accept()?;
        Ok(&self.commitment)
    }

    /// Download this asset to a file, trying each URL in order.
    /// Verifies BLAKE3 hash + size during download.
    /// Calls `on_progress(bytes_written)` for progress tracking.
    pub async fn download(
        &self,
        client: &reqwest::Client,
        dst: &Path,
        on_progress: impl FnMut(u64),
    ) -> Result<(), Error> {
        let mut last_err = None;
        for url in &self.urls {
            match client.get(url.clone()).send().await {
                Ok(response) if response.status().is_success() => {
                    return self
                        .commitment
                        .download_to_file(response, dst, on_progress)
                        .await;
                }
                Ok(response) => {
                    last_err = Some(format!(
                        "HTTP {} from {}",
                        response.status(),
                        url
                    ));
                }
                Err(e) => {
                    last_err = Some(format!("request to {} failed: {e}", url));
                }
            }
        }
        Err(Error::other(format!(
            "all asset URLs failed; last error: {}",
            last_err.unwrap_or_else(|| "no URLs".to_string())
        )))
    }
}
