use crate::prelude::*;
use crate::profiles::reload_system;
use crate::utils::{DeserializeStdin, HandlerExtSerde};
use crate::vpn_server::{UciVpnServer, WgInterface};
use crate::wg::WgKey;
use crate::{CliContext, Error, ErrorKind, ServerContext};
use clap::Parser;
use rpc_toolkit::{from_fn_async_local, HandlerExt, ParentHandler};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use uciedit::{dump_all, parse_all, Arena, Configs, Line, LineComment, Section, Token, TypedSection};

/// VPN client metadata stored in /etc/config/startwrt
#[derive(Debug, TypedSection)]
#[uci(ty = "vpn_client")]
struct UciVpnClient {
    /// WireGuard interface name (e.g., "wg_proton")
    pub interface: String,
    /// Human-readable label
    pub label: String,
    /// "Internet" or another VPN's label (for chaining)
    pub target: String,
}

// === Public API Types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundVpn {
    pub id: String,
    pub label: String,
    pub target: String,
    pub enabled: bool,
    pub used_by: Vec<String>,
    /// True when the VPN's WireGuard interface has at least one IPv6 address;
    /// false means profiles routed through this VPN will have IPv6 disabled
    /// (per `outbound_supports_ipv6` in profiles.rs).
    pub supports_ipv6: bool,
    /// Interface MTU, if explicitly set. `None` means the kernel default
    /// (1420 on a clean path) is in effect. Editable via `update`.
    pub mtu: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundVpnCreateRequest {
    pub label: String,
    pub target: String,
    /// Raw WireGuard .conf file contents
    pub config: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundVpnCreateResponse {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundVpnUpdateRequest {
    pub id: String,
    pub label: String,
    pub target: String,
    /// Desired interface MTU. `Some(n)` writes `option mtu n`; `None` (or
    /// absent) clears it so the tunnel inherits the kernel default. The web
    /// edit form always submits the field's current value, so `None`
    /// unambiguously means "clear". UCI is the single source of truth — there
    /// is no stored `.conf` to round-trip.
    #[serde(default)]
    pub mtu: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct OutboundVpnDeleteRequest {
    #[clap(short, long)]
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundVpnSetEnabledRequest {
    pub id: String,
    pub enabled: bool,
}

// === Parsed WireGuard Config ===

#[derive(Debug)]
struct ParsedWgConfig {
    private_key: String,
    addresses: Vec<String>,
    dns: Vec<String>,
    mtu: Option<u16>,
    public_key: String,
    preshared_key: Option<String>,
    endpoint_host: Option<String>,
    endpoint_port: Option<String>,
    allowed_ips: Vec<String>,
    persistent_keepalive: Option<String>,
}

// === API Handler ===

pub fn vpn_client<C: rpc_toolkit::Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async_local(list)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "create",
            from_fn_async_local(create)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "update",
            from_fn_async_local(update)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "delete",
            from_fn_async_local(delete)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-enabled",
            from_fn_async_local(set_enabled)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
}

// === Implementation ===

/// Restart a WireGuard interface (ifdown + ifup)
async fn restart_wireguard_interface(interface_name: &str) -> Result<(), Error> {
    let _ = crate::run_quiet_async(
        tokio::process::Command::new("ifdown").arg(interface_name),
    )
    .await;

    let status = crate::run_quiet_async(
        tokio::process::Command::new("ifup").arg(interface_name),
    )
    .await
    .map_err(|e| Error::new(eyre!("ifup {interface_name}: {e}"), ErrorKind::Network))?;
    if !status.success() {
        return Err(Error::new(
            eyre!("ifup {interface_name} failed with exit code {:?}", status.code()),
            ErrorKind::Network,
        ));
    }

    Ok(())
}

/// Find all VPN clients whose `target` matches a given label (for chain dependency checks).
fn get_vpn_dependents(cfgs: &Configs, label: &str) -> Vec<String> {
    cfgs["startwrt"]
        .sections
        .iter()
        .filter_map(|s| s.get::<UciVpnClient>().ok())
        .filter(|meta| meta.target == label)
        .map(|meta| meta.label.clone())
        .collect()
}

/// Get the set of VPN server interface names from startwrt config
fn get_vpn_server_interfaces(cfgs: &Configs) -> std::collections::BTreeSet<String> {
    cfgs["startwrt"]
        .sections
        .iter()
        .filter_map(|s| s.get::<UciVpnServer>().ok())
        .map(|meta| meta.interface.clone())
        .collect()
}

/// Compute which profiles use a given WireGuard interface as their outbound route.
/// Reads UciProfile sections directly from startwrt config.
fn get_used_by_profiles(cfgs: &Configs, wg_interface_name: &str) -> Vec<String> {
    #[derive(Debug, TypedSection)]
    #[uci(ty = "profile")]
    struct UciProfileOutbound {
        pub fullname: String,
        #[uci(default)]
        pub outbound: Option<String>,
    }

    cfgs["startwrt"]
        .sections
        .iter()
        .filter_map(|s| s.get::<UciProfileOutbound>().ok())
        .filter(|p| p.outbound.as_deref() == Some(wg_interface_name))
        .map(|p| p.fullname.clone())
        .collect()
}

/// Linux enforces IFNAMSIZ = 16 (15 chars + NUL). The "wg_" prefix takes 3,
/// leaving 12 for the sanitized label.
const MAX_INTERFACE_NAME_LEN: usize = 15;
const INTERFACE_PREFIX: &str = "wg_";
const MAX_LABEL_SANITIZED_LEN: usize = MAX_INTERFACE_NAME_LEN - INTERFACE_PREFIX.len(); // 12

/// Sanitize a label into a valid UCI / Linux interface name.
/// Returns an error if the sanitized label exceeds 12 characters.
fn sanitize_interface_name(label: &str) -> Result<String, Error> {
    let sanitized: String = label
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if sanitized.len() > MAX_LABEL_SANITIZED_LEN {
        return Err(Error::new(eyre!(
            "invalid label: '{}' produces a {}-char interface name, max is {}",
            label,
            INTERFACE_PREFIX.len() + sanitized.len(),
            MAX_INTERFACE_NAME_LEN
        ), ErrorKind::InvalidValue));
    }
    Ok(format!("{}{}", INTERFACE_PREFIX, sanitized))
}

/// Heuristic: does this text look like an OpenVPN config? Used purely to give a
/// precise "OpenVPN isn't supported" error instead of the generic "not a
/// WireGuard config" message. WireGuard .conf keys (PrivateKey, Address, DNS,
/// MTU, PublicKey, Endpoint, AllowedIPs, PresharedKey, PersistentKeepalive)
/// don't collide with any of these OpenVPN directives, so a hit is conclusive.
fn looks_like_openvpn(config: &str) -> bool {
    const MARKERS: &[&str] = &[
        "client",
        "remote ",
        "dev tun",
        "dev tap",
        "proto ",
        "ca ",
        "cert ",
        "tls-auth",
        "tls-crypt",
        "auth-user-pass",
        "cipher ",
        "persist-tun",
        "persist-key",
        "remote-cert-tls",
        "resolv-retry",
        "nobind",
        "<ca>",
        "<cert>",
        "<key>",
        "<tls-auth>",
        "<tls-crypt>",
    ];
    config.lines().any(|line| {
        let line = line.trim().to_lowercase();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            return false;
        }
        MARKERS
            .iter()
            .any(|m| line == m.trim_end() || line.starts_with(m))
    })
}

/// Parse a WireGuard .conf file into its components
fn parse_wireguard_config(config: &str) -> Result<ParsedWgConfig, Error> {
    let mut current_section = String::new();
    let mut saw_interface_header = false;
    let mut saw_peer_header = false;
    let mut private_key = String::new();
    let mut addresses = Vec::new();
    let mut dns = Vec::new();
    let mut mtu = None;
    let mut public_key = String::new();
    let mut preshared_key = None;
    let mut endpoint_host = None;
    let mut endpoint_port = None;
    let mut allowed_ips = Vec::new();
    let mut persistent_keepalive = None;

    for line in config.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let lower = line.to_lowercase();
        if lower == "[interface]" {
            current_section = "interface".into();
            saw_interface_header = true;
            continue;
        }
        if lower == "[peer]" {
            current_section = "peer".into();
            saw_peer_header = true;
            continue;
        }

        let Some((key, value)) = line.split_once('=') else { continue };
        let key = key.trim().to_lowercase();
        let value = value.trim();

        match current_section.as_str() {
            "interface" => match key.as_str() {
                "privatekey" => private_key = value.to_string(),
                "address" => addresses = value.split(',').map(|s| s.trim().to_string()).collect(),
                "dns" => dns = value.split(',').map(|s| s.trim().to_string()).collect(),
                "mtu" => {
                    let parsed = value.parse::<u32>().map_err(|_| Error::new(
                        eyre!("Invalid MTU '{}': must be a number", value),
                        ErrorKind::InvalidValue,
                    ))?;
                    validate_mtu(parsed)?;
                    mtu = Some(parsed as u16);
                }
                _ => {}
            },
            "peer" => match key.as_str() {
                "publickey" => public_key = value.to_string(),
                "presharedkey" => preshared_key = Some(value.to_string()),
                "endpoint" => {
                    let endpoint = value.trim();
                    if let Some(colon_idx) = endpoint.rfind(':') {
                        endpoint_host = Some(endpoint[..colon_idx].to_string());
                        endpoint_port = Some(endpoint[colon_idx + 1..].to_string());
                    }
                }
                "allowedips" => allowed_ips = value.split(',').map(|s| s.trim().to_string()).collect(),
                "persistentkeepalive" => persistent_keepalive = Some(value.to_string()),
                _ => {}
            },
            _ => {}
        }
    }

    // Structural gate: a WireGuard .conf is an INI file with at least an
    // [Interface] section. Reject anything else here so OpenVPN (and other
    // non-WG formats) fail with a clear message instead of the misleading
    // "missing PrivateKey" error they would otherwise hit below — every line of
    // an OpenVPN file is silently skipped, leaving the keys empty.
    if !saw_interface_header {
        if looks_like_openvpn(config) {
            return Err(Error::new(eyre!("This looks like an OpenVPN configuration. Only WireGuard configs are supported."), ErrorKind::InvalidValue));
        }
        return Err(Error::new(eyre!("Not a WireGuard configuration: missing [Interface] section"), ErrorKind::InvalidValue));
    }
    if !saw_peer_header {
        return Err(Error::new(eyre!("WireGuard config missing [Peer] section"), ErrorKind::InvalidValue));
    }

    if private_key.is_empty() {
        return Err(Error::new(eyre!("WireGuard config missing PrivateKey in [Interface] section"), ErrorKind::InvalidValue));
    }
    if public_key.is_empty() {
        return Err(Error::new(eyre!("WireGuard config missing PublicKey in [Peer] section"), ErrorKind::InvalidValue));
    }

    // Validate WireGuard keys are valid base64-encoded 32-byte keys
    WgKey::try_from(private_key.as_str())
        .map_err(|_| Error::new(eyre!("Invalid PrivateKey: must be a valid base64-encoded 32-byte key"), ErrorKind::InvalidValue))?;
    WgKey::try_from(public_key.as_str())
        .map_err(|_| Error::new(eyre!("Invalid PublicKey: must be a valid base64-encoded 32-byte key"), ErrorKind::InvalidValue))?;
    if let Some(ref psk) = preshared_key {
        WgKey::try_from(psk.as_str())
            .map_err(|_| Error::new(eyre!("Invalid PresharedKey: must be a valid base64-encoded 32-byte key"), ErrorKind::InvalidValue))?;
    }

    // Require an interface Address and a peer Endpoint. A WG config can parse
    // cleanly without these, but the resulting outbound tunnel would have no
    // local IP / nothing to connect to — a silent dead interface. Reject early.
    if addresses.is_empty() {
        return Err(Error::new(eyre!("WireGuard config missing Address in [Interface] section"), ErrorKind::InvalidValue));
    }
    if endpoint_host.is_none() {
        return Err(Error::new(eyre!("WireGuard config missing or invalid Endpoint in [Peer] section"), ErrorKind::InvalidValue));
    }

    // Validate endpoint_host: non-empty, no whitespace
    if let Some(ref host) = endpoint_host {
        if host.is_empty() || host.chars().any(|c| c.is_whitespace()) {
            return Err(Error::new(eyre!("Invalid Endpoint host: must be non-empty with no whitespace"), ErrorKind::InvalidValue));
        }
    }

    // Validate endpoint_port: must be a valid u16 in range 1-65535
    if let Some(ref port) = endpoint_port {
        let p = port.parse::<u16>()
            .map_err(|_| Error::new(eyre!("Invalid Endpoint port '{}': must be a valid port number (1-65535)", port), ErrorKind::InvalidValue))?;
        if p == 0 {
            return Err(Error::new(eyre!("Invalid Endpoint port '0': must be 1-65535"), ErrorKind::InvalidValue));
        }
    }

    // Validate persistent_keepalive: must be a valid u16
    if let Some(ref ka) = persistent_keepalive {
        ka.parse::<u16>()
            .map_err(|_| Error::new(eyre!("Invalid PersistentKeepalive '{}': must be a number (0-65535)", ka), ErrorKind::InvalidValue))?;
    }

    // Validate allowed_ips: each entry must be a valid CIDR
    for ip_str in &allowed_ips {
        validate_cidr(ip_str)?;
    }

    Ok(ParsedWgConfig {
        private_key,
        addresses,
        dns,
        mtu,
        public_key,
        preshared_key,
        endpoint_host,
        endpoint_port,
        allowed_ips,
        persistent_keepalive,
    })
}

/// Validate a CIDR notation string (e.g., "10.0.0.0/24" or "0.0.0.0/0")
fn validate_cidr(cidr: &str) -> Result<(), Error> {
    let Some((ip_part, prefix_part)) = cidr.split_once('/') else {
        return Err(Error::new(eyre!("Invalid AllowedIPs entry '{}': expected CIDR notation (e.g., 0.0.0.0/0)", cidr), ErrorKind::InvalidValue));
    };
    ip_part.parse::<IpAddr>()
        .map_err(|_| Error::new(eyre!("Invalid AllowedIPs entry '{}': invalid IP address", cidr), ErrorKind::InvalidValue))?;
    let prefix: u8 = prefix_part.parse()
        .map_err(|_| Error::new(eyre!("Invalid AllowedIPs entry '{}': invalid prefix length", cidr), ErrorKind::InvalidValue))?;
    let max_prefix = if ip_part.contains(':') { 128 } else { 32 };
    if prefix > max_prefix {
        return Err(Error::new(eyre!("Invalid AllowedIPs entry '{}': prefix length exceeds maximum ({})", cidr, max_prefix), ErrorKind::InvalidValue));
    }
    Ok(())
}

/// WireGuard interface MTU bounds. The lower bound is the IPv6 minimum link
/// MTU (the tunnel may carry IPv6, so the interface must be ≥ 1280); the upper
/// bound is standard Ethernet. 1280 is the universal safe value for flaky or
/// obfuscated (e.g. :443) endpoints whose path can't carry the 1420 default.
const MIN_WG_MTU: u32 = 1280;
const MAX_WG_MTU: u32 = 1500;

/// Validate an MTU is within the supported range for a WireGuard tunnel.
/// Takes a `u32` so out-of-range numbers (e.g. 70000) report the range error
/// rather than overflowing a `u16` parse into a misleading "not a number".
fn validate_mtu(mtu: u32) -> Result<(), Error> {
    if !(MIN_WG_MTU..=MAX_WG_MTU).contains(&mtu) {
        return Err(Error::new(
            eyre!("Invalid MTU {mtu}: must be between {MIN_WG_MTU} and {MAX_WG_MTU}"),
            ErrorKind::InvalidValue,
        ));
    }
    Ok(())
}

/// Validate a label is non-empty and safe for UCI
fn validate_label(label: &str) -> Result<(), Error> {
    let trimmed = label.trim();
    if trimmed.is_empty() || trimmed.chars().any(|c| c < '\x20') {
        return Err(Error::new(eyre!("invalid label: {label}"), ErrorKind::InvalidValue));
    }
    Ok(())
}

/// Validate that `target` exists and wouldn't create a routing cycle.
/// `self_label` is the label of the VPN being created/updated.
fn validate_target(cfgs: &Configs, target: &str, self_label: &str) -> Result<(), Error> {
    if target == "Internet" {
        return Ok(());
    }

    // Build label → target map from all VPN client entries
    let clients: Vec<UciVpnClient> = cfgs["startwrt"]
        .sections
        .iter()
        .filter_map(|s| s.get::<UciVpnClient>().ok())
        .collect();

    let label_to_target: std::collections::HashMap<&str, &str> = clients
        .iter()
        .map(|c| (c.label.as_str(), c.target.as_str()))
        .collect();

    // Check that target exists as a label
    if !label_to_target.contains_key(target) {
        return Err(Error::new(eyre!("invalid target: {target}"), ErrorKind::InvalidValue));
    }

    // Walk the chain from target, checking for cycles back to self_label
    let mut visited = vec![self_label.to_string()];
    let mut current = target;
    loop {
        if current == self_label {
            return Err(Error::new(eyre!("VPN chain cycle detected for '{}': {:?}", self_label, visited), ErrorKind::VpnChainCycle));
        }
        if current == "Internet" {
            return Ok(());
        }
        visited.push(current.to_string());
        match label_to_target.get(current) {
            Some(next) => current = next,
            None => return Ok(()), // target chain ends at unknown label
        }
    }
}

/// List all outbound VPN clients
#[instrument(skip_all)]
pub async fn list(_ctx: ServerContext) -> Result<Vec<OutboundVpn>, Error> {
    let arena = Arena::new();
    let cfgs = parse_all("/etc/config", &arena, &["network", "startwrt"]).await?;

    let server_interfaces = get_vpn_server_interfaces(&cfgs);

    let clients: Vec<OutboundVpn> = cfgs["startwrt"]
        .sections
        .iter()
        .filter_map(|section| section.get::<UciVpnClient>().ok())
        .filter_map(|meta| {
            // Find the WireGuard interface in network config
            let wg_iface = cfgs["network"]
                .sections
                .iter()
                .find(|s| s.name().as_deref() == Some(meta.interface.as_str()))
                .and_then(|s| s.get::<WgInterface>().ok())
                .filter(|wg| wg.is_wireguard())?;

            // Skip interfaces that are VPN servers
            if server_interfaces.contains(&meta.interface) {
                return None;
            }

            let enabled = !wg_iface.disabled();
            let supports_ipv6 = wg_iface.addresses.iter().any(|a| a.contains(':'));
            let mtu = wg_iface.mtu;

            let used_by = get_used_by_profiles(&cfgs, &meta.interface);

            Some(OutboundVpn {
                id: meta.interface.clone(),
                label: meta.label.clone(),
                target: meta.target.clone(),
                enabled,
                used_by,
                supports_ipv6,
                mtu,
            })
        })
        .collect();

    Ok(clients)
}

/// Create a new outbound VPN client from a WireGuard .conf file
#[instrument(skip_all)]
pub async fn create(
    _ctx: ServerContext,
    DeserializeStdin(req): DeserializeStdin<OutboundVpnCreateRequest>,
) -> Result<OutboundVpnCreateResponse, Error> {
    validate_label(&req.label)?;

    let parsed = parse_wireguard_config(&req.config)?;
    let interface_name = sanitize_interface_name(&req.label)?;

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all("/etc/config", &arena, &["network", "startwrt"]).await?;

        // Check for interface name conflict
        let name_conflict = cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some(interface_name.as_str()));
        if name_conflict {
            return Err(Error::new(eyre!("interface name conflict: {interface_name}"), ErrorKind::InterfaceNameConflict));
        }

        // Check for duplicate label
        let label_conflict = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnClient>().ok())
            .any(|meta| meta.label == req.label);
        if label_conflict {
            return Err(Error::new(eyre!("invalid label: {} (duplicate)", req.label), ErrorKind::InvalidValue));
        }

        validate_target(&cfgs, &req.target, &req.label)?;

        // Create the WireGuard interface section
        create_wg_client_interface(&mut cfgs, &interface_name, &parsed, &arena)?;

        // Create the peer section
        create_wg_client_peer(&mut cfgs, &interface_name, &parsed, &arena)?;

        // Create vpn_client metadata in startwrt
        let meta = UciVpnClient {
            interface: interface_name.clone(),
            label: req.label.clone(),
            target: req.target.clone(),
        };
        cfgs["startwrt"].append(&meta, Some(&interface_name))?;

        rewrite_vpn_chain_routes(&mut cfgs)?;

        match dump_all("/etc/config", cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log("vpn-client", "created", false, &format!("Failed to create outbound VPN '{}'", req.label), Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                restart_wireguard_interface(&interface_name).await?;
                reload_system().await?;
                crate::activity::log("vpn-client", "created", true, &format!("Created outbound VPN '{}'", req.label), None);
                return Ok(OutboundVpnCreateResponse { id: interface_name });
            }
        }
    }
}

/// Update label and target for an existing VPN client
#[instrument(skip_all)]
pub async fn update(
    _ctx: ServerContext,
    DeserializeStdin(req): DeserializeStdin<OutboundVpnUpdateRequest>,
) -> Result<(), Error> {
    validate_label(&req.label)?;
    if let Some(mtu) = req.mtu {
        validate_mtu(mtu.into())?;
    }

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all("/etc/config", &arena, &["network", "startwrt"]).await?;

        // Check for duplicate label (excluding self)
        let label_conflict = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnClient>().ok())
            .any(|m| m.label == req.label && m.interface != req.id);
        if label_conflict {
            return Err(Error::new(eyre!("invalid label: {} (duplicate)", req.label), ErrorKind::InvalidValue));
        }

        // Find and update metadata, capturing old label for cascade
        let mut found = false;
        let mut old_label = String::new();
        for section in &mut cfgs["startwrt"].sections {
            let Ok(mut meta) = section.get::<UciVpnClient>() else { continue };
            if meta.interface != req.id {
                continue;
            }

            old_label = meta.label.clone();
            meta.label = req.label.clone();
            meta.target = req.target.clone();
            section.set(&meta)?;
            found = true;
            break;
        }

        if !found {
            return Err(Error::new(eyre!("VPN client {} not found", req.id), ErrorKind::NotFound));
        }

        validate_target(&cfgs, &req.target, &req.label)?;

        // Cascade label rename: update any VPN clients targeting the old label
        if old_label != req.label {
            for section in &mut cfgs["startwrt"].sections {
                let Ok(mut meta) = section.get::<UciVpnClient>() else { continue };
                if meta.target == old_label {
                    meta.target = req.label.clone();
                    let _ = section.set(&meta);
                }
            }
        }

        // Apply the MTU to the WG interface (network config). `None` clears it,
        // restoring the kernel default. UCI is the single source of truth.
        let mut mtu_changed = false;
        for section in &mut cfgs["network"].sections {
            if section.name().as_deref() != Some(req.id.as_str()) {
                continue;
            }
            let Ok(iface) = section.get::<WgInterface>() else { continue };
            if !iface.is_wireguard() {
                continue;
            }
            mtu_changed = set_mtu_option(section, req.mtu, &arena);
            break;
        }

        rewrite_vpn_chain_routes(&mut cfgs)?;

        match dump_all("/etc/config", cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log("vpn-client", "updated", false, &format!("Failed to update outbound VPN '{}'", req.label), Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                // An MTU change only takes effect on interface bring-up, so
                // bounce the tunnel when it changed (label/target-only edits
                // don't need this).
                if mtu_changed {
                    restart_wireguard_interface(&req.id).await?;
                }
                reload_system().await?;
                crate::activity::log("vpn-client", "updated", true, &format!("Updated outbound VPN '{}'", req.label), None);
                return Ok(());
            }
        }
    }
}

/// Delete an outbound VPN client
#[instrument(skip_all)]
pub async fn delete(ctx: ServerContext, args: OutboundVpnDeleteRequest) -> Result<(), Error> {
    let interface_name = &args.id;

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all("/etc/config", &arena, &["network", "startwrt", "firewall", "dhcp"]).await?;

        // Block deletion if other VPNs chain through this one
        let this_label = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnClient>().ok())
            .find(|meta| meta.interface == *interface_name)
            .map(|meta| meta.label.clone())
            .ok_or_else(|| Error::new(eyre!("VPN client {} not found", interface_name), ErrorKind::NotFound))?;

        let dependents = get_vpn_dependents(&cfgs, &this_label);
        if !dependents.is_empty() {
            return Err(Error::new(eyre!("VPN '{}' has dependents: {:?}", this_label, dependents), ErrorKind::VpnHasDependents));
        }

        // Find profiles that reference this VPN and reset them to WAN
        let affected_profiles = reset_profiles_using_vpn(&mut cfgs, interface_name);

        // Re-apply each affected profile's FULL config (firewall + dhcp + dns +
        // routing) now that its outbound is "wan". This rebuilds the firewall
        // forwarding as `<zone> → wan` (the dedicated vpn_<wg> zone + its
        // forwarding are torn down by cleanup_orphaned_vpn_zones below). Running
        // only rewrite_routing/rewrite_dns here would leave the profile with no
        // `→ wan` forwarding, blocking all of its WAN traffic.
        for (profile_interface, _vlan_tag, _gateway_ip) in &affected_profiles {
            crate::profiles::reapply_profile_config(
                &ctx,
                &mut cfgs,
                crate::profiles::ProfileIdOpt {
                    fullname: None,
                    interface: Some(profile_interface.clone()),
                    vlan_tag: None,
                },
            )?;
        }

        // Remove the WireGuard interface
        cfgs["network"].sections.retain(|section| {
            if section.name().as_deref() == Some(interface_name.as_str()) {
                if let Ok(iface) = section.get::<WgInterface>() {
                    if iface.is_wireguard() {
                        return false;
                    }
                }
            }
            true
        });

        // Remove peers (sections of type wireguard_<interface_name>)
        let peer_type = format!("wireguard_{}", interface_name);
        cfgs["network"]
            .sections
            .retain(|section| section.ty() != peer_type);

        // Remove vpn_client metadata from startwrt
        cfgs["startwrt"].sections.retain(|section| {
            if let Ok(meta) = section.get::<UciVpnClient>() {
                return meta.interface != *interface_name;
            }
            true
        });

        // Rebuild cross-subnet routes (affected profiles switched to WAN)
        crate::profiles::sync_cross_subnet_routes(&mut cfgs)?;
        // Recompute the /32 peer overrides alongside the /24 cross routes so
        // tables left by removed/reset profiles don't retain stale vxr_ entries.
        crate::vpn_server::sync_vpn_peer_cross_routes(&mut cfgs)?;

        // Remove any orphaned VPN interfaces from the WAN firewall zone.
        // This is more robust than removing just the deleted VPN — it also
        // cleans up any previously orphaned entries.
        crate::profiles::cleanup_orphaned_vpn_zones(&mut cfgs);

        rewrite_vpn_chain_routes(&mut cfgs)?;

        match dump_all("/etc/config", cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log("vpn-client", "deleted", false, &format!("Failed to delete outbound VPN '{}'", this_label), Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                // network reload will see the WG interface is gone from config
                // and tear it down atomically — no separate ifdown needed.
                reload_system().await?;
                crate::activity::log("vpn-client", "deleted", true, &format!("Deleted outbound VPN '{}'", this_label), None);
                return Ok(());
            }
        }
    }
}

/// Reset profiles that use the given VPN interface back to "wan".
/// Returns `(interface, vlan_tag, gateway_ip)` tuples for each modified profile.
fn reset_profiles_using_vpn(cfgs: &mut Configs, vpn_interface: &str) -> Vec<(String, u16, std::net::Ipv4Addr)> {
    #[derive(Debug, TypedSection)]
    #[uci(ty = "profile")]
    struct UciProfileReset {
        pub fullname: String,
        pub interface: String,
        pub vlan_tag: u16,
        #[uci(default)]
        pub outbound: Option<String>,
    }

    // Pre-read gateway IPs from network config
    let gateway_ips: std::collections::HashMap<String, std::net::Ipv4Addr> = cfgs["network"]
        .sections
        .iter()
        .filter_map(|s| {
            let name = s.name()?.to_string();
            let ni = s.get::<uciedit::openwrt::NetworkInterface>().ok()?;
            let ip = ni.ipaddr?;
            Some((name, ip))
        })
        .collect();

    let mut affected = Vec::new();
    for section in &mut cfgs["startwrt"].sections {
        let Ok(mut profile) = section.get::<UciProfileReset>() else { continue };
        if profile.outbound.as_deref() == Some(vpn_interface) {
            let gateway_ip = gateway_ips
                .get(&profile.interface)
                .copied()
                .unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
            affected.push((profile.interface.clone(), profile.vlan_tag, gateway_ip));
            profile.outbound = Some("wan".to_string());
            let _ = section.set(&profile);
        }
    }
    affected
}

/// Enable or disable an outbound VPN client
#[instrument(skip_all)]
pub async fn set_enabled(
    ctx: ServerContext,
    DeserializeStdin(req): DeserializeStdin<OutboundVpnSetEnabledRequest>,
) -> Result<(), Error> {
    let interface_name = &req.id;

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(
            "/etc/config",
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        ).await?;

        // Verify the VPN client exists in metadata and get its label
        let this_meta = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnClient>().ok())
            .find(|meta| meta.interface == *interface_name);
        let Some(this_meta) = this_meta else {
            return Err(Error::new(eyre!("VPN client {} not found", interface_name), ErrorKind::NotFound));
        };
        let vpn_label = this_meta.label.clone();

        // Block disabling if other VPNs chain through this one
        if !req.enabled {
            let dependents = get_vpn_dependents(&cfgs, &this_meta.label);
            if !dependents.is_empty() {
                return Err(Error::new(eyre!("VPN '{}' has dependents: {:?}", this_meta.label, dependents), ErrorKind::VpnHasDependents));
            }
        }

        // When disabling, reset any profiles that route through this VPN back to
        // WAN and re-apply their full config so the firewall forwarding is
        // rebuilt as `<zone> → wan` (see reapply_profile_config). The disabled
        // VPN's now-unused vpn_<wg> zone + stale forwarding are then removed by
        // cleanup_orphaned_vpn_zones.
        if !req.enabled {
            let affected_profiles = reset_profiles_using_vpn(&mut cfgs, interface_name);
            for (profile_interface, _vlan_tag, _gateway_ip) in &affected_profiles {
                crate::profiles::reapply_profile_config(
                    &ctx,
                    &mut cfgs,
                    crate::profiles::ProfileIdOpt {
                        fullname: None,
                        interface: Some(profile_interface.clone()),
                        vlan_tag: None,
                    },
                )?;
            }
            crate::profiles::cleanup_orphaned_vpn_zones(&mut cfgs);
        }

        // Find and update the WireGuard interface disabled state
        let mut found = false;
        for section in &mut cfgs["network"].sections {
            if section.name().as_deref() != Some(interface_name.as_str()) {
                continue;
            }
            let Ok(iface) = section.get::<WgInterface>() else { continue };
            if !iface.is_wireguard() {
                continue;
            }

            // Toggle the disabled option
            set_disabled_option(section, !req.enabled, &arena);
            found = true;
            break;
        }

        if !found {
            return Err(Error::new(eyre!(
                "WireGuard interface {} not found",
                interface_name
            ), ErrorKind::NotFound));
        }

        // Rebuild cross-subnet routes (profile routing may have changed)
        crate::profiles::sync_cross_subnet_routes(&mut cfgs)?;
        // Recompute the /32 peer overrides alongside the /24 cross routes so
        // tables left by removed/reset profiles don't retain stale vxr_ entries.
        crate::vpn_server::sync_vpn_peer_cross_routes(&mut cfgs)?;

        rewrite_vpn_chain_routes(&mut cfgs)?;

        match dump_all("/etc/config", cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                let action = if req.enabled { "enabled" } else { "disabled" };
                crate::activity::log("vpn-client", action, false, &format!("Failed to {} outbound VPN '{}'", if req.enabled { "enable" } else { "disable" }, vpn_label), Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                if req.enabled {
                    restart_wireguard_interface(interface_name).await?;
                } else {
                    let _ = crate::run_quiet_async(
                        tokio::process::Command::new("ifdown").arg(interface_name),
                    )
                    .await;
                }
                reload_system().await?;
                let action = if req.enabled { "enabled" } else { "disabled" };
                crate::activity::log("vpn-client", action, true, &format!("{} outbound VPN '{}'", if req.enabled { "Enabled" } else { "Disabled" }, vpn_label), None);
                return Ok(());
            }
        }
    }
}

// === VPN Chain Routing ===

/// Read the `endpoint_host` from a peer section (`wireguard_<interface>`).
/// Strips the surrounding `[...]` of a bracketed IPv6 literal so the returned
/// string parses as `IpAddr` directly. (The literal in UCI keeps the brackets;
/// `wireguard.sh` handles them before passing to `wg`.)
fn get_peer_endpoint_host(cfgs: &Configs, interface: &str) -> Option<String> {
    let peer_type = format!("wireguard_{}", interface);
    cfgs["network"]
        .sections
        .iter()
        .find(|s| s.ty() == peer_type)
        .and_then(|s| {
            s.lines.iter().find_map(|line| {
                if let Line::Option {
                    option, value, ..
                } = line
                {
                    if option.as_str() == "endpoint_host" {
                        let raw = value.as_str();
                        let stripped = raw
                            .strip_prefix('[')
                            .and_then(|s| s.strip_suffix(']'))
                            .unwrap_or(&raw);
                        return Some(stripped.to_string());
                    }
                }
                None
            })
        })
}

/// Idempotently rebuild all VPN chain endpoint routes (`vcr_*` sections).
///
/// For each VPN client whose target is another VPN (not "Internet"), creates a
/// host route in the main routing table sending the VPN's peer endpoint
/// through its target VPN's tunnel interface. This ensures WireGuard's locally-
/// generated UDP packets traverse the chain instead of exiting via WAN.
/// IPv4 endpoints produce a `route` /32; IPv6 endpoints produce a `route6` /128.
pub(crate) fn rewrite_vpn_chain_routes(cfgs: &mut Configs) -> Result<(), Error> {
    use uciedit::openwrt::{NetworkRoute, NetworkRoute6};

    // 1. Remove all existing vcr_* route / route6 sections
    cfgs["network"]
        .sections
        .retain(|s| !s.name().map(|n| n.starts_with("vcr_")).unwrap_or(false));

    // 2. Build label → interface map from all VPN client entries
    let vpn_clients: Vec<UciVpnClient> = cfgs["startwrt"]
        .sections
        .iter()
        .filter_map(|s| s.get::<UciVpnClient>().ok())
        .collect();

    let label_to_interface: std::collections::HashMap<&str, &str> = vpn_clients
        .iter()
        .map(|c| (c.label.as_str(), c.interface.as_str()))
        .collect();

    // 3. For each VPN client with target ≠ "Internet", create a chain route
    for client in &vpn_clients {
        if client.target == "Internet" {
            continue;
        }

        let Some(target_interface) = label_to_interface.get(client.target.as_str()) else {
            continue;
        };

        let Some(endpoint_host) = get_peer_endpoint_host(cfgs, &client.interface) else {
            continue;
        };

        // Only create routes for IP literals, skip hostnames
        let Ok(addr) = endpoint_host.parse::<IpAddr>() else {
            continue;
        };

        let route_name = format!("vcr_{}", client.interface);
        match addr {
            IpAddr::V4(_) => {
                cfgs["network"].append(
                    &NetworkRoute {
                        interface: target_interface.to_string(),
                        target: format!("{}/32", endpoint_host),
                        ..Default::default()
                    },
                    Some(&route_name),
                )?;
            }
            IpAddr::V6(_) => {
                cfgs["network"].append(
                    &NetworkRoute6 {
                        interface: target_interface.to_string(),
                        target: format!("{}/128", endpoint_host),
                        ..Default::default()
                    },
                    Some(&route_name),
                )?;
            }
        }
    }

    Ok(())
}

// === Helper Functions ===

/// Set the 'disabled' option on a UCI section
fn set_disabled_option<'a>(section: &mut Section<'a>, disabled: bool, arena: &'a Arena) {
    let value = if disabled { "1" } else { "0" };

    // Try to update existing disabled option
    for line in &mut section.lines {
        if let Line::Option { option, value: val, .. } = line {
            if option.as_str() == "disabled" {
                *val = Token::from_str(value, arena);
                return;
            }
        }
    }

    // Add disabled option if not present
    section.lines.push(Line::Option {
        option: Token::from_str("disabled", arena),
        value: Token::from_str(value, arena),
        comment: LineComment::None,
    });
}

/// Set, replace, or remove the `mtu` option on a WG interface section.
/// `None` removes the option (inherit kernel default). Returns `true` when the
/// resulting config differs from what was already there.
fn set_mtu_option<'a>(section: &mut Section<'a>, mtu: Option<u16>, arena: &'a Arena) -> bool {
    let current = section.lines.iter().find_map(|line| match line {
        Line::Option { option, value, .. } if option.as_str() == "mtu" => {
            Some(value.as_str().to_string())
        }
        _ => None,
    });
    let desired = mtu.map(|m| m.to_string());
    if current == desired {
        return false;
    }

    section
        .lines
        .retain(|line| !matches!(line, Line::Option { option, .. } if option.as_str() == "mtu"));

    if let Some(m) = desired {
        let mtu_str: &str = arena.alloc(m);
        section.lines.push(Line::Option {
            option: Token::from_str("mtu", arena),
            value: Token::from_str(mtu_str, arena),
            comment: LineComment::None,
        });
    }
    true
}

/// Create a WireGuard client interface section in network config
fn create_wg_client_interface<'a>(
    cfgs: &mut Configs<'a>,
    interface_name: &str,
    parsed: &ParsedWgConfig,
    arena: &'a Arena,
) -> Result<(), Error> {
    let interface_name_str: &str = arena.alloc(interface_name.to_string());

    let mut lines = vec![Line::Section {
        ty: Token::from_str("interface", arena),
        name: Some(Token::from_str(interface_name_str, arena)),
        comment: LineComment::None,
    }];

    // proto = wireguard
    lines.push(Line::Option {
        option: Token::from_str("proto", arena),
        value: Token::from_str("wireguard", arena),
        comment: LineComment::None,
    });

    // private_key
    let pk_str: &str = arena.alloc(parsed.private_key.clone());
    lines.push(Line::Option {
        option: Token::from_str("private_key", arena),
        value: Token::from_str(pk_str, arena),
        comment: LineComment::None,
    });

    // disabled = 0
    lines.push(Line::Option {
        option: Token::from_str("disabled", arena),
        value: Token::from_str("0", arena),
        comment: LineComment::None,
    });

    // defaultroute = 0 — prevent netifd from adding a default route to the
    // main routing table. Profile-based policy routing handles directing
    // specific profile traffic through this tunnel via per-VLAN tables.
    lines.push(Line::Option {
        option: Token::from_str("defaultroute", arena),
        value: Token::from_str("0", arena),
        comment: LineComment::None,
    });

    // peerdns = 0 — prevent netifd from registering this interface's DNS
    // servers with the system resolver (dnsmasq). Without this, VPN DNS
    // servers pollute the global resolver and all profiles' DNS queries
    // leak through the tunnel. Per-profile DNS is handled separately.
    lines.push(Line::Option {
        option: Token::from_str("peerdns", arena),
        value: Token::from_str("0", arena),
        comment: LineComment::None,
    });

    // mtu — only emitted when the .conf specified one. Omitting it lets
    // wireguard.sh keep the kernel default (1420 on a clean 1500-byte path).
    // A too-high MTU on obfuscated/:443 endpoints silently black-holes large
    // packets; the MSS clamp on the vpn_<X> zone is a TCP-only backstop.
    if let Some(mtu) = parsed.mtu {
        let mtu_str: &str = arena.alloc(mtu.to_string());
        lines.push(Line::Option {
            option: Token::from_str("mtu", arena),
            value: Token::from_str(mtu_str, arena),
            comment: LineComment::None,
        });
    }

    // addresses (list)
    for addr in &parsed.addresses {
        let addr_str: &str = arena.alloc(addr.clone());
        lines.push(Line::List {
            list: Token::from_str("addresses", arena),
            item: Token::from_str(addr_str, arena),
            comment: LineComment::None,
        });
    }

    // dns (list) — stored for reference but peerdns=0 prevents global use
    for dns_entry in &parsed.dns {
        let dns_str: &str = arena.alloc(dns_entry.clone());
        lines.push(Line::List {
            list: Token::from_str("dns", arena),
            item: Token::from_str(dns_str, arena),
            comment: LineComment::None,
        });
    }

    let section = Section { arena, lines };
    cfgs["network"].sections.push(section);

    Ok(())
}

/// Create a WireGuard peer section for the client
fn create_wg_client_peer<'a>(
    cfgs: &mut Configs<'a>,
    interface_name: &str,
    parsed: &ParsedWgConfig,
    arena: &'a Arena,
) -> Result<(), Error> {
    let peer_type = format!("wireguard_{}", interface_name);
    let peer_type_str: &str = arena.alloc(peer_type);
    let peer_name = format!("{}_peer0", interface_name.strip_prefix("wg_").unwrap_or(interface_name));
    let peer_name_str: &str = arena.alloc(peer_name);

    let mut lines = vec![Line::Section {
        ty: Token::from_str(peer_type_str, arena),
        name: Some(Token::from_str(peer_name_str, arena)),
        comment: LineComment::None,
    }];

    // public_key
    let pk_str: &str = arena.alloc(parsed.public_key.clone());
    lines.push(Line::Option {
        option: Token::from_str("public_key", arena),
        value: Token::from_str(pk_str, arena),
        comment: LineComment::None,
    });

    // endpoint_host
    if let Some(ref host) = parsed.endpoint_host {
        let host_str: &str = arena.alloc(host.clone());
        lines.push(Line::Option {
            option: Token::from_str("endpoint_host", arena),
            value: Token::from_str(host_str, arena),
            comment: LineComment::None,
        });
    }

    // endpoint_port
    if let Some(ref port) = parsed.endpoint_port {
        let port_str: &str = arena.alloc(port.clone());
        lines.push(Line::Option {
            option: Token::from_str("endpoint_port", arena),
            value: Token::from_str(port_str, arena),
            comment: LineComment::None,
        });
    }

    // preshared_key
    if let Some(ref psk) = parsed.preshared_key {
        let psk_str: &str = arena.alloc(psk.clone());
        lines.push(Line::Option {
            option: Token::from_str("preshared_key", arena),
            value: Token::from_str(psk_str, arena),
            comment: LineComment::None,
        });
    }

    // persistent_keepalive
    if let Some(ref keepalive) = parsed.persistent_keepalive {
        let ka_str: &str = arena.alloc(keepalive.clone());
        lines.push(Line::Option {
            option: Token::from_str("persistent_keepalive", arena),
            value: Token::from_str(ka_str, arena),
            comment: LineComment::None,
        });
    }

    // route_allowed_ips = 0 — profile-based policy routing (ip rule + per-VLAN
    // routing tables in profiles.rs) handles directing traffic through this tunnel.
    // Setting this to 1 with AllowedIPs = 0.0.0.0/0 would add a default route to
    // the main table, hijacking all traffic including the tunnel's own endpoint
    // reachability, creating a routing loop that kills all connectivity.
    lines.push(Line::Option {
        option: Token::from_str("route_allowed_ips", arena),
        value: Token::from_str("0", arena),
        comment: LineComment::None,
    });

    // allowed_ips (list)
    for ip in &parsed.allowed_ips {
        let ip_str: &str = arena.alloc(ip.clone());
        lines.push(Line::List {
            list: Token::from_str("allowed_ips", arena),
            item: Token::from_str(ip_str, arena),
            comment: LineComment::None,
        });
    }

    let section = Section { arena, lines };
    cfgs["network"].sections.push(section);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wg::{Base64, WgKey};
    use crate::CtrlContext;
    use rpc_toolkit::Context;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    #[derive(Clone)]
    #[allow(dead_code)]
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

    /// Generate a valid WireGuard private key as base64 string
    fn gen_key() -> String {
        Base64::new(WgKey::generate()).to_base64()
    }

    /// Generate a minimal valid WireGuard .conf file
    fn wg_conf(private_key: &str, public_key: &str) -> String {
        format!(
            "[Interface]\n\
             PrivateKey = {private_key}\n\
             Address = 10.2.0.2/32\n\
             DNS = 10.2.0.1\n\
             \n\
             [Peer]\n\
             PublicKey = {public_key}\n\
             Endpoint = vpn.example.com:51820\n\
             AllowedIPs = 0.0.0.0/0, ::/0\n\
             PersistentKeepalive = 25\n"
        )
    }

    /// Write base configs: two profiles (Admin/lan/vlan99, Guest/guest/vlan101)
    /// with network interfaces. No VPN client or server.
    fn setup_base_configs(dir: &Path) {
        std::fs::write(
            dir.join("startwrt"),
            "\
config profile lan
\toption fullname 'Admin'
\toption interface 'lan'
\toption vlan_tag '99'
\toption access_to_new_profiles '1'

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
config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'

config interface 'lan'
\toption device 'br-lan.99'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.255.0'

config interface 'guest'
\toption device 'br-lan.101'
\toption proto 'static'
\toption ipaddr '192.168.101.1'
\toption netmask '255.255.255.0'
",
        )
        .unwrap();
    }

    /// Write configs that include a VPN client (wg_proton) and a VPN server (wg_guest).
    fn setup_with_vpn_client(dir: &Path) -> (String, String) {
        let client_key = gen_key();
        let peer_pubkey = gen_key();

        std::fs::write(
            dir.join("startwrt"),
            format!(
                "\
config profile lan
\toption fullname 'Admin'
\toption interface 'lan'
\toption vlan_tag '99'
\toption access_to_new_profiles '1'
\toption outbound 'wg_proton'

config profile guest
\toption fullname 'Guest'
\toption interface 'guest'
\toption vlan_tag '101'

config vpn_client wg_proton
\toption interface 'wg_proton'
\toption label 'Proton VPN'
\toption target 'Internet'

config vpn_server 'wg_guest'
\toption interface 'wg_guest'
\toption profile_interface 'guest'
\toption label 'Guest VPN'
\toption listen_port '51820'
\toption endpoint 'vpn.example.com'
"
            ),
        )
        .unwrap();

        std::fs::write(
            dir.join("network"),
            format!(
                "\
config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'

config interface 'lan'
\toption device 'br-lan.99'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.255.0'

config interface 'guest'
\toption device 'br-lan.101'
\toption proto 'static'
\toption ipaddr '192.168.101.1'
\toption netmask '255.255.255.0'

config interface 'wg_proton'
\toption proto 'wireguard'
\toption private_key '{client_key}'
\toption disabled '0'
\toption defaultroute '0'
\toption peerdns '0'
\tlist addresses '10.2.0.2/32'
\tlist dns '10.2.0.1'

config wireguard_wg_proton 'proton_peer0'
\toption public_key '{peer_pubkey}'
\toption endpoint_host 'vpn.example.com'
\toption endpoint_port '51820'
\toption route_allowed_ips '0'
\tlist allowed_ips '0.0.0.0/0'
\tlist allowed_ips '::/0'

config interface 'wg_guest'
\toption proto 'wireguard'
\toption private_key '{}'
\toption listen_port '51820'
\tlist addresses '192.168.101.254/32'
",
                gen_key()
            ),
        )
        .unwrap();

        (client_key, peer_pubkey)
    }

    // === Pure function tests: sanitize_interface_name ===

    #[test]
    fn test_sanitize_simple_label() {
        let name = sanitize_interface_name("proton").unwrap();
        assert_eq!(name, "wg_proton");
    }

    #[test]
    fn test_sanitize_uppercased_label() {
        let name = sanitize_interface_name("ProtonVPN").unwrap();
        assert_eq!(name, "wg_protonvpn");
    }

    #[test]
    fn test_sanitize_label_with_spaces() {
        let name = sanitize_interface_name("My VPN").unwrap();
        assert_eq!(name, "wg_my_vpn");
    }

    #[test]
    fn test_sanitize_label_special_chars() {
        let name = sanitize_interface_name("vpn-us.1").unwrap();
        assert_eq!(name, "wg_vpn_us_1");
    }

    #[test]
    fn test_sanitize_label_max_length() {
        // 12 chars exactly should work (wg_ prefix = 3 + 12 = 15 = IFNAMSIZ-1)
        let name = sanitize_interface_name("abcdefghijkl").unwrap();
        assert_eq!(name, "wg_abcdefghijkl");
    }

    #[test]
    fn test_sanitize_label_too_long() {
        // 13 chars → wg_ + 13 = 16, exceeds IFNAMSIZ-1
        let result = sanitize_interface_name("abcdefghijklm");
        assert!(result.is_err());
    }

    // === Pure function tests: validate_label ===

    #[test]
    fn test_validate_label_valid() {
        assert!(validate_label("Proton VPN").is_ok());
        assert!(validate_label("a").is_ok());
    }

    #[test]
    fn test_validate_label_empty() {
        assert!(validate_label("").is_err());
        assert!(validate_label("   ").is_err());
    }

    #[test]
    fn test_validate_label_control_chars() {
        assert!(validate_label("has\nnewline").is_err());
        assert!(validate_label("has\ttab").is_err());
    }

    // === Pure function tests: validate_cidr ===

    #[test]
    fn test_validate_cidr_valid() {
        assert!(validate_cidr("0.0.0.0/0").is_ok());
        assert!(validate_cidr("10.0.0.0/24").is_ok());
        assert!(validate_cidr("192.168.1.1/32").is_ok());
        assert!(validate_cidr("::/0").is_ok());
        assert!(validate_cidr("fd00::1/128").is_ok());
    }

    #[test]
    fn test_validate_cidr_invalid() {
        assert!(validate_cidr("not-cidr").is_err());
        assert!(validate_cidr("10.0.0.0").is_err()); // no prefix
        assert!(validate_cidr("10.0.0.0/33").is_err()); // prefix too large
        assert!(validate_cidr("::/129").is_err()); // IPv6 prefix too large
        assert!(validate_cidr("999.0.0.0/0").is_err()); // invalid IP
    }

    // === Pure function tests: parse_wireguard_config ===

    #[test]
    fn test_parse_wg_config_minimal() {
        let privkey = gen_key();
        let pubkey = gen_key();
        let conf = wg_conf(&privkey, &pubkey);

        let parsed = parse_wireguard_config(&conf).unwrap();
        assert_eq!(parsed.private_key, privkey);
        assert_eq!(parsed.public_key, pubkey);
        assert_eq!(parsed.addresses, vec!["10.2.0.2/32"]);
        assert_eq!(parsed.dns, vec!["10.2.0.1"]);
        assert_eq!(parsed.endpoint_host.as_deref(), Some("vpn.example.com"));
        assert_eq!(parsed.endpoint_port.as_deref(), Some("51820"));
        assert_eq!(parsed.allowed_ips, vec!["0.0.0.0/0", "::/0"]);
        assert_eq!(parsed.persistent_keepalive.as_deref(), Some("25"));
        assert!(parsed.preshared_key.is_none());
        // wg_conf() omits MTU → inherit default
        assert_eq!(parsed.mtu, None);
    }

    #[test]
    fn test_parse_wg_config_mtu() {
        let privkey = gen_key();
        let pubkey = gen_key();
        let conf = format!(
            "[Interface]\n\
             PrivateKey = {privkey}\n\
             Address = 10.2.0.2/32\n\
             MTU = 1280\n\
             \n\
             [Peer]\n\
             PublicKey = {pubkey}\n\
             Endpoint = vpn.example.com:51820\n\
             AllowedIPs = 0.0.0.0/0\n"
        );
        let parsed = parse_wireguard_config(&conf).unwrap();
        assert_eq!(parsed.mtu, Some(1280));
    }

    #[test]
    fn test_parse_wg_config_commented_mtu_ignored() {
        let privkey = gen_key();
        let pubkey = gen_key();
        // A commented MTU line must NOT be applied (mirrors the cstorm config).
        let conf = format!(
            "[Interface]\n\
             PrivateKey = {privkey}\n\
             Address = 10.2.0.2/32\n\
             #MTU = 1280\n\
             \n\
             [Peer]\n\
             PublicKey = {pubkey}\n\
             Endpoint = vpn.example.com:51820\n\
             AllowedIPs = 0.0.0.0/0\n"
        );
        let parsed = parse_wireguard_config(&conf).unwrap();
        assert_eq!(parsed.mtu, None);
    }

    #[test]
    fn test_parse_wg_config_mtu_out_of_range_rejected() {
        let privkey = gen_key();
        let pubkey = gen_key();
        // "70000" exceeds u16 — must still be rejected as out-of-range, not
        // misreported as "not a number".
        for bad in ["1279", "1501", "0", "70000"] {
            let conf = format!(
                "[Interface]\n\
                 PrivateKey = {privkey}\n\
                 Address = 10.2.0.2/32\n\
                 MTU = {bad}\n\
                 \n\
                 [Peer]\n\
                 PublicKey = {pubkey}\n\
                 AllowedIPs = 0.0.0.0/0\n"
            );
            assert!(parse_wireguard_config(&conf).is_err(), "MTU {bad} should be rejected");
        }
    }

    #[test]
    fn test_parse_wg_config_with_preshared_key() {
        let privkey = gen_key();
        let pubkey = gen_key();
        let psk = gen_key(); // valid 32-byte base64
        let conf = format!(
            "[Interface]\n\
             PrivateKey = {privkey}\n\
             Address = 10.2.0.2/32\n\
             \n\
             [Peer]\n\
             PublicKey = {pubkey}\n\
             PresharedKey = {psk}\n\
             Endpoint = vpn.example.com:51820\n\
             AllowedIPs = 0.0.0.0/0\n"
        );

        let parsed = parse_wireguard_config(&conf).unwrap();
        assert_eq!(parsed.preshared_key.as_deref(), Some(psk.as_str()));
    }

    #[test]
    fn test_parse_wg_config_missing_private_key() {
        let pubkey = gen_key();
        let conf = format!(
            "[Interface]\n\
             Address = 10.2.0.2/32\n\
             \n\
             [Peer]\n\
             PublicKey = {pubkey}\n\
             AllowedIPs = 0.0.0.0/0\n"
        );
        assert!(parse_wireguard_config(&conf).is_err());
    }

    #[test]
    fn test_parse_wg_config_missing_public_key() {
        let privkey = gen_key();
        let conf = format!(
            "[Interface]\n\
             PrivateKey = {privkey}\n\
             Address = 10.2.0.2/32\n\
             \n\
             [Peer]\n\
             AllowedIPs = 0.0.0.0/0\n"
        );
        assert!(parse_wireguard_config(&conf).is_err());
    }

    #[test]
    fn test_parse_wg_config_invalid_private_key() {
        let pubkey = gen_key();
        let conf = format!(
            "[Interface]\n\
             PrivateKey = not-valid-base64-key\n\
             Address = 10.2.0.2/32\n\
             \n\
             [Peer]\n\
             PublicKey = {pubkey}\n\
             AllowedIPs = 0.0.0.0/0\n"
        );
        assert!(parse_wireguard_config(&conf).is_err());
    }

    #[test]
    fn test_parse_wg_config_ipv6_endpoint() {
        let privkey = gen_key();
        let pubkey = gen_key();
        let conf = format!(
            "[Interface]\n\
             PrivateKey = {privkey}\n\
             Address = 10.2.0.2/32\n\
             \n\
             [Peer]\n\
             PublicKey = {pubkey}\n\
             Endpoint = [2001:db8::1]:51820\n\
             AllowedIPs = 0.0.0.0/0\n"
        );

        let parsed = parse_wireguard_config(&conf).unwrap();
        assert_eq!(parsed.endpoint_host.as_deref(), Some("[2001:db8::1]"));
        assert_eq!(parsed.endpoint_port.as_deref(), Some("51820"));
    }

    #[test]
    fn test_parse_wg_config_comments_and_blanks() {
        let privkey = gen_key();
        let pubkey = gen_key();
        let conf = format!(
            "# This is a comment\n\
             \n\
             [Interface]\n\
             PrivateKey = {privkey}\n\
             # Another comment\n\
             Address = 10.2.0.2/32\n\
             \n\
             [Peer]\n\
             PublicKey = {pubkey}\n\
             Endpoint = vpn.example.com:51820\n\
             AllowedIPs = 0.0.0.0/0\n"
        );

        let parsed = parse_wireguard_config(&conf).unwrap();
        assert_eq!(parsed.private_key, privkey);
        assert_eq!(parsed.public_key, pubkey);
    }

    #[test]
    fn test_parse_wg_config_invalid_allowed_ips() {
        let privkey = gen_key();
        let pubkey = gen_key();
        let conf = format!(
            "[Interface]\n\
             PrivateKey = {privkey}\n\
             Address = 10.2.0.2/32\n\
             \n\
             [Peer]\n\
             PublicKey = {pubkey}\n\
             Endpoint = vpn.example.com:51820\n\
             AllowedIPs = not-a-cidr\n"
        );
        assert!(parse_wireguard_config(&conf).is_err());
    }

    #[test]
    fn test_parse_wg_config_invalid_endpoint_port() {
        let privkey = gen_key();
        let pubkey = gen_key();
        let conf = format!(
            "[Interface]\n\
             PrivateKey = {privkey}\n\
             Address = 10.2.0.2/32\n\
             \n\
             [Peer]\n\
             PublicKey = {pubkey}\n\
             Endpoint = vpn.example.com:99999\n\
             AllowedIPs = 0.0.0.0/0\n"
        );
        assert!(parse_wireguard_config(&conf).is_err());
    }

    // === Non-WireGuard rejection (hardening) ===

    #[test]
    fn test_parse_rejects_openvpn_config() {
        // A representative OpenVPN .ovpn (no [Interface]/[Peer] headers). Must be
        // rejected with the OpenVPN-specific message, not "missing PrivateKey".
        let conf = "\
client
dev tun
proto udp
remote vpn.example.com 1194
resolv-retry infinite
nobind
persist-key
persist-tun
remote-cert-tls server
cipher AES-256-GCM
auth-user-pass
<ca>
-----BEGIN CERTIFICATE-----
MIID...snip...
-----END CERTIFICATE-----
</ca>
";
        let err = parse_wireguard_config(conf).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("openvpn"),
            "OpenVPN config should report an OpenVPN-specific error, got: {err}"
        );
    }

    #[test]
    fn test_looks_like_openvpn_ignores_wireguard() {
        // A valid WG config must NOT trip the OpenVPN heuristic.
        let conf = wg_conf(&gen_key(), &gen_key());
        assert!(!looks_like_openvpn(&conf));
        // ...and the OpenVPN markers must be detected on a real .ovpn snippet.
        assert!(looks_like_openvpn("client\ndev tun\nremote host 1194\n"));
    }

    #[test]
    fn test_parse_rejects_missing_interface_header() {
        // Bare key=value lines with no [Interface] header: not WG INI, and not
        // OpenVPN either → the generic "not a WireGuard configuration" error.
        let privkey = gen_key();
        let pubkey = gen_key();
        let conf = format!("PrivateKey = {privkey}\nPublicKey = {pubkey}\n");
        let err = parse_wireguard_config(&conf).unwrap_err();
        assert!(
            err.to_string().contains("[Interface]"),
            "expected a missing-[Interface] error, got: {err}"
        );
    }

    #[test]
    fn test_parse_rejects_missing_peer_header() {
        let privkey = gen_key();
        let conf = format!(
            "[Interface]\n\
             PrivateKey = {privkey}\n\
             Address = 10.2.0.2/32\n"
        );
        assert!(parse_wireguard_config(&conf).is_err());
    }

    #[test]
    fn test_parse_rejects_missing_address() {
        // Parses cleanly but the interface would have no local IP → reject.
        let privkey = gen_key();
        let pubkey = gen_key();
        let conf = format!(
            "[Interface]\n\
             PrivateKey = {privkey}\n\
             \n\
             [Peer]\n\
             PublicKey = {pubkey}\n\
             Endpoint = vpn.example.com:51820\n\
             AllowedIPs = 0.0.0.0/0\n"
        );
        let err = parse_wireguard_config(&conf).unwrap_err();
        assert!(
            err.to_string().contains("Address"),
            "expected a missing-Address error, got: {err}"
        );
    }

    #[test]
    fn test_parse_rejects_missing_endpoint() {
        // No Endpoint → nothing to connect to → reject.
        let privkey = gen_key();
        let pubkey = gen_key();
        let conf = format!(
            "[Interface]\n\
             PrivateKey = {privkey}\n\
             Address = 10.2.0.2/32\n\
             \n\
             [Peer]\n\
             PublicKey = {pubkey}\n\
             AllowedIPs = 0.0.0.0/0\n"
        );
        let err = parse_wireguard_config(&conf).unwrap_err();
        assert!(
            err.to_string().contains("Endpoint"),
            "expected a missing-Endpoint error, got: {err}"
        );
    }

    // === Config helper tests: get_vpn_server_interfaces ===

    #[tokio::test]
    async fn test_get_vpn_server_interfaces_filters_servers() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_client(dir.path());

        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        let servers = get_vpn_server_interfaces(&cfgs);
        assert!(servers.contains("wg_guest"), "should contain VPN server interface");
        assert!(!servers.contains("wg_proton"), "should not contain VPN client interface");
    }

    // === Config helper tests: get_used_by_profiles ===

    #[tokio::test]
    async fn test_get_used_by_profiles_finds_matching() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_client(dir.path());

        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        let used_by = get_used_by_profiles(&cfgs, "wg_proton");
        assert_eq!(used_by, vec!["Admin"]);
    }

    #[tokio::test]
    async fn test_get_used_by_profiles_empty_when_unused() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_client(dir.path());

        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        let used_by = get_used_by_profiles(&cfgs, "wg_nonexistent");
        assert!(used_by.is_empty());
    }

    // === Config helper tests: reset_profiles_using_vpn ===

    #[tokio::test]
    async fn test_reset_profiles_using_vpn() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_client(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        let affected = reset_profiles_using_vpn(&mut cfgs, "wg_proton");
        assert_eq!(affected.len(), 1);
        assert_eq!(affected[0].0, "lan"); // interface
        assert_eq!(affected[0].1, 99); // vlan_tag
        assert_eq!(affected[0].2, std::net::Ipv4Addr::new(192, 168, 1, 1)); // gateway_ip

        // Verify outbound was reset to "wan"
        let used_by = get_used_by_profiles(&cfgs, "wg_proton");
        assert!(used_by.is_empty(), "no profiles should reference wg_proton after reset");
    }

    #[tokio::test]
    async fn test_reset_profiles_noop_when_unused() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        let affected = reset_profiles_using_vpn(&mut cfgs, "wg_nonexistent");
        assert!(affected.is_empty());
    }

    // === Config manipulation tests: set_disabled_option ===

    #[tokio::test]
    async fn test_set_disabled_option_updates_existing() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_client(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        let section = cfgs["network"]
            .sections
            .iter_mut()
            .find(|s| s.name().as_deref() == Some("wg_proton"))
            .unwrap();

        set_disabled_option(section, true, &arena);

        let wg = section.get::<WgInterface>().unwrap();
        assert!(wg.disabled(), "should be disabled after set_disabled_option(true)");
    }

    #[tokio::test]
    async fn test_set_disabled_option_adds_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());

        // Create a WG interface without a disabled option
        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        let privkey = gen_key();
        let parsed = parse_wireguard_config(&wg_conf(&privkey, &gen_key())).unwrap();
        create_wg_client_interface(&mut cfgs, "wg_test", &parsed, &arena).unwrap();

        let section = cfgs["network"]
            .sections
            .iter_mut()
            .find(|s| s.name().as_deref() == Some("wg_test"))
            .unwrap();

        // It already has disabled=0 from create, but let's toggle to 1
        set_disabled_option(section, true, &arena);
        let wg = section.get::<WgInterface>().unwrap();
        assert!(wg.disabled());

        // Toggle back
        set_disabled_option(section, false, &arena);
        let wg = section.get::<WgInterface>().unwrap();
        assert!(!wg.disabled());
    }

    // === Config manipulation tests: create_wg_client_interface ===

    #[tokio::test]
    async fn test_create_wg_client_interface() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());

        let privkey = gen_key();
        let pubkey = gen_key();
        let conf = wg_conf(&privkey, &pubkey);
        let parsed = parse_wireguard_config(&conf).unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        create_wg_client_interface(&mut cfgs, "wg_proton", &parsed, &arena).unwrap();

        let wg = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("wg_proton"))
            .and_then(|s| s.get::<WgInterface>().ok())
            .expect("wg_proton interface should exist");

        assert_eq!(wg.proto, "wireguard");
        assert_eq!(wg.private_key, privkey);
        assert!(!wg.disabled(), "should be enabled by default");
        assert_eq!(wg.addresses, vec!["10.2.0.2/32"]);
    }

    #[tokio::test]
    async fn test_create_wg_client_interface_has_defaultroute_and_peerdns() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());

        let parsed = parse_wireguard_config(&wg_conf(&gen_key(), &gen_key())).unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        create_wg_client_interface(&mut cfgs, "wg_test", &parsed, &arena).unwrap();

        let section = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("wg_test"))
            .expect("wg_test should exist");

        let has_defaultroute_0 = section.lines.iter().any(|l| matches!(l,
            Line::Option { option, value, .. }
            if option.as_str() == "defaultroute" && value.as_str() == "0"));
        assert!(has_defaultroute_0, "should have defaultroute=0");

        let has_peerdns_0 = section.lines.iter().any(|l| matches!(l,
            Line::Option { option, value, .. }
            if option.as_str() == "peerdns" && value.as_str() == "0"));
        assert!(has_peerdns_0, "should have peerdns=0");
    }

    // === Config manipulation tests: create_wg_client_peer ===

    #[tokio::test]
    async fn test_create_wg_client_peer() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());

        let privkey = gen_key();
        let pubkey = gen_key();
        let conf = wg_conf(&privkey, &pubkey);
        let parsed = parse_wireguard_config(&conf).unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        create_wg_client_peer(&mut cfgs, "wg_proton", &parsed, &arena).unwrap();

        let peer_sections: Vec<_> = cfgs["network"]
            .sections
            .iter()
            .filter(|s| s.ty() == "wireguard_wg_proton")
            .collect();

        assert_eq!(peer_sections.len(), 1);

        let peer = &peer_sections[0];

        // Check section name
        assert_eq!(peer.name().as_deref(), Some("proton_peer0"));

        // Check public_key
        let has_pubkey = peer.lines.iter().any(|l| matches!(l,
            Line::Option { option, value, .. }
            if option.as_str() == "public_key" && value.as_str() == pubkey));
        assert!(has_pubkey, "should have correct public_key");

        // Check endpoint_host
        let has_endpoint = peer.lines.iter().any(|l| matches!(l,
            Line::Option { option, value, .. }
            if option.as_str() == "endpoint_host" && value.as_str() == "vpn.example.com"));
        assert!(has_endpoint, "should have endpoint_host");

        // Check endpoint_port
        let has_port = peer.lines.iter().any(|l| matches!(l,
            Line::Option { option, value, .. }
            if option.as_str() == "endpoint_port" && value.as_str() == "51820"));
        assert!(has_port, "should have endpoint_port");

        // Check route_allowed_ips = 0
        let has_no_route = peer.lines.iter().any(|l| matches!(l,
            Line::Option { option, value, .. }
            if option.as_str() == "route_allowed_ips" && value.as_str() == "0"));
        assert!(has_no_route, "should have route_allowed_ips=0");

        // Check allowed_ips list
        let allowed_ips: Vec<String> = peer
            .lines
            .iter()
            .filter_map(|l| match l {
                Line::List { list, item, .. } if list.as_str() == "allowed_ips" => {
                    Some(item.as_str().to_string())
                }
                _ => None,
            })
            .collect();
        assert_eq!(allowed_ips, vec!["0.0.0.0/0", "::/0"]);

        // Check persistent_keepalive
        let has_keepalive = peer.lines.iter().any(|l| matches!(l,
            Line::Option { option, value, .. }
            if option.as_str() == "persistent_keepalive" && value.as_str() == "25"));
        assert!(has_keepalive, "should have persistent_keepalive");
    }

    #[tokio::test]
    async fn test_create_wg_client_peer_without_optional_fields() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());

        let privkey = gen_key();
        let pubkey = gen_key();
        let conf = format!(
            "[Interface]\n\
             PrivateKey = {privkey}\n\
             Address = 10.2.0.2/32\n\
             \n\
             [Peer]\n\
             PublicKey = {pubkey}\n\
             Endpoint = vpn.example.com:51820\n\
             AllowedIPs = 0.0.0.0/0\n"
        );
        let parsed = parse_wireguard_config(&conf).unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        create_wg_client_peer(&mut cfgs, "wg_test", &parsed, &arena).unwrap();

        let peer = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.ty() == "wireguard_wg_test")
            .expect("peer should exist");

        // Endpoint is required (so it's present), but preshared_key and
        // persistent_keepalive are genuinely optional and must be omitted here.
        let has_psk = peer.lines.iter().any(|l| matches!(l,
            Line::Option { option, .. } if option.as_str() == "preshared_key"));
        assert!(!has_psk, "should not have preshared_key");

        let has_keepalive = peer.lines.iter().any(|l| matches!(l,
            Line::Option { option, .. } if option.as_str() == "persistent_keepalive"));
        assert!(!has_keepalive, "should not have persistent_keepalive");
    }

    // === Integration-style tests ===

    #[tokio::test]
    async fn test_full_create_flow() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());

        let privkey = gen_key();
        let pubkey = gen_key();
        let conf = wg_conf(&privkey, &pubkey);
        let parsed = parse_wireguard_config(&conf).unwrap();
        let interface_name = sanitize_interface_name("Proton").unwrap();
        assert_eq!(interface_name, "wg_proton");

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        // No name conflict
        let name_conflict = cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some(interface_name.as_str()));
        assert!(!name_conflict);

        // Create interface, peer, and metadata
        create_wg_client_interface(&mut cfgs, &interface_name, &parsed, &arena).unwrap();
        create_wg_client_peer(&mut cfgs, &interface_name, &parsed, &arena).unwrap();

        let meta = UciVpnClient {
            interface: interface_name.clone(),
            label: "Proton".into(),
            target: "Internet".into(),
        };
        cfgs["startwrt"].append(&meta, Some(&interface_name)).unwrap();

        // Persist and re-read to verify round-trip
        dump_all(dir.path(), cfgs).await.unwrap();

        let arena2 = Arena::new();
        let cfgs2 = parse_all(dir.path(), &arena2, &["network", "startwrt"]).await.unwrap();

        // Verify WG interface
        let wg = cfgs2["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("wg_proton"))
            .and_then(|s| s.get::<WgInterface>().ok())
            .expect("wg_proton should exist after round-trip");
        assert_eq!(wg.proto, "wireguard");
        assert_eq!(wg.private_key, privkey);

        // Verify peer section
        let peers: Vec<_> = cfgs2["network"]
            .sections
            .iter()
            .filter(|s| s.ty() == "wireguard_wg_proton")
            .collect();
        assert_eq!(peers.len(), 1);

        // Verify metadata
        let meta_read = cfgs2["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnClient>().ok())
            .find(|m| m.interface == "wg_proton")
            .expect("vpn_client metadata should exist");
        assert_eq!(meta_read.label, "Proton");
        assert_eq!(meta_read.target, "Internet");
    }

    #[tokio::test]
    async fn test_full_delete_flow() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_client(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        let interface_name = "wg_proton";

        // Reset profiles that reference this VPN
        let affected = reset_profiles_using_vpn(&mut cfgs, interface_name);
        assert_eq!(affected.len(), 1, "Admin profile should be affected");

        // Remove WireGuard interface
        cfgs["network"].sections.retain(|section| {
            if section.name().as_deref() == Some(interface_name) {
                if let Ok(iface) = section.get::<WgInterface>() {
                    if iface.is_wireguard() {
                        return false;
                    }
                }
            }
            true
        });

        // Remove peers
        let peer_type = format!("wireguard_{}", interface_name);
        cfgs["network"]
            .sections
            .retain(|section| section.ty() != peer_type);

        // Remove vpn_client metadata
        cfgs["startwrt"].sections.retain(|section| {
            if let Ok(meta) = section.get::<UciVpnClient>() {
                return meta.interface != interface_name;
            }
            true
        });

        // Verify cleanup
        assert!(!cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some(interface_name)));
        assert_eq!(
            cfgs["network"]
                .sections
                .iter()
                .filter(|s| s.ty() == "wireguard_wg_proton")
                .count(),
            0
        );
        assert!(!cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnClient>().ok())
            .any(|m| m.interface == interface_name));

        // Profiles should survive
        let profile_count = cfgs["startwrt"]
            .sections
            .iter()
            .filter(|s| s.ty() == "profile")
            .count();
        assert_eq!(profile_count, 2, "both profiles should survive VPN deletion");

        // VPN server should survive
        let server_count = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnServer>().ok())
            .count();
        assert_eq!(server_count, 1, "VPN server should survive client deletion");

        // wg_guest interface should survive
        assert!(cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("wg_guest")));
    }

    #[tokio::test]
    async fn test_interface_name_conflict_detection() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_client(dir.path());

        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        // "lan" already exists as a network interface
        let conflict = cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("lan"));
        assert!(conflict, "lan interface should conflict");

        // "wg_proton" already exists as a VPN client interface
        let conflict = cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("wg_proton"));
        assert!(conflict, "wg_proton should conflict");

        // "wg_newvpn" doesn't exist
        let no_conflict = cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("wg_newvpn"));
        assert!(!no_conflict, "wg_newvpn should not conflict");
    }

    #[tokio::test]
    async fn test_label_conflict_detection() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_client(dir.path());

        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        // "Proton VPN" already exists
        let conflict = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnClient>().ok())
            .any(|meta| meta.label == "Proton VPN");
        assert!(conflict, "Proton VPN label should conflict");

        // "New VPN" doesn't exist
        let no_conflict = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnClient>().ok())
            .any(|meta| meta.label == "New VPN");
        assert!(!no_conflict, "New VPN label should not conflict");
    }

    #[tokio::test]
    async fn test_update_metadata() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_client(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        // Update label and target
        let mut found = false;
        for section in &mut cfgs["startwrt"].sections {
            let Ok(mut meta) = section.get::<UciVpnClient>() else { continue };
            if meta.interface != "wg_proton" {
                continue;
            }
            meta.label = "Proton US".into();
            meta.target = "wg_mullvad".into();
            section.set(&meta).unwrap();
            found = true;
            break;
        }
        assert!(found, "should find wg_proton metadata to update");

        // Verify update
        let meta = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnClient>().ok())
            .find(|m| m.interface == "wg_proton")
            .unwrap();
        assert_eq!(meta.label, "Proton US");
        assert_eq!(meta.target, "wg_mullvad");

        // Exactly one vpn_client entry
        let client_count = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnClient>().ok())
            .count();
        assert_eq!(client_count, 1, "should have exactly one vpn_client after update");
    }

    // === VPN chain dependency tests ===

    /// Write configs with two chained VPN clients: Mullvad targets Proton's label.
    fn setup_with_chained_vpns(dir: &Path) {
        let client_key1 = gen_key();
        let peer_pubkey1 = gen_key();
        let client_key2 = gen_key();
        let peer_pubkey2 = gen_key();

        std::fs::write(
            dir.join("startwrt"),
            "\
config profile lan
\toption fullname 'Admin'
\toption interface 'lan'
\toption vlan_tag '99'
\toption access_to_new_profiles '1'

config vpn_client wg_proton
\toption interface 'wg_proton'
\toption label 'Proton VPN'
\toption target 'Internet'

config vpn_client wg_mullvad
\toption interface 'wg_mullvad'
\toption label 'Mullvad'
\toption target 'Proton VPN'
",
        )
        .unwrap();

        std::fs::write(
            dir.join("network"),
            format!(
                "\
config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'

config interface 'lan'
\toption device 'br-lan.99'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.255.0'

config interface 'wg_proton'
\toption proto 'wireguard'
\toption private_key '{client_key1}'
\toption disabled '0'
\toption defaultroute '0'
\toption peerdns '0'
\tlist addresses '10.2.0.2/32'

config wireguard_wg_proton 'proton_peer0'
\toption public_key '{peer_pubkey1}'
\toption endpoint_host 'vpn.example.com'
\toption endpoint_port '51820'
\toption route_allowed_ips '0'
\tlist allowed_ips '0.0.0.0/0'

config interface 'wg_mullvad'
\toption proto 'wireguard'
\toption private_key '{client_key2}'
\toption disabled '0'
\toption defaultroute '0'
\toption peerdns '0'
\tlist addresses '10.3.0.2/32'

config wireguard_wg_mullvad 'mullvad_peer0'
\toption public_key '{peer_pubkey2}'
\toption endpoint_host 'mullvad.example.com'
\toption endpoint_port '51820'
\toption route_allowed_ips '0'
\tlist allowed_ips '0.0.0.0/0'
"
            ),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn test_get_vpn_dependents_finds_matching() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_chained_vpns(dir.path());

        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        let deps = get_vpn_dependents(&cfgs, "Proton VPN");
        assert_eq!(deps, vec!["Mullvad"]);
    }

    #[tokio::test]
    async fn test_get_vpn_dependents_empty_when_no_chain() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_chained_vpns(dir.path());

        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        // Mullvad is not targeted by anyone
        let deps = get_vpn_dependents(&cfgs, "Mullvad");
        assert!(deps.is_empty());
    }

    #[tokio::test]
    async fn test_delete_blocked_when_vpn_has_dependents() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_chained_vpns(dir.path());

        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        // Proton VPN has Mullvad as a dependent — should be blocked
        let this_label = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnClient>().ok())
            .find(|meta| meta.interface == "wg_proton")
            .map(|meta| meta.label.clone())
            .unwrap();

        let dependents = get_vpn_dependents(&cfgs, &this_label);
        assert!(!dependents.is_empty(), "Proton VPN should have dependents");
        assert_eq!(dependents, vec!["Mullvad"]);
    }

    #[tokio::test]
    async fn test_delete_allowed_when_no_dependents() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_chained_vpns(dir.path());

        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        // Mullvad has no dependents — should be allowed
        let this_label = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnClient>().ok())
            .find(|meta| meta.interface == "wg_mullvad")
            .map(|meta| meta.label.clone())
            .unwrap();

        let dependents = get_vpn_dependents(&cfgs, &this_label);
        assert!(dependents.is_empty(), "Mullvad should have no dependents");
    }

    #[tokio::test]
    async fn test_disable_blocked_when_vpn_has_dependents() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_chained_vpns(dir.path());

        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        // Proton VPN has Mullvad as a dependent — disabling should be blocked
        let this_label = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnClient>().ok())
            .find(|meta| meta.interface == "wg_proton")
            .map(|meta| meta.label.clone())
            .unwrap();

        let dependents = get_vpn_dependents(&cfgs, &this_label);
        assert!(!dependents.is_empty(), "should block disable for VPN with dependents");
    }

    #[tokio::test]
    async fn test_disable_allowed_when_no_dependents() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_chained_vpns(dir.path());

        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        // Mullvad has no dependents — disabling should be allowed
        let this_label = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnClient>().ok())
            .find(|meta| meta.interface == "wg_mullvad")
            .map(|meta| meta.label.clone())
            .unwrap();

        let dependents = get_vpn_dependents(&cfgs, &this_label);
        assert!(dependents.is_empty(), "should allow disable for VPN without dependents");
    }

    #[tokio::test]
    async fn test_rename_cascades_to_dependents() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_chained_vpns(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        let old_label = "Proton VPN";
        let new_label = "Proton US";

        // Update the primary VPN's label
        for section in &mut cfgs["startwrt"].sections {
            let Ok(mut meta) = section.get::<UciVpnClient>() else { continue };
            if meta.interface != "wg_proton" { continue }
            meta.label = new_label.to_string();
            section.set(&meta).unwrap();
            break;
        }

        // Cascade: update dependents' target fields
        for section in &mut cfgs["startwrt"].sections {
            let Ok(mut meta) = section.get::<UciVpnClient>() else { continue };
            if meta.target == old_label {
                meta.target = new_label.to_string();
                let _ = section.set(&meta);
            }
        }

        // Verify Mullvad now targets "Proton US"
        let mullvad = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnClient>().ok())
            .find(|m| m.interface == "wg_mullvad")
            .unwrap();
        assert_eq!(mullvad.target, "Proton US", "dependent's target should be updated");

        // Verify Proton's label was updated
        let proton = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnClient>().ok())
            .find(|m| m.interface == "wg_proton")
            .unwrap();
        assert_eq!(proton.label, "Proton US");
    }

    // === DNS cleanup on VPN delete ===

    #[tokio::test]
    async fn test_delete_cleans_up_dns_forwarding() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_client(dir.path());

        // Add firewall config (needed for parse_all) and dhcp config with
        // a per-profile dnsmasq section simulating DNS forwarding through the VPN
        std::fs::write(
            dir.path().join("firewall"),
            "\
config zone 'wan'
\toption name 'wan'
\tlist network 'wan'
\tlist network 'wg_proton'
",
        )
        .unwrap();

        std::fs::write(
            dir.path().join("dhcp"),
            "\
config dnsmasq
\tlist notinterface 'loopback'
\tlist notinterface 'lan'

config dnsmasq 'dns_lan'
\tlist server '10.2.0.1@wg_proton'
\toption noresolv '1'
\tlist listen_address '192.168.1.1'
\tlist notinterface 'loopback'
",
        )
        .unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // Verify precondition: dns_lan section exists
        assert!(
            cfgs["dhcp"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("dns_lan")),
            "dns_lan should exist before delete"
        );

        // Verify precondition: main dnsmasq has notinterface 'lan'
        let has_notinterface_lan = cfgs["dhcp"].sections.iter().any(|s| {
            s.ty() == "dnsmasq"
                && !s.name().map(|n| n.starts_with("dns_")).unwrap_or(false)
                && s.lines.iter().any(|l| {
                    matches!(l, Line::List { list, item, .. }
                        if list.as_str() == "notinterface" && item.as_str() == "lan")
                })
        });
        assert!(has_notinterface_lan, "main dnsmasq should have notinterface 'lan'");

        // Reset profiles and run DNS cleanup (same as delete() does)
        let affected = reset_profiles_using_vpn(&mut cfgs, "wg_proton");
        assert_eq!(affected.len(), 1);
        assert_eq!(affected[0].0, "lan");
        assert_eq!(affected[0].2, std::net::Ipv4Addr::new(192, 168, 1, 1));

        for (profile_interface, vlan_tag, gateway_ip) in &affected {
            let profile = crate::profiles::Profile {
                id: crate::profiles::ProfileId {
                    fullname: String::new(),
                    interface: profile_interface.clone(),
                    vlan_tag: *vlan_tag,
                },
                gateway_ip: *gateway_ip,
                outbound: "wan".to_string(),
                lan_access: crate::profiles::LanAccess::SameProfile,
                wan_access: crate::profiles::WanAccess::None,
                dns_override: Vec::new(),
                dns_source: String::new(),
                access_to_new_profiles: false,
                owns_lan: false,
            };
            crate::profiles::rewrite_dns_forwarding(&mut cfgs, &profile).unwrap();
        }

        // dns_lan section should be removed
        assert!(
            !cfgs["dhcp"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("dns_lan")),
            "dns_lan should be removed after delete"
        );

        // notinterface 'lan' should be removed from main dnsmasq
        let still_has_notinterface_lan = cfgs["dhcp"].sections.iter().any(|s| {
            s.ty() == "dnsmasq"
                && !s.name().map(|n| n.starts_with("dns_")).unwrap_or(false)
                && s.lines.iter().any(|l| {
                    matches!(l, Line::List { list, item, .. }
                        if list.as_str() == "notinterface" && item.as_str() == "lan")
                })
        });
        assert!(
            !still_has_notinterface_lan,
            "main dnsmasq should not have notinterface 'lan' after cleanup"
        );
    }

    // === VPN chain route tests ===

    /// Helper: set up two chained VPNs with IP endpoints (sto → dal → Internet)
    fn setup_chain_with_ip_endpoints(dir: &Path) {
        let key_dal = gen_key();
        let pub_dal = gen_key();
        let key_sto = gen_key();
        let pub_sto = gen_key();

        std::fs::write(
            dir.join("startwrt"),
            "\
config profile lan
\toption fullname 'Admin'
\toption interface 'lan'
\toption vlan_tag '99'
\toption access_to_new_profiles '1'

config vpn_client wg_mullvad_dal
\toption interface 'wg_mullvad_dal'
\toption label 'Mullvad Dallas'
\toption target 'Internet'

config vpn_client wg_mullvad_sto
\toption interface 'wg_mullvad_sto'
\toption label 'Mullvad Stockholm'
\toption target 'Mullvad Dallas'
",
        )
        .unwrap();

        std::fs::write(
            dir.join("network"),
            format!(
                "\
config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'

config interface 'lan'
\toption device 'br-lan.99'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.255.0'

config interface 'wg_mullvad_dal'
\toption proto 'wireguard'
\toption private_key '{key_dal}'
\toption disabled '0'
\toption defaultroute '0'
\toption peerdns '0'
\tlist addresses '10.64.0.1/32'

config wireguard_wg_mullvad_dal 'dal_peer0'
\toption public_key '{pub_dal}'
\toption endpoint_host '185.213.154.68'
\toption endpoint_port '51820'
\toption route_allowed_ips '0'
\tlist allowed_ips '0.0.0.0/0'

config interface 'wg_mullvad_sto'
\toption proto 'wireguard'
\toption private_key '{key_sto}'
\toption disabled '0'
\toption defaultroute '0'
\toption peerdns '0'
\tlist addresses '10.65.0.1/32'

config wireguard_wg_mullvad_sto 'sto_peer0'
\toption public_key '{pub_sto}'
\toption endpoint_host '185.65.135.70'
\toption endpoint_port '51820'
\toption route_allowed_ips '0'
\tlist allowed_ips '0.0.0.0/0'
"
            ),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn test_rewrite_chain_routes_creates_route() {
        let dir = tempfile::tempdir().unwrap();
        setup_chain_with_ip_endpoints(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        rewrite_vpn_chain_routes(&mut cfgs).unwrap();

        // sto targets dal, so vcr_wg_mullvad_sto should route sto's endpoint through dal
        let route = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("vcr_wg_mullvad_sto"))
            .expect("vcr_wg_mullvad_sto route should exist");

        let route_data = route.get::<uciedit::openwrt::NetworkRoute>().unwrap();
        assert_eq!(route_data.interface, "wg_mullvad_dal");
        assert_eq!(route_data.target, "185.65.135.70/32");
        assert!(route_data.table.is_none(), "should be in main routing table");

        // dal targets Internet, so no vcr_wg_mullvad_dal route
        assert!(
            cfgs["network"]
                .sections
                .iter()
                .find(|s| s.name().as_deref() == Some("vcr_wg_mullvad_dal"))
                .is_none(),
            "dal targets Internet — no chain route needed"
        );
    }

    #[tokio::test]
    async fn test_rewrite_chain_routes_skips_hostname() {
        let dir = tempfile::tempdir().unwrap();
        let key1 = gen_key();
        let pub1 = gen_key();
        let key2 = gen_key();
        let pub2 = gen_key();

        std::fs::write(
            dir.path().join("startwrt"),
            "\
config vpn_client wg_a
\toption interface 'wg_a'
\toption label 'VPN A'
\toption target 'Internet'

config vpn_client wg_b
\toption interface 'wg_b'
\toption label 'VPN B'
\toption target 'VPN A'
",
        )
        .unwrap();

        std::fs::write(
            dir.path().join("network"),
            format!(
                "\
config interface 'wg_a'
\toption proto 'wireguard'
\toption private_key '{key1}'

config wireguard_wg_a 'a_peer0'
\toption public_key '{pub1}'
\toption endpoint_host '1.2.3.4'
\toption endpoint_port '51820'

config interface 'wg_b'
\toption proto 'wireguard'
\toption private_key '{key2}'

config wireguard_wg_b 'b_peer0'
\toption public_key '{pub2}'
\toption endpoint_host 'vpn.example.com'
\toption endpoint_port '51820'
"
            ),
        )
        .unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        rewrite_vpn_chain_routes(&mut cfgs).unwrap();

        // wg_b has a hostname endpoint — no route should be created
        assert!(
            cfgs["network"]
                .sections
                .iter()
                .find(|s| s.name().as_deref() == Some("vcr_wg_b"))
                .is_none(),
            "hostname endpoints should be skipped"
        );
    }

    #[tokio::test]
    async fn test_rewrite_chain_routes_removes_stale() {
        let dir = tempfile::tempdir().unwrap();
        setup_chain_with_ip_endpoints(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        // First call creates the route
        rewrite_vpn_chain_routes(&mut cfgs).unwrap();
        assert!(
            cfgs["network"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("vcr_wg_mullvad_sto")),
            "route should exist after first call"
        );

        // Change sto's target to Internet
        for section in &mut cfgs["startwrt"].sections {
            let Ok(mut meta) = section.get::<UciVpnClient>() else { continue };
            if meta.interface == "wg_mullvad_sto" {
                meta.target = "Internet".to_string();
                section.set(&meta).unwrap();
                break;
            }
        }

        // Second call should remove the stale route
        rewrite_vpn_chain_routes(&mut cfgs).unwrap();
        assert!(
            cfgs["network"]
                .sections
                .iter()
                .find(|s| s.name().as_deref() == Some("vcr_wg_mullvad_sto"))
                .is_none(),
            "stale route should be removed after target changed to Internet"
        );
    }

    #[tokio::test]
    async fn test_rewrite_chain_routes_multihop() {
        let dir = tempfile::tempdir().unwrap();
        let key_a = gen_key();
        let pub_a = gen_key();
        let key_b = gen_key();
        let pub_b = gen_key();
        let key_c = gen_key();
        let pub_c = gen_key();

        std::fs::write(
            dir.path().join("startwrt"),
            "\
config vpn_client wg_c
\toption interface 'wg_c'
\toption label 'VPN C'
\toption target 'Internet'

config vpn_client wg_b
\toption interface 'wg_b'
\toption label 'VPN B'
\toption target 'VPN C'

config vpn_client wg_a
\toption interface 'wg_a'
\toption label 'VPN A'
\toption target 'VPN B'
",
        )
        .unwrap();

        std::fs::write(
            dir.path().join("network"),
            format!(
                "\
config interface 'wg_a'
\toption proto 'wireguard'
\toption private_key '{key_a}'

config wireguard_wg_a 'a_peer0'
\toption public_key '{pub_a}'
\toption endpoint_host '1.1.1.1'
\toption endpoint_port '51820'

config interface 'wg_b'
\toption proto 'wireguard'
\toption private_key '{key_b}'

config wireguard_wg_b 'b_peer0'
\toption public_key '{pub_b}'
\toption endpoint_host '2.2.2.2'
\toption endpoint_port '51820'

config interface 'wg_c'
\toption proto 'wireguard'
\toption private_key '{key_c}'

config wireguard_wg_c 'c_peer0'
\toption public_key '{pub_c}'
\toption endpoint_host '3.3.3.3'
\toption endpoint_port '51820'
"
            ),
        )
        .unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        rewrite_vpn_chain_routes(&mut cfgs).unwrap();

        // A → B: A's endpoint (1.1.1.1) routed through B's interface
        let route_a = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("vcr_wg_a"))
            .expect("vcr_wg_a should exist");
        let rd_a = route_a.get::<uciedit::openwrt::NetworkRoute>().unwrap();
        assert_eq!(rd_a.interface, "wg_b");
        assert_eq!(rd_a.target, "1.1.1.1/32");

        // B → C: B's endpoint (2.2.2.2) routed through C's interface
        let route_b = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("vcr_wg_b"))
            .expect("vcr_wg_b should exist");
        let rd_b = route_b.get::<uciedit::openwrt::NetworkRoute>().unwrap();
        assert_eq!(rd_b.interface, "wg_c");
        assert_eq!(rd_b.target, "2.2.2.2/32");

        // C → Internet: no chain route for C
        assert!(
            cfgs["network"]
                .sections
                .iter()
                .find(|s| s.name().as_deref() == Some("vcr_wg_c"))
                .is_none(),
            "C targets Internet — no chain route"
        );
    }

    #[tokio::test]
    async fn test_rewrite_chain_routes_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        setup_chain_with_ip_endpoints(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        rewrite_vpn_chain_routes(&mut cfgs).unwrap();
        rewrite_vpn_chain_routes(&mut cfgs).unwrap();

        // Should have exactly one vcr_* route
        let vcr_count = cfgs["network"]
            .sections
            .iter()
            .filter(|s| s.name().map(|n| n.starts_with("vcr_")).unwrap_or(false))
            .count();
        assert_eq!(vcr_count, 1, "idempotent: should have exactly one chain route");

        let route = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("vcr_wg_mullvad_sto"))
            .expect("vcr_wg_mullvad_sto should still exist");
        let rd = route.get::<uciedit::openwrt::NetworkRoute>().unwrap();
        assert_eq!(rd.interface, "wg_mullvad_dal");
        assert_eq!(rd.target, "185.65.135.70/32");
    }

    // === validate_target tests ===

    fn setup_vpn_chain_configs(dir: &Path, clients: &[(&str, &str, &str)]) {
        let mut startwrt = String::new();
        for (iface, label, target) in clients {
            startwrt.push_str(&format!(
                "config vpn_client '{iface}'\n\
                 \toption interface '{iface}'\n\
                 \toption label '{label}'\n\
                 \toption target '{target}'\n\n"
            ));
        }
        std::fs::write(dir.join("startwrt"), startwrt).unwrap();
    }

    #[tokio::test]
    async fn test_validate_target_accepts_internet() {
        let dir = tempfile::tempdir().unwrap();
        setup_vpn_chain_configs(dir.path(), &[]);
        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["startwrt"]).await.unwrap();
        assert!(validate_target(&cfgs, "Internet", "MyVPN").is_ok());
    }

    #[tokio::test]
    async fn test_validate_target_rejects_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        setup_vpn_chain_configs(dir.path(), &[("wg_a", "A", "Internet")]);
        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["startwrt"]).await.unwrap();
        let err = validate_target(&cfgs, "DoesNotExist", "A").unwrap_err();
        assert!(
            matches!(err.kind, ErrorKind::InvalidValue),
            "expected InvalidValue, got: {:?}",
            err.kind
        );
    }

    #[tokio::test]
    async fn test_validate_target_rejects_cycle() {
        let dir = tempfile::tempdir().unwrap();
        // A→B, B→Internet. Now validate B targeting A → cycle
        setup_vpn_chain_configs(
            dir.path(),
            &[("wg_a", "A", "B"), ("wg_b", "B", "Internet")],
        );
        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["startwrt"]).await.unwrap();
        let err = validate_target(&cfgs, "A", "B").unwrap_err();
        assert!(
            matches!(err.kind, ErrorKind::VpnChainCycle),
            "expected VpnChainCycle, got: {:?}",
            err.kind
        );
    }

    #[tokio::test]
    async fn test_validate_target_rejects_longer_cycle() {
        let dir = tempfile::tempdir().unwrap();
        // A→B→C→Internet. Validate C targeting A → cycle
        setup_vpn_chain_configs(
            dir.path(),
            &[
                ("wg_a", "A", "B"),
                ("wg_b", "B", "C"),
                ("wg_c", "C", "Internet"),
            ],
        );
        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["startwrt"]).await.unwrap();
        let err = validate_target(&cfgs, "A", "C").unwrap_err();
        assert!(
            matches!(err.kind, ErrorKind::VpnChainCycle),
            "expected VpnChainCycle, got: {:?}",
            err.kind
        );
    }

    #[tokio::test]
    async fn test_validate_target_accepts_valid_chain() {
        let dir = tempfile::tempdir().unwrap();
        // A→B→Internet. New VPN "C" targeting A is valid (C→A→B→Internet)
        setup_vpn_chain_configs(
            dir.path(),
            &[("wg_a", "A", "B"), ("wg_b", "B", "Internet")],
        );
        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["startwrt"]).await.unwrap();
        assert!(validate_target(&cfgs, "A", "C").is_ok());
    }

    // === IPv6 chain endpoint tests ===

    /// Chain of two VPNs where the upstream peer's endpoint is an IPv6 literal.
    fn setup_chain_with_ipv6_endpoint(dir: &Path) {
        let key_a = gen_key();
        let pub_a = gen_key();
        let key_b = gen_key();
        let pub_b = gen_key();

        std::fs::write(
            dir.join("startwrt"),
            "\
config vpn_client wg_upstream
\toption interface 'wg_upstream'
\toption label 'Upstream'
\toption target 'Internet'

config vpn_client wg_inner
\toption interface 'wg_inner'
\toption label 'Inner'
\toption target 'Upstream'
",
        )
        .unwrap();

        std::fs::write(
            dir.join("network"),
            format!(
                "\
config interface 'wg_upstream'
\toption proto 'wireguard'
\toption private_key '{key_a}'

config wireguard_wg_upstream 'up_peer0'
\toption public_key '{pub_a}'
\toption endpoint_host '1.2.3.4'
\toption endpoint_port '51820'

config interface 'wg_inner'
\toption proto 'wireguard'
\toption private_key '{key_b}'

config wireguard_wg_inner 'in_peer0'
\toption public_key '{pub_b}'
\toption endpoint_host '[2001:db8::1]'
\toption endpoint_port '51820'
"
            ),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn test_get_peer_endpoint_host_strips_brackets() {
        let dir = tempfile::tempdir().unwrap();
        setup_chain_with_ipv6_endpoint(dir.path());

        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        assert_eq!(
            get_peer_endpoint_host(&cfgs, "wg_inner").as_deref(),
            Some("2001:db8::1"),
            "brackets should be stripped from IPv6 literal"
        );
        assert_eq!(
            get_peer_endpoint_host(&cfgs, "wg_upstream").as_deref(),
            Some("1.2.3.4"),
            "IPv4 endpoint pass-through unchanged"
        );
    }

    #[tokio::test]
    async fn test_rewrite_chain_routes_ipv6_endpoint_emits_route6() {
        let dir = tempfile::tempdir().unwrap();
        setup_chain_with_ipv6_endpoint(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        rewrite_vpn_chain_routes(&mut cfgs).unwrap();

        // wg_inner has an IPv6 endpoint and targets wg_upstream → route6 /128
        let route_section = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("vcr_wg_inner"))
            .expect("vcr_wg_inner should exist for IPv6-endpoint chain hop");
        assert_eq!(
            route_section.ty(),
            "route6",
            "IPv6 endpoint must produce a route6 section, not route"
        );
        let route_data = route_section
            .get::<uciedit::openwrt::NetworkRoute6>()
            .unwrap();
        assert_eq!(route_data.interface, "wg_upstream");
        assert_eq!(route_data.target, "2001:db8::1/128");
        assert!(route_data.table.is_none(), "chain routes live in main table");

        // wg_upstream targets Internet → no chain route
        assert!(
            !cfgs["network"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("vcr_wg_upstream")),
            "Internet-targeted VPN should not get a chain route"
        );
    }

    #[tokio::test]
    async fn test_rewrite_chain_routes_idempotent_v6() {
        let dir = tempfile::tempdir().unwrap();
        setup_chain_with_ipv6_endpoint(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        rewrite_vpn_chain_routes(&mut cfgs).unwrap();
        rewrite_vpn_chain_routes(&mut cfgs).unwrap();

        // Exactly one route6 vcr_* section after two calls.
        let vcr_count = cfgs["network"]
            .sections
            .iter()
            .filter(|s| s.name().map(|n| n.starts_with("vcr_")).unwrap_or(false))
            .count();
        assert_eq!(vcr_count, 1, "idempotent: one route6 vcr_* after two calls");
    }

    #[tokio::test]
    async fn test_list_reports_supports_ipv6() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_client(dir.path());

        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"]).await.unwrap();

        // Replicate the list() filter inline so we don't need to spin up the
        // full ServerContext just to read addresses+metadata.
        let server_interfaces = get_vpn_server_interfaces(&cfgs);
        let entries: Vec<OutboundVpn> = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|section| section.get::<UciVpnClient>().ok())
            .filter_map(|meta| {
                let wg = cfgs["network"]
                    .sections
                    .iter()
                    .find(|s| s.name().as_deref() == Some(meta.interface.as_str()))
                    .and_then(|s| s.get::<WgInterface>().ok())
                    .filter(|w| w.is_wireguard())?;
                if server_interfaces.contains(&meta.interface) {
                    return None;
                }
                Some(OutboundVpn {
                    id: meta.interface.clone(),
                    label: meta.label.clone(),
                    target: meta.target.clone(),
                    enabled: !wg.disabled(),
                    used_by: vec![],
                    supports_ipv6: wg.addresses.iter().any(|a| a.contains(':')),
                    mtu: wg.mtu,
                })
            })
            .collect();

        // setup_with_vpn_client gives wg_proton only an IPv4 /32 address.
        let proton = entries.iter().find(|e| e.id == "wg_proton").expect("wg_proton listed");
        assert!(!proton.supports_ipv6, "v4-only VPN must report supports_ipv6=false");
    }
}
