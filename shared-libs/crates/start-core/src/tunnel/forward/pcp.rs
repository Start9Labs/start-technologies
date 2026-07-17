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

use crate::net::port_map::server::{GatewayBackend, PCP_PORT, handle, handle6};
use crate::prelude::*;
use crate::tunnel::context::TunnelContext;
use crate::tunnel::db::PortForward;
use crate::tunnel::forward::igd::{
    apply_peer_forward_range, bind_to_wireguard, external_ipv4, is_known_client,
};
use crate::tunnel::forward::lease::{self, LeaseKey};
use crate::tunnel::forward::sni::SniDemux;
use crate::tunnel::wg::WIREGUARD_INTERFACE_NAME;

/// Run the PCP server (IPv4 DNAT forwards + IPv6 GUA pinholes) for the life of
/// the tunnel, each family self-restarting on error and rebinding when the wg
/// interface's ifindex changes (an `Ok` return from the serve loop).
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

/// The v6 counterpart of [`socket`]: an IPv6-only UDP socket on the WireGuard
/// interface, so a client's PCP MAP for its own GUA reaches us over the tunnel.
fn socket6() -> Result<UdpSocket, Error> {
    let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))
        .with_kind(ErrorKind::Network)?;
    socket
        .set_reuse_address(true)
        .with_kind(ErrorKind::Network)?;
    // v4 requests go to the v4 socket; keep this one v6-only so both can bind :5351.
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
    // Subscribe before binding so a bounce during setup still triggers a rebind.
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
        // `apply_peer_forward_range` stamps the lease itself (Dnat on the DNAT
        // path, SniFallback when the port is SNI-demuxed and this becomes its
        // fallback), so a still-renewing client is never reaped whichever it is.
        apply_peer_forward_range(self, source, target, count, "PCP", lifetime).await
    }

    async fn remove_forward(&self, peer: Ipv4Addr, internal_port: u16) {
        remove_peer_forward(self, peer, internal_port).await
    }

    async fn remove_forward_by_source(&self, source: SocketAddrV4, peer: Ipv4Addr) -> bool {
        let owned = crate::tunnel::forward::igd::current_forward(self, source)
            .await
            .is_some_and(|e| matches!(e, PortForward::Dnat { target, .. } if *target.ip() == peer));
        if !owned {
            return false;
        }
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
        crate::tunnel::forward::pinhole::add_pinhole(
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
        if let Some(lt) = lifetime {
            lease::stamp(
                self,
                LeaseKey::Pinhole(SocketAddrV6::new(gua, external_port, 0, 0)),
                lt,
            );
        }
        Ok(())
    }

    async fn remove_pinhole(&self, gua: Ipv6Addr, external_port: u16) {
        crate::tunnel::forward::pinhole::remove_pinhole(self, gua, external_port).await;
        lease::forget(
            self,
            &LeaseKey::Pinhole(SocketAddrV6::new(gua, external_port, 0, 0)),
        );
    }

    fn sni(&self) -> &Arc<SniDemux> {
        &self.sni
    }

    async fn add_sni_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
        lifetime: Option<u32>,
    ) -> Result<(), u8> {
        // The PCP path owns its routes: mark them automatic, default label `PCP`.
        self.persist_sni_forward(source, target, hostnames, lifetime, true, None)
            .await
    }

    async fn remove_sni_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
    ) {
        self.sni()
            .unregister(*source.ip(), source.port(), hostnames, target);
        for h in hostnames {
            lease::forget(
                self,
                &LeaseKey::Sni {
                    source,
                    hostname: h.clone(),
                },
            );
        }
        let hostnames = hostnames.to_vec();
        self.db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    use crate::tunnel::db::PortForward;
                    let mut now_empty = false;
                    if let Some(PortForward::Sni { routes, fallback }) = pf.0.get_mut(&source) {
                        routes.retain(|h, r| !(r.target == target && hostnames.contains(h)));
                        now_empty = routes.is_empty() && fallback.is_none();
                    }
                    if now_empty {
                        pf.0.remove(&source);
                    }
                    Ok(())
                })
            })
            .await
            .result
            .log_err();
    }
}

impl TunnelContext {
    /// Persist + register SNI-demuxed hostname routes on `source` to `target`.
    /// `auto` records who owns the route for the UI Manual/Automatic split: the
    /// PCP path passes `true`; a manual add passes `false` and its own `label`.
    /// `lifetime` leases the routes (auto only). Shared by both so the two paths
    /// can't drift; only the ownership/label inputs differ.
    pub async fn persist_sni_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
        lifetime: Option<u32>,
        auto: bool,
        label: Option<String>,
    ) -> Result<(), u8> {
        // A fresh route with no explicit label defaults to `PCP` only when auto.
        let default_label = if auto { Some("PCP".to_string()) } else { label };
        // Persist first (DB is source of truth): reject a DNAT-occupied port or a
        // foreign-owned hostname before touching the dataplane. Registering first
        // risked a rollback on a transient DB error tearing down a valid binding.
        let hostnames_owned = hostnames.to_vec();
        let persisted = self
            .db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    use crate::tunnel::db::{PortForward, SniRoute};
                    // SNI routes may share a source (demux by hostname), but the
                    // source must not fall inside a different DNAT range.
                    if let Some(conflict) = pf.overlapping(source, 1) {
                        return Err(Error::new(
                            eyre!("{source} overlaps an existing forward at {conflict}"),
                            ErrorKind::InvalidRequest,
                        ));
                    }
                    // A lone same-owner hostname-less DNAT on this exact port is
                    // promoted to this shared SNI port's fallback, so a bare public
                    // IP and named domains can coexist. A range or another client's
                    // DNAT is rejected (see `plan_dnat_conversion`).
                    let mut converted = None;
                    if plan_dnat_conversion(pf.0.get(&source), source, target)? {
                        if let Some(PortForward::Dnat {
                            target: dt,
                            label,
                            enabled,
                            auto,
                            ..
                        }) = pf.0.remove(&source)
                        {
                            converted = Some(dt);
                            pf.0.insert(
                                source,
                                PortForward::Sni {
                                    routes: std::collections::BTreeMap::new(),
                                    fallback: Some(SniRoute {
                                        target: dt,
                                        label,
                                        enabled,
                                        auto,
                                    }),
                                },
                            );
                        }
                    }
                    let entry = pf.0.entry(source).or_insert_with(|| PortForward::Sni {
                        routes: std::collections::BTreeMap::new(),
                        fallback: None,
                    });
                    match entry {
                        PortForward::Sni { routes, .. } => {
                            for h in &hostnames_owned {
                                if routes.get(h).is_some_and(|r| r.target != target) {
                                    return Err(Error::new(
                                        eyre!(
                                            "SNI hostname {h} on {source} is held by another client"
                                        ),
                                        ErrorKind::InvalidRequest,
                                    ));
                                }
                            }
                            for h in &hostnames_owned {
                                let (label, enabled, auto) =
                                    sni_route_fields(routes.get(h), auto, &default_label);
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
                            Ok(converted)
                        }
                        // Unreachable: a lone DNAT was converted to Sni just above.
                        PortForward::Dnat { .. } => Err(Error::new(
                            eyre!("{source} is already a DNAT forward"),
                            ErrorKind::InvalidRequest,
                        )),
                    }
                })
            })
            .await
            .result;
        let converted = match persisted {
            Ok(c) => c,
            Err(_) => return Err(crate::net::port_map::pcp::hostname::RESULT_HOSTNAME_TAKEN),
        };
        // If a lone DNAT was promoted to this port's fallback, bind the fallback in
        // the demux, tear down the now-superseded kernel DNAT (the SNI listener
        // takes over the port), and carry its lease to the fallback so a still-
        // renewing bare-IP client isn't reaped before its next MAP.
        if let Some(dnat_target) = converted {
            if let Err(code) =
                self.sni()
                    .register_fallback(*source.ip(), source.port(), dnat_target)
            {
                tracing::warn!("failed to register fallback converting DNAT on {source}: {code}");
            }
            if let Some(rc) = self.active_forwards.mutate(|m| m.remove(&source)) {
                drop(rc);
                self.forward.gc().await.log_err();
            }
            let carried = self.leases.mutate(|l| l.remove(&LeaseKey::Dnat(source)));
            if let Some(exp) = carried {
                self.leases.mutate(|l| {
                    l.insert(LeaseKey::SniFallback(source), exp);
                });
            }
        }
        // Mirror into the dataplane; on the unexpected register failure undo the
        // DB routes we just added.
        if self
            .sni()
            .register(*source.ip(), source.port(), hostnames, target, None)
            .is_err()
        {
            self.remove_sni_forward(source, target, hostnames).await;
            return Err(crate::net::port_map::pcp::hostname::RESULT_HOSTNAME_TAKEN);
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

    /// Persist + register the hostname-less fallback on `source -> target`. The
    /// port must already be SNI-demuxed (a lone hostname-less forward stays a
    /// kernel DNAT); on a StartTunnel this is how a bare public IP shares a port
    /// with named domains. `auto`/`label` mirror [`persist_sni_forward`]; the
    /// same target reclaims, a different one is rejected (one fallback per port).
    pub async fn persist_fallback_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        lifetime: Option<u32>,
        auto: bool,
        label: Option<String>,
    ) -> Result<(), u8> {
        let default_label = if auto { Some("PCP".to_string()) } else { label };
        let persisted = self
            .db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    use crate::tunnel::db::{PortForward, SniRoute};
                    match pf.0.get_mut(&source) {
                        Some(PortForward::Sni { fallback, .. }) => {
                            if fallback.as_ref().is_some_and(|f| f.target != target) {
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
                            Ok(())
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
        if persisted.is_err() {
            return Err(crate::net::port_map::pcp::hostname::RESULT_HOSTNAME_TAKEN);
        }
        if self
            .sni()
            .register_fallback(*source.ip(), source.port(), target)
            .is_err()
        {
            self.remove_sni_fallback(source, target).await;
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
        self.sni()
            .unregister_fallback(*source.ip(), source.port(), target);
        lease::forget(self, &LeaseKey::SniFallback(source));
        self.db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    use crate::tunnel::db::PortForward;
                    let mut now_empty = false;
                    if let Some(PortForward::Sni { routes, fallback }) = pf.0.get_mut(&source) {
                        if fallback.as_ref().is_some_and(|f| f.target == target) {
                            *fallback = None;
                        }
                        now_empty = routes.is_empty() && fallback.is_none();
                    }
                    if now_empty {
                        pf.0.remove(&source);
                    }
                    Ok(())
                })
            })
            .await
            .result
            .log_err();
    }
}

/// Whether a hostname MAP arriving on `source` may promote an existing
/// hostname-less DNAT there into this SNI port's fallback. `Ok(true)` converts (a
/// lone, same-owner DNAT); `Ok(false)` means nothing to convert (empty or already
/// SNI). `Err` rejects an incompatible claim: a DNAT range (can't host SNI) or a
/// *different* client's DNAT — a whole-port claim another peer must not carve up.
/// Each peer's forward target is forced to its own tunnel IP, so the target IP
/// identifies the owner.
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

/// The stored `(label, enabled, auto)` for an upserted SNI route. A brand-new
/// route takes the caller's `auto` and `default_label`; an existing one keeps
/// its owner (`auto`), enabled state, and any user label — so a PCP renewal
/// can't hijack a manual route, nor a manual re-add flip an automatic one.
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

/// Remove the peer's forward to `(peer, internal_port)`, if any. We forward both
/// protocols on one entry, so match by target rather than PCP's (proto, port, client).
async fn remove_peer_forward(ctx: &TunnelContext, peer: Ipv4Addr, internal_port: u16) {
    let target = SocketAddrV4::new(peer, internal_port);
    let source = ctx
        .db
        .peek()
        .await
        .as_port_forwards()
        .de()
        .ok()
        .and_then(|pf| {
            pf.0.iter()
                .find(|(_, entry)| {
                    matches!(entry, PortForward::Dnat { target: t, .. } if *t == target)
                })
                .map(|(source, _)| *source)
        });
    let Some(source) = source else {
        return;
    };
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

#[cfg(test)]
mod tests {
    use std::net::SocketAddrV4;

    use super::{plan_dnat_conversion, sni_route_fields};
    use crate::tunnel::db::{PortForward, SniRoute};

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
}
