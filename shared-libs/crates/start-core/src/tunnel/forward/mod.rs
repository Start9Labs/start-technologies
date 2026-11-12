//! The tunnel's gateway-side inbound forwarding: nft DNAT + external-IP
//! resolution ([`igd`]), the PCP [`GatewayBackend`](crate::net::port_map::server::GatewayBackend)
//! implementation ([`pcp`]), and the SNI demultiplexer ([`sni`]).

pub mod igd;
pub mod lease;
pub mod pcp;
pub mod pinhole;
pub mod sni;

use std::collections::BTreeSet;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};

use crate::net::port_map::Protocol;
use crate::prelude::*;
use crate::tunnel::context::TunnelContext;
use crate::tunnel::db::PortForward;
use crate::tunnel::wg6::host_v6;

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
        ctx.remove_peer_forward_by_source(source, peer, false).await;
    }
    for (source, target, host, protocol) in sni_routes {
        ctx.remove_sni_routes(source, target, &[host], protocol, false)
            .await;
    }
    for (source, target, protocol) in sni_fallbacks {
        ctx.remove_sni_fallback(source, target, protocol).await;
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
        let keys: Vec<SocketAddrV6> = ctx
            .db
            .peek()
            .await
            .as_pinholes6()
            .de()?
            .0
            .iter()
            .filter(|(key, ph)| peer_guas.contains(key.ip()) && (!auto_only || ph.auto))
            .map(|(key, _)| *key)
            .collect();
        for key in keys {
            pinhole::remove_pinhole(ctx, *key.ip(), key.port(), false).await;
        }
    }
    Ok(())
}

/// The DNAT sources, SNI routes (`source`, `target`, `hostname`), and SNI
/// fallbacks (`source`, `target`) held by `peer` — everything, or just the
/// automatic entries when `auto_only`.
fn select_peer_forwards(
    forwards: &std::collections::BTreeMap<SocketAddrV4, PortForward>,
    peer: Ipv4Addr,
    auto_only: bool,
) -> (
    Vec<SocketAddrV4>,
    Vec<(SocketAddrV4, SocketAddrV4, String, Protocol)>,
    Vec<(SocketAddrV4, SocketAddrV4, Protocol)>,
) {
    let mut dnat_sources = Vec::new();
    let mut sni_routes = Vec::new();
    let mut sni_fallbacks = Vec::new();
    for (source, entry) in forwards {
        match entry {
            PortForward::Dnat { target, auto, .. }
                if target.ip() == &peer && (!auto_only || *auto) =>
            {
                dnat_sources.push(*source);
            }
            PortForward::Sni { .. } => {
                for (protocol, hostname, route) in entry.sni_routes() {
                    if route.target.ip() == &peer && (!auto_only || route.auto) {
                        match hostname {
                            Some(host) => {
                                sni_routes.push((*source, route.target, host.to_owned(), protocol))
                            }
                            None => sni_fallbacks.push((*source, route.target, protocol)),
                        }
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
        let mut fwds: BTreeMap<SocketAddrV4, PortForward> = BTreeMap::new();
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
                both: Default::default(),
                tcp: crate::tunnel::db::SniBindings {
                    routes,
                    fallback: Some(SniRoute {
                        target: "10.59.0.2:443".parse().unwrap(),
                        label: None,
                        enabled: true,
                        auto: true,
                    }),
                },
                udp: Default::default(),
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
                "10.59.0.2:443".parse().unwrap(),
                Protocol::Tcp
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
    #[test]
    fn device_cleanup_selects_a_combined_mapping_once_with_its_shared_owner() {
        let source = "203.0.113.1:443".parse().unwrap();
        let peer = "10.59.0.2".parse().unwrap();
        for (auto, auto_only, count) in [
            (false, false, 1),
            (false, true, 0),
            (true, false, 1),
            (true, true, 1),
        ] {
            let mut entry = PortForward::empty_sni();
            let bindings = entry.bindings_mut(Protocol::TcpUdp).unwrap();
            let route = SniRoute {
                target: "10.59.0.2:443".parse().unwrap(),
                label: None,
                enabled: false,
                auto,
            };
            bindings
                .routes
                .insert("service.example.com".into(), route.clone());
            bindings.fallback = Some(route);
            let forwards = BTreeMap::from([(source, entry)]);
            let (dnat, routes, fallbacks) = select_peer_forwards(&forwards, peer, auto_only);
            assert!(dnat.is_empty());
            assert_eq!(routes.len(), count);
            assert_eq!(fallbacks.len(), count);
            if count == 1 {
                assert_eq!(routes[0].3, Protocol::TcpUdp);
                assert_eq!(fallbacks[0].2, Protocol::TcpUdp);
            }
            let (_, routes, fallbacks) =
                select_peer_forwards(&forwards, "10.59.0.3".parse().unwrap(), false);
            assert!(routes.is_empty());
            assert!(fallbacks.is_empty());
        }
    }

    #[test]
    fn device_cleanup_scopes_shared_hostname_and_fallback_by_transport_owner() {
        let source = "203.0.113.1:443".parse().unwrap();
        let peer = "10.59.0.2".parse().unwrap();
        let mut entry = PortForward::empty_sni();
        for (protocol, target) in [
            (Protocol::Tcp, "10.59.0.3:443"),
            (Protocol::Udp, "10.59.0.2:8443"),
        ] {
            let route = SniRoute {
                target: target.parse().unwrap(),
                label: None,
                enabled: true,
                auto: true,
            };
            let bindings = entry.bindings_mut(protocol).unwrap();
            bindings
                .routes
                .insert("service.example.com".into(), route.clone());
            bindings.fallback = Some(route);
        }
        let forwards = BTreeMap::from([(source, entry)]);
        let (dnat, routes, fallbacks) = select_peer_forwards(&forwards, peer, true);
        assert!(dnat.is_empty());
        assert_eq!(
            routes,
            vec![(
                source,
                "10.59.0.2:8443".parse().unwrap(),
                "service.example.com".into(),
                Protocol::Udp
            )]
        );
        assert_eq!(
            fallbacks,
            vec![(source, "10.59.0.2:8443".parse().unwrap(), Protocol::Udp)]
        );
    }
}
