use std::collections::HashMap;

use ed25519_dalek::SigningKey;

use super::*;
use crate::registry::asset::RegistryAsset;
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::sign::ed25519::Ed25519ph;
use crate::sign::{AnySignature, AnyVerifyingKey, SignatureScheme};
use crate::trusted_keys::{TrustedKey, TrustedKeys, enabled_keys};
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
    let id = public.interned_der64();
    let mut keys = TrustedKeys::from([(
        id.clone(),
        TrustedKey {
            name: "custom".into(),
            origin: "manual".into(),
            origin_registry: None,
            removable: true,
            public_key: public,
            gpg_exports: Default::default(),
            contact: Default::default(),
            added_at: chrono::Utc::now(),
            marketplace: false,
            os: true,
        },
    )]);
    let policy =
        || trusted_keys::signer_policy(Some(enabled_keys(&keys, TrustContext::Marketplace)));
    assert!(asset.validate_all(SIG_CONTEXT, policy()).is_err());
    assert!(asset.validate_all(SIG_CONTEXT, None).is_ok());
    keys.get_mut(&id).unwrap().marketplace = true;
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
        keys.get_mut(&signer.interned_der64()).unwrap().marketplace = true;
        asset
            .validate_all(
                SIG_CONTEXT,
                trusted_keys::signer_policy(Some(enabled_keys(&keys, TrustContext::Marketplace))),
            )
            .unwrap();
        assert!(!keys[&signer.interned_der64()].os);
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

#[tokio::test]
async fn package_request_pins_use_update_policy_and_remain_required_with_override() {
    use crate::registry::signer::AcceptSigners;
    use crate::s9pk::load_with_signers;

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata/s9pk-trailer/endorsed.s9pk");
    let package = crate::s9pk::S9pk::open(&path, None).await.unwrap();
    let signatures = package.signatures().await.unwrap();
    let embedded = AnyVerifyingKey::Ed25519(SigningKey::from_bytes(&[7; 32]).verifying_key());
    let endorsement = AnyVerifyingKey::Ed25519(SigningKey::from_bytes(&[8; 32]).verifying_key());
    let unrelated = AnyVerifyingKey::Ed25519(SigningKey::from_bytes(&[1; 32]).verifying_key());
    let asset = RegistryAsset {
        published_at: chrono::Utc::now(),
        urls: vec![],
        commitment: package.as_archive().commitment().await.unwrap(),
        signatures,
    };
    let source = std::sync::Arc::<[u8]>::from(tokio::fs::read(&path).await.unwrap());
    for (server, client, expected) in [
        (None, None, true),
        (None, Some(vec![]), false),
        (None, Some(vec![endorsement.clone()]), true),
        (None, Some(vec![unrelated.clone()]), false),
        (Some(vec![]), Some(vec![endorsement.clone()]), false),
        (
            Some(vec![embedded.clone()]),
            Some(vec![endorsement.clone()]),
            true,
        ),
        (Some(vec![embedded]), Some(vec![unrelated]), false),
    ] {
        let server = trusted_keys::signer_policy(server);
        let client = trusted_keys::signer_policy(client);
        let registry_result = asset
            .validate_all(SIG_CONTEXT, server.clone())
            .and_then(|()| {
                if let Some(policy) = client.clone() {
                    asset.validate(SIG_CONTEXT, policy).map(|_| ())
                } else {
                    Ok(())
                }
            });
        assert_eq!(registry_result.is_ok(), expected);
        let policy = match (server, client) {
            (Some(server), Some(client)) => Some(AcceptSigners::All(vec![server, client])),
            (server, client) => server.or(client),
        };
        assert_eq!(
            load_with_signers(source.clone(), || panic!("v2"), None, policy)
                .await
                .is_ok(),
            expected
        );
    }
    for value in [
        serde_json::json!({}),
        serde_json::json!({"acceptSigners": null}),
    ] {
        let params: SideloadParams = serde_json::from_value(value).unwrap();
        assert!(params.accept_signers.is_none());
        assert!(!params.trust);
    }
    let wire = serde_json::json!({"trust":true,"acceptSigners":{"any":[{"signer":endorsement}]}});
    let params: SideloadParams = serde_json::from_value(wire.clone()).unwrap();
    assert!(params.trust);
    assert_eq!(
        serde_json::to_value(params.accept_signers).unwrap(),
        wire["acceptSigners"]
    );
    let pins = local_sideload_policy(&path).await.unwrap().unwrap();
    asset.validate(SIG_CONTEXT, pins).unwrap();
    let mut install = wire;
    install["registry"] = serde_json::json!("https://example.com");
    install["id"] = serde_json::json!("trust-fixture");
    install["version"] = serde_json::json!("1.0.0:0");
    let params: InstallParams = serde_json::from_value(install).unwrap();
    assert!(params.accept_signers.is_some());
}
