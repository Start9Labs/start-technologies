use ed25519_dalek::{Signature, VerifyingKey};
use sha2::Sha512;

use super::SignatureScheme;
use crate::Error;

pub struct Ed25519;

impl SignatureScheme for Ed25519 {
    type VerifyingKey = VerifyingKey;
    type Signature = Signature;
    type Digest = Sha512;

    fn new_digest(&self) -> Self::Digest {
        <Self::Digest as digest::Digest>::new()
    }

    fn verify(
        &self,
        key: &Self::VerifyingKey,
        digest: Self::Digest,
        context: &str,
        signature: &Self::Signature,
    ) -> Result<(), Error> {
        key.verify_prehashed_strict(digest, Some(context.as_bytes()), signature)
            .map_err(|e| Error::other(format!("ed25519 verify failed: {e}")))
    }
}
