use std::collections::BTreeSet;
use std::net::{IpAddr, SocketAddr, SocketAddrV6};

use clap::Parser;
use imbl_value::InternedString;
use rpc_toolkit::{Context, Empty, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::GatewayId;
use crate::context::{CliContext, RpcContext};
use crate::db::model::DatabaseModel;
use crate::hostname::ServerHostname;
use crate::net::acme::AcmeProvider;
use crate::net::dns::QueryDnsRes;
use crate::net::gateway::{
    CheckDnsParams, CheckPortParams, CheckPortRes, CheckPortV6Res, check_dns, check_port,
    check_port_v6,
};
use crate::net::host::binding::DerivedAddressInfo;
use crate::net::host::{HostApiKind, all_hosts};
use crate::net::service_interface::HostnameMetadata;
use crate::prelude::*;
use crate::util::serde::{HandlerExtSerde, display_serializable};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostAddress {
    pub address: InternedString,
    pub public: Option<PublicDomainConfig>,
    pub private: Option<BTreeSet<GatewayId>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct PublicDomainConfig {
    pub gateway: GatewayId,
    pub acme: Option<AcmeProvider>,
}

fn handle_duplicates(db: &mut DatabaseModel) -> Result<(), Error> {
    let mut domains = BTreeSet::<InternedString>::new();
    let check_domain = |domains: &mut BTreeSet<InternedString>, domain: InternedString| {
        if domains.contains(&domain) {
            return Err(Error::new(
                eyre!("domain {domain} is already in use"),
                ErrorKind::InvalidRequest,
            ));
        }
        domains.insert(domain);
        Ok(())
    };
    let mut not_in_use = Vec::new();
    for host in all_hosts(db) {
        let host = host?;
        let in_use = host.as_bindings().de()?.values().any(|v| v.enabled);
        if !in_use {
            not_in_use.push(host);
            continue;
        }
        let public = host.as_public_domains().keys()?;
        for domain in &public {
            check_domain(&mut domains, domain.clone())?;
        }
        for domain in host.as_private_domains().keys()? {
            if !public.contains(&domain) {
                check_domain(&mut domains, domain)?;
            }
        }
    }
    for host in not_in_use {
        host.as_public_domains_mut()
            .mutate(|d| Ok(d.retain(|d, _| !domains.contains(d))))?;
        host.as_private_domains_mut()
            .mutate(|d| Ok(d.retain(|d, _| !domains.contains(d))))?;

        let public = host.as_public_domains().keys()?;
        for domain in &public {
            check_domain(&mut domains, domain.clone())?;
        }
        for domain in host.as_private_domains().keys()? {
            if !public.contains(&domain) {
                check_domain(&mut domains, domain)?;
            }
        }
    }
    Ok(())
}

pub fn address_api<C: Context, Kind: HostApiKind>()
-> ParentHandler<C, Kind::Params, Kind::InheritedParams> {
    ParentHandler::<C, Kind::Params, Kind::InheritedParams>::new()
        .subcommand(
            "domain",
            ParentHandler::<C, Empty, Kind::Inheritance>::new()
                .subcommand(
                    "public",
                    ParentHandler::<C, Empty, Kind::Inheritance>::new()
                        .subcommand(
                            "add",
                            from_fn_async(add_public_domain::<Kind>)
                                .with_metadata("sync_db", Value::Bool(true))
                                .with_inherited(|_, a| a)
                                .no_display()
                                .with_about("about.add-public-domain-to-host")
                                .with_call_remote::<CliContext>(),
                        )
                        .subcommand(
                            "remove",
                            from_fn_async(remove_public_domain::<Kind>)
                                .with_metadata("sync_db", Value::Bool(true))
                                .with_inherited(|_, a| a)
                                .no_display()
                                .with_about("about.remove-public-domain-from-host")
                                .with_call_remote::<CliContext>(),
                        )
                        .with_about("about.commands-host-public-domain")
                        .with_inherited(|_, a| a),
                )
                .subcommand(
                    "private",
                    ParentHandler::<C, Empty, Kind::Inheritance>::new()
                        .subcommand(
                            "add",
                            from_fn_async(add_private_domain::<Kind>)
                                .with_metadata("sync_db", Value::Bool(true))
                                .with_inherited(|_, a| a)
                                .no_display()
                                .with_about("about.add-private-domain-to-host")
                                .with_call_remote::<CliContext>(),
                        )
                        .subcommand(
                            "remove",
                            from_fn_async(remove_private_domain::<Kind>)
                                .with_metadata("sync_db", Value::Bool(true))
                                .with_inherited(|_, a| a)
                                .no_display()
                                .with_about("about.remove-private-domain-from-host")
                                .with_call_remote::<CliContext>(),
                        )
                        .with_about("about.commands-host-private-domain")
                        .with_inherited(|_, a| a),
                )
                .with_about("about.commands-host-address-domain")
                .with_inherited(Kind::inheritance),
        )
        .subcommand(
            "list",
            from_fn_async(list_addresses::<Kind>)
                .with_inherited(Kind::inheritance)
                .with_display_serializable()
                .with_custom_display_fn(|HandlerArgs { params, .. }, res| {
                    use prettytable::*;

                    if let Some(format) = params.format {
                        display_serializable(format, res)?;
                        return Ok(());
                    }

                    let mut table = Table::new();
                    table.add_row(row![bc =>
                        "ADDRESS",
                        "VISIBILITY",
                        "PUBLIC GATEWAY",
                        "ACME PROVIDER",
                        "PRIVATE GATEWAYS",
                    ]);
                    for addr in res.iter() {
                        let visibility = match (&addr.public, &addr.private) {
                            (Some(_), Some(_)) => "public, private",
                            (Some(_), None) => "public",
                            (None, Some(_)) => "private",
                            (None, None) => "none",
                        };
                        let public_gateway = addr
                            .public
                            .as_ref()
                            .map_or_else(|| "—".to_owned(), |p| p.gateway.to_string());
                        let acme = addr
                            .public
                            .as_ref()
                            .and_then(|p| p.acme.as_ref())
                            .map_or_else(|| "—".to_owned(), |a| a.0.to_string());
                        let private_gateways =
                            addr.private.as_ref().filter(|g| !g.is_empty()).map_or_else(
                                || "—".to_owned(),
                                |g| {
                                    g.iter()
                                        .map(|g| g.to_string())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                },
                            );
                        table.add_row(row![
                            addr.address,
                            visibility,
                            public_gateway,
                            acme,
                            private_gateways,
                        ]);
                    }

                    table.print_tty(false)?;

                    Ok(())
                })
                .with_about("about.list-addresses-for-host")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AddPublicDomainParams {
    #[arg(help = "help.arg.fqdn")]
    pub fqdn: InternedString,
    #[arg(long, help = "help.arg.acme-provider")]
    pub acme: Option<AcmeProvider>,
    #[arg(help = "help.arg.gateway-id")]
    pub gateway: GatewayId,
    #[arg(help = "help.arg.internal-port")]
    pub internal_port: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AddPublicDomainRes {
    pub dns: QueryDnsRes,
    pub port: CheckPortRes,
    pub port_v6: Option<CheckPortV6Res>,
}

/// Reconcile a public domain on a *sibling* binding or range — one the domain
/// was not directly added to. A public domain is scoped to its target binding,
/// so by default it is disabled here (kept off the sibling). The exception is
/// the non-SSL case: with no SNI, a non-SSL domain shares the bare WAN IP's
/// packets, so if the co-located WAN IPv4 (same gateway + port) is already
/// enabled on this sibling we honor that and enable the domain to match, keeping
/// the two in lockstep. SSL rows carry their own SNI and are always isolated.
fn reconcile_domain_on_sibling(
    addresses: &mut DerivedAddressInfo,
    fqdn: &InternedString,
    gateway: &GatewayId,
) {
    let mut enable = Vec::new();
    let mut disable = Vec::new();
    for a in &addresses.available {
        let HostnameMetadata::PublicDomain { gateway: gw } = &a.metadata else {
            continue;
        };
        if gw != gateway || !a.public || &a.hostname != fqdn {
            continue;
        }
        let Some(port) = a.port else { continue };
        if a.ssl {
            disable.push(port);
            continue;
        }
        // Non-SSL: honor an already-enabled co-located WAN IPv4 (same gateway +
        // port) — without SNI it is the same packets as the bare IP.
        let wan_enabled = addresses.available.iter().any(|b| {
            !b.ssl
                && b.public
                && matches!(&b.metadata, HostnameMetadata::Ipv4 { gateway: gw2 } if gw2 == gateway)
                && b.to_socket_addr()
                    .map_or(false, |sa| sa.port() == port && addresses.enabled.contains(&sa))
        });
        if wan_enabled {
            enable.push(port);
        } else {
            disable.push(port);
        }
    }
    for port in enable {
        addresses.disabled.remove(&(fqdn.clone(), port));
    }
    for port in disable {
        addresses.disabled.insert((fqdn.clone(), port));
    }
}

pub async fn add_public_domain<Kind: HostApiKind>(
    ctx: RpcContext,
    AddPublicDomainParams {
        fqdn,
        acme,
        gateway,
        internal_port,
    }: AddPublicDomainParams,
    inheritance: Kind::Inheritance,
) -> Result<AddPublicDomainRes, Error> {
    let ext_port = ctx
        .db
        .mutate(|db| {
            if let Some(acme) = &acme {
                if !db
                    .as_public()
                    .as_server_info()
                    .as_network()
                    .as_acme()
                    .contains_key(&acme)?
                {
                    return Err(Error::new(eyre!("unknown acme provider {}, please run acme.init for this provider first", acme.0), ErrorKind::InvalidRequest));
                }
            }

            Kind::host_for(&inheritance, db)?
                .as_public_domains_mut()
                .insert(
                    &fqdn,
                    &PublicDomainConfig {
                        acme,
                        gateway: gateway.clone(),
                    },
                )?;
            handle_duplicates(db)?;
            let hostname = ServerHostname::load(db.as_public().as_server_info())?;
            let gateways = db
                .as_public()
                .as_server_info()
                .as_network()
                .as_gateways()
                .de()?;
            let available_ports = db.as_private().as_available_ports().de()?;
            let host = Kind::host_for(&inheritance, db)?;
            host.update_addresses(&hostname, &gateways, &available_ports)?;

            // Find the external port for the target binding
            let bindings = host.as_bindings().de()?;
            let target_bind = bindings
                .get(&internal_port)
                .ok_or_else(|| Error::new(eyre!("binding not found for internal port {internal_port}"), ErrorKind::NotFound))?;
            let ext_port = target_bind
                .addresses
                .available
                .iter()
                .find(|a| a.public && a.hostname == fqdn)
                .and_then(|a| a.port)
                .ok_or_else(|| Error::new(eyre!("no public address found for {fqdn} on port {internal_port}"), ErrorKind::NotFound))?;

            // On the target binding, enable the WAN IPv4 and all
            // public domains on the same gateway+port (no SNI without SSL).
            host.as_bindings_mut().mutate(|b| {
                if let Some(bind) = b.get_mut(&internal_port) {
                    let non_ssl_port = bind.addresses.available.iter().find_map(|a| {
                        if a.ssl || !a.public || a.hostname != fqdn {
                            return None;
                        }
                        if let HostnameMetadata::PublicDomain { gateway: gw } = &a.metadata {
                            if *gw == gateway {
                                return a.port;
                            }
                        }
                        None
                    });
                    if let Some(dp) = non_ssl_port {
                        for a in &bind.addresses.available {
                            if a.ssl || !a.public {
                                continue;
                            }
                            if let HostnameMetadata::Ipv4 { gateway: gw } = &a.metadata {
                                if *gw == gateway {
                                    if let Some(sa) = a.to_socket_addr() {
                                        if sa.port() == dp {
                                            bind.addresses.enabled.insert(sa);
                                        }
                                    }
                                }
                            }
                        }
                        // No SNI without SSL, so the domain reaches v6 only via
                        // the bare GUA — expose it like the WAN IPv4 above (v6 has
                        // no NAT; the GUA is directly routable).
                        if let Some(ip_info) =
                            gateways.get(&gateway).and_then(|g| g.ip_info.as_ref())
                        {
                            for subnet in &ip_info.subnets {
                                if let IpAddr::V6(ip) = subnet.addr() {
                                    if !crate::net::utils::ipv6_is_local(ip) {
                                        let gua = SocketAddrV6::new(ip, dp, 0, 0);
                                        bind.addresses.gua_wan.insert(gua);
                                        bind.addresses.enabled.insert(SocketAddr::V6(gua));
                                    }
                                }
                            }
                        }
                        for a in &bind.addresses.available {
                            if a.ssl {
                                continue;
                            }
                            if let HostnameMetadata::PublicDomain { gateway: gw } = &a.metadata {
                                if *gw == gateway && a.port == Some(dp) {
                                    bind.addresses.disabled.remove(&(a.hostname.clone(), dp));
                                }
                            }
                        }
                    }
                }

                // Every other binding: isolate the domain by default, but honor
                // an already-enabled non-SSL WAN IP by enabling the domain there
                // to match (no SNI — the same packets as the bare IP).
                for (&port, bind) in b.iter_mut() {
                    if port == internal_port {
                        continue;
                    }
                    reconcile_domain_on_sibling(&mut bind.addresses, &fqdn, &gateway);
                }
                Ok(())
            })?;

            // Same reconciliation for every port range (parity with the sibling
            // bindings above). Ranges are IPv4-only and non-SSL, so a domain is
            // disabled on a range unless the range's own WAN IP is already
            // enabled — in which case the domain is enabled to match, since
            // without SNI it is reachable via that same forward anyway.
            host.as_binding_ranges_mut().mutate(|ranges| {
                for range in ranges.values_mut() {
                    reconcile_domain_on_sibling(&mut range.addresses, &fqdn, &gateway);
                }
                Ok(())
            })?;

            // Re-project: the gua_wan change above must flow into the GUA's
            // HostnameInfo.public so it is treated as WAN-exposed.
            Kind::host_for(&inheritance, db)?
                .update_addresses(&hostname, &gateways, &available_ports)?;

            Ok(ext_port)
        })
        .await
        .result?;

    let ctx2 = ctx.clone();
    let fqdn2 = fqdn.clone();

    let (dns_result, port_result, port_v6_result) = tokio::join!(
        async {
            tokio::task::spawn_blocking(move || {
                crate::net::dns::query_dns(ctx2, crate::net::dns::QueryDnsParams { fqdn: fqdn2 })
            })
            .await
            .with_kind(ErrorKind::Unknown)?
        },
        check_port(
            ctx.clone(),
            CheckPortParams {
                port: ext_port,
                gateway: gateway.clone(),
            },
        ),
        check_port_v6(
            ctx.clone(),
            CheckPortParams {
                port: ext_port,
                gateway: gateway.clone(),
            },
        )
    );

    Ok(AddPublicDomainRes {
        dns: dns_result?,
        port: port_result?,
        port_v6: port_v6_result?,
    })
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
pub struct RemoveDomainParams {
    #[arg(help = "help.arg.fqdn")]
    pub fqdn: InternedString,
}

pub async fn remove_public_domain<Kind: HostApiKind>(
    ctx: RpcContext,
    RemoveDomainParams { fqdn }: RemoveDomainParams,
    inheritance: Kind::Inheritance,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            Kind::host_for(&inheritance, db)?
                .as_public_domains_mut()
                .remove(&fqdn)?;
            let hostname = ServerHostname::load(db.as_public().as_server_info())?;
            let gateways = db
                .as_public()
                .as_server_info()
                .as_network()
                .as_gateways()
                .de()?;
            let ports = db.as_private().as_available_ports().de()?;
            Kind::host_for(&inheritance, db)?.update_addresses(&hostname, &gateways, &ports)
        })
        .await
        .result?;

    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
pub struct AddPrivateDomainParams {
    #[arg(help = "help.arg.fqdn")]
    pub fqdn: InternedString,
    pub gateway: GatewayId,
}

pub async fn add_private_domain<Kind: HostApiKind>(
    ctx: RpcContext,
    AddPrivateDomainParams { fqdn, gateway }: AddPrivateDomainParams,
    inheritance: Kind::Inheritance,
) -> Result<bool, Error> {
    ctx.db
        .mutate(|db| {
            Kind::host_for(&inheritance, db)?
                .as_private_domains_mut()
                .upsert(&fqdn, || Ok(BTreeSet::new()))?
                .mutate(|d| Ok(d.insert(gateway.clone())))?;
            handle_duplicates(db)?;
            let hostname = ServerHostname::load(db.as_public().as_server_info())?;
            let gateways = db
                .as_public()
                .as_server_info()
                .as_network()
                .as_gateways()
                .de()?;
            let ports = db.as_private().as_available_ports().de()?;
            Kind::host_for(&inheritance, db)?.update_addresses(&hostname, &gateways, &ports)
        })
        .await
        .result?;

    check_dns(ctx, CheckDnsParams { gateway, fqdn }).await
}

pub async fn remove_private_domain<Kind: HostApiKind>(
    ctx: RpcContext,
    RemoveDomainParams { fqdn: domain }: RemoveDomainParams,
    inheritance: Kind::Inheritance,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            Kind::host_for(&inheritance, db)?
                .as_private_domains_mut()
                .mutate(|d| Ok(d.remove(&domain)))?;
            let hostname = ServerHostname::load(db.as_public().as_server_info())?;
            let gateways = db
                .as_public()
                .as_server_info()
                .as_network()
                .as_gateways()
                .de()?;
            let ports = db.as_private().as_available_ports().de()?;
            Kind::host_for(&inheritance, db)?.update_addresses(&hostname, &gateways, &ports)
        })
        .await
        .result?;

    Ok(())
}

pub async fn list_addresses<Kind: HostApiKind>(
    ctx: RpcContext,
    _: Empty,
    inheritance: Kind::Inheritance,
) -> Result<Vec<HostAddress>, Error> {
    Ok(Kind::host_for(&inheritance, &mut ctx.db.peek().await)?
        .de()?
        .addresses()
        .collect())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::net::service_interface::{HostnameInfo, HostnameMetadata};

    const FQDN: &str = "turn.start9.dev";

    fn gw(name: &str) -> GatewayId {
        GatewayId::from(InternedString::intern(name))
    }

    fn domain(ssl: bool, port: u16, gateway: &str) -> HostnameInfo {
        HostnameInfo {
            ssl,
            public: true,
            hostname: InternedString::intern(FQDN),
            port: Some(port),
            metadata: HostnameMetadata::PublicDomain {
                gateway: gw(gateway),
            },
        }
    }

    fn wan_ip(port: u16, gateway: &str) -> HostnameInfo {
        HostnameInfo {
            ssl: false,
            public: true,
            hostname: InternedString::intern("64.23.194.12"),
            port: Some(port),
            metadata: HostnameMetadata::Ipv4 {
                gateway: gw(gateway),
            },
        }
    }

    fn domain_disabled(a: &DerivedAddressInfo, port: u16) -> bool {
        a.disabled.contains(&(InternedString::intern(FQDN), port))
    }

    /// The edge case that motivated this: a sibling whose non-SSL WAN IP is
    /// already enabled must keep the domain ENABLED (in lockstep), not disable
    /// it — otherwise the domain reads "disabled" while still reachable.
    #[test]
    fn non_ssl_domain_follows_enabled_wan_ip() {
        let fqdn = InternedString::intern(FQDN);
        let mut a = DerivedAddressInfo::default();
        a.available.insert(domain(false, 42000, "wg1"));
        a.available.insert(wan_ip(42000, "wg1"));
        a.enabled.insert("64.23.194.12:42000".parse().unwrap());

        reconcile_domain_on_sibling(&mut a, &fqdn, &gw("wg1"));

        assert!(
            !domain_disabled(&a, 42000),
            "domain must be enabled to match the already-enabled WAN IP"
        );
    }

    /// Default isolate behavior: WAN IP not enabled -> domain disabled.
    #[test]
    fn non_ssl_domain_disabled_when_wan_ip_off() {
        let fqdn = InternedString::intern(FQDN);
        let mut a = DerivedAddressInfo::default();
        a.available.insert(domain(false, 42000, "wg1"));
        a.available.insert(wan_ip(42000, "wg1"));

        reconcile_domain_on_sibling(&mut a, &fqdn, &gw("wg1"));

        assert!(domain_disabled(&a, 42000), "domain must be isolated by default");
    }

    /// SSL rows have their own SNI, so they are always isolated regardless of IP.
    #[test]
    fn ssl_domain_is_always_isolated() {
        let fqdn = InternedString::intern(FQDN);
        let mut a = DerivedAddressInfo::default();
        a.available.insert(domain(true, 5349, "wg1"));
        a.available.insert(wan_ip(5349, "wg1"));
        a.enabled.insert("64.23.194.12:5349".parse().unwrap());

        reconcile_domain_on_sibling(&mut a, &fqdn, &gw("wg1"));

        assert!(domain_disabled(&a, 5349), "SSL domain must stay isolated");
    }

    /// An enabled WAN IP on a *different* gateway must not enable the domain.
    #[test]
    fn enabled_wan_ip_on_other_gateway_is_not_honored() {
        let fqdn = InternedString::intern(FQDN);
        let mut a = DerivedAddressInfo::default();
        a.available.insert(domain(false, 42000, "wg1"));
        a.available.insert(wan_ip(42000, "eth0"));
        a.enabled.insert("64.23.194.12:42000".parse().unwrap());

        reconcile_domain_on_sibling(&mut a, &fqdn, &gw("wg1"));

        assert!(
            domain_disabled(&a, 42000),
            "an enabled IP on a different gateway must not honor the domain"
        );
    }
}
