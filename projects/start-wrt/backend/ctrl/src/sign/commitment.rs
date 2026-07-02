use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::Error;

/// Base64 engine matching start-os: standard alphabet, no padding on encode,
/// accepts both padded and unpadded on decode.
const BASE64: base64::engine::GeneralPurpose =
    base64::engine::general_purpose::GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        base64::engine::GeneralPurposeConfig::new()
            .with_encode_padding(false)
            .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent),
    );

// ── Digestable ───────────────────────────────────────────────────────

/// Types that can feed their data into a digest (for signature computation).
pub trait Digestable {
    fn update<D: digest::Update>(&self, digest: &mut D);
}

// ── Blake3Commitment ─────────────────────────────────────────────────

/// Commitment to a file's contents: BLAKE3 hash + expected size.
/// Wire-compatible with start-os's `Blake3Commitment`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Blake3Commitment {
    /// BLAKE3 hash as base64-encoded 32-byte array.
    #[serde(with = "base64_bytes")]
    pub hash: [u8; 32],
    pub size: u64,
}

impl Digestable for Blake3Commitment {
    fn update<D: digest::Update>(&self, digest: &mut D) {
        digest.update(&self.hash);
        digest.update(&u64::to_be_bytes(self.size));
    }
}

impl Blake3Commitment {
    /// Verify a file on disk matches this commitment.
    pub async fn verify_file(&self, path: &Path) -> Result<(), Error> {
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|e| Error::other(format!("opening file for verification: {e}")))?;
        let metadata = file
            .metadata()
            .await
            .map_err(|e| Error::other(format!("reading file metadata: {e}")))?;
        if metadata.len() != self.size {
            return Err(Error::other(format!(
                "file size mismatch: expected {}, got {}",
                self.size,
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
        let hash = hasher.finalize();
        if *hash.as_bytes() != self.hash {
            return Err(Error::other("BLAKE3 hash mismatch"));
        }
        Ok(())
    }

    /// Download from a URL to a file, computing BLAKE3 as we go. Returns the
    /// number of bytes written. The caller should verify the hash afterward
    /// or use `download_and_verify`.
    pub async fn download_to_file(
        &self,
        response: reqwest::Response,
        path: &Path,
        mut on_progress: impl FnMut(u64),
    ) -> Result<(), Error> {
        let mut file = tokio::fs::File::create(path)
            .await
            .map_err(|e| Error::other(format!("creating download file: {e}")))?;
        let mut hasher = blake3::Hasher::new();
        let mut total_written: u64 = 0;
        let mut stream = response;
        loop {
            let chunk = stream
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

        // Verify size
        if total_written != self.size {
            return Err(Error::other(format!(
                "download size mismatch: expected {}, got {}",
                self.size, total_written
            )));
        }
        // Verify hash
        let hash = hasher.finalize();
        if *hash.as_bytes() != self.hash {
            return Err(Error::other("BLAKE3 hash mismatch after download"));
        }
        Ok(())
    }
}

// ── base64 serde helper ──────────────────────────────────────────────

mod base64_bytes {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    use super::BASE64;

    pub fn serialize<S: Serializer>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&BASE64.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 32], D::Error> {
        let s = String::deserialize(deserializer)?;
        let bytes = BASE64
            .decode(&s)
            .map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("expected 32 bytes"))
    }
}
