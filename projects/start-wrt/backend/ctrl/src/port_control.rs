//! PCP + UPnP IGD port-control server ("automatic port forwarding").
//!
//! Implements the server half of automatic gateway configuration (the shared
//! protocol cores in start-core's `net::port_map::server`): a LAN device whose
//! per-device toggle is on may create port forwards *to itself* via PCP or UPnP
//! IGD, instead of the user configuring a published port by hand. Authorization
//! is default-off and per-device: `is_known_client` requires a `_allow_pcp`-
//! flagged DHCP host entry for the requesting MAC.
//!
//! Forwards are ordinary UCI `redirect` DNAT sections tagged with
//! `_apf_label`/`_apf_mac` (options fw4 ignores), named `apf_<mac>_<extport>`
//! — keyed by *external* port, matching UPnP's mapping identity, so one device
//! can hold several external ports to the same internal port. They persist
//! across reboots — a router reboot must not break an auto-mapped device — and
//! are invisible to manual published-ports (`published-ports set` only
//! replaces `pp_*`/`_pp_id` sections), though a manual rule claiming an
//! auto-held external port wins: `set` removes the overlapping auto forward.
//!
//! Lease bookkeeping is deliberately in-memory: clients refresh mappings every
//! few minutes, and rewriting flash-backed /etc/config on every renewal would
//! wear the overlay filesystem. UCI is written (and the firewall restarted)
//! only when a forward is created, changed, or removed; a renewal of an
//! unchanged forward just bumps the in-memory expiry to the lifetime the
//! protocol layer granted (and reported) to the client. A section with no
//! lease entry (daemon restart, external edit) is granted one [`GRACE_LEASE`]
//! by the sweep, then collected if never renewed. All lease/UCI mutations
//! serialize on one internal lock, so the sweep can never collect a forward
//! concurrently with its own renewal.
//!
//! A forward is also bound to the *address assignment* behind it, not just to a
//! clock: the sweep drops one whose owning MAC no longer holds the address it
//! points at (RFC 6887 §15, and the RFC 6970 §5.10 requirement to discard
//! mappings when an internal address is released). A renewing client re-resolves
//! its own address anyway — the section is keyed by MAC and external port, so a
//! renumbered device rewrites its own `dest_ip` — but a client that asked for a
//! permanent mapping never renews, and without this its forward would keep
//! pointing at an address DHCP is free to hand to someone else. A static
//! reservation pins the forward regardless, since nobody else can be given that
//! address.
//!
//! # SNI hostname routes
//!
//! Hostname routes live in [`SniDemux`] memory and share one IPv4 WAN admission
//! rule per port. The sweep reconciles those rules and re-keys routes when the
//! WAN address changes. Protocol leases bound every device-created route.
//!
//! # LAN source-address spoofing: cross-segment closed, same-segment open
//!
//! PCP rides on unauthenticated UDP, and authorization resolves the *claimed*
//! source address through the neighbor table — nothing in the datagram itself
//! proves which host really sent it. Two cases, handled differently:
//!
//! - **Across LAN segments (closed).** A host on one bridge presenting a
//!   *different* bridge's device address is rejected by the arrival-interface
//!   check: [`serve_pcp`] reads the receiving interface from `IP_PKTINFO` and
//!   [`arrival_matches`] requires it to be the interface the neighbor table
//!   places the claimed source on. A datagram that arrived on `br-lan` cannot
//!   act as a device the neighbor table locates on `br-lan.101`. UPnP needs no
//!   such check — it is TCP, so the handshake already proves the address.
//! - **Within one segment (open).** Two hosts on the same bridge share an L2
//!   broadcast domain, so a spoofed source is indistinguishable from the real
//!   one — `IP_PKTINFO` reports the same interface for both. A same-bridge host
//!   can therefore still open an authorized neighbor's ports on its behalf
//!   (exposing *that* device, never itself — the shared cores force the DNAT
//!   target to the claimed address) or tear its mappings down with
//!   `lifetime = 0`. Closing this needs L2 source guard (DHCP-snooping-backed
//!   ip↔mac↔port binding), which the K1's single wired bridge can't provide; the
//!   honest boundary is to isolate untrusted devices in their own profile.
//!
//! StartTunnel does not share even the residual exposure: its sockets are
//! `SO_BINDTODEVICE`-bound to the WireGuard interface, which ties a packet's
//! source address to the peer key that decrypted it, so one peer cannot present
//! as another. A LAN bridge offers no equivalent within a segment.
//!
//! RFC 6887 assumes a trusted internal network, and its optional authentication
//! (RFC 7652) is unimplemented in practice, so there is no protocol-layer answer
//! to copy for the same-segment case. The per-device default-off permission
//! bounds the residual blast radius to devices a user explicitly trusted and
//! placed on a shared segment with an attacker.

use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use imbl_value::Value;
use nix::net::if_::if_nametoindex;
use nix::sys::socket::sockopt::Ipv4PacketInfo;
use nix::sys::socket::{recvmsg, setsockopt, ControlMessageOwned, MsgFlags, SockaddrIn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use startos::net::port_map::pcp::hostname::RESULT_HOSTNAME_TAKEN;
use startos::net::port_map::pcp::RESULT_NO_RESOURCES;
use startos::net::port_map::server::igd::{
    format_uuid, handle_control, header_value, render_root_desc, serve_static, ssdp_response,
    st_matches, CIF_SCPD, CIF_SCPD_PATH, CONTROL_PATH, IGD_HTTP_PORT, ROOT_DESC_PATH, SCPD,
    SCPD_PATH, SSDP_MULTICAST, SSDP_PORT,
};
use startos::net::port_map::server::{handle, GatewayBackend, MappingEntry, PCP_PORT};
use startos::tunnel::forward::sni::{FallbackSource, SniDemux, SniRoute};
use tokio::io::Interest;
use tokio::net::UdpSocket;
use uciedit::openwrt::{DhcpHost, FirewallRedirect, FirewallRule, FirewallTarget};
use uciedit::{dump_all, parse_all, Arena};

use crate::invoke::Invoke;
use crate::prelude::*;
use crate::utils::DeserializeStdin;
use crate::{CtrlContext, ServerContext};

/// Lease granted to a section the daemon isn't tracking yet (boot, external
/// edit) — one shared-server `MAX_LIFETIME_SECONDS`, so a surviving client's
/// next refresh lands well within it. Granted mappings otherwise expire on the
/// lifetime the protocol layer reported to the client.
const GRACE_LEASE: Duration = Duration::from_secs(3600);
/// Longest lease we'll track regardless of what a client asks for, and what a
/// request for a *permanent* mapping (`lifetime = None`, i.e. UPnP
/// `NewLeaseDuration = 0`) is granted instead — this gateway has no way to hold
/// a WAN port open safely forever.
const MAX_LEASE: Duration = Duration::from_secs(604_800);
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);
const CLIENT_CACHE_TTL: Duration = Duration::from_secs(10);
const WAN_CACHE_TTL: Duration = Duration::from_secs(30);
const LAN_CACHE_TTL: Duration = Duration::from_secs(30);
const UCI_RETRIES: usize = 4;

pub const KIND_PCP: &str = "PCP";
pub const KIND_UPNP: &str = "UPnP";
pub const KIND_SNI: &str = "SNI";

// UPnP IGD error codes, reused verbatim by the shared PCP core.
const IGD_ACTION_FAILED: u16 = 501;
const IGD_CONFLICT: u16 = 718;

/// The daemon's port-control instance. RPC handlers use it for lease expiry
/// display and to invalidate the authorization cache when a toggle changes.
/// Unset in CLI/configs-only mode.
pub static PORT_CONTROL: OnceLock<Arc<PortControl>> = OnceLock::new();

pub struct PortControl {
    uci_root: PathBuf,
    /// PCP epoch origin (seconds-since-start in every response).
    started: Instant,
    /// Serializes every lease/UCI mutation (create/renew, remove, sweep), so a
    /// renewal can't be granted while the sweep is collecting its section.
    write_serial: tokio::sync::Mutex<()>,
    /// Auto-forward section name → lease expiry. In-memory only; see module doc.
    leases: Mutex<HashMap<String, Instant>>,
    wan_cache: Mutex<Option<(Instant, Option<Ipv4Addr>)>>,
    lan_cache: Mutex<Option<(Instant, Arc<Vec<LanAddr>>)>>,
    /// Requesting IP → resolved+authorized device (None = unauthorized).
    client_cache: Mutex<HashMap<Ipv4Addr, (Instant, Option<Client>)>>,
    sni: Arc<SniDemux>,
}

#[derive(Clone, Debug)]
struct Client {
    /// Uppercase MAC (the devices convention).
    mac: String,
    /// Neighbor-table interface (e.g. "br-lan.101"), for zone resolution.
    iface: String,
}

#[derive(Clone, Debug)]
struct LanAddr {
    addr: Ipv4Addr,
    prefix: u8,
}

impl PortControl {
    /// Requires a tokio runtime: the demux's constructor spawns its prune task.
    pub fn new(uci_root: PathBuf) -> Arc<Self> {
        Arc::new_cyclic(|weak: &std::sync::Weak<Self>| {
            let hairpin_weak = weak.clone();
            let weak = weak.clone();
            Self {
                uci_root,
                started: Instant::now(),
                write_serial: tokio::sync::Mutex::new(()),
                leases: Mutex::new(HashMap::new()),
                wan_cache: Mutex::new(None),
                lan_cache: Mutex::new(None),
                client_cache: Mutex::new(HashMap::new()),
                // Reconcile all live ports because callbacks may arrive out of order.
                sni: SniDemux::with_on_change(
                    move |port, active| {
                        if let Some(pc) = weak.upgrade() {
                            tokio::spawn(on_sni_change(pc, port, active));
                        }
                    },
                    Some(Arc::new(move |ip| {
                        let weak = hairpin_weak.clone();
                        Box::pin(async move { weak.upgrade()?.lan_prefix_for(ip).await })
                    })),
                ),
            }
        })
    }

    /// Seconds remaining on a section's lease, if the daemon is tracking one.
    pub fn lease_remaining(&self, section: &str) -> Option<u64> {
        let leases = self.leases.lock().unwrap();
        leases
            .get(section)
            .map(|t| t.saturating_duration_since(Instant::now()).as_secs())
    }

    /// Drop cached authorization results so a toggle change applies immediately.
    pub fn invalidate_clients(&self) {
        self.client_cache.lock().unwrap().clear();
    }

    /// Stop tracking leases for sections removed outside port control.
    pub fn forget_leases(&self, sections: &[String]) {
        let mut leases = self.leases.lock().unwrap();
        for section in sections {
            leases.remove(section);
        }
    }

    fn epoch(&self) -> u32 {
        self.started.elapsed().as_secs() as u32
    }

    fn bump_lease(&self, section: String, lease: Duration) {
        self.leases
            .lock()
            .unwrap()
            .insert(section, Instant::now() + lease);
    }

    fn reload_firewall(&self) {
        crate::published_ports::reload_firewall();
    }

    async fn reload_firewall_wait(&self) -> Result<(), Error> {
        #[cfg(test)]
        return Ok(());
        #[cfg(not(test))]
        crate::published_ports::reload_firewall_wait().await
    }

    async fn prepare_sni(&self) -> Result<(), u8> {
        #[cfg(test)]
        return Ok(());
        #[cfg(not(test))]
        self.sni.prepare().await
    }

    pub(crate) async fn lock_writes(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.write_serial.lock().await
    }

    /// Resolve the requesting IP to an authorized device: a neighbor-table
    /// entry on a LAN bridge, whose MAC has an `_allow_pcp`-flagged DHCP host
    /// entry. `None` = unknown or not authorized. Both outcomes are cached
    /// briefly (PCP clients probe several times per handshake).
    async fn authorized_client(&self, peer: Ipv4Addr) -> Option<Client> {
        {
            let cache = self.client_cache.lock().unwrap();
            if let Some((at, client)) = cache.get(&peer) {
                if at.elapsed() < CLIENT_CACHE_TTL {
                    return client.clone();
                }
            }
        }
        let client = self.resolve_client(peer).await;
        let mut cache = self.client_cache.lock().unwrap();
        // Evict expired entries so spoofed-source floods can't grow the map
        // without bound; live LAN peers re-enter on their next request.
        cache.retain(|_, (at, _)| at.elapsed() < CLIENT_CACHE_TTL);
        cache.insert(peer, (Instant::now(), client.clone()));
        client
    }

    async fn resolve_client(&self, peer: Ipv4Addr) -> Option<Client> {
        let neigh = tokio::process::Command::new("ip")
            .args(["neigh", "show"])
            .invoke(ErrorKind::Network.into())
            .await
            .ok()
            .and_then(|out| String::from_utf8(out).ok())?;
        let (mac, iface) = parse_neigh(&neigh, peer)?;
        let uci_root = self.uci_root.clone();
        let allowed = uci_task(move || async move {
            let arena = Arena::new();
            let cfgs = parse_all(&uci_root, &arena, &["dhcp"]).await?;
            Ok(pcp_allowed_macs(&cfgs["dhcp"])?)
        })
        .await
        .map(|allowed| allowed.contains(&mac))
        .unwrap_or(false);
        allowed.then_some(Client { mac, iface })
    }

    /// Drops hostname routes attributed to unauthorized devices.
    /// Unattributed routes expire with their leases.
    pub(crate) async fn reap_unauthorized_sni_routes(&self) {
        let routes = self.sni.snapshot();
        if routes.is_empty() {
            return;
        }
        let uci_root = self.uci_root.clone();
        let lookup = uci_task(move || async move {
            let arena = Arena::new();
            let cfgs = parse_all(&uci_root, &arena, &["dhcp"]).await?;
            let ip_to_mac = ip_to_mac_map(&cfgs["dhcp"]).await?;
            Ok((ip_to_mac, pcp_allowed_macs(&cfgs["dhcp"])?))
        })
        .await;
        let (mut ip_to_mac, allowed) = match lookup {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("port-control: SNI route ownership lookup failed: {e}");
                return;
            }
        };
        // Live neighbors override DHCP records.
        if let Some(neigh) = tokio::process::Command::new("ip")
            .args(["neigh", "show"])
            .invoke(ErrorKind::Network.into())
            .await
            .ok()
            .and_then(|out| String::from_utf8(out).ok())
        {
            for e in crate::devices::parse_neigh_output(&neigh) {
                if e.interface.starts_with("br-") {
                    ip_to_mac.insert(e.ip, e.mac);
                }
            }
        }
        let doomed = unauthorized_sni_routes(routes, &ip_to_mac, &allowed);
        if doomed.is_empty() {
            return;
        }
        for route in &doomed {
            tracing::warn!(
                "port-control: dropping SNI route {} on {}:{} -> {}: its device is no longer \
                 authorized",
                route.hostname,
                route.ext_ip,
                route.ext_port,
                route.target
            );
            self.sni.unregister(
                route.ext_ip,
                route.ext_port,
                std::slice::from_ref(&route.hostname),
                route.target,
            );
        }
        {
            let _serial = self.write_serial.lock().await;
            if let Err(e) = self.sync_sni_rules().await {
                tracing::warn!("port-control: reconciling SNI admission failed: {e}");
            }
        }
        let wans: std::collections::BTreeSet<Ipv4Addr> = doomed.iter().map(|r| r.ext_ip).collect();
        for wan in wans {
            self.sync_sni_fallback(wan).await;
        }
    }

    /// The router's WAN IPv4 (via ubus), cached briefly.
    async fn wan_ipv4(&self) -> Option<Ipv4Addr> {
        {
            let cache = self.wan_cache.lock().unwrap();
            if let Some((at, ip)) = *cache {
                if at.elapsed() < WAN_CACHE_TTL {
                    return ip;
                }
            }
        }
        let ip = crate::system::get_wan_ipv4().await.ok().flatten();
        *self.wan_cache.lock().unwrap() = Some((Instant::now(), ip));
        ip
    }

    /// Forces the next WAN lookup to bypass the cache.
    pub fn invalidate_wan(&self) {
        *self.wan_cache.lock().unwrap() = None;
    }

    /// The router's own IPv4 addresses on LAN bridges (`br-*`), cached briefly.
    async fn lan_addrs(&self) -> Arc<Vec<LanAddr>> {
        {
            let cache = self.lan_cache.lock().unwrap();
            if let Some((at, addrs)) = cache.as_ref() {
                if at.elapsed() < LAN_CACHE_TTL {
                    return addrs.clone();
                }
            }
        }
        let out = tokio::process::Command::new("ip")
            .args(["-j", "addr", "show"])
            .invoke(ErrorKind::Network.into())
            .await
            .ok()
            .and_then(|out| String::from_utf8(out).ok())
            .unwrap_or_default();
        let addrs = Arc::new(parse_lan_addrs(&out));
        *self.lan_cache.lock().unwrap() = Some((Instant::now(), addrs.clone()));
        addrs
    }

    /// The router address on the matching bridged subnet.
    async fn lan_addr_for(&self, ip: Ipv4Addr) -> Option<LanAddr> {
        self.lan_addrs().await.iter().find_map(|a| {
            let mask = prefix_mask(a.prefix);
            (u32::from(a.addr) & mask == u32::from(ip) & mask).then(|| a.clone())
        })
    }

    /// The IGD address advertised to the peer.
    async fn lan_ip_for(&self, peer: Ipv4Addr) -> Option<Ipv4Addr> {
        self.lan_addr_for(peer).await.map(|a| a.addr)
    }

    /// The prefix length of the matching bridged subnet.
    async fn lan_prefix_for(&self, ip: Ipv4Addr) -> Option<u8> {
        self.lan_addr_for(ip).await.map(|a| a.prefix)
    }

    async fn add_forward_for_kind(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        count: u16,
        peer: Ipv4Addr,
        lifetime: Option<u32>,
        kind: &'static str,
    ) -> Result<(), u16> {
        if count == 0
            || source.port().checked_add(count - 1).is_none()
            || target.port().checked_add(count - 1).is_none()
        {
            return Err(IGD_ACTION_FAILED);
        }
        // The forwarding target must be the requesting peer.
        if *target.ip() != peer {
            return Err(IGD_ACTION_FAILED);
        }
        let Some(client) = self.authorized_client(peer).await else {
            return Err(IGD_ACTION_FAILED);
        };
        let _serial = self.write_serial.lock().await;
        let section = section_name(&client.mac, source.port());
        let uci_root = self.uci_root.clone();
        let section_task = section.clone();
        let outcome = uci_task(move || async move {
            apply_forward_uci(
                &uci_root,
                &section_task,
                kind,
                &client.mac,
                Some(&client.iface),
                source,
                target,
                count,
            )
            .await
        })
        .await
        .map_err(|e| {
            tracing::warn!("port-control: applying forward failed: {e}");
            IGD_ACTION_FAILED
        })?;
        let lease = lease_for(lifetime);
        match outcome {
            ApplyOutcome::Conflict => Err(IGD_CONFLICT),
            ApplyOutcome::Unchanged => {
                self.bump_lease(section, lease);
                Ok(())
            }
            ApplyOutcome::Written => {
                tracing::info!(
                    "port-control: {kind} forward {}#{count} -> {target} ({section})",
                    source
                );
                self.bump_lease(section, lease);
                self.reload_firewall();
                Ok(())
            }
        }
    }

    /// Remove the client's forwards to `internal_port` (PCP identifies a
    /// mapping by its target; several external ports may forward to it).
    async fn remove_forward_for(&self, peer: Ipv4Addr, internal_port: u16) {
        let Some(client) = self.authorized_client(peer).await else {
            return;
        };
        self.remove_client_forwards(&client.mac, move |r| {
            r.dest_port
                .as_deref()
                .and_then(parse_port_range)
                .is_some_and(|range| range.0 == internal_port)
        })
        .await;
    }

    /// Remove the client's forward at external port `source` (UPnP identifies
    /// a mapping by its external port). Returns whether one was removed.
    async fn remove_by_source(&self, source: SocketAddrV4, peer: Ipv4Addr) -> bool {
        let Some(client) = self.authorized_client(peer).await else {
            return false;
        };
        let external_port = source.port();
        self.remove_client_forwards(&client.mac, move |r| {
            r.src_dport
                .as_deref()
                .and_then(parse_port_range)
                .is_some_and(|range| range.0 == external_port)
        })
        .await
            > 0
    }

    /// Remove the auto-forward sections owned by `mac` whose redirect matches
    /// `matches`, dropping their leases. Returns how many were removed.
    async fn remove_client_forwards(
        &self,
        mac: &str,
        matches: impl Fn(&FirewallRedirect) -> bool + Send + 'static,
    ) -> usize {
        let _serial = self.write_serial.lock().await;
        let uci_root = self.uci_root.clone();
        let mac = mac.to_string();
        let removed: Vec<String> = uci_task(move || async move {
            let arena = Arena::new();
            let cfgs = parse_all(&uci_root, &arena, &["firewall"]).await?;
            let names: Vec<String> = cfgs["firewall"]
                .sections
                .iter()
                .filter_map(|sec| {
                    let r = sec.get::<FirewallRedirect>().ok()?;
                    r._apf_label.as_ref()?;
                    if !r
                        ._apf_mac
                        .as_deref()
                        .is_some_and(|m| m.eq_ignore_ascii_case(&mac))
                    {
                        return None;
                    }
                    if !matches(&r) {
                        return None;
                    }
                    sec.name().map(|n| n.to_string())
                })
                .collect();
            if names.is_empty() {
                return Ok(names);
            }
            remove_auto_sections(&uci_root, &names).await?;
            Ok(names)
        })
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("port-control: removing forward failed: {e}");
            Vec::new()
        });
        if !removed.is_empty() {
            tracing::info!("port-control: removed forward(s) {removed:?}");
            self.forget_leases(&removed);
            self.reload_firewall();
        }
        removed.len()
    }

    /// One sweep: grant a grace lease to any auto section the daemon isn't
    /// tracking yet (boot, external edit), drop lease entries for vanished
    /// sections, and collect sections whose lease has expired. Holds the write
    /// lock throughout, so a renewal either lands before the expiry decision or
    /// re-creates its section afterwards — it can never be granted and then
    /// collected by the same sweep.
    /// The auto forwards pointing at `peer`, as IGD mapping entries. Each
    /// forward opens both transports, so it reports as one TCP and one UDP
    /// entry — a client asks about a single protocol at a time and must find
    /// the mapping it just made.
    async fn forwards_for(&self, peer: Ipv4Addr) -> Vec<MappingEntry> {
        let uci_root = self.uci_root.clone();
        let target = peer.to_string();
        let found: Vec<(String, u16, u16, String)> = uci_task(move || async move {
            let arena = Arena::new();
            let cfgs = parse_all(&uci_root, &arena, &["firewall"]).await?;
            Ok(cfgs["firewall"]
                .sections
                .iter()
                .filter_map(|sec| {
                    let r = sec.get::<FirewallRedirect>().ok()?;
                    let kind = r._apf_label?;
                    if r.dest_ip.as_deref() != Some(target.as_str()) {
                        return None;
                    }
                    let external = r.src_dport.as_deref().and_then(parse_port_range)?.0;
                    let internal = r.dest_port.as_deref().and_then(parse_port_range)?.0;
                    Some((sec.name()?.to_string(), external, internal, kind))
                })
                .collect())
        })
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("port-control: listing forwards failed: {e}");
            Vec::new()
        });

        let now = Instant::now();
        let leases = self.leases.lock().unwrap();
        found
            .into_iter()
            .flat_map(|(section, external_port, internal_port, kind)| {
                // An untracked section (fresh boot) reports 0 = permanent
                // rather than "already expired".
                let lease_seconds = leases
                    .get(&section)
                    .map(|t| t.saturating_duration_since(now).as_secs() as u32)
                    .unwrap_or(0);
                ["TCP", "UDP"]
                    .into_iter()
                    .map(move |protocol| MappingEntry {
                        external_port,
                        internal: SocketAddrV4::new(peer, internal_port),
                        protocol,
                        description: format!("Automatic port use ({kind})"),
                        lease_seconds,
                    })
            })
            .collect()
    }

    async fn sweep(&self) -> Result<(), Error> {
        let _serial = self.write_serial.lock().await;
        let uci_root = self.uci_root.clone();
        let (sections, reserved) = uci_task(move || async move {
            let arena = Arena::new();
            let cfgs = parse_all(&uci_root, &arena, &["firewall", "dhcp"]).await?;
            let sections: Vec<AutoSection> = cfgs["firewall"]
                .sections
                .iter()
                .filter_map(|sec| {
                    let r = sec.get::<FirewallRedirect>().ok()?;
                    r._apf_label.as_ref()?;
                    Some(AutoSection {
                        name: sec.name().map(|n| n.to_string())?,
                        mac: r._apf_mac.map(|m| m.to_uppercase()),
                        dest_ip: r.dest_ip,
                    })
                })
                .collect();
            // Statically reserved addresses can't be handed to anyone else, so
            // they pin a forward even with no lease on file.
            let mut reserved: HashMap<String, String> = HashMap::new();
            cfgs["dhcp"].each::<DhcpHost, Error>(|_, host| {
                if let Some(ip) = host.ip.filter(|ip| !ip.is_empty()) {
                    reserved.insert(host.mac.to_uppercase(), ip);
                }
            })?;
            Ok((sections, reserved))
        })
        .await?;

        let now = Instant::now();
        let expired: Vec<String> = {
            let mut leases = self.leases.lock().unwrap();
            leases.retain(|k, _| sections.iter().any(|s| &s.name == k));
            let mut expired = Vec::new();
            for section in &sections {
                match leases.get(&section.name) {
                    None => {
                        leases.insert(section.name.clone(), now + GRACE_LEASE);
                    }
                    Some(t) if *t <= now => expired.push(section.name.clone()),
                    Some(_) => {}
                }
            }
            expired
        };

        // A forward is only meaningful while the device still holds the address
        // it points at. Renewals re-resolve that address, but a client that
        // asked for a permanent mapping never renews — so bind the forward to
        // the DHCP assignment directly, per RFC 6887 §15 and the RFC 6970 §5.10
        // requirement to drop mappings when an internal address is released.
        // Without lease files we can't tell "no lease" from "unreadable", so we
        // leave every forward alone rather than guess.
        let unbound = match crate::devices::current_lease_ips().await {
            Some(leases) => unbound_sections(&sections, &reserved, &leases)
                .into_iter()
                .filter(|name| !expired.contains(name))
                .collect(),
            None => Vec::new(),
        };

        if expired.is_empty() && unbound.is_empty() {
            return Ok(());
        }
        if !expired.is_empty() {
            tracing::info!("port-control: expiring stale auto forward(s): {expired:?}");
        }
        if !unbound.is_empty() {
            tracing::info!(
                "port-control: removing auto forward(s) whose device no longer holds the \
                 address they point at: {unbound:?}"
            );
        }

        let doomed: Vec<String> = expired.into_iter().chain(unbound).collect();
        let uci_root = self.uci_root.clone();
        let names = doomed.clone();
        let removed =
            uci_task(move || async move { remove_auto_sections(&uci_root, &names).await }).await?;
        self.forget_leases(&doomed);
        if removed > 0 {
            self.reload_firewall();
        }
        Ok(())
    }

    /// Refuses a grant unless its listener and WAN admission both succeed.
    async fn add_sni_route(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
        lifetime: Option<u32>,
    ) -> Result<(), u8> {
        let _serial = self.write_serial.lock().await;
        self.prepare_sni().await?;
        let source =
            SocketAddrV4::new(self.wan_ipv4().await.unwrap_or(*source.ip()), source.port());
        let port = source.port();
        let uci_root = self.uci_root.clone();
        let conflicts = uci_task(move || async move {
            let arena = Arena::new();
            let cfgs = parse_all(&uci_root, &arena, &["firewall"]).await?;
            Ok(sni_port_conflicts(&cfgs["firewall"], port))
        })
        .await
        .map_err(|e| {
            tracing::warn!("port-control: SNI conflict scan failed: {e}");
            RESULT_NO_RESOURCES
        })?;
        if conflicts {
            return Err(RESULT_HOSTNAME_TAKEN);
        }

        let registration =
            self.sni
                .register_transaction(*source.ip(), port, hostnames, target, lifetime)?;

        if let Err(e) = self.sync_sni_rules().await {
            self.sni.rollback(registration);
            if let Err(rollback) = self.sync_sni_rules().await {
                tracing::warn!("port-control: rolling back SNI admission failed: {rollback}");
            }
            tracing::warn!("port-control: SNI admission failed: {e}");
            return Err(RESULT_NO_RESOURCES);
        }

        tracing::info!(
            "port-control: SNI route(s) {hostnames:?} on {source} -> {target} \
             (lease {lifetime:?}s)"
        );
        self.sync_sni_fallback(*source.ip()).await;
        Ok(())
    }

    async fn remove_sni_route(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
    ) {
        let _serial = self.write_serial.lock().await;
        let routes: Vec<_> = self
            .sni
            .snapshot()
            .into_iter()
            .filter(|route| {
                route.ext_port == source.port()
                    && route.target == target
                    && hostnames.contains(&route.hostname)
            })
            .collect();
        for route in routes {
            self.sni.unregister(
                route.ext_ip,
                route.ext_port,
                std::slice::from_ref(&route.hostname),
                target,
            );
        }
        if let Err(e) = self.sync_sni_rules().await {
            tracing::warn!("port-control: reconciling SNI admission failed: {e}");
        }
        self.sync_sni_fallback(*source.ip()).await;
    }

    async fn remove_sni_routes_for_ips(&self, ips: &[String]) {
        if ips.is_empty() {
            return;
        }
        let routes: Vec<SniRoute> = self
            .sni
            .snapshot()
            .into_iter()
            .filter(|route| ips.iter().any(|ip| ip == &route.target.ip().to_string()))
            .collect();
        if routes.is_empty() {
            return;
        }
        let _serial = self.write_serial.lock().await;
        for route in &routes {
            self.sni.unregister(
                route.ext_ip,
                route.ext_port,
                std::slice::from_ref(&route.hostname),
                route.target,
            );
        }
        if let Err(e) = self.sync_sni_rules().await {
            tracing::warn!("port-control: reconciling SNI admission failed: {e}");
        }
        let wans: std::collections::BTreeSet<Ipv4Addr> =
            routes.iter().map(|route| route.ext_ip).collect();
        for wan in wans {
            self.sync_sni_fallback(wan).await;
        }
    }

    pub(crate) async fn displace_sni_routes(&self, ranges: &[(u16, u16)]) {
        let routes: Vec<SniRoute> = self
            .sni
            .snapshot()
            .into_iter()
            .filter(|route| {
                ranges
                    .iter()
                    .any(|range| ranges_overlap(*range, (route.ext_port, route.ext_port)))
            })
            .collect();
        if routes.is_empty() {
            return;
        }
        let _serial = self.write_serial.lock().await;
        for route in &routes {
            self.sni.unregister(
                route.ext_ip,
                route.ext_port,
                std::slice::from_ref(&route.hostname),
                route.target,
            );
        }
        if let Err(e) = self.sync_sni_rules().await {
            tracing::warn!("port-control: reconciling SNI admission failed: {e}");
        }
        let wans: std::collections::BTreeSet<Ipv4Addr> =
            routes.iter().map(|route| route.ext_ip).collect();
        for wan in wans {
            self.sync_sni_fallback(wan).await;
        }
    }

    /// Mirrors Remote Access source policy for unknown SNI on port 443.
    pub(crate) async fn sync_sni_fallback(&self, wan: Ipv4Addr) {
        let has_443 = self
            .sni
            .snapshot()
            .iter()
            .any(|r| r.ext_port == 443 && r.ext_ip == wan);
        let mode = {
            let uci_root = self.uci_root.clone();
            uci_task(move || async move {
                let arena = Arena::new();
                let cfgs = parse_all(&uci_root, &arena, &["startwrt"]).await?;
                Ok(crate::system::preferences(&cfgs["startwrt"])?.remote_access)
            })
            .await
        };
        let ui = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 443);
        let source = match mode.as_ref() {
            Ok(mode) => remote_access_fallback_source(Ok(mode), wan),
            Err(e) => {
                tracing::warn!("port-control: reading Remote Access preference failed: {e}");
                remote_access_fallback_source(Err(()), wan)
            }
        };
        match (has_443, source) {
            (true, Some(source)) => {
                if let Err(code) = self.sni.register_local_fallback(wan, 443, ui, source) {
                    tracing::warn!("port-control: remote-access fallback on 443 refused ({code})");
                }
            }
            _ => self.sni.unregister_fallback(wan, 443, ui),
        }
    }

    /// Reconciles WAN admission against the demux's live ports.
    async fn sync_sni_rules(&self) -> Result<(), Error> {
        let want = self
            .sni
            .snapshot()
            .into_iter()
            .map(|route| route.ext_port)
            .collect();
        self.sync_sni_rules_to(want).await
    }

    async fn sync_sni_rules_to(&self, want: std::collections::BTreeSet<u16>) -> Result<(), Error> {
        let uci_root = self.uci_root.clone();
        if uci_task(move || async move { reconcile_sni_rules_uci(&uci_root, want).await }).await? {
            self.reload_firewall_wait().await?;
        }
        Ok(())
    }

    /// Re-keys routes and reconciles WAN admission.
    async fn sni_maintain(&self) {
        self.reap_unauthorized_sni_routes().await;
        let wan = self.wan_ipv4().await;
        let _serial = self.write_serial.lock().await;
        if let Some(ip) = wan {
            if self.sni.snapshot().iter().any(|r| r.ext_ip != ip) {
                tracing::info!("port-control: re-keying SNI routes onto WAN address {ip}");
                self.sni.rekey_ipv4(ip);
            }
        }
        if let Err(e) = self.sync_sni_rules().await {
            tracing::warn!("port-control: reconciling SNI admission failed: {e}");
        }
        if let Some(ip) = wan {
            self.sync_sni_fallback(ip).await;
        }
    }
}

/// Reconciles live ports so reordered callbacks remain idempotent.
async fn on_sni_change(pc: Arc<PortControl>, port: u16, active: bool) {
    tracing::debug!(
        "port-control: SNI listener on port {port} {}",
        if active { "started" } else { "stopped" }
    );
    let _serial = pc.write_serial.lock().await;
    if let Err(e) = pc.sync_sni_rules().await {
        tracing::warn!("port-control: reconciling SNI admission failed: {e}");
    }
}

/// Which interface a request physically arrived on, for the arrival-interface
/// (reverse-path) check. PCP rides unauthenticated UDP, so a device on one LAN
/// bridge can present a *different* bridge's device address as its source and
/// open that device's ports (a cross-segment spoof). Requiring the datagram to
/// have arrived on the same interface the neighbor table places the claimed
/// source on closes that case. UPnP is TCP — the handshake already proves the
/// source — so it is [`Arrival::Unchecked`]. The same-segment case (attacker and
/// victim on one bridge) is unaffected and unsolvable without L2 source guard.
#[derive(Clone, Copy)]
enum Arrival {
    /// TCP-proven (UPnP): no L2 check applies.
    Unchecked,
    /// PCP: the datagram arrived on this interface index (`IP_PKTINFO`).
    On(u32),
    /// PCP: the arrival interface could not be resolved — fail closed.
    Indeterminate,
}

/// Whether a request that arrived as `arrival` may act as a device the neighbor
/// table currently places on `neigh_iface`. A PCP datagram must have arrived on
/// that same interface; anything else is a cross-segment source spoof and is
/// rejected. Fails closed when either the arrival interface or the neighbor's
/// can't be resolved.
fn arrival_matches(arrival: Arrival, neigh_iface: &str) -> bool {
    match arrival {
        Arrival::Unchecked => true,
        Arrival::On(arrival_idx) => match if_nametoindex(neigh_iface) {
            Ok(idx) if idx == arrival_idx => true,
            Ok(_) => {
                tracing::debug!(
                    "PCP: request arrived on ifindex {arrival_idx} but the claimed device is on \
                     {neigh_iface}; rejecting cross-segment spoof"
                );
                false
            }
            // The neighbor's interface vanished between resolution and now:
            // nothing to validate against, so fail closed.
            Err(e) => {
                tracing::warn!("PCP: cannot resolve ifindex for {neigh_iface} ({e}); rejecting");
                false
            }
        },
        Arrival::Indeterminate => {
            tracing::warn!("PCP: arrival interface undetermined; rejecting (fail-closed)");
            false
        }
    }
}

/// Labels forwards by which listener they arrived through — the trait itself
/// can't distinguish a PCP MAP from a UPnP AddPortMapping.
struct Via {
    pc: Arc<PortControl>,
    kind: &'static str,
    /// Interface the request arrived on, for the PCP arrival-interface check.
    arrival: Arrival,
}

impl GatewayBackend for Via {
    async fn add_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        count: u16,
        peer: Ipv4Addr,
        lifetime: Option<u32>,
    ) -> Result<(), u16> {
        self.pc
            .add_forward_for_kind(source, target, count, peer, lifetime, self.kind)
            .await
    }

    async fn remove_forward(&self, peer: Ipv4Addr, internal_port: u16) {
        self.pc.remove_forward_for(peer, internal_port).await
    }

    async fn remove_forward_by_source(&self, source: SocketAddrV4, peer: Ipv4Addr) -> bool {
        self.pc.remove_by_source(source, peer).await
    }

    async fn external_ipv4(&self, _peer: Ipv4Addr) -> Option<Ipv4Addr> {
        self.pc.wan_ipv4().await
    }

    async fn is_known_client(&self, peer: Ipv4Addr) -> bool {
        let Some(client) = self.pc.authorized_client(peer).await else {
            return false;
        };
        // A PCP request must have arrived on the interface the neighbor table
        // places its claimed source on; a UPnP request is TCP-proven and skips
        // the check. Gating here means a mismatch falls through the shared
        // core's `handle` as NOT_AUTHORIZED, covering both MAP and lifetime-0
        // delete — nothing reaches `add_forward`/`remove_forward` without it.
        arrival_matches(self.arrival, &client.iface)
    }

    async fn list_forwards(&self, peer: Ipv4Addr) -> Vec<MappingEntry> {
        self.pc.forwards_for(peer).await
    }

    fn sni(&self) -> Option<&Arc<SniDemux>> {
        Some(&self.pc.sni)
    }

    async fn add_sni_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
        lifetime: Option<u32>,
    ) -> Result<(), u8> {
        self.pc
            .add_sni_route(source, target, hostnames, lifetime)
            .await
    }

    async fn remove_sni_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
    ) {
        self.pc.remove_sni_route(source, target, hostnames).await
    }
}

/// Runs !Send uciedit work on a dedicated current-thread runtime.
async fn uci_task<T, F, Fut>(f: F) -> Result<T, Error>
where
    T: Send + 'static,
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, Error>>,
{
    tokio::task::spawn_blocking(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| Error::new(eyre!("uci runtime: {e}"), ErrorKind::Filesystem))?
            .block_on(f())
    })
    .await
    .map_err(|e| Error::new(eyre!("uci task panicked: {e}"), ErrorKind::Filesystem))?
}

/// Stable section key scoped by device and external port.
fn section_name(mac: &str, external_port: u16) -> String {
    format!(
        "apf_{}_{}",
        mac.replace(':', "").to_lowercase(),
        external_port
    )
}

/// Clamps permanent and short requests to a sweepable finite lease.
fn lease_for(lifetime: Option<u32>) -> Duration {
    lifetime
        .map(|s| Duration::from_secs(u64::from(s)))
        .unwrap_or(MAX_LEASE)
        .max(SWEEP_INTERVAL)
        .min(MAX_LEASE)
}

struct AutoSection {
    name: String,
    mac: Option<String>,
    dest_ip: Option<String>,
}

/// Sections whose tagged owner no longer holds the destination address.
/// Untagged sections are ignored.
fn unbound_sections(
    sections: &[AutoSection],
    reserved: &HashMap<String, String>,
    leases: &HashMap<String, String>,
) -> Vec<String> {
    sections
        .iter()
        .filter(|section| {
            let (Some(mac), Some(dest_ip)) = (&section.mac, &section.dest_ip) else {
                return false;
            };
            !reserved.get(mac).is_some_and(|ip| ip == dest_ip)
                && !leases.get(mac).is_some_and(|ip| ip == dest_ip)
        })
        .map(|section| section.name.clone())
        .collect()
}

enum ApplyOutcome {
    Conflict,
    Unchanged,
    Written,
}

fn desired_redirect(
    kind: &str,
    mac: &str,
    zone: String,
    source: SocketAddrV4,
    target: SocketAddrV4,
    count: u16,
) -> FirewallRedirect {
    FirewallRedirect {
        name: format!("Automatic port use ({kind})"),
        src: "wan".into(),
        dest: Some(zone),
        target: "DNAT".into(),
        proto: vec!["tcp".into(), "udp".into()],
        src_dport: Some(range_string(source.port(), count)),
        src_ip: None,
        dest_ip: Some(target.ip().to_string()),
        dest_port: Some(range_string(target.port(), count)),
        enabled: Some("1".into()),
        _pp_id: None,
        _pp_mac: None,
        _apf_label: Some(kind.into()),
        _apf_mac: Some(mac.into()),
        _pp_wan_override: None,
    }
}

/// Whether the requested range overlaps a router-owned WAN port.
fn reserves_wan_port(firewall: &uciedit::Config<'_>, want: (u16, u16)) -> bool {
    !wan_reserved_overlaps(firewall, want, true, true).is_empty()
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReservedOverlap {
    pub ports: String,
    pub held_by_sni: bool,
}

/// Enabled IPv4 WAN-input rules overlapping the requested ports and transports.
pub(crate) fn wan_reserved_overlaps(
    firewall: &uciedit::Config<'_>,
    want: (u16, u16),
    tcp: bool,
    udp: bool,
) -> Vec<ReservedOverlap> {
    let mut overlaps: Vec<ReservedOverlap> = Vec::new();
    for sec in &firewall.sections {
        let Ok(rule) = sec.get::<FirewallRule>() else {
            continue;
        };
        if rule.target != FirewallTarget::ACCEPT || rule.enabled.as_deref() == Some("0") {
            continue;
        }
        if rule.src != "wan" || rule.dest.is_some() || rule.family.as_deref() == Some("ipv6") {
            continue;
        }
        // fw4 treats an empty protocol list as TCP and UDP.
        let (rule_tcp, rule_udp) = if rule.proto.is_empty() {
            (true, true)
        } else {
            let any = rule
                .proto
                .iter()
                .any(|p| p.eq_ignore_ascii_case("all") || p.eq_ignore_ascii_case("tcpudp"));
            (
                any || rule.proto.iter().any(|p| p.eq_ignore_ascii_case("tcp")),
                any || rule.proto.iter().any(|p| p.eq_ignore_ascii_case("udp")),
            )
        };
        if !((tcp && rule_tcp) || (udp && rule_udp)) {
            continue;
        }
        let Some(spec) = rule.dest_port.as_deref() else {
            continue;
        };
        let held_by_sni = rule._apf_label.as_deref() == Some(KIND_SNI);
        if parse_port_range(spec).is_some_and(|range| ranges_overlap(want, range))
            && !overlaps
                .iter()
                .any(|p| p.ports == spec && p.held_by_sni == held_by_sni)
        {
            overlaps.push(ReservedOverlap {
                ports: spec.to_string(),
                held_by_sni,
            });
        }
    }
    overlaps
}

/// Matches renewals across PCP and UPnP.
fn redirect_matches(existing: &FirewallRedirect, desired: &FirewallRedirect) -> bool {
    existing.src_dport == desired.src_dport
        && existing.dest_port == desired.dest_port
        && existing.dest_ip == desired.dest_ip
        && existing.dest == desired.dest
        && existing.enabled == desired.enabled
        && existing._apf_mac.as_deref().map(str::to_uppercase)
            == desired._apf_mac.as_deref().map(str::to_uppercase)
}

#[allow(clippy::too_many_arguments)]
async fn apply_forward_uci(
    uci_root: &Path,
    section: &str,
    kind: &str,
    mac: &str,
    arp_iface: Option<&str>,
    source: SocketAddrV4,
    target: SocketAddrV4,
    count: u16,
) -> Result<ApplyOutcome, Error> {
    let want = (source.port(), source.port() + (count - 1));
    let mut retries = UCI_RETRIES;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(uci_root, &arena, &["firewall", "startwrt"]).await?;
        let zone = arp_iface
            .and_then(|iface| crate::published_ports::zone_for_arp_iface(&cfgs, iface))
            .unwrap_or_else(|| "lan".into());
        let desired = desired_redirect(kind, mac, zone, source, target, count);

        // Any *other* enabled WAN-ingress DNAT redirect overlapping the
        // requested external range blocks the mapping — manual published ports
        // included. LAN-side DNAT (e.g. the profiles' DNS-Override hijack on
        // port 53) shares no external port space and must not conflict.
        let mut ours: Option<FirewallRedirect> = None;
        for sec in &cfgs["firewall"].sections {
            let Ok(r) = sec.get::<FirewallRedirect>() else {
                continue;
            };
            if r.target != "DNAT" || r.enabled.as_deref() == Some("0") {
                continue;
            }
            if sec.name().as_deref() == Some(section) {
                ours = Some(r);
                continue;
            }
            if r.src != "wan" {
                continue;
            }
            let Some(range) = r.src_dport.as_deref().and_then(parse_port_range) else {
                continue;
            };
            if ranges_overlap(want, range) {
                return Ok(ApplyOutcome::Conflict);
            }
        }
        // Ports the router answers on itself from the WAN — Remote Access
        // (80/443/22) and the VPN server's listen port — are input-chain
        // ACCEPT *rules*, invisible to the redirect scan above. nftables
        // applies prerouting DNAT before the routing decision, so granting one
        // of these would silently divert the router's own web UI, SSH, or
        // inbound VPN to the requesting device (issue #3451; the manual path
        // surfaces the same collision as a confirmation dialog the user can
        // override — see `published_ports::set`). A protocol client can't be
        // asked, so refuse, as StartTunnel reserves the port its HTTP→HTTPS
        // redirect owns.
        if reserves_wan_port(&cfgs["firewall"], want) {
            return Ok(ApplyOutcome::Conflict);
        }
        if let Some(ours) = &ours {
            if redirect_matches(ours, &desired) {
                return Ok(ApplyOutcome::Unchanged);
            }
        }

        cfgs["firewall"]
            .sections
            .retain(|s| s.name().as_deref() != Some(section));
        cfgs["firewall"].append(&desired, Some(section))?;
        match dump_all(uci_root, cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(e) => return Err(e.into()),
            Ok(()) => return Ok(ApplyOutcome::Written),
        }
    }
}

/// Remove the named sections, but only ones actually tagged as auto forwards.
/// Returns how many were removed.
async fn remove_auto_sections(uci_root: &Path, names: &[String]) -> Result<usize, Error> {
    let mut retries = UCI_RETRIES;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(uci_root, &arena, &["firewall"]).await?;
        let mut removed = 0usize;
        cfgs["firewall"].sections.retain(|sec| {
            let is_target = sec
                .name()
                .as_deref()
                .is_some_and(|n| names.iter().any(|x| x == n))
                && sec
                    .get::<FirewallRedirect>()
                    .ok()
                    .is_some_and(|r| r._apf_label.is_some());
            if is_target {
                removed += 1;
            }
            !is_target
        });
        if removed == 0 {
            return Ok(0);
        }
        match dump_all(uci_root, cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(e) => return Err(e.into()),
            Ok(()) => return Ok(removed),
        }
    }
}

fn sni_section_name(port: u16) -> String {
    format!("apf_sni_{port}")
}

fn desired_sni_rule(port: u16) -> FirewallRule {
    FirewallRule {
        name: "SNI demux (hostname routes)".into(),
        src: "wan".into(),
        proto: vec!["tcp".into()],
        dest_port: Some(port.to_string()),
        target: FirewallTarget::ACCEPT,
        family: Some("ipv4".into()),
        enabled: Some("1".into()),
        _apf_label: Some(KIND_SNI.into()),
        ..Default::default()
    }
}

/// Mirrors Remote Access firewall source policy.
fn remote_access_fallback_source(mode: Result<&str, ()>, wan: Ipv4Addr) -> Option<FallbackSource> {
    match mode {
        Ok("always") => Some(FallbackSource::Any),
        Ok("default") if wan.is_private() => Some(FallbackSource::PrivateOnly),
        _ => None,
    }
}

fn protocols_include_tcp(protocols: &[String]) -> bool {
    protocols.is_empty()
        || protocols.iter().any(|protocol| {
            protocol.eq_ignore_ascii_case("tcp")
                || protocol.eq_ignore_ascii_case("all")
                || protocol.eq_ignore_ascii_case("tcpudp")
        })
}

/// Whether the TCP port is held by a DNAT or incompatible router service.
fn sni_port_conflicts(firewall: &uciedit::Config<'_>, port: u16) -> bool {
    let want = (port, port);
    for sec in &firewall.sections {
        if let Ok(r) = sec.get::<FirewallRedirect>() {
            if r.target == "DNAT"
                && r.enabled.as_deref() != Some("0")
                && r.src == "wan"
                && protocols_include_tcp(&r.proto)
                && r.src_dport
                    .as_deref()
                    .and_then(parse_port_range)
                    .is_some_and(|range| ranges_overlap(want, range))
            {
                return true;
            }
            continue;
        }
        let Ok(rule) = sec.get::<FirewallRule>() else {
            continue;
        };
        if rule._apf_label.as_deref() == Some(KIND_SNI) {
            continue;
        }
        // Remote Access on 443 is served through the SNI fallback.
        if port == 443
            && (rule.name.starts_with(crate::system::REMOTE_RULE_PREFIX)
                || sec
                    .name()
                    .is_some_and(|n| n.starts_with(crate::system::REMOTE_RULE_PREFIX)))
        {
            continue;
        }
        if rule.target != FirewallTarget::ACCEPT || rule.enabled.as_deref() == Some("0") {
            continue;
        }
        if rule.src != "wan" || rule.dest.is_some() || rule.family.as_deref() == Some("ipv6") {
            continue;
        }
        if !protocols_include_tcp(&rule.proto) {
            continue;
        }
        if rule
            .dest_port
            .as_deref()
            .and_then(parse_port_range)
            .is_some_and(|range| ranges_overlap(want, range))
        {
            return true;
        }
    }
    false
}

/// Replaces SNI admission rules with one per requested port.
/// Returns whether UCI changed.
async fn reconcile_sni_rules_uci(
    uci_root: &Path,
    want: std::collections::BTreeSet<u16>,
) -> Result<bool, Error> {
    let mut retries = UCI_RETRIES;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(uci_root, &arena, &["firewall"]).await?;
        let mut seen: std::collections::BTreeSet<u16> = std::collections::BTreeSet::new();
        let mut changed = false;
        cfgs["firewall"].sections.retain(|sec| {
            let Ok(rule) = sec.get::<FirewallRule>() else {
                return true;
            };
            if rule._apf_label.as_deref() != Some(KIND_SNI) {
                return true;
            }
            let port = rule
                .dest_port
                .as_deref()
                .and_then(parse_port_range)
                .map(|r| r.0);
            let keep = port.is_some_and(|p| want.contains(&p) && seen.insert(p));
            if !keep {
                changed = true;
            }
            keep
        });
        for port in want.iter().filter(|p| !seen.contains(p)) {
            cfgs["firewall"].append(&desired_sni_rule(*port), Some(&sni_section_name(*port)))?;
            changed = true;
        }
        if !changed {
            return Ok(false);
        }
        match dump_all(uci_root, cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(e) => return Err(e.into()),
            Ok(()) => return Ok(true),
        }
    }
}

/// "443" → (443, 443); "1000-1009" → (1000, 1009). None on garbage.
pub(crate) fn parse_port_range(s: &str) -> Option<(u16, u16)> {
    match s.split_once('-') {
        Some((a, b)) => {
            let a: u16 = a.trim().parse().ok()?;
            let b: u16 = b.trim().parse().ok()?;
            (a <= b).then_some((a, b))
        }
        None => {
            let p: u16 = s.trim().parse().ok()?;
            Some((p, p))
        }
    }
}

pub(crate) fn ranges_overlap(a: (u16, u16), b: (u16, u16)) -> bool {
    a.0 <= b.1 && b.0 <= a.1
}

fn range_string(start: u16, count: u16) -> String {
    if count <= 1 {
        start.to_string()
    } else {
        format!("{}-{}", start, start as u32 + count as u32 - 1)
    }
}

fn prefix_mask(prefix: u8) -> u32 {
    if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix.min(32) as u32)
    }
}

/// `ip neigh show` entry for `peer` on a LAN bridge, as `(MAC, iface)`.
/// FAILED/INCOMPLETE entries have no lladdr and are skipped by the parser; a
/// non-`br-*` interface (e.g. the WAN uplink) never authorizes.
fn parse_neigh(neigh: &str, peer: Ipv4Addr) -> Option<(String, String)> {
    let peer = peer.to_string();
    crate::devices::parse_neigh_output(neigh)
        .into_iter()
        .find(|e| e.ip == peer && e.interface.starts_with("br-"))
        .map(|e| (e.mac, e.interface))
}

fn pcp_allowed_macs(
    dhcp: &uciedit::Config<'_>,
) -> Result<std::collections::HashSet<String>, Error> {
    let mut allowed = std::collections::HashSet::new();
    dhcp.each::<DhcpHost, Error>(|_, host| {
        if host._allow_pcp.as_deref() == Some("1") {
            allowed.insert(host.mac.to_uppercase());
        }
    })?;
    Ok(allowed)
}

/// Unauthorized routes with identifiable owners.
/// Unattributed routes are retained.
fn unauthorized_sni_routes(
    routes: Vec<SniRoute>,
    ip_to_mac: &HashMap<String, String>,
    allowed: &std::collections::HashSet<String>,
) -> Vec<SniRoute> {
    routes
        .into_iter()
        .filter(|route| {
            ip_to_mac
                .get(&route.target.ip().to_string())
                .is_some_and(|mac| !allowed.contains(&mac.to_uppercase()))
        })
        .collect()
}

/// The router's own `br-*` IPv4 addresses from `ip -j addr show`.
fn parse_lan_addrs(json: &str) -> Vec<LanAddr> {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for iface in parsed.as_array().into_iter().flatten() {
        let Some(name) = iface.get("ifname").and_then(|v| v.as_str()) else {
            continue;
        };
        if !name.starts_with("br-") {
            continue;
        }
        for info in iface
            .get("addr_info")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            if info.get("family").and_then(|v| v.as_str()) != Some("inet") {
                continue;
            }
            let Some(addr) = info
                .get("local")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
            else {
                continue;
            };
            let prefix = info.get("prefixlen").and_then(|v| v.as_u64()).unwrap_or(24) as u8;
            out.push(LanAddr { addr, prefix });
        }
    }
    out
}

/// Stable UPnP device UDN, derived from the router's root CA (generated once
/// at first boot).
fn device_uuid() -> String {
    let seed = crate::ssl::read_root_ca_pem().unwrap_or_else(|_| "startwrt".into());
    format_uuid(&Sha256::digest(seed.as_bytes()))
}

/// Run all port-control servers for the life of the daemon. Each half
/// self-restarts on error, and each runs in its own supervised task.
pub async fn run(pc: Arc<PortControl>) {
    // In-memory routes do not survive restart.
    {
        let _serial = pc.write_serial.lock().await;
        if let Err(e) = pc.sync_sni_rules().await {
            tracing::warn!("port-control: purging SNI admission failed: {e}");
        }
    }
    tokio::join!(
        supervise("PCP", pc.clone(), run_pcp),
        supervise("IGD", pc.clone(), run_igd),
        supervise("SSDP", pc.clone(), run_ssdp),
        supervise("sweep", pc, run_sweep),
    );
}

/// Run one server for the life of the daemon. Each already retries its own
/// errors internally, so the only way out is a panic — and sharing one task
/// would let that panic take the other three with it. Losing the sweep is the
/// case that matters: forwards would stay open with nothing left to close them.
async fn supervise<F, Fut>(name: &'static str, pc: Arc<PortControl>, start: F)
where
    F: Fn(Arc<PortControl>) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    loop {
        match tokio::spawn(start(pc.clone())).await {
            Ok(()) => return,
            Err(e) => {
                tracing::error!("port-control {name} server panicked ({e}); restarting");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

async fn run_pcp(pc: Arc<PortControl>) {
    loop {
        if let Err(e) = serve_pcp(&pc).await {
            tracing::warn!("PCP server failed, retrying: {e}");
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
}

/// Receive one PCP datagram, returning its bytes, sender, and the interface it
/// arrived on (from `IP_PKTINFO`). `Arrival::Indeterminate` when the kernel gave
/// us no usable ifindex, so the caller fails closed rather than skip the check.
async fn recv_pcp(
    socket: &UdpSocket,
    buf: &mut [u8],
) -> std::io::Result<(usize, SocketAddrV4, Arrival)> {
    socket
        .async_io(Interest::READABLE, || {
            let mut iov = [std::io::IoSliceMut::new(&mut *buf)];
            let mut cmsg = nix::cmsg_space!(libc::in_pktinfo);
            let msg = recvmsg::<SockaddrIn>(
                socket.as_raw_fd(),
                &mut iov,
                Some(&mut cmsg),
                MsgFlags::empty(),
            )
            .map_err(std::io::Error::from)?;
            let mut arrival = Arrival::Indeterminate;
            for c in msg.cmsgs()? {
                if let ControlMessageOwned::Ipv4PacketInfo(info) = c {
                    if info.ipi_ifindex != 0 {
                        arrival = Arrival::On(info.ipi_ifindex as u32);
                    }
                }
            }
            let from = msg
                .address
                .map(|a: SockaddrIn| SocketAddrV4::new(a.ip(), a.port()))
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "PCP recvmsg: no source address",
                    )
                })?;
            Ok((msg.bytes, from, arrival))
        })
        .await
}

async fn serve_pcp(pc: &Arc<PortControl>) -> Result<(), Error> {
    let socket = Arc::new(
        UdpSocket::bind((Ipv4Addr::UNSPECIFIED, PCP_PORT))
            .await
            .with_kind(ErrorKind::Network)?,
    );
    // Ask the kernel to attach IP_PKTINFO to each datagram, so `recv_pcp` learns
    // the interface it arrived on — the input to the cross-segment spoof check.
    setsockopt(&*socket, Ipv4PacketInfo, &true)
        .map_err(|e| Error::new(eyre!("IP_PKTINFO: {e}"), ErrorKind::Network))?;
    tracing::info!("PCP server listening on 0.0.0.0:{PCP_PORT}");
    let mut buf = [0u8; 1100];
    loop {
        let (n, from, arrival) = recv_pcp(&socket, &mut buf)
            .await
            .with_kind(ErrorKind::Network)?;
        let peer = *from.ip();
        // The WAN firewall zone already rejects unsolicited input; this guard
        // is defense in depth for the unspecified bind.
        if !peer.is_private() {
            continue;
        }
        // Handle each datagram off the recv loop: a MAP that writes UCI (flash
        // I/O + conflict retries) must not stall every other client's probes.
        // Writers serialize on the PortControl write lock, not the socket.
        let req = buf[..n].to_vec();
        let socket = socket.clone();
        let via = Via {
            pc: pc.clone(),
            kind: KIND_PCP,
            arrival,
        };
        tokio::spawn(async move {
            if let Some(resp) = handle(&via, peer, &req, via.pc.epoch()).await {
                socket.send_to(&resp, from).await.ok();
            }
        });
    }
}

async fn run_igd(pc: Arc<PortControl>) {
    let root_desc: Arc<str> = Arc::from(render_root_desc("StartWRT", &device_uuid()));
    let app = Router::new()
        .route(ROOT_DESC_PATH, {
            let root_desc = root_desc.clone();
            get(move |headers: HeaderMap| serve_static(headers, root_desc.clone(), "text/xml"))
        })
        .route(
            SCPD_PATH,
            get(|headers: HeaderMap| serve_static(headers, Arc::from(SCPD), "text/xml")),
        )
        .route(
            CIF_SCPD_PATH,
            get(|headers: HeaderMap| serve_static(headers, Arc::from(CIF_SCPD), "text/xml")),
        )
        .route(CONTROL_PATH, post(igd_control))
        .with_state(pc);
    loop {
        match tokio::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, IGD_HTTP_PORT)).await {
            Ok(listener) => {
                tracing::info!("UPnP IGD control server listening on 0.0.0.0:{IGD_HTTP_PORT}");
                if let Err(e) = axum::serve(
                    listener,
                    app.clone()
                        .into_make_service_with_connect_info::<SocketAddr>(),
                )
                .await
                {
                    tracing::warn!("UPnP IGD control server exited, retrying: {e}");
                }
            }
            Err(e) => tracing::warn!("UPnP IGD control server bind failed, retrying: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn igd_control(
    State(pc): State<Arc<PortControl>>,
    ConnectInfo(from): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: String,
) -> Response {
    let IpAddr::V4(peer) = from.ip() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !peer.is_private() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let via = Via {
        pc,
        kind: KIND_UPNP,
        // UPnP is TCP: the handshake proves the source address.
        arrival: Arrival::Unchecked,
    };
    handle_control(&via, peer, &headers, &body).await
}

async fn run_ssdp(pc: Arc<PortControl>) {
    loop {
        if let Err(e) = serve_ssdp(&pc).await {
            tracing::warn!("UPnP IGD SSDP responder failed, retrying: {e}");
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
}

async fn serve_ssdp(pc: &Arc<PortControl>) -> Result<(), Error> {
    let socket = {
        use socket2::{Domain, Protocol, Socket, Type};
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .with_kind(ErrorKind::Network)?;
        socket
            .set_reuse_address(true)
            .with_kind(ErrorKind::Network)?;
        socket
            .bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, SSDP_PORT).into())
            .with_kind(ErrorKind::Network)?;
        socket.set_nonblocking(true).with_kind(ErrorKind::Network)?;
        UdpSocket::from_std(socket.into()).with_kind(ErrorKind::Network)?
    };
    tracing::info!("UPnP IGD SSDP responder listening on 0.0.0.0:{SSDP_PORT}");
    let uuid = device_uuid();
    // (Re-)join the SSDP multicast group on every LAN bridge periodically, so
    // bridges created after startup (new profiles/VLANs) are picked up.
    let mut join_tick = tokio::time::interval(Duration::from_secs(60));
    let mut buf = [0u8; 2048];
    loop {
        tokio::select! {
            _ = join_tick.tick() => {
                for a in pc.lan_addrs().await.iter() {
                    // Already-joined groups error; that's fine.
                    let _ = socket.join_multicast_v4(SSDP_MULTICAST, a.addr);
                }
            }
            r = socket.recv_from(&mut buf) => {
                let (n, from) = r.with_kind(ErrorKind::Network)?;
                let Ok(text) = std::str::from_utf8(&buf[..n]) else {
                    continue;
                };
                if !text.starts_with("M-SEARCH") {
                    continue;
                }
                let Some(st) = header_value(text, "st") else {
                    continue;
                };
                if !st_matches(&st) {
                    continue;
                }
                let IpAddr::V4(peer) = from.ip() else {
                    continue;
                };
                if !peer.is_private() {
                    continue;
                }
                // Only announce to authorized devices; everything else sees no
                // IGD on this network.
                if pc.authorized_client(peer).await.is_none() {
                    continue;
                }
                let Some(server_ip) = pc.lan_ip_for(peer).await else {
                    continue;
                };
                let resp = ssdp_response(server_ip, &uuid);
                if let Err(e) = socket.send_to(resp.as_bytes(), from).await {
                    tracing::debug!("UPnP IGD: failed to answer M-SEARCH from {from}: {e}");
                }
            }
        }
    }
}

async fn run_sweep(pc: Arc<PortControl>) {
    let mut tick = tokio::time::interval(SWEEP_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    tick.tick().await; // consume the immediate first tick
    loop {
        tick.tick().await;
        if let Err(e) = pc.sweep().await {
            tracing::warn!("port-control sweep failed: {e}");
        }
        pc.sni_maintain().await;
    }
}

/// One automatic port use for the published-ports UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomaticPortUse {
    /// UCI section name (`apf_<mac>_<extport>`).
    pub id: String,
    /// Which mechanism created it ("PCP", "UPnP", or "SNI").
    pub kind: String,
    pub device_mac: String,
    pub device_name: Option<String>,
    /// Forward target address on the LAN.
    pub internal_ip: Option<String>,
    /// Internal port (or range).
    pub ports: String,
    /// External port (or range) on the WAN.
    pub public_ports: String,
    /// Seconds until the lease expires if not renewed (None when the daemon
    /// isn't tracking it yet, e.g. right after boot).
    pub expires_secs: Option<u64>,
    /// Hostname for an SNI route; absent for plain forwards.
    pub hostname: Option<String>,
}

/// Display names keyed by uppercase MAC; static UCI names take precedence.
pub(crate) fn device_display_names(
    dhcp: &uciedit::Config<'_>,
) -> Result<HashMap<String, String>, Error> {
    let mut names: HashMap<String, String> = crate::device_names::load_all()
        .into_iter()
        .filter_map(|(mac, cached)| cached.hostname.map(|name| (mac, name)))
        .collect();
    dhcp.each::<DhcpHost, Error>(|_, host| {
        if let Some(name) = host.name.as_ref().filter(|n| !n.is_empty()) {
            names.insert(host.mac.to_uppercase(), name.clone());
        }
    })?;
    Ok(names)
}

/// LAN IPv4 to uppercase MAC; static reservations override live leases.
pub(crate) async fn ip_to_mac_map(
    dhcp: &uciedit::Config<'_>,
) -> Result<HashMap<String, String>, Error> {
    let mut ip_to_mac: HashMap<String, String> = HashMap::new();
    if let Some(leases) = crate::devices::current_lease_ips().await {
        for (mac, ip) in leases {
            ip_to_mac.insert(ip, mac);
        }
    }
    dhcp.each::<DhcpHost, Error>(|_, host| {
        if let Some(ip) = host.ip.clone().filter(|ip| !ip.is_empty()) {
            ip_to_mac.insert(ip, host.mac.to_uppercase());
        }
    })?;
    Ok(ip_to_mac)
}

/// Sorted hostname and device labels for routes overlapping the requested range.
/// Lookup failures produce empty labels.
pub(crate) async fn sni_route_holders(
    dhcp: &uciedit::Config<'_>,
    want: (u16, u16),
) -> (Vec<String>, Vec<String>) {
    let Some(pc) = PORT_CONTROL.get() else {
        return (Vec::new(), Vec::new());
    };
    let names = device_display_names(dhcp).unwrap_or_default();
    let ip_to_mac = ip_to_mac_map(dhcp).await.unwrap_or_default();
    let mut hostnames = Vec::new();
    let mut devices = Vec::new();
    for route in pc.sni.snapshot() {
        if !ranges_overlap(want, (route.ext_port, route.ext_port)) {
            continue;
        }
        if !hostnames.contains(&route.hostname) {
            hostnames.push(route.hostname.clone());
        }
        if let Some(mac) = ip_to_mac.get(&route.target.ip().to_string()) {
            let display = names.get(mac).cloned().unwrap_or_else(|| mac.clone());
            if !devices.contains(&display) {
                devices.push(display);
            }
        }
    }
    hostnames.sort();
    devices.sort();
    (hostnames, devices)
}

#[instrument(skip_all)]
pub async fn auto_list(ctx: ServerContext) -> Result<Vec<AutomaticPortUse>, Error> {
    let uci_root = ctx.uci_root();
    let arena = Arena::new();
    let cfgs = parse_all(&uci_root, &arena, &["firewall", "dhcp"]).await?;

    let names = device_display_names(&cfgs["dhcp"])?;

    let mut out = Vec::new();
    for sec in &cfgs["firewall"].sections {
        let Ok(r) = sec.get::<FirewallRedirect>() else {
            continue;
        };
        let Some(kind) = r._apf_label.clone() else {
            continue;
        };
        let id = sec.name().unwrap_or_default().to_string();
        let device_mac = r._apf_mac.clone().unwrap_or_default().to_uppercase();
        out.push(AutomaticPortUse {
            expires_secs: PORT_CONTROL.get().and_then(|pc| pc.lease_remaining(&id)),
            device_name: names.get(&device_mac).cloned(),
            id,
            kind,
            device_mac,
            internal_ip: r.dest_ip.clone(),
            ports: r.dest_port.clone().unwrap_or_default(),
            public_ports: r.src_dport.clone().unwrap_or_default(),
            hostname: None,
        });
    }

    if let Some(pc) = PORT_CONTROL.get() {
        let ip_to_mac = ip_to_mac_map(&cfgs["dhcp"]).await?;
        for route in pc.sni.snapshot() {
            let target_ip = route.target.ip().to_string();
            let device_mac = ip_to_mac.get(&target_ip).cloned().unwrap_or_default();
            out.push(AutomaticPortUse {
                id: format!("sni_{}_{}", route.ext_port, route.hostname),
                kind: KIND_SNI.into(),
                device_name: names.get(&device_mac).cloned(),
                device_mac,
                internal_ip: Some(target_ip),
                ports: route.target.port().to_string(),
                public_ports: route.ext_port.to_string(),
                expires_secs: route.remaining_secs,
                hostname: Some(route.hostname),
            });
        }
    }
    Ok(out)
}

/// Re-keys live SNI routes after a WAN address change.
#[instrument(skip_all)]
pub async fn wan_changed(ctx: ServerContext) -> Result<Value, Error> {
    if !ctx.effectful() {
        return Ok(Value::Null);
    }
    if let Some(pc) = PORT_CONTROL.get() {
        pc.invalidate_wan();
        pc.sni_maintain().await;
    }
    Ok(Value::Null)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetAutoForwardRequest {
    pub mac: String,
    pub allow: bool,
}

/// Changes a device's default-off automatic port-use permission.
#[instrument(skip_all)]
pub async fn set_auto_forward<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<SetAutoForwardRequest>,
) -> Result<(), Error> {
    // Reject malformed UCI host identifiers.
    if !crate::published_ports::validate_mac(&req.mac) {
        return Err(Error::new(
            eyre!("invalid mac: {}", req.mac),
            ErrorKind::InvalidValue,
        ));
    }
    let mac_upper = req.mac.to_uppercase();
    let allow = req.allow;
    let written =
        crate::devices::upsert_dhcp_host(&ctx.uci_root(), &req.mac, move |host, existed| {
            if !existed && !allow {
                return false;
            }
            host._allow_pcp = allow.then(|| "1".to_string());
            true
        })
        .await
        .map_err(|err| {
            crate::activity::log(
                "device",
                "auto-forward",
                false,
                &format!("Failed to update automatic port forwarding for {mac_upper}"),
                Some(&err.to_string()),
            );
            err
        })?;
    if written {
        crate::activity::log(
            "device",
            "auto-forward",
            true,
            &format!(
                "{} automatic port forwarding for {mac_upper}",
                if req.allow { "Enabled" } else { "Disabled" }
            ),
            None,
        );
        // The write may create a host section that dnsmasq must reload.
        if ctx.effectful() {
            crate::devices::reload_dnsmasq();
        }
        if !allow {
            close_device_forwards(&req.mac, &[]).await;
        } else if let Some(pc) = PORT_CONTROL.get() {
            pc.invalidate_clients();
        }
    }
    Ok(())
}

/// Revokes a device's automatic forwards and cached authorization.
/// No-op outside daemon mode.
pub(crate) async fn close_device_forwards(mac: &str, known_ips: &[String]) {
    let Some(pc) = PORT_CONTROL.get() else {
        return;
    };
    pc.invalidate_clients();
    pc.remove_client_forwards(mac, |_| true).await;
    pc.remove_sni_routes_for_ips(known_ips).await;
    pc.reap_unauthorized_sni_routes().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(firewall: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("firewall"), firewall).unwrap();
        std::fs::write(dir.path().join("startwrt"), "").unwrap();
        dir
    }

    const MANUAL_FW: &str = "\
config redirect 'pp_a'
\toption name 'NAS'
\toption src 'wan'
\toption dest 'lan'
\toption target 'DNAT'
\tlist proto 'tcp'
\toption src_dport '443'
\toption dest_port '443'
\toption dest_ip '192.168.1.50'
\toption enabled '1'
\toption _pp_id 'a'
\toption _pp_mac 'AA:AA:AA:AA:AA:AA'
";

    #[test]
    fn section_names_are_stable_and_uci_safe() {
        assert_eq!(
            section_name("AA:BB:CC:DD:EE:FF", 8443),
            "apf_aabbccddeeff_8443"
        );
    }

    #[test]
    fn port_range_parsing() {
        assert_eq!(parse_port_range("443"), Some((443, 443)));
        assert_eq!(parse_port_range("1000-1009"), Some((1000, 1009)));
        assert_eq!(parse_port_range("9-1"), None);
        assert_eq!(parse_port_range("nope"), None);
        assert!(ranges_overlap((100, 200), (200, 300)));
        assert!(ranges_overlap((200, 300), (100, 200)));
        assert!(!ranges_overlap((100, 199), (200, 300)));
        assert_eq!(range_string(443, 1), "443");
        assert_eq!(range_string(1000, 10), "1000-1009");
        // The top of the u16 range must not overflow.
        assert_eq!(range_string(65535, 1), "65535");
    }

    #[test]
    fn neigh_parsing_only_accepts_lan_bridges() {
        let neigh = "\
192.168.1.50 dev br-lan lladdr aa:bb:cc:dd:ee:ff REACHABLE
192.168.2.7 dev br-lan.101 lladdr 11:22:33:44:55:66 STALE
203.0.113.1 dev eth1 lladdr de:ad:be:ef:00:01 REACHABLE
192.168.1.66 dev br-lan  FAILED
";
        assert_eq!(
            parse_neigh(neigh, Ipv4Addr::new(192, 168, 1, 50)),
            Some(("AA:BB:CC:DD:EE:FF".into(), "br-lan".into()))
        );
        assert_eq!(
            parse_neigh(neigh, Ipv4Addr::new(192, 168, 2, 7)),
            Some(("11:22:33:44:55:66".into(), "br-lan.101".into()))
        );
        // WAN-side neighbor: never authorized.
        assert_eq!(parse_neigh(neigh, Ipv4Addr::new(203, 0, 113, 1)), None);
        // FAILED entry has no lladdr.
        assert_eq!(parse_neigh(neigh, Ipv4Addr::new(192, 168, 1, 66)), None);
    }

    #[test]
    fn lan_addr_parsing_and_subnet_match() {
        let json = r#"[
            {"ifname":"lo","addr_info":[{"family":"inet","local":"127.0.0.1","prefixlen":8}]},
            {"ifname":"br-lan","addr_info":[
                {"family":"inet","local":"192.168.1.1","prefixlen":24},
                {"family":"inet6","local":"fd00::1","prefixlen":64}
            ]},
            {"ifname":"eth1","addr_info":[{"family":"inet","local":"203.0.113.7","prefixlen":24}]}
        ]"#;
        let addrs = parse_lan_addrs(json);
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0].addr, Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(addrs[0].prefix, 24);
        let mask = prefix_mask(24);
        assert_eq!(
            u32::from(Ipv4Addr::new(192, 168, 1, 50)) & mask,
            u32::from(addrs[0].addr) & mask
        );
    }

    #[tokio::test]
    async fn apply_creates_then_renews_without_rewrite() {
        let dir = temp_root("");
        let source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 8443);
        let target = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 8443);
        let outcome = apply_forward_uci(
            dir.path(),
            "apf_aabbccddeeff_8443",
            KIND_PCP,
            "AA:BB:CC:DD:EE:FF",
            Some("br-lan"),
            source,
            target,
            1,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, ApplyOutcome::Written));
        let written = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(written.contains("apf_aabbccddeeff_8443"));
        assert!(written.contains("option _apf_label 'PCP'"));
        assert!(written.contains("option _apf_mac 'AA:BB:CC:DD:EE:FF'"));
        assert!(written.contains("option dest_ip '192.168.1.50'"));

        // An identical re-assert (the client's periodic refresh) is a no-op.
        let before = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        let outcome = apply_forward_uci(
            dir.path(),
            "apf_aabbccddeeff_8443",
            KIND_PCP,
            "AA:BB:CC:DD:EE:FF",
            Some("br-lan"),
            source,
            target,
            1,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, ApplyOutcome::Unchanged));
        let after = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert_eq!(before, after, "renewal must not rewrite flash");

        // The device re-addressed: same section is replaced, not conflicted.
        let target2 = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 51), 8443);
        let outcome = apply_forward_uci(
            dir.path(),
            "apf_aabbccddeeff_8443",
            KIND_PCP,
            "AA:BB:CC:DD:EE:FF",
            Some("br-lan"),
            source,
            target2,
            1,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, ApplyOutcome::Written));
        let written = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(written.contains("option dest_ip '192.168.1.51'"));
        assert!(!written.contains("option dest_ip '192.168.1.50'"));
    }

    #[tokio::test]
    async fn apply_conflicts_with_manual_published_port() {
        let dir = temp_root(MANUAL_FW);
        let source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 443);
        let target = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 60), 443);
        let outcome = apply_forward_uci(
            dir.path(),
            "apf_112233445566_443",
            KIND_UPNP,
            "11:22:33:44:55:66",
            Some("br-lan"),
            source,
            target,
            1,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, ApplyOutcome::Conflict));
        // Nothing was written.
        let written = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(!written.contains("apf_"));
    }

    /// Input-chain ACCEPTs for services the router answers on itself: Remote
    /// Access (`system.rs`) and a VPN server's listen port (`vpn_server.rs`),
    /// plus three that must *not* reserve anything.
    const ROUTER_FW: &str = "\
config rule 'startwrt_remote_443'
\toption name 'Remote access 443'
\toption src 'wan'
\toption dest_port '443'
\tlist proto 'tcp'
\toption target 'ACCEPT'

config rule 'startwrt_remote_22'
\toption name 'Remote access 22'
\toption src 'wan'
\toption dest_port '22'
\tlist proto 'tcp'
\toption target 'ACCEPT'

config rule 'wg_listen'
\toption name 'VPN server'
\toption src 'wan'
\toption dest_port '51820'
\tlist proto 'udp'
\toption target 'ACCEPT'

config rule 'remote_v6_only'
\toption name 'Remote access over IPv6'
\toption src 'wan'
\toption dest_port '8443'
\toption family 'ipv6'
\tlist proto 'tcp'
\toption target 'ACCEPT'

config rule 'turned_off'
\toption name 'Disabled service'
\toption src 'wan'
\toption dest_port '9443'
\tlist proto 'tcp'
\toption target 'ACCEPT'
\toption enabled '0'

config rule 'wan_to_lan'
\toption name 'Forwarded, not delivered locally'
\toption src 'wan'
\toption dest 'lan'
\toption dest_port '7443'
\tlist proto 'tcp'
\toption target 'ACCEPT'
";

    /// Request `count` external ports from `port` against a fresh `ROUTER_FW`.
    async fn apply_at(port: u16, count: u16) -> ApplyOutcome {
        let dir = temp_root(ROUTER_FW);
        apply_forward_uci(
            dir.path(),
            &section_name("11:22:33:44:55:66", port),
            KIND_PCP,
            "11:22:33:44:55:66",
            Some("br-lan"),
            SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), port),
            SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 60), port),
            count,
        )
        .await
        .unwrap()
    }

    // A prerouting DNAT beats an input-chain ACCEPT, so granting one of these
    // would divert the router's own web UI, SSH, or inbound VPN to the
    // requesting device.
    #[tokio::test]
    async fn apply_refuses_ports_the_router_answers_on() {
        assert!(matches!(apply_at(443, 1).await, ApplyOutcome::Conflict));
        assert!(matches!(apply_at(51820, 1).await, ApplyOutcome::Conflict));
        // A range that merely covers one of them is refused as a whole.
        assert!(matches!(apply_at(20, 6).await, ApplyOutcome::Conflict));
    }

    // ...but the reservation is read from the live rules, so it's exactly as
    // wide as what the router is actually listening for.
    #[tokio::test]
    async fn router_port_reservation_is_narrowly_scoped() {
        // IPv6-only rule: shares no port space with an IPv4 redirect.
        assert!(matches!(apply_at(8443, 1).await, ApplyOutcome::Written));
        // Disabled rule: the feature is off, so the port is free.
        assert!(matches!(apply_at(9443, 1).await, ApplyOutcome::Written));
        // `dest` set = forwarded through the router, not delivered to it.
        assert!(matches!(apply_at(7443, 1).await, ApplyOutcome::Written));
        // Nothing claims this one at all.
        assert!(matches!(apply_at(8080, 1).await, ApplyOutcome::Written));
    }

    #[test]
    fn forwards_are_bound_to_the_address_assignment() {
        let section = |name: &str, mac: &str, ip: &str| AutoSection {
            name: name.into(),
            mac: Some(mac.into()),
            dest_ip: Some(ip.into()),
        };
        let sections = vec![
            section("current", "AA:AA:AA:AA:AA:AA", "192.168.1.50"),
            section("renumbered", "BB:BB:BB:BB:BB:BB", "192.168.1.60"),
            section("no_lease", "CC:CC:CC:CC:CC:CC", "192.168.1.70"),
            section("reserved", "DD:DD:DD:DD:DD:DD", "192.168.1.80"),
            AutoSection {
                name: "untagged".into(),
                mac: None,
                dest_ip: Some("192.168.1.90".into()),
            },
        ];
        let leases = HashMap::from([
            ("AA:AA:AA:AA:AA:AA".to_string(), "192.168.1.50".to_string()),
            // The device is still on the network, but at a different address —
            // the forward now points at whoever holds .60.
            ("BB:BB:BB:BB:BB:BB".to_string(), "192.168.1.61".to_string()),
        ]);
        // A static reservation survives having no lease on file.
        let reserved =
            HashMap::from([("DD:DD:DD:DD:DD:DD".to_string(), "192.168.1.80".to_string())]);

        let mut unbound = unbound_sections(&sections, &reserved, &leases);
        unbound.sort();
        assert_eq!(unbound, vec!["no_lease", "renumbered"]);
    }

    #[tokio::test]
    async fn apply_range_conflicts_on_overlap_only() {
        let dir = temp_root(MANUAL_FW); // holds external 443
        let source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 440);
        let target = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 60), 8440);
        // 440-449 overlaps 443 → conflict.
        let outcome = apply_forward_uci(
            dir.path(),
            "apf_112233445566_440",
            KIND_PCP,
            "11:22:33:44:55:66",
            Some("br-lan"),
            source,
            target,
            10,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, ApplyOutcome::Conflict));
        // 450-459 doesn't → written.
        let source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 450);
        let target = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 60), 8450);
        let outcome = apply_forward_uci(
            dir.path(),
            "apf_112233445566_450",
            KIND_PCP,
            "11:22:33:44:55:66",
            Some("br-lan"),
            source,
            target,
            10,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, ApplyOutcome::Written));
        let written = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(written.contains("option src_dport '450-459'"));
        assert!(written.contains("option dest_port '8450-8459'"));
    }

    // Mappings are keyed by external port (UPnP's mapping identity): a second
    // external port to the same internal port is a second forward, not a
    // silent replacement of the first.
    #[tokio::test]
    async fn same_internal_port_two_external_ports_coexist() {
        let dir = temp_root("");
        let target = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 5080);
        for ext in [80u16, 8080] {
            let source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), ext);
            let outcome = apply_forward_uci(
                dir.path(),
                &section_name("AA:BB:CC:DD:EE:FF", ext),
                KIND_UPNP,
                "AA:BB:CC:DD:EE:FF",
                Some("br-lan"),
                source,
                target,
                1,
            )
            .await
            .unwrap();
            assert!(matches!(outcome, ApplyOutcome::Written));
        }
        let written = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(written.contains("apf_aabbccddeeff_80"));
        assert!(written.contains("apf_aabbccddeeff_8080"));
        assert_eq!(written.matches("option dest_port '5080'").count(), 2);
    }

    // LAN-ingress DNAT (the profiles' DNS-Override hijack) shares no external
    // port space with WAN forwards and must not block a mapping.
    #[tokio::test]
    async fn lan_side_dnat_does_not_conflict() {
        const LAN_DNS_FW: &str = "\
config redirect 'dns_override_lan'
\toption name 'DNS-Override-lan'
\toption src 'lan'
\toption src_dport '53'
\toption dest_ip '192.168.1.1'
\toption dest_port '53'
\toption target 'DNAT'
\tlist proto 'tcp'
\tlist proto 'udp'
";
        let dir = temp_root(LAN_DNS_FW);
        let source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 53);
        let target = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 53);
        let outcome = apply_forward_uci(
            dir.path(),
            &section_name("AA:BB:CC:DD:EE:FF", 53),
            KIND_PCP,
            "AA:BB:CC:DD:EE:FF",
            Some("br-lan"),
            source,
            target,
            1,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, ApplyOutcome::Written));
    }

    #[tokio::test]
    async fn remove_only_touches_auto_sections() {
        let dir = temp_root(MANUAL_FW);
        let source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 8443);
        let target = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 8443);
        apply_forward_uci(
            dir.path(),
            "apf_aabbccddeeff_8443",
            KIND_PCP,
            "AA:BB:CC:DD:EE:FF",
            Some("br-lan"),
            source,
            target,
            1,
        )
        .await
        .unwrap();
        // Asking to remove both names only removes the tagged one.
        let removed = remove_auto_sections(
            dir.path(),
            &["pp_a".to_string(), "apf_aabbccddeeff_8443".to_string()],
        )
        .await
        .unwrap();
        assert_eq!(removed, 1);
        let written = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(written.contains("pp_a"), "manual port survives");
        assert!(!written.contains("apf_"), "auto forward removed");
    }

    // A client asking for a permanent mapping (UPnP NewLeaseDuration = 0, which
    // reaches us as None) gets our maximum lease, never an unbounded one — the
    // sweep must always have a date on which it can close the port.
    #[test]
    fn permanent_request_is_capped_not_forever() {
        assert_eq!(lease_for(None), MAX_LEASE);
        assert_eq!(lease_for(Some(3600)), Duration::from_secs(3600));
        // Longer than we allow: capped.
        assert_eq!(lease_for(Some(31_536_000)), MAX_LEASE);
        // Shorter than a sweep: raised, so it can't expire unseen.
        assert_eq!(lease_for(Some(1)), SWEEP_INTERVAL);
    }

    // Revoking a device's permission closes the ports it opened, and only its
    // own: another device's auto forward and the user's manual rules survive.
    #[tokio::test]
    async fn revoking_a_device_removes_only_its_forwards() {
        let dir = temp_root(MANUAL_FW);
        let source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 8443);
        let target = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 8443);
        apply_forward_uci(
            dir.path(),
            "apf_aabbccddeeff_8443",
            KIND_PCP,
            "AA:BB:CC:DD:EE:FF",
            Some("br-lan"),
            source,
            target,
            1,
        )
        .await
        .unwrap();
        let other_source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 9000);
        let other_target = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 51), 9000);
        apply_forward_uci(
            dir.path(),
            "apf_112233445566_9000",
            KIND_UPNP,
            "11:22:33:44:55:66",
            Some("br-lan"),
            other_source,
            other_target,
            1,
        )
        .await
        .unwrap();

        let pc = PortControl::new(dir.path().to_path_buf());
        pc.bump_lease("apf_aabbccddeeff_8443".to_string(), MAX_LEASE);
        let removed = pc
            .remove_client_forwards("AA:BB:CC:DD:EE:FF", |_| true)
            .await;

        assert_eq!(removed, 1);
        let written = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(
            !written.contains("apf_aabbccddeeff_8443"),
            "revoked device's forward removed"
        );
        assert!(
            written.contains("apf_112233445566_9000"),
            "another device's forward survives"
        );
        assert!(written.contains("pp_a"), "manual port survives");
        assert_eq!(
            pc.lease_remaining("apf_aabbccddeeff_8443"),
            None,
            "its lease is dropped too, so the sweep won't re-grace it"
        );
    }

    // The arrival-interface check: a PCP datagram is honored only if it arrived
    // on the interface the neighbor table places its claimed source on, closing
    // cross-segment source spoofing. UPnP (TCP-proven) always passes.
    #[test]
    fn arrival_interface_check_rejects_cross_segment() {
        let lo = if_nametoindex("lo").expect("loopback resolvable");

        // UPnP: no L2 check, always allowed for an authorized client.
        assert!(arrival_matches(Arrival::Unchecked, "lo"));

        // PCP arriving on the same interface the device lives on: allowed.
        assert!(arrival_matches(Arrival::On(lo), "lo"));

        // PCP claiming a device on a *different* interface than it arrived on:
        // rejected (the cross-segment spoof).
        assert!(!arrival_matches(Arrival::On(lo.wrapping_add(9999)), "lo"));

        // Fail closed when the neighbor's interface can't be resolved...
        assert!(!arrival_matches(
            Arrival::On(lo),
            "definitely-not-an-interface"
        ));
        // ...and when the arrival interface itself was indeterminate.
        assert!(!arrival_matches(Arrival::Indeterminate, "lo"));
    }

    #[tokio::test]
    async fn sni_route_registers_and_admits_inline() {
        let dir = temp_root("");
        let pc = PortControl::new(dir.path().to_path_buf());
        let source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 8443);
        let target = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 443);
        pc.add_sni_route(source, target, &["nas.example.com".to_string()], Some(3600))
            .await
            .unwrap();

        let snap = pc.sni.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].hostname, "nas.example.com");
        assert_eq!(snap[0].target, target);

        let written = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(written.contains("config rule apf_sni_8443"));
        assert!(written.contains("option _apf_label 'SNI'"));
        assert!(written.contains("option dest_port '8443'"));
        assert!(written.contains("option target 'ACCEPT'"));
        assert!(written.contains("option family 'ipv4'"));
        assert!(written.contains("list proto 'tcp'"));

        let target2 = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 51), 443);
        pc.add_sni_route(
            source,
            target2,
            &["cloud.example.com".to_string()],
            Some(3600),
        )
        .await
        .unwrap();
        assert_eq!(pc.sni.snapshot().len(), 2);
        let written = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert_eq!(written.matches("apf_sni_8443").count(), 1);
    }

    #[tokio::test]
    async fn sni_routes_follow_the_permission() {
        let dir = temp_root("");
        std::fs::write(
            dir.path().join("dhcp"),
            "\
config host
\toption mac 'AA:AA:AA:AA:AA:AA'
\toption ip '192.168.1.50'
\toption _allow_pcp '1'

config host
\toption mac 'BB:BB:BB:BB:BB:BB'
\toption ip '192.168.1.51'
\toption _allow_pcp '1'
",
        )
        .unwrap();
        let pc = PortControl::new(dir.path().to_path_buf());
        let source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 8443);
        let a = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 443);
        let b = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 51), 443);
        let stray = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 52), 443);
        for (target, host) in [
            (a, "a.example.com"),
            (b, "b.example.com"),
            (stray, "c.example.com"),
        ] {
            pc.add_sni_route(source, target, &[host.to_string()], Some(3600))
                .await
                .unwrap();
        }
        assert_eq!(pc.sni.snapshot().len(), 3);

        pc.reap_unauthorized_sni_routes().await;
        assert_eq!(pc.sni.snapshot().len(), 3);

        std::fs::write(
            dir.path().join("dhcp"),
            "\
config host
\toption mac 'AA:AA:AA:AA:AA:AA'
\toption ip '192.168.1.50'

config host
\toption mac 'BB:BB:BB:BB:BB:BB'
\toption ip '192.168.1.51'
\toption _allow_pcp '1'
",
        )
        .unwrap();
        pc.reap_unauthorized_sni_routes().await;
        let left: Vec<String> = pc.sni.snapshot().into_iter().map(|r| r.hostname).collect();
        assert_eq!(left, vec!["b.example.com", "c.example.com"]);

        let written = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(written.contains("config rule apf_sni_8443"));

        std::fs::write(
            dir.path().join("dhcp"),
            "\
config host
\toption mac 'AA:AA:AA:AA:AA:AA'
\toption ip '192.168.1.50'
",
        )
        .unwrap();
        pc.reap_unauthorized_sni_routes().await;
        let left: Vec<String> = pc.sni.snapshot().into_iter().map(|r| r.hostname).collect();
        assert_eq!(left, vec!["b.example.com", "c.example.com"]);
    }

    #[test]
    fn unauthorized_routes_need_a_resolved_owner() {
        let route = |ip: [u8; 4], host: &str| SniRoute {
            ext_ip: Ipv4Addr::new(203, 0, 113, 7),
            ext_port: 443,
            hostname: host.to_string(),
            target: SocketAddrV4::new(Ipv4Addr::from(ip), 443),
            remaining_secs: Some(60),
        };
        let routes = vec![
            route([192, 168, 1, 50], "revoked"),
            route([192, 168, 1, 51], "kept"),
            route([192, 168, 1, 52], "unknown"),
        ];
        let ip_to_mac: HashMap<String, String> = [
            ("192.168.1.50".to_string(), "aa:aa:aa:aa:aa:aa".to_string()),
            ("192.168.1.51".to_string(), "BB:BB:BB:BB:BB:BB".to_string()),
        ]
        .into_iter()
        .collect();
        let allowed = std::collections::HashSet::from(["BB:BB:BB:BB:BB:BB".to_string()]);
        let doomed: Vec<String> = unauthorized_sni_routes(routes, &ip_to_mac, &allowed)
            .into_iter()
            .map(|r| r.hostname)
            .collect();
        assert_eq!(doomed, vec!["revoked"]);
    }

    #[tokio::test]
    async fn sni_route_refused_on_owned_ports() {
        let dir = temp_root(MANUAL_FW);
        let pc = PortControl::new(dir.path().to_path_buf());
        let source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 443);
        let target = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 60), 443);
        let err = pc
            .add_sni_route(source, target, &["a.example.com".to_string()], Some(3600))
            .await
            .unwrap_err();
        assert_eq!(err, RESULT_HOSTNAME_TAKEN);
        assert!(
            pc.sni.snapshot().is_empty(),
            "nothing registered on refusal"
        );

        let dir = temp_root(ROUTER_FW);
        let pc = PortControl::new(dir.path().to_path_buf());
        let ssh = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 22);
        let err = pc
            .add_sni_route(
                ssh,
                SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 60), 22),
                &["a.example.com".to_string()],
                Some(3600),
            )
            .await
            .unwrap_err();
        assert_eq!(err, RESULT_HOSTNAME_TAKEN);

        let wg = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 51820);
        pc.add_sni_route(
            wg,
            SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 60), 51820),
            &["b.example.com".to_string()],
            Some(3600),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn remote_access_coexists_on_443_only() {
        let dir = temp_root(ROUTER_FW);
        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["firewall"]).await.unwrap();
        let fw = &cfgs["firewall"];
        assert!(
            !sni_port_conflicts(fw, 443),
            "RA's 443 rides the UI fallback"
        );
        assert!(sni_port_conflicts(fw, 22), "RA's SSH keeps its refusal");

        let dir = temp_root(MANUAL_FW);
        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["firewall"]).await.unwrap();
        assert!(
            sni_port_conflicts(&cfgs["firewall"], 443),
            "a manual DNAT on 443 is not Remote Access"
        );
    }

    #[tokio::test]
    async fn udp_dnat_does_not_block_sni() {
        let dir = temp_root(
            "config redirect\n\
             \toption src 'wan'\n\
             \toption target 'DNAT'\n\
             \tlist proto 'udp'\n\
             \toption src_dport '8443'\n\
             \toption dest_port '8443'\n\
             \toption dest_ip '192.168.1.50'\n",
        );
        let pc = PortControl::new(dir.path().to_path_buf());
        pc.add_sni_route(
            SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 8443),
            SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 60), 8443),
            &["tcp.example.com".to_string()],
            Some(3600),
        )
        .await
        .unwrap();
    }

    #[test]
    fn fallback_source_mirrors_remote_access_modes() {
        let private_wan = Ipv4Addr::new(192, 168, 10, 92);
        let public_wan = Ipv4Addr::new(203, 0, 113, 7);
        assert_eq!(
            remote_access_fallback_source(Ok("default"), private_wan),
            Some(FallbackSource::PrivateOnly)
        );
        assert_eq!(
            remote_access_fallback_source(Ok("default"), public_wan),
            None
        );
        assert_eq!(
            remote_access_fallback_source(Ok("always"), public_wan),
            Some(FallbackSource::Any)
        );
        assert_eq!(
            remote_access_fallback_source(Ok("never"), private_wan),
            None
        );
        assert_eq!(remote_access_fallback_source(Err(()), private_wan), None);
    }

    #[tokio::test]
    async fn sni_admit_rule_reserves_the_port() {
        let dir = temp_root("");
        let pc = PortControl::new(dir.path().to_path_buf());
        let source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 8443);
        let target = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 443);
        pc.add_sni_route(source, target, &["nas.example.com".to_string()], Some(3600))
            .await
            .unwrap();

        let outcome = apply_forward_uci(
            dir.path(),
            &section_name("11:22:33:44:55:66", 8443),
            KIND_PCP,
            "11:22:33:44:55:66",
            Some("br-lan"),
            source,
            SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 60), 8443),
            1,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, ApplyOutcome::Conflict));
    }

    #[tokio::test]
    async fn sni_teardown_removes_the_admit_rule_inline() {
        let dir = temp_root("");
        let pc = PortControl::new(dir.path().to_path_buf());
        let source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 8443);
        let target = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 443);
        let hostnames = vec!["nas.example.com".to_string()];
        pc.add_sni_route(source, target, &hostnames, Some(3600))
            .await
            .unwrap();

        let via = Via {
            pc: pc.clone(),
            kind: KIND_PCP,
            arrival: Arrival::Unchecked,
        };
        via.remove_sni_forward(source, target, &hostnames).await;

        let written = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(!written.contains("apf_sni_8443"));
    }

    #[tokio::test]
    async fn failed_sni_admission_preserves_an_existing_binding() {
        let dir = temp_root("");
        let pc = PortControl::new(dir.path().to_path_buf());
        let source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 8443);
        let target = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 443);
        let hostnames = vec!["nas.example.com".to_string()];
        pc.add_sni_route(source, target, &hostnames, Some(3600))
            .await
            .unwrap();
        std::fs::remove_file(dir.path().join("firewall")).unwrap();
        std::fs::create_dir(dir.path().join("firewall")).unwrap();

        assert!(pc
            .add_sni_route(source, target, &hostnames, Some(3600))
            .await
            .is_err());
        assert_eq!(pc.sni.snapshot().len(), 1);

        let missing = temp_root("");
        std::fs::remove_file(missing.path().join("firewall")).unwrap();
        std::fs::create_dir(missing.path().join("firewall")).unwrap();
        let fresh = PortControl::new(missing.path().to_path_buf());
        assert!(fresh
            .add_sni_route(source, target, &hostnames, Some(3600))
            .await
            .is_err());
        assert!(fresh.sni.snapshot().is_empty());
    }

    #[tokio::test]
    async fn manual_port_use_displaces_sni_routes() {
        let dir = temp_root("");
        let pc = PortControl::new(dir.path().to_path_buf());
        let source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 8443);
        let target = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 443);
        pc.add_sni_route(source, target, &["nas.example.com".to_string()], Some(3600))
            .await
            .unwrap();

        pc.displace_sni_routes(&[(8443, 8443)]).await;

        assert!(pc.sni.snapshot().is_empty());
        let written = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(!written.contains("apf_sni_8443"));
    }

    #[tokio::test]
    async fn removed_static_ip_identifies_offline_sni_routes() {
        let dir = temp_root("");
        let pc = PortControl::new(dir.path().to_path_buf());
        let source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 8443);
        let target = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 443);
        pc.add_sni_route(source, target, &["nas.example.com".to_string()], Some(3600))
            .await
            .unwrap();

        pc.remove_sni_routes_for_ips(&["192.168.1.50".to_string()])
            .await;

        assert!(pc.sni.snapshot().is_empty());
        let written = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(!written.contains("apf_sni_8443"));
    }

    #[tokio::test]
    async fn sni_maintain_purges_strays_and_heals_missing() {
        let dir = temp_root("");
        let pc = PortControl::new(dir.path().to_path_buf());

        std::fs::write(
            dir.path().join("firewall"),
            "config rule 'apf_sni_9443'\n\
             \toption name 'SNI demux (hostname routes)'\n\
             \toption src 'wan'\n\
             \tlist proto 'tcp'\n\
             \toption dest_port '9443'\n\
             \toption target 'ACCEPT'\n\
             \toption enabled '1'\n\
             \toption _apf_label 'SNI'\n\
             \n\
             config rule 'startwrt_remote_80'\n\
             \toption name 'Remote access 80'\n\
             \toption src 'wan'\n\
             \tlist proto 'tcp'\n\
             \toption dest_port '80'\n\
             \toption target 'ACCEPT'\n",
        )
        .unwrap();
        pc.sni_maintain().await;
        let written = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(!written.contains("apf_sni_9443"), "stray rule purged");
        assert!(
            written.contains("startwrt_remote_80"),
            "non-SNI rules survive the purge"
        );

        let source = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 8443);
        let target = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 443);
        pc.add_sni_route(source, target, &["nas.example.com".to_string()], Some(3600))
            .await
            .unwrap();
        std::fs::write(dir.path().join("firewall"), "").unwrap();
        pc.sni_maintain().await;
        let written = std::fs::read_to_string(dir.path().join("firewall")).unwrap();
        assert!(written.contains("apf_sni_8443"), "rule healed");
    }
}
