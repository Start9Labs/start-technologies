//! Wireless WAN: the router joins another Wi-Fi network as an uplink.
//!
//! StartWRT owns the `wwan` sections of `wireless` and `network`. The uplink
//! interface joins the `wan` firewall zone, so profiles that route through the
//! WAN, published ports and remote access treat it as the WAN. netifd picks the
//! uplink with the lower route metric while both have a default route.
//!
//! The band carrying the uplink has its own Wi-Fi off while the uplink is not
//! connected. [`watch`] stops an uplink the router cannot join, which brings
//! that Wi-Fi back.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use ipnet::Ipv4Net;
use rpc_toolkit::{from_fn_async_local, ParentHandler};
use serde::{Deserialize, Serialize};
use uciedit::openwrt::{FirewallZone, InterfaceProto, WifiDevice, WifiMode};
use uciedit::{dump_all, parse_all, Arena, Configs, TypedSection};

use crate::invoke::Invoke;
use crate::prelude::*;
use crate::profiles::{self, DEFAULT_WAN_ZONE};
use crate::utils::{DeserializeStdin, HandlerExtSerde};
use crate::wan::{WAN_INTERFACE, WWAN_INTERFACE};
use crate::CtrlContext;

/// Route metric of the uplink that carries traffic while both are up.
const PREFERRED_METRIC: u32 = 10;
/// Route metric of the uplink held in reserve.
const BACKUP_METRIC: u32 = 20;

const MAX_SSID_LEN: usize = 32;

/// How long a new or changed uplink has to connect.
const JOIN_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a network in range may refuse the router.
const REFUSED_TIMEOUT: Duration = Duration::from_secs(120);
/// How old a sighting may be and still put the network in range.
const SEEN_RECENTLY: Duration = Duration::from_secs(90);
/// Weaker signal counts as out of range.
const USABLE_SIGNAL_DBM: i32 = -75;
const WATCH_INTERVAL: Duration = Duration::from_secs(5);
/// The `startwrt` section that records why the uplink was stopped.
const STOP_SECTION: &str = "wwan";

/// A client-mode Wi-Fi interface, read without the access-point options.
#[derive(Debug, Clone, TypedSection)]
#[uci(ty = "wifi-iface")]
pub(crate) struct UplinkStation {
    device: String,
    #[uci(default)]
    pub(crate) mode: WifiMode,
    #[uci(default)]
    network: Option<String>,
    #[uci(default)]
    ssid: String,
    #[uci(default_value = "none".to_string())]
    pub(crate) encryption: String,
    #[uci(default)]
    key: Option<String>,
    #[uci(default_value = false)]
    pub(crate) disabled: bool,
}

/// The `disabled` option of a `wifi-iface`, written without touching its other options.
#[derive(Debug, Default, TypedSection)]
#[uci(ty = "wifi-iface")]
struct StationSwitch {
    #[uci(default_value = false)]
    disabled: bool,
}

/// Why StartWRT stopped the uplink.
#[derive(Debug, Default, TypedSection)]
#[uci(ty = "wwan")]
struct UplinkStop {
    #[uci(default)]
    reason: Option<String>,
}

/// The uplink's network interface. A wireless interface names no `device`.
#[derive(Debug, Default, TypedSection)]
#[uci(ty = "interface")]
struct UplinkInterface {
    #[uci(default)]
    proto: InterfaceProto,
    #[uci(default)]
    metric: Option<u32>,
    #[uci(default)]
    peerdns: Option<String>,
    #[uci(default)]
    dns: Vec<String>,
}

/// The `metric` option of an interface, written without touching its other options.
#[derive(Debug, Default, TypedSection)]
#[uci(ty = "interface")]
pub(crate) struct InterfaceMetric {
    #[uci(default)]
    metric: Option<u32>,
}

// ── Types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, visit_rs::VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum WwanSecurity {
    Open,
    Wpa2,
    Wpa2Wpa3,
    Wpa3,
}
rpc_toolkit::reflect_ts!(WwanSecurity);

impl WwanSecurity {
    fn encryption(self) -> &'static str {
        match self {
            WwanSecurity::Open => "none",
            WwanSecurity::Wpa2 => "psk2",
            WwanSecurity::Wpa2Wpa3 => "sae-mixed",
            WwanSecurity::Wpa3 => "sae",
        }
    }

    /// `None` for an encryption StartWRT does not offer, such as WPA Enterprise.
    pub(crate) fn from_encryption(encryption: &str) -> Option<Self> {
        match encryption.split('+').next()? {
            "none" => Some(WwanSecurity::Open),
            "psk2" | "psk-mixed" => Some(WwanSecurity::Wpa2),
            "sae-mixed" => Some(WwanSecurity::Wpa2Wpa3),
            "sae" => Some(WwanSecurity::Wpa3),
            _ => None,
        }
    }
}

/// Which uplink carries traffic while both the WAN port and the Wi-Fi uplink are up.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, visit_rs::VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum WwanPriority {
    /// Used while the WAN port has no connection.
    Backup,
    /// Used while connected; the WAN port takes over when it drops.
    Preferred,
}
rpc_toolkit::reflect_ts!(WwanPriority);

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct WwanRadio {
    /// The `wifi-device` section, e.g. `radio0`.
    pub radio: String,
    pub band: String,
    pub enabled: bool,
}
rpc_toolkit::reflect_ts!(WwanRadio);

/// Why the router is not trying to join the uplink's network.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, visit_rs::VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum WwanStopReason {
    /// The network was in range and did not accept the router.
    Rejected,
    /// The network was not found when the router tried to join it.
    Unreachable,
    /// Turned off outside StartWRT.
    Disabled,
}
rpc_toolkit::reflect_ts!(WwanStopReason);

impl WwanStopReason {
    fn as_uci(self) -> &'static str {
        match self {
            WwanStopReason::Rejected => "rejected",
            WwanStopReason::Unreachable => "unreachable",
            WwanStopReason::Disabled => "disabled",
        }
    }

    fn from_uci(reason: &str) -> Self {
        match reason {
            "rejected" => WwanStopReason::Rejected,
            "unreachable" => WwanStopReason::Unreachable,
            _ => WwanStopReason::Disabled,
        }
    }

    fn detail(self) -> &'static str {
        match self {
            WwanStopReason::Rejected => "the network did not accept the router",
            WwanStopReason::Unreachable => "the network was not found",
            WwanStopReason::Disabled => "the uplink was turned off",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct WwanUplink {
    pub radio: String,
    pub band: String,
    pub ssid: String,
    pub security: WwanSecurity,
    pub priority: WwanPriority,
    /// Set while the router is not trying to join the network.
    pub stopped: Option<WwanStopReason>,
}
rpc_toolkit::reflect_ts!(WwanUplink);

/// How a join ended.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, visit_rs::VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum WwanJoinOutcome {
    Connected,
    /// The router stopped trying; the uplink's `stopped` says why.
    Stopped,
    /// Undecided when the call returned.
    Joining,
}
rpc_toolkit::reflect_ts!(WwanJoinOutcome);

/// What the router observes of the uplink. Absent off the router.
#[derive(Debug, Default, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct WwanState {
    pub associated: bool,
    /// The uplink's IPv4 address, once DHCP has assigned one.
    pub address: Option<String>,
    /// Signal strength in dBm.
    pub signal: Option<i32>,
    pub channel: Option<u32>,
    /// Whether the router's Internet traffic leaves through the Wi-Fi uplink.
    pub active: bool,
    /// Whether the joined network uses the address range of one of the router's own networks.
    pub subnet_conflict: bool,
}
rpc_toolkit::reflect_ts!(WwanState);

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct WwanResponse {
    pub radios: Vec<WwanRadio>,
    pub uplink: Option<WwanUplink>,
    pub state: Option<WwanState>,
    /// Client-mode Wi-Fi interfaces StartWRT did not create, by section name.
    pub unmanaged: Vec<String>,
}
rpc_toolkit::reflect_ts!(WwanResponse);

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct WwanScanRequest {
    pub radio: String,
}
rpc_toolkit::reflect_ts!(WwanScanRequest);

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct WwanNetwork {
    /// Empty for a network that hides its name.
    pub ssid: String,
    pub bssid: String,
    pub channel: Option<u32>,
    /// Signal strength in dBm.
    pub signal: Option<i32>,
    /// `None` for a network StartWRT cannot join, such as WPA Enterprise.
    pub security: Option<WwanSecurity>,
}
rpc_toolkit::reflect_ts!(WwanNetwork);

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct WwanConnectRequest {
    pub radio: String,
    pub ssid: String,
    pub security: WwanSecurity,
    /// Ignored for an open network.
    pub password: Option<String>,
    pub priority: WwanPriority,
}
rpc_toolkit::reflect_ts!(WwanConnectRequest);

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct WwanPrioritySetRequest {
    pub priority: WwanPriority,
}
rpc_toolkit::reflect_ts!(WwanPrioritySetRequest);

// ── Route registration ──────────────────────────────────────

pub fn wwan<C: CtrlContext + Clone>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "get",
            from_fn_async_local(get::<C>).with_display_serializable(),
        )
        .subcommand(
            "scan",
            from_fn_async_local(scan::<C>).with_display_serializable(),
        )
        .subcommand(
            "connect",
            from_fn_async_local(connect::<C>).with_display_serializable(),
        )
        .subcommand(
            "reconnect",
            from_fn_async_local(reconnect::<C>).with_display_serializable(),
        )
        .subcommand(
            "priority-set",
            from_fn_async_local(priority_set::<C>).with_display_serializable(),
        )
        .subcommand(
            "disconnect",
            from_fn_async_local(disconnect::<C>).with_display_serializable(),
        )
}

// ── Config reads ────────────────────────────────────────────

fn radios(cfgs: &Configs) -> Result<BTreeMap<String, WifiDevice>, Error> {
    let mut radios = BTreeMap::new();
    cfgs["wireless"].try_each(|name, device: WifiDevice| {
        if let Some(name) = name {
            radios.insert(name.to_string(), device);
        }
        Ok::<_, Error>(())
    })?;
    Ok(radios)
}

fn stations(cfgs: &Configs) -> Result<Vec<(Option<String>, UplinkStation)>, Error> {
    let mut stations = Vec::new();
    cfgs["wireless"].try_each(|name, station: UplinkStation| {
        if station.mode == WifiMode::STA {
            stations.push((name.map(|n| n.to_string()), station));
        }
        Ok::<_, Error>(())
    })?;
    Ok(stations)
}

/// Radios an enabled client-mode interface uses, whoever configured it.
pub(crate) fn uplink_radios(cfgs: &Configs) -> Result<BTreeSet<String>, Error> {
    Ok(stations(cfgs)?
        .into_iter()
        .filter(|(_, s)| !s.disabled)
        .map(|(_, s)| s.device)
        .collect())
}

fn interface_metric(cfgs: &Configs, name: &str) -> Result<Option<u32>, Error> {
    for section in &cfgs["network"].sections {
        if section.name().as_deref() == Some(name) {
            if let Some(iface) = section.get_typed::<InterfaceMetric>()? {
                return Ok(iface.metric);
            }
        }
    }
    Ok(None)
}

/// Gives a `wan` interface created beside the uplink the metric that keeps the priority.
pub(crate) fn keep_wan_priority(cfgs: &mut Configs) -> Result<(), Error> {
    let Some(wwan) = interface_metric(cfgs, WWAN_INTERFACE)? else {
        return Ok(());
    };
    let metric = match wwan {
        PREFERRED_METRIC => BACKUP_METRIC,
        _ => PREFERRED_METRIC,
    };
    for section in &mut cfgs["network"].sections {
        if section.name().as_deref() == Some(WAN_INTERFACE) {
            if let Some(mut iface) = section.get_typed::<InterfaceMetric>()? {
                iface.metric = Some(metric);
                section.set(&iface)?;
            }
        }
    }
    Ok(())
}

fn priority_of(cfgs: &Configs) -> Result<WwanPriority, Error> {
    let wan = interface_metric(cfgs, WAN_INTERFACE)?.unwrap_or(0);
    let wwan = interface_metric(cfgs, WWAN_INTERFACE)?.unwrap_or(0);
    Ok(if wwan < wan {
        WwanPriority::Preferred
    } else {
        WwanPriority::Backup
    })
}

fn is_stop_section(section: &uciedit::Section) -> bool {
    section.ty() == UplinkStop::TY && section.name().as_deref() == Some(STOP_SECTION)
}

fn stop_reason(cfgs: &Configs) -> Result<WwanStopReason, Error> {
    for section in cfgs["startwrt"]
        .sections
        .iter()
        .filter(|s| is_stop_section(s))
    {
        if let Some(stop) = section.get_typed::<UplinkStop>()? {
            return Ok(stop
                .reason
                .as_deref()
                .map_or(WwanStopReason::Disabled, WwanStopReason::from_uci));
        }
    }
    Ok(WwanStopReason::Disabled)
}

fn get_config(cfgs: &Configs) -> Result<WwanResponse, Error> {
    let radios_by_name = radios(cfgs)?;
    let mut uplink = None;
    let mut unmanaged = Vec::new();
    for (name, station) in stations(cfgs)? {
        if name.as_deref() != Some(WWAN_INTERFACE) {
            unmanaged.push(name.unwrap_or_default());
            continue;
        }
        let security = WwanSecurity::from_encryption(&station.encryption).ok_or_else(|| {
            Error::new(
                eyre!("unsupported uplink encryption {}", station.encryption),
                ErrorKind::CorruptedWifi,
            )
        })?;
        uplink = Some(WwanUplink {
            band: radios_by_name
                .get(&station.device)
                .map(|d| d.band.clone())
                .unwrap_or_default(),
            radio: station.device,
            ssid: station.ssid,
            security,
            priority: priority_of(cfgs)?,
            stopped: station.disabled.then(|| stop_reason(cfgs)).transpose()?,
        });
    }
    Ok(WwanResponse {
        radios: radios_by_name
            .into_iter()
            .map(|(radio, device)| WwanRadio {
                radio,
                band: device.band,
                enabled: !device.disabled,
            })
            .collect(),
        uplink,
        state: None,
        unmanaged,
    })
}

// ── Router state ────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
struct RadioStatus {
    #[serde(default)]
    interfaces: Vec<RadioInterfaceStatus>,
}

#[derive(Debug, Default, Deserialize)]
struct RadioInterfaceStatus {
    #[serde(default)]
    section: Option<String>,
    #[serde(default)]
    ifname: Option<String>,
}

/// `None` when the object is missing or ubus is unavailable.
async fn ubus_call(
    object: &str,
    method: &str,
    args: Option<serde_json::Value>,
) -> Option<serde_json::Value> {
    let mut cmd = tokio::process::Command::new("ubus");
    cmd.args(["call", object, method]);
    if let Some(args) = args {
        cmd.arg(args.to_string());
    }
    let out = cmd.invoke(ErrorKind::Network.into()).await.ok()?;
    serde_json::from_slice(&out).ok()
}

async fn wireless_status() -> Option<BTreeMap<String, RadioStatus>> {
    serde_json::from_value(ubus_call("network.wireless", "status", None).await?).ok()
}

/// The kernel interface netifd created for a `wifi-iface` section.
fn ifname_of<'a>(status: &'a BTreeMap<String, RadioStatus>, section: &str) -> Option<&'a str> {
    status
        .values()
        .flat_map(|r| &r.interfaces)
        .find(|i| i.section.as_deref() == Some(section))?
        .ifname
        .as_deref()
}

/// The IPv4 networks on each interface of a `network.interface dump`, by interface name.
fn interface_networks(dump: &serde_json::Value) -> Vec<(&str, Ipv4Net)> {
    dump.get("interface")
        .and_then(|i| i.as_array())
        .into_iter()
        .flatten()
        .filter_map(|i| {
            Some((
                i.get("interface")?.as_str()?,
                i.get("ipv4-address")?.as_array()?,
            ))
        })
        .flat_map(|(name, addresses)| {
            addresses.iter().filter_map(move |a| {
                let address = a.get("address")?.as_str()?.parse().ok()?;
                let mask = u8::try_from(a.get("mask")?.as_u64()?).ok()?;
                Some((name, Ipv4Net::new(address, mask).ok()?))
            })
        })
        .collect()
}

/// Whether the uplink's network overlaps one the router serves.
fn subnet_conflict(dump: &serde_json::Value) -> bool {
    let networks = interface_networks(dump);
    let is_uplink = |name: &str| matches!(name, WAN_INTERFACE | WWAN_INTERFACE | "loopback");
    networks
        .iter()
        .filter(|(name, _)| *name == WWAN_INTERFACE)
        .any(|(_, uplink)| {
            networks.iter().any(|(name, net)| {
                !is_uplink(name)
                    && (net.contains(&uplink.network()) || uplink.contains(&net.network()))
            })
        })
}

async fn read_state() -> WwanState {
    let mut state = WwanState::default();
    if let Some(dump) = crate::system::interface_dump().await {
        state.address = interface_networks(&dump)
            .into_iter()
            .find(|(name, _)| *name == WWAN_INTERFACE)
            .map(|(_, net)| net.addr().to_string());
        state.subnet_conflict = subnet_conflict(&dump);
    }
    let radios = wireless_status().await.unwrap_or_default();
    if let Some(ifname) = ifname_of(&radios, WWAN_INTERFACE) {
        state.associated = associated(ifname).await;
        if let Some(info) = ubus_call(
            "iwinfo",
            "info",
            Some(serde_json::json!({ "device": ifname })),
        )
        .await
        {
            state.signal = info
                .get("signal")
                .and_then(|v| v.as_i64())
                .map(|s| s as i32);
            state.channel = info
                .get("channel")
                .and_then(|v| v.as_u64())
                .map(|c| c as u32);
        }
    }
    state.active = crate::system::active_uplink().await.as_deref() == Some(WWAN_INTERFACE);
    state
}

/// A station's carrier is up once it authenticates; iwinfo's bssid is its own address.
async fn associated(ifname: &str) -> bool {
    tokio::fs::read_to_string(format!("/sys/class/net/{ifname}/operstate"))
        .await
        .is_ok_and(|s| s.trim() == "up")
}

/// The kernel interface of StartWRT's uplink, while netifd has one up.
pub(crate) async fn uplink_ifname() -> Option<String> {
    let status = wireless_status().await?;
    ifname_of(&status, WWAN_INTERFACE).map(String::from)
}

async fn uplink_associated() -> bool {
    let status = wireless_status().await.unwrap_or_default();
    match ifname_of(&status, WWAN_INTERFACE) {
        Some(ifname) => associated(ifname).await,
        None => false,
    }
}

// ── Sightings ───────────────────────────────────────────────

/// One entry of the station's scan results for the uplink's network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sighting {
    /// Since the network was last seen.
    age: Duration,
    /// Signal strength in dBm.
    level: i32,
}

/// The id, level, age and ssid of each entry, delimited.
const BSS_MASK: u32 = 0x1 | 0x80 | 0x200 | 0x1000 | 0x20000;
/// Bounds the pages read from one station.
const MAX_BSS_PAGES: usize = 8;

/// wpa_supplicant's escaping of an SSID in its control replies.
fn wpa_ssid_text(ssid: &str) -> String {
    let mut text = String::new();
    for &byte in ssid.as_bytes() {
        match byte {
            b'"' => text.push_str("\\\""),
            b'\\' => text.push_str("\\\\"),
            0x1b => text.push_str("\\e"),
            b'\n' => text.push_str("\\n"),
            b'\r' => text.push_str("\\r"),
            b'\t' => text.push_str("\\t"),
            32..=126 => text.push(byte as char),
            _ => text.push_str(&format!("\\x{byte:02x}")),
        }
    }
    text
}

/// One page of `BSS RANGE` output: the sightings of `ssid`, the last id read,
/// and whether the page reached the final entry.
fn parse_bss_page(page: &str, ssid: &str) -> (Vec<Sighting>, Option<u32>, bool) {
    let wanted = wpa_ssid_text(ssid);
    let mut sightings = Vec::new();
    let mut last_id = None;
    let mut entry: BTreeMap<&str, &str> = BTreeMap::new();
    let mut complete = false;
    for line in page.lines() {
        if line == "====" || line == "####" {
            if let Some(id) = entry.get("id").and_then(|v| v.parse().ok()) {
                last_id = Some(id);
            }
            let level = entry.get("level").and_then(|v| v.parse().ok());
            let age = entry.get("age").and_then(|v| v.parse().ok());
            if let (Some(level), Some(age), Some(true)) =
                (level, age, entry.get("ssid").map(|s| *s == wanted))
            {
                sightings.push(Sighting {
                    age: Duration::from_secs(age),
                    level,
                });
            }
            entry.clear();
            complete = line == "####";
        } else if let Some((key, value)) = line.split_once('=') {
            entry.insert(key, value);
        }
    }
    (sightings, last_id, complete)
}

/// Where the station's wpa_supplicant last saw `ssid`; empty when it is not running.
async fn sightings(ifname: &str, ssid: &str) -> Vec<Sighting> {
    let object = format!("wpa_supplicant.{ifname}");
    let mut found = Vec::new();
    let mut from = 0;
    // A reply holds 4 KiB, so a crowded band takes several pages.
    for _ in 0..MAX_BSS_PAGES {
        let command = format!("BSS RANGE={from}- MASK={BSS_MASK:x}");
        let Some(reply) = ubus_call(
            &object,
            "control",
            Some(serde_json::json!({ "command": command })),
        )
        .await
        else {
            break;
        };
        let page = reply
            .get("result")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let (mut seen, last_id, complete) = parse_bss_page(page, ssid);
        found.append(&mut seen);
        match last_id {
            Some(id) if !complete => from = id + 1,
            _ => break,
        }
    }
    found
}

// ── Scan results ────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
struct ScanResult {
    #[serde(default)]
    ssid: Option<String>,
    #[serde(default)]
    bssid: String,
    #[serde(default)]
    channel: Option<u32>,
    #[serde(default)]
    signal: Option<i32>,
    #[serde(default)]
    encryption: ScanEncryption,
}

#[derive(Debug, Default, Deserialize)]
struct ScanEncryption {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    wep: Option<serde_json::Value>,
    #[serde(default)]
    wpa: Vec<u32>,
    #[serde(default)]
    authentication: Vec<String>,
}

impl ScanEncryption {
    fn security(&self) -> Option<WwanSecurity> {
        if !self.enabled {
            return Some(WwanSecurity::Open);
        }
        if self.wep.is_some() || self.authentication.iter().any(|a| a != "psk" && a != "sae") {
            return None;
        }
        let psk = self.authentication.iter().any(|a| a == "psk");
        let sae = self.authentication.iter().any(|a| a == "sae");
        match (psk, sae) {
            (true, true) => Some(WwanSecurity::Wpa2Wpa3),
            (false, true) => Some(WwanSecurity::Wpa3),
            (true, false) if self.wpa.iter().any(|v| *v >= 2) => Some(WwanSecurity::Wpa2),
            _ => None,
        }
    }
}

/// One entry per network name, strongest signal first. Hidden networks keep
/// one entry per access point.
fn scan_networks(results: Vec<ScanResult>) -> Vec<WwanNetwork> {
    let mut networks: Vec<WwanNetwork> = Vec::new();
    for result in results {
        let ssid = result.ssid.unwrap_or_default();
        let network = WwanNetwork {
            security: result.encryption.security(),
            ssid,
            bssid: result.bssid,
            channel: result.channel,
            signal: result.signal,
        };
        match networks
            .iter_mut()
            .find(|n| !n.ssid.is_empty() && n.ssid == network.ssid)
        {
            Some(existing) if existing.signal < network.signal => *existing = network,
            Some(_) => {}
            None => networks.push(network),
        }
    }
    networks.sort_by(|a, b| b.signal.cmp(&a.signal));
    networks
}

// ── Config writes ───────────────────────────────────────────

fn validate(req: &WwanConnectRequest, radios: &BTreeMap<String, WifiDevice>) -> Result<(), Error> {
    if radios.get(&req.radio).map_or(true, |d| d.disabled) {
        return Err(Error::new(
            eyre!("radio {} is off or missing", req.radio),
            ErrorKind::WifiRadioUnavailable,
        ));
    }
    if req.ssid.is_empty() || req.ssid.len() > MAX_SSID_LEN {
        return Err(Error::new(
            eyre!("network name must be 1 to {MAX_SSID_LEN} bytes"),
            ErrorKind::InvalidValue,
        ));
    }
    if req.ssid.contains(char::is_control) {
        return Err(Error::new(
            eyre!("network name must not contain control characters"),
            ErrorKind::InvalidValue,
        ));
    }
    if req.security != WwanSecurity::Open {
        let password = req.password.as_deref().unwrap_or_default();
        if !(crate::wifi::MIN_PSK_LEN..=crate::wifi::MAX_PSK_LEN).contains(&password.len()) {
            return Err(Error::new(
                eyre!(
                    "password must be {} to {} characters",
                    crate::wifi::MIN_PSK_LEN,
                    crate::wifi::MAX_PSK_LEN
                ),
                ErrorKind::InvalidValue,
            ));
        }
        if password.contains(char::is_control) {
            return Err(Error::new(
                eyre!("password must not contain control characters"),
                ErrorKind::InvalidValue,
            ));
        }
    }
    Ok(())
}

fn set_metrics(cfgs: &mut Configs, priority: WwanPriority) -> Result<(), Error> {
    let (wan, wwan) = match priority {
        WwanPriority::Backup => (PREFERRED_METRIC, BACKUP_METRIC),
        WwanPriority::Preferred => (BACKUP_METRIC, PREFERRED_METRIC),
    };
    for section in &mut cfgs["network"].sections {
        let metric = match section.name().as_deref() {
            Some(WAN_INTERFACE) => wan,
            Some(WWAN_INTERFACE) => wwan,
            _ => continue,
        };
        if let Some(mut iface) = section.get_typed::<InterfaceMetric>()? {
            iface.metric = Some(metric);
            section.set(&iface)?;
        }
    }
    Ok(())
}

fn set_zone_membership(cfgs: &mut Configs, member: bool) -> Result<(), Error> {
    for section in &mut cfgs["firewall"].sections {
        let Ok(mut zone) = section.get::<FirewallZone>() else {
            continue;
        };
        if zone.name != DEFAULT_WAN_ZONE {
            continue;
        }
        let present = zone.network.iter().any(|n| n == WWAN_INTERFACE);
        if member && !present {
            zone.network.push(WWAN_INTERFACE.to_string());
        } else if !member && present {
            zone.network.retain(|n| n != WWAN_INTERFACE);
        } else {
            return Ok(());
        }
        section.set(&zone)?;
        return Ok(());
    }
    Err(Error::new(
        eyre!("firewall zone {DEFAULT_WAN_ZONE} not found"),
        ErrorKind::MissingFirewallZone,
    ))
}

fn remove_sections(cfgs: &mut Configs) {
    for config in ["wireless", "network"] {
        cfgs[config]
            .sections
            .retain(|s| s.name().as_deref() != Some(WWAN_INTERFACE));
    }
    clear_stop(cfgs);
}

fn clear_stop(cfgs: &mut Configs) {
    cfgs["startwrt"].sections.retain(|s| !is_stop_section(s));
}

/// The uplink's settings the router joins with. A change to any is a new join.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Attempted {
    device: String,
    ssid: String,
    encryption: String,
    key: Option<String>,
}

impl From<UplinkStation> for Attempted {
    fn from(station: UplinkStation) -> Self {
        Attempted {
            device: station.device,
            ssid: station.ssid,
            encryption: station.encryption,
            key: station.key,
        }
    }
}

/// StartWRT's uplink, unless it is absent or stopped.
fn running_uplink(cfgs: &Configs) -> Result<Option<Attempted>, Error> {
    Ok(stations(cfgs)?
        .into_iter()
        .find(|(name, _)| name.as_deref() == Some(WWAN_INTERFACE))
        .filter(|(_, station)| !station.disabled)
        .map(|(_, station)| station.into()))
}

fn set_station_disabled(cfgs: &mut Configs, disabled: bool) -> Result<(), Error> {
    for section in &mut cfgs["wireless"].sections {
        if section.name().as_deref() == Some(WWAN_INTERFACE) {
            if let Some(mut station) = section.get_typed::<StationSwitch>()? {
                station.disabled = disabled;
                section.set(&station)?;
            }
        }
    }
    Ok(())
}

/// False when the uplink is no longer the one that was attempted.
fn apply_stop(
    cfgs: &mut Configs,
    attempted: &Attempted,
    reason: WwanStopReason,
) -> Result<bool, Error> {
    if running_uplink(cfgs)?.as_ref() != Some(attempted) {
        return Ok(false);
    }
    set_station_disabled(cfgs, true)?;
    clear_stop(cfgs);
    cfgs["startwrt"].append(
        &UplinkStop {
            reason: Some(reason.as_uci().to_string()),
        },
        Some(STOP_SECTION),
    )?;
    Ok(true)
}

fn apply_reconnect(cfgs: &mut Configs) -> Result<(), Error> {
    let station = stations(cfgs)?
        .into_iter()
        .find(|(name, _)| name.as_deref() == Some(WWAN_INTERFACE))
        .map(|(_, station)| station)
        .ok_or_else(|| Error::new(eyre!("no Wi-Fi uplink"), ErrorKind::NotFound))?;
    if radios(cfgs)?
        .get(&station.device)
        .map_or(true, |d| d.disabled)
    {
        return Err(Error::new(
            eyre!("radio {} is off or missing", station.device),
            ErrorKind::WifiRadioUnavailable,
        ));
    }
    set_station_disabled(cfgs, false)?;
    clear_stop(cfgs);
    Ok(())
}

fn apply_connect(cfgs: &mut Configs, req: &WwanConnectRequest) -> Result<(), Error> {
    let radios = radios(cfgs)?;
    validate(req, &radios)?;
    let unmanaged: Vec<String> = stations(cfgs)?
        .into_iter()
        .filter_map(|(name, _)| {
            (name.as_deref() != Some(WWAN_INTERFACE)).then(|| name.unwrap_or_default())
        })
        .collect();
    if !unmanaged.is_empty() {
        return Err(Error::new(
            eyre!(
                "a Wi-Fi uplink set up outside StartWRT exists: {}",
                unmanaged.join(", ")
            ),
            ErrorKind::WifiUplinkConfiguredElsewhere,
        ));
    }
    let custom_dns = !crate::dns::get_system_dns_servers(cfgs).is_empty();

    remove_sections(cfgs);
    cfgs["wireless"].append(
        &UplinkStation {
            device: req.radio.clone(),
            mode: WifiMode::STA,
            network: Some(WWAN_INTERFACE.to_string()),
            ssid: req.ssid.clone(),
            encryption: req.security.encryption().to_string(),
            key: (req.security != WwanSecurity::Open)
                .then(|| req.password.clone())
                .flatten(),
            disabled: false,
        },
        Some(WWAN_INTERFACE),
    )?;
    cfgs["network"].append(
        &UplinkInterface {
            proto: InterfaceProto::DHCP,
            metric: None,
            peerdns: custom_dns.then(|| "0".to_string()),
            dns: Vec::new(),
        },
        Some(WWAN_INTERFACE),
    )?;
    set_metrics(cfgs, req.priority)?;
    set_zone_membership(cfgs, true)?;
    Ok(())
}

fn apply_disconnect(cfgs: &mut Configs) -> Result<(), Error> {
    remove_sections(cfgs);
    set_zone_membership(cfgs, false)?;
    for section in &mut cfgs["network"].sections {
        if section.name().as_deref() == Some(WAN_INTERFACE) {
            if let Some(mut iface) = section.get_typed::<InterfaceMetric>()? {
                iface.metric = None;
                section.set(&iface)?;
            }
        }
    }
    Ok(())
}

fn require_uplink(cfgs: &Configs) -> Result<(), Error> {
    if stations(cfgs)?
        .iter()
        .any(|(name, _)| name.as_deref() == Some(WWAN_INTERFACE))
    {
        Ok(())
    } else {
        Err(Error::new(eyre!("no Wi-Fi uplink"), ErrorKind::NotFound))
    }
}

/// Re-reads and re-applies on a config conflict. False when `apply` declined to write.
async fn update<C: CtrlContext>(
    ctx: &C,
    apply: impl Fn(&mut Configs) -> Result<bool, Error>,
) -> Result<bool, Error> {
    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["wireless", "network", "firewall", "startwrt", "ddns"],
        )
        .await?;
        if !apply(&mut cfgs)? {
            return Ok(false);
        }
        let dump_result = dump_all(ctx.uci_root(), cfgs).await;
        drop(arena);
        match dump_result {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
            }
            Err(err) => return Err(err.into()),
            Ok(()) => return Ok(true),
        }
    }
}

/// Writes, logs the outcome, and reloads the network and a DDNS service whose source moved.
async fn write<C: CtrlContext>(
    ctx: &C,
    action: &str,
    summary: &str,
    apply: impl Fn(&mut Configs) -> Result<(), Error>,
) -> Result<(), Error> {
    let ddns_moved = Cell::new(false);
    let written = update(ctx, |cfgs| {
        apply(cfgs)?;
        ddns_moved.set(crate::wan::sync_ddns_source(cfgs)?);
        Ok(true)
    })
    .await;
    if let Err(err) = written {
        crate::activity::log("wwan", action, false, summary, Some(&err.to_string()));
        return Err(err);
    }
    crate::activity::log("wwan", action, true, summary, None);
    if ctx.effectful() {
        profiles::reload_system().await?;
        if ddns_moved.get() {
            let _ = crate::run_quiet_async(
                tokio::process::Command::new("/etc/init.d/ddns").arg("restart"),
            )
            .await;
        }
    }
    Ok(())
}

/// Waits for the watcher to settle a join.
async fn await_join<C: CtrlContext>(ctx: &C) -> Result<WwanJoinOutcome, Error> {
    if !ctx.effectful() {
        return Ok(WwanJoinOutcome::Joining);
    }
    // The watcher notices the change within one interval and decides by JOIN_TIMEOUT.
    let deadline = Instant::now() + JOIN_TIMEOUT + WATCH_INTERVAL * 3;
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let stopped = {
            let arena = Arena::new();
            let cfgs =
                parse_all(ctx.uci_root(), &arena, &["wireless", "network", "startwrt"]).await?;
            get_config(&cfgs)?.uplink.map(|u| u.stopped.is_some())
        };
        match stopped {
            None => return Ok(WwanJoinOutcome::Joining),
            Some(true) => return Ok(WwanJoinOutcome::Stopped),
            Some(false) if uplink_associated().await => return Ok(WwanJoinOutcome::Connected),
            Some(false) => {}
        }
    }
    Ok(WwanJoinOutcome::Joining)
}

// ── Handlers ────────────────────────────────────────────────

#[instrument(skip_all)]
pub async fn get<C: CtrlContext>(ctx: C) -> Result<WwanResponse, Error> {
    let mut res = {
        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["wireless", "network", "startwrt"]).await?;
        get_config(&cfgs)?
    };
    if ctx.effectful() && res.uplink.as_ref().is_some_and(|u| u.stopped.is_none()) {
        res.state = Some(read_state().await);
    }
    Ok(res)
}

#[instrument(skip_all)]
pub async fn scan<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<WwanScanRequest>,
) -> Result<Vec<WwanNetwork>, Error> {
    let enabled = {
        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["wireless"]).await?;
        radios(&cfgs)?.get(&req.radio).map(|d| !d.disabled)
    };
    if enabled != Some(true) {
        return Err(Error::new(
            eyre!("radio {} is off or missing", req.radio),
            ErrorKind::WifiRadioUnavailable,
        ));
    }
    if !ctx.effectful() {
        return Ok(Vec::new());
    }
    // iwinfo resolves a radio name to one of its interfaces, or a temporary one.
    let results = tokio::process::Command::new("ubus")
        .args([
            "-t",
            "30",
            "call",
            "iwinfo",
            "scan",
            &serde_json::json!({ "device": req.radio }).to_string(),
        ])
        .invoke(ErrorKind::Network.into())
        .await?;
    #[derive(Deserialize)]
    struct Scan {
        #[serde(default)]
        results: Vec<ScanResult>,
    }
    let scan: Scan =
        serde_json::from_slice(&results).map_err(|e| Error::new(e, ErrorKind::Deserialization))?;
    Ok(scan_networks(scan.results))
}

#[instrument(skip_all)]
pub async fn connect<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<WwanConnectRequest>,
) -> Result<WwanJoinOutcome, Error> {
    write(
        &ctx,
        "added",
        &format!("Added Wi-Fi uplink {}", req.ssid),
        |cfgs| apply_connect(cfgs, &req),
    )
    .await?;
    await_join(&ctx).await
}

#[instrument(skip_all)]
pub async fn reconnect<C: CtrlContext>(ctx: C) -> Result<WwanJoinOutcome, Error> {
    write(&ctx, "retried", "Retried Wi-Fi uplink", apply_reconnect).await?;
    await_join(&ctx).await
}

#[instrument(skip_all)]
pub async fn priority_set<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<WwanPrioritySetRequest>,
) -> Result<(), Error> {
    let summary = match req.priority {
        WwanPriority::Backup => "Set Wi-Fi uplink as backup",
        WwanPriority::Preferred => "Set Wi-Fi uplink as preferred",
    };
    write(&ctx, "priority-updated", summary, |cfgs| {
        require_uplink(cfgs)?;
        set_metrics(cfgs, req.priority)
    })
    .await
}

#[instrument(skip_all)]
pub async fn disconnect<C: CtrlContext>(ctx: C) -> Result<(), Error> {
    write(&ctx, "disconnected", "Removed Wi-Fi uplink", |cfgs| {
        require_uplink(cfgs)?;
        apply_disconnect(cfgs)
    })
    .await
}

// ── Watcher ─────────────────────────────────────────────────

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Wait,
    Joined,
    /// Gives up, during a join or after one.
    Stop {
        reason: WwanStopReason,
        joining: bool,
    },
}

struct Attempt {
    uplink: Attempted,
    /// Added or changed while the watcher ran, and not connected since.
    joining: bool,
    /// When the router last had the connection, or began this attempt.
    since: Instant,
    /// When the network was last seen with usable signal after `since`.
    seen: Option<Instant>,
    refused_since: Option<Instant>,
}

/// An out-of-range interval does not count toward the refusal timeout.
#[derive(Default)]
struct Tracker {
    started: bool,
    attempt: Option<Attempt>,
}

impl Tracker {
    /// No uplink is running.
    fn idle(&mut self) {
        self.started = true;
        self.attempt = None;
    }

    fn observe(
        &mut self,
        uplink: Attempted,
        now: Instant,
        connected: bool,
        sightings: &[Sighting],
    ) -> Verdict {
        let joining = self.started;
        self.started = true;
        let attempt = match &mut self.attempt {
            Some(attempt) if attempt.uplink == uplink => attempt,
            slot => slot.insert(Attempt {
                uplink,
                joining,
                since: now,
                seen: None,
                refused_since: None,
            }),
        };
        if connected {
            let joined = attempt.joining;
            attempt.joining = false;
            attempt.since = now;
            attempt.seen = None;
            attempt.refused_since = None;
            return if joined {
                Verdict::Joined
            } else {
                Verdict::Wait
            };
        }
        for sighting in sightings {
            let at = now.checked_sub(sighting.age).unwrap_or(now);
            if sighting.level >= USABLE_SIGNAL_DBM && at > attempt.since {
                attempt.seen = Some(attempt.seen.map_or(at, |seen| seen.max(at)));
            }
        }
        let elapsed = now.saturating_duration_since(attempt.since);
        if attempt.joining {
            if elapsed >= JOIN_TIMEOUT {
                let reason = if attempt.seen.is_some() {
                    WwanStopReason::Rejected
                } else {
                    WwanStopReason::Unreachable
                };
                return Verdict::Stop {
                    reason,
                    joining: true,
                };
            }
        } else if attempt
            .seen
            .is_some_and(|at| now.saturating_duration_since(at) <= SEEN_RECENTLY)
        {
            let refused_since = attempt.refused_since.get_or_insert(now);
            if now.saturating_duration_since(*refused_since) >= REFUSED_TIMEOUT {
                return Verdict::Stop {
                    reason: WwanStopReason::Rejected,
                    joining: false,
                };
            }
        } else {
            attempt.refused_since = None;
        }
        Verdict::Wait
    }
}

/// Stops the uplink when the router cannot join its network, for the daemon's lifetime.
pub async fn watch() {
    let ctx = crate::ServerContext::default();
    let mut tracker = Tracker::default();
    let mut ticks = tokio::time::interval(WATCH_INTERVAL);
    let mut failure = None;
    loop {
        ticks.tick().await;
        let failed = watch_once(&ctx, &mut tracker)
            .await
            .err()
            .map(|e| e.to_string());
        if let Some(e) = failed.as_ref().filter(|e| failure.as_ref() != Some(*e)) {
            tracing::warn!("Wi-Fi uplink watch failed: {e}");
        }
        failure = failed;
    }
}

async fn watch_once<C: CtrlContext>(ctx: &C, tracker: &mut Tracker) -> Result<(), Error> {
    let uplink = {
        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["wireless"]).await?;
        running_uplink(&cfgs)?
    };
    let Some(uplink) = uplink else {
        tracker.idle();
        return Ok(());
    };
    let status = wireless_status().await.unwrap_or_default();
    let ifname = ifname_of(&status, WWAN_INTERFACE);
    let connected = match ifname {
        Some(ifname) => associated(ifname).await,
        None => false,
    };
    let seen = match ifname {
        Some(ifname) if !connected => sightings(ifname, &uplink.ssid).await,
        _ => Vec::new(),
    };
    match tracker.observe(uplink.clone(), Instant::now(), connected, &seen) {
        Verdict::Wait => {}
        Verdict::Joined => crate::activity::log(
            "wwan",
            "joined",
            true,
            &format!("Joined Wi-Fi uplink {}", uplink.ssid),
            None,
        ),
        Verdict::Stop { reason, joining } => {
            if update(ctx, |cfgs| apply_stop(cfgs, &uplink, reason)).await? {
                let summary = if joining {
                    format!("Couldn't join Wi-Fi uplink {}", uplink.ssid)
                } else {
                    format!("Stopped Wi-Fi uplink {}", uplink.ssid)
                };
                crate::activity::log("wwan", "stopped", false, &summary, Some(reason.detail()));
                if ctx.effectful() {
                    profiles::reload_system().await?;
                }
            }
            tracker.idle();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use rpc_toolkit::Context;
    use tokio::runtime::Runtime;

    use super::*;

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

    const WIRELESS: &str = "\
config wifi-device 'radio0'
\toption type 'mac80211'
\toption band '2g'
\toption channel 'auto'

config wifi-device 'radio1'
\toption type 'mac80211'
\toption band '5g'
\toption channel '36'
\toption disabled '1'

config wifi-iface 'default_radio0'
\toption device 'radio0'
\toption mode 'ap'
\toption network 'lan'
\toption ssid 'Start9'
\toption encryption 'psk2'
\toption key 'adminpassword'
";

    const LUCI_UPLINK: &str = "
config wifi-iface 'wifinet2'
\toption device 'radio0'
\toption mode 'sta'
\toption network 'wwan'
\toption ssid 'Upstream'
\toption encryption 'psk2'
\toption key 'upstreampass'
";

    fn setup(dir: &Path, wireless: &str, startwrt: &str) -> TestContext {
        std::fs::write(dir.join("wireless"), wireless).unwrap();
        std::fs::write(
            dir.join("network"),
            "\
config interface 'wan'
\toption device 'eth1'
\toption proto 'dhcp'

config interface 'wan6'
\toption device '@wan'
\toption proto 'dhcpv6'
",
        )
        .unwrap();
        std::fs::write(
            dir.join("firewall"),
            "\
config zone 'wan_zone'
\toption name 'wan'
\toption input 'REJECT'
\toption output 'ACCEPT'
\toption forward 'REJECT'
\tlist network 'wan'
\tlist network 'wan6'
",
        )
        .unwrap();
        std::fs::write(dir.join("startwrt"), startwrt).unwrap();
        TestContext(dir.to_path_buf())
    }

    fn connect_req(security: WwanSecurity, password: Option<&str>) -> WwanConnectRequest {
        WwanConnectRequest {
            radio: "radio0".into(),
            ssid: "Upstream".into(),
            security,
            password: password.map(String::from),
            priority: WwanPriority::Backup,
        }
    }

    fn read(dir: &Path, config: &str) -> String {
        std::fs::read_to_string(dir.join(config)).unwrap()
    }

    /// Every section with this name, as its text. A header names it quoted or bare.
    fn sections(dir: &Path, config: &str, name: &str) -> Vec<String> {
        let mut found = Vec::new();
        let mut current: Option<String> = None;
        for line in read(dir, config).lines() {
            if line.starts_with("config ") {
                found.extend(current.take());
                let named = line.split_whitespace().nth(2).map(|n| n.trim_matches('\''));
                if named == Some(name) {
                    current = Some(String::new());
                }
            }
            if let Some(body) = &mut current {
                body.push_str(line);
                body.push('\n');
            }
        }
        found.extend(current);
        found
    }

    fn section(dir: &Path, config: &str, name: &str) -> Option<String> {
        sections(dir, config, name).into_iter().next()
    }

    #[tokio::test]
    async fn connect_writes_uplink_as_backup() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = setup(dir.path(), WIRELESS, "");

        connect(
            ctx.clone(),
            DeserializeStdin(connect_req(WwanSecurity::Wpa2, Some("upstreampass"))),
        )
        .await
        .unwrap();

        let station = section(dir.path(), "wireless", WWAN_INTERFACE).unwrap();
        assert!(station.contains("option mode 'sta'"));
        assert!(station.contains("option network 'wwan'"));
        assert!(station.contains("option encryption 'psk2'"));
        assert!(station.contains("option key 'upstreampass'"));
        let iface = section(dir.path(), "network", WWAN_INTERFACE).unwrap();
        assert!(iface.contains("option proto 'dhcp'"));
        assert!(iface.contains("option metric '20'"));
        assert!(!iface.contains("device"));
        assert!(!iface.contains("peerdns"));
        assert!(section(dir.path(), "network", WAN_INTERFACE)
            .unwrap()
            .contains("option metric '10'"));
        assert!(read(dir.path(), "firewall").contains("list network 'wwan'"));
        // The access point on the same radio is untouched.
        assert!(section(dir.path(), "wireless", "default_radio0")
            .unwrap()
            .contains("option key 'adminpassword'"));

        let res = get(ctx).await.unwrap();
        let uplink = res.uplink.unwrap();
        assert_eq!(uplink.radio, "radio0");
        assert_eq!(uplink.band, "2g");
        assert_eq!(uplink.ssid, "Upstream");
        assert_eq!(uplink.security, WwanSecurity::Wpa2);
        assert_eq!(uplink.priority, WwanPriority::Backup);
        assert!(res.unmanaged.is_empty());
        assert!(res.state.is_none());
    }

    #[tokio::test]
    async fn connect_again_replaces_the_uplink() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = setup(dir.path(), WIRELESS, "");
        connect(
            ctx.clone(),
            DeserializeStdin(connect_req(WwanSecurity::Wpa2, Some("upstreampass"))),
        )
        .await
        .unwrap();
        let mut req = connect_req(WwanSecurity::Open, Some("ignored-password"));
        req.ssid = "Cafe".into();
        connect(ctx.clone(), DeserializeStdin(req)).await.unwrap();

        assert_eq!(sections(dir.path(), "wireless", WWAN_INTERFACE).len(), 1);
        let station = section(dir.path(), "wireless", WWAN_INTERFACE).unwrap();
        assert!(station.contains("option ssid 'Cafe'"));
        assert!(station.contains("option encryption 'none'"));
        assert!(!station.contains("key"));
        assert_eq!(sections(dir.path(), "network", WWAN_INTERFACE).len(), 1);
        assert_eq!(
            read(dir.path(), "firewall")
                .matches("list network 'wwan'")
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn connect_keeps_custom_dns_off_the_uplink() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = setup(
            dir.path(),
            WIRELESS,
            "config system_dns 'system_dns'\n\tlist servers '{\"address\":\"9.9.9.9\",\"ssl\":false}'\n",
        );
        connect(
            ctx,
            DeserializeStdin(connect_req(WwanSecurity::Wpa3, Some("upstreampass"))),
        )
        .await
        .unwrap();
        let iface = section(dir.path(), "network", WWAN_INTERFACE).unwrap();
        assert!(iface.contains("option peerdns '0'"));
        assert!(section(dir.path(), "wireless", WWAN_INTERFACE)
            .unwrap()
            .contains("option encryption 'sae'"));
    }

    #[tokio::test]
    async fn connect_rejects_bad_requests() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = setup(dir.path(), WIRELESS, "");
        let before = read(dir.path(), "wireless");

        let err = connect(
            ctx.clone(),
            DeserializeStdin(connect_req(WwanSecurity::Wpa2, Some("short"))),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidValue);

        let err = connect(
            ctx.clone(),
            DeserializeStdin(connect_req(WwanSecurity::Wpa2, None)),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidValue);

        let mut req = connect_req(WwanSecurity::Open, None);
        req.ssid = "x".repeat(MAX_SSID_LEN + 1);
        let err = connect(ctx.clone(), DeserializeStdin(req))
            .await
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidValue);

        let mut req = connect_req(WwanSecurity::Open, None);
        req.ssid = "Up\nstream".into();
        let err = connect(ctx.clone(), DeserializeStdin(req))
            .await
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidValue);

        let err = connect(
            ctx.clone(),
            DeserializeStdin(connect_req(WwanSecurity::Wpa2, Some("upstream\npass"))),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidValue);

        let mut req = connect_req(WwanSecurity::Open, None);
        req.radio = "radio1".into();
        let err = connect(ctx.clone(), DeserializeStdin(req))
            .await
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::WifiRadioUnavailable);

        let mut req = connect_req(WwanSecurity::Open, None);
        req.radio = "radio9".into();
        let err = connect(ctx, DeserializeStdin(req)).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::WifiRadioUnavailable);

        assert_eq!(read(dir.path(), "wireless"), before);
    }

    #[tokio::test]
    async fn uplink_from_luci_is_reported_and_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let wireless = format!("{WIRELESS}{LUCI_UPLINK}");
        let ctx = setup(dir.path(), &wireless, "");

        let res = get(ctx.clone()).await.unwrap();
        assert!(res.uplink.is_none());
        assert_eq!(res.unmanaged, vec!["wifinet2".to_string()]);

        let err = connect(
            ctx.clone(),
            DeserializeStdin(connect_req(WwanSecurity::Wpa2, Some("upstreampass"))),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::WifiUplinkConfiguredElsewhere);
        assert_eq!(read(dir.path(), "wireless"), wireless);

        let err = disconnect(ctx).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
        assert_eq!(read(dir.path(), "wireless"), wireless);
    }

    #[tokio::test]
    async fn connect_and_disconnect_keep_a_zone_written_as_an_option() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = setup(dir.path(), WIRELESS, "");
        let firewall = read(dir.path(), "firewall").replace(
            "\tlist network 'wan'\n\tlist network 'wan6'\n",
            "\toption network 'wan wan6'\n",
        );
        std::fs::write(dir.path().join("firewall"), &firewall).unwrap();
        async fn members(dir: &Path) -> Vec<String> {
            let arena = Arena::new();
            let cfgs = parse_all(dir.to_path_buf(), &arena, &["firewall"])
                .await
                .unwrap();
            cfgs["firewall"].sections[0]
                .get::<FirewallZone>()
                .unwrap()
                .network
        }

        connect(
            ctx.clone(),
            DeserializeStdin(connect_req(WwanSecurity::Open, None)),
        )
        .await
        .unwrap();
        assert_eq!(members(dir.path()).await, ["wan", "wan6", "wwan"]);
        assert!(!read(dir.path(), "firewall").contains("option network"));

        disconnect(ctx).await.unwrap();
        assert_eq!(members(dir.path()).await, ["wan", "wan6"]);
    }

    #[tokio::test]
    async fn get_refuses_an_uplink_encryption_it_cannot_show() {
        let dir = tempfile::tempdir().unwrap();
        let wireless = format!(
            "{WIRELESS}\nconfig wifi-iface 'wwan'\n\toption device 'radio0'\n\toption mode 'sta'\n\toption network 'wwan'\n\toption ssid 'Upstream'\n\toption encryption 'wpa2'\n",
        );
        let ctx = setup(dir.path(), &wireless, "");
        let err = get(ctx).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::CorruptedWifi);
    }

    #[tokio::test]
    async fn priority_set_swaps_metrics() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = setup(dir.path(), WIRELESS, "");

        let err = priority_set(
            ctx.clone(),
            DeserializeStdin(WwanPrioritySetRequest {
                priority: WwanPriority::Preferred,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);

        connect(
            ctx.clone(),
            DeserializeStdin(connect_req(WwanSecurity::Wpa2, Some("upstreampass"))),
        )
        .await
        .unwrap();
        priority_set(
            ctx.clone(),
            DeserializeStdin(WwanPrioritySetRequest {
                priority: WwanPriority::Preferred,
            }),
        )
        .await
        .unwrap();

        assert!(section(dir.path(), "network", WWAN_INTERFACE)
            .unwrap()
            .contains("option metric '10'"));
        assert!(section(dir.path(), "network", WAN_INTERFACE)
            .unwrap()
            .contains("option metric '20'"));
        assert_eq!(
            get(ctx).await.unwrap().uplink.unwrap().priority,
            WwanPriority::Preferred
        );
    }

    #[tokio::test]
    async fn disconnect_restores_the_wan() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = setup(dir.path(), WIRELESS, "");
        let wireless = read(dir.path(), "wireless");
        let network = read(dir.path(), "network");
        let firewall = read(dir.path(), "firewall");

        connect(
            ctx.clone(),
            DeserializeStdin(connect_req(WwanSecurity::Wpa2, Some("upstreampass"))),
        )
        .await
        .unwrap();
        disconnect(ctx.clone()).await.unwrap();

        assert_eq!(read(dir.path(), "wireless"), wireless);
        assert_eq!(read(dir.path(), "network"), network);
        assert_eq!(read(dir.path(), "firewall"), firewall);
        assert!(get(ctx).await.unwrap().uplink.is_none());
    }

    const DDNS: &str = "config service 'wan'\n\toption enabled '1'\n\toption service_name 'duckdns.org'\n\toption ip_source 'network'\n\toption ip_network 'wan'\n\toption interface 'wan'\n\toption domain 'home.duckdns.org'\n";

    #[tokio::test]
    async fn ddns_publishes_the_active_uplink_while_an_uplink_exists() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = setup(dir.path(), WIRELESS, "");
        std::fs::write(dir.path().join("ddns"), DDNS).unwrap();

        connect(
            ctx.clone(),
            DeserializeStdin(connect_req(WwanSecurity::Open, None)),
        )
        .await
        .unwrap();
        let service = section(dir.path(), "ddns", "wan").unwrap();
        assert!(service.contains("option ip_source 'script'"), "{service}");
        assert!(
            service.contains("option ip_script '/usr/bin/startwrt-cli wan uplink-ipv4'"),
            "{service}"
        );
        assert!(!service.contains("ip_network"), "{service}");
        assert!(service.contains("option interface 'wan'"), "{service}");
        assert!(
            service.contains("option domain 'home.duckdns.org'"),
            "{service}"
        );

        disconnect(ctx).await.unwrap();
        let service = section(dir.path(), "ddns", "wan").unwrap();
        assert!(service.contains("option ip_source 'network'"), "{service}");
        assert!(service.contains("option ip_network 'wan'"), "{service}");
        assert!(!service.contains("ip_script"), "{service}");
    }

    #[tokio::test]
    async fn wan_port_created_beside_an_uplink_keeps_the_priority() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = setup(dir.path(), WIRELESS, "");
        let recreated = || async {
            let arena = Arena::new();
            let mut cfgs = parse_all(dir.path().to_path_buf(), &arena, &["network"])
                .await
                .unwrap();
            cfgs["network"]
                .sections
                .retain(|s| s.name().as_deref() != Some(WAN_INTERFACE));
            cfgs["network"]
                .append(&UplinkInterface::default(), Some(WAN_INTERFACE))
                .unwrap();
            keep_wan_priority(&mut cfgs).unwrap();
            interface_metric(&cfgs, WAN_INTERFACE).unwrap()
        };
        assert_eq!(recreated().await, None);

        connect(
            ctx.clone(),
            DeserializeStdin(connect_req(WwanSecurity::Open, None)),
        )
        .await
        .unwrap();
        assert_eq!(recreated().await, Some(PREFERRED_METRIC));

        priority_set(
            ctx,
            DeserializeStdin(WwanPrioritySetRequest {
                priority: WwanPriority::Preferred,
            }),
        )
        .await
        .unwrap();
        assert_eq!(recreated().await, Some(BACKUP_METRIC));
    }

    #[test]
    fn uplink_on_a_served_address_range_is_a_conflict() {
        let dump = |wwan: &str, mask: u8| {
            serde_json::json!({ "interface": [
                { "interface": "loopback", "ipv4-address": [{ "address": "127.0.0.1", "mask": 8 }] },
                { "interface": "lan", "ipv4-address": [{ "address": "192.168.0.1", "mask": 24 }] },
                { "interface": "vlan_guest", "ipv4-address": [{ "address": "192.168.101.1", "mask": 24 }] },
                { "interface": "wan", "ipv4-address": [{ "address": "192.168.0.9", "mask": 24 }] },
                { "interface": "wwan", "ipv4-address": [{ "address": wwan, "mask": mask }] },
            ]})
        };
        assert!(subnet_conflict(&dump("192.168.0.57", 24)));
        assert!(subnet_conflict(&dump("192.168.101.4", 24)));
        assert!(subnet_conflict(&dump("192.168.3.4", 16)));
        assert!(!subnet_conflict(&dump("192.168.1.57", 24)));
        assert!(!subnet_conflict(&dump("10.0.0.7", 8)));
        assert!(!subnet_conflict(&serde_json::json!({ "interface": [
            { "interface": "lan", "ipv4-address": [{ "address": "192.168.0.1", "mask": 24 }] },
            { "interface": "wan", "ipv4-address": [{ "address": "192.168.0.9", "mask": 24 }] },
        ]})));
    }

    #[tokio::test]
    async fn uplink_radios_include_luci_uplinks() {
        let dir = tempfile::tempdir().unwrap();
        let disabled =
            LUCI_UPLINK.replace("upstreampass'\n", "upstreampass'\n\toption disabled '1'\n");
        for (extra, expected) in [
            ("", vec![]),
            (LUCI_UPLINK, vec!["radio0"]),
            (disabled.as_str(), vec![]),
        ] {
            setup(dir.path(), &format!("{WIRELESS}{extra}"), "");
            let arena = Arena::new();
            let cfgs = parse_all(dir.path().to_path_buf(), &arena, &["wireless"])
                .await
                .unwrap();
            let radios: Vec<String> = uplink_radios(&cfgs).unwrap().into_iter().collect();
            assert_eq!(radios, expected);
        }
    }

    fn scanned(ssid: &str, signal: i32, encryption: serde_json::Value) -> ScanResult {
        serde_json::from_value(serde_json::json!({
            "ssid": ssid,
            "bssid": format!("02:00:00:00:00:{:02x}", signal.unsigned_abs()),
            "channel": 6,
            "signal": signal,
            "encryption": encryption,
        }))
        .unwrap()
    }

    #[test]
    fn scan_security_follows_iwinfo_encryption() {
        let cases = [
            (
                serde_json::json!({ "enabled": false }),
                Some(WwanSecurity::Open),
            ),
            (
                serde_json::json!({ "enabled": true, "wpa": [2], "authentication": ["psk"], "ciphers": ["ccmp"] }),
                Some(WwanSecurity::Wpa2),
            ),
            (
                serde_json::json!({ "enabled": true, "wpa": [1, 2], "authentication": ["psk"] }),
                Some(WwanSecurity::Wpa2),
            ),
            (
                serde_json::json!({ "enabled": true, "wpa": [2, 3], "authentication": ["psk", "sae"] }),
                Some(WwanSecurity::Wpa2Wpa3),
            ),
            (
                serde_json::json!({ "enabled": true, "wpa": [3], "authentication": ["sae"] }),
                Some(WwanSecurity::Wpa3),
            ),
            (
                serde_json::json!({ "enabled": true, "wpa": [1], "authentication": ["psk"] }),
                None,
            ),
            (
                serde_json::json!({ "enabled": true, "wpa": [2], "authentication": ["802.1x"] }),
                None,
            ),
            (
                serde_json::json!({ "enabled": true, "wpa": [3], "authentication": ["owe"] }),
                None,
            ),
            (
                serde_json::json!({ "enabled": true, "wep": ["open"], "ciphers": ["wep-40"] }),
                None,
            ),
        ];
        for (encryption, expected) in cases {
            let result = scanned("Net", -50, encryption.clone());
            assert_eq!(result.encryption.security(), expected, "{encryption}");
        }
    }

    #[test]
    fn scan_keeps_strongest_entry_per_network() {
        let wpa2 = serde_json::json!({ "enabled": true, "wpa": [2], "authentication": ["psk"] });
        let networks = scan_networks(vec![
            scanned("Home", -70, wpa2.clone()),
            scanned("Cafe", -60, serde_json::json!({ "enabled": false })),
            scanned("Home", -40, wpa2.clone()),
            scanned("", -55, wpa2.clone()),
            scanned("", -80, wpa2),
        ]);
        let summary: Vec<(&str, Option<i32>)> = networks
            .iter()
            .map(|n| (n.ssid.as_str(), n.signal))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("Home", Some(-40)),
                ("", Some(-55)),
                ("Cafe", Some(-60)),
                ("", Some(-80)),
            ]
        );
    }

    #[test]
    fn security_round_trips_through_uci() {
        for security in [
            WwanSecurity::Open,
            WwanSecurity::Wpa2,
            WwanSecurity::Wpa2Wpa3,
            WwanSecurity::Wpa3,
        ] {
            assert_eq!(
                WwanSecurity::from_encryption(security.encryption()),
                Some(security)
            );
        }
        assert_eq!(
            WwanSecurity::from_encryption("psk2+ccmp"),
            Some(WwanSecurity::Wpa2)
        );
        assert_eq!(WwanSecurity::from_encryption("wpa2"), None);
    }

    async fn stop_with(dir: &Path, attempted: &Attempted, reason: WwanStopReason) -> bool {
        update(&TestContext(dir.to_path_buf()), |cfgs| {
            apply_stop(cfgs, attempted, reason)
        })
        .await
        .unwrap()
    }

    fn attempted(key: &str) -> Attempted {
        Attempted {
            device: "radio0".into(),
            ssid: "Upstream".into(),
            encryption: "psk2".into(),
            key: Some(key.into()),
        }
    }

    #[tokio::test]
    async fn stopped_uplink_reports_why_and_frees_its_radio() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = setup(dir.path(), WIRELESS, "");
        connect(
            ctx.clone(),
            DeserializeStdin(connect_req(WwanSecurity::Wpa2, Some("upstreampass"))),
        )
        .await
        .unwrap();
        assert_eq!(
            get(ctx.clone()).await.unwrap().uplink.unwrap().stopped,
            None
        );

        assert!(
            stop_with(
                dir.path(),
                &attempted("upstreampass"),
                WwanStopReason::Rejected
            )
            .await
        );
        assert!(section(dir.path(), "wireless", WWAN_INTERFACE)
            .unwrap()
            .contains("option disabled '1'"));
        assert!(section(dir.path(), "startwrt", STOP_SECTION)
            .unwrap()
            .contains("option reason 'rejected'"));
        let res = get(ctx).await.unwrap();
        assert_eq!(res.uplink.unwrap().stopped, Some(WwanStopReason::Rejected));
        let arena = Arena::new();
        let cfgs = parse_all(dir.path().to_path_buf(), &arena, &["wireless"])
            .await
            .unwrap();
        assert!(uplink_radios(&cfgs).unwrap().is_empty());
    }

    #[tokio::test]
    async fn uplink_turned_off_elsewhere_reads_as_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = setup(dir.path(), WIRELESS, "");
        connect(
            ctx.clone(),
            DeserializeStdin(connect_req(WwanSecurity::Wpa2, Some("upstreampass"))),
        )
        .await
        .unwrap();
        let wireless = read(dir.path(), "wireless").replace(
            "option key 'upstreampass'\n",
            "option key 'upstreampass'\n\toption disabled '1'\n",
        );
        std::fs::write(dir.path().join("wireless"), wireless).unwrap();
        assert_eq!(
            get(ctx).await.unwrap().uplink.unwrap().stopped,
            Some(WwanStopReason::Disabled)
        );
    }

    #[tokio::test]
    async fn stop_leaves_a_changed_or_stopped_uplink_alone() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = setup(dir.path(), WIRELESS, "");
        connect(
            ctx.clone(),
            DeserializeStdin(connect_req(WwanSecurity::Wpa2, Some("newpassword"))),
        )
        .await
        .unwrap();
        let before = read(dir.path(), "wireless");
        assert!(
            !stop_with(
                dir.path(),
                &attempted("upstreampass"),
                WwanStopReason::Rejected
            )
            .await
        );
        assert_eq!(read(dir.path(), "wireless"), before);

        assert!(
            stop_with(
                dir.path(),
                &attempted("newpassword"),
                WwanStopReason::Unreachable
            )
            .await
        );
        let stopped = read(dir.path(), "startwrt");
        assert!(
            !stop_with(
                dir.path(),
                &attempted("newpassword"),
                WwanStopReason::Rejected
            )
            .await
        );
        assert_eq!(read(dir.path(), "startwrt"), stopped);
    }

    #[tokio::test]
    async fn reconnect_clears_the_stop() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = setup(dir.path(), WIRELESS, "");
        connect(
            ctx.clone(),
            DeserializeStdin(connect_req(WwanSecurity::Wpa2, Some("upstreampass"))),
        )
        .await
        .unwrap();
        stop_with(
            dir.path(),
            &attempted("upstreampass"),
            WwanStopReason::Rejected,
        )
        .await;

        assert_eq!(
            reconnect(ctx.clone()).await.unwrap(),
            WwanJoinOutcome::Joining
        );
        assert!(!section(dir.path(), "wireless", WWAN_INTERFACE)
            .unwrap()
            .contains("option disabled '1'"));
        assert!(section(dir.path(), "startwrt", STOP_SECTION).is_none());
        assert_eq!(get(ctx).await.unwrap().uplink.unwrap().stopped, None);
    }

    #[tokio::test]
    async fn reconnect_needs_an_uplink_on_a_radio_that_is_on() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = setup(dir.path(), WIRELESS, "");
        let err = reconnect(ctx.clone()).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);

        connect(
            ctx.clone(),
            DeserializeStdin(connect_req(WwanSecurity::Wpa2, Some("upstreampass"))),
        )
        .await
        .unwrap();
        stop_with(
            dir.path(),
            &attempted("upstreampass"),
            WwanStopReason::Rejected,
        )
        .await;
        let wireless = read(dir.path(), "wireless").replacen(
            "\toption channel 'auto'\n",
            "\toption channel 'auto'\n\toption disabled '1'\n",
            1,
        );
        std::fs::write(dir.path().join("wireless"), &wireless).unwrap();
        let err = reconnect(ctx).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::WifiRadioUnavailable);
        assert_eq!(read(dir.path(), "wireless"), wireless);
    }

    #[tokio::test]
    async fn connect_and_disconnect_clear_the_stop() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = setup(dir.path(), WIRELESS, "");
        for leave in [true, false] {
            connect(
                ctx.clone(),
                DeserializeStdin(connect_req(WwanSecurity::Wpa2, Some("upstreampass"))),
            )
            .await
            .unwrap();
            stop_with(
                dir.path(),
                &attempted("upstreampass"),
                WwanStopReason::Rejected,
            )
            .await;
            if leave {
                disconnect(ctx.clone()).await.unwrap();
            } else {
                connect(
                    ctx.clone(),
                    DeserializeStdin(connect_req(WwanSecurity::Wpa2, Some("upstreampass"))),
                )
                .await
                .unwrap();
            }
            assert!(section(dir.path(), "startwrt", STOP_SECTION).is_none());
        }
    }

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    /// Seen \`ago\` before \`now\`.
    fn sighting(ago: u64, level: i32) -> Sighting {
        Sighting {
            age: secs(ago),
            level,
        }
    }

    /// A tracker past its first observation, and a time to count from.
    fn started() -> (Tracker, Instant) {
        let mut tracker = Tracker::default();
        tracker.idle();
        (tracker, Instant::now())
    }

    #[test]
    fn join_that_never_connects_stops_as_unreachable() {
        let (mut tracker, t0) = started();
        let uplink = attempted("upstreampass");
        assert_eq!(
            tracker.observe(uplink.clone(), t0, false, &[]),
            Verdict::Wait
        );
        assert_eq!(
            tracker.observe(uplink.clone(), t0 + secs(25), false, &[]),
            Verdict::Wait
        );
        assert_eq!(
            tracker.observe(uplink, t0 + JOIN_TIMEOUT, false, &[]),
            Verdict::Stop {
                reason: WwanStopReason::Unreachable,
                joining: true
            }
        );
    }

    #[test]
    fn join_refused_in_range_stops_as_rejected() {
        let (mut tracker, t0) = started();
        let uplink = attempted("upstreampass");
        tracker.observe(uplink.clone(), t0, false, &[]);
        tracker.observe(uplink.clone(), t0 + secs(15), false, &[sighting(2, -40)]);
        assert_eq!(
            tracker.observe(uplink, t0 + JOIN_TIMEOUT, false, &[sighting(17, -40)]),
            Verdict::Stop {
                reason: WwanStopReason::Rejected,
                joining: true
            }
        );
    }

    #[test]
    fn join_to_a_weak_network_stops_as_unreachable() {
        let (mut tracker, t0) = started();
        let uplink = attempted("upstreampass");
        tracker.observe(uplink.clone(), t0, false, &[]);
        assert_eq!(
            tracker.observe(uplink, t0 + JOIN_TIMEOUT, false, &[sighting(1, -82)]),
            Verdict::Stop {
                reason: WwanStopReason::Unreachable,
                joining: true
            }
        );
    }

    #[test]
    fn join_that_connects_is_reported_once() {
        let (mut tracker, t0) = started();
        let uplink = attempted("upstreampass");
        tracker.observe(uplink.clone(), t0, false, &[]);
        assert_eq!(
            tracker.observe(uplink.clone(), t0 + secs(10), true, &[]),
            Verdict::Joined
        );
        assert_eq!(
            tracker.observe(uplink.clone(), t0 + secs(15), true, &[]),
            Verdict::Wait
        );
        // Dropping later is no longer a join.
        assert_eq!(
            tracker.observe(uplink, t0 + secs(15) + JOIN_TIMEOUT, false, &[]),
            Verdict::Wait
        );
    }

    #[test]
    fn uplink_found_at_startup_is_not_a_join() {
        let mut tracker = Tracker::default();
        let t0 = Instant::now();
        let uplink = attempted("upstreampass");
        tracker.observe(uplink.clone(), t0, false, &[]);
        assert_eq!(
            tracker.observe(uplink.clone(), t0 + JOIN_TIMEOUT, false, &[]),
            Verdict::Wait
        );
        assert_eq!(
            tracker.observe(uplink, t0 + secs(600), false, &[]),
            Verdict::Wait
        );
    }

    #[test]
    fn network_refusing_in_range_stops_after_two_minutes() {
        let mut tracker = Tracker::default();
        let t0 = Instant::now();
        let uplink = attempted("upstreampass");
        tracker.observe(uplink.clone(), t0, true, &[]);
        // Refused from here on: seen on every retry.
        tracker.observe(uplink.clone(), t0 + secs(5), false, &[sighting(1, -50)]);
        assert_eq!(
            tracker.observe(uplink.clone(), t0 + secs(100), false, &[sighting(30, -50)]),
            Verdict::Wait
        );
        assert_eq!(
            tracker.observe(
                uplink,
                t0 + secs(5) + REFUSED_TIMEOUT,
                false,
                &[sighting(60, -50)]
            ),
            Verdict::Stop {
                reason: WwanStopReason::Rejected,
                joining: false
            }
        );
    }

    #[test]
    fn network_gone_since_the_drop_keeps_retrying() {
        let mut tracker = Tracker::default();
        let t0 = Instant::now();
        let uplink = attempted("upstreampass");
        tracker.observe(uplink.clone(), t0, true, &[]);
        // Last seen before the drop.
        for t in [10, 60, 125, 400] {
            assert_eq!(
                tracker.observe(uplink.clone(), t0 + secs(t), false, &[sighting(t + 3, -40)]),
                Verdict::Wait
            );
        }
    }

    #[test]
    fn network_returning_after_an_outage_has_time_to_authenticate() {
        let mut tracker = Tracker::default();
        let t0 = Instant::now();
        let uplink = attempted("upstreampass");
        tracker.observe(uplink.clone(), t0, true, &[]);
        for t in [5, 60, 120, 300] {
            assert_eq!(
                tracker.observe(uplink.clone(), t0 + secs(t), false, &[]),
                Verdict::Wait
            );
        }
        assert_eq!(
            tracker.observe(uplink.clone(), t0 + secs(305), false, &[sighting(1, -50)]),
            Verdict::Wait
        );
        assert_eq!(
            tracker.observe(uplink, t0 + secs(310), true, &[]),
            Verdict::Wait
        );
    }

    #[test]
    fn losing_sight_of_the_network_resets_the_refusal_timeout() {
        let mut tracker = Tracker::default();
        let t0 = Instant::now();
        let uplink = attempted("upstreampass");
        tracker.observe(uplink.clone(), t0, true, &[]);
        tracker.observe(uplink.clone(), t0 + secs(5), false, &[sighting(1, -50)]);
        assert_eq!(
            tracker.observe(uplink.clone(), t0 + secs(100), false, &[]),
            Verdict::Wait
        );
        for t in [300, 360, 419] {
            assert_eq!(
                tracker.observe(uplink.clone(), t0 + secs(t), false, &[sighting(1, -50)]),
                Verdict::Wait
            );
        }
        assert_eq!(
            tracker.observe(uplink, t0 + secs(420), false, &[sighting(1, -50)]),
            Verdict::Stop {
                reason: WwanStopReason::Rejected,
                joining: false
            }
        );
    }

    #[test]
    fn network_last_seen_long_ago_keeps_retrying() {
        let mut tracker = Tracker::default();
        let t0 = Instant::now();
        let uplink = attempted("upstreampass");
        tracker.observe(uplink.clone(), t0, true, &[]);
        tracker.observe(uplink.clone(), t0 + secs(10), false, &[sighting(1, -40)]);
        assert_eq!(
            tracker.observe(uplink.clone(), t0 + secs(130), false, &[]),
            Verdict::Wait
        );
        assert_eq!(
            tracker.observe(uplink, t0 + secs(300), false, &[]),
            Verdict::Wait
        );
    }

    #[test]
    fn weak_network_keeps_retrying() {
        let mut tracker = Tracker::default();
        let t0 = Instant::now();
        let uplink = attempted("upstreampass");
        tracker.observe(uplink.clone(), t0, true, &[]);
        for t in [10, 60, 125, 400] {
            assert_eq!(
                tracker.observe(uplink.clone(), t0 + secs(t), false, &[sighting(1, -80)]),
                Verdict::Wait
            );
        }
    }

    #[test]
    fn changed_uplink_is_a_new_join() {
        let mut tracker = Tracker::default();
        let t0 = Instant::now();
        tracker.observe(attempted("upstreampass"), t0, true, &[]);
        let changed = attempted("newpassword");
        tracker.observe(changed.clone(), t0 + secs(5), false, &[]);
        assert_eq!(
            tracker.observe(changed, t0 + secs(5) + JOIN_TIMEOUT, false, &[]),
            Verdict::Stop {
                reason: WwanStopReason::Unreachable,
                joining: true
            }
        );
    }

    #[test]
    fn ssid_text_matches_wpa_supplicant() {
        assert_eq!(wpa_ssid_text("Home"), "Home");
        assert_eq!(
            wpa_ssid_text("Café \"A\\B\"\t"),
            "Caf\\xc3\\xa9 \\\"A\\\\B\\\"\\t"
        );
    }

    #[test]
    fn bss_pages_yield_sightings_of_the_network() {
        let page = "id=3\nlevel=-48\nage=4\nssid=Upstream\n====\nid=5\nlevel=-70\nage=12\nssid=Neighbor\n====\nid=9\nlevel=-61\nage=0\nssid=Upstream\n####";
        let (seen, last, complete) = parse_bss_page(page, "Upstream");
        assert_eq!(seen, vec![sighting(4, -48), sighting(0, -61)]);
        assert_eq!(last, Some(9));
        assert!(complete);

        let truncated = "id=3\nlevel=-48\nage=4\nssid=Caf\\xc3\\xa9\n====\n";
        let (seen, last, complete) = parse_bss_page(truncated, "Café");
        assert_eq!(seen, vec![sighting(4, -48)]);
        assert_eq!(last, Some(3));
        assert!(!complete);

        assert_eq!(parse_bss_page("", "Upstream"), (Vec::new(), None, false));
    }
}
