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
use crate::net::host::binding::{DerivedAddressInfo, set_nonssl_lan_group, set_nonssl_wan_group};
use crate::net::host::{Host, HostApiKind, all_hosts};
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
/// so by default it is isolated here. The exception is the non-SSL case: with no
/// SNI, the domain shares the bare WAN IP's packets, so if a co-located public
/// WAN address (IPv4 or a WAN IPv6 GUA, same gateway + port) is already enabled
/// we honor it and move the whole {domain, IPv4, GUA} group on together — a
/// dual-stack domain links the v4 and v6 sides. SSL rows carry their own SNI and
/// are always isolated.
fn reconcile_public_domain_on_sibling(
    addresses: &mut DerivedAddressInfo,
    fqdn: &InternedString,
    gateway: &GatewayId,
) {
    let mut ssl_ports = Vec::new();
    let mut nonssl_ports = Vec::new();
    for a in &addresses.available {
        let HostnameMetadata::PublicDomain { gateway: gw } = &a.metadata else {
            continue;
        };
        if gw != gateway || !a.public || &a.hostname != fqdn {
            continue;
        }
        let Some(port) = a.port else { continue };
        if a.ssl {
            ssl_ports.push(port);
        } else if !nonssl_ports.contains(&port) {
            nonssl_ports.push(port);
        }
    }
    // SSL rows are isolated to the target (SNI distinguishes them from the IP).
    for port in ssl_ports {
        addresses.disabled.insert((fqdn.clone(), port));
    }
    // Non-SSL: honor an already-enabled co-located WAN address (IPv4 or GUA) and
    // move the whole {domain, IPv4, GUA} group to match; otherwise isolate it.
    for port in nonssl_ports {
        let wan_enabled = addresses.available.iter().any(|b| {
            !b.ssl
                && b.public
                && matches!(
                    &b.metadata,
                    HostnameMetadata::Ipv4 { gateway: gw2 }
                        | HostnameMetadata::Ipv6 { gateway: gw2, .. }
                    if gw2 == gateway
                )
                && b.to_socket_addr().map_or(false, |sa| {
                    sa.port() == port && addresses.enabled.contains(&sa)
                })
        });
        set_nonssl_wan_group(addresses, gateway, port, wan_enabled);
    }
}

/// Reconcile a private domain on a binding or range. Unlike a public domain, a
/// private domain stays on by default — LAN is trusted, so it is not isolated.
/// The exception: if the binding's own bare LAN IPv4 at this gateway+port has
/// been disabled, honor that and take the whole {private domain, LAN IPv4,
/// GUA-as-local} group down too. SSL private domains carry their own SNI and are
/// left on.
fn reconcile_private_domain_on_sibling(
    addresses: &mut DerivedAddressInfo,
    fqdn: &InternedString,
    gateway: &GatewayId,
) {
    let mut nonssl_ports = Vec::new();
    for a in &addresses.available {
        if a.ssl || &a.hostname != fqdn {
            continue;
        }
        let HostnameMetadata::PrivateDomain { gateways } = &a.metadata else {
            continue;
        };
        if !gateways.contains(gateway) {
            continue;
        }
        let Some(port) = a.port else { continue };
        if !nonssl_ports.contains(&port) {
            nonssl_ports.push(port);
        }
    }
    for port in nonssl_ports {
        // On unless the bare LAN IPv4 at this gateway+port is explicitly disabled.
        let lan_disabled = addresses.available.iter().any(|b| {
            !b.ssl
                && b.port == Some(port)
                && matches!(&b.metadata, HostnameMetadata::Ipv4 { gateway: gw2 } if !b.public && gw2 == gateway)
                && addresses.disabled.contains(&(b.hostname.clone(), port))
        });
        set_nonssl_lan_group(addresses, gateway, port, !lan_disabled);
    }
}

impl Model<Host> {
    /// The host's public domains as `(fqdn, gateway)` pairs.
    fn public_domain_gateways(&self) -> Result<Vec<(InternedString, GatewayId)>, Error> {
        Ok(self
            .as_public_domains()
            .de()?
            .into_iter()
            .map(|(fqdn, cfg)| (fqdn, cfg.gateway))
            .collect())
    }

    /// Reconcile a newly-bound single-port binding against the host's existing
    /// public domains — a domain added earlier must not silently leak onto a
    /// binding added afterwards. A fresh binding is always a sibling (never a
    /// domain's target), so this isolates each domain unless the binding's own
    /// WAN address is already enabled. Call after `update_addresses` has
    /// synthesized the domain rows, then re-run `update_addresses` so the port
    /// forwards reflect the isolation. Only run for a genuinely new binding —
    /// re-running it on a re-bind would clobber a domain's target binding.
    pub fn reconcile_public_domains_on_new_binding(
        &mut self,
        internal_port: u16,
    ) -> Result<(), Error> {
        let public_domains = self.public_domain_gateways()?;
        if public_domains.is_empty() {
            return Ok(());
        }
        self.as_bindings_mut().mutate(|b| {
            if let Some(bind) = b.get_mut(&internal_port) {
                for (fqdn, gateway) in &public_domains {
                    reconcile_public_domain_on_sibling(&mut bind.addresses, fqdn, gateway);
                }
            }
            Ok(())
        })
    }

    /// Range counterpart of [`Self::reconcile_public_domains_on_new_binding`].
    pub fn reconcile_public_domains_on_new_range(
        &mut self,
        internal_start_port: u16,
    ) -> Result<(), Error> {
        let public_domains = self.public_domain_gateways()?;
        if public_domains.is_empty() {
            return Ok(());
        }
        self.as_binding_ranges_mut().mutate(|ranges| {
            if let Some(range) = ranges.get_mut(&internal_start_port) {
                for (fqdn, gateway) in &public_domains {
                    reconcile_public_domain_on_sibling(&mut range.addresses, fqdn, gateway);
                }
            }
            Ok(())
        })
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
    // Domains are matched byte-for-byte against the browser's lowercased
    // `location.hostname` — normalize at the boundary (covers UI and CLI).
    let fqdn = InternedString::intern(fqdn.to_ascii_lowercase());
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

            // Adding a domain that is already present is a no-op for exposure:
            // we re-affirm the config below, but skip the target force-enable and
            // the sibling/range reconcile so a re-add can't re-isolate a binding
            // whose WAN IP the operator has since enabled, or clobber any other
            // per-address choice. Enabling an existing domain on a binding is done
            // through set-address-enabled, not by re-adding it.
            let is_new = !Kind::host_for(&inheritance, db)?
                .as_public_domains()
                .keys()?
                .contains(&fqdn);

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

            // Find the external port for the target binding to health-check.
            // Prefer the SSL (vhost) port over the plaintext one: a domain is
            // normally reached over TLS, so that is the forward the operator
            // wants validated.
            let bindings = host.as_bindings().de()?;
            let target_bind = bindings
                .get(&internal_port)
                .ok_or_else(|| Error::new(eyre!("binding not found for internal port {internal_port}"), ErrorKind::NotFound))?;
            let ext_port = target_bind
                .addresses
                .available
                .iter()
                .filter(|a| a.public && a.hostname == fqdn)
                .max_by_key(|a| a.ssl)
                .and_then(|a| a.port)
                .ok_or_else(|| Error::new(eyre!("no public address found for {fqdn} on port {internal_port}"), ErrorKind::NotFound))?;

            // A NEW domain gets force-enabled on its target binding and
            // reconciled across the rest of the host; re-adding an existing one
            // leaves every binding's/range's exposure exactly as it is.
            if is_new {
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
                        reconcile_public_domain_on_sibling(&mut bind.addresses, &fqdn, &gateway);
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
                        reconcile_public_domain_on_sibling(&mut range.addresses, &fqdn, &gateway);
                    }
                    Ok(())
                })?;
            }

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
    let fqdn = InternedString::intern(fqdn.to_ascii_lowercase());
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
    let fqdn = InternedString::intern(fqdn.to_ascii_lowercase());
    ctx.db
        .mutate(|db| {
            let is_new = !Kind::host_for(&inheritance, db)?
                .as_private_domains()
                .de()?
                .get(&fqdn)
                .is_some_and(|gws| gws.contains(&gateway));
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
            let host = Kind::host_for(&inheritance, db)?;
            host.update_addresses(&hostname, &gateways, &ports)?;
            // A private domain stays on by default, but honor it across the host:
            // on every binding and range, take the private domain (and its LAN
            // group) down where the operator has disabled that binding's private
            // addresses. Only for a newly-added domain, so a re-add can't clobber.
            if is_new {
                host.as_bindings_mut().mutate(|b| {
                    for bind in b.values_mut() {
                        reconcile_private_domain_on_sibling(&mut bind.addresses, &fqdn, &gateway);
                    }
                    Ok(())
                })?;
                host.as_binding_ranges_mut().mutate(|ranges| {
                    for range in ranges.values_mut() {
                        reconcile_private_domain_on_sibling(&mut range.addresses, &fqdn, &gateway);
                    }
                    Ok(())
                })?;
                Kind::host_for(&inheritance, db)?.update_addresses(&hostname, &gateways, &ports)?;
            }
            Ok(())
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
    let domain = InternedString::intern(domain.to_ascii_lowercase());
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

    // A WAN-exposed IPv6 GUA (public=true, projected from gua_wan).
    fn gua(port: u16, gateway: &str) -> HostnameInfo {
        HostnameInfo {
            ssl: false,
            public: true,
            hostname: InternedString::intern("2001:db8::1"),
            port: Some(port),
            metadata: HostnameMetadata::Ipv6 {
                gateway: gw(gateway),
                scope_id: 0,
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

        reconcile_public_domain_on_sibling(&mut a, &fqdn, &gw("wg1"));

        assert!(
            !domain_disabled(&a, 42000),
            "domain must be enabled to match the already-enabled WAN IP"
        );
    }

    /// The domain must equally follow an enabled WAN IPv6 GUA (no SNI over v6
    /// either), not just an enabled IPv4 WAN address.
    #[test]
    fn non_ssl_domain_follows_enabled_gua() {
        let fqdn = InternedString::intern(FQDN);
        let mut a = DerivedAddressInfo::default();
        a.available.insert(domain(false, 42000, "wg1"));
        a.available.insert(gua(42000, "wg1"));
        a.enabled.insert("[2001:db8::1]:42000".parse().unwrap());

        reconcile_public_domain_on_sibling(&mut a, &fqdn, &gw("wg1"));

        assert!(
            !domain_disabled(&a, 42000),
            "domain must be enabled to match the already-enabled public GUA"
        );
    }

    /// Transitive dual-stack link: enabling the WAN IPv4 on a non-SSL sibling
    /// that has a domain also publishes the co-located GUA (the domain is
    /// dual-stack, so v4 and v6 must move together).
    #[test]
    fn enabling_v4_with_domain_publishes_gua() {
        let fqdn = InternedString::intern(FQDN);
        let mut a = DerivedAddressInfo::default();
        a.available.insert(domain(false, 42000, "wg1"));
        a.available.insert(wan_ip(42000, "wg1"));
        a.available.insert(gua(42000, "wg1")); // GUA present, not yet published
        a.enabled.insert("64.23.194.12:42000".parse().unwrap()); // only v4 enabled

        reconcile_public_domain_on_sibling(&mut a, &fqdn, &gw("wg1"));

        let gua_v6: std::net::SocketAddrV6 = "[2001:db8::1]:42000".parse().unwrap();
        assert!(
            a.gua_wan.contains(&gua_v6),
            "the GUA must be published to WAN (gua_wan) when v4 + a domain are on"
        );
        assert!(
            a.enabled.contains(&"[2001:db8::1]:42000".parse().unwrap()),
            "the GUA must be enabled"
        );
        assert!(!domain_disabled(&a, 42000), "domain enabled");
    }

    /// Default isolate behavior: WAN IP not enabled -> domain disabled.
    #[test]
    fn non_ssl_domain_disabled_when_wan_ip_off() {
        let fqdn = InternedString::intern(FQDN);
        let mut a = DerivedAddressInfo::default();
        a.available.insert(domain(false, 42000, "wg1"));
        a.available.insert(wan_ip(42000, "wg1"));

        reconcile_public_domain_on_sibling(&mut a, &fqdn, &gw("wg1"));

        assert!(
            domain_disabled(&a, 42000),
            "domain must be isolated by default"
        );
    }

    /// SSL rows have their own SNI, so they are always isolated regardless of IP.
    #[test]
    fn ssl_domain_is_always_isolated() {
        let fqdn = InternedString::intern(FQDN);
        let mut a = DerivedAddressInfo::default();
        a.available.insert(domain(true, 5349, "wg1"));
        a.available.insert(wan_ip(5349, "wg1"));
        a.enabled.insert("64.23.194.12:5349".parse().unwrap());

        reconcile_public_domain_on_sibling(&mut a, &fqdn, &gw("wg1"));

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

        reconcile_public_domain_on_sibling(&mut a, &fqdn, &gw("wg1"));

        assert!(
            domain_disabled(&a, 42000),
            "an enabled IP on a different gateway must not honor the domain"
        );
    }

    #[test]
    fn private_domain_stays_on_but_honors_disabled_lan() {
        let fqdn = InternedString::intern("priv.local");
        let mut a = DerivedAddressInfo::default();
        a.available.insert(HostnameInfo {
            ssl: false,
            public: false,
            hostname: fqdn.clone(),
            port: Some(42000),
            metadata: HostnameMetadata::PrivateDomain {
                gateways: BTreeSet::from([gw("wg1")]),
            },
        });
        a.available.insert(HostnameInfo {
            ssl: false,
            public: false,
            hostname: InternedString::intern("10.0.0.5"),
            port: Some(42000),
            metadata: HostnameMetadata::Ipv4 { gateway: gw("wg1") },
        });
        let priv_key = (fqdn.clone(), 42000u16);

        // Default: the bare LAN IPv4 is on, so the private domain stays ON.
        reconcile_private_domain_on_sibling(&mut a, &fqdn, &gw("wg1"));
        assert!(
            !a.disabled.contains(&priv_key),
            "a private domain is on by default (LAN is not isolated)"
        );

        // Operator disables the bare LAN IPv4 -> the private domain is honored off.
        a.disabled
            .insert((InternedString::intern("10.0.0.5"), 42000));
        reconcile_private_domain_on_sibling(&mut a, &fqdn, &gw("wg1"));
        assert!(
            a.disabled.contains(&priv_key),
            "a disabled LAN IPv4 takes the private domain down too"
        );
    }
}
