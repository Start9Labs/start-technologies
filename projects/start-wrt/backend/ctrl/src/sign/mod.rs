pub mod commitment;
pub mod ed25519;

use std::fmt::Display;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use ::ed25519_dalek::pkcs8::spki::AlgorithmIdentifierRef;
use serde::{Deserialize, Serialize};
use sha2::Sha512;

use crate::Error;

use self::commitment::Digestable;
use self::ed25519::Ed25519;

// ── SignatureScheme trait ─────────────────────────────────────────────

pub trait SignatureScheme {
    type VerifyingKey;
    type Signature;
    type Digest: digest::Update;

    fn new_digest(&self) -> Self::Digest;

    fn verify(
        &self,
        key: &Self::VerifyingKey,
        digest: Self::Digest,
        context: &str,
        signature: &Self::Signature,
    ) -> Result<(), Error>;

    fn verify_commitment<C: Digestable>(
        &self,
        key: &Self::VerifyingKey,
        commitment: &C,
        context: &str,
        signature: &Self::Signature,
    ) -> Result<(), Error> {
        let mut digest = self.new_digest();
        commitment.update(&mut digest);
        self.verify(key, digest, context, signature)
    }
}

// ── AnyScheme ────────────────────────────────────────────────────────

#[non_exhaustive]
pub enum AnyScheme {
    Ed25519(Ed25519),
}

impl SignatureScheme for AnyScheme {
    type VerifyingKey = AnyVerifyingKey;
    type Signature = AnySignature;
    type Digest = AnyDigest;

    fn new_digest(&self) -> Self::Digest {
        match self {
            Self::Ed25519(s) => AnyDigest::Sha512(s.new_digest()),
        }
    }

    fn verify(
        &self,
        key: &Self::VerifyingKey,
        digest: Self::Digest,
        context: &str,
        signature: &Self::Signature,
    ) -> Result<(), Error> {
        match (self, key, digest, signature) {
            (
                Self::Ed25519(s),
                AnyVerifyingKey::Ed25519(key),
                AnyDigest::Sha512(digest),
                AnySignature::Ed25519(signature),
            ) => s.verify(key, digest, context, signature),
            _ => Err(Error::other("mismatched signature algorithm")),
        }
    }
}

// ── AnyDigest ────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum AnyDigest {
    Sha512(Sha512),
}

impl digest::Update for AnyDigest {
    fn update(&mut self, data: &[u8]) {
        match self {
            Self::Sha512(d) => digest::Update::update(d, data),
        }
    }
}

// ── AnyVerifyingKey ──────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AnyVerifyingKey {
    Ed25519(ed25519_dalek::VerifyingKey),
}

impl AnyVerifyingKey {
    pub fn scheme(&self) -> AnyScheme {
        match self {
            Self::Ed25519(_) => AnyScheme::Ed25519(Ed25519),
        }
    }
}

impl Hash for AnyVerifyingKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Ed25519(k) => {
                state.write_u8(0); // discriminant
                k.as_bytes().hash(state);
            }
        }
    }
}

impl FromStr for AnyVerifyingKey {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use pkcs8::DecodePublicKey;
        let key = ed25519_dalek::VerifyingKey::from_public_key_pem(s)
            .map_err(|e| Error::other(format!("invalid SPKI PEM verifying key: {e}")))?;
        Ok(Self::Ed25519(key))
    }
}

impl Display for AnyVerifyingKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use pkcs8::EncodePublicKey;
        match self {
            Self::Ed25519(k) => {
                let pem = k
                    .to_public_key_pem(pkcs8::LineEnding::LF)
                    .map_err(|_| std::fmt::Error)?;
                f.write_str(&pem)
            }
        }
    }
}

impl<'de> Deserialize<'de> for AnyVerifyingKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Serialize for AnyVerifyingKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ── AnySignature ─────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnySignature {
    Ed25519(ed25519_dalek::Signature),
}

impl FromStr for AnySignature {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use der::DecodePem;

        #[derive(der::Sequence)]
        struct AnySignatureDer {
            alg: pkcs8::spki::AlgorithmIdentifierOwned,
            sig: der::asn1::OctetString,
        }
        impl der::pem::PemLabel for AnySignatureDer {
            const PEM_LABEL: &'static str = "SIGNATURE";
        }

        let der = AnySignatureDer::from_pem(s.as_bytes())
            .map_err(|e| Error::other(format!("invalid PEM signature: {e}")))?;

        let ed25519_alg_id: AlgorithmIdentifierRef<'_> = ed25519_dalek::pkcs8::ALGORITHM_ID;
        if der.alg.oid == ed25519_alg_id.oid {
            let sig = ed25519_dalek::Signature::from_slice(der.sig.as_bytes())
                .map_err(|e| Error::other(format!("invalid ed25519 signature: {e}")))?;
            Ok(Self::Ed25519(sig))
        } else {
            Err(Error::other(format!(
                "unknown signature algorithm OID: {}",
                der.alg.oid
            )))
        }
    }
}

impl Display for AnySignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use der::EncodePem;

        #[derive(der::Sequence)]
        struct AnySignatureDer<'a> {
            alg: AlgorithmIdentifierRef<'a>,
            sig: der::asn1::OctetString,
        }
        impl<'a> der::pem::PemLabel for AnySignatureDer<'a> {
            const PEM_LABEL: &'static str = "SIGNATURE";
        }

        let der = match self {
            Self::Ed25519(s) => AnySignatureDer {
                alg: ed25519_dalek::pkcs8::ALGORITHM_ID,
                sig: der::asn1::OctetString::new(s.to_bytes()).map_err(|_| std::fmt::Error)?,
            },
        };
        f.write_str(
            &der.to_pem(der::pem::LineEnding::LF)
                .map_err(|_| std::fmt::Error)?,
        )
    }
}

impl<'de> Deserialize<'de> for AnySignature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Serialize for AnySignature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Derive an `AnyVerifyingKey` for tests without depending on the
    /// (now-removed) `AnySigningKey` wrapper. We still need to generate
    /// a private key to extract a public key from it.
    fn test_verifying_key() -> AnyVerifyingKey {
        let dalek = ed25519_dalek::SigningKey::generate(&mut rand_core_06::OsRng);
        AnyVerifyingKey::Ed25519(dalek.verifying_key())
    }

    #[test]
    fn test_verifying_key_pem_roundtrip() {
        let vk = test_verifying_key();
        let pem = vk.to_string();
        let vk2 = AnyVerifyingKey::from_str(&pem).unwrap();
        assert_eq!(vk, vk2);
    }

    #[test]
    fn test_verifying_key_json_roundtrip() {
        let vk = test_verifying_key();
        let json = serde_json::to_string(&vk).unwrap();
        let vk2: AnyVerifyingKey = serde_json::from_str(&json).unwrap();
        assert_eq!(vk, vk2);
    }
}
