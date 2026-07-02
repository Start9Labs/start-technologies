use crate::prelude::*;
use crate::profiles::Lookup;
use crate::utils::{DeserializeStdin, HandlerExtSerde};
use crate::error::ErrorKind;
use crate::{CliContext, CtrlContext, Error, ServerContext};
use rpc_toolkit::{from_fn_async, from_fn_async_local, HandlerExt as _, ParentHandler};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::invoke::Invoke;
use std::sync::Mutex;
use std::time::Instant;
use uciedit::openwrt::{DhcpHost, WifiDevice, WifiInterface};
use uciedit::{dump_all, parse_all, Arena, Configs, Line, TypedSection};

pub fn devices<C: CtrlContext>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async_local(list)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand("update", from_fn_async_local(update::<C>).no_display())
        .subcommand("forget", from_fn_async_local(forget::<C>).no_display())
        .subcommand(
            "data-usage",
            from_fn_async(data_usage)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
}

// --- Types ---

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum DeviceStatus {
    Online,
    Offline,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Device {
    pub mac: Option<String>,
    /// Fully-resolved display name: UCI static name → live DHCP hostname →
    /// remembered hostname (cache) → `device-<mac>` placeholder.
    pub name: String,
    /// Raw DHCP lease hostname (may be "*"); surfaced as an edit-form hint.
    pub hostname: Option<String>,
    pub status: DeviceStatus,
    pub connection: Option<String>,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub ipv4_static: bool,
    pub ipv6_static: bool,
    pub security_profile: Option<String>,
    pub speed: Option<SpeedData>,
    pub data_usage: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SpeedData {
    pub up: f64,
    pub down: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceUpdateReq {
    pub mac: String,
    pub name: String,
    pub ipv4_static: bool,
    pub ipv4: String,
    pub ipv6_static: bool,
    pub ipv6: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceMacReq {
    pub mac: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataUsagePeriod {
    Week,
    Month,
    #[serde(rename = "3months")]
    ThreeMonths,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DataUsageReq {
    pub mac: String,
    pub period: DataUsagePeriod,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DataUsagePoint {
    pub timestamp: u64,
    pub upload: u64,
    pub download: u64,
}

// --- Traffic cache for speed computation ---

#[derive(Clone)]
struct TrafficSnapshot {
    timestamp: Instant,
    rx_bytes: u64,
    tx_bytes: u64,
}

static TRAFFIC_CACHE: Mutex<Option<HashMap<String, TrafficSnapshot>>> = Mutex::new(None);

/// MACs already attempted over mDNS this daemon run. A device that answers is
/// also persisted to the name cache; one that stays silent is recorded here so
/// it is reverse-resolved at most once per daemon run instead of on every poll.
/// Cleared only by daemon restart (acceptable: a device that later starts
/// answering Bonjour is then picked up on the next restart).
static MDNS_ATTEMPTED: Mutex<Option<std::collections::HashSet<String>>> = Mutex::new(None);

// --- Helpers ---

struct ArpEntry {
    ip: String,
    mac: String,
    interface: String,
    state: String,
}

struct DhcpLease {
    mac: String,
    ip: String,
    hostname: String,
}

/// Placeholder name for a device with no UCI name, DHCP hostname, or cached
/// hostname. Strips colons, takes the last 6 hex chars, lowercases →
/// `device-<suffix>` (kept identical to the frontend's prior name generator).
fn fallback_name(mac: &str) -> String {
    let hex: String = mac.chars().filter(|c| *c != ':').collect();
    let start = hex.len().saturating_sub(6);
    format!("device-{}", hex[start..].to_lowercase())
}

/// Parse a 32-char hex IPv6 address from /proc/net/if_inet6 into standard notation.
fn parse_proc_ipv6_addr(hex: &str) -> Option<String> {
    if hex.len() != 32 {
        return None;
    }
    let mut bytes = [0u8; 16];
    for i in 0..16 {
        bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(std::net::Ipv6Addr::from(bytes).to_string())
}

fn parse_arp_output(output: &str) -> Vec<ArpEntry> {
    let mut entries = Vec::new();
    for line in output.lines() {
        // Format: IP dev INTERFACE lladdr MAC STATE
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 6 && parts[2] == "lladdr" {
            // ip neigh: IP dev IFACE lladdr MAC STATE
            // but sometimes: IP dev IFACE lladdr MAC STATE
            // Actual format can vary. Let's parse more carefully.
        }
        // Try regex-like parsing
        if let Some((ip, rest)) = line.split_once(" dev ") {
            if let Some((iface, rest)) = rest.split_once(" lladdr ") {
                let mut rest_parts = rest.split_whitespace();
                if let Some(mac) = rest_parts.next() {
                    let state = rest_parts.next().unwrap_or("UNKNOWN");
                    // Only LAN interfaces
                    if iface.starts_with("br-lan") {
                        entries.push(ArpEntry {
                            ip: ip.trim().to_string(),
                            mac: mac.to_uppercase(),
                            interface: iface.to_string(),
                            state: state.to_string(),
                        });
                    }
                }
            }
        }
    }
    entries
}

fn parse_dhcp_leases(output: &str) -> Vec<DhcpLease> {
    let mut leases = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Format: expiry_timestamp mac_address ip_address hostname client_id
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            leases.push(DhcpLease {
                mac: parts[1].to_uppercase(),
                ip: parts[2].to_string(),
                hostname: parts[3].to_string(),
            });
        }
    }
    leases
}

/// Directory and filename prefix for dnsmasq lease files.
///
/// StartWRT runs a separate dnsmasq instance per profile when the profile uses
/// custom or VPN DNS (see `profiles.rs`), each writing its own lease file at
/// `/tmp/dhcp.leases.dns_<interface>`; the base `/tmp/dhcp.leases` only holds
/// clients of the main dnsmasq instance. To see every lease we must read them
/// all. (odhcpd's IPv6 lease file lives under `/tmp/hosts/` and is not matched.)
const DHCP_LEASES_DIR: &str = "/tmp";
const DHCP_LEASES_PREFIX: &str = "dhcp.leases";

/// Enumerate every dnsmasq lease file: the base `/tmp/dhcp.leases` plus any
/// per-profile `/tmp/dhcp.leases.dns_*`. Empty if `/tmp` can't be read.
pub(crate) async fn dhcp_lease_files() -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if let Ok(mut dir) = tokio::fs::read_dir(DHCP_LEASES_DIR).await {
        while let Ok(Some(entry)) = dir.next_entry().await {
            if entry
                .file_name()
                .to_str()
                .map_or(false, |n| n.starts_with(DHCP_LEASES_PREFIX))
            {
                files.push(entry.path());
            }
        }
    }
    files
}

/// Read and concatenate the contents of every dnsmasq lease file. Unreadable
/// files are skipped. The result feeds straight into `parse_dhcp_leases`.
pub(crate) async fn read_all_dhcp_leases() -> String {
    let mut out = String::new();
    for path in dhcp_lease_files().await {
        if let Ok(content) = tokio::fs::read_to_string(&path).await {
            out.push_str(&content);
            if !content.ends_with('\n') {
                out.push('\n');
            }
        }
    }
    out
}

/// Run a command and return its stdout, or empty string on failure.
async fn run_cmd(cmd: &str, args: &[&str]) -> String {
    tokio::process::Command::new(cmd)
        .args(args)
        .invoke(ErrorKind::Network.into())
        .await
        .ok()
        .and_then(|out| String::from_utf8(out).ok())
        .unwrap_or_default()
}

/// Parse conntrack output and sum bytes per MAC address.
/// Returns HashMap<MAC, (rx_bytes, tx_bytes)>.
///
/// Each conntrack line has two directions:
///   Direction 1 (src=local): bytes= is tx from the local device
///   Direction 2 (src=remote): bytes= is rx by the local device
fn conntrack_bytes_by_mac(
    content: &str,
    ip_to_mac: &HashMap<String, String>,
) -> HashMap<String, (u64, u64)> {
    let mut result: HashMap<String, (u64, u64)> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse all src= and bytes= fields. A conntrack line has two halves,
        // each with src=, dst=, bytes= fields. We collect them in order.
        let mut srcs: Vec<&str> = Vec::new();
        let mut bytes_vals: Vec<u64> = Vec::new();

        for token in line.split_whitespace() {
            if let Some(val) = token.strip_prefix("src=") {
                srcs.push(val);
            } else if let Some(val) = token.strip_prefix("bytes=") {
                if let Ok(b) = val.parse::<u64>() {
                    bytes_vals.push(b);
                }
            }
        }

        // We need exactly 2 src values and 2 bytes values
        if srcs.len() < 2 || bytes_vals.len() < 2 {
            continue;
        }

        let local_ip = srcs[0]; // direction 1 src = local device
        let tx_bytes = bytes_vals[0]; // direction 1 bytes = tx
        let rx_bytes = bytes_vals[1]; // direction 2 bytes = rx

        if let Some(mac) = ip_to_mac.get(local_ip) {
            let entry = result.entry(mac.clone()).or_insert((0, 0));
            entry.0 += rx_bytes;
            entry.1 += tx_bytes;
        }
    }

    result
}

/// Parse nlbw JSON output. Returns Vec<(mac, rx_bytes, tx_bytes)>.
fn parse_nlbw_json(output: &str) -> Vec<(String, u64, u64)> {
    let parsed: serde_json::Value = match serde_json::from_str(output) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let columns = match parsed.get("columns").and_then(|c| c.as_array()) {
        Some(c) => c,
        None => return Vec::new(),
    };
    let data = match parsed.get("data").and_then(|d| d.as_array()) {
        Some(d) => d,
        None => return Vec::new(),
    };

    let mac_idx = columns
        .iter()
        .position(|c| c.as_str() == Some("mac"));
    let rx_idx = columns
        .iter()
        .position(|c| c.as_str() == Some("rx_bytes"));
    let tx_idx = columns
        .iter()
        .position(|c| c.as_str() == Some("tx_bytes"));

    let (mac_idx, rx_idx, tx_idx) = match (mac_idx, rx_idx, tx_idx) {
        (Some(m), Some(r), Some(t)) => (m, r, t),
        _ => return Vec::new(),
    };

    let mut results = Vec::new();
    for row in data {
        if let Some(row) = row.as_array() {
            let mac = row
                .get(mac_idx)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_uppercase();
            let rx = row
                .get(rx_idx)
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let tx = row
                .get(tx_idx)
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if !mac.is_empty() {
                results.push((mac, rx, tx));
            }
        }
    }
    results
}

/// Discover hostapd interfaces and their connected client MACs,
/// along with radio band info.
/// Returns (MAC→band_label, set of WiFi bridge-port names).
async fn get_wifi_clients() -> (HashMap<String, String>, std::collections::HashSet<String>) {
    // Maps MAC → connection type ("Wi-Fi 2.4GHz" or "Wi-Fi 5GHz")
    let mut wifi_macs: HashMap<String, String> = HashMap::new();
    let mut wifi_ports = std::collections::HashSet::new();

    // 1. List hostapd interfaces via ubus
    let ubus_list = run_cmd("ubus", &["list"]).await;
    let hostapd_ifaces: Vec<&str> = ubus_list
        .lines()
        .filter(|l| l.starts_with("hostapd."))
        .collect();

    if hostapd_ifaces.is_empty() {
        return (wifi_macs, wifi_ports);
    }

    // 2. Get wifi device → band mapping from UCI
    // We'll do this by reading the wireless config
    let mut iface_to_band: HashMap<String, String> = HashMap::new();

    // Read UCI wireless config for device bands
    let arena = Arena::new();
    if let Ok(cfgs) = parse_all("/etc/config", &arena, &["wireless"]).await {
        // Build radio → band map
        let mut radio_band: HashMap<String, String> = HashMap::new();
        cfgs["wireless"].each::<WifiDevice, Error>(|name, dev| {
            if let Some(name) = name {
                let band_label = match dev.band.as_str() {
                    "2g" => "Wi-Fi 2.4GHz",
                    "5g" => "Wi-Fi 5GHz",
                    "6g" => "Wi-Fi 6GHz",
                    _ => "Wi-Fi",
                };
                radio_band.insert(name.to_string(), band_label.to_string());
            }
        }).ok();

        // Map wifi-iface section name → band via its device field
        cfgs["wireless"].try_each(|name, iface: WifiInterface| {
            if let Some(name) = name {
                if let Some(band) = radio_band.get(&iface.device) {
                    // hostapd uses the section name as the interface name
                    iface_to_band.insert(name.to_string(), band.clone());
                }
            }
            Ok::<_, Error>(())
        }).ok();
    }

    // 3. Query each hostapd interface for clients
    for hostapd in hostapd_ifaces {
        // hostapd.wlan0 → wlan0
        let wlan_name = hostapd.strip_prefix("hostapd.").unwrap_or(hostapd);
        wifi_ports.insert(wlan_name.to_string());
        let band = iface_to_band
            .get(wlan_name)
            .cloned()
            .unwrap_or_else(|| "Wi-Fi".to_string());

        let output = run_cmd("ubus", &["call", hostapd, "get_clients"]).await;
        if output.is_empty() {
            continue;
        }

        // Parse the JSON response: { "clients": { "MAC": { ... }, ... } }
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&output) {
            if let Some(clients) = parsed.get("clients").and_then(|c| c.as_object()) {
                for mac in clients.keys() {
                    wifi_macs.insert(mac.to_uppercase(), band.clone());
                }
            }
        }
    }

    (wifi_macs, wifi_ports)
}

/// Parse `bridge fdb show br br-lan` to map each MAC to its bridge port.
/// Only dynamic (learned) entries are included — permanent/self-only entries
/// (multicast groups, etc.) are skipped.
pub async fn get_bridge_fdb() -> HashMap<String, String> {
    let output = run_cmd("bridge", &["fdb", "show", "br", "br-lan"]).await;
    let mut fdb: HashMap<String, String> = HashMap::new();
    for line in output.lines() {
        if line.contains("permanent") {
            continue;
        }
        // Only bridge-level entries (not lower-device "self" entries).
        if !line.contains("master") {
            continue;
        }
        // Format: MAC dev PORT [vlan VLAN] master BRIDGE [offloaded] [...]
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 && parts[1] == "dev" {
            let mac = parts[0].to_uppercase();
            let port = parts[2].to_string();
            fdb.entry(mac).or_insert(port);
        }
    }
    fdb
}

// --- VPN peer types and helpers ---

/// VPN server metadata stored in /etc/config/startwrt (mirrors vpn_server.rs UciVpnServer)
#[derive(Debug, TypedSection)]
#[uci(ty = "vpn_server")]
struct UciVpnServer {
    pub interface: String,
    pub profile_interface: String,
    pub label: String,
    pub listen_port: u16,
    pub endpoint: String,
}

/// Metadata about a VPN server needed for device discovery
struct VpnServerInfo {
    wg_interface: String,
    label: String,
    profile_fullname: String,
}

/// A configured VPN peer from UCI
struct VpnPeerConfig {
    public_key: String,
    name: String,
    ip: Option<String>,
}

/// An active VPN peer from `wg show`
struct WgActivePeer {
    public_key: String,
    latest_handshake: u64,
    rx_bytes: u64,
    tx_bytes: u64,
}

/// Collect VPN server info from already-parsed UCI configs
fn get_vpn_server_info(cfgs: &Configs, lookup: &Lookup) -> Vec<VpnServerInfo> {
    cfgs["startwrt"]
        .sections
        .iter()
        .filter_map(|section| section.get::<UciVpnServer>().ok())
        .filter_map(|meta| {
            let profile_id = lookup.from_interface(&meta.profile_interface)?;
            Some(VpnServerInfo {
                wg_interface: meta.interface.clone(),
                label: meta.label.clone(),
                profile_fullname: profile_id.fullname.clone(),
            })
        })
        .collect()
}

/// Get configured peers for a WireGuard interface from UCI (mirrors vpn_server.rs logic)
fn get_vpn_peer_configs(cfgs: &Configs, wg_interface: &str) -> Vec<VpnPeerConfig> {
    let peer_type = format!("wireguard_{}", wg_interface);

    cfgs["network"]
        .sections
        .iter()
        .filter(|section| section.ty() == peer_type)
        .filter_map(|section| {
            let mut public_key = String::new();
            let mut ip = None;
            let mut description = None;

            for line in &section.lines {
                match line {
                    Line::Option { option, value, .. } => match option.as_str().as_ref() {
                        "public_key" => public_key = value.as_str().to_string(),
                        "description" => description = Some(value.as_str().to_string()),
                        _ => {}
                    },
                    Line::List { list, item, .. } => {
                        if list.as_str() == "allowed_ips" {
                            // Capture the v4 host from "x.x.x.x/32"; ignore the v6
                            // "<ula>/128" entry so it doesn't clobber the IPv4
                            // (mirrors get_peers_for_interface in vpn_server.rs).
                            if let Some(ip_part) = item.as_str().split('/').next() {
                                if ip_part.parse::<std::net::Ipv4Addr>().is_ok() {
                                    ip = Some(ip_part.to_string());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            if public_key.is_empty() {
                return None;
            }

            Some(VpnPeerConfig {
                public_key,
                name: description
                    .unwrap_or_else(|| section.name().unwrap_or_default().to_string()),
                ip,
            })
        })
        .collect()
}

/// Parse `wg show <interface> dump` output into active peer entries.
/// First line is the interface itself; subsequent lines are peers.
fn parse_wg_show_dump(output: &str) -> Vec<WgActivePeer> {
    output
        .lines()
        .skip(1) // skip interface line
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 7 {
                return None;
            }
            Some(WgActivePeer {
                public_key: fields[0].to_string(),
                latest_handshake: fields[4].parse().unwrap_or(0),
                rx_bytes: fields[5].parse().unwrap_or(0),
                tx_bytes: fields[6].parse().unwrap_or(0),
            })
        })
        .collect()
}

/// Query all WireGuard interfaces and return active peer data.
/// Returns Vec<(wg_interface_name, Vec<WgActivePeer>)>.
async fn query_wg_active_peers(wg_interfaces: &[String]) -> Vec<(String, Vec<WgActivePeer>)> {
    let mut results = Vec::new();
    for iface in wg_interfaces {
        let output = run_cmd("wg", &["show", iface, "dump"]).await;
        let peers = parse_wg_show_dump(&output);
        results.push((iface.clone(), peers));
    }
    results
}

fn reload_dnsmasq() {
    tokio::spawn(async {
        let _ = crate::run_quiet_async(
            tokio::process::Command::new("/etc/init.d/dnsmasq").arg("reload"),
        )
        .await;
    });
}

#[allow(dead_code)]
fn reload_firewall_and_dnsmasq() {
    tokio::spawn(async {
        let _ = crate::run_quiet_async(
            tokio::process::Command::new("/etc/init.d/firewall").arg("reload"),
        )
        .await;
        let _ = crate::run_quiet_async(
            tokio::process::Command::new("/etc/init.d/dnsmasq").arg("reload"),
        )
        .await;
    });
}

/// Remove a device's ARP entries and DHCP lease so it disappears from
/// `devices.list`. Also restarts dnsmasq to pick up UCI host removal and
/// the edited lease file. If the device is still connected, it will
/// reappear after its next network activity.
///
/// Dnsmasq owns `/tmp/dhcp.leases` and dumps in-memory state on exit, so
/// we must stop it *first*, edit the file while it's down, then start it.
async fn flush_device_from_network(mac: &str) {
    let mac = mac.to_uppercase();

    // Delete ARP neighbor entries for this MAC
    let arp_output = run_cmd("ip", &["neigh", "show"]).await;
    for entry in parse_arp_output(&arp_output) {
        if entry.mac == mac {
            let _ = tokio::process::Command::new("ip")
                .args(["neigh", "del", &entry.ip, "dev", &entry.interface])
                .invoke(ErrorKind::Network.into())
                .await;
        }
    }

    // Stop dnsmasq so it flushes in-memory leases to disk and exits.
    let _ = crate::run_quiet_async(
        tokio::process::Command::new("/etc/init.d/dnsmasq").arg("stop"),
    )
    .await;

    // Remove the device's lease line from every (now-stable) lease file — the
    // base file and any per-profile `/tmp/dhcp.leases.dns_*`.
    for path in dhcp_lease_files().await {
        if let Ok(content) = tokio::fs::read_to_string(&path).await {
            let filtered: String = content
                .lines()
                .filter(|line| {
                    line.split_whitespace()
                        .nth(1)
                        .map_or(true, |m| m.to_uppercase() != mac)
                })
                .collect::<Vec<_>>()
                .join("\n");
            let filtered = if content.ends_with('\n') && !filtered.is_empty() {
                filtered + "\n"
            } else {
                filtered
            };
            let _ = tokio::fs::write(&path, filtered).await;
        }
    }

    // Start dnsmasq — picks up both the UCI host removal and edited leases.
    let _ = crate::run_quiet_async(
        tokio::process::Command::new("/etc/init.d/dnsmasq").arg("start"),
    )
    .await;
}

// --- Handlers ---

/// Identify non-WiFi MACs that need reachability verification.
///
/// Returns:
/// - `probe_targets`: IPv4 (ip, mac, interface) triples to ping-probe.
///   The interface is included so pings are bound to the correct LAN segment
///   and can't leak to WAN when subnets overlap.
/// - `ipv6_only_macs`: MACs with only IPv6 neighbor entries (no IPv4 to probe)
///    — these are treated as unreachable since we can't verify them
///
/// Active WiFi clients are skipped entirely — hostapd is authoritative.
fn non_wifi_probe_candidates(
    arp_entries: &[ArpEntry],
    wifi_clients: &HashMap<String, String>,
) -> (Vec<(String, String, String)>, std::collections::HashSet<String>) {
    // Collect MACs that have any REACHABLE entry.
    let reachable_macs: std::collections::HashSet<&str> = arp_entries
        .iter()
        .filter(|e| e.state == "REACHABLE" && !e.ip.contains(':'))
        .map(|e| e.mac.as_str())
        .collect();

    // Track which non-WiFi MACs have any alive neighbor entry, and which
    // of those have at least one IPv4 address we can probe.
    let mut alive_non_wifi_macs = std::collections::HashSet::new();
    let mut has_ipv4 = std::collections::HashSet::new();

    let mut targets = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for entry in arp_entries {
        // Skip WiFi clients (hostapd is authoritative for them).
        if wifi_clients.contains_key(&entry.mac) {
            continue;
        }
        // Only consider alive states.
        if !matches!(
            entry.state.as_str(),
            "REACHABLE" | "STALE" | "DELAY" | "PROBE"
        ) {
            continue;
        }
        alive_non_wifi_macs.insert(entry.mac.clone());
        // Only probe IPv4 (ping6 would add complexity).
        if entry.ip.contains(':') {
            continue;
        }
        has_ipv4.insert(entry.mac.clone());
        match entry.state.as_str() {
            // STALE/DELAY/PROBE non-WiFi entries need probing, unless they
            // also have a REACHABLE IPv4 entry (kernel already confirmed them).
            // DELAY and PROBE are intermediate states triggered when the kernel
            // attempts ARP resolution (e.g. from a previous ping probe). If we
            // don't probe these, the device slips through as "Online Ethernet"
            // because the status check counts them as alive but
            // unreachable_macs never flagged them.
            "STALE" | "DELAY" | "PROBE"
                if reachable_macs.contains(entry.mac.as_str()) =>
            {
                continue
            }
            "STALE" | "DELAY" | "PROBE" => {}
            // REACHABLE entries are normally trusted, but if the MAC was
            // recently a WiFi client (not in hostapd anymore, not in
            // wifi_clients), the kernel's REACHABLE state is stale — probe it.
            "REACHABLE" => {}
            _ => continue,
        }
        // One probe per IP is enough.
        if seen.insert(entry.ip.clone()) {
            targets.push((entry.ip.clone(), entry.mac.clone(), entry.interface.clone()));
        }
    }

    // MACs with only IPv6 neighbor entries can't be probed — treat as
    // unreachable so lingering NDP entries don't resurrect offline devices.
    let ipv6_only = alive_non_wifi_macs
        .difference(&has_ipv4)
        .cloned()
        .collect();

    (targets, ipv6_only)
}

/// Ping candidate IPs concurrently. Returns:
/// - `unreachable_macs`: MACs whose *every* probed IP failed to reply (a MAC is
///   only unreachable if ALL of its probed IPs failed).
/// - `live_ipv4s`: the individual IPv4 addresses that *did* reply. This is the
///   ground truth `choose_ipv4_entry` uses to pick the right address when one
///   MAC has neighbor entries on several VLANs — e.g. a device that just roamed
///   profiles, leaving a stale entry on its old bridge.
/// Each target includes the interface to bind to (`-I`), ensuring pings stay
/// on the correct LAN segment and don't leak to WAN on overlapping subnets.
async fn ping_unreachable_macs(
    targets: Vec<(String, String, String)>,
) -> (
    std::collections::HashSet<String>,
    std::collections::HashSet<String>,
) {
    use std::process::Stdio;

    let mut results: Vec<(String, String, tokio::process::Child)> = Vec::new();
    for (ip, mac, iface) in &targets {
        if let Ok(child) = tokio::process::Command::new("ping")
            .args(["-c", "1", "-W", "1", "-I", iface, ip.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            results.push((mac.clone(), ip.clone(), child));
        }
    }
    // Track which MACs were probed, which responded to at least one ping, and
    // the specific IPs that replied.
    let mut probed = std::collections::HashSet::new();
    let mut responded = std::collections::HashSet::new();
    let mut live_ipv4s = std::collections::HashSet::new();
    for (mac, ip, mut child) in results {
        probed.insert(mac.clone());
        let ok = child.wait().await.map(|s| s.success()).unwrap_or(false);
        if ok {
            responded.insert(mac);
            live_ipv4s.insert(ip);
        }
    }
    let unreachable = probed.difference(&responded).cloned().collect();
    (unreachable, live_ipv4s)
}

/// Among a MAC's IPv4 neighbor entries, pick the one most likely to be the
/// device's current address. A device that just roamed to another VLAN leaves a
/// stale entry on its old bridge; both entries share the MAC, so the list must
/// not blindly take the first. Ranking (lower = better):
///   0  REACHABLE      — kernel-confirmed this poll
///   1  probe-confirmed — replied to our active ping (`live_ipv4s`)
///   2  DELAY / PROBE  — kernel mid-resolution
///   3  STALE
///   4  anything else
/// The rank deliberately puts a probe-confirmed STALE entry (1) above an
/// unconfirmed DELAY one (2): the live ping is ground truth, neighbor state is
/// not — a roamed device's *old* entry is often DELAY while the live one is
/// only STALE, so ranking on neighbor state alone would pick the wrong address.
/// Ties keep kernel order (first-listed); a genuinely multi-homed device with
/// two live IPs is served correctly by either.
///
/// Residual window: if a just-roamed device still has a REACHABLE entry on its
/// *old* bridge (not yet decayed), `non_wifi_probe_candidates` skips probing the
/// new STALE entry, so the new IP is absent from `live_ipv4s` and the old
/// REACHABLE entry wins (rank 0) until it ages to STALE — a few tens of seconds.
fn choose_ipv4_entry<'a>(
    arp_list: &[&'a ArpEntry],
    live_ipv4s: &std::collections::HashSet<String>,
) -> Option<&'a ArpEntry> {
    fn rank(e: &ArpEntry, live: &std::collections::HashSet<String>) -> u8 {
        if e.state == "REACHABLE" {
            0
        } else if live.contains(&e.ip) {
            1
        } else if matches!(e.state.as_str(), "DELAY" | "PROBE") {
            2
        } else if e.state == "STALE" {
            3
        } else {
            4
        }
    }
    arp_list
        .iter()
        .filter(|e| !e.ip.contains(':'))
        .copied()
        .min_by_key(|e| rank(e, live_ipv4s))
}

/// Recover a device's name via a reverse mDNS (Bonjour) query against its IPv4.
///
/// Some devices never advertise a hostname via DHCP option 12, so they never
/// land in the lease file — but they still answer mDNS. `avahi-resolve -a <ip>`
/// asks the local avahi daemon (on-link for every LAN VLAN) to issue the
/// reverse query and returns `<ip>\t<name>.local`. Returns the bare `<name>`
/// (sans `.local`), or `None` on timeout, error, or empty result. Bounded by a
/// short timeout with `kill_on_drop` so a non-responder can't stall the device
/// list.
async fn mdns_resolve(ip: &str) -> Option<String> {
    use std::process::Stdio;

    let fut = tokio::process::Command::new("avahi-resolve")
        .args(["-a", ip])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(std::time::Duration::from_millis(1500), fut)
        .await
        .ok()? // timed out
        .ok()?; // spawn/wait failed
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    // First line, second whitespace-delimited field: "<ip>\t<name>.local".
    let line = stdout.lines().next()?;
    let rest = line.split_once(char::is_whitespace)?.1.trim().trim_end_matches('.');
    let name = rest.strip_suffix(".local").unwrap_or(rest).trim();
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

/// Reverse-resolve a batch of `(mac, ipv4)` targets via mDNS concurrently,
/// returning the `MAC -> name` map for those that answered. Bounded fan-out so
/// a large LAN doesn't spawn one `avahi-resolve` per device at once.
async fn resolve_mdns_names(targets: Vec<(String, String)>) -> HashMap<String, String> {
    use futures::stream::{self, StreamExt};
    const CONCURRENCY: usize = 8;
    stream::iter(targets)
        .map(|(mac, ip)| async move { mdns_resolve(&ip).await.map(|name| (mac, name)) })
        .buffer_unordered(CONCURRENCY)
        .filter_map(|r| async move { r })
        .collect()
        .await
}

#[instrument(skip_all)]
pub async fn list(_ctx: ServerContext) -> Result<Vec<Device>, Error> {
    // --- Phase 1: IPv6 multicast discovery (all pings concurrent, ~1s) ---
    //
    // Ping ff02::1 to populate the NDP neighbor table. SLAAC is stateless —
    // the router never learns which addresses clients configured. All pings
    // fire concurrently so wall-clock time is 1s regardless of interface count.
    {
        use std::process::Stdio;

        // When bridge VLAN filtering is active, br-lan.X interfaces exist for
        // each profile VLAN — use those exclusively. Multicast on the bridge
        // master (br-lan) leaks out ALL member ports including WAN, which
        // causes upstream devices to appear as LAN clients.
        // When no VLANs exist (single admin profile), fall back to br-lan.
        let mut ifaces = Vec::new();
        if let Ok(mut entries) = tokio::fs::read_dir("/sys/class/net").await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("br-lan.") {
                    ifaces.push(name);
                }
            }
        }
        if ifaces.is_empty() {
            ifaces.push("br-lan".to_string());
        }

        let mut children: Vec<tokio::process::Child> = Vec::new();

        // Link-local discovery on each LAN VLAN interface.
        for iface in &ifaces {
            if let Ok(child) = tokio::process::Command::new("ping6")
                .args(["-c", "1", "-W", "1", "-I", iface.as_str(), "ff02::1"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
            {
                children.push(child);
            }
        }

        // ULA/global discovery — ping from each interface's global IPv6
        // address so clients respond from their ULA addresses (RFC 6724).
        if let Ok(content) = tokio::fs::read_to_string("/proc/net/if_inet6").await {
            for line in content.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                // Format: addr_hex ifindex prefix_len scope flags ifname
                // scope "00" = global
                if parts.len() >= 6 && parts[3] == "00" && ifaces.iter().any(|i| i == parts[5]) {
                    if let Some(addr) = parse_proc_ipv6_addr(parts[0]) {
                        let dest = format!("ff02::1%{}", parts[5]);
                        if let Ok(child) = tokio::process::Command::new("ping6")
                            .args(["-c", "1", "-W", "1", "-I", &addr, &dest])
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .kill_on_drop(true)
                            .spawn()
                        {
                            children.push(child);
                        }
                    }
                }
            }
        }

        for mut child in children {
            let _ = child.wait().await;
        }
    }

    // --- Phase 2a: Initial data gather (parallel) ---
    //
    // We need ARP + WiFi clients first to identify which STALE entries need
    // probing. UCI parsing has no dependency on any of these, so it runs in
    // parallel too.
    let (arp_output, wifi_result, uci_result, fdb_result) = tokio::join!(
        run_cmd("ip", &["neigh", "show"]),
        get_wifi_clients(),
        async {
            let arena = Arena::new();
            let cfgs = parse_all("/etc/config", &arena, &["dhcp", "startwrt", "network"]).await?;

            let mut hosts_by_mac: HashMap<String, DhcpHost> = HashMap::new();
            cfgs["dhcp"].each::<DhcpHost, Error>(|_, host| {
                hosts_by_mac.insert(host.mac.to_uppercase(), host);
            })?;

            let lookup = Lookup::parse(ServerContext::default(), &cfgs)?;
            let profiles: HashMap<u16, String> = lookup
                .list()
                .iter()
                .map(|p| (p.vlan_tag, p.fullname.clone()))
                .collect();

            // VPN server info and peer configs from UCI
            let vpn_servers = get_vpn_server_info(&cfgs, &lookup);
            let vpn_peer_configs: Vec<(VpnServerInfo, Vec<VpnPeerConfig>)> = vpn_servers
                .into_iter()
                .map(|server| {
                    let peers = get_vpn_peer_configs(&cfgs, &server.wg_interface);
                    (server, peers)
                })
                .collect();

            Ok::<_, Error>((hosts_by_mac, profiles, vpn_peer_configs))
        },
        get_bridge_fdb(),
    );

    let (wifi_clients, wifi_ports) = wifi_result;
    let fdb_by_mac = fdb_result;
    let initial_arp = parse_arp_output(&arp_output);

    // --- Phase 2b: Probe non-WiFi entries + remaining data (parallel) ---
    //
    // For any non-WiFi device (STALE or REACHABLE), ping it to check if it's
    // actually present. This catches both aged-out entries AND recently
    // disconnected WiFi devices whose ARP is still REACHABLE but hostapd has
    // already dropped them. MACs with only IPv6 neighbor entries (no IPv4 to
    // probe) are treated as unreachable. Runs concurrently with nlbw,
    // conntrack, and lease reads so it adds zero net latency.
    let (probe_targets, ipv6_only_macs) =
        non_wifi_probe_candidates(&initial_arp, &wifi_clients);

    // Collect WireGuard interface names for querying active peers.
    // We need to extract this before the uci_result is consumed, but uci_result
    // hasn't been unwrapped yet. Peek at it to get the interface list for wg show.
    let wg_interfaces: Vec<String> = uci_result
        .as_ref()
        .ok()
        .map(|(_, _, vpn_peer_configs)| {
            vpn_peer_configs
                .iter()
                .map(|(server, _)| server.wg_interface.clone())
                .collect()
        })
        .unwrap_or_default();

    let ((unreachable_macs, live_ipv4s), leases_output, nlbw_output, conntrack_output, wg_active_peers) = tokio::join!(
        ping_unreachable_macs(probe_targets),
        read_all_dhcp_leases(),
        run_cmd("nlbw", &["-c", "json", "-g", "mac"]),
        run_cmd("conntrack", &["-L", "-o", "extended"]),
        query_wg_active_peers(&wg_interfaces),
    );

    let mut unreachable_macs = unreachable_macs;
    unreachable_macs.extend(ipv6_only_macs);
    let leases_output = leases_output;

    // Use the initial ARP snapshot — no need to re-read since we use ping
    // exit codes (not kernel NUD state) to determine reachability.
    let arp_entries = initial_arp;
    let dhcp_leases = parse_dhcp_leases(&leases_output);

    // Parse nlbw data for cumulative data usage totals
    let nlbw_data = parse_nlbw_json(&nlbw_output);
    let mut nlbw_by_mac: HashMap<String, (u64, u64)> = HashMap::new();
    for (mac, rx, tx) in &nlbw_data {
        let entry = nlbw_by_mac.entry(mac.clone()).or_insert((0, 0));
        entry.0 += rx;
        entry.1 += tx;
    }

    // Build IP → MAC map from ARP entries for conntrack lookup
    let ip_to_mac: HashMap<String, String> = arp_entries
        .iter()
        .map(|e| (e.ip.clone(), e.mac.clone()))
        .collect();

    // Parse conntrack for real-time byte counters
    let conntrack_by_mac = conntrack_bytes_by_mac(&conntrack_output, &ip_to_mac);

    // Compute speed from conntrack traffic cache
    let now = Instant::now();
    let mut speeds: HashMap<String, SpeedData> = HashMap::new();
    {
        let mut cache_guard = TRAFFIC_CACHE.lock().unwrap();
        let cache = cache_guard.get_or_insert_with(HashMap::new);

        for (mac, &(rx, tx)) in &conntrack_by_mac {
            if let Some(prev_snap) = cache.get(mac) {
                let elapsed = now.duration_since(prev_snap.timestamp).as_secs_f64();
                // Only compute speed if enough time has passed. Use saturating_sub
                // per-direction since conntrack totals can decrease when connections close.
                if elapsed > 1.0 {
                    let rx_delta = rx.saturating_sub(prev_snap.rx_bytes);
                    let tx_delta = tx.saturating_sub(prev_snap.tx_bytes);
                    // Convert to MB/s
                    let down = rx_delta as f64 / elapsed / 1_048_576.0;
                    let up = tx_delta as f64 / elapsed / 1_048_576.0;
                    speeds.insert(
                        mac.clone(),
                        SpeedData {
                            up: (up * 10.0).round() / 10.0,
                            down: (down * 10.0).round() / 10.0,
                        },
                    );
                }
            }
            cache.insert(
                mac.clone(),
                TrafficSnapshot {
                    timestamp: now,
                    rx_bytes: rx,
                    tx_bytes: tx,
                },
            );
        }
    }

    // Unpack UCI results
    let (hosts_by_mac, profile_by_vlan, vpn_peer_configs) = uci_result?;

    // Build ARP index
    let mut arp_by_mac: HashMap<String, Vec<&ArpEntry>> = HashMap::new();
    for entry in &arp_entries {
        arp_by_mac
            .entry(entry.mac.clone())
            .or_default()
            .push(entry);
    }

    // Lease index
    let mut lease_by_mac: HashMap<String, &DhcpLease> = HashMap::new();
    for lease in &dhcp_leases {
        lease_by_mac.insert(lease.mac.clone(), lease);
    }

    // Collect all unique MACs
    let mut all_macs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for mac in arp_by_mac.keys() {
        all_macs.insert(mac.clone());
    }
    for mac in lease_by_mac.keys() {
        all_macs.insert(mac.clone());
    }
    for mac in hosts_by_mac.keys() {
        all_macs.insert(mac.clone());
    }
    // Bridge FDB: a MAC the LAN bridge has learned has an L2 link (link lights
    // on, frames seen) even if it never acquired a DHCP lease or appeared in the
    // IP-neighbor table — e.g. a static-IP device that only talks to other LAN
    // hosts through the external switch, never to the router. Without this it
    // would be silently absent from the list. get_bridge_fdb() already excludes
    // permanent and router-internal (non-`master`) entries.
    for mac in fdb_by_mac.keys() {
        all_macs.insert(mac.clone());
    }

    // --- Phase 4: Build device list ---
    //
    // Load the persistent name cache once. It backfills a remembered hostname
    // for any MAC the live sources (UCI host, DHCP lease) can't name this poll,
    // so a recognized device never reverts to a `device-<mac>` placeholder just
    // because dnsmasq's volatile lease state dropped its name. Observations
    // gathered in the loop are committed back to the cache afterwards.
    let cache_now = chrono::Utc::now().timestamp();
    let name_cache = crate::device_names::load_all();
    let mut name_observations: Vec<crate::device_names::Observation> = Vec::new();

    // mDNS/Bonjour name recovery. For any present device that no live source
    // (UCI host, DHCP lease) or the cache can name, reverse-resolve its IPv4
    // over mDNS — recovers a name for any device that suppresses DHCP option 12
    // but still answers Bonjour. A device that answers is persisted to the name
    // cache below; one that stays silent is recorded in MDNS_ATTEMPTED. Either
    // way a MAC is queried at most once per daemon run, so on a steady-state
    // network there are no targets and this is a no-op. The lock is held only
    // across this synchronous selection loop (no `.await` inside).
    let mut mdns_targets: Vec<(String, String)> = Vec::new();
    {
        let mut guard = MDNS_ATTEMPTED.lock().unwrap();
        let attempted = guard.get_or_insert_with(std::collections::HashSet::new);
        for mac in &all_macs {
            if unreachable_macs.contains(mac) {
                continue;
            }
            if attempted.contains(mac) {
                continue;
            }
            let already_named = hosts_by_mac.get(mac).and_then(|h| h.name.as_ref()).is_some()
                || lease_by_mac
                    .get(mac)
                    .map(|l| l.hostname != "*" && !l.hostname.is_empty())
                    .unwrap_or(false)
                || name_cache.contains_key(mac);
            if already_named {
                continue;
            }
            // Reverse-resolve the device's *current* address, not whatever ARP entry
            // happens to be first — a roamed device's stale old-bridge entry would
            // otherwise be queried and fail. Same chooser used for the displayed IP.
            let ipv4 = arp_by_mac
                .get(mac)
                .and_then(|l| choose_ipv4_entry(l, &live_ipv4s).map(|e| e.ip.clone()))
                .or_else(|| lease_by_mac.get(mac).map(|l| l.ip.clone()));
            if let Some(ip) = ipv4 {
                attempted.insert(mac.clone());
                mdns_targets.push((mac.clone(), ip));
            }
        }
    }
    let mdns_by_mac = resolve_mdns_names(mdns_targets).await;

    let mut devices = Vec::new();
    for mac in &all_macs {
        let arp_list = arp_by_mac.get(mac).cloned().unwrap_or_default();
        let lease = lease_by_mac.get(mac);
        let host = hosts_by_mac.get(mac);

        // Status: hostapd is authoritative for WiFi — if the bridge FDB
        // places a MAC on a WiFi port but hostapd doesn't list it, the
        // device has disconnected even if its ARP entry or ping is alive
        // (the WiFi driver hasn't fully cleaned up yet).
        let on_wifi_port = fdb_by_mac
            .get(mac)
            .map_or(false, |port| wifi_ports.contains(port));
        let status = if on_wifi_port && !wifi_clients.contains_key(mac) {
            DeviceStatus::Offline
        } else if unreachable_macs.contains(mac) {
            DeviceStatus::Offline
        } else if arp_list
            .iter()
            .any(|e| matches!(e.state.as_str(), "REACHABLE" | "STALE" | "DELAY" | "PROBE"))
        {
            DeviceStatus::Online
        } else if fdb_by_mac.contains_key(mac) {
            // Present in the bridge FDB (L2 link up, frames seen) but with no
            // live IP-neighbor entry. The FDB ages (~300 s default), so a
            // just-unplugged device may linger as Online briefly — preferable
            // to a physically-connected device never appearing at all.
            DeviceStatus::Online
        } else {
            DeviceStatus::Offline
        };

        // IPv4: the configured static reservation (UCI `host.ip`) is
        // authoritative when set — it's the address the device is pinned to.
        // Surfacing it first stops the edit form from snapping back to the live
        // DHCP address after a reservation is saved (the client keeps its old
        // lease until it renews, so ARP/lease still hold the previous IP for a
        // while). Otherwise fall back to the best live ARP neighbour — the one
        // chosen by choose_ipv4_entry, which picks the device's current address
        // when a stale entry lingers on its old bridge after roaming VLANs —
        // then the DHCP lease. The same chosen entry drives the profile below,
        // so the displayed IP and profile can never disagree.
        let chosen_arp = choose_ipv4_entry(&arp_list, &live_ipv4s);
        let ipv4 = host
            .and_then(|h| h.ip.clone())
            .or_else(|| chosen_arp.map(|e| e.ip.clone()))
            .or_else(|| lease.map(|l| l.ip.clone()));
        let ipv6 = pick_ipv6(
            arp_list
                .iter()
                .filter(|e| e.ip.contains(':'))
                .map(|e| e.ip.as_str()),
        );

        // Profile from VLAN tag, derived from the same chosen entry as the IPv4
        // address. Fall back to the first entry (e.g. an IPv6-only device with no
        // IPv4 neighbour) then VLAN 1.
        let vlan_tag = chosen_arp
            .or_else(|| arp_list.first().copied())
            .and_then(|e| {
                e.interface
                    .split('.')
                    .nth(1)
                    .and_then(|s| s.parse::<u16>().ok())
            })
            .unwrap_or(1);
        let security_profile = profile_by_vlan.get(&vlan_tag).cloned();

        // Fully-resolved display name. UCI static host (user-assigned) wins,
        // then the live DHCP-lease hostname, then a live mDNS `.local` name,
        // then the remembered hostname from the cache, then a `device-<mac>`
        // placeholder. A fresh DHCP name always overrides a remembered one
        // because the cache sits below it. An mDNS name is learned once for an
        // otherwise-unnamed device and thereafter served from the cache. The
        // reverse-resolve above queries each MAC at most once per daemon run: a
        // cached (named) device is gated out by the cache check, and a device
        // that stays silent is gated out by MDNS_ATTEMPTED.
        let dhcp_hostname = lease.and_then(|l| {
            if l.hostname != "*" && !l.hostname.is_empty() {
                Some(l.hostname.clone())
            } else {
                None
            }
        });
        let mdns_hostname = mdns_by_mac.get(mac).cloned();
        let name = host
            .and_then(|h| h.name.clone())
            .or_else(|| dhcp_hostname.clone())
            .or_else(|| mdns_hostname.clone())
            .or_else(|| name_cache.get(mac).cloned())
            .unwrap_or_else(|| fallback_name(mac));
        let hostname = lease.map(|l| l.hostname.clone());

        // Remember the live-learned name (DHCP, else mDNS), or keep an existing
        // entry alive against prune.
        name_observations.push(crate::device_names::Observation {
            mac: mac.clone(),
            hostname: dhcp_hostname.or(mdns_hostname),
        });

        // Connection type — only label as "Ethernet" when the bridge FDB
        // does NOT place the MAC on a WiFi port. This prevents recently
        // disconnected WiFi clients from being mislabelled.
        let connection = if matches!(status, DeviceStatus::Online) {
            wifi_clients
                .get(mac)
                .cloned()
                .or_else(|| {
                    if !on_wifi_port && (!arp_list.is_empty() || fdb_by_mac.contains_key(mac)) {
                        Some("Ethernet".to_string())
                    } else {
                        None
                    }
                })
        } else {
            None
        };

        // Speed (only for online)
        let speed = if matches!(status, DeviceStatus::Online) {
            speeds.get(mac).cloned()
        } else {
            None
        };

        // Data usage (GB)
        let data_usage = nlbw_by_mac.get(mac).map(|(rx, tx)| {
            ((rx + tx) as f64 / 1_073_741_824.0 * 10.0).round() / 10.0
        });

        devices.push(Device {
            mac: Some(mac.clone()),
            name,
            hostname,
            status,
            connection,
            ipv4,
            ipv6,
            ipv4_static: host.map(|h| h.ip.is_some()).unwrap_or(false),
            ipv6_static: host.map(|h| h.hostid.is_some()).unwrap_or(false),
            security_profile,
            speed,
            data_usage,
        });
    }

    // Persist this poll's observations (learn/refresh names, keep visible
    // entries alive, opportunistic prune). Best-effort — never fails the list.
    crate::device_names::commit(&name_observations, cache_now).await;

    // --- Phase 5: Add VPN-connected peers ---
    //
    // WireGuard peers operate at L3 (no MAC address). We discover them via
    // `wg show` and join with UCI peer configs for name/IP. A peer is
    // considered online if its last handshake was within 180 seconds.
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let handshake_timeout = 180; // 3 minutes

    // Build active peer index: public_key → WgActivePeer, keyed per interface
    let mut wg_active_by_iface: HashMap<String, HashMap<String, &WgActivePeer>> = HashMap::new();
    for (iface, peers) in &wg_active_peers {
        let peer_map: HashMap<String, &WgActivePeer> = peers
            .iter()
            .map(|p| (p.public_key.clone(), p))
            .collect();
        wg_active_by_iface.insert(iface.clone(), peer_map);
    }

    for (server, peer_configs) in &vpn_peer_configs {
        let active_map = wg_active_by_iface.get(&server.wg_interface);

        for peer_cfg in peer_configs {
            let active = active_map.and_then(|m| m.get(&peer_cfg.public_key));

            // Only show peers with a recent handshake
            let is_online = active
                .map(|a| a.latest_handshake > 0 && now_unix.saturating_sub(a.latest_handshake) < handshake_timeout)
                .unwrap_or(false);

            if !is_online {
                continue;
            }

            let active = active.unwrap();

            // Speed from wg show byte deltas via TRAFFIC_CACHE (keyed by public key)
            let cache_key = format!("vpn:{}", peer_cfg.public_key);
            let speed = {
                let mut cache = TRAFFIC_CACHE.lock().unwrap();
                let prev = cache.get_or_insert_with(HashMap::new);
                let speed = if let Some(prev_snap) = prev.get(&cache_key) {
                    let elapsed = now.duration_since(prev_snap.timestamp).as_secs_f64();
                    if elapsed > 1.0 {
                        let rx_delta = active.rx_bytes.saturating_sub(prev_snap.rx_bytes);
                        let tx_delta = active.tx_bytes.saturating_sub(prev_snap.tx_bytes);
                        let down = rx_delta as f64 / elapsed / 1_048_576.0;
                        let up = tx_delta as f64 / elapsed / 1_048_576.0;
                        Some(SpeedData {
                            up: (up * 10.0).round() / 10.0,
                            down: (down * 10.0).round() / 10.0,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };
                prev.insert(
                    cache_key,
                    TrafficSnapshot {
                        timestamp: now,
                        rx_bytes: active.rx_bytes,
                        tx_bytes: active.tx_bytes,
                    },
                );
                speed
            };

            // Data usage from cumulative wg show bytes (resets on interface restart)
            let data_usage = {
                let total = (active.rx_bytes + active.tx_bytes) as f64 / 1_073_741_824.0;
                Some((total * 10.0).round() / 10.0)
            };

            devices.push(Device {
                mac: None,
                name: if peer_cfg.name.is_empty() {
                    "VPN Device".to_string()
                } else {
                    peer_cfg.name.clone()
                },
                hostname: None,
                status: DeviceStatus::Online,
                connection: Some(format!("VPN {}", server.label)),
                ipv4: peer_cfg.ip.clone(),
                ipv6: None,
                ipv4_static: true,
                ipv6_static: false,
                security_profile: Some(server.profile_fullname.clone()),
                speed,
                data_usage,
            });
        }
    }

    Ok(devices)
}

#[instrument(skip_all)]
pub async fn update<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<DeviceUpdateReq>,
) -> Result<(), Error> {
    let mac_upper = req.mac.to_uppercase();
    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["dhcp"]).await?;

        let mut found = false;
        for section in &mut cfgs["dhcp"].sections {
            if let Ok(mut host) = section.get::<DhcpHost>() {
                if host.mac.to_uppercase() == mac_upper {
                    host.name = Some(req.name.clone());
                    if req.ipv4_static && !req.ipv4.is_empty() {
                        host.ip = Some(req.ipv4.clone());
                    } else {
                        host.ip = None;
                    }
                    if req.ipv6_static && !req.ipv6.is_empty() {
                        host.hostid = Some(extract_ipv6_hostid(&req.ipv6));
                    } else {
                        host.hostid = None;
                    }
                    section.set(&host)?;
                    found = true;
                    break;
                }
            }
        }

        if !found {
            // Create new host section
            let new_host = DhcpHost {
                mac: req.mac.clone(),
                name: Some(req.name.clone()),
                ip: if req.ipv4_static && !req.ipv4.is_empty() {
                    Some(req.ipv4.clone())
                } else {
                    None
                },
                hostid: if req.ipv6_static && !req.ipv6.is_empty() {
                    Some(extract_ipv6_hostid(&req.ipv6))
                } else {
                    None
                },
                dns: Some("1".to_string()),
            };
            let section_name = format!(
                "host_{}",
                req.mac.replace(':', "").to_lowercase()
            );
            cfgs["dhcp"].append(&new_host, Some(&section_name))?;
        }

        match dump_all(ctx.uci_root(), cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                let mut details = Vec::new();
                if req.ipv4_static && !req.ipv4.is_empty() {
                    details.push(format!("static IPv4: {}", req.ipv4));
                }
                if req.ipv6_static && !req.ipv6.is_empty() {
                    details.push(format!("static IPv6: {}", req.ipv6));
                }
                let summary = if details.is_empty() {
                    format!("Failed to update device '{}' ({})", req.name, mac_upper)
                } else {
                    format!("Failed to update device '{}' ({}) — {}", req.name, mac_upper, details.join(", "))
                };
                crate::activity::log("device", "updated", false, &summary, Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                let mut details = Vec::new();
                if req.ipv4_static && !req.ipv4.is_empty() {
                    details.push(format!("static IPv4: {}", req.ipv4));
                }
                if req.ipv6_static && !req.ipv6.is_empty() {
                    details.push(format!("static IPv6: {}", req.ipv6));
                }
                let summary = if details.is_empty() {
                    format!("Updated device '{}' ({})", req.name, mac_upper)
                } else {
                    format!("Updated device '{}' ({}) — {}", req.name, mac_upper, details.join(", "))
                };
                crate::activity::log("device", "updated", true, &summary, None);
                if ctx.effectful() {
                    reload_dnsmasq();
                }
                return Ok(());
            }
        }
    }
}

#[instrument(skip_all)]
pub async fn forget<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<DeviceMacReq>,
) -> Result<(), Error> {
    let mac_upper = req.mac.to_uppercase();
    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["dhcp"]).await?;

        // Remove DHCP host
        cfgs["dhcp"].sections.retain(|section| {
            if let Ok(host) = section.get::<DhcpHost>() {
                if host.mac.to_uppercase() == mac_upper {
                    return false;
                }
            }
            true
        });

        match dump_all(ctx.uci_root(), cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log(
                    "device", "forgotten", false,
                    &format!("Failed to forget device {}", mac_upper),
                    Some(&err.to_string()),
                );
                return Err(err.into());
            }
            Ok(()) => {
                crate::activity::log(
                    "device", "forgotten", true,
                    &format!("Forgot device {}", mac_upper),
                    None,
                );
                crate::device_names::forget(&mac_upper).await;
                if ctx.effectful() {
                    flush_device_from_network(&mac_upper).await;
                }
                return Ok(());
            }
        }
    }
}

#[instrument(skip_all)]
pub async fn data_usage(
    _ctx: ServerContext,
    DeserializeStdin(req): DeserializeStdin<DataUsageReq>,
) -> Result<Vec<DataUsagePoint>, Error> {
    use futures::stream::{self, StreamExt};

    let mac_upper = req.mac.to_uppercase();

    let days_back: u64 = match req.period {
        DataUsagePeriod::Week => 7,
        DataUsagePeriod::Month => 30,
        DataUsagePeriod::ThreeMonths => 90,
    };

    // Window: [today - (days_back - 1) .. today], inclusive on both ends.
    // Daily granularity, so days_back=7 gives 7 daily points ending today.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let today_days = now / 86400;
    let earliest_days = today_days.saturating_sub(days_back - 1);

    // List archives nlbwmon currently retains. Each line is YYYY-MM-DD, newest first.
    let list_output = run_cmd("nlbw", &["-c", "list"]).await;
    let retained = std::sync::Arc::new(
        list_output
            .lines()
            .filter_map(|l| parse_ymd_to_days(l.trim()))
            .collect::<std::collections::HashSet<u64>>(),
    );
    let mac_for_query = std::sync::Arc::new(mac_upper);

    // Concurrent fan-out, bounded so we don't spawn 90 subprocesses at once.
    // nlbwmon serializes at the unix-socket level anyway; 8 in flight is plenty.
    const CONCURRENCY: usize = 8;

    let day_range = earliest_days..=today_days;
    let mut points: Vec<DataUsagePoint> = stream::iter(day_range)
        .map(|day| {
            let mac = mac_for_query.clone();
            let retained = retained.clone();
            async move {
                let timestamp = day * 86400;

                // Zero-fill days that aren't in the archive set (no data captured that day).
                if !retained.contains(&day) {
                    return DataUsagePoint { timestamp, upload: 0, download: 0 };
                }

                let (y, m, d) = days_to_ymd(day);
                let date = format!("{:04}-{:02}-{:02}", y, m, d);
                let output = run_cmd("nlbw", &["-c", "json", "-g", "mac", "-t", &date]).await;
                let (download, upload) = lookup_mac_bytes(&output, &mac);
                DataUsagePoint { timestamp, upload, download }
            }
        })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;

    points.sort_by_key(|p| p.timestamp);
    Ok(points)
}

/// Extract `(rx_bytes, tx_bytes)` for a single MAC from an `nlbw -c json -g mac` payload.
/// Returns `(0, 0)` if the MAC isn't present or the payload is malformed.
fn lookup_mac_bytes(output: &str, mac_upper: &str) -> (u64, u64) {
    if output.is_empty() {
        return (0, 0);
    }
    let parsed: serde_json::Value = match serde_json::from_str(output) {
        Ok(v) => v,
        Err(_) => return (0, 0),
    };
    let columns = match parsed.get("columns").and_then(|c| c.as_array()) {
        Some(c) => c,
        None => return (0, 0),
    };
    let data = match parsed.get("data").and_then(|d| d.as_array()) {
        Some(d) => d,
        None => return (0, 0),
    };
    let mac_idx = columns.iter().position(|c| c.as_str() == Some("mac"));
    let rx_idx = columns.iter().position(|c| c.as_str() == Some("rx_bytes"));
    let tx_idx = columns.iter().position(|c| c.as_str() == Some("tx_bytes"));
    let (mac_idx, rx_idx, tx_idx) = match (mac_idx, rx_idx, tx_idx) {
        (Some(m), Some(r), Some(t)) => (m, r, t),
        _ => return (0, 0),
    };
    for row in data {
        if let Some(row) = row.as_array() {
            let row_mac = row
                .get(mac_idx)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_uppercase();
            if row_mac == mac_upper {
                let rx = row.get(rx_idx).and_then(|v| v.as_u64()).unwrap_or(0);
                let tx = row.get(tx_idx).and_then(|v| v.as_u64()).unwrap_or(0);
                return (rx, tx);
            }
        }
    }
    (0, 0)
}

/// Parse a `YYYY-MM-DD` date string into days since the Unix epoch.
fn parse_ymd_to_days(s: &str) -> Option<u64> {
    let mut parts = s.split('-');
    let y: u64 = parts.next()?.parse().ok()?;
    let m: u64 = parts.next()?.parse().ok()?;
    let d: u64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(ymd_to_days(y, m, d))
}

/// Convert (year, month, day) to days since the Unix epoch.
/// Inverse of [`days_to_ymd`].
fn ymd_to_days(y: u64, m: u64, d: u64) -> u64 {
    // Howard Hinnant's algorithm.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Pick the best IPv6 address from candidates, preferring GUA over ULA.
///
/// "Global" is decided by parsing the address and reusing
/// `system::has_global_ipv6` (the `2000::/3` range check), so this shares the
/// single authoritative definition with `is_gua` rather than a string-prefix
/// heuristic that would wrongly accept deprecated/oddball scopes (e.g.
/// `fec0::/10` site-local) as global and return one ahead of a real GUA later
/// in the list. Link-local, any other non-GUA/non-ULA scope, and unparseable
/// strings are unreachable and filtered out, so the result is always a GUA or
/// a ULA. ULA is kept as a fallback for the device-list display and so the
/// published-ports validator can tell "has a local address" apart from
/// "link-local only".
pub fn pick_ipv6<'a>(candidates: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut ula_fallback: Option<&str> = None;
    for ip in candidates {
        let Ok(addr) = ip.parse::<std::net::Ipv6Addr>() else {
            continue;
        };
        if crate::system::has_global_ipv6(std::slice::from_ref(&addr)) {
            return Some(ip.to_string()); // GUA — best possible, return immediately
        }
        // ULA (fc00::/7): first byte 0xfc or 0xfd. Usable fallback if no GUA
        // turns up; keep the first one seen.
        let first_byte = addr.octets()[0];
        if (first_byte == 0xfc || first_byte == 0xfd) && ula_fallback.is_none() {
            ula_fallback = Some(ip);
        }
    }
    ula_fallback.map(|ip| ip.to_string())
}

/// Extract the last 4 hextets from an IPv6 address for use as hostid.
pub(crate) fn extract_ipv6_hostid(ipv6: &str) -> String {
    let parts: Vec<&str> = ipv6.split(':').collect();
    if parts.len() >= 4 {
        parts[parts.len() - 4..].join(":")
    } else {
        ipv6.to_string()
    }
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Adapted from Howard Hinnant's algorithm
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pick_ipv6() {
        // GUA preferred over ULA regardless of order.
        assert_eq!(
            pick_ipv6(["fd00::1", "2001:db8::5"].into_iter()).as_deref(),
            Some("2001:db8::5"),
        );
        assert_eq!(
            pick_ipv6(["2001:db8::5", "fd00::1"].into_iter()).as_deref(),
            Some("2001:db8::5"),
        );

        // ULA returned when there's no GUA.
        assert_eq!(
            pick_ipv6(["fd00::1"].into_iter()).as_deref(),
            Some("fd00::1"),
        );

        // Link-local is excluded — a device with only fe80:: resolves to None,
        // which is what flags it as `ipv6_link_local_only` in published_ports.
        assert_eq!(pick_ipv6(["fe80::1"].into_iter()), None);
        assert_eq!(pick_ipv6(["fe80::1", "fe80::2"].into_iter()), None);

        // Regression: GUA is decided by the parsed 2000::/3 range, not a string
        // prefix. A deprecated site-local (fec0::/10) must NOT be mistaken for a
        // GUA and returned ahead of the real GUA that follows it.
        assert_eq!(
            pick_ipv6(["fec0::1", "2001:db8::5"].into_iter()).as_deref(),
            Some("2001:db8::5"),
        );
        // …and with no GUA present, a non-GUA/non-ULA scope is unreachable, so
        // it's filtered out rather than returned as a usable address.
        assert_eq!(pick_ipv6(["fec0::1"].into_iter()), None);

        // Unparseable candidates are skipped, not returned.
        assert_eq!(pick_ipv6(["not-an-ip"].into_iter()), None);
        assert_eq!(
            pick_ipv6(["not-an-ip", "fd00::1"].into_iter()).as_deref(),
            Some("fd00::1"),
        );

        // Empty → None.
        assert_eq!(pick_ipv6(std::iter::empty()), None);
    }

    #[tokio::test]
    async fn vpn_peer_config_ip_is_v4_not_clobbered_by_v6() {
        // A peer whose allowed_ips lists the v4 /32 first and the WG-/64 v6 /128
        // second (as add_single_peer writes for a v6-serving server). The device
        // list must report the IPv4, not let the trailing v6 entry clobber it.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("network"),
            "\
config interface 'wg_lan'
\toption proto 'wireguard'
\toption private_key 'AAAA'
\tlist addresses '192.168.0.254/32'
\tlist addresses 'fdd3:686e:2179:f001::1/128'

config wireguard_wg_lan 'wg_lan_0'
\toption public_key 'PEERKEY'
\toption description 'Phone'
\tlist allowed_ips '192.168.0.200/32'
\tlist allowed_ips 'fdd3:686e:2179:f001::c8/128'
",
        )
        .unwrap();

        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network"]).await.unwrap();
        let peers = get_vpn_peer_configs(&cfgs, "wg_lan");

        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].ip.as_deref(), Some("192.168.0.200"));
    }

    #[test]
    fn ymd_to_days_round_trip() {
        // Spot-check a few known dates and full round-trip a range.
        assert_eq!(ymd_to_days(1970, 1, 1), 0);
        assert_eq!(ymd_to_days(1970, 1, 2), 1);
        assert_eq!(ymd_to_days(2000, 1, 1), 10957);
        assert_eq!(ymd_to_days(2026, 4, 26), 20569);

        // Round-trip every day across a leap year and a century boundary.
        for days in 10000..12000 {
            let (y, m, d) = days_to_ymd(days);
            assert_eq!(ymd_to_days(y, m, d), days, "{:04}-{:02}-{:02}", y, m, d);
        }
    }

    #[test]
    fn parse_ymd_to_days_accepts_zero_padding() {
        assert_eq!(parse_ymd_to_days("2026-04-26"), Some(20569));
        assert_eq!(parse_ymd_to_days("1970-01-01"), Some(0));
    }

    #[test]
    fn parse_ymd_to_days_rejects_garbage() {
        assert_eq!(parse_ymd_to_days(""), None);
        assert_eq!(parse_ymd_to_days("not a date"), None);
        assert_eq!(parse_ymd_to_days("2026-04"), None);
        assert_eq!(parse_ymd_to_days("2026-04-26-extra"), None);
    }

    #[test]
    fn lookup_mac_bytes_matches_uppercase() {
        let output = r#"{"columns":["mac","conns","rx_bytes","rx_pkts","tx_bytes","tx_pkts"],"data":[["aa:bb:cc:dd:ee:ff",10,1234,5,5678,6],["00:11:22:33:44:55",1,1,1,1,1]]}"#;
        // Lowercase and uppercase queries both work — comparison is uppercase-normalized.
        assert_eq!(lookup_mac_bytes(output, "AA:BB:CC:DD:EE:FF"), (1234, 5678));
    }

    #[test]
    fn lookup_mac_bytes_missing_mac_is_zero() {
        let output = r#"{"columns":["mac","conns","rx_bytes","rx_pkts","tx_bytes","tx_pkts"],"data":[["00:11:22:33:44:55",1,1,1,1,1]]}"#;
        assert_eq!(lookup_mac_bytes(output, "AA:BB:CC:DD:EE:FF"), (0, 0));
    }

    #[test]
    fn lookup_mac_bytes_handles_empty_and_malformed() {
        assert_eq!(lookup_mac_bytes("", "AA:BB:CC:DD:EE:FF"), (0, 0));
        assert_eq!(lookup_mac_bytes("not json", "AA:BB:CC:DD:EE:FF"), (0, 0));
        // Missing rx_bytes column.
        let bad = r#"{"columns":["mac","tx_bytes"],"data":[["aa:bb:cc:dd:ee:ff",1]]}"#;
        assert_eq!(lookup_mac_bytes(bad, "AA:BB:CC:DD:EE:FF"), (0, 0));
    }

    fn arp(ip: &str, iface: &str, state: &str) -> ArpEntry {
        ArpEntry {
            ip: ip.to_string(),
            mac: "94:A9:90:28:9A:A4".to_string(),
            interface: iface.to_string(),
            state: state.to_string(),
        }
    }

    #[test]
    fn choose_ipv4_prefers_probe_confirmed_over_fresher_state() {
        // The real roaming case: the *stale* old-VLAN entry is DELAY (a fresher
        // neighbor state) while the *live* new-VLAN entry is only STALE. The
        // active ping confirmed the new one, so it must win despite ranking
        // lower on neighbor state alone.
        let old = arp("192.168.0.187", "br-lan.1", "DELAY");
        let new = arp("192.168.10.187", "br-lan.101", "STALE");
        let list = vec![&old, &new];
        let live: std::collections::HashSet<String> =
            ["192.168.10.187".to_string()].into_iter().collect();

        let chosen = choose_ipv4_entry(&list, &live).expect("an entry");
        assert_eq!(chosen.ip, "192.168.10.187");
        assert_eq!(chosen.interface, "br-lan.101");
    }

    #[test]
    fn choose_ipv4_prefers_reachable_without_probe() {
        // A REACHABLE entry is kernel-confirmed and never gets probed, so it
        // wins over a STALE entry even when nothing is in the live set.
        let stale = arp("192.168.0.187", "br-lan.1", "STALE");
        let reachable = arp("192.168.10.187", "br-lan.101", "REACHABLE");
        let list = vec![&stale, &reachable];
        let live = std::collections::HashSet::new();

        let chosen = choose_ipv4_entry(&list, &live).expect("an entry");
        assert_eq!(chosen.ip, "192.168.10.187");
    }

    #[test]
    fn choose_ipv4_ignores_ipv6_and_handles_empty() {
        // IPv6 entries are skipped; a MAC with only IPv6 yields None.
        let v6 = arp("fe80::96a9:90ff:fe28:9aa4", "br-lan.101", "STALE");
        let only_v6 = vec![&v6];
        let live = std::collections::HashSet::new();
        assert!(choose_ipv4_entry(&only_v6, &live).is_none());
        assert!(choose_ipv4_entry(&[], &live).is_none());
    }

    #[test]
    fn choose_ipv4_ties_keep_first_listed() {
        // Two equally-ranked (both STALE, neither probed) entries: kernel order
        // wins, deterministically.
        let first = arp("192.168.0.187", "br-lan.1", "STALE");
        let second = arp("192.168.10.187", "br-lan.101", "STALE");
        let list = vec![&first, &second];
        let live = std::collections::HashSet::new();

        let chosen = choose_ipv4_entry(&list, &live).expect("an entry");
        assert_eq!(chosen.ip, "192.168.0.187");
    }
}
