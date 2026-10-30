use std::collections::HashMap;

use chrono::Utc;
use ed25519_dalek::SigningKey;

use super::*;
use crate::sign::ed25519::Ed25519ph;
use crate::sign::{AnySignature, AnyVerifyingKey, SignatureScheme};
use crate::util::serde::Base64;

fn signed_asset(key: &SigningKey) -> RegistryAsset<Blake3Commitment> {
    let commitment = Blake3Commitment {
        hash: Base64([42; 32]),
        size: 123,
    };
    let signature = Ed25519ph
        .sign_commitment(key, &commitment, SIG_CONTEXT)
        .unwrap();
    RegistryAsset {
        published_at: Utc::now(),
        urls: Vec::new(),
        commitment,
        signatures: HashMap::from([(
            AnyVerifyingKey::Ed25519(key.verifying_key()),
            AnySignature::Ed25519ph(signature),
        )]),
    }
}

fn require(key: &SigningKey) -> Option<AcceptSigners> {
    Some(AcceptSigners::Any(vec![AcceptSigners::Signer(
        AnyVerifyingKey::Ed25519(key.verifying_key()),
    )]))
}

#[test]
fn update_requires_a_valid_accepted_signature() {
    let trusted = SigningKey::from_bytes(&[1; 32]);
    let other = SigningKey::from_bytes(&[2; 32]);
    assert!(validate_update_signatures(&signed_asset(&trusted), require(&trusted)).is_ok());
    assert!(validate_update_signatures(&signed_asset(&other), require(&trusted)).is_err());
    assert!(validate_update_signatures(&signed_asset(&other), None).is_ok());
    assert!(validate_update_signatures(&signed_asset(&other), require(&other)).is_ok());

    let mut tampered = signed_asset(&trusted);
    tampered.commitment.size += 1;
    assert!(validate_update_signatures(&tampered, require(&trusted)).is_err());
    assert!(validate_update_signatures(&tampered, None).is_err());
}

#[test]
fn update_keeps_verifying_every_signature_and_rejecting_unsigned_assets() {
    let trusted = SigningKey::from_bytes(&[1; 32]);
    let other = SigningKey::from_bytes(&[2; 32]);
    let mut asset = signed_asset(&trusted);
    asset.signatures.extend(signed_asset(&other).signatures);
    assert!(validate_update_signatures(&asset, require(&trusted)).is_ok());

    asset.signatures.insert(
        AnyVerifyingKey::Ed25519(other.verifying_key()),
        AnySignature::Ed25519ph(ed25519_dalek::Signature::from_bytes(&[0; 64])),
    );
    assert!(validate_update_signatures(&asset, require(&trusted)).is_err());
    assert!(validate_update_signatures(&asset, None).is_err());

    asset.signatures.clear();
    assert!(validate_update_signatures(&asset, require(&trusted)).is_err());
    assert!(validate_update_signatures(&asset, None).is_err());
}

#[test]
fn diagnostic_updates_reject_other_signers_and_unsigned_assets() {
    let key = SigningKey::from_bytes(&[2; 32]);
    let mut asset = signed_asset(&key);
    assert!(validate_update_signatures(&asset, None).is_ok());
    assert!(super::diagnostic::validate_signatures(&asset, false).is_err());
    assert!(super::diagnostic::validate_signatures(&asset, true).is_ok());
    asset.signatures.clear();
    assert!(super::diagnostic::validate_signatures(&asset, false).is_err());
    assert!(super::diagnostic::validate_signatures(&asset, true).is_err());
}

#[test]
fn update_signer_policy_round_trips_and_override_requires_explicit_trust() {
    let mut params: UpdateSystemParams = serde_json::from_value(serde_json::json!({
        "registry": "https://registry.test",
        "targetVersion": "=0.4.0.4"
    }))
    .unwrap();
    assert!(params.accept_signers.is_none());
    assert!(!params.trust);

    let key = SigningKey::from_bytes(&[1; 32]);
    params.accept_signers = require(&key);
    let decoded: UpdateSystemParams =
        serde_json::from_value(serde_json::to_value(params).unwrap()).unwrap();
    assert!(validate_update_signatures(&signed_asset(&key), decoded.accept_signers).is_ok());
}
