use crate::error::{Error, ErrorKind};
use crate::prelude::*;
use crate::utils::{DeserializeStdin, HandlerExtSerde as _};
use crate::{CliContext, CtrlContext, ServerContext};
use chrono::Utc;
use imbl_value::Value;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use rpc_toolkit::{from_fn, from_fn_async, from_fn_async_local, HandlerExt, ParentHandler};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::process::Command as StdCommand;
use crate::invoke::Invoke;
use std::{fs, path::Path};
use tracing::instrument;
use uciedit::openwrt::{FirewallRule, FirewallTarget};
use uciedit::{dump_all, parse_all, Arena, Configs, TypedSection};

#[derive(Debug, TypedSection, Default)]
#[uci(ty = "preferences")]
pub(crate) struct UciPreferences {
    #[uci(default_value = "\"en_US\".to_string()")]
    pub language: String,
    #[uci(default_value = "\"system\".to_string()")]
    pub theme: String,
    #[uci(default_value = "\"default\".to_string()")]
    pub remote_access: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemInfoResponse {
    version: String,
    language: String,
    date: String,
    theme: String,
    remote_access: String,
    timezone: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionInfo {
    version: String,
    release_notes: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetPreferencesReq {
    language: Option<String>,
    theme: Option<String>,
    remote_access: Option<String>,
}

#[instrument(skip_all)]
async fn info<C: CtrlContext>(ctx: C) -> Result<SystemInfoResponse, Error> {
    let arena = Arena::new();
    let cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt"]).await?;

    let mut prefs = UciPreferences::default();
    cfgs["startwrt"].try_each(|name, p: UciPreferences| {
        if name == Some("preferences") {
            prefs = p;
        }
        Ok::<_, Error>(())
    })?;

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
        language: prefs.language,
        date: Utc::now().to_rfc3339(),
        theme: prefs.theme,
        remote_access: prefs.remote_access,
        timezone,
    })
}

/// Default registry URL used when no UCI config override exists.
pub(crate) const DEFAULT_REGISTRY_URL: &str = "https://startwrt-registry.start9.com";

async fn newer_versions(_ctx: ServerContext) -> Result<Vec<VersionInfo>, Error> {
    // Read registry URL from UCI config (fallback to default)
    let registry_url = read_registry_url().unwrap_or_else(|| DEFAULT_REGISTRY_URL.to_string());

    let versions = match crate::update::fetch_newer_versions(&registry_url).await
    {
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
            // Only include versions that have an asset for our platform
            // and are newer than the current version (semver comparison)
            info.asset_for_platform(&platform).is_some()
                && is_newer_version(ver, current)
        })
        .map(|(ver, info)| VersionInfo {
            version: ver,
            release_notes: info.release_notes,
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
const VALID_REMOTE_ACCESS: &[&str] = &["default", "never", "always"];

const REMOTE_RULE_PREFIX: &str = "startwrt_remote_";
const REMOTE_ACCESS_PORTS: &[&str] = &["80", "443", "22"];

/// ULA range (RFC 4193). Disjoint from global unicast (`2000::/3`).
const ULA_PREFIX: &str = "fc00::/7";

/// The three RFC1918 ranges, as `(name suffix, CIDR)`. In `default` mode an IPv4
/// remote-access rule is emitted per range so that any private source is accepted
/// while no global one is. fw4's `option src_ip` holds a single value, so each
/// range needs its own rule; the suffix keeps the generated section names unique.
const RFC1918_BLOCKS: &[(&str, &str)] = &[
    ("ipv4_10", "10.0.0.0/8"),
    ("ipv4_172", "172.16.0.0/12"),
    ("ipv4_192", "192.168.0.0/16"),
];

fn is_private_ipv4(ip: &Ipv4Addr) -> bool {
    let octets = ip.octets();
    // RFC1918 only — excludes CGNAT 100.64.0.0/10
    octets[0] == 10
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
}

pub(crate) fn has_global_ipv6(addrs: &[Ipv6Addr]) -> bool {
    addrs.iter().any(|addr| {
        // 2000::/3 — first 3 bits are 001
        let first = addr.segments()[0];
        (0x2000..=0x3fff).contains(&first)
    })
}

async fn get_wan_ipv4() -> Result<Option<Ipv4Addr>, Error> {
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
    if let Some(arr) = json.get("ipv6-prefix-assignment").and_then(|v| v.as_array()) {
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

fn apply_remote_access_config(
    cfgs: &mut Configs,
    mode: &str,
    wan_ipv4: Option<Ipv4Addr>,
    wan_ipv6s: &[Ipv6Addr],
) {
    // Remove all existing remote access rules
    cfgs["firewall"].sections.retain(|section| {
        let Ok(rule) = section.get::<FirewallRule>() else {
            return true;
        };
        !rule.name.starts_with(REMOTE_RULE_PREFIX)
    });

    // Canonical OpenWrt access restriction: limit who may connect via `src_ip`
    // (mirrors the shipped `Allow-MLD` rule's `src_ip fe80::/10`). In `default`
    // mode, accept only locally-scoped sources — all RFC1918 ranges for IPv4, the
    // ULA supernet for IPv6 — so a globally-routable client can never reach these
    // ports, even if a global address later appears on the WAN. `always` is an
    // explicit opt-in to full exposure and stays unscoped; `never` opens nothing.
    struct RuleSpec {
        /// Disambiguates the section name when one family yields several rules
        /// (one per RFC1918 range). `None` for the unscoped `always` rule.
        suffix: Option<&'static str>,
        family: Option<&'static str>,
        src_ip: Option<&'static str>,
    }
    let specs: Vec<RuleSpec> = match mode {
        "never" => return,
        "always" => vec![RuleSpec {
            suffix: None,
            family: None,
            src_ip: None,
        }],
        "default" => {
            let mut specs = Vec::new();
            // Behind NAT on IPv4 → accept every private (RFC1918) source, but no
            // global one. Gated on the WAN address itself being private.
            if wan_ipv4.is_some_and(|ip| is_private_ipv4(&ip)) {
                for &(suffix, block) in RFC1918_BLOCKS {
                    specs.push(RuleSpec {
                        suffix: Some(suffix),
                        family: Some("ipv4"),
                        src_ip: Some(block),
                    });
                }
            }
            if !wan_ipv6s.is_empty() && !has_global_ipv6(wan_ipv6s) {
                specs.push(RuleSpec {
                    suffix: Some("ipv6"),
                    family: Some("ipv6"),
                    src_ip: Some(ULA_PREFIX),
                });
            }
            specs
        }
        _ => return,
    };

    for spec in &specs {
        for &port in REMOTE_ACCESS_PORTS {
            let name = match spec.suffix {
                Some(suffix) => format!("{REMOTE_RULE_PREFIX}{port}_{suffix}"),
                None => format!("{REMOTE_RULE_PREFIX}{port}"),
            };
            let rule = FirewallRule {
                name: name.clone(),
                src: "wan".to_string(),
                src_ip: spec.src_ip.map(str::to_string),
                dest_port: Some(port.to_string()),
                proto: vec!["tcp".to_string()],
                target: FirewallTarget::ACCEPT,
                family: spec.family.map(str::to_string),
                ..Default::default()
            };
            cfgs["firewall"].append(&rule, Some(name.as_str())).ok();
        }
    }
}

/// Convert an IPv6 address to the hex format used in `/proc/net/tcp6`.
/// Each 32-bit group is stored in little-endian byte order.
fn ipv6_to_proc_hex(addr: Ipv6Addr) -> String {
    let octets = addr.octets();
    let mut hex = String::with_capacity(32);
    // /proc/net/tcp6 stores the address as 4 little-endian 32-bit words
    for chunk in octets.chunks(4) {
        for &byte in chunk.iter().rev() {
            hex.push_str(&format!("{byte:02X}"));
        }
    }
    hex
}

/// Parse `/proc/net/tcp` and collect socket inodes for ESTABLISHED connections
/// on port 22 where the local address matches the given WAN IPv4.
fn collect_tcp4_ssh_inodes(wan_ip: Ipv4Addr, inodes: &mut HashSet<u64>) -> io::Result<()> {
    let content = fs::read_to_string("/proc/net/tcp")?;
    let wan_hex = {
        let octets = wan_ip.octets();
        format!(
            "{:02X}{:02X}{:02X}{:02X}",
            octets[3], octets[2], octets[1], octets[0]
        )
    };
    let local_match = format!("{wan_hex}:0016"); // port 22 in hex

    for line in content.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 {
            continue;
        }
        // fields[1] = local_address:port, fields[3] = connection state
        // state "01" = ESTABLISHED
        if fields[3] != "01" {
            continue;
        }
        if fields[1].eq_ignore_ascii_case(&local_match) {
            if let Ok(inode) = fields[9].parse::<u64>() {
                if inode != 0 {
                    inodes.insert(inode);
                }
            }
        }
    }
    Ok(())
}

/// Parse `/proc/net/tcp6` and collect socket inodes for ESTABLISHED connections
/// on port 22 where the local address matches the given WAN IPv6.
fn collect_tcp6_ssh_inodes(wan_ip: Ipv6Addr, inodes: &mut HashSet<u64>) -> io::Result<()> {
    let content = fs::read_to_string("/proc/net/tcp6")?;
    let wan_hex = ipv6_to_proc_hex(wan_ip);
    let local_match = format!("{wan_hex}:0016"); // port 22 in hex

    for line in content.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 {
            continue;
        }
        if fields[3] != "01" {
            continue;
        }
        if fields[1].eq_ignore_ascii_case(&local_match) {
            if let Ok(inode) = fields[9].parse::<u64>() {
                if inode != 0 {
                    inodes.insert(inode);
                }
            }
        }
    }
    Ok(())
}

/// Scan all `/proc/<pid>/fd/` entries for socket inodes matching the given set.
/// If a match is found and the process is `dropbear`, send SIGTERM.
fn kill_dropbear_by_inodes(inodes: &HashSet<u64>) {
    let proc = Path::new("/proc");
    let Ok(entries) = fs::read_dir(proc) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid_str) = name.to_str() else {
            continue;
        };
        let Ok(pid_num) = pid_str.parse::<i32>() else {
            continue;
        };

        let fd_dir = proc.join(pid_str).join("fd");
        let Ok(fds) = fs::read_dir(&fd_dir) else {
            continue;
        };

        let mut matched = false;
        for fd in fds.flatten() {
            let Ok(link) = fs::read_link(fd.path()) else {
                continue;
            };
            let link_str = link.to_string_lossy();
            // symlink target looks like "socket:[12345]"
            if let Some(inode_str) = link_str.strip_prefix("socket:[").and_then(|s| s.strip_suffix(']')) {
                if let Ok(inode) = inode_str.parse::<u64>() {
                    if inodes.contains(&inode) {
                        matched = true;
                        break;
                    }
                }
            }
        }

        if matched {
            // Verify this is a dropbear process before killing
            let comm_path = proc.join(pid_str).join("comm");
            if let Ok(comm) = fs::read_to_string(&comm_path) {
                if comm.trim() == "dropbear" {
                    let _ = signal::kill(Pid::from_raw(pid_num), Signal::SIGTERM);
                }
            }
        }
    }
}

/// Kill WAN-side SSH (dropbear) sessions by finding their socket inodes in
/// `/proc/net/tcp{,6}` and matching them to process FDs.
fn kill_wan_ssh_sessions(wan_ipv4: Option<Ipv4Addr>, wan_ipv6s: &[Ipv6Addr]) {
    if wan_ipv4.is_none() && wan_ipv6s.is_empty() {
        return;
    }

    let mut inodes = HashSet::new();
    if let Some(ip) = wan_ipv4 {
        let _ = collect_tcp4_ssh_inodes(ip, &mut inodes);
    }
    for &ip in wan_ipv6s {
        let _ = collect_tcp6_ssh_inodes(ip, &mut inodes);
    }

    if !inodes.is_empty() {
        kill_dropbear_by_inodes(&inodes);
    }
}

fn reload_firewall(wan_ipv4: Option<Ipv4Addr>, wan_ipv6s: Vec<Ipv6Addr>) {
    tokio::spawn(async move {
        let _ = crate::run_quiet_async(
            tokio::process::Command::new("/etc/init.d/firewall").arg("reload"),
        )
        .await;
        // Kill WAN-side SSH sessions so they don't survive firewall changes
        // via conntrack ESTABLISHED state. HTTP sessions disconnect naturally
        // on the next request. Only dropbear processes are killed.
        kill_wan_ssh_sessions(wan_ipv4, &wan_ipv6s);
    });
}

#[instrument(skip_all)]
pub async fn apply_remote_access<C: CtrlContext>(ctx: C) -> Result<Value, Error> {
    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt", "firewall"]).await?;

        let mut prefs = UciPreferences::default();
        cfgs["startwrt"].try_each(|name, p: UciPreferences| {
            if name == Some("preferences") {
                prefs = p;
            }
            Ok::<_, Error>(())
        })?;

        let (wan_ipv4, wan_ipv6s) = if ctx.effectful() {
            (get_wan_ipv4().await?, get_wan_ipv6s().await?)
        } else {
            (None, Vec::new())
        };

        apply_remote_access_config(&mut cfgs, &prefs.remote_access, wan_ipv4, &wan_ipv6s);

        match dump_all(ctx.uci_root(), cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                return Err(err.into());
            }
            Ok(()) => {
                if ctx.effectful() {
                    reload_firewall(wan_ipv4, wan_ipv6s);
                }
                return Ok(Value::Null);
            }
        }
    }
}

#[instrument(skip_all)]
async fn set_preferences<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<SetPreferencesReq>,
) -> Result<Value, Error> {
    if let Some(lang) = &req.language {
        if !VALID_LANGUAGES.contains(&lang.as_str()) {
            return Err(Error::new(eyre!("invalid language: {lang}"), ErrorKind::InvalidValue));
        }
    }
    if let Some(theme) = &req.theme {
        if !VALID_THEMES.contains(&theme.as_str()) {
            return Err(Error::new(eyre!("invalid theme: {theme}"), ErrorKind::InvalidValue));
        }
    }
    if let Some(ra) = &req.remote_access {
        if !VALID_REMOTE_ACCESS.contains(&ra.as_str()) {
            return Err(Error::new(eyre!("invalid remoteAccess: {ra}"), ErrorKind::InvalidValue));
        }
    }

    let needs_firewall = req.remote_access.is_some();
    let configs: &[&str] = if needs_firewall {
        &["startwrt", "firewall"]
    } else {
        &["startwrt"]
    };

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, configs).await?;

        // Find existing preferences section or create one
        let mut found = false;
        let mut new_mode = None;
        for section in &mut cfgs["startwrt"].sections {
            if section.name().as_deref() == Some("preferences") {
                if let Some(mut prefs) = section.get_typed::<UciPreferences>()? {
                    if let Some(lang) = &req.language {
                        prefs.language = lang.clone();
                    }
                    if let Some(theme) = &req.theme {
                        prefs.theme = theme.clone();
                    }
                    if let Some(ra) = &req.remote_access {
                        new_mode = Some(ra.clone());
                        prefs.remote_access = ra.clone();
                    }
                    section.set(&prefs)?;
                    found = true;
                    break;
                }
            }
        }

        if !found {
            let ra = req
                .remote_access
                .clone()
                .unwrap_or_else(|| "default".to_string());
            if req.remote_access.is_some() {
                new_mode = Some(ra.clone());
            }
            let prefs = UciPreferences {
                language: req.language.clone().unwrap_or_else(|| "en_US".to_string()),
                theme: req.theme.clone().unwrap_or_else(|| "system".to_string()),
                remote_access: ra,
            };
            cfgs["startwrt"].append(&prefs, Some("preferences"))?;
        }

        // Apply firewall rules if remote_access changed
        if let Some(mode) = &new_mode {
            let (wan_ipv4, wan_ipv6s) = if ctx.effectful() {
                (get_wan_ipv4().await?, get_wan_ipv6s().await?)
            } else {
                (None, Vec::new())
            };
            apply_remote_access_config(&mut cfgs, mode, wan_ipv4, &wan_ipv6s);
        }

        match dump_all(ctx.uci_root(), cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                if needs_firewall {
                    crate::activity::log("system", "remote-access", false, &format!("Failed to update remote access to '{}'", new_mode.as_deref().unwrap_or("unknown")), Some(&err.to_string()));
                }
                return Err(err.into());
            }
            Ok(()) => {
                if new_mode.is_some() {
                    crate::activity::log("system", "remote-access", true, &format!("Updated remote access to '{}'", new_mode.as_deref().unwrap_or("unknown")), None);
                    if ctx.effectful() {
                        let (wan_ipv4, wan_ipv6s) = (get_wan_ipv4().await?, get_wan_ipv6s().await?);
                        reload_firewall(wan_ipv4, wan_ipv6s);
                    }
                }
                return Ok(Value::Null);
            }
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
        crate::activity::log("system", "factory-reset", false, "Failed to initiate factory reset", Some(&e.to_string()));
        return Err(e.into());
    }

    crate::activity::log("system", "factory-reset", true, "Factory reset initiated", None);

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

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SetTimezoneParams {
    /// IANA timezone name, e.g. "America/New_York"
    timezone: String,
}

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
async fn set_timezone<C: CtrlContext>(
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
        tokio::process::Command::new("uci").args([
            "set",
            &format!("system.@system[0].zonename={zonename}"),
        ]),
    )
    .await
    .map_err(|e| Error::new(eyre!("setting zonename: {e}"), ErrorKind::UciEdit))?;

    // Set the POSIX TZ string — this is what actually controls time conversion
    crate::run_quiet_async(
        tokio::process::Command::new("uci").args([
            "set",
            &format!("system.@system[0].timezone={posix_tz}"),
        ]),
    )
    .await
    .map_err(|e| Error::new(eyre!("setting timezone: {e}"), ErrorKind::UciEdit))?;

    crate::run_quiet_async(tokio::process::Command::new("uci").args(["commit", "system"]))
        .await
        .map_err(|e| Error::new(eyre!("committing system config: {e}"), ErrorKind::UciEdit))?;

    // Reload writes the POSIX TZ string to /etc/TZ
    crate::run_quiet_async(
        tokio::process::Command::new("/etc/init.d/system").arg("reload"),
    )
    .await
    .map_err(|e| Error::new(eyre!("reloading system: {e}"), ErrorKind::Filesystem))?;

    // crond reads its zone from /etc/TZ at launch and caches it, so restart it
    // to re-base wall-clock schedules (WiFi blackout, WAN) on the new local time.
    crate::run_quiet_async(
        tokio::process::Command::new("/etc/init.d/cron").arg("restart"),
    )
    .await
    .map_err(|e| Error::new(eyre!("restarting cron: {e}"), ErrorKind::Filesystem))?;

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
                .with_metadata("no_auth", Value::Bool(true))
                .with_display_serializable(),
        )
        .subcommand(
            "newer-versions",
            from_fn_async(newer_versions)
                .with_metadata("no_auth", Value::Bool(true))
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
            "apply-remote-access",
            from_fn_async_local(apply_remote_access::<C>)
                .with_metadata("no_auth", Value::Bool(true))
                .no_display(),
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
            from_fn_async(set_timezone::<C>)
                .with_metadata("no_auth", Value::Bool(true))
                .no_display(),
        )
        .subcommand(
            "get-timezones",
            from_fn_async(get_timezones)
                .with_metadata("no_auth", Value::Bool(true))
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "logs",
            from_fn_async(crate::logs::get_logs)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
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
        assert_eq!(cmp_versions("0.1.0-beta.4", "0.1.0-beta.3"), Ordering::Greater);
        assert_eq!(cmp_versions("0.1.0-beta.3", "0.1.0-beta.3"), Ordering::Equal);
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
    fn test_is_private_ipv4() {
        // RFC1918 ranges
        assert!(is_private_ipv4(&"10.0.0.1".parse().unwrap()));
        assert!(is_private_ipv4(&"10.255.255.255".parse().unwrap()));
        assert!(is_private_ipv4(&"172.16.0.1".parse().unwrap()));
        assert!(is_private_ipv4(&"172.31.255.255".parse().unwrap()));
        assert!(is_private_ipv4(&"192.168.0.1".parse().unwrap()));
        assert!(is_private_ipv4(&"192.168.255.255".parse().unwrap()));

        // Not private
        assert!(!is_private_ipv4(&"8.8.8.8".parse().unwrap()));
        assert!(!is_private_ipv4(&"100.64.0.1".parse().unwrap())); // CGNAT
        assert!(!is_private_ipv4(&"172.32.0.1".parse().unwrap()));
        assert!(!is_private_ipv4(&"1.2.3.4".parse().unwrap()));
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

    fn setup_firewall_config(dir: &std::path::Path, startwrt_content: &str) {
        std::fs::write(dir.join("startwrt"), startwrt_content).unwrap();
        std::fs::write(
            dir.join("firewall"),
            "\
config zone wan
\toption name 'wan'
\toption input 'REJECT'
\toption output 'ACCEPT'
\toption forward 'REJECT'
",
        )
        .unwrap();
    }

    async fn count_remote_rules(dir: &std::path::Path) -> usize {
        let arena = Arena::new();
        let cfgs = parse_all(dir, &arena, &["firewall"]).await.unwrap();
        cfgs["firewall"]
            .sections
            .iter()
            .filter(|s| {
                s.get::<FirewallRule>()
                    .ok()
                    .map_or(false, |r| r.name.starts_with(REMOTE_RULE_PREFIX))
            })
            .count()
    }

    async fn get_remote_rule_families(dir: &std::path::Path) -> Vec<Option<String>> {
        let arena = Arena::new();
        let cfgs = parse_all(dir, &arena, &["firewall"]).await.unwrap();
        cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| {
                let rule = s.get::<FirewallRule>().ok()?;
                if rule.name.starts_with(REMOTE_RULE_PREFIX) {
                    Some(rule.family.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    async fn get_remote_rule_src_ips(dir: &std::path::Path) -> Vec<Option<String>> {
        let arena = Arena::new();
        let cfgs = parse_all(dir, &arena, &["firewall"]).await.unwrap();
        cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| {
                let rule = s.get::<FirewallRule>().ok()?;
                if rule.name.starts_with(REMOTE_RULE_PREFIX) {
                    Some(rule.src_ip.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    #[tokio::test]
    async fn test_apply_always() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall_config(
            dir.path(),
            "config preferences preferences\n\toption remote_access 'always'\n",
        );

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["startwrt", "firewall"]).await.unwrap();
        apply_remote_access_config(&mut cfgs, "always", None, &[]);
        dump_all(dir.path(), cfgs).await.unwrap();

        assert_eq!(count_remote_rules(dir.path()).await, 3);
        // All rules should have no family restriction
        for family in get_remote_rule_families(dir.path()).await {
            assert_eq!(family, None);
        }
        // `always` is an explicit opt-in: rules stay unscoped (any source).
        for src_ip in get_remote_rule_src_ips(dir.path()).await {
            assert_eq!(src_ip, None);
        }
    }

    #[tokio::test]
    async fn test_apply_never() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall_config(
            dir.path(),
            "config preferences preferences\n\toption remote_access 'never'\n",
        );

        // First add some rules
        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["startwrt", "firewall"]).await.unwrap();
        apply_remote_access_config(&mut cfgs, "always", None, &[]);
        dump_all(dir.path(), cfgs).await.unwrap();
        assert_eq!(count_remote_rules(dir.path()).await, 3);

        // Now apply "never" — should remove them all
        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["startwrt", "firewall"]).await.unwrap();
        apply_remote_access_config(&mut cfgs, "never", None, &[]);
        dump_all(dir.path(), cfgs).await.unwrap();
        assert_eq!(count_remote_rules(dir.path()).await, 0);
    }

    #[tokio::test]
    async fn test_apply_default_ipv4_private_ipv6_public() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall_config(
            dir.path(),
            "config preferences preferences\n\toption remote_access 'default'\n",
        );

        let wan_ipv4: Ipv4Addr = "192.168.1.1".parse().unwrap();
        let wan_ipv6: Ipv6Addr = "2001:db8::1".parse().unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["startwrt", "firewall"]).await.unwrap();
        apply_remote_access_config(&mut cfgs, "default", Some(wan_ipv4), &[wan_ipv6]);
        dump_all(dir.path(), cfgs).await.unwrap();

        // Public IPv6 → no IPv6 rule. IPv4 behind NAT → one rule per RFC1918
        // range × 3 ports = 9 rules.
        assert_eq!(count_remote_rules(dir.path()).await, 9);
        for family in get_remote_rule_families(dir.path()).await {
            assert_eq!(family, Some("ipv4".to_string()));
        }
        let src_ips = get_remote_rule_src_ips(dir.path()).await;
        assert!(src_ips.iter().all(|s| s.is_some()));
        assert!(src_ips.iter().any(|s| s.as_deref() == Some("10.0.0.0/8")));
        assert!(src_ips.iter().any(|s| s.as_deref() == Some("172.16.0.0/12")));
        assert!(src_ips.iter().any(|s| s.as_deref() == Some("192.168.0.0/16")));
    }

    #[tokio::test]
    async fn test_apply_default_both_private() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall_config(
            dir.path(),
            "config preferences preferences\n\toption remote_access 'default'\n",
        );

        let wan_ipv4: Ipv4Addr = "10.0.0.1".parse().unwrap();
        let wan_ipv6: Ipv6Addr = "fd00::1".parse().unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["startwrt", "firewall"]).await.unwrap();
        apply_remote_access_config(&mut cfgs, "default", Some(wan_ipv4), &[wan_ipv6]);
        dump_all(dir.path(), cfgs).await.unwrap();

        // Both private — IPv4 gets one rule per RFC1918 range, IPv6 one for the
        // ULA supernet. (3 ranges + 1) × 3 ports = 12 rules.
        assert_eq!(count_remote_rules(dir.path()).await, 12);
        let families = get_remote_rule_families(dir.path()).await;
        assert_eq!(
            families.iter().filter(|f| f.as_deref() == Some("ipv4")).count(),
            9
        );
        assert_eq!(
            families.iter().filter(|f| f.as_deref() == Some("ipv6")).count(),
            3
        );
        let src_ips = get_remote_rule_src_ips(dir.path()).await;
        assert!(
            src_ips.iter().all(|s| s.is_some()),
            "default-mode rules must be src_ip-scoped: {src_ips:?}"
        );
        assert!(src_ips.iter().any(|s| s.as_deref() == Some("10.0.0.0/8")));
        assert!(src_ips.iter().any(|s| s.as_deref() == Some("172.16.0.0/12")));
        assert!(src_ips.iter().any(|s| s.as_deref() == Some("192.168.0.0/16")));
        assert!(src_ips.iter().any(|s| s.as_deref() == Some("fc00::/7")));
    }

    #[tokio::test]
    async fn test_apply_default_both_public() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall_config(
            dir.path(),
            "config preferences preferences\n\toption remote_access 'default'\n",
        );

        let wan_ipv4: Ipv4Addr = "8.8.8.8".parse().unwrap();
        let wan_ipv6: Ipv6Addr = "2001:db8::1".parse().unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["startwrt", "firewall"]).await.unwrap();
        apply_remote_access_config(&mut cfgs, "default", Some(wan_ipv4), &[wan_ipv6]);
        dump_all(dir.path(), cfgs).await.unwrap();

        // Both public — no rules should be created
        assert_eq!(count_remote_rules(dir.path()).await, 0);
    }

    #[tokio::test]
    async fn test_apply_default_no_wan() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall_config(
            dir.path(),
            "config preferences preferences\n\toption remote_access 'default'\n",
        );

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["startwrt", "firewall"]).await.unwrap();
        apply_remote_access_config(&mut cfgs, "default", None, &[]);
        dump_all(dir.path(), cfgs).await.unwrap();

        // No WAN at all — no rules should be created
        assert_eq!(count_remote_rules(dir.path()).await, 0);
    }

    #[tokio::test]
    async fn test_apply_default_ipv4_only() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall_config(
            dir.path(),
            "config preferences preferences\n\toption remote_access 'default'\n",
        );

        let wan_ipv4: Ipv4Addr = "192.168.1.1".parse().unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["startwrt", "firewall"]).await.unwrap();
        apply_remote_access_config(&mut cfgs, "default", Some(wan_ipv4), &[]);
        dump_all(dir.path(), cfgs).await.unwrap();

        // Private IPv4, no IPv6 — one IPv4 rule per RFC1918 range × 3 ports.
        assert_eq!(count_remote_rules(dir.path()).await, 9);
        for family in get_remote_rule_families(dir.path()).await {
            assert_eq!(family, Some("ipv4".to_string()));
        }
        let src_ips = get_remote_rule_src_ips(dir.path()).await;
        assert!(src_ips.iter().any(|s| s.as_deref() == Some("10.0.0.0/8")));
        assert!(src_ips.iter().any(|s| s.as_deref() == Some("172.16.0.0/12")));
        assert!(src_ips.iter().any(|s| s.as_deref() == Some("192.168.0.0/16")));
    }

    #[tokio::test]
    async fn test_apply_default_ipv6_ula_only() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall_config(
            dir.path(),
            "config preferences preferences\n\toption remote_access 'default'\n",
        );

        let wan_ipv6: Ipv6Addr = "fd00::1".parse().unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["startwrt", "firewall"]).await.unwrap();
        apply_remote_access_config(&mut cfgs, "default", None, &[wan_ipv6]);
        dump_all(dir.path(), cfgs).await.unwrap();

        // No IPv4, ULA-only IPv6 — IPv6-only rules
        assert_eq!(count_remote_rules(dir.path()).await, 3);
        for family in get_remote_rule_families(dir.path()).await {
            assert_eq!(family, Some("ipv6".to_string()));
        }
        // Source-scoped to ULA so a globally-routed client can never match.
        for src_ip in get_remote_rule_src_ips(dir.path()).await {
            assert_eq!(src_ip.as_deref(), Some("fc00::/7"));
        }
    }

    #[tokio::test]
    async fn test_apply_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall_config(
            dir.path(),
            "config preferences preferences\n\toption remote_access 'always'\n",
        );

        // Apply twice
        for _ in 0..2 {
            let arena = Arena::new();
            let mut cfgs = parse_all(dir.path(), &arena, &["startwrt", "firewall"]).await.unwrap();
            apply_remote_access_config(&mut cfgs, "always", None, &[]);
            dump_all(dir.path(), cfgs).await.unwrap();
        }

        // Should still have exactly 4 rules, not 8
        assert_eq!(count_remote_rules(dir.path()).await, 3);
    }
}
