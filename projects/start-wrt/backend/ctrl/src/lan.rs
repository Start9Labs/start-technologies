use crate::profiles;
use crate::utils::DeserializeStdin;
use crate::utils::HandlerExtSerde;
use crate::CtrlContext;
use crate::prelude::*;
use crate::invoke::Invoke;
use rpc_toolkit::{from_fn_async_local, ParentHandler};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use uciedit::openwrt::{Dhcp, DhcpHost, FirewallRedirect, NetworkInterface, NetworkRoute, NetworkRule, ProfileDnsmasq};
use uciedit::{dump_all, parse_all, Arena, Configs};

pub const LAN_INTERFACE: &str = "lan";
pub const WAN6_INTERFACE: &str = "wan6";
pub const LAN_NETMASK: Ipv4Addr = Ipv4Addr::new(255, 255, 255, 0);

/// Reject any LAN IPv4 outside the RFC 1918 ranges the UI offers.
///
/// [`Ipv4Addr::is_private`] already encodes exactly the per-block second-octet
/// boundaries that bound each selectable /16:
///   10.0.0.0/8     — second octet 0..=255 (256 /16s)
///   172.16.0.0/12  — second octet 16..=31 (16 /16s)
///   192.168.0.0/16 — second octet == 168 (the whole block, a single /16)
///
/// The third/fourth octets (subnet + host within the block) are intentionally
/// unconstrained — they're the user's `routerOctet` and the fixed `.1` host.
/// The gateway to collision-test when the LAN moves from `current` to `new`.
///
/// On a network-block change every profile keeps its 3rd octet (see
/// [`update_profile_ips_for_block_change`]), so testing the new 3rd octet inside
/// the *current* block is equivalent to testing the post-move state — and it
/// lets the guard run before any config is mutated. With no current address
/// there is nothing to translate; test the requested address as-is.
fn subnet_guard_ip(current: Option<Ipv4Addr>, new: Ipv4Addr) -> Ipv4Addr {
    match current {
        Some(old) => {
            let (o, n) = (old.octets(), new.octets());
            Ipv4Addr::new(o[0], o[1], n[2], n[3])
        }
        None => new,
    }
}

pub(crate) fn validate_lan_block(addr: Ipv4Addr) -> Result<(), Error> {
    if addr.is_private() {
        Ok(())
    } else {
        Err(Error::new(
            eyre!(
                "LAN IPv4 {addr} is outside the supported private ranges \
                 (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)"
            ),
            ErrorKind::InvalidRequest,
        ))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LanIpv4Response {
    pub address: String,
    pub netmask: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LanIpv4SetRequest {
    pub address: String,
    /// When true, forcibly delete VPN peers that would break due to block change.
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LanIpv6Response {
    pub slaac: bool,
    pub dhcpv6: bool,
    /// Prefix delegation length, e.g. 64
    pub prefix: u8,
    /// Current IPv6 address (if assigned)
    pub ip6addr: Option<String>,
    /// WAN prefix length (read-only context for the UI)
    pub wan_prefix: u8,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LanIpv6SetRequest {
    pub slaac: bool,
    pub dhcpv6: bool,
    pub prefix: u8,
}

pub fn lan<C: CtrlContext + Clone>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("ipv4-get", from_fn_async_local(ipv4_get::<C>).with_display_serializable())
        .subcommand("ipv4-set", from_fn_async_local(ipv4_set::<C>).with_display_serializable())
        .subcommand("ipv6-get", from_fn_async_local(ipv6_get::<C>).with_display_serializable())
        .subcommand("ipv6-set", from_fn_async_local(ipv6_set::<C>).with_display_serializable())
}

#[instrument(skip_all)]
pub async fn ipv4_get<C: CtrlContext>(ctx: C) -> Result<LanIpv4Response, Error> {
    let arena = Arena::new();
    let cfgs = parse_all(ctx.uci_root(), &arena, &["network"]).await?;

    for section in &cfgs["network"].sections {
        if section.name().as_deref() == Some(LAN_INTERFACE) {
            if let Some(iface) = section.get_typed::<NetworkInterface>()? {
                return Ok(LanIpv4Response {
                    address: iface
                        .ipaddr
                        .map(|ip| ip.to_string())
                        .unwrap_or_else(|| "192.168.1.1".to_string()),
                    netmask: iface
                        .netmask
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| LAN_NETMASK.to_string()),
                });
            }
        }
    }

    Ok(LanIpv4Response {
        address: "192.168.1.1".to_string(),
        netmask: LAN_NETMASK.to_string(),
    })
}

#[instrument(skip_all)]
pub async fn ipv4_set<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<LanIpv4SetRequest>,
) -> Result<(), Error> {
    let address: Ipv4Addr = req
        .address
        .parse()
        .map_err(|_| Error::new(eyre!("Invalid IPv4 address: {}", req.address), ErrorKind::ParseNetAddress))?;
    // Enforce the RFC 1918 block boundaries the UI exposes — fail before
    // touching any config so a bad value can't partially apply.
    validate_lan_block(address)?;

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["network", "startwrt", "dhcp", "firewall"]).await?;

        // Reject a /24 owned by another profile before touching any config — a
        // colliding LAN subnet strands the router (same guard as
        // profiles.create/profiles.edit; the UI can no longer produce this, but
        // a direct RPC/CLI call can). Must run before the mutation below: the
        // guard reads the LAN's current address to allow no-op edits.
        let current_lan_ip = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some(LAN_INTERFACE))
            .and_then(|s| s.get_typed::<NetworkInterface>().ok().flatten())
            .and_then(|i| i.ipaddr);
        profiles::guard_subnet_collision(
            &cfgs,
            subnet_guard_ip(current_lan_ip, address),
            Some(LAN_INTERFACE),
        )?;

        // Update LAN interface, capturing old IP to detect network block changes
        let mut old_address: Option<Ipv4Addr> = None;
        let mut found = false;
        for section in &mut cfgs["network"].sections {
            if section.name().as_deref() == Some(LAN_INTERFACE) {
                if let Some(mut iface) = section.get_typed::<NetworkInterface>()? {
                    old_address = iface.ipaddr;
                    iface.ipaddr = Some(address);
                    iface.netmask = Some(LAN_NETMASK);
                    section.set(&iface)?;
                    found = true;
                    break;
                }
            }
        }
        if !found {
            return Err(Error::new(eyre!("LAN interface not found in network config"), ErrorKind::MissingLanInterface));
        }

        // Detect network block change (first two octets differ)
        let block_changed = match old_address {
            Some(old) => {
                old.octets()[0] != address.octets()[0]
                    || old.octets()[1] != address.octets()[1]
            }
            None => false,
        };

        // If the network block changed, update all profile interfaces and routing rules
        let profile_interfaces: HashSet<String> = if block_changed {
            let pi: HashSet<String> = profiles::list_config(ctx.clone(), &cfgs)?
                .into_iter()
                .map(|p| p.interface)
                .collect();
            update_profile_ips_for_block_change(&mut cfgs, old_address.unwrap(), address, &pi)?;
            pi
        } else {
            HashSet::new()
        };

        // Guard: reject subnet change when DHCP static hosts exist in the old subnet
        let ip_changed = old_address.map(|old| old != address).unwrap_or(false);
        if block_changed {
            let old = old_address.unwrap().octets();
            guard_dhcp_static_hosts(&cfgs, &[old[0], old[1]])?;
        } else if ip_changed {
            if let Some(old) = old_address {
                let o = old.octets();
                guard_dhcp_static_hosts(&cfgs, &[o[0], o[1], o[2]])?;
            }
        }

        // Guard: check if IP change would break VPN peers
        let removed_vpns = if ip_changed {
            let affected: Vec<&str> = if block_changed {
                // Block change affects all profiles
                profile_interfaces.iter().map(|s| s.as_str())
                    .chain(std::iter::once(LAN_INTERFACE))
                    .collect()
            } else {
                // Only the admin profile's subnet changed
                vec![LAN_INTERFACE]
            };
            crate::vpn_server::guard_vpn_peers(&mut cfgs, &affected, req.force)?
        } else {
            Default::default()
        };

        let dump_result = dump_all(ctx.uci_root(), cfgs).await;
        drop(arena);
        match dump_result {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log(
                    "lan", "ipv4-updated", false,
                    &format!("Failed to update LAN IPv4 to {address}"),
                    Some(&err.to_string()),
                );
                return Err(err.into());
            }
            Ok(()) => {
                if block_changed {
                    crate::activity::log(
                        "lan", "ipv4-block-changed", true,
                        &format!("Updated LAN IPv4 to {address} (subnet block changed — all profiles updated)"),
                        None,
                    );
                } else {
                    crate::activity::log(
                        "lan", "ipv4-updated", true,
                        &format!("Updated LAN IPv4 to {address}"),
                        None,
                    );
                }

                if ctx.effectful() {
                    // Regenerate server cert with updated LAN IP as SAN
                    let ip_changed = old_address.map_or(true, |old| old != address);
                    if ip_changed {
                        let addrs = crate::ssl::LanAddresses {
                            ipv4: address,
                            // IPv6 hasn't changed, carry it forward from the live system
                            ipv6: crate::ssl::read_lan_ipv6_from_ubus().await,
                        };
                        if let Err(e) = crate::ssl::regenerate_server_cert(&addrs).await {
                            tracing::error!("failed to regenerate server cert: {e}");
                        }
                    }

                    let mut ifaces: Vec<String> = vec![LAN_INTERFACE.to_string()];
                    ifaces.extend(profile_interfaces.iter().filter(|i| *i != LAN_INTERFACE).cloned());
                    restart_network_services(address, ifaces).await;
                    removed_vpns.apply_post_reload().await;
                }
                return Ok(());
            }
        }
    }
}

#[instrument(skip_all)]
pub async fn ipv6_get<C: CtrlContext>(ctx: C) -> Result<LanIpv6Response, Error> {
    let arena = Arena::new();
    let cfgs = parse_all(ctx.uci_root(), &arena, &["network", "dhcp"]).await?;

    // Read LAN interface for ip6assign and ip6addr
    let mut ip6assign: Option<String> = None;
    let mut ip6addr: Option<String> = None;
    for section in &cfgs["network"].sections {
        if section.name().as_deref() == Some(LAN_INTERFACE) {
            if let Some(iface) = section.get_typed::<NetworkInterface>()? {
                ip6assign = iface.ip6assign;
                ip6addr = iface.ip6addr;
                break;
            }
        }
    }

    // Read WAN6 interface for reqprefix
    let mut wan_prefix: u8 = 48;
    for section in &cfgs["network"].sections {
        if section.name().as_deref() == Some(WAN6_INTERFACE) {
            if let Some(iface) = section.get_typed::<NetworkInterface>()? {
                if let Some(rp) = &iface.reqprefix {
                    if rp != "auto" {
                        if let Ok(p) = rp.parse::<u8>() {
                            wan_prefix = p;
                        }
                    }
                }
                break;
            }
        }
    }

    // Read DHCP LAN section for RA/DHCPv6
    let mut slaac = false;
    let mut dhcpv6 = false;
    for section in &cfgs["dhcp"].sections {
        if section.name().as_deref() == Some(LAN_INTERFACE) {
            if let Some(dhcp) = section.get_typed::<Dhcp>()? {
                slaac = dhcp.ra.as_deref() == Some("server");
                dhcpv6 = dhcp.dhcpv6.as_deref() == Some("server");
                break;
            }
        }
    }

    let prefix = ip6assign
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(64);

    // Drop arena before any .await — Arena is !Send
    drop(cfgs);
    drop(arena);

    // Prefer the UCI static address; fall back to the runtime ULA address
    // assigned by odhcpd (which isn't written to UCI).
    let ip6addr = match ip6addr {
        Some(v) => Some(v),
        None => crate::ssl::read_lan_ipv6_from_ubus().await.map(|a| a.to_string()),
    };

    // Runtime delegation status
    Ok(LanIpv6Response {
        slaac,
        dhcpv6,
        prefix,
        ip6addr,
        wan_prefix,
    })
}

#[instrument(skip_all)]
pub async fn ipv6_set<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<LanIpv6SetRequest>,
) -> Result<(), Error> {
    // DHCPv6 requires SLAAC (RA) — normalize to prevent inconsistent state
    let req = LanIpv6SetRequest {
        dhcpv6: req.slaac && req.dhcpv6,
        ..req
    };

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["network", "dhcp", "startwrt"]).await?;

        // Update LAN network interface ip6assign
        let mut found_network = false;
        for section in &mut cfgs["network"].sections {
            if section.name().as_deref() == Some(LAN_INTERFACE) {
                if let Some(mut iface) = section.get_typed::<NetworkInterface>()? {
                    if req.slaac || req.dhcpv6 {
                        iface.ip6assign = Some(req.prefix.to_string());
                    } else {
                        iface.ip6assign = None;
                    }
                    section.set(&iface)?;
                    found_network = true;
                    break;
                }
            }
        }
        if !found_network {
            return Err(Error::new(eyre!("LAN interface not found in network config"), ErrorKind::MissingLanInterface));
        }

        // Update DHCP LAN section RA/DHCPv6
        let mut found_dhcp = false;
        for section in &mut cfgs["dhcp"].sections {
            if section.name().as_deref() == Some(LAN_INTERFACE) {
                if let Some(mut dhcp) = section.get_typed::<Dhcp>()? {
                    dhcp.ra = Some(if req.slaac { "server" } else { "disabled" }.to_string());
                    dhcp.dhcpv6 =
                        Some(if req.dhcpv6 { "server" } else { "disabled" }.to_string());
                    section.set(&dhcp)?;
                    found_dhcp = true;
                    break;
                }
            }
        }
        if !found_dhcp {
            return Err(Error::new(eyre!("LAN section not found in DHCP config"), ErrorKind::MissingLanInterface));
        }

        // Update all profile DHCP/network sections to match global IPv6 state.
        // The admin LAN keeps the full delegated prefix (the ISP's GUA /64, set
        // above). Non-admin VLAN profiles get a /64 carved from the device's ULA
        // prefix when their outbound carries IPv6 — odhcpd hands it to clients and
        // the vpn_<X> zone NAT66s it out the tunnel. Profiles whose outbound VPN
        // lacks IPv6 get none, so v6 can't leak outside the tunnel.
        let ipv6_requested = req.slaac || req.dhcpv6;
        let mut profile_ipv6_map: HashMap<String, bool> = HashMap::new();
        for section in &cfgs["startwrt"].sections {
            if let Ok(p) = section.get::<profiles::UciProfile>() {
                let outbound = p.outbound.unwrap_or_else(|| "wan".to_string());
                let has_ipv6 = ipv6_requested
                    && profiles::outbound_supports_ipv6(&cfgs, &outbound);
                profile_ipv6_map.insert(p.interface.clone(), has_ipv6);
            }
        }

        // Sync ip6assign on profile interfaces to match each profile's IPv6
        // state: v6-capable profiles get a /64 (a ULA carved from the device
        // prefix, NAT66'd out the VPN by the vpn_<X> zone's masq6); the rest
        // get none. Mirrors set_config/create_config in profiles.rs — the LAN
        // interface keeps the full delegated prefix, handled above.
        for section in &mut cfgs["network"].sections {
            if let Some(name) = section.name() {
                if name.as_ref() != LAN_INTERFACE {
                    if let Some(&has_ipv6) = profile_ipv6_map.get(name.as_ref()) {
                        if let Some(mut iface) = section.get_typed::<NetworkInterface>()? {
                            let want = if has_ipv6 { Some("64".to_string()) } else { None };
                            if iface.ip6assign != want {
                                iface.ip6assign = want;
                                section.set(&iface)?;
                            }
                        }
                    }
                }
            }
        }

        for section in &mut cfgs["dhcp"].sections {
            if let Some(name) = section.name() {
                if let Some(&profile_ipv6) = profile_ipv6_map.get(name.as_ref()) {
                    if let Some(mut dhcp) = section.get_typed::<Dhcp>()? {
                        dhcp.ra = Some(if profile_ipv6 && req.slaac { "server" } else { "disabled" }.to_string());
                        dhcp.dhcpv6 =
                            Some(if profile_ipv6 && req.dhcpv6 { "server" } else { "disabled" }.to_string());
                        section.set(&dhcp)?;
                    }
                }
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
                crate::activity::log("lan", "ipv6-updated", false, "Failed to update LAN IPv6 settings", Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                crate::activity::log("lan", "ipv6-updated", true, "Updated LAN IPv6 settings", None);
                if ctx.effectful() {
                    let ipv6_enabled = req.slaac || req.dhcpv6;

                    // Restart odhcpd first so it can send a deprecation RA
                    // (prefix lifetimes=0) while the network is still up.
                    // Without this, disabling IPv6 leaves clients with stale
                    // SLAAC addresses until they naturally expire.
                    let _ = crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/odhcpd").arg("restart")).await;
                    let _ = crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/network").arg("restart")).await;

                    // Regenerate server cert to include/remove IPv6 SAN.
                    // When enabling IPv6, poll for the address to appear on
                    // the LAN interface — network restart is async and odhcpd
                    // needs time to assign the ULA address.
                    let ipv6 = if ipv6_enabled {
                        let mut found = None;
                        for _ in 0..30 {
                            if let Some(addr) = crate::ssl::read_lan_ipv6_from_ubus().await {
                                found = Some(addr);
                                break;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                        if found.is_none() {
                            tracing::warn!("IPv6 address not yet available on LAN after network restart");
                        }
                        found
                    } else {
                        None
                    };
                    let addrs = crate::ssl::LanAddresses {
                        ipv4: crate::ssl::read_lan_ip(&ctx.uci_root()).await,
                        ipv6,
                    };
                    if let Err(e) = crate::ssl::regenerate_server_cert(&addrs).await {
                        tracing::error!("failed to regenerate server cert after IPv6 change: {e}");
                    }
                }
                return Ok(());
            }
        }
    }
}

/// Guard: reject subnet changes when DHCP static hosts exist in the affected subnet.
/// `old_prefix` is 2 bytes for a block change (first two octets) or 3 bytes for a
/// single-profile subnet change (first three octets).
///
/// Note: this must run after `update_profile_ips_for_block_change` is safe because
/// that function does not touch DHCP host sections — the old static IPs are still present.
pub(crate) fn guard_dhcp_static_hosts(cfgs: &Configs, old_prefix: &[u8]) -> Result<(), Error> {
    let affected: Vec<String> = cfgs["dhcp"]
        .sections
        .iter()
        .filter_map(|section| section.get::<DhcpHost>().ok())
        .filter_map(|host| {
            let ip: Ipv4Addr = host.ip.as_deref()?.parse().ok()?;
            let octets = ip.octets();
            if old_prefix.iter().enumerate().all(|(i, &p)| octets[i] == p) {
                let label = host.name.as_deref().unwrap_or(&host.mac);
                Some(format!("{} ({})", label, ip))
            } else {
                None
            }
        })
        .collect();

    if !affected.is_empty() {
        return Err(Error::new(
            eyre!(
                "cannot change subnet while devices have static IP reservations: {affected:?}"
            ),
            ErrorKind::DhcpStaticHostsInSubnet,
        ));
    }
    Ok(())
}

/// Update all profile network interface IPs and routing rules when the network
/// block (first two octets) changes. Preserves each profile's 3rd/4th octets.
pub fn update_profile_ips_for_block_change(
    cfgs: &mut uciedit::Configs,
    old_address: Ipv4Addr,
    new_address: Ipv4Addr,
    profile_interfaces: &HashSet<String>,
) -> Result<(), Error> {
    let old_oct = old_address.octets();
    let new_oct = new_address.octets();

    // Update profile network interface IPs
    for section in &mut cfgs["network"].sections {
        if let Some(name) = section.name() {
            if profile_interfaces.contains(name.as_ref()) {
                if let Some(mut iface) = section.get_typed::<NetworkInterface>()? {
                    if let Some(ip) = iface.ipaddr {
                        let o = ip.octets();
                        iface.ipaddr = Some(Ipv4Addr::new(
                            new_oct[0], new_oct[1], o[2], o[3],
                        ));
                        section.set(&iface)?;
                    }
                }
            }
        }
    }

    // Update policy routing rules (prr_<interface>) src field
    for section in &mut cfgs["network"].sections {
        if let Some(name) = section.name() {
            if let Some(iface_name) = name.strip_prefix("prr_") {
                if profile_interfaces.contains(iface_name) {
                    if let Some(mut rule) = section.get_typed::<NetworkRule>()? {
                        if let Some(ref src) = rule.src {
                            if let Some(ip_part) = src.split('/').next() {
                                if let Ok(src_ip) = ip_part.parse::<Ipv4Addr>() {
                                    rule.src = Some(format!(
                                        "{}.{}.{}.0/24",
                                        new_oct[0], new_oct[1], src_ip.octets()[2]
                                    ));
                                    section.set(&rule)?;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Update policy routing local routes (plr_<interface>) target field
    for section in &mut cfgs["network"].sections {
        if let Some(name) = section.name() {
            if let Some(iface_name) = name.strip_prefix("plr_") {
                if profile_interfaces.contains(iface_name) {
                    if let Some(mut route) = section.get_typed::<NetworkRoute>()? {
                        if let Some(ip_part) = route.target.split('/').next() {
                            if let Ok(tgt) = ip_part.parse::<Ipv4Addr>() {
                                route.target = format!(
                                    "{}.{}.{}.0/24",
                                    new_oct[0], new_oct[1], tgt.octets()[2]
                                );
                                section.set(&route)?;
                            }
                        }
                    }
                }
            }
        }
    }

    // Update per-profile dnsmasq listen_address (dns_<interface>)
    for section in &mut cfgs["dhcp"].sections {
        if let Some(name) = section.name() {
            if let Some(iface_name) = name.strip_prefix("dns_") {
                if profile_interfaces.contains(iface_name) {
                    if let Some(mut dnsmasq) = section.get_typed::<ProfileDnsmasq>()? {
                        let mut changed = false;
                        for addr in &mut dnsmasq.listen_address {
                            if let Ok(ip) = addr.parse::<Ipv4Addr>() {
                                let o = ip.octets();
                                *addr = Ipv4Addr::new(
                                    new_oct[0], new_oct[1], o[2], o[3],
                                ).to_string();
                                changed = true;
                            }
                        }
                        if changed {
                            section.set(&dnsmasq)?;
                        }
                    }
                }
            }
        }
    }

    // Update DNS-Override firewall redirects dest_ip
    for section in &mut cfgs["firewall"].sections {
        if let Ok(mut redir) = section.get::<FirewallRedirect>() {
            if redir.name.contains("DNS-Override") {
                if let Some(ref dest) = redir.dest_ip {
                    if let Ok(ip) = dest.parse::<Ipv4Addr>() {
                        let o = ip.octets();
                        if o[0] == old_oct[0] && o[1] == old_oct[1] {
                            redir.dest_ip = Some(Ipv4Addr::new(
                                new_oct[0], new_oct[1], o[2], o[3],
                            ).to_string());
                            section.set(&redir)?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Reload network services after an IP change.
/// Cycles only the specified interfaces via ifdown/ifup (instead of
/// `network reload` which restarts ALL interfaces including WAN, causing
/// its DHCP client to lose its lease and breaking internet).
/// Then reloads firewall and restarts dnsmasq once br-lan has the new IP.
pub async fn restart_network_services(new_lan_ip: Ipv4Addr, interfaces: Vec<String>) {
    // Clear DHCP leases — old leases reference the previous subnet and would
    // cause clients to receive stale IPs. Remove the base file and every
    // per-profile `/tmp/dhcp.leases.dns_*`.
    for path in crate::devices::dhcp_lease_files().await {
        let _ = tokio::fs::remove_file(&path).await;
    }
    // Cycle only the changed interfaces — `network reload` restarts ALL
    // interfaces including WAN, causing its DHCP client to lose its lease.
    for iface in &interfaces {
        let _ = crate::run_quiet_async(tokio::process::Command::new("ifdown").arg(iface)).await;
    }
    for iface in &interfaces {
        let _ = crate::run_quiet_async(tokio::process::Command::new("ifup").arg(iface)).await;
    }
    // Safety: wait for br-lan to have the new IP before reloading
    // dependent services. ifup should be synchronous for static
    // interfaces, but poll as a safety net.
    let expected = new_lan_ip.to_string();
    for _ in 0..30 {
        if let Ok(out) = tokio::process::Command::new("ip")
            .args(["-4", "-o", "addr", "show", "br-lan"])
            .invoke(ErrorKind::Network.into())
            .await
        {
            if String::from_utf8_lossy(&out).contains(&expected) {
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    // Regenerate nftables rules so masquerade/NAT covers the new subnets.
    let _ = crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/firewall").arg("reload")).await;
    // Full restart (not reload) so dnsmasq rebinds to new interface IPs.
    // A reload (SIGHUP) re-reads config but doesn't rebind listeners.
    let _ = crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/dnsmasq").arg("restart")).await;
    // Bounce WiFi so all clients disassociate and reassociate,
    // triggering fresh DHCP on the new subnet. Without this,
    // clients that stay connected keep stale leases from the
    // old network block.
    let _ = crate::run_quiet_async(&mut tokio::process::Command::new("wifi")).await;
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

    /// Write minimal network + dhcp UCI configs for testing.
    fn setup_fixtures(dir: &std::path::Path) {
        std::fs::write(
            dir.join("network"),
            "\
config interface 'lan'
\toption device 'br-lan'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.0.0'
\toption ip6assign '60'

config interface 'wan6'
\toption device '@wan'
\toption proto 'dhcpv6'
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
\toption dhcpv6 'server'
",
        )
        .unwrap();
    }

    /// Write a minimal startwrt config (needed by ipv6_set's profile sync).
    fn setup_startwrt(dir: &std::path::Path) {
        std::fs::write(
            dir.join("startwrt"),
            "\
config profile lan
\toption fullname 'Admin'
\toption interface 'lan'
\toption vlan_tag '99'
",
        )
        .unwrap();
    }

    // ── Block validation ────────────────────────────────────────

    #[test]
    fn validate_lan_block_accepts_supported_ranges() {
        for ip in [
            "10.0.0.1",
            "10.255.0.1",
            "172.16.0.1",
            "172.31.0.1",
            "192.168.0.1",
            "192.168.255.1",
        ] {
            assert!(
                validate_lan_block(ip.parse().unwrap()).is_ok(),
                "{ip} should be accepted"
            );
        }
    }

    #[test]
    fn validate_lan_block_rejects_out_of_block() {
        for ip in [
            "172.15.0.1", // below the 172.16/12 block
            "172.32.0.1", // above the 172.16/12 block
            "192.167.0.1",
            "192.169.0.1",
            "8.8.8.8",
            "127.0.0.1",
            "169.254.0.1",
        ] {
            let err = validate_lan_block(ip.parse().unwrap()).unwrap_err();
            assert_eq!(
                err.kind,
                ErrorKind::InvalidRequest,
                "{ip} should be rejected as InvalidRequest"
            );
        }
    }

    #[tokio::test]
    async fn ipv4_set_rejects_out_of_block_address() {
        let dir = tempfile::tempdir().unwrap();
        setup_fixtures(dir.path());
        setup_startwrt(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let err = ipv4_set(
            ctx,
            DeserializeStdin(LanIpv4SetRequest {
                address: "172.15.0.1".to_string(),
                force: false,
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(err.kind, ErrorKind::InvalidRequest);
    }

    // ── IPv4 tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn ipv4_get_returns_address() {
        let dir = tempfile::tempdir().unwrap();
        setup_fixtures(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let res = ipv4_get(ctx).await.unwrap();
        assert_eq!(res.address, "192.168.1.1");
        assert_eq!(res.netmask, "255.255.0.0");
    }

    #[tokio::test]
    async fn ipv4_get_defaults_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("network"), "").unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        let res = ipv4_get(ctx).await.unwrap();
        assert_eq!(res.address, "192.168.1.1");
        assert_eq!(res.netmask, "255.255.255.0");
    }

    #[tokio::test]
    async fn ipv4_set_updates_address() {
        let dir = tempfile::tempdir().unwrap();
        setup_fixtures(dir.path());
        setup_startwrt(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        ipv4_set(
            ctx.clone(),
            DeserializeStdin(LanIpv4SetRequest {
                address: "10.0.0.1".to_string(),
                force: false,
            }),
        )
        .await
        .unwrap();

        let res = ipv4_get(ctx).await.unwrap();
        assert_eq!(res.address, "10.0.0.1");
        assert_eq!(res.netmask, "255.255.255.0");
    }

    #[tokio::test]
    async fn ipv4_set_errors_when_lan_missing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("network"), "").unwrap();
        setup_startwrt(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let err = ipv4_set(
            ctx,
            DeserializeStdin(LanIpv4SetRequest {
                address: "10.0.0.1".to_string(),
                force: false,
            }),
        )
        .await
        .unwrap_err();

        assert!(
            err.to_string().contains("LAN interface not found"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn ipv4_set_propagates_block_change_to_profiles() {
        let dir = tempfile::tempdir().unwrap();
        // Network config with LAN + a Guest profile interface + routing rules
        std::fs::write(
            dir.path().join("network"),
            "\
config interface 'lan'
\toption device 'br-lan'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.0.0'

config interface 'guest'
\toption device 'br-lan.3'
\toption proto 'static'
\toption ipaddr '192.168.2.1'
\toption netmask '255.255.255.0'

config rule 'prr_guest'
\toption src '192.168.2.0/24'
\toption lookup '200'

config route 'plr_guest'
\toption interface 'guest'
\toption target '192.168.2.0/24'
\toption table '200'
",
        )
        .unwrap();

        // startwrt config with LAN + Guest profiles
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
\toption vlan_tag '3'
",
        )
        .unwrap();

        // dhcp config with per-profile dnsmasq
        std::fs::write(
            dir.path().join("dhcp"),
            "\
config dnsmasq 'dns_lan'
\tlist listen_address '192.168.1.1'
\toption localservice '1'

config dnsmasq 'dns_guest'
\tlist listen_address '192.168.2.1'
\toption localservice '1'
",
        )
        .unwrap();

        // firewall config with DNS-Override redirects
        std::fs::write(
            dir.path().join("firewall"),
            "\
config redirect 'dns_override_admin'
\toption name 'DNS-Override-Admin'
\toption src 'vlan_lan'
\tlist proto 'tcp'
\tlist proto 'udp'
\toption src_dport '53'
\toption dest_ip '192.168.1.1'
\toption dest_port '53'
\toption target 'DNAT'

config redirect 'dns_override_guest'
\toption name 'DNS-Override-Guest'
\toption src 'vlan_guest'
\tlist proto 'tcp'
\tlist proto 'udp'
\toption src_dport '53'
\toption dest_ip '192.168.2.1'
\toption dest_port '53'
\toption target 'DNAT'
",
        )
        .unwrap();

        let ctx = TestContext(dir.path().to_path_buf());

        // Change from 192.168.x.x to 10.0.x.x
        ipv4_set(
            ctx.clone(),
            DeserializeStdin(LanIpv4SetRequest {
                address: "10.0.1.1".to_string(),
                force: false,
            }),
        )
        .await
        .unwrap();

        // Verify LAN IP updated
        let res = ipv4_get(ctx.clone()).await.unwrap();
        assert_eq!(res.address, "10.0.1.1");

        // Re-read all configs
        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["network", "dhcp", "firewall"]).await.unwrap();

        for section in &cfgs["network"].sections {
            if section.name().as_deref() == Some("guest") {
                let iface = section.get_typed::<NetworkInterface>().unwrap().unwrap();
                assert_eq!(
                    iface.ipaddr.unwrap(),
                    Ipv4Addr::new(10, 0, 2, 1),
                    "Guest interface IP should be updated to new block"
                );
            }
            if section.name().as_deref() == Some("prr_guest") {
                let rule = section.get_typed::<NetworkRule>().unwrap().unwrap();
                assert_eq!(
                    rule.src.as_deref(), Some("10.0.2.0/24"),
                    "Guest routing rule src should be updated to new block"
                );
            }
            if section.name().as_deref() == Some("plr_guest") {
                let route = section.get::<NetworkRoute>().unwrap();
                assert_eq!(
                    route.target, "10.0.2.0/24",
                    "Guest local route target should be updated to new block"
                );
            }
        }

        // Verify per-profile dnsmasq listen_address updated
        for section in &cfgs["dhcp"].sections {
            if section.name().as_deref() == Some("dns_lan") {
                let dnsmasq = section.get::<ProfileDnsmasq>().unwrap();
                assert_eq!(
                    dnsmasq.listen_address,
                    vec!["10.0.1.1"],
                    "Admin dnsmasq listen_address should be updated to new block"
                );
            }
            if section.name().as_deref() == Some("dns_guest") {
                let dnsmasq = section.get::<ProfileDnsmasq>().unwrap();
                assert_eq!(
                    dnsmasq.listen_address,
                    vec!["10.0.2.1"],
                    "Guest dnsmasq listen_address should be updated to new block"
                );
            }
        }

        // Verify DNS-Override firewall redirects updated
        for section in &cfgs["firewall"].sections {
            if let Ok(redir) = section.get::<FirewallRedirect>() {
                if redir.name == "DNS-Override-Admin" {
                    assert_eq!(
                        redir.dest_ip.as_deref(),
                        Some("10.0.1.1"),
                        "Admin DNS-Override dest_ip should be updated to new block"
                    );
                }
                if redir.name == "DNS-Override-Guest" {
                    assert_eq!(
                        redir.dest_ip.as_deref(),
                        Some("10.0.2.1"),
                        "Guest DNS-Override dest_ip should be updated to new block"
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn ipv4_set_same_block_does_not_touch_profiles() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("network"),
            "\
config interface 'lan'
\toption device 'br-lan'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.0.0'

config interface 'guest'
\toption device 'br-lan.3'
\toption proto 'static'
\toption ipaddr '192.168.2.1'
\toption netmask '255.255.255.0'
",
        )
        .unwrap();

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
\toption vlan_tag '3'
",
        )
        .unwrap();

        let ctx = TestContext(dir.path().to_path_buf());

        // Change within same /16 block
        ipv4_set(
            ctx.clone(),
            DeserializeStdin(LanIpv4SetRequest {
                address: "192.168.5.1".to_string(),
                force: false,
            }),
        )
        .await
        .unwrap();

        // Guest should be untouched
        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["network"]).await.unwrap();

        for section in &cfgs["network"].sections {
            if section.name().as_deref() == Some("guest") {
                let iface = section.get_typed::<NetworkInterface>().unwrap().unwrap();
                assert_eq!(
                    iface.ipaddr.unwrap(),
                    Ipv4Addr::new(192, 168, 2, 1),
                    "Guest interface IP should remain unchanged for same-block change"
                );
            }
        }
    }

    #[tokio::test]
    async fn ipv4_set_rejects_colliding_profile_subnet() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("network"),
            "\
config interface 'lan'
\toption device 'br-lan'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.0.0'

config interface 'guest'
\toption device 'br-lan.3'
\toption proto 'static'
\toption ipaddr '192.168.2.1'
\toption netmask '255.255.255.0'
",
        )
        .unwrap();

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
\toption vlan_tag '3'
",
        )
        .unwrap();

        let ctx = TestContext(dir.path().to_path_buf());

        let set = |address: &str| {
            ipv4_set(
                ctx.clone(),
                DeserializeStdin(LanIpv4SetRequest {
                    address: address.to_string(),
                    force: false,
                }),
            )
        };

        // Same-block move onto the guest's /24 — rejected, config untouched.
        let err = set("192.168.2.1").await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::SubnetCollision);
        assert_eq!(ipv4_get(ctx.clone()).await.unwrap().address, "192.168.1.1");

        // Block change whose 3rd octet lands on the guest's — profiles keep
        // their 3rd octet across the move, so this collides post-move.
        let err = set("10.0.2.1").await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::SubnetCollision);
        assert_eq!(ipv4_get(ctx.clone()).await.unwrap().address, "192.168.1.1");

        // No-op re-set of the current address — allowed.
        set("192.168.1.1").await.unwrap();

        // Block change keeping the LAN's own 3rd octet — allowed.
        set("10.0.1.1").await.unwrap();
        assert_eq!(ipv4_get(ctx.clone()).await.unwrap().address, "10.0.1.1");
    }

    #[test]
    fn subnet_guard_ip_translates_into_current_block() {
        // Block change: test the new 3rd octet inside the current block.
        assert_eq!(
            subnet_guard_ip(
                Some(Ipv4Addr::new(192, 168, 1, 1)),
                Ipv4Addr::new(10, 0, 2, 1)
            ),
            Ipv4Addr::new(192, 168, 2, 1)
        );
        // Same block: passes through unchanged.
        assert_eq!(
            subnet_guard_ip(
                Some(Ipv4Addr::new(192, 168, 1, 1)),
                Ipv4Addr::new(192, 168, 5, 1)
            ),
            Ipv4Addr::new(192, 168, 5, 1)
        );
        // No current address: nothing to translate.
        assert_eq!(
            subnet_guard_ip(None, Ipv4Addr::new(10, 0, 2, 1)),
            Ipv4Addr::new(10, 0, 2, 1)
        );
    }

    // ── IPv6 tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn ipv6_get_returns_settings() {
        let dir = tempfile::tempdir().unwrap();
        setup_fixtures(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let res = ipv6_get(ctx).await.unwrap();
        assert!(res.slaac);
        assert!(res.dhcpv6);
        assert_eq!(res.prefix, 60);
    }

    #[tokio::test]
    async fn ipv6_get_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("network"), "").unwrap();
        std::fs::write(dir.path().join("dhcp"), "").unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        let res = ipv6_get(ctx).await.unwrap();
        assert!(!res.slaac);
        assert!(!res.dhcpv6);
        assert_eq!(res.prefix, 64);
    }

    #[tokio::test]
    async fn ipv6_set_enables_slaac() {
        let dir = tempfile::tempdir().unwrap();
        setup_fixtures(dir.path());
        setup_startwrt(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        ipv6_set(
            ctx.clone(),
            DeserializeStdin(LanIpv6SetRequest {
                slaac: true,
                dhcpv6: false,
                prefix: 60,
            }),
        ).await
        .unwrap();

        let res = ipv6_get(ctx).await.unwrap();
        assert!(res.slaac);
        assert!(!res.dhcpv6);
        assert_eq!(res.prefix, 60);
    }

    #[tokio::test]
    async fn ipv6_set_disables_all() {
        let dir = tempfile::tempdir().unwrap();
        setup_fixtures(dir.path());
        setup_startwrt(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        ipv6_set(
            ctx.clone(),
            DeserializeStdin(LanIpv6SetRequest {
                slaac: false,
                dhcpv6: false,
                prefix: 64,
            }),
        ).await
        .unwrap();

        let res = ipv6_get(ctx).await.unwrap();
        assert!(!res.slaac);
        assert!(!res.dhcpv6);
        assert_eq!(res.prefix, 64);
    }

    #[tokio::test]
    async fn ipv6_set_errors_when_lan_missing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("network"), "").unwrap();
        std::fs::write(dir.path().join("dhcp"), "").unwrap();
        setup_startwrt(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let err = ipv6_set(
            ctx,
            DeserializeStdin(LanIpv6SetRequest {
                slaac: true,
                dhcpv6: true,
                prefix: 60,
            }),
        ).await
        .unwrap_err();

        assert!(
            err.to_string().contains("LAN interface not found")
                || err.to_string().contains("LAN section not found"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn ipv6_set_syncs_profile_ip6assign_by_outbound() {
        // Enabling IPv6 must give a profile whose outbound VPN carries v6 an
        // ip6assign='64' (ULA /64, NAT66'd out the tunnel) and strip it from a
        // profile on a v4-only VPN (so v6 can't leak). The admin LAN keeps its
        // configured prefix. Regression for the strip-everything loop.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("network"),
            "\
config interface 'lan'
\toption device 'br-lan'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.255.0'
\toption ip6assign '60'

config interface 'guest'
\toption device 'br-lan.101'
\toption proto 'static'
\toption ipaddr '192.168.101.1'
\toption netmask '255.255.255.0'

config interface 'iot'
\toption device 'br-lan.102'
\toption proto 'static'
\toption ipaddr '192.168.102.1'
\toption netmask '255.255.255.0'
\toption ip6assign '64'

config interface 'wg_v6'
\toption proto 'wireguard'
\toption private_key 'aGkmAm6PEDDjyZx/Lwc8AfwlVWuJOaKB6E5Hp+JqgVc='
\tlist addresses '10.2.0.2/32'
\tlist addresses 'fd00:2::2/128'

config interface 'wg_v4'
\toption proto 'wireguard'
\toption private_key 'aGkmAm6PEDDjyZx/Lwc8AfwlVWuJOaKB6E5Hp+JqgVc='
\tlist addresses '10.3.0.2/32'
",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("dhcp"),
            "\
config dhcp 'lan'
\toption interface 'lan'
\toption start '100'
\toption limit '150'
\toption leasetime '12h'
\toption ra 'server'
\toption dhcpv6 'server'

config dhcp 'guest'
\toption interface 'guest'
\toption start '100'
\toption limit '150'
\toption leasetime '12h'

config dhcp 'iot'
\toption interface 'iot'
\toption start '100'
\toption limit '150'
\toption leasetime '12h'
",
        )
        .unwrap();
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
\toption outbound 'wg_v6'

config profile iot
\toption fullname 'IoT'
\toption interface 'iot'
\toption vlan_tag '102'
\toption outbound 'wg_v4'
",
        )
        .unwrap();

        let ctx = TestContext(dir.path().to_path_buf());
        ipv6_set(
            ctx.clone(),
            DeserializeStdin(LanIpv6SetRequest {
                slaac: true,
                dhcpv6: false,
                prefix: 60,
            }),
        )
        .await
        .unwrap();

        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["network"]).await.unwrap();
        let ip6assign_of = |name: &str| -> Option<String> {
            cfgs["network"]
                .sections
                .iter()
                .find(|s| s.name().as_deref() == Some(name))
                .and_then(|s| s.get_typed::<NetworkInterface>().ok().flatten())
                .and_then(|i| i.ip6assign)
        };

        assert_eq!(
            ip6assign_of("lan").as_deref(),
            Some("60"),
            "admin LAN keeps its configured prefix"
        );
        assert_eq!(
            ip6assign_of("guest").as_deref(),
            Some("64"),
            "v6-capable VPN profile gains a /64"
        );
        assert_eq!(
            ip6assign_of("iot"),
            None,
            "v4-only VPN profile has ip6assign stripped to prevent v6 leak"
        );
    }
}
