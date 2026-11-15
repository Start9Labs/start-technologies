use super::AnyVerifyingKey;

const PREFIX: &str = "SHA256:";
const ALGORITHM: &str = "SHA-256";

/// SHA-256 fingerprint of the raw Ed25519 identity public key.
pub fn fingerprint(key: &AnyVerifyingKey) -> String {
    use sha2::Digest;
    let AnyVerifyingKey::Ed25519(key) = key;
    format!(
        "{PREFIX}{}",
        hex::encode(sha2::Sha256::digest(key.as_bytes()))
    )
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
