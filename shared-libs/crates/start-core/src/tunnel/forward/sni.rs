//! SNI demultiplexing for hostname-based shared-port mappings.
//!
//! Exact names take precedence over leading-label wildcards. An optional
//! fallback receives unmatched SNI, no-SNI TLS, and non-TLS traffic.

use std::collections::{BTreeMap, BTreeSet};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use ipnet::Ipv4Net;
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct Binding {
    target: SocketAddrV4,
    /// `None` for a permanent (DB-backed/manual) binding that never expires.
    expiry: Option<Instant>,
}

/// Sources permitted to use a port fallback.
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
    /// Whether upstream connections preserve the client source address.
    transparent: bool,
}

#[derive(Default)]
struct PortBindings {
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
    /// Selects exact, wildcard, then admissible fallback targets.
    /// Returns the target and source-preservation policy.
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

/// Resolves a target's local subnet prefix for hairpin detection.
/// `None` preserves the client source address.
pub type LocalPrefix = Arc<dyn Fn(Ipv4Addr) -> BoxFuture<'static, Option<u8>> + Send + Sync>;

/// A registration that can restore the exact prior bindings.
pub struct SniRegistration {
    key: PortKey,
    previous: Vec<(String, Option<Binding>)>,
    applied: Binding,
}

pub struct SniDemux {
    ports: Arc<SyncMutex<BTreeMap<PortKey, PortBindings>>>,
    listeners: SyncMutex<BTreeMap<PortKey, NonDetachingJoinHandle<()>>>,
    on_change: Option<OnChange>,
    local_prefix: Option<LocalPrefix>,
}

impl SniDemux {
    pub fn new() -> Arc<Self> {
        Self::build(None, None)
    }

    /// Installs listener lifecycle and local-subnet hooks.
    pub fn with_on_change(
        on_change: impl Fn(u16, bool) + Send + Sync + 'static,
        local_prefix: Option<LocalPrefix>,
    ) -> Arc<Self> {
        Self::build(Some(Box::new(on_change)), local_prefix)
    }

    fn build(on_change: Option<OnChange>, local_prefix: Option<LocalPrefix>) -> Arc<Self> {
        let this = Arc::new(Self {
            ports: Arc::new(SyncMutex::new(BTreeMap::new())),
            listeners: SyncMutex::new(BTreeMap::new()),
            on_change,
            local_prefix,
        });
        let weak = Arc::downgrade(&this);
        tokio::spawn(async move {
            let mut divert_ok = true;
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let Some(this) = weak.upgrade() else { break };
                this.prune();
                // Repair external divert-rule flushes while listeners are active.
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

    /// Installs reply-path diversion before a route is granted.
    pub async fn prepare(&self) -> Result<(), u8> {
        #[cfg(test)]
        return Ok(());
        #[cfg(not(test))]
        crate::net::transparent::ensure_divert_infra_once()
            .await
            .map_err(|e| {
                tracing::warn!("SNI demux reply-path diversion failed: {e}");
                RESULT_NO_RESOURCES
            })
    }

    /// Registers all hostnames atomically and starts their shared listener.
    /// Returns `RESULT_HOSTNAME_TAKEN` for an occupied name or
    /// `RESULT_NO_RESOURCES` when the listener cannot bind.
    pub fn register(
        self: &Arc<Self>,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        hostnames: &[String],
        target: SocketAddrV4,
        lifetime_secs: Option<u32>,
    ) -> Result<(), u8> {
        self.register_transaction(ext_ip, ext_port, hostnames, target, lifetime_secs)
            .map(|_| ())
    }

    /// Registers hostnames and returns their prior state.
    pub fn register_transaction(
        self: &Arc<Self>,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        hostnames: &[String],
        target: SocketAddrV4,
        lifetime_secs: Option<u32>,
    ) -> Result<SniRegistration, u8> {
        let now = Instant::now();
        let applied = Binding {
            target,
            expiry: lifetime_secs.map(|s| now + Duration::from_secs(s as u64)),
        };
        let key = (ext_ip, ext_port);
        let previous = self.ports.mutate(|ports| {
            let entry = ports.entry(key).or_default();
            entry.prune(now);
            for name in hostnames {
                if let Some(b) = entry.hostnames.get(name) {
                    if b.target != target {
                        return Err(RESULT_HOSTNAME_TAKEN);
                    }
                }
            }
            let previous = hostnames
                .iter()
                .map(|name| (name.clone(), entry.hostnames.get(name).cloned()))
                .collect::<Vec<_>>();
            for name in hostnames {
                entry.hostnames.insert(name.clone(), applied.clone());
            }
            Ok(previous)
        })?;
        let registration = SniRegistration {
            key,
            previous,
            applied,
        };
        if let Err(e) = self.ensure_listener(key) {
            tracing::warn!(
                "SNI demux bind on {}:{} failed; refusing the grant: {e}",
                key.0,
                key.1
            );
            self.rollback(registration);
            return Err(RESULT_NO_RESOURCES);
        }
        Ok(registration)
    }

    /// Restores bindings unchanged since the registration.
    pub fn rollback(&self, registration: SniRegistration) {
        self.ports.mutate(|ports| {
            let Some(entry) = ports.get_mut(&registration.key) else {
                return;
            };
            for (name, previous) in registration.previous {
                if entry.hostnames.get(&name) != Some(&registration.applied) {
                    continue;
                }
                match previous {
                    Some(binding) => {
                        entry.hostnames.insert(name, binding);
                    }
                    None => {
                        entry.hostnames.remove(&name);
                    }
                }
            }
        });
        self.reap_if_empty(registration.key);
    }

    /// Removes bindings held by the target.
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

    /// Registers a source-preserving fallback for unmatched traffic.
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

    /// Registers a source-scoped fallback to a gateway-local listener.
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
        let previous = self.ports.mutate(|ports| {
            let entry = ports.entry(key).or_default();
            if entry.fallback.is_some_and(|f| f.target != fallback.target) {
                return Err(RESULT_HOSTNAME_TAKEN);
            }
            let previous = entry.fallback;
            entry.fallback = Some(fallback);
            Ok(previous)
        })?;
        if let Err(e) = self.ensure_listener(key) {
            tracing::warn!(
                "SNI demux bind on {}:{} failed; refusing the fallback: {e}",
                key.0,
                key.1
            );
            self.ports.mutate(|ports| {
                if let Some(entry) = ports.get_mut(&key) {
                    entry.fallback = previous;
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

    /// Live hostname routes, excluding port-level fallbacks.
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

    /// Move all bindings to `new_ip`. Existing destination bindings win
    /// collisions. A failed destination listener drops its merged bindings.
    pub fn rekey_ipv4(self: &Arc<Self>, new_ip: Ipv4Addr) {
        let (moved, destinations): (Vec<PortKey>, BTreeSet<PortKey>) = self.ports.mutate(|ports| {
            let old_keys: Vec<PortKey> = ports
                .keys()
                .filter(|key| key.0 != new_ip)
                .copied()
                .collect();
            let mut moved = Vec::new();
            let mut destinations = BTreeSet::new();
            for old in old_keys {
                let Some(bindings) = ports.remove(&old) else {
                    continue;
                };
                let destination = (new_ip, old.1);
                let entry = ports.entry(destination).or_default();
                for (name, binding) in bindings.hostnames {
                    entry.hostnames.entry(name).or_insert(binding);
                }
                if entry.fallback.is_none() {
                    entry.fallback = bindings.fallback;
                }
                moved.push(old);
                destinations.insert(destination);
            }
            (moved, destinations)
        });
        for old in moved {
            if let Some(handle) = self.listeners.mutate(|listeners| listeners.remove(&old)) {
                drop(handle);
            }
        }
        for key in destinations {
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

    /// Starts the address-specific listener before callers grant a route.
    fn ensure_listener(self: &Arc<Self>, key: PortKey) -> std::io::Result<()> {
        let already = self
            .listeners
            .mutate(|listeners| listeners.contains_key(&key));
        if already {
            return Ok(());
        }
        if self
            .ports
            .peek(|ports| ports.get(&key).is_none_or(PortBindings::is_empty))
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no bindings for SNI listener",
            ));
        }
        let listener = crate::net::utils::bind_tokio_listener_reuse_port(
            SocketAddrV4::new(key.0, key.1).into(),
        )?;
        let ports = self.ports.clone();
        let handle = NonDetachingJoinHandle::from(tokio::spawn(run_listener(
            listener,
            key,
            ports,
            self.local_prefix.clone(),
        )));
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
    local_prefix: Option<LocalPrefix>,
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
                let local_prefix = local_prefix.clone();
                tokio::spawn(async move {
                    handle_conn(conn, peer, key, ports, local_prefix).await;
                });
            }
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
    local_prefix: Option<LocalPrefix>,
) {
    // Bound abandoned connections after peer disappearance.
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
                if record_complete(&buf) || buf.len() >= CLIENTHELLO_CAP {
                    break extract_sni(&buf);
                }
            }
            _ => break extract_sni(&buf),
        }
    };

    let SocketAddr::V4(peer) = peer else {
        return;
    };
    let selected = ports.peek(|p| {
        p.get(&key)
            .and_then(|e| e.select(sni.as_deref(), *peer.ip()))
    });
    let Some((target, transparent)) = selected else {
        return;
    };
    // Same-subnet clients need the gateway source address for return traffic.
    let transparent = transparent && !is_hairpin(&local_prefix, *peer.ip(), *target.ip()).await;
    let mut upstream = if transparent {
        // A failed source-preserving connection must not fall back to gateway source.
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

/// Whether the peer and target share a known local subnet.
async fn is_hairpin(local_prefix: &Option<LocalPrefix>, peer: Ipv4Addr, target: Ipv4Addr) -> bool {
    let Some(resolve) = local_prefix else {
        return false;
    };
    let Some(prefix) = resolve(target).await else {
        return false;
    };
    Ipv4Net::new(target, prefix).is_ok_and(|net| net.contains(&peer))
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
            local_prefix: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rustls-generated ClientHello carrying SNI.
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

        demux.register_fallback(ip, port, fb).unwrap();
        assert!(
            demux
                .register_fallback(ip, port, SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 8), 443))
                .is_err()
        );
        assert!(demux.register_fallback(ip, port, fb).is_ok());

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
            assert_eq!(
                pb.select(Some("a.example.com"), public),
                Some((host_target, true))
            );
            assert_eq!(pb.select(None, private), Some((ui, false)));
            assert_eq!(pb.select(None, public), None);
        });
        demux
            .register_local_fallback(ip, port, ui, FallbackSource::Any)
            .unwrap();
        demux.ports.peek(|p| {
            let pb = p.get(&(ip, port)).unwrap();
            assert_eq!(pb.select(None, public), Some((ui, false)));
        });
    }

    #[tokio::test]
    async fn bind_failure_refuses_grant_and_rolls_back() {
        let events = Arc::new(SyncMutex::new(Vec::<(u16, bool)>::new()));
        let recorded = events.clone();
        let demux = SniDemux::with_on_change(
            move |port, active| recorded.mutate(|e| e.push((port, active))),
            None,
        );
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

    #[tokio::test]
    async fn rollback_restores_a_renewed_binding() {
        let probe = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let demux = SniDemux::new();
        let hostname = "a.example.com".to_string();
        let target = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 443);
        demux
            .register(
                Ipv4Addr::LOCALHOST,
                port,
                std::slice::from_ref(&hostname),
                target,
                Some(30),
            )
            .unwrap();
        let before = demux
            .ports
            .peek(|ports| ports[&(Ipv4Addr::LOCALHOST, port)].hostnames[&hostname].clone());
        let registration = demux
            .register_transaction(
                Ipv4Addr::LOCALHOST,
                port,
                std::slice::from_ref(&hostname),
                target,
                Some(3600),
            )
            .unwrap();
        demux.rollback(registration);
        let after = demux
            .ports
            .peek(|ports| ports[&(Ipv4Addr::LOCALHOST, port)].hostnames[&hostname].clone());
        assert_eq!(after, before);
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
    async fn rekey_failure_does_not_create_an_empty_listener() {
        let blocker = Arc::new(SyncMutex::new(Some(
            std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap(),
        )));
        let port = blocker.peek(|listener| listener.as_ref().unwrap().local_addr().unwrap().port());
        let release_blocker = blocker.clone();
        let demux = SniDemux::with_on_change(
            move |_, active| {
                if !active {
                    release_blocker.mutate(Option::take);
                }
            },
            None,
        );
        let target = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 443);
        for old_ip in [Ipv4Addr::new(127, 0, 0, 2), Ipv4Addr::new(127, 0, 0, 3)] {
            demux
                .register(
                    old_ip,
                    port,
                    &[format!("{old_ip}.example.com")],
                    target,
                    None,
                )
                .unwrap();
        }

        demux.rekey_ipv4(Ipv4Addr::LOCALHOST);

        demux.ports.peek(|ports| assert!(ports.is_empty()));
        demux
            .listeners
            .peek(|listeners| assert!(listeners.is_empty()));
    }

    #[tokio::test]
    async fn rekey_moves_bindings_and_never_fires_teardown() {
        let events = Arc::new(SyncMutex::new(Vec::<(u16, bool)>::new()));
        let recorded = events.clone();
        let demux = SniDemux::with_on_change(
            move |port, active| recorded.mutate(|e| e.push((port, active))),
            None,
        );
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

        let before = events.peek(|e| e.len());
        demux.rekey_ipv4(new_ip);
        assert_eq!(events.peek(|e| e.len()), before);
    }

    #[tokio::test]
    async fn hairpin_is_scoped_to_the_targets_own_subnet() {
        let lan = Ipv4Net::new(Ipv4Addr::new(192, 168, 1, 0), 24).unwrap();
        let resolver: LocalPrefix = Arc::new(move |ip: Ipv4Addr| {
            Box::pin(async move { lan.contains(&ip).then_some(lan.prefix_len()) })
                as BoxFuture<'static, Option<u8>>
        });
        let host = Some(resolver);
        let target = Ipv4Addr::new(192, 168, 1, 10);

        assert!(is_hairpin(&host, Ipv4Addr::new(192, 168, 1, 50), target).await);
        assert!(!is_hairpin(&host, Ipv4Addr::new(192, 168, 9, 50), target).await);
        assert!(!is_hairpin(&host, Ipv4Addr::new(203, 0, 113, 50), target).await);
        assert!(
            !is_hairpin(
                &host,
                Ipv4Addr::new(10, 0, 0, 5),
                Ipv4Addr::new(10, 0, 0, 9)
            )
            .await
        );
        assert!(!is_hairpin(&None, Ipv4Addr::new(192, 168, 1, 50), target).await);
    }
}
