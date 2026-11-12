//! SNI demultiplexing for hostname-based shared-port mappings.
//!
//! Exact names take precedence over leading-label wildcards. An optional
//! fallback receives unmatched SNI, no-SNI TLS, and non-TLS traffic.

mod quic;
mod udp;

use std::collections::{BTreeMap, BTreeSet};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use ipnet::Ipv4Net;
use tokio::io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional};
use tokio::net::TcpStream;
#[cfg(test)]
use tokio::time::timeout;

use crate::net::port_map::pcp::RESULT_NO_RESOURCES;
use crate::net::port_map::pcp::hostname::RESULT_HOSTNAME_TAKEN;
use crate::net::port_map::{Protocol, TransportProtocol};
use crate::util::future::NonDetachingJoinHandle;
use crate::util::sync::SyncMutex;

/// (external IP, external port, transport).
type PortKey = (Ipv4Addr, u16, TransportProtocol);

enum PreparedListener {
    Tcp(tokio::net::TcpListener),
    Udp(tokio::net::UdpSocket),
}

const CLIENTHELLO_CAP: usize = 16384;
const CLIENTHELLO_TIMEOUT: Duration = Duration::from_secs(5);
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, PartialEq, Eq)]
enum HelloStatus {
    Pending,
    Complete(Option<String>),
    Invalid,
}

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
    #[cfg(test)]
    fn select(&self, sni: Option<&str>, peer: Ipv4Addr) -> Option<(SocketAddrV4, bool)> {
        self.select_with_fallback(sni, peer, None)
    }

    fn select_with_fallback(
        &self,
        sni: Option<&str>,
        peer: Ipv4Addr,
        local_fallback: Option<Fallback>,
    ) -> Option<(SocketAddrV4, bool)> {
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
        let f = local_fallback.or(self.fallback)?;
        match f.source {
            FallbackSource::Any => {}
            FallbackSource::PrivateOnly if peer.is_private() => {}
            FallbackSource::PrivateOnly => return None,
        }
        Some((f.target, f.transparent))
    }
}

fn inherited_local_fallback(
    ports: &BTreeMap<PortKey, PortBindings>,
    key: PortKey,
) -> Option<Fallback> {
    ports
        .get(&(Ipv4Addr::UNSPECIFIED, key.1, key.2))
        .and_then(|e| e.fallback)
        .filter(|f| !f.transparent)
}

/// One live hostname route, as reported by [`SniDemux::snapshot`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SniRoute {
    pub ext_ip: Ipv4Addr,
    pub ext_port: u16,
    pub protocol: TransportProtocol,
    pub hostname: String,
    pub target: SocketAddrV4,
    /// Seconds until the binding expires; `None` for a permanent binding.
    pub remaining_secs: Option<u64>,
}

/// Reports listener admission changes independently for each transport.
type OnChange = Box<dyn Fn(u16, TransportProtocol, bool) + Send + Sync>;

/// Resolves a target's local subnet prefix for hairpin detection.
pub type LocalPrefix = Arc<dyn Fn(Ipv4Addr) -> BoxFuture<'static, Option<u8>> + Send + Sync>;

/// A registration that can restore the exact prior bindings.
pub struct SniRegistration {
    previous: Vec<(PortKey, Vec<(String, Option<Binding>)>)>,
    applied: Binding,
}

pub struct SniDemux {
    ports: Arc<SyncMutex<BTreeMap<PortKey, PortBindings>>>,
    // Lock `ports` before `listeners` when acquiring both.
    listeners: SyncMutex<BTreeMap<PortKey, NonDetachingJoinHandle<()>>>,
    on_change: Option<OnChange>,
    local_prefix: Option<LocalPrefix>,
    udp_limits: Arc<udp::Limits>,
}

impl SniDemux {
    pub fn new() -> Arc<Self> {
        Self::build(None, None)
    }

    /// Installs listener lifecycle and local-subnet hooks.
    pub fn with_on_change(
        on_change: impl Fn(u16, TransportProtocol, bool) + Send + Sync + 'static,
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
            udp_limits: Arc::new(udp::Limits::default()),
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

    /// Registers TCP hostnames atomically and starts their shared listener.
    /// Returns `RESULT_HOSTNAME_TAKEN` for a name another device holds or
    /// `RESULT_NO_RESOURCES` when the listener cannot bind.
    pub fn register(
        self: &Arc<Self>,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        hostnames: &[String],
        target: SocketAddrV4,
        lifetime_secs: Option<u32>,
    ) -> Result<(), u8> {
        if hostnames.is_empty() {
            return Ok(());
        }
        self.register_transport(
            TransportProtocol::Tcp,
            ext_ip,
            ext_port,
            hostnames,
            target,
            lifetime_secs,
        )
    }

    /// TCP and UDP bindings have independent ownership, leases, and listeners.
    pub fn register_transport(
        self: &Arc<Self>,
        protocol: TransportProtocol,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        hostnames: &[String],
        target: SocketAddrV4,
        lifetime_secs: Option<u32>,
    ) -> Result<(), u8> {
        if hostnames.is_empty() {
            return Ok(());
        }
        self.register_transaction_transport(
            protocol,
            ext_ip,
            ext_port,
            hostnames,
            target,
            lifetime_secs,
        )
        .map(|_| ())
    }

    /// Registers TCP hostnames and returns their prior state.
    pub fn register_transaction(
        self: &Arc<Self>,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        hostnames: &[String],
        target: SocketAddrV4,
        lifetime_secs: Option<u32>,
    ) -> Result<SniRegistration, u8> {
        self.register_transaction_transport(
            TransportProtocol::Tcp,
            ext_ip,
            ext_port,
            hostnames,
            target,
            lifetime_secs,
        )
    }

    /// Registers one transport atomically and returns its prior state.
    pub fn register_transaction_transport(
        self: &Arc<Self>,
        protocol: TransportProtocol,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        hostnames: &[String],
        target: SocketAddrV4,
        lifetime_secs: Option<u32>,
    ) -> Result<SniRegistration, u8> {
        self.register_transaction_protocol(
            protocol.into(),
            ext_ip,
            ext_port,
            hostnames,
            target,
            lifetime_secs,
        )
    }

    /// Publishes every selected transport together; a failed bind changes none.
    pub fn register_protocol(
        self: &Arc<Self>,
        protocol: Protocol,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        hostnames: &[String],
        target: SocketAddrV4,
        lifetime_secs: Option<u32>,
    ) -> Result<(), u8> {
        self.register_transaction_protocol(
            protocol,
            ext_ip,
            ext_port,
            hostnames,
            target,
            lifetime_secs,
        )
        .map(|_| ())
    }

    /// Registers a logical mapping and retains each transport's prior bindings.
    pub fn register_transaction_protocol(
        self: &Arc<Self>,
        protocol: Protocol,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        hostnames: &[String],
        target: SocketAddrV4,
        lifetime_secs: Option<u32>,
    ) -> Result<SniRegistration, u8> {
        let hostnames = lowercased(hostnames);
        let now = Instant::now();
        let applied = Binding {
            target,
            expiry: lifetime_secs.map(|s| now + Duration::from_secs(s as u64)),
        };
        if hostnames.is_empty() {
            return Ok(SniRegistration {
                previous: Vec::new(),
                applied,
            });
        }
        let keys: Vec<_> = protocol
            .transports()
            .iter()
            .map(|&transport| (ext_ip, ext_port, transport))
            .collect();
        let (previous, opened) = self.ports.mutate(|ports| {
            for key in &keys {
                if let Some(entry) = ports.get(key) {
                    for name in &hostnames {
                        if entry.hostnames.get(name).is_some_and(|binding| {
                            binding.expiry.is_none_or(|expiry| expiry > now)
                                && binding.target.ip() != target.ip()
                        }) {
                            return Err(RESULT_HOSTNAME_TAKEN);
                        }
                    }
                }
            }
            self.listeners.mutate(|listeners| {
                let prepared = self.prepare_listeners(&keys, listeners).map_err(|error| {
                    tracing::warn!(
                        "SNI demux bind on {ext_ip}:{ext_port} failed; refusing the grant: {error}"
                    );
                    RESULT_NO_RESOURCES
                })?;
                let previous = keys
                    .iter()
                    .map(|&key| {
                        let entry = ports.entry(key).or_default();
                        entry.prune(now);
                        let previous = hostnames
                            .iter()
                            .map(|name| (name.clone(), entry.hostnames.get(name).cloned()))
                            .collect();
                        for name in &hostnames {
                            entry.hostnames.insert(name.clone(), applied.clone());
                        }
                        (key, previous)
                    })
                    .collect();
                let opened = self.start_listeners(listeners, prepared);
                Ok((previous, opened))
            })
        })?;
        self.notify_opened(&opened);
        Ok(SniRegistration { previous, applied })
    }

    /// Restores bindings unchanged since the registration.
    pub fn rollback(&self, registration: SniRegistration) {
        let keys = self.ports.mutate(|ports| {
            let mut keys = Vec::new();
            for (key, previous) in registration.previous {
                let Some(entry) = ports.get_mut(&key) else {
                    continue;
                };
                for (name, previous) in previous {
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
                keys.push(key);
            }
            keys
        });
        for key in keys {
            self.reap_if_empty(key);
        }
    }

    /// Removes bindings held by the target.
    pub fn unregister(
        &self,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        hostnames: &[String],
        target: SocketAddrV4,
    ) {
        self.unregister_transport(TransportProtocol::Tcp, ext_ip, ext_port, hostnames, target);
    }

    /// Removes hostname bindings owned by the target on one transport.
    pub fn unregister_transport(
        &self,
        protocol: TransportProtocol,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        hostnames: &[String],
        target: SocketAddrV4,
    ) {
        self.unregister_protocol(protocol.into(), ext_ip, ext_port, hostnames, target);
    }

    /// Removes a target-owned logical mapping from every selected transport.
    pub fn unregister_protocol(
        &self,
        protocol: Protocol,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        hostnames: &[String],
        target: SocketAddrV4,
    ) {
        let hostnames = lowercased(hostnames);
        let keys: Vec<_> = protocol
            .transports()
            .iter()
            .map(|&transport| (ext_ip, ext_port, transport))
            .collect();
        self.ports.mutate(|ports| {
            for key in &keys {
                if let Some(entry) = ports.get_mut(key) {
                    for name in &hostnames {
                        if entry
                            .hostnames
                            .get(name)
                            .is_some_and(|binding| binding.target == target)
                        {
                            entry.hostnames.remove(name);
                        }
                    }
                }
            }
        });
        for key in keys {
            self.reap_if_empty(key);
        }
    }

    /// Registers a source-preserving fallback for unmatched traffic.
    pub fn register_fallback(
        self: &Arc<Self>,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        target: SocketAddrV4,
    ) -> Result<(), u8> {
        self.register_fallback_transport(TransportProtocol::Tcp, ext_ip, ext_port, target)
    }

    /// Registers the unmatched-traffic fallback for one transport.
    pub fn register_fallback_transport(
        self: &Arc<Self>,
        protocol: TransportProtocol,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        target: SocketAddrV4,
    ) -> Result<(), u8> {
        self.register_fallback_with(
            protocol,
            ext_ip,
            ext_port,
            Fallback {
                target,
                source: FallbackSource::Any,
                transparent: true,
            },
        )
    }

    /// Publishes a logical catch-all on every selected transport together.
    pub fn register_fallback_protocol(
        self: &Arc<Self>,
        protocol: Protocol,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        target: SocketAddrV4,
    ) -> Result<(), u8> {
        self.register_fallback_protocol_with(
            protocol,
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
    /// An unspecified external address also supplies the fallback to address-specific listeners.
    pub fn register_local_fallback(
        self: &Arc<Self>,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        target: SocketAddrV4,
        source: FallbackSource,
    ) -> Result<(), u8> {
        self.register_fallback_with(
            TransportProtocol::Tcp,
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
        protocol: TransportProtocol,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        fallback: Fallback,
    ) -> Result<(), u8> {
        self.register_fallback_protocol_with(protocol.into(), ext_ip, ext_port, fallback)
    }

    fn register_fallback_protocol_with(
        self: &Arc<Self>,
        protocol: Protocol,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        fallback: Fallback,
    ) -> Result<(), u8> {
        let keys: Vec<_> = protocol
            .transports()
            .iter()
            .map(|&transport| (ext_ip, ext_port, transport))
            .collect();
        let opened = self.ports.mutate(|ports| {
            if keys.iter().any(|key| ports.get(key).and_then(|entry| entry.fallback)
                .is_some_and(|old| old.target.ip() != fallback.target.ip())) {
                return Err(RESULT_HOSTNAME_TAKEN);
            }
            self.listeners.mutate(|listeners| {
                let prepared = self.prepare_listeners(&keys, listeners).map_err(|error| {
                    tracing::warn!("SNI demux bind on {ext_ip}:{ext_port} failed; refusing the fallback: {error}");
                    RESULT_NO_RESOURCES
                })?;
                for &key in &keys {
                    ports.entry(key).or_default().fallback = Some(fallback);
                }
                Ok(self.start_listeners(listeners, prepared))
            })
        })?;
        self.notify_opened(&opened);
        Ok(())
    }

    /// Clear the fallback on `(ext_ip, ext_port)`, only if held by `target`.
    pub fn unregister_fallback(&self, ext_ip: Ipv4Addr, ext_port: u16, target: SocketAddrV4) {
        self.unregister_fallback_transport(TransportProtocol::Tcp, ext_ip, ext_port, target);
    }

    /// Removes a target-owned fallback without changing the other transport.
    pub fn unregister_fallback_transport(
        &self,
        protocol: TransportProtocol,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        target: SocketAddrV4,
    ) {
        self.unregister_fallback_protocol(protocol.into(), ext_ip, ext_port, target)
    }

    /// Removes a target-owned logical fallback from every selected transport.
    pub fn unregister_fallback_protocol(
        &self,
        protocol: Protocol,
        ext_ip: Ipv4Addr,
        ext_port: u16,
        target: SocketAddrV4,
    ) {
        self.ports.mutate(|ports| {
            for &transport in protocol.transports() {
                if let Some(entry) = ports.get_mut(&(ext_ip, ext_port, transport)) {
                    if entry
                        .fallback
                        .is_some_and(|fallback| fallback.target == target)
                    {
                        entry.fallback = None;
                    }
                }
            }
        });
        self.prune();
    }

    /// Live hostname routes, excluding port-level fallbacks.
    pub fn snapshot(&self) -> Vec<SniRoute> {
        let now = Instant::now();
        self.ports.peek(|ports| {
            ports
                .iter()
                .flat_map(|(&(ext_ip, ext_port, protocol), entry)| {
                    entry
                        .hostnames
                        .iter()
                        .filter(|(_, b)| b.expiry.is_none_or(|e| e > now))
                        .map(move |(name, b)| SniRoute {
                            ext_ip,
                            ext_port,
                            protocol,
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
                let destination = (new_ip, old.1, old.2);
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
                    cb(key.1, key.2, false);
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
        let removed = self.ports.mutate(|ports| {
            if !ports.get(&key).is_none_or(PortBindings::is_empty)
                || inherited_local_fallback(ports, key).is_some()
            {
                return None;
            }
            ports.remove(&key);
            self.listeners.mutate(|listeners| listeners.remove(&key))
        });
        if let Some(handle) = removed {
            drop(handle);
            if let Some(cb) = &self.on_change {
                cb(key.1, key.2, false);
            }
        }
    }

    fn prepare_listeners(
        &self,
        keys: &[PortKey],
        listeners: &BTreeMap<PortKey, NonDetachingJoinHandle<()>>,
    ) -> std::io::Result<Vec<(PortKey, PreparedListener)>> {
        keys.iter()
            .filter(|key| !listeners.contains_key(*key))
            .map(|&key| {
                let listener = match key.2 {
                    TransportProtocol::Tcp => {
                        PreparedListener::Tcp(crate::net::utils::bind_tokio_listener_reuse_port(
                            SocketAddrV4::new(key.0, key.1).into(),
                        )?)
                    }
                    TransportProtocol::Udp => PreparedListener::Udp(udp::bind(key)?),
                };
                Ok((key, listener))
            })
            .collect()
    }

    fn start_listeners(
        &self,
        listeners: &mut BTreeMap<PortKey, NonDetachingJoinHandle<()>>,
        prepared: Vec<(PortKey, PreparedListener)>,
    ) -> Vec<PortKey> {
        let mut opened = Vec::new();
        for (key, listener) in prepared {
            let ports = self.ports.clone();
            let local_prefix = self.local_prefix.clone();
            let handle = match listener {
                PreparedListener::Tcp(listener) => NonDetachingJoinHandle::from(tokio::spawn(
                    run_listener(listener, key, ports, local_prefix),
                )),
                PreparedListener::Udp(socket) => NonDetachingJoinHandle::from(tokio::spawn(
                    udp::run_listener(socket, key, ports, local_prefix, self.udp_limits.clone()),
                )),
            };
            listeners.insert(key, handle);
            opened.push(key);
        }
        opened
    }

    fn notify_opened(&self, keys: &[PortKey]) {
        if let Some(callback) = &self.on_change {
            for key in keys {
                callback(key.1, key.2, true);
            }
        }
    }

    /// Starts the address-specific listener before callers grant a route.
    fn ensure_listener(self: &Arc<Self>, key: PortKey) -> std::io::Result<()> {
        let opened = self.ports.peek(|ports| {
            self.listeners.mutate(|listeners| {
                if listeners.contains_key(&key) {
                    return Ok(Vec::new());
                }
                if ports.get(&key).is_none_or(PortBindings::is_empty) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "no bindings for SNI listener",
                    ));
                }
                let prepared = self.prepare_listeners(&[key], listeners)?;
                Ok(self.start_listeners(listeners, prepared))
            })
        })?;
        self.notify_opened(&opened);
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
    let deadline = tokio::time::Instant::now() + CLIENTHELLO_TIMEOUT;
    let sni = loop {
        match tokio::time::timeout_at(deadline, conn.read(&mut tmp)).await {
            Ok(Ok(0)) => break None,
            Ok(Ok(n)) => {
                buf.extend_from_slice(&tmp[..n]);
                match inspect_client_hello(&buf) {
                    HelloStatus::Complete(name) => break name,
                    HelloStatus::Invalid => break None,
                    HelloStatus::Pending if buf.len() >= CLIENTHELLO_CAP => break None,
                    HelloStatus::Pending => {}
                }
            }
            _ => break None,
        }
    };

    let SocketAddr::V4(peer) = peer else {
        return;
    };
    let selected = select_target(&ports, key, sni.as_deref(), *peer.ip());
    let Some((target, transparent)) = selected else {
        return;
    };
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

fn select_target(
    ports: &SyncMutex<BTreeMap<PortKey, PortBindings>>,
    key: PortKey,
    sni: Option<&str>,
    peer: Ipv4Addr,
) -> Option<(SocketAddrV4, bool)> {
    ports.mutate(|ports| {
        let local_fallback = inherited_local_fallback(ports, key);
        let entry = ports.get_mut(&key)?;
        entry.prune(Instant::now());
        entry.select_with_fallback(sni, peer, local_fallback)
    })
}

async fn is_hairpin(local_prefix: &Option<LocalPrefix>, peer: Ipv4Addr, target: Ipv4Addr) -> bool {
    if peer == target {
        return true;
    }
    let Some(resolve) = local_prefix else {
        return false;
    };
    let Some(prefix) = resolve(target).await else {
        return false;
    };
    Ipv4Net::new(target, prefix).is_ok_and(|net| net.contains(&peer))
}

fn lowercased(hostnames: &[String]) -> Vec<String> {
    hostnames.iter().map(|h| h.to_ascii_lowercase()).collect()
}

fn inspect_client_hello(buf: &[u8]) -> HelloStatus {
    let mut acceptor = tokio_rustls::rustls::server::Acceptor::default();
    let mut cursor = std::io::Cursor::new(buf);
    loop {
        match acceptor.read_tls(&mut cursor) {
            Ok(0) => return HelloStatus::Pending,
            Err(_) => return HelloStatus::Invalid,
            Ok(_) => {}
        }
        match acceptor.accept() {
            Ok(Some(accepted)) => {
                return HelloStatus::Complete(
                    accepted
                        .client_hello()
                        .server_name()
                        .map(|s| s.to_ascii_lowercase()),
                );
            }
            Ok(None) => {}
            Err(_) => return HelloStatus::Invalid,
        }
    }
}

#[cfg(test)]
fn extract_sni(buf: &[u8]) -> Option<String> {
    match inspect_client_hello(buf) {
        HelloStatus::Complete(name) => name,
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
            udp_limits: Arc::new(udp::Limits::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn free_port() -> u16 {
        std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

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

    #[tokio::test]
    async fn ui_fallback_survives_shared_port_routes_and_listener_changes() {
        use crate::net::web_server::Accept;
        use crate::tunnel::web::listener::UiListener;

        for listen_ip in [Ipv4Addr::UNSPECIFIED, Ipv4Addr::LOCALHOST] {
            let probe = std::net::TcpListener::bind((listen_ip, 0)).unwrap();
            let port = probe.local_addr().unwrap().port();
            drop(probe);
            let demux = SniDemux::new();
            let source = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
            let service = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let SocketAddr::V4(target) = service.local_addr().unwrap() else {
                unreachable!()
            };
            let hostname = "service.example.com".to_owned();
            demux
                .register(
                    *source.ip(),
                    port,
                    std::slice::from_ref(&hostname),
                    target,
                    None,
                )
                .unwrap();

            let mut ui = UiListener::bind((listen_ip, port).into(), demux.clone()).unwrap();
            for name in ["admin.example.com", "service.example.com"] {
                let hello = real_client_hello(name);
                let mut client = TcpStream::connect(source).await.unwrap();
                client.write_all(&hello).await.unwrap();
                let mut upstream = if name == hostname {
                    Box::pin(
                        timeout(Duration::from_secs(2), service.accept())
                            .await
                            .unwrap()
                            .unwrap()
                            .0,
                    ) as crate::net::web_server::AcceptStream
                } else {
                    timeout(
                        Duration::from_secs(2),
                        futures::future::poll_fn(|cx| ui.poll_accept(cx)),
                    )
                    .await
                    .unwrap()
                    .unwrap()
                    .1
                };
                let mut received = vec![0; hello.len()];
                timeout(Duration::from_secs(2), upstream.read_exact(&mut received))
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(received, hello);
                upstream.write_all(b"reachable").await.unwrap();
                let mut reply = [0; 9];
                timeout(Duration::from_secs(2), client.read_exact(&mut reply))
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(&reply, b"reachable");
            }

            let payload = b"\x16\x03\x01\x00\x00";
            let mut client = TcpStream::connect(source).await.unwrap();
            client.write_all(payload).await.unwrap();
            let (_, mut upstream) = timeout(
                Duration::from_secs(2),
                futures::future::poll_fn(|cx| ui.poll_accept(cx)),
            )
            .await
            .unwrap()
            .unwrap();
            let mut received = [0; 5];
            upstream.read_exact(&mut received).await.unwrap();
            assert_eq!(&received, payload);

            drop(ui);
            demux.ports.peek(|p| {
                assert!(
                    p.get(&(listen_ip, port, TransportProtocol::Tcp))
                        .is_none_or(|b| b.fallback.is_none())
                );
                assert_eq!(
                    p[&(*source.ip(), port, TransportProtocol::Tcp)]
                        .select(None, Ipv4Addr::LOCALHOST),
                    None
                );
            });
            let mut ui = UiListener::bind((listen_ip, port).into(), demux.clone()).unwrap();
            let mut client = TcpStream::connect(source).await.unwrap();
            client.write_all(payload).await.unwrap();
            timeout(
                Duration::from_secs(2),
                futures::future::poll_fn(|cx| ui.poll_accept(cx)),
            )
            .await
            .unwrap()
            .unwrap();
            demux.unregister(*source.ip(), port, &[hostname], target);
            assert!(demux.snapshot().is_empty());
            let mut client = TcpStream::connect(source).await.unwrap();
            client.write_all(payload).await.unwrap();
            timeout(
                Duration::from_secs(2),
                futures::future::poll_fn(|cx| ui.poll_accept(cx)),
            )
            .await
            .unwrap()
            .unwrap();
        }
    }

    #[test]
    fn parses_sni() {
        let hello = real_client_hello("git.example.com");
        assert_eq!(extract_sni(&hello).as_deref(), Some("git.example.com"));
    }

    #[test]
    fn a_complete_record_can_hold_an_incomplete_client_hello() {
        let hello = real_client_hello("fragmented.example.com");
        let payload_length = u16::from_be_bytes([hello[3], hello[4]]) as usize;
        let payload = &hello[5..5 + payload_length];
        let mut records = Vec::new();
        for (index, chunk) in payload.chunks(100).enumerate() {
            records.extend_from_slice(&hello[..3]);
            records.extend_from_slice(&(chunk.len() as u16).to_be_bytes());
            records.extend_from_slice(chunk);
            assert_eq!(
                inspect_client_hello(&records),
                if (index + 1) * 100 < payload_length {
                    HelloStatus::Pending
                } else {
                    HelloStatus::Complete(Some("fragmented.example.com".to_owned()))
                }
            );
        }
    }

    #[test]
    fn non_tls_is_none() {
        assert_eq!(extract_sni(b"GET / HTTP/1.1\r\n"), None);
    }

    #[tokio::test]
    async fn fallback_register_ownership_and_coexistence() {
        let demux = SniDemux::new();
        let ip: Ipv4Addr = Ipv4Addr::LOCALHOST;
        let port = free_port();
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
            let pb = p.get(&(ip, port, TransportProtocol::Tcp)).unwrap();
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
                p.get(&(ip, port, TransportProtocol::Tcp))
                    .unwrap()
                    .fallback
                    .map(|f| f.target),
                Some(fb)
            );
        });
        demux.unregister_fallback(ip, port, fb);
        demux.ports.peek(|p| {
            let pb = p.get(&(ip, port, TransportProtocol::Tcp)).unwrap();
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
        let port = free_port();
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
            let pb = p.get(&(ip, port, TransportProtocol::Tcp)).unwrap();
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
            let pb = p.get(&(ip, port, TransportProtocol::Tcp)).unwrap();
            assert_eq!(pb.select(None, public), Some((ui, false)));
        });
    }

    #[tokio::test]
    async fn bind_failure_refuses_grant_and_rolls_back() {
        let events = Arc::new(SyncMutex::new(Vec::<(u16, TransportProtocol, bool)>::new()));
        let recorded = events.clone();
        let demux = SniDemux::with_on_change(
            move |port, protocol, active| recorded.mutate(|e| e.push((port, protocol, active))),
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
        demux.ports.peek(|p| {
            assert!(!p.contains_key(&(Ipv4Addr::LOCALHOST, port, TransportProtocol::Tcp)))
        });

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
        assert_eq!(
            events.peek(|e| e.clone()),
            vec![(port, TransportProtocol::Tcp, true)]
        );
    }

    #[tokio::test]
    async fn hostnames_match_in_any_case() {
        let wildcard =
            crate::net::utils::bind_tokio_listener_reuse_port((Ipv4Addr::UNSPECIFIED, 0).into())
                .unwrap();
        let port = wildcard.local_addr().unwrap().port();
        let key = (Ipv4Addr::LOCALHOST, port, TransportProtocol::Tcp);
        let target = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 443);
        let demux = SniDemux::new();
        demux
            .register(
                key.0,
                port,
                &["Cloud.Example.com".to_string()],
                target,
                None,
            )
            .unwrap();
        demux.ports.peek(|p| {
            assert_eq!(
                p[&key].select(Some("cloud.example.com"), Ipv4Addr::LOCALHOST),
                Some((target, true))
            );
        });
        demux.unregister(key.0, port, &["CLOUD.example.com".to_string()], target);
        assert!(demux.snapshot().is_empty());
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
        demux.listeners.peek(|l| {
            assert!(l.contains_key(&(Ipv4Addr::LOCALHOST, port, TransportProtocol::Tcp)))
        });
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
        let before = demux.ports.peek(|ports| {
            ports[&(Ipv4Addr::LOCALHOST, port, TransportProtocol::Tcp)].hostnames[&hostname].clone()
        });
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
        let after = demux.ports.peek(|ports| {
            ports[&(Ipv4Addr::LOCALHOST, port, TransportProtocol::Tcp)].hostnames[&hostname].clone()
        });
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
        let port = free_port();
        let fallback_port = free_port();
        demux
            .register(ip, port, &["a.example.com".to_string()], t1, Some(3600))
            .unwrap();
        demux
            .register(ip, port, &["b.example.com".to_string()], t2, None)
            .unwrap();
        demux.register_fallback(ip, fallback_port, t1).unwrap();

        let mut snap = demux.snapshot();
        snap.sort_by(|a, b| a.hostname.cmp(&b.hostname));
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].hostname, "a.example.com");
        assert_eq!(snap[0].target, t1);
        assert_eq!((snap[0].ext_ip, snap[0].ext_port), (ip, port));
        let remaining = snap[0].remaining_secs.unwrap();
        assert!(remaining > 3590 && remaining <= 3600, "got {remaining}");
        assert_eq!(snap[1].remaining_secs, None);
    }

    #[tokio::test]
    async fn a_device_repoints_its_own_bindings() {
        let demux = SniDemux::new();
        let ip = Ipv4Addr::LOCALHOST;
        let port = free_port();
        let hostname = ["a.example.com".to_string()];
        let mine = |port| SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), port);
        let theirs = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 2), 9443);
        let anywhere = Ipv4Addr::new(203, 0, 113, 50);

        demux
            .register(ip, port, &hostname, mine(8443), None)
            .unwrap();
        demux.register_fallback(ip, port, mine(8443)).unwrap();
        assert_eq!(
            demux.register(ip, port, &hostname, theirs, None),
            Err(RESULT_HOSTNAME_TAKEN)
        );
        assert_eq!(
            demux.register_fallback(ip, port, theirs),
            Err(RESULT_HOSTNAME_TAKEN)
        );
        demux
            .register(ip, port, &hostname, mine(9443), None)
            .unwrap();
        demux.register_fallback(ip, port, mine(9443)).unwrap();
        demux.ports.peek(|p| {
            let pb = p.get(&(ip, port, TransportProtocol::Tcp)).unwrap();
            assert_eq!(
                pb.select(Some("a.example.com"), anywhere),
                Some((mine(9443), true))
            );
            assert_eq!(pb.select(None, anywhere), Some((mine(9443), true)));
        });
    }

    #[tokio::test]
    async fn registering_no_hostnames_starts_no_listener() {
        let demux = SniDemux::new();
        let target = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 443);
        demux
            .register(Ipv4Addr::LOCALHOST, 0, &[], target, None)
            .unwrap();
        assert!(demux.ports.peek(|p| p.is_empty()));
        assert!(demux.listeners.peek(|l| l.is_empty()));
    }

    #[tokio::test]
    async fn rekey_failure_does_not_create_an_empty_listener() {
        let blocker = Arc::new(SyncMutex::new(Some(
            std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap(),
        )));
        let port = blocker.peek(|listener| listener.as_ref().unwrap().local_addr().unwrap().port());
        let release_blocker = blocker.clone();
        let demux = SniDemux::with_on_change(
            move |_, _, active| {
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
        let events = Arc::new(SyncMutex::new(Vec::<(u16, TransportProtocol, bool)>::new()));
        let recorded = events.clone();
        let demux = SniDemux::with_on_change(
            move |port, protocol, active| recorded.mutate(|e| e.push((port, protocol, active))),
            None,
        );
        let old_ip = Ipv4Addr::new(127, 0, 0, 2);
        let new_ip = Ipv4Addr::LOCALHOST;
        let port = free_port();
        let t1 = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 443);
        let t2 = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 2), 443);

        demux
            .register(old_ip, port, &["a.example.com".to_string()], t1, Some(3600))
            .unwrap();
        demux.register_fallback(old_ip, port, t1).unwrap();
        demux
            .register(new_ip, port, &["a.example.com".to_string()], t2, None)
            .unwrap();
        demux
            .register(new_ip, port, &["c.example.com".to_string()], t2, None)
            .unwrap();

        demux.rekey_ipv4(new_ip);

        demux.ports.peek(|p| {
            assert!(
                p.get(&(old_ip, port, TransportProtocol::Tcp)).is_none(),
                "old key drained"
            );
            let pb = p.get(&(new_ip, port, TransportProtocol::Tcp)).unwrap();
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
            assert!(
                l.contains_key(&(new_ip, port, TransportProtocol::Tcp))
                    && !l.contains_key(&(old_ip, port, TransportProtocol::Tcp))
            )
        });
        assert!(
            events.peek(|e| e.iter().all(|&(_, _, active)| active)),
            "rekey must never fire on_change(port, false)"
        );

        let before = events.peek(|e| e.len());
        demux.rekey_ipv4(new_ip);
        assert_eq!(events.peek(|e| e.len()), before);
    }

    #[tokio::test]
    async fn transports_have_independent_ownership_expiry_and_rollback() {
        let demux = SniDemux::new();
        let ip = Ipv4Addr::LOCALHOST;
        let port = free_port();
        let hostname = ["shared.example.com".to_owned()];
        let tcp = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 443);
        let udp = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 2), 443);
        demux.register(ip, port, &hostname, tcp, Some(0)).unwrap();
        demux
            .register_transport(TransportProtocol::Udp, ip, port, &hostname, udp, None)
            .unwrap();
        let transaction = demux
            .register_transaction_transport(
                TransportProtocol::Udp,
                ip,
                port,
                &hostname,
                SocketAddrV4::new(*udp.ip(), 8443),
                Some(60),
            )
            .unwrap();
        demux.rollback(transaction);
        assert_eq!(
            select_target(
                &demux.ports,
                (ip, port, TransportProtocol::Tcp),
                Some(&hostname[0]),
                ip
            ),
            None
        );
        assert_eq!(
            select_target(
                &demux.ports,
                (ip, port, TransportProtocol::Udp),
                Some(&hostname[0]),
                ip
            ),
            Some((udp, true))
        );
        demux.unregister_transport(TransportProtocol::Udp, ip, port, &hostname, tcp);
        assert_eq!(demux.snapshot().len(), 1);
        demux.prune();
        demux.listeners.peek(|listeners| {
            assert!(!listeners.contains_key(&(ip, port, TransportProtocol::Tcp)));
            assert!(listeners.contains_key(&(ip, port, TransportProtocol::Udp)));
        });
        demux.unregister_transport(TransportProtocol::Udp, ip, port, &hostname, udp);
        assert!(demux.snapshot().is_empty());
        assert!(demux.listeners.peek(BTreeMap::is_empty));
    }

    #[tokio::test]
    async fn udp_bind_failure_does_not_displace_tcp() {
        let blocker = std::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = blocker.local_addr().unwrap().port();
        let demux = SniDemux::new();
        let hostname = ["shared.example.com".to_owned()];
        let target = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 443);
        demux
            .register(Ipv4Addr::LOCALHOST, port, &hostname, target, None)
            .unwrap();
        assert_eq!(
            demux.register_transport(
                TransportProtocol::Udp,
                Ipv4Addr::LOCALHOST,
                port,
                &hostname,
                target,
                None
            ),
            Err(RESULT_NO_RESOURCES)
        );
        let routes = demux.snapshot();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].protocol, TransportProtocol::Tcp);
        demux.listeners.peek(|listeners| {
            assert!(listeners.contains_key(&(Ipv4Addr::LOCALHOST, port, TransportProtocol::Tcp)));
            assert!(!listeners.contains_key(&(Ipv4Addr::LOCALHOST, port, TransportProtocol::Udp)));
        });
    }

    fn reserve_protocol_pair() -> (tokio::net::TcpListener, tokio::net::UdpSocket) {
        for _ in 0..64 {
            let tcp =
                crate::net::utils::bind_tokio_listener_reuse_port((Ipv4Addr::LOCALHOST, 0).into())
                    .unwrap();
            let port = tcp.local_addr().unwrap().port();
            match udp::bind((Ipv4Addr::LOCALHOST, port, TransportProtocol::Udp)) {
                Ok(udp) => return (tcp, udp),
                Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {}
                Err(error) => panic!("UDP fixture bind failed: {error}"),
            }
        }
        panic!("no free TCP/UDP fixture port");
    }

    fn blocked_udp_port() -> (tokio::net::TcpListener, std::net::UdpSocket) {
        for _ in 0..64 {
            let udp = std::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            let port = udp.local_addr().unwrap().port();
            match crate::net::utils::bind_tokio_listener_reuse_port(
                (Ipv4Addr::LOCALHOST, port).into(),
            ) {
                Ok(tcp) => return (tcp, udp),
                Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {}
                Err(error) => panic!("TCP fixture bind failed: {error}"),
            }
        }
        panic!("no TCP fixture port with blocked UDP");
    }

    #[tokio::test]
    async fn combined_mapping_publishes_both_before_notifying() {
        let owner = Arc::new(std::sync::OnceLock::<std::sync::Weak<SniDemux>>::new());
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let observed_owner = owner.clone();
        let observed_events = events.clone();
        let demux = SniDemux::with_on_change(
            move |_, _, active| {
                let snapshot = observed_owner.get().unwrap().upgrade().unwrap().snapshot();
                observed_events.lock().unwrap().push((
                    active,
                    snapshot
                        .into_iter()
                        .map(|route| route.protocol)
                        .collect::<Vec<_>>(),
                ));
            },
            None,
        );
        assert!(owner.set(Arc::downgrade(&demux)).is_ok());
        let reservation = reserve_protocol_pair();
        let port = reservation.0.local_addr().unwrap().port();
        let target = "127.0.0.2:8443".parse().unwrap();
        let hostnames = ["both.example.com".to_owned()];
        demux
            .register_protocol(
                Protocol::TcpUdp,
                Ipv4Addr::LOCALHOST,
                port,
                &hostnames,
                target,
                None,
            )
            .unwrap();
        assert_eq!(
            *events.lock().unwrap(),
            vec![
                (true, vec![TransportProtocol::Tcp, TransportProtocol::Udp]),
                (true, vec![TransportProtocol::Tcp, TransportProtocol::Udp]),
            ]
        );
        demux.unregister_protocol(
            Protocol::TcpUdp,
            Ipv4Addr::LOCALHOST,
            port,
            &hostnames,
            target,
        );
        assert!(demux.snapshot().is_empty());
        assert!(demux.listeners.peek(BTreeMap::is_empty));
        assert_eq!(
            &events.lock().unwrap()[2..],
            &[(false, Vec::new()), (false, Vec::new())]
        );
    }

    #[tokio::test]
    async fn combined_bind_failure_preserves_prior_routes_and_fallback() {
        let (tcp, _blocked_udp) = blocked_udp_port();
        let port = tcp.local_addr().unwrap().port();
        let demux = SniDemux::new();
        let ip = Ipv4Addr::LOCALHOST;
        let hostnames = ["both.example.com".to_owned()];
        let old = "127.0.0.2:8443".parse().unwrap();
        let new = "127.0.0.2:9443".parse().unwrap();
        demux.register(ip, port, &hostnames, old, None).unwrap();
        demux.register_fallback(ip, port, old).unwrap();
        assert_eq!(
            demux.register_protocol(Protocol::TcpUdp, ip, port, &hostnames, new, None),
            Err(RESULT_NO_RESOURCES)
        );
        assert_eq!(
            demux.register_fallback_protocol(Protocol::TcpUdp, ip, port, new),
            Err(RESULT_NO_RESOURCES)
        );
        assert_eq!(demux.snapshot().len(), 1);
        assert_eq!(demux.snapshot()[0].target, old);
        assert_eq!(
            select_target(&demux.ports, (ip, port, TransportProtocol::Tcp), None, ip),
            Some((old, true))
        );
        assert_eq!(
            select_target(&demux.ports, (ip, port, TransportProtocol::Udp), None, ip),
            None
        );
        assert_eq!(demux.listeners.peek(BTreeMap::len), 1);
    }

    #[tokio::test]
    async fn combined_transaction_restores_each_transports_exact_state() {
        let reservation = reserve_protocol_pair();
        let port = reservation.0.local_addr().unwrap().port();
        let demux = SniDemux::new();
        let ip = Ipv4Addr::LOCALHOST;
        let hostnames = ["both.example.com".to_owned()];
        let tcp = "127.0.0.2:8443".parse().unwrap();
        let udp = "127.0.0.2:9443".parse().unwrap();
        let combined = "127.0.0.2:443".parse().unwrap();
        demux
            .register(ip, port, &hostnames, tcp, Some(300))
            .unwrap();
        demux
            .register_transport(TransportProtocol::Udp, ip, port, &hostnames, udp, None)
            .unwrap();
        let before = demux.ports.peek(|ports| {
            TransportProtocol::ALL.map(|transport| ports[&(ip, port, transport)].hostnames.clone())
        });
        let transaction = demux
            .register_transaction_protocol(
                Protocol::TcpUdp,
                ip,
                port,
                &hostnames,
                combined,
                Some(600),
            )
            .unwrap();
        assert!(
            demux
                .snapshot()
                .iter()
                .all(|route| route.target == combined)
        );
        demux.rollback(transaction);
        let after = demux.ports.peek(|ports| {
            TransportProtocol::ALL.map(|transport| ports[&(ip, port, transport)].hostnames.clone())
        });
        assert_eq!(after, before);
    }

    #[tokio::test]
    async fn combined_registration_relays_tcp_and_quic() {
        let (tcp_backend, udp_backend) = reserve_protocol_pair();
        let SocketAddr::V4(target) = tcp_backend.local_addr().unwrap() else {
            unreachable!()
        };
        let reservation = reserve_protocol_pair();
        let port = reservation.0.local_addr().unwrap().port();
        let demux = SniDemux::new();
        demux
            .register_protocol(
                Protocol::TcpUdp,
                Ipv4Addr::LOCALHOST,
                port,
                &["quic.example.com".into()],
                target,
                None,
            )
            .unwrap();
        drop(reservation);
        timeout(Duration::from_secs(3), async {
            let hello = real_client_hello("quic.example.com");
            let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
                .await
                .unwrap();
            client.write_all(&hello).await.unwrap();
            let (mut upstream, _) = tcp_backend.accept().await.unwrap();
            let mut received = vec![0; hello.len()];
            upstream.read_exact(&mut received).await.unwrap();
            assert_eq!(received, hello);
            upstream.write_all(b"TCP reply").await.unwrap();
            let mut reply = [0; 9];
            client.read_exact(&mut reply).await.unwrap();
            assert_eq!(&reply, b"TCP reply");
            let client = tokio::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let initial = quic::tests::initial(1, 0, 0, &quic::tests::client_hello(1, true));
            client
                .send_to(&initial, (Ipv4Addr::LOCALHOST, port))
                .await
                .unwrap();
            let mut buffer = vec![0; 65535];
            let (length, peer) = udp_backend.recv_from(&mut buffer).await.unwrap();
            assert_eq!(&buffer[..length], initial.as_slice());
            udp_backend.send_to(b"UDP reply", peer).await.unwrap();
            let (length, source) = client.recv_from(&mut buffer).await.unwrap();
            assert_eq!(&buffer[..length], b"UDP reply");
            assert_eq!(source, SocketAddr::from((Ipv4Addr::LOCALHOST, port)));
        })
        .await
        .expect("combined relay timed out");
    }

    #[tokio::test]
    async fn self_hairpin_needs_no_subnet_resolver() {
        let target = Ipv4Addr::new(10, 59, 0, 2);
        assert!(is_hairpin(&None, target, target).await);
        assert!(!is_hairpin(&None, Ipv4Addr::new(10, 59, 0, 3), target).await);
        let unknown: LocalPrefix = Arc::new(|_| Box::pin(async { None }));
        assert!(is_hairpin(&Some(unknown), target, target).await);
    }

    #[tokio::test]
    async fn self_hairpin_relays_hostname_and_fallback_traffic() {
        for named in [true, false] {
            let backend = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let SocketAddr::V4(target) = backend.local_addr().unwrap() else {
                unreachable!();
            };
            let probe = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            let port = probe.local_addr().unwrap().port();
            drop(probe);
            let demux = SniDemux::new();
            if named {
                demux
                    .register(
                        Ipv4Addr::LOCALHOST,
                        port,
                        &["probe.example.com".into()],
                        target,
                        None,
                    )
                    .unwrap();
            } else {
                demux
                    .register_fallback(Ipv4Addr::LOCALHOST, port, target)
                    .unwrap();
            }
            timeout(Duration::from_secs(3), async {
                let hello = real_client_hello("probe.example.com");
                let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
                    .await
                    .unwrap();
                client.write_all(&hello).await.unwrap();
                let (mut upstream, _) = backend.accept().await.unwrap();
                let mut received = vec![0; hello.len()];
                upstream.read_exact(&mut received).await.unwrap();
                assert_eq!(received, hello);
                upstream.write_all(b"reply").await.unwrap();
                let mut reply = [0; 5];
                client.read_exact(&mut reply).await.unwrap();
                assert_eq!(&reply, b"reply");
                client.shutdown().await.unwrap();
                upstream.shutdown().await.unwrap();
            })
            .await
            .expect("self-hairpin relay timed out");
        }
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
