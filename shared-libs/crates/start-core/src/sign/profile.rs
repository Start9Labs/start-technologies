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
    let p256 = p256::ecdsa::SigningKey::from_slice(&[1; 32]).expect("nonzero scalar");
    let p256_der = AnyVerifyingKey::P256(*p256.verifying_key())
        .to_der::<serde_json::Error>()
        .expect("P-256 SPKI encodes");
    let p256_prefix = |compressed| {
        use der::{Decode, Encode};
        let point = p256.verifying_key().to_sec1_point(compressed);
        let spki = pkcs8::SubjectPublicKeyInfoRef::from_der(&p256_der).expect("P-256 SPKI decodes");
        let mut der = pkcs8::SubjectPublicKeyInfo {
            subject_public_key: der::asn1::BitStringRef::new(0, point.as_bytes())
                .expect("SEC1 point encodes"),
            ..spki
        }
        .to_der()
        .expect("P-256 SPKI encodes");
        der.truncate(der.len() - point.as_bytes().len());
        der
    };
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
        "webauthn": { "algorithm": super::webauthn::WEBAUTHN_ALGORITHM_ID_DER },
        "p256": {
            "keyPrefix": p256_prefix(false),
            "keyBytes": p256.verifying_key().to_sec1_point(false).as_bytes().len(),
            "compressedKeyPrefix": p256_prefix(true),
            "compressedKeyBytes": p256.verifying_key().to_sec1_point(true).as_bytes().len(),
        },
        "signatureBytes": signature.to_bytes().len(),
        "fingerprint": super::fingerprint::wire_profile(),
    })
}
