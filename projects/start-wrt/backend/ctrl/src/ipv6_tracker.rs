//! Event-driven tracker of LAN devices' global IPv6 addresses, and election of
//! each device's *stable* address for published-port rules.
//!
//! The LAN is SLAAC-only in practice, so a device controls its own IPv6
//! addresses: RFC 4941 temporary addresses rotate ~daily, and RFC 7217
//! stable-privacy interface IDs are re-derived whenever the delegated prefix
//! changes. An IPv6 published-port rule pins a concrete address, so the rule
//! must (a) pin the address least likely to move — the stable one — and
//! (b) be retargeted when it moves anyway. This module does both:
//!
//! * A daemon task follows `ip -6 monitor neigh` (plus a periodic ff02::1
//!   prod for quiet devices — the kernel GCs neighbor entries within
//!   minutes, so polling alone misses addresses) and records per-(MAC, GUA)
//!   first/last-seen history, persisted across reboots.
//! * [`elect`] ranks a device's currently-held GUAs: an EUI-64 interface ID
//!   (provably derived from the MAC, prefix-independent) beats an address
//!   observed for longer than a temporary can live (RFC 8981
//!   TEMP_VALID_LIFETIME defaults to 2 days), which beats the oldest
//!   first-seen fallback. There is no DHCPv6-lease tier: no client solicits
//!   a stateful address on this LAN (our RAs never set the managed flag),
//!   and a leased address would pass the persistence tier anyway.
//! * When the elected address of a MAC referenced by a `pp_*_v6` rule
//!   changes, the task debounces briefly and re-runs
//!   `published_ports::reconcile`, which retargets the rule to the elected
//!   address within the current delegated prefix ([`elected_live_in_prefix`]).
//!
//! The module is deliberately self-contained — MAC in, elected GUA out — with
//! `published_ports` as its only consumer, so a future device-driven pinhole
//! feature (PCP/UPnP) lands beside it without touching it.

use std::collections::HashMap;
use std::net::Ipv6Addr;
use std::process::Stdio;
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};

const DIR: &str = "/etc/startwrt";
const PATH: &str = "/etc/startwrt/ipv6_neighbors.json";

/// Evict address records not seen for this long (matches `device_names`).
const RETENTION_SECS: i64 = 60 * 24 * 60 * 60;
/// Hard cap on total (MAC, addr) records; evicted oldest-`last_seen`-first.
const MAX_RECORDS: usize = 4096;
/// Persist the map to disk at most this often (plus only when it changed).
const PERSIST_MIN_INTERVAL_SECS: i64 = 5 * 60;
/// Bump a record's `last_seen` (a persistence-relevant change) at most this
/// often; in-memory recency for election still updates on every sighting.
const TOUCH_INTERVAL_SECS: i64 = 10 * 60;

/// An address observed for at least this long cannot be an RFC 4941 temporary
/// (TEMP_VALID_LIFETIME defaults to 2 days), so it is provably stable.
const PERSISTENT_AGE_SECS: i64 = 48 * 60 * 60;
/// "Currently held" horizon for election: with the 10-minute prod, any address
/// the device still owns is re-seen well within this.
const LIVE_SECS: i64 = 30 * 60;
/// Horizon for the offline reconcile fallback ([`eui64_suffix_hints`]).
const HINT_SECS: i64 = 7 * 24 * 60 * 60;

/// Multicast-prod + neighbor-table rescan cadence.
const PROD_INTERVAL_SECS: u64 = 10 * 60;
/// Quiet window after an elected-address change before reconciling, so a burst
/// of neighbor events (device reboot, prefix change) yields one rewrite.
const DEBOUNCE_SECS: u64 = 20;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddrRecord {
    first_seen: i64,
    last_seen: i64,
}

type AddrMap = HashMap<String, HashMap<String, AddrRecord>>;

/// Authoritative in-memory history (`MAC (uppercase) -> addr -> record`),
/// lazily loaded from disk; missing/corrupt file degrades to empty.
static STORE: LazyLock<Mutex<AddrMap>> = LazyLock::new(|| {
    let map = std::fs::read_to_string(PATH)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default();
    Mutex::new(map)
});

// ── Public API (best-effort: tracker failures must never break callers) ──

/// The device's elected stable GUA among addresses it currently holds
/// (seen within [`LIVE_SECS`]). `None` when nothing recent is known.
pub(crate) fn elected_live(mac: &str, now: i64) -> Option<Ipv6Addr> {
    let store = STORE.lock().ok()?;
    let addrs = store.get(&mac.to_uppercase())?;
    elect(mac, addrs, now, LIVE_SECS)
}

/// Like [`elected_live`], but only among the device's addresses within
/// `prefix`/`prefix_len`. `None` when it holds no live address there.
///
/// This is the selector for a *published-port rule target*: an address outside
/// the currently delegated prefix isn't routable, so it must never win — yet
/// [`elect`] ranks purely on stability/age and would otherwise keep preferring
/// an old-prefix address lingering after an ISP rotation. Narrowing the
/// candidate set here leaves [`elect`] itself prefix-agnostic.
pub(crate) fn elected_live_in_prefix(
    mac: &str,
    now: i64,
    prefix: Ipv6Addr,
    prefix_len: u8,
) -> Option<Ipv6Addr> {
    let store = STORE.lock().ok()?;
    let addrs = store.get(&mac.to_uppercase())?;
    elect_in_prefix(mac, addrs, now, LIVE_SECS, prefix, prefix_len)
}

/// `MAC -> hostid-style suffix` for every device whose elected address (seen
/// within [`HINT_SECS`]) has an EUI-64 interface ID — the only class of suffix
/// that provably survives a prefix rotation, and therefore the only one
/// `reconcile` may recombine with a new prefix while the device is offline.
/// Devices with recent history and a *non*-EUI-64 elected address are mapped
/// to `None`: positive evidence that any legacy stored suffix is
/// prefix-dependent and must not be recombined.
pub(crate) fn eui64_suffix_hints(now: i64) -> HashMap<String, Option<String>> {
    let Ok(store) = STORE.lock() else {
        return HashMap::new();
    };
    let mut hints = HashMap::new();
    for (mac, addrs) in store.iter() {
        if let Some(elected) = elect(mac, addrs, now, HINT_SECS) {
            hints.insert(
                mac.clone(),
                is_eui64_of(&elected, mac)
                    .then(|| crate::devices::extract_ipv6_hostid(&elected.to_string())),
            );
        }
    }
    hints
}

/// Daemon entry point: follow the neighbor table forever. Spawned once, in
/// normal (non-setup) mode only.
pub async fn run() {
    // Seed from the current table, then reconcile once: covers address changes
    // that happened while the router was down (persisted history vs. reality).
    record_neigh_dump().await;
    let mut pending =
        Some(tokio::time::Instant::now() + std::time::Duration::from_secs(DEBOUNCE_SECS));

    let mut prod = tokio::time::interval(std::time::Duration::from_secs(PROD_INTERVAL_SECS));
    prod.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    prod.reset(); // the seed above just scanned; skip the immediate first tick

    let mut backoff = 5u64;
    loop {
        let mut child = match tokio::process::Command::new("ip")
            .args(["-6", "monitor", "neigh"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                tracing::error!("ipv6-tracker: failed to spawn ip monitor: {e}");
                tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(60);
                continue;
            }
        };
        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();
        backoff = 5;

        loop {
            // `pending` doubles as the select guard: the sleep branch is only
            // polled while a reconcile is actually scheduled.
            let deadline = pending.unwrap_or_else(tokio::time::Instant::now);
            tokio::select! {
                line = lines.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            if record_line(&line).await {
                                pending.get_or_insert(
                                    tokio::time::Instant::now()
                                        + std::time::Duration::from_secs(DEBOUNCE_SECS),
                                );
                            }
                        }
                        // Monitor died (netlink hiccup, OOM kill): respawn.
                        Ok(None) | Err(_) => break,
                    }
                }
                _ = prod.tick() => {
                    crate::devices::prod_ipv6_neighbors().await;
                    if record_neigh_dump().await {
                        pending.get_or_insert(
                            tokio::time::Instant::now()
                                + std::time::Duration::from_secs(DEBOUNCE_SECS),
                        );
                    }
                }
                _ = tokio::time::sleep_until(deadline), if pending.is_some() => {
                    pending = None;
                    // reconcile prefers the tracker's elected addresses via
                    // resolve_device_info_for_macs; a no-op change writes
                    // nothing and skips the firewall restart.
                    if let Err(e) =
                        crate::published_ports::reconcile(crate::ServerContext::default()).await
                    {
                        tracing::warn!("ipv6-tracker: reconcile failed: {e}");
                    }
                }
            }
        }
        tracing::warn!("ipv6-tracker: ip monitor exited; respawning in {backoff}s");
        tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(60);
    }
}

// ── Observation ──

/// Record one neighbor line. Returns true when a tracked device's *elected*
/// address changed (the caller's cue to schedule a reconcile).
async fn record_line(line: &str) -> bool {
    let Some((mac, addr)) = parse_neigh_line(line) else {
        return false;
    };
    record(&mac, addr, chrono::Utc::now().timestamp()).await
}

/// Scan `ip -6 neigh show` once, recording every entry. Returns true when any
/// elected address changed.
async fn record_neigh_dump() -> bool {
    let output = tokio::process::Command::new("ip")
        .args(["-6", "neigh", "show"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await;
    let Ok(output) = output else {
        return false;
    };
    let now = chrono::Utc::now().timestamp();
    let mut changed = false;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some((mac, addr)) = parse_neigh_line(line) {
            changed |= record(&mac, addr, now).await;
        }
    }
    changed
}

/// Fold one sighting into the store; persist (rate-limited) when the map
/// changed. Returns true when the MAC's elected address changed.
async fn record(mac: &str, addr: Ipv6Addr, now: i64) -> bool {
    static LAST_PERSIST: Mutex<i64> = Mutex::new(0);

    let (elected_changed, snapshot) = {
        let mut store = match STORE.lock() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("ipv6-tracker: lock poisoned: {e}");
                return false;
            }
        };
        let addrs = store.entry(mac.to_string()).or_default();
        let before = elect(mac, addrs, now, LIVE_SECS);

        let mut dirty = false;
        match addrs.get_mut(&addr.to_string()) {
            Some(rec) => {
                if now - rec.last_seen >= TOUCH_INTERVAL_SECS {
                    dirty = true;
                }
                rec.last_seen = now;
            }
            None => {
                addrs.insert(
                    addr.to_string(),
                    AddrRecord {
                        first_seen: now,
                        last_seen: now,
                    },
                );
                dirty = true;
            }
        }
        let after = elect(mac, store.get(mac).unwrap(), now, LIVE_SECS);
        dirty |= prune(&mut store, now);

        let snapshot = if dirty {
            let mut last = LAST_PERSIST.lock().unwrap_or_else(|e| e.into_inner());
            (now - *last >= PERSIST_MIN_INTERVAL_SECS).then(|| {
                *last = now;
                store.clone()
            })
        } else {
            None
        };
        (before != after, snapshot)
    };

    if let Some(snapshot) = snapshot {
        if let Err(e) = persist(&snapshot).await {
            tracing::warn!("ipv6-tracker: persist failed: {e}");
        }
    }
    elected_changed
}

/// Parse one `ip -6 neigh show` / `ip -6 monitor neigh` line into
/// `(uppercase MAC, GUA)`. Rejects deletions, unreachable states, non-GUA
/// addresses, and non-LAN interfaces; unrecognized lines are ignored.
/// Format: `<addr> dev <iface> lladdr <mac> [flags] <STATE>`.
pub(crate) fn parse_neigh_line(line: &str) -> Option<(String, Ipv6Addr)> {
    // "Deleted" entries and FAILED/INCOMPLETE probes say the kernel lost the
    // entry, not that the device lost the address — record neither.
    if line.starts_with("Deleted") {
        return None;
    }
    let (ip, rest) = line.split_once(" dev ")?;
    let addr = ip.trim().parse::<Ipv6Addr>().ok()?;
    if !crate::system::has_global_ipv6(std::slice::from_ref(&addr)) {
        return None;
    }
    let (iface, rest) = rest.split_once(" lladdr ")?;
    if !iface.starts_with("br-lan") {
        return None;
    }
    let mac = rest.split_whitespace().next()?;
    if rest.contains("FAILED") || rest.contains("INCOMPLETE") {
        return None;
    }
    Some((mac.to_uppercase(), addr))
}

// ── Election (pure) ──

/// Pick the stable GUA among the addresses this device currently holds (seen
/// within `live_secs`). Ranking, best first:
///   1. EUI-64 interface ID derived from the device's own MAC — stable across
///      prefix rotations by construction.
///   2. Older than [`PERSISTENT_AGE_SECS`] — outlives any RFC 4941 temporary,
///      so it is a stable (likely RFC 7217) address; oldest `first_seen` wins.
///   3. Oldest `first_seen` overall — at interface-up the stable address
///      typically appears before the temporaries, so age is the best guess
///      until 48 h of history accumulates.
/// Ties break on the address string so the result is deterministic.
fn elect(
    mac: &str,
    addrs: &HashMap<String, AddrRecord>,
    now: i64,
    live_secs: i64,
) -> Option<Ipv6Addr> {
    let live = addrs
        .iter()
        .filter(|(_, rec)| now - rec.last_seen <= live_secs)
        .filter_map(|(addr, rec)| addr.parse::<Ipv6Addr>().ok().map(|a| (a, rec)));

    live.min_by_key(|(addr, rec)| {
        let tier = if is_eui64_of(addr, mac) {
            0
        } else if now - rec.first_seen >= PERSISTENT_AGE_SECS {
            1
        } else {
            2
        };
        (tier, rec.first_seen, addr.to_string())
    })
    .map(|(addr, _)| addr)
}

/// [`elect`] restricted to the candidates within `prefix`/`prefix_len`. Filters
/// the set first, then defers to the prefix-agnostic [`elect`] unchanged, so
/// prefix-currency (routability) gates eligibility while stability/age still
/// picks the winner among the survivors.
fn elect_in_prefix(
    mac: &str,
    addrs: &HashMap<String, AddrRecord>,
    now: i64,
    live_secs: i64,
    prefix: Ipv6Addr,
    prefix_len: u8,
) -> Option<Ipv6Addr> {
    let in_prefix: HashMap<String, AddrRecord> = addrs
        .iter()
        .filter(|(addr, _)| {
            addr.parse::<Ipv6Addr>()
                .is_ok_and(|a| crate::system::addr_in_prefix(a, prefix, prefix_len))
        })
        .map(|(addr, rec)| (addr.clone(), rec.clone()))
        .collect();
    elect(mac, &in_prefix, now, live_secs)
}

/// Whether the address's interface identifier is the modified-EUI-64 form of
/// `mac`: `mac[0]^0x02, mac[1], mac[2], 0xff, 0xfe, mac[3], mac[4], mac[5]`.
pub(crate) fn is_eui64_of(addr: &Ipv6Addr, mac: &str) -> bool {
    let mut bytes = [0u8; 6];
    let mut n = 0;
    for part in mac.split(':') {
        if n == 6 {
            return false;
        }
        match u8::from_str_radix(part, 16) {
            Ok(b) => bytes[n] = b,
            Err(_) => return false,
        }
        n += 1;
    }
    if n != 6 {
        return false;
    }
    let o = addr.octets();
    o[8] == bytes[0] ^ 0x02
        && o[9] == bytes[1]
        && o[10] == bytes[2]
        && o[11] == 0xff
        && o[12] == 0xfe
        && o[13] == bytes[3]
        && o[14] == bytes[4]
        && o[15] == bytes[5]
}

// ── Store maintenance ──

/// Drop expired records and enforce the global cap. Returns true if changed.
fn prune(store: &mut AddrMap, now: i64) -> bool {
    let mut changed = false;
    for addrs in store.values_mut() {
        let before = addrs.len();
        addrs.retain(|_, rec| now - rec.last_seen <= RETENTION_SECS);
        changed |= addrs.len() != before;
    }
    let before = store.len();
    store.retain(|_, addrs| !addrs.is_empty());
    changed |= store.len() != before;

    let total: usize = store.values().map(|a| a.len()).sum();
    if total > MAX_RECORDS {
        // Pathological churn (e.g. MAC-randomizing guests): drop oldest-seen.
        let mut all: Vec<(String, String, i64)> = store
            .iter()
            .flat_map(|(mac, addrs)| {
                addrs
                    .iter()
                    .map(|(addr, rec)| (mac.clone(), addr.clone(), rec.last_seen))
            })
            .collect();
        all.sort_by_key(|(_, _, last_seen)| *last_seen);
        for (mac, addr, _) in all.into_iter().take(total - MAX_RECORDS) {
            if let Some(addrs) = store.get_mut(&mac) {
                addrs.remove(&addr);
                if addrs.is_empty() {
                    store.remove(&mac);
                }
            }
        }
        changed = true;
    }
    changed
}

/// Atomic temp+rename write, same rationale as `device_names::persist`: an
/// external backup `tar` must never see a torn document.
async fn persist(snapshot: &AddrMap) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;

    tokio::fs::create_dir_all(DIR).await?;
    let tmp = format!("{PATH}.tmp");
    let json = serde_json::to_vec(snapshot).map_err(std::io::Error::other)?;
    let mut file = tokio::fs::File::create(&tmp).await?;
    file.write_all(&json).await?;
    file.sync_all().await?;
    drop(file);
    tokio::fs::rename(&tmp, PATH).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(first_seen: i64, last_seen: i64) -> AddrRecord {
        AddrRecord {
            first_seen,
            last_seen,
        }
    }

    const MAC: &str = "AA:BB:CC:DD:EE:FF";
    // EUI-64 of AA:BB:CC:DD:EE:FF → a8bb:ccff:fedd:eeff
    const EUI64: &str = "2001:db8::a8bb:ccff:fedd:eeff";

    #[test]
    fn parse_accepts_reachable_gua_on_lan() {
        let line = "2001:db8::1 dev br-lan.1 lladdr aa:bb:cc:dd:ee:ff REACHABLE";
        assert_eq!(
            parse_neigh_line(line),
            Some((MAC.to_string(), "2001:db8::1".parse().unwrap()))
        );
    }

    #[test]
    fn parse_accepts_stale_on_plain_br_lan() {
        let line = "2001:db8::2 dev br-lan lladdr aa:bb:cc:dd:ee:ff STALE";
        assert!(parse_neigh_line(line).is_some());
    }

    #[test]
    fn parse_rejects_deleted_failed_and_lladdrless() {
        assert!(parse_neigh_line(
            "Deleted 2001:db8::1 dev br-lan.1 lladdr aa:bb:cc:dd:ee:ff STALE"
        )
        .is_none());
        assert!(
            parse_neigh_line("2001:db8::1 dev br-lan.1 lladdr aa:bb:cc:dd:ee:ff FAILED").is_none()
        );
        assert!(parse_neigh_line("2001:db8::1 dev br-lan.1  INCOMPLETE").is_none());
        assert!(parse_neigh_line("2001:db8::1 dev br-lan.1 FAILED").is_none());
    }

    #[test]
    fn parse_rejects_non_gua_and_foreign_interfaces() {
        // link-local, ULA, IPv4, and a WAN-side interface
        assert!(
            parse_neigh_line("fe80::1 dev br-lan.1 lladdr aa:bb:cc:dd:ee:ff REACHABLE").is_none()
        );
        assert!(
            parse_neigh_line("fd00::1 dev br-lan.1 lladdr aa:bb:cc:dd:ee:ff REACHABLE").is_none()
        );
        assert!(
            parse_neigh_line("192.168.1.2 dev br-lan.1 lladdr aa:bb:cc:dd:ee:ff REACHABLE")
                .is_none()
        );
        assert!(
            parse_neigh_line("2001:db8::1 dev eth0 lladdr aa:bb:cc:dd:ee:ff REACHABLE").is_none()
        );
    }

    #[test]
    fn eui64_matches_with_ul_bit_flip() {
        assert!(is_eui64_of(&EUI64.parse().unwrap(), MAC));
        assert!(is_eui64_of(&EUI64.parse().unwrap(), "aa:bb:cc:dd:ee:ff"));
        assert!(!is_eui64_of(
            &"2001:db8::aabb:ccff:fedd:eeff".parse().unwrap(),
            MAC
        ));
        assert!(!is_eui64_of(&"2001:db8::1".parse().unwrap(), MAC));
        assert!(!is_eui64_of(&EUI64.parse().unwrap(), "not-a-mac"));
    }

    #[test]
    fn elect_prefers_eui64_over_older_persistent() {
        let now = 100 * 24 * 60 * 60;
        let mut addrs = HashMap::new();
        addrs.insert("2001:db8::1".into(), rec(0, now)); // ancient, persistent
        addrs.insert(EUI64.into(), rec(now - 60, now)); // brand new, EUI-64
        assert_eq!(
            elect(MAC, &addrs, now, LIVE_SECS),
            Some(EUI64.parse().unwrap())
        );
    }

    #[test]
    fn elect_in_prefix_excludes_out_of_prefix_winner() {
        // A device straddling an ISP prefix rotation: the old-prefix address is
        // older (and persistent) so the prefix-agnostic election prefers it,
        // but it is no longer routable. Scoping to the current /64 must pick the
        // fresh in-prefix address instead.
        let now = 100 * 24 * 60 * 60;
        let old = "2001:db8:1::50"; // older + persistent, old (dead) prefix
        let new = "2001:db8:2::99"; // fresh, current prefix
        let mut addrs = HashMap::new();
        addrs.insert(old.into(), rec(0, now));
        addrs.insert(new.into(), rec(now - 60, now));

        // Unscoped: the stale old-prefix address wins on age — the bug.
        assert_eq!(
            elect(MAC, &addrs, now, LIVE_SECS),
            Some(old.parse().unwrap())
        );

        // Scoped to the live /64: only the in-prefix address is eligible.
        let cur: Ipv6Addr = "2001:db8:2::".parse().unwrap();
        assert_eq!(
            elect_in_prefix(MAC, &addrs, now, LIVE_SECS, cur, 64),
            Some(new.parse().unwrap())
        );

        // No address in an unrelated prefix → None (reconcile then recombines).
        let other: Ipv6Addr = "2001:db8:9::".parse().unwrap();
        assert_eq!(
            elect_in_prefix(MAC, &addrs, now, LIVE_SECS, other, 64),
            None
        );
    }

    #[test]
    fn elect_prefers_persistent_over_fresh_temporary() {
        let now = 100 * 24 * 60 * 60;
        let mut addrs = HashMap::new();
        addrs.insert("2001:db8::aaaa".into(), rec(now - 3 * 24 * 60 * 60, now)); // 3 days old
        addrs.insert("2001:db8::bbbb".into(), rec(now - 60 * 60, now)); // fresh temp
        assert_eq!(
            elect(MAC, &addrs, now, LIVE_SECS),
            Some("2001:db8::aaaa".parse().unwrap())
        );
    }

    #[test]
    fn elect_falls_back_to_oldest_first_seen() {
        // Neither EUI-64 nor 48h-persistent: age is the best guess.
        let now = 10_000;
        let mut addrs = HashMap::new();
        addrs.insert("2001:db8::aaaa".into(), rec(1_000, now));
        addrs.insert("2001:db8::bbbb".into(), rec(2_000, now));
        assert_eq!(
            elect(MAC, &addrs, now, LIVE_SECS),
            Some("2001:db8::aaaa".parse().unwrap())
        );
    }

    #[test]
    fn elect_ignores_addresses_no_longer_held() {
        // The persistent address was dropped by the device (last_seen went
        // quiet); the only currently-held address must win.
        let now = 100 * 24 * 60 * 60;
        let mut addrs = HashMap::new();
        addrs.insert("2001:db8::aaaa".into(), rec(0, now - 2 * LIVE_SECS));
        addrs.insert("2001:db8::bbbb".into(), rec(now - 60, now));
        assert_eq!(
            elect(MAC, &addrs, now, LIVE_SECS),
            Some("2001:db8::bbbb".parse().unwrap())
        );
        assert_eq!(elect(MAC, &HashMap::new(), now, LIVE_SECS), None);
    }

    #[test]
    fn prune_expires_and_caps() {
        let now = 100 * 24 * 60 * 60;
        let mut store: AddrMap = HashMap::new();
        store
            .entry(MAC.into())
            .or_default()
            .insert("2001:db8::1".into(), rec(0, now - RETENTION_SECS - 1));
        assert!(prune(&mut store, now));
        assert!(store.is_empty());

        // Cap: oldest-seen records evicted first.
        let mut store: AddrMap = HashMap::new();
        for i in 0..(MAX_RECORDS + 10) {
            store
                .entry(format!("MAC{i}"))
                .or_default()
                .insert(format!("2001:db8::{:x}", i + 1), rec(0, now - i as i64));
        }
        assert!(prune(&mut store, now));
        let total: usize = store.values().map(|a| a.len()).sum();
        assert_eq!(total, MAX_RECORDS);
        // The most recently seen record survived.
        assert!(store.contains_key("MAC0"));
        assert!(!store.contains_key(&format!("MAC{}", MAX_RECORDS + 9)));
    }

    #[test]
    fn store_json_round_trip() {
        let mut store: AddrMap = HashMap::new();
        store
            .entry(MAC.into())
            .or_default()
            .insert(EUI64.into(), rec(1, 2));
        let json = serde_json::to_string(&store).unwrap();
        let back: AddrMap = serde_json::from_str(&json).unwrap();
        assert_eq!(back[MAC][EUI64].first_seen, 1);
        assert!(serde_json::from_str::<AddrMap>("not json").is_err());
    }
}
