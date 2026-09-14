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

use tokio::sync::broadcast::Receiver;

use crate::prelude::*;
use crate::tunnel::context::TunnelContext;
use crate::tunnel::db::PortForward;
use crate::tunnel::forward::shutdown_pending;

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
pub async fn run(ctx: TunnelContext, mut shutdown: Receiver<Option<bool>>) {
    loop {
        if shutdown_pending(&mut shutdown) {
            break;
        }
        match reap_expired(&ctx).await {
            Some(next) => {
                tokio::select! {
                    biased;
                    _ = shutdown.recv() => break,
                    _ = tokio::time::sleep_until(tokio::time::Instant::from_std(next)) => {}
                    _ = ctx.lease_wake.notified() => {}
                }
            }
            None => {
                tokio::select! {
                    biased;
                    _ = shutdown.recv() => break,
                    _ = ctx.lease_wake.notified() => {}
                }
            }
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
        let result = match &key {
            LeaseKey::Dnat(source) => reap_dnat(ctx, *source).await,
            LeaseKey::Sni { source, hostname } => reap_sni(ctx, *source, hostname).await,
            LeaseKey::SniFallback(source) => reap_sni_fallback(ctx, *source).await,
            LeaseKey::Pinhole(k) => reap_pinhole(ctx, *k).await,
        };
        let success = result.is_ok();
        result.log_err();
        ctx.leases
            .mutate(|leases| finish_reap(leases, &key, now, success));
    }
    ctx.leases.peek(|l| l.values().min().copied())
}

fn finish_reap(leases: &mut Leases, key: &LeaseKey, observed: Instant, success: bool) {
    if let Some(expiry) = leases.get_mut(key).filter(|expiry| **expiry <= observed) {
        if success {
            leases.remove(key);
        } else {
            *expiry = Instant::now() + Duration::from_secs(5);
        }
    }
}

async fn reap_dnat(ctx: &TunnelContext, source: SocketAddrV4) -> Result<(), Error> {
    let _guard = ctx.forward_write_lock.lock().await;
    if ctx.leases.peek(|leases| {
        leases
            .get(&LeaseKey::Dnat(source))
            .is_none_or(|expiry| *expiry > Instant::now())
    }) {
        return Ok(());
    }
    // Never touch a manual forward or an SNI-occupied port; only auto DNAT.
    let auto = ctx
        .db
        .peek()
        .await
        .as_port_forwards()
        .de()?
        .0
        .get(&source)
        .is_some_and(|e| matches!(e, PortForward::Dnat { auto: true, .. }));
    if !auto {
        return Ok(());
    }
    drop(ctx.active_forwards.mutate(|m| m.remove(&source)));
    ctx.forward.gc().await?;
    ctx.db
        .mutate(|db| db.as_port_forwards_mut().remove(&source).map(|_| ()))
        .await
        .result?;
    tracing::info!("PCP lease lapsed: removed auto forward {source}");
    Ok(())
}

async fn reap_sni(ctx: &TunnelContext, source: SocketAddrV4, hostname: &str) -> Result<(), Error> {
    let target = ctx
        .db
        .peek()
        .await
        .as_port_forwards()
        .de()?
        .0
        .get(&source)
        .and_then(|entry| match entry {
            PortForward::Sni { routes, .. } => {
                routes.get(hostname).filter(|r| r.auto).map(|r| r.target)
            }
            _ => None,
        });
    let Some(target) = target else {
        return Ok(());
    };
    ctx.remove_sni_forward_result(source, target, &[hostname.to_string()])
        .await?;
    tracing::info!("PCP lease lapsed: removed auto SNI route {hostname} on {source}");
    Ok(())
}

async fn reap_sni_fallback(ctx: &TunnelContext, source: SocketAddrV4) -> Result<(), Error> {
    let target = ctx
        .db
        .peek()
        .await
        .as_port_forwards()
        .de()?
        .0
        .get(&source)
        .and_then(|entry| match entry {
            PortForward::Sni { fallback, .. } => {
                fallback.as_ref().filter(|f| f.auto).map(|f| f.target)
            }
            _ => None,
        });
    let Some(target) = target else {
        return Ok(());
    };
    ctx.remove_sni_fallback(source, target).await?;
    tracing::info!("PCP lease lapsed: removed auto SNI fallback on {source}");
    Ok(())
}

async fn reap_pinhole(ctx: &TunnelContext, key: SocketAddrV6) -> Result<(), Error> {
    let _guard = ctx.forward_write_lock.lock().await;
    if ctx.leases.peek(|leases| {
        leases
            .get(&LeaseKey::Pinhole(key))
            .is_none_or(|expiry| *expiry > Instant::now())
    }) {
        return Ok(());
    }
    let auto = ctx
        .db
        .peek()
        .await
        .as_pinholes6()
        .de()?
        .0
        .get(&key)
        .is_some_and(|p| p.auto);
    if !auto {
        return Ok(());
    }
    crate::tunnel::forward::pinhole::remove_pinhole_locked(ctx, *key.ip(), key.port()).await?;
    tracing::info!("PCP lease lapsed: removed auto pinhole {key}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_reap_retains_lease_and_paces_retry() {
        let now = Instant::now();
        let key = LeaseKey::Pinhole("[2001:db8::1]:443".parse().unwrap());
        let mut leases = Leases::from([(key.clone(), now)]);
        finish_reap(&mut leases, &key, now, false);
        assert!(expired_keys(&leases, now + Duration::from_secs(4)).is_empty());
        assert_eq!(
            expired_keys(&leases, now + Duration::from_secs(6)),
            vec![key.clone()]
        );
        finish_reap(&mut leases, &key, now + Duration::from_secs(6), true);
        assert!(leases.is_empty());
        leases.insert(key.clone(), now + Duration::from_secs(60));
        finish_reap(&mut leases, &key, now, true);
        assert_eq!(leases[&key], now + Duration::from_secs(60));
    }

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
