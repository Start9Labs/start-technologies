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
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

use crate::net::port_map::pcp::hostname::RESULT_HOSTNAME_TAKEN;
use crate::prelude::*;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::sync::SyncMutex;

/// (external IP, external port).
type PortKey = (Ipv4Addr, u16);

const CLIENTHELLO_CAP: usize = 16384;
const CLIENTHELLO_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
struct Binding {
    target: SocketAddrV4,
    /// `None` for a permanent (DB-backed/manual) binding that never expires.
    expiry: Option<Instant>,
}

#[derive(Default)]
struct PortBindings {
    /// hostname (lowercase) -> binding; a `*.suffix` key is a wildcard.
    hostnames: BTreeMap<String, Binding>,
    fallback: Option<SocketAddrV4>,
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
    fn select(&self, sni: Option<&str>) -> Option<SocketAddrV4> {
        if let Some(name) = sni {
            if let Some(b) = self.hostnames.get(name) {
                return Some(b.target);
            }
            if let Some((_, rest)) = name.split_once('.') {
                if let Some(b) = self.hostnames.get(&format!("*.{rest}")) {
                    return Some(b.target);
                }
            }
        }
        self.fallback
    }
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
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let Some(this) = weak.upgrade() else { break };
                this.prune();
            }
        });
        this
    }

    /// Register hostname bindings for `(ext_ip, ext_port) -> target` and ensure
    /// the listener runs. `Err(RESULT_HOSTNAME_TAKEN)` if any name is held by a
    /// different target — all-or-nothing; the same target reclaims.
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
        self.ensure_listener(key);
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

    fn ensure_listener(self: &Arc<Self>, key: PortKey) {
        let already = self.listeners.mutate(|l| l.contains_key(&key));
        if already {
            return;
        }
        let ports = self.ports.clone();
        let handle = NonDetachingJoinHandle::from(tokio::spawn(async move {
            if let Err(e) = run_listener(key, ports).await {
                tracing::warn!("SNI demux listener on {}:{} exited: {e}", key.0, key.1);
            }
        }));
        self.listeners.mutate(|l| {
            l.insert(key, handle);
        });
        if let Some(cb) = &self.on_change {
            cb(key.1, true);
        }
    }
}

async fn run_listener(
    key: PortKey,
    ports: Arc<SyncMutex<BTreeMap<PortKey, PortBindings>>>,
) -> Result<(), Error> {
    if let Err(e) = crate::net::transparent::ensure_divert_infra().await {
        tracing::warn!(
            "SNI demux reply-path divert setup failed (source preservation may be degraded): {e}"
        );
    }
    let listener = TcpListener::bind(SocketAddrV4::new(key.0, key.1))
        .await
        .with_kind(ErrorKind::Network)?;
    tracing::info!("SNI demux listening on {}:{}", key.0, key.1);
    loop {
        let (conn, peer) = listener.accept().await.with_kind(ErrorKind::Network)?;
        let ports = ports.clone();
        tokio::spawn(async move {
            handle_conn(conn, peer, key, ports).await;
        });
    }
}

async fn handle_conn(
    mut conn: TcpStream,
    peer: SocketAddr,
    key: PortKey,
    ports: Arc<SyncMutex<BTreeMap<PortKey, PortBindings>>>,
) {
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

    let target = ports.peek(|p| p.get(&key).and_then(|e| e.select(sni.as_deref())));
    let Some(target) = target else {
        return; // no match and no fallback: close
    };
    let SocketAddr::V4(peer) = peer else {
        return; // IPv4-only listener; should not occur
    };
    // Open the internal leg from the client's own source address (RFC §4.6).
    let Ok(mut upstream) = crate::net::transparent::transparent_connect(peer, target).await else {
        return;
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

    #[test]
    fn select_exact_wildcard_fallback() {
        let mut pb = PortBindings::default();
        let exp = Instant::now() + Duration::from_secs(60);
        let mk = |o: u8| Binding {
            target: SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, o), 443),
            expiry: Some(exp),
        };
        pb.hostnames.insert("a.example.com".into(), mk(1));
        pb.hostnames.insert("*.example.com".into(), mk(2));
        pb.fallback = Some(SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 9), 443));
        assert_eq!(
            pb.select(Some("a.example.com")).unwrap().ip().octets()[3],
            1
        );
        assert_eq!(
            pb.select(Some("b.example.com")).unwrap().ip().octets()[3],
            2
        );
        assert_eq!(pb.select(Some("other.org")).unwrap().ip().octets()[3], 9);
        assert_eq!(pb.select(None).unwrap().ip().octets()[3], 9);
    }
}
