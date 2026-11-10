//! Wireless WAN: the router joins another Wi-Fi network as an uplink.
//!
//! StartWRT owns the `wwan` sections of `wireless` and `network`. The uplink
//! interface joins the `wan` firewall zone, so profiles that route through the
//! WAN, published ports and remote access treat it as the WAN. netifd picks the
//! uplink with the lower route metric while both have a default route.

use std::collections::{BTreeMap, BTreeSet};

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

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct WwanUplink {
    pub radio: String,
    pub band: String,
    pub ssid: String,
    pub security: WwanSecurity,
    pub priority: WwanPriority,
}
rpc_toolkit::reflect_ts!(WwanUplink);

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

fn priority_of(cfgs: &Configs) -> Result<WwanPriority, Error> {
    let wan = interface_metric(cfgs, WAN_INTERFACE)?.unwrap_or(0);
    let wwan = interface_metric(cfgs, WWAN_INTERFACE)?.unwrap_or(0);
    Ok(if wwan < wan {
        WwanPriority::Preferred
    } else {
        WwanPriority::Backup
    })
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

async fn read_state() -> WwanState {
    let mut state = WwanState::default();
    if let Some(status) = ubus_call("network.interface.wwan", "status", None).await {
        state.address = status
            .pointer("/ipv4-address/0/address")
            .and_then(|v| v.as_str())
            .map(String::from);
    }
    let radios = wireless_status().await.unwrap_or_default();
    if let Some(ifname) = ifname_of(&radios, WWAN_INTERFACE) {
        if let Some(info) = ubus_call(
            "iwinfo",
            "info",
            Some(serde_json::json!({ "device": ifname })),
        )
        .await
        {
            state.associated = info
                .get("bssid")
                .and_then(|v| v.as_str())
                .is_some_and(|b| !b.is_empty() && b != "00:00:00:00:00:00");
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
    if req.security != WwanSecurity::Open {
        let len = req.password.as_deref().map_or(0, str::len);
        if !(crate::wifi::MIN_PSK_LEN..=crate::wifi::MAX_PSK_LEN).contains(&len) {
            return Err(Error::new(
                eyre!(
                    "password must be {} to {} characters",
                    crate::wifi::MIN_PSK_LEN,
                    crate::wifi::MAX_PSK_LEN
                ),
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

/// Re-reads, re-applies and writes on a config conflict, then logs the outcome.
async fn write<C: CtrlContext>(
    ctx: &C,
    action: &str,
    summary: &str,
    apply: impl Fn(&mut Configs) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["wireless", "network", "firewall", "startwrt"],
        )
        .await?;
        if let Err(err) = apply(&mut cfgs) {
            crate::activity::log("wwan", action, false, summary, Some(&err.to_string()));
            return Err(err);
        }
        let dump_result = dump_all(ctx.uci_root(), cfgs).await;
        drop(arena);
        match dump_result {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
            }
            Err(err) => {
                crate::activity::log("wwan", action, false, summary, Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                crate::activity::log("wwan", action, true, summary, None);
                if ctx.effectful() {
                    profiles::reload_system().await?;
                }
                return Ok(());
            }
        }
    }
}

// ── Handlers ────────────────────────────────────────────────

#[instrument(skip_all)]
pub async fn get<C: CtrlContext>(ctx: C) -> Result<WwanResponse, Error> {
    let mut res = {
        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["wireless", "network"]).await?;
        get_config(&cfgs)?
    };
    if ctx.effectful() && res.uplink.is_some() {
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
) -> Result<(), Error> {
    write(
        &ctx,
        "connected",
        &format!("Joined Wi-Fi uplink {}", req.ssid),
        |cfgs| apply_connect(cfgs, &req),
    )
    .await
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
}
