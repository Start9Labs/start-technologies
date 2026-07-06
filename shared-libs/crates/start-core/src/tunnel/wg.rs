use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr};

use imbl_value::InternedString;
use ipnet::{Ipv4Net, Ipv6Net};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use ts_rs::TS;
use x25519_dalek::{PublicKey, StaticSecret};

use crate::prelude::*;
use crate::tunnel::wg6;
use crate::util::Invoke;
use crate::util::io::write_file_atomic;
use crate::util::serde::Base64;

pub const WIREGUARD_INTERFACE_NAME: &str = "wg-start-tunnel";

/// Brand token written into every StartTunnel client config's header (see
/// client.conf.template, rendered as `# StartTunnel config for <name>`).
/// `add_tunnel` looks for this token (case-insensitively) to classify a pasted
/// config as an inbound/outbound StartTunnel gateway — vs outbound-only for
/// plain WireGuard configs like Mullvad. Matching just the brand token keeps
/// detection working across StartTunnel/StartOS version skew and any future
/// rewording of the surrounding header, and stays backwards-compatible with
/// configs generated before this check existed.
pub const START_TUNNEL_MARKER: &str = "StartTunnel";

#[derive(Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct WgServer {
    pub port: u16,
    pub key: Base64<WgKey>,
    pub subnets: WgSubnetMap,
}
impl Default for WgServer {
    fn default() -> Self {
        Self {
            port: 51820,
            key: Base64(WgKey::generate()),
            subnets: WgSubnetMap::default(),
        }
    }
}
impl WgServer {
    pub fn server_config<'a>(&'a self) -> ServerConfig<'a> {
        ServerConfig(self)
    }
    pub async fn sync(&self) -> Result<(), Error> {
        Command::new("wg-quick")
            .arg("down")
            .arg(WIREGUARD_INTERFACE_NAME)
            .env("LANG", "C.UTF-8")
            .invoke(ErrorKind::Network)
            .await
            .or_else(|e| {
                let msg = e.source.to_string();
                if msg.contains("does not exist") || msg.contains("is not a WireGuard interface") {
                    Ok(Vec::new())
                } else {
                    Err(e)
                }
            })?;
        write_file_atomic(
            const_format::formatcp!("/etc/wireguard/{WIREGUARD_INTERFACE_NAME}.conf"),
            self.server_config().to_string().as_bytes(),
        )
        .await?;
        Command::new("wg-quick")
            .arg("up")
            .arg(WIREGUARD_INTERFACE_NAME)
            .invoke(ErrorKind::Network)
            .await?;
        Ok(())
    }
}

#[derive(Default, Deserialize, Serialize, TS)]
pub struct WgSubnetMap(
    #[ts(as = "BTreeMap::<String, WgSubnetConfig>")] pub BTreeMap<Ipv4Net, WgSubnetConfig>,
);
impl Map for WgSubnetMap {
    type Key = Ipv4Net;
    type Value = WgSubnetConfig;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Self::key_string(key)
    }
    fn key_string(key: &Self::Key) -> Result<InternedString, Error> {
        Ok(InternedString::from_display(key))
    }
}

/// Per-subnet DNS proxy configuration. WireGuard clients on a subnet are pointed
/// at the subnet's in-tunnel server address (`.1`) for DNS; the forward-only proxy
/// listening there resolves every query against the upstream(s) selected here.
#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DnsConfig {
    /// Forward to the VPS's own system resolvers.
    #[default]
    Default,
    /// Forward to a device on this subnet (its WireGuard IP) on port 53.
    Device {
        #[ts(type = "string")]
        ip: Ipv4Addr,
    },
    /// Forward to operator-specified upstream servers (1-3, optional `:port`).
    Custom {
        #[ts(type = "string[]")]
        servers: Vec<SocketAddr>,
    },
}

#[derive(Default, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct WgSubnetConfig {
    pub name: InternedString,
    pub clients: WgSubnetClients,
    #[serde(default)]
    pub dns: DnsConfig,
    /// SNAT this subnet's egress to this WAN IP instead of `masquerade`. `None`
    /// keeps the default masquerade; a per-device `wan_ip` overrides this.
    #[serde(default)]
    #[ts(type = "string | null")]
    pub wan_ip: Option<std::net::Ipv4Addr>,
    /// Routed IPv6 prefix delegated to this subnet, if any. Each host on the
    /// subnet (the server and every client) gets one `/128` out of it with its
    /// tunnel IPv4 embedded — see [`crate::tunnel::wg6`].
    #[serde(default)]
    #[ts(type = "string | null")]
    pub ipv6: Option<Ipv6Net>,
}
impl WgSubnetConfig {
    pub fn new(name: InternedString) -> Self {
        Self {
            name,
            ..Self::default()
        }
    }
}

#[derive(Default, Deserialize, Serialize, TS)]
pub struct WgSubnetClients(pub BTreeMap<Ipv4Addr, WgConfig>);
impl Map for WgSubnetClients {
    type Key = Ipv4Addr;
    type Value = WgConfig;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Self::key_string(key)
    }
    fn key_string(key: &Self::Key) -> Result<InternedString, Error> {
        Ok(InternedString::from_display(key))
    }
}

#[derive(Clone)]
pub struct WgKey(StaticSecret);
impl WgKey {
    pub fn generate() -> Self {
        Self(StaticSecret::random_from_rng(
            &mut crate::util::crypto::os_rng(),
        ))
    }
}
impl AsRef<[u8]> for WgKey {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}
impl TryFrom<Vec<u8>> for WgKey {
    type Error = Error;
    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        Ok(Self(
            <[u8; 32]>::try_from(value)
                .map_err(|_| Error::new(eyre!("invalid key length"), ErrorKind::Deserialization))?
                .into(),
        ))
    }
}
impl std::ops::Deref for WgKey {
    type Target = StaticSecret;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Base64<WgKey> {
    pub fn verifying_key(&self) -> Base64<PublicKey> {
        Base64((&*self.0).into())
    }
}

/// A WireGuard client's role. A `Server` is a StartOS box that may configure the
/// gateway (DNS injection / auto port-forward); a `Client` is a plain peer with
/// no autoconfig. Stored and sticky — toggling the capability flags never changes
/// it; the migration backfills it from those flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, TS, clap::ValueEnum)]
#[serde(rename_all = "camelCase")]
pub enum WgClientKind {
    #[default]
    Client,
    Server,
}

#[derive(Clone, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct WgConfig {
    pub name: InternedString,
    pub key: Base64<WgKey>,
    pub psk: Base64<[u8; 32]>,
    /// Client (no autoconfig) vs Server (a StartOS box). Sticky; defaulted by the
    /// migration from the capability flags.
    #[serde(default)]
    pub kind: WgClientKind,
    /// Whether this device may inject DNS records via RFC 2136. Off by default
    /// — only enable for devices you trust (it lets the device add records to
    /// the tunnel's DNS for every peer to resolve).
    #[serde(default)]
    pub allow_dns_injection: bool,
    /// Whether this device may auto-create port forwards via PCP/IGD. Off by
    /// default — paired with `allow_dns_injection` under one "Gateway
    /// Autoconfiguration" toggle, but tracked separately so each capability is
    /// gated on its own.
    #[serde(default)]
    pub allow_auto_port_forward: bool,
    /// SNAT this device's egress to this WAN IP, overriding the subnet's
    /// `wan_ip` / the default masquerade. `None` falls back to the subnet rule.
    #[serde(default)]
    #[ts(type = "string | null")]
    pub wan_ip: Option<std::net::Ipv4Addr>,
}
impl WgConfig {
    pub fn generate(name: InternedString, kind: WgClientKind) -> Self {
        // A Server gets gateway-autoconfig on by default; a Client gets nothing.
        let autoconfig = matches!(kind, WgClientKind::Server);
        Self {
            name,
            key: Base64(WgKey::generate()),
            psk: Base64(rand::random()),
            kind,
            allow_dns_injection: autoconfig,
            allow_auto_port_forward: autoconfig,
            wan_ip: None,
        }
    }
    pub fn server_peer_config<'a>(
        &'a self,
        addr: Ipv4Addr,
        client_v6: Option<Ipv6Net>,
    ) -> ServerPeerConfig<'a> {
        ServerPeerConfig {
            client_config: self,
            client_addr: addr,
            client_v6,
        }
    }
    pub fn client_config(
        self,
        addr: Ipv4Addr,
        subnet: Ipv4Net,
        server_pubkey: Base64<PublicKey>,
        server_addr: SocketAddr,
        client_v6: Option<Ipv6Net>,
    ) -> ClientConfig {
        ClientConfig {
            client_config: self,
            client_addr: addr,
            subnet,
            server_pubkey,
            server_addr,
            client_v6,
        }
    }
}

pub struct ServerPeerConfig<'a> {
    client_config: &'a WgConfig,
    client_addr: Ipv4Addr,
    /// The client's `/128` out of its subnet's prefix, routed back to it over
    /// WireGuard. `None` when the subnet has no IPv6 prefix.
    client_v6: Option<Ipv6Net>,
}
impl<'a> std::fmt::Display for ServerPeerConfig<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let allowed = match self.client_v6 {
            Some(net) => format!("{}/32, {net}", self.client_addr),
            None => format!("{}/32", self.client_addr),
        };
        write!(
            f,
            include_str!("./server-peer.conf.template"),
            pubkey = self.client_config.key.verifying_key().to_padded_string(),
            psk = self.client_config.psk.to_padded_string(),
            allowed = allowed,
        )
    }
}

fn deserialize_verifying_key<'de, D>(deserializer: D) -> Result<Base64<PublicKey>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Base64::<Vec<u8>>::deserialize(deserializer).and_then(|b| {
        Ok(Base64(PublicKey::from(<[u8; 32]>::try_from(b.0).map_err(
            |e: Vec<u8>| serde::de::Error::invalid_length(e.len(), &"a 32 byte base64 string"),
        )?)))
    })
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    client_config: WgConfig,
    client_addr: Ipv4Addr,
    subnet: Ipv4Net,
    #[serde(deserialize_with = "deserialize_verifying_key")]
    server_pubkey: Base64<PublicKey>,
    server_addr: SocketAddr,
    #[serde(default)]
    client_v6: Option<Ipv6Net>,
}
impl std::fmt::Display for ClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let v4_addr = Ipv4Net::new_assert(self.client_addr, self.subnet.prefix_len());
        // The host's /128 rendered at the subnet's prefix length — declaring the
        // tunnel's IPv6 subnet on-link, symmetric with the IPv4 /24 above. The
        // host still owns only this one address; the server routes just its /128.
        let addr = match self.client_v6 {
            Some(v6) => format!("{v4_addr}, {v6}"),
            None => v4_addr.to_string(),
        };
        // Only the subnet's IPv4 `.1` is advertised for DNS — the proxy binds
        // IPv4 only, and AAAA resolution works fine over it. No v6 DNS address
        // is advertised until the proxy also listens on the tunnel's v6.
        let dns = self.subnet.addr().to_string();
        // IPv6 is full-tunnel (`::/0`): replies sourced from the VPS-delegated
        // global address must return through the tunnel, and a plain WireGuard
        // peer can't source-route otherwise. IPv4 stays split (subnet only).
        let allowed = match self.client_v6 {
            Some(_) => format!("{}, ::/0", self.subnet.trunc()),
            None => self.subnet.trunc().to_string(),
        };
        write!(
            f,
            include_str!("./client.conf.template"),
            marker = START_TUNNEL_MARKER,
            name = self.client_config.name,
            privkey = self.client_config.key.to_padded_string(),
            psk = self.client_config.psk.to_padded_string(),
            addr = addr,
            dns = dns,
            allowed = allowed,
            server_pubkey = self.server_pubkey.to_padded_string(),
            server_addr = self.server_addr,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> WgConfig {
        WgConfig::generate("phone".into(), WgClientKind::Client)
    }

    #[test]
    fn client_config_without_ipv6_is_v4_only() {
        // Subnets carry the server's `.1` host in the key; `addr()` is the DNS.
        let cfg = client()
            .client_config(
                "10.59.0.2".parse().unwrap(),
                "10.59.0.1/24".parse().unwrap(),
                Base64(WgKey::generate()).verifying_key(),
                "1.2.3.4:51820".parse().unwrap(),
                None,
            )
            .to_string();
        assert!(cfg.contains("Address = 10.59.0.2/24"));
        assert!(cfg.contains("DNS = 10.59.0.1"));
        assert!(cfg.contains("AllowedIPs = 10.59.0.0/24"));
        assert!(!cfg.contains("::"));
    }

    #[test]
    fn client_config_carries_v6_at_the_subnet_prefix() {
        // 10.59.0.2 == 0x0a3b0002, so the host bits are `a3b:2`; the client's own
        // /128 is carried at the delegated prefix length (declaring the subnet
        // on-link like the IPv4 /24) so the client can derive the server's v6.
        let prefix: Ipv6Net = "2001:db8:abcd::/64".parse().unwrap();
        let v6 = Ipv6Net::new_assert(
            wg6::host_v6(prefix, "10.59.0.2".parse().unwrap()),
            prefix.prefix_len(),
        );
        let cfg = client()
            .client_config(
                "10.59.0.2".parse().unwrap(),
                "10.59.0.1/24".parse().unwrap(),
                Base64(WgKey::generate()).verifying_key(),
                "1.2.3.4:51820".parse().unwrap(),
                Some(v6),
            )
            .to_string();
        assert!(cfg.contains("Address = 10.59.0.2/24, 2001:db8:abcd::a3b:2/64"));
        // Only the IPv4 DNS is advertised (the proxy binds IPv4 only).
        assert!(cfg.contains("DNS = 10.59.0.1"));
        assert!(!cfg.contains("DNS = 10.59.0.1, 2001"));
        // IPv6 is full-tunnel so replies from the delegated GUA return via wg.
        assert!(cfg.contains("AllowedIPs = 10.59.0.0/24, ::/0"));
    }

    #[test]
    fn server_config_lists_subnet_v6_and_routes_client_128() {
        let mut server = WgServer::default();
        server.subnets = WgSubnetMap::default();
        server.subnets.0.insert(
            "10.59.0.1/24".parse().unwrap(),
            WgSubnetConfig {
                name: "net".into(),
                clients: {
                    let mut c = WgSubnetClients::default();
                    c.0.insert("10.59.0.2".parse().unwrap(), client());
                    c
                },
                ipv6: Some("2001:db8:abcd::/64".parse().unwrap()),
                ..Default::default()
            },
        );
        let rendered = server.server_config().to_string();
        // Server's own /128 (subnet .1) on its interface, at the prefix length.
        assert!(rendered.contains("2001:db8:abcd::a3b:1/64"));
        assert!(rendered.contains("AllowedIPs = 10.59.0.2/32, 2001:db8:abcd::a3b:2/128"));
    }
}

pub struct ServerConfig<'a>(&'a WgServer);
impl<'a> std::fmt::Display for ServerConfig<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self(server) = *self;
        // Interface Address: each subnet's v4 (its `.1/len`), plus the server's
        // own /128 out of that subnet's prefix when it carries one.
        let mut addresses = Vec::new();
        for (net, cfg) in &server.subnets.0 {
            addresses.push(net.to_string());
            if let Some(prefix) = cfg.ipv6 {
                let server_v6 = wg6::host_v6(prefix, net.addr());
                addresses.push(format!("{server_v6}/{}", prefix.prefix_len()));
            }
        }
        write!(
            f,
            include_str!("./server.conf.template"),
            subnets = addresses.join(", "),
            server_port = server.port,
            server_privkey = server.key.to_padded_string(),
        )?;
        for cfg in server.subnets.0.values() {
            for (client_addr, peer) in &cfg.clients.0 {
                let client_v6 = cfg
                    .ipv6
                    .map(|p| Ipv6Net::new_assert(wg6::host_v6(p, *client_addr), 128));
                write!(f, "{}", peer.server_peer_config(*client_addr, client_v6))?;
            }
        }
        Ok(())
    }
}
