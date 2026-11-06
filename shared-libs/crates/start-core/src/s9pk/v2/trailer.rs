use std::path::Path;

use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::prelude::*;
use crate::s9pk::merkle_archive::source::ArchiveSource;
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::sign::{AnySignature, AnyVerifyingKey, SignatureMap};
use crate::util::io::{AtomicFile, open_file};
use crate::util::serde::der64::DerEncoding;

pub(super) const MAX_REGION: u64 = 1024 * 1024;
const MAGIC: &[u8; 8] = b"S9PKEXT1";

#[derive(Clone, Debug, Serialize)]
pub struct Diagnostic {
    pub kind: &'static str,
    pub message: String,
    pub offset: Option<u64>,
}

#[derive(Clone, Default)]
pub struct Trailer {
    pub(crate) signatures: SignatureMap,
    pub(crate) diagnostics: Vec<Diagnostic>,
    start: Option<u64>,
    raw: Vec<u8>,
    pub(super) editable: bool,
    present: bool,
}

impl Trailer {
    fn diagnose(&mut self, kind: &'static str, message: &str, offset: Option<u64>) {
        self.diagnostics.push(Diagnostic {
            kind,
            message: message.into(),
            offset,
        });
    }

    pub(crate) fn present(&self) -> bool {
        self.present
    }

    pub(super) async fn read(
        source: &impl ArchiveSource,
        commitment: &MerkleArchiveCommitment,
        archive_end: u64,
    ) -> Self {
        Self::discover(source, commitment, archive_end)
            .await
            .unwrap_or_else(|_| {
                let mut result = Self::default();
                result.diagnose("read-failed", &t!("s9pk.trailer.read-failed"), None);
                result
            })
    }

    async fn discover(
        source: &impl ArchiveSource,
        commitment: &MerkleArchiveCommitment,
        archive_end: u64,
    ) -> Result<Self, Error> {
        let mut result = Self {
            editable: true,
            ..Self::default()
        };
        let Some(size) = source.size().await else {
            result.editable = false;
            result.diagnose("unknown-size", &t!("s9pk.trailer.unknown-size"), None);
            return Ok(result);
        };
        if size < 16 {
            return Ok(result);
        }
        let mut footer = [0; 16];
        source
            .fetch(size - 16, 16)
            .await?
            .read_exact(&mut footer)
            .await?;
        if &footer[8..15] != &MAGIC[..7] {
            return Ok(result);
        }
        result.present = true;
        result.editable = false;
        if &footer[8..] != MAGIC {
            result.diagnose(
                "unsupported-version",
                &t!("s9pk.trailer.unsupported-version"),
                Some(size - 8),
            );
            return Ok(result);
        }
        let length = u64::from_be_bytes(footer[..8].try_into().unwrap());
        let header_size = super::MAGIC_AND_VERSION.len() as u64
            + crate::s9pk::merkle_archive::MerkleArchive::<()>::header_size();
        let Some(start) = (size - 16)
            .checked_sub(length)
            .filter(|s| length <= MAX_REGION && *s >= header_size && *s >= archive_end)
        else {
            result.diagnose(
                "invalid-framing",
                &t!("s9pk.trailer.invalid-framing"),
                Some(size - 16),
            );
            return Ok(result);
        };
        let mut raw = vec![0; length as usize];
        source
            .fetch(start, length)
            .await?
            .read_exact(&mut raw)
            .await?;
        let mut records = Vec::new();
        let mut pos = 0;
        while pos < raw.len() {
            if raw.len() - pos < 10 {
                result.diagnose(
                    "invalid-framing",
                    &t!("s9pk.trailer.invalid-framing"),
                    Some(start + pos as u64),
                );
                return Ok(result);
            }
            let kind = u16::from_be_bytes(raw[pos..pos + 2].try_into().unwrap());
            let len = u64::from_be_bytes(raw[pos + 2..pos + 10].try_into().unwrap());
            if len > (raw.len() - pos - 10) as u64 {
                result.diagnose(
                    "invalid-framing",
                    &t!("s9pk.trailer.invalid-framing"),
                    Some(start + pos as u64),
                );
                return Ok(result);
            }
            let end = pos + 10 + len as usize;
            records.push((kind, pos, end));
            pos = end;
        }
        result.editable = true;
        for (kind, pos, end) in records {
            if kind != 1 {
                continue;
            }
            let value = &raw[pos + 10..end];
            let parsed = decode_endorsement(value);
            let (key, signature) = match parsed {
                Ok(record) => record,
                Err(_) => {
                    result.editable = false;
                    result.diagnose(
                        "invalid-record",
                        &t!("s9pk.trailer.invalid-record"),
                        Some(start + pos as u64),
                    );
                    continue;
                }
            };
            let verified = crate::sign::compat::verify_commitment(
                &key,
                commitment,
                super::SIG_CONTEXT,
                &signature,
            )
            .map(|signature| (key, signature));
            match verified {
                Ok((key, signature)) => {
                    result.signatures.entry(key).or_insert(signature);
                }
                Err(_) => {
                    result.diagnose(
                        "invalid-signature",
                        &t!("s9pk.trailer.invalid-signature"),
                        Some(start + pos as u64),
                    );
                }
            }
        }
        result.raw = raw;
        result.start = Some(start);
        Ok(result)
    }

    /// Imports an existing endorsement without changing any base archive bytes.
    pub async fn append(
        path: &Path,
        signer: AnyVerifyingKey,
        signature: AnySignature,
    ) -> Result<(), Error> {
        let package = super::S9pk::open(path, None).await?;
        let trailer = &package.trailer;
        let needed = package.needs_endorsement(&signer).await?;
        crate::sign::compat::verify_commitment(
            &signer,
            &package.as_archive().commitment().await?,
            super::SIG_CONTEXT,
            &signature,
        )?;
        if !needed {
            return Ok(());
        }
        let record = encode_endorsement(&signer, &signature)?;
        let length = (trailer.raw.len() as u64)
            .checked_add(10)
            .and_then(|length| length.checked_add(record.len() as u64))
            .filter(|length| *length <= MAX_REGION);
        ensure_code!(
            length.is_some(),
            ErrorKind::ParseS9pk,
            "{}",
            t!("s9pk.trailer.too-large")
        );
        let length = length.unwrap();
        let mut region = trailer.raw.clone();
        region.extend_from_slice(&1_u16.to_be_bytes());
        region.extend_from_slice(&(record.len() as u64).to_be_bytes());
        region.extend_from_slice(&record);
        region.extend_from_slice(&length.to_be_bytes());
        region.extend_from_slice(MAGIC);
        let base_len = trailer.start.or(package.size()).ok_or_else(|| {
            Error::new(
                eyre!("{}", t!("s9pk.trailer.unknown-size")),
                ErrorKind::ParseS9pk,
            )
        })?;
        let mut source = open_file(path).await?.take(base_len);
        let mut output = AtomicFile::new(path, None::<&Path>).await?;
        let write = async {
            let copied = tokio::io::copy(&mut source, &mut *output).await?;
            ensure_code!(
                copied == base_len,
                ErrorKind::Filesystem,
                "{}",
                t!("s9pk.trailer.edit-refused")
            );
            output.write_all(&region).await?;
            Ok::<_, Error>(())
        }
        .await;
        if let Err(error) = write {
            output.rollback().await?;
            return Err(error);
        }
        output.save().await
    }
}

fn decode_endorsement(value: &[u8]) -> Result<(AnyVerifyingKey, AnySignature), Error> {
    let parsed = || -> Option<_> {
        let length = u64::from_be_bytes(value.get(..8)?.try_into().ok()?);
        let end = 8_usize.checked_add(usize::try_from(length).ok()?)?;
        if length == 0 || end >= value.len() {
            return None;
        }
        let key = AnyVerifyingKey::from_der::<serde::de::value::Error>(value.get(8..end)?).ok()?;
        let signature =
            AnySignature::from_der::<serde::de::value::Error>(value.get(end..)?).ok()?;
        Some((key, signature))
    };
    parsed().ok_or_else(|| {
        Error::new(
            eyre!("{}", t!("s9pk.trailer.invalid-record")),
            ErrorKind::ParseS9pk,
        )
    })
}

fn encode_endorsement(key: &AnyVerifyingKey, signature: &AnySignature) -> Result<Vec<u8>, Error> {
    let key = key
        .to_der::<serde::de::value::Error>()
        .with_kind(ErrorKind::Serialization)?;
    let signature = signature
        .to_der::<serde::de::value::Error>()
        .with_kind(ErrorKind::Serialization)?;
    let mut value = (key.len() as u64).to_be_bytes().to_vec();
    value.extend(key);
    value.extend(signature);
    Ok(value)
}

#[cfg(test)]
mod tests;
