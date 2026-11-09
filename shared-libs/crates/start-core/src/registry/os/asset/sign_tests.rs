use ed25519_dalek::SigningKey;

use super::test_utils::context;
use super::*;
use crate::registry::asset::RegistryAsset;
use crate::registry::os::SIG_CONTEXT;
use crate::registry::os::asset::sign::{SignAssetParams, sign_squashfs};
use crate::registry::os::index::OsVersionInfo;
use crate::registry::signer::SignerInfo;
use crate::sign::commitment::blake3::Blake3Commitment;
use crate::sign::ed25519::Ed25519ph;
use crate::sign::{AnySignature, AnyVerifyingKey, SignatureScheme};
use crate::util::serde::Base64;

const VERSION: &str = "0.4.1.2";
const PLATFORM: &str = "x86_64";

fn verifying(key: &SigningKey) -> AnyVerifyingKey {
    AnyVerifyingKey::Ed25519(key.verifying_key())
}

fn params(key: &SigningKey, commitment: &Blake3Commitment) -> SignAssetParams {
    let signature = AnySignature::Ed25519ph(
        Ed25519ph
            .sign_commitment(key, commitment, SIG_CONTEXT)
            .unwrap(),
    );
    from_value(imbl_value::json!({
        "version": VERSION,
        "platform": PLATFORM,
        "__Auth_signer": verifying(key),
        "signature": signature,
    }))
    .unwrap()
}

async fn signatures(ctx: &RegistryContext) -> Vec<AnyVerifyingKey> {
    let index = ctx.db.peek().await.as_index().as_os().de().unwrap();
    index.versions.0[&VERSION.parse().unwrap()].squashfs[PLATFORM]
        .signatures
        .keys()
        .cloned()
        .collect()
}

#[tokio::test]
async fn squashfs_signing_requires_version_authorization_or_admin() {
    let admin = SigningKey::from_bytes(&[1; 32]);
    let authorized = SigningKey::from_bytes(&[2; 32]);
    let outsider = SigningKey::from_bytes(&[3; 32]);
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path().join("registry"), verifying(&admin)).await;
    let commitment = Blake3Commitment {
        hash: Base64([9; 32]),
        size: 1,
    };
    ctx.db
        .mutate(|db| {
            let mut add_signer = |name: &str, key: &SigningKey| {
                db.as_index_mut().as_signers_mut().add_signer(&SignerInfo {
                    name: name.into(),
                    contact: vec![],
                    keys: [verifying(key)].into(),
                    gpg_exports: Default::default(),
                })
            };
            let authorized_guid = add_signer("Authorized", &authorized)?;
            add_signer("Outsider", &outsider)?;
            let mut info = OsVersionInfo::default();
            info.authorized.insert(authorized_guid);
            info.squashfs.insert(
                PLATFORM.into(),
                RegistryAsset {
                    published_at: Utc::now(),
                    urls: vec![],
                    commitment: commitment.clone(),
                    signatures: Default::default(),
                },
            );
            db.as_index_mut()
                .as_os_mut()
                .as_versions_mut()
                .insert(&VERSION.parse().unwrap(), &info)
        })
        .await
        .result
        .unwrap();

    let rejected = sign_squashfs(ctx.clone(), params(&outsider, &commitment))
        .await
        .unwrap_err();
    assert_eq!(rejected.kind, ErrorKind::Authorization);
    assert!(signatures(&ctx).await.is_empty());

    sign_squashfs(ctx.clone(), params(&authorized, &commitment))
        .await
        .unwrap();
    sign_squashfs(ctx.clone(), params(&admin, &commitment))
        .await
        .unwrap();
    let signed = signatures(&ctx).await;
    assert_eq!(signed.len(), 2);
    assert!(signed.contains(&verifying(&authorized)));
    assert!(signed.contains(&verifying(&admin)));
}
