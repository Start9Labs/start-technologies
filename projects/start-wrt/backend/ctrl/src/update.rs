use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use startos::middleware::auth::session::SessionContext;
use startos::registry::asset::RegistryAsset;
use startos::sign::commitment::blake3::Blake3Commitment;
use startos::util::io::create_file;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::continuations::{Guid, RpcContinuation, WebSocketPolicy};
use crate::error::Error;
use crate::progress::{FullProgressTracker, ProgressUnits};
use crate::registry::device_info::DeviceInfo;
use crate::registry::os::{asset_for_platform, OsVersionInfo, SIG_CONTEXT};
use crate::registry::{self};
use crate::ServerContext;

#[cfg(test)]
mod tests;

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

/// Uses the router's device identity for registry compatibility.
pub async fn fetch_versions(registry_url: &str) -> Result<BTreeMap<String, OsVersionInfo>, Error> {
    let device_info = DeviceInfo::load()?;
    let device_info_header = device_info.to_header_value()?;
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
        Some(device_info_header),
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
    let platform = DeviceInfo::load()?.os.platform;
    let registry = registry.unwrap_or_else(|| {
        crate::system::read_registry_url()
            .unwrap_or_else(|| crate::system::DEFAULT_REGISTRY_URL.to_string())
    });

    let versions = fetch_versions(&registry).await?;

    if versions.is_empty() {
        return Ok(UpdateSystemRes {
            target: None,
            progress: None,
        });
    }

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

    let asset = asset_for_platform(version_info, &platform)
        .ok_or_else(|| {
            Error::other(format!(
                "no asset for platform '{platform}' in version {target_ver}"
            ))
        })?
        .clone();

    asset.validate_all(SIG_CONTEXT, None)?;
    let tracker = FullProgressTracker::new();
    let mut download_phase = tracker.add_phase("Downloading firmware".into(), Some(100));
    // Configure totals and units after starting the phase.
    download_phase.start();
    download_phase.set_units(Some(ProgressUnits::Bytes));
    download_phase.set_total(asset.commitment.size);
    let mut verify_phase = tracker.add_phase("Verifying integrity".into(), Some(10));
    let mut apply_phase = tracker.add_phase("Applying update".into(), Some(1));

    let guid = Guid::new();
    let guid_str = guid.to_string();
    let tracker_for_ws = tracker.clone();

    ctx.continuations
        .add(
            guid.clone(),
            RpcContinuation::ws(
                WebSocketPolicy::Authenticated,
                ctx.secure_session().cloned(),
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

// Must match `keep.d/startwrt` in `build/stage-files.sh`.
const PENDING_UPDATE_MARKER: &str = "/etc/startwrt/pending-update";

fn write_pending_marker(target_ver: &str) {
    if let Some(dir) = Path::new(PENDING_UPDATE_MARKER).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(PENDING_UPDATE_MARKER, target_ver) {
        tracing::warn!("failed to write pending-update marker: {e}");
    }
}

fn clear_pending_marker() {
    let _ = std::fs::remove_file(PENDING_UPDATE_MARKER);
}

/// Whether this boot follows an update [`check_pending_update_marker`] has
/// yet to confirm.
pub fn update_pending() -> bool {
    Path::new(PENDING_UPDATE_MARKER).exists()
}

/// Logs a pending update's boot outcome in Activity during daemon startup.
pub fn check_pending_update_marker() {
    let target = match std::fs::read_to_string(PENDING_UPDATE_MARKER) {
        Ok(s) => s.trim().to_string(),
        Err(_) => return,
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

async fn download_firmware(
    client: &reqwest::Client,
    asset: &RegistryAsset<Blake3Commitment>,
    path: &Path,
    mut on_progress: impl FnMut(u64),
) -> Result<(), Error> {
    let mut last_error = None;
    for url in &asset.urls {
        let mut response = match client.get(url.clone()).send().await {
            Ok(response) if response.status().is_success() => response,
            Ok(response) => {
                last_error = Some(format!("HTTP {} from {}", response.status(), url));
                continue;
            }
            Err(error) => {
                last_error = Some(format!("request to {} failed: {}", url, error));
                continue;
            }
        };
        let file = create_file(path).await?;
        let mut writer = asset.commitment.verifying_writer(file);
        let mut written = 0;
        while let Some(chunk) = response.chunk().await? {
            writer.write_all(&chunk).await?;
            written += chunk.len() as u64;
            on_progress(written);
        }
        let mut file = writer.verify().await?;
        file.flush().await?;
        file.sync_all().await?;
        return Ok(());
    }
    Err(Error::other(format!(
        "all asset URLs failed; last error: {}",
        last_error.unwrap_or_else(|| "no URLs".to_string())
    )))
}

async fn do_update(
    client: &reqwest::Client,
    asset: RegistryAsset<Blake3Commitment>,
    target_ver: &str,
    download_phase: &mut crate::progress::PhaseProgressTrackerHandle,
    verify_phase: &mut crate::progress::PhaseProgressTrackerHandle,
    apply_phase: &mut crate::progress::PhaseProgressTrackerHandle,
) -> Result<(), Error> {
    let firmware_path = Path::new("/tmp/firmware.bin");

    download_firmware(client, &asset, firmware_path, |bytes_written| {
        download_phase.set_done(bytes_written);
    })
    .await?;
    download_phase.complete();

    verify_phase.start();
    asset.commitment.verify_file(firmware_path).await?;
    verify_phase.complete();

    apply_phase.start();
    crate::activity::log(
        "system",
        "update",
        true,
        &format!("Applying update to v{target_ver}"),
        None,
    );
    write_pending_marker(target_ver);
    // `sysupgrade` must survive cancellation of the daemon's update task.
    let status = Command::new("sysupgrade").arg(firmware_path).status().await;
    clear_pending_marker();
    let status = status.map_err(|e| Error::other(format!("spawning sysupgrade: {e}")))?;
    if !status.success() {
        return Err(Error::other(format!(
            "sysupgrade exited with code {}",
            status.code().unwrap_or(-1)
        )));
    }
    apply_phase.complete();

    Ok(())
}
