use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, SocketAddrV4};
use std::panic::RefUnwindSafe;

use clap::Parser;
use imbl::OrdMap;
use imbl_value::InternedString;
use itertools::Itertools;
use patch_db::DestructureMut;
use patch_db::json_ptr::JsonPointer;
use rpc_toolkit::{Context, Empty, HandlerExt, OrEmpty, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::context::RpcContext;
use crate::db::model::DatabaseModel;
use crate::db::model::public::{GatewayType, NetworkInterfaceInfo, NetworkInterfaceType};
use crate::hostname::ServerHostname;
use crate::net::forward::AvailablePorts;
use crate::net::host::address::{HostAddress, PublicDomainConfig, address_api};
use crate::net::host::binding::{
    BindInfo, BindOptions, BindingRanges, Bindings, RangeBindInfo, binding, internal_span,
    overlap_error,
};
use crate::net::service_interface::{HostnameInfo, HostnameMetadata};
use crate::net::vhost::ACME_CHALLENGE_PORT;
use crate::prelude::*;
use crate::util::serde::const_true;
use crate::{GatewayId, HostId, PackageId};

pub mod address;
pub mod binding;

/// The server UI's assigned SSL port. Operator port reassignment cannot move it.
pub const SERVER_UI_SSL_PORT: u16 = 443;

#[derive(Debug, Default, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct Host {
    pub bindings: Bindings,
    #[serde(default)]
    pub binding_ranges: BindingRanges,
    pub public_domains: BTreeMap<InternedString, PublicDomainConfig>,
    pub private_domains: BTreeMap<InternedString, BTreeSet<GatewayId>>,
    /// COMPUTED: port forwarding rules needed on gateways for public addresses to work.
    #[serde(default)]
    pub port_forwards: BTreeSet<PortForward>,
}

rpc_toolkit::reflect_ts!(Host);
rpc_toolkit::ts_export!(Host, namespaces = [""]);

fn default_port_forward_count() -> u16 {
    1
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct PortForward {
    pub src: SocketAddrV4,
    pub dst: SocketAddrV4,
    pub gateway: GatewayId,
    /// Number of contiguous ports covered by this forward (always >= 1).
    /// Set to >1 by [`crate::net::host::binding::RangeBindInfo`] entries to
    /// represent a single iptables port-range rule rather than N per-port
    /// rules. Defaults to 1 to keep the on-disk shape compatible with
    /// existing single-port `PortForward`s.
    #[serde(default = "default_port_forward_count")]
    pub count: u16,
    /// Whether this forward receives local traffic. Used to gate hairpinning check.
    #[serde(default = "const_true")]
    pub local: bool,
}

rpc_toolkit::reflect_ts!(PortForward);
rpc_toolkit::ts_export!(PortForward, namespaces = [""]);

impl AsRef<Host> for Host {
    fn as_ref(&self) -> &Host {
        self
    }
}
impl Host {
    pub fn new() -> Self {
        Self::default()
    }

    /// Hides disabled insecure candidates from legacy service SDK address readers.
    pub fn into_service_view(mut self) -> Self {
        for binding in self.bindings.values_mut() {
            binding.addresses.project_service_selections();
        }
        self
    }
    /// Hand every external port this host holds back to the pool. Shared by
    /// uninstall and retirement so the two cannot drift apart.
    pub fn release(&self, available_ports: &mut AvailablePorts) {
        for info in self.bindings.values() {
            info.release(available_ports);
        }
        for range in self.binding_ranges.values() {
            range.release(available_ports);
        }
    }
    pub fn addresses<'a>(&'a self) -> impl Iterator<Item = HostAddress> + 'a {
        self.public_domains
            .iter()
            .map(|(address, config)| HostAddress {
                address: address.clone(),
                public: Some(config.clone()),
                private: self.private_domains.get(address).cloned(),
            })
            .chain(
                self.private_domains
                    .iter()
                    .filter(|(domain, _)| !self.public_domains.contains_key(*domain))
                    .map(|(domain, gateways)| HostAddress {
                        address: domain.clone(),
                        public: None,
                        private: Some(gateways.clone()),
                    }),
            )
    }
}
/// Gateways over which `<hostname>.local` is resolvable: a LAN interface
/// serves it by mDNS multicast; a WireGuard interface only once its resolver
/// has accepted the injected record (`net::dns_update`), tracked as its
/// `dns_update` capability. A disconnected gateway keeps its place in `previous`.
fn mdns_gateways(
    gateways: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    previous: &BTreeSet<HostnameInfo>,
) -> BTreeSet<GatewayId> {
    gateways
        .iter()
        .filter(|(id, g)| match &g.ip_info {
            Some(ip_info) => {
                matches!(
                    ip_info.device_type,
                    Some(NetworkInterfaceType::Ethernet | NetworkInterfaceType::Wireless)
                ) || (g.is_wireguard() && g.dns_update.supported == Some(true))
            }
            None => previous.iter().any(|h| {
                matches!(
                    &h.metadata,
                    HostnameMetadata::Mdns { gateways } if gateways.contains(*id)
                )
            }),
        })
        .map(|(id, _)| id.clone())
        .collect()
}

impl Model<Host> {
    /// Runs for service, network and operator changes alike. An operator action
    /// that groups addresses applies its intent first through `derive_addresses`.
    pub fn update_addresses(
        &mut self,
        mdns: &ServerHostname,
        gateways: &OrdMap<GatewayId, NetworkInterfaceInfo>,
        available_ports: &AvailablePorts,
    ) -> Result<(), Error> {
        let previous = self.as_bindings().de()?;
        self.derive_addresses(mdns, gateways, available_ports)?;
        self.as_bindings_mut().mutate(|bindings| {
            for (port, binding) in bindings.iter_mut() {
                let addresses = previous
                    .get(port)
                    .map_or_else(Default::default, |info| info.addresses.clone());
                binding.addresses.resolve_address_groups(&addresses);
            }
            Ok(())
        })?;
        self.derive_addresses(mdns, gateways, available_ports)
    }

    /// Returns unselected candidates without changing the host or reserving ports.
    pub(crate) fn preview_binding(
        &self,
        internal_port: u16,
        options: BindOptions,
        mdns: &ServerHostname,
        gateways: &OrdMap<GatewayId, NetworkInterfaceInfo>,
        available_ports: &AvailablePorts,
    ) -> Result<binding::BindingPreview, Error> {
        let mut preview = self.clone();
        let mut binding = BindInfo::unallocated(options);
        // Port zero is a derivation placeholder, never a reservation.
        binding.net.assigned_port = binding.options.wants_plain_port().then_some(0);
        binding.net.assigned_ssl_port = binding.options.preferred_ssl_port().map(|_| 0);
        preview.as_bindings_mut().insert(&internal_port, &binding)?;
        preview.derive_addresses(mdns, gateways, available_ports)?;
        let binding: BindInfo = preview.as_bindings().as_idx(&internal_port).unwrap().de()?;
        let available = binding
            .addresses
            .available
            .into_iter()
            .map(|mut address| {
                address.port = None;
                address
            })
            .collect();
        let mut addresses = binding::DerivedAddressInfo::default();
        addresses.set_available(available, true);
        Ok(binding::BindingPreview {
            options: binding.options,
            addresses,
        })
    }

    /// Rebuilds the address rows and port forwards from the stored selections.
    fn derive_addresses(
        &mut self,
        mdns: &ServerHostname,
        gateways: &OrdMap<GatewayId, NetworkInterfaceInfo>,
        available_ports: &AvailablePorts,
    ) -> Result<(), Error> {
        let this = self.destructure_mut();
        this.private_domains.remove(&mdns.local_domain_name())?;

        // ips
        for (_, bind) in this.bindings.as_entries_mut()? {
            let net = bind.as_net().de()?;
            let opt = bind.as_options().de()?;

            // Preserve existing plugin-provided addresses across recomputation
            let mut available = bind.as_addresses().as_available().de()?;
            let mdns_gateways = mdns_gateways(gateways, &available);
            available.retain(|h| matches!(h.metadata, HostnameMetadata::Plugin { .. }));
            for (gid, g) in gateways {
                if g.gateway_type == GatewayType::OutboundOnly {
                    continue;
                }
                let Some(ip_info) = &g.ip_info else {
                    continue;
                };
                for subnet in &ip_info.subnets {
                    let host = InternedString::from_display(&subnet.addr());
                    let metadata = if subnet.addr().is_ipv4() {
                        HostnameMetadata::Ipv4 {
                            gateway: gid.clone(),
                        }
                    } else {
                        HostnameMetadata::Ipv6 {
                            gateway: gid.clone(),
                            scope_id: ip_info.scope_id,
                        }
                    };
                    if let Some(port) = net.assigned_port.filter(|_| opt.wants_plain_port()) {
                        available.insert(HostnameInfo {
                            ssl: opt.secure.map_or(false, |s| s.ssl),
                            public: false,
                            hostname: host.clone(),
                            port: Some(port),
                            metadata: metadata.clone(),
                        });
                    }
                    if let Some(port) = net.assigned_ssl_port {
                        available.insert(HostnameInfo {
                            ssl: true,
                            public: false,
                            hostname: host.clone(),
                            port: Some(port),
                            metadata,
                        });
                    }
                }
                if let Some(wan_ip) = &ip_info.wan_ip {
                    let host = InternedString::from_display(&wan_ip);
                    let metadata = HostnameMetadata::Ipv4 {
                        gateway: gid.clone(),
                    };
                    if let Some(port) = net
                        .assigned_port
                        .filter(|_| opt.allows_public_address(false) && opt.wants_plain_port())
                    {
                        available.insert(HostnameInfo {
                            ssl: opt.secure.map_or(false, |s| s.ssl),
                            public: true,
                            hostname: host.clone(),
                            port: Some(port),
                            metadata: metadata.clone(),
                        });
                    }
                    if let Some(port) = net.assigned_ssl_port {
                        available.insert(HostnameInfo {
                            ssl: true,
                            public: true,
                            hostname: host.clone(),
                            port: Some(port),
                            metadata,
                        });
                    }
                }
            }

            // mdns
            let mdns_host = mdns.local_domain_name();
            if let Some(port) = net.assigned_port.filter(|_| opt.wants_plain_port()) {
                let mdns_gateways = mdns_gateways.clone();
                if !mdns_gateways.is_empty() {
                    available.insert(HostnameInfo {
                        ssl: opt.secure.map_or(false, |s| s.ssl),
                        public: false,
                        hostname: mdns_host.clone(),
                        port: Some(port),
                        metadata: HostnameMetadata::Mdns {
                            gateways: mdns_gateways,
                        },
                    });
                }
            }
            if let Some(port) = net.assigned_ssl_port.filter(|_| !mdns_gateways.is_empty()) {
                available.insert(HostnameInfo {
                    ssl: true,
                    public: false,
                    hostname: mdns_host,
                    port: Some(port),
                    metadata: HostnameMetadata::Mdns {
                        gateways: mdns_gateways,
                    },
                });
            }

            // public domains
            for (domain, info) in this.public_domains.de()? {
                let metadata = HostnameMetadata::PublicDomain {
                    gateway: info.gateway.clone(),
                };
                if let Some(port) = net
                    .assigned_port
                    .filter(|_| opt.allows_public_address(false) && opt.wants_plain_port())
                {
                    available.insert(HostnameInfo {
                        ssl: opt.secure.map_or(false, |s| s.ssl),
                        public: true,
                        hostname: domain.clone(),
                        port: Some(port),
                        metadata: metadata.clone(),
                    });
                }
                if let Some(mut port) = net.assigned_ssl_port {
                    // The domain rides the preferred port whenever one of our
                    // TLS listeners answers there — SNI routes it, terminating
                    // (add_ssl) or passthrough (self-TLS) alike.
                    if let Some(preferred) = opt
                        .preferred_ssl_port()
                        .filter(|p| available_ports.is_ssl(*p))
                    {
                        port = preferred;
                    }
                    available.insert(HostnameInfo {
                        ssl: true,
                        public: true,
                        hostname: domain,
                        port: Some(port),
                        metadata,
                    });
                }
            }

            // private domains
            for (domain, domain_gateways) in this.private_domains.de()? {
                if let Some(port) = net.assigned_port.filter(|_| opt.wants_plain_port()) {
                    let gateways = domain_gateways.clone();
                    available.insert(HostnameInfo {
                        ssl: opt.secure.map_or(false, |s| s.ssl),
                        public: false,
                        hostname: domain.clone(),
                        port: Some(port),
                        metadata: HostnameMetadata::PrivateDomain { gateways },
                    });
                }
                if let Some(mut port) = net.assigned_ssl_port {
                    if let Some(preferred) = opt
                        .preferred_ssl_port()
                        .filter(|p| available_ports.is_ssl(*p))
                    {
                        port = preferred;
                    }
                    available.insert(HostnameInfo {
                        ssl: true,
                        public: false,
                        hostname: domain,
                        port: Some(port),
                        metadata: HostnameMetadata::PrivateDomain {
                            gateways: domain_gateways,
                        },
                    });
                }
            }
            let mut addresses = bind.as_addresses().de()?;
            if !opt.allows_public_address(false) {
                addresses
                    .gua_wan
                    .retain(|g| Some(g.port()) != net.assigned_port);
            }
            let mut split = BTreeSet::new();
            for address in available {
                if opt.secure.is_none() && !address.ssl {
                    if let HostnameMetadata::Mdns {
                        gateways: candidates,
                    }
                    | HostnameMetadata::PrivateDomain {
                        gateways: candidates,
                    } = &address.metadata
                    {
                        for gateway in candidates {
                            let mut row = address.clone();
                            match &mut row.metadata {
                                HostnameMetadata::Mdns { gateways }
                                | HostnameMetadata::PrivateDomain { gateways } => {
                                    *gateways = BTreeSet::from([gateway.clone()])
                                }
                                _ => unreachable!(),
                            }
                            split.insert(row);
                        }
                        continue;
                    }
                }
                split.insert(address);
            }
            addresses.set_available(split, opt.default_off);
            addresses.insecure = addresses
                .available
                .iter()
                .filter(|a| opt.secure.is_none() && !a.ssl && !a.public && !a.is_internal())
                .filter(|a| {
                    a.metadata
                        .gateways()
                        .any(|g| !gateways.get(g).is_some_and(|g| g.secure()))
                })
                .filter_map(crate::net::host::binding::insecure_key)
                .collect();
            if opt.default_off && !bind.as_enabled().de()? {
                addresses.disable_all();
            }
            addresses.refresh_effective();
            bind.as_addresses_mut().ser(&addresses)?;
        }

        // Port-range bindings get the same reachable-address set as single-port
        // bindings, but IPv4-only and non-SSL: LAN IPv4 / WAN IPv4 / server mDNS
        // / public+private domains. Every entry uses the range's
        // `external_start_port` as its port so the single-port
        // enabled/disabled + forward machinery applies unchanged.
        let mdns_host = mdns.local_domain_name();
        for (_, range) in this.binding_ranges.as_entries_mut()? {
            let transport = range.de()?.transport;
            // PatchDB clients must receive the serde-defaulted transport before service rebinding.
            range.as_transport_mut().ser(&transport)?;
            let port = range.as_external_start_port().de()?;

            // Preserve any plugin-provided addresses across recomputation.
            let mut available = range.as_addresses().as_available().de()?;
            let mdns_gateways = mdns_gateways(gateways, &available);
            available.retain(|h| matches!(h.metadata, HostnameMetadata::Plugin { .. }));

            for (gid, g) in gateways {
                if g.gateway_type == GatewayType::OutboundOnly {
                    continue;
                }
                let Some(ip_info) = &g.ip_info else {
                    continue;
                };
                for subnet in &ip_info.subnets {
                    // IPv4 only — ranges forward purely as IPv4 nft DNAT.
                    if !subnet.addr().is_ipv4() {
                        continue;
                    }
                    available.insert(HostnameInfo {
                        ssl: false,
                        public: false,
                        hostname: InternedString::from_display(&subnet.addr()),
                        port: Some(port),
                        metadata: HostnameMetadata::Ipv4 {
                            gateway: gid.clone(),
                        },
                    });
                }
                if let Some(wan_ip) = &ip_info.wan_ip {
                    available.insert(HostnameInfo {
                        ssl: false,
                        public: true,
                        hostname: InternedString::from_display(&wan_ip),
                        port: Some(port),
                        metadata: HostnameMetadata::Ipv4 {
                            gateway: gid.clone(),
                        },
                    });
                }
            }

            if !mdns_gateways.is_empty() {
                available.insert(HostnameInfo {
                    ssl: false,
                    public: false,
                    hostname: mdns_host.clone(),
                    port: Some(port),
                    metadata: HostnameMetadata::Mdns {
                        gateways: mdns_gateways,
                    },
                });
            }

            for (domain, info) in this.public_domains.de()? {
                available.insert(HostnameInfo {
                    ssl: false,
                    public: true,
                    hostname: domain,
                    port: Some(port),
                    metadata: HostnameMetadata::PublicDomain {
                        gateway: info.gateway.clone(),
                    },
                });
            }

            for (domain, domain_gateways) in this.private_domains.de()? {
                available.insert(HostnameInfo {
                    ssl: false,
                    public: false,
                    hostname: domain,
                    port: Some(port),
                    metadata: HostnameMetadata::PrivateDomain {
                        gateways: domain_gateways,
                    },
                });
            }

            range.as_addresses_mut().mutate(|addresses| {
                addresses.set_available(available, false);
                Ok(())
            })?;
        }

        // compute port forwards from enabled public addresses. A non-exported
        // binding/range serves internally only (`enabled_addresses`), and no
        // internal address is public, so it contributes nothing here.
        let bindings: Bindings = this.bindings.de()?;
        let public_domains = this.public_domains.de()?;
        let mut port_forwards = BTreeSet::new();
        for bind in bindings.values().filter(|bind| bind.enabled) {
            for addr in bind.enabled_addresses() {
                if !addr.public {
                    continue;
                }
                let Some(port) = addr.port else {
                    continue;
                };
                let gw_id = match &addr.metadata {
                    HostnameMetadata::Ipv4 { gateway }
                    | HostnameMetadata::PublicDomain { gateway } => gateway,
                    _ => continue,
                };
                let Some(gw_info) = gateways.get(gw_id) else {
                    continue;
                };
                let Some(ip_info) = &gw_info.ip_info else {
                    continue;
                };
                let Some(wan_ip) = ip_info.wan_ip else {
                    continue;
                };
                // A certificate authority validates at `ACME_CHALLENGE_PORT`,
                // regardless of which port the address serves. `add_ssl` excludes a
                // service that is its own ACME client.
                let challenge = addr.ssl
                    && bind.options.add_ssl.is_some()
                    && port != ACME_CHALLENGE_PORT
                    && matches!(addr.metadata, HostnameMetadata::PublicDomain { .. })
                    && public_domains
                        .get(&addr.hostname)
                        .map_or(false, |domain| domain.acme.is_some());
                for subnet in &ip_info.subnets {
                    let IpAddr::V4(addr) = subnet.addr() else {
                        continue;
                    };
                    port_forwards.insert(PortForward {
                        src: SocketAddrV4::new(wan_ip, port),
                        dst: SocketAddrV4::new(addr, port),
                        gateway: gw_id.clone(),
                        count: 1,
                        local: true,
                    });
                    if challenge {
                        port_forwards.insert(PortForward {
                            src: SocketAddrV4::new(wan_ip, ACME_CHALLENGE_PORT),
                            dst: SocketAddrV4::new(addr, ACME_CHALLENGE_PORT),
                            gateway: gw_id.clone(),
                            count: 1,
                            local: false,
                        });
                    }
                }
            }
        }

        // Port-range bindings: emit a router PortForward (the WAN exposure the
        // operator must add) for each enabled PUBLIC address — same as the
        // single-port loop above, but with `count = number_of_ports`. A fresh
        // range's public (WAN) addresses are disabled by default, so it
        // contributes nothing until the operator enables the WAN address.
        let binding_ranges: BindingRanges = this.binding_ranges.de()?;
        for (&internal_start, range) in binding_ranges.iter() {
            if !range.enabled {
                continue;
            }
            for addr in range.enabled_addresses() {
                if !addr.public {
                    continue;
                }
                let gw_id = match &addr.metadata {
                    HostnameMetadata::Ipv4 { gateway }
                    | HostnameMetadata::PublicDomain { gateway } => gateway,
                    _ => continue,
                };
                let Some(gw_info) = gateways.get(gw_id) else {
                    continue;
                };
                let Some(ip_info) = &gw_info.ip_info else {
                    continue;
                };
                let Some(wan_ip) = ip_info.wan_ip else {
                    continue;
                };
                for subnet in &ip_info.subnets {
                    let IpAddr::V4(lan_ip) = subnet.addr() else {
                        continue;
                    };
                    port_forwards.insert(PortForward {
                        src: SocketAddrV4::new(wan_ip, range.external_start_port),
                        dst: SocketAddrV4::new(lan_ip, internal_start),
                        gateway: gw_id.clone(),
                        count: range.number_of_ports,
                        local: true,
                    });
                }
            }
        }

        this.port_forwards.ser(&port_forwards)?;

        Ok(())
    }
}

#[derive(Debug, Default, Deserialize, Serialize, HasModel, VisitFields)]
#[model = "Model<Self>"]
pub struct Hosts(pub BTreeMap<HostId, Host>);

rpc_toolkit::reflect_ts!(Hosts);
rpc_toolkit::ts_export!(Hosts, namespaces = [""]);

impl Map for Hosts {
    type Key = HostId;
    type Value = Host;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Ok(key)
    }
    fn key_string(key: &Self::Key) -> Result<InternedString, Error> {
        Ok(key.clone().into())
    }
}

pub fn host_for<'a>(
    db: &'a mut DatabaseModel,
    package_id: &PackageId,
    host_id: &HostId,
) -> Result<&'a mut Model<Host>, Error> {
    // `start-os` is the server itself: its single host (the admin UI's) lives
    // in serverInfo, not packageData.
    if package_id.is_start_os() {
        if *host_id != HostId::admin() {
            return Err(Error::new(
                eyre!("the server has no host {host_id}"),
                ErrorKind::NotFound,
            ));
        }
        return Ok(db
            .as_public_mut()
            .as_server_info_mut()
            .as_network_mut()
            .as_host_mut());
    }
    fn host_info<'a>(
        db: &'a mut DatabaseModel,
        package_id: &PackageId,
    ) -> Result<&'a mut Model<Hosts>, Error> {
        Ok::<_, Error>(
            db.as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(package_id)
                .or_not_found(package_id)?
                .as_hosts_mut(),
        )
    }
    host_info(db, package_id)?.upsert(host_id, || Ok(Host::new()))
}

/// [`host_for`] without the upsert, for paths where a missing host means there
/// is nothing to do. Creating a host on the way to removing something from it
/// resurrects one the service has retired.
pub fn host_for_existing<'a>(
    db: &'a mut DatabaseModel,
    package_id: &PackageId,
    host_id: &HostId,
) -> Result<Option<&'a mut Model<Host>>, Error> {
    if package_id.is_start_os() {
        if *host_id != HostId::admin() {
            return Ok(None);
        }
        return Ok(Some(
            db.as_public_mut()
                .as_server_info_mut()
                .as_network_mut()
                .as_host_mut(),
        ));
    }
    Ok(db
        .as_public_mut()
        .as_package_data_mut()
        .as_idx_mut(package_id)
        .or_not_found(package_id)?
        .as_hosts_mut()
        .as_idx_mut(host_id))
}

pub fn all_hosts(db: &mut DatabaseModel) -> impl Iterator<Item = Result<&mut Model<Host>, Error>> {
    use patch_db::DestructureMut;
    let destructured = db.as_public_mut().destructure_mut();
    [Ok(destructured.server_info.as_network_mut().as_host_mut())]
        .into_iter()
        .chain(
            [destructured.package_data.as_entries_mut()]
                .into_iter()
                .flatten_ok()
                .map(|entry| entry.and_then(|(_, v)| v.as_hosts_mut().as_entries_mut()))
                .flatten_ok()
                .map_ok(|(_, v)| v),
        )
}

impl Model<Host> {
    pub fn add_binding(
        &mut self,
        available_ports: &mut AvailablePorts,
        internal_port: u16,
        options: BindOptions,
        privileged: bool,
    ) -> Result<(), Error> {
        let claim = internal_port..=internal_port;
        if let Some(existing) = self
            .as_binding_ranges()
            .de()?
            .enabled_overlap(&claim, None)?
        {
            return Err(overlap_error(&claim, &existing));
        }
        self.as_bindings_mut().mutate(|b| {
            let info = if let Some(info) = b.remove(&internal_port) {
                info.update(available_ports, options, privileged)?
            } else {
                BindInfo::new(available_ports, options, privileged)?
            };
            b.insert(internal_port, info);
            Ok(())
        })
    }

    /// Add or re-affirm a contiguous port-range binding. The external ports
    /// (`external_start_port..`) are all allocated up-front — partial
    /// collisions are a hard error. `internal_start_port` may differ from
    /// `external_start_port`; the forward-port script maps the two ranges by
    /// offset via an nft verdict map.
    pub fn add_binding_range(
        &mut self,
        available_ports: &mut AvailablePorts,
        internal_start_port: u16,
        external_start_port: u16,
        number_of_ports: u16,
        privileged: bool,
        transport: crate::net::transport::Transport,
    ) -> Result<(), Error> {
        if number_of_ports < 2 {
            return Err(Error::new(
                eyre!("numberOfPorts must be at least 2; use bind for a single port"),
                ErrorKind::InvalidRequest,
            ));
        }
        let claim = internal_span(internal_start_port, number_of_ports)?;
        if let Some(existing) = self.as_bindings().de()?.enabled_overlap(&claim) {
            return Err(overlap_error(&claim, &existing));
        }
        if let Some(existing) = self
            .as_binding_ranges()
            .de()?
            .enabled_overlap(&claim, Some(internal_start_port))?
        {
            return Err(overlap_error(&claim, &existing));
        }
        self.as_binding_ranges_mut().mutate(|ranges| {
            let existing = ranges.get(&internal_start_port);
            // Idempotent re-bind: a package may call bindPortRange on every
            // setupMain pass. An unchanged range keeps its allocation; the
            // insert below just re-affirms (re-enables) it.
            let unchanged = matches!(
                existing,
                Some(e)
                    if e.external_start_port == external_start_port
                        && e.number_of_ports == number_of_ports
            );
            // Preserve the operator's per-address enable/disable choices across
            // rebinds — bindPortRange is service-driven, so it must not clobber
            // a UI choice made between restarts. The `available` set is
            // recomputed by update_addresses.
            let mut addresses = existing.map(|e| e.addresses.clone()).unwrap_or_default();
            // A resized/moved range changes external_start_port; carry the
            // preserved overrides to the new representative port so a disabled
            // WAN address stays disabled.
            if let Some(old) = existing.map(|e| e.external_start_port) {
                addresses.rekey_port(old, external_start_port);
            }
            // Preserve the exported interface across rebinds; the trailing
            // `RangeOrigin.export` re-affirms it and `clearServiceInterfaces`
            // prunes it if the service no longer exports it.
            let interface = existing.and_then(|e| e.interface.clone());
            if !unchanged {
                // New, resized, or moved range: free any prior allocation, then
                // claim the new one. `number_of_ports >= 2`, so subtract before
                // adding — `start + count` would overflow u16 for a range ending
                // at 65535.
                if let Some(e) = existing {
                    available_ports.free(
                        e.external_start_port..=e.external_start_port + (e.number_of_ports - 1),
                    );
                }
                available_ports.try_alloc_range(
                    external_start_port,
                    number_of_ports,
                    privileged,
                )?;
            }
            ranges.insert(
                internal_start_port,
                RangeBindInfo {
                    enabled: true,
                    transport,
                    external_start_port,
                    number_of_ports,
                    addresses,
                    interface,
                },
            );
            Ok(())
        })
    }
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
pub struct RequiresPackageId {
    #[arg(help = "help.arg.package-id")]
    package: PackageId,
}

rpc_toolkit::reflect_ts!(RequiresPackageId);

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
pub struct RequiresHostId {
    #[arg(help = "help.arg.host-id")]
    host: HostId,
}

rpc_toolkit::reflect_ts!(RequiresHostId);

pub trait HostApiKind: 'static {
    type Params: Send + Sync + 'static;
    type InheritedParams: Send + Sync + 'static;
    type Inheritance: RefUnwindSafe + OrEmpty<Self::Inheritance> + Send + Sync + 'static;
    fn inheritance(params: Self::Params, inherited: Self::InheritedParams) -> Self::Inheritance;
    fn is_server(inheritance: &Self::Inheritance) -> bool;
    fn host_for<'a>(
        inheritance: &Self::Inheritance,
        db: &'a mut DatabaseModel,
    ) -> Result<&'a mut Model<Host>, Error>;
    fn host_pointer(inheritance: &Self::Inheritance) -> Result<JsonPointer, Error>;
    fn host_for_existing<'a>(
        inheritance: &Self::Inheritance,
        db: &'a mut DatabaseModel,
    ) -> Result<Option<&'a mut Model<Host>>, Error>;
}
pub struct ForPackage;
impl HostApiKind for ForPackage {
    type Params = RequiresHostId;
    type InheritedParams = PackageId;
    type Inheritance = (PackageId, HostId);
    fn is_server((package, _): &Self::Inheritance) -> bool {
        package.is_start_os()
    }
    fn inheritance(
        RequiresHostId { host }: Self::Params,
        package: Self::InheritedParams,
    ) -> Self::Inheritance {
        (package, host)
    }
    fn host_for<'a>(
        (package, host): &Self::Inheritance,
        db: &'a mut DatabaseModel,
    ) -> Result<&'a mut Model<Host>, Error> {
        host_for(db, package, host)
    }
    fn host_pointer((package, host): &Self::Inheritance) -> Result<JsonPointer, Error> {
        if package.is_start_os() {
            if *host != HostId::admin() {
                return Err(Error::new(
                    eyre!("the server has no host {host}"),
                    ErrorKind::NotFound,
                ));
            }
            return Ok("/public/serverInfo/network/host".parse().unwrap());
        }
        let mut pointer: JsonPointer = "/public/packageData".parse().unwrap();
        pointer.push_end(package);
        pointer.push_end("hosts");
        pointer.push_end(host);
        Ok(pointer)
    }
    fn host_for_existing<'a>(
        (package, host): &Self::Inheritance,
        db: &'a mut DatabaseModel,
    ) -> Result<Option<&'a mut Model<Host>>, Error> {
        host_for_existing(db, package, host)
    }
}
pub struct ForServer;
impl HostApiKind for ForServer {
    type Params = Empty;
    type InheritedParams = Empty;
    type Inheritance = Empty;
    fn is_server(_: &Self::Inheritance) -> bool {
        true
    }
    fn inheritance(_: Self::Params, _: Self::InheritedParams) -> Self::Inheritance {
        Empty {}
    }
    fn host_for<'a>(
        _: &Self::Inheritance,
        db: &'a mut DatabaseModel,
    ) -> Result<&'a mut Model<Host>, Error> {
        host_for(db, &PackageId::start_os(), &HostId::admin())
    }
    fn host_pointer(_: &Self::Inheritance) -> Result<JsonPointer, Error> {
        Ok("/public/serverInfo/network/host".parse().unwrap())
    }
    fn host_for_existing<'a>(
        _: &Self::Inheritance,
        db: &'a mut DatabaseModel,
    ) -> Result<Option<&'a mut Model<Host>>, Error> {
        host_for_existing(db, &PackageId::start_os(), &HostId::admin())
    }
}

pub fn host_api<C: Context>() -> ParentHandler<C, RequiresPackageId> {
    ParentHandler::<C, RequiresPackageId>::new()
        .subcommand(
            "list",
            from_fn_async(list_hosts)
                .with_inherited(|RequiresPackageId { package }, _| package)
                .with_custom_display_fn(|_, ids| {
                    for id in ids {
                        println!("{id}")
                    }
                    Ok(())
                })
                .with_about("about.list-host-ids-for-service"),
        )
        .subcommand(
            "address",
            address_api::<C, ForPackage>()
                .with_inherited(|RequiresPackageId { package }, _| package)
                .with_about("about.commands-host-addresses"),
        )
        .subcommand(
            "binding",
            binding::<C, ForPackage>()
                .with_inherited(|RequiresPackageId { package }, _| package)
                .with_about("about.commands-host-bindings"),
        )
}

pub fn server_host_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::<C>::new()
        .subcommand(
            "address",
            address_api::<C, ForServer>().with_about("about.commands-host-addresses"),
        )
        .subcommand(
            "binding",
            binding::<C, ForServer>().with_about("about.commands-host-bindings"),
        )
}

pub async fn list_hosts(
    ctx: RpcContext,
    _: Empty,
    package: PackageId,
) -> Result<BTreeSet<HostId>, Error> {
    ctx.db
        .peek()
        .await
        .into_public()
        .into_package_data()
        .into_idx(&package)
        .or_not_found(&package)?
        .into_hosts()
        .keys()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::net::Ipv4Addr;
    use std::sync::Arc;

    use imbl::OrdMap;
    use imbl_value::InternedString;
    use ipnet::IpNet;

    use super::{Host, Model, PublicDomainConfig, mdns_gateways};
    use crate::GatewayId;
    use crate::db::model::public::{
        CapabilityVerdict, GatewayType, IpInfo, NetworkInterfaceInfo, NetworkInterfaceType,
    };
    use crate::hostname::ServerHostname;
    use crate::net::forward::AvailablePorts;
    use crate::net::host::binding::BindOptions;
    use crate::net::service_interface::HostnameMetadata;
    use crate::prelude::*;

    fn iface(
        device_type: NetworkInterfaceType,
        dns_update_supported: Option<bool>,
    ) -> NetworkInterfaceInfo {
        NetworkInterfaceInfo {
            ip_info: Some(Arc::new(IpInfo {
                device_type: Some(device_type),
                ..Default::default()
            })),
            dns_update: CapabilityVerdict {
                supported: dns_update_supported,
                at: dns_update_supported.map(|_| chrono::Utc::now()),
            },
            ..Default::default()
        }
    }

    fn gw(id: &str) -> GatewayId {
        GatewayId::from(InternedString::intern(id))
    }

    #[test]
    fn mdns_gateways_lan_always_wireguard_only_when_injection_confirmed() {
        let ifaces: OrdMap<GatewayId, NetworkInterfaceInfo> = [
            (gw("eth0"), iface(NetworkInterfaceType::Ethernet, None)),
            (gw("wlan0"), iface(NetworkInterfaceType::Wireless, None)),
            // Accepted the injected record: list `.local` on it.
            (
                gw("wg-ok"),
                iface(NetworkInterfaceType::Wireguard, Some(true)),
            ),
            // Refused / never attempted: hide it.
            (
                gw("wg-no"),
                iface(NetworkInterfaceType::Wireguard, Some(false)),
            ),
            (gw("wg-new"), iface(NetworkInterfaceType::Wireguard, None)),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            mdns_gateways(&ifaces, &BTreeSet::new()),
            [gw("eth0"), gw("wlan0"), gw("wg-ok")].into_iter().collect()
        );
    }

    fn host() -> Model<Host> {
        Model::<Host>::new(&Host::default()).unwrap()
    }

    fn wireguard(gateway_type: GatewayType) -> NetworkInterfaceInfo {
        NetworkInterfaceInfo {
            ip_info: Some(Arc::new(IpInfo {
                device_type: Some(NetworkInterfaceType::Wireguard),
                subnets: ["192.0.2.2/24".parse::<IpNet>().unwrap()]
                    .into_iter()
                    .collect(),
                wan_ip: Some(Ipv4Addr::new(198, 51, 100, 2)),
                ..Default::default()
            })),
            gateway_type,
            ..Default::default()
        }
    }

    fn plain(preferred_external_port: u16) -> BindOptions {
        BindOptions {
            default_off: false,
            transport: Default::default(),
            preferred_external_port,
            add_ssl: None,
            secure: Some(crate::net::host::binding::Security { ssl: false }),
            proxy: None,
        }
    }

    fn default_off_host() -> (
        Model<Host>,
        AvailablePorts,
        OrdMap<GatewayId, NetworkInterfaceInfo>,
        ServerHostname,
    ) {
        let mut gateway = wireguard(GatewayType::InboundOutbound);
        Arc::make_mut(gateway.ip_info.as_mut().unwrap()).subnets = [
            "192.168.1.2/24",
            "2001:db8::2/64",
            "fd00::2/64",
            "fe80::2/64",
        ]
        .into_iter()
        .map(|address| address.parse::<IpNet>().unwrap())
        .collect();
        let gateways = [(gw("eth0"), gateway)].into_iter().collect();
        let server = ServerHostname::new(InternedString::intern("server")).unwrap();
        let mut ports = AvailablePorts::new();
        let mut host = host();
        host.add_binding(
            &mut ports,
            6881,
            BindOptions {
                default_off: true,
                ..plain(6881)
            },
            true,
        )
        .unwrap();
        host.as_public_domains_mut()
            .insert(
                &InternedString::intern("peers.example.com"),
                &PublicDomainConfig {
                    gateway: gw("eth0"),
                    acme: None,
                },
            )
            .unwrap();
        host.update_addresses(&server, &gateways, &ports).unwrap();
        (host, ports, gateways, server)
    }

    #[test]
    fn default_off_addresses_offer_private_and_public_candidates_without_consent() {
        let (host, _, _, _) = default_off_host();
        let binding = &host.de().unwrap().bindings[&6881];
        assert_eq!(
            binding
                .addresses
                .available
                .iter()
                .map(|address| address.hostname.as_ref())
                .collect::<BTreeSet<_>>(),
            [
                "192.168.1.2",
                "198.51.100.2",
                "2001:db8::2",
                "fd00::2",
                "fe80::2",
                "peers.example.com"
            ]
            .into_iter()
            .collect()
        );
        assert!(
            binding
                .addresses
                .available
                .iter()
                .any(|address| !address.public)
        );
        assert!(binding.addresses.enabled().is_empty());
    }

    #[test]
    fn default_off_rows_opt_in_through_native_dual_stack_domain_grouping() -> Result<(), Error> {
        let (mut host, ports, gateways, server) = default_off_host();
        let address = host.de()?.bindings[&6881]
            .addresses
            .available
            .iter()
            .find(|address| address.hostname == InternedString::intern("peers.example.com"))
            .unwrap()
            .clone();
        host.as_bindings_mut().mutate(|bindings| {
            bindings
                .get_mut(&6881)
                .unwrap()
                .set_address_enabled(&address, true)
        })?;
        host.update_addresses(&server, &gateways, &ports)?;
        let binding = &host.de()?.bindings[&6881];
        assert_eq!(binding.addresses.enabled().len(), 3);
        let selected = &binding.addresses.effective;
        assert_eq!(selected.len(), 3);
        assert!(selected.iter().all(|address| address.public));
        assert!(
            binding
                .addresses
                .enabled()
                .iter()
                .all(|address| address.public)
        );
        Ok(())
    }

    #[test]
    fn default_off_local_ipv6_opt_in_does_not_publish_a_new_domain() -> Result<(), Error> {
        let (mut host, ports, mut gateways, server) = default_off_host();
        host.as_public_domains_mut()
            .remove(&InternedString::intern("peers.example.com"))?;
        Arc::make_mut(
            gateways
                .get_mut(&gw("eth0"))
                .unwrap()
                .ip_info
                .as_mut()
                .unwrap(),
        )
        .wan_ip = None;
        host.update_addresses(&server, &gateways, &ports)?;
        let address = host.de()?.bindings[&6881]
            .addresses
            .available
            .iter()
            .find(|address| address.hostname == InternedString::intern("2001:db8::2"))
            .unwrap()
            .clone();
        assert!(!address.public);
        host.as_bindings_mut().mutate(|bindings| {
            bindings
                .get_mut(&6881)
                .unwrap()
                .set_address_enabled(&address, true)
        })?;
        host.update_addresses(&server, &gateways, &ports)?;
        assert_eq!(host.de()?.bindings[&6881].addresses.enabled().len(), 1);
        host.as_public_domains_mut().insert(
            &InternedString::intern("new-peers.example.com"),
            &PublicDomainConfig {
                gateway: gw("eth0"),
                acme: None,
            },
        )?;
        host.update_addresses(&server, &gateways, &ports)?;
        let binding = &host.de()?.bindings[&6881];
        assert_eq!(binding.addresses.enabled().len(), 1);
        assert!(
            binding
                .addresses
                .enabled()
                .iter()
                .all(|address| !address.public)
        );
        Ok(())
    }

    #[test]
    fn cleared_default_off_rows_stay_off_after_reenabling_binding() -> Result<(), Error> {
        let (mut host, ports, gateways, server) = default_off_host();
        let address = host.de()?.bindings[&6881]
            .addresses
            .available
            .iter()
            .find(|address| address.hostname == InternedString::intern("198.51.100.2"))
            .unwrap()
            .clone();
        host.as_bindings_mut().mutate(|bindings| {
            let binding = bindings.get_mut(&6881).unwrap();
            binding.set_address_enabled(&address, true)?;
            binding.disable();
            Ok(())
        })?;
        host.update_addresses(&server, &gateways, &ports)?;
        host.as_bindings_mut().mutate(|bindings| {
            let binding = bindings.get_mut(&6881).unwrap();
            assert!(binding.addresses.enabled().is_empty());
            assert!(binding.set_address_enabled(&address, true).is_err());
            binding.enabled = true;
            Ok(())
        })?;
        host.update_addresses(&server, &gateways, &ports)?;
        assert!(host.de()?.bindings[&6881].addresses.enabled().is_empty());
        host.as_bindings_mut().mutate(|bindings| {
            bindings
                .get_mut(&6881)
                .unwrap()
                .set_address_enabled(&address, true)
        })?;
        host.update_addresses(&server, &gateways, &ports)?;
        assert_eq!(host.de()?.bindings[&6881].addresses.enabled().len(), 3);
        Ok(())
    }

    #[test]
    fn address_updates_publish_legacy_range_transport_without_service_rebinding() {
        use crate::net::transport::Transport;

        for (transport, stored, expected) in [
            (
                Transport::default(),
                false,
                serde_json::json!(["tcp", "udp"]),
            ),
            (
                Transport::default(),
                true,
                serde_json::json!(["tcp", "udp"]),
            ),
            (Transport::tcp(), true, serde_json::json!(["tcp"])),
            (Transport::udp(), true, serde_json::json!(["udp"])),
        ] {
            let mut ports = AvailablePorts::new();
            let mut host = host();
            host.add_binding_range(&mut ports, 42000, 43000, 3, false, transport)
                .unwrap();
            let mut value = serde_json::to_value(&host).unwrap();
            let range = value["bindingRanges"]["42000"].as_object_mut().unwrap();
            range.insert("enabled".into(), false.into());
            if !stored {
                range.remove("transport");
            }
            let mut host = Model::<Host>::from(imbl_value::to_value(&value).unwrap());
            host.update_addresses(
                &ServerHostname::new(InternedString::intern("server")).unwrap(),
                &Default::default(),
                &ports,
            )
            .unwrap();
            let value = serde_json::to_value(&host).unwrap();
            assert_eq!(value["bindingRanges"]["42000"]["transport"], expected);
            assert_eq!(value["bindingRanges"]["42000"]["enabled"], false);
        }
    }

    #[test]
    fn single_port_addresses_are_offered_only_on_inbound_gateways() {
        let gateways = [
            (gw("wg-in"), wireguard(GatewayType::InboundOutbound)),
            (gw("wg-out"), wireguard(GatewayType::OutboundOnly)),
        ]
        .into_iter()
        .collect();
        let mut ports = AvailablePorts::new();
        let mut host = host();
        host.add_binding(&mut ports, 9735, plain(9735), false)
            .unwrap();

        host.update_addresses(
            &ServerHostname::new(InternedString::intern("server")).unwrap(),
            &gateways,
            &ports,
        )
        .unwrap();

        let host = host.de().unwrap();
        let available = &host.bindings[&9735].addresses.available;
        assert_eq!(
            available
                .iter()
                .filter(|address| matches!(
                    &address.metadata,
                    HostnameMetadata::Ipv4 { gateway } if gateway == &gw("wg-in")
                ))
                .count(),
            2
        );
        assert!(!available.iter().any(|address| matches!(
            &address.metadata,
            HostnameMetadata::Ipv4 { gateway } if gateway == &gw("wg-out")
        )));
    }

    fn mdns_rows(host: &Model<Host>, port: u16) -> Vec<BTreeSet<GatewayId>> {
        host.de().unwrap().bindings[&port]
            .addresses
            .available
            .iter()
            .filter_map(|address| match &address.metadata {
                HostnameMetadata::Mdns { gateways } => Some(gateways.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn mdns_address_outlives_its_gateways_addresses() {
        let server = ServerHostname::new(InternedString::intern("server")).unwrap();
        let mut eth0 = iface(NetworkInterfaceType::Ethernet, None);
        Arc::make_mut(eth0.ip_info.as_mut().unwrap()).subnets =
            ["192.0.2.10/24".parse::<IpNet>().unwrap()]
                .into_iter()
                .collect();
        let mut gateways: OrdMap<GatewayId, NetworkInterfaceInfo> =
            [(gw("eth0"), eth0)].into_iter().collect();
        let mut ports = AvailablePorts::new();
        let mut host = host();
        host.add_binding(&mut ports, 9735, plain(9735), false)
            .unwrap();
        host.update_addresses(&server, &gateways, &ports).unwrap();
        assert_eq!(mdns_rows(&host, 9735), [[gw("eth0")].into_iter().collect()]);

        gateways[&gw("eth0")].ip_info = None;
        host.update_addresses(&server, &gateways, &ports).unwrap();
        assert_eq!(mdns_rows(&host, 9735), [[gw("eth0")].into_iter().collect()]);
        let bind = &host.de().unwrap().bindings[&9735];
        assert!(!bind.addresses.available.iter().any(|a| a.metadata.is_ip()));
        assert!(
            bind.addresses
                .enabled()
                .iter()
                .any(|a| matches!(a.metadata, HostnameMetadata::Mdns { .. }))
        );

        gateways.remove(&gw("eth0"));
        host.update_addresses(&server, &gateways, &ports).unwrap();
        assert!(mdns_rows(&host, 9735).is_empty());
    }

    #[test]
    fn a_binding_turning_plain_publishes_nothing_without_public_bare_ips() {
        let server = ServerHostname::new(InternedString::intern("server")).unwrap();
        let mut eth0 = iface(NetworkInterfaceType::Ethernet, None);
        Arc::make_mut(eth0.ip_info.as_mut().unwrap()).subnets = [
            "192.0.2.10/24".parse::<IpNet>().unwrap(),
            "2001:db8::1/64".parse::<IpNet>().unwrap(),
        ]
        .into_iter()
        .collect();
        let gateways: OrdMap<GatewayId, NetworkInterfaceInfo> =
            [(gw("eth0"), eth0)].into_iter().collect();
        let mut ports = AvailablePorts::new();
        let mut host = host();
        let own_tls = BindOptions {
            secure: Some(crate::net::host::binding::Security { ssl: true }),
            ..plain(5223)
        };
        host.add_binding(&mut ports, 5223, own_tls, false).unwrap();
        host.as_public_domains_mut()
            .insert(
                &InternedString::intern("smp.example.com"),
                &PublicDomainConfig {
                    gateway: gw("eth0"),
                    acme: None,
                },
            )
            .unwrap();
        host.update_addresses(&server, &gateways, &ports).unwrap();
        host.as_bindings_mut()
            .mutate(|bindings| {
                bindings.get_mut(&5223).unwrap().addresses.disabled.clear();
                Ok(())
            })
            .unwrap();

        host.add_binding(&mut ports, 5223, plain(5223), false)
            .unwrap();
        host.update_addresses(&server, &gateways, &ports).unwrap();

        let addresses = host.de().unwrap().bindings[&5223].addresses.clone();
        assert!(addresses.gua_wan.is_empty());
        assert!(addresses.enabled().iter().all(|a| !a.public));
    }

    #[test]
    fn a_range_may_not_cover_a_port_the_host_binds() {
        let mut ports = AvailablePorts::new();
        let mut host = host();
        host.add_binding(&mut ports, 28332, plain(28332), false)
            .unwrap();

        let err = host
            .add_binding_range(&mut ports, 28331, 40000, 4, false, Default::default())
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidRequest);
        assert!(host.as_binding_ranges().de().unwrap().is_empty());
        assert!(ports.try_alloc(40000, false, false).is_some());
    }

    #[test]
    fn a_dormant_binding_does_not_block_the_range_that_supersedes_it() {
        let mut ports = AvailablePorts::new();
        let mut host = host();
        host.add_binding(&mut ports, 28332, plain(28332), false)
            .unwrap();
        host.add_binding(&mut ports, 28333, plain(28333), false)
            .unwrap();
        host.as_bindings_mut()
            .mutate(|b| Ok(b.values_mut().for_each(|i| i.disable())))
            .unwrap();

        host.add_binding_range(&mut ports, 28332, 40000, 2, false, Default::default())
            .unwrap();
        assert!(host.as_binding_ranges().de().unwrap().contains_key(&28332));
    }

    #[test]
    fn a_port_inside_an_enabled_range_may_not_be_bound_singly() {
        let mut ports = AvailablePorts::new();
        let mut host = host();
        host.add_binding_range(&mut ports, 42000, 42000, 500, false, Default::default())
            .unwrap();

        let err = host
            .add_binding(&mut ports, 42499, plain(9000), false)
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidRequest);
        host.add_binding(&mut ports, 42500, plain(9000), false)
            .unwrap();
    }

    #[test]
    fn reaffirming_a_range_and_binding_a_disjoint_port_both_succeed() {
        let mut ports = AvailablePorts::new();
        let mut host = host();
        host.add_binding(&mut ports, 3478, plain(3478), false)
            .unwrap();
        host.add_binding_range(&mut ports, 42000, 42000, 500, false, Default::default())
            .unwrap();
        host.add_binding_range(&mut ports, 42000, 42000, 500, false, Default::default())
            .unwrap();

        let err = host
            .add_binding_range(&mut ports, 42499, 50000, 2, false, Default::default())
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidRequest);
    }

    #[test]
    fn transport_only_rebind_preserves_range_allocation_and_address_choices() {
        use crate::net::transport::Transport;

        let mut ports = AvailablePorts::new();
        let mut host = host();
        host.add_binding_range(&mut ports, 42000, 43000, 10, false, Transport::default())
            .unwrap();
        host.as_binding_ranges_mut()
            .mutate(|ranges| {
                ranges
                    .get_mut(&42000)
                    .unwrap()
                    .addresses
                    .disabled
                    .insert(("relay.example.com".into(), 43000));
                Ok(())
            })
            .unwrap();
        let before = host
            .as_binding_ranges()
            .de()
            .unwrap()
            .remove(&42000)
            .unwrap();
        for transport in [Transport::tcp(), Transport::udp(), Transport::default()] {
            host.add_binding_range(&mut ports, 42000, 43000, 10, false, transport.clone())
                .unwrap();
            let after = host
                .as_binding_ranges()
                .de()
                .unwrap()
                .remove(&42000)
                .unwrap();
            assert_eq!(after.transport, transport);
            assert_eq!(after.external_start_port, before.external_start_port);
            assert_eq!(
                serde_json::to_value(&after.addresses).unwrap(),
                serde_json::to_value(&before.addresses).unwrap()
            );
            assert!(ports.try_alloc(43000, false, false).is_none());
        }
    }

    #[test]
    fn a_range_may_not_run_off_the_end_of_the_port_space() {
        let mut ports = AvailablePorts::new();
        let err = host()
            .add_binding_range(&mut ports, 65535, 40000, 2, false, Default::default())
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidRequest);
    }

    #[test]
    fn insecure_http_addresses_are_private_opt_ins_and_follow_gateway_defaults() {
        use crate::net::host::binding::AddSslOptions;
        let server = ServerHostname::new(InternedString::intern("server")).unwrap();
        let mut network = wireguard(GatewayType::InboundOutbound);
        network.dns_update.supported = Some(true);
        Arc::make_mut(network.ip_info.as_mut().unwrap())
            .subnets
            .insert("2001:db8::2/64".parse().unwrap());
        let mut gateways: OrdMap<_, _> = [(gw("wg0"), network)].into_iter().collect();
        let mut ports = AvailablePorts::new();
        let mut host = host();
        let mut options = plain(8096);
        options.secure = None;
        options.add_ssl = Some(AddSslOptions {
            preferred_external_port: 8443,
            alpn: None,
            upstream_cert_validation: None,
        });
        host.add_binding(&mut ports, 8096, options, false).unwrap();
        host.as_private_domains_mut()
            .insert(
                &InternedString::intern("media.lan"),
                &BTreeSet::from([gw("wg0")]),
            )
            .unwrap();
        host.as_public_domains_mut()
            .insert(
                &InternedString::intern("media.example.com"),
                &PublicDomainConfig {
                    gateway: gw("wg0"),
                    acme: None,
                },
            )
            .unwrap();
        host.update_addresses(&server, &gateways, &ports).unwrap();
        let initial = host.de().unwrap();
        let addresses = &initial.bindings[&8096].addresses;
        assert_eq!(addresses.available.iter().filter(|a| !a.ssl).count(), 4);
        assert!(
            addresses
                .available
                .iter()
                .filter(|a| !a.ssl)
                .all(|a| !a.public)
        );
        assert!(addresses.enabled().iter().all(|a| a.ssl));
        assert_eq!(addresses.insecure.len(), 4);

        gateways[&gw("wg0")].secure = Some(true);
        host.update_addresses(&server, &gateways, &ports).unwrap();
        let secure = host.de().unwrap();
        let addresses = &secure.bindings[&8096].addresses;
        assert!(addresses.insecure.is_empty());
        assert!(
            addresses
                .available
                .iter()
                .filter(|a| !a.ssl)
                .all(|a| addresses.enabled().contains(a))
        );

        gateways[&gw("wg0")].secure = Some(false);
        host.update_addresses(&server, &gateways, &ports).unwrap();
        assert!(
            host.de().unwrap().bindings[&8096]
                .addresses
                .enabled()
                .iter()
                .all(|a| a.ssl)
        );

        let mut trusted = wireguard(GatewayType::InboundOutbound);
        trusted.secure = Some(true);
        trusted.dns_update.supported = Some(true);
        Arc::make_mut(trusted.ip_info.as_mut().unwrap()).subnets =
            ["192.0.3.2/24".parse::<IpNet>().unwrap()]
                .into_iter()
                .collect();
        gateways.insert(gw("wg-trusted"), trusted);
        host.as_private_domains_mut()
            .mutate(|domains| {
                domains
                    .get_mut(&InternedString::intern("media.lan"))
                    .unwrap()
                    .insert(gw("wg-trusted"));
                Ok(())
            })
            .unwrap();
        host.update_addresses(&server, &gateways, &ports).unwrap();
        let snapshot = host.de().unwrap();
        let addresses = &snapshot.bindings[&8096].addresses;
        let insecure_domain = addresses.available.iter().find(|a| !a.ssl && matches!(&a.metadata, HostnameMetadata::PrivateDomain { gateways } if gateways == &BTreeSet::from([gw("wg0")]))).unwrap().clone();
        let trusted_names: BTreeSet<_> = addresses.available.iter().filter(|a| !a.ssl && matches!(&a.metadata, HostnameMetadata::PrivateDomain { gateways } | HostnameMetadata::Mdns { gateways } if gateways.contains(&gw("wg-trusted")))).cloned().collect();
        assert_eq!(trusted_names.len(), 2);
        assert!(
            trusted_names
                .iter()
                .all(|a| addresses.enabled().contains(a))
        );
        assert!(!addresses.enabled().contains(&insecure_domain));
        for on in [true, false] {
            host.as_bindings_mut()
                .mutate(|bindings| {
                    super::binding::set_address_enabled_with_consent(
                        &mut bindings.get_mut(&8096).unwrap().addresses,
                        &insecure_domain,
                        on,
                        on,
                    )
                })
                .unwrap();
            host.update_addresses(&server, &gateways, &ports).unwrap();
            let snapshot = host.de().unwrap();
            let addresses = &snapshot.bindings[&8096].addresses;
            assert_eq!(addresses.enabled().contains(&insecure_domain), on);
            assert!(
                trusted_names
                    .iter()
                    .all(|a| addresses.enabled().contains(a))
            );
            assert_eq!(
                addresses.effective,
                addresses.enabled().into_iter().cloned().collect()
            );
        }
    }
}
