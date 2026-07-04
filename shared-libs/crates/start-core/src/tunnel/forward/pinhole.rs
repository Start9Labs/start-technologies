//! IPv6 GUA firewall pinholes for StartTunnel: the dataplane behind PCP v6 MAPs
//! and the manual pinhole API. A pinhole accepts inbound to a client's own
//! global address with no NAT; a port remap (external != internal, e.g. the
//! 80→443 redirect) additionally DNATs on that same GUA. Rules live in
//! `table ip6 startos`, mirroring the v4 forward path — the destination is
//! always the client's own GUA, so a client can only ever expose itself.

use std::net::{Ipv6Addr, SocketAddrV6};

use crate::net::forward::nft_rule_v6;
use crate::prelude::*;
use crate::tunnel::context::TunnelContext;
use crate::tunnel::db::Pinhole;
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
fn tag(gua: Ipv6Addr, external_port: u16) -> String {
    format!("pinhole:{}", SocketAddrV6::new(gua, external_port, 0, 0))
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
) -> Result<(), Error> {
    let comment = tag(gua, external_port);
    let ext = port_span(external_port, count);
    let int = port_span(internal_port, count);
    if internal_port == external_port {
        let accept =
            format!("ip6 daddr {gua} meta l4proto {{ tcp, udp }} th dport {ext} ct state new accept");
        nft_rule_v6("forward", &comment, false, false, &accept).await?;
        // Pure pinhole: ensure no stale DNAT lingers from a prior remap.
        nft_rule_v6("prerouting", &comment, true, false, "").await?;
    } else {
        let dnat = format!(
            "ip6 daddr {gua} meta l4proto {{ tcp, udp }} th dport {ext} dnat to [{gua}]:{int}"
        );
        nft_rule_v6("prerouting", &comment, false, false, &dnat).await?;
        let accept =
            format!("ip6 daddr {gua} meta l4proto {{ tcp, udp }} th dport {int} ct state new accept");
        nft_rule_v6("forward", &comment, false, false, &accept).await?;
    }
    Ok(())
}

/// Remove every nft rule for the pinhole at `[gua]:external_port` (undo is by
/// comment tag, so it needs no rule text and covers both the pinhole and remap
/// shapes).
pub async fn remove_pinhole_rules(gua: Ipv6Addr, external_port: u16) -> Result<(), Error> {
    let comment = tag(gua, external_port);
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

/// Persist and install a pinhole, with the target forced to `gua` (the caller's
/// own address). Rejects a range overlapping a *different* existing pinhole on
/// the same GUA; an exact-key re-assert refreshes it. `auto` marks a
/// PCP-created entry (part of the UI Automatic split); `label` is the manual
/// caller's label (PCP passes `None`, defaulting a fresh auto entry to `PCP`).
pub async fn add_pinhole(
    ctx: &TunnelContext,
    gua: Ipv6Addr,
    external_port: u16,
    internal_port: u16,
    count: u16,
    label: Option<String>,
    auto: bool,
) -> Result<(), Error> {
    let key = SocketAddrV6::new(gua, external_port, 0, 0);
    let internal = (internal_port != external_port).then_some(internal_port);
    ctx.db
        .mutate(|db| {
            db.as_pinholes6_mut().mutate(|ph| {
                if let Some(conflict) = ph.overlapping(key, count) {
                    return Err(Error::new(
                        eyre!("{key} overlaps an existing pinhole at {conflict}"),
                        ErrorKind::InvalidRequest,
                    ));
                }
                let existing = ph.0.get(&key);
                // A renewal keeps the user's enabled state; the label prefers an
                // explicit one, then any existing, then `PCP` for a fresh auto entry.
                let enabled = existing.map_or(true, |p| p.enabled);
                let label = label
                    .clone()
                    .or_else(|| existing.and_then(|p| p.label.clone()))
                    .or_else(|| auto.then(|| "PCP".to_string()));
                ph.0.insert(
                    key,
                    Pinhole {
                        label,
                        enabled,
                        count,
                        internal_port: internal,
                        auto,
                    },
                );
                Ok(())
            })
        })
        .await
        .result?;
    apply_pinhole(gua, external_port, internal_port, count).await
}

/// Enable or disable a pinhole, installing or tearing down its nft rules to match.
pub async fn set_pinhole_enabled(
    ctx: &TunnelContext,
    gua: Ipv6Addr,
    external_port: u16,
    enabled: bool,
) -> Result<(), Error> {
    let key = SocketAddrV6::new(gua, external_port, 0, 0);
    ctx.db
        .mutate(|db| {
            db.as_pinholes6_mut().mutate(|ph| {
                let e = ph.0.get_mut(&key).ok_or_else(|| {
                    Error::new(eyre!("no pinhole at {key}"), ErrorKind::NotFound)
                })?;
                e.enabled = enabled;
                Ok(())
            })
        })
        .await
        .result?;
    if enabled {
        let ph = ctx.db.peek().await.as_pinholes6().de()?;
        if let Some(e) = ph.0.get(&key) {
            apply_pinhole(gua, external_port, e.internal_port(external_port), e.count).await?;
        }
    } else {
        remove_pinhole_rules(gua, external_port).await?;
    }
    Ok(())
}

/// Relabel a pinhole (no dataplane change).
pub async fn set_pinhole_label(
    ctx: &TunnelContext,
    gua: Ipv6Addr,
    external_port: u16,
    label: Option<String>,
) -> Result<(), Error> {
    let key = SocketAddrV6::new(gua, external_port, 0, 0);
    ctx.db
        .mutate(|db| {
            db.as_pinholes6_mut().mutate(|ph| {
                let e = ph.0.get_mut(&key).ok_or_else(|| {
                    Error::new(eyre!("no pinhole at {key}"), ErrorKind::NotFound)
                })?;
                e.label = label.clone();
                Ok(())
            })
        })
        .await
        .result?;
    Ok(())
}

/// Remove the pinhole at `[gua]:external_port` from the db and tear down its
/// nft rules.
pub async fn remove_pinhole(ctx: &TunnelContext, gua: Ipv6Addr, external_port: u16) {
    let key = SocketAddrV6::new(gua, external_port, 0, 0);
    let removed = ctx
        .db
        .mutate(|db| db.as_pinholes6_mut().remove(&key).map(|_| ()))
        .await
        .result;
    if removed.is_ok() {
        remove_pinhole_rules(gua, external_port).await.log_err();
    }
}

/// Reinstall every enabled pinhole's nft rules from the db (startup / resync).
pub async fn seed_pinholes(ctx: &TunnelContext) -> Result<(), Error> {
    for (key, ph) in ctx.db.peek().await.as_pinholes6().de()?.0 {
        if !ph.enabled {
            continue;
        }
        apply_pinhole(*key.ip(), key.port(), ph.internal_port(key.port()), ph.count).await?;
    }
    Ok(())
}
