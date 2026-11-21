use super::AnyVerifyingKey;

const PREFIX: &str = "SHA256:";
const ALGORITHM: &str = "SHA-256";

/// SHA-256 fingerprint of the raw public key, an uncompressed SEC1 point for P-256.
pub fn fingerprint(key: &AnyVerifyingKey) -> String {
    use sha2::Digest;
    let digest = match key {
        AnyVerifyingKey::Ed25519(key) => sha2::Sha256::digest(key.as_bytes()),
        AnyVerifyingKey::P256(key) => sha2::Sha256::digest(key.to_sec1_point(false).as_bytes()),
    };
    format!("{PREFIX}{}", hex::encode(digest))
}

#[test]
fn identity_fingerprint_matches_the_published_ed25519_fixture() {
    let key = ed25519_dalek::SigningKey::from_bytes(&[21; 32]).verifying_key();
    assert_eq!(
        fingerprint(&AnyVerifyingKey::Ed25519(key)),
        "SHA256:d7e1e084be213b01e506852af8198b9980b43e03749c67d82dcd0903f4001c77",
    );
}

pub(crate) fn wire_profile() -> serde_json::Value {
    serde_json::json!({ "algorithm": ALGORITHM, "prefix": PREFIX })
}
