//! SNI demultiplexer for the PCP HOSTNAME extension: a per-port TCP listener
//! reads the TLS ClientHello, selects a binding (exact → wildcard → fallback),
//! and splices to the internal host. TLS is never terminated; the ClientHello
//! bytes are forwarded verbatim. The internal leg is opened from the client's
//! own source address (source-address preservation, RFC §4.6) via
//! [`crate::net::transparent`].
//!
//! QUIC (§4.5) and wildcards beyond a single leading `*` label are out of scope.

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::net::port_map::pcp::RESULT_NO_RESOURCES;
use crate::net::port_map::pcp::hostname::RESULT_HOSTNAME_TAKEN;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::sync::SyncMutex;

/// (external IP, external port).
type PortKey = (Ipv4Addr, u16);

const CLIENTHELLO_CAP: usize = 16384;
const CLIENTHELLO_TIMEOUT: Duration = Duration::from_secs(5);
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(100);

#[derive(Clone)]
struct Binding {
    target: SocketAddrV4,
    /// `None` for a permanent (DB-backed/manual) binding that never expires.
    expiry: Option<Instant>,
}

/// Which sources a port's fallback admits. Hostname routes are never
/// source-scoped (an SNI client can come from anywhere); the fallback can be,
/// because it may stand in for a firewall rule that was source-scoped — e.g.
/// StartWRT remote access in "behind NAT" mode admits only private sources,
/// and the demux taking its port must not widen that.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallbackSource {
    Any,
    /// RFC1918 sources only.
    PrivateOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fallback {
    target: SocketAddrV4,
    source: FallbackSource,
    /// Open the internal leg source-preserving (`transparent_connect`). False
    /// for a gateway-self fallback: a spoofed-source dial to the gateway's own
    /// listener would be martian-dropped on the loopback reply path.
    transparent: bool,
}

#[derive(Default)]
struct PortBindings {
    /// hostname (lowercase) -> binding; a `*.suffix` key is a wildcard.
    hostnames: BTreeMap<String, Binding>,
    fallback: Option<Fallback>,
}

impl PortBindings {
    fn prune(&mut self, now: Instant) {
        self.hostnames
            .retain(|_, b| b.expiry.is_none_or(|e| e > now));
    }
    fn is_empty(&self) -> bool {
        self.hostnames.is_empty() && self.fallback.is_none()
    }
    /// exact match, then a `*.suffix` wildcard on the parent, then fallback.
    /// Returns the target and whether to open the internal leg
    /// source-preserving. `peer` gates a source-scoped fallback only — hostname
    /// routes match regardless of source.
    fn select(&self, sni: Option<&str>, peer: Ipv4Addr) -> Option<(SocketAddrV4, bool)> {
        if let Some(name) = sni {
            if let Some(b) = self.hostnames.get(name) {
                return Some((b.target, true));
            }
            if let Some((_, rest)) = name.split_once('.') {
                if let Some(b) = self.hostnames.get(&format!("*.{rest}")) {
                    return Some((b.target, true));
                }
            }
        }
        let f = self.fallback?;
        match f.source {
            FallbackSource::Any => {}
            // std's `is_private` is exactly RFC1918, matching the firewall
            // source scoping this fallback stands in for.
            FallbackSource::PrivateOnly if peer.is_private() => {}
            FallbackSource::PrivateOnly => return None,
        }
        Some((f.target, f.transparent))
    }
}

/// One live hostname route, as reported by [`SniDemux::snapshot`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SniRoute {
    pub ext_ip: Ipv4Addr,
    pub ext_port: u16,
    pub hostname: String,
    pub target: SocketAddrV4,
    /// Seconds until the binding expires; `None` for a permanent binding.
    pub remaining_secs: Option<u64>,
}

/// Called `(ext_port, active)` when a port's listener starts/stops, so a gateway
/// can open/close inbound access (e.g. a StartWRT firewall ACCEPT rule).
type OnChange = Box<dyn Fn(u16, bool) + Send + Sync>;

pub struct SniDemux {
    ports: Arc<SyncMutex<BTreeMap<PortKey, PortBindings>>>,
    listeners: SyncMutex<BTreeMap<PortKey, NonDetachingJoinHandle<()>>>,
    on_change: Option<OnChange>,
}

impl SniDemux {
    pub fn new() -> Arc<Self> {
        Self::build(None)
    }

    /// Like [`new`](Self::new) but invokes `on_change` on listener create/teardown.
    pub fn with_on_change(on_change: impl Fn(u16, bool) + Send + Sync + 'static) -> Arc<Self> {
        Self::build(Some(Box::new(on_change)))
    }

    fn build(on_change: Option<OnChange>) -> Arc<Self> {
        let this = Arc::new(Self {
            ports: Arc::new(SyncMutex::new(BTreeMap::new())),
            listeners: SyncMutex::new(BTreeMap::new()),
            on_change,
        });
        let weak = Arc::downgrade(&this);
        tokio::spawn(async move {
            let mut divert_ok = true;
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let Some(this) = weak.upgrade() else { break };
                this.prune();
                // Re-assert the reply-path divert while any listener is active:
                // heals external flushes (networkd restart, nft flush) that
                // would otherwise silently hang all demuxed traffic.
                if this.listeners.peek(|l| !l.is_empty()) {
                    match crate::net::transparent::ensure_divert_infra().await {
                        Ok(repaired) => {
                            if repaired {
                                tracing::warn!(
                                    "SNI demux reply-path divert infra was missing; re-installed"
                                );
                            } else if !divert_ok {
                                tracing::info!("SNI demux reply-path divert re-assert recovered");
                            }
                            divert_ok = true;
                        }
                        Err(e) => {
                            if divert_ok {
                                tracing::warn!("SNI demux reply-path divert re-assert failed: {e}");
                            }
                            divert_ok = false;
                        }
                    }
                }
            }
        });
        this
    }

    /// Register hostname bindings for `(ext_ip, ext_port) -> target` and ensure
    /// the listener runs. `Err(RESULT_HOSTNAME_TAKEN)` if any name is held by a
    /// different target — all-or-nothing; the same target reclaims.
    /// `Err(RESULT_NO_RESOURCES)` if the listener cannot bind: a grant must
    /// never outrun its socket, since the gateway opens firewall access on the
    /// strength of it.
    pub fn register(
        self: &Arc<Self>,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        hostnames: &[String],
        target: SocketAddrV4,
        lifetime_secs: Option<u32>,
    ) -> Result<(), u8> {
        let now = Instant::now();
        let expiry = lifetime_secs.map(|s| now + Duration::from_secs(s as u64));
        let key = (ext_ip, ext_port);
        self.ports.mutate(|ports| {
            let entry = ports.entry(key).or_default();
            entry.prune(now);
            for name in hostnames {
                if let Some(b) = entry.hostnames.get(name) {
                    if b.target != target {
                        return Err(RESULT_HOSTNAME_TAKEN);
                    }
                }
            }
            for name in hostnames {
                entry
                    .hostnames
                    .insert(name.clone(), Binding { target, expiry });
            }
            Ok(())
        })?;
        if let Err(e) = self.ensure_listener(key) {
            tracing::warn!(
                "SNI demux bind on {}:{} failed; refusing the grant: {e}",
                key.0,
                key.1
            );
            // Roll back this call's insertions so snapshot/auto-list never
            // report a route with no listener behind it. Same-target removal is
            // safe: a distinct pre-existing same-target binding implies a live
            // listener, in which case the bind was never attempted.
            self.ports.mutate(|ports| {
                if let Some(entry) = ports.get_mut(&key) {
                    for name in hostnames {
                        if entry
                            .hostnames
                            .get(name)
                            .is_some_and(|b| b.target == target)
                        {
                            entry.hostnames.remove(name);
                        }
                    }
                }
            });
            self.reap_if_empty(key);
            return Err(RESULT_NO_RESOURCES);
        }
        Ok(())
    }

    /// Delete the named bindings (lifetime-0 MAP), only those held by `target`.
    pub fn unregister(
        &self,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        hostnames: &[String],
        target: SocketAddrV4,
    ) {
        let key = (ext_ip, ext_port);
        self.ports.mutate(|ports| {
            if let Some(entry) = ports.get_mut(&key) {
                for name in hostnames {
                    if entry
                        .hostnames
                        .get(name)
                        .is_some_and(|b| b.target == target)
                    {
                        entry.hostnames.remove(name);
                    }
                }
            }
        });
        self.reap_if_empty(key);
    }

    /// Set the hostname-less fallback for `(ext_ip, ext_port) -> target` and
    /// ensure the listener runs. Traffic matching no hostname route (or sending
    /// no SNI) is spliced here, source-preserving, from any source.
    /// `Err(RESULT_HOSTNAME_TAKEN)` if a different target already holds the
    /// fallback; the same target reclaims (idempotent).
    /// `Err(RESULT_NO_RESOURCES)` if the listener cannot bind.
    pub fn register_fallback(
        self: &Arc<Self>,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        target: SocketAddrV4,
    ) -> Result<(), u8> {
        self.register_fallback_with(
            ext_ip,
            ext_port,
            Fallback {
                target,
                source: FallbackSource::Any,
                transparent: true,
            },
        )
    }

    /// Like [`register_fallback`](Self::register_fallback), but for the
    /// gateway's *own* listener (e.g. StartWRT's web UI behind a shared 443):
    /// the internal leg is a plain connect — a source-preserving dial to
    /// ourselves would be martian-dropped — and `source` scopes who may reach
    /// it, mirroring the firewall rule the demux displaced.
    pub fn register_local_fallback(
        self: &Arc<Self>,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        target: SocketAddrV4,
        source: FallbackSource,
    ) -> Result<(), u8> {
        self.register_fallback_with(
            ext_ip,
            ext_port,
            Fallback {
                target,
                source,
                transparent: false,
            },
        )
    }

    fn register_fallback_with(
        self: &Arc<Self>,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        fallback: Fallback,
    ) -> Result<(), u8> {
        let key = (ext_ip, ext_port);
        self.ports.mutate(|ports| {
            let entry = ports.entry(key).or_default();
            if entry.fallback.is_some_and(|f| f.target != fallback.target) {
                return Err(RESULT_HOSTNAME_TAKEN);
            }
            // Same-target re-register also refreshes source/transparency, so a
            // policy change (e.g. a remote-access mode switch) applies in place.
            entry.fallback = Some(fallback);
            Ok(())
        })?;
        if let Err(e) = self.ensure_listener(key) {
            tracing::warn!(
                "SNI demux bind on {}:{} failed; refusing the fallback: {e}",
                key.0,
                key.1
            );
            self.ports.mutate(|ports| {
                if let Some(entry) = ports.get_mut(&key) {
                    if entry.fallback.is_some_and(|f| f.target == fallback.target) {
                        entry.fallback = None;
                    }
                }
            });
            self.reap_if_empty(key);
            return Err(RESULT_NO_RESOURCES);
        }
        Ok(())
    }

    /// Clear the fallback on `(ext_ip, ext_port)`, only if held by `target`.
    pub fn unregister_fallback(&self, ext_ip: Ipv4Addr, ext_port: u16, target: SocketAddrV4) {
        let key = (ext_ip, ext_port);
        self.ports.mutate(|ports| {
            if let Some(entry) = ports.get_mut(&key) {
                if entry.fallback.is_some_and(|f| f.target == target) {
                    entry.fallback = None;
                }
            }
        });
        self.reap_if_empty(key);
    }

    /// The live hostname routes, for gateway UIs (StartWRT's Automatic table).
    /// Fallbacks are not reported — they are port-level, not hostname routes.
    pub fn snapshot(&self) -> Vec<SniRoute> {
        let now = Instant::now();
        self.ports.peek(|ports| {
            ports
                .iter()
                .flat_map(|(&(ext_ip, ext_port), entry)| {
                    entry
                        .hostnames
                        .iter()
                        .filter(|(_, b)| b.expiry.is_none_or(|e| e > now))
                        .map(move |(name, b)| SniRoute {
                            ext_ip,
                            ext_port,
                            hostname: name.clone(),
                            target: b.target,
                            remaining_secs: b
                                .expiry
                                .map(|e| e.saturating_duration_since(now).as_secs()),
                        })
                })
                .collect()
        })
    }

    /// Move every binding keyed to another external IPv4 onto `new_ip` — the
    /// gateway's WAN address changed, but the routes (and their ports) live on.
    /// On hostname collision the binding already at the new key wins; a fallback
    /// already at the new key likewise. Stranded listeners are dropped without
    /// firing `on_change(port, false)` — the port set is unchanged, and a
    /// spawned teardown could race the re-add and close a live port —
    /// then re-ensured on the new key (`on_change(port, true)` is an idempotent
    /// upsert for the gateway). No-op when everything is already on `new_ip`.
    /// If a re-bind on the new key fails, that port's routes are dropped and
    /// `on_change(port, false)` *does* fire — dead routes must not hold the
    /// gateway's port open; clients re-assert within their lease.
    pub fn rekey_ipv4(self: &Arc<Self>, new_ip: Ipv4Addr) {
        let moved: Vec<PortKey> = self.ports.mutate(|ports| {
            let old_keys: Vec<PortKey> = ports.keys().filter(|k| k.0 != new_ip).copied().collect();
            let mut moved = Vec::new();
            for old in old_keys {
                let Some(bindings) = ports.remove(&old) else {
                    continue;
                };
                let entry = ports.entry((new_ip, old.1)).or_default();
                for (name, b) in bindings.hostnames {
                    entry.hostnames.entry(name).or_insert(b);
                }
                if entry.fallback.is_none() {
                    entry.fallback = bindings.fallback;
                }
                moved.push(old);
            }
            moved
        });
        for old in &moved {
            if let Some(handle) = self.listeners.mutate(|l| l.remove(old)) {
                drop(handle); // aborts the stranded listener; no on_change
            }
        }
        for old in moved {
            let key = (new_ip, old.1);
            if let Err(e) = self.ensure_listener(key) {
                tracing::error!(
                    "SNI demux re-key bind on {}:{} failed; dropping the port's routes: {e}",
                    key.0,
                    key.1
                );
                self.ports.mutate(|ports| {
                    ports.remove(&key);
                });
                if let Some(cb) = &self.on_change {
                    cb(key.1, false);
                }
            }
        }
    }

    fn prune(&self) {
        let now = Instant::now();
        let empty: Vec<PortKey> = self.ports.mutate(|ports| {
            for entry in ports.values_mut() {
                entry.prune(now);
            }
            ports
                .iter()
                .filter(|(_, e)| e.is_empty())
                .map(|(k, _)| *k)
                .collect()
        });
        for key in empty {
            self.reap_if_empty(key);
        }
    }

    fn reap_if_empty(&self, key: PortKey) {
        let empty = self
            .ports
            .mutate(|ports| ports.get(&key).is_none_or(|e| e.is_empty()));
        if empty {
            self.ports.mutate(|ports| {
                ports.remove(&key);
            });
            if let Some(handle) = self.listeners.mutate(|l| l.remove(&key)) {
                drop(handle); // aborts the listener task
                if let Some(cb) = &self.on_change {
                    cb(key.1, false);
                }
            }
        }
    }

    /// Ensure a listener for `key`, binding inline so a failure is observable
    /// to the caller — never grant first and bind later, or traffic the
    /// gateway admits for the grant falls through to whatever wildcard socket
    /// shares the port (on StartWRT, the router's own web UI).
    /// `SO_REUSEPORT` lets this specific `(ext_ip, port)` socket coexist with a
    /// same-process wildcard listener on the same port: TCP delivery prefers
    /// the most-specific bound address, so the demux receives only traffic to
    /// its external IP and the wildcard keeps the rest.
    fn ensure_listener(self: &Arc<Self>, key: PortKey) -> std::io::Result<()> {
        let already = self.listeners.mutate(|l| l.contains_key(&key));
        if already {
            return Ok(());
        }
        let listener = crate::net::utils::bind_tokio_listener_reuse_port(
            SocketAddrV4::new(key.0, key.1).into(),
        )?;
        let ports = self.ports.clone();
        let handle = NonDetachingJoinHandle::from(tokio::spawn(run_listener(listener, key, ports)));
        self.listeners.mutate(|l| {
            l.insert(key, handle);
        });
        if let Some(cb) = &self.on_change {
            cb(key.1, true);
        }
        Ok(())
    }
}

async fn run_listener(
    listener: tokio::net::TcpListener,
    key: PortKey,
    ports: Arc<SyncMutex<BTreeMap<PortKey, PortBindings>>>,
) {
    if let Err(e) = crate::net::transparent::ensure_divert_infra_once().await {
        tracing::warn!(
            "SNI demux reply-path divert setup failed (source preservation may be degraded): {e}"
        );
    }
    tracing::info!("SNI demux listening on {}:{}", key.0, key.1);
    loop {
        match listener.accept().await {
            Ok((conn, peer)) => {
                let ports = ports.clone();
                tokio::spawn(async move {
                    handle_conn(conn, peer, key, ports).await;
                });
            }
            // Transient (EMFILE, ECONNABORTED): never tear down the listener.
            Err(e) => {
                tracing::warn!("SNI demux accept on {}:{}: {e}", key.0, key.1);
                tokio::time::sleep(ACCEPT_RETRY_DELAY).await;
            }
        }
    }
}

async fn handle_conn(
    mut conn: TcpStream,
    peer: SocketAddr,
    key: PortKey,
    ports: Arc<SyncMutex<BTreeMap<PortKey, PortBindings>>>,
) {
    // Reap silently-vanished peers, else copy_bidirectional pins the fd pair forever.
    if let Err(e) =
        socket2::SockRef::from(&conn).set_tcp_keepalive(&crate::net::utils::default_keepalive())
    {
        tracing::error!("Failed to set tcp keepalive: {e}");
    }
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let sni = loop {
        match timeout(CLIENTHELLO_TIMEOUT, conn.read(&mut tmp)).await {
            Ok(Ok(0)) => break extract_sni(&buf),
            Ok(Ok(n)) => {
                buf.extend_from_slice(&tmp[..n]);
                if let Some(name) = extract_sni(&buf) {
                    break Some(name);
                }
                // Complete-but-SNI-less, non-TLS, or capped: stop and use fallback.
                if record_complete(&buf) || buf.len() >= CLIENTHELLO_CAP {
                    break extract_sni(&buf);
                }
            }
            _ => break extract_sni(&buf),
        }
    };

    let SocketAddr::V4(peer) = peer else {
        return; // IPv4-only listener; should not occur
    };
    let selected = ports.peek(|p| {
        p.get(&key)
            .and_then(|e| e.select(sni.as_deref(), *peer.ip()))
    });
    let Some((target, transparent)) = selected else {
        return; // no match and no admissible fallback: close
    };
    let mut upstream = if transparent {
        // Open the internal leg from the client's own source address (RFC
        // §4.6). No plain-connect fallback on failure: the backend gates
        // LAN-only addresses on the source being private, and this server's
        // own wg address is private — a fallback would present every WAN
        // client as LAN-local.
        match crate::net::transparent::transparent_connect(
            SocketAddr::V4(peer),
            SocketAddr::V4(target),
        )
        .await
        {
            Ok(upstream) => upstream,
            Err(e) => {
                tracing::warn!("SNI demux transparent egress to {target} for {peer} failed: {e}");
                return;
            }
        }
    } else {
        // Gateway-self fallback: plain connect to our own listener.
        match TcpStream::connect(SocketAddr::V4(target)).await {
            Ok(upstream) => upstream,
            Err(e) => {
                tracing::warn!("SNI demux local egress to {target} for {peer} failed: {e}");
                return;
            }
        }
    };
    if upstream.write_all(&buf).await.is_err() {
        return;
    }
    let _ = copy_bidirectional(&mut conn, &mut upstream).await;
}

/// Whether `buf` holds at least one complete TLS handshake record.
fn record_complete(buf: &[u8]) -> bool {
    buf.len() >= 5 && buf.len() >= 5 + u16::from_be_bytes([buf[3], buf[4]]) as usize
}

/// Extract the (lowercased) SNI host_name from a buffered TLS ClientHello via
/// rustls, or `None` if absent / not yet complete / not TLS. The ClientHello is
/// only parsed, never answered — `buf` is still forwarded verbatim to the peer.
fn extract_sni(buf: &[u8]) -> Option<String> {
    let mut acceptor = tokio_rustls::rustls::server::Acceptor::default();
    let mut cursor = std::io::Cursor::new(buf);
    while let Ok(n) = acceptor.read_tls(&mut cursor) {
        if n == 0 {
            break;
        }
    }
    match acceptor.accept() {
        Ok(Some(accepted)) => accepted
            .client_hello()
            .server_name()
            .map(|s| s.to_ascii_lowercase()),
        _ => None,
    }
}

impl Default for SniDemux {
    fn default() -> Self {
        Self {
            ports: Arc::new(SyncMutex::new(BTreeMap::new())),
            listeners: SyncMutex::new(BTreeMap::new()),
            on_change: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real ClientHello produced by rustls, carrying `sni` in the SNI
    /// extension — so the parser is exercised against genuine wire bytes.
    fn real_client_hello(sni: &str) -> Vec<u8> {
        use tokio_rustls::rustls::pki_types::ServerName;
        use tokio_rustls::rustls::{ClientConfig, ClientConnection, RootCertStore};

        let provider = std::sync::Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
        let config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(RootCertStore::empty())
            .with_no_client_auth();
        let name = ServerName::try_from(sni.to_owned()).unwrap();
        let mut conn = ClientConnection::new(std::sync::Arc::new(config), name).unwrap();
        let mut buf = Vec::new();
        while conn.wants_write() {
            conn.write_tls(&mut buf).unwrap();
        }
        buf
    }

    #[test]
    fn parses_sni() {
        let hello = real_client_hello("git.example.com");
        assert_eq!(extract_sni(&hello).as_deref(), Some("git.example.com"));
    }

    #[test]
    fn non_tls_is_none() {
        assert_eq!(extract_sni(b"GET / HTTP/1.1\r\n"), None);
    }

    #[tokio::test]
    async fn fallback_register_ownership_and_coexistence() {
        let demux = SniDemux::new();
        let ip: Ipv4Addr = Ipv4Addr::LOCALHOST;
        let port = 44300u16;
        let fb = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 9), 443);
        let host_target = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 443);

        // A fallback can be set; a different target can't steal it, same reclaims.
        demux.register_fallback(ip, port, fb).unwrap();
        assert!(
            demux
                .register_fallback(ip, port, SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 8), 443))
                .is_err()
        );
        assert!(demux.register_fallback(ip, port, fb).is_ok());

        // A named route coexists with the fallback: exact SNI hits the route,
        // no/unmatched SNI hits the fallback.
        let anywhere = Ipv4Addr::new(203, 0, 113, 50);
        demux
            .register(ip, port, &["a.example.com".to_string()], host_target, None)
            .unwrap();
        demux.ports.peek(|p| {
            let pb = p.get(&(ip, port)).unwrap();
            assert_eq!(
                pb.select(Some("a.example.com"), anywhere),
                Some((host_target, true))
            );
            assert_eq!(
                pb.select(Some("nope.example.com"), anywhere),
                Some((fb, true))
            );
            assert_eq!(pb.select(None, anywhere), Some((fb, true)));
        });

        // Unregister with the wrong target is a no-op; the right target clears it,
        // leaving the named route intact.
        demux.unregister_fallback(ip, port, SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 8), 443));
        demux.ports.peek(|p| {
            assert_eq!(
                p.get(&(ip, port)).unwrap().fallback.map(|f| f.target),
                Some(fb)
            );
        });
        demux.unregister_fallback(ip, port, fb);
        demux.ports.peek(|p| {
            let pb = p.get(&(ip, port)).unwrap();
            assert_eq!(pb.fallback, None);
            assert_eq!(pb.select(None, anywhere), None);
            assert_eq!(
                pb.select(Some("a.example.com"), anywhere),
                Some((host_target, true))
            );
        });
    }

    // A local (gateway-self) fallback: plain-connect leg, and PrivateOnly
    // scoping admits RFC1918 sources while public sources fall through to a
    // close — never to the gateway's own listener.
    #[tokio::test]
    async fn local_fallback_source_policy() {
        let demux = SniDemux::new();
        let ip = Ipv4Addr::LOCALHOST;
        let port = 44320u16;
        let ui = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 443);
        let host_target = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 443);
        demux
            .register_local_fallback(ip, port, ui, FallbackSource::PrivateOnly)
            .unwrap();
        demux
            .register(ip, port, &["a.example.com".to_string()], host_target, None)
            .unwrap();
        let private = Ipv4Addr::new(192, 168, 1, 2);
        let public = Ipv4Addr::new(203, 0, 113, 50);
        demux.ports.peek(|p| {
            let pb = p.get(&(ip, port)).unwrap();
            // Hostname routes are never source-scoped.
            assert_eq!(
                pb.select(Some("a.example.com"), public),
                Some((host_target, true))
            );
            // The local fallback is plain-connect and private-only.
            assert_eq!(pb.select(None, private), Some((ui, false)));
            assert_eq!(pb.select(None, public), None);
        });
        // Re-registering with a new policy updates in place (mode switch).
        demux
            .register_local_fallback(ip, port, ui, FallbackSource::Any)
            .unwrap();
        demux.ports.peek(|p| {
            let pb = p.get(&(ip, port)).unwrap();
            assert_eq!(pb.select(None, public), Some((ui, false)));
        });
    }

    // A grant must never outrun its socket: with the port held by a non-
    // SO_REUSEPORT listener the bind fails, the register is refused with
    // NO_RESOURCES, nothing is recorded, and on_change never fires — so a
    // gateway never opens firewall access for a route with no listener.
    #[tokio::test]
    async fn bind_failure_refuses_grant_and_rolls_back() {
        let events = Arc::new(SyncMutex::new(Vec::<(u16, bool)>::new()));
        let recorded = events.clone();
        let demux = SniDemux::with_on_change(move |port, active| {
            recorded.mutate(|e| e.push((port, active)))
        });
        // Plain bind (no SO_REUSEPORT) — the demux's reuseport bind cannot join.
        let blocker = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = blocker.local_addr().unwrap().port();
        let target = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 443);

        let err = demux
            .register(
                Ipv4Addr::LOCALHOST,
                port,
                &["a.example.com".to_string()],
                target,
                Some(3600),
            )
            .unwrap_err();
        assert_eq!(err, RESULT_NO_RESOURCES);
        assert!(demux.snapshot().is_empty(), "rolled back on bind failure");
        demux.listeners.peek(|l| assert!(l.is_empty()));
        assert!(events.peek(|e| e.is_empty()), "on_change must not fire");
        assert_eq!(
            demux
                .register_fallback(Ipv4Addr::LOCALHOST, port, target)
                .unwrap_err(),
            RESULT_NO_RESOURCES
        );
        demux
            .ports
            .peek(|p| assert!(!p.contains_key(&(Ipv4Addr::LOCALHOST, port))));

        // Blocker gone: the same register now succeeds and on_change fires.
        drop(blocker);
        demux
            .register(
                Ipv4Addr::LOCALHOST,
                port,
                &["a.example.com".to_string()],
                target,
                Some(3600),
            )
            .unwrap();
        assert_eq!(demux.snapshot().len(), 1);
        assert_eq!(events.peek(|e| e.clone()), vec![(port, true)]);
    }

    // The coexistence the SO_REUSEPORT bind exists for: a same-process
    // wildcard listener (StartWRT's web UI) shares the port with the demux's
    // specific bind.
    #[tokio::test]
    async fn reuseport_bind_coexists_with_wildcard_listener() {
        let wildcard =
            crate::net::utils::bind_tokio_listener_reuse_port((Ipv4Addr::UNSPECIFIED, 0).into())
                .unwrap();
        let port = wildcard.local_addr().unwrap().port();
        let demux = SniDemux::new();
        demux
            .register(
                Ipv4Addr::LOCALHOST,
                port,
                &["a.example.com".to_string()],
                SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 443),
                Some(3600),
            )
            .unwrap();
        demux
            .listeners
            .peek(|l| assert!(l.contains_key(&(Ipv4Addr::LOCALHOST, port))));
    }

    #[test]
    fn select_exact_wildcard_fallback() {
        let mut pb = PortBindings::default();
        let exp = Instant::now() + Duration::from_secs(60);
        let peer = Ipv4Addr::new(203, 0, 113, 50);
        let mk = |o: u8| Binding {
            target: SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, o), 443),
            expiry: Some(exp),
        };
        pb.hostnames.insert("a.example.com".into(), mk(1));
        pb.hostnames.insert("*.example.com".into(), mk(2));
        pb.fallback = Some(Fallback {
            target: SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 9), 443),
            source: FallbackSource::Any,
            transparent: true,
        });
        let sel = |sni| pb.select(sni, peer).unwrap().0;
        assert_eq!(sel(Some("a.example.com")).ip().octets()[3], 1);
        assert_eq!(sel(Some("b.example.com")).ip().octets()[3], 2);
        assert_eq!(sel(Some("other.org")).ip().octets()[3], 9);
        assert_eq!(sel(None).ip().octets()[3], 9);
    }

    #[tokio::test]
    async fn snapshot_reports_live_routes_with_remaining() {
        let demux = SniDemux::new();
        let ip = Ipv4Addr::LOCALHOST;
        let t1 = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 443);
        let t2 = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 2), 443);
        demux
            .register(ip, 44311, &["a.example.com".to_string()], t1, Some(3600))
            .unwrap();
        demux
            .register(ip, 44311, &["b.example.com".to_string()], t2, None)
            .unwrap();
        // Fallbacks are port-level, not hostname routes: not reported.
        demux.register_fallback(ip, 44312, t1).unwrap();

        let mut snap = demux.snapshot();
        snap.sort_by(|a, b| a.hostname.cmp(&b.hostname));
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].hostname, "a.example.com");
        assert_eq!(snap[0].target, t1);
        assert_eq!((snap[0].ext_ip, snap[0].ext_port), (ip, 44311));
        let remaining = snap[0].remaining_secs.unwrap();
        assert!(remaining > 3590 && remaining <= 3600, "got {remaining}");
        assert_eq!(snap[1].remaining_secs, None);
    }

    #[tokio::test]
    async fn rekey_moves_bindings_and_never_fires_teardown() {
        let events = Arc::new(SyncMutex::new(Vec::<(u16, bool)>::new()));
        let recorded = events.clone();
        let demux = SniDemux::with_on_change(move |port, active| {
            recorded.mutate(|e| e.push((port, active)))
        });
        let old_ip = Ipv4Addr::new(203, 0, 113, 1);
        let new_ip = Ipv4Addr::new(203, 0, 113, 2);
        let t1 = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 443);
        let t2 = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 2), 443);

        demux
            .register(
                old_ip,
                44313,
                &["a.example.com".to_string()],
                t1,
                Some(3600),
            )
            .unwrap();
        demux.register_fallback(old_ip, 44313, t1).unwrap();
        // A binding already on the new key: survives the merge and wins any
        // hostname collision.
        demux
            .register(new_ip, 44313, &["a.example.com".to_string()], t2, None)
            .unwrap();
        demux
            .register(new_ip, 44313, &["c.example.com".to_string()], t2, None)
            .unwrap();

        demux.rekey_ipv4(new_ip);

        demux.ports.peek(|p| {
            assert!(p.get(&(old_ip, 44313)).is_none(), "old key drained");
            let pb = p.get(&(new_ip, 44313)).unwrap();
            assert_eq!(
                pb.hostnames.get("a.example.com").unwrap().target,
                t2,
                "existing binding at the new key wins the collision"
            );
            assert_eq!(pb.hostnames.get("c.example.com").unwrap().target, t2);
            assert_eq!(
                pb.fallback.map(|f| f.target),
                Some(t1),
                "moved fallback fills the empty slot"
            );
        });
        demux.listeners.peek(|l| {
            assert!(l.contains_key(&(new_ip, 44313)) && !l.contains_key(&(old_ip, 44313)))
        });
        assert!(
            events.peek(|e| e.iter().all(|&(_, active)| active)),
            "rekey must never fire on_change(port, false)"
        );

        // Already keyed to new_ip: a second rekey is a no-op (no new events).
        let before = events.peek(|e| e.len());
        demux.rekey_ipv4(new_ip);
        assert_eq!(events.peek(|e| e.len()), before);
    }
}
