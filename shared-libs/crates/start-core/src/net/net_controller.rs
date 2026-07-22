use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::{Arc, Weak};

use color_eyre::eyre::eyre;
use imbl_value::InternedString;
use ipnet::IpNet;
use nix::net::if_::if_nametoindex;
use patch_db::json_ptr::JsonPointer;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_rustls::rustls::ClientConfig as TlsClientConfig;
use tokio_rustls::rustls::crypto::CryptoProvider;
use tracing::instrument;

use crate::db::model::Database;
use crate::hostname::ServerHostname;
use crate::net::dns::DnsController;
use crate::net::dns_update::{DnsUpdateController, spawn_server_mdns_injection};
use crate::net::forward::{
    ForwardRequirements, InterfacePortForwardController, START9_BRIDGE_IFACE, nft_rule, nft_rule_v6,
};
use crate::net::gateway::NetworkInterfaceController;
use crate::net::host::binding::{AddSslOptions, BindId, BindOptions, UpstreamCertValidation};
use crate::net::host::{Host, Hosts, host_for};
use crate::net::port_map::{PortMapController, candidate_gateways};
use crate::net::service_interface::{
    AddressInfo, HostnameInfo, HostnameMetadata, ServiceInterface, ServiceInterfaceType,
};
use crate::net::socks::SocksController;
use crate::net::vhost::{AlpnInfo, DynVHostTarget, ProxyTarget, VHostController};
use crate::prelude::*;
use crate::service::effects::callbacks::ServiceCallbacks;
use crate::util::Invoke;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::serde::MaybeUtf8String;
use crate::util::sync::{SyncMutex, Watch};
use crate::{GatewayId, HOST_IP, HostId, Id, OptionExt, PackageId, ServiceInterfaceId};

pub struct NetController {
    pub(crate) db: TypedPatchDb<Database>,
    pub(super) vhost: VHostController,
    crypto_provider: Arc<CryptoProvider>,
    pub(super) tls_client_config: Arc<TlsClientConfig>,
    tls_client_config_no_verify: Arc<TlsClientConfig>,
    /// Cache of upstream client configs keyed by PEM, so a given
    /// `UpstreamCertValidation::Certificate` yields a stable `Arc` across
    /// rebuilds (keeps `ProxyTarget` equality from churning the vhost). Weak so
    /// entries drop once no live `ProxyTarget` holds the config; dead entries
    /// are pruned on insert.
    upstream_cert_configs: SyncMutex<BTreeMap<String, Weak<TlsClientConfig>>>,
    pub(crate) net_iface: Arc<NetworkInterfaceController>,
    pub(super) dns: DnsController,
    pub(super) dns_update: DnsUpdateController,
    /// Publishes the server's `<hostname>.local` name over WireGuard gateways.
    _mdns_injection: NonDetachingJoinHandle<()>,
    pub(super) forward: InterfacePortForwardController,
    pub(crate) port_map: PortMapController,
    pub(super) _socks: SocksController,
    pub(crate) callbacks: Arc<ServiceCallbacks>,
}

impl NetController {
    pub async fn init(
        db: TypedPatchDb<Database>,
        socks_listen: SocketAddr,
        max_proxy_conns_per_target: usize,
    ) -> Result<Self, Error> {
        let net_iface = Arc::new(NetworkInterfaceController::new(db.clone()));
        let socks = SocksController::new(socks_listen)?;
        let crypto_provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
        let tls_client_config = Arc::new(crate::net::tls::client_config(
            crypto_provider.clone(),
            [&*db
                .peek()
                .await
                .as_private()
                .as_key_store()
                .as_local_certs()
                .as_root_cert()
                .de()?
                .0],
        )?);
        let tls_client_config_no_verify = Arc::new(crate::net::tls::client_config_no_verify(
            crypto_provider.clone(),
        )?);
        nft_rule(
            "forward",
            "lxcbr0-egress",
            false,
            false,
            &format!("iifname \"{START9_BRIDGE_IFACE}\" ct state new accept"),
        )
        .await?;
        nft_rule_v6(
            "forward",
            "lxcbr0-egress",
            false,
            false,
            &format!("iifname \"{START9_BRIDGE_IFACE}\" ct state new accept"),
        )
        .await?;
        let peek = db.peek().await;
        let passthroughs = peek
            .as_public()
            .as_server_info()
            .as_network()
            .as_passthroughs()
            .de()?;
        let hostname = peek.as_public().as_server_info().as_hostname().de()?;
        drop(peek);
        let branding = crate::net::ssl::CertBranding::start_os(&hostname);
        // One PortMapController shared by the forward and vhost controllers so a
        // single query answers "is this port automatically forwarded?".
        let port_map = PortMapController::new(net_iface.watcher.subscribe());
        let dns_update = DnsUpdateController::new(
            net_iface.watcher.subscribe(),
            Arc::new(
                |gw: GatewayId| -> std::pin::Pin<
                    Box<dyn std::future::Future<Output = Result<Option<[u8; 32]>, ()>> + Send>,
                > {
                    Box::pin(async move {
                        crate::net::gateway::wireguard_psk(gw.as_str())
                            .await
                            .map_err(|_| ())
                    })
                },
            ),
        );
        let mdns_injection = spawn_server_mdns_injection(
            db.clone(),
            net_iface.watcher.subscribe(),
            dns_update.clone(),
        );
        Ok(Self {
            db: db.clone(),
            vhost: VHostController::new(
                db.clone(),
                net_iface.clone(),
                crypto_provider.clone(),
                branding,
                passthroughs,
                max_proxy_conns_per_target,
                port_map.clone(),
            ),
            crypto_provider,
            tls_client_config,
            tls_client_config_no_verify,
            upstream_cert_configs: SyncMutex::new(BTreeMap::new()),
            dns: DnsController::init(db, &net_iface.watcher).await?,
            dns_update,
            _mdns_injection: mdns_injection,
            forward: InterfacePortForwardController::new(
                net_iface.watcher.subscribe(),
                port_map.clone(),
            ),
            port_map,
            net_iface,
            _socks: socks,
            callbacks: Arc::new(ServiceCallbacks::default()),
        })
    }

    /// Client config for the OS→container TLS leg when rewrapping SSL. Falls
    /// back to validating against the root CA if a supplied certificate fails
    /// to parse (fail closed — the handshake then fails, surfacing the misconfig).
    fn upstream_client_config(
        &self,
        mode: &Option<UpstreamCertValidation>,
    ) -> Arc<TlsClientConfig> {
        match mode {
            None => self.tls_client_config.clone(),
            Some(UpstreamCertValidation::Disable) => self.tls_client_config_no_verify.clone(),
            Some(UpstreamCertValidation::Certificate(pem)) => {
                if let Some(cfg) = self
                    .upstream_cert_configs
                    .peek(|cache| cache.get(pem).and_then(Weak::upgrade))
                {
                    return cfg;
                }
                match crate::net::tls::client_config_with_cert(self.crypto_provider.clone(), pem) {
                    Ok(cfg) => {
                        let cfg = Arc::new(cfg);
                        self.upstream_cert_configs.mutate(|cache| {
                            cache.retain(|_, weak| weak.strong_count() > 0);
                            cache.insert(pem.clone(), Arc::downgrade(&cfg));
                        });
                        cfg
                    }
                    Err(e) => {
                        tracing::error!(
                            "Invalid upstream certificate for SSL rewrap, falling back to root CA validation: {e}"
                        );
                        tracing::debug!("{e:?}");
                        self.tls_client_config.clone()
                    }
                }
            }
        }
    }

    #[instrument(skip_all)]
    pub async fn create_service(
        self: &Arc<Self>,
        package: PackageId,
        ip: Ipv4Addr,
        ipv6: Option<Ipv6Addr>,
    ) -> Result<NetService, Error> {
        let dns = self.dns.add_service(Some(package.clone()), ip)?;

        let res = NetService::new(NetServiceData {
            id: Some(package),
            ip,
            ipv6,
            _dns: dns,
            controller: Arc::downgrade(self),
            binds: BTreeMap::new(),
        })?;
        res.clear_bindings(Default::default()).await?;
        Ok(res)
    }

    pub async fn os_bindings(self: &Arc<Self>) -> Result<NetService, Error> {
        let dns = self.dns.add_service(None, HOST_IP.into())?;

        let service = NetService::new(NetServiceData {
            id: None,
            ip: [127, 0, 0, 1].into(),
            ipv6: None,
            _dns: dns,
            controller: Arc::downgrade(self),
            binds: BTreeMap::new(),
        })?;
        service.clear_bindings(Default::default()).await?;
        service
            .bind(
                HostId::admin(),
                80,
                BindOptions {
                    preferred_external_port: 80,
                    add_ssl: Some(AddSslOptions {
                        preferred_external_port: 443,
                        add_x_forwarded_headers: false,
                        alpn: Some(AlpnInfo::Specified(vec![
                            MaybeUtf8String("h2".into()),
                            MaybeUtf8String("http/1.1".into()),
                        ])),
                        auth: None,
                        upstream_cert_validation: Default::default(),
                    }),
                    secure: None,
                },
            )
            .await?;

        // Sync the OS's own UI as a service interface (idempotent — the OS's
        // equivalent of a service's setupInterfaces) so the server binding
        // always has an exported interface. Bindings/ranges without one are
        // treated as internal-only below.
        self.db
            .mutate(|db| {
                let iface_id = ServiceInterfaceId::from(
                    Id::try_from("admin-ui".to_owned()).expect("valid id"),
                );
                let iface = ServiceInterface {
                    id: iface_id.clone(),
                    name: "StartOS UI".to_owned(),
                    description:
                        "The web user interface for your StartOS server, accessible from any browser."
                            .to_owned(),
                    masked: false,
                    address_info: AddressInfo {
                        username: None,
                        host_id: HostId::admin(),
                        internal_port: 80,
                        scheme: Some(InternedString::intern("http")),
                        ssl_scheme: Some(InternedString::intern("https")),
                        suffix: String::new(),
                    },
                    interface_type: ServiceInterfaceType::Ui,
                };
                db.as_public_mut()
                    .as_server_info_mut()
                    .as_network_mut()
                    .as_host_mut()
                    .as_bindings_mut()
                    .as_idx_mut(&80)
                    .or_not_found(80)?
                    .as_interfaces_mut()
                    .ser(&[(iface_id, iface)].into_iter().collect::<BTreeMap<_, _>>())
            })
            .await
            .result?;

        Ok(service)
    }
}

/// Public bare-IPv4 gateways for a binding's SSL `*` vhost: only SSL-port
/// addresses count — a bare IP enabled on the *plain* port must not mark the
/// SSL vhost public (that would request a pinhole for a port the operator
/// never exposed). Its v6 twin is scoped the same way (`a.ssl`).
fn ssl_vhost_public_v4<'a>(
    enabled_addresses: impl IntoIterator<Item = &'a HostnameInfo>,
) -> BTreeSet<GatewayId> {
    enabled_addresses
        .into_iter()
        .filter(|a| a.public && a.ssl && matches!(a.metadata, HostnameMetadata::Ipv4 { .. }))
        .flat_map(|a| a.metadata.gateways().cloned())
        .collect()
}

#[derive(Default, Debug)]
struct HostBinds {
    /// `(internal-target, count, requirements, rc)` keyed by external start
    /// port. `count == 1` is the single-port case; `count > 1` represents a
    /// contiguous range forward.
    forwards: BTreeMap<u16, (SocketAddrV4, u16, ForwardRequirements, Arc<()>)>,
    vhosts: BTreeMap<(Option<InternedString>, u16), (ProxyTarget, Arc<()>)>,
    private_dns: BTreeMap<InternedString, Arc<()>>,
    /// Non-SSL v6 forwards: `(host GUA, external port) -> (container v6, internal
    /// port, LAN source filter)`. A non-SSL GUA has no host terminator, so its
    /// port is DNAT'd to the container (see `forward6`); tracked so a stale
    /// forward is torn down when the GUA's exposure or target changes.
    gua_forwards: BTreeMap<(Ipv6Addr, u16), (Ipv6Addr, u16, Option<IpNet>)>,
}

pub struct NetServiceData {
    id: Option<PackageId>,
    ip: Ipv4Addr,
    /// The container's SLAAC ULA, DNAT target for a non-SSL GUA forward. `None`
    /// until the container has a v6 (or for the OS's own bindings).
    ipv6: Option<Ipv6Addr>,
    _dns: Arc<()>,
    controller: Weak<NetController>,
    binds: BTreeMap<HostId, HostBinds>,
}
impl NetServiceData {
    fn net_controller(&self) -> Result<Arc<NetController>, Error> {
        Weak::upgrade(&self.controller).ok_or_else(|| {
            Error::new(
                eyre!("NetController is shutdown"),
                crate::ErrorKind::Network,
            )
        })
    }

    async fn update(&mut self, ctrl: &NetController, id: HostId, host: Host) -> Result<(), Error> {
        let mut forwards: BTreeMap<u16, (SocketAddrV4, u16, ForwardRequirements)> = BTreeMap::new();
        let mut vhosts: BTreeMap<(Option<InternedString>, u16), ProxyTarget> = BTreeMap::new();
        let mut private_dns: BTreeMap<InternedString, BTreeSet<GatewayId>> = BTreeMap::new();
        // Non-SSL v6 DNAT forwards to the container — net_controller's concern (no
        // vhost owns them), but the forward controller opens their upstream pinhole
        // together with the DNAT (see the reconcile below).
        let mut gua_forwards: BTreeMap<(Ipv6Addr, u16), (Ipv6Addr, u16, Option<IpNet>)> =
            BTreeMap::new();
        let binds = self.binds.entry(id.clone()).or_default();

        let net_ifaces = ctrl.net_iface.watcher.ip_info();
        let host_addresses: Vec<_> = host.addresses().collect();

        // ── Build controller entries from enabled addresses ──
        for (port, bind) in host.bindings.iter() {
            if !bind.enabled {
                continue;
            }
            if bind.net.assigned_port.is_none() && bind.net.assigned_ssl_port.is_none() {
                continue;
            }
            // A binding with no exported interface is internal-only: it forwards
            // to lo / lxcbr0 but never to a gateway (see `enabled_addresses`).
            let enabled_addresses = bind.enabled_addresses();
            let addr: SocketAddr = (self.ip, *port).into();

            // Key private DNS by its live gateways so the resolver only answers
            // locally over those gateways — works even when also public (split DNS).
            for addr_info in &enabled_addresses {
                if let HostnameMetadata::PrivateDomain { gateways } = &addr_info.metadata {
                    let live: BTreeSet<GatewayId> = gateways
                        .iter()
                        .filter(|gw| {
                            net_ifaces
                                .get(*gw)
                                .map_or(false, |info| info.ip_info.is_some())
                        })
                        .cloned()
                        .collect();
                    if !live.is_empty() {
                        private_dns
                            .entry(addr_info.hostname.clone())
                            .or_default()
                            .extend(live);
                    }
                }
            }

            // SSL vhosts
            if let Some(ssl) = &bind.options.add_ssl {
                let connect_ssl: Result<Arc<TlsClientConfig>, AlpnInfo> =
                    if let Some(alpn) = ssl.alpn.clone() {
                        Err(alpn)
                    } else if bind.options.secure.as_ref().map_or(false, |s| s.ssl) {
                        Ok(ctrl.upstream_client_config(&ssl.upstream_cert_validation))
                    } else {
                        Err(AlpnInfo::Reflect)
                    };

                if let Some(assigned_ssl_port) = bind.net.assigned_ssl_port {
                    // Collect private IPs from enabled LAN-only addresses' gateways
                    // (a GUA set to LAN+WAN is WAN, so it lands in the public set).
                    let server_private_ips: BTreeSet<IpAddr> = enabled_addresses
                        .iter()
                        .filter(|a| !a.public)
                        .flat_map(|a| a.metadata.gateways())
                        .filter_map(|gw| net_ifaces.get(gw).and_then(|info| info.ip_info.as_ref()))
                        .flat_map(|ip_info| ip_info.subnets.iter().map(|s| s.addr()))
                        .collect();

                    // Public gateways, split by family: a bare public IPv4 (WAN IP)
                    // and a LAN+WAN GUA are independently toggleable, and the vhost
                    // must accept (and forward) each family only where that family
                    // is actually public — so a GUA-only-public gateway never
                    // accepts or forwards bare IPv4. The controller derives the IPv4
                    // `*` pinhole from `public_v4`; the GUA needs no NAT.
                    let server_public_v4 = ssl_vhost_public_v4(enabled_addresses.iter().copied());
                    // Per-IP (per-GUA), not per-gateway: one gateway can carry
                    // several GUAs that are independently Local vs Public, so only
                    // the specific enabled-public GUAs accept WAN. Scoped to the SSL
                    // exposure (`a.ssl`), since this `*` vhost terminates TLS on the
                    // ssl port — a GUA public only on a non-SSL port is served by the
                    // v6 DNAT forward, not this vhost.
                    let server_public_v6: BTreeSet<Ipv6Addr> = enabled_addresses
                        .iter()
                        .filter(|a| a.public && a.ssl)
                        .filter_map(|a| a.gua().map(|g| *g.ip()))
                        .collect();

                    // * vhost (on assigned_ssl_port)
                    if !server_private_ips.is_empty()
                        || !server_public_v4.is_empty()
                        || !server_public_v6.is_empty()
                    {
                        vhosts.insert(
                            (None, assigned_ssl_port),
                            ProxyTarget {
                                public_v4: server_public_v4,
                                public_v6: server_public_v6,
                                private: server_private_ips.clone(),
                                acme: None,
                                addr,
                                add_x_forwarded_headers: ssl.add_x_forwarded_headers,
                                auth: ssl.auth.clone(),
                                connect_ssl: connect_ssl.clone(),
                                passthrough: false,
                                preserve_source_ip: false,
                            },
                        );
                    }
                }

                // Domain vhosts: group by (domain, ssl_port), merge public/private sets
                for addr_info in &enabled_addresses {
                    if !addr_info.ssl {
                        continue;
                    }
                    match &addr_info.metadata {
                        HostnameMetadata::PublicDomain { .. }
                        | HostnameMetadata::PrivateDomain { .. } => {}
                        _ => continue,
                    }
                    let domain = &addr_info.hostname;
                    let Some(domain_ssl_port) = addr_info.port else {
                        continue;
                    };
                    let key = (Some(domain.clone()), domain_ssl_port);
                    let target = vhosts.entry(key).or_insert_with(|| ProxyTarget {
                        public_v4: BTreeSet::new(),
                        public_v6: BTreeSet::new(),
                        private: BTreeSet::new(),
                        acme: host_addresses
                            .iter()
                            .find(|a| a.address == *domain)
                            .and_then(|a| a.public.as_ref())
                            .and_then(|p| p.acme.clone()),
                        addr,
                        add_x_forwarded_headers: ssl.add_x_forwarded_headers,
                        auth: ssl.auth.clone(),
                        connect_ssl: connect_ssl.clone(),
                        passthrough: false,
                        preserve_source_ip: false,
                    });
                    if addr_info.public {
                        // A public domain is dual-stack (A + AAAA): public on its
                        // gateways' bare IPv4 and on each of their GUAs.
                        let gws: BTreeSet<GatewayId> =
                            addr_info.metadata.gateways().cloned().collect();
                        target.public_v4.extend(gws.iter().cloned());
                        target
                            .public_v6
                            .extend(crate::net::utils::gua_ips(&net_ifaces, &gws));
                    } else {
                        for gw in addr_info.metadata.gateways() {
                            if let Some(info) = net_ifaces.get(gw) {
                                if let Some(ip_info) = &info.ip_info {
                                    for subnet in &ip_info.subnets {
                                        target.private.insert(subnet.addr());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Non-SSL forwards
            if bind
                .options
                .secure
                .map_or(true, |s| !(s.ssl && bind.options.add_ssl.is_some()))
            {
                let external = bind.net.assigned_port.or_not_found("assigned lan port")?;
                // Only addresses at this port drive its forward (ssl-port entries are the vhost's).
                let fwd_public: BTreeSet<GatewayId> = enabled_addresses
                    .iter()
                    .filter(|a| a.public && a.port == Some(external))
                    .flat_map(|a| a.metadata.gateways())
                    .cloned()
                    .collect();
                // Declare which address makes each gateway public, so a stray
                // auto-port-map can be traced back to the exposure driving it.
                for a in enabled_addresses
                    .iter()
                    .filter(|a| a.public && a.port == Some(external))
                {
                    tracing::debug!(
                        "port {external}: WAN address {} (ip={}) on gateway(s) {:?}",
                        a.hostname,
                        a.metadata.is_ip(),
                        a.metadata.gateways().collect::<Vec<_>>(),
                    );
                }
                let fwd_private: BTreeSet<IpAddr> = enabled_addresses
                    .iter()
                    .filter(|a| !a.public && a.port == Some(external))
                    .flat_map(|a| a.metadata.gateways())
                    .filter_map(|gw| net_ifaces.get(gw).and_then(|i| i.ip_info.as_ref()))
                    .flat_map(|ip| ip.subnets.iter().map(|s| s.addr()))
                    .collect();
                forwards.insert(
                    external,
                    (
                        SocketAddrV4::new(self.ip, *port),
                        1,
                        ForwardRequirements {
                            public_gateways: fwd_public,
                            private_ips: fwd_private,
                            secure: bind.options.secure.is_some(),
                        },
                    ),
                );

                // Non-SSL GUAs have no host terminator, so DNAT the host's
                // GUA:external to the container's v6:internal. A LAN-only GUA is
                // source-restricted to its on-link subnet; a LAN+WAN GUA is
                // unrestricted. Fail closed: skip a LAN-only GUA whose subnet we
                // can't determine rather than expose it unrestricted.
                if let Some(container_v6) = self.ipv6 {
                    for a in enabled_addresses.iter() {
                        // SSL-port GUAs are the vhost listener's; DNAT only the non-SSL port.
                        let Some(gua) = a.gua().filter(|g| g.port() == external) else {
                            continue;
                        };
                        // Secure when StartOS terminates TLS (add_ssl → a.ssl) or the
                        // underlying protocol is itself secure. The WAN is never secure, so an
                        // insecure exposure that requested public serves the LAN instead.
                        let secure_exposure = a.ssl || bind.options.secure.is_some();
                        let src_filter = if a.public && secure_exposure {
                            None
                        } else {
                            // LAN: insecure reaches it only over a secure gateway (IPv4 parity).
                            if !secure_exposure
                                && !a
                                    .metadata
                                    .gateways()
                                    .filter_map(|gw| net_ifaces.get(gw))
                                    .any(|info| info.secure())
                            {
                                continue;
                            }
                            match a
                                .metadata
                                .gateways()
                                .filter_map(|gw| net_ifaces.get(gw))
                                .filter_map(|info| info.ip_info.as_ref())
                                .flat_map(|ip| ip.subnets.iter())
                                .find(|s| s.contains(&IpAddr::V6(*gua.ip())))
                                .copied()
                            {
                                Some(subnet) => Some(subnet),
                                None => continue,
                            }
                        };
                        // A public bare GUA is DNAT'd to the container, but no vhost
                        // owns it (add_ssl builds the `*` vhost; passthroughs are
                        // domain-only). The forward controller opens its upstream
                        // pinhole together with the DNAT (see the reconcile below).
                        gua_forwards
                            .insert((*gua.ip(), gua.port()), (container_v6, *port, src_filter));
                    }
                }
            }

            // Passthrough vhosts: if the service handles its own TLS
            // (secure.ssl && no add_ssl) and a domain address is enabled on
            // an SSL port different from assigned_port, add a passthrough
            // vhost so the service's TLS endpoint is reachable on that port.
            if bind.options.secure.map_or(false, |s| s.ssl) && bind.options.add_ssl.is_none() {
                let assigned = bind.net.assigned_port;
                for addr_info in &enabled_addresses {
                    if !addr_info.ssl {
                        continue;
                    }
                    let Some(pt_port) = addr_info.port.filter(|p| assigned != Some(*p)) else {
                        continue;
                    };
                    match &addr_info.metadata {
                        HostnameMetadata::PublicDomain { .. }
                        | HostnameMetadata::PrivateDomain { .. } => {}
                        _ => continue,
                    }
                    let domain = &addr_info.hostname;
                    let key = (Some(domain.clone()), pt_port);
                    let target = vhosts.entry(key).or_insert_with(|| ProxyTarget {
                        public_v4: BTreeSet::new(),
                        public_v6: BTreeSet::new(),
                        private: BTreeSet::new(),
                        acme: None,
                        addr,
                        add_x_forwarded_headers: false,
                        auth: None,
                        connect_ssl: Err(AlpnInfo::Reflect),
                        passthrough: true,
                        // Container handles its own TLS and the box is its
                        // gateway, so preserve the client source IP.
                        preserve_source_ip: true,
                    });
                    if addr_info.public {
                        // Passthrough domain is dual-stack, like the SSL domain vhost.
                        let gws: BTreeSet<GatewayId> =
                            addr_info.metadata.gateways().cloned().collect();
                        target.public_v4.extend(gws.iter().cloned());
                        target
                            .public_v6
                            .extend(crate::net::utils::gua_ips(&net_ifaces, &gws));
                    } else {
                        for gw in addr_info.metadata.gateways() {
                            if let Some(info) = net_ifaces.get(gw) {
                                if let Some(ip_info) = &info.ip_info {
                                    for subnet in &ip_info.subnets {
                                        target.private.insert(subnet.addr());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Port-range bindings: forward each enabled range the same way as a
        // single-port non-SSL binding — `private_ips` from enabled private
        // (LAN) addresses, `public_gateways` from enabled public (WAN)
        // addresses — but with `count = number_of_ports`. Outbound-only
        // gateways never get range addresses synthesized, so they never appear
        // here. Private domains register their resolver entries like single-port.
        //
        // `secure: true` is intentional: ranges carry no Security option (no
        // SSL/vhost), so without it the forward.rs security gate
        // (`!reqs.secure && !info.secure()`) would drop every range on a normal
        // (non-secure) WAN gateway, since no gateway is ever marked secure.
        for (&internal_start, range) in host.binding_ranges.iter() {
            if !range.enabled {
                continue;
            }
            // A range with no exported interface is internal-only: it forwards
            // to lo / lxcbr0 but never to a gateway (see `enabled_addresses`).
            let enabled_addresses = range.enabled_addresses();

            for addr_info in &enabled_addresses {
                if let HostnameMetadata::PrivateDomain { gateways } = &addr_info.metadata {
                    let live: BTreeSet<GatewayId> = gateways
                        .iter()
                        .filter(|gw| {
                            net_ifaces
                                .get(*gw)
                                .map_or(false, |info| info.ip_info.is_some())
                        })
                        .cloned()
                        .collect();
                    if !live.is_empty() {
                        private_dns
                            .entry(addr_info.hostname.clone())
                            .or_default()
                            .extend(live);
                    }
                }
            }

            let public_gateways: BTreeSet<GatewayId> = enabled_addresses
                .iter()
                .filter(|a| a.public)
                .flat_map(|a| a.metadata.gateways())
                .cloned()
                .collect();
            let private_ips: BTreeSet<IpAddr> = enabled_addresses
                .iter()
                .filter(|a| !a.public)
                .flat_map(|a| a.metadata.gateways())
                .filter_map(|gw| net_ifaces.get(gw).and_then(|i| i.ip_info.as_ref()))
                .flat_map(|ip| ip.subnets.iter().map(|s| s.addr()))
                .collect();
            if public_gateways.is_empty() && private_ips.is_empty() {
                continue;
            }
            forwards.insert(
                range.external_start_port,
                (
                    SocketAddrV4::new(self.ip, internal_start),
                    range.number_of_ports,
                    ForwardRequirements {
                        public_gateways,
                        private_ips,
                        secure: true,
                    },
                ),
            );
        }

        // The vhost controller owns every upstream port map for its ports — the
        // IPv6 GUA pinholes (bare `*` and public-domain vhosts) and their v6 80->443
        // redirect included — deriving them from each ProxyTarget's `public_v6`. The
        // non-vhost DNAT'd GUA pinholes are owned by the forward controller (with the
        // DNAT). So net_controller itself requests no port maps. The IPv4 gateway
        // serves its own port-80 HTTP->HTTPS redirect.

        // Reconcile non-SSL v6 forwards: tear down any that changed or went away,
        // then install new/changed ones. The forward controller owns each forward's
        // DNAT *and* its upstream pinhole (WAN forwards only), just like the v4 path.
        // Best-effort — a nft failure on one forward is logged, not fatal.
        for (key, spec) in &binds.gua_forwards {
            if gua_forwards.get(key) != Some(spec) {
                let &(gua, ext) = key;
                let &(tgt, int, ref src) = spec;
                if let Err(e) = ctrl
                    .forward
                    .unforward6(
                        SocketAddrV6::new(gua, ext, 0, 0),
                        SocketAddrV6::new(tgt, int, 0, 0),
                        64,
                        src.clone(),
                    )
                    .await
                {
                    tracing::error!("failed to remove v6 forward [{gua}]:{ext}: {e}");
                    tracing::debug!("{e:?}");
                }
            }
        }
        for (key, spec) in &gua_forwards {
            if binds.gua_forwards.get(key) != Some(spec) {
                let &(gua, ext) = key;
                let &(tgt, int, ref src) = spec;
                // PCP candidates for the pinhole (only used for a WAN forward).
                let gateways: Vec<(IpAddr, Option<u32>)> = if src.is_none() {
                    net_ifaces
                        .iter()
                        .map(|(_, i)| i)
                        .find(|info| {
                            info.ip_info.as_ref().map_or(false, |i| {
                                i.subnets.iter().any(|s| s.addr() == IpAddr::V6(gua))
                            })
                        })
                        .map(|info| {
                            candidate_gateways(info)
                                .into_iter()
                                .filter(|(g, _)| g.is_ipv6())
                                .collect()
                        })
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                if let Err(e) = ctrl
                    .forward
                    .forward6(
                        SocketAddrV6::new(gua, ext, 0, 0),
                        SocketAddrV6::new(tgt, int, 0, 0),
                        64,
                        src.clone(),
                        gateways,
                    )
                    .await
                {
                    tracing::error!("failed to add v6 forward [{gua}]:{ext} -> [{tgt}]:{int}: {e}");
                    tracing::debug!("{e:?}");
                }
            }
        }
        binds.gua_forwards = gua_forwards;

        // ── Phase 3: Reconcile ──
        let all = binds
            .forwards
            .keys()
            .chain(forwards.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        for external in all {
            let mut prev = binds.forwards.remove(&external);
            if let Some((internal, count, reqs)) = forwards.remove(&external) {
                prev = prev.filter(|(i, c, r, _)| i == &internal && *c == count && *r == reqs);
                binds.forwards.insert(
                    external,
                    if let Some(prev) = prev {
                        prev
                    } else {
                        (
                            internal,
                            count,
                            reqs.clone(),
                            ctrl.forward
                                .add_range(
                                    external,
                                    count,
                                    reqs,
                                    internal,
                                    net_ifaces
                                        .iter()
                                        .find_map(|(_, i)| {
                                            i.ip_info.as_ref().and_then(|i| {
                                                i.subnets.iter().find(|i| {
                                                    i.contains(&IpAddr::from(*internal.ip()))
                                                })
                                            })
                                        })
                                        .map(|s| s.prefix_len())
                                        .unwrap_or(32),
                                )
                                .await?,
                        )
                    },
                );
            }
        }
        ctrl.forward.gc().await?;

        // The vhost controller owns every upstream IPv4 port map for its ports —
        // PCP HOSTNAME for a public domain, a plain pinhole for a bare public IPv4
        // (`*` vhost) — deriving them from the vhost targets themselves (their
        // per-family `public_v4`). Called on the complete `vhosts` before the drain
        // loop below consumes it, and on every update (even with none) so a host
        // that dropped its exposure withdraws its prior mappings.
        ctrl.vhost
            .reconcile_port_maps((self.id.clone(), id.clone()), &vhosts);

        let all = binds
            .vhosts
            .keys()
            .chain(vhosts.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        for key in all {
            let mut prev = binds.vhosts.remove(&key);
            if let Some(target) = vhosts.remove(&key) {
                prev = prev.filter(|(t, _)| t == &target);
                binds.vhosts.insert(
                    key.clone(),
                    if let Some(prev) = prev {
                        prev
                    } else {
                        (
                            target.clone(),
                            ctrl.vhost.add(key.0, key.1, DynVHostTarget::new(target))?,
                        )
                    },
                );
            } else {
                if let Some((_, rc)) = prev {
                    drop(rc);
                    ctrl.vhost.gc(key.0, key.1);
                }
            }
        }

        let mut rm = BTreeSet::new();
        binds.private_dns.retain(|fqdn, _| {
            if private_dns.contains_key(fqdn) {
                true
            } else {
                rm.insert(fqdn.clone());
                false
            }
        });
        for (fqdn, gateways) in private_dns {
            // Best-effort: also publish the record to the gateway's own DNS via
            // RFC 2136 so LAN devices not using StartOS's resolver can resolve it.
            ctrl.dns_update.add(fqdn.clone(), gateways.clone());
            binds
                .private_dns
                .insert(fqdn.clone(), ctrl.dns.add_private_domain(fqdn, gateways)?);
        }
        ctrl.dns.gc_private_domains(&rm)?;
        ctrl.dns_update.gc(rm);

        Ok(())
    }
}

pub struct NetService {
    shutdown: bool,
    data: Arc<Mutex<NetServiceData>>,
    sync_task: JoinHandle<()>,
    synced: Watch<u64>,
}
impl NetService {
    pub(crate) fn dummy() -> Self {
        Self {
            shutdown: true,
            data: Arc::new(Mutex::new(NetServiceData {
                id: None,
                ip: Ipv4Addr::new(0, 0, 0, 0),
                ipv6: None,
                _dns: Default::default(),
                controller: Default::default(),
                binds: BTreeMap::new(),
            })),
            sync_task: tokio::spawn(futures::future::ready(())),
            synced: Watch::new(0u64),
        }
    }

    fn new(data: NetServiceData) -> Result<Self, Error> {
        let ctrl = data.net_controller()?;
        let pkg_id = data.id.clone();
        let db = ctrl.db.clone();
        drop(ctrl);

        let synced = Watch::new(0u64);
        let synced_writer = synced.clone();

        let ip = data.ip;
        let data = Arc::new(Mutex::new(data));
        let thread_data = data.clone();
        let sync_task = tokio::spawn(async move {
            if let Some(ref id) = pkg_id {
                let ptr: JsonPointer = format!("/public/packageData/{}/hosts", id).parse().unwrap();
                let mut watch = db.watch(ptr).await.typed::<Hosts>();

                // Outbound gateway enforcement
                let service_ip = ip.to_string();
                // Purge any stale rules from a previous instance
                loop {
                    if Command::new("ip")
                        .arg("rule")
                        .arg("del")
                        .arg("from")
                        .arg(&service_ip)
                        .arg("priority")
                        .arg("100")
                        .invoke(ErrorKind::Network)
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                let mut outbound_sub = db
                    .subscribe(
                        format!("/public/packageData/{}/outboundGateway", id)
                            .parse::<JsonPointer<_, _>>()
                            .unwrap(),
                    )
                    .await;
                let ctrl_for_ip = thread_data.lock().await.net_controller().ok();
                let mut ip_info_watch = ctrl_for_ip
                    .as_ref()
                    .map(|c| c.net_iface.watcher.subscribe());
                if let Some(ref mut w) = ip_info_watch {
                    w.mark_seen();
                }
                drop(ctrl_for_ip);
                let mut current_outbound_table: Option<u32> = None;

                loop {
                    let (hosts_changed, outbound_changed) = tokio::select! {
                        res = watch.changed() => {
                            if let Err(e) = res {
                                tracing::error!("DB watch disconnected for {id}: {e}");
                                break;
                            }
                            (true, false)
                        }
                        _ = outbound_sub.recv() => (false, true),
                        _ = async {
                            if let Some(ref mut w) = ip_info_watch {
                                w.changed().await;
                            } else {
                                std::future::pending::<()>().await;
                            }
                        } => (false, true),
                    };

                    // Handle host updates
                    if hosts_changed {
                        if let Err(e) = async {
                            let hosts = watch.peek()?.de().unwrap_or_default();
                            let mut data = thread_data.lock().await;
                            let ctrl = data.net_controller()?;
                            for (host_id, host) in hosts.0 {
                                data.update(&*ctrl, host_id, host).await?;
                            }
                            Ok::<_, Error>(())
                        }
                        .await
                        {
                            tracing::error!("Failed to update network info for {id}: {e}");
                            tracing::debug!("{e:?}");
                        }
                    }

                    // Handle outbound gateway changes
                    if outbound_changed {
                        if let Err(e) = async {
                            // Remove old rule if any
                            if let Some(old_table) = current_outbound_table.take() {
                                let old_table_str = old_table.to_string();
                                let _ = Command::new("ip")
                                    .arg("rule")
                                    .arg("del")
                                    .arg("from")
                                    .arg(&service_ip)
                                    .arg("lookup")
                                    .arg(&old_table_str)
                                    .arg("priority")
                                    .arg("100")
                                    .invoke(ErrorKind::Network)
                                    .await;
                            }
                            // Read current outbound gateway from DB
                            let outbound_gw: Option<GatewayId> = db
                                .peek()
                                .await
                                .as_public()
                                .as_package_data()
                                .as_idx(id)
                                .map(|p| p.as_outbound_gateway().de().ok())
                                .flatten()
                                .flatten();
                            if let Some(gw_id) = outbound_gw {
                                // Look up table ID for this gateway
                                if let Some(table_id) = if_nametoindex(gw_id.as_str())
                                    .map(|idx| 1000 + idx)
                                    .log_err()
                                {
                                    let table_str = table_id.to_string();
                                    Command::new("ip")
                                        .arg("rule")
                                        .arg("add")
                                        .arg("from")
                                        .arg(&service_ip)
                                        .arg("lookup")
                                        .arg(&table_str)
                                        .arg("priority")
                                        .arg("100")
                                        .invoke(ErrorKind::Network)
                                        .await
                                        .log_err();
                                    current_outbound_table = Some(table_id);
                                }
                            }
                            Ok::<_, Error>(())
                        }
                        .await
                        {
                            tracing::error!("Failed to update outbound gateway for {id}: {e}");
                            tracing::debug!("{e:?}");
                        }
                    }

                    synced_writer.send_modify(|v| *v += 1);
                }

                // Cleanup outbound rule on task exit
                if let Some(table_id) = current_outbound_table {
                    let table_str = table_id.to_string();
                    let _ = Command::new("ip")
                        .arg("rule")
                        .arg("del")
                        .arg("from")
                        .arg(&service_ip)
                        .arg("lookup")
                        .arg(&table_str)
                        .arg("priority")
                        .arg("100")
                        .invoke(ErrorKind::Network)
                        .await;
                }
            } else {
                let ptr: JsonPointer = "/public/serverInfo/network/host".parse().unwrap();
                let mut watch = db.watch(ptr).await.typed::<Host>();
                loop {
                    if let Err(e) = watch.changed().await {
                        tracing::error!("DB watch disconnected for Main UI: {e}");
                        break;
                    }
                    if let Err(e) = async {
                        let host = watch.peek()?.de()?;
                        let mut data = thread_data.lock().await;
                        let ctrl = data.net_controller()?;
                        data.update(&*ctrl, HostId::admin(), host).await?;
                        Ok::<_, Error>(())
                    }
                    .await
                    {
                        tracing::error!("Failed to update network info for Main UI: {e}");
                        tracing::debug!("{e:?}");
                    }
                    synced_writer.send_modify(|v| *v += 1);
                }
            }
        });

        Ok(Self {
            shutdown: false,
            data,
            sync_task,
            synced,
        })
    }

    pub async fn bind(
        &self,
        id: HostId,
        internal_port: u16,
        options: BindOptions,
    ) -> Result<(), Error> {
        let (ctrl, pkg_id) = {
            let data = self.data.lock().await;
            (data.net_controller()?, data.id.clone())
        };
        ctrl.db
            .mutate(|db| {
                let gateways = db
                    .as_public()
                    .as_server_info()
                    .as_network()
                    .as_gateways()
                    .de()?;
                let hostname = ServerHostname::load(db.as_public().as_server_info())?;
                let mut ports = db.as_private().as_available_ports().de()?;
                let host = host_for(db, pkg_id.as_ref().unwrap_or(&PackageId::start_os()), &id)?;
                let is_new = !host.as_bindings().contains_key(&internal_port)?;
                host.add_binding(&mut ports, internal_port, options)?;
                host.update_addresses(&hostname, &gateways, &ports)?;
                // Isolate a newly-bound binding from any pre-existing public
                // domain on this host, then re-derive so port forwards match.
                if is_new {
                    host.reconcile_public_domains_on_new_binding(internal_port)?;
                    host.update_addresses(&hostname, &gateways, &ports)?;
                }
                db.as_private_mut().as_available_ports_mut().ser(&ports)?;
                Ok(())
            })
            .await
            .result
    }

    pub async fn bind_range(
        &self,
        id: HostId,
        internal_start_port: u16,
        external_start_port: u16,
        number_of_ports: u16,
    ) -> Result<(), Error> {
        let (ctrl, pkg_id) = {
            let data = self.data.lock().await;
            (data.net_controller()?, data.id.clone())
        };
        ctrl.db
            .mutate(|db| {
                let gateways = db
                    .as_public()
                    .as_server_info()
                    .as_network()
                    .as_gateways()
                    .de()?;
                let hostname = ServerHostname::load(db.as_public().as_server_info())?;
                let mut ports = db.as_private().as_available_ports().de()?;
                let host = host_for(db, pkg_id.as_ref().unwrap_or(&PackageId::start_os()), &id)?;
                let is_new = !host
                    .as_binding_ranges()
                    .contains_key(&internal_start_port)?;
                host.add_binding_range(
                    &mut ports,
                    internal_start_port,
                    external_start_port,
                    number_of_ports,
                )?;
                host.update_addresses(&hostname, &gateways, &ports)?;
                // Isolate a newly-bound range from any pre-existing public domain
                // on this host, then re-derive so port forwards match.
                if is_new {
                    host.reconcile_public_domains_on_new_range(internal_start_port)?;
                    host.update_addresses(&hostname, &gateways, &ports)?;
                }
                db.as_private_mut().as_available_ports_mut().ser(&ports)?;
                Ok(())
            })
            .await
            .result
    }

    /// Returns `true` if the mutate actually changed something (a non-empty
    /// patch). A no-op clears nothing, so the caller can skip waiting on the
    /// sync-task for a change that will never arrive.
    pub async fn clear_bindings(&self, except: BTreeSet<BindId>) -> Result<bool, Error> {
        let (ctrl, pkg_id) = {
            let data = self.data.lock().await;
            (data.net_controller()?, data.id.clone())
        };
        let rev = ctrl
            .db
            .mutate(|db| {
                let gateways = db
                    .as_public()
                    .as_server_info()
                    .as_network()
                    .as_gateways()
                    .de()?;
                let hostname = ServerHostname::load(db.as_public().as_server_info())?;
                let ports = db.as_private().as_available_ports().de()?;
                if let Some(ref pkg_id) = pkg_id {
                    for (host_id, host) in db
                        .as_public_mut()
                        .as_package_data_mut()
                        .as_idx_mut(pkg_id)
                        .or_not_found(pkg_id)?
                        .as_hosts_mut()
                        .as_entries_mut()?
                    {
                        host.as_bindings_mut().mutate(|b| {
                            for (internal_port, info) in b.iter_mut() {
                                if !except.contains(&BindId {
                                    id: host_id.clone(),
                                    internal_port: *internal_port,
                                }) {
                                    info.disable();
                                }
                            }
                            Ok(())
                        })?;
                        host.as_binding_ranges_mut().mutate(|r| {
                            for (internal_port, info) in r.iter_mut() {
                                if !except.contains(&BindId {
                                    id: host_id.clone(),
                                    internal_port: *internal_port,
                                }) {
                                    info.disable();
                                }
                            }
                            Ok(())
                        })?;
                        host.update_addresses(&hostname, &gateways, &ports)?;
                    }
                } else {
                    let host = db
                        .as_public_mut()
                        .as_server_info_mut()
                        .as_network_mut()
                        .as_host_mut();
                    host.as_bindings_mut().mutate(|b| {
                        for (internal_port, info) in b.iter_mut() {
                            if !except.contains(&BindId {
                                id: HostId::admin(),
                                internal_port: *internal_port,
                            }) {
                                info.disable();
                            }
                        }
                        Ok(())
                    })?;
                    host.as_binding_ranges_mut().mutate(|r| {
                        for (internal_port, info) in r.iter_mut() {
                            if !except.contains(&BindId {
                                id: HostId::admin(),
                                internal_port: *internal_port,
                            }) {
                                info.disable();
                            }
                        }
                        Ok(())
                    })?;
                    host.update_addresses(&hostname, &gateways, &ports)?;
                }
                Ok(())
            })
            .await;
        rev.result.map(|_| rev.revision.is_some())
    }

    pub async fn remove_all(mut self) -> Result<(), Error> {
        if Weak::upgrade(&self.data.lock().await.controller).is_none() {
            self.shutdown = true;
            tracing::warn!("NetService dropped after NetController is shutdown");
            return Err(Error::new(
                eyre!("NetController is shutdown"),
                crate::ErrorKind::Network,
            ));
        }
        let current = self.synced.peek(|v| *v);
        // Only wait for the sync-task to apply the teardown if clear_bindings
        // actually changed something. A no-op mutate produces no patch, so the
        // sync-task's `hosts` watch never fires and `synced` never advances —
        // without this guard the wait below blocks forever (its `sync_task` arm
        // only rescues us when the task is already dead, not alive-but-idle).
        if self.clear_bindings(Default::default()).await? {
            let mut w = self.synced.clone();
            tokio::select! {
                _ = w.wait_for(|v| *v > current) => {}
                // sync-task already dead (e.g. aborted by a prior remove_all):
                // `synced` will never advance again, so don't block on it.
                _ = &mut self.sync_task => {}
            }
        }
        self.sync_task.abort();
        // Clean up any outbound gateway ip rules for this service
        let service_ip = self.data.lock().await.ip.to_string();
        loop {
            if Command::new("ip")
                .arg("rule")
                .arg("del")
                .arg("from")
                .arg(&service_ip)
                .arg("priority")
                .arg("100")
                .invoke(ErrorKind::Network)
                .await
                .is_err()
            {
                break;
            }
        }
        // Set last: an earlier failure leaves shutdown false so Drop's fallback re-runs.
        self.shutdown = true;
        Ok(())
    }

    pub async fn get_ip(&self) -> Ipv4Addr {
        self.data.lock().await.ip
    }
}

impl Drop for NetService {
    fn drop(&mut self) {
        if !self.shutdown {
            self.shutdown = true;
            let svc = std::mem::replace(self, Self::dummy());
            tokio::spawn(async move { svc.remove_all().await.log_err() });
        }
    }
}

#[cfg(test)]
mod tests {
    use imbl_value::InternedString;

    use super::*;

    fn bare_v4(ssl: bool, port: u16, gateway: &GatewayId) -> HostnameInfo {
        HostnameInfo {
            ssl,
            public: true,
            hostname: InternedString::intern("203.0.113.10"),
            port: Some(port),
            metadata: HostnameMetadata::Ipv4 {
                gateway: gateway.clone(),
            },
        }
    }

    // Regression for the coturn/`—`-fallback bug: enabling the bare WAN IP on a
    // binding's *plain* port marked the SSL `*` vhost public, requesting a bare
    // pinhole for the SSL port the operator never enabled. Only SSL-port
    // addresses may make the SSL vhost public.
    #[test]
    fn plain_port_bare_ip_does_not_publicize_ssl_vhost() {
        let wg = GatewayId::from(InternedString::intern("wg0"));
        let addrs = [bare_v4(false, 57551, &wg)];
        assert!(
            ssl_vhost_public_v4(addrs.iter()).is_empty(),
            "a plain-port bare IP must not mark the SSL vhost public"
        );

        let addrs = [bare_v4(false, 57551, &wg), bare_v4(true, 5349, &wg)];
        assert_eq!(
            ssl_vhost_public_v4(addrs.iter()),
            BTreeSet::from([wg.clone()]),
            "an SSL-port bare IP does mark it public"
        );
    }
}
