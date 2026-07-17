//! Per-IPv4 HTTP→HTTPS redirect listeners for StartTunnel.
//!
//! By default the tunnel runs an HTTP→HTTPS redirect on port 80 of every public
//! IPv4 it holds, so a plain `http://` request to an exposed service bounces to
//! `https://`. Each address gets a userspace TCP listener that hands every
//! connection to the shared OS redirect handler
//! ([`handle_http_on_https`](crate::net::http::handle_http_on_https)). The live
//! set is reconciled from the db, so a user's per-IP toggle (default on) and any
//! port-80 port-forward (which the redirect yields to) take effect with no
//! restart.

use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

use imbl::OrdMap;
use tokio::net::TcpListener;

use crate::GatewayId;
use crate::db::model::public::{NetworkInterfaceInfo, NetworkInterfaceType};
use crate::prelude::*;
use crate::tunnel::context::TunnelContext;
use crate::tunnel::wg::WIREGUARD_INTERFACE_NAME;
use crate::util::future::NonDetachingJoinHandle;

/// The only port an HTTP→HTTPS upgrade makes sense on.
pub const HTTP_PORT: u16 = 80;

/// A public IPv4: not loopback and not RFC1918. Mirrors the web UI's
/// `IpNet.isPublic`, so the checkbox list and the bound listeners agree on the
/// same set of addresses.
fn is_public_v4(ip: &Ipv4Addr) -> bool {
    !ip.is_loopback() && !ip.is_private()
}

/// The public IPv4 addresses this tunnel host holds, read from the same db
/// `gateways` the UI reads so both sides derive the same set.
pub(crate) fn public_ipv4s(
    gateways: &OrdMap<GatewayId, NetworkInterfaceInfo>,
) -> BTreeSet<Ipv4Addr> {
    let mut out = BTreeSet::new();
    for (id, info) in gateways.iter() {
        if id.as_str() == WIREGUARD_INTERFACE_NAME {
            continue;
        }
        let Some(ip_info) = info.ip_info.as_ref() else {
            continue;
        };
        if ip_info.device_type == Some(NetworkInterfaceType::Loopback) {
            continue;
        }
        for net in ip_info.subnets.iter() {
            if let IpAddr::V4(v4) = net.addr() {
                if is_public_v4(&v4) {
                    out.insert(v4);
                }
            }
        }
    }
    out
}

/// The `IP:80` sockets a redirect should currently listen on: every public IPv4
/// the user has not turned off and that is not already claimed by a port-80
/// forward.
async fn desired(ctx: &TunnelContext) -> Result<BTreeSet<SocketAddr>, Error> {
    let peek = ctx.db.peek().await;
    let disabled = peek.as_http_redirects().de()?.disabled;
    let forwards = peek.as_port_forwards().de()?;
    let gateways = peek.as_gateways().de()?;
    Ok(public_ipv4s(&gateways)
        .into_iter()
        .filter(|ip| !disabled.contains(ip))
        .map(|ip| SocketAddrV4::new(ip, HTTP_PORT))
        .filter(|sa| !forwards.occupied(*sa))
        .map(SocketAddr::V4)
        .collect())
}

/// Accept loop for one redirect address: each connection is handed to the shared
/// OS HTTP→HTTPS redirect. Dropping the returned handle aborts the loop and
/// closes the listener.
fn spawn_listener(addr: SocketAddr, listener: TcpListener) -> NonDetachingJoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    tokio::spawn(async move {
                        if let Err(e) = crate::net::http::handle_http_on_https(stream).await {
                            crate::dev_log!(debug, "http redirect on {addr} closed: {e}");
                        }
                    });
                }
                Err(e) => {
                    tracing::warn!("http redirect accept error on {addr}: {e}");
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            }
        }
    })
    .into()
}

/// Reconcile the live listeners in `active` against what the db wants: drop the
/// ones no longer desired, bind the newly desired. Idempotent — safe to call on
/// every db revision. A failed bind is logged and retried on the next call
/// rather than aborting the reconcile.
pub async fn reconcile(
    ctx: &TunnelContext,
    active: &mut BTreeMap<SocketAddr, NonDetachingJoinHandle<()>>,
) -> Result<(), Error> {
    let desired = desired(ctx).await?;
    active.retain(|addr, _| desired.contains(addr));
    for addr in &desired {
        if active.contains_key(addr) {
            continue;
        }
        match TcpListener::bind(addr).await {
            Ok(listener) => {
                active.insert(*addr, spawn_listener(*addr, listener));
            }
            Err(e) => tracing::warn!("failed to bind http redirect on {addr}: {e}"),
        }
    }
    Ok(())
}
