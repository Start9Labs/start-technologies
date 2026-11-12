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
use crate::net::port_map::{Protocol as MappingProtocol, TransportProtocol};
use crate::prelude::*;
use crate::tunnel::context::TunnelContext;
use crate::tunnel::db::{PortForward, PortForwards};
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
    ) -> Result<(), u16> {
        // The forward helper stamps the selected DNAT or SNI-fallback lease.
        apply_peer_forward_range(self, source, target, count, "PCP", lifetime).await
    }

    async fn remove_forward(&self, peer: Ipv4Addr, internal_port: u16) {
        remove_peer_forward(self, peer, internal_port).await
    }

    async fn remove_forward_by_source(&self, source: SocketAddrV4, peer: Ipv4Addr) -> bool {
        self.remove_peer_forward_by_source(source, peer, true).await
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
    ) -> Result<(), u16> {
        let _guard = self.forward_write_lock.lock().await;
        let pinhole = crate::tunnel::forward::pinhole::add_pinhole_locked(
            self,
            gua,
            external_port,
            internal_port,
            count,
            None,
            true,
        )
        .await
        .map_err(|e| {
            tracing::warn!("PCP v6 pinhole {gua}:{external_port} failed: {e}");
            0u16
        })?;
        if pinhole.auto {
            if let Some(lt) = lifetime {
                lease::stamp(
                    self,
                    LeaseKey::Pinhole(SocketAddrV6::new(gua, external_port, 0, 0)),
                    lt,
                );
            }
        }
        Ok(())
    }

    async fn remove_pinhole(&self, gua: Ipv6Addr, external_port: u16) {
        let _guard = self.forward_write_lock.lock().await;
        crate::tunnel::forward::pinhole::remove_pinhole_locked(self, gua, external_port, true)
            .await;
        lease::forget(
            self,
            &LeaseKey::Pinhole(SocketAddrV6::new(gua, external_port, 0, 0)),
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
        protocol: TransportProtocol,
        lifetime: Option<u32>,
    ) -> Result<(), u8> {
        self.persist_sni_forward(source, target, hostnames, protocol, lifetime, true, None)
            .await
    }

    async fn remove_sni_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
        protocol: TransportProtocol,
    ) {
        self.remove_sni_routes(source, target, hostnames, protocol, true)
            .await;
    }
}

impl TunnelContext {
    pub(super) async fn remove_peer_forward_by_source(
        &self,
        source: SocketAddrV4,
        peer: Ipv4Addr,
        auto_only: bool,
    ) -> bool {
        let _guard = self.forward_write_lock.lock().await;
        match current_forward(self, source).await {
            Some(PortForward::Dnat { target, auto, .. })
                if *target.ip() == peer && (!auto_only || auto) =>
            {
                if self
                    .db
                    .mutate(|db| db.as_port_forwards_mut().remove(&source).map(|_| ()))
                    .await
                    .result
                    .is_err()
                {
                    return false;
                }
                if let Some(rc) = self.active_forwards.mutate(|m| m.remove(&source)) {
                    drop(rc);
                    self.forward.gc().await.log_err();
                }
                lease::forget(self, &LeaseKey::Dnat(source));
                true
            }
            Some(entry @ PortForward::Sni { .. }) => {
                let mut removed = false;
                for (protocol, bindings) in entry.sni_bindings() {
                    if let Some(fallback) = bindings
                        .fallback
                        .as_ref()
                        .filter(|f| *f.target.ip() == peer && (!auto_only || f.auto))
                    {
                        self.remove_sni_fallback_locked(source, fallback.target, protocol)
                            .await;
                        removed = true;
                    }
                }
                removed
            }
            _ => false,
        }
    }

    pub(in crate::tunnel) async fn remove_sni_routes(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
        protocol: impl Into<MappingProtocol>,
        auto_only: bool,
    ) {
        let _guard = self.forward_write_lock.lock().await;
        self.remove_sni_routes_locked(source, target, hostnames, protocol.into(), auto_only)
            .await;
    }

    pub(in crate::tunnel) async fn remove_sni_routes_locked(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
        protocol: MappingProtocol,
        auto_only: bool,
    ) {
        let Some(removed) = self
            .db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    Ok(remove_owned_sni_routes(
                        pf, source, target, hostnames, protocol, auto_only,
                    ))
                })
            })
            .await
            .result
            .log_err()
        else {
            return;
        };
        self.sni
            .unregister_protocol(protocol, *source.ip(), source.port(), &removed, target);
        for hostname in removed {
            lease::forget(
                self,
                &LeaseKey::Sni {
                    source,
                    hostname,
                    protocol,
                },
            );
        }
    }

    /// Persists one owner for the complete protocol selection; overlapping selections conflict.
    /// Automatic single-transport requests reuse matching manual combined intent without changes or leases.
    pub async fn persist_sni_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
        protocol: impl Into<MappingProtocol>,
        lifetime: Option<u32>,
        auto: bool,
        label: Option<String>,
    ) -> Result<(), u8> {
        let protocol = protocol.into();
        let _guard = self.forward_write_lock.lock().await;
        self.sni.prepare().await?;
        let default_label = if auto {
            Some("Automatic".to_string())
        } else {
            label
        };
        let hostnames: Vec<_> = hostnames.iter().map(|h| h.to_ascii_lowercase()).collect();
        let persisted = self
            .db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    let previous = pf.0.get(&source).cloned();
                    if let Some(conflict) = pf.overlapping(source, 1) {
                        return Err(Error::new(
                            eyre!("{source} overlaps an existing forward at {conflict}"),
                            ErrorKind::InvalidRequest,
                        ));
                    }
                    let converted = plan_dnat_conversion(previous.as_ref(), source, target)?;
                    if converted {
                        let entry = pf.0.get_mut(&source).unwrap();
                        entry.convert_to_sni();
                    }
                    let entry = pf.0.entry(source).or_insert_with(PortForward::empty_sni);
                    let claims = claim_sni_routes(
                        entry,
                        protocol,
                        &hostnames,
                        target,
                        auto,
                        &default_label,
                    )?;
                    Ok((previous, converted, claims))
                })
            })
            .await
            .result;
        let (
            previous,
            converted,
            SniClaims {
                enabled_hostnames,
                leased_hostnames,
            },
        ) = persisted.map_err(|_| crate::net::port_map::pcp::hostname::RESULT_HOSTNAME_TAKEN)?;
        let fallback_target = match &previous {
            Some(PortForward::Dnat {
                target,
                enabled: true,
                ..
            }) => Some(*target),
            _ => None,
        };
        if converted {
            let restore = previous.clone();
            register_converted_sni(
                &self.sni,
                source,
                target,
                &enabled_hostnames,
                protocol,
                fallback_target,
                || self.restore_persisted_forward(source, restore),
            )
            .await?;
        } else if let Err(code) = self.sni.register_protocol(
            protocol,
            *source.ip(),
            source.port(),
            &enabled_hostnames,
            target,
            None,
        ) {
            self.restore_persisted_forward(source, previous).await;
            return Err(code);
        }
        if converted {
            if let Some(rc) = self.active_forwards.mutate(|m| m.remove(&source)) {
                drop(rc);
                self.forward.gc().await.log_err();
            }
            self.leases
                .mutate(|leases| lease::convert_dnat_to_sni(leases, source));
        }
        if let Some(lt) = lifetime {
            for hostname in leased_hostnames {
                lease::stamp(
                    self,
                    LeaseKey::Sni {
                        source,
                        hostname,
                        protocol,
                    },
                    lt,
                );
            }
        }
        Ok(())
    }

    pub(in crate::tunnel) async fn restore_persisted_forward(
        &self,
        source: SocketAddrV4,
        previous: Option<PortForward>,
    ) {
        self.db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|forwards| {
                    match previous.clone() {
                        Some(forward) => restore_forward_entry(forwards, source, forward),
                        None => {
                            forwards.0.remove(&source);
                        }
                    }
                    Ok(())
                })
            })
            .await
            .result
            .log_err();
    }

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

    /// Plain forwards own one combined fallback, preserving DNAT's shared state.
    pub(in crate::tunnel) async fn persist_fallback_forward_locked(
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
        let (previous, (owner, mapping)) = self
            .db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    let entry = pf.0.get_mut(&source).ok_or_else(|| {
                        Error::new(eyre!(t!("tunnel.api.route-not-found")), ErrorKind::NotFound)
                    })?;
                    let previous = entry.clone();
                    let claimed = claim_sni_fallback(entry, target, auto, &default_label)?;
                    Ok((previous, claimed))
                })
            })
            .await
            .result
            .map_err(|_| crate::net::port_map::pcp::hostname::RESULT_HOSTNAME_TAKEN)?;
        if mapping.enabled {
            if let Err(code) =
                self.sni
                    .register_fallback_protocol(owner, *source.ip(), source.port(), target)
            {
                self.restore_persisted_forward(source, Some(previous)).await;
                return Err(code);
            }
        }
        if mapping.auto {
            if let Some(lt) = lifetime {
                lease::stamp(
                    self,
                    LeaseKey::SniFallback {
                        source,
                        protocol: owner,
                    },
                    lt,
                );
            }
        }
        Ok(())
    }

    pub async fn remove_sni_fallback(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        protocol: impl Into<MappingProtocol>,
    ) {
        let _guard = self.forward_write_lock.lock().await;
        self.remove_sni_fallback_locked(source, target, protocol.into())
            .await;
    }

    pub(in crate::tunnel) async fn remove_sni_fallback_locked(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        protocol: MappingProtocol,
    ) {
        let removed = self
            .db
            .mutate(|db| {
                db.as_port_forwards_mut()
                    .mutate(|pf| Ok(remove_owned_sni_fallback(pf, source, target, protocol)))
            })
            .await
            .result
            .log_err()
            .unwrap_or(false);
        if removed {
            self.sni
                .unregister_fallback_protocol(protocol, *source.ip(), source.port(), target);
            lease::forget(self, &LeaseKey::SniFallback { source, protocol });
        }
    }
}

async fn register_converted_sni<F, Fut>(
    sni: &Arc<SniDemux>,
    source: SocketAddrV4,
    target: SocketAddrV4,
    hostnames: &[String],
    protocol: impl Into<MappingProtocol>,
    fallback_target: Option<SocketAddrV4>,
    restore: F,
) -> Result<(), u8>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let protocol = protocol.into();
    let mut fallback_registered = false;
    let result = (|| {
        if let Some(fallback) = fallback_target {
            sni.register_fallback_protocol(
                MappingProtocol::TcpUdp,
                *source.ip(),
                source.port(),
                fallback,
            )?;
            fallback_registered = true;
        }
        sni.register_protocol(
            protocol,
            *source.ip(),
            source.port(),
            hostnames,
            target,
            None,
        )
    })();
    if let Err(code) = result {
        if fallback_registered {
            sni.unregister_fallback_protocol(
                MappingProtocol::TcpUdp,
                *source.ip(),
                source.port(),
                fallback_target.unwrap(),
            );
        }
        restore().await;
        return Err(code);
    }
    Ok(())
}

fn remove_owned_sni_routes(
    pf: &mut PortForwards,
    source: SocketAddrV4,
    target: SocketAddrV4,
    hostnames: &[String],
    protocol: impl Into<MappingProtocol>,
    auto_only: bool,
) -> Vec<String> {
    let mut removed = Vec::new();
    if let Some(entry) = pf.0.get_mut(&source) {
        if let Some(bindings) = entry.bindings_mut(protocol) {
            bindings.routes.retain(|hostname, route| {
                let remove = route.target == target
                    && hostnames.contains(hostname)
                    && (!auto_only || route.auto);
                if remove {
                    removed.push(hostname.clone());
                }
                !remove
            });
        }
        if entry.sni_is_empty() {
            pf.0.remove(&source);
        }
    }

    removed
}

fn remove_owned_sni_fallback(
    pf: &mut PortForwards,
    source: SocketAddrV4,
    target: SocketAddrV4,
    protocol: impl Into<MappingProtocol>,
) -> bool {
    let mut removed = false;
    if let Some(entry) = pf.0.get_mut(&source) {
        if let Some(bindings) = entry.bindings_mut(protocol) {
            if bindings
                .fallback
                .as_ref()
                .is_some_and(|f| f.target == target)
            {
                bindings.fallback = None;
                removed = true;
            }
        }
        if entry.sni_is_empty() {
            pf.0.remove(&source);
        }
    }

    removed
}

struct SniClaims {
    enabled_hostnames: Vec<String>,
    leased_hostnames: Vec<String>,
}

fn claim_sni_routes(
    entry: &mut PortForward,
    protocol: impl Into<MappingProtocol>,
    hostnames: &[String],
    target: SocketAddrV4,
    auto: bool,
    default_label: &Option<String>,
) -> Result<SniClaims, Error> {
    let protocol = protocol.into();
    let mut claimed = Vec::new();
    for h in hostnames {
        if let Some((owner, route)) = entry.overlapping_route(protocol, Some(h)) {
            if auto && owner == MappingProtocol::TcpUdp && manual_route_matches(route, target) {
                continue;
            }
            if owner != protocol {
                return Err(Error::new(
                    eyre!(t!(
                        "tunnel.forward.hostname-protocol-conflict",
                        hostname = h
                    )),
                    ErrorKind::InvalidRequest,
                ));
            }
            if route_conflicts(route, target, auto) {
                return Err(Error::new(
                    eyre!(t!("tunnel.forward.hostname-held", hostname = h)),
                    ErrorKind::InvalidRequest,
                ));
            }
        }
        claimed.push(h);
    }
    let routes = &mut entry.bindings_mut(protocol).unwrap().routes;
    let mut claims = SniClaims {
        enabled_hostnames: Vec::new(),
        leased_hostnames: Vec::new(),
    };
    for h in claimed {
        let (label, enabled, auto) = sni_route_fields(routes.get(h), auto, default_label);
        if enabled {
            claims.enabled_hostnames.push(h.clone());
        }
        if auto {
            claims.leased_hostnames.push(h.clone());
        }
        routes.insert(
            h.clone(),
            crate::tunnel::db::SniRoute {
                target,
                label,
                enabled,
                auto,
            },
        );
    }
    Ok(claims)
}

fn claim_sni_fallback(
    entry: &mut PortForward,
    target: SocketAddrV4,
    auto: bool,
    default_label: &Option<String>,
) -> Result<(MappingProtocol, crate::tunnel::db::SniRoute), Error> {
    let mut owner = MappingProtocol::TcpUdp;
    if let Some((protocol, route)) = entry.overlapping_route(MappingProtocol::TcpUdp, None) {
        let legacy_renewal = protocol != MappingProtocol::TcpUdp && route.target == target;
        if !legacy_renewal
            && (protocol != MappingProtocol::TcpUdp || route_conflicts(route, target, auto))
        {
            return Err(Error::new(
                eyre!(t!("tunnel.forward.fallback-conflict")),
                ErrorKind::InvalidRequest,
            ));
        }
        if auto && manual_route_matches(route, target) {
            return Ok((protocol, route.clone()));
        }
        owner = protocol;
    }
    let fallback = &mut entry.bindings_mut(owner).unwrap().fallback;
    let (label, enabled, auto) = sni_route_fields(fallback.as_ref(), auto, default_label);
    let mapping = crate::tunnel::db::SniRoute {
        target,
        label,
        enabled,
        auto,
    };
    *fallback = Some(mapping.clone());
    Ok((owner, mapping))
}

fn restore_forward_entry(forwards: &mut PortForwards, source: SocketAddrV4, forward: PortForward) {
    forwards.0.insert(source, forward);
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

fn manual_route_matches(route: &crate::tunnel::db::SniRoute, target: SocketAddrV4) -> bool {
    !route.auto && route.target == target
}

/// A route's owner repoints it within its own device. Any other change conflicts.
fn route_conflicts(route: &crate::tunnel::db::SniRoute, target: SocketAddrV4, auto: bool) -> bool {
    route.target != target && (route.auto != auto || route.target.ip() != target.ip())
}

/// Retains ownership and disabled intent; an unset label inherits the default.
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
        PortForward::Sni { .. } => entry.sni_bindings().any(|(_, b)| {
            b.fallback
                .as_ref()
                .is_some_and(|f| f.auto && &f.target == target)
        }),
    }
}

fn peer_forward_source(
    forwards: &PortForwards,
    target: &SocketAddrV4,
) -> Option<(SocketAddrV4, bool)> {
    forwards
        .0
        .iter()
        .find(|(_, entry)| peer_forward_matches(entry, target))
        .map(|(source, entry)| (*source, matches!(entry, PortForward::Sni { .. })))
}

/// Remove the peer's forward to `(peer, internal_port)`, if any. We forward both
/// protocols on one entry, so match by target rather than PCP's (proto, port, client).
async fn remove_peer_forward(ctx: &TunnelContext, peer: Ipv4Addr, internal_port: u16) {
    let _guard = ctx.forward_write_lock.lock().await;
    let target = SocketAddrV4::new(peer, internal_port);
    let source = ctx
        .db
        .peek()
        .await
        .as_port_forwards()
        .de()
        .ok()
        .and_then(|pf| peer_forward_source(&pf, &target));
    let Some((source, is_sni)) = source else {
        return;
    };
    if is_sni {
        if let Some(entry) = current_forward(ctx, source).await {
            for (protocol, bindings) in entry.sni_bindings() {
                if bindings
                    .fallback
                    .as_ref()
                    .is_some_and(|f| f.auto && f.target == target)
                {
                    ctx.remove_sni_fallback_locked(source, target, protocol)
                        .await;
                }
            }
        }
        return;
    }
    ctx.db
        .mutate(|db| db.as_port_forwards_mut().remove(&source).map(|_| ()))
        .await
        .result
        .log_err();
    if let Some(rc) = ctx.active_forwards.mutate(|m| m.remove(&source)) {
        drop(rc);
        ctx.forward.gc().await.log_err();
    }
    lease::forget(ctx, &LeaseKey::Dnat(source));
}

/// The gateway-created forwards pointing at `peer`, as IGD mapping entries.
///
/// Scoped to `auto` forwards owned by the requesting peer: a manually added
/// forward is the operator's, not something this device created, and belongs no
/// more in its UPnP view than another device's would. SNI routes are skipped —
/// a hostname-demuxed route isn't expressible as a port mapping. A PORT_SET
/// range expands to one entry per port so a client asking about any port in the
/// range finds it, and each is reported for both transports, matching the
/// protocol-agnostic DNAT the tunnel actually installs.
///
/// `lease_seconds` is reported as `0` — "permanent" in IGD terms. The tunnel
/// tracks expiry in the lease map rather than on the forward itself, and the
/// mappings clients read back are overwhelmingly the lease-0 kind this is
/// exactly right for; a timed mapping is under-reported rather than wrong in a
/// way that would make a client drop it.
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
            out.extend(["TCP", "UDP"].into_iter().map(|protocol| MappingEntry {
                external_port,
                internal: SocketAddrV4::new(peer, internal_port),
                protocol,
                description: description.clone(),
                lease_seconds: 0,
            }));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddrV4;

    use super::{
        claim_sni_fallback, claim_sni_routes, mapping_entries, peer_forward_matches,
        peer_forward_source, plan_dnat_conversion, register_converted_sni,
        remove_owned_sni_fallback, remove_owned_sni_routes, restore_forward_entry, route_conflicts,
        sni_route_fields,
    };
    use crate::net::port_map::{Protocol as MappingProtocol, TransportProtocol};
    use crate::tunnel::db::{PortForward, PortForwards, SniRoute};

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
            both: Default::default(),
            tcp: crate::tunnel::db::SniBindings {
                routes: Default::default(),
                fallback: Some(SniRoute {
                    target,
                    label: None,
                    enabled: true,
                    auto,
                }),
            },
            udp: Default::default(),
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
                    both: Default::default(),
                    tcp: crate::tunnel::db::SniBindings {
                        routes: Default::default(),
                        fallback: None,
                    },
                    udp: Default::default()
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
            manual_source,
            owned_dnat("10.59.217.2:2525", 1, true, false),
        );
        forwards
            .0
            .insert(auto_source, owned_dnat("10.59.217.2:2525", 1, true, true));

        assert_eq!(
            peer_forward_source(&forwards, &target),
            Some((auto_source, false))
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
            both: Default::default(),
            tcp: crate::tunnel::db::SniBindings {
                routes: std::collections::BTreeMap::new(),
                fallback: None,
            },
            udp: Default::default(),
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
        let mut converted = dnat.clone();
        converted.convert_to_sni();
        forwards.0.insert(source, converted);
        let forwards = std::sync::Arc::new(std::sync::Mutex::new(forwards));
        let restore_forwards = forwards.clone();
        let sni = crate::tunnel::forward::sni::SniDemux::new();

        let code = register_converted_sni(
            &sni,
            source,
            target,
            &["blocked.example.com".to_string()],
            TransportProtocol::Tcp,
            Some(target),
            move || async move {
                restore_forward_entry(&mut restore_forwards.lock().unwrap(), source, dnat);
            },
        )
        .await
        .unwrap_err();
        assert_eq!(code, crate::net::port_map::pcp::RESULT_NO_RESOURCES);
        assert!(matches!(
            forwards.lock().unwrap().0.get(&source),
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
                .insert(source.parse::<SocketAddrV4>().unwrap(), f);
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
            "203.0.113.1:5000".parse::<SocketAddrV4>().unwrap(),
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
    #[test]
    fn same_hostname_has_independent_transport_owners_and_state() {
        let mut entry = PortForward::empty_sni();
        let hostname = vec!["service.example.com".to_owned()];
        let tcp = "10.59.0.2:443".parse().unwrap();
        let udp = "10.59.0.3:8443".parse().unwrap();
        claim_sni_routes(
            &mut entry,
            TransportProtocol::Tcp,
            &hostname,
            tcp,
            false,
            &Some("TLS".into()),
        )
        .unwrap();
        entry
            .bindings_mut(TransportProtocol::Tcp)
            .unwrap()
            .routes
            .get_mut(&hostname[0])
            .unwrap()
            .enabled = false;
        claim_sni_routes(
            &mut entry,
            TransportProtocol::Udp,
            &hostname,
            udp,
            true,
            &Some("QUIC".into()),
        )
        .unwrap();
        assert!(
            claim_sni_routes(
                &mut entry,
                TransportProtocol::Udp,
                &hostname,
                tcp,
                true,
                &None
            )
            .is_err()
        );
        let tcp_route = entry
            .bindings(TransportProtocol::Tcp)
            .unwrap()
            .route(Some(&hostname[0]))
            .unwrap();
        assert_eq!(tcp_route.target, tcp);
        assert_eq!(tcp_route.label.as_deref(), Some("TLS"));
        assert!(!tcp_route.enabled);
        assert!(!tcp_route.auto);
        let udp_route = entry
            .bindings(TransportProtocol::Udp)
            .unwrap()
            .route(Some(&hostname[0]))
            .unwrap();
        assert_eq!(udp_route.target, udp);
        assert_eq!(udp_route.label.as_deref(), Some("QUIC"));
        assert!(udp_route.enabled);
        assert!(udp_route.auto);
    }

    #[test]
    fn combined_claim_has_one_record_and_preserves_shared_manual_state() {
        let source = "203.0.113.1:443".parse().unwrap();
        let target = "10.59.0.2:443".parse().unwrap();
        let repointed = "10.59.0.2:8443".parse().unwrap();
        let hosts = vec!["service.example.com".to_owned()];
        let mut entry = PortForward::empty_sni();
        let claims = claim_sni_routes(
            &mut entry,
            MappingProtocol::TcpUdp,
            &hosts,
            target,
            false,
            &Some("HTTPS".into()),
        )
        .unwrap();
        assert_eq!(claims.enabled_hostnames, hosts);
        assert!(claims.leased_hostnames.is_empty());
        assert_eq!(entry.sni_routes().count(), 1);
        assert!(entry.bindings(MappingProtocol::Tcp).unwrap().is_empty());
        assert!(entry.bindings(MappingProtocol::Udp).unwrap().is_empty());
        entry
            .bindings_mut(MappingProtocol::TcpUdp)
            .unwrap()
            .route_mut(Some(&hosts[0]))
            .unwrap()
            .enabled = false;
        let claims = claim_sni_routes(
            &mut entry,
            MappingProtocol::TcpUdp,
            &hosts,
            repointed,
            false,
            &Some("replacement".into()),
        )
        .unwrap();
        assert!(claims.enabled_hostnames.is_empty());
        assert!(claims.leased_hostnames.is_empty());
        let encoded = serde_json::to_value(&entry).unwrap();
        assert_eq!(
            encoded["both"]["routes"][&hosts[0]]["target"],
            "10.59.0.2:8443"
        );
        assert_eq!(encoded["both"]["routes"][&hosts[0]]["label"], "HTTPS");
        assert_eq!(encoded["both"]["routes"][&hosts[0]]["enabled"], false);
        let mut forwards = PortForwards(std::collections::BTreeMap::from([(source, entry)]));
        assert!(
            remove_owned_sni_routes(
                &mut forwards,
                source,
                repointed,
                &hosts,
                MappingProtocol::Tcp,
                false
            )
            .is_empty()
        );
        assert!(
            remove_owned_sni_routes(
                &mut forwards,
                source,
                repointed,
                &hosts,
                MappingProtocol::TcpUdp,
                true
            )
            .is_empty()
        );
        assert_eq!(
            remove_owned_sni_routes(
                &mut forwards,
                source,
                repointed,
                &hosts,
                MappingProtocol::TcpUdp,
                false
            ),
            hosts
        );
        assert!(!forwards.0.contains_key(&source));
    }

    #[test]
    fn overlapping_logical_claims_fail_before_mutating_any_hostname() {
        let target = "10.59.0.2:443".parse().unwrap();
        for (owner, requested) in [
            (MappingProtocol::TcpUdp, MappingProtocol::Tcp),
            (MappingProtocol::TcpUdp, MappingProtocol::Udp),
            (MappingProtocol::Tcp, MappingProtocol::TcpUdp),
            (MappingProtocol::Udp, MappingProtocol::TcpUdp),
        ] {
            for enabled in [false, true] {
                for auto in [false, true] {
                    let mut entry = PortForward::empty_sni();
                    entry.bindings_mut(owner).unwrap().routes.insert(
                        "held.example.com".into(),
                        SniRoute {
                            target,
                            label: Some("owner".into()),
                            enabled,
                            auto,
                        },
                    );
                    let previous = serde_json::to_value(&entry).unwrap();
                    assert!(
                        claim_sni_routes(
                            &mut entry,
                            requested,
                            &["new.example.com".into(), "held.example.com".into()],
                            target,
                            false,
                            &None
                        )
                        .is_err()
                    );
                    assert_eq!(serde_json::to_value(&entry).unwrap(), previous);
                }
            }
        }
    }

    #[test]
    fn automatic_claims_reuse_manual_combined_intent_without_grants() {
        let target = "10.59.0.2:443".parse().unwrap();
        let hosts = vec!["service.example.com".to_owned()];
        for enabled in [false, true] {
            for transport in MappingProtocol::ALL {
                let mut entry = PortForward::empty_sni();
                claim_sni_routes(
                    &mut entry,
                    MappingProtocol::TcpUdp,
                    &hosts,
                    target,
                    false,
                    &None,
                )
                .unwrap();
                entry
                    .bindings_mut(MappingProtocol::TcpUdp)
                    .unwrap()
                    .route_mut(Some(&hosts[0]))
                    .unwrap()
                    .enabled = enabled;
                let previous = serde_json::to_value(&entry).unwrap();
                let claims = claim_sni_routes(
                    &mut entry,
                    transport,
                    &hosts,
                    target,
                    true,
                    &Some("automatic".into()),
                )
                .unwrap();
                assert!(claims.enabled_hostnames.is_empty());
                assert!(claims.leased_hostnames.is_empty());
                assert_eq!(serde_json::to_value(&entry).unwrap(), previous);
                assert!(
                    claim_sni_routes(
                        &mut entry,
                        transport,
                        &hosts,
                        "10.59.0.2:8443".parse().unwrap(),
                        true,
                        &None
                    )
                    .is_err()
                );
                assert_eq!(serde_json::to_value(&entry).unwrap(), previous);
            }
        }
    }

    #[test]
    fn combined_fallback_is_one_owned_record_and_rejects_split_fallbacks() {
        let target = "10.59.0.2:443".parse().unwrap();
        let mut entry = PortForward::empty_sni();
        let (owner, mapping) = claim_sni_fallback(&mut entry, target, false, &None).unwrap();
        assert_eq!(owner, MappingProtocol::TcpUdp);
        assert!(!mapping.auto);
        assert_eq!(entry.sni_routes().count(), 1);
        entry
            .bindings_mut(MappingProtocol::TcpUdp)
            .unwrap()
            .fallback
            .as_mut()
            .unwrap()
            .enabled = false;
        let (_, mapping) =
            claim_sni_fallback(&mut entry, target, true, &Some("automatic".into())).unwrap();
        assert!(mapping.label.is_none());
        assert!(!mapping.enabled);
        assert!(!mapping.auto);
        let other_device = "10.59.0.3:443".parse().unwrap();
        for transport in TransportProtocol::ALL {
            let mut entry = PortForward::empty_sni();
            entry.bindings_mut(transport).unwrap().fallback = Some(mapping.clone());
            let previous = serde_json::to_value(&entry).unwrap();
            assert!(claim_sni_fallback(&mut entry, other_device, false, &None).is_err());
            assert_eq!(serde_json::to_value(&entry).unwrap(), previous);
        }
    }

    #[test]
    fn legacy_single_transport_fallback_renews_under_its_owner() {
        let target = "10.59.0.2:443".parse().unwrap();
        let mut entry: PortForward = serde_json::from_value(serde_json::json!({
            "kind": "sni",
            "routes": {},
            "fallback": { "target": "10.59.0.2:443", "label": "Automatic", "enabled": true, "auto": true }
        }))
        .unwrap();
        let (owner, mapping) =
            claim_sni_fallback(&mut entry, target, true, &Some("Automatic".into())).unwrap();
        assert_eq!(owner, MappingProtocol::Tcp);
        assert!(mapping.auto);
        assert!(entry.bindings(MappingProtocol::TcpUdp).unwrap().fallback.is_none());
    }

    #[tokio::test]
    async fn blocked_udp_conversion_restores_dnat_and_releases_tcp_fallback() {
        let blocked = std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let source = match blocked.local_addr().unwrap() {
            std::net::SocketAddr::V4(source) => source,
            _ => unreachable!(),
        };
        let target = "10.59.0.2:443".parse().unwrap();
        let previous = dnat("10.59.0.2:443", 1);
        let mut converted = previous.clone();
        converted.convert_to_sni();
        let forwards = std::sync::Arc::new(std::sync::Mutex::new(PortForwards(
            std::collections::BTreeMap::from([(source, converted)]),
        )));
        let restored = forwards.clone();
        let sni = crate::tunnel::forward::sni::SniDemux::new();
        let code = register_converted_sni(
            &sni,
            source,
            target,
            &["quic.example.com".into()],
            TransportProtocol::Udp,
            Some(target),
            move || async move {
                restore_forward_entry(&mut restored.lock().unwrap(), source, previous);
            },
        )
        .await
        .unwrap_err();
        assert_eq!(code, crate::net::port_map::pcp::RESULT_NO_RESOURCES);
        assert!(
            matches!(forwards.lock().unwrap().0.get(&source), Some(PortForward::Dnat { target: t, .. }) if *t == target)
        );
        assert!(sni.snapshot().is_empty());
        tokio::task::yield_now().await;
        std::net::TcpListener::bind(source).expect("cancelled TCP listener releases its socket");
    }

    #[test]
    fn deleting_one_transport_preserves_the_other_routes_fallback_and_manual_owner() {
        let source = "203.0.113.1:443".parse().unwrap();
        let tcp = "10.59.0.2:443".parse().unwrap();
        let udp = "10.59.0.3:443".parse().unwrap();
        let hosts = vec!["service.example.com".to_owned()];
        let mut entry = PortForward::empty_sni();
        for (protocol, target, auto) in [
            (TransportProtocol::Tcp, tcp, false),
            (TransportProtocol::Udp, udp, true),
        ] {
            claim_sni_routes(&mut entry, protocol, &hosts, target, auto, &None).unwrap();
            entry.bindings_mut(protocol).unwrap().fallback = Some(SniRoute {
                target,
                label: None,
                enabled: true,
                auto,
            });
        }
        let mut forwards = PortForwards(std::collections::BTreeMap::from([(source, entry)]));
        assert!(
            remove_owned_sni_routes(
                &mut forwards,
                source,
                udp,
                &hosts,
                TransportProtocol::Tcp,
                false
            )
            .is_empty()
        );
        assert!(
            remove_owned_sni_routes(
                &mut forwards,
                source,
                tcp,
                &hosts,
                TransportProtocol::Tcp,
                true
            )
            .is_empty()
        );
        assert!(!remove_owned_sni_fallback(
            &mut forwards,
            source,
            udp,
            TransportProtocol::Tcp
        ));
        assert_eq!(
            remove_owned_sni_routes(
                &mut forwards,
                source,
                udp,
                &hosts,
                TransportProtocol::Udp,
                true
            ),
            hosts
        );
        assert!(remove_owned_sni_fallback(
            &mut forwards,
            source,
            udp,
            TransportProtocol::Udp
        ));
        let entry = &forwards.0[&source];
        assert!(entry.bindings(TransportProtocol::Udp).unwrap().is_empty());
        assert_eq!(
            entry.bindings(TransportProtocol::Tcp).unwrap().routes[&hosts[0]].target,
            tcp
        );
        assert!(
            entry
                .bindings(TransportProtocol::Tcp)
                .unwrap()
                .fallback
                .is_some()
        );
        remove_owned_sni_routes(
            &mut forwards,
            source,
            tcp,
            &hosts,
            TransportProtocol::Tcp,
            false,
        );
        assert!(forwards.0.contains_key(&source));
        remove_owned_sni_fallback(&mut forwards, source, tcp, TransportProtocol::Tcp);
        assert!(!forwards.0.contains_key(&source));
    }
}
