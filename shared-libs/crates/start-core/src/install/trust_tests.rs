use std::collections::HashMap;

use ed25519_dalek::SigningKey;

use super::*;
use crate::registry::asset::RegistryAsset;
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::sign::ed25519::Ed25519ph;
use crate::sign::{AnySignature, AnyVerifyingKey, SignatureScheme};
use crate::trusted_keys::{TrustedKeyring, TrustedKeys, enabled_keys};
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
        TrustedKeyring {
            name: "custom".into(),
            removable: true,
            public_keys: std::collections::BTreeSet::from([public]),
            gpg_exports: Default::default(),
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
fn production_and_community_build_signers_require_explicit_alpha_opt_in() {
    for (fixture, signer) in [
        (
            include_str!("fixtures/production-bitcoind-31.1-26.json"),
            "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAGaAYxsY7iO5GyQrCA5vcLWYIQyoKImp0du4WWvIzr2Y=\n-----END PUBLIC KEY-----\n",
        ),
        (
            include_str!("fixtures/community-bitcoind-knotsprerdts-29.3-31.json"),
            "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA4P+LwrLkZfhgcM3MNz6g8ihBvNnIfGSu+ojr+lFoDxU=\n-----END PUBLIC KEY-----\n",
        ),
    ] {
        let mut asset: RegistryAsset<MerkleArchiveCommitment> =
            serde_json::from_str(fixture).unwrap();
        asset.validate_all(SIG_CONTEXT, None).unwrap();
        let signer: AnyVerifyingKey = signer.parse().unwrap();
        asset.signatures.retain(|key, _| key == &signer);
        assert_eq!(asset.signatures.len(), 1);
        let mut keys = trusted_keys::initial_trusted_keys();
        assert!(
            asset
                .validate_all(
                    SIG_CONTEXT,
                    trusted_keys::signer_policy(Some(enabled_keys(
                        &keys,
                        TrustContext::Marketplace
                    )))
                )
                .is_err()
        );
        keys.get_mut("start9-alpha").unwrap().marketplace = true;
        asset
            .validate_all(
                SIG_CONTEXT,
                trusted_keys::signer_policy(Some(enabled_keys(&keys, TrustContext::Marketplace))),
            )
            .unwrap();
        assert!(!keys["start9-alpha"].os);
    }
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
