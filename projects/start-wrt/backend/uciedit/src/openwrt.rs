use std::{fmt, net::Ipv4Addr};

use inpt::Inpt;
use uciedit_macros::TypedSection;

#[derive(strum::EnumString, strum::Display, Default, PartialEq, Eq, Debug)]
pub enum FirewallTarget {
    #[default]
    ACCEPT,
    REJECT,
    DROP,
    MARK,
    NOTRACK,
}

#[derive(Debug, TypedSection, Default)]
#[uci(ty = "zone")]
pub struct FirewallZone {
    pub name: String,
    pub input: FirewallTarget,
    pub output: FirewallTarget,
    pub forward: FirewallTarget,
    pub network: Vec<String>,
    /// IPv4 masquerading (NAT). fw4 native.
    #[uci(default)]
    pub masq: Option<bool>,
    /// IPv6 masquerading (NAT66). fw4 native — used to SNAT VPN-routed LAN
    /// traffic to the tunnel's assigned address on a dedicated `vpn_<X>` zone.
    #[uci(default)]
    pub masq6: Option<bool>,
    /// TCP MSS clamping to path MTU (fw4 `option mtu_fix`). Enabled on VPN
    /// egress zones so forwarded TCP doesn't black-hole when the tunnel MTU is
    /// smaller than the LAN's — a backstop that complements an explicit
    /// interface `mtu`. TCP-only; does not cover UDP/QUIC.
    #[uci(default)]
    pub mtu_fix: Option<bool>,
}

#[derive(Debug, TypedSection, Default)]
#[uci(ty = "rule")]
pub struct FirewallRule {
    /*
    option	name		'Reject LAN to WAN for custom IP'
    option	src		'lan'
    option	src_ip		'192.168.1.2'
    option	src_mac		'00:11:22:33:44:55'
    option	src_port	'80'
    option	dest		'wan'
    option	dest_ip		'194.25.2.129'
    option	dest_port	'120'
    option	proto		'tcp'
    option	target		'REJECT'
    */
    pub name: String,
    pub src: String,
    #[uci(default)]
    pub src_ip: Option<String>,
    #[uci(default)]
    pub src_mac: Option<String>,
    #[uci(default)]
    pub src_port: Option<String>,
    #[uci(default)]
    pub dest: Option<String>,
    #[uci(default)]
    pub dest_ip: Option<String>,
    #[uci(default)]
    pub dest_port: Option<String>,
    pub proto: Vec<String>,
    pub target: FirewallTarget,
    #[uci(default)]
    pub family: Option<String>,
    #[uci(default)]
    pub enabled: Option<String>,
    #[uci(default)]
    pub set_mark: Option<String>,
    /// Published-port metadata: links IPv4 redirect + IPv6 rule
    #[uci(default)]
    pub _pp_id: Option<String>,
    /// Published-port metadata: device MAC
    #[uci(default)]
    pub _pp_mac: Option<String>,
}

#[derive(Debug, TypedSection, Default)]
#[uci(ty = "redirect")]
pub struct FirewallRedirect {
    pub name: String,
    pub src: String,
    #[uci(default)]
    pub dest: Option<String>,
    pub proto: Vec<String>,
    #[uci(default)]
    pub src_dport: Option<String>,
    #[uci(default)]
    pub src_ip: Option<String>,
    #[uci(default)]
    pub dest_ip: Option<String>,
    #[uci(default)]
    pub dest_port: Option<String>,
    pub target: String,
    #[uci(default)]
    pub enabled: Option<String>,
    /// Published-port metadata: links IPv4 redirect + IPv6 rule
    #[uci(default)]
    pub _pp_id: Option<String>,
    /// Published-port metadata: device MAC
    #[uci(default)]
    pub _pp_mac: Option<String>,
}

#[derive(Debug, TypedSection)]
#[uci(ty = "forwarding")]
pub struct FirewallForwarding {
    pub src: String,
    pub dest: String,
}

#[derive(strum::EnumString, strum::Display, Default, PartialEq, Eq, Debug)]
#[strum(serialize_all = "lowercase")]
pub enum InterfaceProto {
    #[default]
    NONE,
    STATIC,
    DHCP,
    DHCPV6,
    PPPOE,
    #[strum(serialize = "6rd")]
    SIXRD,
    WIREGUARD,
}

#[derive(Debug, Default, TypedSection)]
#[uci(ty = "interface")]
pub struct NetworkInterface {
    #[uci(default)]
    pub device: String,
    #[uci(default)]
    pub proto: InterfaceProto,
    pub ipaddr: Option<Ipv4Addr>,
    pub netmask: Option<Ipv4Addr>,
    #[uci(default)]
    pub gateway: Option<String>,
    // PPPoE fields
    #[uci(default)]
    pub username: Option<String>,
    #[uci(default)]
    pub password: Option<String>,
    // MAC override
    #[uci(default)]
    pub macaddr: Option<String>,
    // DNS fields
    #[uci(default)]
    pub peerdns: Option<String>,
    pub dns: Vec<String>,
    // IPv6 fields
    #[uci(default)]
    pub ip6assign: Option<String>,
    #[uci(default)]
    pub ip6addr: Option<String>,
    #[uci(default)]
    pub ip6gw: Option<String>,
    #[uci(default)]
    pub reqaddress: Option<String>,
    #[uci(default)]
    pub reqprefix: Option<String>,
    // 6RD fields
    #[uci(default)]
    pub peeraddr: Option<String>,
    #[uci(default)]
    pub ip6prefix: Option<String>,
    #[uci(default)]
    pub ip6prefixlen: Option<String>,
    #[uci(default)]
    pub ip4prefixlen: Option<String>,
    #[uci(default)]
    pub proxy_arp: Option<String>,
}

#[derive(strum::EnumString, strum::Display, PartialEq, Eq, Debug)]
#[strum(serialize_all = "lowercase")]
pub enum DeviceType {
    BRIDGE,
}

#[derive(Debug, TypedSection)]
#[uci(ty = "device")]
pub struct NetworkDevice {
    pub name: String,
    #[uci(rename = "type")]
    pub ty: Option<DeviceType>,
    pub ports: Vec<String>,
    #[uci(default)]
    pub macaddr: Option<String>,
}

#[derive(Inpt, Debug)]
#[inpt(regex = "([^:]+):?([ut*]+)?")]
pub struct NetworkVlanPort {
    pub port: String,
    #[inpt(from_str)]
    pub tagging: Option<NetworkVlanPortTagging>,
}

impl fmt::Display for NetworkVlanPort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.tagging {
            Some(tagging) => write!(f, "{}:{}", self.port, tagging),
            None => write!(f, "{}", self.port),
        }
    }
}

#[derive(strum::EnumString, strum::Display, PartialEq, Eq, Debug)]
#[strum(serialize_all = "lowercase")]
pub enum NetworkVlanPortTagging {
    #[strum(serialize = "u")]
    UNTAGGED,
    #[strum(serialize = "u*")]
    PRIMARY,
    #[strum(serialize = "t")]
    TAGGED,
}

#[derive(Debug, TypedSection)]
#[uci(ty = "bridge-vlan")]
pub struct NetworkBridgeVlan {
    pub device: String,
    pub vlan: u16,
    #[uci(inpt)]
    pub ports: Vec<NetworkVlanPort>,
}

#[derive(
    Clone, Copy, strum::FromRepr, strum::EnumString, Debug, strum::Display, Default, PartialEq, Eq,
)]
pub enum WifiDynamicVlan {
    #[strum(serialize = "0")]
    #[default]
    OFF = 0,
    #[strum(serialize = "1")]
    ALLOWED = 1,
    #[strum(serialize = "2")]
    REQUIRED = 2,
}

#[derive(Clone, Copy, strum::EnumString, Debug, strum::Display, Default, PartialEq, Eq)]
pub enum WifiMode {
    #[default]
    #[strum(serialize = "ap")]
    AP,
    #[strum(serialize = "sta")]
    STA,
    #[strum(serialize = "adhoc")]
    ADHOC,
    #[strum(serialize = "monitor")]
    MONITOR,
    #[strum(serialize = "mesh")]
    MESH,
}

#[derive(Clone, Copy, Inpt, Debug, Default, PartialEq, Eq)]
pub enum WifiChannel {
    #[default]
    #[inpt(regex = "auto")]
    Auto,
    Int(u32),
}

impl fmt::Display for WifiChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WifiChannel::Auto => write!(f, "auto"),
            WifiChannel::Int(i) => write!(f, "{}", i),
        }
    }
}

#[derive(Clone, Debug, TypedSection)]
#[uci(ty = "wifi-device")]
pub struct WifiDevice {
    #[uci(rename = "type")]
    pub device_type: String,
    pub path: Option<String>,
    #[uci(default_value = false)]
    pub disabled: bool,
    pub band: String,
    #[uci(inpt)]
    pub channel: WifiChannel,
}

#[derive(Clone, Debug, TypedSection)]
#[uci(ty = "wifi-iface")]
pub struct WifiInterface {
    pub device: String,
    #[uci(default)]
    pub mode: WifiMode,
    pub ssid: String,
    #[uci(default_value = false)]
    pub hidden: bool,
    #[uci(default_value = "none".to_string())]
    pub encryption: String,
    pub key: Option<String>,
    #[uci(default)]
    pub dynamic_vlan: WifiDynamicVlan,
    pub maxassoc: Option<u32>,
}

#[derive(Debug, TypedSection)]
#[uci(ty = "wifi-vlan")]
pub struct WifiVlan {
    pub name: String,
    pub network: String,
    pub vid: u16,
    pub iface: Option<String>,
}

#[derive(Debug, TypedSection)]
#[uci(ty = "wifi-station")]
pub struct WifiStation {
    pub key: String,
    pub vid: Option<u16>,
    pub iface: Option<String>,
    #[uci(default)]
    pub label: Option<String>,
}

#[derive(Debug, TypedSection, Default)]
#[uci(ty = "route")]
pub struct NetworkRoute {
    pub interface: String,
    pub target: String,
    #[uci(default)]
    pub gateway: Option<String>,
    #[uci(default)]
    pub netmask: Option<String>,
    #[uci(default)]
    pub metric: Option<u32>,
    #[uci(default)]
    pub table: Option<u32>,
    /// Route type, e.g. "unreachable" for a fail-closed kill-switch fallback.
    #[uci(default, rename = "type")]
    pub kind: Option<String>,
}

#[derive(Debug, TypedSection, Default)]
#[uci(ty = "route6")]
pub struct NetworkRoute6 {
    pub interface: String,
    /// IPv6 CIDR (e.g. `::/0`, `2001:db8::1/128`)
    pub target: String,
    #[uci(default)]
    pub gateway: Option<String>,
    #[uci(default)]
    pub metric: Option<u32>,
    #[uci(default)]
    pub mtu: Option<u32>,
    #[uci(default)]
    pub table: Option<u32>,
    /// Route type, e.g. "unreachable" for a fail-closed kill-switch fallback.
    #[uci(default, rename = "type")]
    pub kind: Option<String>,
}

#[derive(Debug, TypedSection, Default)]
#[uci(ty = "rule")]
pub struct NetworkRule {
    #[uci(default)]
    pub src: Option<String>,
    pub lookup: u32,
    #[uci(default)]
    pub mark: Option<String>,
    #[uci(default)]
    pub priority: Option<u32>,
}

#[derive(Debug, TypedSection, Default)]
#[uci(ty = "rule6")]
pub struct NetworkRule6 {
    /// Logical netifd interface name; netifd substitutes the kernel netdev at install time.
    #[uci(default, rename = "in")]
    pub in_iface: Option<String>,
    #[uci(default, rename = "out")]
    pub out_iface: Option<String>,
    #[uci(default)]
    pub src: Option<String>,
    #[uci(default)]
    pub dest: Option<String>,
    pub lookup: u32,
    #[uci(default)]
    pub mark: Option<String>,
    #[uci(default)]
    pub priority: Option<u32>,
    /// `FRA_SUPPRESS_PREFIXLEN`: suppress matches whose prefix length is ≤ this value.
    /// Set to 0 to fall through on default-route-only matches in the looked-up table.
    #[uci(default)]
    pub suppress_prefixlength: Option<u32>,
}

/// The `config globals 'globals'` section of /etc/config/network. Holds the
/// device-wide ULA prefix. With `ula_prefix 'auto'` the stock uci-default
/// `12_network-generate-ula` resolves it to a concrete random /48 and commits it
/// at first boot, so post-first-boot reads return a stable per-device prefix.
#[derive(Debug, TypedSection, Default)]
#[uci(ty = "globals")]
pub struct NetworkGlobals {
    #[uci(default)]
    pub ula_prefix: Option<String>,
}

#[derive(Debug, TypedSection)]
#[uci(ty = "dhcp")]
pub struct Dhcp {
    pub interface: String,
    pub start: u32,
    pub limit: u32,
    pub leasetime: String,
    // IPv6 fields
    #[uci(default)]
    pub ra: Option<String>,
    #[uci(default)]
    pub dhcpv6: Option<String>,
    #[uci(default)]
    pub ra_management: Option<String>,
    /// odhcpd default-router advertisement policy: "0" never, "1" always,
    /// "2" only when wan6 has a default route. Must be set to "1" on profiles
    /// whose outbound is a VPN with IPv6 — otherwise clients have no default
    /// route when the router itself has no upstream IPv6 PD.
    #[uci(default)]
    pub ra_default: Option<String>,
}

#[derive(Debug, TypedSection, Default)]
#[uci(ty = "dnsmasq")]
pub struct ProfileDnsmasq {
    #[uci(default)]
    pub server: Vec<String>,
    #[uci(default)]
    pub noresolv: Option<String>,
    #[uci(default)]
    pub interface: Vec<String>,
    #[uci(default)]
    pub localservice: Option<String>,
    #[uci(default)]
    pub nonwildcard: Option<String>,
    #[uci(default)]
    pub listen_address: Vec<String>,
    #[uci(default)]
    pub notinterface: Vec<String>,
    #[uci(default)]
    pub rebind_domain: Vec<String>,
    #[uci(default)]
    pub rebind_protection: Option<String>,
    #[uci(default)]
    pub localuse: Option<String>,
    #[uci(default)]
    pub leasefile: Option<String>,
    #[uci(default)]
    pub domain: Option<String>,
    #[uci(default)]
    pub expandhosts: Option<String>,
    #[uci(default)]
    pub boguspriv: Option<String>,
    #[uci(default)]
    pub local: Option<String>,
}

#[derive(Debug, TypedSection, Default)]
#[uci(ty = "host")]
pub struct DhcpHost {
    #[uci(default)]
    pub mac: String,
    #[uci(default)]
    pub name: Option<String>,
    #[uci(default)]
    pub ip: Option<String>,
    #[uci(default)]
    pub hostid: Option<String>,
    #[uci(default)]
    pub dns: Option<String>,
}

#[derive(Debug, TypedSection, Default)]
#[uci(ty = "system_dns")]
pub struct UciSystemDns {
    #[uci(default)]
    pub servers: Vec<String>, // JSON-encoded DnsServer entries
}

#[derive(Debug, TypedSection, Default)]
#[uci(ty = "service")]
pub struct DdnsService {
    #[uci(default)]
    pub enabled: Option<String>,
    #[uci(default)]
    pub service_name: Option<String>,
    #[uci(default)]
    pub ip_source: Option<String>,
    #[uci(default)]
    pub ip_network: Option<String>,
    #[uci(default)]
    pub username: Option<String>,
    #[uci(default)]
    pub password: Option<String>,
    #[uci(default)]
    pub domain: Option<String>,
    #[uci(default)]
    pub lookup_host: Option<String>,
}

