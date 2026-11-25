use std::sync::Arc;

use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Router;
use futures_util::StreamExt;
use openssl::pkey::PKey;
use startos::s9pk::manifest::LocaleString;
use startos::sign::{AnySignature, AnySigningKey, SignatureMap, Signer};
use startos::util::future::NonDetachingJoinHandle;
use startos::util::serde::Base64;

use super::*;
use crate::progress::Progress;

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
        private: None,
        readers: Default::default(),
    }
}

#[tokio::test]
async fn registry_metadata_accepts_current_and_legacy_signature_encodings() {
    let asset = asset(Vec::new()).await;
    let info = OsVersionInfo {
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
        assert_eq!(
            info.release_notes.localized_for(Some("en_US"), None),
            "Release notes"
        );
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

#[tokio::test]
async fn revocation_closes_legacy_update_progress_while_update_is_active() {
    use startos::middleware::auth::signature::SignatureAuthorizationContext;
    use tokio_tungstenite::tungstenite::Message;

    let context = ServerContext::default();
    let signer: Signer = AnySigningKey::generate().into();
    let key = crate::rpc::tests::enroll(&context, &signer).await;
    let subscription = startos::auth::continuation_subscription(&context, Some(key.clone()))
        .await
        .unwrap();
    let tracker = FullProgressTracker::new();
    let mut phase = tracker.add_phase("Downloading firmware".into(), Some(1));
    phase.start();
    let guid = Guid::new();
    context
        .continuations
        .add(
            guid.clone(),
            legacy_progress_continuation(subscription, None, tracker.clone()),
        )
        .await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let path = format!("/progress/{guid}");
    let continuations = context.continuations.clone();
    let app = Router::new().route(
        &path,
        get(move |upgrade: axum::extract::ws::WebSocketUpgrade| {
            let continuations = continuations.clone();
            let guid = guid.clone();
            async move {
                continuations
                    .get_ws_handler(&guid)
                    .await
                    .unwrap()
                    .on_upgrade(upgrade, guid)
            }
        }),
    );
    let _server: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    })
    .into();
    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{address}{path}"))
        .await
        .unwrap();
    let initial = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let initial: FullProgress = serde_json::from_str(initial.to_text().unwrap()).unwrap();
    assert!(!initial.overall.is_complete());

    context.unenroll([key]).await.unwrap();
    let closed = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .expect("revocation must close an idle legacy update observer");
    assert!(matches!(
        closed,
        None | Some(Err(_)) | Some(Ok(Message::Close(_)))
    ));
    assert!(!tracker.snapshot().overall.is_complete());
    phase.complete();
    tracker.complete();
    assert_eq!(tracker.snapshot().overall, Progress::Complete(true));
}

async fn streamed(succeed: bool) -> Vec<Result<FullProgress, Error>> {
    let tracker = FullProgressTracker::new();
    let mut phase = tracker.add_phase("Downloading firmware".into(), Some(1));
    let finished = tracker.clone();
    let done = tokio::spawn(async move {
        phase.start();
        if succeed {
            phase.complete();
            finished.complete();
            Ok(())
        } else {
            finished.fail();
            Err(Error::other("sysupgrade exited with code 1"))
        }
    });
    progress_until_done(tracker, done).collect().await
}

#[tokio::test]
async fn streamed_progress_ends_with_the_update_result() {
    let succeeded = streamed(true).await;
    let last = succeeded.last().unwrap().as_ref().unwrap();
    assert_eq!(last.overall, Progress::Complete(true));
    assert!(succeeded.iter().all(Result::is_ok));

    let failed = streamed(false).await;
    let (error, snapshots) = failed.split_last().unwrap();
    assert!(error
        .as_ref()
        .unwrap_err()
        .to_string()
        .contains("sysupgrade exited with code 1"));
    assert_eq!(
        snapshots.last().unwrap().as_ref().unwrap().overall,
        Progress::Complete(false)
    );
}

async fn stalled_registry() -> (
    String,
    tokio::sync::watch::Sender<bool>,
    NonDetachingJoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let firmware: reqwest::Url = format!("{base}/firmware").parse().unwrap();
    let info = serde_json::to_value(OsVersionInfo {
        source_version: "*".parse().unwrap(),
        squashfs: BTreeMap::from([(
            DeviceInfo::load().unwrap().os.platform.into(),
            asset(vec![firmware]).await,
        )]),
        ..Default::default()
    })
    .unwrap();
    let (release, released) = tokio::sync::watch::channel(false);
    let router = Router::new()
        .route(
            "/firmware",
            get(move || {
                let mut released = released.clone();
                async move {
                    let _ = released.wait_for(|released| *released).await;
                    StatusCode::NOT_FOUND
                }
            }),
        )
        .route(
            "/rpc/v0",
            post(move |axum::Json(request): axum::Json<serde_json::Value>| {
                let info = info.clone();
                async move {
                    assert_eq!(request["method"], "os.version.get");
                    axum::Json(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request["id"],
                        "result": { "99.0.0": info },
                    }))
                }
            }),
        );
    let task = NonDetachingJoinHandle::from(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    (base, release, task)
}

fn update_args(
    context: &ServerContext,
    registry: &str,
) -> HandlerArgs<ServerContext, UpdateSystemParams, Empty, FullProgress> {
    HandlerArgs {
        context: context.clone(),
        parent_method: Default::default(),
        method: Default::default(),
        params: UpdateSystemParams {
            registry: Some(registry.to_string()),
            target_version: None,
            signer: None,
        },
        inherited_params: Empty {},
        raw_params: imbl_value::Value::Null,
        rpc_stream: None,
    }
}

#[tokio::test]
async fn an_update_in_flight_refuses_another_until_it_fails() {
    let (registry, release, _server) = stalled_registry().await;
    let context = ServerContext::default();

    let first = update_system(update_args(&context, &registry))
        .await
        .unwrap();
    assert_eq!(first.target.as_deref(), Some("99.0.0"));
    let refused = update_system(update_args(&context, &registry))
        .await
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::InvalidRequest);

    release.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while context.system_update.clone().try_lock_owned().is_err() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("a failed update must release the next one");
    let retried = update_system(update_args(&context, &registry))
        .await
        .unwrap();
    assert_eq!(retried.target.as_deref(), Some("99.0.0"));
}

#[tokio::test]
async fn a_request_that_fails_before_starting_leaves_updates_available() {
    let context = ServerContext::default();
    let error = update_system(update_args(&context, "http://127.0.0.1:1"))
        .await
        .unwrap_err();
    assert_ne!(error.kind, ErrorKind::InvalidRequest);
    assert!(context.system_update.clone().try_lock_owned().is_ok());
}

#[tokio::test]
async fn a_mirror_that_stops_sending_fails_the_download() {
    let router = Router::new().route(
        "/firmware",
        get(|| async {
            let partial = futures_util::stream::once(async {
                Ok::<_, std::io::Error>(axum::body::Bytes::from_static(&FIRMWARE[..4]))
            });
            axum::body::Body::from_stream(partial.chain(futures_util::stream::pending()))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/firmware", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let _server = NonDetachingJoinHandle::from(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    let asset = asset(vec![url]).await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("firmware.bin");
    let client = firmware_client(Duration::from_millis(200)).unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        download_firmware(&client, &asset, &path, |_| {}),
    )
    .await
    .expect("an idle mirror must fail the download");
    assert!(result.is_err());
}

#[tokio::test]
async fn a_slow_mirror_that_keeps_sending_completes_the_download() {
    let router = Router::new().route(
        "/firmware",
        get(|| async {
            let trickle = futures_util::stream::iter(FIRMWARE.chunks(1)).then(|byte| async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok::<_, std::io::Error>(axum::body::Bytes::from_static(byte))
            });
            axum::body::Body::from_stream(trickle)
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/firmware", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let _server = NonDetachingJoinHandle::from(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    let asset = asset(vec![url]).await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("firmware.bin");
    let client = firmware_client(Duration::from_millis(500)).unwrap();
    download_firmware(&client, &asset, &path, |_| {})
        .await
        .unwrap();
    asset.commitment.verify_file(&path).await.unwrap();
}

fn exits_with(code: u8) -> Command {
    let mut sysupgrade = Command::new("sh");
    sysupgrade.args(["-c", &format!("exit {code}")]);
    sysupgrade
}

struct Pending {
    _dir: tempfile::TempDir,
    marker: std::path::PathBuf,
    lock: Arc<tokio::sync::Mutex<()>>,
}

impl Pending {
    fn new() -> (Self, OwnedMutexGuard<()>) {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("pending-update");
        std::fs::write(&marker, "1.5.0").unwrap();
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        let in_flight = lock.clone().try_lock_owned().unwrap();
        (
            Self {
                _dir: dir,
                marker,
                lock,
            },
            in_flight,
        )
    }

    fn assert_handed_off(&self, handed_off: bool) {
        assert_eq!(self.marker.exists(), handed_off, "marker kept");
        assert_eq!(self.lock.try_lock().is_err(), handed_off, "lock held");
    }
}

fn stopping_after(delay: Duration) -> (CancellationToken, NonDetachingJoinHandle<()>) {
    let shutdown = CancellationToken::new();
    let stop = shutdown.clone();
    let task = NonDetachingJoinHandle::from(tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        stop.cancel();
    }));
    (shutdown, task)
}

#[tokio::test]
async fn a_lost_ubusd_during_shutdown_is_a_hand_off() {
    let shutdown = CancellationToken::new();
    shutdown.cancel();
    let (pending, in_flight) = Pending::new();
    run_sysupgrade(exits_with(246), &pending.marker, in_flight, &shutdown)
        .await
        .unwrap();
    pending.assert_handed_off(true);

    let (shutdown, _stop) = stopping_after(HANDOFF_GRACE / 4);
    let (pending, in_flight) = Pending::new();
    run_sysupgrade(exits_with(246), &pending.marker, in_flight, &shutdown)
        .await
        .unwrap();
    pending.assert_handed_off(true);
}

#[tokio::test]
async fn a_lost_ubusd_without_shutdown_is_reported_after_the_grace() {
    let shutdown = CancellationToken::new();
    let (pending, in_flight) = Pending::new();
    let started = tokio::time::Instant::now();
    let error = run_sysupgrade(exits_with(246), &pending.marker, in_flight, &shutdown)
        .await
        .unwrap_err();
    assert!(started.elapsed() >= HANDOFF_GRACE);
    assert!(
        error
            .to_string()
            .contains("sysupgrade exited with code 246"),
        "{error}"
    );
    pending.assert_handed_off(false);
}

#[tokio::test]
async fn a_zero_exit_is_a_hand_off() {
    let (pending, in_flight) = Pending::new();
    let started = tokio::time::Instant::now();
    run_sysupgrade(
        exits_with(0),
        &pending.marker,
        in_flight,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(started.elapsed() < HANDOFF_GRACE);
    pending.assert_handed_off(true);
}

#[tokio::test]
async fn a_sysupgrade_that_never_ran_fails_during_shutdown() {
    let shutdown = CancellationToken::new();
    shutdown.cancel();
    let (pending, in_flight) = Pending::new();
    let error = run_sysupgrade(
        Command::new("/nonexistent/sysupgrade"),
        &pending.marker,
        in_flight,
        &shutdown,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("spawning sysupgrade"), "{error}");
    pending.assert_handed_off(false);
}

#[tokio::test]
async fn a_rejection_is_reported_at_once_whatever_the_daemon_does() {
    for code in [1, 248] {
        let never = CancellationToken::new();
        let stopped = CancellationToken::new();
        stopped.cancel();
        let (stopping, _stop) = stopping_after(HANDOFF_GRACE / 4);
        for shutdown in [&never, &stopped, &stopping] {
            let (pending, in_flight) = Pending::new();
            let started = tokio::time::Instant::now();
            let error = run_sysupgrade(exits_with(code), &pending.marker, in_flight, shutdown)
                .await
                .unwrap_err();
            assert!(started.elapsed() < HANDOFF_GRACE);
            assert!(
                error
                    .to_string()
                    .contains(&format!("sysupgrade exited with code {code}")),
                "{error}"
            );
            pending.assert_handed_off(false);
        }
    }
}
