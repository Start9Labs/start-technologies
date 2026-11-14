use super::commitment::SignedCommitment;
use super::commitment::request::RequestCommitment;
use super::{AnySignature, AnyVerifyingKey, COMMITMENT_SIG_TAG};
use crate::util::serde::DerEncoding;

/// Signature wire constants projected from the native encoding owners.
pub fn browser_profile() -> serde_json::Value {
    let key = ed25519_dalek::SigningKey::from_bytes(&[0; 32]).verifying_key();
    let key_len = key.as_bytes().len();
    let mut key_prefix = AnyVerifyingKey::Ed25519(key)
        .to_der::<serde_json::Error>()
        .expect("Ed25519 SPKI encodes");
    key_prefix.truncate(key_prefix.len() - key_len);
    let signature = ed25519_dalek::Signature::from_bytes(&[0; 64]);
    let prefix = |value: AnySignature| {
        let mut der = value
            .to_der::<serde_json::Error>()
            .expect("signature encodes");
        der.truncate(der.len() - signature.to_bytes().len());
        der
    };
    serde_json::json!({
        "commitmentTag": COMMITMENT_SIG_TAG,
        "requestType": RequestCommitment::TYPE_ID,
        "requestName": RequestCommitment::TYPE_NAME,
        "ed25519": {
            "keyPrefix": key_prefix, "keyBytes": key_len,
            "signaturePrefix": prefix(AnySignature::Ed25519(signature)),
        },
        "ed25519ph": { "signaturePrefix": prefix(AnySignature::Ed25519ph(signature)) },
        "signatureBytes": signature.to_bytes().len(),
        "fingerprint": super::fingerprint::wire_profile(),
    })
}
