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

use crate::net::port_map::Protocol;
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
        protocol: Protocol,
    },
    /// The hostname-less fallback on an SNI-demuxed port, keyed by its single
    /// external address (there is at most one fallback per port).
    SniFallback {
        source: SocketAddrV4,
        protocol: Protocol,
    },
    Pinhole(SocketAddrV6),
}

impl LeaseKey {
    pub fn sni(
        source: SocketAddrV4,
        hostname: Option<&str>,
        protocol: impl Into<Protocol>,
    ) -> Self {
        let protocol = protocol.into();
        match hostname {
            Some(hostname) => Self::Sni {
                source,
                hostname: hostname.to_owned(),
                protocol,
            },
            None => Self::SniFallback { source, protocol },
        }
    }

    fn automatic_target(
        &self,
        forwards: &BTreeMap<SocketAddrV4, PortForward>,
    ) -> Option<SocketAddrV4> {
        let (target, auto) = match self {
            Self::Dnat(source) => match forwards.get(source)? {
                PortForward::Dnat { target, auto, .. } => Some((*target, *auto)),
                _ => None,
            },
            Self::Sni {
                source,
                hostname,
                protocol,
            } => forwards
                .get(source)?
                .bindings(*protocol)?
                .route(Some(hostname))
                .map(|r| (r.target, r.auto)),
            Self::SniFallback { source, protocol } => forwards
                .get(source)?
                .bindings(*protocol)?
                .route(None)
                .map(|r| (r.target, r.auto)),
            Self::Pinhole(_) => None,
        }?;
        auto.then_some(target)
    }
}

/// Carries deadlines with automatic mappings that retain their target after a WAN-key change.
pub fn rekey(
    leases: &mut Leases,
    moves: &[(LeaseKey, LeaseKey)],
    old: &BTreeMap<SocketAddrV4, PortForward>,
    want: &BTreeMap<SocketAddrV4, PortForward>,
) {
    let previous = std::mem::take(leases);
    for (key, expiry) in previous {
        if matches!(key, LeaseKey::Pinhole(_)) {
            leases.insert(key, expiry);
            continue;
        }
        if let Some((_, new)) = moves.iter().find(|(old, _)| *old == key) {
            if key
                .automatic_target(old)
                .is_some_and(|target| Some(target) == new.automatic_target(want))
            {
                leases
                    .entry(new.clone())
                    .and_modify(|e| *e = (*e).max(expiry))
                    .or_insert(expiry);
            }
        }
    }
}

pub type Leases = BTreeMap<LeaseKey, Instant>;

/// Transfers the DNAT deadline to its single combined fallback.
pub fn convert_dnat_to_sni(leases: &mut Leases, source: SocketAddrV4) {
    if let Some(expiry) = leases.remove(&LeaseKey::Dnat(source)) {
        leases.insert(LeaseKey::sni(source, None, Protocol::TcpUdp), expiry);
    }
}

/// Stamp (or refresh) an auto mapping's lease to `now + lifetime`, and wake the
/// reaper so it can pull its next wake-up earlier if this lease is now the
/// soonest to expire. The caller holds `forward_write_lock` through renewal.
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
            PortForward::Sni { .. } => {
                for (protocol, hostname, route) in entry.sni_routes() {
                    if route.auto {
                        seed.push(LeaseKey::sni(source, hostname, protocol));
                    }
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
        let Some(_guard) = lock_expired(&ctx.forward_write_lock, &ctx.leases, &key, now).await
        else {
            continue;
        };
        match &key {
            LeaseKey::Dnat(source) => reap_dnat(ctx, *source).await,
            LeaseKey::Sni {
                source,
                hostname,
                protocol,
            } => reap_sni(ctx, *source, hostname, *protocol).await,
            LeaseKey::SniFallback { source, protocol } => {
                reap_sni_fallback(ctx, *source, *protocol).await
            }
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

async fn lock_expired<'a>(
    write_lock: &'a tokio::sync::Mutex<()>,
    leases: &crate::util::sync::SyncMutex<Leases>,
    key: &LeaseKey,
    now: Instant,
) -> Option<tokio::sync::MutexGuard<'a, ()>> {
    let guard = write_lock.lock().await;
    leases
        .peek(|l| l.get(key).is_some_and(|expiry| *expiry <= now))
        .then_some(guard)
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

async fn reap_sni(ctx: &TunnelContext, source: SocketAddrV4, hostname: &str, protocol: Protocol) {
    let target = ctx
        .db
        .peek()
        .await
        .as_port_forwards()
        .de()
        .ok()
        .and_then(|pf| match pf.0.get(&source) {
            Some(entry @ PortForward::Sni { .. }) => entry
                .bindings(protocol)
                .unwrap()
                .routes
                .get(hostname)
                .filter(|r| r.auto)
                .map(|r| r.target),
            _ => None,
        });
    let Some(target) = target else {
        return;
    };
    ctx.remove_sni_routes_locked(source, target, &[hostname.to_string()], protocol, true)
        .await;
    tracing::info!("PCP lease lapsed: removed auto SNI route {hostname} on {source}");
}

async fn reap_sni_fallback(ctx: &TunnelContext, source: SocketAddrV4, protocol: Protocol) {
    let target = ctx
        .db
        .peek()
        .await
        .as_port_forwards()
        .de()
        .ok()
        .and_then(|pf| match pf.0.get(&source) {
            Some(entry @ PortForward::Sni { .. }) => entry
                .bindings(protocol)
                .unwrap()
                .fallback
                .as_ref()
                .filter(|f| f.auto)
                .map(|f| f.target),
            _ => None,
        });
    let Some(target) = target else {
        return;
    };
    ctx.remove_sni_fallback_locked(source, target, protocol)
        .await;
    tracing::info!("PCP lease lapsed: removed auto SNI fallback on {source}");
}

async fn reap_pinhole(ctx: &TunnelContext, key: SocketAddrV6) {
    if crate::tunnel::forward::pinhole::remove_pinhole_locked(ctx, *key.ip(), key.port(), true)
        .await
    {
        tracing::info!("PCP lease lapsed: removed auto pinhole {key}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::port_map::TransportProtocol;

    #[tokio::test]
    async fn a_renewal_before_expiry_lock_preserves_the_mapping() {
        let source = "203.0.113.1:443".parse().unwrap();
        for key in [
            LeaseKey::Dnat(source),
            LeaseKey::sni(source, Some("quic.example.com"), TransportProtocol::Udp),
            LeaseKey::sni(source, None, TransportProtocol::Tcp),
            LeaseKey::sni(source, Some("combined.example.com"), Protocol::TcpUdp),
            LeaseKey::sni(source, None, Protocol::TcpUdp),
            LeaseKey::Pinhole("[2001:db8::1]:443".parse().unwrap()),
        ] {
            let lock = tokio::sync::Mutex::new(());
            let now = Instant::now();
            let leases = crate::util::sync::SyncMutex::new(Leases::from([(key.clone(), now)]));
            let renewal = lock.lock().await;
            let reap = lock_expired(&lock, &leases, &key, now);
            tokio::pin!(reap);
            assert!(futures::poll!(&mut reap).is_pending());
            leases.mutate(|leases| {
                leases.insert(key.clone(), now + Duration::from_secs(60));
            });
            drop(renewal);
            assert!(reap.await.is_none());
            assert!(
                lock_expired(&lock, &leases, &key, now + Duration::from_secs(60))
                    .await
                    .is_some()
            );
        }
    }

    #[test]
    fn dnat_conversion_transfers_one_deadline_to_the_combined_fallback() {
        let source = "203.0.113.1:443".parse().unwrap();
        let expiry = Instant::now();
        let mut leases = Leases::from([(LeaseKey::Dnat(source), expiry)]);
        convert_dnat_to_sni(&mut leases, source);
        let combined = LeaseKey::sni(source, None, Protocol::TcpUdp);
        assert_eq!(leases.len(), 1);
        assert_eq!(leases[&combined], expiry);
        assert_eq!(expired_keys(&leases, expiry), vec![combined]);
        let mut manual = Leases::new();
        convert_dnat_to_sni(&mut manual, source);
        assert!(manual.is_empty());
    }

    #[test]
    fn wan_rekey_does_not_give_a_manual_destination_an_incoming_automatic_lease() {
        use crate::tunnel::db::SniRoute;
        let source = "192.0.2.1:443".parse().unwrap();
        let destination = "203.0.113.1:443".parse().unwrap();
        for protocol in Protocol::ALL {
            for hostname in [Some("service.example.com"), None] {
                let mut entry = PortForward::empty_sni();
                let route = SniRoute {
                    target: "10.59.0.2:443".parse().unwrap(),
                    label: None,
                    enabled: true,
                    auto: true,
                };
                let bindings = entry.bindings_mut(protocol).unwrap();
                match hostname {
                    Some(hostname) => {
                        bindings.routes.insert(hostname.to_owned(), route);
                    }
                    None => bindings.fallback = Some(route),
                }
                let old = BTreeMap::from([(source, entry.clone())]);
                entry
                    .bindings_mut(protocol)
                    .unwrap()
                    .route_mut(hostname)
                    .unwrap()
                    .auto = false;
                let want = BTreeMap::from([(destination, entry)]);
                let key = LeaseKey::sni(source, hostname, protocol);
                let mut leases = Leases::from([(key.clone(), Instant::now())]);
                rekey(
                    &mut leases,
                    &[(key, LeaseKey::sni(destination, hostname, protocol))],
                    &old,
                    &want,
                );
                assert!(leases.is_empty());
            }
        }
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
            protocol: Protocol::Tcp,
        };
        let b = LeaseKey::Sni {
            source,
            hostname: "b.example.com".into(),
            protocol: Protocol::Tcp,
        };
        leases.insert(a.clone(), now - Duration::from_secs(1));
        leases.insert(b.clone(), now + Duration::from_secs(60));
        assert_eq!(expired_keys(&leases, now), vec![a]);
    }
    #[test]
    fn transports_and_fallbacks_have_independent_lease_deadlines() {
        let source = "203.0.113.1:443".parse().unwrap();
        let now = Instant::now();
        let mut leases = Leases::new();
        let tcp = LeaseKey::sni(source, Some("service.example.com"), TransportProtocol::Tcp);
        let udp = LeaseKey::sni(source, Some("service.example.com"), TransportProtocol::Udp);
        let tcp_fallback = LeaseKey::sni(source, None, TransportProtocol::Tcp);
        let udp_fallback = LeaseKey::sni(source, None, TransportProtocol::Udp);
        for key in [tcp.clone(), tcp_fallback.clone()] {
            leases.insert(key, now + Duration::from_secs(60));
        }
        for key in [udp.clone(), udp_fallback.clone()] {
            leases.insert(key, now);
        }
        let due = expired_keys(&leases, now);
        assert_eq!(due.len(), 2);
        assert!(due.contains(&udp));
        assert!(due.contains(&udp_fallback));
        assert!(!due.contains(&tcp));
        assert!(!due.contains(&tcp_fallback));
    }

    #[test]
    fn wan_rekey_preserves_each_transport_deadline_and_drops_lost_mappings() {
        let source = "203.0.113.1:443".parse().unwrap();
        let new_source = "203.0.113.2:443".parse().unwrap();
        let mut entry = PortForward::empty_sni();
        for (protocol, target) in [
            (TransportProtocol::Tcp, "10.59.0.2:443"),
            (TransportProtocol::Udp, "10.59.0.3:443"),
        ] {
            let route = crate::tunnel::db::SniRoute {
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
        let old = BTreeMap::from([(source, entry.clone())]);
        let mut want = BTreeMap::from([(new_source, entry)]);
        want.get_mut(&new_source)
            .unwrap()
            .bindings_mut(TransportProtocol::Tcp)
            .unwrap()
            .fallback = None;
        let now = Instant::now();
        let mut leases = Leases::new();
        let mut moves = Vec::new();
        for (protocol, hostname, seconds) in [
            (TransportProtocol::Tcp, Some("service.example.com"), 0),
            (TransportProtocol::Tcp, None, 1),
            (TransportProtocol::Udp, Some("service.example.com"), 2),
            (TransportProtocol::Udp, None, 3),
        ] {
            let from = LeaseKey::sni(source, hostname, protocol);
            let to = LeaseKey::sni(new_source, hostname, protocol);
            leases.insert(from.clone(), now + Duration::from_secs(seconds));
            moves.push((from, to));
        }
        rekey(&mut leases, &moves, &old, &want);
        assert_eq!(leases.len(), 3);
        assert_eq!(
            leases[&LeaseKey::sni(
                new_source,
                Some("service.example.com"),
                TransportProtocol::Tcp
            )],
            now
        );
        assert_eq!(
            leases[&LeaseKey::sni(
                new_source,
                Some("service.example.com"),
                TransportProtocol::Udp
            )],
            now + Duration::from_secs(2)
        );
        assert_eq!(
            leases[&LeaseKey::sni(new_source, None, TransportProtocol::Udp)],
            now + Duration::from_secs(3)
        );
        assert!(!leases.contains_key(&LeaseKey::sni(new_source, None, TransportProtocol::Tcp)));
    }
}
