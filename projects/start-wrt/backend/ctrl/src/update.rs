use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use startos::registry::asset::RegistryAsset;
use startos::registry::client::Product;
use startos::registry::os::index::OsVersionInfo;
use startos::registry::os::SIG_CONTEXT;
use startos::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use startos::sign::commitment::blake3::Blake3Commitment;
use startos::sign::commitment::Commitment;
use startos::util::io::IOHook;
use tokio::process::Command;

use crate::continuations::{Guid, RpcContinuation};
use crate::error::Error;
use crate::progress::{FullProgressTracker, ProgressUnits};
use crate::{registry, ServerContext};

// ── Request / Response types ─────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, clap::Parser, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
#[group(skip)]
pub struct UpdateSystemParams {
    #[arg(long)]
    pub registry: Option<String>,
    #[arg(long)]
    pub target_version: Option<String>,
}
rpc_toolkit::reflect_ts!(UpdateSystemParams);

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSystemRes {
    pub target: Option<String>,
    pub progress: Option<String>,
}
rpc_toolkit::reflect_ts!(UpdateSystemRes);

// ── Handlers ─────────────────────────────────────────────────────────

/// Check the registry for newer OS versions.
/// Used by both `system.newer-versions` and `system.update`.
pub async fn fetch_newer_versions(
    registry_url: &str,
) -> Result<BTreeMap<String, OsVersionInfo>, Error> {
    let registry_url = registry_url
        .parse()
        .map_err(|e| Error::other(format!("invalid registry URL: {e}")))?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| Error::other(format!("building HTTP client: {e}")))?;

    let result = startos::registry::client::call(
        client,
        registry_url,
        Product::StartWrt,
        Some(&registry::device_info()?),
        "os.version.get",
        imbl_value::json!({
            "sourceVersion": env!("CARGO_PKG_VERSION"),
        }),
    )
    .await?;

    let versions: BTreeMap<String, OsVersionInfo> = imbl_value::from_value(result)
        .map_err(|e| Error::other(format!("parsing registry version response: {e}")))?;

    Ok(versions)
}

/// Main update handler: fetch, validate, download, verify, sysupgrade.
pub async fn update_system(
    ctx: ServerContext,
    UpdateSystemParams {
        registry,
        target_version,
    }: UpdateSystemParams,
) -> Result<UpdateSystemRes, Error> {
    let platform = registry::platform();
    let registry = registry.unwrap_or_else(|| {
        crate::system::read_registry_url()
            .unwrap_or_else(|| crate::system::DEFAULT_REGISTRY_URL.to_string())
    });

    // 1. Fetch available versions
    let versions = fetch_newer_versions(&registry).await?;

    if versions.is_empty() {
        return Ok(UpdateSystemRes {
            target: None,
            progress: None,
        });
    }

    // 2. Select target version. Note: BTreeMap keys sort lexicographically,
    // which is wrong for semver (e.g. "9.0.0" > "10.0.0"). Select the max via
    // numeric comparison instead.
    let (target_ver, version_info) = if let Some(ref target) = target_version {
        let info = versions
            .get(target)
            .ok_or_else(|| Error::other(format!("requested version {target} not available")))?;
        (target.clone(), info)
    } else {
        let Some((ver, info)) = versions
            .iter()
            .max_by(|(a, _), (b, _)| crate::system::cmp_versions(a, b))
        else {
            return Ok(UpdateSystemRes {
                target: None,
                progress: None,
            });
        };
        (ver.clone(), info)
    };

    // 3. Find asset for our platform
    let asset = asset_for_platform(version_info, &platform)
        .ok_or_else(|| {
            Error::other(format!(
                "no asset for platform '{platform}' in version {target_ver}"
            ))
        })?
        .clone();

    // 4. Validate signatures
    asset.validate(SIG_CONTEXT, asset.all_signers())?;

    // 5. Create progress tracker
    let tracker = FullProgressTracker::new();
    let mut download_phase = tracker.add_phase("Downloading firmware".into(), Some(100));
    // start() must precede set_units/set_total: those only mutate an
    // already-started Progress::Progress, so on a NotStarted phase they are
    // silent no-ops and start() would then reset total/units back to None.
    download_phase.start();
    download_phase.set_units(Some(ProgressUnits::Bytes));
    download_phase.set_total(asset.commitment.size);
    let mut verify_phase = tracker.add_phase("Verifying integrity".into(), Some(10));
    let mut apply_phase = tracker.add_phase("Applying update".into(), Some(1));

    // 6. Register WebSocket continuation for progress streaming.
    // The WS handler creates its own stream from the tracker clone, so the
    // tracker must outlive both the handler and the background update task.
    let guid = Guid::new();
    let guid_str = guid.to_string();
    let tracker_for_ws = tracker.clone();

    ctx.continuations
        .add(
            guid.clone(),
            RpcContinuation::ws(
                move |mut ws| async move {
                    let stream = tracker_for_ws.stream(Some(Duration::from_millis(300)));
                    tokio::pin!(stream);
                    while let Some(progress) = stream.next().await {
                        let json = match serde_json::to_string(&progress) {
                            Ok(j) => j,
                            Err(_) => break,
                        };
                        if ws
                            .send(axum::extract::ws::Message::Text(json.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                        if progress.overall.is_complete() {
                            break;
                        }
                    }
                    let _ = ws.send(axum::extract::ws::Message::Close(None)).await;
                },
                Duration::from_secs(600),
            ),
        )
        .await;

    // 7. Spawn background update task (owns tracker + phase handles)
    let client = reqwest::Client::new();
    let target_ver_for_task = target_ver.clone();
    tokio::spawn(async move {
        let target_ver = target_ver_for_task;
        let result = do_update(
            &client,
            asset,
            &target_ver,
            &mut download_phase,
            &mut verify_phase,
            &mut apply_phase,
        )
        .await;

        match result {
            Ok(()) => {
                tracing::info!("Update to {target_ver} initiated successfully");
                tracker.complete();
            }
            Err(e) => {
                tracing::error!("Update to {target_ver} failed: {e}");
                crate::activity::log(
                    "system",
                    "update",
                    false,
                    &format!("Update to v{target_ver} failed"),
                    Some(&e.to_string()),
                );
                tracker.fail();
            }
        }
    });

    Ok(UpdateSystemRes {
        target: Some(target_ver),
        progress: Some(guid_str),
    })
}

// ── Pending-update marker ────────────────────────────────────────────
//
// `do_update` writes this file (the target version) just before invoking
// `sysupgrade`. A successful sysupgrade reboots, leaving the marker for the
// freshly-booted firmware to confirm; a failed sysupgrade returns and the
// marker is cleared. `keep.d/startwrt` lists this path so it survives the
// sysupgrade overlay wipe.
const PENDING_UPDATE_MARKER: &str = "/etc/startwrt/pending-update";

/// Record the in-flight update target so the next boot can confirm it.
/// Best-effort — a marker failure must not abort the update.
fn write_pending_marker(target_ver: &str) {
    if let Some(dir) = Path::new(PENDING_UPDATE_MARKER).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(PENDING_UPDATE_MARKER, target_ver) {
        tracing::warn!("failed to write pending-update marker: {e}");
    }
}

/// Remove the pending-update marker (the update did not reach a reboot).
fn clear_pending_marker() {
    let _ = std::fs::remove_file(PENDING_UPDATE_MARKER);
}

/// On boot, confirm the outcome of a pending update in the Activity log.
/// Called once from the daemon's normal-mode startup.
pub fn check_pending_update_marker() {
    let target = match std::fs::read_to_string(PENDING_UPDATE_MARKER) {
        Ok(s) => s.trim().to_string(),
        Err(_) => return, // no marker — an ordinary boot
    };
    let current = env!("CARGO_PKG_VERSION");
    if target == current {
        crate::activity::log(
            "system",
            "update",
            true,
            &format!("Updated to v{current}"),
            None,
        );
    } else {
        crate::activity::log(
            "system",
            "update",
            false,
            &format!("Update to v{target} failed: booted v{current}"),
            Some("version mismatch after reboot"),
        );
    }
    clear_pending_marker();
}

/// Execute the actual update: download, verify, sysupgrade.
async fn do_update(
    client: &reqwest::Client,
    asset: RegistryAsset<Blake3Commitment>,
    target_ver: &str,
    download_phase: &mut crate::progress::PhaseProgressTrackerHandle,
    verify_phase: &mut crate::progress::PhaseProgressTrackerHandle,
    apply_phase: &mut crate::progress::PhaseProgressTrackerHandle,
) -> Result<(), Error> {
    let firmware_path = Path::new("/tmp/firmware.bin");

    // Phase 1: Download
    let mut file = tokio::fs::File::create(firmware_path)
        .await
        .map_err(|e| Error::other(format!("creating download file: {e}")))?;
    let phase = &mut *download_phase;
    let mut written = 0u64;
    let mut writer = IOHook::new(&mut file);
    writer.post_write(move |buf| {
        written += buf.len() as u64;
        phase.set_done(written);
    });
    asset.download(client.clone(), &mut writer).await?;
    drop(writer);
    file.sync_all()
        .await
        .map_err(|e| Error::other(format!("syncing download file: {e}")))?;
    download_phase.complete();

    // Phase 2: Verify (re-read and check blake3)
    verify_phase.start();
    asset
        .commitment
        .check(&MultiCursorFile::open(&file).await?)
        .await?;
    verify_phase.complete();

    // Phase 3: Apply via sysupgrade
    apply_phase.start();
    crate::activity::log(
        "system",
        "update",
        true,
        &format!("Applying update to v{target_ver}"),
        None,
    );
    write_pending_marker(target_ver);
    let status = Command::new("sysupgrade")
        .arg("/tmp/firmware.bin")
        .status()
        .await;
    // Reached only if sysupgrade did not reboot us — i.e. it failed.
    clear_pending_marker();
    let status = status.map_err(|e| Error::other(format!("spawning sysupgrade: {e}")))?;

    if !status.success() {
        return Err(Error::other(format!(
            "sysupgrade exited with code {}",
            status.code().unwrap_or(-1)
        )));
    }
    apply_phase.complete();

    // Device reboots — this point is likely unreachable
    Ok(())
}

/// Prefers squashfs, then img, then iso.
pub(crate) fn asset_for_platform<'a>(
    info: &'a OsVersionInfo,
    platform: &str,
) -> Option<&'a RegistryAsset<Blake3Commitment>> {
    info.squashfs
        .get(platform)
        .or_else(|| info.img.get(platform))
        .or_else(|| info.iso.get(platform))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use base64::Engine;
    use ed25519_dalek::pkcs8::EncodePrivateKey;
    use serde_json::json;
    use startos::sign::{AnySignature, AnySigningKey, Signer};
    use startos::util::serde::Base64;

    use super::*;

    const PLATFORM: &str = "spacemit,k1-x";

    /// AlgorithmIdentifier for OID 2.25.31184504503259998342399313009964778715.
    const ED25519PH_ALGORITHM_ID_DER: &[u8] = &[
        0x30, 0x15, 0x06, 0x13, 0x69, 0xae, 0xf5, 0xf5, 0xb1, 0xe7, 0x89, 0x8a, 0x85, 0xc9, 0x9c,
        0x9a, 0xe3, 0xbb, 0x85, 0xfb, 0xf2, 0x89, 0x5b,
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
        let mut versions: BTreeMap<String, OsVersionInfo> =
            serde_json::from_value(response).unwrap();
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
        asset.validate(SIG_CONTEXT, asset.all_signers()).unwrap();
        assert!(asset.validate("s9pk", asset.all_signers()).is_err());
    }

    #[tokio::test]
    async fn id_ed25519_label_on_ed25519ph_signature_verifies() {
        let sig = ph_signature(&commitment()).await;
        let asset = parse_asset(relabeled_pure(sig).to_string());
        asset.validate(SIG_CONTEXT, asset.all_signers()).unwrap();
        assert!(asset.validate("s9pk", asset.all_signers()).is_err());
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
        asset.validate(SIG_CONTEXT, asset.all_signers()).unwrap();
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
            assert!(asset.validate(SIG_CONTEXT, asset.all_signers()).is_err());
        }
    }
}
