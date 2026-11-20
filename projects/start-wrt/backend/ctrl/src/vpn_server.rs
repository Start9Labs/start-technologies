use std::collections::{BTreeMap, BTreeSet};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::Path;

use clap::Parser;
use rpc_toolkit::{from_fn_async_local, HandlerExt, ParentHandler};
use serde::{Deserialize, Serialize};
use uciedit::openwrt::{NetworkGlobals, NetworkInterface, NetworkRoute, NetworkRule, NetworkRule6};
use uciedit::{
    dump_all, parse_all, Arena, Configs, Line, LineComment, Section, Token, TypedSection,
};

use crate::prelude::*;
use crate::profiles::{self, reload_system};
use crate::utils::{DeserializeStdin, HandlerExtSerde};
use crate::wg::{generate_psk, Base64, WgKey};
use crate::{CliContext, CtrlContext, Error, ErrorKind, ServerContext};

// === IP Allocation Constants ===
/// The server's address inside its profile's /24, kept while legacy peers use it.
const VPN_SERVER_HOST_OCTET: u8 = 254;
/// The router's host in a peer subnet. StartOS peers send DNS updates there.
const PEER_SUBNET_ROUTER_HOST: u8 = 1;
const PEER_HOST_FIRST: u8 = 2;
const PEER_HOST_LAST: u8 = 254;

/// WireGuard interface section in /etc/config/network
#[derive(Debug, TypedSection)]
#[uci(ty = "interface")]
pub struct WgInterface {
    #[uci(rename = "proto")]
    pub proto: String,
    pub private_key: String,
    #[uci(default)]
    pub listen_port: Option<u16>,
    #[uci(default)]
    pub addresses: Vec<String>,
    #[uci(default)]
    pub disabled: Option<String>,
    /// Interface MTU. When unset, wireguard.sh leaves the kernel default
    /// (1420 on a clean 1500-byte path).
    #[uci(default)]
    pub mtu: Option<u16>,
}

impl WgInterface {
    pub fn is_wireguard(&self) -> bool {
        self.proto == "wireguard"
    }

    pub fn disabled(&self) -> bool {
        self.disabled.as_deref() == Some("1")
    }
}

/// VPN server metadata stored in /etc/config/startwrt
#[derive(Debug, TypedSection)]
#[uci(ty = "vpn_server")]
pub struct UciVpnServer {
    /// WireGuard interface name (e.g., "wg_lan")
    pub interface: String,
    /// Profile interface name (e.g., "lan")
    pub profile_interface: String,
    /// Human-readable label
    pub label: String,
    /// Listen port for WireGuard
    pub listen_port: u16,
    /// Public endpoint address for client connections (hostname or IP)
    pub endpoint: String,
    /// The `/24` in the LAN block that new peers are addressed from.
    #[uci(default)]
    pub peer_subnet: Option<String>,
}

// === Public API Types ===

/// Peer configuration for VPN server
#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
pub struct VpnServerPeer {
    /// Human-readable name for the peer
    pub name: String,
    /// Assigned IP address within the server's peer subnet (auto-allocated if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<Ipv4Addr>,
    /// Public key. If not provided when adding a peer, a key pair will be generated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    /// Pre-shared key (auto-generated if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preshared_key: Option<String>,
    /// Route all traffic (LAN + WAN) through the tunnel. Default (false/absent) = split tunnel (LAN only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_all: Option<bool>,
    /// May publish DNS records into the router's resolver. Default (false/absent) = denied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_dns_injection: Option<bool>,
    /// Listed only: the address sits in the profile's own subnet, from before peer
    /// subnets. Reissuing the config moves it. Ignored on add.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_address: Option<bool>,
}
rpc_toolkit::reflect_ts!(VpnServerPeer);

/// VPN server configuration returned by list (excludes sensitive data)
#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
pub struct VpnServer {
    /// Profile interface name this VPN server is associated with (e.g., "lan", "guest")
    pub profile: String,
    /// Human-readable label for the VPN server
    pub label: String,
    /// Whether the VPN server is enabled (listening)
    pub enabled: bool,
    /// Listen port for WireGuard connections
    pub listen_port: u16,
    /// Public endpoint address for inbound client connections (hostname or IP)
    pub endpoint: String,
    /// Server's public key
    pub public_key: String,
    /// The router's address in the peer subnet; without one, its address in the profile's subnet
    pub server_address: Ipv4Addr,
    /// The `/24` new peers are addressed from. Absent until the server is saved or gains a peer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_subnet: Option<String>,
    /// Connected peers
    pub peers: Vec<VpnServerPeer>,
}
rpc_toolkit::reflect_ts!(VpnServer);

/// VPN server configuration for set/create requests
#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
pub struct VpnServerConfig {
    /// Human-readable label for the VPN server
    pub label: String,
    /// Whether the VPN server is enabled (listening)
    pub enabled: bool,
    /// Listen port for WireGuard connections
    pub listen_port: u16,
    /// Public endpoint address for inbound client connections (hostname or IP)
    pub endpoint: String,
    /// Private key. If not provided, a new key will be generated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
}
rpc_toolkit::reflect_ts!(VpnServerConfig);

/// Response containing all VPN servers
#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
pub struct VpnServers {
    pub servers: Vec<VpnServer>,
}
rpc_toolkit::reflect_ts!(VpnServers);

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
pub struct SetArgs {
    /// Profile interface name (e.g., "lan", "guest")
    pub profile: String,
    /// VPN server configuration
    pub config: VpnServerConfig,
}
rpc_toolkit::reflect_ts!(SetArgs);

#[derive(Debug, Clone, Serialize, Deserialize, Parser, visit_rs::VisitFields)]
pub struct DeleteArgs {
    /// Profile interface name
    #[clap(short, long)]
    pub profile: String,
}
rpc_toolkit::reflect_ts!(DeleteArgs);

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
pub struct PeerAddArgs {
    /// Profile interface name (e.g., "lan", "guest")
    pub profile: String,
    /// Peer configuration
    pub peer: VpnServerPeer,
}
rpc_toolkit::reflect_ts!(PeerAddArgs);

#[derive(Debug, Clone, Serialize, Deserialize, Parser, visit_rs::VisitFields)]
pub struct PeerDeleteArgs {
    /// Profile interface name
    #[clap(short, long)]
    pub profile: String,
    /// Public key of the peer to delete
    #[clap(long)]
    pub public_key: String,
}
rpc_toolkit::reflect_ts!(PeerDeleteArgs);

#[derive(Debug, Clone, Serialize, Deserialize, Parser, visit_rs::VisitFields)]
pub struct PeerRenameArgs {
    /// Profile interface name
    #[clap(short, long)]
    pub profile: String,
    /// Public key of the peer to rename
    #[clap(long)]
    pub public_key: String,
    /// New human-readable name for the peer
    #[clap(long)]
    pub name: String,
}
rpc_toolkit::reflect_ts!(PeerRenameArgs);

#[derive(Debug, Clone, Serialize, Deserialize, Parser, visit_rs::VisitFields)]
pub struct PeerSetDnsInjectionArgs {
    /// Profile interface name
    #[clap(short, long)]
    pub profile: String,
    /// Public key of the peer
    #[clap(long)]
    pub public_key: String,
    /// Grant rather than revoke the permission
    #[clap(long)]
    pub allow: bool,
}
rpc_toolkit::reflect_ts!(PeerSetDnsInjectionArgs);

#[derive(Debug, Clone, Serialize, Deserialize, Parser, visit_rs::VisitFields)]
pub struct PeerConfigArgs {
    /// Profile interface name
    #[clap(short, long)]
    pub profile: String,
    /// Public key of the peer
    #[clap(long)]
    pub public_key: String,
}
rpc_toolkit::reflect_ts!(PeerConfigArgs);

/// A peer's client configuration, without its keys
#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
pub struct PeerConfigResponse {
    /// WireGuard client configuration with placeholders for the private and preshared keys
    pub client_config: String,
}
rpc_toolkit::reflect_ts!(PeerConfigResponse);

#[derive(Debug, Clone, Serialize, Deserialize, Parser, visit_rs::VisitFields)]
pub struct PeerReissueArgs {
    /// Profile interface name
    #[clap(short, long)]
    pub profile: String,
    /// Public key of the peer to replace
    #[clap(long)]
    pub public_key: String,
}
rpc_toolkit::reflect_ts!(PeerReissueArgs);

/// Response from adding a peer
#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
pub struct PeerAddResponse {
    /// WireGuard client configuration. Its private key is a placeholder when the request supplied the public key.
    pub client_config: String,
    /// The public key of the peer
    pub public_key: String,
    /// The assigned IP address
    pub ip: Ipv4Addr,
}
rpc_toolkit::reflect_ts!(PeerAddResponse);

/// Rejected by `wg` as a key.
const CLIENT_PRIVATE_KEY_PLACEHOLDER: &str = "<PRIVATE_KEY>";
/// Rejected by `wg` as a key.
const CLIENT_PRESHARED_KEY_PLACEHOLDER: &str = "<PRESHARED_KEY>";

struct ClientConfig<'a> {
    name: &'a str,
    /// `None` renders the placeholder.
    private_key: Option<&'a str>,
    address: &'a str,
    dns: Ipv4Addr,
    server_public_key: &'a str,
    /// `None` renders the placeholder.
    preshared_key: Option<&'a str>,
    endpoint: &'a str,
    port: u16,
    allowed_ips: &'a str,
}

impl ClientConfig<'_> {
    fn render(&self) -> String {
        format!(
            "# WireGuard client configuration for {name}\n\
             # Generated by StartWRT\n\
             \n\
             [Interface]\n\
             PrivateKey = {privkey}\n\
             Address = {address}\n\
             DNS = {dns}\n\
             \n\
             [Peer]\n\
             PublicKey = {server_pubkey}\n\
             PresharedKey = {psk}\n\
             Endpoint = {endpoint}:{port}\n\
             AllowedIPs = {allowed_ips}\n\
             PersistentKeepalive = 25\n",
            name = self.name,
            privkey = self.private_key.unwrap_or(CLIENT_PRIVATE_KEY_PLACEHOLDER),
            address = self.address,
            dns = self.dns,
            server_pubkey = self.server_public_key,
            psk = self
                .preshared_key
                .unwrap_or(CLIENT_PRESHARED_KEY_PLACEHOLDER),
            endpoint = self.endpoint,
            port = self.port,
            allowed_ips = self.allowed_ips,
        )
    }
}

// === API Handler ===

pub fn vpn_server<C: CtrlContext>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async_local(list)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set",
            from_fn_async_local(set)
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
            "peer-add",
            from_fn_async_local(peer_add)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "peer-delete",
            from_fn_async_local(peer_delete)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "peer-rename",
            from_fn_async_local(peer_rename)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "peer-set-dns-injection",
            from_fn_async_local(peer_set_dns_injection::<C>).no_display(),
        )
        .subcommand(
            "peer-config",
            from_fn_async_local(peer_config)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "peer-reissue",
            from_fn_async_local(peer_reissue)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
}

// === Implementation ===

/// Restart a WireGuard interface to apply peer changes.
/// This is more reliable than network reload for WireGuard peer updates.
async fn restart_wireguard_interface(interface_name: &str) -> Result<(), Error> {
    // ifdown may fail if interface isn't up yet — that's fine
    let _ =
        crate::run_quiet_async(tokio::process::Command::new("ifdown").arg(interface_name)).await;

    // ifup must succeed for the new config to take effect
    let status = crate::run_quiet_async(tokio::process::Command::new("ifup").arg(interface_name))
        .await
        .map_err(|e| Error::new(eyre!("ifup {interface_name}: {e}"), ErrorKind::Network))?;
    if !status.success() {
        return Err(Error::new(
            eyre!(
                "ifup {interface_name} failed with exit code {:?}",
                status.code()
            ),
            ErrorKind::Network,
        ));
    }

    Ok(())
}

// === Input Validation ===

/// Validate that an endpoint string is safe for use in UCI config and WireGuard client configs.
/// Rejects empty strings, newlines, and whitespace to prevent config injection.
fn validate_endpoint(endpoint: &str) -> Result<(), Error> {
    if endpoint.is_empty()
        || endpoint.contains('\n')
        || endpoint.contains('\r')
        || endpoint.contains(char::is_whitespace)
    {
        return Err(Error::new(
            eyre!("invalid endpoint: {endpoint}"),
            ErrorKind::InvalidValue,
        ));
    }
    Ok(())
}

/// Validate that a peer name is safe for use in UCI config and WireGuard client configs.
/// A blank name is written as a valueless option that the config parser rejects.
fn validate_peer_name(name: &str) -> Result<(), Error> {
    if name.trim().is_empty() || name.contains('\n') || name.contains('\r') {
        return Err(Error::new(
            eyre!("invalid name: {name}"),
            ErrorKind::InvalidValue,
        ));
    }
    Ok(())
}

/// Validate that a user-supplied private key is valid base64 and exactly 32 bytes.
fn validate_private_key(key: &str) -> Result<(), Error> {
    let _: Base64<WgKey> = key.parse().map_err(|_| {
        Error::new(
            eyre!("invalid private_key: [redacted]"),
            ErrorKind::InvalidValue,
        )
    })?;
    Ok(())
}

/// The client config's `Address`. The IPv4 address carries its /24, whose
/// first host a StartOS client sends DNS updates to.
fn client_address_line(peer_ip: Ipv4Addr, v6_groups: Option<[u16; 4]>) -> String {
    match v6_groups {
        Some(g) => format!(
            "{}/24, {}/128",
            peer_ip,
            wg_peer_v6_addr(g, peer_ip.octets()[3])
        ),
        None => format!("{}/24", peer_ip),
    }
}

/// Derive VPN server address from profile's gateway IP
/// Gateway is typically .1, VPN server uses .254
fn derive_server_address(gateway_ip: Ipv4Addr) -> Ipv4Addr {
    let octets = gateway_ip.octets();
    Ipv4Addr::new(octets[0], octets[1], octets[2], VPN_SERVER_HOST_OCTET)
}

/// The dedicated stable WireGuard ULA /64 for an inbound VPN server on
/// `profile_interface`, as its 4 leading 16-bit groups, or `None` when the
/// profile does not currently serve IPv6 to clients or no concrete device ULA
/// prefix is configured (pre-first-boot `ula_prefix 'auto'`).
///
/// Peers can't sit in the profile's own /64 (DHCPv6-PD assigns it at runtime,
/// non-deterministically) and WireGuard client configs are issued once and
/// static — so peers need a STABLE address. We carve a dedicated /64 from the
/// device ULA /48 (`network.globals.ula_prefix`, concrete and stable
/// post-first-boot) using a high subnet-id band (`0xf000 | vlan_tag`) that stays
/// clear of odhcpd's low sequential per-profile assignments. The peer is reached
/// purely via the /128 that `route_allowed_ips` installs in the main table plus
/// the v6 escape rules — so no proxy_ndp and no per-table peer routes are needed.
fn wg_server_v6_groups(cfgs: &Configs, profile_interface: &str) -> Option<[u16; 4]> {
    use crate::profiles::UciProfile;

    let profile = cfgs["startwrt"]
        .sections
        .iter()
        .filter_map(|s| s.get::<UciProfile>().ok())
        .find(|p| p.interface == profile_interface)?;

    // Only hand out a v6 address when the profile actually serves v6 to clients.
    if !crate::profiles::is_ipv6_enabled(cfgs) {
        return None;
    }
    let outbound = profile
        .outbound
        .clone()
        .unwrap_or_else(|| "wan".to_string());
    if !crate::profiles::outbound_supports_ipv6(cfgs, &outbound) {
        return None;
    }

    let prefix = cfgs["network"]
        .sections
        .iter()
        .find_map(|s| s.get::<NetworkGlobals>().ok().and_then(|g| g.ula_prefix))?;
    let base: Ipv6Addr = prefix.split('/').next()?.parse().ok()?;
    let g = base.segments();
    // Must be a ULA (fc00::/7); rejects the literal 'auto', GUAs, and garbage.
    if g[0] & 0xfe00 != 0xfc00 {
        return None;
    }
    Some([g[0], g[1], g[2], 0xf000 | profile.vlan_tag])
}

/// Server address in the WG /64: `<wg64>::1`.
fn wg_server_v6_addr(groups: [u16; 4]) -> Ipv6Addr {
    Ipv6Addr::new(groups[0], groups[1], groups[2], groups[3], 0, 0, 0, 1)
}

/// Peer address in the WG /64, reusing the peer's v4 host octet as the host id:
/// `<wg64>::<octet>`.
fn wg_peer_v6_addr(groups: [u16; 4], host_octet: u8) -> Ipv6Addr {
    Ipv6Addr::new(
        groups[0],
        groups[1],
        groups[2],
        groups[3],
        0,
        0,
        0,
        host_octet as u16,
    )
}

/// Reconciles the server interface's addresses with its peers: the address in
/// the profile's subnet while legacy peers use it, the router's host in the
/// peer subnet, and the WG-/64 ULA while the profile serves IPv6.
fn sync_server_addresses(cfgs: &mut Configs, wg_interface_name: &str) -> Result<(), Error> {
    let profile_interface = profile_of_server(wg_interface_name);
    let subnet = peer_subnet_of(cfgs, wg_interface_name);
    let mut want = Vec::new();
    // First: legacy peers accept traffic from the primary address alone.
    if subnet.is_none() || !legacy_peer_ips(cfgs, wg_interface_name).is_empty() {
        if let Ok(gateway) = get_gateway_ip(cfgs, profile_interface) {
            want.push(format!("{}/32", derive_server_address(gateway)));
        }
    }
    if let Some(subnet) = subnet {
        want.push(format!("{}/24", peer_subnet_router(subnet)));
    }
    if let Some(groups) = wg_server_v6_groups(cfgs, profile_interface) {
        want.push(format!("{}/128", wg_server_v6_addr(groups)));
    }
    for section in &mut cfgs["network"].sections {
        if section.name().as_deref() != Some(wg_interface_name) {
            continue;
        }
        let Ok(mut wg) = section.get::<WgInterface>() else {
            continue;
        };
        if !wg.is_wireguard() {
            continue;
        }
        if wg.addresses != want {
            wg.addresses = want;
            section.set(&wg)?;
        }
        break;
    }
    Ok(())
}

/// Inbound servers are named after their profile: `wg_<profile>`.
fn profile_of_server(wg_interface_name: &str) -> &str {
    wg_interface_name
        .strip_prefix("wg_")
        .unwrap_or(wg_interface_name)
}

/// The network address of an `a.b.c.0/24`.
fn parse_subnet24(cidr: &str) -> Option<Ipv4Addr> {
    let (addr, len) = cidr.split_once('/')?;
    let addr: Ipv4Addr = addr.parse().ok()?;
    (len == "24" && addr.octets()[3] == 0).then_some(addr)
}

fn same_24(a: Ipv4Addr, b: Ipv4Addr) -> bool {
    a.octets()[..3] == b.octets()[..3]
}

fn peer_subnet_router(subnet: Ipv4Addr) -> Ipv4Addr {
    let o = subnet.octets();
    Ipv4Addr::new(o[0], o[1], o[2], PEER_SUBNET_ROUTER_HOST)
}

fn vpn_servers(cfgs: &Configs) -> Vec<UciVpnServer> {
    cfgs["startwrt"]
        .sections
        .iter()
        .filter_map(|s| s.get::<UciVpnServer>().ok())
        .collect()
}

/// The network address of the server's peer subnet.
fn peer_subnet_of(cfgs: &Configs, wg_interface_name: &str) -> Option<Ipv4Addr> {
    vpn_servers(cfgs)
        .into_iter()
        .find(|meta| meta.interface == wg_interface_name)?
        .peer_subnet
        .as_deref()
        .and_then(parse_subnet24)
}

/// Peers addressed inside their profile's own subnet, which LAN devices reach
/// through proxy ARP.
pub(crate) fn legacy_peer_ips(cfgs: &Configs, wg_interface_name: &str) -> BTreeSet<Ipv4Addr> {
    let Ok(gateway) = get_gateway_ip(cfgs, profile_of_server(wg_interface_name)) else {
        return BTreeSet::new();
    };
    get_vpn_peer_ips(cfgs, wg_interface_name)
        .into_iter()
        .filter(|ip| same_24(*ip, gateway))
        .collect()
}

/// Third octets in the LAN block a peer subnet must not take: addresses the
/// router holds or routes, and every server's peer subnet.
fn taken_block_octets(cfgs: &Configs, block: [u8; 2], wan: &[Ipv4Addr]) -> BTreeSet<u8> {
    let in_block = |ip: Ipv4Addr| {
        let o = ip.octets();
        (o[0] == block[0] && o[1] == block[1]).then_some(o[2])
    };
    let mut taken = BTreeSet::new();
    for section in &cfgs["network"].sections {
        if let Some(ip) = section
            .get::<NetworkInterface>()
            .ok()
            .and_then(|iface| iface.ipv4())
        {
            taken.extend(in_block(ip));
        }
        if let Ok(wg) = section.get::<WgInterface>() {
            taken.extend(
                wg.addresses
                    .iter()
                    .filter_map(|a| a.split('/').next()?.parse().ok())
                    .filter_map(in_block),
            );
        }
    }
    taken.extend(wan.iter().copied().filter_map(in_block));
    taken.extend(
        vpn_servers(cfgs)
            .iter()
            .filter_map(|meta| meta.peer_subnet.as_deref().and_then(parse_subnet24))
            .filter_map(in_block),
    );
    taken
}

/// A free `/24` in the LAN block, from the top: profile subnets are picked
/// from the bottom.
fn allocate_peer_subnet(cfgs: &Configs, wan: &[Ipv4Addr]) -> Result<Ipv4Addr, Error> {
    let lan = get_gateway_ip(cfgs, crate::lan::LAN_INTERFACE)?.octets();
    let block = [lan[0], lan[1]];
    let taken = taken_block_octets(cfgs, block, wan);
    (0..=u8::MAX)
        .rev()
        .find(|octet| !taken.contains(octet))
        .map(|octet| Ipv4Addr::new(block[0], block[1], octet, 0))
        .ok_or_else(|| {
            Error::new(
                eyre!(
                    "no free /24 left in the LAN network block {}.{}.0.0/16 for the VPN's clients",
                    block[0],
                    block[1]
                ),
                ErrorKind::SubnetCollision,
            )
        })
}

/// The server's peer subnet, allocating one when it has none.
fn ensure_peer_subnet(
    cfgs: &mut Configs,
    wg_interface_name: &str,
    wan: &[Ipv4Addr],
) -> Result<Ipv4Addr, Error> {
    if let Some(subnet) = peer_subnet_of(cfgs, wg_interface_name) {
        return Ok(subnet);
    }
    let subnet = allocate_peer_subnet(cfgs, wan)?;
    for section in &mut cfgs["startwrt"].sections {
        let Ok(mut meta) = section.get::<UciVpnServer>() else {
            continue;
        };
        if meta.interface == wg_interface_name {
            meta.peer_subnet = Some(format!("{subnet}/24"));
            section.set(&meta)?;
            return Ok(subnet);
        }
    }
    Err(Error::new(
        eyre!("VPN server not found: {wg_interface_name}"),
        ErrorKind::NotFound,
    ))
}

/// The host octets of the server's peers, legacy included: each is also the
/// peer's IPv6 host id.
fn used_peer_hosts(cfgs: &Configs, wg_interface_name: &str) -> BTreeSet<u8> {
    get_vpn_peer_ips(cfgs, wg_interface_name)
        .into_iter()
        .map(|ip| ip.octets()[3])
        .collect()
}

fn allocate_peer_ip(subnet: Ipv4Addr, used: &BTreeSet<u8>) -> Result<Ipv4Addr, Error> {
    let o = subnet.octets();
    (PEER_HOST_FIRST..=PEER_HOST_LAST)
        .find(|host| !used.contains(host))
        .map(|host| Ipv4Addr::new(o[0], o[1], o[2], host))
        .ok_or_else(|| {
            Error::new(
                eyre!("No available IP addresses for new peer in {subnet}/24"),
                ErrorKind::InvalidValue,
            )
        })
}

fn validate_peer_ip(ip: Ipv4Addr, subnet: Ipv4Addr, used: &BTreeSet<u8>) -> Result<(), Error> {
    let host = ip.octets()[3];
    if !same_24(ip, subnet) {
        return Err(Error::new(
            eyre!("Peer IP must be in the VPN subnet {subnet}/24"),
            ErrorKind::InvalidValue,
        ));
    }
    if !(PEER_HOST_FIRST..=PEER_HOST_LAST).contains(&host) {
        return Err(Error::new(
            eyre!(
                "Peer IP host octet must be between {} and {}",
                PEER_HOST_FIRST,
                PEER_HOST_LAST
            ),
            ErrorKind::InvalidValue,
        ));
    }
    if used.contains(&host) {
        return Err(Error::new(
            eyre!("Host .{host} is already used by another client of this VPN"),
            ErrorKind::InvalidValue,
        ));
    }
    Ok(())
}

/// Extract gateway IP for a profile from already-parsed configs.
/// Avoids redundant re-parsing that `profiles::get()` would do.
fn get_gateway_ip(cfgs: &Configs, interface_name: &str) -> Result<Ipv4Addr, Error> {
    use uciedit::openwrt::NetworkInterface;

    for section in &cfgs["network"].sections {
        let Ok(iface) = section.get::<NetworkInterface>() else {
            continue;
        };
        let Some(name) = section.name() else { continue };
        if name == interface_name {
            return iface.ipv4().ok_or_else(|| {
                Error::new(
                    eyre!("No IP address configured for interface {}", interface_name),
                    ErrorKind::Network,
                )
            });
        }
    }
    Err(Error::new(
        eyre!("Network interface {} not found", interface_name),
        ErrorKind::NotFound,
    ))
}

/// Get the set of allocated peer IPs for a VPN server from WireGuard peer configs
fn get_vpn_peer_ips(cfgs: &Configs, wg_interface_name: &str) -> BTreeSet<Ipv4Addr> {
    let peer_type = format!("wireguard_{}", wg_interface_name);

    cfgs["network"]
        .sections
        .iter()
        .filter(|section| section.ty() == peer_type)
        .filter_map(|section| {
            for line in &section.lines {
                if let Line::List { list, item, .. } = line {
                    if list.as_str() == "allowed_ips" {
                        // Parse IP from "x.x.x.x/32" format
                        let ip_str = item.as_str();
                        if let Some(ip_part) = ip_str.split('/').next() {
                            if let Ok(ip) = ip_part.parse::<Ipv4Addr>() {
                                return Some(ip);
                            }
                        }
                    }
                }
            }
            None
        })
        .collect()
}

/// Syncs the inbound server's routes and rules in its profile's policy routing
/// table, which a VPN-routed profile's traffic uses.
///
/// That table routes the profile's /24 to the LAN bridge, so each legacy peer
/// gets a /32 back to the tunnel. The peer subnet gets a route to the tunnel and
/// a rule sending its traffic through the table, as the profile's own `prr_` does.
pub(crate) fn sync_peer_policy_routes(
    cfgs: &mut Configs,
    profile_interface: &str,
) -> Result<(), Error> {
    use crate::profiles::UciProfile;

    let wg_interface_name = format!("wg_{}", profile_interface);
    let route_prefix = format!("vpr_{}_", profile_interface);
    let vsn_name = format!("vsn_{}", profile_interface);
    let vsr_name = format!("vsr_{}", profile_interface);
    let vsl6_name = format!("vsl6_{}", profile_interface);
    let vsr6_name = format!("vsr6_{}", profile_interface);

    // 1. Remove all existing VPN peer routes and v6 steering rules for this
    //    profile (idempotent recompute; also cleans them up if the profile
    //    became wan-routed or lost its inbound server in the gates below).
    cfgs["network"]
        .sections
        .retain(|s| match s.name().as_deref() {
            Some(n) => {
                !n.starts_with(&route_prefix)
                    && ![&vsn_name, &vsr_name, &vsl6_name, &vsr6_name]
                        .iter()
                        .any(|name| **name == *n)
            }
            None => true,
        });

    // 2. Find the profile's outbound and vlan_tag
    let profile = cfgs["startwrt"]
        .sections
        .iter()
        .filter_map(|s| s.get::<UciProfile>().ok())
        .find(|p| p.interface == profile_interface);

    let Some(profile) = profile else {
        return Ok(());
    };

    let outbound = profile.outbound.unwrap_or_else(|| "wan".to_string());
    if outbound == "wan" {
        return Ok(());
    }

    let vlan_tag = profile.vlan_tag;

    // 3. Check if a VPN server exists for this profile
    let vpn_exists = cfgs["startwrt"]
        .sections
        .iter()
        .filter_map(|s| s.get::<UciVpnServer>().ok())
        .any(|meta| meta.interface == wg_interface_name);

    if !vpn_exists {
        return Ok(());
    }

    // 4. Add a /32 route for each legacy peer in the policy routing table
    for ip in legacy_peer_ips(cfgs, &wg_interface_name) {
        let route_name = format!("vpr_{}_{}", profile_interface, ip.octets()[3]);
        cfgs["network"].append(
            &NetworkRoute {
                interface: wg_interface_name.clone(),
                target: format!("{}/32", ip),
                table: Some(vlan_tag as u32),
                ..Default::default()
            },
            Some(&route_name),
        )?;
    }

    // 4b. The peer subnet: reachable from the table, and routed through it.
    if let Some(subnet) = peer_subnet_of(cfgs, &wg_interface_name) {
        cfgs["network"].append(
            &NetworkRoute {
                interface: wg_interface_name.clone(),
                target: format!("{subnet}/24"),
                table: Some(vlan_tag as u32),
                ..Default::default()
            },
            Some(&vsn_name),
        )?;
        cfgs["network"].append(
            &NetworkRule {
                src: Some(format!("{subnet}/24")),
                lookup: vlan_tag as u32,
                priority: Some(crate::profiles::VPN_ROUTING_PRIORITY),
                ..Default::default()
            },
            Some(&vsr_name),
        )?;
    }

    // 5. IPv6 egress steering for the inbound server interface. Peer v6 ingresses
    //    on wg_<X>, which the profile's prl6_/prr6_ rules (keyed on `in: br-lan.N`)
    //    do NOT match — so without these it would fall to the main table and leak
    //    out wan6 on a vpn-routed profile. Mirror prl6_/prr6_ for `in: wg_<X>`:
    //      * vsl6_: lookup main, suppress_prefixlength=0 — local/cross-VLAN escape
    //        so peer→sibling and peer→own-LAN v6 stay local (matching the connected
    //        /64 routes in main and the peer /128s route_allowed_ips installs
    //        there). Suppressing the default route keeps it from sending public v6
    //        to wan6.
    //      * vsr6_: lookup <vlan_tag> — captures the rest to the per-VLAN tunnel
    //        default (prt6_) or the unreachable kill-switch (prt6b_). This closes
    //        the leak and gives the peer public v6 via the tunnel's NAT66.
    //    Installed whenever an inbound server exists on a vpn-routed profile,
    //    independent of v6 capability, so peer v6 can never leak (a v4-only tunnel
    //    drops it on the kill-switch). v6 reachability needs no per-table peer
    //    routes (unlike v4's vpr_): the escape rule + the /128-in-main beat any
    //    /64, and no proxy_ndp is needed because the peer lives on a dedicated /64.
    cfgs["network"].append(
        &NetworkRule6 {
            in_iface: Some(wg_interface_name.clone()),
            lookup: 254, // main
            suppress_prefixlength: Some(0),
            priority: Some(crate::profiles::VPN_ROUTING_V6_LOCAL_PRIORITY),
            ..Default::default()
        },
        Some(&vsl6_name),
    )?;
    cfgs["network"].append(
        &NetworkRule6 {
            in_iface: Some(wg_interface_name.clone()),
            lookup: vlan_tag as u32,
            priority: Some(crate::profiles::VPN_ROUTING_PRIORITY),
            ..Default::default()
        },
        Some(&vsr6_name),
    )?;

    Ok(())
}

/// Routes every inbound server's peers into the policy routing tables of the
/// OTHER vpn-routed profiles: its peer subnet, and a /32 for each legacy peer.
///
/// A vpn-routed sibling's table sends a peer subnet to the VPN's default route,
/// and a legacy peer to the /24-via-bridge route `sync_cross_subnet_routes`
/// adds for its profile, so cross-profile replies miss the tunnel.
///
/// wan-routed siblings use the main table, which already routes both to `wg_<P>`.
///
/// Access between profiles stays governed solely by the firewall forwarding
/// rules (driven by `lan_access`) — these routes only correct the L3 path and
/// grant no reachability the firewall does not already permit.
pub(crate) fn sync_vpn_peer_cross_routes(cfgs: &mut Configs) -> Result<(), Error> {
    use crate::profiles::UciProfile;

    // 1. Drop stale routes for an idempotent recompute.
    cfgs["network"].sections.retain(|s| {
        !s.name()
            .as_deref()
            .map_or(false, |n| n.starts_with("vxr_") || n.starts_with("vxs_"))
    });

    // 2. vpn-routed profiles are the destination tables that need the /32s.
    let vpn_profiles: Vec<(String, u16)> = cfgs["startwrt"]
        .sections
        .iter()
        .filter_map(|s| {
            let p = s.get::<UciProfile>().ok()?;
            if p.outbound.as_deref().unwrap_or("wan") != "wan" {
                Some((p.interface, p.vlan_tag))
            } else {
                None
            }
        })
        .collect();
    if vpn_profiles.is_empty() {
        return Ok(());
    }

    // 3. Each inbound VPN server: (profile_interface, wg_interface).
    let servers: Vec<(String, String)> = cfgs["startwrt"]
        .sections
        .iter()
        .filter_map(|s| s.get::<UciVpnServer>().ok())
        .map(|meta| (meta.profile_interface, meta.interface))
        .collect();

    // 4. Add each server's routes into every OTHER vpn-routed profile's table.
    for (server_iface, wg_iface) in &servers {
        let peer_ips = legacy_peer_ips(cfgs, wg_iface);
        let subnet = peer_subnet_of(cfgs, wg_iface);
        for (table_iface, vlan_tag) in &vpn_profiles {
            if table_iface == server_iface {
                continue; // own table is handled by sync_peer_policy_routes
            }
            if let Some(subnet) = subnet {
                cfgs["network"].append(
                    &NetworkRoute {
                        interface: wg_iface.clone(),
                        target: format!("{subnet}/24"),
                        table: Some(*vlan_tag as u32),
                        ..Default::default()
                    },
                    Some(&format!("vxs_{server_iface}_{table_iface}")),
                )?;
            }
            for ip in &peer_ips {
                let route_name = format!("vxr_{}_{}_{}", server_iface, table_iface, ip.octets()[3]);
                cfgs["network"].append(
                    &NetworkRoute {
                        interface: wg_iface.clone(),
                        target: format!("{}/32", ip),
                        table: Some(*vlan_tag as u32),
                        ..Default::default()
                    },
                    Some(&route_name),
                )?;
            }
        }
    }

    Ok(())
}

/// The /24 CIDR string (`x.y.z.0/24`) covering a profile gateway address.
fn subnet_cidr_of_ip(ip: Ipv4Addr) -> String {
    let o = ip.octets();
    format!("{}.{}.{}.0/24", o[0], o[1], o[2])
}

/// Resolve a profile interface to its /24 from the static network config.
fn profile_subnet_cidr(cfgs: &Configs, interface: &str) -> Option<String> {
    cfgs["network"].sections.iter().find_map(|s| {
        if s.name().as_deref() != Some(interface) {
            return None;
        }
        let ni = s.get::<NetworkInterface>().ok()?;
        ni.ipv4().map(subnet_cidr_of_ip)
    })
}

/// Build the split-tunnel `AllowedIPs` LAN list for a "LAN only" peer: the
/// profile's own /24 plus the /24 of every profile its `lan_access` permits it
/// to reach. Returns a comma-separated list with no default route ("All traffic"
/// peers use `0.0.0.0/0` instead).
///
/// Scoped to the profile's OUTBOUND `lan_access` (what the client may initiate
/// to), so a limited-access profile's peer reaches exactly the profiles that
/// profile can — keeping the VPN client consistent with the profile rules.
///
/// TODO(reverse-parity): a profile permitted to initiate INTO this one (e.g. an
/// admin profile with access to all) cannot reach this peer, because its subnet
/// is absent from `AllowedIPs` and WireGuard drops inbound packets whose source
/// falls outside `AllowedIPs`. A wired client in the same profile WOULD be
/// reachable. To close this gap, also include the subnets of profiles whose
/// firewall zone forwards into this profile's zone.
fn lan_only_allowed_ips(
    ctx: impl CtrlContext,
    cfgs: &Configs,
    profile_interface: &str,
    gateway_ip: Ipv4Addr,
) -> Result<String, Error> {
    use crate::profiles::{LanAccess, Lookup, ProfileIdOpt};

    let mut subnets = vec![subnet_cidr_of_ip(gateway_ip)];

    let profile = crate::profiles::get_config(
        ctx.clone(),
        cfgs,
        ProfileIdOpt {
            fullname: None,
            interface: Some(profile_interface.to_string()),
            vlan_tag: None,
        },
    )?;

    let add = |iface: &str, subnets: &mut Vec<String>| {
        if iface == profile_interface {
            return;
        }
        if let Some(cidr) = profile_subnet_cidr(cfgs, iface) {
            if !subnets.contains(&cidr) {
                subnets.push(cidr);
            }
        }
    };

    match profile.lan_access {
        LanAccess::SameProfile => {}
        LanAccess::OtherProfiles(ids) => {
            for id in &ids {
                add(&id.interface, &mut subnets);
            }
        }
        LanAccess::All => {
            let lookup = Lookup::parse(ctx, cfgs)?;
            for id in lookup.list() {
                add(&id.interface, &mut subnets);
            }
        }
    }

    Ok(subnets.join(", "))
}

/// Returns a map of profile_interface -> peer_count for profiles that have VPN peers.
pub(crate) fn profiles_with_vpn_peers(
    cfgs: &Configs,
    profile_interfaces: &[&str],
) -> BTreeMap<String, usize> {
    let mut result = BTreeMap::new();
    for section in &cfgs["startwrt"].sections {
        let Ok(vpn_meta) = section.get::<UciVpnServer>() else {
            continue;
        };
        if !profile_interfaces.contains(&vpn_meta.profile_interface.as_str()) {
            continue;
        }
        let peers = get_peers_for_interface(cfgs, &vpn_meta.interface);
        if !peers.is_empty() {
            result.insert(vpn_meta.profile_interface.clone(), peers.len());
        }
    }
    result
}

/// Fully remove the VPN server for a profile interface, including the WireGuard
/// interface, all peers, metadata, firewall rules, policy routes, and proxy ARP UCI.
/// Operates on already-parsed configs (caller handles dump_all and service reload).
///
/// Returns the profile's device, whose proxy ARP sysctl the caller clears after the reload.
pub(crate) fn remove_vpn_server(
    cfgs: &mut Configs,
    profile_interface: &str,
) -> Result<Option<String>, Error> {
    let wg_interface_name = format!("wg_{}", profile_interface);
    let peer_type = format!("wireguard_{}", wg_interface_name);
    let device = profile_device(cfgs, profile_interface);

    // Remove WireGuard interface
    cfgs["network"].sections.retain(|section| {
        if section.name().as_deref() == Some(wg_interface_name.as_str()) {
            if let Ok(iface) = section.get::<WgInterface>() {
                if iface.is_wireguard() {
                    return false;
                }
            }
        }
        true
    });

    // Remove all peers
    cfgs["network"]
        .sections
        .retain(|section| section.ty() != peer_type);

    // Remove VPN server metadata
    cfgs["startwrt"].sections.retain(|section| {
        if let Ok(meta) = section.get::<UciVpnServer>() {
            return meta.interface != wg_interface_name;
        }
        true
    });

    // Clean up firewall
    remove_from_firewall_zones(cfgs, &wg_interface_name)?;
    remove_wireguard_firewall_rule(cfgs, &wg_interface_name)?;

    // Clean up routing and proxy ARP UCI
    sync_peer_policy_routes(cfgs, profile_interface)?;
    sync_vpn_peer_cross_routes(cfgs)?;
    sync_proxy_arp(cfgs, profile_interface)?;

    Ok(device)
}

/// Whether the profile's VPN server has peers that LAN devices reach through proxy ARP.
pub(crate) fn proxy_arp_wanted(cfgs: &Configs, profile_interface: &str) -> bool {
    let wg_interface_name = format!("wg_{}", profile_interface);
    vpn_servers(cfgs)
        .iter()
        .any(|meta| meta.interface == wg_interface_name)
        && !legacy_peer_ips(cfgs, &wg_interface_name).is_empty()
}

/// Sets the UCI `proxy_arp` flag the boot hotplug reads to [`proxy_arp_wanted`].
pub(crate) fn sync_proxy_arp(cfgs: &mut Configs, profile_interface: &str) -> Result<(), Error> {
    let desired = proxy_arp_wanted(cfgs, profile_interface).then(|| "1".to_string());

    // Find the profile's network interface and update proxy_arp if needed
    let section = cfgs["network"]
        .sections
        .iter_mut()
        .find(|s| s.name().as_deref() == Some(profile_interface));

    let Some(section) = section else {
        return Ok(());
    };

    let mut iface = section.get::<NetworkInterface>().map_err(|_| {
        Error::new(
            eyre!("corrupted profile: interface={}", profile_interface),
            ErrorKind::CorruptedProfile,
        )
    })?;

    if iface.proxy_arp != desired {
        iface.proxy_arp = desired;
        section.set(&iface)?;
    }

    Ok(())
}

/// Not always `br-lan.<tag>`: a lone profile sits on `br-lan` itself.
fn profile_device(cfgs: &Configs, profile_interface: &str) -> Option<String> {
    cfgs["network"]
        .sections
        .iter()
        .find(|s| s.name().as_deref() == Some(profile_interface))
        .and_then(|s| s.get::<NetworkInterface>().ok())
        .map(|iface| iface.device)
        .filter(|device| !device.is_empty())
}

/// Writes the live sysctls, which netifd never sets from the UCI `proxy_arp`
/// option. An enabled device answers without the kernel's random delay, ahead
/// of a peer that also sits on the LAN.
pub(crate) async fn apply_proxy_arp_sysctl(device: &str, enable: bool) {
    let path = format!("/proc/sys/net/ipv4/conf/{device}/proxy_arp");
    if let Err(e) = tokio::fs::write(&path, if enable { "1" } else { "0" }).await {
        tracing::warn!("Failed to set proxy ARP on {device}: {e}");
    }
    let delay = if enable {
        "0".to_string()
    } else {
        match tokio::fs::read_to_string("/proc/sys/net/ipv4/neigh/default/proxy_delay").await {
            Ok(default) => default.trim().to_string(),
            Err(e) => {
                tracing::warn!("Failed to read the default proxy ARP delay: {e}");
                return;
            }
        }
    };
    let path = format!("/proc/sys/net/ipv4/neigh/{device}/proxy_delay");
    if let Err(e) = tokio::fs::write(&path, delay).await {
        tracing::warn!("Failed to set the proxy ARP delay on {device}: {e}");
    }
}

/// The LAN guard's set of legacy peer addresses. fw4 loads it after the guard table.
const VPN_PEER_GUARD_PATH: &str = "/usr/share/nftables.d/ruleset-pre/12-startwrt-lan-vpn-peers.nft";

/// Declares the set as well as filling it. A guard table without one still loads.
fn vpn_peer_guard_ruleset(cfgs: &Configs) -> String {
    let ips: BTreeSet<Ipv4Addr> = vpn_servers(cfgs)
        .iter()
        .flat_map(|meta| legacy_peer_ips(cfgs, &meta.interface))
        .collect();
    let mut out = String::from(
        "# Generated by startwrt from the inbound VPN peers addressed inside a profile's subnet.\n\
         add table bridge startwrt_lan_guard\n\
         add set bridge startwrt_lan_guard vpn_peers { type ipv4_addr; }\n",
    );
    if !ips.is_empty() {
        let ips: Vec<String> = ips.iter().map(Ipv4Addr::to_string).collect();
        out.push_str(&format!(
            "add element bridge startwrt_lan_guard vpn_peers {{ {} }}\n",
            ips.join(", ")
        ));
    }
    out
}

/// Re-renders [`VPN_PEER_GUARD_PATH`] from the config on disk. Takes effect on
/// the next firewall reload. Returns whether it wrote.
pub(crate) async fn sync_vpn_peer_guard(uci_root: impl AsRef<Path>) -> Result<bool, Error> {
    sync_vpn_peer_guard_file(uci_root.as_ref(), Path::new(VPN_PEER_GUARD_PATH)).await
}

async fn sync_vpn_peer_guard_file(uci_root: &Path, path: &Path) -> Result<bool, Error> {
    crate::published_ports::sync_nft_include(uci_root, path, &["network", "startwrt"], |cfgs| {
        vpn_peer_guard_ruleset(cfgs)
    })
    .await
}

/// [`sync_vpn_peer_guard`], then a firewall reload whether or not it wrote.
pub(crate) async fn apply_vpn_peer_guard(uci_root: impl AsRef<Path>) -> Result<(), Error> {
    sync_vpn_peer_guard(uci_root).await?;
    crate::published_ports::reload_firewall_wait().await
}

/// Logs a failed LAN guard update, which boot repairs, instead of failing a
/// change that already applied.
pub(crate) fn warn_guard<T>(result: Result<T, Error>) {
    if let Err(e) = result {
        tracing::warn!("Failed to update the LAN guard's VPN peers: {e}");
    }
}

/// Boot heal: the include lives outside `/etc` and is not kept by sysupgrade.
pub async fn heal_vpn_peer_guard(uci_root: impl AsRef<Path>) -> Result<(), Error> {
    if sync_vpn_peer_guard(uci_root).await? {
        tracing::info!("lan-guard: inbound VPN peer set rewritten; reloading firewall");
        crate::published_ports::reload_firewall();
    }
    Ok(())
}

/// Tracks VPN servers removed by [`guard_vpn_peers`] so callers can tear down
/// the live WireGuard interfaces and proxy ARP sysctls after dumping configs.
#[derive(Default)]
pub(crate) struct RemovedVpnServers {
    pub wg_ifaces: Vec<String>,
    pub proxy_arp_devices: Vec<String>,
}

impl RemovedVpnServers {
    /// Bring down orphaned WireGuard interfaces, clear proxy ARP sysctls and
    /// drop their peers from the LAN guard. Call AFTER `dump_all` and the main
    /// service reload.
    pub async fn apply_post_reload(&self) {
        for wg in &self.wg_ifaces {
            let _ = crate::run_quiet_async(tokio::process::Command::new("ifdown").arg(wg)).await;
        }
        for device in &self.proxy_arp_devices {
            apply_proxy_arp_sysctl(device, false).await;
        }
        if !self.wg_ifaces.is_empty() {
            warn_guard(apply_vpn_peer_guard("/etc/config").await);
        }
    }
}

/// Check whether changing the IP/subnet of the given profile interfaces would
/// break existing VPN peers. If `force` is false and peers exist, returns
/// `VpnPeersWouldBreak`. If `force` is true, removes the affected VPN servers
/// from the parsed configs (caller still needs to `dump_all`).
pub(crate) fn guard_vpn_peers(
    cfgs: &mut Configs,
    affected_interfaces: &[&str],
    force: bool,
) -> Result<RemovedVpnServers, Error> {
    let peers = profiles_with_vpn_peers(cfgs, affected_interfaces);
    if peers.is_empty() {
        return Ok(RemovedVpnServers::default());
    }

    let total: usize = peers.values().sum();
    if !force {
        let profiles: Vec<_> = peers.keys().cloned().collect();
        return Err(Error::new(
            eyre!("VPN peers would break: {total} peer(s) across profiles {profiles:?}"),
            ErrorKind::VpnPeersWouldBreak,
        ));
    }

    let mut removed = RemovedVpnServers::default();
    for iface in peers.keys() {
        if let Some(device) = remove_vpn_server(cfgs, iface)? {
            removed.proxy_arp_devices.push(device);
        }
        removed.wg_ifaces.push(format!("wg_{}", iface));
    }
    Ok(removed)
}

/// Moves every peer subnet into a new LAN block, keeping its third octet, with
/// the server addresses and routes derived from it.
pub(crate) fn move_peer_subnets(cfgs: &mut Configs, block: [u8; 2]) -> Result<(), Error> {
    let mut moved = Vec::new();
    for section in &mut cfgs["startwrt"].sections {
        let Ok(mut meta) = section.get::<UciVpnServer>() else {
            continue;
        };
        let Some(old) = meta.peer_subnet.as_deref().and_then(parse_subnet24) else {
            continue;
        };
        let new = Ipv4Addr::new(block[0], block[1], old.octets()[2], 0);
        if new != old {
            meta.peer_subnet = Some(format!("{new}/24"));
            section.set(&meta)?;
            moved.push((meta.interface.clone(), meta.profile_interface.clone()));
        }
    }
    for (wg_interface_name, profile_interface) in &moved {
        sync_server_addresses(cfgs, wg_interface_name)?;
        sync_peer_policy_routes(cfgs, profile_interface)?;
    }
    if !moved.is_empty() {
        sync_vpn_peer_cross_routes(cfgs)?;
    }
    Ok(())
}

/// The profile whose server's peer subnet is this `/24`, for a collision report.
pub(crate) fn peer_subnet_owner(cfgs: &Configs, gateway_ip: Ipv4Addr) -> Option<String> {
    vpn_servers(cfgs)
        .into_iter()
        .find(|meta| {
            meta.peer_subnet
                .as_deref()
                .and_then(parse_subnet24)
                .is_some_and(|subnet| same_24(subnet, gateway_ip))
        })
        .map(|meta| meta.profile_interface)
}

/// A peer's client `AllowedIPs`. A legacy peer's omit the peer subnet, as its
/// original configuration did.
fn client_allowed_ips(
    cfgs: &Configs,
    profile_interface: &str,
    gateway_ip: Ipv4Addr,
    peer_ip: Ipv4Addr,
    route_all: bool,
    subnet: Option<Ipv4Addr>,
    v6_groups: Option<[u16; 4]>,
) -> Result<String, Error> {
    if route_all {
        return Ok("0.0.0.0/0, ::/0".to_string());
    }
    let mut allowed = lan_only_allowed_ips(
        ServerContext::default(),
        cfgs,
        profile_interface,
        gateway_ip,
    )?;
    if let Some(subnet) = subnet.filter(|subnet| same_24(*subnet, peer_ip)) {
        allowed.push_str(&format!(", {subnet}/24"));
    }
    if let Some(g) = v6_groups {
        // The device ULA /48 covers every profile's dynamic /64 plus the
        // WG /64. v6 lan_access is enforced at the firewall — we can't
        // scope to specific sibling /64s because DHCPv6-PD assigns them
        // at runtime (unlike the static v4 /24s above).
        allowed.push_str(&format!(", {:x}:{:x}:{:x}::/48", g[0], g[1], g[2]));
    }
    Ok(allowed)
}

/// List all VPN servers
#[instrument(skip_all)]
pub async fn list(_ctx: ServerContext) -> Result<VpnServers, Error> {
    let arena = Arena::new();
    let cfgs = parse_all("/etc/config", &arena, &["network", "startwrt", "firewall"]).await?;

    let profile_lookup = profiles::Lookup::parse(ServerContext::default(), &cfgs)?;

    let servers = cfgs["startwrt"]
        .sections
        .iter()
        .filter_map(|section| section.get::<UciVpnServer>().ok())
        .filter_map(|vpn_meta| {
            // Verify profile exists
            let _profile_id = profile_lookup.from_interface(&vpn_meta.profile_interface)?;
            let gateway_ip = get_gateway_ip(&cfgs, &vpn_meta.profile_interface).ok()?;

            // Get WireGuard interface
            let wg_iface = cfgs["network"]
                .sections
                .iter()
                .find(|s| s.name().as_deref() == Some(vpn_meta.interface.as_str()))
                .and_then(|s| s.get::<WgInterface>().ok())
                .filter(|wg| wg.is_wireguard())?;

            let peers = get_peers_for_interface(&cfgs, &vpn_meta.interface);
            let public_key = derive_public_key(&wg_iface.private_key).ok()?;
            let subnet = peer_subnet_of(&cfgs, &vpn_meta.interface);
            let server_address = subnet
                .map(peer_subnet_router)
                .unwrap_or_else(|| derive_server_address(gateway_ip));

            Some(VpnServer {
                profile: vpn_meta.profile_interface.clone(),
                label: vpn_meta.label.clone(),
                enabled: wg_iface.listen_port.is_some(),
                listen_port: vpn_meta.listen_port,
                endpoint: vpn_meta.endpoint.clone(),
                public_key,
                server_address,
                peer_subnet: subnet.map(|subnet| format!("{subnet}/24")),
                peers,
            })
        })
        .collect();

    Ok(VpnServers { servers })
}

/// Set (create or update) VPN server configuration for a profile
#[instrument(skip_all)]
pub async fn set(
    _ctx: ServerContext,
    DeserializeStdin(SetArgs { profile, config }): DeserializeStdin<SetArgs>,
) -> Result<(), Error> {
    // Validate inputs before touching any config
    validate_endpoint(&config.endpoint)?;
    if config.listen_port == 0 {
        return Err(Error::new(
            eyre!("invalid listen_port: 0"),
            ErrorKind::InvalidValue,
        ));
    }
    if let Some(ref key) = config.private_key {
        validate_private_key(key)?;
    }

    let profile_interface = &profile;
    let wg_interface_name = format!("wg_{}", profile_interface);

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        // `dhcp` is needed so set_wireguard_interface can gate the server's v6
        // address on is_ipv6_enabled (which reads the LAN RA setting).
        let mut cfgs = parse_all(
            "/etc/config",
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await?;

        // Verify the profile exists and get its gateway IP
        let profile_lookup = profiles::Lookup::parse(ServerContext::default(), &cfgs)?;
        let profile_id = profile_lookup
            .from_interface(profile_interface)
            .ok_or_else(|| {
                Error::new(
                    eyre!("missing profile: interface={}", profile_interface),
                    ErrorKind::MissingProfile,
                )
            })?;
        let profile_name = profile_id.fullname.clone();

        // Check for listen port conflicts with other VPN servers
        let port_conflict = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnServer>().ok())
            .any(|meta| {
                meta.listen_port == config.listen_port && meta.interface != wg_interface_name
            });
        if port_conflict {
            return Err(Error::new(
                eyre!(
                    "invalid listen_port: {} (already in use)",
                    config.listen_port
                ),
                ErrorKind::InvalidValue,
            ));
        }

        get_gateway_ip(&cfgs, profile_interface)?;

        // Update or create VPN server metadata
        set_vpn_server_metadata(&mut cfgs, &wg_interface_name, profile_interface, &config)?;
        let had_subnet = peer_subnet_of(&cfgs, &wg_interface_name).is_some();
        ensure_peer_subnet(
            &mut cfgs,
            &wg_interface_name,
            &ServerContext::default().wan_ipv4_addrs(),
        )?;

        // Update or create the WireGuard interface
        set_wireguard_interface(&mut cfgs, &wg_interface_name, &config)?;

        // Ensure the WireGuard interface is in the profile's firewall zone
        ensure_firewall_zone(&mut cfgs, &wg_interface_name, profile_interface)?;

        if config.enabled {
            // Ensure firewall rule allows WireGuard traffic on WAN
            ensure_wireguard_firewall_rule(&mut cfgs, &wg_interface_name, config.listen_port)?;
        } else {
            // Remove firewall rule when disabled — no inbound WireGuard traffic should be accepted
            remove_wireguard_firewall_rule(&mut cfgs, &wg_interface_name)?;
        }

        // Sync the peer routes in the profile's policy routing table
        sync_peer_policy_routes(&mut cfgs, profile_interface)?;
        // Keep cross-profile peer routes in sibling VPN tables in sync.
        sync_vpn_peer_cross_routes(&mut cfgs)?;

        sync_proxy_arp(&mut cfgs, profile_interface)?;

        let device = profile_device(&cfgs, profile_interface);
        let proxy_arp = proxy_arp_wanted(&cfgs, profile_interface);

        match dump_all("/etc/config", cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log(
                    "vpn-server",
                    "configured",
                    false,
                    &format!(
                        "Failed to configure inbound VPN for profile '{}'",
                        profile_name
                    ),
                    Some(&err.to_string()),
                );
                return Err(err.into());
            }
            Ok(()) => {
                if config.enabled {
                    // Restart the interface to ensure it comes up with new config.
                    // reload_system() alone won't bring up an interface that was
                    // previously taken down with ifdown.
                    restart_wireguard_interface(&wg_interface_name).await?;
                } else {
                    // Bring the WireGuard interface down to disconnect existing clients.
                    // reload_system() alone won't tear down an active WireGuard tunnel.
                    let _ = crate::run_quiet_async(
                        tokio::process::Command::new("ifdown").arg(&wg_interface_name),
                    )
                    .await;
                }
                warn_guard(sync_vpn_peer_guard("/etc/config").await);
                reload_system().await?;
                if let Some(device) = &device {
                    apply_proxy_arp_sysctl(device, proxy_arp).await;
                }
                if !had_subnet {
                    crate::dns_inject::permission_changed();
                }
                crate::activity::log(
                    "vpn-server",
                    "configured",
                    true,
                    &format!("Configured inbound VPN for profile '{}'", profile_name),
                    None,
                );
                return Ok(());
            }
        }
    }
}

/// Delete VPN server configuration for a profile
#[instrument(skip_all)]
pub async fn delete(_ctx: ServerContext, args: DeleteArgs) -> Result<(), Error> {
    let profile_interface = &args.profile;
    let wg_interface_name = format!("wg_{}", profile_interface);

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(
            "/etc/config",
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await?;

        // Resolve profile fullname before modifying configs
        let profile_name = {
            let profile_lookup = profiles::Lookup::parse(ServerContext::default(), &cfgs)?;
            profile_lookup
                .from_interface(profile_interface)
                .map(|p| p.fullname.clone())
                .unwrap_or_else(|| profile_interface.clone())
        };

        // Remove the WireGuard interface from network config
        cfgs["network"].sections.retain(|section| {
            if section.name().as_deref() == Some(wg_interface_name.as_str()) {
                if let Ok(iface) = section.get::<WgInterface>() {
                    if iface.is_wireguard() {
                        return false;
                    }
                }
            }
            true
        });

        // Remove peers (sections of type wireguard_<interface_name>)
        let peer_type = format!("wireguard_{}", wg_interface_name);
        let had_dns_injection = cfgs["network"].sections.iter().any(|section| {
            section.ty() == peer_type && crate::dns_inject::peer_allows_dns_inject(section)
        });
        cfgs["network"]
            .sections
            .retain(|section| section.ty() != peer_type);

        // Remove VPN server metadata from startwrt
        cfgs["startwrt"].sections.retain(|section| {
            if let Ok(meta) = section.get::<UciVpnServer>() {
                return meta.interface != wg_interface_name;
            }
            true
        });

        // Remove from firewall zones
        remove_from_firewall_zones(&mut cfgs, &wg_interface_name)?;

        // Remove the firewall rule allowing WireGuard on WAN
        remove_wireguard_firewall_rule(&mut cfgs, &wg_interface_name)?;

        // Clean up the peer routes from the profile's policy routing table
        sync_peer_policy_routes(&mut cfgs, profile_interface)?;
        // Keep cross-profile peer routes in sibling VPN tables in sync.
        sync_vpn_peer_cross_routes(&mut cfgs)?;

        // Disable proxy ARP now that no VPN server exists for this profile
        sync_proxy_arp(&mut cfgs, profile_interface)?;

        let device = profile_device(&cfgs, profile_interface);

        match dump_all("/etc/config", cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log(
                    "vpn-server",
                    "deleted",
                    false,
                    &format!(
                        "Failed to delete inbound VPN for profile '{}'",
                        profile_name
                    ),
                    Some(&err.to_string()),
                );
                return Err(err.into());
            }
            Ok(()) => {
                warn_guard(sync_vpn_peer_guard("/etc/config").await);
                reload_system().await?;
                if had_dns_injection {
                    crate::dns_inject::permission_changed();
                }
                if let Some(device) = &device {
                    apply_proxy_arp_sysctl(device, false).await;
                }
                crate::activity::log(
                    "vpn-server",
                    "deleted",
                    true,
                    &format!("Deleted inbound VPN for profile '{}'", profile_name),
                    None,
                );
                return Ok(());
            }
        }
    }
}

/// Add a peer to a VPN server
#[instrument(skip_all)]
pub async fn peer_add(
    _ctx: ServerContext,
    DeserializeStdin(PeerAddArgs { profile, mut peer }): DeserializeStdin<PeerAddArgs>,
) -> Result<PeerAddResponse, Error> {
    // Validate peer name before any other processing
    validate_peer_name(&peer.name)?;

    let profile_interface = &profile;
    let wg_interface_name = format!("wg_{}", profile_interface);

    // Generate key pair if public_key not provided (using native Rust crypto)
    let (peer_public_key, client_private_key) = match &peer.public_key {
        Some(pk) => (pk.clone(), None),
        None => {
            let private_key = Base64::new(WgKey::generate());
            let public_key = private_key.public_key();
            (public_key.to_base64(), Some(private_key))
        }
    };

    let preshared_key = peer
        .preshared_key
        .get_or_insert_with(|| Base64::new(generate_psk()).to_base64())
        .clone();

    // Update peer with the public key (in case it was generated)
    peer.public_key = Some(peer_public_key.clone());

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(
            "/etc/config",
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await?;

        // Verify the profile exists and get its gateway IP
        let profile_lookup = profiles::Lookup::parse(ServerContext::default(), &cfgs)?;
        let profile_id = profile_lookup
            .from_interface(profile_interface)
            .ok_or_else(|| {
                Error::new(
                    eyre!("missing profile: interface={}", profile_interface),
                    ErrorKind::MissingProfile,
                )
            })?;
        let profile_name = profile_id.fullname.clone();

        if !vpn_servers(&cfgs)
            .iter()
            .any(|meta| meta.interface == wg_interface_name)
        {
            return Err(Error::new(
                eyre!("VPN server not found for profile: {}", profile_interface),
                ErrorKind::NotFound,
            ));
        }
        if peer_by_key(&cfgs, &wg_interface_name, &peer_public_key).is_ok() {
            return Err(Error::new(
                eyre!("Peer with public key {} already exists", peer_public_key),
                ErrorKind::Duplicate,
            ));
        }

        let placed = place_peer(
            &mut cfgs,
            &arena,
            &wg_interface_name,
            &mut peer,
            &ServerContext::default().wan_ipv4_addrs(),
        )?;
        let peer_ip = placed.ip;

        let private_key = client_private_key.as_ref().map(|k| k.to_base64());
        let client_config = render_client_config(
            &cfgs,
            &wg_interface_name,
            &peer.name,
            peer_ip,
            peer.route_all == Some(true),
            private_key.as_deref(),
            Some(&preshared_key),
        )?;

        match dump_all("/etc/config", cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log(
                    "vpn-server",
                    "peer-added",
                    false,
                    &format!(
                        "Failed to add peer '{}' to inbound VPN for profile '{}'",
                        peer.name, profile_name
                    ),
                    Some(&err.to_string()),
                );
                return Err(err.into());
            }
            Ok(()) => {
                placed.activate(&wg_interface_name).await?;
                if peer.allow_dns_injection == Some(true) {
                    crate::dns_inject::permission_changed();
                }

                crate::activity::log(
                    "vpn-server",
                    "peer-added",
                    true,
                    &format!(
                        "Added peer '{}' to inbound VPN for profile '{}'",
                        peer.name, profile_name
                    ),
                    None,
                );
                return Ok(PeerAddResponse {
                    client_config,
                    public_key: peer_public_key,
                    ip: peer_ip,
                });
            }
        }
    }
}

/// Delete a peer from a VPN server
#[instrument(skip_all)]
pub async fn peer_delete(_ctx: ServerContext, args: PeerDeleteArgs) -> Result<(), Error> {
    let profile_interface = &args.profile;
    let wg_interface_name = format!("wg_{}", profile_interface);
    let public_key = &args.public_key;

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        // `dhcp` gates the server's v6 address in sync_server_addresses.
        let mut cfgs = parse_all("/etc/config", &arena, &["network", "startwrt", "dhcp"]).await?;

        // Resolve profile fullname before modifying configs
        let profile_name = {
            let profile_lookup = profiles::Lookup::parse(ServerContext::default(), &cfgs)?;
            profile_lookup
                .from_interface(profile_interface)
                .map(|p| p.fullname.clone())
                .unwrap_or_else(|| profile_interface.clone())
        };

        // Verify the VPN server exists for this profile
        let vpn_exists = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnServer>().ok())
            .any(|meta| meta.interface == wg_interface_name);

        if !vpn_exists {
            return Err(Error::new(
                eyre!("VPN server not found for profile: {}", profile_interface),
                ErrorKind::NotFound,
            ));
        }

        remove_peer_section(&mut cfgs, &wg_interface_name, public_key)?;

        sync_server_addresses(&mut cfgs, &wg_interface_name)?;
        sync_peer_policy_routes(&mut cfgs, profile_interface)?;
        sync_vpn_peer_cross_routes(&mut cfgs)?;
        sync_proxy_arp(&mut cfgs, profile_interface)?;

        let device = profile_device(&cfgs, profile_interface);
        let proxy_arp = proxy_arp_wanted(&cfgs, profile_interface);

        match dump_all("/etc/config", cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log(
                    "vpn-server",
                    "peer-deleted",
                    false,
                    &format!(
                        "Failed to delete peer from inbound VPN for profile '{}'",
                        profile_name
                    ),
                    Some(&err.to_string()),
                );
                return Err(err.into());
            }
            Ok(()) => {
                // Restart the WireGuard interface to apply peer changes immediately
                restart_wireguard_interface(&wg_interface_name).await?;
                warn_guard(apply_vpn_peer_guard("/etc/config").await);
                if let Some(device) = &device {
                    apply_proxy_arp_sysctl(device, proxy_arp).await;
                }
                // Drops the peer's records, and rebinds to the server's addresses.
                crate::dns_inject::permission_changed();
                crate::activity::log(
                    "vpn-server",
                    "peer-deleted",
                    true,
                    &format!(
                        "Deleted peer from inbound VPN for profile '{}'",
                        profile_name
                    ),
                    None,
                );
                return Ok(());
            }
        }
    }
}

/// A peer's client configuration, with placeholders for its keys
#[instrument(skip_all)]
pub async fn peer_config(
    _ctx: ServerContext,
    args: PeerConfigArgs,
) -> Result<PeerConfigResponse, Error> {
    let wg_interface_name = format!("wg_{}", args.profile);
    let arena = Arena::new();
    let cfgs = parse_all(
        "/etc/config",
        &arena,
        &["network", "startwrt", "firewall", "dhcp"],
    )
    .await?;
    let peer = peer_by_key(&cfgs, &wg_interface_name, &args.public_key)?;
    let ip = peer.ip.ok_or_else(|| {
        Error::new(
            eyre!("Peer {} has no IPv4 address", args.public_key),
            ErrorKind::CorruptedProfile,
        )
    })?;
    Ok(PeerConfigResponse {
        client_config: render_client_config(
            &cfgs,
            &wg_interface_name,
            &peer.name,
            ip,
            peer.route_all == Some(true),
            None,
            None,
        )?,
    })
}

/// Replaces a peer's keys and address, keeping its name, routing and DNS
/// permission. The new address is in the server's peer subnet.
#[instrument(skip_all)]
pub async fn peer_reissue(
    _ctx: ServerContext,
    args: PeerReissueArgs,
) -> Result<PeerAddResponse, Error> {
    let profile_interface = &args.profile;
    let wg_interface_name = format!("wg_{}", profile_interface);
    let private_key = Base64::new(WgKey::generate());
    let public_key = private_key.public_key().to_base64();
    let private_key = private_key.to_base64();
    let preshared_key = Base64::new(generate_psk()).to_base64();

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(
            "/etc/config",
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await?;
        let profile_name = profiles::Lookup::parse(ServerContext::default(), &cfgs)?
            .from_interface(profile_interface)
            .map(|p| p.fullname.clone())
            .unwrap_or_else(|| profile_interface.clone());

        let old = peer_by_key(&cfgs, &wg_interface_name, &args.public_key)?;
        remove_peer_section(&mut cfgs, &wg_interface_name, &args.public_key)?;
        let mut peer = VpnServerPeer {
            name: old.name,
            ip: None,
            public_key: Some(public_key.clone()),
            preshared_key: Some(preshared_key.clone()),
            route_all: old.route_all,
            allow_dns_injection: old.allow_dns_injection,
            legacy_address: None,
        };
        let placed = place_peer(
            &mut cfgs,
            &arena,
            &wg_interface_name,
            &mut peer,
            &ServerContext::default().wan_ipv4_addrs(),
        )?;
        let client_config = render_client_config(
            &cfgs,
            &wg_interface_name,
            &peer.name,
            placed.ip,
            peer.route_all == Some(true),
            Some(&private_key),
            Some(&preshared_key),
        )?;
        let device = profile_device(&cfgs, profile_interface);
        let proxy_arp = proxy_arp_wanted(&cfgs, profile_interface);

        match dump_all("/etc/config", cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log(
                    "vpn-server",
                    "peer-reissued",
                    false,
                    &format!(
                        "Failed to reissue peer '{}' of inbound VPN for profile '{}'",
                        peer.name, profile_name
                    ),
                    Some(&err.to_string()),
                );
                return Err(err.into());
            }
            Ok(()) => {
                warn_guard(sync_vpn_peer_guard("/etc/config").await);
                placed.activate(&wg_interface_name).await?;
                if !placed.new_subnet {
                    warn_guard(crate::published_ports::reload_firewall_wait().await);
                }
                if let Some(device) = &device {
                    apply_proxy_arp_sysctl(device, proxy_arp).await;
                }
                // Drops the old key's records, and rebinds to the server's addresses.
                crate::dns_inject::permission_changed();
                crate::activity::log(
                    "vpn-server",
                    "peer-reissued",
                    true,
                    &format!(
                        "Reissued peer '{}' of inbound VPN for profile '{}'",
                        peer.name, profile_name
                    ),
                    None,
                );
                return Ok(PeerAddResponse {
                    client_config,
                    public_key,
                    ip: placed.ip,
                });
            }
        }
    }
}

/// Rename a peer of a VPN server
#[instrument(skip_all)]
pub async fn peer_rename(_ctx: ServerContext, args: PeerRenameArgs) -> Result<(), Error> {
    validate_peer_name(&args.name)?;

    let profile_interface = &args.profile;
    let wg_interface_name = format!("wg_{}", profile_interface);

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all("/etc/config", &arena, &["network", "startwrt"]).await?;

        let profile_name = {
            let profile_lookup = profiles::Lookup::parse(ServerContext::default(), &cfgs)?;
            profile_lookup
                .from_interface(profile_interface)
                .map(|p| p.fullname.clone())
                .unwrap_or_else(|| profile_interface.clone())
        };

        let vpn_exists = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnServer>().ok())
            .any(|meta| meta.interface == wg_interface_name);

        if !vpn_exists {
            return Err(Error::new(
                eyre!("VPN server not found for profile: {}", profile_interface),
                ErrorKind::NotFound,
            ));
        }

        rename_peer_section(
            &mut cfgs,
            &arena,
            &wg_interface_name,
            &args.public_key,
            &args.name,
        )?;

        match dump_all("/etc/config", cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log(
                    "vpn-server",
                    "updated",
                    false,
                    &format!(
                        "Failed to rename peer of inbound VPN for profile '{}'",
                        profile_name
                    ),
                    Some(&err.to_string()),
                );
                return Err(err.into());
            }
            Ok(()) => {
                crate::activity::log(
                    "vpn-server",
                    "updated",
                    true,
                    &format!(
                        "Renamed peer to '{}' on inbound VPN for profile '{}'",
                        args.name, profile_name
                    ),
                    None,
                );
                return Ok(());
            }
        }
    }
}

/// Sets an inbound peer's DNS-injection permission on its WireGuard peer
/// section. Revoking it drops the records the peer has published.
#[instrument(skip_all)]
pub async fn peer_set_dns_injection<C: CtrlContext>(
    ctx: C,
    args: PeerSetDnsInjectionArgs,
) -> Result<(), Error> {
    let peer_type = format!("wireguard_wg_{}", args.profile);
    let log_failure = |err: &Error| {
        crate::activity::log(
            "vpn-server",
            "peer-dns-injection",
            false,
            "Failed to update DNS injection for inbound VPN peer",
            Some(&err.to_string()),
        );
    };
    let mut retries = 4;
    let peer_name = loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["network"]).await?;
        let section = cfgs["network"]
            .sections
            .iter_mut()
            .find(|section| {
                section.ty() == peer_type
                    && section.lines.iter().any(|line| {
                        matches!(line, Line::Option { option, value, .. }
                            if option.as_str() == "public_key"
                                && value.as_str() == args.public_key.as_str())
                    })
            })
            .ok_or_else(|| {
                Error::new(
                    eyre!("Peer with public key {} not found", args.public_key),
                    ErrorKind::NotFound,
                )
            })?;
        if crate::dns_inject::peer_allows_dns_inject(section) == args.allow {
            return Ok(());
        }
        crate::dns_inject::set_peer_dns_inject(section, args.allow);
        let name = section
            .lines
            .iter()
            .find_map(|line| match line {
                Line::Option { option, value, .. } if option.as_str() == "description" => {
                    Some(value.as_str().to_string())
                }
                _ => None,
            })
            .unwrap_or_else(|| section.name().unwrap_or_default().to_string());
        match dump_all(ctx.uci_root(), cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => retries -= 1,
            Err(err) => {
                let err = err.into();
                log_failure(&err);
                return Err(err);
            }
            Ok(()) => break name,
        }
    };
    crate::dns_inject::permission_changed();
    crate::activity::log(
        "vpn-server",
        "peer-dns-injection",
        true,
        &format!(
            "{} DNS injection for inbound VPN peer '{peer_name}'",
            if args.allow { "Enabled" } else { "Disabled" }
        ),
        None,
    );
    Ok(())
}

// === Helper Functions ===

fn is_peer_with_key(section: &Section, peer_type: &str, public_key: &str) -> bool {
    section.ty() == peer_type
        && section.lines.iter().any(|line| {
            matches!(line, Line::Option { option, value, .. }
                if option.as_str() == "public_key" && value.as_str() == public_key)
        })
}

fn peer_not_found(public_key: &str) -> Error {
    Error::new(
        eyre!("Peer with public key {} not found", public_key),
        ErrorKind::NotFound,
    )
}

fn peer_by_key(
    cfgs: &Configs,
    wg_interface_name: &str,
    public_key: &str,
) -> Result<VpnServerPeer, Error> {
    get_peers_for_interface(cfgs, wg_interface_name)
        .into_iter()
        .find(|peer| peer.public_key.as_deref() == Some(public_key))
        .ok_or_else(|| peer_not_found(public_key))
}

fn remove_peer_section(
    cfgs: &mut Configs,
    wg_interface_name: &str,
    public_key: &str,
) -> Result<(), Error> {
    let peer_type = format!("wireguard_{wg_interface_name}");
    let sections = &mut cfgs["network"].sections;
    let index = sections
        .iter()
        .position(|s| is_peer_with_key(s, &peer_type, public_key))
        .ok_or_else(|| peer_not_found(public_key))?;
    sections.remove(index);
    Ok(())
}

/// Where [`place_peer`] put a peer.
struct PlacedPeer {
    ip: Ipv4Addr,
    /// The server gained its peer subnet, whose policy rule only a network reload installs.
    new_subnet: bool,
}

impl PlacedPeer {
    async fn activate(&self, wg_interface_name: &str) -> Result<(), Error> {
        restart_wireguard_interface(wg_interface_name).await?;
        if self.new_subnet {
            reload_system().await?;
            crate::dns_inject::permission_changed();
        }
        Ok(())
    }
}

/// Adds a peer at its requested address, else the next free one, in the
/// server's peer subnet, with the addresses, routes and proxy ARP that follow.
fn place_peer<'a>(
    cfgs: &mut Configs<'a>,
    arena: &'a Arena,
    wg_interface_name: &str,
    peer: &mut VpnServerPeer,
    wan: &[Ipv4Addr],
) -> Result<PlacedPeer, Error> {
    let new_subnet = peer_subnet_of(cfgs, wg_interface_name).is_none();
    let subnet = ensure_peer_subnet(cfgs, wg_interface_name, wan)?;
    let used = used_peer_hosts(cfgs, wg_interface_name);
    let ip = match peer.ip {
        Some(ip) => {
            validate_peer_ip(ip, subnet, &used)?;
            ip
        }
        None => allocate_peer_ip(subnet, &used)?,
    };
    peer.ip = Some(ip);
    add_single_peer(cfgs, wg_interface_name, peer, arena)?;
    let profile_interface = profile_of_server(wg_interface_name);
    sync_server_addresses(cfgs, wg_interface_name)?;
    sync_peer_policy_routes(cfgs, profile_interface)?;
    sync_vpn_peer_cross_routes(cfgs)?;
    sync_proxy_arp(cfgs, profile_interface)?;
    Ok(PlacedPeer { ip, new_subnet })
}

/// Whether the server routes the peer's address in the WG /64 to it.
fn peer_routes_v6(cfgs: &Configs, wg_interface_name: &str, ip: Ipv4Addr, groups: [u16; 4]) -> bool {
    let peer_type = format!("wireguard_{wg_interface_name}");
    let v4 = format!("{ip}/32");
    let v6 = format!("{}/128", wg_peer_v6_addr(groups, ip.octets()[3]));
    cfgs["network"]
        .sections
        .iter()
        .filter(|section| section.ty() == peer_type)
        .any(|section| {
            let allows = |cidr: &str| {
                section.lines.iter().any(|line| {
                    matches!(line, Line::List { list, item, .. }
                        if list.as_str() == "allowed_ips" && item.as_str() == cidr)
                })
            };
            allows(&v4) && allows(&v6)
        })
}

/// A client configuration for a peer of the server. Absent keys render as
/// placeholders. IPv6 appears only when the server routes it to the peer.
fn render_client_config(
    cfgs: &Configs,
    wg_interface_name: &str,
    name: &str,
    ip: Ipv4Addr,
    route_all: bool,
    private_key: Option<&str>,
    preshared_key: Option<&str>,
) -> Result<String, Error> {
    let profile_interface = profile_of_server(wg_interface_name);
    let gateway_ip = get_gateway_ip(cfgs, profile_interface)?;
    let meta = vpn_servers(cfgs)
        .into_iter()
        .find(|meta| meta.interface == wg_interface_name)
        .ok_or_else(|| {
            Error::new(
                eyre!("VPN server not found for profile: {}", profile_interface),
                ErrorKind::NotFound,
            )
        })?;
    let server_public_key = cfgs["network"]
        .sections
        .iter()
        .find(|s| s.name().as_deref() == Some(wg_interface_name))
        .and_then(|s| s.get::<WgInterface>().ok())
        .filter(|wg| wg.is_wireguard())
        .map(|wg| derive_public_key(&wg.private_key))
        .transpose()?
        .ok_or_else(|| Error::new(eyre!("WireGuard interface not found"), ErrorKind::NotFound))?;
    let v6_groups = wg_server_v6_groups(cfgs, profile_interface)
        .filter(|groups| peer_routes_v6(cfgs, wg_interface_name, ip, *groups));
    let allowed_ips = client_allowed_ips(
        cfgs,
        profile_interface,
        gateway_ip,
        ip,
        route_all,
        peer_subnet_of(cfgs, wg_interface_name),
        v6_groups,
    )?;
    Ok(ClientConfig {
        name,
        private_key,
        address: &client_address_line(ip, v6_groups),
        dns: gateway_ip,
        server_public_key: &server_public_key,
        preshared_key,
        endpoint: &meta.endpoint,
        port: meta.listen_port,
        allowed_ips: &allowed_ips,
    }
    .render())
}

/// Sets the `description` of the peer with the given public key.
fn rename_peer_section<'a>(
    cfgs: &mut Configs<'a>,
    arena: &'a Arena,
    interface_name: &str,
    public_key: &str,
    name: &str,
) -> Result<(), Error> {
    let peer_type = format!("wireguard_{}", interface_name);
    let section = cfgs["network"]
        .sections
        .iter_mut()
        .filter(|s| s.ty() == peer_type)
        .find(|s| {
            s.lines.iter().any(|line| {
                matches!(line, Line::Option { option, value, .. }
                    if option.as_str() == "public_key" && value.as_str() == public_key)
            })
        })
        .ok_or_else(|| {
            Error::new(
                eyre!("Peer with public key {} not found", public_key),
                ErrorKind::NotFound,
            )
        })?;

    let name_str: &'a str = arena.alloc(name.to_string());
    let existing = section.lines.iter_mut().find_map(|line| match line {
        Line::Option { option, value, .. } if option.as_str() == "description" => Some(value),
        _ => None,
    });
    match existing {
        Some(value) => *value = Token::from_str(name_str, arena),
        None => section.lines.push(Line::Option {
            option: Token::from_str("description", arena),
            value: Token::from_str(name_str, arena),
            comment: LineComment::None,
        }),
    }
    Ok(())
}

fn add_single_peer<'a>(
    cfgs: &mut Configs<'a>,
    interface_name: &str,
    peer: &VpnServerPeer,
    arena: &'a Arena,
) -> Result<(), Error> {
    let peer_type = format!("wireguard_{}", interface_name);
    let peer_type_str: &str = arena.alloc(peer_type);

    // Generate a unique peer section name using max existing suffix + 1
    // Using count would collide after deletions (e.g., delete peer 1 of 3, count=2, collides with peer 2)
    let prefix = format!("{}_", interface_name);
    let next_index = cfgs["network"]
        .sections
        .iter()
        .filter(|s| s.ty() == peer_type_str)
        .filter_map(|s| s.name()?.strip_prefix(&prefix)?.parse::<u32>().ok())
        .max()
        .map(|n| n + 1)
        .unwrap_or(0);
    let peer_name = format!("{}{}", prefix, next_index);
    let peer_name_str: &str = arena.alloc(peer_name);

    let mut lines = vec![Line::Section {
        ty: Token::from_str(peer_type_str, arena),
        name: Some(Token::from_str(peer_name_str, arena)),
        comment: LineComment::None,
    }];

    // Add public_key option (must be set by caller)
    let public_key = peer.public_key.as_ref().ok_or_else(|| {
        Error::new(
            eyre!("public_key must be set when adding peer"),
            ErrorKind::InvalidValue,
        )
    })?;
    let pub_key_str: &str = arena.alloc(public_key.clone());
    lines.push(Line::Option {
        option: Token::from_str("public_key", arena),
        value: Token::from_str(pub_key_str, arena),
        comment: LineComment::None,
    });

    // Add description option (for peer name)
    let desc_str: &str = arena.alloc(peer.name.clone());
    lines.push(Line::Option {
        option: Token::from_str("description", arena),
        value: Token::from_str(desc_str, arena),
        comment: LineComment::None,
    });

    // Add preshared_key if present
    if let Some(ref psk) = peer.preshared_key {
        let psk_str: &str = arena.alloc(psk.clone());
        lines.push(Line::Option {
            option: Token::from_str("preshared_key", arena),
            value: Token::from_str(psk_str, arena),
            comment: LineComment::None,
        });
    }

    // Add persistent_keepalive (always 25 for mobile clients)
    let keepalive_str: &str = arena.alloc("25".to_string());
    lines.push(Line::Option {
        option: Token::from_str("persistent_keepalive", arena),
        value: Token::from_str(keepalive_str, arena),
        comment: LineComment::None,
    });

    // Store route_all flag (only when true, absence = split tunnel)
    if peer.route_all == Some(true) {
        lines.push(Line::Option {
            option: Token::from_str("startwrt_route_all", arena),
            value: Token::from_str("1", arena),
            comment: LineComment::None,
        });
    }

    // Enable route creation for this peer's allowed_ips
    lines.push(Line::Option {
        option: Token::from_str("route_allowed_ips", arena),
        value: Token::from_str("1", arena),
        comment: LineComment::None,
    });

    // Add allowed_ips with the peer's assigned IP
    let ip = peer
        .ip
        .ok_or_else(|| Error::new(eyre!("peer IP must be set"), ErrorKind::InvalidValue))?;
    let ip_str: &str = arena.alloc(format!("{}/32", ip));
    lines.push(Line::List {
        list: Token::from_str("allowed_ips", arena),
        item: Token::from_str(ip_str, arena),
        comment: LineComment::None,
    });

    // When the host profile serves IPv6, also allow the peer's WG-/64 ULA /128.
    // route_allowed_ips='1' (set above) then installs this /128 in the main
    // table, which is how the router and other profiles reach the peer over v6.
    let profile_interface = interface_name.strip_prefix("wg_").unwrap_or(interface_name);
    if let Some(groups) = wg_server_v6_groups(cfgs, profile_interface) {
        let v6_str: &str = arena.alloc(format!("{}/128", wg_peer_v6_addr(groups, ip.octets()[3])));
        lines.push(Line::List {
            list: Token::from_str("allowed_ips", arena),
            item: Token::from_str(v6_str, arena),
            comment: LineComment::None,
        });
    }

    let mut section = Section { arena, lines };
    crate::dns_inject::set_peer_dns_inject(&mut section, peer.allow_dns_injection == Some(true));
    cfgs["network"].sections.push(section);

    Ok(())
}

fn get_peers_for_interface(cfgs: &Configs, interface_name: &str) -> Vec<VpnServerPeer> {
    let peer_type = format!("wireguard_{}", interface_name);
    let gateway = get_gateway_ip(cfgs, profile_of_server(interface_name)).ok();

    cfgs["network"]
        .sections
        .iter()
        .filter(|section| section.ty() == peer_type)
        .filter_map(|section| {
            let mut public_key = String::new();
            let mut ip: Option<Ipv4Addr> = None;
            let mut description = None;
            let mut route_all = false;

            for line in &section.lines {
                match line {
                    Line::Option { option, value, .. } => match option.as_str().as_ref() {
                        "public_key" => public_key = value.as_str().to_string(),
                        "description" => description = Some(value.as_str().to_string()),
                        "startwrt_route_all" => route_all = value.as_str() == "1",
                        _ => {}
                    },
                    Line::List { list, item, .. } => {
                        if list.as_str() == "allowed_ips" {
                            // Parse the v4 host from "x.x.x.x/32"; ignore the v6
                            // "<ula>/128" entry (parse fails, must not clobber).
                            let ip_str = item.as_str();
                            if let Some(ip_part) = ip_str.split('/').next() {
                                if let Ok(v4) = ip_part.parse::<Ipv4Addr>() {
                                    ip = Some(v4);
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

            Some(VpnServerPeer {
                name: description.unwrap_or_else(|| section.name().unwrap_or_default().to_string()),
                ip,
                public_key: Some(public_key),
                // Never expose PSK in list responses — it's only needed at peer creation time
                preshared_key: None,
                route_all: if route_all { Some(true) } else { None },
                allow_dns_injection: crate::dns_inject::peer_allows_dns_inject(section)
                    .then_some(true),
                legacy_address: ip
                    .zip(gateway)
                    .is_some_and(|(ip, gateway)| same_24(ip, gateway))
                    .then_some(true),
            })
        })
        .collect()
}

/// Creates or updates the server interface, then reconciles its addresses
/// with [`sync_server_addresses`].
fn set_wireguard_interface(
    cfgs: &mut Configs,
    interface_name: &str,
    config: &VpnServerConfig,
) -> Result<(), Error> {
    let mut found = false;

    for section in &mut cfgs["network"].sections {
        if section.name().as_deref() != Some(interface_name) {
            continue;
        }

        let Ok(mut wg_iface) = section.get::<WgInterface>() else {
            continue;
        };

        if !wg_iface.is_wireguard() {
            continue;
        }

        // Update existing interface - keep existing private_key if not provided
        if let Some(ref new_key) = config.private_key {
            wg_iface.private_key = new_key.clone();
        }
        wg_iface.listen_port = if config.enabled {
            Some(config.listen_port)
        } else {
            None
        };
        section.set(&wg_iface)?;
        found = true;
        break;
    }

    if !found {
        // Create new interface - generate private_key if not provided (using native Rust crypto)
        let private_key = match &config.private_key {
            Some(key) => key.clone(),
            None => Base64::new(WgKey::generate()).to_base64(),
        };

        let new_iface = WgInterface {
            proto: "wireguard".to_string(),
            private_key,
            listen_port: if config.enabled {
                Some(config.listen_port)
            } else {
                None
            },
            addresses: Vec::new(),
            disabled: None,
            mtu: None,
        };
        cfgs["network"].append(&new_iface, Some(interface_name))?;
    }

    sync_server_addresses(cfgs, interface_name)
}

fn set_vpn_server_metadata(
    cfgs: &mut Configs,
    interface_name: &str,
    profile_interface: &str,
    config: &VpnServerConfig,
) -> Result<(), Error> {
    let mut found = false;

    for section in &mut cfgs["startwrt"].sections {
        let Ok(mut meta) = section.get::<UciVpnServer>() else {
            continue;
        };

        if meta.interface != interface_name {
            continue;
        }

        // Update existing metadata
        meta.label = config.label.clone();
        meta.listen_port = config.listen_port;
        meta.endpoint = config.endpoint.clone();
        meta.profile_interface = profile_interface.to_string();
        section.set(&meta)?;
        found = true;
        break;
    }

    if !found {
        // Create new metadata
        let meta = UciVpnServer {
            interface: interface_name.to_string(),
            profile_interface: profile_interface.to_string(),
            label: config.label.clone(),
            listen_port: config.listen_port,
            endpoint: config.endpoint.clone(),
            peer_subnet: None,
        };
        cfgs["startwrt"].append(&meta, Some(interface_name))?;
    }

    Ok(())
}

fn ensure_firewall_zone(
    cfgs: &mut Configs,
    wg_interface_name: &str,
    profile_interface: &str,
) -> Result<(), Error> {
    // Find the firewall zone for the profile
    let zone_name = format!("vlan_{}", profile_interface);

    for section in &mut cfgs["firewall"].sections {
        let Ok(mut zone) = section.get::<uciedit::openwrt::FirewallZone>() else {
            continue;
        };

        // Check if this is the profile's zone (by name or by containing the profile interface)
        let is_profile_zone =
            zone.name == zone_name || zone.network.iter().any(|n| n == profile_interface);

        if !is_profile_zone {
            continue;
        }

        // Add the WireGuard interface to this zone if not already present
        if !zone.network.iter().any(|n| n == wg_interface_name) {
            zone.network.push(wg_interface_name.to_string());
            section.set(&zone)?;
        }

        return Ok(());
    }

    // Zone not found - this shouldn't happen if the profile exists
    Err(Error::new(
        eyre!("missing firewall zone for interface: {}", profile_interface),
        ErrorKind::MissingFirewallZone,
    ))
}

fn remove_from_firewall_zones(cfgs: &mut Configs, wg_interface_name: &str) -> Result<(), Error> {
    for section in &mut cfgs["firewall"].sections {
        let Ok(mut zone) = section.get::<uciedit::openwrt::FirewallZone>() else {
            continue;
        };

        // Remove the WireGuard interface from this zone
        let original_len = zone.network.len();
        zone.network.retain(|n| n != wg_interface_name);

        if zone.network.len() != original_len {
            section.set(&zone)?;
        }
    }

    Ok(())
}

/// Firewall rule name for a WireGuard VPN server (used in the 'name' option)
fn wireguard_rule_name(wg_interface_name: &str) -> String {
    format!("Allow-WireGuard-{}", wg_interface_name)
}

/// UCI section name for the firewall rule (must be alphanumeric/underscore only)
fn wireguard_rule_section_name(wg_interface_name: &str) -> String {
    format!("allow_wireguard_{}", wg_interface_name)
}

/// Ensure a firewall rule exists to allow WireGuard traffic on the WAN interface.
/// This is required because the WAN zone typically has input=REJECT.
fn ensure_wireguard_firewall_rule(
    cfgs: &mut Configs,
    wg_interface_name: &str,
    listen_port: u16,
) -> Result<(), Error> {
    use uciedit::openwrt::{FirewallRule, FirewallTarget};

    let rule_name = wireguard_rule_name(wg_interface_name);
    let port_str = listen_port.to_string();

    // Check if rule already exists and update it
    for section in &mut cfgs["firewall"].sections {
        let Ok(mut rule) = section.get::<FirewallRule>() else {
            continue;
        };

        if rule.name == rule_name {
            // Update existing rule with new port if changed
            rule.dest_port = Some(port_str.clone());
            section.set(&rule)?;
            return Ok(());
        }
    }

    // Create new rule
    let rule = FirewallRule {
        name: rule_name.clone(),
        src: "wan".to_string(),
        src_ip: None,
        src_mac: None,
        src_port: None,
        dest: None,
        dest_ip: None,
        dest_port: Some(port_str),
        proto: vec!["udp".to_string()],
        target: FirewallTarget::ACCEPT,
        family: None,
        enabled: None,
        set_mark: None,
        _pp_id: None,
        _pp_mac: None,
        _apf_label: None,
        _pp_hairpin: None,
    };

    // Use a valid UCI section name (alphanumeric and underscores only)
    let section_name = wireguard_rule_section_name(wg_interface_name);
    cfgs["firewall"].append(&rule, Some(&section_name))?;

    Ok(())
}

/// Remove the firewall rule for a WireGuard VPN server.
pub(crate) fn remove_wireguard_firewall_rule(
    cfgs: &mut Configs,
    wg_interface_name: &str,
) -> Result<(), Error> {
    use uciedit::openwrt::FirewallRule;

    let rule_name = wireguard_rule_name(wg_interface_name);

    cfgs["firewall"].sections.retain(|section| {
        if let Ok(rule) = section.get::<FirewallRule>() {
            return rule.name != rule_name;
        }
        true
    });

    Ok(())
}

/// Derive public key from a base64-encoded private key using native Rust crypto.
fn derive_public_key(private_key: &str) -> Result<String, Error> {
    let key: Base64<WgKey> = private_key.parse()?;
    Ok(key.public_key().to_base64())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use rpc_toolkit::Context;
    use tokio::runtime::Runtime;

    use super::*;
    use crate::CtrlContext;

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

    /// Write base configs: two profiles (Admin/lan/vlan99, Guest/guest/vlan101)
    /// with network interfaces, firewall zones, and DHCP. No VPN server.
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

        std::fs::write(
            dir.join("firewall"),
            "\
config zone
\toption name 'lan'
\tlist network 'lan'
\toption input 'ACCEPT'
\toption output 'ACCEPT'
\toption forward 'ACCEPT'

config zone
\toption name 'wan'
\tlist network 'wan'
\toption input 'REJECT'
\toption output 'ACCEPT'
\toption forward 'REJECT'

config zone
\toption name 'vlan_guest'
\tlist network 'guest'
\toption input 'ACCEPT'
\toption output 'ACCEPT'
\toption forward 'ACCEPT'

config forwarding
\toption src 'lan'
\toption dest 'wan'

config forwarding
\toption src 'vlan_guest'
\toption dest 'wan'
",
        )
        .unwrap();

        std::fs::write(
            dir.join("dhcp"),
            "\
config dhcp 'lan'
\toption interface 'lan'
\toption start '100'
\toption limit '150'
\toption leasetime '12h'

config dhcp 'guest'
\toption interface 'guest'
\toption start '100'
\toption limit '150'
\toption leasetime '12h'
",
        )
        .unwrap();
    }

    /// Write configs that include a VPN server for Guest profile.
    fn setup_with_vpn_server(dir: &Path) -> (String, String, String) {
        let server_key = gen_key();
        let peer0_key = gen_key();
        let peer1_key = gen_key();

        std::fs::write(
            dir.join("startwrt"),
            format!(
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

config interface 'wg_guest'
\toption proto 'wireguard'
\toption private_key '{server_key}'
\toption listen_port '51820'
\tlist addresses '192.168.101.254/32'

config wireguard_wg_guest 'wg_guest_0'
\toption public_key '{peer0_key}'
\toption description 'Phone'
\toption persistent_keepalive '25'
\toption route_allowed_ips '1'
\tlist allowed_ips '192.168.101.200/32'

config wireguard_wg_guest 'wg_guest_1'
\toption public_key '{peer1_key}'
\toption description 'Laptop'
\toption persistent_keepalive '25'
\toption route_allowed_ips '1'
\tlist allowed_ips '192.168.101.201/32'
"
            ),
        )
        .unwrap();

        std::fs::write(
            dir.join("firewall"),
            "\
config zone
\toption name 'lan'
\tlist network 'lan'
\toption input 'ACCEPT'
\toption output 'ACCEPT'
\toption forward 'ACCEPT'

config zone
\toption name 'wan'
\tlist network 'wan'
\toption input 'REJECT'
\toption output 'ACCEPT'
\toption forward 'REJECT'

config zone
\toption name 'vlan_guest'
\tlist network 'guest'
\tlist network 'wg_guest'
\toption input 'ACCEPT'
\toption output 'ACCEPT'
\toption forward 'ACCEPT'

config forwarding
\toption src 'lan'
\toption dest 'wan'

config forwarding
\toption src 'vlan_guest'
\toption dest 'wan'

config rule 'allow_wireguard_wg_guest'
\toption name 'Allow-WireGuard-wg_guest'
\toption src 'wan'
\toption dest_port '51820'
\tlist proto 'udp'
\toption target 'ACCEPT'
",
        )
        .unwrap();

        std::fs::write(
            dir.join("dhcp"),
            "\
config dhcp 'lan'
\toption interface 'lan'
\toption start '100'
\toption limit '150'
\toption leasetime '12h'

config dhcp 'guest'
\toption interface 'guest'
\toption start '100'
\toption limit '150'
\toption leasetime '12h'
",
        )
        .unwrap();

        (server_key, peer0_key, peer1_key)
    }

    // === Pure function tests ===

    #[test]
    fn client_address_carries_the_profile_prefix() {
        let peer = Ipv4Addr::new(192, 168, 101, 200);
        assert_eq!(client_address_line(peer, None), "192.168.101.200/24");
        assert_eq!(
            client_address_line(peer, Some([0xfdaa, 0xbbbb, 0xcccc, 0xf065])),
            format!(
                "192.168.101.200/24, {}/128",
                wg_peer_v6_addr([0xfdaa, 0xbbbb, 0xcccc, 0xf065], 200)
            ),
        );
    }

    #[test]
    fn test_derive_server_address() {
        let gateway = Ipv4Addr::new(192, 168, 101, 1);
        let server = derive_server_address(gateway);
        assert_eq!(server, Ipv4Addr::new(192, 168, 101, 254));
    }

    #[test]
    fn test_validate_endpoint_valid() {
        assert!(validate_endpoint("vpn.example.com").is_ok());
        assert!(validate_endpoint("1.2.3.4").is_ok());
    }

    #[test]
    fn test_validate_endpoint_invalid() {
        assert!(validate_endpoint("").is_err());
        assert!(validate_endpoint("has space").is_err());
        assert!(validate_endpoint("has\nnewline").is_err());
    }

    #[test]
    fn test_validate_peer_name_valid() {
        assert!(validate_peer_name("My Phone").is_ok());
    }

    #[test]
    fn test_validate_peer_name_invalid() {
        assert!(validate_peer_name("has\nnewline").is_err());
        assert!(validate_peer_name("").is_err());
        assert!(validate_peer_name("   ").is_err());
    }

    #[test]
    fn test_validate_private_key() {
        let valid_key = gen_key();
        assert!(validate_private_key(&valid_key).is_ok());
        assert!(validate_private_key("not-valid").is_err());
        // Valid base64 but wrong length (only 16 bytes)
        assert!(validate_private_key("dGVzdHRlc3R0ZXN0dGVzdHQ=").is_err());
    }

    #[test]
    fn allocates_peer_hosts_from_two_skipping_used_hosts() {
        let subnet = Ipv4Addr::new(192, 168, 255, 0);
        assert_eq!(
            allocate_peer_ip(subnet, &BTreeSet::new()).unwrap(),
            Ipv4Addr::new(192, 168, 255, 2)
        );
        let used = BTreeSet::from([2, 3]);
        assert_eq!(
            allocate_peer_ip(subnet, &used).unwrap(),
            Ipv4Addr::new(192, 168, 255, 4)
        );
        let full: BTreeSet<u8> = (PEER_HOST_FIRST..=PEER_HOST_LAST).collect();
        assert!(allocate_peer_ip(subnet, &full).is_err());
    }

    #[test]
    fn validates_requested_peer_addresses() {
        let subnet = Ipv4Addr::new(192, 168, 255, 0);
        let used = BTreeSet::from([200]);
        assert!(validate_peer_ip(Ipv4Addr::new(192, 168, 255, 9), subnet, &used).is_ok());
        // Outside the peer subnet, including the profile's own.
        assert!(validate_peer_ip(Ipv4Addr::new(192, 168, 101, 9), subnet, &used).is_err());
        // The router's host and the broadcast address.
        assert!(validate_peer_ip(Ipv4Addr::new(192, 168, 255, 1), subnet, &used).is_err());
        assert!(validate_peer_ip(Ipv4Addr::new(192, 168, 255, 255), subnet, &used).is_err());
        // A legacy peer's host octet is also its IPv6 host id.
        assert!(validate_peer_ip(Ipv4Addr::new(192, 168, 255, 200), subnet, &used).is_err());
    }

    #[test]
    fn parses_only_network_addresses_of_slash_24s() {
        assert_eq!(
            parse_subnet24("10.42.7.0/24"),
            Some(Ipv4Addr::new(10, 42, 7, 0))
        );
        assert_eq!(parse_subnet24("10.42.7.1/24"), None);
        assert_eq!(parse_subnet24("10.42.0.0/16"), None);
        assert_eq!(parse_subnet24("garbage"), None);
    }

    #[test]
    fn test_wireguard_rule_names() {
        assert_eq!(wireguard_rule_name("wg_guest"), "Allow-WireGuard-wg_guest");
        assert_eq!(
            wireguard_rule_section_name("wg_guest"),
            "allow_wireguard_wg_guest"
        );
    }

    // === Config manipulation tests ===

    #[tokio::test]
    async fn test_set_wireguard_interface_creates() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let private_key = gen_key();
        let config = VpnServerConfig {
            label: "Guest VPN".into(),
            enabled: true,
            listen_port: 51820,
            endpoint: "vpn.example.com".into(),
            private_key: Some(private_key.clone()),
        };

        set_wireguard_interface(&mut cfgs, "wg_guest", &config).unwrap();

        // Verify interface was created
        let wg = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("wg_guest"))
            .and_then(|s| s.get::<WgInterface>().ok())
            .expect("wg_guest interface should exist");

        assert_eq!(wg.proto, "wireguard");
        assert_eq!(wg.listen_port, Some(51820));
        assert_eq!(wg.addresses, vec!["192.168.101.254/32"]);
        assert_eq!(wg.private_key, private_key);
    }

    #[tokio::test]
    async fn test_set_wireguard_interface_updates() {
        let dir = tempfile::tempdir().unwrap();
        let (server_key, _, _) = setup_with_vpn_server(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // Update with no private_key → should preserve existing
        let config = VpnServerConfig {
            label: "Guest VPN".into(),
            enabled: true,
            listen_port: 51821, // changed port
            endpoint: "vpn.example.com".into(),
            private_key: None,
        };

        set_wireguard_interface(&mut cfgs, "wg_guest", &config).unwrap();

        let wg = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("wg_guest"))
            .and_then(|s| s.get::<WgInterface>().ok())
            .unwrap();

        assert_eq!(wg.listen_port, Some(51821));
        assert_eq!(
            wg.private_key, server_key,
            "private_key should be preserved when None"
        );

        // Update with new private_key → should replace
        let new_key = gen_key();
        let config2 = VpnServerConfig {
            label: "Guest VPN".into(),
            enabled: true,
            listen_port: 51821,
            endpoint: "vpn.example.com".into(),
            private_key: Some(new_key.clone()),
        };
        set_wireguard_interface(&mut cfgs, "wg_guest", &config2).unwrap();

        let wg2 = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("wg_guest"))
            .and_then(|s| s.get::<WgInterface>().ok())
            .unwrap();

        assert_eq!(
            wg2.private_key, new_key,
            "private_key should be replaced when Some"
        );
    }

    #[tokio::test]
    async fn test_set_wireguard_interface_disabled() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let config = VpnServerConfig {
            label: "Guest VPN".into(),
            enabled: false,
            listen_port: 51820,
            endpoint: "vpn.example.com".into(),
            private_key: Some(gen_key()),
        };

        set_wireguard_interface(&mut cfgs, "wg_guest", &config).unwrap();

        let wg = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("wg_guest"))
            .and_then(|s| s.get::<WgInterface>().ok())
            .unwrap();

        assert_eq!(
            wg.listen_port, None,
            "disabled interface should have no listen_port"
        );
    }

    #[tokio::test]
    async fn test_set_vpn_server_metadata_creates() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let config = VpnServerConfig {
            label: "Guest VPN".into(),
            enabled: true,
            listen_port: 51820,
            endpoint: "vpn.example.com".into(),
            private_key: None,
        };

        set_vpn_server_metadata(&mut cfgs, "wg_guest", "guest", &config).unwrap();

        let meta = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnServer>().ok())
            .find(|m| m.interface == "wg_guest")
            .expect("vpn_server metadata should exist");

        assert_eq!(meta.profile_interface, "guest");
        assert_eq!(meta.label, "Guest VPN");
        assert_eq!(meta.listen_port, 51820);
        assert_eq!(meta.endpoint, "vpn.example.com");
    }

    #[tokio::test]
    async fn test_set_vpn_server_metadata_updates() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let config = VpnServerConfig {
            label: "Updated VPN".into(),
            enabled: true,
            listen_port: 51821,
            endpoint: "new.example.com".into(),
            private_key: None,
        };

        set_vpn_server_metadata(&mut cfgs, "wg_guest", "guest", &config).unwrap();

        let metas: Vec<_> = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnServer>().ok())
            .filter(|m| m.interface == "wg_guest")
            .collect();

        assert_eq!(
            metas.len(),
            1,
            "should have exactly one vpn_server, no duplicate"
        );
        assert_eq!(metas[0].label, "Updated VPN");
        assert_eq!(metas[0].listen_port, 51821);
        assert_eq!(metas[0].endpoint, "new.example.com");
    }

    #[tokio::test]
    async fn test_ensure_firewall_zone() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        ensure_firewall_zone(&mut cfgs, "wg_guest", "guest").unwrap();

        let zone = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<uciedit::openwrt::FirewallZone>().ok())
            .find(|z| z.name == "vlan_guest")
            .expect("vlan_guest zone should exist");

        assert!(
            zone.network.iter().any(|n| n == "wg_guest"),
            "wg_guest should be in zone network list"
        );
        assert!(
            zone.network.iter().any(|n| n == "guest"),
            "guest should still be in zone network list"
        );
    }

    #[tokio::test]
    async fn test_ensure_firewall_zone_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        ensure_firewall_zone(&mut cfgs, "wg_guest", "guest").unwrap();
        ensure_firewall_zone(&mut cfgs, "wg_guest", "guest").unwrap();

        let zone = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<uciedit::openwrt::FirewallZone>().ok())
            .find(|z| z.name == "vlan_guest")
            .unwrap();

        let wg_count = zone.network.iter().filter(|n| *n == "wg_guest").count();
        assert_eq!(
            wg_count, 1,
            "wg_guest should appear exactly once after two calls"
        );
    }

    #[tokio::test]
    async fn test_ensure_firewall_zone_missing() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let result = ensure_firewall_zone(&mut cfgs, "wg_nonexistent", "nonexistent");
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err().kind, ErrorKind::MissingFirewallZone),
            "should return MissingFirewallZone error"
        );
    }

    #[tokio::test]
    async fn test_ensure_wireguard_firewall_rule() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        ensure_wireguard_firewall_rule(&mut cfgs, "wg_guest", 51820).unwrap();

        let rule = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<uciedit::openwrt::FirewallRule>().ok())
            .find(|r| r.name == "Allow-WireGuard-wg_guest")
            .expect("WireGuard firewall rule should exist");

        assert_eq!(rule.src, "wan");
        assert_eq!(rule.dest_port.as_deref(), Some("51820"));
        assert_eq!(rule.proto, vec!["udp"]);
        assert_eq!(rule.target, uciedit::openwrt::FirewallTarget::ACCEPT);
    }

    #[tokio::test]
    async fn test_ensure_wireguard_firewall_rule_updates_port() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        ensure_wireguard_firewall_rule(&mut cfgs, "wg_guest", 51820).unwrap();
        ensure_wireguard_firewall_rule(&mut cfgs, "wg_guest", 51821).unwrap();

        let rules: Vec<_> = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<uciedit::openwrt::FirewallRule>().ok())
            .filter(|r| r.name == "Allow-WireGuard-wg_guest")
            .collect();

        assert_eq!(
            rules.len(),
            1,
            "should update existing rule, not create duplicate"
        );
        assert_eq!(
            rules[0].dest_port.as_deref(),
            Some("51821"),
            "port should be updated"
        );
    }

    #[tokio::test]
    async fn test_remove_wireguard_firewall_rule() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // Verify rule exists before removal
        let has_rule = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<uciedit::openwrt::FirewallRule>().ok())
            .any(|r| r.name == "Allow-WireGuard-wg_guest");
        assert!(has_rule, "rule should exist before removal");

        remove_wireguard_firewall_rule(&mut cfgs, "wg_guest").unwrap();

        let has_rule_after = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<uciedit::openwrt::FirewallRule>().ok())
            .any(|r| r.name == "Allow-WireGuard-wg_guest");
        assert!(!has_rule_after, "rule should be gone after removal");

        // Other sections should survive
        let zone_count = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<uciedit::openwrt::FirewallZone>().ok())
            .count();
        assert!(
            zone_count >= 3,
            "firewall zones should survive rule removal"
        );
    }

    #[tokio::test]
    async fn test_remove_from_firewall_zones() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        remove_from_firewall_zones(&mut cfgs, "wg_guest").unwrap();

        let zone = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<uciedit::openwrt::FirewallZone>().ok())
            .find(|z| z.name == "vlan_guest")
            .unwrap();

        assert!(
            !zone.network.iter().any(|n| n == "wg_guest"),
            "wg_guest should be removed"
        );
        assert!(
            zone.network.iter().any(|n| n == "guest"),
            "guest should remain"
        );
    }

    #[tokio::test]
    async fn test_add_single_peer() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let peer_key = gen_key();
        let psk = Base64::new(generate_psk()).to_base64();
        let peer = VpnServerPeer {
            name: "Tablet".into(),
            ip: Some(Ipv4Addr::new(192, 168, 101, 202)),
            public_key: Some(peer_key.clone()),
            preshared_key: Some(psk.clone()),
            route_all: None,
            allow_dns_injection: None,
            legacy_address: None,
        };

        add_single_peer(&mut cfgs, "wg_guest", &peer, &arena).unwrap();

        // Verify the peer was created
        let peer_sections: Vec<_> = cfgs["network"]
            .sections
            .iter()
            .filter(|s| s.ty() == "wireguard_wg_guest")
            .collect();

        // 2 existing + 1 new
        assert_eq!(peer_sections.len(), 3);

        // Find our new peer by public key
        let new_peer = peer_sections
            .iter()
            .find(|s| {
                s.lines.iter().any(|l| {
                    matches!(l, Line::Option { option, value, .. }
                    if option.as_str() == "public_key" && value.as_str() == peer_key)
                })
            })
            .expect("new peer should exist");

        // Verify fields
        let has_description = new_peer.lines.iter().any(|l| {
            matches!(l, Line::Option { option, value, .. }
            if option.as_str() == "description" && value.as_str() == "Tablet")
        });
        assert!(has_description, "should have description");

        let has_psk = new_peer.lines.iter().any(|l| {
            matches!(l, Line::Option { option, value, .. }
            if option.as_str() == "preshared_key" && value.as_str() == psk)
        });
        assert!(has_psk, "should have preshared_key");

        let has_keepalive = new_peer.lines.iter().any(|l| {
            matches!(l, Line::Option { option, value, .. }
            if option.as_str() == "persistent_keepalive" && value.as_str() == "25")
        });
        assert!(has_keepalive, "should have persistent_keepalive=25");

        let has_route = new_peer.lines.iter().any(|l| {
            matches!(l, Line::Option { option, value, .. }
            if option.as_str() == "route_allowed_ips" && value.as_str() == "1")
        });
        assert!(has_route, "should have route_allowed_ips=1");

        let has_ip = new_peer.lines.iter().any(|l| {
            matches!(l, Line::List { list, item, .. }
            if list.as_str() == "allowed_ips" && item.as_str() == "192.168.101.202/32")
        });
        assert!(has_ip, "should have allowed_ips with correct IP/32");
        assert!(
            !crate::dns_inject::peer_allows_dns_inject(new_peer),
            "a peer is added without the DNS-injection permission by default"
        );
    }

    #[tokio::test]
    async fn test_add_single_peer_with_dns_injection() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path());
        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();
        let peer_key = gen_key();
        let peer = VpnServerPeer {
            name: "Server".into(),
            ip: Some(Ipv4Addr::new(192, 168, 101, 202)),
            public_key: Some(peer_key.clone()),
            preshared_key: Some(Base64::new(generate_psk()).to_base64()),
            route_all: None,
            allow_dns_injection: Some(true),
            legacy_address: None,
        };
        add_single_peer(&mut cfgs, "wg_guest", &peer, &arena).unwrap();
        let listed = get_peers_for_interface(&cfgs, "wg_guest");
        let listed = listed
            .iter()
            .find(|p| p.public_key.as_deref() == Some(peer_key.as_str()))
            .unwrap();
        assert_eq!(listed.allow_dns_injection, Some(true));
    }

    /// Granting and revoking a peer's permission edits its section alone.
    #[tokio::test]
    async fn test_peer_set_dns_injection() {
        let dir = tempfile::tempdir().unwrap();
        let (_, peer0_key, _) = setup_with_vpn_server(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());
        let set = |allow: bool| {
            peer_set_dns_injection(
                ctx.clone(),
                PeerSetDnsInjectionArgs {
                    profile: "guest".into(),
                    public_key: peer0_key.clone(),
                    allow,
                },
            )
        };
        let read = || async {
            let arena = Arena::new();
            let cfgs = parse_all(dir.path(), &arena, &["network"]).await.unwrap();
            let peers = get_peers_for_interface(&cfgs, "wg_guest");
            let allowed = peers
                .iter()
                .find(|p| p.public_key.as_deref() == Some(peer0_key.as_str()))
                .unwrap()
                .allow_dns_injection;
            let dhcp = std::fs::read_to_string(dir.path().join("dhcp")).unwrap_or_default();
            (allowed, dhcp)
        };

        let (_, dhcp_before) = read().await;
        set(true).await.unwrap();
        assert_eq!(read().await, (Some(true), dhcp_before.clone()));
        set(false).await.unwrap();
        assert_eq!(read().await, (None, dhcp_before));

        let missing = peer_set_dns_injection(
            ctx.clone(),
            PeerSetDnsInjectionArgs {
                profile: "guest".into(),
                public_key: "absent".into(),
                allow: true,
            },
        )
        .await;
        assert!(missing.is_err(), "an unknown peer is an error");
    }

    #[tokio::test]
    async fn test_add_peer_index_after_deletion() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path()); // has peers _0 and _1

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // Remove peer _0 (middle deletion scenario)
        cfgs["network"]
            .sections
            .retain(|s| s.name().as_deref() != Some("wg_guest_0"));

        // Add a new peer → should be _2 (max existing suffix + 1), not _1 (count)
        let peer = VpnServerPeer {
            name: "New Device".into(),
            ip: Some(Ipv4Addr::new(192, 168, 101, 202)),
            public_key: Some(gen_key()),
            preshared_key: None,
            route_all: None,
            allow_dns_injection: None,
            legacy_address: None,
        };
        add_single_peer(&mut cfgs, "wg_guest", &peer, &arena).unwrap();

        let has_2 = cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("wg_guest_2"));
        assert!(
            has_2,
            "new peer should be named wg_guest_2 (max existing suffix + 1)"
        );

        let has_0 = cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("wg_guest_0"));
        assert!(!has_0, "wg_guest_0 should not exist (was deleted)");

        // _1 should still exist
        let has_1 = cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("wg_guest_1"));
        assert!(has_1, "wg_guest_1 should still exist");
    }

    #[tokio::test]
    async fn test_rename_peer_section() {
        let dir = tempfile::tempdir().unwrap();
        let (_, peer0_key, _) = setup_with_vpn_server(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();
        let before = get_peers_for_interface(&cfgs, "wg_guest");

        rename_peer_section(&mut cfgs, &arena, "wg_guest", &peer0_key, "Work Phone").unwrap();

        let after = get_peers_for_interface(&cfgs, "wg_guest");
        assert_eq!(after.len(), 2);
        assert_eq!(after[0].name, "Work Phone");
        assert_eq!(after[0].public_key, before[0].public_key);
        assert_eq!(after[0].ip, before[0].ip);
        assert_eq!(after[1].name, before[1].name);

        let missing = rename_peer_section(&mut cfgs, &arena, "wg_guest", &gen_key(), "Ghost");
        assert!(missing.is_err());
    }

    #[tokio::test]
    async fn test_get_peers_for_interface() {
        let dir = tempfile::tempdir().unwrap();
        let (_, peer0_key, peer1_key) = setup_with_vpn_server(dir.path());

        let arena = Arena::new();
        let cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let peers = get_peers_for_interface(&cfgs, "wg_guest");

        assert_eq!(peers.len(), 2);
        assert_eq!(peers[0].name, "Phone");
        assert_eq!(peers[0].ip, Some(Ipv4Addr::new(192, 168, 101, 200)));
        assert_eq!(peers[0].public_key.as_deref(), Some(peer0_key.as_str()));
        assert_eq!(
            peers[0].preshared_key, None,
            "PSK should never be exposed in list"
        );

        assert_eq!(peers[1].name, "Laptop");
        assert_eq!(peers[1].ip, Some(Ipv4Addr::new(192, 168, 101, 201)));
        assert_eq!(peers[1].public_key.as_deref(), Some(peer1_key.as_str()));

        // Peers from setup_with_vpn_server have route_all: None
        assert_eq!(
            peers[0].route_all, None,
            "default peers should have route_all=None"
        );
        assert_eq!(
            peers[1].route_all, None,
            "default peers should have route_all=None"
        );
    }

    #[tokio::test]
    async fn test_add_peer_route_all_stored_in_uci() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // Add peer with route_all = true
        let peer_key = gen_key();
        let peer = VpnServerPeer {
            name: "Full Tunnel".into(),
            ip: Some(Ipv4Addr::new(192, 168, 101, 202)),
            public_key: Some(peer_key.clone()),
            preshared_key: None,
            route_all: Some(true),
            allow_dns_injection: None,
            legacy_address: None,
        };
        add_single_peer(&mut cfgs, "wg_guest", &peer, &arena).unwrap();

        // Find the new peer section
        let new_peer = cfgs["network"]
            .sections
            .iter()
            .find(|s| {
                s.lines.iter().any(|l| {
                    matches!(l, Line::Option { option, value, .. }
                    if option.as_str() == "public_key" && value.as_str() == peer_key)
                })
            })
            .expect("new peer should exist");

        // Should have startwrt_route_all = 1
        let has_route_all = new_peer.lines.iter().any(|l| {
            matches!(l, Line::Option { option, value, .. }
            if option.as_str() == "startwrt_route_all" && value.as_str() == "1")
        });
        assert!(
            has_route_all,
            "route_all peer should have startwrt_route_all=1 in UCI"
        );
    }

    #[tokio::test]
    async fn test_add_peer_route_all_absent_when_false() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // Add peer with route_all = None (split tunnel default)
        let peer_key = gen_key();
        let peer = VpnServerPeer {
            name: "Split Tunnel".into(),
            ip: Some(Ipv4Addr::new(192, 168, 101, 202)),
            public_key: Some(peer_key.clone()),
            preshared_key: None,
            route_all: None,
            allow_dns_injection: None,
            legacy_address: None,
        };
        add_single_peer(&mut cfgs, "wg_guest", &peer, &arena).unwrap();

        let new_peer = cfgs["network"]
            .sections
            .iter()
            .find(|s| {
                s.lines.iter().any(|l| {
                    matches!(l, Line::Option { option, value, .. }
                    if option.as_str() == "public_key" && value.as_str() == peer_key)
                })
            })
            .expect("new peer should exist");

        // Should NOT have startwrt_route_all option
        let has_route_all = new_peer.lines.iter().any(|l| {
            matches!(l, Line::Option { option, .. }
            if option.as_str() == "startwrt_route_all")
        });
        assert!(
            !has_route_all,
            "split tunnel peer should not have startwrt_route_all in UCI"
        );
    }

    #[tokio::test]
    async fn test_get_peers_reads_route_all() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // Add a peer with route_all = true
        let peer = VpnServerPeer {
            name: "Full Tunnel".into(),
            ip: Some(Ipv4Addr::new(192, 168, 101, 202)),
            public_key: Some(gen_key()),
            preshared_key: None,
            route_all: Some(true),
            allow_dns_injection: None,
            legacy_address: None,
        };
        add_single_peer(&mut cfgs, "wg_guest", &peer, &arena).unwrap();

        let peers = get_peers_for_interface(&cfgs, "wg_guest");

        assert_eq!(peers.len(), 3);
        // First two peers from setup have no route_all
        assert_eq!(peers[0].route_all, None);
        assert_eq!(peers[1].route_all, None);
        // Third peer has route_all = true
        assert_eq!(peers[2].route_all, Some(true));
    }

    // === Peer subnet tests ===

    const ALL_CONFIGS: &[&str] = &["network", "startwrt", "firewall", "dhcp"];

    fn wg_addresses(cfgs: &Configs, name: &str) -> Vec<String> {
        cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some(name))
            .and_then(|s| s.get::<WgInterface>().ok())
            .unwrap()
            .addresses
    }

    fn append_to(dir: &Path, config: &str, text: &str) {
        let content = std::fs::read_to_string(dir.join(config)).unwrap();
        std::fs::write(dir.join(config), format!("{content}\n{text}")).unwrap();
    }

    #[tokio::test]
    async fn allocates_peer_subnets_from_the_top_of_the_lan_block() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path());
        append_to(
            dir.path(),
            "startwrt",
            "config vpn_server 'wg_lan'\n\toption interface 'wg_lan'\n\toption profile_interface 'lan'\n\
             \toption label 'Admin VPN'\n\toption listen_port '51821'\n\toption endpoint 'vpn.example.com'\n",
        );
        // A profile, an outbound VPN address and a WAN address high in the block.
        append_to(
            dir.path(),
            "network",
            "config interface 'iot'\n\toption device 'br-lan.120'\n\toption proto 'static'\n\
             \toption ipaddr '192.168.254.1'\n\toption netmask '255.255.255.0'\n\n\
             config interface 'wg_out'\n\toption proto 'wireguard'\n\toption private_key 'AAAA'\n\
             \tlist addresses '192.168.253.7/32'\n",
        );
        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, ALL_CONFIGS).await.unwrap();
        let wan = [Ipv4Addr::new(192, 168, 255, 40)];

        let guest = ensure_peer_subnet(&mut cfgs, "wg_guest", &wan).unwrap();
        assert_eq!(guest, Ipv4Addr::new(192, 168, 252, 0));
        // Stored, so asking again returns the same subnet.
        assert_eq!(
            ensure_peer_subnet(&mut cfgs, "wg_guest", &[]).unwrap(),
            guest
        );
        let lan = ensure_peer_subnet(&mut cfgs, "wg_lan", &wan).unwrap();
        assert_eq!(lan, Ipv4Addr::new(192, 168, 251, 0));
        assert_eq!(
            peer_subnet_owner(&cfgs, Ipv4Addr::new(192, 168, 251, 1)).as_deref(),
            Some("lan")
        );
    }

    #[tokio::test]
    async fn places_new_peers_in_the_peer_subnet_beside_legacy_peers() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path());
        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, ALL_CONFIGS).await.unwrap();

        let mut peer = VpnServerPeer {
            name: "Tablet".into(),
            ip: None,
            public_key: Some(gen_key()),
            preshared_key: None,
            route_all: None,
            allow_dns_injection: None,
            legacy_address: None,
        };
        let placed = place_peer(&mut cfgs, &arena, "wg_guest", &mut peer, &[]).unwrap();
        assert!(placed.new_subnet);
        assert_eq!(placed.ip, Ipv4Addr::new(192, 168, 255, 2));
        // The legacy address stays first while legacy peers remain.
        assert_eq!(
            wg_addresses(&cfgs, "wg_guest"),
            vec!["192.168.101.254/32", "192.168.255.1/24"]
        );
        assert!(proxy_arp_wanted(&cfgs, "guest"));

        let peers = get_peers_for_interface(&cfgs, "wg_guest");
        let legacy: Vec<_> = peers
            .iter()
            .map(|p| (p.ip.unwrap(), p.legacy_address))
            .collect();
        assert_eq!(
            legacy,
            vec![
                (Ipv4Addr::new(192, 168, 101, 200), Some(true)),
                (Ipv4Addr::new(192, 168, 101, 201), Some(true)),
                (Ipv4Addr::new(192, 168, 255, 2), None),
            ]
        );

        // A second peer may not reuse a legacy peer's host octet.
        let mut second = VpnServerPeer {
            ip: Some(Ipv4Addr::new(192, 168, 255, 200)),
            public_key: Some(gen_key()),
            ..peer.clone()
        };
        assert!(place_peer(&mut cfgs, &arena, "wg_guest", &mut second, &[]).is_err());
    }

    #[tokio::test]
    async fn dropping_the_last_legacy_peer_drops_its_address_and_proxy_arp() {
        let dir = tempfile::tempdir().unwrap();
        let (_, peer0, peer1) = setup_with_vpn_server(dir.path());
        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, ALL_CONFIGS).await.unwrap();
        ensure_peer_subnet(&mut cfgs, "wg_guest", &[]).unwrap();
        sync_proxy_arp(&mut cfgs, "guest").unwrap();
        assert!(proxy_arp_wanted(&cfgs, "guest"));

        remove_peer_section(&mut cfgs, "wg_guest", &peer0).unwrap();
        sync_server_addresses(&mut cfgs, "wg_guest").unwrap();
        assert!(proxy_arp_wanted(&cfgs, "guest"));

        remove_peer_section(&mut cfgs, "wg_guest", &peer1).unwrap();
        sync_server_addresses(&mut cfgs, "wg_guest").unwrap();
        sync_proxy_arp(&mut cfgs, "guest").unwrap();
        assert_eq!(wg_addresses(&cfgs, "wg_guest"), vec!["192.168.255.1/24"]);
        assert!(!proxy_arp_wanted(&cfgs, "guest"));
        let guest = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("guest"))
            .and_then(|s| s.get::<NetworkInterface>().ok())
            .unwrap();
        assert_eq!(guest.proxy_arp, None);
        assert!(remove_peer_section(&mut cfgs, "wg_guest", &peer1).is_err());
    }

    #[tokio::test]
    async fn guard_set_holds_only_legacy_peers() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path());
        let include = dir.path().join("ruleset-pre/vpn-peers.nft");

        assert!(sync_vpn_peer_guard_file(dir.path(), &include)
            .await
            .unwrap());
        assert!(!sync_vpn_peer_guard_file(dir.path(), &include)
            .await
            .unwrap());
        let rendered = tokio::fs::read_to_string(&include).await.unwrap();
        assert!(rendered.contains("add set bridge startwrt_lan_guard vpn_peers"));
        assert!(rendered.ends_with(
            "add element bridge startwrt_lan_guard vpn_peers { 192.168.101.200, 192.168.101.201 }\n"
        ));

        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, ALL_CONFIGS).await.unwrap();
        let mut peer = VpnServerPeer {
            name: "Tablet".into(),
            ip: None,
            public_key: Some(gen_key()),
            preshared_key: None,
            route_all: None,
            allow_dns_injection: None,
            legacy_address: None,
        };
        place_peer(&mut cfgs, &arena, "wg_guest", &mut peer, &[]).unwrap();
        let rendered = vpn_peer_guard_ruleset(&cfgs);
        assert!(!rendered.contains("192.168.255.2"));

        cfgs["network"]
            .sections
            .retain(|s| s.ty() != "wireguard_wg_guest");
        let rendered = vpn_peer_guard_ruleset(&cfgs);
        assert!(!rendered.contains("add element"));
    }

    #[tokio::test]
    async fn client_configs_route_the_peer_subnet_for_new_peers_only() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path());
        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, ALL_CONFIGS).await.unwrap();
        let mut peer = VpnServerPeer {
            name: "Tablet".into(),
            ip: None,
            public_key: Some(gen_key()),
            preshared_key: None,
            route_all: None,
            allow_dns_injection: None,
            legacy_address: None,
        };
        let placed = place_peer(&mut cfgs, &arena, "wg_guest", &mut peer, &[]).unwrap();

        let new = render_client_config(&cfgs, "wg_guest", "Tablet", placed.ip, false, None, None)
            .unwrap();
        assert!(new.contains("Address = 192.168.255.2/24\n"), "{new}");
        assert!(
            new.contains("AllowedIPs = 192.168.101.0/24, 192.168.255.0/24\n"),
            "{new}"
        );
        assert!(new.contains("DNS = 192.168.101.1\n"), "{new}");
        assert!(new.contains("PrivateKey = <PRIVATE_KEY>\n"), "{new}");
        assert!(new.contains("PresharedKey = <PRESHARED_KEY>\n"), "{new}");

        let legacy = render_client_config(
            &cfgs,
            "wg_guest",
            "Phone",
            Ipv4Addr::new(192, 168, 101, 200),
            false,
            Some("priv"),
            Some("psk"),
        )
        .unwrap();
        assert!(
            legacy.contains("Address = 192.168.101.200/24\n"),
            "{legacy}"
        );
        assert!(
            legacy.contains("AllowedIPs = 192.168.101.0/24\n"),
            "{legacy}"
        );
        assert!(legacy.contains("PrivateKey = priv\n"), "{legacy}");
        assert!(legacy.contains("PresharedKey = psk\n"), "{legacy}");

        let all =
            render_client_config(&cfgs, "wg_guest", "Tablet", placed.ip, true, None, None).unwrap();
        assert!(all.contains("AllowedIPs = 0.0.0.0/0, ::/0\n"), "{all}");
    }

    #[tokio::test]
    async fn client_configs_carry_ipv6_only_where_the_server_routes_it() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path());
        // IPv6 turned on after the fixture's peers were added.
        append_to(
            dir.path(),
            "network",
            "config globals 'globals'\n\toption ula_prefix 'fdaa:bbbb:cccc::/48'\n",
        );
        let dhcp = std::fs::read_to_string(dir.path().join("dhcp")).unwrap();
        std::fs::write(
            dir.path().join("dhcp"),
            dhcp.replacen(
                "\toption interface 'lan'\n",
                "\toption interface 'lan'\n\toption ra 'server'\n",
                1,
            ),
        )
        .unwrap();
        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, ALL_CONFIGS).await.unwrap();
        let mut peer = VpnServerPeer {
            name: "Tablet".into(),
            ip: None,
            public_key: Some(gen_key()),
            preshared_key: None,
            route_all: None,
            allow_dns_injection: None,
            legacy_address: None,
        };
        let placed = place_peer(&mut cfgs, &arena, "wg_guest", &mut peer, &[]).unwrap();

        let new = render_client_config(&cfgs, "wg_guest", "Tablet", placed.ip, false, None, None)
            .unwrap();
        assert!(
            new.contains("Address = 192.168.255.2/24, fdaa:bbbb:cccc:f065::2/128\n"),
            "{new}"
        );
        assert!(new.contains(", fdaa:bbbb:cccc::/48\n"), "{new}");

        let earlier = render_client_config(
            &cfgs,
            "wg_guest",
            "Phone",
            Ipv4Addr::new(192, 168, 101, 200),
            false,
            None,
            None,
        )
        .unwrap();
        assert!(
            earlier.contains("Address = 192.168.101.200/24\n"),
            "{earlier}"
        );
        assert!(
            earlier.contains("AllowedIPs = 192.168.101.0/24\n"),
            "{earlier}"
        );
    }

    #[tokio::test]
    async fn vpn_routed_tables_route_the_peer_subnet() {
        let dir = tempfile::tempdir().unwrap();
        setup_v6_configs(dir.path());
        // Admin is VPN-routed too, so it needs a route into Guest's server.
        let startwrt = std::fs::read_to_string(dir.path().join("startwrt"))
            .unwrap()
            .replace(
                "\toption vlan_tag '99'\n",
                "\toption vlan_tag '99'\n\toption outbound 'wg_vpn1'\n",
            );
        std::fs::write(dir.path().join("startwrt"), startwrt).unwrap();
        append_to(
            dir.path(),
            "network",
            "config wireguard_wg_guest 'wg_guest_0'\n\toption public_key 'legacy'\n\
             \tlist allowed_ips '192.168.101.200/32'\n",
        );
        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, ALL_CONFIGS).await.unwrap();
        ensure_peer_subnet(&mut cfgs, "wg_guest", &[]).unwrap();
        sync_peer_policy_routes(&mut cfgs, "guest").unwrap();
        sync_vpn_peer_cross_routes(&mut cfgs).unwrap();

        let route = |cfgs: &Configs, name: &str| {
            cfgs["network"]
                .sections
                .iter()
                .find(|s| s.name().as_deref() == Some(name))
                .and_then(|s| s.get::<NetworkRoute>().ok())
        };
        let vsn = route(&cfgs, "vsn_guest").expect("peer subnet route");
        assert_eq!(vsn.target, "192.168.255.0/24");
        assert_eq!((vsn.interface.as_str(), vsn.table), ("wg_guest", Some(101)));
        let vsr = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("vsr_guest"))
            .and_then(|s| s.get::<NetworkRule>().ok())
            .expect("peer subnet rule");
        assert_eq!(vsr.src.as_deref(), Some("192.168.255.0/24"));
        assert_eq!(vsr.lookup, 101);
        assert_eq!(vsr.priority, Some(crate::profiles::VPN_ROUTING_PRIORITY));
        assert_eq!(
            route(&cfgs, "vpr_guest_200").map(|r| r.target).as_deref(),
            Some("192.168.101.200/32")
        );

        let vxs = route(&cfgs, "vxs_guest_lan").expect("sibling route to the peer subnet");
        assert_eq!(
            (vxs.target.as_str(), vxs.table),
            ("192.168.255.0/24", Some(99))
        );
        assert!(route(&cfgs, "vxr_guest_lan_200").is_some());

        // Removing the server removes them all.
        remove_vpn_server(&mut cfgs, "guest").unwrap();
        for name in [
            "vsn_guest",
            "vpr_guest_200",
            "vxs_guest_lan",
            "vxr_guest_lan_200",
        ] {
            assert!(route(&cfgs, name).is_none(), "{name} should be gone");
        }
        assert!(!cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("vsr_guest")));
    }

    #[tokio::test]
    async fn block_changes_move_peer_subnets() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());
        append_to(
            dir.path(),
            "startwrt",
            "config vpn_server 'wg_guest'\n\toption interface 'wg_guest'\n\toption profile_interface 'guest'\n\
             \toption label 'Guest VPN'\n\toption listen_port '51820'\n\toption endpoint 'vpn.example.com'\n\
             \toption peer_subnet '192.168.255.0/24'\n",
        );
        append_to(
            dir.path(),
            "network",
            &format!(
                "config interface 'wg_guest'\n\toption proto 'wireguard'\n\toption private_key '{}'\n\
                 \tlist addresses '192.168.255.1/24'\n",
                gen_key()
            ),
        );
        let arena = Arena::new();
        let mut cfgs = parse_all(dir.path(), &arena, ALL_CONFIGS).await.unwrap();

        move_peer_subnets(&mut cfgs, [10, 42]).unwrap();
        assert_eq!(
            peer_subnet_of(&cfgs, "wg_guest"),
            Some(Ipv4Addr::new(10, 42, 255, 0))
        );
        assert_eq!(wg_addresses(&cfgs, "wg_guest"), vec!["10.42.255.1/24"]);
    }

    // === Integration-style tests ===

    #[tokio::test]
    async fn test_full_create_and_delete_flow() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let config = VpnServerConfig {
            label: "Guest VPN".into(),
            enabled: true,
            listen_port: 51820,
            endpoint: "vpn.example.com".into(),
            private_key: Some(gen_key()),
        };

        // Create VPN server (all 4 helpers)
        set_wireguard_interface(&mut cfgs, "wg_guest", &config).unwrap();
        set_vpn_server_metadata(&mut cfgs, "wg_guest", "guest", &config).unwrap();
        ensure_firewall_zone(&mut cfgs, "wg_guest", "guest").unwrap();
        ensure_wireguard_firewall_rule(&mut cfgs, "wg_guest", 51820).unwrap();

        // Add 2 peers
        let peer1 = VpnServerPeer {
            name: "Phone".into(),
            ip: Some(Ipv4Addr::new(192, 168, 101, 200)),
            public_key: Some(gen_key()),
            preshared_key: Some(Base64::new(generate_psk()).to_base64()),
            route_all: None,
            allow_dns_injection: None,
            legacy_address: None,
        };
        let peer2 = VpnServerPeer {
            name: "Laptop".into(),
            ip: Some(Ipv4Addr::new(192, 168, 101, 201)),
            public_key: Some(gen_key()),
            preshared_key: Some(Base64::new(generate_psk()).to_base64()),
            route_all: None,
            allow_dns_injection: None,
            legacy_address: None,
        };
        add_single_peer(&mut cfgs, "wg_guest", &peer1, &arena).unwrap();
        add_single_peer(&mut cfgs, "wg_guest", &peer2, &arena).unwrap();

        // Verify everything exists
        assert!(cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("wg_guest")));
        assert_eq!(
            cfgs["network"]
                .sections
                .iter()
                .filter(|s| s.ty() == "wireguard_wg_guest")
                .count(),
            2
        );
        assert!(cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnServer>().ok())
            .any(|m| m.interface == "wg_guest"));

        // Now delete — mimicking vpn_server::delete logic
        cfgs["network"].sections.retain(|section| {
            if section.name().as_deref() == Some("wg_guest") {
                if let Ok(iface) = section.get::<WgInterface>() {
                    if iface.is_wireguard() {
                        return false;
                    }
                }
            }
            true
        });
        cfgs["network"]
            .sections
            .retain(|section| section.ty() != "wireguard_wg_guest");
        cfgs["startwrt"].sections.retain(|section| {
            if let Ok(meta) = section.get::<UciVpnServer>() {
                return meta.interface != "wg_guest";
            }
            true
        });
        remove_from_firewall_zones(&mut cfgs, "wg_guest").unwrap();
        remove_wireguard_firewall_rule(&mut cfgs, "wg_guest").unwrap();

        // Verify cleanup
        assert!(!cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("wg_guest")));
        assert_eq!(
            cfgs["network"]
                .sections
                .iter()
                .filter(|s| s.ty() == "wireguard_wg_guest")
                .count(),
            0
        );
        assert!(!cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciVpnServer>().ok())
            .any(|m| m.interface == "wg_guest"));

        let zone = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<uciedit::openwrt::FirewallZone>().ok())
            .find(|z| z.name == "vlan_guest")
            .unwrap();
        assert!(
            !zone.network.iter().any(|n| n == "wg_guest"),
            "wg_guest should be removed from zone"
        );
        assert!(
            zone.network.iter().any(|n| n == "guest"),
            "guest should still be in zone"
        );

        let has_wg_rule = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<uciedit::openwrt::FirewallRule>().ok())
            .any(|r| r.name == "Allow-WireGuard-wg_guest");
        assert!(!has_wg_rule);

        // Profile still exists
        let profile_count = cfgs["startwrt"]
            .sections
            .iter()
            .filter(|s| s.ty() == "profile")
            .count();
        assert_eq!(
            profile_count, 2,
            "both profiles should survive VPN deletion"
        );
    }

    #[tokio::test]
    async fn test_enable_disable_toggle() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // Create enabled
        let config_enabled = VpnServerConfig {
            label: "Guest VPN".into(),
            enabled: true,
            listen_port: 51820,
            endpoint: "vpn.example.com".into(),
            private_key: Some(gen_key()),
        };
        set_wireguard_interface(&mut cfgs, "wg_guest", &config_enabled).unwrap();
        ensure_wireguard_firewall_rule(&mut cfgs, "wg_guest", 51820).unwrap();

        // Verify enabled state
        let wg = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("wg_guest"))
            .and_then(|s| s.get::<WgInterface>().ok())
            .unwrap();
        assert_eq!(wg.listen_port, Some(51820));

        // Disable
        let config_disabled = VpnServerConfig {
            label: "Guest VPN".into(),
            enabled: false,
            listen_port: 51820,
            endpoint: "vpn.example.com".into(),
            private_key: None,
        };
        set_wireguard_interface(&mut cfgs, "wg_guest", &config_disabled).unwrap();
        remove_wireguard_firewall_rule(&mut cfgs, "wg_guest").unwrap();

        let wg = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("wg_guest"))
            .and_then(|s| s.get::<WgInterface>().ok())
            .unwrap();
        assert_eq!(wg.listen_port, None, "disabled should have no listen_port");

        let has_rule = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<uciedit::openwrt::FirewallRule>().ok())
            .any(|r| r.name == "Allow-WireGuard-wg_guest");
        assert!(!has_rule, "disabled should have no WAN rule");

        // Re-enable
        let config_reenable = VpnServerConfig {
            label: "Guest VPN".into(),
            enabled: true,
            listen_port: 51820,
            endpoint: "vpn.example.com".into(),
            private_key: None,
        };
        set_wireguard_interface(&mut cfgs, "wg_guest", &config_reenable).unwrap();
        ensure_wireguard_firewall_rule(&mut cfgs, "wg_guest", 51820).unwrap();

        let wg = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("wg_guest"))
            .and_then(|s| s.get::<WgInterface>().ok())
            .unwrap();
        assert_eq!(
            wg.listen_port,
            Some(51820),
            "re-enabled should restore listen_port"
        );

        let has_rule = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<uciedit::openwrt::FirewallRule>().ok())
            .any(|r| r.name == "Allow-WireGuard-wg_guest");
        assert!(has_rule, "re-enabled should restore WAN rule");
    }

    /// Configs for a v6-serving, vpn-routed Guest profile (outbound wg_vpn1, a
    /// v6-capable tunnel) hosting an inbound server wg_guest. Includes a concrete
    /// device ULA prefix and global RA, so wg_server_v6_groups returns Some.
    fn setup_v6_configs(dir: &Path) {
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
\toption outbound 'wg_vpn1'

config vpn_server 'wg_guest'
\toption interface 'wg_guest'
\toption profile_interface 'guest'
\toption label 'Guest VPN'
\toption listen_port '51820'
\toption endpoint 'vpn.example.com'
",
        )
        .unwrap();

        std::fs::write(
            dir.join("network"),
            "\
config globals 'globals'
\toption ula_prefix 'fdaa:bbbb:cccc::/48'

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

config interface 'wg_vpn1'
\toption proto 'wireguard'
\toption private_key 'AAAA'
\tlist addresses '10.9.0.2/32'
\tlist addresses 'fd00:9::2/128'

config interface 'wg_guest'
\toption proto 'wireguard'
\toption private_key 'BBBB'
\tlist addresses '192.168.101.254/32'
",
        )
        .unwrap();

        std::fs::write(
            dir.join("dhcp"),
            "\
config dhcp 'lan'
\toption interface 'lan'
\toption start '100'
\toption limit '150'
\toption leasetime '12h'
\toption ra 'server'

config dhcp 'guest'
\toption interface 'guest'
\toption start '100'
\toption limit '150'
\toption leasetime '12h'
\toption ra 'server'
",
        )
        .unwrap();

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
\toption name 'vlan_guest'
\tlist network 'guest'
\toption input 'ACCEPT'
\toption output 'ACCEPT'
\toption forward 'ACCEPT'
",
        )
        .unwrap();
    }

    #[tokio::test]
    async fn test_wg_server_v6_groups_derivation() {
        let dir = tempfile::tempdir().unwrap();
        setup_v6_configs(dir.path());
        let arena = Arena::new();
        let cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let groups = wg_server_v6_groups(&cfgs, "guest").expect("guest serves v6");
        // ULA /48 high groups + high subnet-id band (0xf000 | vlan_tag 101).
        assert_eq!(groups, [0xfdaa, 0xbbbb, 0xcccc, 0xf000 | 101]);
        assert_eq!(
            wg_server_v6_addr(groups).to_string(),
            "fdaa:bbbb:cccc:f065::1"
        );
        // Peer host id reuses the v4 octet (.202 = 0xca).
        assert_eq!(
            wg_peer_v6_addr(groups, 202).to_string(),
            "fdaa:bbbb:cccc:f065::ca"
        );
    }

    #[tokio::test]
    async fn test_wg_server_v6_groups_none_when_not_serving_v6() {
        // setup_with_vpn_server has no ULA prefix and no global RA → v4-only.
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path());
        let arena = Arena::new();
        let cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();
        assert!(wg_server_v6_groups(&cfgs, "guest").is_none());
    }

    #[tokio::test]
    async fn test_set_wireguard_interface_adds_v6_when_serving() {
        let dir = tempfile::tempdir().unwrap();
        setup_v6_configs(dir.path());
        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let config = VpnServerConfig {
            label: "Guest VPN".into(),
            enabled: true,
            listen_port: 51820,
            endpoint: "vpn.example.com".into(),
            private_key: Some(gen_key()),
        };
        set_wireguard_interface(&mut cfgs, "wg_guest", &config).unwrap();

        let wg = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("wg_guest"))
            .and_then(|s| s.get::<WgInterface>().ok())
            .unwrap();
        assert_eq!(
            wg.addresses,
            vec!["192.168.101.254/32", "fdaa:bbbb:cccc:f065::1/128"]
        );
    }

    #[tokio::test]
    async fn test_add_single_peer_includes_v6_when_serving() {
        let dir = tempfile::tempdir().unwrap();
        setup_v6_configs(dir.path());
        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let key = gen_key();
        let peer = VpnServerPeer {
            name: "Tablet".into(),
            ip: Some(Ipv4Addr::new(192, 168, 101, 202)),
            public_key: Some(key.clone()),
            preshared_key: None,
            route_all: None,
            allow_dns_injection: None,
            legacy_address: None,
        };
        add_single_peer(&mut cfgs, "wg_guest", &peer, &arena).unwrap();

        let new_peer = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.ty() == "wireguard_wg_guest")
            .expect("peer should exist");
        let has_v4 = new_peer.lines.iter().any(|l| {
            matches!(l, Line::List { list, item, .. }
            if list.as_str() == "allowed_ips" && item.as_str() == "192.168.101.202/32")
        });
        let has_v6 = new_peer.lines.iter().any(|l| {
            matches!(l, Line::List { list, item, .. }
            if list.as_str() == "allowed_ips" && item.as_str() == "fdaa:bbbb:cccc:f065::ca/128")
        });
        assert!(has_v4, "peer should have v4 /32 allowed_ips");
        assert!(has_v6, "peer should have v6 /128 allowed_ips");
    }

    #[tokio::test]
    async fn test_add_single_peer_no_v6_when_not_serving() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path());
        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let key = gen_key();
        let peer = VpnServerPeer {
            name: "Tablet".into(),
            ip: Some(Ipv4Addr::new(192, 168, 101, 202)),
            public_key: Some(key.clone()),
            preshared_key: None,
            route_all: None,
            allow_dns_injection: None,
            legacy_address: None,
        };
        add_single_peer(&mut cfgs, "wg_guest", &peer, &arena).unwrap();

        let new_peer = cfgs["network"]
            .sections
            .iter()
            .filter(|s| s.ty() == "wireguard_wg_guest")
            .find(|s| {
                s.lines.iter().any(|l| {
                    matches!(l, Line::Option { option, value, .. }
                if option.as_str() == "public_key" && value.as_str() == key)
                })
            })
            .unwrap();
        let v6_count = new_peer
            .lines
            .iter()
            .filter(|l| {
                matches!(l, Line::List { list, item, .. }
            if list.as_str() == "allowed_ips" && item.as_str().contains(':'))
            })
            .count();
        assert_eq!(
            v6_count, 0,
            "v4-only profile peer must have no v6 allowed_ips"
        );
    }

    #[tokio::test]
    async fn test_sync_peer_policy_routes_v6_steering() {
        let dir = tempfile::tempdir().unwrap();
        setup_v6_configs(dir.path());
        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        sync_peer_policy_routes(&mut cfgs, "guest").unwrap();

        let vsl6 = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("vsl6_guest"))
            .and_then(|s| s.get::<NetworkRule6>().ok())
            .expect("vsl6_guest rule should exist");
        assert_eq!(vsl6.in_iface.as_deref(), Some("wg_guest"));
        assert_eq!(vsl6.lookup, 254, "local escape looks up main");
        assert_eq!(vsl6.suppress_prefixlength, Some(0));
        assert_eq!(vsl6.priority, Some(150), "must match prl6_ priority");

        let vsr6 = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("vsr6_guest"))
            .and_then(|s| s.get::<NetworkRule6>().ok())
            .expect("vsr6_guest rule should exist");
        assert_eq!(vsr6.in_iface.as_deref(), Some("wg_guest"));
        assert_eq!(vsr6.lookup, 101, "captures to the per-VLAN table");
        assert_eq!(vsr6.priority, Some(200), "must match prr6_ priority");
    }

    #[tokio::test]
    async fn test_sync_peer_policy_routes_no_v6_steering_for_wan_routed() {
        // setup_with_vpn_server's Guest is wan-routed (no outbound) but has an
        // inbound server — it must NOT get v6 steering (main table is correct).
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path());
        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        sync_peer_policy_routes(&mut cfgs, "guest").unwrap();

        let has_steering = cfgs["network"].sections.iter().any(|s| {
            let n = s.name();
            n.as_deref() == Some("vsl6_guest") || n.as_deref() == Some("vsr6_guest")
        });
        assert!(
            !has_steering,
            "wan-routed profile must not get v6 steering rules"
        );
    }

    #[tokio::test]
    async fn test_sync_server_addresses_adds_v6_when_serving() {
        let dir = tempfile::tempdir().unwrap();
        setup_v6_configs(dir.path());
        let arena = Arena::new();
        let mut cfgs = parse_all(
            dir.path(),
            &arena,
            &["network", "startwrt", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // The fixture's wg_guest starts with only a v4 address.
        sync_server_addresses(&mut cfgs, "wg_guest").unwrap();

        let wg = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("wg_guest"))
            .and_then(|s| s.get::<WgInterface>().ok())
            .unwrap();
        assert!(
            wg.addresses.iter().any(|a| a == "192.168.101.254/32"),
            "v4 preserved"
        );
        assert!(
            wg.addresses
                .iter()
                .any(|a| a == "fdaa:bbbb:cccc:f065::1/128"),
            "should add the server v6 /128 when the profile serves v6"
        );
    }

    fn client_config(private_key: Option<&str>) -> String {
        ClientConfig {
            name: "Laptop",
            private_key,
            address: "192.168.0.200/32",
            dns: Ipv4Addr::new(192, 168, 0, 1),
            server_public_key: "c2VydmVy",
            preshared_key: Some("cHNr"),
            endpoint: "router.example.com",
            port: 51820,
            allowed_ips: "192.168.0.0/24",
        }
        .render()
    }

    #[test]
    fn client_config_with_supplied_public_key_has_placeholder_and_psk() {
        let config = client_config(None);
        let lines: Vec<&str> = config.lines().collect();
        for expected in [
            "PrivateKey = <PRIVATE_KEY>",
            "Address = 192.168.0.200/32",
            "DNS = 192.168.0.1",
            "PublicKey = c2VydmVy",
            "PresharedKey = cHNr",
            "Endpoint = router.example.com:51820",
            "AllowedIPs = 192.168.0.0/24",
        ] {
            assert!(
                lines.contains(&expected),
                "missing {expected:?} in {config}"
            );
        }
    }

    #[test]
    fn client_config_with_generated_key_carries_it() {
        let key = gen_key();
        let config = client_config(Some(key.as_str()));
        assert!(config.lines().any(|l| l == format!("PrivateKey = {key}")));
        assert!(!config.contains(CLIENT_PRIVATE_KEY_PLACEHOLDER));
    }

    async fn forced_guard_devices(dir: &Path, profile_interface: &str) -> Vec<String> {
        let arena = Arena::new();
        let mut cfgs = parse_all(dir, &arena, &["network", "startwrt", "firewall", "dhcp"])
            .await
            .unwrap();
        guard_vpn_peers(&mut cfgs, &[profile_interface], true)
            .unwrap()
            .proxy_arp_devices
    }

    #[tokio::test]
    async fn proxy_arp_targets_the_vlan_device_of_a_profile_among_several() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_vpn_server(dir.path());

        assert_eq!(
            forced_guard_devices(dir.path(), "guest").await,
            ["br-lan.101"]
        );
    }

    #[tokio::test]
    async fn proxy_arp_targets_the_bridge_of_a_lone_profile() {
        let dir = tempfile::tempdir().unwrap();
        setup_base_configs(dir.path());
        std::fs::write(
            dir.path().join("startwrt"),
            "\
config profile lan
\toption fullname 'Admin'
\toption interface 'lan'
\toption vlan_tag '1'

config vpn_server 'wg_lan'
\toption interface 'wg_lan'
\toption profile_interface 'lan'
\toption label 'Admin VPN'
\toption listen_port '51820'
\toption endpoint 'vpn.example.com'
",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("network"),
            format!(
                "\
config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'

config interface 'lan'
\toption device 'br-lan'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.255.0'
\toption proxy_arp '1'

config interface 'wg_lan'
\toption proto 'wireguard'
\toption private_key '{}'
\toption listen_port '51820'
\tlist addresses '192.168.1.254/32'

config wireguard_wg_lan 'wg_lan_0'
\toption public_key '{}'
\toption description 'Phone'
\tlist allowed_ips '192.168.1.200/32'
",
                gen_key(),
                gen_key()
            ),
        )
        .unwrap();

        assert_eq!(forced_guard_devices(dir.path(), "lan").await, ["br-lan"]);
    }
}
