use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Router;
use openssl::pkey::PKey;
use startos::s9pk::manifest::LocaleString;
use startos::sign::{AnySignature, AnySigningKey, SignatureMap, Signer};
use startos::util::future::NonDetachingJoinHandle;
use startos::util::serde::Base64;

use super::*;

const FIRMWARE: &[u8] = b"firmware fixture";

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
        torrent: None,
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
                    assert_eq!(
                        request,
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": 0,
                            "method": "fixture.method",
                            "params": { "fixture": true },
                        }),
                    );
                    assert_eq!(
                        headers[crate::registry::device_info::DEVICE_INFO_HEADER],
                        "fixture"
                    );
                    (
                        [(
                            axum::http::header::CONTENT_TYPE,
                            "application/json; charset=utf-8",
                        )],
                        axum::Json(serde_json::json!({
                            "jsonrpc": "2.0", "id": 0, "result": { "ok": true },
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
async fn registry_rpc_uses_shared_envelope_and_preserves_json_charset_support() {
    let (url, _server) = mirror().await;
    let result = crate::registry::call_registry_rpc(
        &reqwest::Client::new(),
        url.join("/").unwrap().as_str(),
        "fixture.method",
        serde_json::json!({ "fixture": true }),
        Some(axum::http::HeaderValue::from_static("fixture")),
    )
    .await
    .unwrap();
    assert_eq!(result, serde_json::json!({ "ok": true }));
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
