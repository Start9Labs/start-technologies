use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4};
use std::str::FromStr;

use clap::{Parser, ValueEnum};
use hickory_server::proto::rr::{Name, RecordType};
use imbl_value::InternedString;
use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use rpc_toolkit::{Context, Empty, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::context::CliContext;
use crate::db::model::public::NetworkInterfaceType;
use crate::net::forward::nft_rule;
use crate::net::dns_update::rfc2136::InjectedRecord;
use crate::prelude::*;
use crate::tunnel::context::TunnelContext;
use crate::net::port_map::server::GatewayBackend;
use crate::tunnel::db::{DnsRecordEntry, PortForward};
use crate::tunnel::forward::pinhole;
use crate::tunnel::wg::{
    DnsConfig, WIREGUARD_INTERFACE_NAME, WgClientKind, WgConfig, WgSubnetClients, WgSubnetConfig,
};
use crate::tunnel::wg6;
use crate::util::serde::{HandlerExtSerde, display_serializable};

pub fn tunnel_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "web",
            super::web::web_api::<C>().with_about("about.commands-tunnel-web"),
        )
        .subcommand(
            "db",
            super::db::db_api::<C>().with_about("about.commands-interact-with-db-dump-apply"),
        )
        .subcommand(
            "auth",
            super::auth::auth_api::<C>().with_about("about.add-or-remove-authorized-clients"),
        )
        .subcommand(
            "subnet",
            subnet_api::<C>().with_about("about.add-remove-or-modify-subnets"),
        )
        .subcommand(
            "device",
            device_api::<C>().with_about("about.add-remove-or-list-devices-in-subnets"),
        )
        .subcommand(
            "dns",
            dns_api::<C>().with_about("about.view-or-edit-injected-dns-records"),
        )
        .subcommand(
            "port-forward",
            ParentHandler::<C>::new()
                .subcommand(
                    "add",
                    from_fn_async(add_forward)
                        .with_metadata("sync_db", Value::Bool(true))
                        .no_display()
                        .with_about("about.add-new-port-forward")
                        .with_call_remote::<CliContext>(),
                )
                .subcommand(
                    "remove",
                    from_fn_async(remove_forward)
                        .with_metadata("sync_db", Value::Bool(true))
                        .no_display()
                        .with_about("about.remove-port-forward")
                        .with_call_remote::<CliContext>(),
                )
                .subcommand(
                    "update-label",
                    from_fn_async(update_forward_label)
                        .with_metadata("sync_db", Value::Bool(true))
                        .no_display()
                        .with_about("about.update-port-forward-label")
                        .with_call_remote::<CliContext>(),
                )
                .subcommand(
                    "set-enabled",
                    from_fn_async(set_forward_enabled)
                        .with_metadata("sync_db", Value::Bool(true))
                        .no_display()
                        .with_about("about.enable-or-disable-port-forward")
                        .with_call_remote::<CliContext>(),
                )
                .with_about("about.commands-port-forward"),
        )
        .subcommand(
            "pinhole",
            ParentHandler::<C>::new()
                .subcommand(
                    "add",
                    from_fn_async(add_pinhole)
                        .with_metadata("sync_db", Value::Bool(true))
                        .no_display()
                        .with_about("about.add-new-pinhole")
                        .with_call_remote::<CliContext>(),
                )
                .subcommand(
                    "remove",
                    from_fn_async(remove_pinhole)
                        .with_metadata("sync_db", Value::Bool(true))
                        .no_display()
                        .with_about("about.remove-pinhole")
                        .with_call_remote::<CliContext>(),
                )
                .subcommand(
                    "update-label",
                    from_fn_async(update_pinhole_label)
                        .with_metadata("sync_db", Value::Bool(true))
                        .no_display()
                        .with_about("about.update-pinhole-label")
                        .with_call_remote::<CliContext>(),
                )
                .subcommand(
                    "set-enabled",
                    from_fn_async(set_pinhole_enabled)
                        .with_metadata("sync_db", Value::Bool(true))
                        .no_display()
                        .with_about("about.enable-or-disable-pinhole")
                        .with_call_remote::<CliContext>(),
                )
                .with_about("about.commands-pinhole"),
        )
        .subcommand(
            "restart",
            from_fn_async(restart)
                .no_display()
                .with_about("about.restart-tunnel")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "update",
            ParentHandler::<C>::new()
                .subcommand(
                    "check",
                    from_fn_async(super::update::check_update)
                        .with_display_serializable()
                        .with_about("about.check-for-updates")
                        .with_call_remote::<CliContext>(),
                )
                .subcommand(
                    "apply",
                    from_fn_async(super::update::apply_update)
                        .with_display_serializable()
                        .with_about("about.apply-available-update")
                        .with_call_remote::<CliContext>(),
                )
                .with_about("about.commands-tunnel-update"),
        )
}

pub async fn restart(ctx: TunnelContext) -> Result<(), Error> {
    ctx.shutdown.send(Some(true)).ok();
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct SubnetParams {
    #[ts(type = "string")]
    subnet: Ipv4Net,
}

pub fn subnet_api<C: Context>() -> ParentHandler<C, SubnetParams> {
    ParentHandler::new()
        .subcommand(
            "add",
            from_fn_async(add_subnet)
                .with_metadata("sync_db", Value::Bool(true))
                .with_inherited(|a, _| a)
                .no_display()
                .with_about("about.add-new-subnet")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove_subnet)
                .with_metadata("sync_db", Value::Bool(true))
                .with_inherited(|a, _| a)
                .no_display()
                .with_about("about.remove-subnet")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-dns",
            from_fn_async(set_subnet_dns)
                .with_metadata("sync_db", Value::Bool(true))
                .with_inherited(|a, _| a)
                .no_display()
                .with_about("about.set-subnet-dns")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-wan",
            from_fn_async(set_subnet_wan)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.set-subnet-wan")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-ipv6",
            from_fn_async(set_subnet_ipv6)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.set-subnet-ipv6")
                .with_call_remote::<CliContext>(),
        )
}

pub fn device_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "add",
            from_fn_async(add_device)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.add-device-to-subnet")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove_device)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.remove-device-from-subnet")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "list",
            from_fn_async(list_devices)
                .with_display_serializable()
                .with_custom_display_fn(|HandlerArgs { params, .. }, res| {
                    use prettytable::*;

                    if let Some(format) = params.format {
                        return display_serializable(format, res);
                    }

                    let mut table = Table::new();
                    table.add_row(row![bc => "NAME", "IP", "PUBLIC KEY"]);
                    for (ip, config) in res.clients.0 {
                        table.add_row(row![config.name, ip, config.key.verifying_key()]);
                    }

                    table.print_tty(false)?;

                    Ok(())
                })
                .with_about("about.list-devices-in-subnet")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "show-config",
            from_fn_async(show_config)
                .with_about("about.show-wireguard-configuration-for-device")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-dns-injection",
            from_fn_async(set_dns_injection)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.allow-or-deny-device-dns-injection")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-auto-port-forward",
            from_fn_async(set_auto_port_forward)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.allow-or-deny-device-auto-port-forward")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-wan",
            from_fn_async(set_device_wan)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.set-device-wan")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-kind",
            from_fn_async(set_device_kind)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.promote-or-demote-device-kind")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct SetDnsInjectionParams {
    #[ts(type = "string")]
    subnet: Ipv4Net,
    #[ts(type = "string")]
    ip: Ipv4Addr,
    #[arg(long)]
    enabled: bool,
}

/// Allow/deny a device to inject DNS records via RFC 2136. Off by default: an
/// allowed device can add records the whole tunnel resolves, so trust only.
pub async fn set_dns_injection(
    ctx: TunnelContext,
    SetDnsInjectionParams {
        subnet,
        ip,
        enabled,
    }: SetDnsInjectionParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_wg_mut()
                .as_subnets_mut()
                .as_idx_mut(&subnet)
                .or_not_found(&subnet)?
                .as_clients_mut()
                .as_idx_mut(&ip)
                .or_not_found(&ip)?
                .as_allow_dns_injection_mut()
                .ser(&enabled)
        })
        .await
        .result?;
    ctx.dns_allowed.mutate(|s| {
        if enabled {
            s.insert(IpAddr::V4(ip));
        } else {
            s.remove(&IpAddr::V4(ip));
        }
    });
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct SetAutoPortForwardParams {
    #[ts(type = "string")]
    subnet: Ipv4Net,
    #[ts(type = "string")]
    ip: Ipv4Addr,
    #[arg(long)]
    enabled: bool,
}

/// Allow/deny a device to auto-create port forwards via PCP/IGD. Off by
/// default; paired with DNS injection under the gateway-autoconfig toggle.
/// `is_known_client` reads this live, so no cache to update here.
pub async fn set_auto_port_forward(
    ctx: TunnelContext,
    SetAutoPortForwardParams {
        subnet,
        ip,
        enabled,
    }: SetAutoPortForwardParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_wg_mut()
                .as_subnets_mut()
                .as_idx_mut(&subnet)
                .or_not_found(&subnet)?
                .as_clients_mut()
                .as_idx_mut(&ip)
                .or_not_found(&ip)?
                .as_allow_auto_port_forward_mut()
                .ser(&enabled)
        })
        .await
        .result?;
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct SetDeviceKindParams {
    #[ts(type = "string")]
    subnet: Ipv4Net,
    #[ts(type = "string")]
    ip: Ipv4Addr,
    #[arg(long, value_enum)]
    kind: WgClientKind,
}

/// Promote a device to Server or demote to Client. The role is sticky, but the
/// transition resets both capability flags to the kind's default (Server: both
/// on; Client: both off), since a Client has no autoconfig.
pub async fn set_device_kind(
    ctx: TunnelContext,
    SetDeviceKindParams { subnet, ip, kind }: SetDeviceKindParams,
) -> Result<(), Error> {
    let autoconfig = matches!(kind, WgClientKind::Server);
    ctx.db
        .mutate(|db| {
            db.as_wg_mut()
                .as_subnets_mut()
                .as_idx_mut(&subnet)
                .or_not_found(&subnet)?
                .as_clients_mut()
                .as_idx_mut(&ip)
                .or_not_found(&ip)?
                .as_kind_mut()
                .ser(&kind)?;
            db.as_wg_mut()
                .as_subnets_mut()
                .as_idx_mut(&subnet)
                .or_not_found(&subnet)?
                .as_clients_mut()
                .as_idx_mut(&ip)
                .or_not_found(&ip)?
                .as_allow_dns_injection_mut()
                .ser(&autoconfig)?;
            db.as_wg_mut()
                .as_subnets_mut()
                .as_idx_mut(&subnet)
                .or_not_found(&subnet)?
                .as_clients_mut()
                .as_idx_mut(&ip)
                .or_not_found(&ip)?
                .as_allow_auto_port_forward_mut()
                .ser(&autoconfig)
        })
        .await
        .result?;
    ctx.dns_allowed.mutate(|s| {
        if autoconfig {
            s.insert(IpAddr::V4(ip));
        } else {
            s.remove(&IpAddr::V4(ip));
        }
    });
    Ok(())
}

pub fn dns_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async(list_dns_records)
                .with_display_serializable()
                .with_about("about.list-injected-dns-records")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "add",
            from_fn_async(add_dns_record)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.add-or-replace-a-dns-record")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove_dns_record)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.remove-a-dns-record")
                .with_call_remote::<CliContext>(),
        )
}

pub async fn list_dns_records(ctx: TunnelContext) -> Result<Vec<DnsRecordEntry>, Error> {
    Ok(ctx
        .dns_injector
        .list()
        .iter()
        .map(|r| {
            let (name, rtype, value, ttl, source) = r.to_parts();
            DnsRecordEntry {
                name,
                rtype,
                value,
                ttl,
                source: source.map(|s| s.to_string()),
            }
        })
        .collect())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct AddDnsRecordParams {
    name: String,
    #[serde(rename = "type")]
    #[arg(long = "type")]
    rtype: String,
    value: String,
    #[arg(long)]
    ttl: Option<u32>,
}

pub async fn add_dns_record(
    ctx: TunnelContext,
    AddDnsRecordParams {
        name,
        rtype,
        value,
        ttl,
    }: AddDnsRecordParams,
) -> Result<(), Error> {
    let record = InjectedRecord::from_parts(
        &name,
        &rtype,
        &value,
        ttl.unwrap_or(300),
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
    )?;
    ctx.dns_injector.upsert(record);
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct RemoveDnsRecordParams {
    name: String,
    #[serde(rename = "type")]
    #[arg(long = "type")]
    rtype: Option<String>,
}

pub async fn remove_dns_record(
    ctx: TunnelContext,
    RemoveDnsRecordParams { name, rtype }: RemoveDnsRecordParams,
) -> Result<(), Error> {
    let mut fqdn = Name::from_utf8(&name).with_kind(ErrorKind::ParseUrl)?;
    fqdn.set_fqdn(true);
    let rtype = rtype
        .as_deref()
        .map(|s| RecordType::from_str(&s.to_ascii_uppercase()))
        .transpose()
        .with_kind(ErrorKind::InvalidRequest)?;
    ctx.dns_injector.delete(&fqdn, rtype);
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct AddSubnetParams {
    name: InternedString,
}

pub async fn add_subnet(
    ctx: TunnelContext,
    AddSubnetParams { name }: AddSubnetParams,
    SubnetParams { mut subnet }: SubnetParams,
) -> Result<(), Error> {
    if subnet.prefix_len() > 24 {
        return Err(Error::new(
            eyre!("invalid subnet"),
            ErrorKind::InvalidRequest,
        ));
    }
    let addr = subnet
        .hosts()
        .next()
        .ok_or_else(|| Error::new(eyre!("invalid subnet"), ErrorKind::InvalidRequest))?;
    subnet = Ipv4Net::new_assert(addr, subnet.prefix_len());
    let server = ctx
        .db
        .mutate(|db| {
            let map = db.as_wg_mut().as_subnets_mut();
            if let Some(s) = map
                .keys()?
                .into_iter()
                .find(|s| s != &subnet && (s.contains(&subnet) || subnet.contains(s)))
            {
                return Err(Error::new(
                    eyre!("{subnet} overlaps with existing subnet {s}"),
                    ErrorKind::InvalidRequest,
                ));
            }
            map.upsert(&subnet, || {
                Ok(WgSubnetConfig::new(InternedString::default()))
            })?
            .as_name_mut()
            .ser(&name)?;
            db.as_wg().de()
        })
        .await
        .result?;
    // sync_network → resync_egress installs this subnet's postrouting rule.
    ctx.sync_network(&server).await?;

    Ok(())
}

pub async fn remove_subnet(
    ctx: TunnelContext,
    _: Empty,
    SubnetParams { subnet }: SubnetParams,
) -> Result<(), Error> {
    let (server, (keep, dropped_sni)) = ctx
        .db
        .mutate(|db| {
            db.as_wg_mut().as_subnets_mut().remove(&subnet)?;
            Ok((db.as_wg().de()?, db.gc_forwards()?))
        })
        .await
        .result?;
    ctx.sync_network(&server).await?;
    ctx.gc_forwards(&keep, &dropped_sni).await?;

    for iface in ctx.net_iface.peek(|i| {
        i.iter()
            .filter(|(_, info)| {
                info.ip_info.as_ref().map_or(false, |i| {
                    i.device_type != Some(NetworkInterfaceType::Loopback)
                })
            })
            .map(|(name, _)| name)
            .filter(|id| id.as_str() != WIREGUARD_INTERFACE_NAME)
            .cloned()
            .collect::<Vec<_>>()
    }) {
        let net = subnet.trunc();
        nft_rule(
            "postrouting",
            &format!("tunnel-masq-{net}-{iface}"),
            true,
            false,
            &format!("ip saddr {net} oifname \"{iface}\" masquerade"),
        )
        .await?;
    }

    Ok(())
}

/// Which upstream a subnet's DNS proxy forwards to. `Device`/`Custom` draw
/// their data from companion fields on [`SetSubnetDnsParams`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, TS, ValueEnum)]
#[serde(rename_all = "camelCase")]
pub enum DnsMode {
    Default,
    Device,
    Custom,
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct SetSubnetDnsParams {
    #[arg(long, help = "help.arg.dns-mode")]
    mode: DnsMode,
    /// The selected device's WireGuard IP; required when `mode` is `device`.
    #[arg(long, help = "help.arg.dns-device-ip")]
    #[ts(type = "string | null")]
    device_ip: Option<Ipv4Addr>,
    /// Up to 3 upstream servers (bare IP or `ip:port`); used when `mode` is `custom`.
    #[arg(long = "server", action = clap::ArgAction::Append, help = "help.arg.dns-server")]
    #[ts(type = "string[]")]
    servers: Vec<String>,
}

/// Parse a DNS upstream; a bare IP defaults to port 53.
fn parse_dns_server(s: &str) -> Result<SocketAddr, Error> {
    if let Ok(ip) = s.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, 53));
    }
    s.parse::<SocketAddr>().map_err(|_| {
        Error::new(
            eyre!("invalid DNS server `{s}` (expected an IP or IP:port)"),
            ErrorKind::InvalidRequest,
        )
    })
}

pub async fn set_subnet_dns(
    ctx: TunnelContext,
    SetSubnetDnsParams {
        mode,
        device_ip,
        servers,
    }: SetSubnetDnsParams,
    SubnetParams { subnet }: SubnetParams,
) -> Result<(), Error> {
    let dns = match mode {
        DnsMode::Default => DnsConfig::Default,
        DnsMode::Device => DnsConfig::Device {
            ip: device_ip.ok_or_else(|| {
                Error::new(
                    eyre!("device DNS requires --device-ip"),
                    ErrorKind::InvalidRequest,
                )
            })?,
        },
        DnsMode::Custom => {
            if servers.is_empty() || servers.len() > 3 {
                return Err(Error::new(
                    eyre!("custom DNS requires between 1 and 3 servers"),
                    ErrorKind::InvalidRequest,
                ));
            }
            DnsConfig::Custom {
                servers: servers
                    .iter()
                    .map(|s| parse_dns_server(s))
                    .collect::<Result<_, _>>()?,
            }
        }
    };

    let server = ctx
        .db
        .mutate(|db| {
            let subnet_model = db
                .as_wg_mut()
                .as_subnets_mut()
                .as_idx_mut(&subnet)
                .or_not_found(&subnet)?;
            if let DnsConfig::Device { ip } = &dns {
                if subnet_model.as_clients().as_idx(ip).is_none() {
                    return Err(Error::new(
                        eyre!("no device with ip {ip} on subnet {subnet}"),
                        ErrorKind::InvalidRequest,
                    ));
                }
            }
            subnet_model.as_dns_mut().ser(&dns)?;
            db.as_wg().de()
        })
        .await
        .result?;

    // Client configs always point DNS at the subnet's `.1`, so a mode switch
    // only changes the proxy's upstreams — no `server.sync()` / wg-quick bounce.
    ctx.dns_proxy.sync(&server, ctx.dns_injector.clone()).await
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct SetSubnetWanParams {
    #[ts(type = "string")]
    subnet: Ipv4Net,
    #[arg(long)]
    #[ts(type = "string | null")]
    wan_ip: Option<Ipv4Addr>,
}

/// Pin the WAN IP a subnet's egress SNATs to; `null` falls back to masquerade.
/// Per-device overrides still take precedence.
pub async fn set_subnet_wan(
    ctx: TunnelContext,
    SetSubnetWanParams { subnet, wan_ip }: SetSubnetWanParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_wg_mut()
                .as_subnets_mut()
                .as_idx_mut(&subnet)
                .or_not_found(&subnet)?
                .as_wan_ip_mut()
                .ser(&wan_ip)
        })
        .await
        .result?;
    ctx.resync_egress().await?;
    ctx.resync_forward_keys().await
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct SetSubnetIpv6Params {
    #[ts(type = "string")]
    subnet: Ipv4Net,
    /// The routed IPv6 prefix delegated to this subnet (e.g. a /64 from Hetzner,
    /// a /56 from Linode). `null` disables IPv6 on the subnet.
    #[arg(long)]
    #[ts(type = "string | null")]
    prefix: Option<Ipv6Net>,
}

/// Set (or, with `null`, clear) the routed IPv6 prefix delegated to a subnet.
/// Every host on the subnet (the server and each client) is assigned one global
/// `/128` out of it, and the WireGuard configs are re-rendered to carry them.
/// Validates that the server can actually route the prefix before persisting.
pub async fn set_subnet_ipv6(
    ctx: TunnelContext,
    SetSubnetIpv6Params { subnet, prefix }: SetSubnetIpv6Params,
) -> Result<(), Error> {
    let prefix = prefix
        .map(|p| {
            let net = p.trunc();
            let addr = net.network();
            if addr.is_loopback()
                || addr.is_unspecified()
                || addr.is_multicast()
                || addr.is_unicast_link_local()
            {
                return Err(Error::new(
                    eyre!("{net} is not a usable routed prefix"),
                    ErrorKind::InvalidRequest,
                ));
            }
            Ok(net)
        })
        .transpose()?;

    if let Some(net) = prefix {
        // A delegated prefix is useless without working IPv6 egress: clients
        // would get a full-tunnel `::/0` that blackholes. Reject at set-time
        // (before mutating the DB) rather than silently handing out dead IPv6.
        if !crate::net::utils::has_ipv6_default_route().await? {
            return Err(Error::new(
                eyre!(
                    "this tunnel server has no IPv6 connectivity (no IPv6 default route); configure IPv6 on the server before delegating an IPv6 prefix"
                ),
                ErrorKind::Network,
            ));
        }

        // On-link prefixes are delivered via proxy-NDP (see `resync_v6`). A
        // prefix that is not on-link may still be a valid provider-routed
        // delegation (a /56 or /64 the VPS routes to this host), which we can't
        // verify locally — warn instead of rejecting, so we don't false-reject
        // valid routed delegations.
        let on_link = ctx.net_iface.peek(|ifaces| {
            for (id, info) in ifaces.iter() {
                if id.as_str() == WIREGUARD_INTERFACE_NAME {
                    continue;
                }
                let Some(ip) = info.ip_info.as_ref() else {
                    continue;
                };
                if ip.device_type == Some(NetworkInterfaceType::Loopback) {
                    continue;
                }
                for subnet in ip.subnets.iter() {
                    if let IpNet::V6(n) = subnet {
                        // Only a global address on the WAN link makes the prefix
                        // reachable on-link; exclude link-local / ULA / loopback.
                        if crate::net::utils::ipv6_is_local(n.addr()) {
                            continue;
                        }
                        let n = n.trunc();
                        if n.contains(&net) || net.contains(&n) {
                            return true;
                        }
                    }
                }
            }
            false
        });
        if !on_link {
            tracing::warn!(
                "prefix {net} is not on-link on any WAN interface; ensure your provider routes it to this host, otherwise clients will have no IPv6"
            );
        }

    }

    ctx.db
        .mutate(|db| {
            let subnet_model = db
                .as_wg_mut()
                .as_subnets_mut()
                .as_idx_mut(&subnet)
                .or_not_found(&subnet)?;
            // Reject a prefix too small to give every host on the subnet (the
            // server `.1` and each device) a distinct IPv6 — inside the mutate so
            // a concurrent add can't slip a colliding device in between the check
            // and the write.
            if let Some(net) = prefix {
                let clients = subnet_model.as_clients().de()?;
                let hosts = std::iter::once(subnet.addr()).chain(clients.0.into_keys());
                if let Some((a, b, addr)) = wg6::first_v6_collision(net, hosts) {
                    return Err(Error::new(
                        eyre!(
                            "prefix {net} is too small: {a} and {b} would both get IPv6 {addr}; give them IPs whose low bits differ, or use a larger prefix"
                        ),
                        ErrorKind::InvalidRequest,
                    ));
                }
            }
            subnet_model.as_ipv6_mut().ser(&prefix)
        })
        .await
        .result?;
    let server = ctx.db.peek().await.as_wg().de()?;
    ctx.sync_network(&server).await
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct SetDeviceWanParams {
    #[ts(type = "string")]
    subnet: Ipv4Net,
    #[ts(type = "string")]
    ip: Ipv4Addr,
    #[arg(long)]
    #[ts(type = "string | null")]
    wan_ip: Option<Ipv4Addr>,
}

/// Pin the WAN IP a device's egress SNATs to, overriding its subnet's `wan_ip`.
/// `null` falls back to the subnet rule / masquerade.
pub async fn set_device_wan(
    ctx: TunnelContext,
    SetDeviceWanParams {
        subnet,
        ip,
        wan_ip,
    }: SetDeviceWanParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_wg_mut()
                .as_subnets_mut()
                .as_idx_mut(&subnet)
                .or_not_found(&subnet)?
                .as_clients_mut()
                .as_idx_mut(&ip)
                .or_not_found(&ip)?
                .as_wan_ip_mut()
                .ser(&wan_ip)
        })
        .await
        .result?;
    ctx.resync_egress().await?;
    ctx.resync_forward_keys().await
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct AddDeviceParams {
    #[ts(type = "string")]
    subnet: Ipv4Net,
    name: InternedString,
    #[ts(type = "string | null")]
    ip: Option<Ipv4Addr>,
    /// Client (no autoconfig) or Server (gateway-autoconfig on by default).
    #[serde(default)]
    #[arg(long, value_enum, default_value = "client")]
    kind: WgClientKind,
}

pub async fn add_device(
    ctx: TunnelContext,
    AddDeviceParams {
        subnet,
        name,
        ip,
        kind,
    }: AddDeviceParams,
) -> Result<(), Error> {
    let server = ctx
        .db
        .mutate(|db| {
            let subnet_model = db
                .as_wg_mut()
                .as_subnets_mut()
                .as_idx_mut(&subnet)
                .or_not_found(&subnet)?;
            // The subnet's IPv6 prefix (if any) — every host gets a /128 out of
            // it, so a new device must not map to one already taken.
            let prefix = subnet_model.as_ipv6().de()?;
            subnet_model
                .as_clients_mut()
                .mutate(|WgSubnetClients(clients)| {
                    let server_v4 = subnet.addr();
                    let ip = if let Some(ip) = ip {
                        ip
                    } else {
                        // Auto-assign the first free IP that also has a free IPv6
                        // (a small prefix can exhaust IPv6 before IPv4).
                        subnet
                            .hosts()
                            .find(|ip| {
                                !clients.contains_key(ip)
                                    && *ip != server_v4
                                    && prefix.map_or(true, |p| {
                                        wg6::v6_conflict(
                                            p,
                                            *ip,
                                            std::iter::once(server_v4)
                                                .chain(clients.keys().copied()),
                                        )
                                        .is_none()
                                    })
                            })
                            .ok_or_else(|| {
                                Error::new(
                                    eyre!("no available ips in subnet"),
                                    ErrorKind::InvalidRequest,
                                )
                            })?
                    };

                    if ip.octets()[3] == 0 || ip.octets()[3] == 255 {
                        return Err(Error::new(eyre!("invalid ip"), ErrorKind::InvalidRequest));
                    }
                    if ip == server_v4 {
                        return Err(Error::new(eyre!("invalid ip"), ErrorKind::InvalidRequest));
                    }
                    if !subnet.contains(&ip) {
                        return Err(Error::new(
                            eyre!("ip not in subnet"),
                            ErrorKind::InvalidRequest,
                        ));
                    }
                    // No two hosts may share an IPv6 address.
                    if let Some(p) = prefix {
                        let existing = std::iter::once(server_v4)
                            .chain(clients.keys().copied().filter(|c| *c != ip));
                        if let Some(conflict) = wg6::v6_conflict(p, ip, existing) {
                            let addr = wg6::host_v6(p, ip);
                            let with = if conflict == server_v4 {
                                "the tunnel itself".to_string()
                            } else {
                                format!("device {conflict}")
                            };
                            return Err(Error::new(
                                eyre!(
                                    "device {ip} would take IPv6 {addr}, which collides with {with}; choose an IP whose low bits differ, or use a larger IPv6 prefix"
                                ),
                                ErrorKind::InvalidRequest,
                            ));
                        }
                    }
                    let client = clients
                        .entry(ip)
                        .or_insert_with(|| WgConfig::generate(name.clone(), kind));
                    client.name = name;

                    Ok(())
                })?;
            db.as_wg().de()
        })
        .await
        .result?;
    ctx.sync_network(&server).await
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct RemoveDeviceParams {
    #[ts(type = "string")]
    subnet: Ipv4Net,
    #[ts(type = "string")]
    ip: Ipv4Addr,
}

pub async fn remove_device(
    ctx: TunnelContext,
    RemoveDeviceParams { subnet, ip }: RemoveDeviceParams,
) -> Result<(), Error> {
    let (server, (keep, dropped_sni)) = ctx
        .db
        .mutate(|db| {
            db.as_wg_mut()
                .as_subnets_mut()
                .as_idx_mut(&subnet)
                .or_not_found(&subnet)?
                .as_clients_mut()
                .remove(&ip)?
                .or_not_found(&ip)?;
            Ok((db.as_wg().de()?, db.gc_forwards()?))
        })
        .await
        .result?;
    ctx.sync_network(&server).await?;
    ctx.gc_forwards(&keep, &dropped_sni).await
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct ListDevicesParams {
    #[ts(type = "string")]
    subnet: Ipv4Net,
}

pub async fn list_devices(
    ctx: TunnelContext,
    ListDevicesParams { subnet }: ListDevicesParams,
) -> Result<WgSubnetConfig, Error> {
    ctx.db
        .peek()
        .await
        .as_wg()
        .as_subnets()
        .as_idx(&subnet)
        .or_not_found(&subnet)?
        .de()
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct ShowConfigParams {
    #[ts(type = "string")]
    subnet: Ipv4Net,
    #[ts(type = "string")]
    ip: Ipv4Addr,
    #[ts(type = "string | null")]
    wan_addr: Option<IpAddr>,
    #[serde(rename = "__ConnectInfo_local_addr")]
    #[arg(skip)]
    #[ts(skip)]
    local_addr: Option<SocketAddr>,
}

pub async fn show_config(
    ctx: TunnelContext,
    ShowConfigParams {
        subnet,
        ip,
        wan_addr,
        local_addr,
    }: ShowConfigParams,
) -> Result<String, Error> {
    let peek = ctx.db.peek().await;
    let wg = peek.as_wg();
    let subnet_model = wg.as_subnets().as_idx(&subnet).or_not_found(&subnet)?;
    let subnet_v6 = subnet_model.as_ipv6().de()?;
    let client = subnet_model
        .as_clients()
        .as_idx(&ip)
        .or_not_found(&ip)?
        .de()?;
    let wan_addr = if let Some(wan_addr) = wan_addr.or(local_addr.map(|a| a.ip())).filter(|ip| {
        !ip.is_loopback()
            && !match ip {
                IpAddr::V4(ipv4) => ipv4.is_private() || ipv4.is_link_local(),
                IpAddr::V6(ipv6) => ipv6.is_unique_local() || ipv6.is_unicast_link_local(),
            }
    }) {
        wan_addr
    } else if let Some(webserver) = peek.as_webserver().as_listen().de()? {
        webserver.ip()
    } else {
        ctx.net_iface
            .peek(|i| {
                i.iter().find_map(|(_, info)| {
                    info.ip_info
                        .as_ref()
                        .and_then(|ip_info| ip_info.wan_ip)
                        .map(IpAddr::from)
                })
            })
            .or_not_found("a public IP address")?
    };
    Ok(client
        .client_config(
            ip,
            subnet,
            wg.as_key().de()?.verifying_key(),
            (wan_addr, wg.as_port().de()?).into(),
            subnet_v6.map(|p| crate::tunnel::wg6::host_v6(p, ip)),
        )
        .to_string())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct AddPortForwardParams {
    /// External (WAN) port to forward. The external IP is fixed to the target's
    /// WAN so return traffic stays symmetric.
    external_port: u16,
    #[ts(type = "string")]
    target: SocketAddrV4,
    #[arg(long)]
    label: Option<String>,
    /// Hostnames to SNI-demux on the shared external port. Empty = normal DNAT.
    #[arg(long = "sni")]
    #[serde(default)]
    sni: Vec<String>,
    /// Number of contiguous ports to forward (a PCP PORT_SET range), counting up
    /// from both `external_port` and the target port. Defaults to 1. Not valid
    /// together with SNI demux.
    #[arg(long)]
    #[serde(default)]
    #[ts(optional)]
    count: Option<u16>,
}

pub async fn add_forward(
    ctx: TunnelContext,
    AddPortForwardParams {
        external_port,
        target,
        label,
        sni,
        count,
    }: AddPortForwardParams,
) -> Result<(), Error> {
    let count = count.unwrap_or(1);
    if count == 0 {
        return Err(Error::new(
            eyre!("count must be at least 1"),
            ErrorKind::InvalidRequest,
        ));
    }

    let external_ip = ctx.external_ipv4(*target.ip()).await.ok_or_else(|| {
        Error::new(
            eyre!("no WAN IP available for device {}", target.ip()),
            ErrorKind::Network,
        )
    })?;
    let source = SocketAddrV4::new(external_ip, external_port);
    if !sni.is_empty() {
        if count > 1 {
            return Err(Error::new(
                eyre!("SNI demux does not support port ranges"),
                ErrorKind::InvalidRequest,
            ));
        }
        ctx.add_sni_forward(source, target, &sni, None)
            .await
            .map_err(|code| {
                Error::new(
                    eyre!("SNI registration failed (code {code})"),
                    ErrorKind::InvalidRequest,
                )
            })?;
        return Ok(());
    }

    // The span must fit the u16 port range on both the external and target side.
    if external_port.checked_add(count - 1).is_none()
        || target.port().checked_add(count - 1).is_none()
    {
        return Err(Error::new(
            eyre!(
                "port range of {count} starting at {external_port} (-> {}) exceeds 65535",
                target.port(),
            ),
            ErrorKind::InvalidRequest,
        ));
    }

    // DB is the source of truth: atomically reject an overlapping or duplicate
    // forward and reserve the slot in one mutate before touching the dataplane
    // (mirrors add_sni_forward). A detached peek-then-insert let two concurrent
    // adds both pass the overlap check.
    ctx.db
        .mutate(|db| {
            db.as_port_forwards_mut().mutate(|pf| {
                if let Some(conflict) = pf.overlapping(source, count) {
                    return Err(Error::new(
                        eyre!(
                            "external ports {external_port}-{} overlap an existing forward at {conflict}",
                            external_port + count - 1,
                        ),
                        ErrorKind::InvalidRequest,
                    ));
                }
                if pf
                    .0
                    .insert(
                        source,
                        PortForward::Dnat {
                            target,
                            label: label.clone(),
                            enabled: true,
                            count,
                            auto: false,
                        },
                    )
                    .is_some()
                {
                    return Err(Error::new(
                        eyre!("Port forward from {source} already exists"),
                        ErrorKind::InvalidRequest,
                    ));
                }
                Ok(())
            })
        })
        .await
        .result?;

    // Install the dataplane forward; roll back the reservation on failure.
    let prefix = crate::tunnel::forward::igd::prefix_for(&ctx, target.ip()).await;
    let rc = match ctx
        .forward
        .add_forward_range(source, target, count, prefix, None)
        .await
    {
        Ok(rc) => rc,
        Err(e) => {
            ctx.db
                .mutate(|db| {
                    db.as_port_forwards_mut().mutate(|pf| {
                        pf.0.remove(&source);
                        Ok(())
                    })
                })
                .await
                .result
                .ok();
            return Err(e);
        }
    };
    ctx.active_forwards.mutate(|m| {
        m.insert(source, rc);
    });

    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct RemovePortForwardParams {
    #[ts(type = "string")]
    source: SocketAddrV4,
    /// Remove a single SNI route on `source`; omit to remove the whole forward.
    #[arg(long)]
    #[serde(default)]
    hostname: Option<String>,
}

pub async fn remove_forward(
    ctx: TunnelContext,
    RemovePortForwardParams { source, hostname }: RemovePortForwardParams,
) -> Result<(), Error> {
    let entry = ctx
        .db
        .peek()
        .await
        .as_port_forwards()
        .de()?
        .0
        .get(&source)
        .cloned();
    match entry {
        Some(PortForward::Sni { routes }) => {
            let to_remove: Vec<(String, SocketAddrV4)> = match &hostname {
                Some(h) => routes
                    .get(h)
                    .map(|r| vec![(h.clone(), r.target)])
                    .unwrap_or_default(),
                None => routes.iter().map(|(h, r)| (h.clone(), r.target)).collect(),
            };
            for (h, route_target) in to_remove {
                ctx.remove_sni_forward(source, route_target, &[h]).await;
            }
            Ok(())
        }
        Some(PortForward::Dnat { .. }) => {
            ctx.db
                .mutate(|db| db.as_port_forwards_mut().remove(&source))
                .await
                .result?;
            if let Some(rc) = ctx.active_forwards.mutate(|m| m.remove(&source)) {
                drop(rc);
                ctx.forward.gc().await?;
            }
            Ok(())
        }
        None => Ok(()),
    }
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePortForwardLabelParams {
    #[ts(type = "string")]
    source: SocketAddrV4,
    label: Option<String>,
    /// Label a single SNI route on `source`; omit to label the DNAT forward.
    #[arg(long)]
    #[serde(default)]
    hostname: Option<String>,
}

pub async fn update_forward_label(
    ctx: TunnelContext,
    UpdatePortForwardLabelParams {
        source,
        label,
        hostname,
    }: UpdatePortForwardLabelParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_port_forwards_mut().mutate(|pf| {
                let entry = pf.0.get_mut(&source).ok_or_else(|| {
                    Error::new(
                        eyre!("Port forward from {source} not found"),
                        ErrorKind::NotFound,
                    )
                })?;
                match entry {
                    PortForward::Dnat { label: l, .. } => {
                        *l = label;
                        Ok(())
                    }
                    PortForward::Sni { routes } => {
                        let hostname = hostname.ok_or_else(|| {
                            Error::new(
                                eyre!("--hostname is required to label an SNI route"),
                                ErrorKind::InvalidRequest,
                            )
                        })?;
                        let route = routes.get_mut(&hostname).ok_or_else(|| {
                            Error::new(
                                eyre!("No SNI route for {hostname} on {source}"),
                                ErrorKind::NotFound,
                            )
                        })?;
                        route.label = label;
                        Ok(())
                    }
                }
            })
        })
        .await
        .result
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct SetPortForwardEnabledParams {
    #[ts(type = "string")]
    source: SocketAddrV4,
    #[arg(long)]
    enabled: bool,
    /// Toggle a single SNI route on `source`; omit for a DNAT forward.
    #[arg(long)]
    #[serde(default)]
    hostname: Option<String>,
}

/// Carries what the db.mutate selected so the dataplane action runs after it.
enum ForwardToggle {
    Dnat(SocketAddrV4),
    Sni { hostname: String, target: SocketAddrV4 },
}

pub async fn set_forward_enabled(
    ctx: TunnelContext,
    SetPortForwardEnabledParams {
        source,
        enabled,
        hostname,
    }: SetPortForwardEnabledParams,
) -> Result<(), Error> {
    let toggle = ctx
        .db
        .mutate(|db| {
            db.as_port_forwards_mut().mutate(|pf| {
                let entry = pf.0.get_mut(&source).ok_or_else(|| {
                    Error::new(
                        eyre!("Port forward from {source} not found"),
                        ErrorKind::NotFound,
                    )
                })?;
                match entry {
                    PortForward::Dnat {
                        enabled: e, target, ..
                    } => {
                        *e = enabled;
                        Ok(ForwardToggle::Dnat(*target))
                    }
                    PortForward::Sni { routes } => {
                        let hostname = hostname.clone().ok_or_else(|| {
                            Error::new(
                                eyre!("--hostname is required to toggle an SNI route"),
                                ErrorKind::InvalidRequest,
                            )
                        })?;
                        let route = routes.get_mut(&hostname).ok_or_else(|| {
                            Error::new(
                                eyre!("No SNI route for {hostname} on {source}"),
                                ErrorKind::NotFound,
                            )
                        })?;
                        route.enabled = enabled;
                        Ok(ForwardToggle::Sni {
                            hostname,
                            target: route.target,
                        })
                    }
                }
            })
        })
        .await
        .result?;

    match toggle {
        ForwardToggle::Dnat(target) => {
            if enabled {
                let prefix = crate::tunnel::forward::igd::prefix_for(&ctx, target.ip()).await;
                let rc = ctx
                    .forward
                    .add_forward(source, target, prefix, None)
                    .await?;
                ctx.active_forwards.mutate(|m| {
                    m.insert(source, rc);
                });
            } else if let Some(rc) = ctx.active_forwards.mutate(|m| m.remove(&source)) {
                drop(rc);
                ctx.forward.gc().await?;
            }
        }
        ForwardToggle::Sni { hostname, target } => {
            if enabled {
                ctx.sni()
                    .register(*source.ip(), source.port(), &[hostname], target, None)
                    .map_err(|code| {
                        Error::new(
                            eyre!("SNI registration failed (code {code})"),
                            ErrorKind::InvalidRequest,
                        )
                    })?;
            } else {
                ctx.sni()
                    .unregister(*source.ip(), source.port(), &[hostname], target);
            }
        }
    }

    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct AddPinholeParams {
    /// The client's global IPv6 (GUA) to expose. Must be an address this tunnel
    /// delegates to a client — its subnet needs an IPv6 prefix.
    #[ts(type = "string")]
    gua: Ipv6Addr,
    /// External port opened on the GUA.
    external_port: u16,
    /// Destination port on the GUA. Omit for a pure pinhole (no NAT, internal ==
    /// external); set a different value for a port remap (e.g. 80 -> 443).
    #[arg(long)]
    #[serde(default)]
    #[ts(optional)]
    internal_port: Option<u16>,
    #[arg(long)]
    label: Option<String>,
    /// Number of contiguous ports, counting up from external/internal. Default 1.
    #[arg(long)]
    #[serde(default)]
    #[ts(optional)]
    count: Option<u16>,
}

pub async fn add_pinhole(
    ctx: TunnelContext,
    AddPinholeParams {
        gua,
        external_port,
        internal_port,
        label,
        count,
    }: AddPinholeParams,
) -> Result<(), Error> {
    let count = count.unwrap_or(1);
    if count == 0 {
        return Err(Error::new(
            eyre!("count must be at least 1"),
            ErrorKind::InvalidRequest,
        ));
    }
    let internal = internal_port.unwrap_or(external_port);
    if external_port == 0 || internal == 0 {
        return Err(Error::new(
            eyre!("ports must be non-zero"),
            ErrorKind::InvalidRequest,
        ));
    }
    if external_port.checked_add(count - 1).is_none() || internal.checked_add(count - 1).is_none() {
        return Err(Error::new(
            eyre!("port range of {count} exceeds 65535"),
            ErrorKind::InvalidRequest,
        ));
    }
    if !pinhole::is_known_gua(&ctx, gua).await {
        return Err(Error::new(
            eyre!("{gua} is not a client address on any IPv6-enabled subnet"),
            ErrorKind::InvalidRequest,
        ));
    }
    pinhole::add_pinhole(&ctx, gua, external_port, internal, count, label, false).await
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct RemovePinholeParams {
    #[ts(type = "string")]
    gua: Ipv6Addr,
    external_port: u16,
}

pub async fn remove_pinhole(
    ctx: TunnelContext,
    RemovePinholeParams { gua, external_port }: RemovePinholeParams,
) -> Result<(), Error> {
    pinhole::remove_pinhole(&ctx, gua, external_port).await;
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePinholeLabelParams {
    #[ts(type = "string")]
    gua: Ipv6Addr,
    external_port: u16,
    label: Option<String>,
}

pub async fn update_pinhole_label(
    ctx: TunnelContext,
    UpdatePinholeLabelParams {
        gua,
        external_port,
        label,
    }: UpdatePinholeLabelParams,
) -> Result<(), Error> {
    pinhole::set_pinhole_label(&ctx, gua, external_port, label).await
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct SetPinholeEnabledParams {
    #[ts(type = "string")]
    gua: Ipv6Addr,
    external_port: u16,
    #[arg(long)]
    enabled: bool,
}

pub async fn set_pinhole_enabled(
    ctx: TunnelContext,
    SetPinholeEnabledParams {
        gua,
        external_port,
        enabled,
    }: SetPinholeEnabledParams,
) -> Result<(), Error> {
    pinhole::set_pinhole_enabled(&ctx, gua, external_port, enabled).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dns_server_defaults_to_port_53() {
        assert_eq!(
            parse_dns_server("1.1.1.1").unwrap(),
            "1.1.1.1:53".parse().unwrap()
        );
        assert_eq!(
            parse_dns_server("2606:4700:4700::1111").unwrap(),
            "[2606:4700:4700::1111]:53".parse().unwrap()
        );
    }

    #[test]
    fn parse_dns_server_accepts_explicit_port() {
        assert_eq!(
            parse_dns_server("9.9.9.9:5353").unwrap(),
            "9.9.9.9:5353".parse().unwrap()
        );
        assert_eq!(
            parse_dns_server("[2606:4700:4700::1111]:53").unwrap(),
            "[2606:4700:4700::1111]:53".parse().unwrap()
        );
    }

    #[test]
    fn parse_dns_server_rejects_invalid() {
        assert!(parse_dns_server("not-an-ip").is_err());
        assert!(parse_dns_server("1.1.1.1:99999").is_err());
        assert!(parse_dns_server("").is_err());
    }
}
