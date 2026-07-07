use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::task::Poll;
use std::time::{Duration, Instant};

use clap::Parser;
use futures::{FutureExt, Stream, StreamExt, TryStreamExt};
use imbl::{OrdMap, OrdSet};
use imbl_value::{InternedString, Value};
use ipnet::IpNet;
use itertools::Itertools;
use nix::net::if_::if_nametoindex;
use patch_db::json_ptr::JsonPointer;
use rpc_toolkit::{Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;
use tokio::process::Command;
use tokio::sync::oneshot;
use ts_rs::TS;
use url::Url;
use visit_rs::{Visit, VisitFields};
use zbus::proxy::{PropertyChanged, PropertyStream, SignalStream};
use zbus::zvariant::{
    DeserializeDict, Dict, OwnedObjectPath, OwnedValue, Type as ZType, Value as ZValue,
};
use zbus::{Connection, proxy};

use crate::context::{CliContext, RpcContext};
use crate::db::model::Database;
use crate::db::model::public::{IpInfo, NetworkInterfaceInfo, NetworkInterfaceType};
use crate::net::forward::{START9_BRIDGE_IFACE, nft_ensure_base};
use crate::net::gateway::device::DeviceProxy;
use crate::net::host::all_hosts;
use crate::net::utils::{bind_mio_listener, find_wifi_iface, ipv6_is_local};
use crate::net::web_server::{Accept, AcceptStream, MetadataVisitor, TcpMetadata};
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::collections::OrdMapIterMut;
use crate::util::future::{NonDetachingJoinHandle, Until};
use crate::util::io::open_file;
use crate::util::serde::{HandlerExtSerde, display_serializable};
use crate::util::sync::Watch;
use crate::{GatewayId, PackageId};

pub fn gateway_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async(list_interfaces)
                .with_display_serializable()
                .with_custom_display_fn(|HandlerArgs { params, .. }, res| {
                    use prettytable::*;

                    if let Some(format) = params.format {
                        return display_serializable(format, res);
                    }

                    let mut table = Table::new();
                    table.add_row(row![bc => "INTERFACE", "TYPE", "ADDRESSES", "WAN IP"]);
                    for (iface, info) in res {
                        table.add_row(row![
                            iface,
                            info.ip_info
                                .as_ref()
                                .and_then(|ip_info| ip_info.device_type)
                                .map_or_else(|| "UNKNOWN".to_owned(), |ty| format!("{ty:?}")),
                            info.ip_info.as_ref().map_or_else(
                                || "<DISCONNECTED>".to_owned(),
                                |ip_info| ip_info
                                    .subnets
                                    .iter()
                                    .map(|ipnet| match ipnet.addr() {
                                        IpAddr::V4(ip) => format!("{ip}/{}", ipnet.prefix_len()),
                                        IpAddr::V6(ip) => format!(
                                            "[{ip}%{}]/{}",
                                            ip_info.scope_id,
                                            ipnet.prefix_len()
                                        ),
                                    })
                                    .join(", ")
                            ),
                            info.ip_info
                                .as_ref()
                                .and_then(|ip_info| ip_info.wan_ip)
                                .map_or_else(|| "N/A".to_owned(), |ip| ip.to_string())
                        ]);
                    }

                    table.print_tty(false)?;

                    Ok(())
                })
                .with_about("about.show-gateways-startos-can-listen-on")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "forget",
            from_fn_async(forget_iface)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.forget-disconnected-gateway")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-name",
            from_fn_async(set_name)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.rename-gateway")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "check-port",
            from_fn_async(check_port)
                .with_display_serializable()
                .with_about("about.check-port-reachability")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "check-port-v6",
            from_fn_async(check_port_v6)
                .with_display_serializable()
                .with_about("about.check-port-v6-reachability")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "check-dns",
            from_fn_async(check_dns)
                .with_display_serializable()
                .with_about("about.check-dns-configuration")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-default-outbound",
            from_fn_async(set_default_outbound)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.set-default-outbound-gateway")
                .with_call_remote::<CliContext>(),
        )
}

async fn list_interfaces(
    ctx: RpcContext,
) -> Result<OrdMap<GatewayId, NetworkInterfaceInfo>, Error> {
    Ok(ctx.net_controller.net_iface.watcher.ip_info())
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
struct ForgetGatewayParams {
    #[arg(help = "help.arg.gateway-id")]
    gateway: GatewayId,
}

async fn forget_iface(
    ctx: RpcContext,
    ForgetGatewayParams { gateway }: ForgetGatewayParams,
) -> Result<(), Error> {
    ctx.net_controller.net_iface.forget(&gateway).await
}

/// The WireGuard preshared key NetworkManager holds for `interface`'s peer, if
/// any. `GetSecrets` is root-only, so a sandboxed service can't read it — which
/// is what makes a TSIG key derived from it unforgeable. `None` for a
/// non-WireGuard gateway or a peer configured without a PSK.
pub(crate) async fn wireguard_psk(interface: &str) -> Result<Option<[u8; 32]>, Error> {
    let connection = Connection::system().await?;
    let netman = NetworkManagerProxy::new(&connection).await?;
    let device = netman.get_device_by_ip_iface(interface).await?;
    if &*device == "/" {
        return Ok(None);
    }
    let device = DeviceProxy::new(&connection, device).await?;
    let ac = device.active_connection().await?;
    if &*ac == "/" {
        return Ok(None);
    }
    let ac = active_connection::ActiveConnectionProxy::new(&connection, ac).await?;
    let settings = ConnectionSettingsProxy::new(&connection, ac.connection().await?).await?;
    let secrets = settings.get_secrets("wireguard").await?;
    let Some(peers) = secrets.get("wireguard").and_then(|wg| wg.get("peers")) else {
        return Ok(None);
    };
    let peers = peers
        .downcast_ref::<zbus::zvariant::Array>()
        .with_kind(ErrorKind::Network)?;
    for peer in peers.inner() {
        let Ok(dict) = peer.downcast_ref::<Dict>() else {
            continue;
        };
        let psk = dict
            .get::<_, String>(&zbus::zvariant::Str::from_static("preshared-key"))
            .with_kind(ErrorKind::Network)?;
        if let Some(psk) = psk.filter(|s| !s.is_empty()) {
            return Ok(Some(psk.parse::<crate::util::serde::Base64<[u8; 32]>>()?.0));
        }
    }
    Ok(None)
}

/// Update2 flag: persist the change to disk (NM_SETTINGS_UPDATE2_FLAG_TO_DISK).
const NM_UPDATE2_TO_DISK: u32 = 0x1;
/// Resolver priority for an imported WireGuard config's `DNS =` — preferred but
/// not exclusive (positive, below the LAN default). Mirrors import_wireguard.
const WG_DNS_PRIORITY: i32 = 10;

struct WgPeer {
    public_key: String,
    preshared_key: Option<String>,
    endpoint: Option<String>,
    persistent_keepalive: Option<u32>,
}

/// The fields of a WireGuard `.conf` we map onto a NetworkManager connection.
/// `AllowedIPs` is intentionally dropped: like `sanitize_config` (tunnel.rs) we
/// force a full-tunnel `0.0.0.0/0, ::/0` so every gateway routes the same way.
struct WgConfig {
    private_key: String,
    addresses: Vec<(IpAddr, u8)>,
    dns: Vec<IpAddr>,
    listen_port: Option<u32>,
    peers: Vec<WgPeer>,
}

impl WgConfig {
    fn parse(config: &str) -> Result<Self, Error> {
        let invalid = |what: &str| {
            Error::new(
                eyre!("invalid WireGuard config: {what}"),
                ErrorKind::InvalidRequest,
            )
        };
        enum Section {
            None,
            Interface,
            Peer,
        }
        let mut section = Section::None;
        let mut private_key = None;
        let mut addresses = Vec::new();
        let mut dns = Vec::new();
        let mut listen_port = None;
        let mut peers: Vec<WgPeer> = Vec::new();

        for raw in config.lines() {
            let line = raw.split(['#', ';']).next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if line.eq_ignore_ascii_case("[interface]") {
                section = Section::Interface;
                continue;
            }
            if line.eq_ignore_ascii_case("[peer]") {
                section = Section::Peer;
                peers.push(WgPeer {
                    public_key: String::new(),
                    preshared_key: None,
                    endpoint: None,
                    persistent_keepalive: None,
                });
                continue;
            }
            let Some((key, val)) = line.split_once('=') else {
                continue;
            };
            let (key, val) = (key.trim().to_ascii_lowercase(), val.trim());
            match section {
                Section::Interface => match key.as_str() {
                    "privatekey" => private_key = Some(val.to_string()),
                    "address" => {
                        for part in val.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                            let (ip, prefix) = match part.split_once('/') {
                                Some((ip, p)) => (
                                    ip.trim().parse().map_err(|_| invalid("address"))?,
                                    p.trim().parse().map_err(|_| invalid("prefix"))?,
                                ),
                                None => {
                                    let ip: IpAddr = part.parse().map_err(|_| invalid("address"))?;
                                    (ip, if ip.is_ipv4() { 32 } else { 128 })
                                }
                            };
                            if (ip.is_ipv4() && prefix > 32) || (ip.is_ipv6() && prefix > 128) {
                                return Err(invalid("address prefix out of range"));
                            }
                            addresses.push((ip, prefix));
                        }
                    }
                    "dns" => {
                        for part in val.split(',').map(str::trim) {
                            if let Ok(ip) = part.parse::<IpAddr>() {
                                dns.push(ip);
                            }
                        }
                    }
                    "listenport" => listen_port = val.parse().ok(),
                    _ => {}
                },
                Section::Peer => {
                    if let Some(p) = peers.last_mut() {
                        match key.as_str() {
                            "publickey" => p.public_key = val.to_string(),
                            "presharedkey" => p.preshared_key = Some(val.to_string()),
                            "endpoint" => p.endpoint = Some(val.to_string()),
                            "persistentkeepalive" => p.persistent_keepalive = val.parse().ok(),
                            _ => {}
                        }
                    }
                }
                Section::None => {}
            }
        }

        Ok(Self {
            private_key: private_key.ok_or_else(|| invalid("missing PrivateKey"))?,
            addresses,
            dns,
            listen_port,
            peers,
        })
    }

    /// Build the NetworkManager settings dict that `nmcli connection import` of
    /// this (sanitized) config would produce, so an in-place Update2 is
    /// equivalent to a fresh import.
    fn to_nm_settings(
        &self,
        iface: &str,
        uuid: Option<&str>,
    ) -> Result<HashMap<String, HashMap<String, OwnedValue>>, Error> {
        let ov = |v: ZValue<'_>| v.try_to_owned().with_kind(ErrorKind::Network);
        let mut settings: HashMap<String, HashMap<String, OwnedValue>> = HashMap::new();

        let mut conn = HashMap::from([
            ("id".into(), ov(iface.into())?),
            ("type".into(), ov("wireguard".into())?),
            ("interface-name".into(), ov(iface.into())?),
        ]);
        // On add NM generates the uuid; on update we keep the existing one so the
        // profile is replaced rather than redefined.
        if let Some(uuid) = uuid {
            conn.insert("uuid".into(), ov(uuid.into())?);
        }
        settings.insert("connection".into(), conn);

        let mut wg: HashMap<String, OwnedValue> = HashMap::new();
        wg.insert("private-key".into(), ov(self.private_key.as_str().into())?);
        if let Some(port) = self.listen_port {
            wg.insert("listen-port".into(), ov(port.into())?);
        }
        let peers: Vec<HashMap<String, ZValue>> = self
            .peers
            .iter()
            .map(|p| {
                let mut d: HashMap<String, ZValue> = HashMap::new();
                d.insert("public-key".into(), p.public_key.as_str().into());
                if let Some(psk) = &p.preshared_key {
                    d.insert("preshared-key".into(), psk.as_str().into());
                    // NM_SETTING_SECRET_FLAG_NONE (system-owned, stored). Required
                    // for the update-config path: `Update2` persists the interface
                    // `private-key` but silently DROPS this nested peer secret when
                    // no flag is set, so a re-issued tunnel loses its PSK and the
                    // handshake fails. `AddAndActivateConnection` normalizes the
                    // flag for us; `Update2` does not — so we set it explicitly.
                    d.insert("preshared-key-flags".into(), 0u32.into());
                }
                if let Some(ep) = &p.endpoint {
                    d.insert("endpoint".into(), ep.as_str().into());
                }
                if let Some(ka) = p.persistent_keepalive {
                    d.insert("persistent-keepalive".into(), ka.into());
                }
                d.insert(
                    "allowed-ips".into(),
                    vec!["0.0.0.0/0".to_string(), "::/0".to_string()].into(),
                );
                d
            })
            .collect();
        wg.insert("peers".into(), ov(peers.into())?);
        settings.insert("wireguard".into(), wg);

        for (proto, is_v4) in [("ipv4", true), ("ipv6", false)] {
            let addrs: Vec<_> = self
                .addresses
                .iter()
                .filter(|(ip, _)| ip.is_ipv4() == is_v4)
                .collect();
            let dns: Vec<_> = self.dns.iter().filter(|ip| ip.is_ipv4() == is_v4).collect();
            let mut s: HashMap<String, OwnedValue> = HashMap::new();
            if addrs.is_empty() {
                s.insert("method".into(), ov("disabled".into())?);
            } else {
                s.insert("method".into(), ov("manual".into())?);
                let address_data: Vec<HashMap<String, ZValue>> = addrs
                    .iter()
                    .map(|(ip, prefix)| {
                        HashMap::from([
                            ("address".into(), ip.to_string().into()),
                            ("prefix".into(), (*prefix as u32).into()),
                        ])
                    })
                    .collect();
                s.insert("address-data".into(), ov(address_data.into())?);
                // import_wireguard sets dns-priority on every active protocol,
                // regardless of whether this config carries resolvers.
                s.insert("dns-priority".into(), ov(WG_DNS_PRIORITY.into())?);
                if !dns.is_empty() {
                    let dns_val = if is_v4 {
                        // ipv4.dns is au: the address octets as a little-endian u32.
                        ZValue::from(
                            dns.iter()
                                .filter_map(|ip| match ip {
                                    IpAddr::V4(v4) => Some(u32::from_le_bytes(v4.octets())),
                                    _ => None,
                                })
                                .collect::<Vec<u32>>(),
                        )
                    } else {
                        // ipv6.dns is aay: each address as 16 raw bytes.
                        ZValue::from(
                            dns.iter()
                                .filter_map(|ip| match ip {
                                    IpAddr::V6(v6) => Some(v6.octets().to_vec()),
                                    _ => None,
                                })
                                .collect::<Vec<Vec<u8>>>(),
                        )
                    };
                    s.insert("dns".into(), ov(dns_val)?);
                    s.insert("dns-search".into(), ov(vec!["~".to_string()].into())?);
                    s.insert("dns-priority".into(), ov(WG_DNS_PRIORITY.into())?);
                }
            }
            settings.insert(proto.into(), s);
        }

        Ok(settings)
    }
}

/// Apply a re-issued WireGuard config to `interface`'s existing NM connection in
/// place via D-Bus Update2 + Device.Reapply. The wg device is never deleted, so
/// the tunnel — and the request that triggered the update, when it rides this
/// same tunnel — is not torn down. Reapply applies the change to the live device
/// without bringing the interface down (NM supports this for WireGuard).
pub(crate) async fn update_wireguard_config(interface: &str, config: &str) -> Result<(), Error> {
    let parsed = WgConfig::parse(config)?;

    let connection = Connection::system().await?;
    let netman = NetworkManagerProxy::new(&connection).await?;
    let device = netman.get_device_by_ip_iface(interface).await?;
    if &*device == "/" {
        return Err(Error::new(
            eyre!("{interface} not found in NetworkManager"),
            ErrorKind::NotFound,
        ));
    }
    let device_proxy = DeviceProxy::new(&connection, device).await?;
    let ac = device_proxy.active_connection().await?;
    if &*ac == "/" {
        return Err(Error::new(
            eyre!("{interface} has no active connection to update"),
            ErrorKind::InvalidRequest,
        ));
    }
    let ac_proxy = active_connection::ActiveConnectionProxy::new(&connection, ac).await?;
    let settings =
        ConnectionSettingsProxy::new(&connection, ac_proxy.connection().await?).await?;

    // Preserve the connection's uuid so this updates the existing profile rather
    // than defining a new one.
    let existing = settings.get_settings().await?;
    let uuid = existing
        .get("connection")
        .and_then(|c| c.get("uuid"))
        .and_then(|u| u.downcast_ref::<zbus::zvariant::Str>().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            Error::new(
                eyre!("{interface} connection is missing its uuid"),
                ErrorKind::Network,
            )
        })?;

    settings
        .update2(
            parsed.to_nm_settings(interface, Some(uuid.as_str()))?,
            NM_UPDATE2_TO_DISK,
            HashMap::new(),
        )
        .await?;
    // Full settings so Reapply doesn't strip the connection's other peer fields.
    device_proxy
        .reapply(parsed.to_nm_settings(interface, Some(uuid.as_str()))?, 0, 0)
        .await?;
    // NM (<=1.52) drops WG peer PSKs on Reapply even when passed explicitly;
    // restore them on the kernel device directly (PSK over stdin, never argv).
    for peer in &parsed.peers {
        if let Some(psk) = &peer.preshared_key {
            Command::new("wg")
                .arg("set")
                .arg(interface)
                .arg("peer")
                .arg(&peer.public_key)
                .arg("preshared-key")
                .arg("/dev/stdin")
                .input(Some(&mut std::io::Cursor::new(psk.clone().into_bytes())))
                .invoke(ErrorKind::Network)
                .await?;
        }
    }
    Ok(())
}

/// Create `interface`'s WireGuard connection from a config and bring it up, using
/// the same parse + conversion as [`update_wireguard_config`] so an added gateway
/// and an updated one are byte-for-byte identical. Replaces the old
/// `nmcli connection import` helper.
pub(crate) async fn add_wireguard_config(interface: &str, config: &str) -> Result<(), Error> {
    let settings = WgConfig::parse(config)?.to_nm_settings(interface, None)?;
    let connection = Connection::system().await?;
    let netman = NetworkManagerProxy::new(&connection).await?;
    // `/` for device + specific_object: NM creates the virtual wg device on activation.
    let root = zbus::zvariant::ObjectPath::try_from("/").with_kind(ErrorKind::Network)?;
    netman
        .add_and_activate_connection(settings, &root, &root)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
struct RenameGatewayParams {
    #[arg(help = "help.arg.gateway-id")]
    id: GatewayId,
    #[arg(help = "help.arg.gateway-name")]
    name: InternedString,
}

async fn set_name(
    ctx: RpcContext,
    RenameGatewayParams { id, name }: RenameGatewayParams,
) -> Result<(), Error> {
    ctx.net_controller.net_iface.set_name(&id, name).await
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheckPortParams {
    #[arg(help = "help.arg.port")]
    pub port: u16,
    #[arg(help = "help.arg.gateway-id")]
    pub gateway: GatewayId,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheckPortRes {
    pub ip: Ipv4Addr,
    pub port: u16,
    pub open_externally: bool,
    pub open_internally: bool,
    pub hairpinning: bool,
}

/// v6 reachability of the box's GUA at a port. v6 is NAT-free (the GUA is the
/// box's own address), so there is no hairpinning. Queried separately from
/// [`CheckPortRes`] so the IPv4 and IPv6 checks can run independently.
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheckPortV6Res {
    pub ip: Ipv6Addr,
    pub open_externally: bool,
    pub open_internally: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IfconfigPortRes {
    pub ip: Ipv4Addr,
    pub port: u16,
    pub reachable: bool,
}

#[derive(Deserialize)]
struct PortReachable {
    reachable: bool,
}

pub async fn check_port(
    ctx: RpcContext,
    CheckPortParams { port, gateway }: CheckPortParams,
) -> Result<CheckPortRes, Error> {
    let db = ctx.db.peek().await;
    let base_urls = db.as_public().as_server_info().as_echoip_urls().de()?;
    let gateways = db
        .as_public()
        .as_server_info()
        .as_network()
        .as_gateways()
        .de()?;
    let gw_info = gateways
        .get(&gateway)
        .ok_or_else(|| Error::new(eyre!("unknown gateway: {gateway}"), ErrorKind::NotFound))?;
    let ip_info = gw_info.ip_info.as_ref().ok_or_else(|| {
        Error::new(
            eyre!("gateway {gateway} has no IP info"),
            ErrorKind::NotFound,
        )
    })?;

    let internal_ips = ip_info
        .subnets
        .iter()
        .map(|i| i.addr())
        .filter(|a| a.is_ipv4())
        .map(|a| SocketAddr::new(a, port))
        .collect::<Vec<_>>();

    let open_internally = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::TcpStream::connect(&*internal_ips),
    )
    .await
    .map_or(false, |r| r.is_ok());

    let local_ipv4 = ip_info
        .subnets
        .iter()
        .find_map(|s| match s.addr() {
            IpAddr::V4(v4) => Some(v4),
            _ => None,
        })
        .unwrap_or(Ipv4Addr::UNSPECIFIED);

    // If automatic port forwarding (PCP/NAT-PMP/UPnP) already succeeded for this
    // port, it's reachable — skip the remote echo service and report success.
    // The mapping reports the gateway-assigned external IP, so we still know our
    // public address without a third-party round-trip.
    for ip in ip_info.subnets.iter().filter_map(|s| match s.addr() {
        IpAddr::V4(v4) => Some(v4),
        _ => None,
    }) {
        if let Ok(Some(IpAddr::V4(ip))) = tokio::time::timeout(
            Duration::from_secs(2),
            ctx.net_controller
                .port_map
                .mapped_external_ip(IpAddr::V4(ip), port),
        )
        .await
        {
            let hairpinning = check_hairpin(gateway, local_ipv4, ip, port).await;
            return Ok(CheckPortRes {
                ip,
                port,
                open_externally: true,
                open_internally,
                hairpinning,
            });
        }
    }

    let client = reqwest::Client::builder();
    #[cfg(target_os = "linux")]
    let client = client
        .interface(gateway.as_str())
        .local_address(IpAddr::V4(local_ipv4));
    let client = client.build()?;

    let mut res = None;
    for base_url in base_urls {
        let url = base_url
            .join(&format!("/port/{port}"))
            .with_kind(ErrorKind::ParseUrl)?;
        res = Some(
            async {
                client
                    .get(url)
                    .timeout(Duration::from_secs(5))
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await
            }
            .await,
        );
        if res.as_ref().map_or(false, |r| r.is_ok()) {
            break;
        }
    }
    let Some(IfconfigPortRes {
        ip,
        port,
        reachable: open_externally,
    }) = res.transpose()?
    else {
        return Err(Error::new(
            eyre!("{}", t!("net.gateway.no-configured-echoip-urls")),
            ErrorKind::Network,
        ));
    };

    let hairpinning = check_hairpin(gateway, local_ipv4, ip, port).await;

    Ok(CheckPortRes {
        ip,
        port,
        open_externally,
        open_internally,
        hairpinning,
    })
}

/// v6 counterpart of [`check_port`]: probe the box's IPv6 GUA reachability at
/// `port` on `gateway`. `None` when the gateway has no GUA.
pub async fn check_port_v6(
    ctx: RpcContext,
    CheckPortParams { port, gateway }: CheckPortParams,
) -> Result<Option<CheckPortV6Res>, Error> {
    let db = ctx.db.peek().await;
    let base_urls = db.as_public().as_server_info().as_echoip_urls().de()?;
    let gateways = db
        .as_public()
        .as_server_info()
        .as_network()
        .as_gateways()
        .de()?;
    let gw_info = gateways
        .get(&gateway)
        .ok_or_else(|| Error::new(eyre!("unknown gateway: {gateway}"), ErrorKind::NotFound))?;
    let ip_info = gw_info.ip_info.as_ref().ok_or_else(|| {
        Error::new(
            eyre!("gateway {gateway} has no IP info"),
            ErrorKind::NotFound,
        )
    })?;
    check_gua_port(
        &gateway,
        ip_info,
        port,
        &base_urls,
        &ctx.net_controller.port_map,
    )
    .await
}

/// Probe the box's IPv6 GUA reachability at `port` on `gateway`, if it has one.
/// v6 is NAT-free — the GUA is the box's own address — so "external" reachability
/// is just whether the upstream firewall lets an inbound connection through; there
/// is no hairpinning. An open PCP pinhole is taken as reachable; otherwise the
/// echoip backends (which support v6) probe the GUA from outside.
async fn check_gua_port(
    gateway: &GatewayId,
    ip_info: &IpInfo,
    port: u16,
    base_urls: &[Url],
    port_map: &crate::net::port_map::PortMapController,
) -> Result<Option<CheckPortV6Res>, Error> {
    let Some(gua) = ip_info.subnets.iter().find_map(|s| match s.addr() {
        IpAddr::V6(v6) if !ipv6_is_local(v6) => Some(v6),
        _ => None,
    }) else {
        return Ok(None);
    };

    let open_internally = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::TcpStream::connect(SocketAddr::new(IpAddr::V6(gua), port)),
    )
    .await
    .map_or(false, |r| r.is_ok());

    if let Ok(Some(_)) = tokio::time::timeout(
        Duration::from_secs(2),
        port_map.mapped_external_ip(IpAddr::V6(gua), port),
    )
    .await
    {
        return Ok(Some(CheckPortV6Res {
            ip: gua,
            open_externally: true,
            open_internally,
        }));
    }

    let client = reqwest::Client::builder();
    #[cfg(target_os = "linux")]
    let client = client
        .interface(gateway.as_str())
        .local_address(IpAddr::V6(gua));
    let client = client.build()?;

    let mut open_externally = false;
    for base_url in base_urls {
        let url = base_url
            .join(&format!("/port/{port}"))
            .with_kind(ErrorKind::ParseUrl)?;
        if let Ok(PortReachable { reachable }) = async {
            client
                .get(url)
                .timeout(Duration::from_secs(5))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await
        }
        .await
        {
            open_externally = reachable;
            break;
        }
    }

    Ok(Some(CheckPortV6Res {
        ip: gua,
        open_externally,
        open_internally,
    }))
}

#[cfg(target_os = "linux")]
async fn check_hairpin(gateway: GatewayId, local_ipv4: Ipv4Addr, ip: Ipv4Addr, port: u16) -> bool {
    let hairpinning = tokio::time::timeout(Duration::from_secs(5), async {
        let dest = SocketAddr::new(ip.into(), port);
        let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None)?;
        socket.bind_device(Some(gateway.as_str().as_bytes()))?;
        socket.bind(&SocketAddr::new(IpAddr::V4(local_ipv4), 0).into())?;
        socket.set_nonblocking(true)?;
        let socket = unsafe {
            use std::os::fd::{FromRawFd, IntoRawFd};
            tokio::net::TcpSocket::from_raw_fd(socket.into_raw_fd())
        };
        socket.connect(dest).await.map(|_| ())
    })
    .await
    .map_or(false, |r| r.is_ok());
    hairpinning
}

#[cfg(not(target_os = "linux"))]
async fn check_hairpin(_: GatewayId, _: Ipv4Addr, _: Ipv4Addr, _: u16) -> bool {
    false
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheckDnsParams {
    #[arg(help = "help.arg.gateway-id")]
    pub gateway: GatewayId,
    #[arg(help = "help.arg.fqdn")]
    pub fqdn: InternedString,
}

/// Verify a private domain works on the LAN by asking the LAN's own DNS
/// server(s) for `fqdn` and confirming the answer is one of this server's
/// addresses on that gateway — i.e. the record actually resolves correctly on
/// the LAN, rather than merely checking whether we are the LAN's DNS server.
pub async fn check_dns(
    ctx: RpcContext,
    CheckDnsParams { gateway, fqdn }: CheckDnsParams,
) -> Result<bool, Error> {
    use hickory_server::net::runtime::TokioRuntimeProvider;
    use hickory_server::resolver::Resolver;
    use hickory_server::resolver::config::{ResolverConfig, ResolverOpts};

    use crate::net::dns::forward_name_server;

    let ip_info = ctx.net_controller.net_iface.watcher.ip_info();
    let gw_info = ip_info
        .get(&gateway)
        .ok_or_else(|| Error::new(eyre!("unknown gateway: {gateway}"), ErrorKind::NotFound))?;
    let gw_ip_info = gw_info.ip_info.as_ref().ok_or_else(|| {
        Error::new(
            eyre!("gateway {gateway} has no IP info"),
            ErrorKind::NotFound,
        )
    })?;

    // The private domain should resolve to one of our addresses on this LAN.
    let expected: BTreeSet<IpAddr> = gw_ip_info.subnets.iter().map(|s| s.addr()).collect();
    if expected.is_empty() {
        return Ok(false);
    }

    // Query each resolver this gateway advertises — for a StartTunnel link this
    // is the imported config's `DNS =` (the in-tunnel `.1` that injects/serves
    // the record), now tracked in dns_servers (see poll_ip_info).
    for dns_ip in &gw_ip_info.dns_servers {
        let mut config = ResolverConfig::from_parts(None, Vec::new(), Vec::new());
        config.add_name_server(forward_name_server(SocketAddr::new(*dns_ip, 53)));
        let mut opts = ResolverOpts::default();
        opts.timeout = Duration::from_secs(5);
        opts.attempts = 1;

        let resolver = Resolver::builder_with_config(config, TokioRuntimeProvider::default())
            .with_options(opts)
            .build()
            .map_err(|e| Error::new(eyre!("{e}"), ErrorKind::Network))?;

        if let Ok(lookup) = resolver.lookup_ip(&*fqdn).await {
            if lookup.iter().any(|ip| expected.contains(&ip)) {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct SetDefaultOutboundParams {
    #[arg(help = "help.arg.gateway-id")]
    gateway: Option<GatewayId>,
}

async fn set_default_outbound(
    ctx: RpcContext,
    SetDefaultOutboundParams { gateway }: SetDefaultOutboundParams,
) -> Result<(), Error> {
    if let Some(ref gw) = gateway {
        let ip_info = ctx.net_controller.net_iface.watcher.ip_info();
        let info = ip_info
            .get(gw)
            .ok_or_else(|| Error::new(eyre!("unknown gateway: {gw}"), ErrorKind::NotFound))?;
        ensure_code!(
            info.ip_info.is_some(),
            ErrorKind::InvalidRequest,
            "gateway {gw} is not connected"
        );
    }
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_network_mut()
                .as_default_outbound_mut()
                .ser(&gateway)
        })
        .await
        .result
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SetOutboundGatewayParams {
    #[arg(help = "help.arg.package-id")]
    package: PackageId,
    #[arg(help = "help.arg.gateway-id")]
    gateway: Option<GatewayId>,
}

pub async fn set_outbound_gateway(
    ctx: RpcContext,
    SetOutboundGatewayParams { package, gateway }: SetOutboundGatewayParams,
) -> Result<(), Error> {
    if let Some(ref gw) = gateway {
        let ip_info = ctx.net_controller.net_iface.watcher.ip_info();
        let info = ip_info
            .get(gw)
            .ok_or_else(|| Error::new(eyre!("unknown gateway: {gw}"), ErrorKind::NotFound))?;
        ensure_code!(
            info.ip_info.is_some(),
            ErrorKind::InvalidRequest,
            "gateway {gw} is not connected"
        );
    }
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(&package)
                .or_not_found(&package)?
                .as_outbound_gateway_mut()
                .ser(&gateway)
        })
        .await
        .result
}

#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NetworkManager {
    fn get_device_by_ip_iface(&self, iface: &str) -> Result<OwnedObjectPath, Error>;

    /// Add a persistent connection from the given settings and activate it in one
    /// call. Pass `/` for device/specific_object to let NM pick or create the
    /// device (a virtual WireGuard interface is created on activation).
    fn add_and_activate_connection(
        &self,
        connection: HashMap<String, HashMap<String, OwnedValue>>,
        device: &zbus::zvariant::ObjectPath<'_>,
        specific_object: &zbus::zvariant::ObjectPath<'_>,
    ) -> Result<(OwnedObjectPath, OwnedObjectPath), Error>;

    #[zbus(property)]
    fn all_devices(&self) -> Result<Vec<OwnedObjectPath>, Error>;

    #[zbus(signal)]
    fn device_added(&self) -> Result<(), Error>;

    #[zbus(signal)]
    fn device_removed(&self) -> Result<(), Error>;

    #[zbus(signal)]
    fn state_changed(&self) -> Result<(), Error>;
}

mod active_connection {
    use zbus::proxy;
    use zbus::zvariant::OwnedObjectPath;

    use crate::prelude::*;

    #[proxy(
        interface = "org.freedesktop.NetworkManager.Connection.Active",
        default_service = "org.freedesktop.NetworkManager"
    )]
    pub trait ActiveConnection {
        #[zbus(property)]
        fn connection(&self) -> Result<OwnedObjectPath, Error>;

        #[zbus(property)]
        fn id(&self) -> Result<String, Error>;

        #[zbus(property)]
        fn state_flags(&self) -> Result<u32, Error>;

        #[zbus(property, name = "Type")]
        fn connection_type(&self) -> Result<String, Error>;

        #[zbus(signal)]
        fn state_changed(&self) -> Result<(), Error>;

        #[zbus(property)]
        fn dhcp4_config(&self) -> Result<OwnedObjectPath, Error>;
    }
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Settings.Connection",
    default_service = "org.freedesktop.NetworkManager"
)]
trait ConnectionSettings {
    fn delete(&self) -> Result<(), Error>;

    fn get_settings(&self) -> Result<HashMap<String, HashMap<String, OwnedValue>>, Error>;

    fn get_secrets(
        &self,
        setting_name: &str,
    ) -> Result<HashMap<String, HashMap<String, OwnedValue>>, Error>;

    fn update2(
        &self,
        settings: HashMap<String, HashMap<String, OwnedValue>>,
        flags: u32,
        args: HashMap<String, ZValue<'_>>,
    ) -> Result<HashMap<String, OwnedValue>, Error>;

    #[zbus(signal)]
    fn updated(&self) -> Result<(), Error>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.IP4Config",
    default_service = "org.freedesktop.NetworkManager"
)]
trait Ip4Config {
    #[zbus(property)]
    fn address_data(&self) -> Result<Vec<AddressData>, Error>;

    #[zbus(property)]
    fn route_data(&self) -> Result<Vec<RouteData>, Error>;

    #[zbus(property)]
    fn gateway(&self) -> Result<String, Error>;

    #[zbus(property)]
    fn nameserver_data(&self) -> Result<Vec<NameserverData>, Error>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.IP6Config",
    default_service = "org.freedesktop.NetworkManager"
)]
trait Ip6Config {
    #[zbus(property)]
    fn address_data(&self) -> Result<Vec<AddressData>, Error>;

    #[zbus(property)]
    fn gateway(&self) -> Result<String, Error>;

    // IP6Config has no `NameserverData`; its resolvers are `Nameservers` (aay),
    // each a raw 16-byte address.
    #[zbus(property)]
    fn nameservers(&self) -> Result<Vec<Vec<u8>>, Error>;
}

#[derive(Clone, Debug, DeserializeDict, ZValue, ZType)]
#[zvariant(signature = "dict")]
struct AddressData {
    address: String,
    prefix: u32,
}
impl TryFrom<AddressData> for IpNet {
    type Error = Error;
    fn try_from(value: AddressData) -> Result<Self, Self::Error> {
        IpNet::new(value.address.parse()?, value.prefix as u8).with_kind(ErrorKind::ParseNetAddress)
    }
}

#[derive(Clone, Debug, DeserializeDict, ZValue, ZType)]
#[zvariant(signature = "dict")]
struct RouteData {
    dest: String,
    prefix: u32,
    table: Option<u32>,
}

#[derive(Clone, Debug, DeserializeDict, ZValue, ZType)]
#[zvariant(signature = "dict")]
struct NameserverData {
    address: String,
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.DHCP4Config",
    default_service = "org.freedesktop.NetworkManager"
)]
trait Dhcp4Config {
    #[zbus(property)]
    fn options(&self) -> Result<Dhcp4Options, Error>;
}

#[derive(Clone, Debug, DeserializeDict, ZType)]
#[zvariant(signature = "dict")]
struct Dhcp4Options {
    ntp_servers: Option<String>,
    domain_name_servers: Option<String>,
}
impl TryFrom<OwnedValue> for Dhcp4Options {
    type Error = zbus::Error;
    fn try_from(value: OwnedValue) -> Result<Self, Self::Error> {
        let dict = value.downcast_ref::<Dict>()?;
        Ok(Self {
            ntp_servers: dict.get::<_, String>(&zbus::zvariant::Str::from_static("ntp_servers"))?,
            domain_name_servers: dict
                .get::<_, String>(&zbus::zvariant::Str::from_static("domain_name_servers"))?,
        })
    }
}

mod device {
    use std::collections::HashMap;

    use zbus::proxy;
    use zbus::zvariant::{OwnedObjectPath, OwnedValue};

    use crate::prelude::*;

    #[proxy(
        interface = "org.freedesktop.NetworkManager.Device",
        default_service = "org.freedesktop.NetworkManager"
    )]
    pub trait Device {
        fn delete(&self) -> Result<(), Error>;

        /// Apply changed connection settings to the live device without
        /// deactivating it. Empty `connection` reapplies the active connection's
        /// current (just-updated) settings; `version_id` 0 skips the version check.
        fn reapply(
            &self,
            connection: HashMap<String, HashMap<String, OwnedValue>>,
            version_id: u64,
            flags: u32,
        ) -> Result<(), Error>;

        #[zbus(property)]
        fn ip_interface(&self) -> Result<String, Error>;

        #[zbus(property)]
        fn managed(&self) -> Result<bool, Error>;

        #[zbus(property)]
        fn active_connection(&self) -> Result<OwnedObjectPath, Error>;

        #[zbus(property)]
        fn ip4_config(&self) -> Result<OwnedObjectPath, Error>;

        #[zbus(property)]
        fn ip6_config(&self) -> Result<OwnedObjectPath, Error>;

        #[zbus(property, name = "State")]
        fn _state(&self) -> Result<u32, Error>;

        #[zbus(property)]
        fn device_type(&self) -> Result<u32, Error>;

        #[zbus(signal)]
        fn state_changed(&self) -> Result<(), Error>;
    }
}

trait StubStream<'a> {
    fn stub(self) -> impl Stream<Item = Result<(), Error>> + 'a;
}
impl<'a, T> StubStream<'a> for PropertyStream<'a, T>
where
    T: Unpin + TryFrom<OwnedValue> + std::fmt::Debug + 'a,
    T::Error: Into<zbus::Error>,
{
    fn stub(self) -> impl Stream<Item = Result<(), Error>> + 'a {
        StreamExt::then(self, |d| async move {
            PropertyChanged::get(&d).await.map(|_| ())
        })
        .map_err(Error::from)
    }
}
impl<'a> StubStream<'a> for SignalStream<'a> {
    fn stub(self) -> impl Stream<Item = Result<(), Error>> + 'a {
        self.map(|_| Ok(()))
    }
}

#[instrument(skip_all)]
async fn watcher(
    watch_ip_info: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    watch_activation: Watch<BTreeMap<GatewayId, bool>>,
    db: Option<TypedPatchDb<Database>>,
) {
    loop {
        let res: Result<(), Error> = async {
            Command::new("systemctl")
                .arg("start")
                .arg("NetworkManager")
                .invoke(ErrorKind::Network)
                .await?;

            let connection = Connection::system().await?;

            let netman_proxy = NetworkManagerProxy::new(&connection).await?;

            let mut until = Until::new()
                .with_stream(netman_proxy.receive_all_devices_changed().await.stub())
                .with_stream(
                    netman_proxy
                        .receive_device_added()
                        .await?
                        .into_inner()
                        .stub(),
                )
                .with_stream(
                    netman_proxy
                        .receive_device_removed()
                        .await?
                        .into_inner()
                        .stub(),
                )
                .with_stream(
                    netman_proxy
                        .receive_state_changed()
                        .await?
                        .into_inner()
                        .stub(),
                );

            loop {
                until
                    .run(async {
                        let devices = netman_proxy.all_devices().await?;
                        ensure_code!(
                            !devices.is_empty(),
                            ErrorKind::Network,
                            "{}",
                            t!("net.gateway.no-devices-returned")
                        );
                        let mut ifaces = BTreeSet::new();
                        let mut policy_ifaces: BTreeMap<GatewayId, u32> = BTreeMap::new();
                        let mut jobs = Vec::new();
                        for device in devices {
                            use futures::future::Either;

                            let device_proxy =
                                device::DeviceProxy::new(&connection, device.clone()).await?;
                            let iface = InternedString::intern(device_proxy.ip_interface().await?);
                            if iface.is_empty() {
                                continue;
                            }
                            let iface: GatewayId = iface.into();
                            // Only policy-route managed, connected gateways —
                            // mirrors the gating in `watch_ip` and excludes
                            // unmanaged plumbing like container veth pairs.
                            if device_proxy.managed().await?
                                && &*device_proxy.active_connection().await? != "/"
                                && let Some(table_id) = policy_table_for(
                                    device_type_of(device_proxy.device_type().await?),
                                    &iface,
                                )
                            {
                                policy_ifaces.insert(iface.clone(), table_id);
                            }
                            if watch_activation.peek(|a| a.contains_key(&iface)) {
                                jobs.push(Either::Left(watch_activated(
                                    &connection,
                                    device_proxy.clone(),
                                    iface.clone(),
                                    &watch_activation,
                                )));
                            }

                            jobs.push(Either::Right(watch_ip(
                                &connection,
                                device_proxy.clone(),
                                iface.clone(),
                                &watch_ip_info,
                                db.as_ref(),
                            )));
                            ifaces.insert(iface);
                        }

                        watch_ip_info.send_if_modified(|m| {
                            let mut changed = false;
                            for (iface, info) in OrdMapIterMut::from(m) {
                                if !ifaces.contains(iface) {
                                    info.ip_info = None;
                                    changed = true;
                                }
                            }
                            changed
                        });
                        gc_policy_routing(&ifaces).await;
                        reconcile_mangle_rules(&policy_ifaces).await.log_err();
                        for result in futures::future::join_all(jobs).await {
                            result.log_err();
                        }

                        Ok::<_, Error>(())
                    })
                    .await?;
            }
        }
        .await;
        if let Err(e) = res {
            tracing::error!("{e}");
            tracing::debug!("{e:?}");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

/// Sentinel key used to rate-limit the per-interface UPnP WAN-IP probe inside
/// the echoip rate-limit map (it is not a real echoip URL and is never fetched).
fn upnp_probe_key() -> Url {
    Url::parse("upnp://get-external-ip-address").unwrap()
}

async fn get_wan_ipv4(
    iface: &str,
    base_url: &Url,
    local_ipv4: Ipv4Addr,
) -> Result<Option<Ipv4Addr>, Error> {
    let client = reqwest::Client::builder();
    #[cfg(target_os = "linux")]
    let client = client
        .interface(iface)
        .local_address(IpAddr::V4(local_ipv4));
    let url = base_url.join("/ip").with_kind(ErrorKind::ParseUrl)?;
    let text = client
        .build()?
        .get(url)
        .timeout(Duration::from_secs(5))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(trimmed.parse()?))
}

struct PolicyRoutingGuard {
    table_id: u32,
}

/// Remove stale per-interface policy-routing state (fwmark ip rules and their
/// routing tables) for interfaces that no longer exist. The nft mark rules are
/// owned declaratively by [`reconcile_mangle_rules`], so they need no GC here.
async fn gc_policy_routing(active_ifaces: &BTreeSet<GatewayId>) {
    let active_tables: BTreeSet<u32> = active_ifaces
        .iter()
        .filter_map(|iface| if_nametoindex(iface.as_str()).ok().map(|idx| 1000 + idx))
        .collect();

    // Collect stale per-interface table ids from the priority-50 fwmark rules in
    // both families (IPv4 and IPv6 install the same `1000 + ifindex` rule).
    let mut stale = BTreeSet::<u32>::new();
    for v6 in [false, true] {
        let mut cmd = Command::new("ip");
        if v6 {
            cmd.arg("-6");
        }
        let Ok(rules) = cmd
            .arg("rule")
            .arg("show")
            .invoke(ErrorKind::Network)
            .await
            .and_then(|b| String::from_utf8(b).with_kind(ErrorKind::Utf8))
        else {
            continue;
        };
        for line in rules.lines() {
            let line = line.trim();
            if !line.starts_with("50:") {
                continue;
            }
            let Some(pos) = line.find("lookup ") else {
                continue;
            };
            let token = line[pos + 7..].split_whitespace().next().unwrap_or("");
            let Ok(table_id) = token.parse::<u32>() else {
                continue;
            };
            if table_id < 1000 || active_tables.contains(&table_id) {
                continue;
            }
            stale.insert(table_id);
        }
    }

    // For each stale table, remove the priority-50 fwmark rule and flush its
    // routing table in both families, so a removed interface leaves no stale
    // reply-routing rule, v4 default, or v6 default/blackhole behind.
    for table_id in stale {
        let table_str = table_id.to_string();
        tracing::debug!("gc_policy_routing: removing stale table {table_id}");
        for v6 in [false, true] {
            let mut del = Command::new("ip");
            if v6 {
                del.arg("-6");
            }
            del.arg("rule")
                .arg("del")
                .arg("fwmark")
                .arg(&table_str)
                .arg("lookup")
                .arg(&table_str)
                .arg("priority")
                .arg("50")
                .invoke(ErrorKind::Network)
                .await
                .ok();
            let mut flush = Command::new("ip");
            if v6 {
                flush.arg("-6");
            }
            flush
                .arg("route")
                .arg("flush")
                .arg("table")
                .arg(&table_str)
                .invoke(ErrorKind::Network)
                .await
                .ok();
        }
    }
}

/// Snapshot the fwmarks at priority 74 and table ids at priority 75 already
/// present in one family's rule table (`v6` selects `ip -6`). Both families are
/// reconciled from the same desired set, so `apply_default_outbound` snapshots
/// each separately.
async fn snapshot_outbound_rules(v6: bool) -> (BTreeSet<u32>, BTreeSet<u32>) {
    match async {
        let mut cmd = Command::new("ip");
        if v6 {
            cmd.arg("-6");
        }
        let output =
            String::from_utf8(cmd.arg("rule").arg("show").invoke(ErrorKind::Network).await?)?;
        let mut fwmarks_74 = BTreeSet::<u32>::new();
        let mut tables_75 = BTreeSet::<u32>::new();
        for line in output.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("74:") {
                if let Some(pos) = rest.find("fwmark ") {
                    let after = &rest[pos + 7..];
                    let token = after.split_whitespace().next().unwrap_or("");
                    if let Ok(v) =
                        u32::from_str_radix(token.strip_prefix("0x").unwrap_or(token), 16)
                    {
                        fwmarks_74.insert(v);
                    }
                }
            } else if let Some(rest) = line.strip_prefix("75:") {
                if let Some(pos) = rest.find("lookup ") {
                    let after = &rest[pos + 7..];
                    let token = after.split_whitespace().next().unwrap_or("");
                    if let Ok(v) = token.parse::<u32>() {
                        tables_75.insert(v);
                    }
                }
            }
        }
        Ok::<_, Error>((fwmarks_74, tables_75))
    }
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("failed to snapshot outbound rules: {e}");
            (BTreeSet::new(), BTreeSet::new())
        }
    }
}

/// Reconcile one family's priority-74 (wg-encap fwmark → main) and priority-75
/// (catch-all → chosen gateway's table) outbound rules from `existing` toward
/// `desired`. The desired sets are family-agnostic — the same wg fwmarks and
/// gateway table id — so only the rule table (`v6` selects `ip -6`) differs.
async fn reconcile_outbound_rules(
    v6: bool,
    existing_74: &BTreeSet<u32>,
    desired_74: &BTreeSet<u32>,
    existing_75: &BTreeSet<u32>,
    desired_75: &BTreeSet<u32>,
) {
    let ip = || {
        let mut c = Command::new("ip");
        if v6 {
            c.arg("-6");
        }
        c
    };
    for fwmark in desired_74.difference(existing_74) {
        ip().arg("rule")
            .arg("add")
            .arg("fwmark")
            .arg(fwmark.to_string())
            .arg("lookup")
            .arg("main")
            .arg("priority")
            .arg("74")
            .invoke(ErrorKind::Network)
            .await
            .log_err();
    }
    for table in desired_75.difference(existing_75) {
        ip().arg("rule")
            .arg("add")
            .arg("table")
            .arg(table.to_string())
            .arg("priority")
            .arg("75")
            .invoke(ErrorKind::Network)
            .await
            .log_err();
    }
    for fwmark in existing_74.difference(desired_74) {
        ip().arg("rule")
            .arg("del")
            .arg("fwmark")
            .arg(fwmark.to_string())
            .arg("lookup")
            .arg("main")
            .arg("priority")
            .arg("74")
            .invoke(ErrorKind::Network)
            .await
            .log_err();
    }
    for table in existing_75.difference(desired_75) {
        ip().arg("rule")
            .arg("del")
            .arg("table")
            .arg(table.to_string())
            .arg("priority")
            .arg("75")
            .invoke(ErrorKind::Network)
            .await
            .log_err();
    }
}

/// Map a NetworkManager device-type id to our [`NetworkInterfaceType`].
fn device_type_of(raw: u32) -> Option<NetworkInterfaceType> {
    match raw {
        1 => Some(NetworkInterfaceType::Ethernet),
        2 => Some(NetworkInterfaceType::Wireless),
        13 => Some(NetworkInterfaceType::Bridge),
        29 => Some(NetworkInterfaceType::Wireguard),
        32 => Some(NetworkInterfaceType::Loopback),
        _ => None,
    }
}

/// The policy-routing table id (`1000 + ifindex`) for an interface, or `None`
/// for interface types we don't policy-route (bridges, loopback). Single source
/// of truth shared by the coordinator (which renders the nft mark rules) and
/// `watch_ip` (which installs the matching routes and ip rule).
fn policy_table_for(device_type: Option<NetworkInterfaceType>, iface: &GatewayId) -> Option<u32> {
    if matches!(
        device_type,
        Some(NetworkInterfaceType::Bridge | NetworkInterfaceType::Loopback)
    ) {
        return None;
    }
    if_nametoindex(iface.as_str()).map(|idx| 1000 + idx).log_err()
}

/// Declaratively reconcile the StartOS-owned mangle policy-routing rules.
///
/// Single-writer (only the gateway coordinator calls this) and applied as one
/// atomic nft transaction: the mangle chains are flushed and rebuilt from the
/// active interface set, so there is no multi-writer race and defunct
/// `mark-<iface>` rules are removed for free by the flush. `restore-mark` is
/// emitted first so it runs before the per-interface set-mark rules (a new
/// packet then leaves with mark 0 and routes via the main table).
///
/// IPv4 and IPv6 carry the same CONNMARK reply-routing layer, rebuilt together
/// so a reply to a v6 connection that arrived on a tunnel (host-terminated or
/// DNAT'd to a container) routes back out it via the priority-50 fwmark rule,
/// exactly like v4. `sni-divert` is IPv4-only (IP_TRANSPARENT egress is v4).
async fn reconcile_mangle_rules(policy_ifaces: &BTreeMap<GatewayId, u32>) -> Result<(), Error> {
    nft_ensure_base().await?;
    let mut script = String::new();
    script.push_str("flush chain ip startos mangle_prerouting\n");
    script.push_str("flush chain ip startos mangle_output\n");
    script.push_str("flush chain ip6 startos mangle_prerouting\n");
    script.push_str("flush chain ip6 startos mangle_output\n");
    script.push_str(
        "add rule ip startos mangle_prerouting meta mark 0x00000000 meta mark set ct mark comment \"restore-mark\"\n",
    );
    // Divert replies destined to a local transparent (SNI-demux) socket back into
    // it: mark them so the priority-49 ip rule routes them to the local table.
    script.push_str(&crate::net::transparent::divert_mark_rule());
    script.push_str(
        "add rule ip startos mangle_output meta mark 0x00000000 meta mark set ct mark comment \"restore-mark\"\n",
    );
    script.push_str(
        "add rule ip6 startos mangle_prerouting meta mark 0x00000000 meta mark set ct mark comment \"restore-mark\"\n",
    );
    script.push_str(
        "add rule ip6 startos mangle_output meta mark 0x00000000 meta mark set ct mark comment \"restore-mark\"\n",
    );
    for (iface, table_id) in policy_ifaces {
        script.push_str(&format!(
            "add rule ip startos mangle_prerouting iifname \"{iface}\" ct state new ct mark set {table_id} comment \"mark-{iface}\"\n",
            iface = iface.as_str(),
        ));
        script.push_str(&format!(
            "add rule ip6 startos mangle_prerouting iifname \"{iface}\" ct state new ct mark set {table_id} comment \"mark-{iface}\"\n",
            iface = iface.as_str(),
        ));
    }
    Command::new("nft")
        .arg(&script)
        .invoke(ErrorKind::Network)
        .await?;
    Ok(())
}

#[instrument(skip(connection, device_proxy, write_to, db))]
async fn watch_ip(
    connection: &Connection,
    device_proxy: device::DeviceProxy<'_>,
    iface: GatewayId,
    write_to: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    db: Option<&TypedPatchDb<Database>>,
) -> Result<(), Error> {
    let mut until = Until::new()
        .with_stream(
            device_proxy
                .receive_active_connection_changed()
                .await
                .stub(),
        )
        .with_stream(
            device_proxy
                .receive_state_changed()
                .await?
                .into_inner()
                .stub(),
        )
        .with_stream(device_proxy.receive_ip4_config_changed().await.stub())
        .with_stream(device_proxy.receive_ip6_config_changed().await.stub())
        .with_async_fn(|| {
            async {
                tokio::time::sleep(Duration::from_secs(600)).await;
                Ok(())
            }
            .fuse()
        });

    let mut echoip_ratelimit_state: BTreeMap<Url, Instant> = BTreeMap::new();

    loop {
        until
            .run(async {
                let ip4_config = device_proxy.ip4_config().await?;
                let ip6_config = device_proxy.ip6_config().await?;

                let managed = device_proxy.managed().await?;
                if !managed {
                    return Ok(());
                }
                let dac = device_proxy.active_connection().await?;
                if &*dac == "/" {
                    return Ok(());
                }

                let active_connection_proxy =
                    active_connection::ActiveConnectionProxy::new(&connection, dac).await?;

                let settings_proxy = ConnectionSettingsProxy::new(
                    &connection,
                    active_connection_proxy.connection().await?,
                )
                .await?;

                let mut until = Until::new()
                    .with_stream(
                        active_connection_proxy
                            .receive_state_changed()
                            .await?
                            .into_inner()
                            .stub(),
                    )
                    .with_stream(
                        active_connection_proxy
                            .receive_dhcp4_config_changed()
                            .await
                            .stub(),
                    )
                    .with_stream(settings_proxy.receive_updated().await?.into_inner().stub());

                loop {
                    until
                        .run(async {
                            let device_type = device_type_of(device_proxy.device_type().await?);

                            let name = InternedString::from(active_connection_proxy.id().await?);

                            let dhcp4_config = active_connection_proxy.dhcp4_config().await?;
                            let ip4_proxy =
                                Ip4ConfigProxy::new(&connection, ip4_config.clone()).await?;
                            let ip6_proxy =
                                Ip6ConfigProxy::new(&connection, ip6_config.clone()).await?;
                            let mut until = Until::new()
                                .with_stream(ip4_proxy.receive_address_data_changed().await.stub())
                                .with_stream(ip4_proxy.receive_route_data_changed().await.stub())
                                .with_stream(ip4_proxy.receive_gateway_changed().await.stub())
                                .with_stream(
                                    ip4_proxy.receive_nameserver_data_changed().await.stub(),
                                )
                                .with_stream(ip6_proxy.receive_address_data_changed().await.stub())
                                .with_stream(ip6_proxy.receive_gateway_changed().await.stub())
                                .with_stream(
                                    ip6_proxy.receive_nameservers_changed().await.stub(),
                                );

                            let dhcp4_proxy = if &*dhcp4_config != "/" {
                                let dhcp4_proxy =
                                    Dhcp4ConfigProxy::new(&connection, dhcp4_config).await?;
                                until = until.with_stream(
                                    dhcp4_proxy.receive_options_changed().await.stub(),
                                );
                                Some(dhcp4_proxy)
                            } else {
                                None
                            };

                            // Policy routing: track per-interface table for cleanup on scope exit
                            let policy_guard: Option<PolicyRoutingGuard> =
                                policy_table_for(device_type, &iface)
                                    .map(|table_id| PolicyRoutingGuard { table_id });

                            loop {
                                until
                                    .run(poll_ip_info(
                                        &ip4_proxy,
                                        &ip6_proxy,
                                        &dhcp4_proxy,
                                        &policy_guard,
                                        &iface,
                                        &mut echoip_ratelimit_state,
                                        db,
                                        write_to,
                                        device_type,
                                        &name,
                                    ))
                                    .await?;
                            }
                        })
                        .await?;
                }
            })
            .await?;
    }
}

async fn apply_policy_routing(
    guard: &PolicyRoutingGuard,
    iface: &GatewayId,
    lan_ip: &OrdSet<IpAddr>,
) -> Result<(), Error> {
    let table_id = guard.table_id;
    let table_str = table_id.to_string();

    let ipv4_gateway: Option<Ipv4Addr> = lan_ip
        .iter()
        .find_map(|ip| match ip {
            IpAddr::V4(v4) => Some(v4),
            _ => None,
        })
        .copied();

    // Rebuild per-interface routing table using `ip route replace` to avoid
    // the connectivity gap that a flush+add cycle would create.  We replace
    // every desired route in-place (each replace is atomic in the kernel),
    // then delete any stale routes that are no longer in the desired set.

    // Collect the set of desired non-default route prefixes (the first
    // whitespace-delimited token of each `ip route show` line is the
    // destination prefix, e.g. "192.168.1.0/24" or "10.0.0.0/8").
    let mut desired_prefixes = BTreeSet::<String>::new();

    if let Ok(main_routes) = Command::new("ip")
        .arg("route")
        .arg("show")
        .arg("table")
        .arg("main")
        .invoke(ErrorKind::Network)
        .await
        .and_then(|b| String::from_utf8(b).with_kind(ErrorKind::Utf8))
    {
        for line in main_routes.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("default") {
                continue;
            }
            if let Some(prefix) = line.split_whitespace().next() {
                desired_prefixes.insert(prefix.to_owned());
            }
            let mut cmd = Command::new("ip");
            cmd.arg("route").arg("replace");
            for part in line.split_whitespace() {
                // Skip status flags that appear in route output but
                // are not valid for `ip route replace`.
                if part == "linkdown" || part == "dead" {
                    continue;
                }
                cmd.arg(part);
            }
            cmd.arg("table").arg(&table_str);
            if let Err(e) = cmd.invoke(ErrorKind::Network).await {
                // Transient interfaces (podman, wg-quick, etc.) may
                // vanish between reading the main table and replaying
                // the route — demote to debug to avoid log noise.
                if e.source.to_string().contains("No such file or directory") {
                    tracing::trace!("ip route replace (transient device): {e}");
                } else {
                    tracing::error!("{e}");
                    tracing::debug!("{e:?}");
                }
            }
        }
    }

    // Replace the default route via this interface's gateway.
    {
        let mut cmd = Command::new("ip");
        cmd.arg("route").arg("replace").arg("default");
        if let Some(gw) = ipv4_gateway {
            cmd.arg("via").arg(gw.to_string());
        }
        cmd.arg("dev")
            .arg(iface.as_str())
            .arg("table")
            .arg(&table_str);
        if ipv4_gateway.is_none() {
            cmd.arg("scope").arg("link");
        }
        cmd.invoke(ErrorKind::Network).await.log_err();
    }

    // Delete stale routes: any non-default route in the per-interface table
    // whose prefix is not in the desired set.
    if let Ok(existing_routes) = Command::new("ip")
        .arg("route")
        .arg("show")
        .arg("table")
        .arg(&table_str)
        .invoke(ErrorKind::Network)
        .await
        .and_then(|b| String::from_utf8(b).with_kind(ErrorKind::Utf8))
    {
        for line in existing_routes.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("default") {
                continue;
            }
            let Some(prefix) = line.split_whitespace().next() else {
                continue;
            };
            if desired_prefixes.contains(prefix) {
                continue;
            }
            Command::new("ip")
                .arg("route")
                .arg("del")
                .arg(prefix)
                .arg("table")
                .arg(&table_str)
                .invoke(ErrorKind::Network)
                .await
                .log_err();
        }
    }

    // The mangle CONNMARK rules — the global `restore-mark` (mangle PREROUTING +
    // OUTPUT) and this interface's `mark-<iface>` — are reconciled centrally and
    // atomically by `reconcile_mangle_rules`, so they are not touched here.

    // Ensure fwmark-based ip rule for this interface's table
    let rules_output = String::from_utf8(
        Command::new("ip")
            .arg("rule")
            .arg("list")
            .invoke(ErrorKind::Network)
            .await?,
    )?;
    if !rules_output
        .lines()
        .any(|l| l.contains("fwmark") && l.contains(&format!("lookup {table_id}")))
    {
        Command::new("ip")
            .arg("rule")
            .arg("add")
            .arg("fwmark")
            .arg(&table_str)
            .arg("lookup")
            .arg(&table_str)
            .arg("priority")
            .arg("50")
            .invoke(ErrorKind::Network)
            .await
            .log_err();
    }

    Ok(())
}

/// IPv6 counterpart of [`apply_policy_routing`]'s per-interface table work.
///
/// The table `1000 + ifindex` mirrors `main`'s non-default v6 routes (so
/// on-link/local v6 still goes direct) plus a default: `default [via gw] dev
/// iface` when the interface can carry v6, otherwise `blackhole default`. That
/// per-gateway blackhole is the leak guard — a gateway with no IPv6 selected as
/// the default outbound (the priority-75 catch-all) then drops v6 rather than
/// letting it fall through to some other interface's default.
///
/// The table backs two rule layers, both IPv4 parity: the priority-75
/// default-outbound catch-all ([`apply_default_outbound`]) and the priority-50
/// CONNMARK reply-routing rule installed at the end here (marks set by
/// [`reconcile_mangle_rules`]). IPv6 has no NAT/SNI-demux reply layer to build —
/// the mark alone routes a reply back out the interface its connection arrived
/// on, so a v6 service reached through a tunnel can answer.
async fn apply_policy_routing_v6(
    guard: &PolicyRoutingGuard,
    iface: &GatewayId,
    lan_ip: &OrdSet<IpAddr>,
    subnets: &OrdSet<IpNet>,
) -> Result<(), Error> {
    let table_id = guard.table_id;
    let table_str = table_id.to_string();

    let ipv6_gateway: Option<Ipv6Addr> = lan_ip.iter().find_map(|ip| match ip {
        IpAddr::V6(v6) => Some(*v6),
        _ => None,
    });
    // The interface can carry v6 if it has a v6 gateway, or holds a global
    // (non-link-local, non-ULA) v6 address of its own — the delegated-GUA case,
    // where a point-to-point wg tunnel has no gateway but still routes v6.
    let v6_capable = ipv6_gateway.is_some()
        || subnets.iter().any(|n| match n {
            IpNet::V6(v6n) => !ipv6_is_local(v6n.addr()),
            _ => false,
        });

    // Mirror main's non-default v6 routes into the per-interface table, so the
    // priority-75 catch-all does not send on-link/local v6 through the gateway.
    let mut desired_prefixes = BTreeSet::<String>::new();
    if let Ok(main_routes) = Command::new("ip")
        .arg("-6")
        .arg("route")
        .arg("show")
        .arg("table")
        .arg("main")
        .invoke(ErrorKind::Network)
        .await
        .and_then(|b| String::from_utf8(b).with_kind(ErrorKind::Utf8))
    {
        for line in main_routes.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("default") {
                continue;
            }
            if let Some(prefix) = line.split_whitespace().next() {
                desired_prefixes.insert(prefix.to_owned());
            }
            let mut cmd = Command::new("ip");
            cmd.arg("-6").arg("route").arg("replace");
            for part in line.split_whitespace() {
                if part == "linkdown" || part == "dead" {
                    continue;
                }
                cmd.arg(part);
            }
            cmd.arg("table").arg(&table_str);
            if let Err(e) = cmd.invoke(ErrorKind::Network).await {
                if e.source.to_string().contains("No such file or directory") {
                    tracing::trace!("ip -6 route replace (transient device): {e}");
                } else {
                    tracing::error!("{e}");
                    tracing::debug!("{e:?}");
                }
            }
        }
    }

    // Replace the default: a real route when the interface can carry v6, else a
    // blackhole so a non-v6 gateway selected as default outbound drops v6.
    {
        let mut cmd = Command::new("ip");
        cmd.arg("-6").arg("route").arg("replace");
        if v6_capable {
            cmd.arg("default");
            if let Some(gw) = ipv6_gateway {
                cmd.arg("via").arg(gw.to_string());
            }
            cmd.arg("dev").arg(iface.as_str()).arg("table").arg(&table_str);
        } else {
            cmd.arg("blackhole")
                .arg("default")
                .arg("table")
                .arg(&table_str);
        }
        cmd.invoke(ErrorKind::Network).await.log_err();
    }

    // Delete stale non-default v6 routes no longer mirrored from main.
    if let Ok(existing_routes) = Command::new("ip")
        .arg("-6")
        .arg("route")
        .arg("show")
        .arg("table")
        .arg(&table_str)
        .invoke(ErrorKind::Network)
        .await
        .and_then(|b| String::from_utf8(b).with_kind(ErrorKind::Utf8))
    {
        for line in existing_routes.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("default") || line.starts_with("blackhole") {
                continue;
            }
            let Some(prefix) = line.split_whitespace().next() else {
                continue;
            };
            if desired_prefixes.contains(prefix) {
                continue;
            }
            Command::new("ip")
                .arg("-6")
                .arg("route")
                .arg("del")
                .arg(prefix)
                .arg("table")
                .arg(&table_str)
                .invoke(ErrorKind::Network)
                .await
                .log_err();
        }
    }

    // Ensure the priority-50 fwmark ip rule for this interface's table — the
    // IPv6 half of the CONNMARK reply-routing layer (marks set by
    // `reconcile_mangle_rules`). Mirrors `apply_policy_routing`'s IPv4 rule so a
    // reply to a connection that arrived on this interface (host-terminated or
    // DNAT'd to a container) routes back out it rather than being lost.
    let rules_output = String::from_utf8(
        Command::new("ip")
            .arg("-6")
            .arg("rule")
            .arg("list")
            .invoke(ErrorKind::Network)
            .await?,
    )?;
    if !rules_output
        .lines()
        .any(|l| l.contains("fwmark") && l.contains(&format!("lookup {table_id}")))
    {
        Command::new("ip")
            .arg("-6")
            .arg("rule")
            .arg("add")
            .arg("fwmark")
            .arg(&table_str)
            .arg("lookup")
            .arg(&table_str)
            .arg("priority")
            .arg("50")
            .invoke(ErrorKind::Network)
            .await
            .log_err();
    }

    Ok(())
}

async fn poll_ip_info(
    ip4_proxy: &Ip4ConfigProxy<'_>,
    ip6_proxy: &Ip6ConfigProxy<'_>,
    dhcp4_proxy: &Option<Dhcp4ConfigProxy<'_>>,
    policy_guard: &Option<PolicyRoutingGuard>,
    iface: &GatewayId,
    echoip_ratelimit_state: &mut BTreeMap<Url, Instant>,
    db: Option<&TypedPatchDb<Database>>,
    write_to: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    device_type: Option<NetworkInterfaceType>,
    name: &InternedString,
) -> Result<(), Error> {
    let addresses = ip4_proxy
        .address_data()
        .await?
        .into_iter()
        .chain(ip6_proxy.address_data().await?)
        .collect_vec();
    let lan_ip: OrdSet<IpAddr> = [
        Some(ip4_proxy.gateway().await?)
            .filter(|g| !g.is_empty())
            .and_then(|g| g.parse::<IpAddr>().log_err()),
        Some(ip6_proxy.gateway().await?)
            .filter(|g| !g.is_empty())
            .and_then(|g| g.parse::<IpAddr>().log_err()),
    ]
    .into_iter()
    .filter_map(|a| a)
    .collect();
    let mut ntp_servers = OrdSet::new();
    let mut dns_servers = OrdSet::new();
    if let Some(dhcp4_proxy) = dhcp4_proxy {
        let dhcp = dhcp4_proxy.options().await?;
        if let Some(ntp) = dhcp.ntp_servers {
            ntp_servers.extend(ntp.split_whitespace().map(InternedString::intern));
        }
        if let Some(dns) = dhcp.domain_name_servers {
            dns_servers.extend(
                dns.split_ascii_whitespace()
                    .filter_map(|s| s.parse::<IpAddr>().log_err())
                    .collect::<Vec<_>>(),
            );
        }
    }
    // Applied resolvers from the active IP config — the only source for a static
    // link, e.g. an imported WireGuard config's `DNS =` (NM stores it as
    // ipv4.dns and surfaces it here); DHCP options miss it entirely.
    dns_servers.extend(
        ip4_proxy
            .nameserver_data()
            .await?
            .into_iter()
            .filter_map(|ns| ns.address.parse::<IpAddr>().log_err()),
    );
    dns_servers.extend(
        ip6_proxy
            .nameservers()
            .await?
            .into_iter()
            .filter_map(|raw| <[u8; 16]>::try_from(raw).ok())
            .map(|octets| IpAddr::V6(Ipv6Addr::from(octets))),
    );
    let scope_id = if_nametoindex(iface.as_str()).with_kind(ErrorKind::Network)?;
    let subnets: OrdSet<IpNet> = addresses.into_iter().map(IpNet::try_from).try_collect()?;

    // Policy routing: ensure replies exit the same interface they arrived on,
    // eliminating the need for MASQUERADE.
    if let Some(guard) = policy_guard {
        apply_policy_routing(guard, iface, &lan_ip).await?;
        // v6 has no NAT/SNI-demux reply layer, but this now installs the v6
        // CONNMARK reply-routing rule (priority 50, IPv4 parity — marks set by
        // `reconcile_mangle_rules`) alongside readying the interface's v6 table
        // for the default-outbound catch-all and the per-gateway leak-guard
        // blackhole (when the interface can't carry v6).
        apply_policy_routing_v6(guard, iface, &lan_ip, &subnets).await?;
    }

    // Write IP info to the watch immediately so the gateway appears in the
    // DB without waiting for the (slow) WAN IP fetch.  The echoip HTTP
    // request has a 5-second timeout per URL and is easily cancelled by
    // D-Bus signals via the Until mechanism, which would prevent the
    // gateway from ever appearing if we waited.
    let mut ip_info = IpInfo {
        name: name.clone(),
        scope_id,
        device_type,
        subnets: subnets.clone(),
        lan_ip,
        wan_ip: None,
        ntp_servers,
        dns_servers,
    };

    write_to.send_if_modified(|m: &mut OrdMap<GatewayId, NetworkInterfaceInfo>| {
        let (name, secure, gateway_type, prev_wan_ip) =
            m.get(iface).map_or((None, None, Default::default(), None), |i| {
                (
                    i.name.clone(),
                    i.secure,
                    i.gateway_type,
                    i.ip_info.as_ref().and_then(|i| i.wan_ip),
                )
            });
        ip_info.wan_ip = prev_wan_ip;
        let ip_info = Arc::new(ip_info);
        m.insert(
            iface.clone(),
            NetworkInterfaceInfo {
                name,
                secure,
                ip_info: Some(ip_info.clone()),
                gateway_type,
            },
        )
        .filter(|old| &old.ip_info == &Some(ip_info))
        .is_none()
    });

    // Now fetch the WAN IP in a second pass.  Even if this is slow or
    // gets cancelled, the gateway already has valid ip_info above.
    let echoip_urls = if let Some(db) = db {
        db.peek()
            .await
            .as_public()
            .as_server_info()
            .as_echoip_urls()
            .de()
            .unwrap_or_else(|_| crate::db::model::public::default_echoip_urls())
    } else {
        crate::db::model::public::default_echoip_urls()
    };
    let mut wan_ip = None;
    let mut err = None;
    let local_ipv4 = subnets.iter().find_map(|s| match s.addr() {
        IpAddr::V4(v4) => Some(v4),
        _ => None,
    });
    let forwardable = !subnets.is_empty()
        && !matches!(
            device_type,
            Some(NetworkInterfaceType::Bridge | NetworkInterfaceType::Loopback)
        );

    // Ask the gateway's own UPnP IGD first. This works for a home router and
    // for a StartTunnel gateway, which answers GetExternalIPAddress over the
    // WireGuard link (see crate::tunnel::forward::igd). Rate-limited like echoip below,
    // keyed by a sentinel URL in the shared rate-limit map.
    if let Some(local_ipv4) = local_ipv4.filter(|_| forwardable) {
        let upnp_probe_key = upnp_probe_key();
        if echoip_ratelimit_state
            .get(&upnp_probe_key)
            .map_or(true, |i| i.elapsed() > Duration::from_secs(300))
        {
            echoip_ratelimit_state.insert(upnp_probe_key, Instant::now());
            match crate::net::port_map::upnp::get_external_ipv4(local_ipv4).await {
                Ok(Some(ip)) => wan_ip = Some(ip),
                Ok(None) => (),
                Err(e) => tracing::debug!("UPnP WAN IP probe on {iface} failed: {e}"),
            }
        }
    }

    for echoip_url in echoip_urls {
        if wan_ip.is_some() {
            break;
        }
        if echoip_ratelimit_state
            .get(&echoip_url)
            .map_or(true, |i| i.elapsed() > Duration::from_secs(300))
            && forwardable
        {
            let local_ipv4 = local_ipv4.unwrap_or(Ipv4Addr::UNSPECIFIED);
            match get_wan_ipv4(iface.as_str(), &echoip_url, local_ipv4).await {
                Ok(a) => {
                    wan_ip = a;
                }
                Err(e) => {
                    err = Some(e);
                }
            };
            echoip_ratelimit_state.insert(echoip_url, Instant::now());
            if wan_ip.is_some() {
                break;
            }
        };
    }
    if wan_ip.is_none()
        && let Some(e) = err
    {
        tracing::error!(
            "{}",
            t!(
                "net.gateway.failed-to-determine-wan-ip",
                iface = iface.to_string(),
                error = e.to_string()
            )
        );
        tracing::debug!("{e:?}");
    }

    // Update with WAN IP if we obtained one
    if wan_ip.is_some() {
        write_to.send_if_modified(|m: &mut OrdMap<GatewayId, NetworkInterfaceInfo>| {
            let Some(entry) = m.get_mut(iface) else {
                return false;
            };
            let Some(ref existing_ip) = entry.ip_info else {
                return false;
            };
            if existing_ip.wan_ip == wan_ip {
                return false;
            }
            let mut updated = (**existing_ip).clone();
            updated.wan_ip = wan_ip;
            entry.ip_info = Some(Arc::new(updated));
            true
        });
    }

    Ok(())
}

#[instrument(skip(_connection, device_proxy, watch_activation))]
async fn watch_activated(
    _connection: &Connection,
    device_proxy: device::DeviceProxy<'_>,
    iface: GatewayId,
    watch_activation: &Watch<BTreeMap<GatewayId, bool>>,
) -> Result<(), Error> {
    let mut until = Until::new()
        .with_stream(
            device_proxy
                .receive_active_connection_changed()
                .await
                .stub(),
        )
        .with_stream(
            device_proxy
                .receive_state_changed()
                .await?
                .into_inner()
                .stub(),
        );

    loop {
        until
            .run(async {
                let activated = device_proxy._state().await? == 100;
                watch_activation.send_if_modified(|a| {
                    a.get_mut(&iface)
                        .map_or(false, |a| std::mem::replace(a, activated) != activated)
                });
                Ok(())
            })
            .await?;
    }
}

pub struct NetworkInterfaceWatcher {
    activated: Watch<BTreeMap<GatewayId, bool>>,
    ip_info: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    _watcher: NonDetachingJoinHandle<()>,
}
impl NetworkInterfaceWatcher {
    pub fn new(
        seed: impl Future<Output = OrdMap<GatewayId, NetworkInterfaceInfo>> + Send + Sync + 'static,
        watch_activated: impl IntoIterator<Item = GatewayId>,
        db: TypedPatchDb<Database>,
    ) -> Self {
        let ip_info = Watch::new(OrdMap::new());
        let activated = Watch::new(watch_activated.into_iter().map(|k| (k, false)).collect());
        Self {
            activated: activated.clone(),
            ip_info: ip_info.clone(),
            _watcher: tokio::spawn(async move {
                let seed = seed.await;
                if !seed.is_empty() {
                    ip_info.send_replace(seed);
                }
                watcher(ip_info, activated, Some(db)).await
            })
            .into(),
        }
    }

    pub fn activated(&self) -> Watch<BTreeMap<GatewayId, bool>> {
        self.activated.clone_unseen()
    }

    pub fn wait_for_activated(
        &self,
        interface: GatewayId,
    ) -> impl Future<Output = ()> + Send + Sync + 'static {
        let mut activated = self.activated();
        async move {
            activated
                .wait_for(|a| a.get(&interface).copied().unwrap_or(false))
                .await;
        }
    }

    pub fn subscribe(&self) -> Watch<OrdMap<GatewayId, NetworkInterfaceInfo>> {
        self.ip_info.clone_unseen()
    }

    pub fn ip_info(&self) -> OrdMap<GatewayId, NetworkInterfaceInfo> {
        self.ip_info.read()
    }
}

pub struct NetworkInterfaceController {
    db: TypedPatchDb<Database>,
    pub watcher: NetworkInterfaceWatcher,
    _sync: NonDetachingJoinHandle<()>,
}
impl NetworkInterfaceController {
    async fn sync(
        db: &TypedPatchDb<Database>,
        info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    ) -> Result<(), Error> {
        tracing::debug!("syncronizing {info:?} to db");

        let wifi_iface = find_wifi_iface()
            .await
            .ok()
            .and_then(|a| a)
            .map(InternedString::from)
            .map(GatewayId::from);

        db.mutate(|db| {
            let network = db.as_public_mut().as_server_info_mut().as_network_mut();
            network.as_gateways_mut().ser(info)?;
            network.as_wifi_mut().as_interface_mut().ser(&wifi_iface)?;
            let hostname = crate::hostname::ServerHostname::load(db.as_public().as_server_info())?;
            let ports = db.as_private().as_available_ports().de()?;
            for host in all_hosts(db) {
                host?.update_addresses(&hostname, info, &ports)?;
            }
            Ok(())
        })
        .await
        .result?;

        let ntp: BTreeSet<_> = info
            .values()
            .filter_map(|i| i.ip_info.as_ref())
            .flat_map(|i| &i.ntp_servers)
            .cloned()
            .collect();
        let prev_ntp = tokio_stream::wrappers::LinesStream::new(
            BufReader::new(open_file("/etc/systemd/timesyncd.conf").await?).lines(),
        )
        .try_filter_map(|l| async move {
            Ok(l.strip_prefix("NTP=").map(|s| {
                s.split_whitespace()
                    .map(InternedString::intern)
                    .collect::<BTreeSet<_>>()
            }))
        })
        .boxed()
        .try_next()
        .await?
        .unwrap_or_default();
        if ntp != prev_ntp {
            // sed -i '/\(^\|#\)NTP=/c\NTP='"${servers}" /etc/systemd/timesyncd.conf
            Command::new("sed")
                .arg("-i")
                .arg(
                    [r#"/\(^\|#\)NTP=/c\NTP="#]
                        .into_iter()
                        .chain(Itertools::intersperse(
                            {
                                fn to_str(ntp: &InternedString) -> &str {
                                    &*ntp
                                }
                                ntp.iter().map(to_str)
                            },
                            " ",
                        ))
                        .join(""),
                )
                .arg("/etc/systemd/timesyncd.conf")
                .invoke(ErrorKind::Filesystem)
                .await?;
            Command::new("systemctl")
                .arg("restart")
                .arg("systemd-timesyncd")
                .invoke(ErrorKind::Systemd)
                .await?;
        }

        Ok(())
    }

    async fn apply_default_outbound(
        default_outbound: &Option<GatewayId>,
        ip_info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    ) {
        // 1. Snapshot existing priority-74/75 rules in both families. The desired
        //    set computed below is family-agnostic (the same wg-encap fwmarks and
        //    gateway table id), so v4 and v6 differ only in the rule table they
        //    are reconciled against.
        let (existing_74, existing_75) = snapshot_outbound_rules(false).await;
        let (existing_74_v6, existing_75_v6) = snapshot_outbound_rules(true).await;

        // 2. Compute desired rules
        let mut desired_74 = BTreeSet::<u32>::new();
        let mut desired_75 = BTreeSet::<u32>::new();

        if let Some(gw_id) = default_outbound {
            let connected = ip_info
                .get(gw_id)
                .map_or(false, |info| info.ip_info.is_some());
            if !connected {
                if ip_info.contains_key(gw_id) {
                    tracing::warn!("default outbound gateway {gw_id} is not connected");
                } else {
                    tracing::warn!("default outbound gateway {gw_id} not found in ip_info");
                }
            } else {
                match if_nametoindex(gw_id.as_str()) {
                    Ok(idx) => {
                        let table_id = 1000 + idx;
                        desired_75.insert(table_id);

                        // Exempt ALL active WireGuard interfaces' encapsulation packets.
                        // Our priority-75 catch-all would otherwise swallow their encap
                        // traffic before NM's fwmark rules at priority 31610 can route
                        // it correctly.
                        for (iface_id, iface_info) in ip_info {
                            let Some(ref ip) = iface_info.ip_info else {
                                continue;
                            };
                            if ip.device_type != Some(NetworkInterfaceType::Wireguard) {
                                continue;
                            }
                            match Command::new("wg")
                                .arg("show")
                                .arg(iface_id.as_str())
                                .arg("fwmark")
                                .invoke(ErrorKind::Network)
                                .await
                            {
                                Ok(output) => {
                                    let fwmark_hex =
                                        String::from_utf8_lossy(&output).trim().to_owned();
                                    if fwmark_hex.is_empty() || fwmark_hex == "off" {
                                        continue;
                                    }
                                    match u32::from_str_radix(
                                        fwmark_hex.strip_prefix("0x").unwrap_or(&fwmark_hex),
                                        16,
                                    ) {
                                        Ok(v) => {
                                            desired_74.insert(v);
                                        }
                                        Err(e) => {
                                            tracing::error!(
                                                "failed to parse WireGuard fwmark '{fwmark_hex}' for {iface_id}: {e}"
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "failed to read WireGuard fwmark for {iface_id}: {e}"
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("failed to get ifindex for {gw_id}: {e}");
                    }
                }
            }
        }

        // 3. Reconcile each family toward the desired set. Both use the same
        //    desired fwmarks/tables but separate rule tables; v6 carries no NAT,
        //    so this is the only v6 default-outbound machinery — the per-gateway
        //    v6 table it points at is populated by `apply_policy_routing_v6`
        //    (a real default when the gateway carries v6, else a blackhole so a
        //    non-v6 default outbound drops v6 instead of leaking it).
        reconcile_outbound_rules(false, &existing_74, &desired_74, &existing_75, &desired_75).await;
        reconcile_outbound_rules(
            true,
            &existing_74_v6,
            &desired_74,
            &existing_75_v6,
            &desired_75,
        )
        .await;
    }

    pub fn new(db: TypedPatchDb<Database>) -> Self {
        let (seeded_send, seeded) = oneshot::channel();
        let watcher = NetworkInterfaceWatcher::new(
            {
                let db = db.clone();
                async move {
                    let info = match db
                        .peek()
                        .await
                        .as_public()
                        .as_server_info()
                        .as_network()
                        .as_gateways()
                        .de()
                    {
                        Ok(mut info) => {
                            for (_, info) in OrdMapIterMut::from(&mut info) {
                                info.ip_info = None;
                            }
                            info
                        }
                        Err(e) => {
                            tracing::error!(
                                "{}",
                                t!(
                                    "net.gateway.error-loading-interface-info",
                                    error = e.to_string()
                                )
                            );
                            tracing::debug!("{e:?}");
                            OrdMap::new()
                        }
                    };
                    let _ = seeded_send.send(info.clone());
                    info
                }
            },
            [InternedString::from_static(START9_BRIDGE_IFACE).into()],
            db.clone(),
        );
        let mut ip_info_watch = watcher.subscribe();
        ip_info_watch.mark_seen();
        Self {
            db: db.clone(),
            watcher,
            _sync: tokio::spawn(async move {
                let res: Result<(), Error> = async {
                    let mut ip_info = seeded.await.ok();
                    let mut outbound_sub = db
                        .subscribe(
                            "/public/serverInfo/network/defaultOutbound"
                                .parse::<JsonPointer<_, _>>()
                                .unwrap(),
                        )
                        .await;
                    loop {
                        if let Err(e) = async {
                            if let Some(ref ip_info) = ip_info {
                                Self::sync(&db, ip_info).boxed().await?;
                            }
                            if let Some(ref ip_info) = ip_info {
                                let default_outbound: Option<GatewayId> = db
                                    .peek()
                                    .await
                                    .as_public()
                                    .as_server_info()
                                    .as_network()
                                    .as_default_outbound()
                                    .de()?;
                                Self::apply_default_outbound(&default_outbound, ip_info).await;
                            }

                            Ok::<_, Error>(())
                        }
                        .await
                        {
                            tracing::error!(
                                "{}",
                                t!("net.gateway.error-syncing-ip-info", error = e.to_string())
                            );
                            tracing::debug!("{e:?}");
                        }

                        tokio::select! {
                            _ = ip_info_watch.changed() => {
                                ip_info = Some(ip_info_watch.read());
                            }
                            _ = outbound_sub.recv() => {}
                        }
                    }
                }
                .await;
                if let Err(e) = res {
                    tracing::error!(
                        "{}",
                        t!("net.gateway.error-syncing-ip-info", error = e.to_string())
                    );
                    tracing::debug!("{e:?}");
                }
            })
            .into(),
        }
    }

    pub async fn forget(&self, interface: &GatewayId) -> Result<(), Error> {
        let mut sub = self
            .db
            .subscribe(
                "/public/serverInfo/network/gateways"
                    .parse::<JsonPointer<_, _>>()
                    .with_kind(ErrorKind::Database)?,
            )
            .await;
        let mut err = None;
        let changed = self.watcher.ip_info.send_if_modified(|ip_info| {
            if ip_info
                .get(interface)
                .map_or(false, |i| i.ip_info.is_some())
            {
                err = Some(Error::new(
                    eyre!("{}", t!("net.gateway.cannot-forget-connected-interface")),
                    ErrorKind::InvalidRequest,
                ));
                return false;
            }
            ip_info.remove(interface).is_some()
        });
        if let Some(e) = err {
            return Err(e);
        }
        if changed {
            sub.recv().await;
        }
        Ok(())
    }

    pub async fn delete_iface(&self, interface: &GatewayId) -> Result<(), Error> {
        let Some(has_ip_info) = self
            .watcher
            .ip_info
            .peek(|ifaces| ifaces.get(interface).map(|i| i.ip_info.is_some()))
        else {
            return self.forget(interface).await;
        };

        if has_ip_info {
            let mut ip_info = self.watcher.ip_info.clone_unseen();

            let connection = Connection::system().await?;

            let netman_proxy = NetworkManagerProxy::new(&connection).await?;

            let device = Some(
                netman_proxy
                    .get_device_by_ip_iface(interface.as_str())
                    .await?,
            )
            .filter(|o| &**o != "/")
            .or_not_found(lazy_format!("{interface} in NetworkManager"))?;

            let device_proxy = DeviceProxy::new(&connection, device).await?;

            let ac = device_proxy.active_connection().await?;

            if &*ac == "/" {
                return Err(Error::new(
                    eyre!("{}", t!("net.gateway.cannot-delete-without-connection")),
                    ErrorKind::InvalidRequest,
                ));
            }

            let ac_proxy = active_connection::ActiveConnectionProxy::new(&connection, ac).await?;

            let settings =
                ConnectionSettingsProxy::new(&connection, ac_proxy.connection().await?).await?;

            settings.delete().await?;

            ip_info
                .wait_for(|ifaces| ifaces.get(interface).map_or(true, |i| i.ip_info.is_none()))
                .await;
        }

        self.forget(interface).await?;

        Ok(())
    }

    pub async fn set_name(&self, interface: &GatewayId, name: InternedString) -> Result<(), Error> {
        let mut sub = self
            .db
            .subscribe(
                "/public/serverInfo/network/gateways"
                    .parse::<JsonPointer<_, _>>()
                    .with_kind(ErrorKind::Database)?
                    .join_end(interface.as_str())
                    .join_end("name"),
            )
            .await;
        let changed = self.watcher.ip_info.send_if_modified(|i| {
            i.get_mut(interface)
                .map(|i| {
                    if i.name.as_ref() != Some(&name) {
                        i.name = Some(name);
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false)
        });
        if changed {
            sub.recv().await;
        }

        Ok(())
    }
}

pub fn lookup_info_by_addr(
    ip_info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    addr: SocketAddr,
) -> Option<(&GatewayId, &NetworkInterfaceInfo)> {
    ip_info.iter().find(|(_, i)| {
        i.ip_info
            .as_ref()
            .map_or(false, |i| i.subnets.iter().any(|i| i.addr() == addr.ip()))
    })
}

#[derive(Clone, Debug)]
pub struct GatewayInfo {
    pub id: GatewayId,
    pub info: NetworkInterfaceInfo,
}
impl<V: MetadataVisitor> Visit<V> for GatewayInfo {
    fn visit(&self, visitor: &mut V) -> <V as visit_rs::Visitor>::Result {
        visitor.visit(self)
    }
}

/// Metadata for connections accepted by WildcardListener or VHostBindListener.
#[derive(Clone, Debug, VisitFields)]
pub struct NetworkInterfaceListenerAcceptMetadata {
    pub inner: TcpMetadata,
    pub info: GatewayInfo,
}
impl<V: MetadataVisitor> Visit<V> for NetworkInterfaceListenerAcceptMetadata {
    fn visit(&self, visitor: &mut V) -> V::Result {
        self.visit_fields(visitor).collect()
    }
}

/// A simple TCP listener on 0.0.0.0:port that looks up GatewayInfo from the
/// connection's local address on each accepted connection.
pub struct WildcardListener {
    listener: TcpListener,
    ip_info: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    /// Handle to the self-contained watcher task started in `new()`.
    /// Dropped (and thus aborted) when `set_ip_info` replaces the ip_info source.
    _watcher: Option<NonDetachingJoinHandle<()>>,
}
impl WildcardListener {
    pub fn new(port: u16) -> Result<Self, Error> {
        let listener = TcpListener::from_std(
            bind_mio_listener(SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), port))
                .with_kind(ErrorKind::Network)?
                .into(),
        )
        .with_kind(ErrorKind::Network)?;
        let ip_info = Watch::new(OrdMap::new());
        let watcher_handle =
            tokio::spawn(watcher(ip_info.clone(), Watch::new(BTreeMap::new()), None)).into();
        Ok(Self {
            listener,
            ip_info,
            _watcher: Some(watcher_handle),
        })
    }

    /// Replace the ip_info source with the one from the NetworkInterfaceController.
    /// Aborts the self-contained watcher task.
    pub fn set_ip_info(&mut self, ip_info: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>) {
        self.ip_info = ip_info;
        self._watcher = None;
    }
}
impl Accept for WildcardListener {
    type Metadata = NetworkInterfaceListenerAcceptMetadata;
    fn poll_accept(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(Self::Metadata, AcceptStream), Error>> {
        if let Poll::Ready((stream, peer_addr)) = TcpListener::poll_accept(&self.listener, cx)? {
            if let Err(e) = socket2::SockRef::from(&stream)
                .set_tcp_keepalive(&crate::net::utils::default_keepalive())
            {
                tracing::error!("Failed to set tcp keepalive: {e}");
                tracing::debug!("{e:?}");
            }
            let local_addr = stream.local_addr()?;
            let info = self
                .ip_info
                .peek(|ip_info| {
                    lookup_info_by_addr(ip_info, local_addr).map(|(id, info)| GatewayInfo {
                        id: id.clone(),
                        info: info.clone(),
                    })
                })
                .unwrap_or_else(|| GatewayInfo {
                    id: InternedString::from_static("").into(),
                    info: NetworkInterfaceInfo::default(),
                });
            return Poll::Ready(Ok((
                NetworkInterfaceListenerAcceptMetadata {
                    inner: TcpMetadata {
                        local_addr,
                        peer_addr,
                    },
                    info,
                },
                Box::pin(stream),
            )));
        }
        Poll::Pending
    }
}

#[cfg(test)]
mod wg_config_tests {
    use super::*;

    fn as_str(v: &OwnedValue) -> String {
        v.downcast_ref::<zbus::zvariant::Str>().unwrap().to_string()
    }

    #[test]
    fn parses_interface_and_peer() {
        let config = "[Interface]\n\
            PrivateKey = aPrivateKey=\n\
            Address = 10.59.0.2/24, fd00::2/64\n\
            DNS = 10.59.0.1\n\
            ListenPort = 51820\n\
            \n\
            [Peer]   # the tunnel server\n\
            PublicKey = aPublicKey=\n\
            PresharedKey = aPSK=\n\
            Endpoint = 1.2.3.4:51820\n\
            AllowedIPs = 10.0.0.0/8\n\
            PersistentKeepalive = 25\n";
        let p = WgConfig::parse(config).unwrap();
        assert_eq!(p.private_key, "aPrivateKey=");
        assert_eq!(
            p.addresses,
            vec![
                (IpAddr::V4(Ipv4Addr::new(10, 59, 0, 2)), 24),
                (IpAddr::V6("fd00::2".parse().unwrap()), 64),
            ]
        );
        assert_eq!(p.dns, vec![IpAddr::V4(Ipv4Addr::new(10, 59, 0, 1))]);
        assert_eq!(p.listen_port, Some(51820));
        assert_eq!(p.peers.len(), 1);
        assert_eq!(p.peers[0].public_key, "aPublicKey=");
        assert_eq!(p.peers[0].preshared_key.as_deref(), Some("aPSK="));
        assert_eq!(p.peers[0].endpoint.as_deref(), Some("1.2.3.4:51820"));
        assert_eq!(p.peers[0].persistent_keepalive, Some(25));
    }

    // ipv4.dns is `au` with each address as little-endian octets — the format we
    // captured from a real NM 1.52 import (10.x -> from_le_bytes). A regression
    // here would silently point the gateway at the wrong DNS server.
    #[test]
    fn builds_nm_settings_matching_import() {
        let config = "[Interface]\n\
            PrivateKey = aPrivateKey=\n\
            Address = 10.59.0.2/24\n\
            DNS = 10.59.0.1\n\
            [Peer]\n\
            PublicKey = aPublicKey=\n\
            Endpoint = 1.2.3.4:51820\n\
            AllowedIPs = 10.0.0.0/8\n";
        let s = WgConfig::parse(config)
            .unwrap()
            .to_nm_settings("wg0", Some("the-uuid"))
            .unwrap();

        assert_eq!(as_str(&s["connection"]["uuid"]), "the-uuid");
        assert_eq!(as_str(&s["connection"]["type"]), "wireguard");
        assert_eq!(as_str(&s["connection"]["interface-name"]), "wg0");
        assert_eq!(as_str(&s["wireguard"]["private-key"]), "aPrivateKey=");

        let ipv4 = &s["ipv4"];
        assert_eq!(as_str(&ipv4["method"]), "manual");
        let dns = Vec::<u32>::try_from(ipv4["dns"].try_clone().unwrap()).unwrap();
        assert_eq!(dns, vec![u32::from_le_bytes([10, 59, 0, 1])]);
        let priority = i32::try_from(ipv4["dns-priority"].try_clone().unwrap()).unwrap();
        assert_eq!(priority, WG_DNS_PRIORITY);

        // No ipv6 address in this config, so ipv6 is disabled (matches import).
        assert_eq!(as_str(&s["ipv6"]["method"]), "disabled");

        // Peer's allowed-ips is forced to full-tunnel regardless of the config.
        let peers = Vec::<HashMap<String, OwnedValue>>::try_from(
            s["wireguard"]["peers"].try_clone().unwrap(),
        )
        .unwrap();
        assert_eq!(peers.len(), 1);
        assert_eq!(as_str(&peers[0]["public-key"]), "aPublicKey=");
        let allowed = Vec::<String>::try_from(peers[0]["allowed-ips"].try_clone().unwrap()).unwrap();
        assert_eq!(allowed, vec!["0.0.0.0/0".to_string(), "::/0".to_string()]);
    }

    #[test]
    fn rejects_out_of_range_prefix() {
        assert!(WgConfig::parse("[Interface]\nPrivateKey = k=\nAddress = 10.0.0.2/40\n").is_err());
        assert!(WgConfig::parse("[Interface]\nPrivateKey = k=\nAddress = fd00::2/200\n").is_err());
    }

    // add() lets NM generate the uuid; update() preserves the existing one so the
    // same profile is replaced in place rather than redefined.
    #[test]
    fn uuid_present_only_on_update() {
        let parsed =
            WgConfig::parse("[Interface]\nPrivateKey = k=\nAddress = 10.0.0.2/24\n").unwrap();
        assert!(!parsed.to_nm_settings("wg0", None).unwrap()["connection"].contains_key("uuid"));
        let updated = parsed.to_nm_settings("wg0", Some("uuid-x")).unwrap();
        assert_eq!(as_str(&updated["connection"]["uuid"]), "uuid-x");
    }
}
