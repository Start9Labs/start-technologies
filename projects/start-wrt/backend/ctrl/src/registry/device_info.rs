use startos::registry::device_info::{DeviceInfo, OsInfo};

use crate::Error;

/// Device info of the running router.
pub fn load() -> Result<DeviceInfo, Error> {
    let version = env!("CARGO_PKG_VERSION");
    Ok(DeviceInfo {
        os: OsInfo {
            version: version.parse().map_err(Error::other)?,
            compat: compute_compat_range(version)
                .parse()
                .map_err(Error::other)?,
            platform: platform().into(),
            language: None,
        },
        hardware: None,
    })
}

/// The OpenWrt board name.
pub fn platform() -> String {
    std::fs::read_to_string("/tmp/sysinfo/board_name")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_this_build() {
        let info = load().unwrap();
        assert_eq!(info.os.version.to_string(), env!("CARGO_PKG_VERSION"));
        assert!(info.os.version.satisfies(&info.os.compat));
    }
}
