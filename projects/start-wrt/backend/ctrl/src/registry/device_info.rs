use http::HeaderValue;
use serde::{Deserialize, Serialize};

use crate::Error;

pub const DEVICE_INFO_HEADER: &str = "X-StartOS-Device-Info";

/// Device information sent to the registry in the X-StartOS-Device-Info header.
/// Wire-compatible with start-os's `DeviceInfo`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub os: OsInfo,
    pub hardware: Option<HardwareInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OsInfo {
    pub version: String,
    /// Semver range this OS is compatible with (e.g. `^0.1.0`).
    pub compat: String,
    pub platform: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    pub arch: String,
    pub ram: u64,
}

impl DeviceInfo {
    /// Load device info from the running system.
    pub fn load() -> Result<Self, Error> {
        let version = env!("CARGO_PKG_VERSION").to_string();
        let compat = compute_compat_range(&version);
        Ok(Self {
            os: OsInfo {
                version,
                compat,
                platform: detect_platform(),
            },
            hardware: Some(HardwareInfo {
                arch: detect_arch(),
                ram: detect_ram(),
            }),
        })
    }

    /// Encode as a URL-query-encoded header value for the registry.
    /// Matches start-os: only `os.*` fields are sent.
    pub fn to_header_value(&self) -> Result<HeaderValue, Error> {
        let mut ser = form_urlencoded::Serializer::new(String::new());
        ser.append_pair("os.version", &self.os.version);
        ser.append_pair("os.compat", &self.os.compat);
        ser.append_pair("os.platform", &self.os.platform);
        let s = ser.finish();
        HeaderValue::from_str(&s).map_err(|e| Error::other(format!("invalid header value: {e}")))
    }
}

/// Derive a caret-range compatibility string from a version string.
/// `0.1.5` → `^0.1.0`. Falls back to `^{version}` if parsing fails.
fn compute_compat_range(version: &str) -> String {
    let v = version.strip_prefix('v').unwrap_or(version);
    let v = v.split('-').next().unwrap_or(v);
    let mut parts = v.splitn(3, '.');
    match (parts.next(), parts.next()) {
        (Some(major), Some(minor)) => format!("^{major}.{minor}.0"),
        _ => format!("^{version}"),
    }
}

/// Detect the platform from the OpenWrt board name.
fn detect_platform() -> String {
    std::fs::read_to_string("/tmp/sysinfo/board_name")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Detect CPU architecture.
fn detect_arch() -> String {
    std::env::consts::ARCH.to_string()
}

/// Detect total RAM in bytes.
fn detect_ram() -> u64 {
    if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                let rest = rest.trim();
                if let Some(kb_str) = rest.strip_suffix("kB").or_else(|| rest.strip_suffix("KB")) {
                    if let Ok(kb) = kb_str.trim().parse::<u64>() {
                        return kb * 1024;
                    }
                }
            }
        }
    }
    0
}
