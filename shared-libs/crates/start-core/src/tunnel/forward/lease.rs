//! In-memory leases for auto (PCP-created) forwards, pinholes, and SNI routes.
//!
//! StartOS renews each PCP mapping before its lease lapses. If it stops (server
//! offline, exposure withdrawn, WireGuard key rotated) the [`reaper`](run) tears
//! the mapping down so a stale auto-forward can't linger on the gateway. Manual
//! (user-added) entries carry no lease and never expire.
//!
//! Volatile by design — leases live here, not in PatchDb, so a client's periodic
//! renewal never churns the persisted config (and never wakes DB subscribers).
//! On startup every auto DB entry is granted a fresh lease ([`seed_from_db`]);
//! the client's re-MAP after a tunnel restart (RFC 6887 §8.5 epoch reset) renews
//! it, and anything a departed client never renews is reaped after one lease.

use std::collections::BTreeMap;
use std::net::{SocketAddrV4, SocketAddrV6};
use std::time::{Duration, Instant};

use crate::net::port_map::server::GatewayBackend;
use crate::prelude::*;
use crate::tunnel::context::TunnelContext;
use crate::tunnel::db::PortForward;

/// Lease granted to an auto entry restored from the DB on startup; the client's
/// re-MAP refreshes it well within this. Matches the server's max granted lease.
const STARTUP_LEASE_SECONDS: u32 = 3600;

/// Identity of one auto mapping's lease. DNAT and pinhole are keyed by their
/// single external address; an SNI route is per-hostname, since many hostnames
/// share one external port, each an independent client-owned mapping.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum LeaseKey {
    Dnat(SocketAddrV4),
    Sni {
        source: SocketAddrV4,
        hostname: String,
    },
    /// The hostname-less fallback on an SNI-demuxed port, keyed by its single
    /// external address (there is at most one fallback per port).
    SniFallback(SocketAddrV4),
    Pinhole(SocketAddrV6),
}

pub type Leases = BTreeMap<LeaseKey, Instant>;

/// Stamp (or refresh) an auto mapping's lease to `now + lifetime`, and wake the
/// reaper so it can pull its next wake-up earlier if this lease is now the
/// soonest to expire.
pub fn stamp(ctx: &TunnelContext, key: LeaseKey, lifetime: u32) {
    let expiry = Instant::now() + Duration::from_secs(u64::from(lifetime));
    ctx.leases.mutate(|l| {
        l.insert(key, expiry);
    });
    ctx.lease_wake.notify_one();
}

/// Forget a lease (the mapping was explicitly removed).
pub fn forget(ctx: &TunnelContext, key: &LeaseKey) {
    ctx.leases.mutate(|l| {
        l.remove(key);
    });
}

/// Grant a fresh lease to every auto forward / pinhole / SNI route in the DB —
/// run once at startup so restored auto entries expire if their client never
/// returns. Manual entries are left unleased (permanent).
pub async fn seed_from_db(ctx: &TunnelContext) -> Result<(), Error> {
    let peek = ctx.db.peek().await;
    let expiry = Instant::now() + Duration::from_secs(u64::from(STARTUP_LEASE_SECONDS));
    let mut seed = Vec::new();
    for (source, entry) in peek.as_port_forwards().de()?.0 {
        match entry {
            PortForward::Dnat { auto: true, .. } => seed.push(LeaseKey::Dnat(source)),
            PortForward::Sni { routes, fallback } => {
                for (hostname, route) in routes {
                    if route.auto {
                        seed.push(LeaseKey::Sni { source, hostname });
                    }
                }
                if fallback.is_some_and(|f| f.auto) {
                    seed.push(LeaseKey::SniFallback(source));
                }
            }
            PortForward::Dnat { .. } => {}
        }
    }
    for (key, ph) in peek.as_pinholes6().de()?.0 {
        if ph.auto {
            seed.push(LeaseKey::Pinhole(key));
        }
    }
    ctx.leases.mutate(|l| {
        for key in seed {
            l.entry(key).or_insert(expiry);
        }
    });
    Ok(())
}

/// The reaper: tear down any auto mapping whose lease has lapsed, then sleep
/// exactly until the soonest remaining lease is due (or until a newly stamped,
/// sooner lease wakes it). Runs for the life of the tunnel.
pub async fn run(ctx: TunnelContext) {
    loop {
        match reap_expired(&ctx).await {
            Some(next) => {
                tokio::select! {
                    _ = tokio::time::sleep_until(tokio::time::Instant::from_std(next)) => {}
                    _ = ctx.lease_wake.notified() => {}
                }
            }
            // No leases outstanding — wait for the next stamp to wake us.
            None => ctx.lease_wake.notified().await,
        }
    }
}

/// Keys whose lease has lapsed as of `now`.
fn expired_keys(leases: &Leases, now: Instant) -> Vec<LeaseKey> {
    leases
        .iter()
        .filter(|(_, exp)| **exp <= now)
        .map(|(k, _)| k.clone())
        .collect()
}

/// Reap every lapsed auto mapping, returning the soonest still-pending expiry
/// (the reaper's next wake-up), or `None` if no leases remain.
async fn reap_expired(ctx: &TunnelContext) -> Option<Instant> {
    let now = Instant::now();
    let expired = ctx.leases.peek(|l| expired_keys(l, now));
    for key in expired {
        // Re-check under the lock: a renewal between the snapshot and here
        // re-stamps a later expiry, in which case the client still wants it.
        if ctx
            .leases
            .peek(|l| l.get(&key).is_none_or(|exp| *exp > now))
        {
            continue;
        }
        match &key {
            LeaseKey::Dnat(source) => reap_dnat(ctx, *source).await,
            LeaseKey::Sni { source, hostname } => reap_sni(ctx, *source, hostname).await,
            LeaseKey::SniFallback(source) => reap_sni_fallback(ctx, *source).await,
            LeaseKey::Pinhole(k) => reap_pinhole(ctx, *k).await,
        }
        // Drop the lease unless a renewal extended it while we reaped.
        ctx.leases.mutate(|l| {
            if l.get(&key).is_some_and(|exp| *exp <= now) {
                l.remove(&key);
            }
        });
    }
    ctx.leases.peek(|l| l.values().min().copied())
}

async fn reap_dnat(ctx: &TunnelContext, source: SocketAddrV4) {
    // Never touch a manual forward or an SNI-occupied port; only auto DNAT.
    let auto = ctx
        .db
        .peek()
        .await
        .as_port_forwards()
        .de()
        .ok()
        .and_then(|pf| pf.0.get(&source).cloned())
        .is_some_and(|e| matches!(e, PortForward::Dnat { auto: true, .. }));
    if !auto {
        return;
    }
    if ctx
        .db
        .mutate(|db| db.as_port_forwards_mut().remove(&source).map(|_| ()))
        .await
        .result
        .is_ok()
    {
        if let Some(rc) = ctx.active_forwards.mutate(|m| m.remove(&source)) {
            drop(rc);
            ctx.forward.gc().await.log_err();
        }
        tracing::info!("PCP lease lapsed: removed auto forward {source}");
    }
}

async fn reap_sni(ctx: &TunnelContext, source: SocketAddrV4, hostname: &str) {
    let target = ctx
        .db
        .peek()
        .await
        .as_port_forwards()
        .de()
        .ok()
        .and_then(|pf| match pf.0.get(&source) {
            Some(PortForward::Sni { routes, .. }) => {
                routes.get(hostname).filter(|r| r.auto).map(|r| r.target)
            }
            _ => None,
        });
    let Some(target) = target else {
        return;
    };
    ctx.remove_sni_forward(source, target, &[hostname.to_string()])
        .await;
    tracing::info!("PCP lease lapsed: removed auto SNI route {hostname} on {source}");
}

async fn reap_sni_fallback(ctx: &TunnelContext, source: SocketAddrV4) {
    let target = ctx
        .db
        .peek()
        .await
        .as_port_forwards()
        .de()
        .ok()
        .and_then(|pf| match pf.0.get(&source) {
            Some(PortForward::Sni { fallback, .. }) => {
                fallback.as_ref().filter(|f| f.auto).map(|f| f.target)
            }
            _ => None,
        });
    let Some(target) = target else {
        return;
    };
    ctx.remove_sni_fallback(source, target).await;
    tracing::info!("PCP lease lapsed: removed auto SNI fallback on {source}");
}

async fn reap_pinhole(ctx: &TunnelContext, key: SocketAddrV6) {
    let auto = ctx
        .db
        .peek()
        .await
        .as_pinholes6()
        .de()
        .ok()
        .and_then(|ph| ph.0.get(&key).cloned())
        .is_some_and(|p| p.auto);
    if !auto {
        return;
    }
    crate::tunnel::forward::pinhole::remove_pinhole(ctx, *key.ip(), key.port()).await;
    tracing::info!("PCP lease lapsed: removed auto pinhole {key}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_lapsed_leases_are_selected() {
        let now = Instant::now();
        let mut leases = Leases::new();
        let live = LeaseKey::Dnat("1.2.3.4:443".parse().unwrap());
        let lapsed = LeaseKey::Dnat("1.2.3.4:8443".parse().unwrap());
        let boundary = LeaseKey::Pinhole("[2001:db8::1]:443".parse().unwrap());
        leases.insert(live.clone(), now + Duration::from_secs(60));
        leases.insert(lapsed.clone(), now - Duration::from_secs(1));
        leases.insert(boundary.clone(), now); // exactly due

        let mut expired = expired_keys(&leases, now);
        expired.sort();
        let mut want = vec![lapsed, boundary];
        want.sort();
        assert_eq!(expired, want, "live lease must survive, due/lapsed reaped");
    }

    // SNI leases are per-hostname: two hostnames sharing one external port are
    // independent, so one lapsing never selects the other.
    #[test]
    fn sni_leases_are_per_hostname() {
        let now = Instant::now();
        let source: SocketAddrV4 = "5.6.7.8:443".parse().unwrap();
        let mut leases = Leases::new();
        let a = LeaseKey::Sni {
            source,
            hostname: "a.example.com".into(),
        };
        let b = LeaseKey::Sni {
            source,
            hostname: "b.example.com".into(),
        };
        leases.insert(a.clone(), now - Duration::from_secs(1));
        leases.insert(b.clone(), now + Duration::from_secs(60));
        assert_eq!(expired_keys(&leases, now), vec![a]);
    }
}
