use crate::devices::{self, Device, DeviceStatus};
use crate::error::ErrorKind;
use crate::invoke::Invoke;
use crate::prelude::*;
use crate::profiles::Lookup;
use crate::utils::{DeserializeStdin, HandlerExtSerde};
use crate::{CliContext, CtrlContext, Error, ServerContext};
use imbl_value::Value;
use rpc_toolkit::{from_fn_async_local, HandlerExt as _, ParentHandler};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::net::{Ipv4Addr, Ipv6Addr};
use uciedit::openwrt::{
    DhcpHost, FirewallRedirect, FirewallRule, FirewallTarget, FirewallZone, InterfaceProto,
    NetworkInterface,
};
use uciedit::{dump_all, parse_all, Arena, Configs};

pub fn published_ports<C: CtrlContext>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async_local(list)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand("set", from_fn_async_local(set::<C>).no_display())
        .subcommand(
            "reconcile",
            from_fn_async_local(reconcile::<C>)
                .with_metadata("no_auth", Value::Bool(true))
                .no_display(),
        )
}

// ── Types ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Tcp,
    Udp,
    #[serde(rename = "tcp+udp")]
    TcpUdp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PublishedPortStatus {
    Active,
    Partial,
    Paused,
    Error,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedPort {
    pub id: String,
    pub enabled: bool,
    pub label: String,
    pub device_mac: String,
    pub ports: String,
    pub protocol: Protocol,
    pub ipv4: bool,
    pub ipv6: bool,
    pub ipv4_public_port: Option<String>,
    pub source: String,
    pub status: PublishedPortStatus,
    pub status_reason: Option<String>,
    pub device_name: Option<String>,
    pub device_ipv4: Option<String>,
    pub device_ipv6: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublishedPortInput {
    pub id: String,
    pub enabled: bool,
    pub label: String,
    pub device_mac: String,
    pub ports: String,
    pub protocol: Protocol,
    pub ipv4: bool,
    pub ipv6: bool,
    pub ipv4_public_port: Option<String>,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublishedPortsSetRequest {
    pub ports: Vec<PublishedPortInput>,
}

/// A published port that will be removed because the device it forwards to can
/// no longer be reached at its current address — its security profile is changing
/// (ethernet-port or WiFi-password reassignment) or its WiFi password is being
/// deleted (disconnecting it) — so its DNAT rule, bound to the device's old
/// subnet, would otherwise break. Surfaced to the UI to confirm the deletion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedPublishedPort {
    pub id: String,
    pub label: String,
    pub device_mac: String,
    pub device_name: Option<String>,
}

/// List the published ports whose target device MAC is in `macs` (compared
/// case-insensitively), enriched with the device name from `device_names`
/// (best-effort). Reads firewall config from `uci_root`.
pub async fn affected_ports_for_macs(
    uci_root: &std::path::Path,
    macs: &HashSet<String>,
    device_names: &HashMap<String, String>,
) -> Result<Vec<AffectedPublishedPort>, Error> {
    if macs.is_empty() {
        return Ok(Vec::new());
    }
    let macs: HashSet<String> = macs.iter().map(|m| m.to_uppercase()).collect();
    let arena = Arena::new();
    let raw = extract_ports(&arena, uci_root).await?;
    Ok(raw
        .into_iter()
        .filter(|p| macs.contains(&p.device_mac.to_uppercase()))
        .map(|p| {
            let device_name = device_names.get(&p.device_mac.to_uppercase()).cloned();
            AffectedPublishedPort {
                id: p.id,
                label: p.label,
                device_mac: p.device_mac,
                device_name,
            }
        })
        .collect())
}

/// Published ports that will break when a profile is "vacated" (loses its last
/// WiFi password): the target device is currently WiFi-connected (`wifi_macs`)
/// **and** its static DHCP reservation IP falls inside a vacated profile's
/// subnet.
///
/// Profile attribution for a port with an IPv4 reservation comes from the
/// reservation subnet (config-derived, so it doesn't depend on transient
/// neighbour state). An IPv6-only port has no IPv4 reservation to consult, so it
/// is attributed instead by the device's live security profile (`device_profiles`,
/// MAC → profile fullname, as resolved by `devices::list`) — the only signal
/// available for it. This depends on live neighbour state, but is scoped to the
/// IPv6-only case so it can't weaken the robust IPv4 path.
///
/// Returns the affected device MACs (uppercased, for `remove_ports_for_macs`)
/// and the `AffectedPublishedPort` list (for the confirmation dialog).
pub async fn affected_wifi_ports_for_vacated_profiles<C: CtrlContext>(
    ctx: &C,
    vacated_fullnames: &HashSet<String>,
    wifi_macs: &HashSet<String>,
    device_names: &HashMap<String, String>,
    device_profiles: &HashMap<String, String>,
) -> Result<(HashSet<String>, Vec<AffectedPublishedPort>), Error> {
    if vacated_fullnames.is_empty() || wifi_macs.is_empty() {
        return Ok((HashSet::new(), Vec::new()));
    }
    let wifi_macs: HashSet<String> = wifi_macs.iter().map(|m| m.to_uppercase()).collect();
    let uci_root = ctx.uci_root();

    let arena = Arena::new();
    let cfgs = parse_all(&uci_root, &arena, &["network", "startwrt", "dhcp"]).await?;
    let lookup = Lookup::parse(ctx.clone(), &cfgs)?;

    // Vacated profile subnets as (network, mask) u32 pairs, read from each
    // profile's STATIC bridge interface in the network config.
    let mut vacated_subnets: Vec<(u32, u32)> = Vec::new();
    for profile in lookup.list() {
        if !vacated_fullnames.contains(&profile.fullname) {
            continue;
        }
        for section in &cfgs["network"].sections {
            let Ok(iface) = section.get::<NetworkInterface>() else {
                continue;
            };
            let Some(name) = section.name() else { continue };
            if name != profile.interface || iface.proto != InterfaceProto::STATIC {
                continue;
            }
            if let (Some(ip), Some(mask)) = (iface.ipaddr, iface.netmask) {
                let mask = u32::from(mask);
                vacated_subnets.push((u32::from(ip) & mask, mask));
            }
            break;
        }
    }

    // MAC → reservation IP, for reservations that pin an IPv4 address.
    let mut reservation_ip: HashMap<String, Ipv4Addr> = HashMap::new();
    cfgs["dhcp"].each::<DhcpHost, Error>(|_, host| {
        if let Some(ip) = host.ip.as_ref().and_then(|s| s.parse::<Ipv4Addr>().ok()) {
            reservation_ip.insert(host.mac.to_uppercase(), ip);
        }
    })?;

    let raw = extract_ports(&arena, uci_root.as_path()).await?;
    let mut macs = HashSet::new();
    let mut ports = Vec::new();
    for p in raw {
        let mac_upper = p.device_mac.to_uppercase();
        if !wifi_macs.contains(&mac_upper) {
            continue;
        }
        // A port with an IPv4 reservation is attributed by its (config-derived,
        // robust) reservation subnet. A port without one — an IPv6-only forward —
        // is attributed by the device's live security profile, the only signal
        // available for it.
        let affected = if let Some(ip) = reservation_ip.get(&mac_upper) {
            let ip_u32 = u32::from(*ip);
            vacated_subnets
                .iter()
                .any(|(net, mask)| ip_u32 & mask == *net)
        } else {
            device_profiles
                .get(&mac_upper)
                .is_some_and(|prof| vacated_fullnames.contains(prof))
        };
        if affected {
            macs.insert(mac_upper.clone());
            ports.push(AffectedPublishedPort {
                id: p.id,
                label: p.label,
                device_name: device_names.get(&mac_upper).cloned(),
                device_mac: p.device_mac,
            });
        }
    }
    Ok((macs, ports))
}

/// Remove, in place, the published-port firewall sections (IPv4 DNAT redirects
/// and IPv6 ACCEPT rules) for the given MACs, plus each device's now-stale static
/// DHCP **IPv4** reservation. `cfgs` must already include the "firewall" and
/// "dhcp" configs. Returns the number of firewall sections removed. The caller
/// is responsible for `dump_all` and reloading services.
///
/// DHCP cleanup is surgical: a user-named host (created via `devices.update`) is
/// kept and only its stale `ip` is cleared — its `name` and any manual IPv6
/// `hostid` (a prefix-independent suffix that's still valid under the new
/// profile) survive. An *unnamed* host is a published-port auto-reservation with
/// no user data, so it's removed outright.
pub fn remove_ports_for_macs(cfgs: &mut Configs, macs: &HashSet<String>) -> usize {
    let macs: HashSet<String> = macs.iter().map(|m| m.to_uppercase()).collect();
    let mut removed = 0usize;
    cfgs["firewall"].sections.retain(|section| {
        let pp_mac = section
            .get::<FirewallRedirect>()
            .ok()
            .filter(|r| r._pp_id.is_some() && r.target == "DNAT")
            .and_then(|r| r._pp_mac.clone())
            .or_else(|| {
                section
                    .get::<FirewallRule>()
                    .ok()
                    .filter(is_pp_v6_rule)
                    .and_then(|r| r._pp_mac.clone())
            });
        if let Some(mac) = pp_mac {
            if !mac.is_empty() && macs.contains(&mac.to_uppercase()) {
                removed += 1;
                return false;
            }
        }
        true
    });
    // Clean up each device's now-stale static DHCP reservation. The pinned IPv4
    // is on the old subnet and unusable, so the device should fall back to a
    // fresh lease — but don't destroy user-owned data: keep a named host (just
    // clear its `ip`) and only remove anonymous published-port auto-reservations.
    cfgs["dhcp"].sections.retain_mut(|section| {
        let Ok(host) = section.get::<DhcpHost>() else {
            return true;
        };
        if !macs.contains(&host.mac.to_uppercase()) {
            return true;
        }
        let named = host.name.as_deref().is_some_and(|n| !n.is_empty());
        if named {
            // User-owned: keep the section, drop only the stale IPv4 pin.
            if host.ip.is_some() {
                let mut host = host;
                host.ip = None;
                let _ = section.set(&host);
            }
            true
        } else {
            // Anonymous pp auto-reservation: nothing worth keeping.
            false
        }
    });
    removed
}

// ── Helpers ──────────────────────────────────────────────

/// Convert our Protocol enum to UCI proto string
fn protocol_to_uci(proto: &Protocol) -> Vec<String> {
    match proto {
        Protocol::Tcp => vec!["tcp".into()],
        Protocol::Udp => vec!["udp".into()],
        Protocol::TcpUdp => vec!["tcp".into(), "udp".into()],
    }
}

/// Check whether an IPv6 address is a Global Unicast Address (2000::/3 —
/// routable from the internet). Delegates to `system::has_global_ipv6` so the
/// backend keeps a single, authoritative definition of "global" rather than a
/// separate prefix-string heuristic; a string that doesn't parse as an IPv6
/// address is treated as non-global.
fn is_gua(ip: &str) -> bool {
    ip.parse::<std::net::Ipv6Addr>()
        .map(|addr| crate::system::has_global_ipv6(std::slice::from_ref(&addr)))
        .unwrap_or(false)
}

/// Combine a delegated prefix with a stored hostid suffix into a full GUA. This
/// is how `reconcile` rebuilds a device's forward address after the ISP rotates
/// the delegated prefix: the host suffix (the interface identifier) is stable, so
/// `new_prefix ++ same_suffix` yields the device's new address without having to
/// reach the device. `suffix` is a hostid like "dead:beef:0:50" (the form
/// produced by `devices::extract_ipv6_hostid`); parsing it as `::<suffix>` zeroes
/// the high bits, so the prefix mask cleanly selects which half comes from where.
fn recombine_gua(prefix: Ipv6Addr, prefix_len: u8, suffix: &str) -> Option<Ipv6Addr> {
    if prefix_len > 128 {
        return None;
    }
    let suffix_bits = format!("::{suffix}").parse::<Ipv6Addr>().ok()?;
    let mask: u128 = if prefix_len == 0 {
        0
    } else {
        u128::MAX << (128 - prefix_len)
    };
    Some(Ipv6Addr::from(
        (u128::from(prefix) & mask) | (u128::from(suffix_bits) & !mask),
    ))
}

/// Whether a firewall rule is a published-port IPv6 ACCEPT rule (a `pp_*_v6`
/// forward), as written by `set`. Shared by `set`'s clear pass and `reconcile`.
fn is_pp_v6_rule(rule: &FirewallRule) -> bool {
    rule._pp_id.is_some()
        && rule.family.as_deref() == Some("ipv6")
        && matches!(rule.target, FirewallTarget::ACCEPT)
}

/// Convert UCI proto list to our Protocol enum
fn uci_to_protocol(proto: &[String]) -> Protocol {
    let has_tcp = proto.iter().any(|p| p == "tcp");
    let has_udp = proto.iter().any(|p| p == "udp");
    match (has_tcp, has_udp) {
        (true, true) => Protocol::TcpUdp,
        (false, true) => Protocol::Udp,
        _ => Protocol::Tcp,
    }
}

// ── Validation ──────────────────────────────────────────

fn validate_port_or_range(s: &str) -> bool {
    if let Some((a, b)) = s.split_once('-') {
        let (Ok(a), Ok(b)) = (a.parse::<u16>(), b.parse::<u16>()) else {
            return false;
        };
        a >= 1 && b >= 1 && a <= b
    } else {
        matches!(s.parse::<u16>(), Ok(p) if p >= 1)
    }
}

fn validate_mac(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()))
}

fn validate_source(s: &str) -> bool {
    if let Some((ip_str, prefix_str)) = s.split_once('/') {
        let Ok(prefix) = prefix_str.parse::<u8>() else {
            return false;
        };
        prefix <= 32 && ip_str.parse::<std::net::Ipv4Addr>().is_ok()
    } else {
        s.parse::<std::net::Ipv4Addr>().is_ok()
    }
}

fn validate_id(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn validate_inputs(ports: &[PublishedPortInput]) -> Result<(), Error> {
    for port in ports {
        if !validate_id(&port.id) {
            return Err(Error::new(eyre!("invalid id: {}", port.id), ErrorKind::InvalidValue));
        }
        if port.label.trim().is_empty() {
            return Err(Error::new(eyre!("invalid label: {}", port.label), ErrorKind::InvalidValue));
        }
        if !port.device_mac.is_empty() && !validate_mac(&port.device_mac) {
            return Err(Error::new(eyre!("invalid device_mac: {}", port.device_mac), ErrorKind::InvalidValue));
        }
        if !validate_port_or_range(&port.ports) {
            return Err(Error::new(eyre!("invalid ports: {}", port.ports), ErrorKind::InvalidValue));
        }
        if let Some(ref pub_port) = port.ipv4_public_port {
            if !validate_port_or_range(pub_port) {
                return Err(Error::new(eyre!("invalid ipv4_public_port: {pub_port}"), ErrorKind::InvalidValue));
            }
        }
        if port.source != "any" && !validate_source(&port.source) {
            return Err(Error::new(eyre!("invalid source: {}", port.source), ErrorKind::InvalidValue));
        }
    }
    Ok(())
}

struct RawPort {
    id: String,
    enabled: bool,
    label: String,
    device_mac: String,
    ports: String,
    protocol: Protocol,
    ipv4: bool,
    ipv6: bool,
    ipv4_public_port: Option<String>,
    source: String,
    /// The IPv6 rule's stored `dest_ip` (the full GUA it forwards to), if any.
    /// Used by `compute_status` to detect a rule stranded on an old prefix.
    dest_ipv6: Option<String>,
}

/// Extract raw published ports from firewall config sections.
async fn extract_ports(arena: &Arena, uci_root: &std::path::Path) -> Result<Vec<RawPort>, Error> {
    let cfgs = parse_all(uci_root, arena, &["firewall"]).await?;

    // Collect redirects and rules with _pp_id metadata
    let mut redirects_by_id: HashMap<String, FirewallRedirect> = HashMap::new();
    let mut rules_by_id: HashMap<String, FirewallRule> = HashMap::new();
    let mut unmatched_redirects: Vec<(Option<String>, FirewallRedirect)> = Vec::new();
    let mut unmatched_rules: Vec<(Option<String>, FirewallRule)> = Vec::new();

    for section in &cfgs["firewall"].sections {
        let sec_name = section.name().map(|s| s.to_string());

        // Try as redirect
        if let Ok(redirect) = section.get::<FirewallRedirect>() {
            if redirect.target == "DNAT" {
                if let Some(ref pp_id) = redirect._pp_id {
                    if redirect._pp_mac.as_ref().is_some_and(|m| !m.is_empty()) {
                        redirects_by_id.insert(pp_id.clone(), redirect);
                    }
                } else {
                    unmatched_redirects.push((sec_name, redirect));
                }
                continue;
            }
        }

        // Try as rule (IPv6 port forward)
        if let Ok(rule) = section.get::<FirewallRule>() {
            if rule.family.as_deref() == Some("ipv6")
                && matches!(rule.target, FirewallTarget::ACCEPT)
                && rule.src == "wan"
                && rule.dest_port.is_some()
            {
                if let Some(ref pp_id) = rule._pp_id {
                    if rule._pp_mac.as_ref().is_some_and(|m| !m.is_empty()) {
                        rules_by_id.insert(pp_id.clone(), rule);
                    }
                } else {
                    unmatched_rules.push((sec_name, rule));
                }
            }
        }
    }

    let mut ports = Vec::new();

    // Process matched pairs
    let mut processed = std::collections::HashSet::new();
    for (pp_id, redirect) in &redirects_by_id {
        let rule = rules_by_id.get(pp_id);
        ports.push(sections_to_raw_port(pp_id.clone(), Some(redirect), rule));
        processed.insert(pp_id.clone());
    }

    // IPv6-only rules with _pp_id but no matching redirect
    for (pp_id, rule) in &rules_by_id {
        if !processed.contains(pp_id) {
            ports.push(sections_to_raw_port(pp_id.clone(), None, Some(rule)));
        }
    }

    // Unmatched redirects/rules are only treated as legacy published ports
    // if their section name starts with "pp_". Otherwise they are standard
    // firewall entries (e.g. Allow_DHCPv6) that happen to match the shape.
    for (sec_name, redirect) in &unmatched_redirects {
        let name = sec_name.as_deref().unwrap_or("");
        if let Some(id) = name.strip_prefix("pp_") {
            ports.push(sections_to_raw_port(id.to_string(), Some(redirect), None));
        }
    }

    for (sec_name, rule) in &unmatched_rules {
        let name = sec_name.as_deref().unwrap_or("");
        if let Some(id) = name.strip_prefix("pp_").and_then(|s| s.strip_suffix("_v6")) {
            ports.push(sections_to_raw_port(id.to_string(), None, Some(rule)));
        }
    }

    Ok(ports)
}

fn sections_to_raw_port(
    id: String,
    redirect: Option<&FirewallRedirect>,
    rule: Option<&FirewallRule>,
) -> RawPort {
    // Determine enabled/protocol/label from whichever section exists
    let (enabled, label, protocol, ports, source, device_mac) = if let Some(r) = redirect {
        let enabled = r.enabled.as_deref() != Some("0");
        let protocol = uci_to_protocol(&r.proto);
        let ports = r.dest_port.clone().unwrap_or_default();
        let source = r.src_ip.clone().unwrap_or_else(|| "any".into());
        let mac = r._pp_mac.clone().unwrap_or_default();
        (enabled, r.name.clone(), protocol, ports, source, mac)
    } else if let Some(r) = rule {
        let enabled = r.enabled.as_deref() != Some("0");
        let protocol = uci_to_protocol(&r.proto);
        let ports = r.dest_port.clone().unwrap_or_default();
        let source = r.src_ip.clone().unwrap_or_else(|| "any".into());
        let mac = r._pp_mac.clone().unwrap_or_default();
        (enabled, r.name.clone(), protocol, ports, source, mac)
    } else {
        unreachable!("at least one section must exist")
    };

    let ipv4_public_port = redirect.and_then(|r| {
        let src_dport = r.src_dport.as_deref()?;
        let dest_port = r.dest_port.as_deref()?;
        if src_dport != dest_port {
            Some(src_dport.to_string())
        } else {
            None
        }
    });

    RawPort {
        id,
        enabled,
        label,
        device_mac,
        ports,
        protocol,
        ipv4: redirect.is_some(),
        ipv6: rule.is_some(),
        ipv4_public_port,
        source,
        dest_ipv6: rule.and_then(|r| r.dest_ip.clone()),
    }
}

fn compute_status(
    port: &RawPort,
    device: Option<&Device>,
) -> (PublishedPortStatus, Option<String>) {
    if !port.enabled {
        return (PublishedPortStatus::Disabled, None);
    }

    let device = match device {
        Some(d) => d,
        None => return (PublishedPortStatus::Paused, Some("Device not found".into())),
    };

    if matches!(device.status, DeviceStatus::Offline) {
        return (PublishedPortStatus::Paused, Some("Device offline".into()));
    }
    let has_ipv4 = device.ipv4.is_some();
    let has_ipv6 = device.ipv6.is_some();

    if port.ipv4 && !has_ipv4 {
        if port.ipv6 && has_ipv6 {
            return (
                PublishedPortStatus::Partial,
                Some("IPv4 unavailable".into()),
            );
        }
        return (
            PublishedPortStatus::Error,
            Some("No addresses available".into()),
        );
    }

    if port.ipv6 && !has_ipv6 {
        if port.ipv4 && has_ipv4 {
            return (
                PublishedPortStatus::Partial,
                Some("IPv6 unavailable".into()),
            );
        }
        return (
            PublishedPortStatus::Error,
            Some("No addresses available".into()),
        );
    }

    // Stale-prefix backstop. An IPv6 forward's stored dest_ip is a full GUA. If
    // the device now has a GUA in a *different* /64, the ISP-delegated prefix
    // rotated and this rule is stranded on the old one. `reconcile` normally
    // repairs this on the wan6 hotplug, but a device that was offline during the
    // rotation (or a privacy-address client whose suffix moved) can slip past it
    // — so surface it here instead of showing false-green. Only compares two
    // GUAs; a ULA-only device is handled by the availability checks above.
    if port.ipv6 {
        if let (Some(dest), Some(cur)) = (port.dest_ipv6.as_deref(), device.ipv6.as_deref()) {
            if is_gua(dest) && is_gua(cur) && same_v6_prefix64(dest, cur) == Some(false) {
                if port.ipv4 && has_ipv4 {
                    return (
                        PublishedPortStatus::Partial,
                        Some("IPv6 address out of date".into()),
                    );
                }
                return (
                    PublishedPortStatus::Error,
                    Some("IPv6 address out of date".into()),
                );
            }
        }
    }

    (PublishedPortStatus::Active, None)
}

/// Whether two IPv6 addresses share the same /64 prefix (high 64 bits). `None`
/// if either fails to parse. Used to detect a published-port rule left stranded
/// on an old delegated prefix after an ISP prefix rotation.
fn same_v6_prefix64(a: &str, b: &str) -> Option<bool> {
    let a = a.parse::<Ipv6Addr>().ok()?;
    let b = b.parse::<Ipv6Addr>().ok()?;
    Some((u128::from(a) >> 64) == (u128::from(b) >> 64))
}

/// Generate a v4-style UUID from random bytes
#[cfg(test)]
fn uuid_v4() -> String {
    use rand::Rng;
    let mut bytes = [0u8; 16];
    rand::rng().fill(&mut bytes);
    // Set version (4) and variant (RFC 4122)
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u16::from_be_bytes([bytes[4], bytes[5]]),
        u16::from_be_bytes([bytes[6], bytes[7]]),
        u16::from_be_bytes([bytes[8], bytes[9]]),
        u64::from_be_bytes([0, 0, bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]]),
    )
}

// ── Handlers ──────────────────────────────────────────────

#[instrument(skip_all)]
pub async fn list(_ctx: ServerContext) -> Result<Vec<PublishedPort>, Error> {
    // Read firewall config and device list in parallel
    let (ports_result, devices_result) = tokio::join!(
        async {
            let arena = Arena::new();
            extract_ports(&arena, std::path::Path::new("/etc/config")).await
        },
        devices::list(ServerContext::default()),
    );

    let raw_ports = ports_result?;
    let devices = devices_result?;

    // Index devices by MAC (skip VPN devices which have no MAC)
    let devices_by_mac: HashMap<String, &Device> = devices
        .iter()
        .filter_map(|d| Some((d.mac.as_ref()?.to_uppercase(), d)))
        .collect();

    let ports = raw_ports
        .iter()
        .map(|raw| {
            let device = devices_by_mac.get(&raw.device_mac.to_uppercase()).copied();
            let (status, status_reason) = compute_status(raw, device);

            PublishedPort {
                id: raw.id.clone(),
                enabled: raw.enabled,
                label: raw.label.clone(),
                device_mac: raw.device_mac.clone(),
                ports: raw.ports.clone(),
                protocol: raw.protocol.clone(),
                ipv4: raw.ipv4,
                ipv6: raw.ipv6,
                ipv4_public_port: raw.ipv4_public_port.clone(),
                source: raw.source.clone(),
                status,
                status_reason,
                device_name: device.map(|d| d.name.clone()),
                device_ipv4: device.and_then(|d| d.ipv4.clone()),
                device_ipv6: device.and_then(|d| d.ipv6.clone()),
            }
        })
        .collect();

    Ok(ports)
}

#[instrument(skip_all)]
pub async fn set<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<PublishedPortsSetRequest>,
) -> Result<(), Error> {
    validate_inputs(&req.ports)?;

    // A LAN device can only have a Global Unicast Address (GUA) if the router
    // itself has a globally-routable IPv6 prefix delegated. Resolve this once;
    // it gates IPv6 publishing in the validation loop below. In configs-only
    // mode we can't probe the WAN (and validation is skipped anyway), so this
    // stays false. Failing closed here is the safe direction for a hardening
    // guard: a transient WAN-probe miss yields a clear, retryable rejection
    // rather than silently saving a forward that can never work.
    let router_has_gua = ctx.effectful()
        && crate::system::has_global_ipv6(
            &crate::system::get_wan_ipv6s().await.unwrap_or_default(),
        );

    let mut retries = 4;
    loop {
        // Resolve device IPs and interfaces inside the retry loop so they stay fresh
        let device_info: HashMap<String, DeviceNetInfo> = if ctx.effectful() {
            resolve_device_info(&req.ports).await
        } else {
            HashMap::new()
        };

        // Validate that enabled ports have the required device addresses
        if ctx.effectful() {
            for port in &req.ports {
                if !port.enabled || port.device_mac.is_empty() {
                    continue;
                }
                let info = device_info.get(&port.device_mac.to_uppercase());
                let (ipv4_addr, ipv6_addr) = info
                    .map(|i| (i.ipv4.as_ref(), i.ipv6.as_ref()))
                    .unwrap_or((None, None));
                if port.ipv4 && ipv4_addr.is_none() {
                    return Err(Error::new(eyre!("missing IPv4 address for device {} (port forward '{}')", port.device_mac, port.label), ErrorKind::MissingDeviceAddress));
                }
                // IPv6 publishing requires a Global Unicast Address: ULA and
                // link-local are unreachable from the WAN, so a rule for them is
                // a silent no-op. Reject the whole port (matching the IPv4 check
                // above) rather than save a forward that can never work. Three
                // confident signals trigger rejection:
                //   * the router has no global IPv6 prefix at all, so no LAN
                //     device can possibly have a GUA;
                //   * the device resolved to a concrete non-GUA address (ULA); or
                //   * the device is online but has only a link-local address
                //     (excluded from resolution, surfaced via the flag below).
                // Only a *genuinely offline* device (no IPv6 seen at all) on a
                // GUA-capable router is left to the rule-creation GUA guard, so a
                // briefly-offline device doesn't fail an otherwise-valid save.
                if port.ipv6 {
                    if !router_has_gua {
                        return Err(Error::new(eyre!("IPv6 port forward '{}' requires a global IPv6 address, but this router has no global IPv6 prefix (only ULA/link-local, which are not reachable from the internet)", port.label), ErrorKind::MissingDeviceAddress));
                    }
                    if let Some(addr) = ipv6_addr {
                        if !is_gua(addr) {
                            return Err(Error::new(eyre!("IPv6 port forward '{}' requires a global IPv6 address for device {}, but it only has a local address ({}); ULA/link-local are not reachable from the internet", port.label, port.device_mac, addr), ErrorKind::MissingDeviceAddress));
                        }
                    } else if info.is_some_and(|i| i.ipv6_link_local_only) {
                        return Err(Error::new(eyre!("IPv6 port forward '{}' requires a global IPv6 address for device {}, but it only has a link-local address; ULA/link-local are not reachable from the internet", port.label, port.device_mac), ErrorKind::MissingDeviceAddress));
                    }
                }
            }
        }

        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["firewall", "dhcp"]).await?;

        // Auto-reserve stable addressing for enabled ports that lack it:
        //   * IPv4 — a static DHCP lease (dest_ip of the DNAT redirect), and
        //   * IPv6 — a hostid (the interface suffix), so the device's GUA is
        //     `delegated_prefix ++ hostid`. Pinning the suffix lets `reconcile`
        //     recompute the forward address after the ISP rotates the prefix,
        //     and makes the address deterministic for DHCPv6 clients. odhcpd
        //     consumes the hostid; we never clobber a hostid the user already
        //     set manually (which may intentionally differ from a transient
        //     privacy address).
        let mut dhcp_modified = false;
        if ctx.effectful() {
            let mut host_indices: HashMap<String, usize> = HashMap::new();
            for (i, section) in cfgs["dhcp"].sections.iter().enumerate() {
                if let Ok(host) = section.get::<DhcpHost>() {
                    host_indices.insert(host.mac.to_uppercase(), i);
                }
            }

            for port in &req.ports {
                if !port.enabled {
                    continue;
                }
                let mac = port.device_mac.to_uppercase();
                let Some(info) = device_info.get(&mac) else {
                    continue;
                };

                // Desired IPv4 reservation (only when the device has a dynamic
                // address and no existing static reservation).
                let want_ip = if port.ipv4 && !info.has_static_ipv4 {
                    info.ipv4.clone()
                } else {
                    None
                };
                // Desired IPv6 hostid, derived from the device's current GUA.
                let want_hostid = if port.ipv6 {
                    info.ipv6
                        .as_deref()
                        .filter(|a| is_gua(a))
                        .map(crate::devices::extract_ipv6_hostid)
                } else {
                    None
                };

                if want_ip.is_none() && want_hostid.is_none() {
                    continue;
                }

                if let Some(&idx) = host_indices.get(&mac) {
                    // Existing host entry — fill in the missing fields only.
                    if let Ok(mut host) = cfgs["dhcp"].sections[idx].get::<DhcpHost>() {
                        let mut changed = false;
                        if let Some(ip) = want_ip {
                            host.ip = Some(ip);
                            changed = true;
                        }
                        if let (Some(hostid), true) = (want_hostid, host.hostid.is_none()) {
                            host.hostid = Some(hostid);
                            changed = true;
                        }
                        if changed {
                            cfgs["dhcp"].sections[idx].set(&host)?;
                            dhcp_modified = true;
                        }
                    }
                } else {
                    // No host entry at all — create one.
                    let host = DhcpHost {
                        mac: mac.clone(),
                        ip: want_ip,
                        hostid: want_hostid,
                        dns: Some("1".to_string()),
                        ..Default::default()
                    };
                    let section_name =
                        format!("host_{}", mac.replace(':', "").to_lowercase());
                    cfgs["dhcp"].append(&host, Some(&section_name))?;
                    dhcp_modified = true;
                }
            }
        }

        // Build MAC → zone lookup from profiles and firewall zones
        let mac_zones = if ctx.effectful() {
            resolve_device_zones(ctx.uci_root().as_path(), &device_info).await
        } else {
            HashMap::new()
        };

        // Remove all existing published-port sections (name starts with "pp_")
        cfgs["firewall"].sections.retain(|section| {
            if let Some(name) = section.name() {
                if name.starts_with("pp_") {
                    return false;
                }
            }
            // Also remove sections with _pp_id metadata (in case name doesn't match)
            if let Ok(redirect) = section.get::<FirewallRedirect>() {
                if redirect._pp_id.is_some() && redirect.target == "DNAT" {
                    return false;
                }
            }
            if let Ok(rule) = section.get::<FirewallRule>() {
                if is_pp_v6_rule(&rule) {
                    return false;
                }
            }
            true
        });

        // Add new sections for each published port
        for port in &req.ports {
            let proto = protocol_to_uci(&port.protocol);
            let safe_id: String = port
                .id
                .chars()
                .map(|c| if c == '-' { '_' } else { c })
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            let mac_upper = port.device_mac.to_uppercase();
            let info = device_info.get(&mac_upper);
            let ipv4_addr = info.and_then(|i| i.ipv4.clone());
            let ipv6_addr = info.and_then(|i| i.ipv6.clone());
            let dest_zone = mac_zones
                .get(&mac_upper)
                .cloned()
                .unwrap_or_else(|| "lan".into());

            // IPv4 redirect (DNAT)
            if port.ipv4 {
                let redirect = FirewallRedirect {
                    name: port.label.clone(),
                    src: "wan".into(),
                    dest: Some(dest_zone.clone()),
                    target: "DNAT".into(),
                    proto: proto.clone(),
                    src_dport: Some(
                        port.ipv4_public_port
                            .clone()
                            .unwrap_or_else(|| port.ports.clone()),
                    ),
                    dest_ip: ipv4_addr,
                    dest_port: Some(port.ports.clone()),
                    src_ip: if port.source != "any" {
                        Some(port.source.clone())
                    } else {
                        None
                    },
                    enabled: Some(if port.enabled { "1" } else { "0" }.into()),
                    _pp_id: Some(port.id.clone()),
                    _pp_mac: Some(port.device_mac.clone()),
                };
                let section_name = format!("pp_{}", safe_id);
                cfgs["firewall"].append(&redirect, Some(&section_name))?;
            }

            // IPv6 rule (ACCEPT) — only for globally routable addresses
            if port.ipv6 && ipv6_addr.as_ref().map_or(false, |a| is_gua(a)) {
                let rule = FirewallRule {
                    name: port.label.clone(),
                    src: "wan".into(),
                    dest: Some(dest_zone.clone()),
                    target: FirewallTarget::ACCEPT,
                    proto: proto.clone(),
                    dest_ip: ipv6_addr,
                    dest_port: Some(port.ports.clone()),
                    family: Some("ipv6".into()),
                    src_ip: if port.source != "any" {
                        Some(port.source.clone())
                    } else {
                        None
                    },
                    enabled: Some(if port.enabled { "1" } else { "0" }.into()),
                    _pp_id: Some(port.id.clone()),
                    _pp_mac: Some(port.device_mac.clone()),
                    ..Default::default()
                };
                let section_name = format!("pp_{}_v6", safe_id);
                cfgs["firewall"].append(&rule, Some(&section_name))?;
            }
        }

        match dump_all(ctx.uci_root(), cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log("published-ports", "updated", false, "Failed to update published ports", Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                if ctx.effectful() {
                    restart_firewall();
                    if dhcp_modified {
                        reload_dnsmasq();
                        // odhcpd owns DHCPv6/RA and the hostid → suffix mapping;
                        // reload it so a newly-pinned hostid takes runtime effect.
                        reload_odhcpd();
                    }
                }
                crate::activity::log("published-ports", "updated", true, &format!("Updated published ports ({} rules)", req.ports.len()), None);
                return Ok(());
            }
        }
    }
}

/// Recompute the `dest_ip` of every IPv6 published-port forward against the
/// router's *current* delegated prefix, then reload the firewall if anything
/// changed.
///
/// An IPv6 forward's `dest_ip` is a full GUA — `delegated_prefix ++ host_suffix`.
/// The suffix is stable (it's the device's interface identifier, pinned via the
/// DHCP `hostid`), but the prefix is the ISP's and can rotate. When it does,
/// every stored `dest_ip` points into a prefix the router no longer owns and the
/// forward silently breaks. This routine fixes that and is fired by the
/// `wan6` hotplug hook on `ifup`/`ifupdate`.
///
/// For each forward the new address is, in order of preference:
///   1. the device's freshly-observed GUA from the neighbor table — authoritative
///      for *any* addressing mode (DHCPv6, SLAAC EUI-64, privacy), used whenever
///      the device is currently reachable; else
///   2. `current_prefix ++ stored_hostid` — the offline fallback, exact for a
///      stable suffix and best-effort otherwise.
/// A forward whose device is offline and has no stored hostid is left untouched.
///
/// Fail-safe: if the router currently has no global prefix at all (a flap to
/// "none"), nothing is rewritten — the next `ifup` with a real prefix re-runs us.
#[instrument(skip_all)]
pub async fn reconcile<C: CtrlContext>(ctx: C) -> Result<Value, Error> {
    // Configs-only/CLI mode can't probe the WAN; there's nothing to reconcile.
    if !ctx.effectful() {
        return Ok(Value::Null);
    }

    let Some((prefix, prefix_len)) = crate::ssl::read_lan_gua_prefix().await else {
        tracing::debug!("published-ports reconcile: no global IPv6 prefix; skipping");
        return Ok(Value::Null);
    };

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["firewall", "dhcp"]).await?;

        // MAC → pinned hostid suffix (offline fallback source).
        let mut hostids: HashMap<String, String> = HashMap::new();
        cfgs["dhcp"].each::<DhcpHost, Error>(|_, host| {
            if let Some(hostid) = host.hostid.clone() {
                hostids.insert(host.mac.to_uppercase(), hostid);
            }
        })?;

        // MACs targeted by IPv6 forwards, for live address resolution.
        let mut v6_macs: HashSet<String> = HashSet::new();
        for section in &cfgs["firewall"].sections {
            if let Ok(rule) = section.get::<FirewallRule>() {
                if is_pp_v6_rule(&rule) {
                    if let Some(mac) = rule._pp_mac.filter(|m| !m.is_empty()) {
                        v6_macs.insert(mac.to_uppercase());
                    }
                }
            }
        }
        if v6_macs.is_empty() {
            return Ok(Value::Null);
        }

        // mac → live GUA from the neighbor table (authoritative when present).
        let device_ipv6: HashMap<String, String> = resolve_device_info_for_macs(v6_macs)
            .await
            .into_iter()
            .filter_map(|(mac, info)| {
                info.ipv6.filter(|a| is_gua(a)).map(|gua| (mac, gua))
            })
            .collect();

        let changed = rewrite_v6_dest_ips(&mut cfgs, prefix, prefix_len, &device_ipv6, &hostids)?;
        if changed == 0 {
            return Ok(Value::Null);
        }

        match dump_all(ctx.uci_root(), cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log("published-ports", "reconciled", false, "Failed to reconcile IPv6 published ports", Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                restart_firewall();
                crate::activity::log("published-ports", "reconciled", true, &format!("Reconciled {changed} IPv6 published port(s) to new prefix"), None);
                return Ok(Value::Null);
            }
        }
    }
}

/// In-memory core of [`reconcile`]: rewrite each `pp_*_v6` rule's `dest_ip` to the
/// device's current GUA, returning how many rules changed. Pure (no I/O) so it's
/// unit-testable. For each rule the new address is the device's live GUA
/// (`device_ipv6`) when known, else `prefix ++ stored_hostid` (`hostids`); a rule
/// with neither is left untouched.
fn rewrite_v6_dest_ips(
    cfgs: &mut Configs,
    prefix: Ipv6Addr,
    prefix_len: u8,
    device_ipv6: &HashMap<String, String>,
    hostids: &HashMap<String, String>,
) -> Result<usize, Error> {
    let mut changed = 0usize;
    for section in &mut cfgs["firewall"].sections {
        let Ok(mut rule) = section.get::<FirewallRule>() else {
            continue;
        };
        if !is_pp_v6_rule(&rule) {
            continue;
        }
        let Some(mac) = rule._pp_mac.as_ref().filter(|m| !m.is_empty()) else {
            continue;
        };
        let mac = mac.to_uppercase();

        // Live address wins; otherwise reconstruct from the pinned suffix.
        let new_ip = device_ipv6.get(&mac).cloned().or_else(|| {
            hostids
                .get(&mac)
                .and_then(|suffix| recombine_gua(prefix, prefix_len, suffix))
                .map(|addr| addr.to_string())
        });

        let Some(new_ip) = new_ip else {
            tracing::warn!(
                "published-ports reconcile: cannot determine current IPv6 for {mac} (offline, no hostid); leaving {:?}",
                rule.dest_ip
            );
            continue;
        };

        // Compare parsed addresses so formatting differences don't churn.
        let unchanged = rule
            .dest_ip
            .as_deref()
            .and_then(|s| s.parse::<Ipv6Addr>().ok())
            .zip(new_ip.parse::<Ipv6Addr>().ok())
            .is_some_and(|(a, b)| a == b);
        if unchanged {
            continue;
        }

        rule.dest_ip = Some(new_ip);
        section.set(&rule)?;
        changed += 1;
    }
    Ok(changed)
}

struct DeviceNetInfo {
    ipv4: Option<String>,
    /// Best usable IPv6 (GUA preferred, ULA fallback). `None` when the device
    /// has no GUA/ULA — either it's genuinely offline, or it's online but only
    /// link-local; `ipv6_link_local_only` disambiguates the two.
    ipv6: Option<String>,
    /// The device is present on the network but its only IPv6 address(es) are
    /// link-local (fe80::/10), which `pick_ipv6` excludes. Lets the caller
    /// reject an IPv6 forward to a known-unreachable device instead of
    /// deferring it like a briefly-offline one.
    ipv6_link_local_only: bool,
    /// ARP interface name (e.g. "br-lan.101") for zone resolution
    arp_interface: Option<String>,
    /// Whether the device already has a static DHCP IPv4 reservation
    has_static_ipv4: bool,
}

/// Resolve IPv4/IPv6 addresses and ARP interface for devices referenced by published ports.
async fn resolve_device_info(
    ports: &[PublishedPortInput],
) -> HashMap<String, DeviceNetInfo> {
    let macs: HashSet<String> = ports
        .iter()
        .map(|p| p.device_mac.to_uppercase())
        .collect();
    resolve_device_info_for_macs(macs).await
}

/// Core of [`resolve_device_info`], keyed directly on uppercased MACs so callers
/// that work from firewall sections (e.g. `reconcile`) can drive it without
/// synthesising `PublishedPortInput`s.
async fn resolve_device_info_for_macs(
    macs: HashSet<String>,
) -> HashMap<String, DeviceNetInfo> {
    let mut result: HashMap<String, DeviceNetInfo> = HashMap::new();

    if macs.is_empty() {
        return result;
    }

    // Read DHCP hosts for static IPs
    let arena = Arena::new();
    let mut static_ips: HashMap<String, Option<String>> = HashMap::new();
    if let Ok(cfgs) = parse_all(std::path::Path::new("/etc/config"), &arena, &["dhcp"]).await {
        cfgs["dhcp"]
            .each::<DhcpHost, Error>(|_, host| {
                let mac = host.mac.to_uppercase();
                if macs.contains(&mac) {
                    static_ips.insert(mac, host.ip.clone());
                }
            })
            .ok();
    }

    // Read ARP/neighbor table for dynamic IPs and interface names
    let neigh_output = tokio::process::Command::new("ip")
        .args(["neigh", "show"])
        .invoke(ErrorKind::Network.into())
        .await
        .ok()
        .and_then(|out| String::from_utf8(out).ok())
        .unwrap_or_default();

    let mut arp_ipv4: HashMap<String, String> = HashMap::new();
    let mut arp_ipv6_candidates: HashMap<String, Vec<String>> = HashMap::new();
    let mut arp_iface: HashMap<String, String> = HashMap::new();

    for line in neigh_output.lines() {
        if let Some((ip, rest)) = line.split_once(" dev ") {
            if let Some((iface, rest)) = rest.split_once(" lladdr ") {
                if let Some(mac) = rest.split_whitespace().next() {
                    let mac_upper = mac.to_uppercase();
                    if macs.contains(&mac_upper) {
                        let ip = ip.trim();
                        // Capture interface (prefer br-lan* interfaces)
                        if iface.starts_with("br-lan") {
                            arp_iface
                                .entry(mac_upper.clone())
                                .or_insert_with(|| iface.to_string());
                        }
                        if ip.contains(':') {
                            arp_ipv6_candidates
                                .entry(mac_upper)
                                .or_default()
                                .push(ip.to_string());
                        } else {
                            arp_ipv4
                                .entry(mac_upper)
                                .or_insert_with(|| ip.to_string());
                        }
                    }
                }
            }
        }
    }

    // Pick best IPv6 per MAC: GUA preferred over ULA, link-local excluded
    let arp_ipv6: HashMap<String, String> = arp_ipv6_candidates
        .iter()
        .filter_map(|(mac, candidates)| {
            devices::pick_ipv6(candidates.iter().map(|s| s.as_str()))
                .map(|ip| (mac.clone(), ip))
        })
        .collect();

    // Read DHCP leases as fallback for IPv4 — merge the base file and every
    // per-profile `/tmp/dhcp.leases.dns_*` instance.
    let leases = devices::read_all_dhcp_leases().await;
    let mut lease_ipv4: HashMap<String, String> = HashMap::new();
    for line in leases.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let mac = parts[1].to_uppercase();
            if macs.contains(&mac) {
                lease_ipv4.entry(mac).or_insert_with(|| parts[2].to_string());
            }
        }
    }

    // Merge: prefer static > ARP > lease
    for mac in &macs {
        let has_static_ipv4 = static_ips.get(mac).and_then(|ip| ip.as_ref()).is_some();
        let ipv4 = static_ips
            .get(mac)
            .and_then(|ip| ip.clone())
            .or_else(|| arp_ipv4.get(mac).cloned())
            .or_else(|| lease_ipv4.get(mac).cloned());
        let ipv6 = arp_ipv6.get(mac).cloned();
        // No usable (GUA/ULA) address picked, but the neighbor table did have
        // IPv6 candidates for this MAC → they were all link-local. The device
        // is online with only an unreachable address, not merely offline.
        let ipv6_link_local_only = ipv6.is_none()
            && arp_ipv6_candidates
                .get(mac)
                .is_some_and(|c| !c.is_empty());
        let arp_interface = arp_iface.get(mac).cloned();
        result.insert(
            mac.clone(),
            DeviceNetInfo { ipv4, ipv6, ipv6_link_local_only, arp_interface, has_static_ipv4 },
        );
    }

    result
}

/// Resolve MAC addresses to firewall zone names via ARP interface → VLAN tag → profile → zone.
async fn resolve_device_zones(
    uci_root: &std::path::Path,
    device_info: &HashMap<String, DeviceNetInfo>,
) -> HashMap<String, String> {
    let mut mac_zones: HashMap<String, String> = HashMap::new();

    let arena = Arena::new();
    let Ok(cfgs) = parse_all(uci_root, &arena, &["startwrt", "firewall"]).await else {
        return mac_zones;
    };

    // Build VLAN tag → profile interface name
    let Ok(lookup) = Lookup::parse(ServerContext::default(), &cfgs) else {
        return mac_zones;
    };

    // Build interface name → zone name from firewall config
    let mut iface_to_zone: HashMap<String, String> = HashMap::new();
    cfgs["firewall"]
        .each::<FirewallZone, Error>(|_, zone| {
            for iface in &zone.network {
                iface_to_zone.insert(iface.clone(), zone.name.clone());
            }
        })
        .ok();

    // For each device, resolve: ARP interface → VLAN tag → profile → zone
    for (mac, info) in device_info {
        let Some(ref arp_iface) = info.arp_interface else {
            continue;
        };
        // Extract VLAN tag from interface name (e.g. "br-lan.101" → 101, "br-lan" → 1)
        let vlan_tag = arp_iface
            .split('.')
            .nth(1)
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(1);

        if let Some(profile) = lookup.from_vlan(vlan_tag) {
            if let Some(zone) = iface_to_zone.get(&profile.interface) {
                mac_zones.insert(mac.clone(), zone.clone());
            }
        }
    }

    mac_zones
}

fn restart_firewall() {
    tokio::spawn(async {
        if let Err(e) = crate::run_quiet_async(
            tokio::process::Command::new("/etc/init.d/firewall").arg("restart"),
        )
        .await
        {
            tracing::error!("failed to restart firewall: {e}");
        }
    });
}

pub(crate) fn reload_dnsmasq() {
    tokio::spawn(async {
        if let Err(e) = crate::run_quiet_async(
            tokio::process::Command::new("/etc/init.d/dnsmasq").arg("reload"),
        )
        .await
        {
            tracing::error!("failed to reload dnsmasq: {e}");
        }
    });
}

pub(crate) fn reload_odhcpd() {
    tokio::spawn(async {
        if let Err(e) = crate::run_quiet_async(
            tokio::process::Command::new("/etc/init.d/odhcpd").arg("reload"),
        )
        .await
        {
            tracing::error!("failed to reload odhcpd: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::DeserializeStdin;
    use crate::CtrlContext;
    use rpc_toolkit::Context;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    #[derive(Clone)]
    struct TestContext(PathBuf);

    impl Context for TestContext {
        fn runtime(&self) -> Option<Arc<Runtime>> {
            None
        }
    }

    impl CtrlContext for TestContext {
        fn uci_root(&self) -> PathBuf {
            self.0.clone()
        }
        fn effectful(&self) -> bool {
            false
        }
    }

    fn setup_firewall(dir: &std::path::Path, content: &str) {
        std::fs::write(dir.join("firewall"), content).unwrap();
    }

    // ── is_gua tests ──

    #[test]
    fn test_is_gua() {
        // Global Unicast (2000::/3) — the only WAN-reachable scope.
        assert!(is_gua("2001:db8::1"));
        assert!(is_gua("2606:4700::1111"));
        assert!(is_gua("3fff::1")); // upper edge of 2000::/3
        assert!(is_gua("2001:DB8::1")); // case-insensitive (parsed, not string-matched)

        // Not global.
        assert!(!is_gua("fd00::1")); // ULA
        assert!(!is_gua("fc00::1")); // ULA
        assert!(!is_gua("fe80::1")); // link-local
        assert!(!is_gua("::1")); // loopback
        assert!(!is_gua("::")); // unspecified
        assert!(!is_gua("1fff::1")); // just below 2000::/3
        assert!(!is_gua("4000::1")); // just above 2000::/3
        // Regression: the old prefix-string heuristic wrongly classified
        // deprecated site-local (fec0::/10) as global; the parsed range check
        // correctly rejects it.
        assert!(!is_gua("fec0::1"));

        // Garbage never parses → not global.
        assert!(!is_gua("not-an-ip"));
        assert!(!is_gua(""));
    }

    // ── recombine_gua tests ──

    #[test]
    fn test_recombine_gua() {
        let p: Ipv6Addr = "2001:db8:abcd::".parse().unwrap();

        // /64: high 64 from prefix, low 64 from the hostid suffix.
        assert_eq!(
            recombine_gua(p, 64, "dead:beef:0:50").unwrap(),
            "2001:db8:abcd:0:dead:beef:0:50".parse::<Ipv6Addr>().unwrap()
        );
        // Short suffix (just the last hextet).
        assert_eq!(
            recombine_gua(p, 64, "50").unwrap(),
            "2001:db8:abcd::50".parse::<Ipv6Addr>().unwrap()
        );

        // A rotated prefix keeps the same suffix → new address.
        let p2: Ipv6Addr = "2001:db8:9999::".parse().unwrap();
        assert_eq!(
            recombine_gua(p2, 64, "50").unwrap(),
            "2001:db8:9999::50".parse::<Ipv6Addr>().unwrap()
        );

        // Non-/64 boundary (/48): the prefix supplies the top 3 hextets and the
        // hostid (lower 64 bits) supplies the rest — proving the mask width is
        // honored rather than hard-coded to /64.
        assert_eq!(
            recombine_gua("2001:db8:abcd::".parse().unwrap(), 48, "dead:beef:0:50")
                .unwrap(),
            "2001:db8:abcd:0:dead:beef:0:50".parse::<Ipv6Addr>().unwrap()
        );

        // Garbage suffix → None.
        assert!(recombine_gua(p, 64, "not-a-suffix").is_none());
        assert!(recombine_gua(p, 200, "50").is_none());
    }

    // ── rewrite_v6_dest_ips / reconcile tests ──

    const V6_FW: &str = "\
config redirect 'pp_a'
\toption name 'NAS v4'
\toption src 'wan'
\toption dest 'lan'
\toption target 'DNAT'
\tlist proto 'tcp'
\toption src_dport '443'
\toption dest_port '443'
\toption dest_ip '192.168.1.50'
\toption enabled '1'
\toption _pp_id 'a'
\toption _pp_mac 'AA:AA:AA:AA:AA:AA'

config rule 'pp_a_v6'
\toption name 'NAS v6'
\toption src 'wan'
\toption dest 'lan'
\toption target 'ACCEPT'
\tlist proto 'tcp'
\toption dest_port '443'
\toption dest_ip '2001:db8:abcd:0:dead:beef:0:50'
\toption family 'ipv6'
\toption enabled '1'
\toption _pp_id 'a'
\toption _pp_mac 'AA:AA:AA:AA:AA:AA'
";

    async fn rewrite_fixture(
        fw: &str,
        prefix: &str,
        device_ipv6: &[(&str, &str)],
        hostids: &[(&str, &str)],
    ) -> (usize, String) {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(dir.path(), fw);
        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["firewall", "dhcp"])
            .await
            .unwrap();
        let dev: HashMap<String, String> = device_ipv6
            .iter()
            .map(|(m, a)| (m.to_string(), a.to_string()))
            .collect();
        let hid: HashMap<String, String> = hostids
            .iter()
            .map(|(m, s)| (m.to_string(), s.to_string()))
            .collect();
        let changed =
            rewrite_v6_dest_ips(&mut cfgs, prefix.parse().unwrap(), 64, &dev, &hid).unwrap();
        dump_all(dir.path(), cfgs).await.unwrap();
        let content = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        (changed, content)
    }

    #[tokio::test]
    async fn reconcile_reconstructs_from_hostid_when_offline() {
        // Prefix rotated to 2001:db8:9999::/64; device offline (no live addr) but
        // has a pinned hostid → dest_ip rebuilt from prefix + suffix.
        let (changed, content) = rewrite_fixture(
            V6_FW,
            "2001:db8:9999::",
            &[],
            &[("AA:AA:AA:AA:AA:AA", "dead:beef:0:50")],
        )
        .await;
        assert_eq!(changed, 1);
        assert!(
            content.contains("option dest_ip '2001:db8:9999:0:dead:beef:0:50'"),
            "v6 rule should be rebuilt to the new prefix, got:\n{content}"
        );
        // IPv4 redirect must be untouched.
        assert!(content.contains("option dest_ip '192.168.1.50'"));
    }

    #[tokio::test]
    async fn reconcile_prefers_live_address() {
        // Live neighbor address wins over hostid reconstruction.
        let (changed, content) = rewrite_fixture(
            V6_FW,
            "2001:db8:9999::",
            &[("AA:AA:AA:AA:AA:AA", "2001:db8:9999::abcd")],
            &[("AA:AA:AA:AA:AA:AA", "dead:beef:0:50")],
        )
        .await;
        assert_eq!(changed, 1);
        assert!(content.contains("option dest_ip '2001:db8:9999::abcd'"), "got:\n{content}");
    }

    #[tokio::test]
    async fn reconcile_noop_when_already_current() {
        // Prefix unchanged → nothing rewritten, no churn.
        let (changed, _content) = rewrite_fixture(
            V6_FW,
            "2001:db8:abcd::",
            &[],
            &[("AA:AA:AA:AA:AA:AA", "dead:beef:0:50")],
        )
        .await;
        assert_eq!(changed, 0);
    }

    #[tokio::test]
    async fn reconcile_leaves_rule_when_no_source() {
        // Device offline and no hostid → rule left as-is rather than wiped.
        let (changed, content) = rewrite_fixture(V6_FW, "2001:db8:9999::", &[], &[]).await;
        assert_eq!(changed, 0);
        assert!(content.contains("option dest_ip '2001:db8:abcd:0:dead:beef:0:50'"));
    }

    // ── compute_status stale-prefix tests ──

    fn raw_v6_port(ipv4: bool, dest_ipv6: Option<&str>) -> RawPort {
        RawPort {
            id: "x".into(),
            enabled: true,
            label: "X".into(),
            device_mac: "AA:AA:AA:AA:AA:AA".into(),
            ports: "443".into(),
            protocol: Protocol::Tcp,
            ipv4,
            ipv6: true,
            ipv4_public_port: None,
            source: "any".into(),
            dest_ipv6: dest_ipv6.map(str::to_string),
        }
    }

    fn online_device(ipv4: Option<&str>, ipv6: Option<&str>) -> Device {
        Device {
            mac: Some("AA:AA:AA:AA:AA:AA".into()),
            name: "Dev".into(),
            hostname: None,
            status: DeviceStatus::Online,
            connection: None,
            ipv4: ipv4.map(str::to_string),
            ipv6: ipv6.map(str::to_string),
            ipv4_static: false,
            ipv6_static: false,
            security_profile: None,
            speed: None,
            data_usage: None,
        }
    }

    #[test]
    fn status_active_when_v6_prefix_matches() {
        // Same /64, different suffix → still in the live prefix, not stale.
        let port = raw_v6_port(false, Some("2001:db8:abcd::50"));
        let dev = online_device(None, Some("2001:db8:abcd::99"));
        let (status, _) = compute_status(&port, Some(&dev));
        assert!(matches!(status, PublishedPortStatus::Active));
    }

    #[test]
    fn status_error_when_v6_only_prefix_rotated() {
        // Rule on the old /64, device on a new /64 → stranded, v6-only → Error.
        let port = raw_v6_port(false, Some("2001:db8:abcd::50"));
        let dev = online_device(None, Some("2001:db8:9999::50"));
        let (status, reason) = compute_status(&port, Some(&dev));
        assert!(matches!(status, PublishedPortStatus::Error));
        assert_eq!(reason.as_deref(), Some("IPv6 address out of date"));
    }

    #[test]
    fn status_partial_when_dualstack_prefix_rotated() {
        // Dual-stack: v4 still works, only v6 stranded → Partial.
        let port = raw_v6_port(true, Some("2001:db8:abcd::50"));
        let dev = online_device(Some("192.168.1.50"), Some("2001:db8:9999::50"));
        let (status, reason) = compute_status(&port, Some(&dev));
        assert!(matches!(status, PublishedPortStatus::Partial));
        assert_eq!(reason.as_deref(), Some("IPv6 address out of date"));
    }

    #[test]
    fn status_not_stale_against_ula() {
        // Device only has a ULA (no GUA): the stale check must not fire (it's a
        // ULA-availability situation, not a rotated prefix).
        let port = raw_v6_port(false, Some("2001:db8:abcd::50"));
        let dev = online_device(None, Some("fd00:abcd::50"));
        let (status, _) = compute_status(&port, Some(&dev));
        // has_ipv6 is true (ULA present), GUA stale check skipped → Active.
        assert!(matches!(status, PublishedPortStatus::Active));
    }

    // ── extract_ports tests ──

    #[tokio::test]
    async fn extract_empty_firewall() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(
            dir.path(),
            "\
config defaults
\toption input 'REJECT'
",
        );
        let arena = Arena::new();
        let ports = extract_ports(&arena, dir.path()).await.unwrap();
        assert!(ports.is_empty());
    }

    #[tokio::test]
    async fn extract_ipv4_only() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(
            dir.path(),
            "\
config redirect 'pp_abc123'
\toption name 'Web Server'
\toption src 'wan'
\toption dest 'lan'
\toption target 'DNAT'
\tlist proto 'tcp'
\toption src_dport '80'
\toption dest_port '80'
\toption enabled '1'
\toption _pp_id 'abc123'
\toption _pp_mac 'AA:BB:CC:DD:EE:FF'
",
        );
        let arena = Arena::new();
        let ports = extract_ports(&arena, dir.path()).await.unwrap();
        assert_eq!(ports.len(), 1);
        assert!(ports[0].ipv4);
        assert!(!ports[0].ipv6);
        assert_eq!(ports[0].id, "abc123");
        assert_eq!(ports[0].label, "Web Server");
        assert_eq!(ports[0].device_mac, "AA:BB:CC:DD:EE:FF");
    }

    #[tokio::test]
    async fn extract_ipv6_only() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(
            dir.path(),
            "\
config rule 'pp_abc123_v6'
\toption name 'Web Server v6'
\toption src 'wan'
\toption dest 'lan'
\toption target 'ACCEPT'
\tlist proto 'tcp'
\toption dest_port '80'
\toption family 'ipv6'
\toption enabled '1'
\toption _pp_id 'abc123'
\toption _pp_mac 'AA:BB:CC:DD:EE:FF'
",
        );
        let arena = Arena::new();
        let ports = extract_ports(&arena, dir.path()).await.unwrap();
        assert_eq!(ports.len(), 1);
        assert!(!ports[0].ipv4);
        assert!(ports[0].ipv6);
        assert_eq!(ports[0].id, "abc123");
    }

    #[tokio::test]
    async fn extract_paired_ipv4_ipv6() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(
            dir.path(),
            "\
config redirect 'pp_abc123'
\toption name 'Web Server'
\toption src 'wan'
\toption dest 'lan'
\toption target 'DNAT'
\tlist proto 'tcp'
\toption src_dport '80'
\toption dest_port '80'
\toption enabled '1'
\toption _pp_id 'abc123'
\toption _pp_mac 'AA:BB:CC:DD:EE:FF'

config rule 'pp_abc123_v6'
\toption name 'Web Server v6'
\toption src 'wan'
\toption dest 'lan'
\toption target 'ACCEPT'
\tlist proto 'tcp'
\toption dest_port '80'
\toption family 'ipv6'
\toption enabled '1'
\toption _pp_id 'abc123'
\toption _pp_mac 'AA:BB:CC:DD:EE:FF'
",
        );
        let arena = Arena::new();
        let ports = extract_ports(&arena, dir.path()).await.unwrap();
        assert_eq!(ports.len(), 1);
        assert!(ports[0].ipv4);
        assert!(ports[0].ipv6);
        assert_eq!(ports[0].id, "abc123");
    }

    #[tokio::test]
    async fn extract_legacy_no_pp_id() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(
            dir.path(),
            "\
config redirect 'pp_foo'
\toption name 'Legacy Port'
\toption src 'wan'
\toption dest 'lan'
\toption target 'DNAT'
\tlist proto 'tcp'
\toption src_dport '443'
\toption dest_port '443'
",
        );
        let arena = Arena::new();
        let ports = extract_ports(&arena, dir.path()).await.unwrap();
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].id, "foo");
    }

    #[tokio::test]
    async fn extract_legacy_unnamed_ignored() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(
            dir.path(),
            "\
config redirect
\toption name 'Unnamed Port'
\toption src 'wan'
\toption dest 'lan'
\toption target 'DNAT'
\tlist proto 'tcp'
\toption src_dport '8080'
\toption dest_port '8080'
",
        );
        let arena = Arena::new();
        let ports = extract_ports(&arena, dir.path()).await.unwrap();
        // Redirects without pp_ prefix or _pp_id are not published ports
        assert!(ports.is_empty());
    }

    #[tokio::test]
    async fn extract_ignores_standard_firewall_rules() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(
            dir.path(),
            "\
config rule 'Allow_DHCPv6'
\toption name 'Allow-DHCPv6'
\toption src 'wan'
\toption proto 'udp'
\toption dest_port '546'
\toption family 'ipv6'
\toption target 'ACCEPT'

config rule 'Allow_MLD'
\toption name 'Allow-MLD'
\toption src 'wan'
\toption proto 'icmp'
\toption family 'ipv6'
\toption target 'ACCEPT'

config rule 'Allow_ICMPv6_Forward'
\toption name 'Allow-ICMPv6-Forward'
\toption src 'wan'
\toption dest '*'
\toption proto 'icmp'
\toption family 'ipv6'
\toption target 'ACCEPT'
",
        );
        let arena = Arena::new();
        let ports = extract_ports(&arena, dir.path()).await.unwrap();
        assert!(ports.is_empty(), "standard firewall rules should not appear as published ports");
    }

    #[tokio::test]
    async fn extract_ignores_ghost_entries_with_empty_mac() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(
            dir.path(),
            "\
config redirect 'cfg0a0a0a'
\toption name 'Allow-DHCPv6'
\toption src 'wan'
\toption dest 'lan'
\toption target 'DNAT'
\tlist proto 'udp'
\toption src_dport '546'
\toption dest_port '546'
\toption enabled '1'
\toption _pp_id 'ghost1'
\toption _pp_mac ''

config rule 'cfg0b0b0b'
\toption name 'Allow-DHCPv6 v6'
\toption src 'wan'
\toption dest 'lan'
\toption target 'ACCEPT'
\tlist proto 'udp'
\toption dest_port '546'
\toption family 'ipv6'
\toption enabled '1'
\toption _pp_id 'ghost1'
\toption _pp_mac ''
",
        );
        let arena = Arena::new();
        let ports = extract_ports(&arena, dir.path()).await.unwrap();
        assert!(ports.is_empty(), "ghost entries with empty _pp_mac should be ignored");
    }

    #[tokio::test]
    async fn extract_disabled() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(
            dir.path(),
            "\
config redirect 'pp_dis1'
\toption name 'Disabled Port'
\toption src 'wan'
\toption dest 'lan'
\toption target 'DNAT'
\tlist proto 'tcp'
\toption src_dport '22'
\toption dest_port '22'
\toption enabled '0'
\toption _pp_id 'dis1'
\toption _pp_mac 'AA:BB:CC:DD:EE:FF'
",
        );
        let arena = Arena::new();
        let ports = extract_ports(&arena, dir.path()).await.unwrap();
        assert_eq!(ports.len(), 1);
        assert!(!ports[0].enabled);
    }

    #[tokio::test]
    async fn extract_public_port_differs() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(
            dir.path(),
            "\
config redirect 'pp_pub1'
\toption name 'Custom Public Port'
\toption src 'wan'
\toption dest 'lan'
\toption target 'DNAT'
\tlist proto 'tcp'
\toption src_dport '9090'
\toption dest_port '80'
\toption enabled '1'
\toption _pp_id 'pub1'
\toption _pp_mac 'AA:BB:CC:DD:EE:FF'
",
        );
        let arena = Arena::new();
        let ports = extract_ports(&arena, dir.path()).await.unwrap();
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].ipv4_public_port.as_deref(), Some("9090"));
    }

    #[tokio::test]
    async fn extract_public_port_same() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(
            dir.path(),
            "\
config redirect 'pp_pub2'
\toption name 'Same Public Port'
\toption src 'wan'
\toption dest 'lan'
\toption target 'DNAT'
\tlist proto 'tcp'
\toption src_dport '80'
\toption dest_port '80'
\toption enabled '1'
\toption _pp_id 'pub2'
\toption _pp_mac 'AA:BB:CC:DD:EE:FF'
",
        );
        let arena = Arena::new();
        let ports = extract_ports(&arena, dir.path()).await.unwrap();
        assert_eq!(ports.len(), 1);
        assert!(ports[0].ipv4_public_port.is_none());
    }

    // ── set() tests ──

    fn make_port(id: &str, ipv4: bool, ipv6: bool) -> PublishedPortInput {
        PublishedPortInput {
            id: id.to_string(),
            enabled: true,
            label: format!("Port {}", id),
            device_mac: "AA:BB:CC:DD:EE:FF".to_string(),
            ports: "80".to_string(),
            protocol: Protocol::Tcp,
            ipv4,
            ipv6,
            ipv4_public_port: None,
            source: "any".to_string(),
        }
    }

    #[tokio::test]
    async fn set_creates_redirect_and_rule() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(dir.path(), "");
        let ctx = TestContext(dir.path().to_path_buf());

        set(
            ctx,
            DeserializeStdin(PublishedPortsSetRequest {
                ports: vec![make_port("test1", true, false)],
            }),
        ).await
        .unwrap();

        let arena = Arena::new();
        let ports = extract_ports(&arena, dir.path()).await.unwrap();
        assert_eq!(ports.len(), 1);
        assert!(ports[0].ipv4);
        assert_eq!(ports[0].id, "test1");

        // Verify section names in raw config
        let content = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(content.contains("config redirect pp_test1"), "missing redirect section");
    }

    #[tokio::test]
    async fn set_replaces_existing() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(
            dir.path(),
            "\
config redirect 'pp_old1'
\toption name 'Old Port'
\toption src 'wan'
\toption dest 'lan'
\toption target 'DNAT'
\tlist proto 'tcp'
\toption src_dport '22'
\toption dest_port '22'
\toption _pp_id 'old1'
\toption _pp_mac 'AA:BB:CC:DD:EE:FF'
",
        );
        let ctx = TestContext(dir.path().to_path_buf());

        set(
            ctx,
            DeserializeStdin(PublishedPortsSetRequest {
                ports: vec![make_port("new1", true, false)],
            }),
        ).await
        .unwrap();

        let arena = Arena::new();
        let ports = extract_ports(&arena, dir.path()).await.unwrap();
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].id, "new1");

        let content = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(!content.contains("pp_old1"), "old section should be removed");
    }

    #[tokio::test]
    async fn set_clears_all() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(
            dir.path(),
            "\
config redirect 'pp_del1'
\toption name 'To Delete'
\toption src 'wan'
\toption dest 'lan'
\toption target 'DNAT'
\tlist proto 'tcp'
\toption src_dport '80'
\toption dest_port '80'
\toption _pp_id 'del1'
\toption _pp_mac 'AA:BB:CC:DD:EE:FF'
",
        );
        let ctx = TestContext(dir.path().to_path_buf());

        set(
            ctx,
            DeserializeStdin(PublishedPortsSetRequest { ports: vec![] }),
        ).await
        .unwrap();

        let arena = Arena::new();
        let ports = extract_ports(&arena, dir.path()).await.unwrap();
        assert!(ports.is_empty());
    }

    #[tokio::test]
    async fn set_disabled_port() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(dir.path(), "");
        let ctx = TestContext(dir.path().to_path_buf());

        let mut port = make_port("dis1", true, true);
        port.enabled = false;

        set(
            ctx,
            DeserializeStdin(PublishedPortsSetRequest {
                ports: vec![port],
            }),
        ).await
        .unwrap();

        let arena = Arena::new();
        let ports = extract_ports(&arena, dir.path()).await.unwrap();
        assert_eq!(ports.len(), 1);
        assert!(!ports[0].enabled);

        let content = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(content.contains("option enabled '0'"), "expected 'option enabled 0' in:\n{content}");
    }

    #[tokio::test]
    async fn set_custom_public_port() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(dir.path(), "");
        let ctx = TestContext(dir.path().to_path_buf());

        let mut port = make_port("pub1", true, false);
        port.ipv4_public_port = Some("9090".to_string());

        set(
            ctx,
            DeserializeStdin(PublishedPortsSetRequest {
                ports: vec![port],
            }),
        ).await
        .unwrap();

        let content = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(content.contains("option src_dport '9090'"), "public port should be 9090");
        assert!(content.contains("option dest_port '80'"), "internal port should be 80");

        let arena = Arena::new();
        let ports = extract_ports(&arena, dir.path()).await.unwrap();
        assert_eq!(ports[0].ipv4_public_port.as_deref(), Some("9090"));
    }

    #[tokio::test]
    async fn set_source_restriction() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(dir.path(), "");
        let ctx = TestContext(dir.path().to_path_buf());

        let mut port = make_port("src1", true, true);
        port.source = "10.0.0.0/24".to_string();

        set(
            ctx.clone(),
            DeserializeStdin(PublishedPortsSetRequest {
                ports: vec![port],
            }),
        ).await
        .unwrap();

        let content = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(content.contains("src_ip"), "expected src_ip in:\n{content}");
        assert!(content.contains("10.0.0.0/24"), "expected source CIDR in:\n{content}");

        // Now test with "any" — no src_ip
        let port_any = make_port("src2", true, false);
        set(
            ctx,
            DeserializeStdin(PublishedPortsSetRequest {
                ports: vec![port_any],
            }),
        ).await
        .unwrap();

        let content = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(!content.contains("src_ip"), "source 'any' should not produce src_ip");
    }

    // ── uuid_v4 tests ──

    #[test]
    fn uuid_v4_format() {
        let a = uuid_v4();
        let b = uuid_v4();
        assert_ne!(a, b, "two UUIDs should differ");

        for id in [&a, &b] {
            let parts: Vec<&str> = id.split('-').collect();
            assert_eq!(parts.len(), 5, "UUID should have 5 parts: {}", id);
            assert_eq!(parts[0].len(), 8, "part 0 length");
            assert_eq!(parts[1].len(), 4, "part 1 length");
            assert_eq!(parts[2].len(), 4, "part 2 length");
            assert_eq!(parts[3].len(), 4, "part 3 length");
            assert_eq!(parts[4].len(), 12, "part 4 length");
            assert!(
                id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'),
                "UUID contains invalid chars: {}",
                id
            );
        }
    }

    #[test]
    fn uuid_v4_version_bits() {
        let id = uuid_v4();
        let parts: Vec<&str> = id.split('-').collect();
        // Third group starts with version nibble
        let version_nibble = u8::from_str_radix(&parts[2][..1], 16).unwrap();
        assert_eq!(version_nibble, 4, "version nibble should be 4");
        // Fourth group starts with variant bits (8, 9, a, or b)
        let variant_nibble = u8::from_str_radix(&parts[3][..1], 16).unwrap();
        assert!(
            (0x8..=0xb).contains(&variant_nibble),
            "variant nibble should be 8-b, got {:x}",
            variant_nibble
        );
    }

    #[tokio::test]
    async fn set_sanitizes_section_name() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(dir.path(), "");
        let ctx = TestContext(dir.path().to_path_buf());

        set(
            ctx,
            DeserializeStdin(PublishedPortsSetRequest {
                ports: vec![make_port("a-b-c", true, false)],
            }),
        ).await
        .unwrap();

        let content = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        // Section names must use underscores (hyphens are illegal in UCI names)
        assert!(
            content.contains("config redirect pp_a_b_c"),
            "section name should have underscores, not hyphens, in:\n{content}"
        );
        // But the _pp_id option value preserves the original hyphenated ID
        assert!(
            content.contains("option _pp_id 'a-b-c'"),
            "_pp_id should preserve original ID in:\n{content}"
        );
    }

    // ── Validation tests ──

    fn make_input(overrides: impl FnOnce(&mut PublishedPortInput)) -> PublishedPortInput {
        let mut p = make_port("valid-id", true, false);
        overrides(&mut p);
        p
    }

    fn assert_validation_fails(port: PublishedPortInput, expected_field: &str) {
        let err = validate_inputs(&[port]).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains(expected_field),
            "expected error about '{expected_field}', got: {msg}"
        );
    }

    #[test]
    fn validate_rejects_invalid_id() {
        assert_validation_fails(make_input(|p| p.id = "has spaces".into()), "id");
        assert_validation_fails(make_input(|p| p.id = "has.dots".into()), "id");
        assert_validation_fails(make_input(|p| p.id = "".into()), "id");
        assert_validation_fails(make_input(|p| p.id = "has/slash".into()), "id");
    }

    #[test]
    fn validate_accepts_valid_id() {
        validate_inputs(&[make_input(|p| p.id = "abc-123".into())]).unwrap();
        validate_inputs(&[make_input(|p| p.id = "ABC".into())]).unwrap();
        validate_inputs(&[make_input(|p| p.id = "a".into())]).unwrap();
    }

    #[test]
    fn validate_rejects_empty_label() {
        assert_validation_fails(make_input(|p| p.label = "".into()), "label");
        assert_validation_fails(make_input(|p| p.label = "   ".into()), "label");
    }

    #[test]
    fn validate_rejects_invalid_mac() {
        assert_validation_fails(make_input(|p| p.device_mac = "not-a-mac".into()), "device_mac");
        assert_validation_fails(make_input(|p| p.device_mac = "AA:BB:CC:DD:EE".into()), "device_mac");
        assert_validation_fails(make_input(|p| p.device_mac = "GG:BB:CC:DD:EE:FF".into()), "device_mac");
    }

    #[test]
    fn validate_accepts_valid_mac() {
        validate_inputs(&[make_input(|p| p.device_mac = "aa:bb:cc:dd:ee:ff".into())]).unwrap();
        validate_inputs(&[make_input(|p| p.device_mac = "AA:BB:CC:DD:EE:FF".into())]).unwrap();
    }

    #[test]
    fn validate_allows_empty_mac() {
        // Disabled ports may have empty MAC
        validate_inputs(&[make_input(|p| p.device_mac = "".into())]).unwrap();
    }

    #[test]
    fn validate_rejects_invalid_ports() {
        assert_validation_fails(make_input(|p| p.ports = "0".into()), "ports");
        assert_validation_fails(make_input(|p| p.ports = "99999".into()), "ports");
        assert_validation_fails(make_input(|p| p.ports = "abc".into()), "ports");
        assert_validation_fails(make_input(|p| p.ports = "".into()), "ports");
        assert_validation_fails(make_input(|p| p.ports = "100-50".into()), "ports"); // start > end
    }

    #[test]
    fn validate_accepts_valid_ports() {
        validate_inputs(&[make_input(|p| p.ports = "80".into())]).unwrap();
        validate_inputs(&[make_input(|p| p.ports = "1".into())]).unwrap();
        validate_inputs(&[make_input(|p| p.ports = "65535".into())]).unwrap();
        validate_inputs(&[make_input(|p| p.ports = "80-443".into())]).unwrap();
    }

    #[test]
    fn validate_rejects_invalid_public_port() {
        assert_validation_fails(
            make_input(|p| p.ipv4_public_port = Some("0".into())),
            "ipv4_public_port",
        );
        assert_validation_fails(
            make_input(|p| p.ipv4_public_port = Some("abc".into())),
            "ipv4_public_port",
        );
    }

    #[test]
    fn validate_accepts_valid_public_port() {
        validate_inputs(&[make_input(|p| p.ipv4_public_port = Some("8080".into()))]).unwrap();
        validate_inputs(&[make_input(|p| p.ipv4_public_port = None)]).unwrap();
    }

    #[test]
    fn validate_rejects_invalid_source() {
        assert_validation_fails(make_input(|p| p.source = "".into()), "source");
        assert_validation_fails(make_input(|p| p.source = "not-an-ip".into()), "source");
        assert_validation_fails(make_input(|p| p.source = "10.0.0.0/33".into()), "source");
    }

    #[test]
    fn validate_accepts_valid_source() {
        validate_inputs(&[make_input(|p| p.source = "any".into())]).unwrap();
        validate_inputs(&[make_input(|p| p.source = "10.0.0.0/24".into())]).unwrap();
        validate_inputs(&[make_input(|p| p.source = "192.168.1.1".into())]).unwrap();
    }

    #[tokio::test]
    async fn set_rejects_invalid_input() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(dir.path(), "");
        let ctx = TestContext(dir.path().to_path_buf());

        let result = set(
            ctx,
            DeserializeStdin(PublishedPortsSetRequest {
                ports: vec![make_input(|p| p.ports = "0".into())],
            }),
        ).await;
        assert!(result.is_err());
    }

    // ── affected_ports_for_macs / remove_ports_for_macs tests ──

    const TWO_PORTS_FW: &str = "\
config redirect 'pp_a'
\toption name 'Home Assistant'
\toption src 'wan'
\toption dest 'lan'
\toption target 'DNAT'
\tlist proto 'tcp'
\toption src_dport '8123'
\toption dest_port '8123'
\toption enabled '1'
\toption _pp_id 'a'
\toption _pp_mac 'AA:AA:AA:AA:AA:AA'

config redirect 'pp_b'
\toption name 'Plex'
\toption src 'wan'
\toption dest 'lan'
\toption target 'DNAT'
\tlist proto 'tcp'
\toption src_dport '32400'
\toption dest_port '32400'
\toption enabled '1'
\toption _pp_id 'b'
\toption _pp_mac 'BB:BB:BB:BB:BB:BB'
";

    #[tokio::test]
    async fn affected_filters_by_mac_and_enriches_name() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(dir.path(), TWO_PORTS_FW);

        let mut macs = HashSet::new();
        macs.insert("AA:AA:AA:AA:AA:AA".to_string());
        let mut names = HashMap::new();
        names.insert("AA:AA:AA:AA:AA:AA".to_string(), "Living Room".to_string());

        let affected = affected_ports_for_macs(dir.path(), &macs, &names)
            .await
            .unwrap();
        assert_eq!(affected.len(), 1, "only the matching MAC's port");
        assert_eq!(affected[0].id, "a");
        assert_eq!(affected[0].label, "Home Assistant");
        assert_eq!(affected[0].device_name.as_deref(), Some("Living Room"));
    }

    #[tokio::test]
    async fn affected_case_insensitive_and_empty_macs() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(dir.path(), TWO_PORTS_FW);

        // lower-case input still matches the upper-case stored MAC
        let mut macs = HashSet::new();
        macs.insert("bb:bb:bb:bb:bb:bb".to_string());
        let affected = affected_ports_for_macs(dir.path(), &macs, &HashMap::new())
            .await
            .unwrap();
        assert_eq!(affected.len(), 1);
        assert_eq!(affected[0].id, "b");
        assert!(affected[0].device_name.is_none());

        // empty MAC set short-circuits to no results
        let none = affected_ports_for_macs(dir.path(), &HashSet::new(), &HashMap::new())
            .await
            .unwrap();
        assert!(none.is_empty());
    }

    #[tokio::test]
    async fn remove_ports_drops_sections_and_reservations_for_macs() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(dir.path(), TWO_PORTS_FW);
        std::fs::write(
            dir.path().join("dhcp"),
            "\
config host 'host_aaaaaaaaaaaa'
\toption mac 'AA:AA:AA:AA:AA:AA'
\toption ip '192.168.3.50'
\toption dns '1'

config host 'host_bbbbbbbbbbbb'
\toption mac 'BB:BB:BB:BB:BB:BB'
\toption ip '192.168.1.60'
\toption dns '1'
",
        )
        .unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["firewall", "dhcp"])
            .await
            .unwrap();

        let mut macs = HashSet::new();
        macs.insert("AA:AA:AA:AA:AA:AA".to_string());
        let removed = remove_ports_for_macs(&mut cfgs, &macs);
        assert_eq!(removed, 1, "one firewall section removed");

        dump_all(dir.path(), cfgs).await.unwrap();

        let fw = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(!fw.contains("pp_a"), "MAC A's port should be gone");
        assert!(fw.contains("pp_b"), "MAC B's port should remain");

        let dhcp = std::fs::read_to_string(dir.path().join("dhcp")).unwrap();
        assert!(
            !dhcp.contains("AA:AA:AA:AA:AA:AA"),
            "MAC A's stale reservation should be removed"
        );
        assert!(
            dhcp.contains("BB:BB:BB:BB:BB:BB"),
            "MAC B's reservation should remain"
        );

        // The surviving published port is still parseable.
        let remaining = extract_ports(&Arena::new(), dir.path()).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, "b");
    }

    // ── affected_wifi_ports_for_vacated_profiles tests ──

    /// Two profiles: Admin (lan, 192.168.1.0/24) and Guest (guest,
    /// 192.168.101.0/24). pp_a's device (AA) is reserved on the Guest subnet,
    /// pp_b's device (BB) on the Admin subnet.
    fn setup_two_profile_subnets(dir: &std::path::Path) {
        std::fs::write(
            dir.join("startwrt"),
            "\
config profile lan
\toption fullname 'Admin'
\toption interface 'lan'
\toption vlan_tag '99'

config profile guest
\toption fullname 'Guest'
\toption interface 'guest'
\toption vlan_tag '101'
",
        )
        .unwrap();
        std::fs::write(
            dir.join("network"),
            "\
config interface 'lan'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.255.0'

config interface 'guest'
\toption proto 'static'
\toption ipaddr '192.168.101.1'
\toption netmask '255.255.255.0'
",
        )
        .unwrap();
        setup_firewall(dir, TWO_PORTS_FW);
        std::fs::write(
            dir.join("dhcp"),
            "\
config host 'host_a'
\toption mac 'AA:AA:AA:AA:AA:AA'
\toption ip '192.168.101.50'

config host 'host_b'
\toption mac 'BB:BB:BB:BB:BB:BB'
\toption ip '192.168.1.60'
",
        )
        .unwrap();
    }

    #[tokio::test]
    async fn affected_wifi_ports_filters_by_subnet_and_connection() {
        let dir = tempfile::tempdir().unwrap();
        setup_two_profile_subnets(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let mut vacated = HashSet::new();
        vacated.insert("Guest".to_string());

        // Both devices WiFi-connected: only AA (on the vacated Guest subnet) is
        // affected; BB sits on the Admin subnet, which isn't vacated.
        let wifi_macs: HashSet<String> = ["AA:AA:AA:AA:AA:AA", "BB:BB:BB:BB:BB:BB"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (macs, ports) = affected_wifi_ports_for_vacated_profiles(
            &ctx,
            &vacated,
            &wifi_macs,
            &HashMap::new(),
            &HashMap::new(),
        )
        .await
        .unwrap();
        assert_eq!(ports.len(), 1, "only the Guest-subnet device's port");
        assert_eq!(ports[0].id, "a");
        assert_eq!(macs.len(), 1);
        assert!(macs.contains("AA:AA:AA:AA:AA:AA"));

        // AA is on Ethernet (not in wifi_macs) and BB is on the Admin subnet, so
        // nothing is affected even though Guest is vacated.
        let eth_only: HashSet<String> = ["BB:BB:BB:BB:BB:BB"].iter().map(|s| s.to_string()).collect();
        let (macs, ports) = affected_wifi_ports_for_vacated_profiles(
            &ctx,
            &vacated,
            &eth_only,
            &HashMap::new(),
            &HashMap::new(),
        )
        .await
        .unwrap();
        assert!(ports.is_empty() && macs.is_empty());

        // Empty vacated set short-circuits to nothing.
        let (macs, ports) = affected_wifi_ports_for_vacated_profiles(
            &ctx,
            &HashSet::new(),
            &wifi_macs,
            &HashMap::new(),
            &HashMap::new(),
        )
        .await
        .unwrap();
        assert!(ports.is_empty() && macs.is_empty());
    }

    /// An IPv6-only published port (no IPv4 reservation) is attributed to its
    /// profile via the live `device_profiles` map and caught on vacate.
    #[tokio::test]
    async fn affected_wifi_ports_catches_ipv6_only_via_profile() {
        let dir = tempfile::tempdir().unwrap();
        // Two profiles, but a single IPv6-only port for device CC with NO DHCP
        // reservation at all — only the live profile attribution can place it.
        std::fs::write(
            dir.path().join("startwrt"),
            "\
config profile lan
\toption fullname 'Admin'
\toption interface 'lan'
\toption vlan_tag '99'

config profile guest
\toption fullname 'Guest'
\toption interface 'guest'
\toption vlan_tag '101'
",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("network"),
            "\
config interface 'guest'
\toption proto 'static'
\toption ipaddr '192.168.101.1'
\toption netmask '255.255.255.0'
",
        )
        .unwrap();
        setup_firewall(
            dir.path(),
            "\
config rule 'pp_c_v6'
\toption name 'Cam v6'
\toption src 'wan'
\toption dest 'guest'
\toption target 'ACCEPT'
\tlist proto 'tcp'
\toption dest_port '554'
\toption dest_ip '2001:db8:abcd::cc'
\toption family 'ipv6'
\toption enabled '1'
\toption _pp_id 'c'
\toption _pp_mac 'CC:CC:CC:CC:CC:CC'
",
        );
        std::fs::write(dir.path().join("dhcp"), "").unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        let vacated: HashSet<String> = ["Guest".to_string()].into_iter().collect();
        let wifi_macs: HashSet<String> =
            ["CC:CC:CC:CC:CC:CC".to_string()].into_iter().collect();
        let device_profiles: HashMap<String, String> =
            [("CC:CC:CC:CC:CC:CC".to_string(), "Guest".to_string())]
                .into_iter()
                .collect();

        let (macs, ports) = affected_wifi_ports_for_vacated_profiles(
            &ctx,
            &vacated,
            &wifi_macs,
            &HashMap::new(),
            &device_profiles,
        )
        .await
        .unwrap();
        assert_eq!(ports.len(), 1, "the IPv6-only port should be caught");
        assert_eq!(ports[0].id, "c");
        assert!(macs.contains("CC:CC:CC:CC:CC:CC"));

        // Same device on a non-vacated profile → not affected.
        let admin_profiles: HashMap<String, String> =
            [("CC:CC:CC:CC:CC:CC".to_string(), "Admin".to_string())]
                .into_iter()
                .collect();
        let (macs, ports) = affected_wifi_ports_for_vacated_profiles(
            &ctx,
            &vacated,
            &wifi_macs,
            &HashMap::new(),
            &admin_profiles,
        )
        .await
        .unwrap();
        assert!(ports.is_empty() && macs.is_empty());
    }

    #[tokio::test]
    async fn remove_ports_preserves_named_host_but_clears_ip() {
        let dir = tempfile::tempdir().unwrap();
        setup_firewall(dir.path(), TWO_PORTS_FW);
        // host_a is user-named with a manual IPv6 hostid; host_b is an anonymous
        // published-port auto-reservation.
        std::fs::write(
            dir.path().join("dhcp"),
            "\
config host 'host_a'
\toption name 'Living Room NAS'
\toption mac 'AA:AA:AA:AA:AA:AA'
\toption ip '192.168.3.50'
\toption hostid 'dead:beef:0:50'
\toption dns '1'

config host 'host_b'
\toption mac 'BB:BB:BB:BB:BB:BB'
\toption ip '192.168.1.60'
\toption dns '1'
",
        )
        .unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["firewall", "dhcp"])
            .await
            .unwrap();

        let macs: HashSet<String> = ["AA:AA:AA:AA:AA:AA", "BB:BB:BB:BB:BB:BB"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        remove_ports_for_macs(&mut cfgs, &macs);
        dump_all(dir.path(), cfgs).await.unwrap();

        let dhcp = std::fs::read_to_string(dir.path().join("dhcp")).unwrap();
        // Named host kept; its stale IPv4 cleared but name + manual hostid survive.
        assert!(dhcp.contains("Living Room NAS"), "named host should survive:\n{dhcp}");
        assert!(dhcp.contains("dead:beef:0:50"), "manual hostid should survive:\n{dhcp}");
        assert!(!dhcp.contains("192.168.3.50"), "stale IPv4 should be cleared:\n{dhcp}");
        // Anonymous host removed entirely.
        assert!(!dhcp.contains("BB:BB:BB:BB:BB:BB"), "anonymous host should be removed:\n{dhcp}");
    }
}
