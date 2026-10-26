use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use startos::registry::signer::AcceptSigners;
use startos::sign::commitment::blake3::Blake3Commitment;
use startos::sign::{AnySignature, AnyVerifyingKey};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;

use crate::Error;

/// A downloadable, signed asset from the registry.
/// Wire-compatible with start-os's `RegistryAsset<Commitment>`.
#[derive(Clone, Debug, Deserialize, Serialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct RegistryAsset<Commitment> {
    pub published_at: DateTime<Utc>,
    pub urls: Vec<Url>,
    pub commitment: Commitment,
    pub signatures: HashMap<AnyVerifyingKey, AnySignature>,
}
rpc_toolkit::reflect_ts!(impl [Commitment] for RegistryAsset<Commitment> where [Commitment: rpc_toolkit::ts::TS]);

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
                    return self.download_to_file(response, dst, on_progress).await;
                }
                Ok(response) => {
                    last_err = Some(format!("HTTP {} from {}", response.status(), url));
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

    async fn download_to_file(
        &self,
        mut response: reqwest::Response,
        path: &Path,
        mut on_progress: impl FnMut(u64),
    ) -> Result<(), Error> {
        let mut file = tokio::fs::File::create(path)
            .await
            .map_err(|e| Error::other(format!("creating download file: {e}")))?;
        let mut hasher = blake3::Hasher::new();
        let mut total_written: u64 = 0;
        loop {
            let chunk = response
                .chunk()
                .await
                .map_err(|e| Error::other(format!("downloading chunk: {e}")))?;
            let Some(chunk) = chunk else { break };
            hasher.update(&chunk);
            file.write_all(&chunk)
                .await
                .map_err(|e| Error::other(format!("writing chunk: {e}")))?;
            total_written += chunk.len() as u64;
            on_progress(total_written);
        }
        file.flush()
            .await
            .map_err(|e| Error::other(format!("flushing download file: {e}")))?;
        file.sync_all()
            .await
            .map_err(|e| Error::other(format!("syncing download file: {e}")))?;

        if total_written != self.commitment.size {
            return Err(Error::other(format!(
                "download size mismatch: expected {}, got {}",
                self.commitment.size, total_written
            )));
        }
        if *hasher.finalize().as_bytes() != *self.commitment.hash {
            return Err(Error::other("BLAKE3 hash mismatch after download"));
        }
        Ok(())
    }

    /// Verify a file on disk matches this asset's commitment.
    pub async fn verify_file(&self, path: &Path) -> Result<(), Error> {
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|e| Error::other(format!("opening file for verification: {e}")))?;
        let metadata = file
            .metadata()
            .await
            .map_err(|e| Error::other(format!("reading file metadata: {e}")))?;
        if metadata.len() != self.commitment.size {
            return Err(Error::other(format!(
                "file size mismatch: expected {}, got {}",
                self.commitment.size,
                metadata.len()
            )));
        }
        let mut hasher = blake3::Hasher::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = file
                .read(&mut buf)
                .await
                .map_err(|e| Error::other(format!("reading file for verification: {e}")))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        if *hasher.finalize().as_bytes() != *self.commitment.hash {
            return Err(Error::other("BLAKE3 hash mismatch"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use base64::Engine;
    use ed25519_dalek::pkcs8::EncodePrivateKey;
    use serde_json::json;
    use startos::sign::{AnySigningKey, Signer};
    use startos::util::serde::Base64;

    use super::*;
    use crate::registry::os::{OsVersionInfo, SIG_CONTEXT};

    const PLATFORM: &str = "spacemit,k1-x";

    /// AlgorithmIdentifier for OID 2.25.31184504503259998342399313009964778715.
    const ED25519PH_ALGORITHM_ID_DER: &[u8] = &[
        0x30, 0x15, 0x06, 0x13, 0x69, 0xae, 0xf5, 0xf5, 0xb1, 0xe7, 0x89, 0x8a, 0x85, 0xc9, 0x9c,
        0x9a, 0xe3, 0xbb, 0x85, 0xfb, 0xf2, 0x89, 0x5b,
    ];

    fn key() -> AnySigningKey {
        ed25519_dalek::SigningKey::from_bytes(&[7; 32])
            .to_pkcs8_pem(Default::default())
            .unwrap()
            .parse()
            .unwrap()
    }

    fn commitment() -> Blake3Commitment {
        Blake3Commitment {
            hash: Base64([3; 32]),
            size: 4096,
        }
    }

    async fn ph_signature(commitment: &Blake3Commitment) -> AnySignature {
        Signer::Key(key())
            .sign_commitment(commitment, SIG_CONTEXT)
            .await
            .unwrap()
    }

    fn relabeled_pure(sig: AnySignature) -> AnySignature {
        match sig {
            AnySignature::Ed25519ph(s) => AnySignature::Ed25519(s),
            other => other,
        }
    }

    fn ed25519ph_pem(sig: &AnySignature) -> String {
        let AnySignature::Ed25519ph(sig) = sig else {
            panic!("expected an Ed25519ph signature")
        };
        let mut der = vec![0x30, 0x59];
        der.extend_from_slice(ED25519PH_ALGORITHM_ID_DER);
        der.extend_from_slice(&[0x04, 0x40]);
        der.extend_from_slice(&sig.to_bytes());
        let b64 = base64::engine::general_purpose::STANDARD.encode(der);
        let (first, rest) = b64.split_at(64);
        format!("-----BEGIN SIGNATURE-----\n{first}\n{rest}\n-----END SIGNATURE-----\n")
    }

    fn parse_asset(signature: String) -> RegistryAsset<Blake3Commitment> {
        let c = commitment();
        let signer = key().verifying_key().to_string();
        let response = json!({
            "1.3.1": {
                "headline": "",
                "releaseNotes": "",
                "sourceVersion": ">=1.0.0",
                "squashfs": {
                    PLATFORM: {
                        "publishedAt": "2026-10-01T00:00:00Z",
                        "urls": ["https://example.com/startwrt.img"],
                        "commitment": { "hash": c.hash.to_string(), "size": c.size },
                        "signatures": { signer: signature },
                    },
                },
            },
        });
        let mut versions: BTreeMap<String, OsVersionInfo> =
            serde_json::from_value(response).unwrap();
        versions
            .remove("1.3.1")
            .unwrap()
            .squashfs
            .remove(PLATFORM)
            .unwrap()
    }

    #[tokio::test]
    async fn ed25519ph_label_parses_and_verifies() {
        let sig = ph_signature(&commitment()).await;
        let asset = parse_asset(ed25519ph_pem(&sig));
        assert_eq!(asset.signatures.values().next(), Some(&sig));
        asset.validate(SIG_CONTEXT, asset.all_signers()).unwrap();
        assert!(asset.validate("s9pk", asset.all_signers()).is_err());
    }

    #[tokio::test]
    async fn id_ed25519_label_on_ed25519ph_signature_verifies() {
        let sig = ph_signature(&commitment()).await;
        let asset = parse_asset(relabeled_pure(sig).to_string());
        asset.validate(SIG_CONTEXT, asset.all_signers()).unwrap();
        assert!(asset.validate("s9pk", asset.all_signers()).is_err());
    }

    #[tokio::test]
    async fn pure_ed25519_signature_verifies() {
        let c = commitment();
        let mut msg = b"Start-Commitment-Sig v1\0".to_vec();
        msg.push(SIG_CONTEXT.len() as u8);
        msg.extend_from_slice(SIG_CONTEXT.as_bytes());
        msg.push(1);
        msg.extend_from_slice(&*c.hash);
        msg.extend_from_slice(&c.size.to_be_bytes());
        let sig = Signer::Key(key()).sign_message(&msg).await.unwrap();
        let asset = parse_asset(sig.to_string());
        asset.validate(SIG_CONTEXT, asset.all_signers()).unwrap();
    }

    #[tokio::test]
    async fn signature_over_another_commitment_is_rejected() {
        let sig = ph_signature(&Blake3Commitment {
            size: 4097,
            ..commitment()
        })
        .await;
        for pem in [ed25519ph_pem(&sig), relabeled_pure(sig).to_string()] {
            let asset = parse_asset(pem);
            assert!(asset.validate(SIG_CONTEXT, asset.all_signers()).is_err());
        }
    }
}
