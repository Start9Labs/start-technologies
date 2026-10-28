use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use sha2::Sha512;

use crate::prelude::*;
use crate::sign::SignatureScheme;

/// DER `AlgorithmIdentifier` for OID 2.25.31184504503259998342399313009964778715, absent parameters.
/// Hand-encoded: `const-oid` rejects arcs wider than 32 bits.
pub(super) const ED25519PH_ALGORITHM_ID_DER: &[u8] = &[
    0x30, 0x15, 0x06, 0x13, 0x69, 0xae, 0xf5, 0xf5, 0xb1, 0xe7, 0x89, 0x8a, 0x85, 0xc9, 0x9c, 0x9a,
    0xe3, 0xbb, 0x85, 0xfb, 0xf2, 0x89, 0x5b,
];

/// Ed25519ph (RFC 8032) with the signing context.
pub struct Ed25519ph;
impl SignatureScheme for Ed25519ph {
    type SigningKey = SigningKey;
    type VerifyingKey = VerifyingKey;
    type Signature = Signature;
    type Digest = Sha512;
    fn new_digest(&self) -> Self::Digest {
        <Self::Digest as digest::Digest>::new()
    }
    fn sign(
        &self,
        key: &Self::SigningKey,
        digest: Self::Digest,
        context: &str,
    ) -> Result<Self::Signature, Error> {
        Ok(key.sign_prehashed(digest, Some(context.as_bytes()))?)
    }
    fn verify(
        &self,
        key: &Self::VerifyingKey,
        digest: Self::Digest,
        context: &str,
        signature: &Self::Signature,
    ) -> Result<(), Error> {
        key.verify_prehashed_strict(digest, Some(context.as_bytes()), signature)?;
        Ok(())
    }
}
