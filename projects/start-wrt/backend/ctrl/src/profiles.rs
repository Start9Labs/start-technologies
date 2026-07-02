use crate::dns::{self, DnsServer};
use crate::ethernet::find_lan_bridge;
use crate::system::UciPreferences;
use crate::utils::DeserializeStdin;
use crate::CtrlContext;
use crate::prelude::*;
use crate::utils::HandlerExtSerde;
use clap::Parser;
use rpc_toolkit::{from_fn_async_local, HandlerExt as _, ParentHandler};
use serde::{Deserialize, Serialize};
use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use crate::invoke::Invoke;
use uciedit::openwrt::{
    DeviceType, Dhcp, FirewallForwarding, FirewallRedirect, FirewallRule, FirewallTarget,
    FirewallZone, InterfaceProto, NetworkBridgeVlan, NetworkDevice, NetworkInterface, NetworkRoute,
    NetworkRoute6, NetworkRule, NetworkRule6, NetworkVlanPort, NetworkVlanPortTagging,
    ProfileDnsmasq, WifiStation, WifiVlan,
};
use uciedit::{dump_all, parse_all, Arena, Configs, Line, LineComment, Token, TypedSection};

pub const DEFAULT_WAN_ZONE: &str = "wan";
/// Max length of a generated profile interface id. The binding constraint is the
/// WiFi VLAN netdev OpenWrt derives for a profile's `wifi-vlan`:
/// `<ap_ifname>-<iface>` (e.g. `phy0-ap0-<iface>`; see `iface_vlan` in
/// wifi-scripts' `ap.uc` / `hostapd.sh`). That name must fit Linux IFNAMSIZ
/// (15 chars). With a 2-digit phy/ap index the AP ifname is up to 9 chars,
/// leaving 5 for the id. (The VPN-server device `wg_<iface>` is looser, at 12.)
/// Do NOT raise this without re-checking every interface-derived netdev name.
pub const INTERFACE_NAME_LIMIT: usize = 5;

#[derive(Debug, Serialize, Deserialize)]
pub enum LanAccess<Id: Ord> {
    #[serde(rename = "ALL")]
    All,
    #[serde(rename = "SAME_PROFILE")]
    SameProfile,
    #[serde(rename = "other_profiles")]
    OtherProfiles(BTreeSet<Id>),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum WanAccess {
    #[serde(rename = "ALL")]
    All,
    #[serde(rename = "NONE")]
    None,
    #[serde(rename = "whitelist")]
    Whitelist(Vec<String>), // List of allowed destination IPs/CIDRs
    #[serde(rename = "blacklist")]
    Blacklist(Vec<String>), // List of blocked destination IPs/CIDRs
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleWindow {
    pub start_time: String,
    pub end_time: String,
    pub days: [bool; 7],
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScheduleWindows {
    pub interface: String,
    pub windows: Vec<ScheduleWindow>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Profile<Id: Ord = ProfileId> {
    #[serde(flatten)]
    pub id: Id,
    pub gateway_ip: Ipv4Addr,
    pub outbound: String,
    pub lan_access: LanAccess<Id>,
    pub wan_access: WanAccess,
    #[serde(default)]
    pub dns_override: Vec<DnsServer>,
    #[serde(default)]
    pub dns_source: String,
    pub access_to_new_profiles: bool,
    pub owns_lan: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProfileSetRequest {
    #[serde(flatten)]
    pub profile: Profile<ProfileIdOpt>,
    #[serde(default)]
    pub force: bool,
}

pub fn profiles<C: CtrlContext>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("get", from_fn_async_local(get::<C>).with_display_serializable())
        .subcommand("set", from_fn_async_local(set::<C>).with_display_serializable())
        .subcommand("delete", from_fn_async_local(delete::<C>).no_display())
        .subcommand("list", from_fn_async_local(list::<C>).with_display_serializable())
        .subcommand("create", from_fn_async_local(create::<C>).with_display_serializable())
        .subcommand("edit", from_fn_async_local(edit::<C>).with_display_serializable())
        .subcommand(
            "schedule-get",
            from_fn_async_local(schedule_get::<C>).with_display_serializable(),
        )
        .subcommand(
            "schedule-set",
            from_fn_async_local(schedule_set::<C>).no_display(),
        )
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProfileId {
    pub fullname: String,
    pub interface: String,
    pub vlan_tag: u16,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Parser)]
pub struct ProfileIdOpt {
    #[clap(short, long)]
    pub fullname: Option<String>,
    #[clap(short, long)]
    pub interface: Option<String>,
    #[clap(short, long)]
    pub vlan_tag: Option<u16>,
}

impl From<ProfileId> for ProfileIdOpt {
    fn from(id: ProfileId) -> Self {
        ProfileIdOpt {
            fullname: Some(id.fullname),
            interface: Some(id.interface),
            vlan_tag: Some(id.vlan_tag),
        }
    }
}

impl ProfileIdOpt {
    pub fn matches(&self, other: &ProfileIdOpt) -> bool {
        let i = match (&self.interface, &other.interface) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        };
        let v = match (self.vlan_tag, other.vlan_tag) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        };
        let n = match (&self.fullname, &other.fullname) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        };
        i && v && n
    }
}

#[derive(Debug, TypedSection)]
#[uci(ty = "profile")]
pub(crate) struct UciProfile {
    pub fullname: String,
    pub interface: String,
    pub vlan_tag: u16,
    pub outbound: Option<String>,
    #[uci(default_value = "false")]
    pub access_to_new_profiles: bool,
    #[uci(default)]
    pub wan_access: Option<String>,
    #[uci(default)]
    pub wan_access_list: Vec<String>,
    #[uci(default)]
    pub dns_override: Vec<String>,
    #[uci(default)]
    pub wan_schedule: Vec<String>,
}

/// Snapshot of profile fields before an update, for activity log diffing.
#[derive(Debug)]
struct OldProfileState {
    outbound: Option<String>,
    wan_access: Option<String>,
    access_to_new_profiles: bool,
    dns_override: Vec<String>,
    gateway_ip: Option<Ipv4Addr>,
}

impl UciProfile {
    pub fn id(&self) -> ProfileId {
        ProfileId {
            fullname: self.fullname.clone(),
            interface: self.interface.clone(),
            vlan_tag: self.vlan_tag,
        }
    }
}

fn wan_access_type_str(wa: &WanAccess) -> String {
    match wa {
        WanAccess::All => "all".into(),
        WanAccess::None => "none".into(),
        WanAccess::Whitelist(_) => "whitelist".into(),
        WanAccess::Blacklist(_) => "blacklist".into(),
    }
}

fn wan_access_destinations(wa: &WanAccess) -> Vec<String> {
    match wa {
        WanAccess::Whitelist(d) | WanAccess::Blacklist(d) => d.clone(),
        _ => Vec::new(),
    }
}

fn get_vpn_dns(cfgs: &Configs, vpn_interface: &str) -> Vec<String> {
    for section in &cfgs["network"].sections {
        if let Ok(iface) = section.get::<NetworkInterface>() {
            if section.name().as_deref() == Some(vpn_interface) && !iface.dns.is_empty() {
                return iface.dns.iter().map(|s| s.to_string()).collect();
            }
        }
    }
    Vec::new()
}

fn compute_dns_source(cfgs: &Configs, profile: &Profile) -> String {
    if !profile.dns_override.is_empty() {
        "custom".into()
    } else if profile.outbound != "wan" && !get_vpn_dns(cfgs, &profile.outbound).is_empty() {
        "vpn".into()
    } else {
        "system".into()
    }
}

/// Returns true if the profile has non-system DNS (custom override or VPN DNS).
fn has_effective_dns(cfgs: &Configs, profile: &Profile) -> bool {
    !profile.dns_override.is_empty()
        || (profile.outbound != "wan" && !get_vpn_dns(cfgs, &profile.outbound).is_empty())
}

/// Rewrite DNS forwarding (dnsmasq sections) for ALL profiles.
/// Call after any operation that changes system DNS settings.
pub(crate) fn rewrite_all_dns_forwarding(cfgs: &mut Configs) -> Result<(), Error> {
    // Collect all profiles first to avoid borrow issues
    let profiles: Vec<Profile> = cfgs["startwrt"]
        .sections
        .iter()
        .filter_map(|section| {
            let uci_profile = section.get::<UciProfile>().ok()?;
            let dns_override = dns::parse_dns_server_list(&uci_profile.dns_override);
            Some(Profile {
                id: uci_profile.id(),
                gateway_ip: {
                    // Look up gateway from network config
                    let interface = &uci_profile.interface;
                    cfgs["network"]
                        .sections
                        .iter()
                        .find_map(|s| {
                            if s.name().as_deref() == Some(interface) {
                                s.get::<NetworkInterface>().ok().and_then(|i| i.ipaddr)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(Ipv4Addr::new(0, 0, 0, 0))
                },
                outbound: uci_profile.outbound.unwrap_or_else(|| "wan".into()),
                lan_access: LanAccess::All, // Not used by rewrite_dns_forwarding
                wan_access: WanAccess::All,  // Not used by rewrite_dns_forwarding
                dns_override,
                dns_source: String::new(),
                access_to_new_profiles: false,
                owns_lan: false,
            })
        })
        .collect();

    for profile in &profiles {
        rewrite_dns_forwarding(cfgs, profile)?;
        rewrite_dns_redirect(cfgs, profile)?;
    }
    Ok(())
}

/// Rewrite the DNS-Override DNAT redirect rule for a single profile.
/// Adds or removes the redirect based on whether the profile needs DNS hijacking.
fn rewrite_dns_redirect(cfgs: &mut Configs, profile: &Profile) -> Result<(), Error> {
    // Find this profile's firewall zone name
    let zone_name = cfgs["firewall"]
        .sections
        .iter()
        .find_map(|s| {
            let zone = s.get::<FirewallZone>().ok()?;
            if zone.network.iter().any(|n| n == &profile.id.interface) {
                Some(zone.name.clone())
            } else {
                None
            }
        })
        .unwrap_or_else(|| format!("vlan_{}", profile.id.interface));

    // Remove existing DNS-Override redirect for this profile
    cfgs["firewall"].sections.retain(|section| {
        let Ok(redir) = section.get::<FirewallRedirect>() else {
            return true;
        };
        !(redir.src == zone_name && redir.name.contains("DNS-Override"))
    });

    // Add redirect if this profile needs DNS hijacking
    if has_effective_dns(cfgs, profile) || !dns::get_system_dns_servers(cfgs).is_empty() {
        cfgs["firewall"].append(
            &FirewallRedirect {
                name: format!(
                    "DNS-Override-{}",
                    profile.id.fullname.replace(" ", "-"),
                ),
                src: zone_name,
                proto: vec!["tcp".into(), "udp".into()],
                src_dport: Some("53".into()),
                dest_ip: Some(profile.gateway_ip.to_string()),
                dest_port: Some("53".into()),
                target: "DNAT".into(),
                ..Default::default()
            },
            None,
        )?;
    }
    Ok(())
}

pub(crate) fn rewrite_dns_forwarding(cfgs: &mut Configs, profile: &Profile) -> Result<(), Error> {
    let section_name = format!("dns_{}", profile.id.interface);

    // Remove any existing per-profile dnsmasq section
    cfgs["dhcp"]
        .sections
        .retain(|s| s.name().as_deref() != Some(&section_name));

    // Remove notinterface entry for this profile from main dnsmasq
    for section in &mut cfgs["dhcp"].sections {
        if section.ty() == "dnsmasq"
            && !section
                .name()
                .map(|n| n.starts_with("dns_"))
                .unwrap_or(false)
        {
            section.lines.retain(|line| {
                !matches!(line, Line::List { list, item, .. }
                    if list.as_str() == "notinterface"
                        && item.as_str() == profile.id.interface.as_str())
            });
            break;
        }
    }

    // Determine the dnsmasq server list based on DNS source:
    // 1. Custom DNS → SmartDNS profile group
    // 2. VPN DNS → dnsmasq server IP@interface directly (SmartDNS can't bind to VPN iface)
    // 3. System DNS (custom) → SmartDNS system group
    // 4. System DNS (ISP) → no per-profile dnsmasq (uses default resolvfile)
    let servers: Vec<String> = if !profile.dns_override.is_empty() {
        // Custom DNS: route through SmartDNS profile group
        let port = dns::smartdns_port_for_vlan(profile.id.vlan_tag);
        vec![format!("127.0.0.1#{}", port)]
    } else if profile.outbound != "wan" {
        // VPN DNS: use dnsmasq server IP@interface directly
        let vpn_dns = get_vpn_dns(cfgs, &profile.outbound);
        if vpn_dns.is_empty() {
            vec![]
        } else {
            vpn_dns
                .into_iter()
                .map(|ip| format!("{}@{}", ip, profile.outbound))
                .collect()
        }
    } else {
        // System DNS: route through SmartDNS system group if configured
        let system_dns = dns::get_system_dns_servers(cfgs);
        if !system_dns.is_empty() {
            vec![format!("127.0.0.1#{}", dns::SMARTDNS_SYSTEM_PORT)]
        } else {
            vec![] // ISP mode — no per-profile dnsmasq needed
        }
    };

    if !servers.is_empty() {
        cfgs["dhcp"].append(
            &ProfileDnsmasq {
                server: servers,
                noresolv: Some("1".to_string()),
                interface: vec![],
                localservice: Some("1".to_string()),
                nonwildcard: Some("1".to_string()),
                listen_address: vec![profile.gateway_ip.to_string()],
                notinterface: vec!["loopback".to_string()],
                rebind_domain: vec![],
                rebind_protection: Some("0".to_string()),
                localuse: Some("0".to_string()),
                leasefile: Some(format!("/tmp/dhcp.leases.{}", section_name)),
                domain: Some("lan".to_string()),
                expandhosts: Some("1".to_string()),
                boguspriv: Some("0".to_string()),
                local: Some("/lan/".to_string()),
            },
            Some(&section_name),
        )?;

        // Add notinterface to main dnsmasq
        for section in &mut cfgs["dhcp"].sections {
            if section.ty() == "dnsmasq"
                && !section
                    .name()
                    .map(|n| n.starts_with("dns_"))
                    .unwrap_or(false)
            {
                let arena = section.arena;
                section.lines.push(Line::List {
                    list: Token::from_string("notinterface".to_string(), arena),
                    item: Token::from_string(profile.id.interface.clone(), arena),
                    comment: uciedit::LineComment::None,
                });
                break;
            }
        }
    }

    Ok(())
}

#[instrument(skip_all)]
pub async fn get<C: CtrlContext>(ctx: C, query: ProfileIdOpt) -> Result<Profile, Error> {
    let arena = Arena::new();
    let cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt", "network", "firewall"]).await?;
    get_config(ctx, &cfgs, query)
}

pub(crate) fn get_config(
    ctx: impl CtrlContext,
    cfgs: &Configs,
    query: ProfileIdOpt,
) -> Result<Profile, Error> {
    let lookup = Lookup::parse(ctx.clone(), cfgs)?;
    let id = lookup.resolve(&query)?;
    let mut uciprofile = None;
    cfgs["startwrt"].each::<UciProfile, Error>(|_, profile| {
        if &profile.id() == id {
            uciprofile = Some(profile);
        }
    })?;
    let mut wip_profile = Profile {
        id: id.clone(),
        owns_lan: id.interface == "lan",
        gateway_ip: Ipv4Addr::new(0, 0, 0, 0),
        outbound: match &uciprofile {
            Some(p) => p.outbound.clone().unwrap_or_else(|| "wan".to_string()),
            None => "wan".to_string(),
        },
        lan_access: LanAccess::SameProfile,
        wan_access: WanAccess::None,
        dns_override: uciprofile
            .as_ref()
            .map(|p| dns::parse_dns_server_list(&p.dns_override))
            .unwrap_or_default(),
        dns_source: String::new(),
        access_to_new_profiles: match &uciprofile {
            Some(p) => p.access_to_new_profiles,
            None => false,
        },
    };
    let mut found = false;
    for section in &cfgs["network"].sections {
        if let Ok(iface) = section.get::<NetworkInterface>() {
            let Some(name) = section.name() else { continue };
            if name == id.interface {
                if iface.proto != InterfaceProto::STATIC {
                    return Err(Error::new(eyre!("corrupted profile: {query:?}"), ErrorKind::CorruptedProfile));
                }
                if let Some(ip) = iface.ipaddr {
                    wip_profile.gateway_ip = ip;
                } else {
                    return Err(Error::new(eyre!("corrupted profile: {query:?}"), ErrorKind::CorruptedProfile));
                }
                found = true;
                break;
            }
        }
    }
    if !found {
        return Err(Error::new(eyre!("corrupted profile: {query:?}"), ErrorKind::CorruptedProfile));
    }
    let mut this_zone_name = format!("vlan_{}", id.interface);
    cfgs["firewall"].each::<FirewallZone, Error>(|_, zone| {
        for zone_interface in zone.network {
            if id.interface == zone_interface {
                this_zone_name = zone.name.clone();
            }
        }
    })?;
    wip_profile.wan_access = match uciprofile.as_ref().and_then(|p| p.wan_access.as_deref()) {
        Some("whitelist") => WanAccess::Whitelist(
            uciprofile
                .as_ref()
                .map(|p| p.wan_access_list.clone())
                .unwrap_or_default(),
        ),
        Some("blacklist") => WanAccess::Blacklist(
            uciprofile
                .as_ref()
                .map(|p| p.wan_access_list.clone())
                .unwrap_or_default(),
        ),
        Some("none") => WanAccess::None,
        _ => WanAccess::All,
    };
    let mut forwarding = BTreeSet::new();
    cfgs["firewall"].each::<FirewallForwarding, Error>(|_, FirewallForwarding { src, dest }| {
        if src == this_zone_name && src != dest {
            forwarding.insert(dest);
        }
    })?;
    let mut other_profiles = BTreeSet::new();
    cfgs["firewall"].each::<FirewallZone, Error>(|_, FirewallZone { name, network, .. }| {
        if forwarding.contains(&name) {
            for other_interface in network {
                if let Some(other) = lookup.from_interface(&other_interface) {
                    other_profiles.insert(other.clone());
                }
            }
        }
    })?;
    if other_profiles.is_empty() {
        wip_profile.lan_access = LanAccess::SameProfile;
    } else if (other_profiles.len() + 1) >= lookup.list().len() {
        wip_profile.lan_access = LanAccess::All;
    } else {
        wip_profile.lan_access = LanAccess::OtherProfiles(other_profiles);
    }
    wip_profile.dns_source = compute_dns_source(cfgs, &wip_profile);
    Ok(wip_profile)
}

#[instrument(skip_all)]
pub async fn delete<C: CtrlContext>(ctx: C, id: ProfileIdOpt) -> Result<(), Error> {
    let name = id.fullname.clone().unwrap_or_default();

    // Resolve the profile interface name before the retry loop so we can
    // use it for ifdown after dump_all succeeds (delete_config consumes the query).
    let interface_name = {
        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt"]).await?;
        let lookup = Lookup::parse(ctx.clone(), &cfgs)?;
        lookup.resolve(&id)?.interface.clone()
    };
    let wg_interface_name = format!("wg_{}", interface_name);

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp", "wireless"],
        ).await?;
        if let Err(e) = delete_config(ctx.clone(), &mut cfgs, &id) {
            crate::activity::log("profile", "deleted", false, &format!("Failed to delete profile '{name}'"), Some(&e.to_string()));
            return Err(e);
        }
        let dump_result = dump_all(ctx.uci_root(), cfgs).await;
        drop(arena);
        match dump_result {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log("profile", "deleted", false, &format!("Failed to delete profile '{name}'"), Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                if ctx.effectful() {
                    // Regenerate SmartDNS config after UCI dump so the deleted
                    // profile's group is removed.
                    let smartdns_groups = {
                        let arena2 = Arena::new();
                        let cfgs2 = parse_all(ctx.uci_root(), &arena2, &["startwrt"]).await?;
                        dns::collect_smartdns_groups(&cfgs2)
                    };
                    dns::apply_smartdns_groups(smartdns_groups).await?;

                    // Bring down the WireGuard interface before reloading services.
                    // reload_system() alone won't tear down an active WireGuard tunnel.
                    let _ = crate::run_quiet_async(tokio::process::Command::new("ifdown").arg(&wg_interface_name)).await;
                    reload_system_and_wifi().await?;
                }
                // Regenerate schedule crontab to remove deleted profile's entries
                if let Err(e) = regenerate_schedule_crontab(&ctx).await {
                    tracing::error!("Failed to regenerate schedule crontab after profile delete: {e}");
                }
                if ctx.effectful() {
                    let _ = crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/cron").arg("restart")).await;
                }
                crate::activity::log("profile", "deleted", true, &format!("Deleted profile '{name}'"), None);
                return Ok(());
            }
        }
    }
}

fn delete_config(
    ctx: impl CtrlContext,
    cfgs: &mut Configs,
    query: &ProfileIdOpt,
) -> Result<(), Error> {
    let lookup = Lookup::parse(ctx, cfgs)?;
    let id = lookup.resolve(query)?.clone();

    // Prevent deleting the LAN owner profile
    if id.interface == "lan" {
        return Err(Error::new(eyre!("cannot delete LAN owner profile"), ErrorKind::CannotDeleteLanOwner));
    }

    // Remove profile and its VPN server metadata from startwrt config
    let wg_interface_name = format!("wg_{}", id.interface);
    cfgs["startwrt"].sections.retain(|section| {
        if let Ok(profile) = section.get::<UciProfile>() {
            if profile.id() == id {
                return false;
            }
        }
        // Remove VPN server metadata for this profile's WireGuard interface
        if section.ty() == "vpn_server" {
            if section.lines.iter().any(|line| {
                matches!(line, uciedit::Line::Option { option, value, .. }
                    if option.as_str() == "interface" && value.as_str() == wg_interface_name.as_str())
            }) {
                return false;
            }
        }
        true
    });

    // Remove network interface, bridge-vlan, and WireGuard interface + peers
    let wg_peer_type = format!("wireguard_{}", wg_interface_name);
    cfgs["network"].sections.retain(|section| {
        if section.get::<NetworkInterface>().is_ok() {
            if section.name().as_deref() == Some(&id.interface) {
                return false;
            }
        }
        // Remove WireGuard interface for this profile (checked separately because
        // WireGuard interfaces lack a `device` field so NetworkInterface parsing fails)
        if section.ty() == "interface"
            && section.name().as_deref() == Some(wg_interface_name.as_str())
        {
            return false;
        }
        if let Ok(vlan) = section.get::<NetworkBridgeVlan>() {
            if vlan.vlan == id.vlan_tag {
                return false;
            }
        }
        // Remove WireGuard peer sections for this profile's VPN server
        if section.ty() == wg_peer_type {
            return false;
        }
        true
    });

    // Find the firewall zone name for this profile's interface
    let mut zone_name = None;
    for section in &cfgs["firewall"].sections {
        let Ok(zone) = section.get::<FirewallZone>() else {
            continue;
        };
        if zone.network.iter().any(|n| n == &id.interface) {
            zone_name = Some(zone.name.clone());
            break;
        }
    }

    // Remove zone, its DHCP/DNS rule, and all forwarding rules from/to this zone
    if let Some(zone_name) = &zone_name {
        cfgs["firewall"].sections.retain(|section| {
            if let Ok(zone) = section.get::<FirewallZone>() {
                if &zone.name == zone_name {
                    return false;
                }
            }
            if let Ok(rule) = section.get::<FirewallRule>() {
                if &rule.src == zone_name {
                    return false;
                }
            }
            if let Ok(fwd) = section.get::<FirewallForwarding>() {
                if &fwd.src == zone_name || &fwd.dest == zone_name {
                    return false;
                }
            }
            if let Ok(redir) = section.get::<FirewallRedirect>() {
                if &redir.src == zone_name {
                    return false;
                }
            }
            true
        });
    }

    // Remove DHCP server for this interface
    cfgs["dhcp"].sections.retain(|section| {
        let Ok(dhcp) = section.get::<Dhcp>() else {
            return true;
        };
        dhcp.interface != id.interface
    });

    // Remove per-profile dnsmasq section
    let dns_section_name = format!("dns_{}", id.interface);
    cfgs["dhcp"]
        .sections
        .retain(|s| s.name().as_deref() != Some(dns_section_name.as_str()));

    // Remove notinterface entry from main dnsmasq
    for section in &mut cfgs["dhcp"].sections {
        if section.ty() == "dnsmasq"
            && !section
                .name()
                .map(|n| n.starts_with("dns_"))
                .unwrap_or(false)
        {
            section.lines.retain(|line| {
                !matches!(line, Line::List { list, item, .. }
                    if list.as_str() == "notinterface"
                        && item.as_str() == id.interface.as_str())
            });
            break;
        }
    }

    // Remove WiFi stations and VLAN sections referencing this profile's VLAN tag
    cfgs["wireless"].sections.retain(|section| {
        if let Ok(station) = section.get::<WifiStation>() {
            if station.vid == Some(id.vlan_tag) {
                return false;
            }
        }
        if let Ok(vlan) = section.get::<WifiVlan>() {
            if vlan.vid == id.vlan_tag {
                return false;
            }
        }
        true
    });

    // Remove policy routing entries for this profile
    let route_name = format!("prt_{}", id.interface);
    let local_route_name = format!("plr_{}", id.interface);
    let rule_name = format!("prr_{}", id.interface);
    cfgs["network"].sections.retain(|s| {
        let n = s.name();
        n.as_deref() != Some(route_name.as_str())
            && n.as_deref() != Some(local_route_name.as_str())
            && n.as_deref() != Some(rule_name.as_str())
    });

    // Remove WireGuard WAN firewall rule (src: "wan", not caught by zone cleanup above)
    crate::vpn_server::remove_wireguard_firewall_rule(cfgs, &wg_interface_name)?;

    // Rebuild cross-subnet routes (the deleted profile's subnet is removed,
    // and remaining VPN profiles no longer need a route to it)
    sync_cross_subnet_routes(cfgs)?;
    crate::vpn_server::sync_vpn_peer_cross_routes(cfgs)?;

    // Clean up orphaned VPN interfaces from WAN zone
    cleanup_orphaned_vpn_zones(cfgs);

    Ok(())
}

#[instrument(skip_all)]
pub async fn list<C: CtrlContext>(ctx: C) -> Result<Vec<ProfileId>, Error> {
    let arena = Arena::new();
    let cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt"]).await?;
    list_config(ctx, &cfgs)
}

pub(crate) fn list_config(_ctx: impl CtrlContext, cfgs: &Configs) -> Result<Vec<ProfileId>, Error> {
    let mut found = Vec::new();
    for section in &cfgs["startwrt"].sections {
        let Ok(UciProfile {
            fullname,
            interface,
            vlan_tag,
            ..
        }) = section.get()
        else {
            continue;
        };
        found.push(ProfileId {
            fullname,
            interface,
            vlan_tag,
        })
    }
    Ok(found)
}

pub async fn reload_system() -> Result<(), Error> {
    reload_system_inner(false).await
}

/// Like [`reload_system`] but does a full `network restart` instead of `reload`.
/// Profile create/edit may add or change an interface's `ip6assign`; netifd only
/// recomputes its global IPv6 prefix distribution on a process restart, so a plain
/// `reload` leaves a newly v6-eligible profile without its delegated /64 (and
/// odhcpd with no prefix to advertise) until the next full restart.
pub async fn reload_system_full() -> Result<(), Error> {
    reload_system_inner(true).await
}

async fn reload_system_inner(restart_network: bool) -> Result<(), Error> {
    let network_action = if restart_network { "restart" } else { "reload" };
    let _ = crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/network").arg(network_action)).await;
    // odhcpd holds RA/DHCPv6 config in memory; restart so dhcp.*.ra_default
    // (and other v6 advertisement changes) actually take effect.
    let _ = crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/odhcpd").arg("restart")).await;
    let _ = crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/smartdns").arg("restart")).await;
    let _ = crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/firewall").arg("restart")).await;
    let _ = crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/dnsmasq").arg("restart")).await;
    // Re-apply WAN schedules — firewall restart rebuilds the nftables ruleset
    reapply_schedules_after_reload().await;
    Ok(())
}

/// Combined reload for operations that also need a WiFi restart (profile create/delete).
/// Runs everything sequentially in one thread to avoid race conditions — the `wifi` command
/// tears down and recreates wireless interfaces, which destabilizes the network if the
/// firewall reloads concurrently.
pub async fn reload_system_and_wifi() -> Result<(), Error> {
    reload_system_and_wifi_inner(false).await
}

/// Full-restart variant of [`reload_system_and_wifi`] — see [`reload_system_full`]
/// for why profile create needs `network restart` rather than `reload`.
pub async fn reload_system_and_wifi_full() -> Result<(), Error> {
    reload_system_and_wifi_inner(true).await
}

async fn reload_system_and_wifi_inner(restart_network: bool) -> Result<(), Error> {
    let network_action = if restart_network { "restart" } else { "reload" };
    let _ = crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/network").arg(network_action)).await;
    let _ = crate::run_quiet_async(&mut tokio::process::Command::new("wifi")).await;
    // odhcpd holds RA/DHCPv6 config in memory; restart so dhcp.*.ra_default
    // (and other v6 advertisement changes) actually take effect.
    let _ = crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/odhcpd").arg("restart")).await;
    let _ = crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/smartdns").arg("restart")).await;
    let _ = crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/firewall").arg("restart")).await;
    let _ = crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/dnsmasq").arg("restart")).await;
    // Re-apply WAN schedules — firewall restart rebuilds the nftables ruleset
    reapply_schedules_after_reload().await;
    Ok(())
}

/// Helper for reload functions (which have no CtrlContext) to re-apply WAN schedules
/// after a firewall restart. The schedule rules are already in UCI, so this just
/// ensures the current time-based state is correct.
async fn reapply_schedules_after_reload() {
    #[derive(Clone)]
    struct ReloadCtx;
    impl rpc_toolkit::Context for ReloadCtx {}
    impl CtrlContext for ReloadCtx {
        fn uci_root(&self) -> PathBuf {
            PathBuf::from("/etc/config/")
        }
        fn effectful(&self) -> bool {
            true
        }
    }
    if let Err(e) = evaluate_and_apply_schedules(&ReloadCtx).await {
        tracing::error!("Failed to re-apply WAN schedules after firewall reload: {e}");
    }
}

/// When the first bridge-vlan is about to be created on a bridge, we must also create
/// a VLAN 1 entry with all existing bridge ports. Once VLAN filtering is active, any
/// port without an explicit VLAN entry loses all traffic.
fn ensure_vlan_filtering(cfgs: &mut Configs, bridge_name: &str) -> Result<(), Error> {
    let has_any = cfgs["network"]
        .sections
        .iter()
        .any(|s| {
            s.get::<NetworkBridgeVlan>()
                .map(|v| v.device == bridge_name)
                .unwrap_or(false)
        });
    if has_any {
        return Ok(());
    }

    // Get bridge ports
    let bridge_ports: Vec<String> = cfgs["network"]
        .sections
        .iter()
        .filter_map(|s| {
            s.get::<NetworkDevice>()
                .ok()
                .filter(|d| d.ty == Some(DeviceType::BRIDGE) && d.name == bridge_name)
                .map(|d| d.ports)
        })
        .next()
        .unwrap_or_default();

    // Create VLAN 1 with all bridge ports as PRIMARY (untagged + PVID)
    if !bridge_ports.is_empty() {
        cfgs["network"].append(
            &NetworkBridgeVlan {
                device: bridge_name.to_string(),
                vlan: 1,
                ports: bridge_ports
                    .into_iter()
                    .map(|port| NetworkVlanPort {
                        port,
                        tagging: Some(NetworkVlanPortTagging::PRIMARY),
                    })
                    .collect(),
            },
            None,
        )?;
    }

    // Update LAN interface from br-lan to br-lan.1
    for section in &mut cfgs["network"].sections {
        if let Ok(mut iface) = section.get::<NetworkInterface>() {
            if section.name().as_deref() == Some("lan") && iface.device == bridge_name {
                iface.device = format!("{bridge_name}.1");
                section.set(&iface)?;
            }
        }
    }
    Ok(())
}

#[instrument(skip_all)]
pub async fn set<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<ProfileSetRequest>,
) -> Result<ProfileId, Error> {
    let profile = req.profile;
    let force = req.force;
    let name = profile.id.fullname.clone().unwrap_or_default();
    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        ).await?;

        // Capture old LAN IP before set_config modifies it
        let old_lan_ip = if profile.owns_lan {
            get_lan_ip(&cfgs)
        } else {
            None
        };

        let (out, old_state) = match set_config(ctx.clone(), &mut cfgs, &profile) {
            Ok(result) => result,
            Err(e) => {
                crate::activity::log("profile", "updated", false, &format!("Failed to update profile '{name}'"), Some(&e.to_string()));
                return Err(e);
            }
        };

        // Detect admin IP change and propagate block changes to sibling profiles
        let admin_ip_changed = profile.owns_lan
            && old_lan_ip.is_some()
            && old_lan_ip != Some(profile.gateway_ip);
        let block_changed = match (admin_ip_changed, old_lan_ip) {
            (true, Some(old)) => {
                old.octets()[0] != profile.gateway_ip.octets()[0]
                    || old.octets()[1] != profile.gateway_ip.octets()[1]
            }
            _ => false,
        };
        let restart_ifaces: Vec<String> = if block_changed {
            let pi: std::collections::HashSet<String> =
                list_config(ctx.clone(), &cfgs)?
                    .into_iter()
                    .filter(|p| p.interface != crate::lan::LAN_INTERFACE)
                    .map(|p| p.interface)
                    .collect();
            crate::lan::update_profile_ips_for_block_change(
                &mut cfgs,
                old_lan_ip.unwrap(),
                profile.gateway_ip,
                &pi,
            )?;
            let mut v = vec![crate::lan::LAN_INTERFACE.to_string()];
            v.extend(pi);
            v
        } else {
            vec![crate::lan::LAN_INTERFACE.to_string()]
        };

        // Guard: reject subnet change when DHCP static hosts exist in the old subnet
        if let Some(old_gw) = old_state.as_ref().and_then(|s| s.gateway_ip) {
            if old_gw != profile.gateway_ip {
                let o = old_gw.octets();
                if block_changed {
                    crate::lan::guard_dhcp_static_hosts(&cfgs, &[o[0], o[1]])?;
                } else {
                    crate::lan::guard_dhcp_static_hosts(&cfgs, &[o[0], o[1], o[2]])?;
                }
            }
        }

        // Guard: check if IP change would break VPN peers
        let ip_changed = old_state
            .as_ref()
            .and_then(|s| s.gateway_ip)
            .map(|old| old != profile.gateway_ip)
            .unwrap_or(false);
        let removed_vpns = if ip_changed || block_changed {
            let affected: Vec<&str> = if block_changed {
                restart_ifaces.iter().map(|s| s.as_str()).collect()
            } else if let Some(ref iface) = profile.id.interface {
                vec![iface.as_str()]
            } else {
                vec![]
            };
            crate::vpn_server::guard_vpn_peers(&mut cfgs, &affected, force)?
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
                crate::activity::log("profile", "updated", false, &format!("Failed to update profile '{name}'"), Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                if ctx.effectful() {
                    // Regenerate SmartDNS config after UCI dump so it reflects
                    // any dns_override changes on this profile.
                    let smartdns_groups = {
                        let arena2 = Arena::new();
                        let cfgs2 = parse_all(ctx.uci_root(), &arena2, &["startwrt"]).await?;
                        dns::collect_smartdns_groups(&cfgs2)
                    };
                    dns::apply_smartdns_groups(smartdns_groups).await?;

                    if admin_ip_changed {
                        crate::lan::restart_network_services(profile.gateway_ip, restart_ifaces).await;
                    } else {
                        // Full network restart (not reload) so an outbound change
                        // that flips IPv6 eligibility re-runs netifd prefix
                        // distribution (adds/removes the profile's /64).
                        reload_system_full().await?;
                    }
                    removed_vpns.apply_post_reload().await;
                }
                // Keep the schedule crontab in sync: its window-start commands
                // embed the egress zone ("wan" vs "vpn_<X>"), which depends on
                // this profile's outbound — so an outbound change must rewrite
                // them, or the next blackout boundary would REJECT toward the
                // stale zone.
                if let Err(e) = regenerate_schedule_crontab(&ctx).await {
                    tracing::error!(
                        "Failed to regenerate schedule crontab after profile update: {e}"
                    );
                }
                if ctx.effectful() {
                    let _ = crate::run_quiet_async(
                        tokio::process::Command::new("/etc/init.d/cron").arg("restart"),
                    )
                    .await;
                }
                let mut changes = Vec::new();
                if let Some(ref old) = old_state {
                    if old.outbound.as_deref() != Some(&profile.outbound) {
                        changes.push(format!("outbound: {}", profile.outbound));
                    }
                    let new_wan = wan_access_type_str(&profile.wan_access);
                    if old.wan_access.as_deref() != Some(&new_wan) {
                        changes.push(format!("WAN access: {}", new_wan));
                    }
                    if old.access_to_new_profiles != profile.access_to_new_profiles {
                        changes.push(format!(
                            "access to new profiles: {}",
                            profile.access_to_new_profiles
                        ));
                    }
                    if old.gateway_ip.is_some() && old.gateway_ip != Some(profile.gateway_ip) {
                        changes.push(format!("gateway: {}", profile.gateway_ip));
                    }
                    if old.dns_override
                        != dns::serialize_dns_server_list(&profile.dns_override)
                    {
                        changes.push("DNS".to_string());
                    }
                }
                let summary = if changes.is_empty() {
                    format!("Updated profile '{name}'")
                } else {
                    format!("Updated profile '{name}' — {}", changes.join(", "))
                };
                crate::activity::log("profile", "updated", true, &summary, None);
                return Ok(out);
            }
        }
    }
}

/// Read the current LAN interface IP from parsed configs.
fn get_lan_ip(cfgs: &uciedit::Configs) -> Option<Ipv4Addr> {
    for section in &cfgs["network"].sections {
        if section.name().as_deref() == Some(crate::lan::LAN_INTERFACE) {
            if let Ok(Some(iface)) = section.get_typed::<NetworkInterface>() {
                return iface.ipaddr;
            }
        }
    }
    None
}

/// Validate a profile's gateway IP stays within the allowed network block.
///
/// The LAN-owning (admin) profile defines the block, so it must itself be a
/// valid RFC 1918 selection — the same rule `lan::ipv4_set` enforces (this is a
/// second path that sets the LAN /16). Every other profile must live inside the
/// admin LAN's /16 (matching first two octets) so VLAN subnets can't escape the
/// chosen private range — `sync_cross_subnet_routes` assumes siblings share the
/// block, and an out-of-block subnet would also dodge the LAN-level boundary.
fn validate_profile_block(
    cfgs: &Configs,
    profile: &Profile<ProfileIdOpt>,
) -> Result<(), Error> {
    if profile.owns_lan {
        return crate::lan::validate_lan_block(profile.gateway_ip);
    }
    if let Some(lan) = get_lan_ip(cfgs) {
        let g = profile.gateway_ip.octets();
        let l = lan.octets();
        if g[0] != l[0] || g[1] != l[1] {
            return Err(Error::new(
                eyre!(
                    "profile subnet {} is outside the LAN network block {}.{}.0.0/16",
                    profile.gateway_ip,
                    l[0],
                    l[1]
                ),
                ErrorKind::InvalidRequest,
            ));
        }
    }
    Ok(())
}

/// Reject a profile gateway whose /24 collides with another profile's subnet.
///
/// Every profile — including the admin LAN (`owns_lan`) — must occupy a distinct
/// /24 within the shared /16. Two interfaces on the same /24 produce overlapping
/// connected routes and a duplicate gateway IP, which silently breaks L3
/// reachability to the router. The web UI blocks this per-field
/// (`duplicateSubnet`), but a direct RPC/CLI call bypasses that, so enforce it
/// on the backend too. `self_interface` is the interface being changed; its own
/// subnet is skipped so a no-op edit passes.
pub(crate) fn guard_subnet_collision(
    cfgs: &Configs,
    gateway_ip: Ipv4Addr,
    self_interface: Option<&str>,
) -> Result<(), Error> {
    let new = gateway_ip.octets();
    // Read the current /24 of the interface being changed. Only enforce on an
    // actual subnet change — a no-op edit (e.g. changing DNS) must not be blocked
    // by a collision that already exists in the stored config.
    let current_ip = self_interface.and_then(|iface| {
        cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some(iface))
            .and_then(|s| s.get_typed::<NetworkInterface>().ok().flatten())
            .and_then(|i| i.ipaddr)
    });
    if let Some(cur) = current_ip {
        let c = cur.octets();
        if c[0] == new[0] && c[1] == new[1] && c[2] == new[2] {
            return Ok(());
        }
    }
    for section in &cfgs["startwrt"].sections {
        let Ok(existing) = section.get::<UciProfile>() else {
            continue;
        };
        if Some(existing.interface.as_str()) == self_interface {
            continue;
        }
        let existing_ip = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some(existing.interface.as_str()))
            .and_then(|s| s.get_typed::<NetworkInterface>().ok().flatten())
            .and_then(|i| i.ipaddr);
        if let Some(ip) = existing_ip {
            let o = ip.octets();
            if o[0] == new[0] && o[1] == new[1] && o[2] == new[2] {
                return Err(Error::new(
                    eyre!(
                        "subnet {}.{}.{}.0/24 is already in use by profile '{}'",
                        new[0],
                        new[1],
                        new[2],
                        existing.fullname
                    ),
                    ErrorKind::SubnetCollision,
                ));
            }
        }
    }
    Ok(())
}

fn set_config<C: CtrlContext>(
    ctx: C,
    cfgs: &mut Configs,
    profile: &Profile<ProfileIdOpt>,
) -> Result<(ProfileId, Option<OldProfileState>), Error> {
    validate_profile_block(cfgs, profile)?;
    let ipv6 = is_ipv6_enabled(cfgs)
        && outbound_supports_ipv6(cfgs, &profile.outbound);
    // Check fullname uniqueness before renaming
    if let Some(given_fullname) = &profile.id.fullname {
        let pre_lookup = Lookup::parse(ctx.clone(), cfgs)?;
        if let Some(existing) = pre_lookup.from_fullname(given_fullname) {
            // Only error if the name belongs to a different profile
            if !profile.id.matches(&existing.clone().into()) {
                return Err(Error::new(eyre!("duplicate profile name: {given_fullname}"), ErrorKind::DuplicateFullname));
            }
        }
    }
    let mut old_state: Option<OldProfileState> = None;
    for section in &mut cfgs["startwrt"].sections {
        if let Some(mut existing_profile) = section.get_typed::<UciProfile>()? {
            if let Some(given_fullname) = &profile.id.fullname {
                if &existing_profile.fullname != given_fullname
                    && Some(&existing_profile.vlan_tag) == profile.id.vlan_tag.as_ref()
                    && Some(&existing_profile.interface) == profile.id.interface.as_ref()
                {
                    // rename
                    existing_profile.fullname = given_fullname.clone();
                }
            }
            if profile.id.matches(&existing_profile.id().into()) {
                old_state = Some(OldProfileState {
                    outbound: existing_profile.outbound.clone(),
                    wan_access: existing_profile.wan_access.clone(),
                    access_to_new_profiles: existing_profile.access_to_new_profiles,
                    dns_override: existing_profile.dns_override.clone(),
                    gateway_ip: None, // filled from network config below
                });
                existing_profile.access_to_new_profiles = profile.access_to_new_profiles;
                existing_profile.outbound = Some(profile.outbound.clone());
                existing_profile.wan_access = Some(wan_access_type_str(&profile.wan_access));
                existing_profile.wan_access_list = wan_access_destinations(&profile.wan_access);
                existing_profile.dns_override =
                    dns::serialize_dns_server_list(&profile.dns_override);
                section.set(&existing_profile)?;
            }
        }
    }
    // only do lookup after potentially handling renames
    let lookup = Lookup::parse(ctx.clone(), &cfgs)?;
    let profile = Profile {
        id: lookup.resolve(&profile.id)?.clone(),
        gateway_ip: profile.gateway_ip,
        outbound: profile.outbound.clone(),
        lan_access: match &profile.lan_access {
            LanAccess::All => LanAccess::All,
            LanAccess::SameProfile => LanAccess::SameProfile,
            LanAccess::OtherProfiles(set) => LanAccess::OtherProfiles(
                set.into_iter()
                    .map(|p| lookup.resolve(&p).cloned())
                    .collect::<Result<_, _>>()?,
            ),
        },
        wan_access: profile.wan_access.clone(),
        dns_override: profile.dns_override.clone(),
        dns_source: String::new(),
        access_to_new_profiles: profile.access_to_new_profiles,
        owns_lan: profile.owns_lan,
    };
    // Reject a /24 that collides with a sibling (or the admin LAN). Runs on the
    // resolved interface, before the network config is mutated below, so the
    // profile's own current subnet is correctly skipped.
    guard_subnet_collision(cfgs, profile.gateway_ip, Some(&profile.id.interface))?;
    let found_bridge = find_lan_bridge(cfgs)?.map(|dev| dev.name);
    let mut all_interfaces = BTreeSet::<String>::new();
    let mut found_interface = false;
    for section in &mut cfgs["network"].sections {
        if let Some(mut iface) = section.get_typed::<NetworkInterface>()? {
            let Some(name) = section.name() else { continue };
            if iface.proto == InterfaceProto::STATIC {
                all_interfaces.insert(name.to_string());
            }
            if name == profile.id.interface {
                if let Some(ref mut old) = old_state {
                    old.gateway_ip = iface.ipaddr;
                }
                iface.proto = InterfaceProto::STATIC;
                iface.ipaddr = Some(profile.gateway_ip);
                iface.netmask = Some(Ipv4Addr::new(255, 255, 255, 0));
                // The admin LAN's prefix delegation is owned by lan::ipv6_set
                // (the LAN IPv6 page) and uses the user-configured prefix — don't
                // reset it here. Non-admin VLANs get a /64, gated on whether their
                // outbound carries v6. (A VPN-routed admin still fails v6 closed
                // via the kill-switch route in rewrite_routing.)
                if !profile.owns_lan {
                    iface.ip6assign = if ipv6 { Some("64".into()) } else { None };
                }
                section.set(&iface)?;
                found_interface = true;
            }
        }
    }
    if !found_interface {
        let found_bridge = found_bridge.clone().ok_or_else(|| Error::new(eyre!("missing LAN bridge"), ErrorKind::MissingLanBridge))?;
        cfgs["network"].append(
            &NetworkInterface {
                device: format!("{}.{}", found_bridge, profile.id.vlan_tag),
                proto: InterfaceProto::STATIC,
                ipaddr: Some(profile.gateway_ip),
                netmask: Some(Ipv4Addr::new(255, 255, 255, 0)),
                ip6assign: if ipv6 { Some("64".into()) } else { None },
                ..Default::default()
            },
            Some(&profile.id.interface),
        )?;
    }
    // Ensure a portless bridge-vlan exists for this profile's VLAN
    if !profile.owns_lan {
        let bridge_name = found_bridge.as_ref().ok_or_else(|| Error::new(eyre!("missing LAN bridge"), ErrorKind::MissingLanBridge))?;
        // Transition to VLAN filtering: ensure VLAN 1 exists for existing bridge ports
        ensure_vlan_filtering(cfgs, bridge_name)?;
        let has_bridge_vlan = cfgs["network"]
            .sections
            .iter()
            .any(|s| {
                s.get::<NetworkBridgeVlan>()
                    .map(|v| v.vlan == profile.id.vlan_tag && v.device == *bridge_name)
                    .unwrap_or(false)
            });
        if !has_bridge_vlan {
            cfgs["network"].append(
                &NetworkBridgeVlan {
                    device: bridge_name.clone(),
                    vlan: profile.id.vlan_tag,
                    ports: Vec::new(),
                },
                None,
            )?;
        }
    }
    rewrite_firewall(&ctx, cfgs, &profile, &all_interfaces, &[], false)?;
    rewrite_dhcp(&ctx, cfgs, &profile)?;
    rewrite_dns_forwarding(cfgs, &profile)?;
    rewrite_routing(&ctx, cfgs, &profile)?;
    sync_cross_subnet_routes(cfgs)?;
    crate::vpn_server::sync_vpn_peer_cross_routes(cfgs)?;
    cleanup_orphaned_vpn_zones(cfgs);
    Ok((profile.id, old_state))
}

/// Re-apply a profile's full firewall/dhcp/dns/routing config from its current
/// UCI state. Used after a profile's outbound is reset to "wan" (because its VPN
/// was deleted or disabled) so the dedicated-vpn-zone forwarding (`<zone> →
/// vpn_<wg>`) is rebuilt as `<zone> → wan`. Mirrors the rewrite sequence at the
/// tail of `set_config`; the VPN delete/disable paths previously ran only
/// `rewrite_routing` + `rewrite_dns_forwarding`, leaving the profile with no
/// `→ wan` forwarding (fw4 then dropped all of its WAN traffic).
pub(crate) fn reapply_profile_config<C: CtrlContext>(
    ctx: &C,
    cfgs: &mut Configs,
    query: ProfileIdOpt,
) -> Result<(), Error> {
    let profile = get_config(ctx.clone(), cfgs, query)?;
    let all_interfaces: BTreeSet<String> = cfgs["network"]
        .sections
        .iter()
        .filter_map(|s| {
            let ni = s.get::<NetworkInterface>().ok()?;
            if ni.proto == InterfaceProto::STATIC {
                Some(s.name()?.to_string())
            } else {
                None
            }
        })
        .collect();
    rewrite_firewall(ctx, cfgs, &profile, &all_interfaces, &[], false)?;
    rewrite_dhcp(ctx, cfgs, &profile)?;
    rewrite_dns_forwarding(cfgs, &profile)?;
    rewrite_routing(ctx, cfgs, &profile)?;
    Ok(())
}

#[instrument(skip_all)]
pub async fn create<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(profile): DeserializeStdin<Profile<ProfileIdOpt>>,
) -> Result<ProfileId, Error> {
    let name = profile.id.fullname.clone().unwrap_or_default();
    // Pre-allocate the interface id before the retry loop to avoid holding the
    // !Send Arena across an .await point. Gather the ids already in use (in their
    // own arena scope, dropped before the loop) so the random allocation avoids
    // them instead of hard-failing in create_config — every id is random, so a
    // freed-then-reused profile name, or two names that sanitize to the same
    // prefix, each get a distinct id.
    let pre_allocated_interface = if !profile.owns_lan {
        let taken = {
            let arena = Arena::new();
            let cfgs = parse_all(ctx.uci_root(), &arena, &["network", "startwrt"]).await?;
            reserved_interface_names(&cfgs)
        };
        Some(allocate_interface_name(&ctx, &taken).await?)
    } else {
        None
    };
    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp", "wireless"],
        ).await?;
        let out = match create_config(ctx.clone(), &mut cfgs, &profile, pre_allocated_interface.clone()) {
            Ok(out) => out,
            Err(e) => {
                crate::activity::log("profile", "created", false, &format!("Failed to create profile '{name}'"), Some(&e.to_string()));
                return Err(e);
            }
        };
        let dump_result = dump_all(ctx.uci_root(), cfgs).await;
        drop(arena);
        match dump_result {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log("profile", "created", false, &format!("Failed to create profile '{name}'"), Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                if ctx.effectful() {
                    // Regenerate SmartDNS config after UCI dump so it reflects
                    // any dns_override on the new profile.
                    let smartdns_groups = {
                        let arena2 = Arena::new();
                        let cfgs2 = parse_all(ctx.uci_root(), &arena2, &["startwrt"]).await?;
                        dns::collect_smartdns_groups(&cfgs2)
                    };
                    dns::apply_smartdns_groups(smartdns_groups).await?;

                    // Full network restart (not reload) so netifd re-runs IPv6
                    // prefix distribution and the new profile's VLAN gets its /64.
                    reload_system_and_wifi_full().await?;
                }
                crate::activity::log("profile", "created", true, &format!("Created profile '{name}'"), None);
                return Ok(out);
            }
        }
    }
}

fn create_config(
    ctx: impl CtrlContext,
    cfgs: &mut Configs,
    profile: &Profile<ProfileIdOpt>,
    pre_allocated_interface: Option<String>,
) -> Result<ProfileId, Error> {
    validate_profile_block(cfgs, profile)?;
    // The new interface isn't in the config yet, so no self-exclusion is needed:
    // reject if the requested /24 already belongs to any existing profile.
    guard_subnet_collision(cfgs, profile.gateway_ip, None)?;
    let ipv6 = is_ipv6_enabled(cfgs)
        && outbound_supports_ipv6(cfgs, &profile.outbound);
    let interface = if profile.owns_lan {
        if Lookup::parse(ctx.clone(), cfgs)?.lan_owner.is_some() {
            return Err(Error::new(eyre!("LAN owner already exists"), ErrorKind::LanOwnerExists));
        }
        "lan".into()
    } else {
        pre_allocated_interface.ok_or_else(|| {
            Error::new(eyre!("missing pre-allocated interface name"), ErrorKind::Filesystem)
        })?
    };
    let fullname = profile
        .id
        .fullname
        .clone()
        .unwrap_or_else(|| "Untitled".into());
    // Check fullname uniqueness
    {
        let lookup = Lookup::parse(ctx.clone(), cfgs)?;
        if lookup.from_fullname(&fullname).is_some() {
            return Err(Error::new(eyre!("duplicate profile name: {fullname}"), ErrorKind::DuplicateFullname));
        }
    }
    let mut all_interfaces = BTreeSet::<String>::new();
    let mut existing_tags = BTreeSet::new();
    for section in &cfgs["network"].sections {
        match &*section.ty() {
            NetworkInterface::TY => {
                let Some(name) = section.name() else { continue };
                if name == interface && !profile.owns_lan {
                    return Err(Error::new(eyre!("interface name conflict: {interface}"), ErrorKind::InterfaceNameConflict));
                }
                if let Ok(iface) = section.get::<NetworkInterface>() {
                    if iface.proto == InterfaceProto::STATIC {
                        all_interfaces.insert(name.to_string());
                    }
                }
            }
            NetworkBridgeVlan::TY => {
                if let Ok(vlan) = section.get::<NetworkBridgeVlan>() {
                    existing_tags.insert(vlan.vlan);
                }
            }
            _ => (),
        }
    }
    let found_bridge = find_lan_bridge(cfgs)?
        .map(|dev| dev.name)
        .ok_or_else(|| Error::new(eyre!("missing LAN bridge"), ErrorKind::MissingLanBridge))?;
    let vlan_tag = match profile.id.vlan_tag {
        Some(chosen_tag) => {
            if existing_tags.contains(&chosen_tag) {
                return Err(Error::new(eyre!("duplicate VLAN tag: {chosen_tag}"), ErrorKind::DuplicateVlanTag));
            }
            chosen_tag
        }
        None => (101..2_u16.pow(12))
            .find(|t| !existing_tags.contains(t))
            .expect("we somehow exausted all vlan tags, that should be impossible"),
    };
    if profile.owns_lan {
        let mut found_lan_interface = false;
        for section in &mut cfgs["network"].sections {
            if let Ok(mut iface) = section.get::<NetworkInterface>() {
                if section.name().as_deref() != Some("lan") {
                    continue;
                }
                found_lan_interface = true;
                iface.device = format!("{found_bridge}.{vlan_tag}");
                section.set(&iface)?;
            }
        }
        if !found_lan_interface {
            return Err(Error::new(eyre!("missing LAN interface"), ErrorKind::MissingLanInterface));
        }
        // Create bridge-vlan with all bridge ports so ethernet devices reach the LAN owner
        let bridge_ports: Vec<String> = cfgs["network"]
            .sections
            .iter()
            .filter_map(|s| {
                s.get::<NetworkDevice>()
                    .ok()
                    .filter(|d| d.ty == Some(DeviceType::BRIDGE) && d.name == found_bridge)
                    .map(|d| d.ports)
            })
            .next()
            .unwrap_or_default();
        cfgs["network"].append(
            &NetworkBridgeVlan {
                device: found_bridge.clone(),
                vlan: vlan_tag,
                ports: bridge_ports
                    .into_iter()
                    .map(|port| NetworkVlanPort {
                        port,
                        tagging: Some(NetworkVlanPortTagging::PRIMARY),
                    })
                    .collect(),
            },
            None,
        )?;
    } else {
        cfgs["network"].append(
            &NetworkInterface {
                device: format!("{found_bridge}.{vlan_tag}"),
                proto: InterfaceProto::STATIC,
                ipaddr: Some(profile.gateway_ip),
                netmask: Some(Ipv4Addr::new(255, 255, 255, 0)),
                ip6assign: if ipv6 { Some("64".into()) } else { None },
                ..Default::default()
            },
            Some(&interface),
        )?;
        // Transition to VLAN filtering: ensure VLAN 1 exists for existing bridge ports
        ensure_vlan_filtering(cfgs, &found_bridge)?;
        // Create a portless bridge-vlan so br-lan.<vlan> can exist
        cfgs["network"].append(
            &NetworkBridgeVlan {
                device: found_bridge.clone(),
                vlan: vlan_tag,
                ports: Vec::new(),
            },
            None,
        )?;
        // Create wifi-vlan sections so hostapd knows about this VLAN at boot
        for iface_name in crate::wifi::find_ap_interface_names(cfgs)? {
            cfgs["wireless"].append(
                &WifiVlan {
                    name: interface.clone(),
                    network: interface.clone(),
                    vid: vlan_tag,
                    iface: Some(iface_name),
                },
                None,
            )?;
        }
    }
    let mut wants_access = Vec::new();
    for section in &cfgs["startwrt"].sections {
        if let Some(other_profile) = section.get_typed::<UciProfile>()? {
            if other_profile.access_to_new_profiles {
                wants_access.push(other_profile.id());
            }
        }
    }
    cfgs["startwrt"].append(
        &UciProfile {
            fullname: fullname.clone(),
            interface: interface.clone(),
            vlan_tag,
            outbound: Some(profile.outbound.clone()),
            access_to_new_profiles: profile.access_to_new_profiles,
            wan_access: Some(wan_access_type_str(&profile.wan_access)),
            wan_access_list: wan_access_destinations(&profile.wan_access),
            dns_override: dns::serialize_dns_server_list(&profile.dns_override),
            wan_schedule: Vec::new(),
        },
        Some(&interface),
    )?;
    let lookup = Lookup::parse(ctx.clone(), &cfgs)?; // only lookup after pushing, in case there is a self-reference
    let profile = Profile {
        id: ProfileId {
            fullname,
            interface,
            vlan_tag,
        },
        gateway_ip: profile.gateway_ip,
        outbound: profile.outbound.clone(),
        lan_access: match &profile.lan_access {
            LanAccess::All => LanAccess::All,
            LanAccess::SameProfile => LanAccess::SameProfile,
            LanAccess::OtherProfiles(set) => LanAccess::OtherProfiles(
                set.into_iter()
                    .map(|p| lookup.resolve(&p).cloned())
                    .collect::<Result<_, _>>()?,
            ),
        },
        wan_access: profile.wan_access.clone(),
        dns_override: profile.dns_override.clone(),
        dns_source: String::new(),
        access_to_new_profiles: profile.access_to_new_profiles,
        owns_lan: profile.owns_lan,
    };
    rewrite_firewall(&ctx, cfgs, &profile, &all_interfaces, &wants_access, true)?;
    rewrite_dhcp(&ctx, cfgs, &profile)?;
    rewrite_dns_forwarding(cfgs, &profile)?;
    rewrite_routing(&ctx, cfgs, &profile)?;
    sync_cross_subnet_routes(cfgs)?;
    crate::vpn_server::sync_vpn_peer_cross_routes(cfgs)?;
    cleanup_orphaned_vpn_zones(cfgs);
    Ok(profile.id)
}

fn rewrite_firewall(
    _ctx: &impl CtrlContext,
    cfgs: &mut Configs,
    profile: &Profile,
    all_interfaces: &BTreeSet<String>,
    wants_access: &[ProfileId],
    remake_zone: bool,
) -> Result<(), Error> {
    // Make sure the required zones exist
    let mut found_wan = false;
    let mut this_zone_name = format!("vlan_{}", profile.id.interface);
    let mut all_zones = BTreeMap::new();
    let mut existing_zone_index = None;
    for (index, section) in cfgs["firewall"].sections.iter().enumerate() {
        let Ok(zone) = section.get::<FirewallZone>() else {
            continue;
        };
        if zone.name == DEFAULT_WAN_ZONE {
            found_wan = true;
        } else if zone.name == this_zone_name {
            existing_zone_index = Some(index);
        } else {
            for zone_interface in zone.network {
                if profile.id.interface == zone_interface {
                    this_zone_name = zone.name.clone();
                    existing_zone_index = Some(index);
                }
                if all_interfaces.contains(&zone_interface) {
                    all_zones.insert(zone_interface, zone.name.clone());
                }
            }
        }
    }
    if !found_wan {
        return Err(Error::new(eyre!("missing WAN interface"), ErrorKind::MissingWanInterface));
    }
    if remake_zone || existing_zone_index.is_none() {
        if let Some(index) = existing_zone_index {
            let removed = cfgs["firewall"].sections.remove(index);
            assert!(removed.ty() == FirewallZone::TY);
        }
        cfgs["firewall"].append(
            &FirewallZone {
                name: this_zone_name.clone(),
                input: FirewallTarget::ACCEPT,
                output: FirewallTarget::ACCEPT,
                forward: FirewallTarget::ACCEPT,
                network: vec![profile.id.interface.clone()],
                masq: None,
                masq6: None,
                mtu_fix: None,
            },
            None,
        )?;
    }

    // Egress zone for this profile's internet-bound traffic.
    // "wan" for wan-routed profiles; "vpn_<wg_X>" (a dedicated NAT66-enabled
    // zone, ensured by ensure_vpn_outbound_zone in rewrite_routing) for
    // VPN-routed profiles — the dedicated zone carries masq6=1 so NAT applies
    // on VPN egress without touching wan6.
    let outbound_zone = resolve_outbound_zone(&profile.outbound);

    // Setup forwarding for DNS and DHCP
    let mut found_dhcp_dns_rule = false;
    cfgs["firewall"].sections.retain(|section| {
        let Ok(rule) = section.get::<FirewallRule>() else {
            return true;
        };
        if rule.src == this_zone_name && rule.name.contains("DHCP") && rule.name.contains("DNS") {
            if remake_zone {
                return false;
            } else {
                found_dhcp_dns_rule = true;
            }
        }
        true
    });
    if !found_dhcp_dns_rule {
        cfgs["firewall"].append(
            &FirewallRule {
                name: format!("Allow-DHCP-DNS-{}", profile.id.fullname.replace(" ", "-")),
                src: this_zone_name.clone(),
                dest_port: Some("53 67 68".into()),
                proto: vec!["tcp".into(), "udp".into(), "icmp".into()],
                target: FirewallTarget::ACCEPT,
                ..Default::default()
            },
            None,
        )?;
    }

    // Clean up old DNS override redirects for this profile
    cfgs["firewall"].sections.retain(|section| {
        let Ok(redir) = section.get::<FirewallRedirect>() else {
            return true;
        };
        !(redir.src == this_zone_name && redir.name.contains("DNS-Override"))
    });

    // DNS hijacking: redirect all port 53 traffic to the profile's gateway,
    // where per-profile dnsmasq handles .lan locally and forwards the rest
    // via SmartDNS (for custom/system DNS) or directly (for VPN DNS).
    if has_effective_dns(cfgs, profile) || !dns::get_system_dns_servers(cfgs).is_empty() {
        cfgs["firewall"].append(
            &FirewallRedirect {
                name: format!(
                    "DNS-Override-{}",
                    profile.id.fullname.replace(" ", "-"),
                ),
                src: this_zone_name.clone(),
                proto: vec!["tcp".into(), "udp".into()],
                src_dport: Some("53".into()),
                dest_ip: Some(profile.gateway_ip.to_string()),
                dest_port: Some("53".into()),
                target: "DNAT".into(),
                ..Default::default()
            },
            None,
        )?;
    }

    // Clean up old WAN-access rules for this profile. The dest could be either
    // "wan" or a previous "vpn_<X>" zone (if the profile's outbound changed),
    // so match on src + non-DHCP name only. (Schedule `sched_*` rules are
    // re-applied separately by evaluate_and_apply_schedules after reload.)
    cfgs["firewall"].sections.retain(|section| {
        let Ok(rule) = section.get::<FirewallRule>() else {
            return true;
        };
        !(rule.src == this_zone_name && !rule.name.contains("DHCP"))
    });

    // Setup forwarding for lan access
    cfgs["firewall"].sections.retain(|section| {
        let Ok(fwd) = section.get::<FirewallForwarding>() else {
            return true;
        };
        fwd.src != this_zone_name
    });
    match &profile.lan_access {
        LanAccess::All => {
            for other_zone in all_zones.values() {
                if other_zone != &this_zone_name {
                    cfgs["firewall"].append(
                        &FirewallForwarding {
                            src: this_zone_name.clone(),
                            dest: other_zone.clone(),
                        },
                        None,
                    )?;
                }
            }
        }
        LanAccess::SameProfile => (),
        LanAccess::OtherProfiles(profile_ids) => {
            for other_profile in profile_ids {
                match all_zones.get(&other_profile.interface) {
                    Some(other_zone) => cfgs["firewall"].append(
                        &FirewallForwarding {
                            src: this_zone_name.clone(),
                            dest: other_zone.clone(),
                        },
                        None,
                    )?,
                    None => {
                        return Err(Error::new(eyre!("missing firewall zone for interface: {}", other_profile.interface), ErrorKind::MissingFirewallZone));
                    }
                }
            }
        }
    }
    for other_profile in wants_access {
        match all_zones.get(&other_profile.interface) {
            Some(other_zone) => cfgs["firewall"].append(
                &FirewallForwarding {
                    src: other_zone.clone(),
                    dest: this_zone_name.clone(),
                },
                None,
            )?,
            None => {
                return Err(Error::new(eyre!("missing firewall zone for interface: {}", other_profile.interface), ErrorKind::MissingFirewallZone));
            }
        }
    }

    // Setup forwarding for wan access. `outbound_zone` is "wan" for wan-routed
    // profiles and "vpn_<wg_X>" for VPN-routed ones; routing sends the traffic
    // out the corresponding interface, so the forward chain must permit it to
    // that zone.
    //
    // TODO(ipv6/nat66): a wan-routed non-admin profile on a /64-only ISP can only
    // get a ULA (the single GUA /64 goes to the admin LAN) and the `wan` zone has
    // no masq6, so its IPv6 is local-only — no internet path. To give such
    // profiles v6 internet without a larger PD, NAT66 their ULA out wan (e.g. a
    // dedicated wan-egress zone with masq6=1, mirroring ensure_vpn_outbound_zone).
    // Today v6 internet for non-admin profiles works only via a GUA (larger ISP
    // delegation) or VPN egress (masq6 on the vpn_<X> zone).
    match &profile.wan_access {
        WanAccess::All => cfgs["firewall"].append(
            &FirewallForwarding {
                src: this_zone_name.clone(),
                dest: outbound_zone.clone(),
            },
            None,
        )?,
        WanAccess::None => (),
        WanAccess::Whitelist(destinations) => {
            // Forwarding needed so rule matching works in the forward chain
            cfgs["firewall"].append(
                &FirewallForwarding {
                    src: this_zone_name.clone(),
                    dest: outbound_zone.clone(),
                },
                None,
            )?;
            // ACCEPT rules for each allowed destination
            for dest_ip in destinations {
                cfgs["firewall"].append(
                    &FirewallRule {
                        name: format!(
                            "WAN-WL-{}-{}",
                            profile.id.fullname.replace(" ", "-"),
                            dest_ip
                        ),
                        src: this_zone_name.clone(),
                        dest: Some(outbound_zone.clone()),
                        dest_ip: Some(dest_ip.clone()),
                        proto: vec!["all".into()],
                        target: FirewallTarget::ACCEPT,
                        ..Default::default()
                    },
                    None,
                )?;
            }
            // Catch-all REJECT for everything else
            cfgs["firewall"].append(
                &FirewallRule {
                    name: format!(
                        "WAN-WL-{}-reject",
                        profile.id.fullname.replace(" ", "-")
                    ),
                    src: this_zone_name.clone(),
                    dest: Some(outbound_zone.clone()),
                    proto: vec!["all".into()],
                    target: FirewallTarget::REJECT,
                    ..Default::default()
                },
                None,
            )?;
        }
        WanAccess::Blacklist(destinations) => {
            // Default allow via forwarding
            cfgs["firewall"].append(
                &FirewallForwarding {
                    src: this_zone_name.clone(),
                    dest: outbound_zone.clone(),
                },
                None,
            )?;
            // REJECT rules for each blocked destination
            for dest_ip in destinations {
                cfgs["firewall"].append(
                    &FirewallRule {
                        name: format!(
                            "WAN-BL-{}-{}",
                            profile.id.fullname.replace(" ", "-"),
                            dest_ip
                        ),
                        src: this_zone_name.clone(),
                        dest: Some(outbound_zone.clone()),
                        dest_ip: Some(dest_ip.clone()),
                        proto: vec!["all".into()],
                        target: FirewallTarget::REJECT,
                        ..Default::default()
                    },
                    None,
                )?;
            }
        }
    }

    Ok(())
}

pub(crate) fn is_ipv6_enabled(cfgs: &Configs) -> bool {
    cfgs["dhcp"].sections.iter().any(|s| {
        s.name().as_deref() == Some("lan")
            && s.get_typed::<Dhcp>()
                .ok()
                .flatten()
                .and_then(|d| d.ra)
                .map(|ra| ra == "server")
                .unwrap_or(false)
    })
}

/// Check whether an outbound VPN interface has IPv6 addresses configured.
/// If the outbound is "wan" or the VPN has at least one IPv6 address, returns true.
/// If the VPN has no IPv6 addresses, returns false (IPv6 would leak outside the tunnel).
pub(crate) fn outbound_supports_ipv6(cfgs: &Configs, outbound: &str) -> bool {
    use crate::vpn_server::WgInterface;
    if outbound == "wan" {
        return true;
    }
    cfgs["network"]
        .sections
        .iter()
        .find(|s| s.name().as_deref() == Some(outbound))
        .and_then(|s| s.get::<WgInterface>().ok())
        .filter(|wg| wg.is_wireguard())
        .map(|wg| wg.addresses.iter().any(|addr| addr.contains(':')))
        .unwrap_or(true) // if we can't find the interface, don't restrict
}

pub fn rewrite_dhcp(
    _ctx: &impl CtrlContext,
    cfgs: &mut Configs,
    profile: &Profile,
) -> Result<(), Error> {
    let ipv6 = is_ipv6_enabled(cfgs)
        && outbound_supports_ipv6(cfgs, &profile.outbound);
    let ra_value = if ipv6 { "server" } else { "disabled" }.to_string();
    let dhcpv6_value = if ipv6 { "server" } else { "disabled" }.to_string();
    // When IPv6 routes through a VPN, force odhcpd to advertise this router as
    // the IPv6 default. Without this, downstream-without-PD setups (where wan6
    // has no default route) make odhcpd suppress the Default Router Lifetime,
    // so LAN clients have no IPv6 default route and traffic never reaches the
    // policy-routing layer. `wan` keeps odhcpd's default behavior (mode 2 —
    // advertise only if wan6 has a default), so we don't override there.
    let ra_default_value = if ipv6 && profile.outbound != "wan" {
        Some("1".to_string())
    } else {
        None
    };

    let mut found_dhcp = false;
    for section in &mut cfgs["dhcp"].sections {
        let Ok(mut dhcp) = section.get::<Dhcp>() else {
            continue;
        };
        if dhcp.interface == profile.id.interface {
            let mut changed = false;
            if dhcp.ra.as_deref() != Some(&ra_value) || dhcp.dhcpv6.as_deref() != Some(&dhcpv6_value) {
                dhcp.ra = Some(ra_value.clone());
                dhcp.dhcpv6 = Some(dhcpv6_value.clone());
                changed = true;
            }
            if dhcp.ra_default != ra_default_value {
                dhcp.ra_default = ra_default_value.clone();
                changed = true;
            }
            // Enforce DHCP pool bounds (avoid overlap with VPN peer range .200-.253)
            if dhcp.start != 2 || dhcp.limit != 198 {
                dhcp.start = 2;
                dhcp.limit = 198;
                changed = true;
            }
            if changed {
                section.set(&dhcp)?;
            }
            found_dhcp = true;
        }
    }
    if !found_dhcp {
        cfgs["dhcp"].append(
            &Dhcp {
                interface: profile.id.interface.clone(),
                start: 2,
                limit: 198,
                leasetime: "12h".into(),
                ra: Some(ra_value),
                dhcpv6: Some(dhcpv6_value),
                ra_management: None,
                ra_default: ra_default_value,
            },
            Some(&profile.id.interface),
        )?;
    }
    Ok(())
}

pub(crate) fn rewrite_routing(
    _ctx: &impl CtrlContext,
    cfgs: &mut Configs,
    profile: &Profile,
) -> Result<(), Error> {
    let route_name = format!("prt_{}", profile.id.interface);
    let route_block_name = format!("prtb_{}", profile.id.interface);
    let local_route_name = format!("plr_{}", profile.id.interface);
    let rule_name = format!("prr_{}", profile.id.interface);
    let route6_name = format!("prt6_{}", profile.id.interface);
    let route6_block_name = format!("prt6b_{}", profile.id.interface);
    let local6_rule_name = format!("prl6_{}", profile.id.interface);
    let rule6_name = format!("prr6_{}", profile.id.interface);
    let mark_rule_name = format!("dnat_mark_{}", profile.id.interface);

    // 1. Remove old route/rule/mark for this profile (both v4 and v6 siblings,
    //    incl. the kill-switch fallback routes)
    cfgs["network"].sections.retain(|s| {
        let n = s.name();
        n.as_deref() != Some(route_name.as_str())
            && n.as_deref() != Some(route_block_name.as_str())
            && n.as_deref() != Some(local_route_name.as_str())
            && n.as_deref() != Some(rule_name.as_str())
            && n.as_deref() != Some(route6_name.as_str())
            && n.as_deref() != Some(route6_block_name.as_str())
            && n.as_deref() != Some(local6_rule_name.as_str())
            && n.as_deref() != Some(rule6_name.as_str())
    });
    cfgs["firewall"]
        .sections
        .retain(|s| s.name().as_deref() != Some(mark_rule_name.as_str()));

    // 2. If outbound is "wan", main table suffices — no policy routing needed
    if profile.outbound == "wan" {
        // Clean up any stale VPN peer routes from when this profile used a VPN outbound
        crate::vpn_server::sync_peer_policy_routes(cfgs, &profile.id.interface)?;
        return Ok(());
    }

    // Whether this VPN carries IPv6. The v6 policy routing (rules + unreachable
    // fallback) is installed for every VPN-routed profile so v6 always fails
    // closed; the per-VLAN default points at the tunnel only when the VPN
    // actually carries v6 (otherwise the unreachable fallback is the only
    // default). RA/address service stays gated by is_ipv6_enabled in
    // rewrite_dhcp — that controls whether clients GET v6, independent of where
    // v6 routes if present.
    let vpn_has_v6 = outbound_supports_ipv6(cfgs, &profile.outbound);

    // 3. Create routing table entry:
    //    config route 'prt_<interface>'
    //      option interface '<outbound>'
    //      option target '0.0.0.0/0'
    //      option table '<vlan_tag>'
    //      option metric '1'   (beats the kill-switch fallback below)
    cfgs["network"].append(
        &NetworkRoute {
            interface: profile.outbound.clone(),
            target: "0.0.0.0/0".to_string(),
            gateway: None,
            netmask: None,
            metric: Some(VPN_DEFAULT_ROUTE_METRIC),
            table: Some(profile.id.vlan_tag as u32),
            kind: None,
        },
        Some(&route_name),
    )?;

    // 3b. IPv4 kill-switch fallback: an `unreachable` default in the per-VLAN
    //     table, attached to loopback (always up) at a higher metric than the
    //     `dev <wg>` route above. While the WG interface is up the dev route
    //     wins; the moment it goes down (netifd removes interface-bound routes)
    //     this fallback catches the traffic with ENETUNREACH instead of letting
    //     the ip rule fall through to the main table and leak out WAN.
    cfgs["network"].append(
        &NetworkRoute {
            interface: "loopback".to_string(),
            target: "0.0.0.0/0".to_string(),
            metric: Some(VPN_KILLSWITCH_METRIC),
            table: Some(profile.id.vlan_tag as u32),
            kind: Some("unreachable".to_string()),
            ..Default::default()
        },
        Some(&route_block_name),
    )?;

    // 4. Create connected route for the profile's local subnet so LAN traffic
    //    stays local instead of being sent through the VPN tunnel:
    //    config route 'plr_<interface>'
    //      option interface '<interface>'
    //      option target '<network>/24'
    //      option table '<vlan_tag>'
    let octets = profile.gateway_ip.octets();
    let network_addr = format!("{}.{}.{}.0/24", octets[0], octets[1], octets[2]);
    cfgs["network"].append(
        &NetworkRoute {
            interface: profile.id.interface.clone(),
            target: network_addr.clone(),
            table: Some(profile.id.vlan_tag as u32),
            ..Default::default()
        },
        Some(&local_route_name),
    )?;

    // 5. Create ip rule:
    //    config rule 'prr_<interface>'
    //      option src '<network_addr>/24'
    //      option lookup '<vlan_tag>'
    //      option priority '200'
    cfgs["network"].append(
        &NetworkRule {
            src: Some(network_addr),
            lookup: profile.id.vlan_tag as u32,
            priority: Some(VPN_ROUTING_PRIORITY),
            ..Default::default()
        },
        Some(&rule_name),
    )?;

    // 5b. IPv6 policy routing. Installed for EVERY VPN-routed profile (mirrors
    //     the always-on v4 fallback above), independent of whether IPv6 is
    //     globally served — so v6 fails closed in all states (global v6 off,
    //     v4-only VPN, tunnel interface down). We can't mirror IPv4's
    //     `src=<prefix>` matcher because LAN /64s are dynamic under DHCPv6-PD,
    //     so two rule6 sections instead:
    //       * `prl6_<iface>`: lookup main, suppress_prefixlength=0 — match any
    //         specific route (e.g. on-link /64 to a sibling LAN) but fall
    //         through on default-route-only matches. Lets cross-VLAN and
    //         link-local traffic stay local instead of being captured by the
    //         per-VLAN table's ::/0 → VPN entry.
    //         NOTE (v4/v6 asymmetry, accepted): this escape can also reach the
    //         WAN interface's own on-link /64 (e.g. an upstream ULA/GUA segment)
    //         directly, bypassing the tunnel. It only ever permits on-link
    //         destinations — public-internet traffic has no specific main-table
    //         route, so it is always captured by `prr6_` below (tunnel or
    //         unreachable) and never leaks. The v4 path has no equivalent
    //         because its per-VLAN table is seeded with explicit local/sibling
    //         routes (plr_/pxr_). This matches upstream `pbr`'s escape rule.
    //       * `prr6_<iface>`: lookup <vlan_tag>, the per-VLAN table holding
    //         the VPN default route (or the unreachable kill switch).
    //     netifd matches `in: <logical_iface>` against logical interface names
    //     (iprule.c:194) and substitutes the kernel netdev at install time
    //     (iprule.c:134), so we pass the profile interface name directly.
    cfgs["network"].append(
        &NetworkRule6 {
            in_iface: Some(profile.id.interface.clone()),
            lookup: 254, // main
            suppress_prefixlength: Some(0),
            priority: Some(VPN_ROUTING_V6_LOCAL_PRIORITY),
            ..Default::default()
        },
        Some(&local6_rule_name),
    )?;
    cfgs["network"].append(
        &NetworkRule6 {
            in_iface: Some(profile.id.interface.clone()),
            lookup: profile.id.vlan_tag as u32,
            priority: Some(VPN_ROUTING_PRIORITY),
            ..Default::default()
        },
        Some(&rule6_name),
    )?;

    // When the VPN carries v6, the per-VLAN default points at the tunnel.
    if vpn_has_v6 {
        cfgs["network"].append(
            &NetworkRoute6 {
                interface: profile.outbound.clone(),
                target: "::/0".to_string(),
                metric: Some(VPN_DEFAULT_ROUTE_METRIC),
                table: Some(profile.id.vlan_tag as u32),
                ..Default::default()
            },
            Some(&route6_name),
        )?;
    }

    // IPv6 kill switch / fail-closed fallback: an `unreachable` default in the
    // per-VLAN table, attached to loopback at a higher metric. For a v4-only VPN
    // it is the only default (v6 is always blocked); for a v6-capable VPN it
    // backstops the `dev <wg>` route when the tunnel interface goes down — so v6
    // never falls through to wan6.
    cfgs["network"].append(
        &NetworkRoute6 {
            interface: "loopback".to_string(),
            target: "::/0".to_string(),
            metric: Some(VPN_KILLSWITCH_METRIC),
            table: Some(profile.id.vlan_tag as u32),
            kind: Some("unreachable".to_string()),
            ..Default::default()
        },
        Some(&route6_block_name),
    )?;

    // 6. DNAT-return marking: replies to inbound port-forwarded connections must
    //    route via the main table, not the VPN tunnel. The marks are set by
    //    static nftables chains (fw4 has no UCI option for `ct status dnat` /
    //    connmark matching); here we only ensure the matching ip rules.
    //      * IPv4: 10-startwrt-dnat-mark.nft marks `ct status dnat` packets;
    //        dnat_return routes fwmark 0x80 -> main.
    //      * IPv6: 11-startwrt-inbound6-mark.nft connection-marks WAN-initiated
    //        flows (v6 port-forwards aren't DNAT'd, so there's no dnat status to
    //        key on); dnat_return6 routes fwmark 0x80 -> main, ahead of prl6_/
    //        prr6_, so a published-port reply leaves via wan6 instead of the VPN.
    ensure_dnat_return_rule(cfgs)?;
    ensure_dnat_return6_rule(cfgs)?;

    // 7. Ensure a dedicated firewall zone exists for the VPN outbound, carrying
    //    masq=1/masq6=1. The per-profile forwardings created by rewrite_firewall
    //    target this zone (not wan), so NAT (incl. NAT66) applies on the VPN
    //    egress while wan6's GUA path and inbound port-forwards stay intact.
    ensure_vpn_outbound_zone(cfgs, &profile.outbound)?;

    // 8. Add /32 peer routes so locally-generated responses (DNS, HTTP) reach
    //    VPN clients via wg_X instead of being caught by the /24 subnet route
    crate::vpn_server::sync_peer_policy_routes(cfgs, &profile.id.interface)?;

    Ok(())
}

const DNAT_RETURN_RULE: &str = "dnat_return";
const DNAT_RETURN_RULE6: &str = "dnat_return6";
const DNAT_RETURN_MARK: &str = "0x80/0x80";
/// Must be lower (higher priority) than VPN_ROUTING_PRIORITY so that the
/// fwmark rule is evaluated before source-based VPN routing rules.
const DNAT_RETURN_PRIORITY: u32 = 100;
/// Explicit priority for source-based VPN ip rules, ensuring they don't
/// collide with DNAT_RETURN_PRIORITY through netifd auto-assignment.
pub(crate) const VPN_ROUTING_PRIORITY: u32 = 200;
/// IPv6 cross-VLAN escape rule: must fire before VPN_ROUTING_PRIORITY so
/// specific (sibling-LAN /64) destinations escape to the main table before
/// the per-VLAN default route captures them.
pub(crate) const VPN_ROUTING_V6_LOCAL_PRIORITY: u32 = 150;
/// Metric for the per-VLAN `dev <wg>` default route. Lower than
/// VPN_KILLSWITCH_METRIC so the tunnel route is preferred whenever the WG
/// interface is up.
const VPN_DEFAULT_ROUTE_METRIC: u32 = 1;
/// Metric for the fail-closed `unreachable` fallback in the per-VLAN table.
/// Higher than VPN_DEFAULT_ROUTE_METRIC and any netifd-default (≈1024), so it
/// only wins when the `dev <wg>` route is absent (interface down / v4-only VPN).
const VPN_KILLSWITCH_METRIC: u32 = 2048;

/// Ensure a single `ip rule` exists that routes fwmark 0x80 packets via the
/// main routing table.  This cooperates with the static nftables chain
/// (/etc/nftables.d/10-startwrt-dnat-mark.nft), which marks DNAT-state packets
/// with 0x80, to prevent DNAT reply traffic from being captured by source-based
/// VPN policy routing rules.
fn ensure_dnat_return_rule(cfgs: &mut Configs) -> Result<(), Error> {
    // Remove-then-append so the rule is always rewritten from the current
    // constants. A name-only existence check would let a stale definition
    // (an old priority/mark from a prior release, or a manual edit) survive
    // forever on already-provisioned devices, silently breaking the ordering
    // invariant with the source-based VPN rules.
    cfgs["network"]
        .sections
        .retain(|s| s.name().as_deref() != Some(DNAT_RETURN_RULE));
    cfgs["network"].append(
        &NetworkRule {
            src: None,
            lookup: 254, // main table
            mark: Some(DNAT_RETURN_MARK.to_string()),
            priority: Some(DNAT_RETURN_PRIORITY),
        },
        Some(DNAT_RETURN_RULE),
    )?;
    Ok(())
}

/// Ensure a single `ip -6 rule` exists that routes fwmark 0x80 packets via the
/// main routing table.  This is the IPv6 sibling of [`ensure_dnat_return_rule`]:
/// it cooperates with the static nftables chain
/// (/etc/nftables.d/11-startwrt-inbound6-mark.nft), which connection-marks
/// IPv6 flows initiated from WAN, so that replies to inbound port-forwarded
/// connections route out wan6 instead of being captured by the per-VLAN VPN
/// policy rules (prr6_*). Priority 100 (DNAT_RETURN_PRIORITY) keeps it ahead of
/// both prl6_ (150) and prr6_ (200).
fn ensure_dnat_return6_rule(cfgs: &mut Configs) -> Result<(), Error> {
    // Remove-then-append: see ensure_dnat_return_rule for why a name-only
    // existence check is insufficient.
    cfgs["network"]
        .sections
        .retain(|s| s.name().as_deref() != Some(DNAT_RETURN_RULE6));
    cfgs["network"].append(
        &NetworkRule6 {
            lookup: 254, // main table
            mark: Some(DNAT_RETURN_MARK.to_string()),
            priority: Some(DNAT_RETURN_PRIORITY),
            ..Default::default()
        },
        Some(DNAT_RETURN_RULE6),
    )?;
    Ok(())
}

/// Ensure each VPN-routed profile's routing table has routes for all sibling
/// profile subnets, so cross-VLAN traffic to/from the router stays local
/// instead of being sent through the VPN tunnel's default route.
///
/// Without these routes, a response from the router's admin IP (e.g. 192.168.0.1)
/// to a guest device (e.g. 192.168.8.X) would match the admin profile's
/// source-based policy rule and exit through the VPN instead of br-lan.101.
pub(crate) fn sync_cross_subnet_routes(cfgs: &mut Configs) -> Result<(), Error> {
    // 1. Remove all old cross-subnet routes
    cfgs["network"]
        .sections
        .retain(|s| !s.name().as_deref().map_or(false, |n| n.starts_with("pxr_")));

    // 2. Collect profiles with VPN routing (outbound != "wan")
    let vpn_profiles: Vec<(String, u16)> = cfgs["startwrt"]
        .sections
        .iter()
        .filter_map(|s| {
            let p = s.get::<UciProfile>().ok()?;
            let outbound = p.outbound.as_deref().unwrap_or("wan");
            if outbound != "wan" {
                Some((p.interface.clone(), p.vlan_tag))
            } else {
                None
            }
        })
        .collect();

    if vpn_profiles.is_empty() {
        return Ok(());
    }

    // 3. Collect all profile subnets from static bridge network interfaces
    let subnets: Vec<(String, String)> = cfgs["network"]
        .sections
        .iter()
        .filter_map(|s| {
            let iface = s.get::<NetworkInterface>().ok()?;
            let name = s.name()?.to_string();
            if iface.proto != InterfaceProto::STATIC {
                return None;
            }
            // Only include profile interfaces (br-lan / br-lan.N), not loopback etc.
            if !iface.device.starts_with("br-lan") {
                return None;
            }
            let ip = iface.ipaddr?;
            let o = ip.octets();
            Some((name, format!("{}.{}.{}.0/24", o[0], o[1], o[2])))
        })
        .collect();

    // 4. For each VPN profile, add routes for all sibling subnets
    for (vpn_iface, vlan_tag) in &vpn_profiles {
        for (sibling_iface, subnet) in &subnets {
            if sibling_iface == vpn_iface {
                continue; // self-route already handled by rewrite_routing
            }
            cfgs["network"].append(
                &NetworkRoute {
                    interface: sibling_iface.clone(),
                    target: subnet.clone(),
                    table: Some(*vlan_tag as u32),
                    ..Default::default()
                },
                Some(&format!("pxr_{}_{}", vpn_iface, sibling_iface)),
            )?;
        }
    }

    Ok(())
}

/// The firewall zone name for a profile's outbound:
///   - `"wan"` when outbound is `"wan"`.
///   - `"vpn_<wg_name>"` (a dedicated NAT66-enabled zone) otherwise.
///
/// Used in `rewrite_firewall`/`evaluate_and_apply_schedules` to compute
/// forwarding/rule `dest` fields and in `ensure_vpn_outbound_zone` to manage
/// the zone lifecycle.
pub(crate) fn resolve_outbound_zone(outbound: &str) -> String {
    if outbound == DEFAULT_WAN_ZONE {
        DEFAULT_WAN_ZONE.to_string()
    } else {
        format!("vpn_{outbound}")
    }
}

/// Ensure a dedicated firewall zone exists for a VPN outbound, with v4 and v6
/// masquerade enabled. Returns the zone name.
///
/// The separate zone (vs. dumping the wg interface into `wan`) is what lets us
/// apply NAT66 to VPN-routed traffic without touching wan6 — preserving the
/// admin LAN's GUA path and inbound port-forwards.
///
/// Side effects: creates `vpn_<wg_name>` if missing; strips `wg_name` from any
/// other zone's `network` (notably leftover `wan` membership from before this
/// layout).
pub(crate) fn ensure_vpn_outbound_zone(
    cfgs: &mut Configs,
    wg_name: &str,
) -> Result<String, Error> {
    let zone_name = resolve_outbound_zone(wg_name);

    // Strip wg_name from any zone that isn't ours (esp. wan).
    for section in &mut cfgs["firewall"].sections {
        let Ok(mut zone) = section.get::<FirewallZone>() else {
            continue;
        };
        if zone.name == zone_name {
            continue;
        }
        if zone.network.iter().any(|n| n == wg_name) {
            zone.network.retain(|n| n != wg_name);
            section.set(&zone)?;
        }
    }

    // Find or update the dedicated zone.
    let mut exists = false;
    for section in &mut cfgs["firewall"].sections {
        let Ok(mut zone) = section.get::<FirewallZone>() else {
            continue;
        };
        if zone.name != zone_name {
            continue;
        }
        exists = true;
        let mut changed = false;
        if !zone.network.iter().any(|n| n == wg_name) {
            zone.network.push(wg_name.to_string());
            changed = true;
        }
        if zone.masq != Some(true) {
            zone.masq = Some(true);
            changed = true;
        }
        if zone.masq6 != Some(true) {
            zone.masq6 = Some(true);
            changed = true;
        }
        // MSS-clamp forwarded TCP to the tunnel's route MTU so a tunnel MTU
        // smaller than the LAN's doesn't silently black-hole large packets.
        if zone.mtu_fix != Some(true) {
            zone.mtu_fix = Some(true);
            changed = true;
        }
        if changed {
            section.set(&zone)?;
        }
        break;
    }
    if !exists {
        cfgs["firewall"].append(
            &FirewallZone {
                name: zone_name.clone(),
                input: FirewallTarget::REJECT,
                output: FirewallTarget::ACCEPT,
                forward: FirewallTarget::REJECT,
                network: vec![wg_name.to_string()],
                masq: Some(true),
                masq6: Some(true),
                mtu_fix: Some(true),
            },
            None,
        )?;
    }

    Ok(zone_name)
}

/// After routing changes, remove `vpn_<X>` zones (and any forwardings or rules
/// that reference them) when no profile uses `wg_X` as outbound. Also strips
/// stray `wg_X` entries from the wan zone (pre-migration leftovers from when
/// VPN outbounds lived directly in the wan zone).
pub(crate) fn cleanup_orphaned_vpn_zones(cfgs: &mut Configs) {
    // Collect all VPN interface names referenced by any profile's outbound
    let mut referenced_vpns = std::collections::HashSet::new();
    for section in &cfgs["startwrt"].sections {
        if section.ty() != "profile" {
            continue;
        }
        if let Ok(profile) = section.get::<UciProfile>() {
            let outbound = profile.outbound.unwrap_or_else(|| "wan".to_string());
            if outbound != "wan" {
                referenced_vpns.insert(outbound);
            }
        }
    }
    let referenced_zone_names: std::collections::HashSet<String> = referenced_vpns
        .iter()
        .map(|wg| resolve_outbound_zone(wg))
        .collect();

    // 1. Collect orphaned vpn_<X> zone names.
    let mut orphaned_zone_names: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for section in &cfgs["firewall"].sections {
        let Ok(zone) = section.get::<FirewallZone>() else {
            continue;
        };
        if !zone.name.starts_with("vpn_") {
            continue;
        }
        if !referenced_zone_names.contains(&zone.name) {
            orphaned_zone_names.insert(zone.name);
        }
    }

    // 2. Drop orphaned zones + any forwarding/rule referencing them.
    cfgs["firewall"].sections.retain(|section| {
        if let Ok(zone) = section.get::<FirewallZone>() {
            if orphaned_zone_names.contains(&zone.name) {
                return false;
            }
        }
        if let Ok(fwd) = section.get::<FirewallForwarding>() {
            if orphaned_zone_names.contains(&fwd.src) || orphaned_zone_names.contains(&fwd.dest) {
                return false;
            }
        }
        if let Ok(rule) = section.get::<FirewallRule>() {
            if orphaned_zone_names.contains(&rule.src)
                || rule
                    .dest
                    .as_ref()
                    .map_or(false, |d| orphaned_zone_names.contains(d))
            {
                return false;
            }
        }
        true
    });

    // 3. Defense in depth: strip any stray wg_X entries still in the wan zone.
    for section in &mut cfgs["firewall"].sections {
        if let Ok(mut zone) = section.get::<FirewallZone>() {
            if zone.name == DEFAULT_WAN_ZONE {
                let before_len = zone.network.len();
                zone.network
                    .retain(|n| !n.starts_with("wg_") || referenced_vpns.contains(n));
                if zone.network.len() != before_len {
                    let _ = section.set(&zone);
                }
                break;
            }
        }
    }

    // 4. The global DNAT-return ip rules exist only to keep port-forward replies
    //    on the main table for VPN-routed profiles. With no VPN-routed profile
    //    left they have no consumer — tear them down so they exist iff at least
    //    one VPN-routed profile does, mirroring the zone lifecycle above.
    if referenced_vpns.is_empty() {
        cfgs["network"].sections.retain(|s| {
            let n = s.name();
            n.as_deref() != Some(DNAT_RETURN_RULE) && n.as_deref() != Some(DNAT_RETURN_RULE6)
        });
    }
}

/// A random `INTERFACE_NAME_LIMIT`-char lowercase interface id. Always starts
/// with a letter (the alphabet is `a..=z`), so it is a valid UCI section name.
fn random_interface_name() -> String {
    String::from_iter([(); INTERFACE_NAME_LIMIT].map(|_| rand::random_range('a'..='z')))
}

/// True if a kernel network device with this name currently exists. An
/// ambiguous error is treated as "exists" so the caller picks another name.
async fn netdev_exists(name: &str) -> bool {
    // `ip link show <name>` exits non-zero with a "device not found" message
    // when no such device exists.
    match tokio::process::Command::new("ip")
        .arg("link")
        .arg("show")
        .arg(name)
        .invoke(ErrorKind::Network.into())
        .await
    {
        Err(e) if e.to_string().contains("can't find device")
            || e.to_string().contains("does not exist") =>
        {
            false
        }
        Err(_) | Ok(_) => true,
    }
}

/// The interface ids already in use, so a newly created profile can avoid them.
/// Mirrors the conflict surface checked in `create_config`: every `network`
/// interface section name (`lan`, `wan`, `wan6`, `loopback`, `wg_*` VPN ifaces,
/// other profiles' ifaces, …) plus every existing profile's interface.
fn reserved_interface_names(cfgs: &Configs) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for section in &cfgs["network"].sections {
        if &*section.ty() == NetworkInterface::TY {
            if let Some(name) = section.name() {
                set.insert(name.to_string());
            }
        }
    }
    for section in &cfgs["startwrt"].sections {
        if let Ok(Some(profile)) = section.get_typed::<UciProfile>() {
            set.insert(profile.interface);
        }
    }
    set
}

/// Allocate a random UCI interface id that collides with neither an existing
/// UCI interface/profile id (`taken`) nor a live kernel netdev.
///
/// The id is opaque and never shown in the UI — profiles are displayed by their
/// separate, freely-renamable `fullname` — so it is always random rather than
/// derived from the profile name. That carries no UX cost and means the id is
/// never re-derived on rename, which is what keeps a renamed profile's
/// WireGuard device, firewall zone, and DHCP/route names stable.
pub async fn allocate_interface_name(
    ctx: &impl CtrlContext,
    taken: &BTreeSet<String>,
) -> Result<String, Error> {
    let mut name = random_interface_name();
    for _ in 0..100 {
        if !taken.contains(&name) && !(ctx.effectful() && netdev_exists(&name).await) {
            return Ok(name);
        }
        // Collides with a reserved id or a live device — try another random one.
        name = random_interface_name();
    }
    Err(Error::new(eyre!("gave up looking for a new interface name"), ErrorKind::InterfaceNameConflict))
}

pub struct Lookup {
    list: Vec<ProfileId>,
    lan_owner: Option<ProfileId>,
    from_vlan: OnceCell<HashMap<u16, ProfileId>>,
    from_interface: OnceCell<HashMap<String, ProfileId>>,
    from_fullname: OnceCell<HashMap<String, ProfileId>>,
}

impl Lookup {
    pub fn parse<C: CtrlContext>(ctx: C, cfgs: &Configs) -> Result<Self, Error> {
        let list = list_config(ctx, cfgs)?;
        let system = list.iter().find(|p| p.interface == "lan").cloned();
        Ok(Self {
            list,
            lan_owner: system,
            from_vlan: OnceCell::new(),
            from_interface: OnceCell::new(),
            from_fullname: OnceCell::new(),
        })
    }

    pub fn list(&self) -> &[ProfileId] {
        &self.list
    }

    pub fn resolve(
        &self,
        q @ ProfileIdOpt {
            fullname,
            interface,
            vlan_tag,
        }: &ProfileIdOpt,
    ) -> Result<&ProfileId, Error> {
        if let Some(q) = interface {
            if let Some(o) = self.from_interface(q) {
                return Ok(o);
            }
        }
        if let Some(q) = vlan_tag {
            if let Some(o) = self.from_vlan(*q) {
                return Ok(o);
            }
        }
        if let Some(q) = fullname {
            if let Some(o) = self.from_fullname(q) {
                return Ok(o);
            }
        }
        Err(Error::new(eyre!("missing profile: {q:?}"), ErrorKind::MissingProfile))
    }

    pub fn from_vlan(&self, vlan_tag: u16) -> Option<&ProfileId> {
        self.from_vlan
            .get_or_init(|| {
                self.list
                    .iter()
                    .map(|id| (id.vlan_tag, id.clone()))
                    .collect()
            })
            .get(&vlan_tag)
    }

    pub fn from_interface(&self, interface: &str) -> Option<&ProfileId> {
        self.from_interface
            .get_or_init(|| {
                self.list
                    .iter()
                    .map(|id| (id.interface.clone(), id.clone()))
                    .collect()
            })
            .get(interface)
    }

    pub fn from_fullname(&self, fullname: &str) -> Option<&ProfileId> {
        self.from_fullname
            .get_or_init(|| {
                self.list
                    .iter()
                    .map(|id| (id.fullname.clone(), id.clone()))
                    .collect()
            })
            .get(fullname)
    }
}

/// Bootstrap the default Admin security profile on first boot.
///
/// Registers the existing LAN infrastructure (network interface, firewall zone,
/// DHCP server) as the "Admin" profile in the `startwrt` UCI config. This is a
/// lightweight operation — it only writes the `startwrt` config entry and does
/// not create VLANs, bridge-vlans, or modify network topology.
///
/// Idempotent: returns `Ok(())` immediately if a LAN owner profile already exists.
pub async fn bootstrap_admin_profile(uci_root: &str) -> Result<(), Error> {
    let arena = Arena::new();
    let mut cfgs = parse_all(uci_root, &arena, &["startwrt"]).await?;

    // Check if a LAN owner profile already exists
    for section in &cfgs["startwrt"].sections {
        if let Ok(profile) = section.get::<UciProfile>() {
            if profile.interface == "lan" {
                return Ok(());
            }
        }
    }

    // Register the existing LAN as the Admin profile
    cfgs["startwrt"].append(
        &UciProfile {
            fullname: "Admin".into(),
            interface: "lan".into(),
            vlan_tag: 1,
            outbound: Some("wan".into()),
            access_to_new_profiles: true,
            wan_access: Some("all".into()),
            wan_access_list: Vec::new(),
            dns_override: Vec::new(),
            wan_schedule: Vec::new(),
        },
        Some("lan"),
    )?;

    // Write default preferences so the frontend has values on first load
    cfgs["startwrt"].append(
        &UciPreferences {
            language: "en_US".to_string(),
            theme: "system".to_string(),
            remote_access: "default".to_string(),
        },
        Some("preferences"),
    )?;

    dump_all(uci_root, cfgs).await?;
    Ok(())
}

#[derive(Debug, Parser, Serialize, Deserialize)]
pub struct EditArgs {
    #[clap(flatten)]
    pub get: ProfileIdOpt,
    #[clap(long)]
    pub create: bool,
}

#[instrument(skip_all)]
pub async fn edit<C: CtrlContext>(ctx: C, args: EditArgs) -> Result<ProfileId, Error> {
    if args.create {
        let template = Profile {
            id: args.get.clone(),
            gateway_ip: std::net::Ipv4Addr::new(192, 168, 1, 1),
            outbound: "wan".to_string(),
            lan_access: LanAccess::All,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: true,
            owns_lan: list(ctx.clone()).await?.is_empty(),
        };
        let modified_profile: Profile<ProfileIdOpt> = crate::utils::edit_in_editor(&template)?;
        create(ctx, DeserializeStdin(modified_profile)).await
    } else {
        // Edit mode: get existing profile, edit, then set
        let current_profile = get(ctx.clone(), args.get).await?;
        let current_profile = Profile {
            id: current_profile.id.into(),
            gateway_ip: current_profile.gateway_ip,
            outbound: current_profile.outbound,
            lan_access: match current_profile.lan_access {
                LanAccess::All => LanAccess::All,
                LanAccess::SameProfile => LanAccess::SameProfile,
                LanAccess::OtherProfiles(set) => {
                    LanAccess::OtherProfiles(set.into_iter().map(Into::into).collect())
                }
            },
            wan_access: current_profile.wan_access,
            dns_override: current_profile.dns_override,
            dns_source: String::new(),
            access_to_new_profiles: current_profile.access_to_new_profiles,
            owns_lan: current_profile.owns_lan,
        };
        let modified_profile = crate::utils::edit_in_editor(&current_profile)?;
        set(ctx, DeserializeStdin(ProfileSetRequest {
            profile: modified_profile,
            force: false,
        })).await
    }
}

// ── Schedule helpers ──────────────────────────────────────────────────

const SCHEDULE_TAG: &str = "# start-wrt-wan-schedule";
const CRONTAB_PATH: &str = "/etc/crontabs/root";
const SCHEDULE_RULE_PREFIX: &str = "sched_";

fn schedule_crontab_path(ctx: &impl CtrlContext) -> PathBuf {
    if ctx.effectful() {
        PathBuf::from(CRONTAB_PATH)
    } else {
        ctx.uci_root().join("crontab_root")
    }
}

fn parse_hhmm(s: &str) -> Result<(u32, u32), Error> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err(Error::new(eyre!("invalid time format: {s:?}"), ErrorKind::InvalidValue));
    }
    let h: u32 = parts[0]
        .parse()
        .map_err(|_| Error::new(eyre!("invalid hour: {}", parts[0]), ErrorKind::InvalidValue))?;
    let m: u32 = parts[1]
        .parse()
        .map_err(|_| Error::new(eyre!("invalid minute: {}", parts[1]), ErrorKind::InvalidValue))?;
    if h > 23 || m > 59 {
        return Err(Error::new(eyre!("time out of range: {s:?}"), ErrorKind::InvalidValue));
    }
    Ok((h, m))
}

/// Is `(day, minute)` inside the window described by `start_min`/`end_min` and
/// the weekday `mask`? Wrap-aware: a window with `end < start` crosses midnight,
/// and `end == start` is a full 24h window — both block from `start` on their own
/// day through `end` the *following* day, so the after-midnight tail is gated on
/// the previous day's mask bit. This mirrors the edge logic in
/// `regenerate_schedule_crontab` (which shifts the unblock edge forward a day) so
/// the boot/reload reconciler agrees with the cron edges.
pub(crate) fn window_contains(start_min: u32, end_min: u32, mask: &[bool; 7], day: usize, minute: u32) -> bool {
    let prev_day = (day + 6) % 7;
    if start_min < end_min {
        mask[day] && minute >= start_min && minute < end_min
    } else {
        (mask[day] && minute >= start_min) || (mask[prev_day] && minute < end_min)
    }
}

fn serialize_schedule_windows(windows: &[ScheduleWindow]) -> Vec<String> {
    crate::wifi::serialize_windows(
        windows
            .iter()
            .map(|w| (w.start_time.as_str(), w.end_time.as_str(), &w.days)),
    )
}

fn parse_schedule_windows(raw: &[String]) -> Vec<ScheduleWindow> {
    crate::wifi::parse_windows(raw)
        .into_iter()
        .map(|(start_time, end_time, days)| ScheduleWindow {
            start_time,
            end_time,
            days,
        })
        .collect()
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ScheduleGetParams {
    interface: String,
}

/// Read schedule data for a single profile from UCI.
#[instrument(skip_all)]
pub async fn schedule_get<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(params): DeserializeStdin<ScheduleGetParams>,
) -> Result<Vec<ScheduleWindow>, Error> {
    let arena = Arena::new();
    let cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt"]).await?;

    for section in &cfgs["startwrt"].sections {
        if let Some(profile) = section.get_typed::<UciProfile>()? {
            if profile.interface == params.interface {
                return Ok(parse_schedule_windows(&profile.wan_schedule));
            }
        }
    }
    Err(Error::new(eyre!("missing profile: {}", params.interface), ErrorKind::MissingProfile))
}

/// Write schedule data for a single profile, regenerate crontab and firewall rules.
#[instrument(skip_all)]
pub async fn schedule_set<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(params): DeserializeStdin<ScheduleWindows>,
) -> Result<(), Error> {
    // Validate all times before writing to UCI. end < start denotes a window
    // that crosses midnight; end == start denotes a full 24-hour window. Both
    // are allowed.
    let mut parsed: Vec<(u32, u32, [bool; 7])> = Vec::with_capacity(params.windows.len());
    for window in &params.windows {
        let (start_h, start_m) = parse_hhmm(&window.start_time)?;
        let (end_h, end_m) = parse_hhmm(&window.end_time)?;
        let start_min = start_h * 60 + start_m;
        let end_min = end_h * 60 + end_m;
        parsed.push((start_min, end_min, window.days));
    }
    if crate::wifi::windows_overlap(&parsed) {
        return Err(Error::new(
            eyre!("schedule windows overlap"),
            ErrorKind::InvalidValue,
        ));
    }
    if crate::wifi::covers_full_week(&parsed) {
        return Err(Error::new(
            eyre!("schedule covers the entire week with no gap; disable WAN for this profile directly instead"),
            ErrorKind::InvalidValue,
        ));
    }

    let serialized = serialize_schedule_windows(&params.windows);

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt"]).await?;

        let mut found = false;
        for section in &mut cfgs["startwrt"].sections {
            if let Some(profile) = section.get_typed::<UciProfile>()? {
                if profile.interface == params.interface {
                    // Remove existing wan_schedule list entries
                    section.lines.retain(|line| {
                        !matches!(line, Line::List { list, .. } if list.as_str() == "wan_schedule")
                    });
                    // Add new entries
                    let arena = section.arena;
                    for val in &serialized {
                        section.lines.push(Line::List {
                            list: Token::from_string("wan_schedule".to_string(), arena),
                            item: Token::from_string(val.clone(), arena),
                            comment: LineComment::None,
                        });
                    }
                    found = true;
                    break;
                }
            }
        }
        if !found {
            return Err(Error::new(eyre!("missing profile: {}", params.interface), ErrorKind::MissingProfile));
        }

        let dump_result = dump_all(ctx.uci_root(), cfgs).await;
        drop(arena);
        match dump_result {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log(
                    "profile",
                    "schedule-updated",
                    false,
                    "Failed to update WAN schedule",
                    Some(&err.to_string()),
                );
                return Err(err.into());
            }
            Ok(()) => break,
        }
    }

    // Regenerate crontab entries for all profile schedules
    regenerate_schedule_crontab(&ctx).await?;

    // Evaluate current schedule state and apply UCI firewall rules
    if ctx.effectful() {
        evaluate_and_apply_schedules(&ctx).await?;
        crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/cron").arg("restart"))
            .await
            .map_err(|e| Error::new(eyre!("restarting cron: {e}"), ErrorKind::Filesystem))?;
    }

    crate::activity::log(
        "profile",
        "schedule-updated",
        true,
        &format!("Updated WAN schedule for profile {}", params.interface),
        None,
    );
    Ok(())
}

/// Regenerate all profile schedule crontab entries from UCI.
pub(crate) async fn regenerate_schedule_crontab(ctx: &impl CtrlContext) -> Result<(), Error> {
    let path = schedule_crontab_path(ctx);
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();

    // Remove old schedule entries
    let filtered: Vec<&str> = content
        .lines()
        .filter(|l| !l.contains(SCHEDULE_TAG))
        .collect();
    let mut new_content = filtered.join("\n");
    if !new_content.is_empty() && !new_content.ends_with('\n') {
        new_content.push('\n');
    }

    // Read profiles and firewall config (for zone name resolution)
    let arena = Arena::new();
    let cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt", "firewall"]).await?;

    for section in &cfgs["startwrt"].sections {
        if let Some(profile) = section.get_typed::<UciProfile>()? {
            let windows = crate::wifi::parse_windows(&profile.wan_schedule);
            if windows.is_empty() {
                continue;
            }
            let iface = &profile.interface;
            let zone = find_zone_for_interface(&cfgs, iface);
            // Block traffic toward the profile's egress zone — "wan" for
            // wan-routed profiles, "vpn_<wg_X>" for VPN-routed ones (else the
            // blackout wouldn't cover VPN egress).
            let egress = resolve_outbound_zone(profile.outbound.as_deref().unwrap_or("wan"));
            let sec = format!("{SCHEDULE_RULE_PREFIX}{iface}");

            // Project this profile's windows into deconflicted block/unblock cron
            // edges so adjacent/consecutive windows never race at a shared tick.
            // (Overlap and full-week-no-gap coverage are rejected in schedule_set;
            // the unblock day-shift for wrap/24h windows happens inside
            // deconflict_edges. Windows with unparseable times are dropped + logged
            // by windows_to_minutes.)
            let minute_windows = crate::wifi::windows_to_minutes(&windows);
            let (downs, ups) = crate::wifi::deconflict_edges(&minute_windows);

            for (&min, mask) in &downs {
                let (start_m, start_h) = (min % 60, min / 60);
                let days_str = crate::wifi::days_to_cron(mask);
                // Block at start_time: add UCI firewall rule + reload
                new_content.push_str(&format!(
                    "{start_m} {start_h} * * {days_str} \
                     uci set firewall.{sec}=rule; \
                     uci set firewall.{sec}.name='WAN-Schedule-{iface}'; \
                     uci set firewall.{sec}.src='{zone}'; \
                     uci set firewall.{sec}.dest='{egress}'; \
                     uci set firewall.{sec}.target='REJECT'; \
                     uci commit firewall; \
                     /etc/init.d/firewall reload \
                     {SCHEDULE_TAG}\n"
                ));
            }
            for (&min, mask) in &ups {
                let (end_m, end_h) = (min % 60, min / 60);
                let days_str = crate::wifi::days_to_cron(mask);
                // Unblock at end_time: remove UCI section + reload
                new_content.push_str(&format!(
                    "{end_m} {end_h} * * {days_str} \
                     uci -q delete firewall.{sec}; \
                     uci commit firewall; \
                     /etc/init.d/firewall reload \
                     {SCHEDULE_TAG}\n"
                ));
            }
        }
    }

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, &new_content).await?;
    Ok(())
}

/// Find the firewall zone name for a profile's interface.
fn find_zone_for_interface(cfgs: &Configs, interface: &str) -> String {
    cfgs["firewall"]
        .sections
        .iter()
        .find_map(|s| {
            let zone = s.get::<FirewallZone>().ok()?;
            zone.network
                .iter()
                .any(|n| n == interface)
                .then(|| zone.name.clone())
        })
        .unwrap_or_else(|| format!("vlan_{interface}"))
}

/// Evaluate current time against all profile schedules and manage UCI firewall
/// REJECT rules for currently-blocked profiles. Adds/removes named `sched_*`
/// sections in the firewall config, then reloads fw3 if anything changed.
pub(crate) async fn evaluate_and_apply_schedules(ctx: &impl CtrlContext) -> Result<(), Error> {
    let now = chrono_now().await;

    let arena = Arena::new();
    let mut cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt", "firewall"]).await?;
    let current_day = now.0; // 0=Sun..6=Sat
    let current_minutes = now.1; // minutes since midnight

    // Collect interfaces that should be blocked right now
    let mut should_block: BTreeSet<String> = BTreeSet::new();
    // Map each profile interface to its egress zone so the REJECT rule blocks
    // the right zone ("wan" or "vpn_<wg_X>").
    let mut iface_egress: BTreeMap<String, String> = BTreeMap::new();

    for section in &cfgs["startwrt"].sections {
        if let Some(profile) = section.get_typed::<UciProfile>()? {
            iface_egress.insert(
                profile.interface.clone(),
                resolve_outbound_zone(profile.outbound.as_deref().unwrap_or("wan")),
            );
            let windows = parse_schedule_windows(&profile.wan_schedule);

            for window in &windows {
                if let (Ok((sh, sm)), Ok((eh, em))) =
                    (parse_hhmm(&window.start_time), parse_hhmm(&window.end_time))
                {
                    let start_min = sh * 60 + sm;
                    let end_min = eh * 60 + em;
                    // Wrap-aware: a window crossing midnight blocks on its own day
                    // (>= start) and the following day (< end), so the after-midnight
                    // tail must be gated on the previous day's mask — not today's.
                    if window_contains(
                        start_min,
                        end_min,
                        &window.days,
                        current_day,
                        current_minutes,
                    ) {
                        should_block.insert(profile.interface.clone());
                        break;
                    }
                }
            }
        }
    }

    // Determine which sched_ sections already exist
    let existing: BTreeSet<String> = cfgs["firewall"]
        .sections
        .iter()
        .filter_map(|s| {
            let name = s.name()?;
            name.strip_prefix(SCHEDULE_RULE_PREFIX)
                .map(|iface| iface.to_string())
        })
        .collect();

    let to_add: BTreeSet<&String> = should_block.difference(&existing).collect();
    let to_remove: BTreeSet<&String> = existing.difference(&should_block).collect();

    if to_add.is_empty() && to_remove.is_empty() {
        return Ok(());
    }

    // Remove stale schedule rules
    if !to_remove.is_empty() {
        cfgs["firewall"].sections.retain(|s| {
            s.name()
                .and_then(|n| n.strip_prefix(SCHEDULE_RULE_PREFIX).map(String::from))
                .map(|iface| !to_remove.contains(&iface))
                .unwrap_or(true)
        });
    }

    // Add missing schedule rules
    for iface in &to_add {
        let zone = find_zone_for_interface(&cfgs, iface);
        let dest_zone = iface_egress
            .get(*iface)
            .cloned()
            .unwrap_or_else(|| DEFAULT_WAN_ZONE.to_string());
        let section_name = format!("{SCHEDULE_RULE_PREFIX}{iface}");
        cfgs["firewall"].append(
            &FirewallRule {
                name: format!("WAN-Schedule-{iface}"),
                src: zone,
                dest: Some(dest_zone),
                target: FirewallTarget::REJECT,
                ..Default::default()
            },
            Some(&section_name),
        )?;
    }

    dump_all(ctx.uci_root(), cfgs).await?;
    drop(arena);

    if ctx.effectful() {
        crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/firewall").arg("reload"))
            .await
            .map_err(|e| Error::new(eyre!("reloading firewall: {e}"), ErrorKind::Network))?;
    }

    Ok(())
}

/// Returns (day_of_week 0=Sun..6=Sat, minutes_since_midnight).
pub(crate) async fn chrono_now() -> (usize, u32) {
    // Use the `date` command for portability on OpenWrt (no chrono crate).
    let output = tokio::process::Command::new("date")
        .args(["+%w %H %M"])
        .invoke(ErrorKind::Filesystem.into())
        .await
        .ok()
        .and_then(|o| String::from_utf8(o).ok())
        .unwrap_or_default();
    let parts: Vec<&str> = output.trim().split_whitespace().collect();
    if parts.len() == 3 {
        let dow = parts[0].parse::<usize>().unwrap_or(0);
        let h = parts[1].parse::<u32>().unwrap_or(0);
        let m = parts[2].parse::<u32>().unwrap_or(0);
        (dow, h * 60 + m)
    } else {
        tracing::warn!("Failed to parse current time from `date` command");
        (0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rpc_toolkit::Context;
    use std::path::PathBuf;

    #[test]
    fn test_window_contains_non_wrap() {
        // 09:00-17:00 on Monday (day 1) only.
        let mut mon = [false; 7];
        mon[1] = true;
        let (s, e) = (9 * 60, 17 * 60);
        assert!(window_contains(s, e, &mon, 1, 12 * 60)); // Mon noon: blocked
        assert!(!window_contains(s, e, &mon, 1, 8 * 60)); // Mon 08:00: before
        assert!(!window_contains(s, e, &mon, 1, 17 * 60)); // Mon 17:00: half-open end
        assert!(!window_contains(s, e, &mon, 2, 12 * 60)); // Tue: wrong day
    }

    #[test]
    fn test_window_contains_wrap() {
        // 22:00-06:00 on Saturday (day 6) only; tail spills into Sunday (day 0).
        let mut sat = [false; 7];
        sat[6] = true;
        let (s, e) = (22 * 60, 6 * 60);
        // Head: Saturday night, gated on Saturday's mask bit.
        assert!(window_contains(s, e, &sat, 6, 23 * 60)); // Sat 23:00: blocked
        assert!(!window_contains(s, e, &sat, 6, 21 * 60)); // Sat 21:00: before start
        // Tail: Sunday morning, gated on the *previous* day (Saturday) — this is
        // the case the old reconciler missed (it checked days[Sunday]).
        assert!(window_contains(s, e, &sat, 0, 2 * 60)); // Sun 02:00: blocked
        assert!(!window_contains(s, e, &sat, 0, 6 * 60)); // Sun 06:00: half-open end
        assert!(!window_contains(s, e, &sat, 0, 8 * 60)); // Sun 08:00: after end
        // Sunday-night must NOT be blocked: neither Sun nor Mon mask is set.
        assert!(!window_contains(s, e, &sat, 0, 23 * 60));
    }

    #[test]
    fn test_window_contains_full_day() {
        // 09:00-09:00 on Monday (day 1): a full 24h window blocking Mon 09:00
        // through Tue 09:00. Equal start/end routes through the wrap branch.
        let mut mon = [false; 7];
        mon[1] = true;
        let (s, e) = (9 * 60, 9 * 60);
        // Head: from 09:00 to midnight on Monday itself.
        assert!(window_contains(s, e, &mon, 1, 12 * 60)); // Mon noon: blocked
        assert!(window_contains(s, e, &mon, 1, 9 * 60)); // Mon 09:00: start, blocked
        assert!(!window_contains(s, e, &mon, 1, 8 * 60)); // Mon 08:00: before start
        // Tail: from midnight to 09:00 on Tuesday, gated on Monday's mask bit.
        assert!(window_contains(s, e, &mon, 2, 2 * 60)); // Tue 02:00: blocked
        assert!(window_contains(s, e, &mon, 2, 8 * 60)); // Tue 08:00: blocked
        assert!(!window_contains(s, e, &mon, 2, 9 * 60)); // Tue 09:00: half-open end
        assert!(!window_contains(s, e, &mon, 2, 10 * 60)); // Tue 10:00: after end
    }
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

    /// Write all five UCI configs used by profile delete.
    /// Two profiles: Admin (lan, vlan 99) and Guest (guest, vlan 101).
    /// The Guest profile has a WiFi station + VLAN section in wireless.
    fn setup_configs(dir: &std::path::Path) {
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

config bridge-vlan
\toption device 'br-lan'
\toption vlan '99'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '101'
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

config rule
\toption name 'Allow-DHCP-DNS-Guest'
\toption src 'vlan_guest'
\toption dest_port '53 67 68'
\tlist proto 'tcp'
\tlist proto 'udp'
\tlist proto 'icmp'
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

        std::fs::write(
            dir.join("wireless"),
            "\
config wifi-device 'radio0'
\toption type 'mac80211'
\toption band '2g'
\toption channel '1'

config wifi-iface 'default_radio0'
\toption device 'radio0'
\toption mode 'ap'
\toption ssid 'TestNet'
\toption encryption 'psk2'
\toption key 'adminpass1'
\toption dynamic_vlan '1'

config wifi-station
\toption key 'guestpass1'
\toption vid '101'
\toption iface 'default_radio0'
\toption label 'Guest WiFi'

config wifi-vlan
\toption name 'guest'
\toption network 'guest'
\toption vid '101'
\toption iface 'default_radio0'
",
        )
        .unwrap();
    }

    #[tokio::test]
    async fn test_delete_removes_wireless_sections() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp", "wireless"],
        )
        .await
        .unwrap();

        // Verify wireless sections exist before delete
        let mut station_count = 0;
        let mut vlan_count = 0;
        for section in &cfgs["wireless"].sections {
            if let Ok(s) = section.get::<WifiStation>() {
                if s.vid == Some(101) {
                    station_count += 1;
                }
            }
            if let Ok(v) = section.get::<WifiVlan>() {
                if v.vid == 101 {
                    vlan_count += 1;
                }
            }
        }
        assert_eq!(station_count, 1, "expected 1 WifiStation with vid=101 before delete");
        assert_eq!(vlan_count, 1, "expected 1 WifiVlan with vid=101 before delete");

        // Delete the guest profile
        delete_config(
            ctx,
            &mut cfgs,
            &ProfileIdOpt {
                fullname: None,
                interface: Some("guest".into()),
                vlan_tag: None,
            },
        )
        .unwrap();

        // WifiStation and WifiVlan for vid=101 must be gone
        for section in &cfgs["wireless"].sections {
            if let Ok(s) = section.get::<WifiStation>() {
                assert_ne!(
                    s.vid,
                    Some(101),
                    "WifiStation with vid=101 should have been removed"
                );
            }
            if let Ok(v) = section.get::<WifiVlan>() {
                assert_ne!(v.vid, 101, "WifiVlan with vid=101 should have been removed");
            }
        }

        // wifi-device and wifi-iface sections must survive
        let remaining_types: Vec<String> = cfgs["wireless"]
            .sections
            .iter()
            .map(|s| s.ty().to_string())
            .collect();
        assert!(
            remaining_types.contains(&"wifi-device".to_string()),
            "wifi-device should survive delete"
        );
        assert!(
            remaining_types.contains(&"wifi-iface".to_string()),
            "wifi-iface should survive delete"
        );
    }

    #[tokio::test]
    async fn test_delete_preserves_other_profile_wireless() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        // Add a second profile "kids" with vlan 102 and its own wireless sections
        std::fs::write(
            dir.path().join("startwrt"),
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

config profile kids
\toption fullname 'Kids'
\toption interface 'kids'
\toption vlan_tag '102'
",
        )
        .unwrap();

        // Append kids wireless sections alongside guest's
        std::fs::write(
            dir.path().join("wireless"),
            "\
config wifi-device 'radio0'
\toption type 'mac80211'
\toption band '2g'
\toption channel '1'

config wifi-iface 'default_radio0'
\toption device 'radio0'
\toption mode 'ap'
\toption ssid 'TestNet'
\toption encryption 'psk2'
\toption key 'adminpass1'
\toption dynamic_vlan '1'

config wifi-station
\toption key 'guestpass1'
\toption vid '101'
\toption iface 'default_radio0'

config wifi-vlan
\toption name 'guest'
\toption network 'guest'
\toption vid '101'
\toption iface 'default_radio0'

config wifi-station
\toption key 'kidspass1'
\toption vid '102'
\toption iface 'default_radio0'

config wifi-vlan
\toption name 'kids'
\toption network 'kids'
\toption vid '102'
\toption iface 'default_radio0'
",
        )
        .unwrap();

        // Also need network/firewall/dhcp entries for kids so Lookup doesn't fail
        std::fs::write(
            dir.path().join("network"),
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

config interface 'kids'
\toption device 'br-lan.102'
\toption proto 'static'
\toption ipaddr '192.168.102.1'
\toption netmask '255.255.255.0'
",
        )
        .unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp", "wireless"],
        )
        .await
        .unwrap();

        // Delete guest (vlan 101)
        delete_config(
            ctx,
            &mut cfgs,
            &ProfileIdOpt {
                fullname: None,
                interface: Some("guest".into()),
                vlan_tag: None,
            },
        )
        .unwrap();

        // Kids wireless sections (vid=102) must survive
        let mut found_kids_station = false;
        let mut found_kids_vlan = false;
        for section in &cfgs["wireless"].sections {
            if let Ok(s) = section.get::<WifiStation>() {
                assert_ne!(s.vid, Some(101), "guest station should be gone");
                if s.vid == Some(102) {
                    found_kids_station = true;
                }
            }
            if let Ok(v) = section.get::<WifiVlan>() {
                assert_ne!(v.vid, 101, "guest vlan should be gone");
                if v.vid == 102 {
                    found_kids_vlan = true;
                }
            }
        }
        assert!(found_kids_station, "kids WifiStation (vid=102) should survive");
        assert!(found_kids_vlan, "kids WifiVlan (vid=102) should survive");
    }

    #[tokio::test]
    async fn test_delete_cannot_delete_lan_owner() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp", "wireless"],
        )
        .await
        .unwrap();

        let result = delete_config(
            ctx,
            &mut cfgs,
            &ProfileIdOpt {
                fullname: None,
                interface: Some("lan".into()),
                vlan_tag: None,
            },
        );
        assert!(result.is_err(), "deleting lan owner should fail");
    }

    #[tokio::test]
    async fn test_set_config_prefers_br_lan_bridge() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        // Network with two bridges: br-guest first, then br-lan
        std::fs::write(
            dir.path().join("network"),
            "\
config device
\toption name 'br-guest'
\toption type 'bridge'
\tlist ports 'eth1'

config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'

config interface 'lan'
\toption device 'br-lan.99'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
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
\toption access_to_new_profiles '1'

config profile newp
\toption fullname 'NewProfile'
\toption interface 'newp'
\toption vlan_tag '200'
",
        )
        .unwrap();

        std::fs::write(
            dir.path().join("firewall"),
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

config forwarding
\toption src 'lan'
\toption dest 'wan'
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
",
        )
        .unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // set_config for "newp" — interface doesn't exist yet, so it must
        // pick a bridge. With the fix, it should prefer br-lan over br-guest.
        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("NewProfile".into()),
                interface: Some("newp".into()),
                vlan_tag: Some(200),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 200, 1),
            outbound: "wan".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };

        set_config(ctx.clone(), &mut cfgs, &profile).unwrap();

        // Read the newly created interface directly — no dump+parse round-trip needed.
        let found_device = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("newp"))
            .and_then(|s| s.get::<NetworkInterface>().ok())
            .map(|iface| iface.device);
        assert_eq!(
            found_device.as_deref(),
            Some("br-lan.200"),
            "set_config should prefer br-lan even when br-guest appears first"
        );
    }

    #[tokio::test]
    async fn set_config_rejects_out_of_block_subnet() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path()); // LAN is 192.168.1.1

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // Non-admin profile whose subnet escapes the LAN's 192.168.0.0/16 block.
        let escaping = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(10, 0, 5, 1),
            outbound: "wan".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };
        let err = set_config(ctx.clone(), &mut cfgs, &escaping).unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::InvalidRequest,
            "non-admin subnet outside the LAN /16 must be rejected"
        );

        // The admin profile defines the block, so it must itself be a valid
        // RFC 1918 selection — this is a second path that sets the LAN /16.
        let bad_admin = Profile {
            id: ProfileIdOpt {
                fullname: Some("Admin".into()),
                interface: Some("lan".into()),
                vlan_tag: Some(99),
            },
            gateway_ip: Ipv4Addr::new(8, 8, 8, 8),
            outbound: "wan".into(),
            lan_access: LanAccess::All,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: true,
            owns_lan: true,
        };
        let err = set_config(ctx.clone(), &mut cfgs, &bad_admin).unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::InvalidRequest,
            "admin gateway outside RFC 1918 ranges must be rejected"
        );
    }

    #[tokio::test]
    async fn validate_profile_block_accepts_alternate_rfc1918_block() {
        // A LAN in 10.42.0.0/16 must accept siblings within that block and an
        // admin keeping a valid RFC 1918 selection, while still rejecting a
        // sibling that leaves the block. Locks in that non-192.168 blocks pass.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("network"),
            "\
config interface 'lan'
\toption device 'br-lan'
\toption proto 'static'
\toption ipaddr '10.42.1.1'
\toption netmask '255.255.255.0'
",
        )
        .unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["network"]).await.unwrap();

        let make = |gw: Ipv4Addr, owns_lan: bool| Profile {
            id: ProfileIdOpt {
                fullname: Some("X".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: gw,
            outbound: "wan".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan,
        };

        // Sibling inside the LAN's 10.42.0.0/16 — accepted.
        validate_profile_block(&cfgs, &make(Ipv4Addr::new(10, 42, 5, 1), false))
            .expect("sibling in the LAN /16 should be accepted");
        // Admin keeping a valid RFC 1918 selection — accepted.
        validate_profile_block(&cfgs, &make(Ipv4Addr::new(10, 42, 1, 1), true))
            .expect("admin in a valid RFC 1918 block should be accepted");
        // Sibling escaping the block — still rejected.
        let err = validate_profile_block(&cfgs, &make(Ipv4Addr::new(192, 168, 5, 1), false))
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidRequest);
    }

    #[tokio::test]
    async fn guard_subnet_collision_detects_and_allows() {
        // lan=10.0.0.1, guest=10.0.5.1. No profile (including the admin LAN) may
        // land on another's /24, but a free /24 and a no-op self-edit both pass.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("network"),
            "\
config interface 'lan'
\toption device 'br-lan.1'
\toption proto 'static'
\toption ipaddr '10.0.0.1'
\toption netmask '255.255.255.0'

config interface 'guest'
\toption device 'br-lan.101'
\toption proto 'static'
\toption ipaddr '10.0.5.1'
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
\toption vlan_tag '101'
",
        )
        .unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt", "network"])
            .await
            .unwrap();

        // New profile onto the guest's /24 — rejected.
        let err = guard_subnet_collision(&cfgs, Ipv4Addr::new(10, 0, 5, 1), None).unwrap_err();
        assert_eq!(err.kind, ErrorKind::SubnetCollision);

        // New profile onto the admin LAN's /24 — also rejected.
        let err = guard_subnet_collision(&cfgs, Ipv4Addr::new(10, 0, 0, 1), None).unwrap_err();
        assert_eq!(err.kind, ErrorKind::SubnetCollision);

        // Editing the admin LAN onto the guest's /24 — rejected (the reported bug).
        let err =
            guard_subnet_collision(&cfgs, Ipv4Addr::new(10, 0, 5, 1), Some("lan")).unwrap_err();
        assert_eq!(err.kind, ErrorKind::SubnetCollision);

        // A free /24 — accepted.
        guard_subnet_collision(&cfgs, Ipv4Addr::new(10, 0, 9, 1), None)
            .expect("an unused /24 must be accepted");

        // No-op edits: a profile keeping its own /24 is skipped via self_interface.
        guard_subnet_collision(&cfgs, Ipv4Addr::new(10, 0, 5, 1), Some("guest"))
            .expect("a profile keeping its own subnet must be accepted");
        guard_subnet_collision(&cfgs, Ipv4Addr::new(10, 0, 0, 1), Some("lan"))
            .expect("the admin keeping its own subnet must be accepted");
    }

    #[tokio::test]
    async fn guard_subnet_collision_allows_noop_edit_of_already_broken_config() {
        // Two profiles already share a /24 (the stranded state). Editing one
        // without changing its subnet (e.g. a DNS change) must NOT be blocked —
        // the guard only prevents *introducing* a collision, so recovery edits
        // stay possible.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("network"),
            "\
config interface 'lan'
\toption device 'br-lan.1'
\toption proto 'static'
\toption ipaddr '10.0.15.1'
\toption netmask '255.255.255.0'

config interface 'guest'
\toption device 'br-lan.101'
\toption proto 'static'
\toption ipaddr '10.0.15.1'
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
\toption vlan_tag '101'
",
        )
        .unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt", "network"])
            .await
            .unwrap();

        // No-op edit of the admin (subnet unchanged) — allowed despite the
        // pre-existing collision with guest.
        guard_subnet_collision(&cfgs, Ipv4Addr::new(10, 0, 15, 1), Some("lan"))
            .expect("a no-op subnet edit must pass even when the config is already colliding");
        // Moving the admin off the collision to a free /24 — allowed (recovery).
        guard_subnet_collision(&cfgs, Ipv4Addr::new(10, 0, 20, 1), Some("lan"))
            .expect("moving off a collision to a free /24 must be accepted");
    }

    #[tokio::test]
    async fn test_whitelist_firewall_rules() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wan".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::Whitelist(vec!["1.1.1.1/32".into(), "8.8.8.8/32".into()]),
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };

        set_config(ctx, &mut cfgs, &profile).unwrap();

        // Should have a forwarding to wan (needed for rule matching)
        let has_wan_fwd = cfgs["firewall"].sections.iter().any(|s| {
            s.get::<FirewallForwarding>()
                .map(|f| f.src == "vlan_guest" && f.dest == DEFAULT_WAN_ZONE)
                .unwrap_or(false)
        });
        assert!(has_wan_fwd, "whitelist should create forwarding to wan");

        // Should have ACCEPT rules for each destination
        let accept_rules: Vec<_> = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<FirewallRule>().ok())
            .filter(|r| {
                r.src == "vlan_guest"
                    && r.dest.as_deref() == Some(DEFAULT_WAN_ZONE)
                    && r.target == FirewallTarget::ACCEPT
                    && r.dest_ip.is_some()
            })
            .collect();
        assert_eq!(accept_rules.len(), 2, "should have 2 ACCEPT rules for whitelisted IPs");
        assert_eq!(accept_rules[0].dest_ip.as_deref(), Some("1.1.1.1/32"));
        assert_eq!(accept_rules[1].dest_ip.as_deref(), Some("8.8.8.8/32"));

        // Should have a catch-all REJECT rule
        let reject_rule = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<FirewallRule>().ok())
            .find(|r| {
                r.src == "vlan_guest"
                    && r.dest.as_deref() == Some(DEFAULT_WAN_ZONE)
                    && r.target == FirewallTarget::REJECT
                    && r.dest_ip.is_none()
            });
        assert!(reject_rule.is_some(), "whitelist should have a catch-all REJECT rule");
    }

    #[tokio::test]
    async fn test_blacklist_firewall_rules() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wan".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::Blacklist(vec!["10.0.0.0/8".into()]),
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };

        set_config(ctx, &mut cfgs, &profile).unwrap();

        // Should have forwarding to wan (default allow)
        let has_wan_fwd = cfgs["firewall"].sections.iter().any(|s| {
            s.get::<FirewallForwarding>()
                .map(|f| f.src == "vlan_guest" && f.dest == DEFAULT_WAN_ZONE)
                .unwrap_or(false)
        });
        assert!(has_wan_fwd, "blacklist should create forwarding to wan");

        // Should have REJECT rules for blocked destinations
        let reject_rules: Vec<_> = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<FirewallRule>().ok())
            .filter(|r| {
                r.src == "vlan_guest"
                    && r.dest.as_deref() == Some(DEFAULT_WAN_ZONE)
                    && r.target == FirewallTarget::REJECT
                    && r.dest_ip.is_some()
            })
            .collect();
        assert_eq!(reject_rules.len(), 1, "should have 1 REJECT rule for blacklisted IP");
        assert_eq!(reject_rules[0].dest_ip.as_deref(), Some("10.0.0.0/8"));

        // Should NOT have a catch-all reject (blacklist allows everything else)
        let catchall_reject = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<FirewallRule>().ok())
            .find(|r| {
                r.src == "vlan_guest"
                    && r.dest.as_deref() == Some(DEFAULT_WAN_ZONE)
                    && r.target == FirewallTarget::REJECT
                    && r.dest_ip.is_none()
            });
        assert!(catchall_reject.is_none(), "blacklist should NOT have a catch-all REJECT");
    }

    #[tokio::test]
    async fn test_wan_access_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // Set guest profile with whitelist
        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wan".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::Whitelist(vec!["1.1.1.1/32".into(), "8.8.8.8/32".into()]),
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };

        set_config(ctx.clone(), &mut cfgs, &profile).unwrap();

        // Dump and re-parse to simulate a real round-trip
        dump_all(ctx.uci_root(), cfgs).await.unwrap();
        let arena2 = Arena::new();
        let cfgs2 = parse_all(
            ctx.uci_root(),
            &arena2,
            &["startwrt", "network", "firewall"],
        )
        .await
        .unwrap();

        let result = get_config(
            ctx,
            &cfgs2,
            ProfileIdOpt {
                fullname: None,
                interface: Some("guest".into()),
                vlan_tag: None,
            },
        )
        .unwrap();

        match &result.wan_access {
            WanAccess::Whitelist(dests) => {
                assert_eq!(dests, &["1.1.1.1/32", "8.8.8.8/32"]);
            }
            other => panic!("expected Whitelist, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_set_persists_outbound() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // Set guest profile with non-default outbound
        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wg0".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };

        set_config(ctx.clone(), &mut cfgs, &profile).unwrap();

        // Dump and re-parse
        dump_all(ctx.uci_root(), cfgs).await.unwrap();
        let arena2 = Arena::new();
        let cfgs2 = parse_all(
            ctx.uci_root(),
            &arena2,
            &["startwrt", "network", "firewall"],
        )
        .await
        .unwrap();

        let result = get_config(
            ctx,
            &cfgs2,
            ProfileIdOpt {
                fullname: None,
                interface: Some("guest".into()),
                vlan_tag: None,
            },
        )
        .unwrap();

        assert_eq!(result.outbound, "wg0", "outbound should persist through set_config");
    }

    #[tokio::test]
    async fn test_set_config_preserves_admin_lan_prefix() {
        // The admin LAN's ip6assign prefix is owned by lan::ipv6_set (the LAN
        // IPv6 page). Editing the admin profile must not clobber it. Regression
        // for the /60 → /64 reset.
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // Seed the LAN with a user-configured /60 and enable IPv6 (ra=server).
        // With IPv6 enabled the old code reset ip6assign to "64".
        for section in &mut cfgs["network"].sections {
            if section.name().as_deref() == Some("lan") {
                let mut iface = section.get_typed::<NetworkInterface>().unwrap().unwrap();
                iface.ip6assign = Some("60".into());
                section.set(&iface).unwrap();
            }
        }
        for section in &mut cfgs["dhcp"].sections {
            if section.name().as_deref() == Some("lan") {
                let mut dhcp = section.get_typed::<Dhcp>().unwrap().unwrap();
                dhcp.ra = Some("server".into());
                section.set(&dhcp).unwrap();
            }
        }

        // Edit the admin profile (owns_lan).
        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Admin".into()),
                interface: Some("lan".into()),
                vlan_tag: Some(99),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 1, 1),
            outbound: "wan".into(),
            lan_access: LanAccess::All,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: true,
            owns_lan: true,
        };
        set_config(ctx.clone(), &mut cfgs, &profile).unwrap();

        let lan = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("lan"))
            .and_then(|s| s.get_typed::<NetworkInterface>().ok().flatten())
            .expect("lan interface");
        assert_eq!(
            lan.ip6assign.as_deref(),
            Some("60"),
            "editing the admin profile must preserve the LAN's configured ip6assign"
        );
    }

    #[tokio::test]
    async fn test_outbound_vpn_creates_routing() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // Set guest profile with VPN outbound
        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wg_test".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };

        set_config(ctx, &mut cfgs, &profile).unwrap();

        // Verify route section was created
        let route = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("prt_guest"))
            .expect("route section prt_guest should exist");
        let route_data = route.get::<NetworkRoute>().unwrap();
        assert_eq!(route_data.interface, "wg_test");
        assert_eq!(route_data.target, "0.0.0.0/0");
        assert_eq!(route_data.table, Some(101));

        // Verify rule section was created
        let rule = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("prr_guest"))
            .expect("rule section prr_guest should exist");
        let rule_data = rule.get::<NetworkRule>().unwrap();
        assert_eq!(rule_data.src.as_deref(), Some("192.168.101.0/24"));
        assert_eq!(rule_data.lookup, 101);
        assert_eq!(rule_data.priority, Some(200));

        // Verify dnat_return ip rule was created for port-forward reply routing
        let dnat_rule = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("dnat_return"))
            .expect("dnat_return rule should exist when VPN routing is active");
        let dnat_data = dnat_rule.get::<NetworkRule>().unwrap();
        assert_eq!(dnat_data.mark.as_deref(), Some("0x80/0x80"));
        assert_eq!(dnat_data.lookup, 254);
        assert_eq!(dnat_data.priority, Some(100));
        assert!(dnat_data.src.is_none());

        // The DNAT-return mark is no longer a per-profile UCI firewall rule —
        // it moved to a static nftables chain (/etc/nftables.d/10-startwrt-dnat-mark.nft)
        // because fw4 has no UCI option for `ct status dnat` matching.
        assert!(
            !cfgs["firewall"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("dnat_mark_guest")),
            "per-profile dnat_mark rule should no longer be written (moved to nftables.d)"
        );

        // The VPN outbound lives in a dedicated vpn_<wg> zone with masq/masq6,
        // and the profile's wan-access forwarding targets that zone (not wan).
        let vpn_zone = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<FirewallZone>().ok())
            .find(|z| z.name == "vpn_wg_test")
            .expect("vpn_wg_test zone should exist");
        assert_eq!(vpn_zone.network, vec!["wg_test".to_string()]);
        assert_eq!(vpn_zone.masq, Some(true));
        assert_eq!(vpn_zone.masq6, Some(true));

        let fwd_to_vpn = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<FirewallForwarding>().ok())
            .any(|f| f.src == "vlan_guest" && f.dest == "vpn_wg_test");
        assert!(
            fwd_to_vpn,
            "VPN-routed profile forwarding must target the vpn_<wg> zone"
        );
    }

    #[tokio::test]
    async fn test_rewrite_routing_v4_killswitch_fallback() {
        // A VPN-routed profile gets both the `dev <wg>` v4 default (metric 1)
        // and an `unreachable` fallback on loopback (metric 2048) so v4 fails
        // closed if the WG interface goes down instead of falling through to WAN.
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wg_test".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };

        set_config(ctx, &mut cfgs, &profile).unwrap();

        // dev-wg default: metric 1, no special type.
        let dev = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("prt_guest"))
            .expect("prt_guest should exist")
            .get::<NetworkRoute>()
            .unwrap();
        assert_eq!(dev.interface, "wg_test");
        assert_eq!(dev.target, "0.0.0.0/0");
        assert_eq!(dev.metric, Some(VPN_DEFAULT_ROUTE_METRIC));
        assert!(dev.kind.is_none());

        // Fallback: unreachable on loopback, higher metric, same table.
        let fallback = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("prtb_guest"))
            .expect("prtb_guest kill-switch fallback should exist")
            .get::<NetworkRoute>()
            .unwrap();
        assert_eq!(fallback.target, "0.0.0.0/0");
        assert_eq!(fallback.interface, "loopback");
        assert_eq!(fallback.kind.as_deref(), Some("unreachable"));
        assert_eq!(fallback.table, Some(101));
        assert_eq!(fallback.metric, Some(VPN_KILLSWITCH_METRIC));
    }

    #[tokio::test]
    async fn test_outbound_wan_no_routing() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // Set guest profile with WAN outbound
        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wan".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };

        set_config(ctx, &mut cfgs, &profile).unwrap();

        // No route or rule sections should exist
        let has_route = cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("prt_guest"));
        let has_rule = cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("prr_guest"));
        assert!(!has_route, "WAN outbound should not create route");
        assert!(!has_rule, "WAN outbound should not create rule");
    }

    #[tokio::test]
    async fn test_outbound_change_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // First: set guest to VPN
        let profile_vpn = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wg_test".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };
        set_config(ctx.clone(), &mut cfgs, &profile_vpn).unwrap();

        // Verify route exists
        assert!(
            cfgs["network"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("prt_guest")),
            "route should exist after VPN assignment"
        );

        // Now switch back to WAN
        let profile_wan = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wan".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };
        set_config(ctx, &mut cfgs, &profile_wan).unwrap();

        // Route and rule should be cleaned up
        assert!(
            !cfgs["network"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("prt_guest")),
            "route should be removed after switching to WAN"
        );
        assert!(
            !cfgs["network"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("prr_guest")),
            "rule should be removed after switching to WAN"
        );
    }

    #[tokio::test]
    async fn test_dnat_return_rule_recreated_from_constants() {
        // A stale dnat_return (an old priority/mark from a prior release, or a
        // manual edit) must be rewritten from the current constants — a
        // name-only existence check would leave it in place forever.
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // Pre-seed a stale dnat_return rule with wrong mark/lookup/priority.
        cfgs["network"]
            .append(
                &NetworkRule {
                    src: None,
                    lookup: 99,
                    mark: Some("0xdead/0xdead".into()),
                    priority: Some(999),
                },
                Some("dnat_return"),
            )
            .unwrap();

        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wg_test".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };
        set_config(ctx, &mut cfgs, &profile).unwrap();

        let dnat_rules: Vec<_> = cfgs["network"]
            .sections
            .iter()
            .filter(|s| s.name().as_deref() == Some("dnat_return"))
            .collect();
        assert_eq!(
            dnat_rules.len(),
            1,
            "stale dnat_return must be replaced, not duplicated"
        );
        let data = dnat_rules[0].get::<NetworkRule>().unwrap();
        assert_eq!(data.mark.as_deref(), Some("0x80/0x80"));
        assert_eq!(data.lookup, 254);
        assert_eq!(data.priority, Some(100));
        assert!(data.src.is_none());
    }

    #[tokio::test]
    async fn test_dnat_return_rules_removed_when_no_vpn_profile() {
        // The global dnat_return / dnat_return6 ip rules should exist iff at
        // least one VPN-routed profile does. Switching the last VPN profile back
        // to WAN must tear them down.
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let profile_vpn = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wg_test".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };
        set_config(ctx.clone(), &mut cfgs, &profile_vpn).unwrap();
        assert!(
            cfgs["network"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("dnat_return")),
            "dnat_return should exist while a VPN profile is active"
        );

        // Switch the only VPN-routed profile back to WAN.
        let profile_wan = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wan".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };
        set_config(ctx, &mut cfgs, &profile_wan).unwrap();

        assert!(
            !cfgs["network"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("dnat_return")),
            "dnat_return must be removed once no VPN profile remains"
        );
        assert!(
            !cfgs["network"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("dnat_return6")),
            "dnat_return6 must be removed once no VPN profile remains"
        );
    }

    #[tokio::test]
    async fn test_outbound_vpn_creates_dedicated_zone() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wg_mullvad".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };

        set_config(ctx, &mut cfgs, &profile).unwrap();

        // VPN interface lives in a dedicated vpn_<wg> zone (NOT the wan zone),
        // with masq + masq6 so NAT/NAT66 applies on VPN egress while wan6 stays
        // untouched.
        let vpn_zone = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<FirewallZone>().ok())
            .find(|z| z.name == "vpn_wg_mullvad")
            .expect("vpn_wg_mullvad zone should exist");
        assert_eq!(vpn_zone.network, vec!["wg_mullvad".to_string()]);
        assert_eq!(vpn_zone.masq, Some(true));
        assert_eq!(vpn_zone.masq6, Some(true));

        // WAN zone must NOT contain the VPN interface.
        let wan_zone = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<FirewallZone>().ok())
            .find(|z| z.name == DEFAULT_WAN_ZONE)
            .expect("WAN zone should exist");
        assert!(
            !wan_zone.network.contains(&"wg_mullvad".to_string()),
            "VPN interface must not be in the WAN zone, got: {:?}",
            wan_zone.network
        );
    }

    #[tokio::test]
    async fn test_reapply_restores_wan_forwarding_after_vpn_reset() {
        // Reproduces the VPN-delete/disable cleanup bug: a profile routed through
        // a VPN, once its outbound is reset to "wan", must get its firewall
        // forwarding rebuilt as `<zone> → wan` (not left pointing at the removed
        // vpn_<wg> zone) and its kill-switch routes removed — otherwise all WAN
        // traffic is dropped.
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs_with_ipv6_vpn(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // 1. Route the guest profile through a v6-capable VPN.
        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wg_v6".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };
        set_config(ctx.clone(), &mut cfgs, &profile).unwrap();

        // Sanity: forwarding now targets the dedicated vpn zone, kill switch present.
        let fwd_to_vpn = |cfgs: &Configs| {
            cfgs["firewall"]
                .sections
                .iter()
                .filter_map(|s| s.get::<FirewallForwarding>().ok())
                .any(|f| f.src == "vlan_guest" && f.dest == "vpn_wg_v6")
        };
        assert!(fwd_to_vpn(&cfgs), "precondition: vlan_guest → vpn_wg_v6 forwarding");
        assert!(
            cfgs["network"].sections.iter().any(|s| s.name().as_deref() == Some("prt6b_guest")),
            "precondition: v6 kill switch present"
        );

        // 2. Simulate the VPN delete/disable reset: outbound → "wan" in UCI
        //    (what reset_profiles_using_vpn does), then the fixed cleanup path.
        for section in &mut cfgs["startwrt"].sections {
            if let Some(mut p) = section.get_typed::<UciProfile>().unwrap() {
                if p.interface == "guest" {
                    p.outbound = Some("wan".to_string());
                    section.set(&p).unwrap();
                }
            }
        }
        reapply_profile_config(
            &ctx,
            &mut cfgs,
            ProfileIdOpt { fullname: None, interface: Some("guest".into()), vlan_tag: None },
        )
        .unwrap();
        cleanup_orphaned_vpn_zones(&mut cfgs);

        // 3. WAN forwarding restored; vpn zone + its forwarding + kill switch gone.
        assert!(
            cfgs["firewall"]
                .sections
                .iter()
                .filter_map(|s| s.get::<FirewallForwarding>().ok())
                .any(|f| f.src == "vlan_guest" && f.dest == "wan"),
            "vlan_guest → wan forwarding must be restored"
        );
        assert!(!fwd_to_vpn(&cfgs), "stale vlan_guest → vpn_wg_v6 forwarding must be gone");
        assert!(
            !cfgs["firewall"]
                .sections
                .iter()
                .filter_map(|s| s.get::<FirewallZone>().ok())
                .any(|z| z.name == "vpn_wg_v6"),
            "orphaned vpn_wg_v6 zone must be removed"
        );
        for name in &["prt_guest", "prtb_guest", "prr_guest", "prt6_guest", "prt6b_guest", "prr6_guest", "prl6_guest"] {
            assert!(
                !cfgs["network"].sections.iter().any(|s| s.name().as_deref() == Some(*name)),
                "{name} kill-switch/policy section must be removed after reset to wan"
            );
        }
    }

    #[tokio::test]
    async fn test_delete_profile_removes_routing() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        // Set guest outbound to VPN first
        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wg_test".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };
        set_config(ctx.clone(), &mut cfgs, &profile).unwrap();
        dump_all(ctx.uci_root(), cfgs).await.unwrap();

        // Now delete the guest profile
        let arena2 = Arena::new();
        let mut cfgs2 = parse_all(
            ctx.uci_root(),
            &arena2,
            &["startwrt", "network", "firewall", "dhcp", "wireless"],
        )
        .await
        .unwrap();

        delete_config(
            ctx,
            &mut cfgs2,
            &ProfileIdOpt {
                fullname: None,
                interface: Some("guest".into()),
                vlan_tag: None,
            },
        )
        .unwrap();

        // Route and rule should be gone
        assert!(
            !cfgs2["network"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("prt_guest")),
            "route should be removed on profile delete"
        );
        assert!(
            !cfgs2["network"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("prr_guest")),
            "rule should be removed on profile delete"
        );
    }

    #[tokio::test]
    async fn test_dns_override_creates_redirect() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wan".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: vec![DnsServer { address: "1.1.1.1".into(), ssl: false }],
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };

        set_config(ctx, &mut cfgs, &profile).unwrap();

        let redirects: Vec<_> = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<FirewallRedirect>().ok())
            .filter(|r| r.name.contains("DNS-Override") && r.src == "vlan_guest")
            .collect();
        assert_eq!(redirects.len(), 1, "should have 1 DNS override redirect");
        assert_eq!(redirects[0].dest.as_deref(), None);
        assert_eq!(redirects[0].dest_ip.as_deref(), Some("192.168.101.1"));
        assert_eq!(redirects[0].dest_port.as_deref(), Some("53"));
        assert_eq!(redirects[0].src_dport.as_deref(), Some("53"));
        assert_eq!(redirects[0].target, "DNAT");
        assert_eq!(redirects[0].proto, vec!["tcp", "udp"]);
    }

    #[tokio::test]
    async fn test_dns_override_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wan".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: vec![
                DnsServer { address: "1.1.1.1".into(), ssl: false },
                DnsServer { address: "8.8.8.8".into(), ssl: false },
            ],
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };

        set_config(ctx.clone(), &mut cfgs, &profile).unwrap();
        dump_all(ctx.uci_root(), cfgs).await.unwrap();

        // Re-parse and read back via get_config
        let arena2 = Arena::new();
        let cfgs2 = parse_all(
            ctx.uci_root(),
            &arena2,
            &["startwrt", "network", "firewall"],
        )
        .await
        .unwrap();

        let got = get_config(
            ctx,
            &cfgs2,
            ProfileIdOpt {
                fullname: None,
                interface: Some("guest".into()),
                vlan_tag: None,
            },
        )
        .unwrap();

        assert_eq!(
            got.dns_override,
            vec![
                DnsServer { address: "1.1.1.1".into(), ssl: false },
                DnsServer { address: "8.8.8.8".into(), ssl: false },
            ],
            "dns_override should round-trip through UCI"
        );
    }

    #[tokio::test]
    async fn test_dns_override_empty_no_redirect() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wan".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };

        set_config(ctx, &mut cfgs, &profile).unwrap();

        let redirect_count = cfgs["firewall"]
            .sections
            .iter()
            .filter(|s| {
                s.get::<FirewallRedirect>()
                    .map(|r| r.name.contains("DNS-Override"))
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(
            redirect_count, 0,
            "empty dns_override should not create any redirect sections"
        );
    }

    #[tokio::test]
    async fn test_delete_removes_vpn_server_resources() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        use crate::wg::{Base64, WgKey};
        let server_key = Base64::new(WgKey::generate()).to_base64();
        let peer_key = Base64::new(WgKey::generate()).to_base64();

        // Write configs with VPN server resources for Guest profile
        std::fs::write(
            dir.path().join("startwrt"),
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
\toption device 'br-lan.99'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.255.0'

config interface 'guest'
\toption device 'br-lan.101'
\toption proto 'static'
\toption ipaddr '192.168.101.1'
\toption netmask '255.255.255.0'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '99'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '101'

config interface 'wg_guest'
\toption proto 'wireguard'
\toption private_key '{server_key}'
\toption listen_port '51820'
\tlist addresses '192.168.101.254/32'

config wireguard_wg_guest 'wg_guest_0'
\toption public_key '{peer_key}'
\toption description 'Phone'
\toption persistent_keepalive '25'
\toption route_allowed_ips '1'
\tlist allowed_ips '192.168.101.200/32'
"
            ),
        )
        .unwrap();

        std::fs::write(
            dir.path().join("firewall"),
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

config rule
\toption name 'Allow-DHCP-DNS-Guest'
\toption src 'vlan_guest'
\toption dest_port '53 67 68'
\tlist proto 'tcp'
\tlist proto 'udp'
\tlist proto 'icmp'
\toption target 'ACCEPT'

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
            dir.path().join("dhcp"),
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

        std::fs::write(
            dir.path().join("wireless"),
            "\
config wifi-device 'radio0'
\toption type 'mac80211'
\toption band '2g'
\toption channel '1'

config wifi-iface 'default_radio0'
\toption device 'radio0'
\toption mode 'ap'
\toption ssid 'TestNet'
\toption encryption 'psk2'
\toption key 'adminpass1'
\toption dynamic_vlan '1'
",
        )
        .unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp", "wireless"],
        )
        .await
        .unwrap();

        // Delete Guest profile
        delete_config(
            ctx,
            &mut cfgs,
            &ProfileIdOpt {
                fullname: None,
                interface: Some("guest".into()),
                vlan_tag: None,
            },
        )
        .unwrap();

        // 1. No vpn_server section with interface=wg_guest in startwrt
        let has_vpn_meta = cfgs["startwrt"]
            .sections
            .iter()
            .any(|s| s.ty() == "vpn_server");
        assert!(!has_vpn_meta, "vpn_server metadata should be removed");

        // 2. No wg_guest interface section in network
        let has_wg_iface = cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("wg_guest"));
        assert!(!has_wg_iface, "wg_guest interface should be removed");

        // 3. No wireguard_wg_guest peer sections in network
        let has_peers = cfgs["network"]
            .sections
            .iter()
            .any(|s| s.ty() == "wireguard_wg_guest");
        assert!(!has_peers, "wireguard peer sections should be removed");

        // 4. No allow_wireguard_wg_guest rule in firewall (the specific bug that was fixed)
        let has_wg_rule = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<FirewallRule>().ok())
            .any(|r| r.name == "Allow-WireGuard-wg_guest");
        assert!(
            !has_wg_rule,
            "WireGuard WAN firewall rule should be removed on profile deletion"
        );

        // 5. Admin profile resources untouched
        let admin_exists = cfgs["startwrt"]
            .sections
            .iter()
            .any(|s| s.ty() == "profile");
        assert!(admin_exists, "Admin profile should survive");

        let admin_iface = cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("lan"));
        assert!(admin_iface, "Admin network interface should survive");
    }

    #[tokio::test]
    async fn test_collect_smartdns_groups_with_profile_dns() {
        let dir = tempfile::tempdir().unwrap();
        // Create startwrt config with a profile that has dns_override
        std::fs::write(
            dir.path().join("startwrt"),
            "\
config profile lan
\toption fullname 'Admin'
\toption interface 'lan'
\toption vlan_tag '1'

config profile guest
\toption fullname 'Guest'
\toption interface 'guest'
\toption vlan_tag '101'
\tlist dns_override '{\"address\":\"1.1.1.1\",\"ssl\":false}'
\tlist dns_override '{\"address\":\"8.8.8.8\",\"ssl\":true}'

config system_dns system_dns
\tlist servers '{\"address\":\"9.9.9.9\",\"ssl\":false}'
",
        )
        .unwrap();

        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["startwrt"]).await.unwrap();
        let groups = dns::collect_smartdns_groups(&cfgs);

        // Should have both system and profile groups
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].name, "system");
        assert_eq!(groups[0].port, dns::SMARTDNS_SYSTEM_PORT);
        assert_eq!(groups[1].name, "profile_guest");
        assert_eq!(groups[1].port, dns::smartdns_port_for_vlan(101));
        assert_eq!(groups[1].servers.len(), 2);
    }

    #[tokio::test]
    async fn test_collect_smartdns_groups_no_profile_dns() {
        let dir = tempfile::tempdir().unwrap();
        // Profile without dns_override should not produce a SmartDNS group
        std::fs::write(
            dir.path().join("startwrt"),
            "\
config profile lan
\toption fullname 'Admin'
\toption interface 'lan'
\toption vlan_tag '1'
",
        )
        .unwrap();

        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["startwrt"]).await.unwrap();
        let groups = dns::collect_smartdns_groups(&cfgs);

        assert!(groups.is_empty(), "No groups when no system or profile DNS");
    }

    #[tokio::test]
    async fn test_rewrite_all_dns_forwarding_creates_dnsmasq_for_system_dns() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("startwrt"),
            "\
config profile lan
\toption fullname 'Admin'
\toption interface 'lan'
\toption vlan_tag '1'

config profile guest
\toption fullname 'Guest'
\toption interface 'guest'
\toption vlan_tag '101'

config system_dns system_dns
\tlist servers '{\"address\":\"1.1.1.1\",\"ssl\":false}'
",
        )
        .unwrap();

        std::fs::write(
            dir.path().join("network"),
            "\
config interface 'lan'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption device 'br-lan.1'
\toption netmask '255.255.255.0'

config interface 'guest'
\toption proto 'static'
\toption ipaddr '192.168.101.1'
\toption device 'br-lan.101'
\toption netmask '255.255.255.0'
",
        )
        .unwrap();

        std::fs::write(
            dir.path().join("dhcp"),
            "\
config dnsmasq
\toption domainneeded '1'
",
        )
        .unwrap();

        std::fs::write(
            dir.path().join("firewall"),
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

config zone
\toption name 'vlan_guest'
\tlist network 'guest'
\toption input 'ACCEPT'
\toption output 'ACCEPT'
\toption forward 'ACCEPT'
",
        )
        .unwrap();

        let arena = Arena::new();
        let mut cfgs =
            parse_all(dir.path(), &arena, &["startwrt", "network", "dhcp", "firewall"]).await.unwrap();
        rewrite_all_dns_forwarding(&mut cfgs).unwrap();

        // Both profiles should get per-profile dnsmasq sections pointing to SmartDNS
        let dns_lan = cfgs["dhcp"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("dns_lan"));
        let dns_guest = cfgs["dhcp"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("dns_guest"));
        assert!(dns_lan, "Admin profile should get per-profile dnsmasq");
        assert!(dns_guest, "Guest profile should get per-profile dnsmasq");
    }

    // === IPv6 routing tests ===
    //
    // These exercise the v6 leg of rewrite_routing: route6 prt6_*, rule6 prl6_*
    // (suppress_prefixlength=0 escape), and rule6 prr6_* (VPN default).

    /// Augment setup_configs by adding two WG client interfaces (v6-capable
    /// `wg_v6` and v4-only `wg_v4`) and switching the LAN dhcp to RA=server so
    /// `is_ipv6_enabled` returns true.
    fn setup_configs_with_ipv6_vpn(dir: &std::path::Path) {
        setup_configs(dir);

        // Append WG interfaces to network config
        let mut network = std::fs::read_to_string(dir.join("network")).unwrap();
        network.push_str(
            "\n\
config interface 'wg_v6'
\toption proto 'wireguard'
\toption private_key 'aGkmAm6PEDDjyZx/Lwc8AfwlVWuJOaKB6E5Hp+JqgVc='
\toption disabled '0'
\toption defaultroute '0'
\toption peerdns '0'
\tlist addresses '10.2.0.2/32'
\tlist addresses 'fd00:2::2/128'

config wireguard_wg_v6 'v6_peer0'
\toption public_key 'aGkmAm6PEDDjyZx/Lwc8AfwlVWuJOaKB6E5Hp+JqgVc='
\toption endpoint_host 'vpn.example.com'
\toption endpoint_port '51820'
\toption route_allowed_ips '0'
\tlist allowed_ips '0.0.0.0/0'
\tlist allowed_ips '::/0'

config interface 'wg_v4'
\toption proto 'wireguard'
\toption private_key 'aGkmAm6PEDDjyZx/Lwc8AfwlVWuJOaKB6E5Hp+JqgVc='
\toption disabled '0'
\toption defaultroute '0'
\toption peerdns '0'
\tlist addresses '10.3.0.2/32'

config wireguard_wg_v4 'v4_peer0'
\toption public_key 'aGkmAm6PEDDjyZx/Lwc8AfwlVWuJOaKB6E5Hp+JqgVc='
\toption endpoint_host 'v4vpn.example.com'
\toption endpoint_port '51820'
\toption route_allowed_ips '0'
\tlist allowed_ips '0.0.0.0/0'
",
        );
        std::fs::write(dir.join("network"), network).unwrap();

        // Flip lan DHCP to RA/dhcpv6 server (is_ipv6_enabled looks at 'lan').
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

config dhcp 'guest'
\toption interface 'guest'
\toption start '100'
\toption limit '150'
\toption leasetime '12h'
\toption ra 'server'
\toption dhcpv6 'server'
",
        )
        .unwrap();
    }

    #[tokio::test]
    async fn test_rewrite_routing_emits_ipv6_when_vpn_supports_it() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs_with_ipv6_vpn(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wg_v6".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };

        set_config(ctx, &mut cfgs, &profile).unwrap();

        // prt6_guest: ::/0 via wg_v6, table 101
        let route6 = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("prt6_guest"))
            .expect("prt6_guest route6 should exist");
        let route6_data = route6.get::<NetworkRoute6>().unwrap();
        assert_eq!(route6_data.interface, "wg_v6");
        assert_eq!(route6_data.target, "::/0");
        assert_eq!(route6_data.table, Some(101));
        assert_eq!(route6_data.metric, Some(VPN_DEFAULT_ROUTE_METRIC));
        assert!(route6_data.kind.is_none(), "dev-wg route is not unreachable");

        // prt6b_guest: the fail-closed fallback that backstops the dev-wg route
        // if the WG interface goes down (higher metric, loopback, unreachable).
        let fallback = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("prt6b_guest"))
            .expect("prt6b_guest fallback route should exist")
            .get::<NetworkRoute6>()
            .unwrap();
        assert_eq!(fallback.target, "::/0");
        assert_eq!(fallback.interface, "loopback");
        assert_eq!(fallback.kind.as_deref(), Some("unreachable"));
        assert_eq!(fallback.table, Some(101));
        assert_eq!(fallback.metric, Some(VPN_KILLSWITCH_METRIC));

        // prl6_guest: lookup main with suppress_prefixlength=0 — the cross-VLAN
        // escape rule that lets connected /64s be matched ahead of ::/0 dev VPN.
        let escape = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("prl6_guest"))
            .expect("prl6_guest rule6 should exist");
        let escape_data = escape.get::<NetworkRule6>().unwrap();
        assert_eq!(escape_data.in_iface.as_deref(), Some("guest"));
        assert_eq!(escape_data.lookup, 254);
        assert_eq!(escape_data.suppress_prefixlength, Some(0));
        assert_eq!(escape_data.priority, Some(150));

        // prr6_guest: the per-VLAN VPN rule
        let rule6 = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("prr6_guest"))
            .expect("prr6_guest rule6 should exist");
        let rule6_data = rule6.get::<NetworkRule6>().unwrap();
        assert_eq!(rule6_data.in_iface.as_deref(), Some("guest"));
        assert_eq!(rule6_data.lookup, 101);
        assert_eq!(rule6_data.priority, Some(200));
        assert!(rule6_data.suppress_prefixlength.is_none());

        // Phase 5a: dhcp.guest should have ra_default='1' so odhcpd announces
        // Router B as the IPv6 default — required when downstream has no PD.
        let dhcp_guest = cfgs["dhcp"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("guest"))
            .expect("dhcp 'guest' should exist")
            .get::<Dhcp>()
            .unwrap();
        assert_eq!(
            dhcp_guest.ra_default.as_deref(),
            Some("1"),
            "ra_default=1 required so odhcpd advertises default router"
        );

        // IPv6 SNAT (ULA → tunnel GUA) is handled by the dedicated vpn_<wg>
        // egress zone carrying masq6=1 (NAT66), so VPN-routed v6 traffic is
        // masqueraded to the tunnel address without touching wan6.
        let vpn_zone = cfgs["firewall"]
            .sections
            .iter()
            .filter_map(|s| s.get::<FirewallZone>().ok())
            .find(|z| z.name == "vpn_wg_v6")
            .expect("vpn_wg_v6 zone should exist");
        assert_eq!(vpn_zone.masq, Some(true));
        assert_eq!(vpn_zone.masq6, Some(true));
    }

    #[tokio::test]
    async fn test_rewrite_routing_creates_dnat_return6_rule() {
        // A VPN-routed profile must get the global `dnat_return6` ip6 rule
        // (fwmark 0x80 -> main, priority 100) so that replies to inbound IPv6
        // port-forwards leave via wan6 instead of being captured by prr6_.
        // Created for every VPN-routed profile (the inbound problem exists even
        // for a v4-only VPN, since the device can still hold a wan6-PD GUA).
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs_with_ipv6_vpn(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wg_v6".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };
        set_config(ctx, &mut cfgs, &profile).unwrap();

        let rule6 = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("dnat_return6"))
            .expect("dnat_return6 rule6 should exist for a VPN-routed profile")
            .get::<NetworkRule6>()
            .unwrap();
        assert_eq!(rule6.lookup, 254, "dnat_return6 must look up the main table");
        assert_eq!(rule6.mark.as_deref(), Some("0x80/0x80"));
        assert_eq!(
            rule6.priority,
            Some(100),
            "must beat prl6_ (150) and prr6_ (200)"
        );
        // It's a global, unconditional rule — not scoped to one interface/source.
        assert!(rule6.in_iface.is_none());
        assert!(rule6.src.is_none());

        // The v4 sibling is still there too.
        assert!(
            cfgs["network"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("dnat_return")),
            "v4 dnat_return rule should still be created"
        );
    }

    #[tokio::test]
    async fn test_rewrite_routing_no_dnat_return6_for_wan_profile() {
        // A wan-routed profile takes the early return in rewrite_routing before
        // any DNAT-return rule is ensured, so a config with only wan profiles
        // never gains dnat_return6.
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs_with_ipv6_vpn(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wan".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };
        set_config(ctx, &mut cfgs, &profile).unwrap();

        assert!(
            !cfgs["network"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("dnat_return6")),
            "wan-routed profile should not create dnat_return6"
        );
    }

    #[tokio::test]
    async fn test_rewrite_routing_killswitch_when_vpn_v4_only() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs_with_ipv6_vpn(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wg_v4".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };

        set_config(ctx, &mut cfgs, &profile).unwrap();

        // IPv4 routing should still be created
        assert!(
            cfgs["network"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("prt_guest")),
            "IPv4 route should still exist for v4-only VPN"
        );

        // The v6 source rules ARE installed (kill switch is active whenever IPv6
        // is globally enabled), but there is NO `dev <wg>` v6 default route.
        for name in &["prl6_guest", "prr6_guest"] {
            assert!(
                cfgs["network"]
                    .sections
                    .iter()
                    .any(|s| s.name().as_deref() == Some(*name)),
                "{} should exist (v6 policy routing installed for fail-closed)",
                name
            );
        }
        assert!(
            !cfgs["network"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("prt6_guest")),
            "prt6_guest (dev wg v6 default) must NOT exist for a v4-only VPN"
        );

        // The v6 kill switch: an `unreachable ::/0` in the per-VLAN table,
        // attached to loopback. This blocks IPv6 egress so the ISP's v6 can't
        // leak when the VPN can't carry v6.
        let ks = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("prt6b_guest"))
            .expect("prt6b_guest kill-switch route should exist")
            .get::<NetworkRoute6>()
            .unwrap();
        assert_eq!(ks.target, "::/0");
        assert_eq!(ks.interface, "loopback");
        assert_eq!(ks.kind.as_deref(), Some("unreachable"));
        assert_eq!(ks.table, Some(101));
        assert_eq!(ks.metric, Some(VPN_KILLSWITCH_METRIC));

        // dhcp.ra_default should NOT be set when the VPN is v4-only — RA/DHCPv6
        // stay disabled on the profile; the kill switch is the active backstop.
        let dhcp_guest = cfgs["dhcp"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("guest"))
            .and_then(|s| s.get::<Dhcp>().ok());
        if let Some(dhcp) = dhcp_guest {
            assert!(
                dhcp.ra_default.is_none(),
                "ra_default should not be set when VPN is v4-only"
            );
        }
    }

    #[tokio::test]
    async fn test_rewrite_routing_cleans_ipv6_on_wan_switch() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs_with_ipv6_vpn(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // First: outbound = wg_v6 → IPv6 sections appear
        let profile_vpn = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wg_v6".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };
        set_config(ctx.clone(), &mut cfgs, &profile_vpn).unwrap();
        assert!(cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("prt6_guest")));

        // Then: switch back to wan → IPv6 sections should be removed
        let profile_wan = Profile {
            outbound: "wan".into(),
            ..profile_vpn
        };
        set_config(ctx, &mut cfgs, &profile_wan).unwrap();

        for name in &["prt6_guest", "prl6_guest", "prr6_guest"] {
            assert!(
                !cfgs["network"]
                    .sections
                    .iter()
                    .any(|s| s.name().as_deref() == Some(*name)),
                "{} should be removed after switching to WAN",
                name
            );
        }

        // ra_default should be removed after switching to wan — odhcpd's
        // default mode 2 behavior resumes (announce only if wan6 has default).
        let dhcp_guest = cfgs["dhcp"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("guest"))
            .and_then(|s| s.get::<Dhcp>().ok())
            .expect("dhcp 'guest' should exist");
        assert!(
            dhcp_guest.ra_default.is_none(),
            "ra_default should be cleared when outbound is wan"
        );
    }

    #[tokio::test]
    async fn test_rewrite_routing_installs_v6_killswitch_even_when_globally_disabled() {
        // Even with IPv6 globally disabled (no ra=server on lan), a VPN-routed
        // profile still gets the v6 policy routing + unreachable kill switch, so
        // a client with a residual/static v6 address can't leak out wan6. This
        // mirrors the always-on v4 fallback (fail closed in all states).
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path()); // baseline: no ra=server on lan

        // Add a v6-capable WG interface but leave dhcp baseline
        let mut network = std::fs::read_to_string(dir.path().join("network")).unwrap();
        network.push_str(
            "\nconfig interface 'wg_v6'
\toption proto 'wireguard'
\toption private_key 'aGkmAm6PEDDjyZx/Lwc8AfwlVWuJOaKB6E5Hp+JqgVc='
\tlist addresses 'fd00:2::2/128'
",
        );
        std::fs::write(dir.path().join("network"), network).unwrap();

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        let profile = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wg_v6".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };

        set_config(ctx, &mut cfgs, &profile).unwrap();

        // The v6 source rules and the unreachable kill switch are installed even
        // though global IPv6 is off — fail closed regardless of RA state.
        for name in &["prl6_guest", "prr6_guest", "prt6b_guest"] {
            assert!(
                cfgs["network"]
                    .sections
                    .iter()
                    .any(|s| s.name().as_deref() == Some(*name)),
                "{} should exist even when IPv6 is globally disabled (fail closed)",
                name
            );
        }
        // This VPN advertises a v6 address, so the dev-wg default is present too.
        assert!(
            cfgs["network"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("prt6_guest")),
            "prt6_guest dev-wg route present for a v6-capable VPN"
        );
        // The kill switch is an unreachable route on loopback.
        let ks = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("prt6b_guest"))
            .unwrap()
            .get::<NetworkRoute6>()
            .unwrap();
        assert_eq!(ks.kind.as_deref(), Some("unreachable"));
        assert_eq!(ks.interface, "loopback");
    }

    #[tokio::test]
    async fn test_rewrite_routing_converts_to_killswitch_on_switch_to_v4_only_vpn() {
        // Switching outbound from a v6-capable VPN to a v4-only VPN must drop the
        // `dev <wg>` v6 default (prt6_) but KEEP the v6 source rules and convert
        // the per-VLAN default to the `unreachable` kill switch (prt6b_).
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs_with_ipv6_vpn(dir.path());

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["startwrt", "network", "firewall", "dhcp"],
        )
        .await
        .unwrap();

        // First: outbound = wg_v6 → v6 sections appear.
        let profile_v6 = Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: Some("guest".into()),
                vlan_tag: Some(101),
            },
            gateway_ip: Ipv4Addr::new(192, 168, 101, 1),
            outbound: "wg_v6".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        };
        set_config(ctx.clone(), &mut cfgs, &profile_v6).unwrap();
        assert!(cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some("prt6_guest")));

        // Then: switch outbound to v4-only wg_v4. v4 routing must persist; v6
        // sections must be cleaned up because outbound_supports_ipv6 gates off.
        let profile_v4 = Profile {
            outbound: "wg_v4".into(),
            ..profile_v6
        };
        set_config(ctx, &mut cfgs, &profile_v4).unwrap();

        assert!(
            cfgs["network"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("prt_guest")),
            "v4 route should still exist after switching to v4-only VPN"
        );
        // The dev-wg v6 default is gone; the source rules and the kill-switch
        // fallback remain.
        assert!(
            !cfgs["network"]
                .sections
                .iter()
                .any(|s| s.name().as_deref() == Some("prt6_guest")),
            "prt6_guest (dev wg v6 default) should be removed after switching to v4-only VPN"
        );
        for name in &["prl6_guest", "prr6_guest", "prt6b_guest"] {
            assert!(
                cfgs["network"]
                    .sections
                    .iter()
                    .any(|s| s.name().as_deref() == Some(*name)),
                "{} should persist after switching to v4-only VPN",
                name
            );
        }
        let ks = cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some("prt6b_guest"))
            .unwrap()
            .get::<NetworkRoute6>()
            .unwrap();
        assert_eq!(ks.kind.as_deref(), Some("unreachable"));
        assert_eq!(ks.interface, "loopback");
    }

    // === Interface id allocation ===

    #[tokio::test]
    async fn test_allocate_is_random_and_well_formed() {
        let ctx = TestContext(PathBuf::from(".")); // non-effectful: no `ip link` calls
        let taken = BTreeSet::new();
        let name = allocate_interface_name(&ctx, &taken).await.unwrap();
        // INTERFACE_NAME_LIMIT lowercase letters, valid UCI section name.
        assert_eq!(name.len(), INTERFACE_NAME_LIMIT);
        assert!(name.chars().all(|c| c.is_ascii_lowercase()));
    }

    #[tokio::test]
    async fn test_allocate_avoids_taken_names() {
        let ctx = TestContext(PathBuf::from("."));
        // A random draw could collide with a reserved id (e.g. another profile's
        // id, or `lan`/`wan`); the allocator must retry until it finds a free one.
        let taken = BTreeSet::from(["wan".to_string(), "lan".to_string()]);
        let name = allocate_interface_name(&ctx, &taken).await.unwrap();
        assert!(!taken.contains(&name));
    }

    #[tokio::test]
    async fn test_reserved_interface_names_collects_network_and_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_configs(dir.path());
        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["network", "startwrt"])
            .await
            .unwrap();
        let reserved = reserved_interface_names(&cfgs);
        // From network interface sections and startwrt profiles.
        assert!(reserved.contains("lan"));
        assert!(reserved.contains("guest"));
    }
}
