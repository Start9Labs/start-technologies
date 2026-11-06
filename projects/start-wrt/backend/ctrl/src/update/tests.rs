use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Router;
use base64::Engine;
use ed25519_dalek::pkcs8::EncodePrivateKey;
use openssl::pkey::PKey;
use serde_json::json;
use startos::registry::client::{RegistryClient, REGISTRY_CLIENT_HEADER};
use startos::registry::device_info::DEVICE_INFO_HEADER;
use startos::s9pk::manifest::LocaleString;
use startos::sign::{AnySignature, AnySigningKey, SignatureMap, Signer};
use startos::util::future::NonDetachingJoinHandle;
use startos::util::serde::Base64;

use super::*;

const FIRMWARE: &[u8] = b"firmware fixture";
const PLATFORM: &str = "spacemit,k1-x";

async fn asset(urls: Vec<reqwest::Url>) -> RegistryAsset<Blake3Commitment> {
    let commitment = Blake3Commitment {
        hash: Base64(*blake3::hash(FIRMWARE).as_bytes()),
        size: FIRMWARE.len() as u64,
    };
    let pem = PKey::generate_ed25519()
        .unwrap()
        .private_key_to_pem_pkcs8()
        .unwrap();
    let key: AnySigningKey = String::from_utf8(pem).unwrap().parse().unwrap();
    let signer = Signer::Key(key);
    let key = signer.verifying_key().await.unwrap();
    let signature = signer
        .sign_commitment(&commitment, SIG_CONTEXT)
        .await
        .unwrap();
    RegistryAsset {
        published_at: chrono::DateTime::UNIX_EPOCH,
        commitment,
        signatures: SignatureMap::from([(key, signature)]),
        urls,
    }
}

#[tokio::test]
async fn registry_metadata_accepts_current_and_legacy_signature_encodings() {
    let asset = asset(Vec::new()).await;
    let info = OsVersionInfo {
        headline: "Firmware".into(),
        release_notes: serde_json::from_value::<LocaleString>(serde_json::json!({
            "en_US": "Release notes",
            "fr_FR": "Notes de version",
        }))
        .unwrap(),
        source_version: "*".parse().unwrap(),
        squashfs: BTreeMap::from([("platform".into(), asset.clone())]),
        ..Default::default()
    };
    let current = serde_json::to_value(&info).unwrap();
    let mut legacy = current.clone();
    legacy["releaseNotes"] = serde_json::json!("Release notes");
    let (key, signature) = asset.signatures.first_key_value().unwrap();
    let AnySignature::Ed25519ph(signature) = signature else {
        panic!("in-memory signer must use Ed25519ph")
    };
    let legacy_signature = AnySignature::Ed25519(*signature);
    let legacy_asset = &mut legacy["squashfs"]["platform"];
    legacy_asset["signatures"] = serde_json::to_value(BTreeMap::from([(
        key.to_string(),
        legacy_signature.to_string(),
    )]))
    .unwrap();
    legacy_asset["commitment"]
        .as_object_mut()
        .unwrap()
        .remove("type");

    for wire in [current, legacy] {
        let info: OsVersionInfo = serde_json::from_value(wire).unwrap();
        let asset = asset_for_platform(&info, "platform").unwrap();
        assert_eq!(asset.signatures.first_key_value().unwrap().0, key);
        asset.validate_all(SIG_CONTEXT, None).unwrap();
        assert!(asset.validate_all("wrong-context", None).is_err());
        let mut altered = asset.clone();
        altered.commitment.size += 1;
        assert!(altered.validate_all(SIG_CONTEXT, None).is_err());
        let mut unsigned = asset.clone();
        unsigned.signatures.clear();
        assert!(unsigned.validate_all(SIG_CONTEXT, None).is_err());
        assert_eq!(info.release_notes.localized_for("en_US"), "Release notes");
    }
}

#[tokio::test]
async fn firmware_asset_selection_preserves_format_preference() {
    let asset = asset(Vec::new()).await;
    let mut info = OsVersionInfo {
        squashfs: BTreeMap::from([("platform".into(), asset.clone())]),
        img: BTreeMap::from([("platform".into(), asset.clone())]),
        iso: BTreeMap::from([("platform".into(), asset)]),
        ..Default::default()
    };
    assert!(std::ptr::eq(
        asset_for_platform(&info, "platform").unwrap(),
        &info.squashfs["platform"],
    ));
    info.squashfs.clear();
    assert!(std::ptr::eq(
        asset_for_platform(&info, "platform").unwrap(),
        &info.img["platform"],
    ));
    info.img.clear();
    assert!(std::ptr::eq(
        asset_for_platform(&info, "platform").unwrap(),
        &info.iso["platform"],
    ));
    assert!(asset_for_platform(&info, "unknown").is_none());
}

async fn mirror() -> (reqwest::Url, NonDetachingJoinHandle<()>) {
    let router = Router::new()
        .route(
            "/firmware",
            get(|| async { FIRMWARE }).head(|| async { StatusCode::METHOD_NOT_ALLOWED }),
        )
        .route(
            "/rpc/v0",
            post(
                |headers: axum::http::HeaderMap,
                 axum::Json(request): axum::Json<serde_json::Value>| async move {
                    assert_eq!(request["jsonrpc"], "2.0");
                    assert_eq!(request["method"], "os.version.get");
                    assert_eq!(
                        request["params"],
                        json!({ "sourceVersion": env!("CARGO_PKG_VERSION") }),
                    );
                    assert_eq!(
                        headers[REGISTRY_CLIENT_HEADER],
                        RegistryClient::current(Some(Product::StartWrt)).to_header_value(),
                    );
                    assert_eq!(
                        headers[DEVICE_INFO_HEADER],
                        registry::device_info().unwrap().to_header_value(),
                    );
                    let info = OsVersionInfo {
                        squashfs: BTreeMap::from([(PLATFORM.into(), asset(Vec::new()).await)]),
                        ..Default::default()
                    };
                    (
                        [(
                            axum::http::header::CONTENT_TYPE,
                            "application/json; charset=utf-8",
                        )],
                        axum::Json(json!({
                            "jsonrpc": "2.0", "id": request["id"], "result": { "1.4.1": info },
                        })),
                    )
                },
            ),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/firmware", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let task = NonDetachingJoinHandle::from(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    (url, task)
}

#[tokio::test]
async fn version_query_declares_the_router() {
    let (url, _server) = mirror().await;
    let versions = fetch_versions(url.join("/").unwrap().as_str())
        .await
        .unwrap();
    assert_eq!(versions.keys().collect::<Vec<_>>(), ["1.4.1"]);
    let asset = asset_for_platform(&versions["1.4.1"], PLATFORM).unwrap();
    asset.validate_all(SIG_CONTEXT, None).unwrap();
}

#[tokio::test]
async fn firmware_download_uses_get_only_mirrors_and_reports_written_bytes() {
    let (url, _server) = mirror().await;
    let missing = url.join("/missing").unwrap();
    let asset = asset(vec![missing, url]).await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("firmware.bin");
    let mut progress = Vec::new();
    download_firmware(&reqwest::Client::new(), &asset, &path, |done| {
        progress.push(done)
    })
    .await
    .unwrap();
    assert_eq!(tokio::fs::read(&path).await.unwrap(), FIRMWARE);
    assert_eq!(progress.last(), Some(&(FIRMWARE.len() as u64)));
    asset.commitment.verify_file(&path).await.unwrap();

    let mut corrupted = FIRMWARE.to_vec();
    corrupted[0] ^= 1;
    for bytes in [
        corrupted,
        FIRMWARE[..FIRMWARE.len() - 1].to_vec(),
        [FIRMWARE, b"extra"].concat(),
    ] {
        tokio::fs::write(&path, bytes).await.unwrap();
        assert!(asset.commitment.verify_file(&path).await.is_err());
    }
}

#[tokio::test]
async fn firmware_download_rejects_invalid_commitments_and_unavailable_mirrors() {
    let (url, _server) = mirror().await;
    let mut asset = asset(vec![url.clone()]).await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("firmware.bin");
    let client = reqwest::Client::new();
    let mut wrong_hash = asset.commitment.clone();
    wrong_hash.hash.0[0] ^= 1;
    let mut too_short = asset.commitment.clone();
    too_short.size -= 1;
    let mut too_long = asset.commitment.clone();
    too_long.size += 1;
    for commitment in [wrong_hash, too_short, too_long] {
        asset.commitment = commitment;
        assert!(download_firmware(&client, &asset, &path, |_| {})
            .await
            .is_err());
    }
    asset.urls = vec![url.join("/missing").unwrap()];
    assert!(download_firmware(&client, &asset, &path, |_| {})
        .await
        .is_err());
    asset.urls.clear();
    assert!(download_firmware(&client, &asset, &path, |_| {})
        .await
        .is_err());
}

/// AlgorithmIdentifier for OID 2.25.31184504503259998342399313009964778715.
const ED25519PH_ALGORITHM_ID_DER: &[u8] = &[
    0x30, 0x15, 0x06, 0x13, 0x69, 0xae, 0xf5, 0xf5, 0xb1, 0xe7, 0x89, 0x8a, 0x85, 0xc9, 0x9c, 0x9a,
    0xe3, 0xbb, 0x85, 0xfb, 0xf2, 0x89, 0x5b,
];

fn key() -> AnySigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[7; 32])
        .to_pkcs8_pem(Default::default())
        .unwrap()
        .parse()
        .unwrap()
}

fn commitment() -> Blake3Commitment {
    Blake3Commitment {
        hash: Base64([3; 32]),
        size: 4096,
    }
}

async fn ph_signature(commitment: &Blake3Commitment) -> AnySignature {
    Signer::Key(key())
        .sign_commitment(commitment, SIG_CONTEXT)
        .await
        .unwrap()
}

fn relabeled_pure(sig: AnySignature) -> AnySignature {
    match sig {
        AnySignature::Ed25519ph(s) => AnySignature::Ed25519(s),
        other => other,
    }
}

fn ed25519ph_pem(sig: &AnySignature) -> String {
    let AnySignature::Ed25519ph(sig) = sig else {
        panic!("expected an Ed25519ph signature")
    };
    let mut der = vec![0x30, 0x59];
    der.extend_from_slice(ED25519PH_ALGORITHM_ID_DER);
    der.extend_from_slice(&[0x04, 0x40]);
    der.extend_from_slice(&sig.to_bytes());
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    let (first, rest) = b64.split_at(64);
    format!("-----BEGIN SIGNATURE-----\n{first}\n{rest}\n-----END SIGNATURE-----\n")
}

fn parse_asset(signature: String) -> RegistryAsset<Blake3Commitment> {
    let c = commitment();
    let signer = key().verifying_key().to_string();
    let response = json!({
        "1.3.1": {
            "headline": "",
            "releaseNotes": "",
            "sourceVersion": ">=1.0.0",
            "authorized": [],
            "iso": {},
            "img": {},
            "squashfs": {
                PLATFORM: {
                    "publishedAt": "2026-10-01T00:00:00Z",
                    "urls": ["https://example.com/startwrt.img"],
                    "commitment": { "hash": c.hash.to_string(), "size": c.size },
                    "signatures": { signer: signature },
                },
            },
        },
    });
    let mut versions: BTreeMap<String, OsVersionInfo> = serde_json::from_value(response).unwrap();
    versions
        .remove("1.3.1")
        .unwrap()
        .squashfs
        .remove(PLATFORM)
        .unwrap()
}

#[tokio::test]
async fn ed25519ph_label_parses_and_verifies() {
    let sig = ph_signature(&commitment()).await;
    let asset = parse_asset(ed25519ph_pem(&sig));
    assert_eq!(asset.signatures.values().next(), Some(&sig));
    asset.validate_all(SIG_CONTEXT, None).unwrap();
    assert!(asset.validate_all("s9pk", None).is_err());
}

#[tokio::test]
async fn id_ed25519_label_on_ed25519ph_signature_verifies() {
    let sig = ph_signature(&commitment()).await;
    let asset = parse_asset(relabeled_pure(sig).to_string());
    asset.validate_all(SIG_CONTEXT, None).unwrap();
    assert!(asset.validate_all("s9pk", None).is_err());
}

#[tokio::test]
async fn pure_ed25519_signature_verifies() {
    let c = commitment();
    let mut msg = b"Start-Commitment-Sig v1\0".to_vec();
    msg.push(SIG_CONTEXT.len() as u8);
    msg.extend_from_slice(SIG_CONTEXT.as_bytes());
    msg.push(1);
    msg.extend_from_slice(&*c.hash);
    msg.extend_from_slice(&c.size.to_be_bytes());
    let sig = Signer::Key(key()).sign_message(&msg).await.unwrap();
    let asset = parse_asset(sig.to_string());
    asset.validate_all(SIG_CONTEXT, None).unwrap();
}

#[tokio::test]
async fn signature_over_another_commitment_is_rejected() {
    let sig = ph_signature(&Blake3Commitment {
        size: 4097,
        ..commitment()
    })
    .await;
    for pem in [ed25519ph_pem(&sig), relabeled_pure(sig).to_string()] {
        let asset = parse_asset(pem);
        assert!(asset.validate_all(SIG_CONTEXT, None).is_err());
    }
}
