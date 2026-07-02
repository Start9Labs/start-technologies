use crate::dns::{self, DnsServer};
use crate::profiles;
use crate::system::{get_wan_ipv6s, has_global_ipv6};
use crate::utils::DeserializeStdin;
use crate::utils::HandlerExtSerde;
use crate::CtrlContext;
use crate::prelude::*;
use rpc_toolkit::{from_fn_async_local, ParentHandler};
use serde::{Deserialize, Serialize};
use crate::invoke::Invoke;
use uciedit::openwrt::{DdnsService, InterfaceProto, NetworkDevice, NetworkInterface, UciSystemDns};
use uciedit::{dump_all, parse_all, Arena};

/// Returns the serde-serialized string representation of an enum variant (e.g. "dhcp", "6rd").
fn serde_name(v: &impl Serialize) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

pub const WAN_INTERFACE: &str = "wan";
pub const WAN6_INTERFACE: &str = "wan6";
const DDNS_SECTION: &str = "wan";

// ── IPv4 types ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WanIpv4Mode {
    Dhcp,
    Static,
    Pppoe,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WanIpv4Response {
    pub mode: WanIpv4Mode,
    pub assigned_ip: Option<String>,
    pub address: Option<String>,
    pub netmask: Option<String>,
    pub gateway: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub device: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WanIpv4SetRequest {
    pub mode: WanIpv4Mode,
    pub address: Option<String>,
    pub netmask: Option<String>,
    pub gateway: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub device: Option<String>,
}

// ── IPv6 types ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WanIpv6Mode {
    Disabled,
    Slaac,
    #[serde(rename = "dhcpv6")]
    Dhcpv6,
    Static,
    #[serde(rename = "6rd")]
    SixRd,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WanIpv6Response {
    pub mode: WanIpv6Mode,
    pub address: Option<String>,
    pub prefix: Option<String>,
    pub gateway: Option<String>,
    pub ip6prefix: Option<String>,
    pub ip6prefixlen: Option<String>,
    pub ip4prefixlen: Option<String>,
    pub border_relay: Option<String>,
    pub assigned_ipv6: Option<String>,
    /// Static mode: LAN prefix pool for sub-delegation, e.g. "2001:db8:abcd::/48"
    pub lan_prefix: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WanIpv6SetRequest {
    pub mode: WanIpv6Mode,
    pub address: Option<String>,
    pub prefix: Option<String>,
    pub gateway: Option<String>,
    pub ip6prefix: Option<String>,
    pub ip6prefixlen: Option<String>,
    pub ip4prefixlen: Option<String>,
    pub border_relay: Option<String>,
    /// Static mode: LAN prefix pool for sub-delegation, e.g. "2001:db8:abcd::/48"
    pub lan_prefix: Option<String>,
}

// ── MAC types ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MacStrategy {
    Router,
    Custom,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WanMacResponse {
    pub strategy: MacStrategy,
    pub mac: String,
    pub default_mac: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WanMacSetRequest {
    pub strategy: MacStrategy,
    pub mac: Option<String>,
}

// ── DNS types ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DnsMode {
    Isp,
    Custom,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WanDnsResponse {
    pub mode: DnsMode,
    pub servers: Vec<DnsServer>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WanDnsSetRequest {
    pub mode: DnsMode,
    pub servers: Option<Vec<DnsServer>>,
}

// ── DDNS types ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DdnsProvider {
    Start9,
    Dyndns,
    Noip,
    Cloudflare,
    Duckdns,
    Freedns,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WanDdnsResponse {
    pub enabled: bool,
    pub provider: DdnsProvider,
    pub hostname: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub token: Option<String>,
    pub zone: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WanDdnsSetRequest {
    pub enabled: bool,
    pub provider: DdnsProvider,
    pub hostname: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub token: Option<String>,
    pub zone: Option<String>,
}

// ── Provider mapping ────────────────────────────────────────

fn provider_to_service(p: &DdnsProvider) -> &'static str {
    match p {
        DdnsProvider::Start9 => "start9",
        DdnsProvider::Dyndns => "dyndns.org",
        DdnsProvider::Noip => "no-ip.com",
        DdnsProvider::Cloudflare => "cloudflare.com-v4",
        DdnsProvider::Duckdns => "duckdns.org",
        DdnsProvider::Freedns => "freedns.afraid.org",
    }
}

fn service_to_provider(s: &str) -> DdnsProvider {
    match s {
        "dyndns.org" => DdnsProvider::Dyndns,
        "no-ip.com" => DdnsProvider::Noip,
        "cloudflare.com-v4" => DdnsProvider::Cloudflare,
        "duckdns.org" => DdnsProvider::Duckdns,
        "freedns.afraid.org" => DdnsProvider::Freedns,
        _ => DdnsProvider::Start9,
    }
}

// ── Route registration ──────────────────────────────────────

pub fn wan<C: CtrlContext + Clone>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("ipv4-get", from_fn_async_local(ipv4_get::<C>).with_display_serializable())
        .subcommand("ipv4-set", from_fn_async_local(ipv4_set::<C>).with_display_serializable())
        .subcommand("ipv6-get", from_fn_async_local(ipv6_get::<C>).with_display_serializable())
        .subcommand("ipv6-set", from_fn_async_local(ipv6_set::<C>).with_display_serializable())
        .subcommand("mac-get", from_fn_async_local(mac_get::<C>).with_display_serializable())
        .subcommand("mac-set", from_fn_async_local(mac_set::<C>).with_display_serializable())
        .subcommand("dns-get", from_fn_async_local(dns_get::<C>).with_display_serializable())
        .subcommand("dns-set", from_fn_async_local(dns_set::<C>).with_display_serializable())
        .subcommand("ddns-get", from_fn_async_local(ddns_get::<C>).with_display_serializable())
        .subcommand("ddns-set", from_fn_async_local(ddns_set::<C>).with_display_serializable())
}

// ── Helpers ─────────────────────────────────────────────────

async fn get_assigned_wan_ip() -> Option<String> {
    let output = tokio::process::Command::new("ubus")
        .args(["call", "network.interface.wan", "status"])
        .invoke(ErrorKind::Network.into())
        .await
        .ok()?;
    let json: serde_json::Value = serde_json::from_slice(&output).ok()?;
    json.pointer("/ipv4-address/0/address")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

async fn get_default_mac(dev_name: &str) -> String {
    // Read the hardware MAC from the WAN device via ip link
    let output = tokio::process::Command::new("ip")
        .args(["link", "show", dev_name])
        .invoke(ErrorKind::Network.into())
        .await
        .ok();
    if let Some(out) = output {
        let stdout = String::from_utf8_lossy(&out);
        for line in stdout.lines() {
            if let Some(mac_line) = line.strip_prefix("    link/ether ") {
                if let Some(mac) = mac_line.split_whitespace().next() {
                    return mac.to_uppercase();
                }
            }
        }
    }
    "00:00:00:00:00:00".to_string()
}

async fn get_start9_hostname() -> Option<String> {
    tokio::fs::read_to_string("/etc/start9/hostname")
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

async fn restart_network() {
    let _ = crate::run_quiet_async(
        tokio::process::Command::new("/etc/init.d/network").arg("restart"),
    )
    .await;
}

// ── IPv4 handlers ───────────────────────────────────────────

#[instrument(skip_all)]
pub async fn ipv4_get<C: CtrlContext>(ctx: C) -> Result<WanIpv4Response, Error> {
    let effectful = ctx.effectful();
    let result = {
        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["network"]).await?;

        let mut found = None;
        for section in &cfgs["network"].sections {
            if section.name().as_deref() == Some(WAN_INTERFACE) {
                if let Some(iface) = section.get_typed::<NetworkInterface>()? {
                    let mode = match iface.proto {
                        InterfaceProto::STATIC => WanIpv4Mode::Static,
                        InterfaceProto::PPPOE => WanIpv4Mode::Pppoe,
                        _ => WanIpv4Mode::Dhcp,
                    };
                    found = Some((mode, iface));
                    break;
                }
            }
        }
        found
    };

    match result {
        Some((mode, iface)) => {
            let assigned_ip = if effectful {
                get_assigned_wan_ip().await
            } else {
                None
            };
            Ok(WanIpv4Response {
                mode,
                assigned_ip,
                address: iface.ipaddr.map(|ip| ip.to_string()),
                netmask: iface.netmask.map(|m| m.to_string()),
                gateway: iface.gateway,
                username: iface.username,
                password: iface.password,
                device: Some(iface.device),
            })
        }
        None => Err(Error::new(eyre!("WAN interface not found"), ErrorKind::MissingWanInterface)),
    }
}

#[instrument(skip_all)]
pub async fn ipv4_set<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<WanIpv4SetRequest>,
) -> Result<(), Error> {
    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["network"]).await?;

        let mut found = false;
        for section in &mut cfgs["network"].sections {
            if section.name().as_deref() == Some(WAN_INTERFACE) {
                if let Some(mut iface) = section.get_typed::<NetworkInterface>()? {
                    match req.mode {
                        WanIpv4Mode::Dhcp => {
                            iface.proto = InterfaceProto::DHCP;
                            iface.ipaddr = None;
                            iface.netmask = None;
                            iface.gateway = None;
                            iface.username = None;
                            iface.password = None;
                        }
                        WanIpv4Mode::Static => {
                            iface.proto = InterfaceProto::STATIC;
                            iface.ipaddr = req.address.as_ref().and_then(|a| a.parse().ok());
                            iface.netmask = req.netmask.as_ref().and_then(|n| n.parse().ok());
                            iface.gateway = req.gateway.clone();
                            iface.username = None;
                            iface.password = None;
                            // If no custom DNS is configured, use the gateway as DNS
                            // so dnsmasq has an upstream to forward to
                            if iface.peerdns.as_deref() != Some("0") {
                                if let Some(gw) = &iface.gateway {
                                    iface.dns = vec![gw.clone()];
                                    iface.peerdns = Some("0".to_string());
                                }
                            }
                        }
                        WanIpv4Mode::Pppoe => {
                            iface.proto = InterfaceProto::PPPOE;
                            iface.username = req.username.clone();
                            iface.password = req.password.clone();
                            if let Some(dev) = &req.device {
                                iface.device = dev.clone();
                            }
                            iface.ipaddr = None;
                            iface.netmask = None;
                            iface.gateway = None;
                        }
                    }
                    section.set(&iface)?;
                    found = true;
                    break;
                }
            }
        }
        if !found {
            return Err(Error::new(eyre!("WAN interface not found"), ErrorKind::MissingWanInterface));
        }

        let dump_result = dump_all(ctx.uci_root(), cfgs).await;
        drop(arena);
        match dump_result {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log("wan", "ipv4-updated", false, "Failed to update WAN IPv4", Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                crate::activity::log("wan", "ipv4-updated", true, &format!("Updated WAN IPv4 (mode: {})", serde_name(&req.mode)), None);
                if ctx.effectful() {
                    restart_network().await;
                }
                return Ok(());
            }
        }
    }
}

// ── IPv6 handlers ───────────────────────────────────────────

#[instrument(skip_all)]
pub async fn ipv6_get<C: CtrlContext>(ctx: C) -> Result<WanIpv6Response, Error> {
    let arena = Arena::new();
    let cfgs = parse_all(ctx.uci_root(), &arena, &["network"]).await?;

    let mut wan6: Option<NetworkInterface> = None;
    for section in &cfgs["network"].sections {
        if section.name().as_deref() == Some(WAN6_INTERFACE) {
            if let Some(iface) = section.get_typed::<NetworkInterface>()? {
                wan6 = Some(iface);
                break;
            }
        }
    }

    let Some(iface) = wan6 else {
        return Ok(WanIpv6Response {
            mode: WanIpv6Mode::Disabled,
            address: None,
            prefix: None,
            gateway: None,
            ip6prefix: None,
            ip6prefixlen: None,
            ip4prefixlen: None,
            border_relay: None,
            assigned_ipv6: None,
            lan_prefix: None,
        });
    };

    let mode = match iface.proto {
        InterfaceProto::NONE => WanIpv6Mode::Disabled,
        InterfaceProto::DHCPV6 => {
            if iface.reqaddress.as_deref() == Some("force") {
                WanIpv6Mode::Dhcpv6
            } else {
                WanIpv6Mode::Slaac
            }
        }
        InterfaceProto::STATIC => WanIpv6Mode::Static,
        InterfaceProto::SIXRD => WanIpv6Mode::SixRd,
        _ => WanIpv6Mode::Disabled,
    };

    let (address, prefix) = match mode {
        WanIpv6Mode::Slaac | WanIpv6Mode::Dhcpv6 => {
            let prefix = iface.reqprefix
                .filter(|p| p != "auto" && !p.is_empty())
                .map(|p| format!("/{}", p));
            (None, prefix)
        }
        _ => match &iface.ip6addr {
            Some(ip6addr) if !ip6addr.is_empty() => {
                let parts: Vec<&str> = ip6addr.splitn(2, '/').collect();
                (
                    Some(parts[0].to_string()),
                    parts.get(1).map(|p| format!("/{}", p)),
                )
            }
            _ => (None, None),
        },
    };

    // Drop arena before any .await — Arena is !Send
    drop(cfgs);
    drop(arena);

    let assigned_ipv6 = if ctx.effectful() {
        let addrs = get_wan_ipv6s().await.unwrap_or_default();
        // Prefer a Global Unicast Address: it's the only WAN scope that can back
        // inbound/published-port forwarding, and every consumer (the
        // published-ports IPv6 gate, the inbound endpoint list, the WAN IPv6
        // page) wants the reachable address. Fall back to the first address so a
        // ULA-only WAN still reports something truthful.
        addrs
            .iter()
            .copied()
            .find(|a| has_global_ipv6(std::slice::from_ref(a)))
            .or_else(|| addrs.first().copied())
            .map(|a| a.to_string())
    } else {
        None
    };

    // For Static mode, ip6prefix is the LAN delegation prefix.
    // For 6RD, ip6prefix is the tunnel parameter.
    let lan_prefix = if mode == WanIpv6Mode::Static {
        iface.ip6prefix.clone()
    } else {
        None
    };
    let sixrd_ip6prefix = if mode == WanIpv6Mode::SixRd {
        iface.ip6prefix
    } else {
        None
    };

    Ok(WanIpv6Response {
        mode,
        address,
        prefix,
        gateway: iface.ip6gw,
        ip6prefix: sixrd_ip6prefix,
        ip6prefixlen: iface.ip6prefixlen.map(|p| format!("/{}", p)),
        ip4prefixlen: iface.ip4prefixlen.map(|p| format!("/{}", p)),
        border_relay: iface.peeraddr,
        assigned_ipv6,
        lan_prefix,
    })
}

#[instrument(skip_all)]
pub async fn ipv6_set<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<WanIpv6SetRequest>,
) -> Result<(), Error> {
    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["network"]).await?;

        let wan6_idx = cfgs["network"]
            .sections
            .iter()
            .position(|s| s.name().as_deref() == Some(WAN6_INTERFACE));

        if req.mode == WanIpv6Mode::Disabled {
            if let Some(idx) = wan6_idx {
                cfgs["network"].sections.remove(idx);
            }
        } else {
            let reqprefix = req.prefix.as_ref()
                .map(|p| p.trim_start_matches('/').to_string())
                .filter(|p| !p.is_empty());

            let new_iface = match &req.mode {
                WanIpv6Mode::Slaac => NetworkInterface {
                    device: "@wan".to_string(),
                    proto: InterfaceProto::DHCPV6,
                    reqaddress: Some("try".to_string()),
                    reqprefix: Some(reqprefix.unwrap_or_else(|| "auto".to_string())),
                    ..Default::default()
                },
                WanIpv6Mode::Dhcpv6 => NetworkInterface {
                    device: "@wan".to_string(),
                    proto: InterfaceProto::DHCPV6,
                    reqaddress: Some("force".to_string()),
                    reqprefix: Some(reqprefix.unwrap_or_else(|| "auto".to_string())),
                    ..Default::default()
                },
                WanIpv6Mode::Static => {
                    let ip6addr = match (&req.address, &req.prefix) {
                        (Some(addr), Some(pfx)) => Some(format!("{}{}", addr, pfx)),
                        (Some(addr), None) => Some(addr.clone()),
                        _ => None,
                    };
                    // Parse lan_prefix "addr/len" into ip6prefix for odhcpd distribution
                    let ip6prefix = req.lan_prefix.clone();
                    NetworkInterface {
                        device: "@wan".to_string(),
                        proto: InterfaceProto::STATIC,
                        ip6addr,
                        ip6gw: req.gateway.clone(),
                        ip6prefix,
                        ..Default::default()
                    }
                }
                WanIpv6Mode::SixRd => NetworkInterface {
                    device: "@wan".to_string(),
                    proto: InterfaceProto::SIXRD,
                    peeraddr: req.border_relay.clone(),
                    ip6prefix: req.ip6prefix.clone(),
                    ip6prefixlen: req.ip6prefixlen.as_ref().map(|p| p.trim_start_matches('/').to_string()),
                    ip4prefixlen: req.ip4prefixlen.as_ref().map(|p| p.trim_start_matches('/').to_string()),
                    ..Default::default()
                },
                WanIpv6Mode::Disabled => unreachable!(),
            };

            if let Some(idx) = wan6_idx {
                let section = &mut cfgs["network"].sections[idx];
                let mut updated = new_iface;
                if let Some(existing) = section.get_typed::<NetworkInterface>()? {
                    updated.device = existing.device;
                    updated.dns = existing.dns;
                    updated.peerdns = existing.peerdns;
                    updated.macaddr = existing.macaddr;
                    updated.ip6assign = existing.ip6assign;
                }
                section.set(&updated)?;
            } else {
                cfgs["network"].append(&new_iface, Some(WAN6_INTERFACE))?;
            }
        }

        let dump_result = dump_all(ctx.uci_root(), cfgs).await;
        drop(arena);
        match dump_result {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log("wan", "ipv6-updated", false, "Failed to update WAN IPv6", Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                crate::activity::log("wan", "ipv6-updated", true, &format!("Updated WAN IPv6 (mode: {})", serde_name(&req.mode)), None);
                if ctx.effectful() {
                    restart_network().await;
                    let _ = crate::run_quiet_async(
                        tokio::process::Command::new("/etc/init.d/odhcpd").arg("restart"),
                    )
                    .await;
                }
                return Ok(());
            }
        }
    }
}

// ── MAC handlers ────────────────────────────────────────────

#[instrument(skip_all)]
pub async fn mac_get<C: CtrlContext>(ctx: C) -> Result<WanMacResponse, Error> {
    let arena = Arena::new();
    let cfgs = parse_all(ctx.uci_root(), &arena, &["network"]).await?;

    let mut wan_device_name: Option<String> = None;
    let mut device_macaddr: Option<String> = None;

    for section in &cfgs["network"].sections {
        if section.name().as_deref() == Some(WAN_INTERFACE) {
            if let Some(iface) = section.get_typed::<NetworkInterface>()? {
                wan_device_name = Some(iface.device.clone());
            }
        }
    }

    // Look up the device section for its MAC override
    if let Some(ref dev_name) = wan_device_name {
        for section in &cfgs["network"].sections {
            if let Some(device) = section.get_typed::<NetworkDevice>().ok().flatten() {
                if device.name == *dev_name {
                    device_macaddr = device.macaddr;
                    break;
                }
            }
        }
    }

    // Drop arena before any .await — Arena is !Send
    drop(cfgs);
    drop(arena);

    let default_mac = if ctx.effectful() {
        get_default_mac(wan_device_name.as_deref().unwrap_or("eth1")).await
    } else {
        "00:00:00:00:00:00".to_string()
    };

    let has_custom = device_macaddr.is_some();
    let effective_mac = device_macaddr
        .unwrap_or_else(|| default_mac.clone());

    Ok(WanMacResponse {
        strategy: if has_custom { MacStrategy::Custom } else { MacStrategy::Router },
        mac: effective_mac,
        default_mac,
    })
}

#[instrument(skip_all)]
pub async fn mac_set<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<WanMacSetRequest>,
) -> Result<(), Error> {
    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["network"]).await?;

        // Find the WAN interface's device name
        let mut wan_device_name: Option<String> = None;
        for section in &cfgs["network"].sections {
            if section.name().as_deref() == Some(WAN_INTERFACE) {
                if let Some(iface) = section.get_typed::<NetworkInterface>()? {
                    wan_device_name = Some(iface.device.clone());
                }
            }
        }
        let dev_name =
            wan_device_name.ok_or_else(|| Error::new(eyre!("WAN interface not found"), ErrorKind::MissingWanInterface))?;

        // Set macaddr on the device section, creating it if needed
        let mut found = false;
        for section in &mut cfgs["network"].sections {
            if let Some(mut device) = section.get_typed::<NetworkDevice>().ok().flatten() {
                if device.name == dev_name {
                    match req.strategy {
                        MacStrategy::Custom => {
                            device.macaddr = req.mac.clone();
                        }
                        MacStrategy::Router => {
                            device.macaddr = None;
                        }
                    }
                    section.set(&device)?;
                    found = true;
                    break;
                }
            }
        }
        if !found {
            if req.strategy == MacStrategy::Custom {
                let device = NetworkDevice {
                    name: dev_name,
                    ty: None,
                    ports: vec![],
                    macaddr: req.mac.clone(),
                };
                cfgs["network"].append(&device, None)?;
            }
            // Router strategy with no device section = already using hardware default
        }

        let dump_result = dump_all(ctx.uci_root(), cfgs).await;
        drop(arena);
        match dump_result {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log("wan", "mac-updated", false, "Failed to update WAN MAC address", Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                crate::activity::log("wan", "mac-updated", true, "Updated WAN MAC address", None);
                if ctx.effectful() {
                    restart_network().await;
                }
                return Ok(());
            }
        }
    }
}

// ── DNS handlers ────────────────────────────────────────────

#[instrument(skip_all)]
pub async fn dns_get<C: CtrlContext>(ctx: C) -> Result<WanDnsResponse, Error> {
    let arena = Arena::new();
    let cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt"]).await?;

    let servers = dns::get_system_dns_servers(&cfgs);
    if !servers.is_empty() {
        Ok(WanDnsResponse {
            mode: DnsMode::Custom,
            servers,
        })
    } else {
        Ok(WanDnsResponse {
            mode: DnsMode::Isp,
            servers: vec![],
        })
    }
}

#[instrument(skip_all)]
pub async fn dns_set<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<WanDnsSetRequest>,
) -> Result<(), Error> {
    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "dhcp", "firewall"],
        ).await?;

        // Remove existing system_dns section
        cfgs["startwrt"]
            .sections
            .retain(|s| s.name().as_deref() != Some("system_dns"));

        match req.mode {
            DnsMode::Custom => {
                let servers = req.servers.clone().unwrap_or_default();
                cfgs["startwrt"].append(
                    &UciSystemDns {
                        servers: dns::serialize_dns_server_list(&servers),
                    },
                    Some("system_dns"),
                )?;

                // Set peerdns=0 on WAN/WAN6 to prevent ISP DNS from appearing in resolvfile
                for section in &mut cfgs["network"].sections {
                    let name = section.name();
                    let n = name.as_deref();
                    if n == Some(WAN_INTERFACE) || n == Some(WAN6_INTERFACE) {
                        if let Some(mut iface) = section.get_typed::<NetworkInterface>()? {
                            iface.peerdns = Some("0".to_string());
                            iface.dns = vec![];
                            section.set(&iface)?;
                        }
                    }
                }
            }
            DnsMode::Isp => {
                // Restore peerdns=1 on WAN/WAN6
                for section in &mut cfgs["network"].sections {
                    let name = section.name();
                    let n = name.as_deref();
                    if n == Some(WAN_INTERFACE) || n == Some(WAN6_INTERFACE) {
                        if let Some(mut iface) = section.get_typed::<NetworkInterface>()? {
                            iface.peerdns = Some("1".to_string());
                            iface.dns = vec![];
                            section.set(&iface)?;
                        }
                    }
                }
            }
        }

        // Rewrite DNS forwarding (dnsmasq + firewall DNAT) for all profiles
        // so they pick up the new system DNS setting.
        profiles::rewrite_all_dns_forwarding(&mut cfgs)?;

        let dump_result = dump_all(ctx.uci_root(), cfgs).await;
        drop(arena);
        match dump_result {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log("wan", "dns-updated", false, "Failed to update WAN DNS", Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                crate::activity::log("wan", "dns-updated", true, &format!("Updated WAN DNS (mode: {})", serde_name(&req.mode)), None);
                if ctx.effectful() {
                    // Re-read configs to regenerate SmartDNS with the updated state
                    let smartdns_groups = {
                        let arena2 = Arena::new();
                        let cfgs2 = parse_all(ctx.uci_root(), &arena2, &["startwrt"]).await?;
                        dns::collect_smartdns_groups(&cfgs2)
                    };
                    dns::apply_smartdns_groups(smartdns_groups).await?;
                    // Use reload_system() which restarts network + smartdns + firewall + dnsmasq
                    profiles::reload_system().await?;
                }
                return Ok(());
            }
        }
    }
}

// ── DDNS handlers ───────────────────────────────────────────

#[instrument(skip_all)]
pub async fn ddns_get<C: CtrlContext>(ctx: C) -> Result<WanDdnsResponse, Error> {
    let arena = Arena::new();
    let cfgs = parse_all(ctx.uci_root(), &arena, &["ddns"]).await?;

    for section in &cfgs["ddns"].sections {
        if section.name().as_deref() == Some(DDNS_SECTION) {
            if let Some(svc) = section.get_typed::<DdnsService>()? {
                let provider = service_to_provider(svc.service_name.as_deref().unwrap_or("start9"));
                let enabled = svc.enabled.as_deref() == Some("1");

                let hostname = if provider == DdnsProvider::Start9 && enabled {
                    if ctx.effectful() {
                        get_start9_hostname().await
                    } else {
                        None
                    }
                } else {
                    svc.domain.or(svc.lookup_host)
                };

                // For token-based providers (Cloudflare, DuckDNS, FreeDNS),
                // the password field stores the token
                let (username, password, token) = match provider {
                    DdnsProvider::Cloudflare | DdnsProvider::Duckdns | DdnsProvider::Freedns => {
                        (None, None, svc.password)
                    }
                    _ => (svc.username, svc.password, None),
                };

                return Ok(WanDdnsResponse {
                    enabled,
                    provider,
                    hostname,
                    username,
                    password,
                    token,
                    zone: None, // Cloudflare zone stored differently in future
                });
            }
        }
    }

    Ok(WanDdnsResponse {
        enabled: false,
        provider: DdnsProvider::Start9,
        hostname: None,
        username: None,
        password: None,
        token: None,
        zone: None,
    })
}

#[instrument(skip_all)]
pub async fn ddns_set<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<WanDdnsSetRequest>,
) -> Result<(), Error> {
    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["ddns"]).await?;

        let ddns_idx = cfgs["ddns"]
            .sections
            .iter()
            .position(|s| s.name().as_deref() == Some(DDNS_SECTION));

        let new_svc = DdnsService {
            enabled: Some(if req.enabled { "1" } else { "0" }.to_string()),
            service_name: Some(provider_to_service(&req.provider).to_string()),
            ip_source: Some("network".to_string()),
            ip_network: Some("wan".to_string()),
            username: if req.enabled && req.provider != DdnsProvider::Start9 {
                req.username.clone()
            } else {
                None
            },
            password: if req.enabled && req.provider != DdnsProvider::Start9 {
                // Token-based providers use the password field
                req.token.clone().or(req.password.clone())
            } else {
                None
            },
            domain: if req.enabled && req.provider != DdnsProvider::Start9 {
                req.hostname.clone()
            } else {
                None
            },
            lookup_host: if req.enabled && req.provider != DdnsProvider::Start9 {
                req.hostname.clone()
            } else {
                None
            },
        };

        if let Some(idx) = ddns_idx {
            cfgs["ddns"].sections[idx].set(&new_svc)?;
        } else {
            cfgs["ddns"].append(&new_svc, Some(DDNS_SECTION))?;
        }

        let dump_result = dump_all(ctx.uci_root(), cfgs).await;
        drop(arena);
        match dump_result {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log("wan", "ddns-updated", false, "Failed to update DDNS", Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                crate::activity::log("wan", "ddns-updated", true, &format!("Updated DDNS (provider: {})", serde_name(&req.provider)), None);
                if ctx.effectful() {
                    let _ = crate::run_quiet_async(
                        tokio::process::Command::new("/etc/init.d/ddns")
                            .arg(if req.enabled { "restart" } else { "stop" }),
                    )
                    .await;
                }
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn setup_network(dir: &std::path::Path) {
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
    }

    fn setup_ddns(dir: &std::path::Path) {
        std::fs::write(
            dir.join("ddns"),
            "\
config service 'wan'
\toption enabled '1'
\toption service_name 'dyndns.org'
\toption username 'myuser'
\toption password 'mypass'
\toption domain 'myhost.dyndns.org'
\toption lookup_host 'myhost.dyndns.org'
\toption ip_source 'network'
\toption ip_network 'wan'
",
        )
        .unwrap();
    }

    // ── IPv4 ──

    #[tokio::test]
    async fn ipv4_get_dhcp() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let res = ipv4_get(ctx).await.unwrap();
        assert_eq!(res.mode, WanIpv4Mode::Dhcp);
        assert!(res.assigned_ip.is_none()); // effectful=false
    }

    #[tokio::test]
    async fn ipv4_set_static() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        ipv4_set(
            ctx.clone(),
            DeserializeStdin(WanIpv4SetRequest {
                mode: WanIpv4Mode::Static,
                address: Some("10.0.0.2".to_string()),
                netmask: Some("255.255.255.0".to_string()),
                gateway: Some("10.0.0.1".to_string()),
                username: None,
                password: None,
                device: None,
            }),
        ).await
        .unwrap();

        let res = ipv4_get(ctx).await.unwrap();
        assert_eq!(res.mode, WanIpv4Mode::Static);
        assert_eq!(res.address.as_deref(), Some("10.0.0.2"));
        assert_eq!(res.gateway.as_deref(), Some("10.0.0.1"));
    }

    #[tokio::test]
    async fn ipv4_set_pppoe() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        ipv4_set(
            ctx.clone(),
            DeserializeStdin(WanIpv4SetRequest {
                mode: WanIpv4Mode::Pppoe,
                address: None,
                netmask: None,
                gateway: None,
                username: Some("user@isp".to_string()),
                password: Some("secret".to_string()),
                device: None,
            }),
        ).await
        .unwrap();

        let res = ipv4_get(ctx).await.unwrap();
        assert_eq!(res.mode, WanIpv4Mode::Pppoe);
        assert_eq!(res.username.as_deref(), Some("user@isp"));
    }

    // ── IPv6 ──

    #[tokio::test]
    async fn ipv6_get_slaac() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let res = ipv6_get(ctx).await.unwrap();
        // Default wan6 proto=dhcpv6 with no reqaddress → SLAAC
        assert_eq!(res.mode, WanIpv6Mode::Slaac);
    }

    #[tokio::test]
    async fn ipv6_set_disabled() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        ipv6_set(
            ctx.clone(),
            DeserializeStdin(WanIpv6SetRequest {
                mode: WanIpv6Mode::Disabled,
                address: None,
                prefix: None,
                gateway: None,
                ip6prefix: None,
                ip6prefixlen: None,
                ip4prefixlen: None,
                border_relay: None,
                lan_prefix: None,
            }),
        ).await
        .unwrap();

        let res = ipv6_get(ctx).await.unwrap();
        assert_eq!(res.mode, WanIpv6Mode::Disabled);
    }

    // ── MAC ──

    #[tokio::test]
    async fn mac_get_router_default() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let res = mac_get(ctx).await.unwrap();
        assert_eq!(res.strategy, MacStrategy::Router);
    }

    #[tokio::test]
    async fn mac_set_custom() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        mac_set(
            ctx.clone(),
            DeserializeStdin(WanMacSetRequest {
                strategy: MacStrategy::Custom,
                mac: Some("AA:BB:CC:DD:EE:FF".to_string()),
            }),
        ).await
        .unwrap();

        let res = mac_get(ctx).await.unwrap();
        assert_eq!(res.strategy, MacStrategy::Custom);
        assert_eq!(res.mac, "AA:BB:CC:DD:EE:FF");
    }

    // ── DNS ──

    fn setup_startwrt(dir: &std::path::Path) {
        std::fs::write(
            dir.join("startwrt"),
            "\
config profile lan
\toption fullname 'Admin'
\toption interface 'lan'
\toption vlan_tag '1'
",
        )
        .unwrap();

        // dns_set now loads dhcp and firewall configs too
        if !dir.join("dhcp").exists() {
            std::fs::write(
                dir.join("dhcp"),
                "\
config dnsmasq
\toption domainneeded '1'
\toption localise_queries '1'
",
            )
            .unwrap();
        }
        if !dir.join("firewall").exists() {
            std::fs::write(
                dir.join("firewall"),
                "\
config zone
\toption name 'wan'
\tlist network 'wan'
\toption input 'REJECT'
\toption output 'ACCEPT'
\toption forward 'REJECT'

config zone
\toption name 'lan'
\tlist network 'lan'
\toption input 'ACCEPT'
\toption output 'ACCEPT'
\toption forward 'ACCEPT'
",
            )
            .unwrap();
        }
    }

    #[tokio::test]
    async fn dns_get_isp_default() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        setup_startwrt(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let res = dns_get(ctx).await.unwrap();
        assert_eq!(res.mode, DnsMode::Isp);
        assert!(res.servers.is_empty());
    }

    #[tokio::test]
    async fn dns_set_custom() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        setup_startwrt(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        dns_set(
            ctx.clone(),
            DeserializeStdin(WanDnsSetRequest {
                mode: DnsMode::Custom,
                servers: Some(vec![
                    DnsServer { address: "1.1.1.1".into(), ssl: false },
                    DnsServer { address: "8.8.8.8".into(), ssl: true },
                ]),
            }),
        ).await
        .unwrap();

        let res = dns_get(ctx).await.unwrap();
        assert_eq!(res.mode, DnsMode::Custom);
        assert_eq!(res.servers, vec![
            DnsServer { address: "1.1.1.1".into(), ssl: false },
            DnsServer { address: "8.8.8.8".into(), ssl: true },
        ]);
    }

    // ── DDNS ──

    #[tokio::test]
    async fn ddns_get_existing() {
        let dir = tempfile::tempdir().unwrap();
        setup_ddns(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let res = ddns_get(ctx).await.unwrap();
        assert!(res.enabled);
        assert_eq!(res.provider, DdnsProvider::Dyndns);
        assert_eq!(res.username.as_deref(), Some("myuser"));
        assert_eq!(res.hostname.as_deref(), Some("myhost.dyndns.org"));
    }

    #[tokio::test]
    async fn ddns_get_empty() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ddns"), "").unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        let res = ddns_get(ctx).await.unwrap();
        assert!(!res.enabled);
        assert_eq!(res.provider, DdnsProvider::Start9);
    }

    #[tokio::test]
    async fn ddns_set_cloudflare() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ddns"), "").unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        ddns_set(
            ctx.clone(),
            DeserializeStdin(WanDdnsSetRequest {
                enabled: true,
                provider: DdnsProvider::Cloudflare,
                hostname: Some("mysite.example.com".to_string()),
                username: None,
                password: None,
                token: Some("cf-api-token-123".to_string()),
                zone: None,
            }),
        ).await
        .unwrap();

        let res = ddns_get(ctx).await.unwrap();
        assert!(res.enabled);
        assert_eq!(res.provider, DdnsProvider::Cloudflare);
        assert_eq!(res.token.as_deref(), Some("cf-api-token-123"));
        assert_eq!(res.hostname.as_deref(), Some("mysite.example.com"));
    }

    #[tokio::test]
    async fn ipv4_static_auto_sets_gateway_as_wan_dns() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        setup_startwrt(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        ipv4_set(
            ctx.clone(),
            DeserializeStdin(WanIpv4SetRequest {
                mode: WanIpv4Mode::Static,
                address: Some("192.168.10.69".to_string()),
                netmask: Some("255.255.255.0".to_string()),
                gateway: Some("192.168.10.1".to_string()),
                username: None,
                password: None,
                device: None,
            }),
        ).await
        .unwrap();

        // ipv4_set sets peerdns=0 and dns=gateway on the WAN interface,
        // but system DNS (dns_get) reads from startwrt config
        let res = dns_get(ctx).await.unwrap();
        assert_eq!(res.mode, DnsMode::Isp);
    }

    #[tokio::test]
    async fn ipv4_static_preserves_custom_dns() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        setup_startwrt(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        // Set custom system DNS first
        dns_set(
            ctx.clone(),
            DeserializeStdin(WanDnsSetRequest {
                mode: DnsMode::Custom,
                servers: Some(vec![DnsServer { address: "1.1.1.1".into(), ssl: false }]),
            }),
        ).await
        .unwrap();

        // Switch to static — should NOT overwrite system DNS
        ipv4_set(
            ctx.clone(),
            DeserializeStdin(WanIpv4SetRequest {
                mode: WanIpv4Mode::Static,
                address: Some("192.168.10.69".to_string()),
                netmask: Some("255.255.255.0".to_string()),
                gateway: Some("192.168.10.1".to_string()),
                username: None,
                password: None,
                device: None,
            }),
        ).await
        .unwrap();

        let res = dns_get(ctx).await.unwrap();
        assert_eq!(res.mode, DnsMode::Custom);
        assert_eq!(res.servers, vec![DnsServer { address: "1.1.1.1".into(), ssl: false }]);
    }

    // ── IPv4 mode switching ──

    #[tokio::test]
    async fn ipv4_static_to_dhcp_clears_fields() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        // Set static first
        ipv4_set(
            ctx.clone(),
            DeserializeStdin(WanIpv4SetRequest {
                mode: WanIpv4Mode::Static,
                address: Some("10.0.0.2".to_string()),
                netmask: Some("255.255.255.0".to_string()),
                gateway: Some("10.0.0.1".to_string()),
                username: None,
                password: None,
                device: None,
            }),
        ).await
        .unwrap();

        // Switch back to DHCP
        ipv4_set(
            ctx.clone(),
            DeserializeStdin(WanIpv4SetRequest {
                mode: WanIpv4Mode::Dhcp,
                address: None,
                netmask: None,
                gateway: None,
                username: None,
                password: None,
                device: None,
            }),
        ).await
        .unwrap();

        let res = ipv4_get(ctx).await.unwrap();
        assert_eq!(res.mode, WanIpv4Mode::Dhcp);
        assert!(res.address.is_none(), "ipaddr should be cleared");
        assert!(res.netmask.is_none(), "netmask should be cleared");
        assert!(res.gateway.is_none(), "gateway should be cleared");
    }

    #[tokio::test]
    async fn ipv4_pppoe_to_dhcp_clears_fields() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        ipv4_set(
            ctx.clone(),
            DeserializeStdin(WanIpv4SetRequest {
                mode: WanIpv4Mode::Pppoe,
                address: None,
                netmask: None,
                gateway: None,
                username: Some("user@isp".to_string()),
                password: Some("secret".to_string()),
                device: None,
            }),
        ).await
        .unwrap();

        ipv4_set(
            ctx.clone(),
            DeserializeStdin(WanIpv4SetRequest {
                mode: WanIpv4Mode::Dhcp,
                address: None,
                netmask: None,
                gateway: None,
                username: None,
                password: None,
                device: None,
            }),
        ).await
        .unwrap();

        let res = ipv4_get(ctx).await.unwrap();
        assert_eq!(res.mode, WanIpv4Mode::Dhcp);
        assert!(res.username.is_none(), "username should be cleared");
        assert!(res.password.is_none(), "password should be cleared");
    }

    #[tokio::test]
    async fn ipv4_static_to_pppoe_clears_static_fields() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        ipv4_set(
            ctx.clone(),
            DeserializeStdin(WanIpv4SetRequest {
                mode: WanIpv4Mode::Static,
                address: Some("10.0.0.2".to_string()),
                netmask: Some("255.255.255.0".to_string()),
                gateway: Some("10.0.0.1".to_string()),
                username: None,
                password: None,
                device: None,
            }),
        ).await
        .unwrap();

        ipv4_set(
            ctx.clone(),
            DeserializeStdin(WanIpv4SetRequest {
                mode: WanIpv4Mode::Pppoe,
                address: None,
                netmask: None,
                gateway: None,
                username: Some("user@isp".to_string()),
                password: Some("secret".to_string()),
                device: None,
            }),
        ).await
        .unwrap();

        let res = ipv4_get(ctx).await.unwrap();
        assert_eq!(res.mode, WanIpv4Mode::Pppoe);
        assert_eq!(res.username.as_deref(), Some("user@isp"));
        assert!(res.address.is_none(), "ipaddr should be cleared");
        assert!(res.netmask.is_none(), "netmask should be cleared");
        assert!(res.gateway.is_none(), "gateway should be cleared");
    }

    // ── IPv6 all modes ──

    #[tokio::test]
    async fn ipv6_set_slaac() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        // Disable first, then set SLAAC
        ipv6_set(
            ctx.clone(),
            DeserializeStdin(WanIpv6SetRequest {
                mode: WanIpv6Mode::Disabled,
                address: None, prefix: None, gateway: None,
                ip6prefix: None, ip6prefixlen: None, ip4prefixlen: None, border_relay: None,
                lan_prefix: None,
            }),
        ).await
        .unwrap();

        ipv6_set(
            ctx.clone(),
            DeserializeStdin(WanIpv6SetRequest {
                mode: WanIpv6Mode::Slaac,
                address: None, prefix: None, gateway: None,
                ip6prefix: None, ip6prefixlen: None, ip4prefixlen: None, border_relay: None,
                lan_prefix: None,
            }),
        ).await
        .unwrap();

        let res = ipv6_get(ctx).await.unwrap();
        assert_eq!(res.mode, WanIpv6Mode::Slaac);
    }

    #[tokio::test]
    async fn ipv6_set_dhcpv6() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        ipv6_set(
            ctx.clone(),
            DeserializeStdin(WanIpv6SetRequest {
                mode: WanIpv6Mode::Dhcpv6,
                address: None, prefix: None, gateway: None,
                ip6prefix: None, ip6prefixlen: None, ip4prefixlen: None, border_relay: None,
                lan_prefix: None,
            }),
        ).await
        .unwrap();

        let res = ipv6_get(ctx).await.unwrap();
        assert_eq!(res.mode, WanIpv6Mode::Dhcpv6);
    }

    #[tokio::test]
    async fn ipv6_set_static() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        ipv6_set(
            ctx.clone(),
            DeserializeStdin(WanIpv6SetRequest {
                mode: WanIpv6Mode::Static,
                address: Some("fd00::2".to_string()),
                prefix: Some("/64".to_string()),
                gateway: Some("fd00::1".to_string()),
                ip6prefix: None, ip6prefixlen: None, ip4prefixlen: None, border_relay: None,
                lan_prefix: None,
            }),
        ).await
        .unwrap();

        let res = ipv6_get(ctx).await.unwrap();
        assert_eq!(res.mode, WanIpv6Mode::Static);
        assert_eq!(res.address.as_deref(), Some("fd00::2"));
        assert_eq!(res.prefix.as_deref(), Some("/64"));
        assert_eq!(res.gateway.as_deref(), Some("fd00::1"));
    }

    #[tokio::test]
    async fn ipv6_set_6rd() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        ipv6_set(
            ctx.clone(),
            DeserializeStdin(WanIpv6SetRequest {
                mode: WanIpv6Mode::SixRd,
                address: None,
                prefix: None,
                gateway: None,
                ip6prefix: Some("2001:db8::".to_string()),
                ip6prefixlen: Some("/32".to_string()),
                ip4prefixlen: Some("/0".to_string()),
                border_relay: Some("203.0.113.1".to_string()),
                lan_prefix: None,
            }),
        ).await
        .unwrap();

        let res = ipv6_get(ctx).await.unwrap();
        assert_eq!(res.mode, WanIpv6Mode::SixRd);
        assert_eq!(res.ip6prefix.as_deref(), Some("2001:db8::"));
        assert_eq!(res.ip6prefixlen.as_deref(), Some("/32"));
        assert_eq!(res.ip4prefixlen.as_deref(), Some("/0"));
        assert_eq!(res.border_relay.as_deref(), Some("203.0.113.1"));
    }

    // ── IPv6 mode switching ──

    #[tokio::test]
    async fn ipv6_static_to_slaac_clears_fields() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        ipv6_set(
            ctx.clone(),
            DeserializeStdin(WanIpv6SetRequest {
                mode: WanIpv6Mode::Static,
                address: Some("fd00::2".to_string()),
                prefix: Some("/64".to_string()),
                gateway: Some("fd00::1".to_string()),
                ip6prefix: None, ip6prefixlen: None, ip4prefixlen: None, border_relay: None,
                lan_prefix: None,
            }),
        ).await
        .unwrap();

        ipv6_set(
            ctx.clone(),
            DeserializeStdin(WanIpv6SetRequest {
                mode: WanIpv6Mode::Slaac,
                address: None, prefix: None, gateway: None,
                ip6prefix: None, ip6prefixlen: None, ip4prefixlen: None, border_relay: None,
                lan_prefix: None,
            }),
        ).await
        .unwrap();

        let res = ipv6_get(ctx).await.unwrap();
        assert_eq!(res.mode, WanIpv6Mode::Slaac);
        assert!(res.address.is_none(), "address should be cleared");
        assert!(res.gateway.is_none(), "gateway should be cleared");
    }

    #[tokio::test]
    async fn ipv6_6rd_to_disabled_clears_fields() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        ipv6_set(
            ctx.clone(),
            DeserializeStdin(WanIpv6SetRequest {
                mode: WanIpv6Mode::SixRd,
                address: None,
                prefix: None,
                gateway: None,
                ip6prefix: Some("2001:db8::".to_string()),
                ip6prefixlen: Some("/32".to_string()),
                ip4prefixlen: Some("/0".to_string()),
                border_relay: Some("203.0.113.1".to_string()),
                lan_prefix: None,
            }),
        ).await
        .unwrap();

        ipv6_set(
            ctx.clone(),
            DeserializeStdin(WanIpv6SetRequest {
                mode: WanIpv6Mode::Disabled,
                address: None, prefix: None, gateway: None,
                ip6prefix: None, ip6prefixlen: None, ip4prefixlen: None, border_relay: None,
                lan_prefix: None,
            }),
        ).await
        .unwrap();

        let res = ipv6_get(ctx).await.unwrap();
        assert_eq!(res.mode, WanIpv6Mode::Disabled);
        assert!(res.border_relay.is_none(), "border relay should be cleared");
        assert!(res.ip6prefix.is_none(), "ip6prefix should be cleared");
    }

    // ── MAC revert ──

    #[tokio::test]
    async fn mac_custom_to_router_clears_macaddr() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        mac_set(
            ctx.clone(),
            DeserializeStdin(WanMacSetRequest {
                strategy: MacStrategy::Custom,
                mac: Some("AA:BB:CC:DD:EE:FF".to_string()),
            }),
        ).await
        .unwrap();

        mac_set(
            ctx.clone(),
            DeserializeStdin(WanMacSetRequest {
                strategy: MacStrategy::Router,
                mac: None,
            }),
        ).await
        .unwrap();

        let res = mac_get(ctx).await.unwrap();
        assert_eq!(res.strategy, MacStrategy::Router);
    }

    // ── DNS revert + dual interface ──

    #[tokio::test]
    async fn dns_custom_to_isp_clears_servers() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        setup_startwrt(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        dns_set(
            ctx.clone(),
            DeserializeStdin(WanDnsSetRequest {
                mode: DnsMode::Custom,
                servers: Some(vec![DnsServer { address: "1.1.1.1".into(), ssl: false }]),
            }),
        ).await
        .unwrap();

        dns_set(
            ctx.clone(),
            DeserializeStdin(WanDnsSetRequest {
                mode: DnsMode::Isp,
                servers: None,
            }),
        ).await
        .unwrap();

        let res = dns_get(ctx).await.unwrap();
        assert_eq!(res.mode, DnsMode::Isp);
        assert!(res.servers.is_empty(), "servers should be cleared");
    }

    #[tokio::test]
    async fn dns_set_custom_rewrites_profile_dnsmasq() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        setup_startwrt(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        // Set system DNS to custom
        dns_set(
            ctx.clone(),
            DeserializeStdin(WanDnsSetRequest {
                mode: DnsMode::Custom,
                servers: Some(vec![DnsServer { address: "9.9.9.9".into(), ssl: false }]),
            }),
        ).await
        .unwrap();

        // Verify that a per-profile dnsmasq section was created for the Admin profile
        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["dhcp", "network"]).await.unwrap();
        let has_dns_lan = cfgs["dhcp"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("dns_lan"));
        assert!(has_dns_lan, "dns_set should create per-profile dnsmasq for existing profiles");

        // Verify peerdns=0 on both WAN and WAN6
        for iface_name in &[WAN_INTERFACE, WAN6_INTERFACE] {
            let section = cfgs["network"]
                .sections
                .iter()
                .find(|s| s.name().as_deref() == Some(iface_name))
                .unwrap_or_else(|| panic!("missing {iface_name} section"));
            let iface = section.get_typed::<NetworkInterface>().unwrap().unwrap();
            assert_eq!(
                iface.peerdns.as_deref(),
                Some("0"),
                "peerdns should be 0 on {iface_name} in custom DNS mode"
            );
        }
    }

    #[tokio::test]
    async fn dns_set_isp_removes_profile_dnsmasq() {
        let dir = tempfile::tempdir().unwrap();
        setup_network(dir.path());
        setup_startwrt(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        // Set system DNS to custom first
        dns_set(
            ctx.clone(),
            DeserializeStdin(WanDnsSetRequest {
                mode: DnsMode::Custom,
                servers: Some(vec![DnsServer { address: "9.9.9.9".into(), ssl: false }]),
            }),
        ).await
        .unwrap();

        // Switch back to ISP
        dns_set(
            ctx.clone(),
            DeserializeStdin(WanDnsSetRequest {
                mode: DnsMode::Isp,
                servers: None,
            }),
        ).await
        .unwrap();

        // Verify that the per-profile dnsmasq section was removed
        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["dhcp", "network"]).await.unwrap();
        let has_dns_lan = cfgs["dhcp"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("dns_lan"));
        assert!(!has_dns_lan, "dns_set ISP should remove per-profile dnsmasq sections");

        // Verify peerdns=1 on both WAN and WAN6
        for iface_name in &[WAN_INTERFACE, WAN6_INTERFACE] {
            let section = cfgs["network"]
                .sections
                .iter()
                .find(|s| s.name().as_deref() == Some(iface_name))
                .unwrap_or_else(|| panic!("missing {iface_name} section"));
            let iface = section.get_typed::<NetworkInterface>().unwrap().unwrap();
            assert_eq!(
                iface.peerdns.as_deref(),
                Some("1"),
                "peerdns should be 1 on {iface_name} in ISP DNS mode"
            );
        }
    }

    // ── DDNS revert + provider switching ──

    #[tokio::test]
    async fn ddns_enable_then_disable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ddns"), "").unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        ddns_set(
            ctx.clone(),
            DeserializeStdin(WanDdnsSetRequest {
                enabled: true,
                provider: DdnsProvider::Duckdns,
                hostname: Some("myhost".to_string()),
                username: None,
                password: None,
                token: Some("tok123".to_string()),
                zone: None,
            }),
        ).await
        .unwrap();

        ddns_set(
            ctx.clone(),
            DeserializeStdin(WanDdnsSetRequest {
                enabled: false,
                provider: DdnsProvider::Duckdns,
                hostname: None,
                username: None,
                password: None,
                token: None,
                zone: None,
            }),
        ).await
        .unwrap();

        let res = ddns_get(ctx).await.unwrap();
        assert!(!res.enabled);
    }

    #[tokio::test]
    async fn ddns_switch_provider_dyndns_to_duckdns() {
        let dir = tempfile::tempdir().unwrap();
        setup_ddns(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        // Start with dyndns (from setup_ddns), switch to duckdns
        ddns_set(
            ctx.clone(),
            DeserializeStdin(WanDdnsSetRequest {
                enabled: true,
                provider: DdnsProvider::Duckdns,
                hostname: Some("myhost.duckdns.org".to_string()),
                username: None,
                password: None,
                token: Some("duck-token".to_string()),
                zone: None,
            }),
        ).await
        .unwrap();

        let res = ddns_get(ctx).await.unwrap();
        assert_eq!(res.provider, DdnsProvider::Duckdns);
        assert_eq!(res.token.as_deref(), Some("duck-token"));
        assert_eq!(res.hostname.as_deref(), Some("myhost.duckdns.org"));
        assert!(res.username.is_none(), "username should not be set for token-based provider");
    }
}
