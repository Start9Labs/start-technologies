use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use startos::registry::asset::RegistryAsset;
use startos::registry::os::index::OsVersionInfo;
use startos::registry::os::SIG_CONTEXT;
use startos::sign::commitment::blake3::Blake3Commitment;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use crate::continuations::{Guid, RpcContinuation};
use crate::error::Error;
use crate::progress::{FullProgressTracker, ProgressUnits};
use crate::registry::{self, device_info};
use crate::ServerContext;

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
    let device_info = device_info::load()?;
    let client = reqwest::Client::new();

    let current_version = env!("CARGO_PKG_VERSION");
    let params = serde_json::json!({
        "sourceVersion": current_version,
    });

    let result = registry::call_registry_rpc(
        &client,
        registry_url,
        "os.version.get",
        params,
        Some(&device_info),
    )
    .await?;

    let versions: BTreeMap<String, OsVersionInfo> = serde_json::from_value(result)
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
    let platform = device_info::platform();
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
    download(&asset, client, firmware_path, |bytes_written| {
        download_phase.set_done(bytes_written);
    })
    .await?;
    download_phase.complete();

    // Phase 2: Verify (re-read and check blake3)
    verify_phase.start();
    verify_file(&asset.commitment, firmware_path).await?;
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

/// Downloads from the first URL that answers, checking size and BLAKE3 as it writes.
async fn download(
    asset: &RegistryAsset<Blake3Commitment>,
    client: &reqwest::Client,
    dst: &Path,
    on_progress: impl FnMut(u64),
) -> Result<(), Error> {
    let mut last_err = None;
    for url in &asset.urls {
        match client.get(url.clone()).send().await {
            Ok(response) if response.status().is_success() => {
                return download_to_file(&asset.commitment, response, dst, on_progress).await;
            }
            Ok(response) => {
                last_err = Some(format!("HTTP {} from {}", response.status(), url));
            }
            Err(e) => {
                last_err = Some(format!("request to {} failed: {e}", url));
            }
        }
    }
    Err(Error::other(format!(
        "all asset URLs failed; last error: {}",
        last_err.unwrap_or_else(|| "no URLs".to_string())
    )))
}

async fn download_to_file(
    commitment: &Blake3Commitment,
    mut response: reqwest::Response,
    path: &Path,
    mut on_progress: impl FnMut(u64),
) -> Result<(), Error> {
    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(|e| Error::other(format!("creating download file: {e}")))?;
    let mut hasher = blake3::Hasher::new();
    let mut total_written: u64 = 0;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| Error::other(format!("downloading chunk: {e}")))?
    {
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|e| Error::other(format!("writing chunk: {e}")))?;
        total_written += chunk.len() as u64;
        on_progress(total_written);
    }
    file.flush()
        .await
        .map_err(|e| Error::other(format!("flushing download file: {e}")))?;
    file.sync_all()
        .await
        .map_err(|e| Error::other(format!("syncing download file: {e}")))?;
    if total_written != commitment.size {
        return Err(Error::other(format!(
            "download size mismatch: expected {}, got {}",
            commitment.size, total_written
        )));
    }
    if *hasher.finalize().as_bytes() != *commitment.hash {
        return Err(Error::other("BLAKE3 hash mismatch after download"));
    }
    Ok(())
}

async fn verify_file(commitment: &Blake3Commitment, path: &Path) -> Result<(), Error> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| Error::other(format!("opening file for verification: {e}")))?;
    let metadata = file
        .metadata()
        .await
        .map_err(|e| Error::other(format!("reading file metadata: {e}")))?;
    if metadata.len() != commitment.size {
        return Err(Error::other(format!(
            "file size mismatch: expected {}, got {}",
            commitment.size,
            metadata.len()
        )));
    }
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .await
            .map_err(|e| Error::other(format!("reading file for verification: {e}")))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    if *hasher.finalize().as_bytes() != *commitment.hash {
        return Err(Error::other("BLAKE3 hash mismatch"));
    }
    Ok(())
}
