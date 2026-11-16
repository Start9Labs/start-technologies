//! The tunnel's gateway-side inbound forwarding: nft DNAT + external-IP
//! resolution ([`igd`]), the PCP [`GatewayBackend`](crate::net::port_map::server::GatewayBackend)
//! implementation ([`pcp`]), and the SNI demultiplexer ([`sni`]).

use crate::tunnel::forward::address::{ForwardAddress, PinholeAddress};
pub mod address;
pub mod igd;
pub mod lease;
pub mod pcp;
pub mod pinhole;
pub mod sni;

use std::collections::BTreeSet;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4};

use crate::net::transport::{Transport, TransportProtocol};
use crate::prelude::*;
use crate::tunnel::context::TunnelContext;
use crate::tunnel::db::PortForward;
use crate::tunnel::web::listener_holds;
use crate::tunnel::wg6::host_v6;

/// Whether `count` ports from `source` include one kept for the host.
fn host_holds(
    rpc: SocketAddr,
    reserved: &BTreeSet<u16>,
    wg_port: u16,
    source: SocketAddrV4,
    count: u16,
    transport: &Transport,
) -> bool {
    let span = source.port()..=source.port().saturating_add(count.saturating_sub(1));
    let protocols = transport.protocols();
    reserved.range(span.clone()).next().is_some()
        || protocols.contains(&TransportProtocol::Udp) && span.contains(&wg_port)
        || protocols.contains(&TransportProtocol::Tcp) && listener_holds(rpc, source, count)
}

impl TunnelContext {
    /// Rejects a published port that would take the WireGuard port, the API
    /// listener, or a configured reserved port.
    pub(crate) async fn check_host_ports(
        &self,
        source: SocketAddrV4,
        count: u16,
        transport: &Transport,
    ) -> Result<(), Error> {
        let wg_port = self.db.peek().await.as_wg().as_port().de()?;
        if host_holds(
            self.listen,
            &self.reserved_ports,
            wg_port,
            source,
            count,
            transport,
        ) {
            return Err(Error::new(
                eyre!(t!("tunnel.forward.host-port-reserved")),
                ErrorKind::InvalidRequest,
            ));
        }
        Ok(())
    }
}

/// Tear down a device's inbound exposure — v4 DNAT forwards, SNI routes, and v6
/// pinholes — when it loses the right to it: a device deleted, demoted to a
/// client, or (with `auto_only`) having automatic forwarding switched off.
/// `auto_only` limits the sweep to gateway-created (PCP/UPnP) entries, leaving
/// the operator's manual forwards; a delete or demote passes `false` to remove
/// everything the device held. Each removal also drops the entry's lease.
pub async fn clear_for_peer(
    ctx: &TunnelContext,
    peer: Ipv4Addr,
    auto_only: bool,
) -> Result<(), Error> {
    // v4 DNAT + SNI live in port_forwards, matched by their target (the device).
    let forwards = ctx.db.peek().await.as_port_forwards().de()?;
    let (dnat_sources, sni_routes, sni_fallbacks) =
        select_peer_forwards(&forwards.0, peer, auto_only);
    for source in dnat_sources {
        ctx.remove_peer_forward_by_source(source.address, peer, false, source.transport)
            .await;
    }
    for (source, target, host) in sni_routes {
        ctx.remove_sni_routes(source, target, &[host], false).await;
    }
    for (source, target) in sni_fallbacks {
        ctx.remove_sni_fallback(source, target).await;
    }

    // v6 pinholes are keyed by the device's own GUA, derivable from each subnet's
    // prefix and the device's tunnel IPv4 (independent of client membership, so
    // this still resolves during a delete).
    let peer_guas: BTreeSet<Ipv6Addr> = ctx
        .db
        .peek()
        .await
        .as_wg()
        .de()?
        .subnets
        .0
        .values()
        .filter_map(|cfg| cfg.ipv6)
        .map(|prefix| host_v6(prefix, peer))
        .collect();
    if !peer_guas.is_empty() {
        let keys: Vec<PinholeAddress> = ctx
            .db
            .peek()
            .await
            .as_pinholes6()
            .de()?
            .0
            .iter()
            .filter(|(key, ph)| peer_guas.contains(key.ip()) && (!auto_only || ph.auto))
            .map(|(key, _)| key.clone())
            .collect();
        for key in keys {
            pinhole::remove_pinhole(ctx, *key.ip(), key.port(), false, key.transport).await;
        }
    }
    Ok(())
}

/// The DNAT sources, SNI routes (`source`, `target`, `hostname`), and SNI
/// fallbacks (`source`, `target`) held by `peer` — everything, or just the
/// automatic entries when `auto_only`.
fn select_peer_forwards(
    forwards: &std::collections::BTreeMap<ForwardAddress, PortForward>,
    peer: Ipv4Addr,
    auto_only: bool,
) -> (
    Vec<ForwardAddress>,
    Vec<(SocketAddrV4, SocketAddrV4, String)>,
    Vec<(SocketAddrV4, SocketAddrV4)>,
) {
    let mut dnat_sources = Vec::new();
    let mut sni_routes = Vec::new();
    let mut sni_fallbacks = Vec::new();
    for (source, entry) in forwards {
        match entry {
            PortForward::Dnat { target, auto, .. }
                if target.ip() == &peer && (!auto_only || *auto) =>
            {
                dnat_sources.push(source.clone());
            }
            PortForward::Sni { routes, fallback } => {
                for (host, route) in routes {
                    if route.target.ip() == &peer && (!auto_only || route.auto) {
                        sni_routes.push((source.address, route.target, host.clone()));
                    }
                }
                if let Some(f) = fallback {
                    if f.target.ip() == &peer && (!auto_only || f.auto) {
                        sni_fallbacks.push((source.address, f.target));
                    }
                }
            }
            _ => {}
        }
    }
    (dnat_sources, sni_routes, sni_fallbacks)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::tunnel::db::SniRoute;

    #[test]
    fn host_ports_are_kept_from_forwards() {
        let reserved = BTreeSet::from([22]);
        let holds = |rpc: &str, source: &str, count, transport: &Transport| {
            host_holds(
                rpc.parse().unwrap(),
                &reserved,
                51820,
                source.parse().unwrap(),
                count,
                transport,
            )
        };
        let public = "0.0.0.0:5960";
        let both = Transport::default();
        assert!(holds(public, "203.0.113.4:22", 1, &Transport::tcp()));
        assert!(holds(public, "203.0.113.4:22", 1, &Transport::udp()));
        assert!(holds(public, "203.0.113.4:20", 3, &both));
        assert!(!holds(public, "203.0.113.4:20", 2, &both));
        assert!(holds(public, "203.0.113.4:51820", 1, &both));
        assert!(holds(public, "203.0.113.4:51820", 1, &Transport::udp()));
        assert!(!holds(public, "203.0.113.4:51820", 1, &Transport::tcp()));
        assert!(holds(public, "203.0.113.4:5960", 1, &Transport::tcp()));
        assert!(!holds(public, "203.0.113.4:5960", 1, &Transport::udp()));
        assert!(!holds("127.0.59.60:5960", "203.0.113.4:5960", 1, &both));
    }

    fn dnat(target: &str, auto: bool) -> PortForward {
        PortForward::Dnat {
            target: target.parse().unwrap(),
            label: None,
            enabled: true,
            count: 1,
            auto,
        }
    }

    // auto_only=true retires only the automatic entries; the operator's manual
    // forwards to the same device survive. auto_only=false takes both.
    #[test]
    fn select_respects_auto_only_and_target() {
        let peer: Ipv4Addr = "10.59.0.2".parse().unwrap();
        let other: Ipv4Addr = "10.59.0.3".parse().unwrap();
        let mut fwds: BTreeMap<ForwardAddress, PortForward> = BTreeMap::new();
        fwds.insert("1.2.3.4:443".parse().unwrap(), dnat("10.59.0.2:443", true));
        fwds.insert(
            "1.2.3.4:8443".parse().unwrap(),
            dnat("10.59.0.2:8443", false),
        );
        fwds.insert(
            "1.2.3.4:9000".parse().unwrap(),
            dnat("10.59.0.3:9000", true),
        );
        let mut routes = BTreeMap::new();
        routes.insert(
            "auto.example.com".to_string(),
            SniRoute {
                target: "10.59.0.2:443".parse().unwrap(),
                label: None,
                enabled: true,
                auto: true,
            },
        );
        routes.insert(
            "manual.example.com".to_string(),
            SniRoute {
                target: "10.59.0.2:443".parse().unwrap(),
                label: None,
                enabled: true,
                auto: false,
            },
        );
        fwds.insert(
            "5.6.7.8:443".parse().unwrap(),
            PortForward::Sni {
                routes,
                fallback: Some(SniRoute {
                    target: "10.59.0.2:443".parse().unwrap(),
                    label: None,
                    enabled: true,
                    auto: true,
                }),
            },
        );

        let (auto_dnat, auto_sni, auto_fb) = select_peer_forwards(&fwds, peer, true);
        assert_eq!(auto_dnat, vec!["1.2.3.4:443".parse().unwrap()], "auto only");
        assert_eq!(auto_sni.len(), 1);
        assert_eq!(auto_sni[0].2, "auto.example.com");
        // The auto fallback to this peer is swept too.
        assert_eq!(
            auto_fb,
            vec![(
                "5.6.7.8:443".parse().unwrap(),
                "10.59.0.2:443".parse().unwrap()
            )]
        );

        let (all_dnat, all_sni, all_fb) = select_peer_forwards(&fwds, peer, false);
        assert_eq!(all_dnat.len(), 2, "manual + auto for peer");
        assert_eq!(all_sni.len(), 2);
        assert_eq!(all_fb.len(), 1);

        // A different device's forward is never swept.
        let (other_dnat, _, other_fb) = select_peer_forwards(&fwds, other, false);
        assert_eq!(other_dnat, vec!["1.2.3.4:9000".parse().unwrap()]);
        assert!(other_fb.is_empty());
    }
}
