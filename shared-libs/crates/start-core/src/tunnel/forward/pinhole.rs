//! IPv6 GUA firewall pinholes for StartTunnel: the dataplane behind PCP v6 MAPs
//! and the manual pinhole API. A pinhole accepts inbound to a client's own
//! global address with no NAT; a port remap (external != internal, e.g. the
//! 80→443 redirect) additionally DNATs on that same GUA. Rules live in
//! `table ip6 startos`, mirroring the v4 forward path — the destination is
//! always the client's own GUA, so a client can only ever expose itself.

use std::net::{Ipv6Addr, SocketAddrV6};

use crate::net::forward::nft_rule_v6;
use crate::net::transport::Transport;
use crate::prelude::*;
use crate::tunnel::context::TunnelContext;
use crate::tunnel::db::{Pinhole, Pinholes6};
use crate::tunnel::forward::address::PinholeAddress;
use crate::tunnel::wg6::host_v6;

/// nft dport token for `count` ports counting up from `base`: `8443` or `8000-8009`.
fn port_span(base: u16, count: u16) -> String {
    if count <= 1 {
        base.to_string()
    } else {
        format!("{base}-{}", base.saturating_add(count - 1))
    }
}

/// Comment tag shared by every nft rule of one pinhole, keyed by its exposed
/// `[GUA]:external_port` — so the whole entry tears down by that tag.
fn tag(gua: Ipv6Addr, external_port: u16, transport: Transport) -> String {
    format!(
        "pinhole:{}",
        PinholeAddress::new(SocketAddrV6::new(gua, external_port, 0, 0), transport)
    )
}

/// Install (reconciling both chains) the nft rules for a pinhole at
/// `[gua]:external_port` delivering to `[gua]:internal_port` for `count` ports.
/// Ports equal → a pure forward-chain accept, no NAT. Ports differ → a prerouting
/// DNAT plus a forward accept on the internal port.
pub async fn apply_pinhole(
    gua: Ipv6Addr,
    external_port: u16,
    internal_port: u16,
    count: u16,
    transport: Transport,
) -> Result<(), Error> {
    let comment = tag(gua, external_port, transport);
    let protocols = match transport {
        Transport::Tcp => "tcp",
        Transport::Udp => "udp",
        Transport::Both => "{ tcp, udp }",
    };
    let ext = port_span(external_port, count);
    let int = port_span(internal_port, count);
    if internal_port == external_port {
        let accept =
            format!("ip6 daddr {gua} meta l4proto {protocols} th dport {ext} ct state new accept");
        nft_rule_v6("forward", &comment, false, false, &accept).await?;
        // Pure pinhole: ensure no stale DNAT lingers from a prior remap.
        nft_rule_v6("prerouting", &comment, true, false, "").await?;
    } else {
        let dnat = format!(
            "ip6 daddr {gua} meta l4proto {protocols} th dport {ext} dnat to [{gua}]:{int}"
        );
        nft_rule_v6("prerouting", &comment, false, false, &dnat).await?;
        let accept =
            format!("ip6 daddr {gua} meta l4proto {protocols} th dport {int} ct state new accept");
        nft_rule_v6("forward", &comment, false, false, &accept).await?;
    }
    Ok(())
}

/// Remove every nft rule for the pinhole at `[gua]:external_port` (undo is by
/// comment tag, so it needs no rule text and covers both the pinhole and remap
/// shapes).
pub async fn remove_pinhole_rules(
    gua: Ipv6Addr,
    external_port: u16,
    transport: Transport,
) -> Result<(), Error> {
    let comment = tag(gua, external_port, transport);
    nft_rule_v6("prerouting", &comment, true, false, "").await?;
    nft_rule_v6("forward", &comment, true, false, "").await?;
    Ok(())
}

/// Whether `gua` is the `/128` this tunnel delegates to some client — the
/// authorization check for a v6 pinhole. Matches a client's [`host_v6`] on any
/// subnet carrying an IPv6 prefix. The server's own address is never a target.
pub async fn is_known_gua(ctx: &TunnelContext, gua: Ipv6Addr) -> bool {
    let peek = ctx.db.peek().await;
    let Ok(subnets) = peek.as_wg().as_subnets().as_entries() else {
        return false;
    };
    for (_, cfg) in subnets {
        let Ok(Some(prefix)) = cfg.as_ipv6().de() else {
            continue;
        };
        if !prefix.contains(&gua) {
            continue;
        }
        let Ok(clients) = cfg.as_clients().keys() else {
            continue;
        };
        if clients.into_iter().any(|v4| host_v6(prefix, v4) == gua) {
            return true;
        }
    }
    false
}

/// Returns whether the pinhole is enabled. An existing entry keeps its owner
/// and enabled state.
fn upsert_pinhole(
    pinholes: &mut Pinholes6,
    key: PinholeAddress,
    internal_port: u16,
    count: u16,
    label: Option<String>,
    auto: bool,
) -> Result<bool, Error> {
    if let Some(conflict) = pinholes.overlapping(key, count) {
        return Err(Error::new(
            eyre!("{key} overlaps an existing pinhole at {conflict}"),
            ErrorKind::InvalidRequest,
        ));
    }
    let pinhole = pinholes.0.entry(key).or_insert_with(|| Pinhole {
        label: auto.then(|| "PCP".to_string()),
        enabled: true,
        count,
        internal_port: None,
        auto,
    });
    if pinhole.auto != auto
        && (pinhole.internal_port(key.port()) != internal_port || pinhole.count != count)
    {
        return Err(Error::new(
            eyre!("{key} is held by a different mapping"),
            ErrorKind::InvalidRequest,
        ));
    }
    pinhole.internal_port = (internal_port != key.port()).then_some(internal_port);
    pinhole.count = count;
    if label.is_some() {
        pinhole.label = label;
    }
    Ok(pinhole.enabled)
}

/// Persists a pinhole and installs its rules if it is enabled.
pub async fn add_pinhole(
    ctx: &TunnelContext,
    gua: Ipv6Addr,
    external_port: u16,
    internal_port: u16,
    count: u16,
    label: Option<String>,
    auto: bool,
    transport: Transport,
) -> Result<(), Error> {
    let _guard = ctx.forward_write_lock.lock().await;
    let key = PinholeAddress::new(SocketAddrV6::new(gua, external_port, 0, 0), transport);
    let enabled = ctx
        .db
        .mutate(|db| {
            db.as_pinholes6_mut()
                .mutate(|ph| upsert_pinhole(ph, key, internal_port, count, label, auto))
        })
        .await
        .result?;
    if enabled {
        apply_pinhole(gua, external_port, internal_port, count, transport).await?;
    }
    Ok(())
}

/// Enable or disable a pinhole, installing or tearing down its nft rules to match.
pub async fn set_pinhole_enabled(
    ctx: &TunnelContext,
    gua: Ipv6Addr,
    external_port: u16,
    enabled: bool,
    transport: Transport,
) -> Result<(), Error> {
    let _guard = ctx.forward_write_lock.lock().await;
    let entries = ctx
        .db
        .mutate(|db| {
            db.as_pinholes6_mut().mutate(|ph| {
                let mut changed = Vec::new();
                for (key, entry) in &mut ph.0 {
                    if matches_key(*key, gua, external_port, transport) {
                        entry.enabled = enabled;
                        changed.push((*key, entry.clone()));
                    }
                }
                if changed.is_empty() {
                    return Err(Error::new(
                        eyre!("no pinhole at {gua}:{external_port}"),
                        ErrorKind::NotFound,
                    ));
                }
                Ok(changed)
            })
        })
        .await
        .result?;
    for (key, entry) in entries {
        if enabled {
            apply_pinhole(
                gua,
                external_port,
                entry.internal_port(external_port),
                entry.count,
                key.transport,
            )
            .await?;
        } else {
            remove_pinhole_rules(gua, external_port, key.transport).await?;
        }
    }
    Ok(())
}

pub async fn set_pinhole_label(
    ctx: &TunnelContext,
    gua: Ipv6Addr,
    external_port: u16,
    label: Option<String>,
    transport: Transport,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_pinholes6_mut().mutate(|ph| {
                let mut found = false;
                for (key, entry) in &mut ph.0 {
                    if matches_key(*key, gua, external_port, transport) {
                        entry.label = label.clone();
                        found = true;
                    }
                }
                if !found {
                    return Err(Error::new(
                        eyre!("no pinhole at {gua}:{external_port}"),
                        ErrorKind::NotFound,
                    ));
                }
                Ok(())
            })
        })
        .await
        .result
}

fn matches_key(key: PinholeAddress, gua: Ipv6Addr, port: u16, transport: Transport) -> bool {
    *key.ip() == gua
        && key.port() == port
        && (transport == Transport::Both || key.transport == transport)
}

/// Returns whether a pinhole was removed.
pub async fn remove_pinhole(
    ctx: &TunnelContext,
    gua: Ipv6Addr,
    external_port: u16,
    auto_only: bool,
    transport: Transport,
) -> bool {
    let _guard = ctx.forward_write_lock.lock().await;
    let removed = ctx
        .db
        .mutate(|db| {
            db.as_pinholes6_mut().mutate(|ph| {
                let keys: Vec<_> =
                    ph.0.iter()
                        .filter(|(key, entry)| {
                            matches_key(**key, gua, external_port, transport)
                                && (!auto_only || entry.auto)
                        })
                        .map(|(key, _)| *key)
                        .collect();
                for key in &keys {
                    ph.0.remove(key);
                }
                Ok(keys)
            })
        })
        .await
        .result
        .log_err()
        .unwrap_or_default();
    for key in &removed {
        remove_pinhole_rules(gua, external_port, key.transport)
            .await
            .log_err();
        crate::tunnel::forward::lease::forget(
            ctx,
            &crate::tunnel::forward::lease::LeaseKey::Pinhole(*key),
        );
    }
    !removed.is_empty()
}

/// Reinstall every enabled pinhole's nft rules from the db (startup / resync).
pub async fn seed_pinholes(ctx: &TunnelContext) -> Result<(), Error> {
    for (key, ph) in ctx.db.peek().await.as_pinholes6().de()?.0 {
        // Upgrades retain socket-only pinhole tags in the kernel.
        if key.transport != Transport::Both {
            remove_pinhole_rules(*key.ip(), key.port(), Transport::Both).await?;
        }
        if !ph.enabled {
            continue;
        }
        apply_pinhole(
            *key.ip(),
            key.port(),
            ph.internal_port(key.port()),
            ph.count,
            key.transport,
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_pinhole_updates_preserve_the_other_transport() {
        let address = "[2001:db8::2]:5000".parse().unwrap();
        let tcp = PinholeAddress::new(address, Transport::Tcp);
        let udp = PinholeAddress::new(address, Transport::Udp);
        let mut pinholes = Pinholes6::default();
        upsert_pinhole(&mut pinholes, tcp, 6000, 2, None, true).unwrap();
        upsert_pinhole(&mut pinholes, udp, 7000, 2, None, true).unwrap();
        pinholes.0.get_mut(&udp).unwrap().enabled = false;
        upsert_pinhole(&mut pinholes, tcp, 8000, 2, None, true).unwrap();
        assert!(!pinholes.0[&udp].enabled);
        assert_eq!(pinholes.0[&udp].internal_port(5000), 7000);
        assert_eq!(pinholes.0[&tcp].internal_port(5000), 8000);
        assert!(matches_key(tcp, *address.ip(), 5000, Transport::Tcp));
        assert!(!matches_key(udp, *address.ip(), 5000, Transport::Tcp));
        assert!(matches_key(udp, *address.ip(), 5000, Transport::Both));
    }

    const KEY: &str = "[2001:db8::2]:443";

    fn pinholes(enabled: bool, auto: bool) -> Pinholes6 {
        let mut pinholes = Pinholes6::default();
        pinholes.0.insert(
            KEY.parse().unwrap(),
            Pinhole {
                label: None,
                enabled,
                count: 1,
                internal_port: Some(8443),
                auto,
            },
        );
        pinholes
    }

    fn upsert(pinholes: &mut Pinholes6, internal_port: u16, auto: bool) -> Result<bool, Error> {
        upsert_pinhole(pinholes, KEY.parse().unwrap(), internal_port, 1, None, auto)
    }

    #[test]
    fn a_renewal_leaves_a_disabled_pinhole_disabled() {
        let mut pinholes = pinholes(false, true);
        assert!(!upsert(&mut pinholes, 8443, true).unwrap());
        assert!(!pinholes.0[&KEY.parse().unwrap()].enabled);
    }

    #[test]
    fn a_device_changes_its_own_internal_port() {
        let mut pinholes = pinholes(true, true);
        assert!(upsert(&mut pinholes, 9443, true).unwrap());
        assert_eq!(pinholes.0[&KEY.parse().unwrap()].internal_port, Some(9443));
    }

    #[test]
    fn a_device_renews_a_manual_pinhole_without_owning_it() {
        let mut pinholes = pinholes(true, false);
        assert!(upsert(&mut pinholes, 8443, true).unwrap());
        assert!(!pinholes.0[&KEY.parse().unwrap()].auto);
    }

    #[test]
    fn a_device_cannot_change_a_manual_pinhole() {
        let mut pinholes = pinholes(true, false);
        assert!(upsert(&mut pinholes, 9443, true).is_err());
        assert_eq!(pinholes.0[&KEY.parse().unwrap()].internal_port, Some(8443));
    }
}
