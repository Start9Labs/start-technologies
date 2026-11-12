use ed25519_dalek::{Signer as _, SigningKey};

use super::test_utils::{context, registry_server};
use super::*;
use crate::context::config::ClientConfig;
use crate::developer::write_signing_key;
use crate::registry::asset::RegistryAsset;
use crate::registry::os::SIG_CONTEXT;
use crate::registry::os::asset::add::{AddAssetParams, add_iso};
use crate::registry::os::index::OsVersionInfo;
use crate::registry::os::promote::{CliOsPromoteParams, cli_os_promote};
use crate::registry::os::version::{AddVersionParams, add_version};
use crate::rpc_continuations::Guid;
use crate::sign::commitment::blake3::Blake3Commitment;
use crate::sign::ed25519::Ed25519ph;
use crate::sign::{
    AnySignature, COMMITMENT_SIG_TAG, SignatureMap, SignatureScheme, signing_message,
};
use crate::util::serde::Base64;

fn commitment() -> Blake3Commitment {
    Blake3Commitment {
        hash: Base64([3; 32]),
        size: 123,
    }
}

fn signature(key: &SigningKey, commitment: &Blake3Commitment) -> AnySignature {
    AnySignature::Ed25519ph(
        Ed25519ph
            .sign_commitment(key, commitment, SIG_CONTEXT)
            .unwrap(),
    )
}

fn params(uploader: &SigningKey, signatures: SignatureMap) -> AddAssetParams {
    let commitment = commitment();
    AddAssetParams {
        version: "0.4.0.5".parse().unwrap(),
        platform: "x86_64".into(),
        url: "https://example.com/os.iso".parse().unwrap(),
        signer: AnyVerifyingKey::Ed25519(uploader.verifying_key()),
        signature: signature(uploader, &commitment),
        signatures,
        commitment,
    }
}

#[tokio::test]
async fn os_promotion_preserves_all_asset_signatures_and_destination_endorsements() {
    let uploader = SigningKey::from_bytes(&[12; 32]);
    let uploader_key = AnyVerifyingKey::Ed25519(uploader.verifying_key());
    let temp = tempfile::tempdir().unwrap();
    let source = context(temp.path().join("source"), uploader_key.clone()).await;
    let target = context(temp.path().join("target"), uploader_key.clone()).await;
    let first = SigningKey::from_bytes(&[7; 32]);
    let second = SigningKey::from_bytes(&[8; 32]);
    let destination_signer = SigningKey::from_bytes(&[9; 32]);
    let commitment = commitment();
    let asset =
        RegistryAsset {
            published_at: chrono::Utc::now(),
            urls: vec![
                "https://example.com/os.iso".parse().unwrap(),
                "https://mirror.example.com/os.iso".parse().unwrap(),
            ],
            commitment: commitment.clone(),
            signatures: SignatureMap::from([
                (
                    AnyVerifyingKey::Ed25519(first.verifying_key()),
                    AnySignature::Ed25519(first.sign(
                        &signing_message(COMMITMENT_SIG_TAG, &commitment, SIG_CONTEXT).unwrap(),
                    )),
                ),
                (
                    AnyVerifyingKey::Ed25519(second.verifying_key()),
                    signature(&second, &commitment),
                ),
            ]),
        };
    let version: exver::Version = "0.4.0.5".parse().unwrap();
    let info = OsVersionInfo {
        iso: [("x86_64".into(), asset.clone())].into(),
        img: [("aarch64".into(), asset.clone())].into(),
        squashfs: [("x86_64".into(), asset.clone())].into(),
        ..Default::default()
    };
    source
        .db
        .mutate(|db| {
            db.as_index_mut()
                .as_os_mut()
                .as_versions_mut()
                .insert(&version, &info)
        })
        .await
        .result
        .unwrap();
    let source_before = snapshot(&source).await;
    let target_before = target.db.peek().await.as_index().de().unwrap();
    let (from, source_task) = registry_server(source.clone()).await;
    let (to, target_task) = registry_server(target.clone()).await;
    let key_path = temp.path().join("identity.pem");
    write_signing_key(&uploader, &key_path).await.unwrap();
    let cli = CliContext::init(ClientConfig {
        id_key_path: Some(key_path),
        ..Default::default()
    })
    .unwrap();
    let mut expected = asset.signatures.clone();
    expected.insert(uploader_key, signature(&uploader, &commitment));
    let mut expected_urls = asset.urls.clone();
    let destination_url: Url = "https://destination.example.com/os.iso".parse().unwrap();

    for repeat in [false, true] {
        if repeat {
            target
                .db
                .mutate(|db| {
                    db.as_index_mut()
                        .as_os_mut()
                        .as_versions_mut()
                        .as_idx_mut(&version)
                        .unwrap()
                        .mutate(|info| {
                            for asset in info
                                .iso
                                .values_mut()
                                .chain(info.img.values_mut())
                                .chain(info.squashfs.values_mut())
                            {
                                asset.signatures.insert(
                                    AnyVerifyingKey::Ed25519(destination_signer.verifying_key()),
                                    signature(&destination_signer, &commitment),
                                );
                                asset.urls.push(destination_url.clone());
                            }
                            Ok(())
                        })
                })
                .await
                .result
                .unwrap();
            expected.insert(
                AnyVerifyingKey::Ed25519(destination_signer.verifying_key()),
                signature(&destination_signer, &commitment),
            );
            expected_urls.push(destination_url.clone());
        }
        cli_os_promote(
            cli.clone(),
            CliOsPromoteParams {
                from: Some(from.to_string()),
                to: Some(to.to_string()),
                version: version.clone(),
                expected_git_hash: None,
            },
        )
        .await
        .unwrap();
        let promoted = target.db.peek().await.as_index().de().unwrap();
        let promoted_info = &promoted.os.versions.0[&version];
        assert_eq!(promoted_info.iso.len(), 1);
        assert_eq!(promoted_info.img.len(), 1);
        assert_eq!(promoted_info.squashfs.len(), 1);
        for promoted_asset in promoted_info
            .iso
            .values()
            .chain(promoted_info.img.values())
            .chain(promoted_info.squashfs.values())
        {
            assert_eq!(promoted_asset.signatures, expected);
            assert_eq!(promoted_asset.commitment, commitment);
            assert_eq!(promoted_asset.urls, expected_urls);
        }
        assert_eq!(promoted.signers, target_before.signers);
        assert_eq!(promoted.trust_signers, target_before.trust_signers);
    }
    assert_eq!(snapshot(&source).await, source_before);
    source_task.abort();
    target_task.abort();
}

#[tokio::test]
async fn os_asset_ingestion_defaults_old_requests_and_verifies_signatures_atomically() {
    let uploader = SigningKey::from_bytes(&[12; 32]);
    let endorser = SigningKey::from_bytes(&[7; 32]);
    let endorser_key = AnyVerifyingKey::Ed25519(endorser.verifying_key());
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(
        temp.path().join("registry"),
        AnyVerifyingKey::Ed25519(uploader.verifying_key()),
    )
    .await;
    add_version(
        ctx.clone(),
        AddVersionParams {
            version: "0.4.0.5".parse().unwrap(),
            release_notes: "Notes".parse().unwrap(),
            source_version: ">=0.4.0".parse().unwrap(),
            git_hash: None,
            expected_git_hash: None,
            security: false,
            signer: None,
        },
    )
    .await
    .unwrap();
    let mut old_request = to_value(&params(&uploader, SignatureMap::new())).unwrap();
    old_request.as_object_mut().unwrap().remove("signatures");
    let old_params: AddAssetParams = from_value(old_request).unwrap();
    assert!(old_params.signatures.is_empty());
    add_iso(ctx.clone(), old_params).await.unwrap();

    let legacy = match signature(&endorser, &commitment()) {
        AnySignature::Ed25519ph(s) => AnySignature::Ed25519(s),
        _ => unreachable!(),
    };
    add_iso(
        ctx.clone(),
        params(&uploader, [(endorser_key.clone(), legacy)].into()),
    )
    .await
    .unwrap();
    let uploader_key = AnyVerifyingKey::Ed25519(uploader.verifying_key());
    let pure = AnySignature::Ed25519(
        uploader.sign(&signing_message(COMMITMENT_SIG_TAG, &commitment(), SIG_CONTEXT).unwrap()),
    );
    let mut same_signer = params(&uploader, [(uploader_key.clone(), pure.clone())].into());
    same_signer.platform = "riscv64".into();
    add_iso(ctx.clone(), same_signer).await.unwrap();
    let mut repeated_signer = params(&uploader, SignatureMap::new());
    repeated_signer.platform = "riscv64".into();
    repeated_signer.signature = pure;
    add_iso(ctx.clone(), repeated_signer).await.unwrap();

    let index = ctx.db.peek().await.as_index().de().unwrap();
    let assets = &index.os.versions.0[&"0.4.0.5".parse().unwrap()].iso;
    assert_eq!(
        assets["riscv64"].signatures[&uploader_key],
        signature(&uploader, &commitment())
    );
    let asset = &assets["x86_64"];
    assert_eq!(asset.signatures.len(), 2);
    assert_eq!(
        asset.signatures[&endorser_key],
        signature(&endorser, &commitment())
    );

    let before = snapshot(&ctx).await;
    let mut different_hash = commitment();
    different_hash.hash = Base64([4; 32]);
    let mut different_size = commitment();
    different_size.size += 1;
    let invalid_signatures = [
        signature(&endorser, &different_hash),
        signature(&endorser, &different_size),
        signature(&uploader, &commitment()),
        AnySignature::Ed25519ph(
            Ed25519ph
                .sign_commitment(&endorser, &commitment(), "s9pk")
                .unwrap(),
        ),
    ];
    for invalid in invalid_signatures {
        let mut request = params(&uploader, [(endorser_key.clone(), invalid)].into());
        request.platform = "aarch64".into();
        assert!(add_iso(ctx.clone(), request).await.is_err());
        assert_eq!(snapshot(&ctx).await, before);
    }
    let mut invalid_uploader = params(
        &uploader,
        [(endorser_key, signature(&endorser, &commitment()))].into(),
    );
    invalid_uploader.signature = signature(&endorser, &commitment());
    assert!(add_iso(ctx.clone(), invalid_uploader).await.is_err());
    assert_eq!(snapshot(&ctx).await, before);

    let invalid_same_key = params(
        &uploader,
        [(uploader_key, signature(&endorser, &commitment()))].into(),
    );
    assert!(add_iso(ctx.clone(), invalid_same_key).await.is_err());
    assert_eq!(snapshot(&ctx).await, before);

    let mut different_commitment = params(&uploader, SignatureMap::new());
    different_commitment.commitment = different_size;
    different_commitment.signature = signature(&uploader, &different_commitment.commitment);
    assert!(add_iso(ctx.clone(), different_commitment).await.is_err());
    assert_eq!(snapshot(&ctx).await, before);

    ctx.db
        .mutate(|db| db.as_admins_mut().ser(&BTreeSet::<Guid>::new()))
        .await
        .result
        .unwrap();
    let before = snapshot(&ctx).await;
    assert!(
        add_iso(
            ctx.clone(),
            params(
                &uploader,
                [(
                    AnyVerifyingKey::Ed25519(endorser.verifying_key()),
                    signature(&endorser, &commitment())
                )]
                .into()
            )
        )
        .await
        .is_err()
    );
    assert_eq!(snapshot(&ctx).await, before);
}

#[tokio::test]
async fn legacy_devices_can_discover_headline_free_releases() {
    use crate::registry::device_info::DEVICE_INFO_HEADER;

    let uploader = SigningKey::from_bytes(&[12; 32]);
    let key = AnyVerifyingKey::Ed25519(uploader.verifying_key());
    let temp = tempfile::tempdir().unwrap();
    let key_path = temp.path().join("identity.pem");
    write_signing_key(&uploader, &key_path).await.unwrap();
    let cli = CliContext::init(ClientConfig {
        id_key_path: Some(key_path),
        ..Default::default()
    })
    .unwrap();
    for startwrt in [false, true] {
        let mut ctx = context(temp.path().join(startwrt.to_string()), key.clone()).await;
        Arc::get_mut(&mut ctx.0).unwrap().startwrt = startwrt;
        let version: exver::Version = "0.4.2.2".parse().unwrap();
        let info = OsVersionInfo {
            release_notes: "Release notes".parse().unwrap(),
            squashfs: [(
                "x86_64".into(),
                RegistryAsset {
                    published_at: chrono::Utc::now(),
                    urls: vec!["https://example.com/os.squashfs".parse().unwrap()],
                    commitment: commitment(),
                    signatures: [(key.clone(), signature(&uploader, &commitment()))].into(),
                },
            )]
            .into(),
            ..Default::default()
        };
        ctx.db
            .mutate(|db| {
                db.as_index_mut()
                    .as_os_mut()
                    .as_versions_mut()
                    .insert(&version, &info)
            })
            .await
            .result
            .unwrap();
        let before = snapshot(&ctx).await;
        let (url, task) = registry_server(ctx.clone()).await;
        for method in ["os.version.get", "os.index", "index"] {
            for (header, source, projected) in [
                (
                    Some("os.version=0.4.2.1&os.compat=*&os.platform=x86_64"),
                    None,
                    true,
                ),
                (
                    Some("os.version=0.4.2.2&os.compat=*&os.platform=x86_64"),
                    Some("0.4.2.1"),
                    startwrt,
                ),
                (None, Some("0.4.2.1"), method == "os.version.get"),
                (None, None, false),
            ] {
                let mut headers = http::HeaderMap::new();
                if let Some(header) = header {
                    headers.insert(
                        http::HeaderName::from_bytes(DEVICE_INFO_HEADER.as_bytes()).unwrap(),
                        http::HeaderValue::from_static(header),
                    );
                }
                let result = crate::middleware::auth::signature::call_remote(
                    &cli,
                    url.join("rpc/v0").unwrap(),
                    headers,
                    url.host_str(),
                    method,
                    imbl_value::json!({ "sourceVersion": source }),
                )
                .await
                .unwrap();
                let versions = match method {
                    "os.version.get" => &result,
                    "os.index" => &result["versions"],
                    _ => &result["os"]["versions"],
                };
                let release = &versions["0.4.2.2"];
                assert_eq!(release["releaseNotes"].as_str(), Some("Release notes"));
                if projected {
                    assert_eq!(release["headline"].as_str(), Some("v0.4.2.2"));
                } else {
                    assert!(release.get("headline").is_none(), "{result:?}");
                }
            }
        }
        assert_eq!(snapshot(&ctx).await, before);
        task.abort();
    }
}

async fn snapshot(ctx: &RegistryContext) -> Value {
    to_value(&ctx.db.peek().await.de().unwrap()).unwrap()
}
