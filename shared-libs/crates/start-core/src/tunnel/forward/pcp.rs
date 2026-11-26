//! Server-side PCP for StartTunnel: the WireGuard-bound socket + serve loop and
//! the [`GatewayBackend`] impl mapping PCP forwards onto nftables + PatchDb. The
//! protocol core (RFC 6887 + HOSTNAME/PORT_SET extensions) lives in
//! [`crate::net::port_map::server`].
//!
//! The socket is `SO_BINDTODEVICE`-bound to the WireGuard interface, so the PCP
//! server is never reachable from the VPS's public interface.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use tokio::net::UdpSocket;

use crate::net::port_map::pcp::RESULT_NO_RESOURCES;
use crate::net::port_map::pcp::hostname::RESULT_HOSTNAME_TAKEN;
use crate::net::port_map::server::{GatewayBackend, MappingEntry, PCP_PORT, handle, handle6};
use crate::net::transport::{Transport, TransportProtocol};
use crate::prelude::*;
use crate::tunnel::context::TunnelContext;
use crate::tunnel::db::{PortForward, PortForwards, SniRoute};
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
            .map_err(|err| err.pcp_code())
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

#[derive(Debug)]
pub struct SniForwardError {
    pcp_code: u8,
    error: Error,
}

impl SniForwardError {
    fn hostname_taken(error: Error) -> Self {
        Self {
            pcp_code: RESULT_HOSTNAME_TAKEN,
            error,
        }
    }

    fn no_resources(error: Error) -> Self {
        Self {
            pcp_code: RESULT_NO_RESOURCES,
            error,
        }
    }

    pub fn pcp_code(&self) -> u8 {
        self.pcp_code
    }

    pub(super) fn igd_code(&self) -> u16 {
        if self.error.kind == ErrorKind::InvalidRequest {
            718
        } else {
            501
        }
    }
}

impl From<SniForwardError> for Error {
    fn from(err: SniForwardError) -> Self {
        err.error
    }
}

pub(in crate::tunnel) fn sni_prepare_error(code: u8) -> SniForwardError {
    SniForwardError {
        pcp_code: code,
        error: Error::new(
            eyre!(t!("tunnel.forward.pcp.diversion-setup-failed", code = code)),
            ErrorKind::Network,
        ),
    }
}

pub(in crate::tunnel) fn sni_demux_error(
    source: SocketAddrV4,
    hostname: Option<&str>,
    code: u8,
) -> SniForwardError {
    if code == RESULT_HOSTNAME_TAKEN {
        let msg = match hostname {
            Some(hostname) => t!(
                "tunnel.forward.pcp.hostname-held-by-other-client",
                hostname = hostname,
                source = source
            ),
            None => t!("tunnel.forward.pcp.sni-hostname-taken", source = source),
        };
        SniForwardError::hostname_taken(Error::new(eyre!("{msg}"), ErrorKind::InvalidRequest))
    } else {
        SniForwardError {
            pcp_code: code,
            error: Error::new(
                eyre!(t!(
                    "tunnel.forward.pcp.sni-listener-failed",
                    source = source
                )),
                ErrorKind::Network,
            ),
        }
    }
}

fn sni_fallback_error(source: SocketAddrV4, code: u8) -> SniForwardError {
    if code == RESULT_HOSTNAME_TAKEN {
        SniForwardError::no_resources(Error::new(
            eyre!(t!(
                "tunnel.forward.pcp.fallback-held-by-other-client",
                source = source
            )),
            ErrorKind::InvalidRequest,
        ))
    } else {
        SniForwardError {
            pcp_code: code,
            error: Error::new(
                eyre!(t!(
                    "tunnel.forward.pcp.sni-listener-failed",
                    source = source
                )),
                ErrorKind::Network,
            ),
        }
    }
}

struct StagedSniForward {
    converted: bool,
    previous_key: ForwardAddress,
    previous: Option<PortForward>,
    staged: PortForward,
    enabled_hostnames: Vec<String>,
}

fn stage_sni_forward(
    pf: &mut PortForwards,
    source: SocketAddrV4,
    target: SocketAddrV4,
    hostnames: &[String],
    auto: bool,
    default_label: &Option<String>,
) -> Result<StagedSniForward, SniForwardError> {
    let tcp_key = ForwardAddress::new(source, Transport::tcp());
    let previous_key = if pf.0.contains_key(&tcp_key) {
        tcp_key.clone()
    } else {
        source.into()
    };
    let previous = pf.0.get(&previous_key).cloned();
    if let Some(conflict) = pf
        .overlapping(tcp_key, 1)
        .filter(|key| *key != previous_key)
    {
        return Err(SniForwardError::no_resources(Error::new(
            eyre!(t!(
                "tunnel.forward.pcp.overlaps-existing",
                source = source,
                conflict = conflict
            )),
            ErrorKind::InvalidRequest,
        )));
    }
    let convert = plan_dnat_conversion(pf.0.get(&previous_key), source, target)
        .map_err(SniForwardError::no_resources)?;
    if !convert {
        match pf.0.get(&source.into()) {
            Some(PortForward::Sni { routes, .. }) => {
                for h in hostnames {
                    if let Some(existing) =
                        routes.get(h).filter(|r| route_conflicts(r, target, auto))
                    {
                        if existing.target.ip() != target.ip() {
                            return Err(SniForwardError::hostname_taken(Error::new(
                                eyre!(t!(
                                    "tunnel.forward.pcp.hostname-held-by-other-client",
                                    hostname = h,
                                    source = source
                                )),
                                ErrorKind::InvalidRequest,
                            )));
                        }
                        let msg = if existing.auto {
                            t!(
                                "tunnel.forward.pcp.hostname-automatic-route",
                                hostname = h,
                                source = source,
                                target = existing.target
                            )
                        } else {
                            t!(
                                "tunnel.forward.pcp.hostname-manual-route",
                                hostname = h,
                                source = source,
                                target = existing.target
                            )
                        };
                        return Err(SniForwardError::no_resources(Error::new(
                            eyre!("{msg}"),
                            ErrorKind::InvalidRequest,
                        )));
                    }
                }
            }
            Some(PortForward::Dnat { .. }) => {
                return Err(SniForwardError::no_resources(Error::new(
                    eyre!(t!("tunnel.forward.pcp.already-dnat", source = source)),
                    ErrorKind::InvalidRequest,
                )));
            }
            None => {}
        }
    }
    let mut converted = false;
    if convert {
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
            if previous_key.transport == Transport::default() {
                pf.0.insert(ForwardAddress::new(source, Transport::udp()), dnat.clone());
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
            converted = true;
        }
    }
    let entry =
        pf.0.entry(source.into())
            .or_insert_with(|| PortForward::Sni {
                routes: std::collections::BTreeMap::new(),
                fallback: None,
            });
    let PortForward::Sni { routes, .. } = entry else {
        unreachable!()
    };
    let mut enabled_hostnames = Vec::new();
    for h in hostnames {
        let (label, enabled, auto) = sni_route_fields(routes.get(h), auto, default_label);
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
    let staged = entry.clone();
    Ok(StagedSniForward {
        converted,
        previous_key,
        previous,
        staged,
        enabled_hostnames,
    })
}

fn stage_sni_fallback(
    pf: &mut PortForwards,
    source: SocketAddrV4,
    target: SocketAddrV4,
    auto: bool,
    default_label: &Option<String>,
) -> Result<(bool, Option<SniRoute>, SniRoute), SniForwardError> {
    match pf.0.get_mut(&source.into()) {
        Some(PortForward::Sni { fallback, .. }) => {
            if let Some(existing) = fallback
                .as_ref()
                .filter(|f| route_conflicts(f, target, auto))
            {
                let msg = if existing.target.ip() != target.ip() {
                    t!(
                        "tunnel.forward.pcp.fallback-held-by-other-client",
                        source = source
                    )
                } else if existing.auto {
                    t!(
                        "tunnel.forward.pcp.fallback-automatic-route",
                        source = source,
                        target = existing.target
                    )
                } else {
                    t!(
                        "tunnel.forward.pcp.fallback-manual-route",
                        source = source,
                        target = existing.target
                    )
                };
                return Err(SniForwardError::no_resources(Error::new(
                    eyre!("{msg}"),
                    ErrorKind::InvalidRequest,
                )));
            }
            let previous = fallback.clone();
            let (label, enabled, auto) = sni_route_fields(fallback.as_ref(), auto, default_label);
            let staged = SniRoute {
                target,
                label,
                enabled,
                auto,
            };
            *fallback = Some(staged.clone());
            Ok((enabled, previous, staged))
        }
        _ => Err(SniForwardError::no_resources(Error::new(
            eyre!(t!("tunnel.forward.pcp.not-sni-port", source = source)),
            ErrorKind::InvalidRequest,
        ))),
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
        let key = ForwardAddress::new(source, transport.clone());
        let entry =
            current_forward(self, key.clone())
                .await
                .or(current_forward(self, source.into()).await.filter(|e| {
                    matches!(e, PortForward::Sni { .. }) && transport != Transport::udp()
                }));
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
    ) -> Result<(), SniForwardError> {
        let _guard = self.forward_write_lock.lock().await;
        self.sni.prepare().await.map_err(sni_prepare_error)?;
        let default_label = if auto {
            Some("Automatic".to_string())
        } else {
            label
        };
        // Reject conflicts before displacing a working DNAT.
        let hostnames_owned = hostnames.to_vec();
        let err_code = AtomicU8::new(RESULT_NO_RESOURCES);
        let persisted = self
            .db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    stage_sni_forward(pf, source, target, &hostnames_owned, auto, &default_label)
                        .map_err(|err| {
                            err_code.store(err.pcp_code(), Ordering::Relaxed);
                            Error::from(err)
                        })
                })
            })
            .await
            .result;
        let StagedSniForward {
            converted,
            previous_key,
            previous,
            staged,
            enabled_hostnames,
        } = match persisted {
            Ok(staged) => staged,
            Err(error) => {
                return Err(SniForwardError {
                    pcp_code: err_code.load(Ordering::Relaxed),
                    error,
                });
            }
        };
        let converted_dnat = converted
            .then_some(previous.as_ref())
            .flatten()
            .and_then(|forward| match forward {
                PortForward::Dnat {
                    target, enabled, ..
                } => Some((*target, *enabled)),
                PortForward::Sni { .. } => None,
            });
        let single_hostname = (enabled_hostnames.len() == 1).then(|| enabled_hostnames[0].as_str());
        if let Some((dnat_target, dnat_enabled)) = converted_dnat {
            register_converted_sni(
                &self.sni,
                source,
                target,
                &enabled_hostnames,
                dnat_enabled.then_some(dnat_target),
                || self.restore_persisted_forward(previous_key.clone(), previous.clone(), &staged),
            )
            .await?;
        } else if let Err(code) = self.sni.register(
            *source.ip(),
            source.port(),
            &enabled_hostnames,
            target,
            None,
        ) {
            self.restore_persisted_forward(previous_key, previous, &staged)
                .await;
            return Err(sni_demux_error(source, single_hostname, code));
        }
        if let Some((dnat_target, dnat_enabled)) = converted_dnat {
            let udp_rc = if previous_key.transport == Transport::default() && dnat_enabled {
                let prefix = crate::tunnel::forward::igd::prefix_for(self, dnat_target.ip()).await;
                match self
                    .forward
                    .add_forward_range(source, dnat_target, 1, prefix, None, Transport::udp())
                    .await
                {
                    Ok(rc) => Some(rc),
                    Err(err) => {
                        self.sni.unregister(
                            *source.ip(),
                            source.port(),
                            &enabled_hostnames,
                            target,
                        );
                        self.sni
                            .unregister_fallback(*source.ip(), source.port(), dnat_target);
                        self.restore_persisted_forward(previous_key, previous, &staged)
                            .await;
                        return Err(SniForwardError::no_resources(err));
                    }
                }
            } else {
                None
            };
            if let Some(rc) = udp_rc {
                self.active_forwards
                    .mutate(|m| m.insert(ForwardAddress::new(source, Transport::udp()), rc));
            }
            if let Some(rc) = self.active_forwards.mutate(|m| {
                m.remove(&ForwardAddress::new(source, Transport::tcp()))
                    .or_else(|| m.remove(&source.into()))
            }) {
                drop(rc);
                self.forward.gc().await.log_err();
            }
            let carried = self.leases.mutate(|l| {
                l.remove(&LeaseKey::Dnat(ForwardAddress::new(
                    source,
                    Transport::tcp(),
                )))
            });
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
        expected_staged: &PortForward,
    ) {
        let expected_staged = expected_staged.clone();
        self.db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|forwards| {
                    restore_forward_entry(forwards, source, previous.clone(), &expected_staged);
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
    ) -> Result<(), SniForwardError> {
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
    ) -> Result<(), SniForwardError> {
        self.sni.prepare().await.map_err(sni_prepare_error)?;
        let default_label = if auto {
            Some("Automatic".to_string())
        } else {
            label
        };
        let persisted = self
            .db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    stage_sni_fallback(pf, source, target, auto, &default_label)
                        .map_err(Error::from)
                })
            })
            .await
            .result;
        let (enabled, previous, staged) = persisted.map_err(SniForwardError::no_resources)?;
        if enabled {
            if let Err(code) = self
                .sni
                .register_fallback(*source.ip(), source.port(), target)
            {
                self.restore_sni_fallback_locked(source, previous, &staged)
                    .await;
                return Err(sni_fallback_error(source, code));
            }
        }
        if let Some(lt) = lifetime {
            lease::stamp(self, LeaseKey::SniFallback(source), lt);
        }
        Ok(())
    }

    async fn restore_sni_fallback_locked(
        &self,
        source: SocketAddrV4,
        previous: Option<SniRoute>,
        expected_staged: &SniRoute,
    ) {
        let expected_staged = expected_staged.clone();
        self.db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    let mut now_empty = false;
                    if let Some(PortForward::Sni { routes, fallback }) =
                        pf.0.get_mut(&source.into())
                    {
                        if fallback.as_ref() == Some(&expected_staged) {
                            *fallback = previous.clone();
                            now_empty = routes.is_empty() && fallback.is_none();
                        }
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
) -> Result<(), SniForwardError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    if let Some(fallback_target) = fallback_target {
        if let Err(code) = sni.register_fallback(*source.ip(), source.port(), fallback_target) {
            restore().await;
            return Err(sni_fallback_error(source, code));
        }
    }
    if let Err(code) = sni.register(*source.ip(), source.port(), hostnames, target, None) {
        if let Some(fallback_target) = fallback_target {
            sni.unregister_fallback(*source.ip(), source.port(), fallback_target);
        }
        restore().await;
        let single_hostname = (hostnames.len() == 1).then(|| hostnames[0].as_str());
        return Err(sni_demux_error(source, single_hostname, code));
    }
    Ok(())
}

fn restore_forward_entry(
    forwards: &mut PortForwards,
    source: impl Into<ForwardAddress>,
    previous: Option<PortForward>,
    expected_staged: &PortForward,
) -> bool {
    let source = source.into();
    if forwards.0.get(&source.address.into()) != Some(expected_staged) {
        return false;
    }
    forwards.0.remove(&source.address.into());
    if source.transport == Transport::default()
        && matches!(&previous, Some(PortForward::Dnat { .. }))
    {
        let udp_key = ForwardAddress::new(source.address, Transport::udp());
        if forwards.0.get(&udp_key) == previous.as_ref() {
            forwards.0.remove(&udp_key);
        }
    }
    match previous {
        Some(forward) => {
            forwards.0.insert(source, forward);
        }
        None => {
            forwards.0.remove(&source);
        }
    }
    true
}

fn plan_dnat_conversion(
    existing: Option<&PortForward>,
    source: SocketAddrV4,
    new_target: SocketAddrV4,
) -> Result<bool, Error> {
    match existing {
        Some(PortForward::Dnat {
            count,
            target,
            auto,
            ..
        }) => {
            if *count != 1 {
                let msg = if *auto {
                    t!(
                        "tunnel.forward.pcp.already-auto-dnat-range",
                        source = source
                    )
                } else {
                    t!("tunnel.forward.pcp.already-dnat-range", source = source)
                };
                return Err(Error::new(eyre!("{msg}"), ErrorKind::InvalidRequest));
            }
            if target.ip() != new_target.ip() {
                let msg = if *auto {
                    t!(
                        "tunnel.forward.pcp.already-auto-dnat-other-client",
                        source = source
                    )
                } else {
                    t!(
                        "tunnel.forward.pcp.already-dnat-other-client",
                        source = source
                    )
                };
                return Err(Error::new(eyre!("{msg}"), ErrorKind::InvalidRequest));
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
            entry
                .transport((**source).clone())
                .protocols()
                .contains(&transport)
                && peer_forward_matches(entry, target)
        })
        .map(|(source, entry)| (source.clone(), matches!(entry, PortForward::Sni { .. })))
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
                    .transport(source.clone())
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
        register_converted_sni, restore_forward_entry, route_conflicts, sni_demux_error,
        sni_fallback_error, sni_prepare_error, sni_route_fields, stage_sni_fallback,
        stage_sni_forward,
    };
    use crate::net::port_map::pcp::RESULT_NO_RESOURCES;
    use crate::net::port_map::pcp::hostname::RESULT_HOSTNAME_TAKEN;
    use crate::net::port_map::server::GatewayBackend;
    use crate::net::transport::{Transport, TransportProtocol};
    use crate::prelude::*;
    use crate::tunnel::context::TunnelContext;
    use crate::tunnel::db::{PortForward, PortForwards, SniRoute, TunnelDatabase};
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
        let staged = PortForward::Sni {
            routes: Default::default(),
            fallback: Some(SniRoute {
                target,
                label: None,
                enabled: true,
                auto: true,
            }),
        };
        let mut forwards = PortForwards(Default::default());
        forwards.0.insert(source.into(), staged.clone());
        let forwards = std::sync::Arc::new(std::sync::Mutex::new(forwards));
        let restore_forwards = forwards.clone();
        let sni = crate::tunnel::forward::sni::SniDemux::new();

        let err = register_converted_sni(
            &sni,
            source,
            target,
            &["blocked.example.com".to_string()],
            Some(target),
            move || async move {
                restore_forward_entry(
                    &mut restore_forwards.lock().unwrap(),
                    source,
                    Some(dnat),
                    &staged,
                );
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.pcp_code(), RESULT_NO_RESOURCES);
        assert_eq!(Error::from(err).kind, ErrorKind::Network);
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
        let tcp = ForwardAddress::new(source, Transport::tcp());
        let udp = ForwardAddress::new(source, Transport::udp());
        let target: SocketAddrV4 = "10.59.0.2:6000".parse().unwrap();
        let mut forwards = PortForwards(Default::default());
        forwards
            .0
            .insert(tcp.clone(), owned_dnat(&target.to_string(), 2, true, true));
        forwards
            .0
            .insert(udp.clone(), owned_dnat(&target.to_string(), 2, true, true));
        assert_eq!(
            peer_forward_source(&forwards, &target, TransportProtocol::Tcp),
            Some((tcp.clone(), false))
        );
        assert_eq!(
            peer_forward_source(&forwards, &target, TransportProtocol::Udp),
            Some((udp.clone(), false))
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
            Some((udp.clone(), false))
        );
    }

    #[test]
    fn failed_sni_registration_does_not_remove_an_independent_udp_mapping() {
        let source: SocketAddrV4 = "203.0.113.1:443".parse().unwrap();
        let udp = ForwardAddress::new(source, Transport::udp());
        let mut forwards = PortForwards(Default::default());
        forwards
            .0
            .insert(udp.clone(), owned_dnat("10.59.0.2:443", 1, true, true));
        let sni = PortForward::Sni {
            routes: Default::default(),
            fallback: Some(route(None, true, true)),
        };
        forwards.0.insert(source.into(), sni.clone());
        assert!(restore_forward_entry(
            &mut forwards,
            source,
            Some(sni.clone()),
            &sni,
        ));
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

    fn localhost_v4_listener() -> (std::net::TcpListener, SocketAddrV4) {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = match listener.local_addr().unwrap() {
            std::net::SocketAddr::V4(addr) => addr,
            std::net::SocketAddr::V6(_) => unreachable!(),
        };
        (listener, addr)
    }

    fn assert_err(err: super::SniForwardError, pcp: u8, kind: ErrorKind, msg: &str) {
        assert_eq!(err.pcp_code(), pcp);
        let err = Error::from(err);
        assert_eq!(err.kind, kind);
        assert_eq!(err.source.to_string(), msg);
    }

    #[test]
    fn stage_sni_forward_reports_distinct_conflict_errors_and_pcp_codes() {
        let src: SocketAddrV4 = "203.0.113.1:443".parse().unwrap();
        let client_a: SocketAddrV4 = "10.59.0.2:443".parse().unwrap();
        let client_b: SocketAddrV4 = "10.59.0.3:443".parse().unwrap();
        let host = vec!["app.example.com".to_string()];

        let mut forwards = PortForwards(Default::default());
        forwards.0.insert(
            "203.0.113.1:440".parse::<SocketAddrV4>().unwrap().into(),
            owned_dnat("10.59.0.2:440", 4, true, false),
        );
        assert_err(
            stage_sni_forward(&mut forwards, src, client_b, &host, false, &None)
                .err()
                .unwrap(),
            RESULT_NO_RESOURCES,
            ErrorKind::InvalidRequest,
            "203.0.113.1:443 overlaps an existing forward at 203.0.113.1:440",
        );
        assert_eq!(forwards.0.len(), 1);

        let mut forwards = PortForwards(Default::default());
        forwards.0.insert(
            ForwardAddress::new(src, Transport::tcp()),
            owned_dnat("10.59.0.2:443", 4, true, true),
        );
        assert_err(
            stage_sni_forward(&mut forwards, src, client_a, &host, false, &None)
                .err()
                .unwrap(),
            RESULT_NO_RESOURCES,
            ErrorKind::InvalidRequest,
            "203.0.113.1:443 is already an automatic DNAT range forward",
        );

        let mut forwards = PortForwards(Default::default());
        forwards
            .0
            .insert(src.into(), owned_dnat("10.59.0.2:443", 4, true, false));
        assert_err(
            stage_sni_forward(&mut forwards, src, client_a, &host, false, &None)
                .err()
                .unwrap(),
            RESULT_NO_RESOURCES,
            ErrorKind::InvalidRequest,
            "203.0.113.1:443 is already a DNAT range forward",
        );

        let mut forwards = PortForwards(Default::default());
        forwards.0.insert(
            ForwardAddress::new(src, Transport::tcp()),
            owned_dnat("10.59.0.2:443", 1, true, true),
        );
        assert_err(
            stage_sni_forward(&mut forwards, src, client_b, &host, false, &None)
                .err()
                .unwrap(),
            RESULT_NO_RESOURCES,
            ErrorKind::InvalidRequest,
            "203.0.113.1:443 is already an automatic DNAT forward for another client",
        );
        assert!(matches!(
            forwards.0.get(&ForwardAddress::new(src, Transport::tcp())),
            Some(PortForward::Dnat { target, .. }) if *target == client_a
        ));

        let mut forwards = PortForwards(Default::default());
        forwards
            .0
            .insert(src.into(), owned_dnat("10.59.0.2:443", 1, true, false));
        assert_err(
            stage_sni_forward(&mut forwards, src, client_b, &host, false, &None)
                .err()
                .unwrap(),
            RESULT_NO_RESOURCES,
            ErrorKind::InvalidRequest,
            "203.0.113.1:443 is already a DNAT forward for another client",
        );

        let mut forwards = PortForwards(Default::default());
        forwards.0.insert(
            src.into(),
            PortForward::Sni {
                routes: std::collections::BTreeMap::from([(
                    "app.example.com".to_string(),
                    route(None, true, true),
                )]),
                fallback: None,
            },
        );
        assert_err(
            stage_sni_forward(
                &mut forwards,
                src,
                client_b,
                &[
                    "free.example.com".to_string(),
                    "app.example.com".to_string(),
                ],
                false,
                &None,
            )
            .err()
            .unwrap(),
            RESULT_HOSTNAME_TAKEN,
            ErrorKind::InvalidRequest,
            "SNI hostname app.example.com on 203.0.113.1:443 is held by another client",
        );
        let PortForward::Sni { routes, .. } = &forwards.0[&src.into()] else {
            unreachable!()
        };
        assert!(!routes.contains_key("free.example.com"));

        assert_err(
            stage_sni_forward(
                &mut forwards,
                src,
                "10.59.0.2:8443".parse().unwrap(),
                &host,
                false,
                &None,
            )
            .err()
            .unwrap(),
            RESULT_NO_RESOURCES,
            ErrorKind::InvalidRequest,
            "SNI hostname app.example.com on 203.0.113.1:443 is already an automatic route to 10.59.0.2:443",
        );

        let mut manual_forwards = PortForwards(Default::default());
        manual_forwards.0.insert(
            src.into(),
            PortForward::Sni {
                routes: std::collections::BTreeMap::from([(
                    "app.example.com".to_string(),
                    route(None, true, false),
                )]),
                fallback: None,
            },
        );
        assert_err(
            stage_sni_forward(
                &mut manual_forwards,
                src,
                "10.59.0.2:8443".parse().unwrap(),
                &host,
                true,
                &None,
            )
            .err()
            .unwrap(),
            RESULT_NO_RESOURCES,
            ErrorKind::InvalidRequest,
            "SNI hostname app.example.com on 203.0.113.1:443 is already a manual route to 10.59.0.2:443",
        );
    }

    #[test]
    fn stage_sni_fallback_reports_distinct_errors_and_igd_codes() {
        let src: SocketAddrV4 = "203.0.113.1:443".parse().unwrap();
        let client_b: SocketAddrV4 = "10.59.0.3:443".parse().unwrap();

        let mut empty = PortForwards(Default::default());
        let err = stage_sni_fallback(&mut empty, src, client_b, true, &None)
            .err()
            .unwrap();
        assert_eq!(err.igd_code(), 718);
        assert_err(
            err,
            RESULT_NO_RESOURCES,
            ErrorKind::InvalidRequest,
            "203.0.113.1:443 is not an SNI-demuxed port",
        );

        let mut with_fallback = PortForwards(Default::default());
        with_fallback.0.insert(
            src.into(),
            PortForward::Sni {
                routes: Default::default(),
                fallback: Some(route(None, true, true)),
            },
        );
        let err = stage_sni_fallback(&mut with_fallback, src, client_b, true, &None)
            .err()
            .unwrap();
        assert_eq!(err.igd_code(), 718);
        assert_err(
            err,
            RESULT_NO_RESOURCES,
            ErrorKind::InvalidRequest,
            "fallback on 203.0.113.1:443 is held by another client",
        );

        let err = stage_sni_fallback(
            &mut with_fallback,
            src,
            "10.59.0.2:8443".parse().unwrap(),
            false,
            &None,
        )
        .err()
        .unwrap();
        assert_eq!(err.igd_code(), 718);
        assert_err(
            err,
            RESULT_NO_RESOURCES,
            ErrorKind::InvalidRequest,
            "fallback on 203.0.113.1:443 is already an automatic route to 10.59.0.2:443",
        );

        let bind_err = sni_fallback_error(src, RESULT_NO_RESOURCES);
        assert_eq!(bind_err.igd_code(), 501);
        assert_err(
            bind_err,
            RESULT_NO_RESOURCES,
            ErrorKind::Network,
            "SNI listener on 203.0.113.1:443 could not bind",
        );

        let taken_err = sni_fallback_error(src, RESULT_HOSTNAME_TAKEN);
        assert_eq!(taken_err.igd_code(), 718);
        assert_err(
            taken_err,
            RESULT_NO_RESOURCES,
            ErrorKind::InvalidRequest,
            "fallback on 203.0.113.1:443 is held by another client",
        );

        assert_err(
            sni_prepare_error(RESULT_NO_RESOURCES),
            RESULT_NO_RESOURCES,
            ErrorKind::Network,
            "SNI diversion setup failed (code 8)",
        );

        assert_err(
            sni_demux_error(src, Some("app.example.com"), RESULT_HOSTNAME_TAKEN),
            RESULT_HOSTNAME_TAKEN,
            ErrorKind::InvalidRequest,
            "SNI hostname app.example.com on 203.0.113.1:443 is held by another client",
        );
    }

    #[tokio::test]
    async fn converted_sni_fallback_conflict_restores_dnat_without_code_192() {
        let (probe, source) = localhost_v4_listener();
        drop(probe);
        let target: SocketAddrV4 = "10.59.0.2:443".parse().unwrap();
        let other: SocketAddrV4 = "10.59.0.3:443".parse().unwrap();
        let dnat = dnat("10.59.0.2:443", 1);
        let staged = PortForward::Sni {
            routes: Default::default(),
            fallback: Some(route(None, true, true)),
        };
        let mut forwards = PortForwards(Default::default());
        forwards.0.insert(source.into(), staged.clone());
        let forwards = std::sync::Arc::new(std::sync::Mutex::new(forwards));
        let restore_forwards = forwards.clone();
        let sni = crate::tunnel::forward::sni::SniDemux::new();
        sni.register_fallback(*source.ip(), source.port(), other)
            .unwrap();

        let err = register_converted_sni(
            &sni,
            source,
            target,
            &["app.example.com".to_string()],
            Some(target),
            move || async move {
                restore_forward_entry(
                    &mut restore_forwards.lock().unwrap(),
                    source,
                    Some(dnat),
                    &staged,
                );
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.pcp_code(), RESULT_NO_RESOURCES);
        let err = Error::from(err);
        assert_eq!(err.kind, ErrorKind::InvalidRequest);
        assert!(err.source.to_string().contains("fallback on "));
        assert!(matches!(
            forwards.lock().unwrap().0.get(&source.into()),
            Some(PortForward::Dnat { target: restored, .. }) if *restored == target
        ));
    }

    #[tokio::test]
    async fn persist_sni_forward_preserves_errors_for_api_and_maps_pcp_codes() {
        let mut initial = TunnelDatabase::default();
        let auto_dnat_port: SocketAddrV4 = "127.0.0.1:443".parse().unwrap();
        initial.port_forwards.0.insert(
            ForwardAddress::new(auto_dnat_port, Transport::tcp()),
            owned_dnat("10.59.0.2:443", 1, true, true),
        );
        let range_base: SocketAddrV4 = "127.0.0.1:5000".parse().unwrap();
        initial.port_forwards.0.insert(
            range_base.into(),
            owned_dnat("10.59.0.2:5000", 3, true, false),
        );
        let (_dir, ctx) = TunnelContext::for_test(initial).await;

        let client_a: SocketAddrV4 = "10.59.0.2:443".parse().unwrap();
        let client_b: SocketAddrV4 = "10.59.0.3:443".parse().unwrap();
        let hosts = vec!["b.example.com".to_string()];

        assert_err(
            ctx.persist_sni_forward(auto_dnat_port, client_b, &hosts, None, false, None)
                .await
                .unwrap_err(),
            RESULT_NO_RESOURCES,
            ErrorKind::InvalidRequest,
            "127.0.0.1:443 is already an automatic DNAT forward for another client",
        );
        assert_eq!(
            GatewayBackend::add_sni_forward(&ctx, auto_dnat_port, client_b, &hosts, Some(300))
                .await
                .unwrap_err(),
            RESULT_NO_RESOURCES
        );

        let overlap_src: SocketAddrV4 = "127.0.0.1:5001".parse().unwrap();
        assert_err(
            ctx.persist_sni_forward(overlap_src, client_b, &hosts, None, false, None)
                .await
                .unwrap_err(),
            RESULT_NO_RESOURCES,
            ErrorKind::InvalidRequest,
            "127.0.0.1:5001 overlaps an existing forward at 127.0.0.1:5000",
        );
        assert_eq!(
            GatewayBackend::add_sni_forward(&ctx, overlap_src, client_b, &hosts, Some(300))
                .await
                .unwrap_err(),
            RESULT_NO_RESOURCES
        );

        let (probe, free_src) = localhost_v4_listener();
        drop(probe);
        ctx.persist_sni_forward(free_src, client_a, &hosts, None, false, None)
            .await
            .unwrap();
        assert_err(
            ctx.persist_sni_forward(free_src, client_b, &hosts, None, false, None)
                .await
                .unwrap_err(),
            RESULT_HOSTNAME_TAKEN,
            ErrorKind::InvalidRequest,
            &format!("SNI hostname b.example.com on {free_src} is held by another client"),
        );
        assert_eq!(
            GatewayBackend::add_sni_forward(&ctx, free_src, client_b, &hosts, Some(300))
                .await
                .unwrap_err(),
            RESULT_HOSTNAME_TAKEN
        );
        assert_eq!(
            GatewayBackend::add_sni_forward(
                &ctx,
                free_src,
                "10.59.0.2:8443".parse().unwrap(),
                &hosts,
                Some(300),
            )
            .await
            .unwrap_err(),
            RESULT_NO_RESOURCES
        );

        // A failed fallback bind restores the prior fallback.
        let (_blocked, blocked_src) = localhost_v4_listener();
        let prior_fallback = route(Some("keep"), true, true);
        ctx.db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    pf.0.insert(
                        blocked_src.into(),
                        PortForward::Sni {
                            routes: std::collections::BTreeMap::from([(
                                "existing.example.com".to_string(),
                                route(None, false, false),
                            )]),
                            fallback: Some(prior_fallback.clone()),
                        },
                    );
                    Ok(())
                })
            })
            .await
            .result
            .unwrap();
        let err = ctx
            .persist_fallback_forward_locked(
                blocked_src,
                "10.59.0.2:8443".parse().unwrap(),
                None,
                true,
                None,
            )
            .await
            .unwrap_err();
        assert_eq!(err.igd_code(), 501);
        assert_err(
            err,
            RESULT_NO_RESOURCES,
            ErrorKind::Network,
            &format!("SNI listener on {blocked_src} could not bind"),
        );
        let pf = ctx.db.peek().await.as_port_forwards().de().unwrap();
        assert!(matches!(
            pf.0.get(&blocked_src.into()),
            Some(PortForward::Sni { fallback, .. }) if fallback.as_ref() == Some(&prior_fallback)
        ));

        ctx.db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    if let Some(PortForward::Sni { fallback, .. }) =
                        pf.0.get_mut(&blocked_src.into())
                    {
                        *fallback = None;
                    }
                    Ok(())
                })
            })
            .await
            .result
            .unwrap();
        let err = ctx
            .persist_fallback_forward_locked(blocked_src, client_a, None, true, None)
            .await
            .unwrap_err();
        assert_eq!(err.igd_code(), 501);
        let pf = ctx.db.peek().await.as_port_forwards().de().unwrap();
        assert!(matches!(
            pf.0.get(&blocked_src.into()),
            Some(PortForward::Sni { fallback: None, .. })
        ));
    }

    #[tokio::test]
    async fn converted_dnat_udp_failure_rolls_back_state() {
        let (probe, src) = localhost_v4_listener();
        drop(probe);
        let target: SocketAddrV4 = "10.59.0.2:443".parse().unwrap();
        let prior_dnat = owned_dnat("10.59.0.2:443", 1, true, true);
        let mut initial = TunnelDatabase::default();
        initial
            .port_forwards
            .0
            .insert(src.into(), prior_dnat.clone());
        let (_dir, ctx) = TunnelContext::for_test(initial).await;

        let active_handle = std::sync::Arc::new(());
        ctx.active_forwards
            .mutate(|m| m.insert(src.into(), active_handle.clone()));
        let dnat_lease_key = super::LeaseKey::Dnat(ForwardAddress::new(src, Transport::tcp()));
        let expiry = std::time::Instant::now() + std::time::Duration::from_secs(300);
        ctx.leases
            .mutate(|l| l.insert(dnat_lease_key.clone(), expiry));

        let err = ctx
            .persist_sni_forward(
                src,
                target,
                &["app.example.com".to_string()],
                Some(300),
                true,
                None,
            )
            .await
            .unwrap_err();
        assert_eq!(err.pcp_code(), RESULT_NO_RESOURCES);

        let pf = ctx.db.peek().await.as_port_forwards().de().unwrap();
        assert_eq!(pf.0.get(&src.into()), Some(&prior_dnat));
        assert!(
            !pf.0
                .contains_key(&ForwardAddress::new(src, Transport::udp()))
        );

        assert!(ctx.sni.snapshot().is_empty());
        assert!(
            ctx.sni
                .register_fallback(*src.ip(), src.port(), "10.59.0.3:443".parse().unwrap())
                .is_ok()
        );
        ctx.sni
            .unregister_fallback(*src.ip(), src.port(), "10.59.0.3:443".parse().unwrap());

        ctx.active_forwards.peek(|m| {
            let held = m.get(&src.into()).expect("prior active handle retained");
            assert!(std::sync::Arc::ptr_eq(held, &active_handle));
            assert!(!m.contains_key(&ForwardAddress::new(src, Transport::udp())));
        });

        ctx.leases.peek(|l| {
            assert_eq!(l.get(&dnat_lease_key), Some(&expiry));
            assert!(!l.contains_key(&super::LeaseKey::SniFallback(src)));
            assert!(!l.contains_key(&super::LeaseKey::Sni {
                source: src,
                hostname: "app.example.com".to_string(),
            }));
        });
    }

    #[tokio::test]
    async fn resync_forward_keys_serializes_with_forward_write_lock_and_cas_guards_restore() {
        let src: SocketAddrV4 = "203.0.113.1:443".parse().unwrap();
        let expected_staged = PortForward::Sni {
            routes: std::collections::BTreeMap::from([(
                "old.example.com".to_string(),
                route(None, true, true),
            )]),
            fallback: None,
        };
        let newer = PortForward::Sni {
            routes: std::collections::BTreeMap::from([(
                "new.example.com".to_string(),
                route(Some("kept"), true, false),
            )]),
            fallback: None,
        };
        let mut forwards = PortForwards(Default::default());
        forwards.0.insert(src.into(), newer.clone());
        assert!(!restore_forward_entry(
            &mut forwards,
            src,
            Some(dnat("10.59.0.2:443", 1)),
            &expected_staged,
        ));
        assert_eq!(forwards.0.get(&src.into()), Some(&newer));

        let (_dir, ctx) = TunnelContext::for_test(TunnelDatabase::default()).await;
        let guard = ctx.forward_write_lock.lock().await;
        let ctx_bg = ctx.clone();
        let mut resync = tokio::spawn(async move { ctx_bg.resync_forward_keys().await });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), &mut resync)
                .await
                .is_err()
        );
        drop(guard);
        resync.await.unwrap().unwrap();
    }
}
