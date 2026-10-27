use std::collections::HashMap;

use ed25519_dalek::SigningKey;

use super::*;
use crate::registry::asset::RegistryAsset;
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::sign::ed25519::Ed25519ph;
use crate::sign::{AnySignature, AnyVerifyingKey, SignatureScheme};
use crate::update::trusted_keys::{TrustedKey, TrustedKeys, enabled_keys};
use crate::util::serde::Base64;

#[test]
fn package_commitment_requires_an_enabled_marketplace_signer() {
    let key = SigningKey::from_bytes(&[9; 32]);
    let public = AnyVerifyingKey::Ed25519(key.verifying_key());
    let commitment = MerkleArchiveCommitment {
        root_sighash: Base64([8; 32]),
        root_maxsize: 123,
    };
    let signature = Ed25519ph
        .sign_commitment(&key, &commitment, SIG_CONTEXT)
        .unwrap();
    let mut asset = RegistryAsset {
        published_at: chrono::Utc::now(),
        urls: vec![],
        commitment,
        signatures: HashMap::from([(public.clone(), AnySignature::Ed25519ph(signature))]),
    };
    let mut keys = TrustedKeys::from([(
        "custom".into(),
        TrustedKey {
            name: "custom".into(),
            removable: true,
            public_keys: vec![public],
            marketplace: false,
            os: true,
        },
    )]);
    let policy =
        || trusted_keys::signer_policy(Some(enabled_keys(&keys, TrustContext::Marketplace)));
    assert!(asset.validate_all(SIG_CONTEXT, policy()).is_err());
    assert!(asset.validate_all(SIG_CONTEXT, None).is_ok());
    keys.get_mut("custom").unwrap().marketplace = true;
    assert!(
        asset
            .validate_all(
                SIG_CONTEXT,
                trusted_keys::signer_policy(Some(enabled_keys(&keys, TrustContext::Marketplace)))
            )
            .is_ok()
    );
    asset.commitment.root_maxsize += 1;
    assert!(asset.validate_all(SIG_CONTEXT, None).is_err());
    asset.signatures.clear();
    assert!(asset.validate_all(SIG_CONTEXT, None).is_err());
}

#[test]
fn cli_override_is_explicit_for_registry_and_sideload() {
    for (arguments, expected) in [
        (vec!["install", "hello-world"], false),
        (vec!["install", "hello-world", "--trust"], true),
        (vec!["install", "--sideload", "package.s9pk"], false),
        (
            vec!["install", "--sideload", "package.s9pk", "--trust"],
            true,
        ),
    ] {
        let matches = CliInstallParams::command()
            .try_get_matches_from(arguments)
            .unwrap();
        let params = CliInstallParams::from_arg_matches(&matches).unwrap();
        let trust = match params {
            CliInstallParams::Marketplace { trust, .. }
            | CliInstallParams::Sideload { trust, .. } => trust,
        };
        assert_eq!(trust, expected);
    }
    let params: InstallParams = serde_json::from_value(serde_json::json!({ "registry": "https://example.com", "id": "hello-world", "version": "0.1.0:0" })).unwrap();
    assert!(!params.trust);
    let params: SideloadParams = serde_json::from_value(serde_json::json!({})).unwrap();
    assert!(!params.trust);
}
