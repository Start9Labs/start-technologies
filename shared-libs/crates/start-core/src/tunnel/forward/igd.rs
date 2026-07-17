//! Server-side UPnP IGD for StartTunnel. The tunnel answers UPnP IGD requests
//! over the WireGuard interface, so a client's UPnP code path
//! ([`crate::net::port_map::upnp`]) is identical behind a router and a tunnel.
//!
//! The IGD is reachable only over WireGuard (`SO_BINDTODEVICE`-bound sockets) and
//! only honors configured peers. Unlike classic UPnP, a peer can only forward to
//! **itself**: the SOAP `NewInternalClient` is ignored and the target is forced
//! to the requesting peer's own tunnel IP.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
#[cfg(target_os = "linux")]
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use nix::net::if_::if_nametoindex;
use socket2::{Domain, InterfaceIndexOrAddress, Protocol, SockAddr, Socket, Type};
use tokio::net::{TcpListener, UdpSocket};

use crate::db::model::public::NetworkInterfaceType;
use crate::net::port_map::server::igd::{
    CONTROL_PATH, IGD_HTTP_PORT, ROOT_DESC_PATH, SCPD, SCPD_PATH, SSDP_MULTICAST, SSDP_PORT,
    format_uuid, handle_control, header_value, render_root_desc, serve_static, ssdp_response,
    st_matches,
};
use crate::prelude::*;
use crate::tunnel::context::TunnelContext;
use crate::tunnel::db::PortForward;
use crate::tunnel::forward::lease::{self, LeaseKey};
use crate::tunnel::wg::WIREGUARD_INTERFACE_NAME;

/// Run the IGD server (SSDP responder + HTTP control server) for the life of
/// the tunnel. Both halves self-restart on error.
pub async fn run(ctx: TunnelContext) {
    let uuid = match device_uuid(&ctx).await {
        Ok(uuid) => uuid,
        Err(e) => {
            tracing::error!("UPnP IGD: cannot derive device uuid: {e}");
            return;
        }
    };
    let root_desc: Arc<str> = Arc::from(render_root_desc(&uuid));
    tokio::join!(http_server(ctx.clone(), root_desc), ssdp_server(ctx, uuid));
}

async fn device_uuid(ctx: &TunnelContext) -> Result<String, Error> {
    let key = ctx.db.peek().await.as_wg().as_key().de()?.verifying_key();
    Ok(format_uuid(key.0.as_bytes()))
}

async fn ssdp_server(ctx: TunnelContext, uuid: String) {
    loop {
        if let Err(e) = ssdp_loop(&ctx, &uuid).await {
            tracing::warn!("UPnP IGD SSDP responder failed, retrying: {e}");
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
}

fn ssdp_socket() -> Result<UdpSocket, Error> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
        .with_kind(ErrorKind::Network)?;
    socket
        .set_reuse_address(true)
        .with_kind(ErrorKind::Network)?;
    bind_to_wireguard(&socket)?;
    socket
        .bind(&SockAddr::from(SocketAddrV4::new(
            Ipv4Addr::UNSPECIFIED,
            SSDP_PORT,
        )))
        .with_kind(ErrorKind::Network)?;
    let ifindex = if_nametoindex(WIREGUARD_INTERFACE_NAME).with_kind(ErrorKind::Network)?;
    socket
        .join_multicast_v4_n(&SSDP_MULTICAST, &InterfaceIndexOrAddress::Index(ifindex))
        .with_kind(ErrorKind::Network)?;
    socket
        .set_multicast_loop_v4(false)
        .with_kind(ErrorKind::Network)?;
    socket.set_nonblocking(true).with_kind(ErrorKind::Network)?;
    UdpSocket::from_std(socket.into()).with_kind(ErrorKind::Network)
}

async fn ssdp_loop(ctx: &TunnelContext, uuid: &str) -> Result<(), Error> {
    // Subscribe before binding so a bounce during setup still triggers a rebind.
    let mut ifindex = ctx.forward_ifindex.subscribe();
    ifindex.borrow_and_update();
    let socket = ssdp_socket()?;
    tracing::info!("UPnP IGD SSDP responder listening on {WIREGUARD_INTERFACE_NAME}");
    let mut buf = [0u8; 2048];
    loop {
        let (n, from) = tokio::select! {
            res = socket.recv_from(&mut buf) => res.with_kind(ErrorKind::Network)?,
            _ = ifindex.changed() => {
                tracing::info!("{WIREGUARD_INTERFACE_NAME} ifindex changed; rebinding SSDP responder");
                return Ok(());
            }
        };
        let Ok(text) = std::str::from_utf8(&buf[..n]) else {
            continue;
        };
        if !text.starts_with("M-SEARCH") {
            continue;
        }
        let Some(st) = header_value(text, "st") else {
            continue;
        };
        if !st_matches(&st) {
            continue;
        }
        let IpAddr::V4(peer) = from.ip() else {
            continue;
        };
        // Only answer a configured peer, and advertise the `.1` of its subnet.
        let Some(server_ip) = subnet_gateway_for(ctx, peer).await else {
            continue;
        };
        let resp = ssdp_response(server_ip, uuid);
        if let Err(e) = socket.send_to(resp.as_bytes(), from).await {
            tracing::debug!("UPnP IGD: failed to answer M-SEARCH from {from}: {e}");
        }
    }
}

/// The `.1` server address of the subnet that contains `peer`, if `peer` is a
/// configured client on it. Doubles as the peer-authorization check for SSDP.
pub(super) async fn subnet_gateway_for(ctx: &TunnelContext, peer: Ipv4Addr) -> Option<Ipv4Addr> {
    let subnets = ctx.db.peek().await.as_wg().as_subnets().de().ok()?;
    subnets.0.iter().find_map(|(subnet, cfg)| {
        if cfg.clients.0.contains_key(&peer) {
            Some(subnet.addr())
        } else {
            None
        }
    })
}

async fn http_server(ctx: TunnelContext, root_desc: Arc<str>) {
    let app = Router::new()
        .route(
            ROOT_DESC_PATH,
            get(move || serve_static(root_desc.clone(), "text/xml")),
        )
        .route(SCPD_PATH, get(|| serve_static(Arc::from(SCPD), "text/xml")))
        .route(CONTROL_PATH, post(control))
        .with_state(ctx.clone());
    loop {
        // Subscribe before binding so a bounce during setup still triggers a rebind.
        let mut ifindex = ctx.forward_ifindex.subscribe();
        ifindex.borrow_and_update();
        match igd_http_listener() {
            Ok(listener) => {
                tracing::info!(
                    "UPnP IGD control server listening on {WIREGUARD_INTERFACE_NAME}:{IGD_HTTP_PORT}"
                );
                tokio::select! {
                    res = axum::serve(
                        listener,
                        app.clone()
                            .into_make_service_with_connect_info::<SocketAddr>(),
                    ) => {
                        if let Err(e) = res {
                            tracing::warn!("UPnP IGD control server exited, retrying: {e}");
                        }
                    }
                    _ = ifindex.changed() => {
                        tracing::info!("{WIREGUARD_INTERFACE_NAME} ifindex changed; rebinding IGD control server");
                        continue;
                    }
                }
            }
            Err(e) => tracing::warn!("UPnP IGD control server bind failed, retrying: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

fn igd_http_listener() -> Result<TcpListener, Error> {
    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
        .with_kind(ErrorKind::Network)?;
    socket
        .set_reuse_address(true)
        .with_kind(ErrorKind::Network)?;
    bind_to_wireguard(&socket)?;
    socket
        .bind(&SockAddr::from(SocketAddrV4::new(
            Ipv4Addr::UNSPECIFIED,
            IGD_HTTP_PORT,
        )))
        .with_kind(ErrorKind::Network)?;
    socket.listen(128).with_kind(ErrorKind::Network)?;
    socket.set_nonblocking(true).with_kind(ErrorKind::Network)?;
    TcpListener::from_std(socket.into()).with_kind(ErrorKind::Network)
}

/// `SO_BINDTODEVICE` to the WireGuard interface so the socket never sees the
/// VPS's public interface — this is what keeps the IGD private to peers. The
/// non-linux stub below only exists to compile `core` on other CI targets
/// (apple-darwin lacks `SO_BINDTODEVICE`) and is never reached at runtime.
#[cfg(target_os = "linux")]
pub(super) fn bind_to_wireguard(socket: &Socket) -> Result<(), Error> {
    let name = WIREGUARD_INTERFACE_NAME.as_bytes();
    // SAFETY: fd from `socket`; `name`/`len` describe a valid slice that SO_BINDTODEVICE copies.
    let ret = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_BINDTODEVICE,
            name.as_ptr() as *const libc::c_void,
            name.len() as libc::socklen_t,
        )
    };
    if ret != 0 {
        return Err(Error::new(
            eyre!(
                "SO_BINDTODEVICE({WIREGUARD_INTERFACE_NAME}): {}",
                std::io::Error::last_os_error()
            ),
            ErrorKind::Network,
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub(super) fn bind_to_wireguard(_socket: &Socket) -> Result<(), Error> {
    Err(Error::new(
        eyre!("SO_BINDTODEVICE is only supported on Linux"),
        ErrorKind::Network,
    ))
}

async fn control(
    State(ctx): State<TunnelContext>,
    ConnectInfo(from): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: String,
) -> Response {
    let IpAddr::V4(peer) = from.ip() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    handle_control(&ctx, peer, &headers, &body).await
}

/// Installs (or re-asserts) a forward of `count` contiguous ports (a PCP
/// PORT_SET range) from `source` to `target`. `protocol_label` is the DB label
/// (e.g. "UPnP", "PCP"); the requesting device is already shown by the forward's
/// target.
pub(super) async fn apply_peer_forward_range(
    ctx: &TunnelContext,
    source: SocketAddrV4,
    target: SocketAddrV4,
    count: u16,
    protocol_label: &str,
    lifetime: Option<u32>,
) -> Result<(), u16> {
    // Port 80 is reserved for the tunnel's HTTP→HTTPS redirect; never
    // automatically create a forward that would take it (PCP/UPnP alike).
    let lo = source.port();
    if lo <= 80 && 80 <= lo.saturating_add(count.saturating_sub(1)) {
        return Err(718); // ConflictInMappingEntry — port 80 owned by the redirect
    }
    match current_forward(ctx, source).await {
        Some(PortForward::Dnat {
            target: t,
            count: c,
            ..
        }) if t != target || c != count => {
            return Err(718); // ConflictInMappingEntry
        }
        // The external port is SNI-demuxed. A single hostname-less MAP becomes the
        // port's fallback (a bare public IP sharing the port with named domains);
        // `persist_fallback_forward` stamps its own SniFallback lease. A range
        // can't be a single-port fallback, so it still conflicts.
        Some(PortForward::Sni { .. }) => {
            if count != 1 {
                return Err(718); // ConflictInMappingEntry
            }
            return ctx
                .persist_fallback_forward(source, target, lifetime, true, None)
                .await
                .map_err(|_| 718u16);
        }
        Some(PortForward::Dnat { .. }) => {
            // Idempotent re-assert from the client's periodic refresh: ensure the
            // nft forward is actually installed.
            let active = ctx.active_forwards.mutate(|m| m.contains_key(&source));
            if !active {
                let prefix = prefix_for(ctx, target.ip()).await;
                let rc = ctx
                    .forward
                    .add_forward_range(source, target, count, prefix, None)
                    .await
                    .map_err(|_| 501u16)?;
                ctx.active_forwards.mutate(|m| {
                    m.insert(source, rc);
                });
            }
            if let Some(lt) = lifetime {
                lease::stamp(ctx, LeaseKey::Dnat(source), lt);
            }
            return Ok(());
        }
        None => {}
    }

    // A new range must not overlap a different existing forward's ports on this
    // external IP (the exact-source cases are handled by the match above).
    // Reserve the slot atomically before touching the dataplane so concurrent
    // auto-forwards can't both pass the overlap check.
    let reserved = ctx
        .db
        .mutate(|db| {
            db.as_port_forwards_mut().mutate(|pf| {
                if pf.overlapping(source, count).is_some() {
                    return Ok(false);
                }
                pf.0.insert(
                    source,
                    PortForward::Dnat {
                        target,
                        label: Some(protocol_label.to_string()),
                        enabled: true,
                        count,
                        auto: true,
                    },
                );
                Ok(true)
            })
        })
        .await
        .result
        .map_err(|_| 501u16)?;
    if !reserved {
        return Err(718); // ConflictInMappingEntry
    }

    let prefix = prefix_for(ctx, target.ip()).await;
    let rc = match ctx
        .forward
        .add_forward_range(source, target, count, prefix, None)
        .await
    {
        Ok(rc) => rc,
        Err(_) => {
            ctx.db
                .mutate(|db| {
                    db.as_port_forwards_mut().mutate(|pf| {
                        pf.0.remove(&source);
                        Ok(())
                    })
                })
                .await
                .result
                .ok();
            return Err(501);
        }
    };
    ctx.active_forwards.mutate(|m| {
        m.insert(source, rc);
    });
    if let Some(lt) = lifetime {
        lease::stamp(ctx, LeaseKey::Dnat(source), lt);
    }
    Ok(())
}

pub(super) async fn current_forward(
    ctx: &TunnelContext,
    source: SocketAddrV4,
) -> Option<PortForward> {
    ctx.db
        .peek()
        .await
        .as_port_forwards()
        .de()
        .ok()
        .and_then(|pf| pf.0.get(&source).cloned())
}

/// Whether `peer` may auto-create port forwards (PCP/IGD) — the per-device
/// `allow_auto_port_forward` flag, set alongside `allow_dns_injection` by the
/// "Gateway Autoconfiguration" toggle. An untrusted client gets no forwards.
pub(super) async fn is_known_client(ctx: &TunnelContext, peer: Ipv4Addr) -> bool {
    let Ok(subnets) = ctx.db.peek().await.as_wg().as_subnets().de() else {
        return false;
    };
    subnets.0.values().any(|cfg| {
        cfg.clients
            .0
            .get(&peer)
            .is_some_and(|c| c.allow_auto_port_forward)
    })
}

/// The WAN IPv4 `peer`'s egress uses: its assigned WAN if pinned, else the
/// gateway's default WAN.
pub(in crate::tunnel) async fn external_ipv4(
    ctx: &TunnelContext,
    peer: Ipv4Addr,
) -> Option<Ipv4Addr> {
    assigned_wan_for(ctx, peer)
        .await
        .or_else(|| default_wan(ctx))
}

/// First usable WAN candidate across the gateway's non-loopback, non-wg
/// interfaces — the egress when no `wan_ip` is pinned for the peer or its subnet.
fn default_wan(ctx: &TunnelContext) -> Option<Ipv4Addr> {
    ctx.net_iface.peek(|ifaces| {
        ifaces.iter().find_map(|(id, info)| {
            if id.as_str() == WIREGUARD_INTERFACE_NAME {
                return None;
            }
            let ip_info = info.ip_info.as_ref()?;
            if ip_info.device_type == Some(NetworkInterfaceType::Loopback) {
                return None;
            }
            ip_info
                .wan_ip
                .filter(|v4| crate::net::port_map::upnp::is_wan_candidate(*v4))
                .or_else(|| {
                    ip_info.subnets.iter().find_map(|s| match s.addr() {
                        IpAddr::V4(v4) if crate::net::port_map::upnp::is_wan_candidate(v4) => {
                            Some(v4)
                        }
                        _ => None,
                    })
                })
        })
    })
}

/// The WAN IP pinned for `peer`: its device override, else its subnet's `wan_ip`.
async fn assigned_wan_for(ctx: &TunnelContext, peer: Ipv4Addr) -> Option<Ipv4Addr> {
    let subnets = ctx.db.peek().await.as_wg().as_subnets().de().ok()?;
    subnets.0.values().find_map(|cfg| {
        let client = cfg.clients.0.get(&peer)?;
        client.wan_ip.or(cfg.wan_ip)
    })
}

pub(in crate::tunnel) async fn prefix_for(ctx: &TunnelContext, target_ip: &Ipv4Addr) -> u8 {
    ctx.net_iface
        .peek(|ifaces| {
            ifaces.iter().find_map(|(_, info)| {
                info.ip_info.as_ref().and_then(|i| {
                    i.subnets
                        .iter()
                        .find(|s| s.contains(&IpAddr::V4(*target_ip)))
                        .map(|s| s.prefix_len())
                })
            })
        })
        .unwrap_or(32)
}
