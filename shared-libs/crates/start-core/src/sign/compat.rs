//! Compatibility with clients and registries that label Ed25519ph signatures `id-Ed25519`.

use crate::prelude::*;
use crate::sign::commitment::SignedCommitment;
use crate::sign::ed25519::Ed25519ph;
use crate::sign::{AnySignature, AnyVerifyingKey, SignatureScheme};

/// Returns a verified signature with its canonical algorithm tag.
pub fn verify_commitment<C: SignedCommitment>(
    key: &AnyVerifyingKey,
    commitment: &C,
    context: &str,
    signature: &AnySignature,
) -> Result<AnySignature, Error> {
    key.verify_commitment(commitment, context, signature)
        .map(|()| signature.clone())
        .or_else(|e| match (key, signature) {
            (AnyVerifyingKey::Ed25519(k), AnySignature::Ed25519(s)) => Ed25519ph
                .verify_commitment(k, commitment, context, s)
                .map(|()| AnySignature::Ed25519ph(*s))
                .map_err(|_| e),
            _ => Err(e),
        })
}

/// Relabels stored `id-Ed25519` registry signatures as Ed25519ph.
pub fn relabel_legacy_signatures(value: &mut Value) {
    map_signatures(value, &mut |sig| match sig.as_str()?.parse() {
        Ok(AnySignature::Ed25519(s)) => {
            Some(Value::String(AnySignature::Ed25519ph(s).to_string().into()))
        }
        _ => Some(sig.clone()),
    });
}

/// Rewrites a response for a client that reads `id-Ed25519` as Ed25519ph.
/// Drops every signature such a client cannot verify.
pub fn downgrade_signatures(value: &mut Value) {
    map_signatures(value, &mut |sig| match sig.as_str()?.parse() {
        Ok(AnySignature::Ed25519ph(s)) => {
            Some(Value::String(AnySignature::Ed25519(s).to_string().into()))
        }
        _ => None,
    });
}

fn map_signatures(value: &mut Value, f: &mut impl FnMut(&Value) -> Option<Value>) {
    match value {
        Value::Object(obj) => {
            for (k, v) in obj.iter_mut() {
                match (&**k, v) {
                    ("signatures", Value::Object(sigs)) => {
                        *sigs = sigs
                            .iter()
                            .filter_map(|(k, sig)| Some((k.clone(), f(sig)?)))
                            .collect();
                    }
                    (_, v) => map_signatures(v, f),
                }
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                map_signatures(v, f);
            }
        }
        _ => (),
    }
}

#[cfg(test)]
mod tests {
    use imbl_value::json;

    use super::*;
    use crate::sign::ed25519::Ed25519ph;

    fn key() -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&[5; 32])
    }

    fn ph_signature() -> ed25519_dalek::Signature {
        let commitment = crate::sign::commitment::blake3::Blake3Commitment {
            hash: crate::util::serde::Base64([1; 32]),
            size: 7,
        };
        Ed25519ph
            .sign_commitment(&key(), &commitment, "startos")
            .unwrap()
    }

    fn asset(sig: &AnySignature) -> Value {
        json!({ "x86_64": { "commitment": {}, "signatures": { "signer": sig.to_string() } } })
    }

    #[test]
    fn legacy_db_signatures_become_ed25519ph() {
        let s = ph_signature();
        let mut db = json!({ "index": { "os": { "versions": { "0.4.0.2": { "squashfs": asset(&AnySignature::Ed25519(s)) } } } } });
        relabel_legacy_signatures(&mut db);
        assert_eq!(
            db["index"]["os"]["versions"]["0.4.0.2"]["squashfs"],
            asset(&AnySignature::Ed25519ph(s))
        );
    }

    #[test]
    fn downgrade_relabels_ed25519ph_and_drops_pure() {
        let s = ph_signature();
        let mut res = json!([[{}, {
            "signatures": {
                "file": AnySignature::Ed25519ph(s).to_string(),
                "agent": AnySignature::Ed25519(s).to_string(),
            }
        }]]);
        downgrade_signatures(&mut res);
        assert_eq!(
            res,
            json!([[{}, { "signatures": { "file": AnySignature::Ed25519(s).to_string() } }]])
        );
    }

    fn assert_signature_normalization<C: SignedCommitment>(
        commitment: &C,
        changed: &C,
        context: &str,
    ) {
        use ed25519_dalek::Signer as _;

        use crate::sign::{COMMITMENT_SIG_TAG, signing_message};

        let signing_key = key();
        let key = AnyVerifyingKey::Ed25519(signing_key.verifying_key());
        let wrong_key = AnyVerifyingKey::Ed25519(
            ed25519_dalek::SigningKey::from_bytes(&[9; 32]).verifying_key(),
        );
        let ph = Ed25519ph
            .sign_commitment(&signing_key, commitment, context)
            .unwrap();
        let pure =
            signing_key.sign(&signing_message(COMMITMENT_SIG_TAG, commitment, context).unwrap());
        let legacy = AnySignature::Ed25519(ph);
        assert!(key.verify_commitment(commitment, context, &legacy).is_err());

        for (input, expected) in [
            (legacy, AnySignature::Ed25519ph(ph)),
            (AnySignature::Ed25519ph(ph), AnySignature::Ed25519ph(ph)),
            (AnySignature::Ed25519(pure), AnySignature::Ed25519(pure)),
        ] {
            let normalized = verify_commitment(&key, commitment, context, &input).unwrap();
            assert_eq!(normalized, expected);
            let stored: AnySignature = from_value(to_value(&normalized).unwrap()).unwrap();
            assert_eq!(stored, expected);
            key.verify_commitment(commitment, context, &stored).unwrap();
            assert!(verify_commitment(&key, changed, context, &input).is_err());
            assert!(verify_commitment(&key, commitment, "wrong-context", &input).is_err());
            assert!(verify_commitment(&wrong_key, commitment, context, &input).is_err());
        }
        assert!(
            verify_commitment(&key, commitment, context, &AnySignature::Ed25519ph(pure)).is_err()
        );
    }

    #[test]
    fn os_signatures_normalize_verified_algorithm_tags() {
        use crate::sign::commitment::blake3::Blake3Commitment;
        use crate::util::serde::Base64;

        let commitment = Blake3Commitment {
            hash: Base64([1; 32]),
            size: 7,
        };
        assert_signature_normalization(
            &commitment,
            &Blake3Commitment {
                size: 8,
                ..commitment
            },
            crate::registry::os::SIG_CONTEXT,
        );
        assert_signature_normalization(
            &commitment,
            &Blake3Commitment {
                hash: Base64([2; 32]),
                ..commitment
            },
            crate::registry::os::SIG_CONTEXT,
        );
    }

    #[test]
    fn package_signatures_normalize_verified_algorithm_tags() {
        use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
        use crate::util::serde::Base64;

        let commitment = MerkleArchiveCommitment {
            root_sighash: Base64([1; 32]),
            root_maxsize: 7,
        };
        assert_signature_normalization(
            &commitment,
            &MerkleArchiveCommitment {
                root_maxsize: 8,
                ..commitment
            },
            crate::s9pk::v2::SIG_CONTEXT,
        );
        assert_signature_normalization(
            &commitment,
            &MerkleArchiveCommitment {
                root_sighash: Base64([2; 32]),
                ..commitment
            },
            crate::s9pk::v2::SIG_CONTEXT,
        );
    }
}
