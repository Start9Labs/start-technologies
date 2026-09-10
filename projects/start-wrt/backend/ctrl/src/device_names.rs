//! Persistent cache of auto-learned device identity, keyed by MAC: the last
//! hostname a device advertised, and its last DHCP fingerprint.
//!
//! dnsmasq's `/tmp/dhcp.leases` is RAM-backed and loses a device's hostname
//! whenever the in-memory state churns (lease expiry, dnsmasq restart, clients
//! that omit DHCP option 12 on renewals). Without a persistent record, such a
//! device falls back to a `device-<mac>` placeholder in the UI until it happens
//! to re-advertise its name. This cache remembers the last hostname a device
//! advertised so the placeholder is only ever shown for a device we have never
//! seen named. DHCP fingerprints (see `device_ident`) are cached alongside for
//! the same reason: the live capture file is tmpfs, so without this a device
//! that never renews between reboots would lose its OS label. An entry may
//! carry either or both — a Chromebook has a fingerprint but no hostname.
//!
//! Scope: the cache holds auto-learned data — DHCP-lease names, reverse-mDNS
//! (`.local`) names, and DHCP fingerprints. UCI static-host names (set via the
//! UI) are authoritative, live in `/etc/config/dhcp`, already survive
//! sysupgrade, and must never be pruned — so they are deliberately not cached
//! here. `devices::list` consults the cache only as a fallback below the live
//! UCI, DHCP-lease, and mDNS sources; a fresh live name always wins.
//!
//! Storage: a JSON file written atomically (temp + rename), NOT SQLite. The file
//! is listed in `keep.d` so it survives sysupgrade and is captured by the
//! config-backup feature (`sysupgrade --create-backup`, which tars keep.d). An
//! atomic rename swaps inodes rather than mutating in place, so the external
//! `tar`/`fs::read` that copies it always sees one complete document — a live
//! SQLite file copied the same way could be torn mid-write. The in-memory map is
//! authoritative (mirrors the `sessions.json` pattern in `auth.rs`); disk is
//! rewritten only when the map actually changes.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::prelude::*;

const DIR: &str = "/etc/startwrt";
const PATH: &str = "/etc/startwrt/device_names.json";

/// Evict entries not seen for this long. 60 days comfortably covers MAC
/// rotation (modern macOS rotates its private address ~every 2 weeks) and
/// devices that are off the network for extended trips.
const RETENTION_SECS: i64 = 60 * 24 * 60 * 60;
/// Hard cap on entries, guarding against pathological churn (e.g. guest networks
/// with per-connection MAC randomization). Evicted oldest-first by `last_seen`.
const MAX_ENTRIES: usize = 1000;
/// Bump `last_seen` for an unchanged entry at most this often. Gates disk writes:
/// a quiet, stable network produces no map change and therefore no write.
const TOUCH_INTERVAL_SECS: i64 = 24 * 60 * 60;

/// One device's cached identity. `hostname: Option` (rather than a separate
/// map) keeps pre-fingerprint `device_names.json` files loading unchanged:
/// legacy entries always carried a hostname, and the new fields default.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedName {
    pub hostname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<crate::device_ident::Fingerprint>,
    last_seen: i64,
}

/// What `devices::list` saw for one MAC during a single poll.
pub struct Observation {
    /// Canonical uppercase MAC (matches the in-memory convention in devices.rs).
    pub mac: String,
    /// The name this MAC advertised this poll via a live source — the DHCP-lease
    /// hostname, or a reverse-mDNS `.local` name when DHCP gave none (`None` if
    /// neither). UCI static-host names are intentionally excluded.
    pub hostname: Option<String>,
    /// The DHCP fingerprint captured for this MAC in the live hook file
    /// (`None` when the device hasn't DHCPed since boot — the cached value
    /// is kept, not erased).
    pub fingerprint: Option<crate::device_ident::Fingerprint>,
}

/// Authoritative in-memory map, lazily loaded from disk on first access.
/// A poisoned/missing/corrupt file degrades to an empty map (best-effort).
static STORE: LazyLock<Mutex<HashMap<String, CachedName>>> = LazyLock::new(|| {
    let map = std::fs::read_to_string(PATH)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default();
    Mutex::new(map)
});

// ── Public API (best-effort: cache failures must never break devices.list) ──

/// Snapshot of `MAC (uppercase) -> cached identity` for this poll's resolution.
pub fn load_all() -> HashMap<String, CachedName> {
    match STORE.lock() {
        Ok(map) => map.clone(),
        Err(e) => {
            tracing::warn!("device_names: lock poisoned on load: {e}");
            HashMap::new()
        }
    }
}

/// Record this poll's observations: learn/refresh DHCP-advertised names and
/// fingerprints, keep visible entries alive, and prune. Persists only if the
/// map actually changed.
pub async fn commit(observations: &[Observation], now: i64) {
    let snapshot = {
        let mut map = match STORE.lock() {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("device_names: lock poisoned on commit: {e}");
                return;
            }
        };
        let mut changed = apply_observations(&mut map, observations, now);
        changed |= prune(&mut map, now);
        if !changed {
            return;
        }
        map.clone()
    };
    if let Err(e) = persist(&snapshot).await {
        tracing::warn!("device_names: persist failed: {e}");
    }
}

/// Drop a device's cached name (called when the user forgets the device).
pub async fn forget(mac: &str) {
    let snapshot = {
        let mut map = match STORE.lock() {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("device_names: lock poisoned on forget: {e}");
                return;
            }
        };
        if !forget_in(&mut map, mac) {
            return;
        }
        map.clone()
    };
    if let Err(e) = persist(&snapshot).await {
        tracing::warn!("device_names: persist failed: {e}");
    }
}

// ── Pure logic (operates on the map; unit-tested without touching disk) ──

fn apply_observations(
    map: &mut HashMap<String, CachedName>,
    observations: &[Observation],
    now: i64,
) -> bool {
    let mut changed = false;
    for obs in observations {
        match map.get_mut(&obs.mac) {
            Some(entry) => {
                // Known device: refresh whichever facets it advertised this
                // poll (an absent facet keeps its cached value — it is stale,
                // not wrong). Otherwise touch `last_seen` (rate-limited) so a
                // visible device is kept alive against prune.
                let mut updated = false;
                if let Some(name) = &obs.hostname {
                    if entry.hostname.as_ref() != Some(name) {
                        entry.hostname = Some(name.clone());
                        updated = true;
                    }
                }
                if let Some(fp) = &obs.fingerprint {
                    if entry.fingerprint.as_ref() != Some(fp) {
                        entry.fingerprint = Some(fp.clone());
                        updated = true;
                    }
                }
                if updated {
                    entry.last_seen = now;
                    changed = true;
                } else if now - entry.last_seen >= TOUCH_INTERVAL_SECS {
                    entry.last_seen = now;
                    changed = true;
                }
            }
            None => {
                // New device: learn whatever it advertised. Nothing advertised,
                // nothing to cache.
                if obs.hostname.is_some() || obs.fingerprint.is_some() {
                    map.insert(
                        obs.mac.clone(),
                        CachedName {
                            hostname: obs.hostname.clone(),
                            fingerprint: obs.fingerprint.clone(),
                            last_seen: now,
                        },
                    );
                    changed = true;
                }
            }
        }
    }
    changed
}

fn prune(map: &mut HashMap<String, CachedName>, now: i64) -> bool {
    let before = map.len();

    // Age cap.
    let cutoff = now - RETENTION_SECS;
    map.retain(|_, v| v.last_seen >= cutoff);

    // Count cap: keep the newest MAX_ENTRIES by last_seen.
    if map.len() > MAX_ENTRIES {
        let mut by_recency: Vec<(String, i64)> = map
            .iter()
            .map(|(mac, v)| (mac.clone(), v.last_seen))
            .collect();
        by_recency.sort_by(|a, b| b.1.cmp(&a.1));
        for (mac, _) in by_recency.into_iter().skip(MAX_ENTRIES) {
            map.remove(&mac);
        }
    }

    map.len() != before
}

fn forget_in(map: &mut HashMap<String, CachedName>, mac: &str) -> bool {
    map.remove(&mac.to_uppercase()).is_some()
}

// ── Disk persistence (atomic temp + rename, mirrors auth.rs sessions) ──

async fn persist(map: &HashMap<String, CachedName>) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;

    let content = serde_json::to_string_pretty(map)
        .map_err(|e| Error::new(eyre!("serialize device_names: {e}"), ErrorKind::Filesystem))?;

    let _ = tokio::fs::create_dir_all(DIR).await;

    let mut file = startos::util::io::AtomicFile::new(Path::new(PATH), None::<&Path>)
        .await
        .map_err(Error::from)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .await
        .map_err(|e| Error::new(eyre!("chmod device_names: {e}"), ErrorKind::Filesystem))?;
    file.write_all(content.as_bytes())
        .await
        .map_err(|e| Error::new(eyre!("write device_names: {e}"), ErrorKind::Filesystem))?;
    file.save().await.map_err(Error::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(mac: &str, hostname: Option<&str>) -> Observation {
        Observation {
            mac: mac.to_string(),
            hostname: hostname.map(String::from),
            fingerprint: None,
        }
    }

    fn fp(options: &str, vendor_class: &str) -> crate::device_ident::Fingerprint {
        crate::device_ident::Fingerprint {
            options: options.into(),
            vendor_class: vendor_class.into(),
        }
    }

    fn obs_fp(mac: &str, hostname: Option<&str>, options: &str, vc: &str) -> Observation {
        Observation {
            mac: mac.to_string(),
            hostname: hostname.map(String::from),
            fingerprint: Some(fp(options, vc)),
        }
    }

    #[test]
    fn learns_dhcp_name() {
        let mut m = HashMap::new();
        assert!(apply_observations(
            &mut m,
            &[obs("AA:BB:CC:00:00:01", Some("debian"))],
            1000
        ));
        assert_eq!(
            m.get("AA:BB:CC:00:00:01").unwrap().hostname.as_deref(),
            Some("debian")
        );
    }

    #[test]
    fn ignores_unknown_without_name() {
        let mut m = HashMap::new();
        assert!(!apply_observations(
            &mut m,
            &[obs("AA:BB:CC:00:00:02", None)],
            1000
        ));
        assert!(m.is_empty());
    }

    #[test]
    fn name_change_overwrites() {
        let mut m = HashMap::new();
        apply_observations(&mut m, &[obs("AA:BB:CC:00:00:03", Some("old"))], 1000);
        assert!(apply_observations(
            &mut m,
            &[obs("AA:BB:CC:00:00:03", Some("new"))],
            1001
        ));
        assert_eq!(
            m.get("AA:BB:CC:00:00:03").unwrap().hostname.as_deref(),
            Some("new")
        );
    }

    #[test]
    fn touch_within_interval_is_noop() {
        let mut m = HashMap::new();
        apply_observations(&mut m, &[obs("AA:BB:CC:00:00:04", Some("nas"))], 1000);
        let changed = apply_observations(
            &mut m,
            &[obs("AA:BB:CC:00:00:04", None)],
            1000 + TOUCH_INTERVAL_SECS - 1,
        );
        assert!(!changed);
        assert_eq!(m.get("AA:BB:CC:00:00:04").unwrap().last_seen, 1000);
    }

    #[test]
    fn touch_past_interval_updates_last_seen_only() {
        let mut m = HashMap::new();
        apply_observations(&mut m, &[obs("AA:BB:CC:00:00:05", Some("nas"))], 1000);
        let t = 1000 + TOUCH_INTERVAL_SECS + 1;
        assert!(apply_observations(
            &mut m,
            &[obs("AA:BB:CC:00:00:05", None)],
            t
        ));
        let entry = m.get("AA:BB:CC:00:00:05").unwrap();
        assert_eq!(entry.last_seen, t);
        assert_eq!(entry.hostname.as_deref(), Some("nas"));
    }

    #[test]
    fn learns_fingerprint_without_hostname() {
        // The Chromebook case: fingerprint, never a name.
        let mut m = HashMap::new();
        assert!(apply_observations(
            &mut m,
            &[obs_fp("AA:BB:CC:00:00:08", None, "1,3,6", "dhcpcd-10.0.6")],
            1000
        ));
        let entry = m.get("AA:BB:CC:00:00:08").unwrap();
        assert_eq!(entry.hostname, None);
        assert_eq!(entry.fingerprint, Some(fp("1,3,6", "dhcpcd-10.0.6")));
    }

    #[test]
    fn fingerprint_and_hostname_merge_independently() {
        let mut m = HashMap::new();
        apply_observations(
            &mut m,
            &[obs_fp("AA:BB:CC:00:00:09", None, "1,3,6", "MSFT 5.0")],
            1000,
        );
        // Hostname arrives later without a fingerprint: fingerprint is kept.
        assert!(apply_observations(
            &mut m,
            &[obs("AA:BB:CC:00:00:09", Some("desktop"))],
            1001
        ));
        let entry = m.get("AA:BB:CC:00:00:09").unwrap();
        assert_eq!(entry.hostname.as_deref(), Some("desktop"));
        assert_eq!(entry.fingerprint, Some(fp("1,3,6", "MSFT 5.0")));
        // Changed fingerprint refreshes; unchanged one within the touch
        // interval is a no-op.
        assert!(apply_observations(
            &mut m,
            &[obs_fp("AA:BB:CC:00:00:09", None, "1,121,3", "")],
            1002
        ));
        assert!(!apply_observations(
            &mut m,
            &[obs_fp("AA:BB:CC:00:00:09", None, "1,121,3", "")],
            1003
        ));
    }

    #[test]
    fn legacy_json_without_fingerprint_loads() {
        // Pre-fingerprint device_names.json entries carry only hostname+lastSeen.
        let m: HashMap<String, CachedName> =
            serde_json::from_str(r#"{"AA:BB:CC:00:00:0A":{"hostname":"nas","lastSeen":1000}}"#)
                .unwrap();
        let entry = m.get("AA:BB:CC:00:00:0A").unwrap();
        assert_eq!(entry.hostname.as_deref(), Some("nas"));
        assert_eq!(entry.fingerprint, None);
        assert_eq!(entry.last_seen, 1000);
    }

    #[test]
    fn prune_evicts_by_age() {
        let mut m = HashMap::new();
        m.insert(
            "AA:BB:CC:00:00:06".to_string(),
            CachedName {
                hostname: Some("stale".to_string()),
                fingerprint: None,
                last_seen: 1000,
            },
        );
        assert!(prune(&mut m, 1000 + RETENTION_SECS + 1));
        assert!(m.is_empty());
    }

    #[test]
    fn prune_enforces_count_cap_keeping_newest() {
        let mut m = HashMap::new();
        for i in 0..(MAX_ENTRIES as i64 + 5) {
            m.insert(
                format!("MAC-{i}"),
                CachedName {
                    hostname: Some(format!("h{i}")),
                    fingerprint: None,
                    last_seen: 1000 + i,
                },
            );
        }
        assert!(prune(&mut m, 2000));
        assert_eq!(m.len(), MAX_ENTRIES);
        // The five oldest (lowest last_seen) were dropped; the newest survives.
        assert!(!m.contains_key("MAC-0"));
        assert!(m.contains_key(&format!("MAC-{}", MAX_ENTRIES as i64 + 4)));
    }

    #[test]
    fn forget_is_case_insensitive() {
        let mut m = HashMap::new();
        apply_observations(&mut m, &[obs("AA:BB:CC:00:00:07", Some("gone"))], 1000);
        assert!(forget_in(&mut m, "aa:bb:cc:00:00:07"));
        assert!(m.is_empty());
    }
}
