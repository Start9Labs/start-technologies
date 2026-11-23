use std::pin::Pin;
use std::task::{Context, Poll, ready};

use aes::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use zeroize::Zeroizing;

use crate::prelude::*;
use crate::s9pk::merkle_archive::source::ArchiveSource;
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::sign::{AnySignature, Signer};
use crate::util::serde::Base64;

type Aes256Ctr128 = ctr::Ctr128BE<aes::Aes256>;

const DERIVATION_MESSAGE_PREFIX: &[u8] = b"start9 s9pk keystream v1\0";
const DERIVATION_INFO: &[u8] = b"s9pk keystream";

/// An AES-256-CTR keystream with a 128-bit big-endian counter starting at `iv`.
///
/// Ciphertext has the plaintext's length, and any byte range decrypts on its own.
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct KeyStreamInfo {
    pub key: Base64<[u8; 32]>,
    pub iv: Base64<[u8; 16]>,
}

rpc_toolkit::reflect_ts!(KeyStreamInfo);
rpc_toolkit::ts_export!(KeyStreamInfo, namespaces = [""]);

impl std::fmt::Debug for KeyStreamInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyStreamInfo").finish_non_exhaustive()
    }
}

impl KeyStreamInfo {
    /// The keystream an identity encrypts an archive with. Deterministic per identity and archive.
    pub async fn derive(
        signer: &Signer,
        commitment: &MerkleArchiveCommitment,
    ) -> Result<Self, Error> {
        let message = [
            DERIVATION_MESSAGE_PREFIX,
            &*commitment.root_sighash,
            &commitment.root_maxsize.to_be_bytes(),
        ]
        .concat();
        let signature = Zeroizing::new(match signer.sign_message(&message).await? {
            AnySignature::Ed25519(s) | AnySignature::Ed25519ph(s) => s.to_bytes(),
        });
        let mut okm = Zeroizing::new([0u8; 48]);
        Hkdf::<Sha256>::new(None, &*signature)
            .expand(DERIVATION_INFO, &mut *okm)
            .map_err(|_| Error::new(eyre!("HKDF output length"), ErrorKind::Unknown))?;
        let mut key = [0u8; 32];
        let mut iv = [0u8; 16];
        key.copy_from_slice(&okm[..32]);
        iv.copy_from_slice(&okm[32..]);
        Ok(Self {
            key: Base64(key),
            iv: Base64(iv),
        })
    }

    fn cipher_at(&self, position: u64) -> Result<Aes256Ctr128, Error> {
        let mut cipher = Aes256Ctr128::new_from_slices(&*self.key, &*self.iv)
            .map_err(|_| Error::new(eyre!("invalid keystream length"), ErrorKind::Unknown))?;
        cipher.try_seek(position).map_err(|_| {
            Error::new(
                eyre!("{}", t!("s9pk.source.invalid-range")),
                ErrorKind::ParseS9pk,
            )
        })?;
        Ok(cipher)
    }
}

/// Applies a keystream to the bytes of a reader as they flow.
pub struct KeyStreamReader<R> {
    inner: R,
    cipher: Option<Aes256Ctr128>,
}

impl<R: AsyncRead + Unpin> AsyncRead for KeyStreamReader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let start = buf.filled().len();
        ready!(Pin::new(&mut this.inner).poll_read(cx, buf))?;
        if let Some(cipher) = &mut this.cipher {
            cipher
                .try_apply_keystream(&mut buf.filled_mut()[start..])
                .map_err(|_| std::io::Error::other("keystream exhausted"))?;
        }
        Poll::Ready(Ok(()))
    }
}

/// An archive source read through a keystream: ciphertext reads as plaintext and plaintext as
/// ciphertext.
///
/// Passes bytes through unchanged without a keystream.
pub struct KeyStreamSource<S> {
    inner: S,
    key: Option<KeyStreamInfo>,
}

impl<S> KeyStreamSource<S> {
    pub fn new(inner: S, key: Option<KeyStreamInfo>) -> Self {
        Self { inner, key }
    }
    /// The underlying source, which yields ciphertext for an encrypted archive.
    pub fn inner(&self) -> &S {
        &self.inner
    }
    pub fn is_encrypted(&self) -> bool {
        self.key.is_some()
    }
    fn reader<R>(&self, inner: R, position: u64) -> Result<KeyStreamReader<R>, Error> {
        Ok(KeyStreamReader {
            inner,
            cipher: self
                .key
                .as_ref()
                .map(|key| key.cipher_at(position))
                .transpose()?,
        })
    }
}

impl<S: ArchiveSource> ArchiveSource for KeyStreamSource<S> {
    type FetchReader = KeyStreamReader<S::FetchReader>;
    type FetchAllReader = KeyStreamReader<S::FetchAllReader>;
    async fn size(&self) -> Option<u64> {
        self.inner.size().await
    }
    async fn fetch_all(&self) -> Result<Self::FetchAllReader, Error> {
        self.reader(self.inner.fetch_all().await?, 0)
    }
    async fn fetch(&self, position: u64, size: u64) -> Result<Self::FetchReader, Error> {
        self.reader(self.inner.fetch(position, size).await?, position)
    }
    async fn copy_all_to<W: AsyncWrite + Unpin + Send + ?Sized>(
        &self,
        w: &mut W,
    ) -> Result<(), Error> {
        if self.key.is_none() {
            return self.inner.copy_all_to(w).await;
        }
        tokio::io::copy(&mut self.fetch_all().await?, w).await?;
        Ok(())
    }
    async fn copy_to<W: AsyncWrite + Unpin + Send + ?Sized>(
        &self,
        position: u64,
        size: u64,
        w: &mut W,
    ) -> Result<(), Error> {
        if self.key.is_none() {
            return self.inner.copy_to(position, size, w).await;
        }
        tokio::io::copy(&mut self.fetch(position, size).await?, w).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::io::AsyncReadExt;

    use super::*;
    use crate::sign::AnySigningKey;
    use crate::util::io::open_file;

    fn signer(seed: u8) -> Signer {
        Signer::Key(AnySigningKey::Ed25519(
            ed25519_dalek::SigningKey::from_bytes(&[seed; 32]),
        ))
    }

    fn commitment(seed: u8) -> MerkleArchiveCommitment {
        MerkleArchiveCommitment {
            root_sighash: Base64([seed; 32]),
            root_maxsize: 4096,
        }
    }

    #[tokio::test]
    async fn derivation_is_deterministic_per_identity_and_archive() {
        let key = KeyStreamInfo::derive(&signer(1), &commitment(1))
            .await
            .unwrap();
        assert_eq!(
            key,
            KeyStreamInfo::derive(&signer(1), &commitment(1))
                .await
                .unwrap()
        );
        assert_ne!(
            key,
            KeyStreamInfo::derive(&signer(1), &commitment(2))
                .await
                .unwrap()
        );
        assert_ne!(
            key,
            KeyStreamInfo::derive(&signer(2), &commitment(1))
                .await
                .unwrap()
        );
        let mut longer = commitment(1);
        longer.root_maxsize += 1;
        assert_ne!(
            key,
            KeyStreamInfo::derive(&signer(1), &longer).await.unwrap()
        );
    }

    #[tokio::test]
    async fn ranged_reads_of_ciphertext_decrypt_at_any_offset() {
        let key = KeyStreamInfo::derive(&signer(3), &commitment(3))
            .await
            .unwrap();
        let plaintext: Vec<u8> = (0..1000u32).map(|i| (i * 7 + i / 13) as u8).collect();
        let mut ciphertext = plaintext.clone();
        key.cipher_at(0).unwrap().apply_keystream(&mut ciphertext);
        assert_eq!(ciphertext.len(), plaintext.len());
        assert_ne!(ciphertext, plaintext);

        let source = KeyStreamSource::new(Arc::<[u8]>::from(ciphertext.clone()), Some(key));
        for (position, size) in [
            (0, 1000),
            (0, 1),
            (1, 15),
            (15, 2),
            (16, 16),
            (17, 100),
            (31, 33),
            (500, 499),
            (999, 1),
        ] {
            let mut read = Vec::new();
            source
                .fetch(position, size)
                .await
                .unwrap()
                .read_to_end(&mut read)
                .await
                .unwrap();
            let range = position as usize..(position + size) as usize;
            assert_eq!(read, plaintext[range.clone()], "fetch {range:?}");

            let mut copied = Vec::new();
            source.copy_to(position, size, &mut copied).await.unwrap();
            assert_eq!(copied, plaintext[range.clone()], "copy {range:?}");
        }
        let mut all = Vec::new();
        source
            .fetch_all()
            .await
            .unwrap()
            .read_to_end(&mut all)
            .await
            .unwrap();
        assert_eq!(all, plaintext);

        let plain = KeyStreamSource::new(Arc::<[u8]>::from(plaintext.clone()), None);
        let mut read = Vec::new();
        plain
            .fetch(17, 40)
            .await
            .unwrap()
            .read_to_end(&mut read)
            .await
            .unwrap();
        assert_eq!(read, plaintext[17..57]);
    }

    #[tokio::test]
    async fn a_file_encrypted_through_the_source_decrypts_through_it() {
        use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;

        let key = KeyStreamInfo::derive(&signer(4), &commitment(4))
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let plaintext: Vec<u8> = (0..300u32).map(|i| i as u8).collect();
        tokio::fs::write(dir.path().join("plain"), &plaintext)
            .await
            .unwrap();
        let mut ciphertext = Vec::new();
        KeyStreamSource::new(
            MultiCursorFile::from(open_file(dir.path().join("plain")).await.unwrap()),
            Some(key.clone()),
        )
        .copy_all_to(&mut ciphertext)
        .await
        .unwrap();
        assert_eq!(ciphertext.len(), plaintext.len());
        assert_ne!(ciphertext, plaintext);
        let mut read = Vec::new();
        KeyStreamSource::new(Arc::<[u8]>::from(ciphertext), Some(key))
            .fetch(33, 200)
            .await
            .unwrap()
            .read_to_end(&mut read)
            .await
            .unwrap();
        assert_eq!(read, plaintext[33..233]);
    }
}
