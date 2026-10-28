use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::instrument;

use crate::prelude::*;
use crate::tunnel::context::TunnelContext;
use crate::util::Invoke;

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct TunnelUpdateResult {
    /// "up-to-date", "update-available", or "updating"
    pub status: String,
    /// Currently installed version
    pub installed: String,
    /// Available candidate version
    pub candidate: String,
}

rpc_toolkit::reflect_ts!(TunnelUpdateResult);
rpc_toolkit::ts_export!(TunnelUpdateResult, namespaces = ["tunnel"]);

#[instrument(skip_all)]
pub async fn check_update(_ctx: TunnelContext) -> Result<TunnelUpdateResult, Error> {
    Command::new("apt-get")
        .arg("update")
        .invoke(ErrorKind::UpdateFailed)
        .await?;

    let policy_output = Command::new("apt-cache")
        .arg("policy")
        .arg("start-tunnel")
        .invoke(ErrorKind::UpdateFailed)
        .await?;

    let policy_str = String::from_utf8_lossy(&policy_output).to_string();
    let installed = parse_version_field(&policy_str, "Installed:");
    let candidate = parse_version_field(&policy_str, "Candidate:");

    let status = if installed == candidate {
        "up-to-date"
    } else {
        "update-available"
    };

    Ok(TunnelUpdateResult {
        status: status.to_string(),
        installed: installed.unwrap_or_default(),
        candidate: candidate.unwrap_or_default(),
    })
}

#[instrument(skip_all)]
pub async fn apply_update(_ctx: TunnelContext) -> Result<TunnelUpdateResult, Error> {
    let policy_output = Command::new("apt-cache")
        .arg("policy")
        .arg("start-tunnel")
        .invoke(ErrorKind::UpdateFailed)
        .await?;

    let policy_str = String::from_utf8_lossy(&policy_output).to_string();
    let installed = parse_version_field(&policy_str, "Installed:");
    let candidate = parse_version_field(&policy_str, "Candidate:");

    // Spawn in a separate cgroup via systemd-run so the process survives
    // when the postinst script restarts start-tunneld.service.
    // After the install completes, reboot the system.
    // Uses --reinstall so the update applies even when versions match.
    Command::new("systemd-run")
        .arg("--scope")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("apt-get install --reinstall -y start-tunnel && reboot")
        .env("DEBIAN_FRONTEND", "noninteractive")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_kind(ErrorKind::UpdateFailed)?;

    Ok(TunnelUpdateResult {
        status: "updating".to_string(),
        installed: installed.unwrap_or_default(),
        candidate: candidate.unwrap_or_default(),
    })
}

fn parse_version_field(policy: &str, field: &str) -> Option<String> {
    policy
        .lines()
        .find(|l| l.trim().starts_with(field))
        .and_then(|l| l.split_whitespace().nth(1))
        .filter(|v| *v != "(none)")
        .map(|s| s.to_string())
}
