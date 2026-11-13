use std::net::{Ipv4Addr, Ipv6Addr};
use std::process::Command as StdCommand;

use chrono::Utc;
use imbl_value::Value;
use rpc_toolkit::{from_fn_async, from_fn_async_local, HandlerExt, ParentHandler};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uciedit::{dump_all, parse_all, Arena, TypedSection};

use crate::error::{Error, ErrorKind};
use crate::invoke::Invoke;
use crate::prelude::*;
use crate::utils::{DeserializeStdin, HandlerExtSerde as _};
use crate::{CliContext, CtrlContext, ServerContext};

#[derive(Debug, TypedSection, Default)]
#[uci(ty = "preferences")]
pub(crate) struct UciPreferences {
    #[uci(default_value = "\"en_US\".to_string()")]
    pub language: String,
    #[uci(default_value = "\"system\".to_string()")]
    pub theme: String,
}

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
struct SystemInfoResponse {
    version: String,
    git_hash: String,
    language: String,
    date: String,
    theme: String,
    timezone: String,
}
rpc_toolkit::reflect_ts!(SystemInfoResponse);

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
struct VersionInfo {
    version: String,
    release_notes: String,
}
rpc_toolkit::reflect_ts!(VersionInfo);

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
struct SetPreferencesReq {
    language: Option<String>,
    theme: Option<String>,
}
rpc_toolkit::reflect_ts!(SetPreferencesReq);

/// The `startwrt.preferences` section, defaulted when absent.
pub(crate) fn preferences(cfg: &uciedit::Config<'_>) -> Result<UciPreferences, Error> {
    let mut prefs = UciPreferences::default();
    cfg.try_each(|name, p: UciPreferences| {
        if name == Some("preferences") {
            prefs = p;
        }
        Ok::<_, Error>(())
    })?;
    Ok(prefs)
}

#[instrument(skip_all)]
async fn info<C: CtrlContext>(ctx: C) -> Result<SystemInfoResponse, Error> {
    let arena = Arena::new();
    let cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt"]).await?;

    let prefs = preferences(&cfgs["startwrt"])?;

    // Read timezone from system UCI config (spaces → underscores for IANA format)
    let timezone = String::from_utf8(
        tokio::process::Command::new("uci")
            .args(["get", "system.@system[0].zonename"])
            .invoke(ErrorKind::Filesystem.into())
            .await
            .unwrap_or_default(),
    )
    .ok()
    .map(|s| s.trim().replace(' ', "_"))
    .unwrap_or_default();

    Ok(SystemInfoResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        // Same stamp the web build bakes into config.json (check-git-hash.sh
        // format); the UI compares the two to detect a stale cached bundle.
        git_hash: env!("STARTWRT_GIT_HASH").to_string(),
        language: prefs.language,
        date: Utc::now().to_rfc3339(),
        theme: prefs.theme,
        timezone,
    })
}

/// Default registry URL used when no UCI config override exists.
pub(crate) const DEFAULT_REGISTRY_URL: &str = "https://startwrt-registry.start9.com";

async fn newer_versions(_ctx: ServerContext) -> Result<Vec<VersionInfo>, Error> {
    // Read registry URL from UCI config (fallback to default)
    let registry_url = read_registry_url().unwrap_or_else(|| DEFAULT_REGISTRY_URL.to_string());

    let versions = match crate::update::fetch_versions(&registry_url).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("Failed to check for updates: {e}");
            return Ok(Vec::new());
        }
    };

    let platform = crate::registry::device_info::DeviceInfo::load()
        .map(|d| d.os.platform)
        .unwrap_or_else(|_| "unknown".to_string());

    let current: &str = env!("CARGO_PKG_VERSION");

    let mut result: Vec<VersionInfo> = versions
        .into_iter()
        .filter(|(ver, info)| {
            crate::registry::os::asset_for_platform(info, &platform).is_some()
                && is_newer_version(ver, current)
        })
        .map(|(ver, info)| VersionInfo {
            version: ver,
            release_notes: info.release_notes.localized_for("en_US"),
        })
        .collect();
    // Sort ascending by semver precedence. The map above iterates in
    // lexicographic key order, which mis-ranks multi-digit pre-releases
    // (`0.1.0-beta.10` sorts before `0.1.0-beta.9`); the frontend treats the
    // last element as the newest, so the ordering must be semver-correct.
    result.sort_by(|a, b| cmp_versions(&a.version, &b.version));
    Ok(result)
}

/// Parse a version string as semver, tolerating an optional leading `v`.
/// Returns `None` for strings that are not valid semver.
fn parse_version(v: &str) -> Option<semver::Version> {
    semver::Version::parse(v.strip_prefix('v').unwrap_or(v)).ok()
}

/// Compare two version strings by semver precedence.
///
/// This honours the pre-release suffix per the semver spec, so
/// `0.1.0-beta.3 < 0.1.0-beta.4 < 0.1.0`. A plain `(major, minor, patch)`
/// compare collapses every `0.1.0-beta.N` to `0.1.0`, which makes an OTA
/// between two betas never register as "newer". Strings that are not valid
/// semver sort *below* any version that is; two unparseable strings are
/// `Equal`.
pub(crate) fn cmp_versions(a: &str, b: &str) -> std::cmp::Ordering {
    match (parse_version(a), parse_version(b)) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// `true` iff `candidate` is strictly newer than `current` by semver.
fn is_newer_version(candidate: &str, current: &str) -> bool {
    cmp_versions(candidate, current) == std::cmp::Ordering::Greater
}

/// Read the registry URL from UCI config `startwrt.system.registry`.
pub(crate) fn read_registry_url() -> Option<String> {
    StdCommand::new("uci")
        .args(["get", "startwrt.system.registry"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

const VALID_LANGUAGES: &[&str] = &["en_US", "es_ES", "de_DE", "fr_FR", "pl_PL"];
const VALID_THEMES: &[&str] = &["dark", "light", "system"];
pub(crate) fn has_global_ipv6(addrs: &[Ipv6Addr]) -> bool {
    addrs.iter().any(|addr| {
        // 2000::/3 — first 3 bits are 001
        let first = addr.segments()[0];
        (0x2000..=0x3fff).contains(&first)
    })
}

/// Whether `addr` falls within the `prefix_len`-bit network `prefix` (their top
/// `prefix_len` bits match). `prefix_len == 0` matches everything; `> 128`
/// matches nothing.
pub(crate) fn addr_in_prefix(addr: Ipv6Addr, prefix: Ipv6Addr, prefix_len: u8) -> bool {
    if prefix_len == 0 {
        return true;
    }
    if prefix_len > 128 {
        return false;
    }
    let mask = u128::MAX << (128 - prefix_len);
    (u128::from(addr) & mask) == (u128::from(prefix) & mask)
}

pub(crate) async fn get_wan_ipv4() -> Result<Option<Ipv4Addr>, Error> {
    let stdout = match tokio::process::Command::new("ubus")
        .args(["call", "network.interface.wan", "status"])
        .invoke(ErrorKind::Network.into())
        .await
    {
        Ok(out) => out,
        Err(_) => return Ok(None),
    };
    let json: serde_json::Value = match serde_json::from_slice(&stdout) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let addr_str = json
        .pointer("/ipv4-address/0/address")
        .and_then(|v| v.as_str());
    match addr_str {
        Some(s) => Ok(s.parse().ok()),
        None => Ok(None),
    }
}

/// Every IPv4 address on the `wan` interface, in ubus order. Empty when the
/// interface is down or ubus is unavailable.
pub(crate) fn wan_ipv4_addrs() -> Vec<Ipv4Addr> {
    read_wan_ipv4_addrs().unwrap_or_default()
}

/// `None` when ubus is unavailable; empty when the interface is down.
pub(crate) fn read_wan_ipv4_addrs() -> Option<Vec<Ipv4Addr>> {
    let out = StdCommand::new("ubus")
        .args(["call", "network.interface.wan", "status"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let json = serde_json::from_slice::<serde_json::Value>(&out.stdout).ok()?;
    Some(
        json.get("ipv4-address")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| e.get("address")?.as_str()?.parse().ok())
                    .collect()
            })
            .unwrap_or_default(),
    )
}

pub async fn get_wan_ipv6s() -> Result<Vec<Ipv6Addr>, Error> {
    let stdout = match tokio::process::Command::new("ubus")
        .args(["call", "network.interface.wan6", "status"])
        .invoke(ErrorKind::Network.into())
        .await
    {
        Ok(out) => out,
        Err(_) => return Ok(Vec::new()),
    };
    let json: serde_json::Value = match serde_json::from_slice(&stdout) {
        Ok(v) => v,
        Err(_) => return Ok(Vec::new()),
    };
    let mut addrs = Vec::new();
    if let Some(arr) = json.get("ipv6-address").and_then(|v| v.as_array()) {
        for entry in arr {
            if let Some(s) = entry.get("address").and_then(|v| v.as_str()) {
                if let Ok(addr) = s.parse() {
                    addrs.push(addr);
                }
            }
        }
    }
    // Also check prefix-assignment local addresses (DHCPv6-PD)
    if let Some(arr) = json
        .get("ipv6-prefix-assignment")
        .and_then(|v| v.as_array())
    {
        for entry in arr {
            if let Some(s) = entry
                .pointer("/local-address/address")
                .and_then(|v| v.as_str())
            {
                if let Ok(addr) = s.parse() {
                    addrs.push(addr);
                }
            }
        }
    }
    Ok(addrs)
}

/// Length of the largest global prefix delegated to `wan6`, read from netifd.
/// `None` without a global delegation or when ubus is unavailable.
pub async fn read_wan6_delegated_prefix_len() -> Option<u8> {
    let stdout = tokio::process::Command::new("ubus")
        .args(["call", "network.interface.wan6", "status"])
        .invoke(ErrorKind::Network.into())
        .await
        .ok()?;
    let json = serde_json::from_slice::<serde_json::Value>(&stdout).ok()?;
    delegated_prefix_len(&json)
}

fn delegated_prefix_len(status: &serde_json::Value) -> Option<u8> {
    status
        .get("ipv6-prefix")?
        .as_array()?
        .iter()
        .filter(|entry| {
            entry
                .get("address")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<Ipv6Addr>().ok())
                .is_some_and(|addr| has_global_ipv6(&[addr]))
        })
        .filter_map(|entry| u8::try_from(entry.get("mask")?.as_u64()?).ok())
        .min()
}

#[instrument(skip_all)]
async fn set_preferences<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<SetPreferencesReq>,
) -> Result<Value, Error> {
    if let Some(lang) = &req.language {
        if !VALID_LANGUAGES.contains(&lang.as_str()) {
            return Err(Error::new(
                eyre!("invalid language: {lang}"),
                ErrorKind::InvalidValue,
            ));
        }
    }
    if let Some(theme) = &req.theme {
        if !VALID_THEMES.contains(&theme.as_str()) {
            return Err(Error::new(
                eyre!("invalid theme: {theme}"),
                ErrorKind::InvalidValue,
            ));
        }
    }

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt"]).await?;

        // Find existing preferences section or create one
        let mut found = false;
        for section in &mut cfgs["startwrt"].sections {
            if section.name().as_deref() == Some("preferences") {
                if let Some(mut prefs) = section.get_typed::<UciPreferences>()? {
                    if let Some(lang) = &req.language {
                        prefs.language = lang.clone();
                    }
                    if let Some(theme) = &req.theme {
                        prefs.theme = theme.clone();
                    }
                    section.set(&prefs)?;
                    found = true;
                    break;
                }
            }
        }

        if !found {
            let prefs = UciPreferences {
                language: req.language.clone().unwrap_or_else(|| "en_US".to_string()),
                theme: req.theme.clone().unwrap_or_else(|| "system".to_string()),
            };
            cfgs["startwrt"].append(&prefs, Some("preferences"))?;
        }

        match dump_all(ctx.uci_root(), cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => return Err(err.into()),
            Ok(()) => return Ok(Value::Null),
        }
    }
}

#[instrument(skip_all)]
async fn factory_reset(_ctx: ServerContext) -> Result<(), Error> {
    if let Err(e) = tokio::process::Command::new("firstboot")
        .arg("-y")
        .invoke(ErrorKind::Filesystem.into())
        .await
    {
        crate::activity::log(
            "system",
            "factory-reset",
            false,
            "Failed to initiate factory reset",
            Some(&e.to_string()),
        );
        return Err(e.into());
    }

    crate::activity::log(
        "system",
        "factory-reset",
        true,
        "Factory reset initiated",
        None,
    );

    // Spawn reboot after a short delay so the HTTP response can reach the client
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if let Err(e) = tokio::process::Command::new("reboot")
            .invoke(ErrorKind::Filesystem.into())
            .await
        {
            tracing::error!("failed to reboot: {e}");
        }
    });

    Ok(())
}

#[instrument(skip_all)]
async fn restart(_ctx: ServerContext) -> Result<(), Error> {
    crate::activity::log("system", "restart", true, "System restart initiated", None);

    crate::boot::record_reboot(crate::boot::RunState::Restarting);
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if let Err(e) = tokio::process::Command::new("reboot")
            .invoke(ErrorKind::Filesystem.into())
            .await
        {
            crate::boot::record_running();
            tracing::error!("failed to reboot: {e}");
        }
    });

    Ok(())
}

#[derive(Debug, Deserialize, Serialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SetTimezoneParams {
    /// IANA timezone name, e.g. "America/New_York"
    pub timezone: String,
}
rpc_toolkit::reflect_ts!(SetTimezoneParams);

/// Resolve a POSIX TZ string for an IANA name using the on-device LuCI
/// zoneinfo table — the same authoritative source LuCI itself uses. The table
/// (`/usr/share/ucode/luci/zoneinfo.uc`, autogenerated from tzdata) is exposed
/// over ubus as `luci.getTimezones`, which returns
/// `{ "<IANA>": { "tzstring": "<POSIX>", ... }, ... }`. Returns `None` if the
/// name isn't in the table. We never ship or hand-maintain this mapping.
pub(crate) async fn resolve_posix_tz(iana: &str) -> Result<Option<String>, Error> {
    // UTC isn't in the generated table (LuCI prepends it as a special case); the
    // POSIX string "UTC" is what OpenWrt ships as the default in /etc/config/system.
    if iana == "UTC" || iana == "Etc/UTC" {
        return Ok(Some("UTC".to_string()));
    }
    let output = tokio::process::Command::new("ubus")
        .args(["call", "luci", "getTimezones"])
        .invoke(ErrorKind::Filesystem.into())
        .await?;
    let json: serde_json::Value = serde_json::from_slice(&output)
        .map_err(|e| Error::new(eyre!("parsing getTimezones: {e}"), ErrorKind::Filesystem))?;
    Ok(json
        .get(iana)
        .and_then(|v| v.get("tzstring"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

/// List every IANA timezone name the device can resolve, sourced from the
/// on-device LuCI zoneinfo table (`ubus call luci getTimezones`). Backs the
/// settings dropdown so the offered list is exactly the set the device can set.
async fn get_timezones(_ctx: ServerContext) -> Result<Vec<String>, Error> {
    let output = tokio::process::Command::new("ubus")
        .args(["call", "luci", "getTimezones"])
        .invoke(ErrorKind::Filesystem.into())
        .await?;
    let json: serde_json::Value = serde_json::from_slice(&output)
        .map_err(|e| Error::new(eyre!("parsing getTimezones: {e}"), ErrorKind::Filesystem))?;
    let mut zones: Vec<String> = json
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    zones.sort();
    // UTC isn't in the table; LuCI prepends it as an explicit choice. Mirror that.
    let mut result = vec!["UTC".to_string()];
    result.extend(zones);
    Ok(result)
}

/// Set the system timezone.
///
/// The frontend sends only the IANA name; the backend resolves the matching
/// POSIX TZ string from the device's authoritative LuCI zoneinfo table
/// (`resolve_posix_tz`). OpenWrt writes the POSIX string to `/etc/TZ` on
/// `system reload`, which makes `date`, `cron`, and all libc time functions use
/// the correct local time. No `zoneinfo` package needed. crond caches the zone
/// at launch, so we restart it afterward to pick up the new `/etc/TZ`.
#[instrument(skip_all)]
pub(crate) async fn set_timezone<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(params): DeserializeStdin<SetTimezoneParams>,
) -> Result<(), Error> {
    if !ctx.effectful() {
        return Ok(());
    }

    // Store the IANA name verbatim (underscores), matching modern LuCI's writer
    // and the zoneinfo table keys, so `luci.getTimezones`' active flag agrees.
    let zonename = params.timezone.as_str();

    // Skip if unchanged
    let current = String::from_utf8(
        tokio::process::Command::new("uci")
            .args(["get", "system.@system[0].zonename"])
            .invoke(ErrorKind::UciEdit.into())
            .await
            .unwrap_or_default(),
    )
    .unwrap_or_default();
    if current.trim() == zonename {
        return Ok(());
    }

    // Resolve the POSIX TZ string from the device's authoritative table.
    // An unknown name (e.g. an exotic browser zone during setup) leaves the
    // device on its existing zone — UTC by default — so log that fallback.
    let posix_tz = match resolve_posix_tz(&params.timezone).await? {
        Some(tz) => tz,
        None => {
            crate::activity::log(
                "system",
                "timezone-updated",
                false,
                &format!("Timezone '{}' not recognized; kept UTC", params.timezone),
                Some("not found in zoneinfo table"),
            );
            return Err(Error::new(
                eyre!("unknown timezone: {}", params.timezone),
                ErrorKind::InvalidRequest,
            ));
        }
    };

    // Set the display name (IANA)
    crate::run_quiet_async(
        tokio::process::Command::new("uci")
            .args(["set", &format!("system.@system[0].zonename={zonename}")]),
    )
    .await
    .map_err(|e| Error::new(eyre!("setting zonename: {e}"), ErrorKind::UciEdit))?;

    // Set the POSIX TZ string — this is what actually controls time conversion
    crate::run_quiet_async(
        tokio::process::Command::new("uci")
            .args(["set", &format!("system.@system[0].timezone={posix_tz}")]),
    )
    .await
    .map_err(|e| Error::new(eyre!("setting timezone: {e}"), ErrorKind::UciEdit))?;

    crate::run_quiet_async(tokio::process::Command::new("uci").args(["commit", "system"]))
        .await
        .map_err(|e| Error::new(eyre!("committing system config: {e}"), ErrorKind::UciEdit))?;

    // Reload writes the POSIX TZ string to /etc/TZ
    crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/system").arg("reload"))
        .await
        .map_err(|e| Error::new(eyre!("reloading system: {e}"), ErrorKind::Filesystem))?;

    // crond reads its zone from /etc/TZ at launch and caches it, so restart it
    // to re-base wall-clock schedules (WiFi blackout, WAN) on the new local time.
    crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/cron").arg("restart"))
        .await
        .map_err(|e| Error::new(eyre!("restarting cron: {e}"), ErrorKind::Filesystem))?;
    crate::wifi::apply_blackout(ctx.uci_root()).await?;

    crate::activity::log(
        "system",
        "timezone-updated",
        true,
        &format!("Set timezone to {}", params.timezone),
        None,
    );

    Ok(())
}

pub fn system<C: CtrlContext>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "info",
            from_fn_async_local(info::<C>)
                .with_metadata("authenticated", Value::Bool(false))
                .with_display_serializable(),
        )
        .subcommand(
            "newer-versions",
            from_fn_async(newer_versions)
                .with_metadata("authenticated", Value::Bool(false))
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "update",
            from_fn_async(crate::update::update_system)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-preferences",
            from_fn_async_local(set_preferences::<C>).with_display_serializable(),
        )
        .subcommand(
            "restart",
            from_fn_async(restart)
                .no_display()
                .with_about("Reboot the device")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "factory-reset",
            from_fn_async(factory_reset)
                .no_display()
                .with_about("Wipe overlay and reboot (factory reset)")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-timezone",
            from_fn_async(set_timezone::<C>).no_display(),
        )
        .subcommand(
            "get-timezones",
            from_fn_async(get_timezones)
                .with_metadata("authenticated", Value::Bool(false))
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "logs",
            from_fn_async(crate::logs::get_logs)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "logs-follow",
            from_fn_async(crate::logs::follow_logs)
                .with_metadata("get_signer", Value::Bool(true))
                .no_cli(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cmp_versions_prerelease() {
        use std::cmp::Ordering;
        // Pre-release ordering — the case the old tuple compare got wrong.
        assert_eq!(cmp_versions("0.1.0-beta.3", "0.1.0-beta.4"), Ordering::Less);
        assert_eq!(
            cmp_versions("0.1.0-beta.4", "0.1.0-beta.3"),
            Ordering::Greater
        );
        assert_eq!(
            cmp_versions("0.1.0-beta.3", "0.1.0-beta.3"),
            Ordering::Equal
        );
        // A pre-release is older than its own final release.
        assert_eq!(cmp_versions("0.1.0-beta.4", "0.1.0"), Ordering::Less);
        // The numeric core still dominates the pre-release tag.
        assert_eq!(cmp_versions("0.2.0-beta.1", "0.1.0"), Ordering::Greater);
        assert_eq!(cmp_versions("v1.2.3", "1.2.3"), Ordering::Equal);
        // Unparseable strings sort below anything that parses.
        assert_eq!(cmp_versions("garbage", "0.1.0-beta.3"), Ordering::Less);
        assert_eq!(cmp_versions("nonsense", "rubbish"), Ordering::Equal);

        assert!(is_newer_version("0.1.0-beta.4", "0.1.0-beta.3"));
        assert!(!is_newer_version("0.1.0-beta.3", "0.1.0-beta.4"));
        assert!(!is_newer_version("0.1.0-beta.3", "0.1.0-beta.3"));
    }

    #[test]
    fn test_has_global_ipv6() {
        let gua: Ipv6Addr = "2001:db8::1".parse().unwrap();
        let ula: Ipv6Addr = "fd00::1".parse().unwrap();
        let link_local: Ipv6Addr = "fe80::1".parse().unwrap();

        assert!(has_global_ipv6(&[gua]));
        assert!(has_global_ipv6(&[ula, gua]));
        assert!(!has_global_ipv6(&[ula]));
        assert!(!has_global_ipv6(&[link_local]));
        assert!(!has_global_ipv6(&[]));

        // 2000::/3 boundaries (is_gua now relies on this range).
        assert!(!has_global_ipv6(&["1fff::1".parse().unwrap()])); // just below
        assert!(has_global_ipv6(&["2000::1".parse().unwrap()])); // lower edge
        assert!(has_global_ipv6(&["3fff::1".parse().unwrap()])); // upper edge
        assert!(!has_global_ipv6(&["4000::1".parse().unwrap()])); // just above
    }

    #[test]
    fn delegated_prefix_len_reads_netifd_mask() {
        let status = serde_json::json!({
            "ipv6-prefix": [
                { "address": "fd12:3456:789a::", "mask": 48 },
                { "address": "2001:db8:0:100::", "mask": 64 },
            ]
        });
        assert_eq!(delegated_prefix_len(&status), Some(64));

        let status = serde_json::json!({
            "ipv6-prefix": [
                { "address": "2001:db8:0:100::", "mask": 60 },
                { "address": "2001:db8:1::", "mask": 56 },
            ]
        });
        assert_eq!(delegated_prefix_len(&status), Some(56));

        let status = serde_json::json!({ "ipv6-prefix": [] });
        assert_eq!(delegated_prefix_len(&status), None);
        assert_eq!(delegated_prefix_len(&serde_json::json!({})), None);
    }
}
