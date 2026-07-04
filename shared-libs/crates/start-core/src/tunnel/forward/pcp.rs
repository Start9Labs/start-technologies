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
use crate::tunnel::forward::sni::SniDemux;
use crate::tunnel::wg::WIREGUARD_INTERFACE_NAME;

/// Run the PCP server (IPv4 DNAT forwards + IPv6 GUA pinholes) for the life of
/// the tunnel, each family self-restarting on error.
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
    let socket =
        Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).with_kind(ErrorKind::Network)?;
    socket.set_reuse_address(true).with_kind(ErrorKind::Network)?;
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
    let socket =
        Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP)).with_kind(ErrorKind::Network)?;
    socket.set_reuse_address(true).with_kind(ErrorKind::Network)?;
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
    let socket = socket()?;
    tracing::info!("PCP server listening on {WIREGUARD_INTERFACE_NAME}:{PCP_PORT}");
    let mut buf = [0u8; 1100];
    loop {
        let (n, from) = socket.recv_from(&mut buf).await.with_kind(ErrorKind::Network)?;
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
    let socket = socket6()?;
    tracing::info!("PCP v6 server listening on {WIREGUARD_INTERFACE_NAME}:{PCP_PORT}");
    let mut buf = [0u8; 1100];
    loop {
        let (n, from) = socket.recv_from(&mut buf).await.with_kind(ErrorKind::Network)?;
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
    ) -> Result<(), u16> {
        apply_peer_forward_range(self, source, target, count, "PCP").await
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
            0
        })
    }

    async fn remove_pinhole(&self, gua: Ipv6Addr, external_port: u16) {
        crate::tunnel::forward::pinhole::remove_pinhole(self, gua, external_port).await
    }

    fn sni(&self) -> &Arc<SniDemux> {
        &self.sni
    }

    async fn add_sni_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
        _lifetime: Option<u32>,
    ) -> Result<(), u8> {
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
                    let entry = pf.0.entry(source).or_insert_with(|| PortForward::Sni {
                        routes: std::collections::BTreeMap::new(),
                    });
                    match entry {
                        PortForward::Sni { routes } => {
                            for h in &hostnames_owned {
                                if routes.get(h).is_some_and(|r| r.target != target) {
                                    return Err(Error::new(
                                        eyre!("SNI hostname {h} on {source} is held by another client"),
                                        ErrorKind::InvalidRequest,
                                    ));
                                }
                            }
                            for h in &hostnames_owned {
                                // A renewal must not clobber a user's edits, so
                                // keep any existing label/enabled; a brand-new
                                // route gets a default label marking it PCP-added.
                                let (label, enabled) = match routes.get(h) {
                                    Some(r) => (
                                        r.label.clone().or_else(|| Some("PCP".into())),
                                        r.enabled,
                                    ),
                                    None => (Some("PCP".into()), true),
                                };
                                routes.insert(h.clone(), SniRoute { target, label, enabled, auto: true });
                            }
                            Ok(())
                        }
                        // external port already used by a DNAT forward
                        PortForward::Dnat { .. } => Err(Error::new(
                            eyre!("{source} is already a DNAT forward"),
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
        Ok(())
    }

    async fn remove_sni_forward(&self, source: SocketAddrV4, target: SocketAddrV4, hostnames: &[String]) {
        self.sni().unregister(*source.ip(), source.port(), hostnames, target);
        let hostnames = hostnames.to_vec();
        self.db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    use crate::tunnel::db::PortForward;
                    let mut now_empty = false;
                    if let Some(PortForward::Sni { routes }) = pf.0.get_mut(&source) {
                        routes.retain(|h, r| !(r.target == target && hostnames.contains(h)));
                        now_empty = routes.is_empty();
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
}
