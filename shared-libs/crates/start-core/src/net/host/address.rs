use std::collections::BTreeSet;

use clap::Parser;
use imbl_value::InternedString;
use rpc_toolkit::{Context, Empty, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::GatewayId;
use crate::context::{CliContext, RpcContext};
use crate::db::model::DatabaseModel;
use crate::hostname::ServerHostname;
use crate::net::acme::{AcmeProvider, CheckChallengeParams, CheckChallengeRes, check_challenge};
use crate::net::dns::QueryDnsRes;
use crate::net::gateway::{
    CheckDnsParams, CheckPortParams, CheckPortRes, CheckPortV6Res, check_dns, check_port,
    check_port_v6,
};
use crate::net::host::binding::{DerivedAddressInfo, set_nonssl_lan_group, set_nonssl_wan_group};
use crate::net::host::{Host, HostApiKind, all_hosts};
use crate::net::service_interface::HostnameMetadata;
use crate::net::vhost::ACME_CHALLENGE_PORT;
use crate::prelude::*;
use crate::util::serde::{HandlerExtSerde, display_serializable};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[derive(VisitFields)]
pub struct HostAddress {
    pub address: InternedString,
    pub public: Option<PublicDomainConfig>,
    pub private: Option<BTreeSet<GatewayId>>,
}

rpc_toolkit::reflect_ts!(HostAddress);

#[derive(Debug, Clone, Deserialize, Serialize, VisitFields)]
pub struct PublicDomainConfig {
    pub gateway: GatewayId,
    pub acme: Option<AcmeProvider>,
}

rpc_toolkit::reflect_ts!(PublicDomainConfig);
rpc_toolkit::ts_export!(PublicDomainConfig, namespaces = [""]);

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
        let in_use = host.as_bindings().de()?.values().any(|v| v.enabled)
            || host.as_binding_ranges().de()?.values().any(|v| v.enabled);
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

                    print_table(&table)?;

                    Ok(())
                })
                .with_about("about.list-addresses-for-host")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
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

rpc_toolkit::reflect_ts!(AddPublicDomainParams);
rpc_toolkit::ts_export!(AddPublicDomainParams, namespaces = [""]);

#[derive(Debug, Clone, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct AddPublicDomainRes {
    pub dns: QueryDnsRes,
    pub port: CheckPortRes,
    pub port_v6: Option<CheckPortV6Res>,
    /// The authority's own reachability requirement, where the domain has one.
    pub challenge: Option<CheckChallengeRes>,
}

rpc_toolkit::reflect_ts!(AddPublicDomainRes);
rpc_toolkit::ts_export!(AddPublicDomainRes, namespaces = [""]);

// Non-TLS domains share their gateway's WAN IP exposure.
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
    for port in ssl_ports {
        addresses.disabled.insert((fqdn.clone(), port));
    }
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

// Non-TLS private domains follow their gateway's LAN IP exposure.
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
        let lan_disabled = addresses.available.iter().any(|b| {
            !b.ssl
                && b.port == Some(port)
                && matches!(&b.metadata, HostnameMetadata::Ipv4 { gateway: gw2 } if !b.public && gw2 == gateway)
                && addresses.disabled.contains(&(b.hostname.clone(), port))
        });
        set_nonssl_lan_group(addresses, gateway, port, !lan_disabled);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PublicDomainTarget {
    Binding(u16),
    Range(u16),
}

impl Host {
    fn public_domain_target(
        &self,
        internal_port: u16,
        fqdn: &InternedString,
    ) -> Result<(PublicDomainTarget, u16), Error> {
        let binding = self.bindings.get(&internal_port);
        let range = self.binding_ranges.get(&internal_port);
        // A dormant binding may share the active range's start port.
        let (target, addresses) = match (binding, range) {
            (Some(binding), range)
                if binding.enabled || range.is_none_or(|range| !range.enabled) =>
            {
                (
                    PublicDomainTarget::Binding(internal_port),
                    &binding.addresses,
                )
            }
            (_, Some(range)) => (PublicDomainTarget::Range(internal_port), &range.addresses),
            _ => {
                return Err(Error::new(
                    eyre!("binding not found for internal port {internal_port}"),
                    ErrorKind::NotFound,
                ));
            }
        };
        let port = addresses
            .available
            .iter()
            .filter(|address| address.public && &address.hostname == fqdn)
            .max_by_key(|address| address.ssl)
            .and_then(|address| address.port)
            .ok_or_else(|| {
                Error::new(
                    eyre!("no public address found for {fqdn} on port {internal_port}"),
                    ErrorKind::NotFound,
                )
            })?;
        Ok((target, port))
    }

    fn enable_new_public_domain(
        &mut self,
        target: PublicDomainTarget,
        fqdn: &InternedString,
        gateway: &GatewayId,
    ) {
        let bindings = self
            .bindings
            .iter_mut()
            .map(|(&port, binding)| (PublicDomainTarget::Binding(port), &mut binding.addresses));
        let ranges = self
            .binding_ranges
            .iter_mut()
            .map(|(&port, range)| (PublicDomainTarget::Range(port), &mut range.addresses));
        for (binding, addresses) in bindings.chain(ranges) {
            if binding == target {
                let nonssl_port = addresses.available.iter().find_map(|address| {
                    if !address.ssl && address.public && &address.hostname == fqdn
                        && matches!(&address.metadata, HostnameMetadata::PublicDomain { gateway: gw } if gw == gateway)
                    {
                        address.port
                    } else {
                        None
                    }
                });
                if let Some(port) = nonssl_port {
                    set_nonssl_wan_group(addresses, gateway, port, true);
                }
            } else {
                reconcile_public_domain_on_sibling(addresses, fqdn, gateway);
            }
        }
    }
}

impl Model<Host> {
    fn public_domain_gateways(&self) -> Result<Vec<(InternedString, GatewayId)>, Error> {
        Ok(self
            .as_public_domains()
            .de()?
            .into_iter()
            .map(|(fqdn, cfg)| (fqdn, cfg.gateway))
            .collect())
    }

    /// Isolates existing public domains on a new binding unless its WAN IP is enabled.
    /// Call between `update_addresses` passes, only for newly-created bindings.
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

fn configure_public_domain<Kind: HostApiKind>(
    db: &mut DatabaseModel,
    params: &AddPublicDomainParams,
    inheritance: &Kind::Inheritance,
) -> Result<u16, Error> {
    let AddPublicDomainParams {
        fqdn,
        acme,
        gateway,
        internal_port,
    } = params;
    if let Some(acme) = acme {
        if !db
            .as_public()
            .as_server_info()
            .as_network()
            .as_acme()
            .contains_key(acme)?
        {
            return Err(Error::new(
                eyre!(
                    "unknown acme provider {}, please run acme.init for this provider first",
                    acme.0
                ),
                ErrorKind::InvalidRequest,
            ));
        }
    }

    let is_new = !Kind::host_for(inheritance, db)?
        .as_public_domains()
        .keys()?
        .contains(fqdn);

    Kind::host_for(inheritance, db)?
        .as_public_domains_mut()
        .insert(
            fqdn,
            &PublicDomainConfig {
                acme: acme.clone(),
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
    let host = Kind::host_for(inheritance, db)?;
    host.update_addresses(&hostname, &gateways, &available_ports)?;

    let ext_port = host.mutate(|host| {
        let (target, port) = host.public_domain_target(*internal_port, fqdn)?;
        // Re-adding a domain preserves the operator's address choices.
        if is_new {
            host.enable_new_public_domain(target, fqdn, gateway);
        }
        Ok(port)
    })?;

    host.update_addresses(&hostname, &gateways, &available_ports)?;
    Ok(ext_port)
}

pub async fn add_public_domain<Kind: HostApiKind>(
    ctx: RpcContext,
    mut params: AddPublicDomainParams,
    inheritance: Kind::Inheritance,
) -> Result<AddPublicDomainRes, Error> {
    // Browser hostnames are lowercased; domain matching is byte-for-byte.
    params.fqdn = InternedString::intern(params.fqdn.to_ascii_lowercase());
    let ext_port = ctx
        .db
        .mutate(|db| configure_public_domain::<Kind>(db, &params, &inheritance))
        .await
        .result?;
    let AddPublicDomainParams {
        fqdn,
        acme: authority,
        gateway,
        ..
    } = params;

    let ctx2 = ctx.clone();
    let fqdn2 = fqdn.clone();

    let (dns_result, port_result, port_v6_result, challenge_result) = tokio::join!(
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
        ),
        // TLS-ALPN-01 is validated at 443, regardless of which port the address serves.
        async {
            let Some(acme) = authority.filter(|_| ext_port != ACME_CHALLENGE_PORT) else {
                return Ok(None);
            };
            check_challenge(
                ctx.clone(),
                CheckChallengeParams {
                    fqdn: fqdn.clone(),
                    gateway: gateway.clone(),
                    acme,
                },
            )
            .await
        }
    );

    Ok(AddPublicDomainRes {
        dns: dns_result?,
        port: port_result?,
        port_v6: port_v6_result?,
        challenge: challenge_result?,
    })
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
pub struct RemoveDomainParams {
    #[arg(help = "help.arg.fqdn")]
    pub fqdn: InternedString,
}

rpc_toolkit::reflect_ts!(RemoveDomainParams);
rpc_toolkit::ts_export!(RemoveDomainParams, namespaces = [""]);

pub async fn remove_public_domain<Kind: HostApiKind>(
    ctx: RpcContext,
    RemoveDomainParams { fqdn }: RemoveDomainParams,
    inheritance: Kind::Inheritance,
) -> Result<(), Error> {
    let fqdn = InternedString::intern(fqdn.to_ascii_lowercase());
    ctx.db
        .mutate(|db| {
            let Some(host) = Kind::host_for_existing(&inheritance, db)? else {
                return Ok(());
            };
            host.as_public_domains_mut().remove(&fqdn)?;
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

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
pub struct AddPrivateDomainParams {
    #[arg(help = "help.arg.fqdn")]
    pub fqdn: InternedString,
    pub gateway: GatewayId,
}

rpc_toolkit::reflect_ts!(AddPrivateDomainParams);
rpc_toolkit::ts_export!(AddPrivateDomainParams, namespaces = [""]);

pub async fn add_private_domain<Kind: HostApiKind>(
    ctx: RpcContext,
    AddPrivateDomainParams { fqdn, gateway }: AddPrivateDomainParams,
    inheritance: Kind::Inheritance,
) -> Result<bool, Error> {
    let fqdn = InternedString::intern(fqdn.trim_end_matches('.').to_ascii_lowercase());
    ctx.db
        .mutate(|db| {
            let hostname = ServerHostname::load(db.as_public().as_server_info())?;
            hostname.reject_private_domain(&fqdn)?;
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
            let Some(host) = Kind::host_for_existing(&inheritance, db)? else {
                return Ok(());
            };
            host.as_private_domains_mut()
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
    use std::sync::Arc;

    use super::*;
    use crate::db::model::public::{IpInfo, NetworkInterfaceInfo};
    use crate::net::forward::AvailablePorts;
    use crate::net::host::ForPackage;
    use crate::net::host::binding::{
        AddSslOptions, BindInfo, BindOptions, RangeBindInfo, Security,
    };
    use crate::net::service_interface::{HostnameInfo, HostnameMetadata, RangeServiceInterface};

    const FQDN: &str = "turn.start9.dev";

    fn range(external_start_port: u16) -> RangeBindInfo {
        RangeBindInfo {
            enabled: true,
            external_start_port,
            number_of_ports: 3,
            addresses: DerivedAddressInfo::default(),
            interface: Some(RangeServiceInterface {
                id: "range".parse().unwrap(),
                name: "Range".into(),
                description: String::new(),
                scheme: None,
            }),
        }
    }

    fn single(port: u16, ssl_port: Option<u16>) -> BindInfo {
        BindInfo::new(
            &mut AvailablePorts::new(),
            BindOptions {
                preferred_external_port: port,
                add_ssl: ssl_port.map(|preferred_external_port| AddSslOptions {
                    preferred_external_port,
                    alpn: None,
                    upstream_cert_validation: None,
                }),
                secure: Some(Security { ssl: false }),
                proxy: None,
            },
            true,
        )
        .unwrap()
    }

    fn database(host: Host) -> DatabaseModel {
        let gateway = NetworkInterfaceInfo {
            ip_info: Some(Arc::new(IpInfo {
                subnets: [
                    "192.0.2.2/24".parse::<ipnet::IpNet>().unwrap(),
                    "2001:db8::1/64".parse().unwrap(),
                ]
                .into_iter()
                .collect(),
                wan_ip: Some("64.23.194.12".parse().unwrap()),
                ..Default::default()
            })),
            ..Default::default()
        };
        DatabaseModel::from(imbl_value::json!({
            "public": {
                "ui": {},
                "serverInfo": {
                    "hostname": "server",
                    "network": {
                        "host": imbl_value::to_value(&Host::default()).unwrap(),
                        "gateways": { "wg1": imbl_value::to_value(&gateway).unwrap() },
                        "acme": {},
                    },
                },
                "packageData": { "rustdesk-server": { "hosts": { "rustdesk": imbl_value::to_value(&host).unwrap() } } },
            },
            "private": { "availablePorts": imbl_value::to_value(&AvailablePorts::new()).unwrap() },
        }))
    }

    fn inheritance() -> <ForPackage as HostApiKind>::Inheritance {
        (
            "rustdesk-server".parse().unwrap(),
            "rustdesk".parse().unwrap(),
        )
    }

    fn add_domain(db: &mut DatabaseModel, internal_port: u16) -> Result<u16, Error> {
        configure_public_domain::<ForPackage>(
            db,
            &AddPublicDomainParams {
                fqdn: InternedString::intern(FQDN),
                acme: None,
                gateway: gw("wg1"),
                internal_port,
            },
            &inheritance(),
        )
    }

    fn package_host(db: &mut DatabaseModel) -> &mut Model<Host> {
        ForPackage::host_for(&inheritance(), db).unwrap()
    }

    #[test]
    fn public_domain_on_range_only_host_enables_the_range_and_forward() {
        let mut host = Host::default();
        host.binding_ranges.insert(21115, range(42000));
        let mut db = database(host);

        assert_eq!(add_domain(&mut db, 21115).unwrap(), 42000);

        let host = package_host(&mut db).de().unwrap();
        assert!(
            host.public_domains
                .contains_key(&InternedString::intern(FQDN))
        );
        let addresses = &host.binding_ranges[&21115].addresses;
        assert!(addresses.enabled().contains(&domain(false, 42000, "wg1")));
        assert!(
            addresses
                .enabled
                .contains(&"64.23.194.12:42000".parse().unwrap())
        );
        assert!(addresses.gua_wan.is_empty());
        let forward = host.port_forwards.iter().next().unwrap();
        assert_eq!(host.port_forwards.len(), 1);
        assert_eq!(forward.src.to_string(), "64.23.194.12:42000");
        assert_eq!(forward.dst.to_string(), "192.0.2.2:21115");
        assert_eq!(forward.count, 3);
    }

    #[test]
    fn range_target_reconciles_single_port_and_range_siblings() {
        let mut host = Host::default();
        host.binding_ranges.insert(21115, range(42000));
        host.binding_ranges.insert(21118, range(43000));
        let mut exposed = range(44000);
        exposed
            .addresses
            .enabled
            .insert("64.23.194.12:44000".parse().unwrap());
        host.binding_ranges.insert(21121, exposed);
        host.bindings.insert(8080, single(8080, Some(8443)));
        let mut db = database(host);

        add_domain(&mut db, 21115).unwrap();

        let host = package_host(&mut db).de().unwrap();
        assert!(!domain_disabled(
            &host.binding_ranges[&21115].addresses,
            42000
        ));
        assert!(domain_disabled(
            &host.binding_ranges[&21118].addresses,
            43000
        ));
        assert!(!domain_disabled(
            &host.binding_ranges[&21121].addresses,
            44000
        ));
        assert!(domain_disabled(&host.bindings[&8080].addresses, 8080));
        assert!(domain_disabled(&host.bindings[&8080].addresses, 8443));
    }

    #[test]
    fn readding_range_domain_preserves_address_overrides() {
        let mut host = Host::default();
        host.binding_ranges.insert(21115, range(42000));
        host.binding_ranges.insert(21118, range(43000));
        let mut db = database(host);
        add_domain(&mut db, 21115).unwrap();
        package_host(&mut db)
            .as_binding_ranges_mut()
            .mutate(|ranges| {
                let target = &mut ranges.get_mut(&21115).unwrap().addresses;
                target.enabled.clear();
                target
                    .disabled
                    .insert((InternedString::intern(FQDN), 42000));
                let sibling = &mut ranges.get_mut(&21118).unwrap().addresses;
                sibling
                    .enabled
                    .insert("64.23.194.12:43000".parse().unwrap());
                sibling
                    .disabled
                    .remove(&(InternedString::intern(FQDN), 43000));
                Ok(())
            })
            .unwrap();
        let before = imbl_value::to_value(package_host(&mut db).as_binding_ranges()).unwrap();

        assert_eq!(add_domain(&mut db, 21115).unwrap(), 42000);

        assert_eq!(
            imbl_value::to_value(package_host(&mut db).as_binding_ranges()).unwrap(),
            before
        );
    }

    #[test]
    fn single_port_target_prefers_tls_probe_and_enables_dual_stack_plaintext() {
        let mut host = Host::default();
        host.bindings.insert(8080, single(8080, Some(8443)));
        host.binding_ranges.insert(21115, range(42000));
        let mut db = database(host);

        assert_eq!(add_domain(&mut db, 8080).unwrap(), 8443);

        let host = package_host(&mut db).de().unwrap();
        let addresses = &host.bindings[&8080].addresses;
        assert!(
            addresses
                .enabled
                .contains(&"64.23.194.12:8080".parse().unwrap())
        );
        assert!(
            addresses
                .gua_wan
                .contains(&"[2001:db8::1]:8080".parse().unwrap())
        );
        assert!(addresses.enabled().contains(&domain(true, 8443, "wg1")));
        assert!(domain_disabled(
            &host.binding_ranges[&21115].addresses,
            42000
        ));
    }

    #[test]
    fn domain_target_requires_a_binding_or_range_start_port() {
        let mut host = Host::default();
        host.binding_ranges.insert(21115, range(42000));
        let mut db = database(host);

        assert!(
            add_domain(&mut db, 21116)
                .unwrap_err()
                .to_string()
                .contains("binding not found")
        );
    }

    #[test]
    fn active_range_takes_precedence_over_a_dormant_single_port_binding() {
        let mut host = Host::default();
        let mut dormant = single(21115, None);
        dormant.enabled = false;
        host.bindings.insert(21115, dormant);
        host.binding_ranges.insert(21115, range(42000));
        let mut db = database(host);

        assert_eq!(add_domain(&mut db, 21115).unwrap(), 42000);

        let host = package_host(&mut db).de().unwrap();
        assert!(!domain_disabled(
            &host.binding_ranges[&21115].addresses,
            42000
        ));
        assert!(domain_disabled(&host.bindings[&21115].addresses, 21115));
    }

    #[test]
    fn active_single_port_binding_takes_precedence_over_a_dormant_range() {
        let mut host = Host::default();
        host.bindings.insert(21115, single(21115, None));
        let mut dormant = range(42000);
        dormant.enabled = false;
        host.binding_ranges.insert(21115, dormant);
        let mut db = database(host);

        assert_eq!(add_domain(&mut db, 21115).unwrap(), 21115);

        let host = package_host(&mut db).de().unwrap();
        assert!(!domain_disabled(&host.bindings[&21115].addresses, 21115));
        assert!(domain_disabled(
            &host.binding_ranges[&21115].addresses,
            42000
        ));
    }

    #[test]
    fn enabled_ranges_keep_domain_ownership() {
        let mut host = Host::default();
        host.binding_ranges.insert(21115, range(42000));
        let mut db = database(host);
        add_domain(&mut db, 21115).unwrap();
        let mut server_host = Host::default();
        server_host.bindings.insert(80, single(80, None));
        server_host.public_domains.insert(
            InternedString::intern(FQDN),
            PublicDomainConfig {
                gateway: gw("wg1"),
                acme: None,
            },
        );
        db.as_public_mut()
            .as_server_info_mut()
            .as_network_mut()
            .as_host_mut()
            .ser(&server_host)
            .unwrap();

        assert!(
            handle_duplicates(&mut db)
                .unwrap_err()
                .to_string()
                .contains("already in use")
        );
    }

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

    #[test]
    fn enabling_v4_with_domain_publishes_gua() {
        let fqdn = InternedString::intern(FQDN);
        let mut a = DerivedAddressInfo::default();
        a.available.insert(domain(false, 42000, "wg1"));
        a.available.insert(wan_ip(42000, "wg1"));
        a.available.insert(gua(42000, "wg1"));
        a.enabled.insert("64.23.194.12:42000".parse().unwrap());

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
    fn rejects_the_servers_mdns_name_as_a_private_domain() {
        let hostname = ServerHostname::new(InternedString::intern("myserver")).unwrap();

        assert!(hostname.reject_private_domain("myserver.local").is_err());
        assert!(hostname.reject_private_domain("myserver.local.").is_err());
        assert!(hostname.reject_private_domain("service.local").is_ok());
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

        reconcile_private_domain_on_sibling(&mut a, &fqdn, &gw("wg1"));
        assert!(
            !a.disabled.contains(&priv_key),
            "a private domain is on by default (LAN is not isolated)"
        );

        a.disabled
            .insert((InternedString::intern("10.0.0.5"), 42000));
        reconcile_private_domain_on_sibling(&mut a, &fqdn, &gw("wg1"));
        assert!(
            a.disabled.contains(&priv_key),
            "a disabled LAN IPv4 takes the private domain down too"
        );
    }
}
