//! Server-side PCP for StartTunnel: the WireGuard-bound socket + serve loop and
//! the [`GatewayBackend`] impl mapping PCP forwards onto nftables + PatchDb. The
//! protocol core (RFC 6887 + HOSTNAME/PORT_SET extensions) lives in
//! [`crate::net::port_map::server`].
//!
//! The socket is `SO_BINDTODEVICE`-bound to the WireGuard interface, so the PCP
//! server is never reachable from the VPS's public interface.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
use std::sync::Arc;
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use tokio::net::UdpSocket;

use crate::net::port_map::server::{GatewayBackend, MappingEntry, PCP_PORT, handle, handle6};
use crate::net::transport::{Transport, TransportProtocol};
use crate::prelude::*;
use crate::tunnel::context::TunnelContext;
use crate::tunnel::db::{PortForward, PortForwards};
use crate::tunnel::forward::address::{ForwardAddress, PinholeAddress};
use crate::tunnel::forward::igd::{
    apply_peer_forward_range, bind_to_wireguard, current_forward, external_ipv4, is_known_client,
};
use crate::tunnel::forward::lease::{self, LeaseKey};
use crate::tunnel::forward::sni::SniDemux;
use crate::tunnel::wg::WIREGUARD_INTERFACE_NAME;

/// Runs IPv4 and IPv6 PCP listeners, rebinding after WireGuard recreation.
pub async fn run(ctx: TunnelContext) {
    let started = Instant::now();
    let v4 = async {
        loop {
            if let Err(e) = serve(&ctx, started).await {
                tracing::warn!("PCP v4 server failed, retrying: {e}");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    };
    let v6 = async {
        loop {
            if let Err(e) = serve6(&ctx, started).await {
                tracing::warn!("PCP v6 server failed, retrying: {e}");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    };
    tokio::join!(v4, v6);
}

fn socket() -> Result<UdpSocket, Error> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
        .with_kind(ErrorKind::Network)?;
    socket
        .set_reuse_address(true)
        .with_kind(ErrorKind::Network)?;
    bind_to_wireguard(&socket)?;
    socket
        .bind(&SockAddr::from(SocketAddrV4::new(
            Ipv4Addr::UNSPECIFIED,
            PCP_PORT,
        )))
        .with_kind(ErrorKind::Network)?;
    socket.set_nonblocking(true).with_kind(ErrorKind::Network)?;
    UdpSocket::from_std(socket.into()).with_kind(ErrorKind::Network)
}

/// A WireGuard-bound IPv6-only PCP socket.
fn socket6() -> Result<UdpSocket, Error> {
    let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))
        .with_kind(ErrorKind::Network)?;
    socket
        .set_reuse_address(true)
        .with_kind(ErrorKind::Network)?;
    socket.set_only_v6(true).with_kind(ErrorKind::Network)?;
    bind_to_wireguard(&socket)?;
    socket
        .bind(&SockAddr::from(SocketAddrV6::new(
            Ipv6Addr::UNSPECIFIED,
            PCP_PORT,
            0,
            0,
        )))
        .with_kind(ErrorKind::Network)?;
    socket.set_nonblocking(true).with_kind(ErrorKind::Network)?;
    UdpSocket::from_std(socket.into()).with_kind(ErrorKind::Network)
}

async fn serve(ctx: &TunnelContext, started: Instant) -> Result<(), Error> {
    // Subscribe before binding to close the setup race.
    let mut ifindex = ctx.forward_ifindex.subscribe();
    ifindex.borrow_and_update();
    let socket = socket()?;
    tracing::info!("PCP server listening on {WIREGUARD_INTERFACE_NAME}:{PCP_PORT}");
    let mut buf = [0u8; 1100];
    loop {
        let (n, from) = tokio::select! {
            res = socket.recv_from(&mut buf) => res.with_kind(ErrorKind::Network)?,
            _ = ifindex.changed() => {
                tracing::info!("{WIREGUARD_INTERFACE_NAME} ifindex changed; rebinding PCP server");
                return Ok(());
            }
        };
        let IpAddr::V4(peer) = from.ip() else {
            continue;
        };
        let epoch = started.elapsed().as_secs() as u32;
        if let Some(resp) = handle(ctx, peer, &buf[..n], epoch).await {
            socket.send_to(&resp, from).await.ok();
        }
    }
}

async fn serve6(ctx: &TunnelContext, started: Instant) -> Result<(), Error> {
    let mut ifindex = ctx.forward_ifindex.subscribe();
    ifindex.borrow_and_update();
    let socket = socket6()?;
    tracing::info!("PCP v6 server listening on {WIREGUARD_INTERFACE_NAME}:{PCP_PORT}");
    let mut buf = [0u8; 1100];
    loop {
        let (n, from) = tokio::select! {
            res = socket.recv_from(&mut buf) => res.with_kind(ErrorKind::Network)?,
            _ = ifindex.changed() => {
                tracing::info!("{WIREGUARD_INTERFACE_NAME} ifindex changed; rebinding PCP v6 server");
                return Ok(());
            }
        };
        let IpAddr::V6(peer) = from.ip() else {
            continue;
        };
        let epoch = started.elapsed().as_secs() as u32;
        if let Some(resp) = handle6(ctx, peer, &buf[..n], epoch).await {
            socket.send_to(&resp, from).await.ok();
        }
    }
}

/// Maps PCP forward operations onto the tunnel's nftables forwards + PatchDb. A
/// peer can only forward to its own tunnel IP (caller passes `target = peer`).
impl GatewayBackend for TunnelContext {
    async fn add_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        count: u16,
        _peer: Ipv4Addr,
        lifetime: Option<u32>,
        transport: TransportProtocol,
    ) -> Result<(), u16> {
        // The forward helper stamps the selected DNAT or SNI-fallback lease.
        apply_peer_forward_range(self, source, target, count, "PCP", lifetime, transport).await
    }

    async fn remove_forward(
        &self,
        peer: Ipv4Addr,
        internal_port: u16,
        transport: TransportProtocol,
    ) {
        remove_peer_forward(self, peer, internal_port, transport).await
    }

    async fn remove_forward_by_source(
        &self,
        source: SocketAddrV4,
        peer: Ipv4Addr,
        transport: TransportProtocol,
    ) -> bool {
        self.remove_peer_forward_by_source(source, peer, true, transport.into())
            .await
    }

    async fn external_ipv4(&self, peer: Ipv4Addr) -> Option<Ipv4Addr> {
        external_ipv4(self, peer).await
    }

    async fn is_known_client(&self, peer: Ipv4Addr) -> bool {
        is_known_client(self, peer).await
    }

    async fn is_known_gua(&self, gua: Ipv6Addr) -> bool {
        crate::tunnel::forward::pinhole::is_known_gua(self, gua).await
    }

    async fn add_pinhole(
        &self,
        gua: Ipv6Addr,
        external_port: u16,
        internal_port: u16,
        count: u16,
        lifetime: Option<u32>,
        transport: TransportProtocol,
    ) -> Result<(), u16> {
        crate::tunnel::forward::pinhole::add_pinhole(
            self,
            gua,
            external_port,
            internal_port,
            count,
            None,
            true,
            transport.into(),
        )
        .await
        .map_err(|e| {
            tracing::warn!("PCP v6 pinhole {gua}:{external_port} failed: {e}");
            0u16
        })?;
        if let Some(lt) = lifetime {
            lease::stamp(
                self,
                LeaseKey::Pinhole(PinholeAddress::new(
                    SocketAddrV6::new(gua, external_port, 0, 0),
                    transport.into(),
                )),
                lt,
            );
        }
        Ok(())
    }

    async fn remove_pinhole(
        &self,
        gua: Ipv6Addr,
        external_port: u16,
        transport: TransportProtocol,
    ) {
        crate::tunnel::forward::pinhole::remove_pinhole(
            self,
            gua,
            external_port,
            true,
            transport.into(),
        )
        .await;
        lease::forget(
            self,
            &LeaseKey::Pinhole(PinholeAddress::new(
                SocketAddrV6::new(gua, external_port, 0, 0),
                transport.into(),
            )),
        );
    }

    async fn list_forwards(&self, peer: Ipv4Addr) -> Vec<MappingEntry> {
        match self.db.peek().await.as_port_forwards().de() {
            Ok(forwards) => mapping_entries(&forwards, peer),
            Err(e) => {
                tracing::warn!("failed to read port forwards for {peer}: {e}");
                Vec::new()
            }
        }
    }

    fn sni(&self) -> Option<&Arc<SniDemux>> {
        Some(&self.sni)
    }

    async fn add_sni_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
        lifetime: Option<u32>,
    ) -> Result<(), u8> {
        self.persist_sni_forward(source, target, hostnames, lifetime, true, None)
            .await
    }

    async fn remove_sni_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
    ) {
        self.remove_sni_routes(source, target, hostnames, true)
            .await;
    }
}

impl TunnelContext {
    pub(super) async fn remove_peer_forward_by_source(
        &self,
        source: SocketAddrV4,
        peer: Ipv4Addr,
        auto_only: bool,
        transport: Transport,
    ) -> bool {
        let _guard = self.forward_write_lock.lock().await;
        let key = ForwardAddress::new(source, transport);
        let entry = current_forward(self, key)
            .await
            .or(current_forward(self, source.into())
                .await
                .filter(|e| matches!(e, PortForward::Sni { .. }) && transport != Transport::Udp));
        match entry {
            Some(PortForward::Dnat { target, auto, .. })
                if *target.ip() == peer && (!auto_only || auto) =>
            {
                if self
                    .db
                    .mutate(|db| {
                        db.as_port_forwards_mut().mutate(|pf| {
                            pf.0.remove(&key);
                            Ok(())
                        })
                    })
                    .await
                    .result
                    .is_err()
                {
                    return false;
                }
                if let Some(rc) = self.active_forwards.mutate(|m| m.remove(&key)) {
                    drop(rc);
                    self.forward.gc().await.log_err();
                }
                lease::forget(self, &LeaseKey::Dnat(key));
                true
            }
            // Plain mappings on SNI ports own only the fallback.
            Some(PortForward::Sni {
                fallback: Some(fallback),
                ..
            }) if *fallback.target.ip() == peer && (!auto_only || fallback.auto) => {
                self.remove_sni_fallback_locked(source, fallback.target)
                    .await;
                true
            }
            _ => false,
        }
    }

    pub(in crate::tunnel) async fn remove_sni_routes(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
        auto_only: bool,
    ) {
        let _guard = self.forward_write_lock.lock().await;
        let hostnames = hostnames.to_vec();
        let Some(removed) = self
            .db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    let mut removed = Vec::new();
                    let mut now_empty = false;
                    if let Some(PortForward::Sni { routes, fallback }) =
                        pf.0.get_mut(&source.into())
                    {
                        routes.retain(|hostname, route| {
                            let remove = route.target == target
                                && hostnames.contains(hostname)
                                && (!auto_only || route.auto);
                            if remove {
                                removed.push(hostname.clone());
                            }
                            !remove
                        });
                        now_empty = routes.is_empty() && fallback.is_none();
                    }
                    if now_empty {
                        pf.0.remove(&source.into());
                    }
                    Ok(removed)
                })
            })
            .await
            .result
            .log_err()
        else {
            return;
        };
        self.sni
            .unregister(*source.ip(), source.port(), &removed, target);
        for hostname in removed {
            lease::forget(self, &LeaseKey::Sni { source, hostname });
        }
    }

    /// Persists and registers SNI-demuxed hostname routes.
    pub async fn persist_sni_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
        lifetime: Option<u32>,
        auto: bool,
        label: Option<String>,
    ) -> Result<(), u8> {
        let _guard = self.forward_write_lock.lock().await;
        self.sni.prepare().await?;
        let default_label = if auto {
            Some("Automatic".to_string())
        } else {
            label
        };
        // Reject conflicts before displacing a working DNAT.
        let hostnames_owned = hostnames.to_vec();
        let persisted = self
            .db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    use crate::tunnel::db::{PortForward, SniRoute};
                    let tcp_key = ForwardAddress::new(source, Transport::Tcp);
                    let previous_key = if pf.0.contains_key(&tcp_key) {
                        tcp_key
                    } else {
                        source.into()
                    };
                    let previous = pf.0.get(&previous_key).cloned();
                    if let Some(conflict) = pf
                        .overlapping(tcp_key, 1)
                        .filter(|key| *key != previous_key)
                    {
                        return Err(Error::new(
                            eyre!("{source} overlaps an existing forward at {conflict}"),
                            ErrorKind::InvalidRequest,
                        ));
                    }
                    let mut converted = None;
                    if plan_dnat_conversion(pf.0.get(&previous_key), source, target)? {
                        if let Some(dnat @ PortForward::Dnat { .. }) = pf.0.remove(&previous_key) {
                            let PortForward::Dnat {
                                target,
                                label,
                                enabled,
                                auto,
                                ..
                            } = &dnat
                            else {
                                unreachable!()
                            };
                            if previous_key.transport == Transport::Both {
                                pf.0.insert(
                                    ForwardAddress::new(source, Transport::Udp),
                                    dnat.clone(),
                                );
                            }
                            pf.0.insert(
                                source.into(),
                                PortForward::Sni {
                                    routes: std::collections::BTreeMap::new(),
                                    fallback: Some(SniRoute {
                                        target: *target,
                                        label: label.clone(),
                                        enabled: *enabled,
                                        auto: *auto,
                                    }),
                                },
                            );
                            converted = Some(dnat);
                        }
                    }
                    let entry =
                        pf.0.entry(source.into())
                            .or_insert_with(|| PortForward::Sni {
                                routes: std::collections::BTreeMap::new(),
                                fallback: None,
                            });
                    match entry {
                        PortForward::Sni { routes, .. } => {
                            for h in &hostnames_owned {
                                if routes
                                    .get(h)
                                    .is_some_and(|r| route_conflicts(r, target, auto))
                                {
                                    return Err(Error::new(
                                        eyre!(
                                            "SNI hostname {h} on {source} is held by another client"
                                        ),
                                        ErrorKind::InvalidRequest,
                                    ));
                                }
                            }
                            let mut enabled_hostnames = Vec::new();
                            for h in &hostnames_owned {
                                let (label, enabled, auto) =
                                    sni_route_fields(routes.get(h), auto, &default_label);
                                if enabled {
                                    enabled_hostnames.push(h.clone());
                                }
                                routes.insert(
                                    h.clone(),
                                    SniRoute {
                                        target,
                                        label,
                                        enabled,
                                        auto,
                                    },
                                );
                            }
                            Ok((converted, previous_key, previous, enabled_hostnames))
                        }
                        PortForward::Dnat { .. } => Err(Error::new(
                            eyre!("{source} is already a DNAT forward"),
                            ErrorKind::InvalidRequest,
                        )),
                    }
                })
            })
            .await
            .result;
        let (converted, previous_key, previous, enabled_hostnames) = match persisted {
            Ok(c) => c,
            Err(_) => return Err(crate::net::port_map::pcp::hostname::RESULT_HOSTNAME_TAKEN),
        };
        let converted_dnat = converted.as_ref().and_then(|forward| match forward {
            PortForward::Dnat {
                target, enabled, ..
            } => Some((*target, *enabled)),
            PortForward::Sni { .. } => None,
        });
        if let Some((dnat_target, dnat_enabled)) = converted_dnat {
            let dnat = converted.unwrap();
            register_converted_sni(
                &self.sni,
                source,
                target,
                &enabled_hostnames,
                dnat_enabled.then_some(dnat_target),
                || self.restore_persisted_forward(previous_key, Some(dnat)),
            )
            .await?;
        } else if let Err(code) = self.sni.register(
            *source.ip(),
            source.port(),
            &enabled_hostnames,
            target,
            None,
        ) {
            self.restore_persisted_forward(previous_key, previous).await;
            return Err(code);
        }
        if let Some((dnat_target, dnat_enabled)) = converted_dnat {
            if let Some(rc) = self.active_forwards.mutate(|m| {
                m.remove(&ForwardAddress::new(source, Transport::Tcp))
                    .or_else(|| m.remove(&source.into()))
            }) {
                drop(rc);
                self.forward.gc().await.log_err();
            }
            if previous_key.transport == Transport::Both && dnat_enabled {
                let prefix = crate::tunnel::forward::igd::prefix_for(self, dnat_target.ip()).await;
                let rc = self
                    .forward
                    .add_forward_range(source, dnat_target, 1, prefix, None, Transport::Udp)
                    .await
                    .map_err(|_| crate::net::port_map::pcp::RESULT_NO_RESOURCES)?;
                self.active_forwards
                    .mutate(|m| m.insert(ForwardAddress::new(source, Transport::Udp), rc));
            }
            let carried = self
                .leases
                .mutate(|l| l.remove(&LeaseKey::Dnat(ForwardAddress::new(source, Transport::Tcp))));
            if let Some(exp) = carried {
                self.leases.mutate(|l| {
                    l.insert(LeaseKey::SniFallback(source), exp);
                });
            }
        }
        if let Some(lt) = lifetime {
            for h in hostnames {
                lease::stamp(
                    self,
                    LeaseKey::Sni {
                        source,
                        hostname: h.clone(),
                    },
                    lt,
                );
            }
        }
        Ok(())
    }

    async fn restore_persisted_forward(
        &self,
        source: ForwardAddress,
        previous: Option<PortForward>,
    ) {
        self.db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|forwards| {
                    restore_forward_entry(forwards, source, previous.clone());
                    Ok(())
                })
            })
            .await
            .result
            .log_err();
    }

    /// Persists a hostname-less fallback on an existing SNI port.
    /// The same target reclaims it; a different target is rejected.
    pub async fn persist_fallback_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        lifetime: Option<u32>,
        auto: bool,
        label: Option<String>,
    ) -> Result<(), u8> {
        let _guard = self.forward_write_lock.lock().await;
        self.persist_fallback_forward_locked(source, target, lifetime, auto, label)
            .await
    }

    pub(super) async fn persist_fallback_forward_locked(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        lifetime: Option<u32>,
        auto: bool,
        label: Option<String>,
    ) -> Result<(), u8> {
        self.sni.prepare().await?;
        let default_label = if auto {
            Some("Automatic".to_string())
        } else {
            label
        };
        let persisted = self
            .db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    use crate::tunnel::db::{PortForward, SniRoute};
                    match pf.0.get_mut(&source.into()) {
                        Some(PortForward::Sni { fallback, .. }) => {
                            if fallback
                                .as_ref()
                                .is_some_and(|f| route_conflicts(f, target, auto))
                            {
                                return Err(Error::new(
                                    eyre!("fallback on {source} is held by another client"),
                                    ErrorKind::InvalidRequest,
                                ));
                            }
                            let (label, enabled, auto) =
                                sni_route_fields(fallback.as_ref(), auto, &default_label);
                            *fallback = Some(SniRoute {
                                target,
                                label,
                                enabled,
                                auto,
                            });
                            Ok(enabled)
                        }
                        _ => Err(Error::new(
                            eyre!("{source} is not an SNI-demuxed port"),
                            ErrorKind::InvalidRequest,
                        )),
                    }
                })
            })
            .await
            .result;
        let Ok(enabled) = persisted else {
            return Err(crate::net::port_map::pcp::hostname::RESULT_HOSTNAME_TAKEN);
        };
        if enabled
            && self
                .sni
                .register_fallback(*source.ip(), source.port(), target)
                .is_err()
        {
            self.remove_sni_fallback_locked(source, target).await;
            return Err(crate::net::port_map::pcp::hostname::RESULT_HOSTNAME_TAKEN);
        }
        if let Some(lt) = lifetime {
            lease::stamp(self, LeaseKey::SniFallback(source), lt);
        }
        Ok(())
    }

    /// Remove the hostname-less fallback on `source`, only if held by `target`.
    /// Drops the shared port entirely if no SNI routes remain either.
    pub async fn remove_sni_fallback(&self, source: SocketAddrV4, target: SocketAddrV4) {
        let _guard = self.forward_write_lock.lock().await;
        self.remove_sni_fallback_locked(source, target).await;
    }

    pub(super) async fn remove_sni_fallback_locked(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
    ) {
        self.sni
            .unregister_fallback(*source.ip(), source.port(), target);
        lease::forget(self, &LeaseKey::SniFallback(source));
        self.db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    use crate::tunnel::db::PortForward;
                    let mut now_empty = false;
                    if let Some(PortForward::Sni { routes, fallback }) =
                        pf.0.get_mut(&source.into())
                    {
                        if fallback.as_ref().is_some_and(|f| f.target == target) {
                            *fallback = None;
                        }
                        now_empty = routes.is_empty() && fallback.is_none();
                    }
                    if now_empty {
                        pf.0.remove(&source.into());
                    }
                    Ok(())
                })
            })
            .await
            .result
            .log_err();
    }
}

async fn register_converted_sni<F, Fut>(
    sni: &Arc<SniDemux>,
    source: SocketAddrV4,
    target: SocketAddrV4,
    hostnames: &[String],
    fallback_target: Option<SocketAddrV4>,
    restore: F,
) -> Result<(), u8>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    if let Some(fallback_target) = fallback_target {
        if let Err(code) = sni.register_fallback(*source.ip(), source.port(), fallback_target) {
            restore().await;
            return Err(code);
        }
    }
    if let Err(code) = sni.register(*source.ip(), source.port(), hostnames, target, None) {
        if let Some(fallback_target) = fallback_target {
            sni.unregister_fallback(*source.ip(), source.port(), fallback_target);
        }
        restore().await;
        return Err(code);
    }
    Ok(())
}

fn restore_forward_entry(
    forwards: &mut PortForwards,
    source: impl Into<ForwardAddress>,
    previous: Option<PortForward>,
) {
    let source = source.into();
    forwards.0.remove(&source.address.into());
    if source.transport == Transport::Both && matches!(&previous, Some(PortForward::Dnat { .. })) {
        forwards
            .0
            .remove(&ForwardAddress::new(source.address, Transport::Udp));
    }
    match previous {
        Some(forward) => {
            forwards.0.insert(source, forward);
        }
        None => {
            forwards.0.remove(&source);
        }
    }
}

fn plan_dnat_conversion(
    existing: Option<&PortForward>,
    source: SocketAddrV4,
    new_target: SocketAddrV4,
) -> Result<bool, Error> {
    match existing {
        Some(PortForward::Dnat { count, target, .. }) => {
            if *count != 1 {
                return Err(Error::new(
                    eyre!("{source} is already a DNAT range forward"),
                    ErrorKind::InvalidRequest,
                ));
            }
            if target.ip() != new_target.ip() {
                return Err(Error::new(
                    eyre!("{source} is already a DNAT forward for another client"),
                    ErrorKind::InvalidRequest,
                ));
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// A route's owner repoints it within its own device. Any other change conflicts.
fn route_conflicts(route: &crate::tunnel::db::SniRoute, target: SocketAddrV4, auto: bool) -> bool {
    route.target != target && (route.auto != auto || route.target.ip() != target.ip())
}

/// Preserves an existing SNI route's owner, enabled state, and label.
fn sni_route_fields(
    existing: Option<&crate::tunnel::db::SniRoute>,
    auto: bool,
    default_label: &Option<String>,
) -> (Option<String>, bool, bool) {
    match existing {
        Some(r) => (
            r.label.clone().or_else(|| default_label.clone()),
            r.enabled,
            r.auto,
        ),
        None => (default_label.clone(), true, auto),
    }
}

/// Matches the automatic mapping a peer's lifetime-0 request owns.
fn peer_forward_matches(entry: &PortForward, target: &SocketAddrV4) -> bool {
    match entry {
        PortForward::Dnat {
            target: t, auto, ..
        } => *auto && t == target,
        PortForward::Sni { fallback, .. } => fallback
            .as_ref()
            .is_some_and(|f| f.auto && &f.target == target),
    }
}

fn peer_forward_source(
    forwards: &PortForwards,
    target: &SocketAddrV4,
    transport: TransportProtocol,
) -> Option<(ForwardAddress, bool)> {
    forwards
        .0
        .iter()
        .find(|(source, entry)| {
            entry.transport(**source).protocols().contains(&transport)
                && peer_forward_matches(entry, target)
        })
        .map(|(source, entry)| (*source, matches!(entry, PortForward::Sni { .. })))
}

/// Removes automatic mappings matching PCP's transport, internal port, and client.
async fn remove_peer_forward(
    ctx: &TunnelContext,
    peer: Ipv4Addr,
    internal_port: u16,
    transport: TransportProtocol,
) {
    let target = SocketAddrV4::new(peer, internal_port);
    loop {
        let source = ctx
            .db
            .peek()
            .await
            .as_port_forwards()
            .de()
            .ok()
            .and_then(|pf| peer_forward_source(&pf, &target, transport));
        let Some((source, _)) = source else {
            return;
        };
        if !ctx
            .remove_peer_forward_by_source(source.address, peer, true, transport.into())
            .await
        {
            return;
        }
    }
}

/// The gateway-created forwards pointing at `peer`, as IGD mapping entries.
///
/// Reports leases as permanent; expiry belongs to the lease map.
fn mapping_entries(forwards: &PortForwards, peer: Ipv4Addr) -> Vec<MappingEntry> {
    let mut out = Vec::new();
    for (source, forward) in &forwards.0 {
        let PortForward::Dnat {
            target,
            enabled,
            count,
            auto,
            label,
            ..
        } = forward
        else {
            continue;
        };
        if !enabled || !auto || *target.ip() != peer {
            continue;
        }
        let description = label
            .clone()
            .unwrap_or_else(|| "Automatic forward".to_string());
        for offset in 0..*count {
            let (Some(external_port), Some(internal_port)) = (
                source.port().checked_add(offset),
                target.port().checked_add(offset),
            ) else {
                break;
            };
            out.extend(
                forward
                    .transport(*source)
                    .protocols()
                    .iter()
                    .map(|protocol| MappingEntry {
                        external_port,
                        internal: SocketAddrV4::new(peer, internal_port),
                        protocol: protocol.igd_name(),
                        description: description.clone(),
                        lease_seconds: 0,
                    }),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddrV4;

    use super::{
        mapping_entries, peer_forward_matches, peer_forward_source, plan_dnat_conversion,
        register_converted_sni, restore_forward_entry, route_conflicts, sni_route_fields,
    };
    use crate::net::transport::{Transport, TransportProtocol};
    use crate::tunnel::db::{PortForward, PortForwards, SniRoute};
    use crate::tunnel::forward::address::ForwardAddress;

    fn route(label: Option<&str>, enabled: bool, auto: bool) -> SniRoute {
        SniRoute {
            target: "10.59.0.2:443".parse::<SocketAddrV4>().unwrap(),
            label: label.map(str::to_string),
            enabled,
            auto,
        }
    }

    fn dnat(target: &str, count: u16) -> PortForward {
        PortForward::Dnat {
            target: target.parse().unwrap(),
            label: None,
            enabled: true,
            count,
            auto: true,
        }
    }

    // A lifetime-0 delete matches the peer's own SNI fallback (what its bare
    // MAP created) as well as its DNAT — never another client's fallback, and
    // not a route-only SNI port.
    #[test]
    fn peer_delete_matches_dnat_and_own_fallback() {
        let mine: SocketAddrV4 = "10.59.217.2:5349".parse().unwrap();
        let sni = |target: SocketAddrV4, auto| PortForward::Sni {
            routes: Default::default(),
            fallback: Some(SniRoute {
                target,
                label: None,
                enabled: true,
                auto,
            }),
        };
        assert!(peer_forward_matches(&sni(mine, true), &mine));
        assert!(!peer_forward_matches(&sni(mine, false), &mine));
        assert!(
            !peer_forward_matches(&sni("10.59.217.9:5349".parse().unwrap(), true), &mine),
            "another client's fallback must not match"
        );
        assert!(
            !peer_forward_matches(
                &PortForward::Sni {
                    routes: Default::default(),
                    fallback: None,
                },
                &mine
            ),
            "a route-only SNI port has nothing for a bare delete"
        );
        assert!(peer_forward_matches(&dnat("10.59.217.2:5349", 1), &mine));
        assert!(!peer_forward_matches(
            &owned_dnat("10.59.217.2:5349", 1, true, false),
            &mine
        ));
        assert!(!peer_forward_matches(&dnat("10.59.217.9:5349", 1), &mine));
    }

    #[test]
    fn peer_delete_skips_an_earlier_manual_forward() {
        let target: SocketAddrV4 = "10.59.217.2:2525".parse().unwrap();
        let manual_source: SocketAddrV4 = "203.0.113.1:25".parse().unwrap();
        let auto_source: SocketAddrV4 = "203.0.113.1:2525".parse().unwrap();
        let mut forwards = PortForwards(Default::default());
        forwards.0.insert(
            manual_source.into(),
            owned_dnat("10.59.217.2:2525", 1, true, false),
        );
        forwards.0.insert(
            auto_source.into(),
            owned_dnat("10.59.217.2:2525", 1, true, true),
        );

        assert_eq!(
            peer_forward_source(
                &forwards,
                &target,
                crate::net::transport::TransportProtocol::Tcp
            ),
            Some((auto_source.into(), false))
        );
    }

    // A hostname MAP may promote *its own* client's lone DNAT to the port's
    // fallback, but must not carve up another client's whole-port DNAT, a DNAT
    // range, and does nothing on an empty or already-SNI port.
    #[test]
    fn dnat_conversion_is_owner_scoped() {
        let src: SocketAddrV4 = "1.2.3.4:443".parse().unwrap();
        let mine: SocketAddrV4 = "10.59.0.2:443".parse().unwrap();
        let theirs: SocketAddrV4 = "10.59.0.3:443".parse().unwrap();

        // Same owner's lone DNAT -> convert.
        assert!(plan_dnat_conversion(Some(&dnat("10.59.0.2:443", 1)), src, mine).unwrap());
        // A different client's DNAT is an exclusive whole-port claim -> reject.
        assert!(plan_dnat_conversion(Some(&dnat("10.59.0.3:443", 1)), src, mine).is_err());
        // Even the owner can't fold a DNAT *range* into a single SNI port.
        assert!(plan_dnat_conversion(Some(&dnat("10.59.0.2:443", 4)), src, mine).is_err());
        // Nothing to convert on an empty port or one already SNI.
        assert!(!plan_dnat_conversion(None, src, mine).unwrap());
        let sni = PortForward::Sni {
            routes: std::collections::BTreeMap::new(),
            fallback: None,
        };
        assert!(!plan_dnat_conversion(Some(&sni), src, theirs).unwrap());
    }

    #[tokio::test]
    async fn blocked_sni_listener_restores_converted_dnat() {
        let blocked = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let source = match blocked.local_addr().unwrap() {
            std::net::SocketAddr::V4(source) => source,
            std::net::SocketAddr::V6(_) => unreachable!(),
        };
        let target: SocketAddrV4 = "10.59.0.2:443".parse().unwrap();
        let dnat = dnat("10.59.0.2:443", 1);
        let mut forwards = PortForwards(Default::default());
        forwards.0.insert(
            source.into(),
            PortForward::Sni {
                routes: Default::default(),
                fallback: Some(SniRoute {
                    target,
                    label: None,
                    enabled: true,
                    auto: true,
                }),
            },
        );
        let forwards = std::sync::Arc::new(std::sync::Mutex::new(forwards));
        let restore_forwards = forwards.clone();
        let sni = crate::tunnel::forward::sni::SniDemux::new();

        let code = register_converted_sni(
            &sni,
            source,
            target,
            &["blocked.example.com".to_string()],
            Some(target),
            move || async move {
                restore_forward_entry(&mut restore_forwards.lock().unwrap(), source, Some(dnat));
            },
        )
        .await
        .unwrap_err();
        assert_eq!(code, crate::net::port_map::pcp::RESULT_NO_RESOURCES);
        assert!(matches!(
            forwards.lock().unwrap().0.get(&source.into()),
            Some(PortForward::Dnat { target: restored, .. }) if *restored == target
        ));
    }

    // A manually-added hostname on a fresh source is stored as manual, with the
    // user's own label — never as an automatic `PCP` route (the reported bug).
    #[test]
    fn new_manual_route_is_not_auto() {
        let (label, enabled, auto) = sni_route_fields(None, false, &Some("my label".to_string()));
        assert_eq!(label.as_deref(), Some("my label"));
        assert!(enabled);
        assert!(!auto);
    }

    // A fresh PCP route defaults to the `PCP` label and is automatic.
    #[test]
    fn new_pcp_route_is_auto() {
        let (label, enabled, auto) = sni_route_fields(None, true, &Some("PCP".to_string()));
        assert_eq!(label.as_deref(), Some("PCP"));
        assert!(enabled);
        assert!(auto);
    }

    // A PCP re-assert of a hostname the user added manually keeps it manual — the
    // renewal preserves the existing owner, label, and enabled state.
    #[test]
    fn pcp_renewal_preserves_manual_owner() {
        let existing = route(Some("mine"), false, false);
        let (label, enabled, auto) =
            sni_route_fields(Some(&existing), true, &Some("PCP".to_string()));
        assert_eq!(label.as_deref(), Some("mine"));
        assert!(!enabled);
        assert!(!auto);
    }

    #[test]
    fn a_route_is_repointed_only_by_its_owner_within_its_device() {
        let auto_route = route(None, true, true);
        let same = "10.59.0.2:443".parse().unwrap();
        let new_port = "10.59.0.2:8443".parse().unwrap();
        let other_device = "10.59.0.3:443".parse().unwrap();
        assert!(!route_conflicts(&auto_route, same, true));
        assert!(!route_conflicts(&auto_route, same, false));
        assert!(!route_conflicts(&auto_route, new_port, true));
        assert!(route_conflicts(&auto_route, new_port, false));
        assert!(route_conflicts(&auto_route, other_device, true));
        assert!(route_conflicts(&route(None, true, false), new_port, true));
    }

    // Symmetrically, a manual re-add over an existing automatic route leaves it
    // automatic; an unlabeled existing route inherits the caller's default label.
    #[test]
    fn manual_readd_preserves_auto_owner_and_backfills_label() {
        let existing = route(None, true, true);
        let (label, _enabled, auto) =
            sni_route_fields(Some(&existing), false, &Some("ignored".to_string()));
        assert_eq!(label.as_deref(), Some("ignored"));
        assert!(auto);
    }

    #[test]
    fn tcp_and_udp_mappings_have_independent_identity_and_listing() {
        let source: SocketAddrV4 = "203.0.113.1:5000".parse().unwrap();
        let tcp = ForwardAddress::new(source, Transport::Tcp);
        let udp = ForwardAddress::new(source, Transport::Udp);
        let target: SocketAddrV4 = "10.59.0.2:6000".parse().unwrap();
        let mut forwards = PortForwards(Default::default());
        forwards
            .0
            .insert(tcp, owned_dnat(&target.to_string(), 2, true, true));
        forwards
            .0
            .insert(udp, owned_dnat(&target.to_string(), 2, true, true));
        assert_eq!(
            peer_forward_source(&forwards, &target, TransportProtocol::Tcp),
            Some((tcp, false))
        );
        assert_eq!(
            peer_forward_source(&forwards, &target, TransportProtocol::Udp),
            Some((udp, false))
        );
        assert_eq!(mapping_entries(&forwards, *target.ip()).len(), 4);
        forwards.0.remove(&tcp);
        let entries = mapping_entries(&forwards, *target.ip());
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|entry| entry.protocol == "UDP"));
        assert_eq!(entries[1].internal.port(), 6001);
        assert_eq!(
            peer_forward_source(&forwards, &target, TransportProtocol::Tcp),
            None
        );
        assert_eq!(
            peer_forward_source(&forwards, &target, TransportProtocol::Udp),
            Some((udp, false))
        );
    }

    #[test]
    fn failed_sni_registration_does_not_remove_an_independent_udp_mapping() {
        let source: SocketAddrV4 = "203.0.113.1:443".parse().unwrap();
        let udp = ForwardAddress::new(source, Transport::Udp);
        let mut forwards = PortForwards(Default::default());
        forwards
            .0
            .insert(udp, owned_dnat("10.59.0.2:443", 1, true, true));
        let sni = PortForward::Sni {
            routes: Default::default(),
            fallback: Some(route(None, true, true)),
        };
        forwards.0.insert(source.into(), sni.clone());
        restore_forward_entry(&mut forwards, source, Some(sni));
        assert!(forwards.0.contains_key(&udp));
        assert!(matches!(
            forwards.0.get(&source.into()),
            Some(PortForward::Sni { .. })
        ));
    }

    fn owned_dnat(target: &str, count: u16, enabled: bool, auto: bool) -> PortForward {
        PortForward::Dnat {
            target: target.parse().unwrap(),
            label: None,
            enabled,
            count,
            auto,
        }
    }

    // A client reads a mapping back after creating it and treats a failed read
    // as a failed mapping, so this must find the peer's own automatic forwards
    // — and only those.
    #[test]
    fn mapping_entries_report_only_the_peers_own_automatic_forwards() {
        let peer: std::net::Ipv4Addr = "10.59.0.2".parse().unwrap();
        let mut forwards = PortForwards(Default::default());
        let mut insert = |source: &str, f: PortForward| {
            forwards
                .0
                .insert(source.parse::<SocketAddrV4>().unwrap().into(), f);
        };
        insert(
            "203.0.113.1:443",
            owned_dnat("10.59.0.2:8443", 1, true, true),
        );
        // Another device's forward, a manual one, and a disabled one: all excluded.
        insert(
            "203.0.113.1:444",
            owned_dnat("10.59.0.9:8443", 1, true, true),
        );
        insert(
            "203.0.113.1:445",
            owned_dnat("10.59.0.2:9443", 1, true, false),
        );
        insert(
            "203.0.113.1:446",
            owned_dnat("10.59.0.2:9444", 1, false, true),
        );
        drop(insert);

        let entries = mapping_entries(&forwards, peer);
        // One forward, reported for both transports.
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.external_port == 443));
        assert!(entries.iter().all(|e| e.internal.port() == 8443));
        assert!(entries.iter().all(|e| *e.internal.ip() == peer));
        assert!(entries.iter().all(|e| e.lease_seconds == 0));
        let mut protocols: Vec<_> = entries.iter().map(|e| e.protocol).collect();
        protocols.sort();
        assert_eq!(protocols, vec!["TCP", "UDP"]);
    }

    // A PORT_SET range answers for every port it covers, not just its base.
    #[test]
    fn mapping_entries_expand_a_port_range() {
        let peer: std::net::Ipv4Addr = "10.59.0.2".parse().unwrap();
        let mut forwards = PortForwards(Default::default());
        forwards.0.insert(
            "203.0.113.1:5000".parse::<SocketAddrV4>().unwrap().into(),
            owned_dnat("10.59.0.2:6000", 3, true, true),
        );

        let entries = mapping_entries(&forwards, peer);
        assert_eq!(entries.len(), 6, "3 ports x 2 transports");
        let tcp: Vec<_> = entries
            .iter()
            .filter(|e| e.protocol == "TCP")
            .map(|e| (e.external_port, e.internal.port()))
            .collect();
        assert_eq!(tcp, vec![(5000, 6000), (5001, 6001), (5002, 6002)]);
    }
}
