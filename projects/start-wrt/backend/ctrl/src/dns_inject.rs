//! DNS injection: RFC 2136 UPDATE ingress for the router's resolver.
//!
//! The nft include `13-startwrt-dns-update-divert.nft` redirects UPDATE
//! packets arriving on a gateway's port 53 to the per-profile listeners here;
//! dnsmasq keeps every query. Accepted records are rendered into per-profile
//! addn-hosts files that dnsmasq re-reads on SIGHUP.
//!
//! `policy` decides two tiers. A TSIG-signed UPDATE (an inbound WireGuard
//! peer, key derived from its PSK) may publish any A/AAAA/CNAME/TXT record.
//! An unsigned one (a LAN device with the permission) may publish A/AAAA
//! records pointing at its own source address. Both refuse names under
//! `lan.`, and a name belongs to the first owner that claims it.
//!
//! Listeners are `SO_BINDTODEVICE`-bound per profile, so the arrival
//! interface is the kernel's fact. Records, ownership and the directory live
//! in memory; clients re-assert within 180 s of a restart.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use hickory_server::proto::op::ResponseCode;
use hickory_server::proto::rr::{DNSClass, LowerName, Name, RData, Record, RecordType};
use hickory_server::server::Server;
use rpc_toolkit::{from_fn_async_local, HandlerExt as _, ParentHandler};
use serde::{Deserialize, Serialize};
use startos::net::dns_update::rfc2136::{DnsInjector, InjectedRecord, InjectingHandler};
use startos::net::dns_update::{derive_tsig_key, forwarding_catalog};
use startos::util::future::NonDetachingJoinHandle;
use startos::util::sync::SyncMutex;
use tokio_util::sync::CancellationToken;
use uciedit::openwrt::{DhcpHost, FirewallForwarding, FirewallZone, NetworkInterface};
use uciedit::{dump_all, parse_all, Arena, Configs, Line};

use crate::error::ErrorKind;
use crate::invoke::Invoke;
use crate::port_control::{supervise, uci_task};
use crate::prelude::*;
use crate::utils::{DeserializeStdin, HandlerExtSerde};
use crate::{CliContext, CtrlContext, Error, ServerContext};

/// Redirect targets of the nft include. LAN bridges and inbound WireGuard
/// interfaces take separate ports so no two device-bound sockets share an
/// (addr, port). Both sit above SmartDNS's 5300–9394 range.
pub(crate) const DNS_UPDATE_PORT_LAN: u16 = 9553;
pub(crate) const DNS_UPDATE_PORT_WG: u16 = 9554;

/// Directory rebuild and sweep cadence.
const REFRESH_INTERVAL: Duration = Duration::from_secs(60);
const FORWARD_TIMEOUT: Duration = Duration::from_secs(30);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const INJECT_FILE_PREFIX: &str = "startwrt-dns-inject.dns_";

/// The daemon's DNS-injection service; unset in CLI / `--configs-only` mode.
pub static DNS_INJECT: OnceLock<Arc<DnsInject>> = OnceLock::new();

/// The tmpfs addn-hosts file of a profile's dnsmasq instance. dnsmasq's ujail
/// bind-mounts it at instance start, so it must exist before dnsmasq starts
/// (the `startwrt-dnsinject` init script creates it) and is only ever
/// rewritten in place.
pub(crate) fn inject_hosts_path(interface: &str) -> String {
    format!("/tmp/{INJECT_FILE_PREFIX}{interface}")
}

/// Who injected a record: a LAN MAC, or an inbound WireGuard peer's public key.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Owner {
    Mac(String),
    WgPeer(String),
}

/// An address currently allowed to inject, resolved by the refresher.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Injector {
    owner: Owner,
    /// Profile interface (e.g. "lan") whose subnet holds this address.
    profile: String,
}

/// One profile's network identity, as the listeners and renderer need it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProfileNet {
    /// UCI interface name, e.g. "lan" / "guest".
    interface: String,
    /// Kernel bridge device, e.g. "br-lan.101".
    device: String,
    gateway: Ipv4Addr,
    /// Firewall zone, for the `lan_access` visibility relation.
    zone: String,
    /// The inbound-VPN device (`wg_<interface>`), when one is configured.
    wg_device: Option<String>,
}

/// State the injector's synchronous hooks read. A refresher task rebuilds it
/// from UCI, the DHCP leases and the neighbor table.
#[derive(Default)]
struct Directory {
    by_ip: BTreeMap<IpAddr, Injector>,
    /// Derived TSIG keys for inbound WireGuard peers, by tunnel address.
    wg_keys: BTreeMap<IpAddr, [u8; 32]>,
    /// First-come name ownership.
    owners: BTreeMap<(LowerName, RecordType), Owner>,
    profiles: Vec<ProfileNet>,
    /// `(viewer zone, source zone)` pairs the firewall forwards. A record is
    /// served only to zones that can reach its source.
    reach: BTreeSet<(String, String)>,
}

/// What the refresher learns from one UCI pass, before live lease/neighbor
/// data joins it.
#[derive(Default)]
struct NetSnapshot {
    profiles: Vec<ProfileNet>,
    reach: BTreeSet<(String, String)>,
    /// Uppercase MACs with `_allow_dns_inject '1'`.
    allowed_macs: BTreeSet<String>,
    /// Uppercase MAC → static reservation IP.
    reserved: HashMap<String, Ipv4Addr>,
    /// Inbound-WG peer tunnel IP → (public key, derived TSIG key, profile).
    wg_peers: BTreeMap<Ipv4Addr, (String, [u8; 32], String)>,
}

pub struct DnsInject {
    uci_root: PathBuf,
    injector: Arc<DnsInjector>,
    directory: Arc<SyncMutex<Directory>>,
    /// Latest record snapshot; the renderer always writes the newest one.
    render_rx: tokio::sync::watch::Receiver<Vec<InjectedRecord>>,
    /// Shared with the injector's `on_change`. The refresher sends on it too:
    /// the rendered files encode the directory as well as the records.
    render_tx: tokio::sync::watch::Sender<Vec<InjectedRecord>>,
    /// Wakes the refresher without waiting out the interval.
    poke: Arc<tokio::sync::Notify>,
    listeners: tokio::sync::Mutex<Vec<Listener>>,
}

/// A running per-profile UPDATE listener. Shut down by cancelling `shutdown`
/// and then awaiting `task`; an abort alone leaves the socket bound.
struct Listener {
    net: ProfileNet,
    shutdown: CancellationToken,
    task: NonDetachingJoinHandle<()>,
}

impl DnsInject {
    pub fn new(uci_root: PathBuf) -> Arc<Self> {
        let directory = Arc::new(SyncMutex::new(Directory::default()));
        let (tx, rx) = tokio::sync::watch::channel(Vec::new());
        let render_tx = tx.clone();
        let injector = {
            let key_dir = directory.clone();
            let policy_dir = directory.clone();
            DnsInjector::new(
                Vec::new(),
                // `policy` is the one gate; it logs its refusals.
                |_| true,
                move |src| key_dir.peek(|d| d.wg_keys.get(&src).copied()),
                move |records| {
                    let _ = tx.send(records);
                },
                move |src, updates, tsig_ok| policy(&policy_dir, src, updates, tsig_ok),
            )
        };
        Arc::new(Self {
            uci_root,
            injector,
            directory,
            render_rx: rx,
            render_tx,
            poke: Arc::new(tokio::sync::Notify::new()),
            listeners: tokio::sync::Mutex::new(Vec::new()),
        })
    }

    /// Wakes the refresher now; a revocation drops the device's records on
    /// this pass.
    pub fn invalidate(&self) {
        self.poke.notify_one();
    }
}

/// Run the service for the life of the daemon.
pub async fn run(di: Arc<DnsInject>) {
    // The rendered files outlived the last daemon; the records did not.
    purge_rendered_files().await;
    tokio::join!(
        supervise("dns-inject refresh", di.clone(), run_refresh),
        supervise("dns-inject render", di, run_render),
    );
}

async fn run_refresh(di: Arc<DnsInject>) {
    loop {
        if let Err(e) = refresh(&di).await {
            tracing::warn!("dns-inject refresh failed: {e}");
        }
        tokio::select! {
            _ = tokio::time::sleep(REFRESH_INTERVAL) => {}
            _ = di.poke.notified() => {}
        }
    }
}

/// One refresher pass: rebuild the directory, sweep stale records, rebind
/// listeners to the current profile set.
async fn refresh(di: &Arc<DnsInject>) -> Result<(), Error> {
    let uci_root = di.uci_root.clone();
    let snapshot = uci_task(move || async move {
        let arena = Arena::new();
        let cfgs = parse_all(
            &uci_root,
            &arena,
            &["startwrt", "network", "dhcp", "firewall"],
        )
        .await?;
        read_snapshot(&cfgs)
    })
    .await?;

    let leases = crate::devices::current_lease_ips()
        .await
        .unwrap_or_default();
    let neigh = tokio::process::Command::new("ip")
        .args(["neigh", "show"])
        .invoke(ErrorKind::Network.into())
        .await
        .ok()
        .and_then(|out| String::from_utf8(out).ok())
        .unwrap_or_default();
    let neighbors = crate::devices::parse_neigh_output(&neigh);

    // Candidate addresses per allowed MAC: reservation, lease, neighbor entry.
    let mut by_ip: BTreeMap<IpAddr, Injector> = BTreeMap::new();
    let mut wg_keys = BTreeMap::new();
    for mac in &snapshot.allowed_macs {
        let mut addrs: BTreeSet<Ipv4Addr> = BTreeSet::new();
        if let Some(ip) = snapshot.reserved.get(mac) {
            addrs.insert(*ip);
        }
        if let Some(ip) = leases.get(mac).and_then(|ip| ip.parse().ok()) {
            addrs.insert(ip);
        }
        for entry in &neighbors {
            if entry.mac.eq_ignore_ascii_case(mac) {
                if let Ok(ip) = entry.ip.parse::<Ipv4Addr>() {
                    addrs.insert(ip);
                }
            }
        }
        for ip in addrs {
            if let Some(p) = profile_for(&snapshot.profiles, ip) {
                by_ip.insert(
                    IpAddr::V4(ip),
                    Injector {
                        owner: Owner::Mac(mac.clone()),
                        profile: p.interface.clone(),
                    },
                );
            }
        }
    }
    for (ip, (pubkey, key, profile)) in &snapshot.wg_peers {
        by_ip.insert(
            IpAddr::V4(*ip),
            Injector {
                owner: Owner::WgPeer(pubkey.clone()),
                profile: profile.clone(),
            },
        );
        wg_keys.insert(IpAddr::V4(*ip), *key);
    }

    // The sweep runs under the same lock that publishes the directory.
    let stale = di.directory.mutate(|d| {
        d.by_ip = by_ip;
        d.wg_keys = wg_keys;
        d.profiles = snapshot.profiles.clone();
        d.reach = snapshot.reach.clone();
        sweep_owners(d, &snapshot, &leases, &neighbors, &di.injector.list())
    });
    // The daemon's default filter is `warn`.
    for (name, rtype) in stale {
        tracing::warn!(
            "DNS-inject sweep dropped {name} {rtype}: its owner no longer holds \
             the permission or the address the record points at"
        );
        di.injector.delete(&Name::from(name), Some(rtype));
    }

    // The rendered files encode the directory too; the renderer's content
    // diff keeps a quiet pass free.
    di.render_tx.send_replace(di.injector.list());

    sync_listeners(di, &snapshot.profiles).await;
    Ok(())
}

/// The (name, rtype) rrsets whose owner lost the permission, the peer entry,
/// or the address an A record points at.
fn sweep_owners(
    d: &mut Directory,
    snapshot: &NetSnapshot,
    leases: &HashMap<String, String>,
    neighbors: &[crate::devices::ArpEntry],
    records: &[InjectedRecord],
) -> Vec<(LowerName, RecordType)> {
    // The same three address sources the directory build admits.
    let mac_holds = |mac: &str, ip: Ipv4Addr| {
        snapshot.reserved.get(mac) == Some(&ip)
            || leases.get(mac).and_then(|l| l.parse().ok()) == Some(ip)
            || neighbors
                .iter()
                .any(|e| e.mac.eq_ignore_ascii_case(mac) && e.ip.parse().ok() == Some(ip))
    };
    let mut stale = Vec::new();
    for ((name, rtype), owner) in &d.owners {
        let rdatas = records
            .iter()
            .filter(|r| &LowerName::from(&r.name) == name && r.rtype == *rtype);
        let live = match owner {
            Owner::Mac(mac) => {
                snapshot.allowed_macs.contains(mac)
                    && rdatas.clone().all(|r| match &r.rdata {
                        RData::A(a) => mac_holds(mac, (*a).into()),
                        _ => true,
                    })
            }
            Owner::WgPeer(pubkey) => snapshot.wg_peers.values().any(|(pk, _, _)| pk == pubkey),
        };
        if !live {
            stale.push((name.clone(), *rtype));
        }
    }
    for key in &stale {
        d.owners.remove(key);
    }
    // A withdrawn name releases its claim.
    d.owners.retain(|(name, rtype), _| {
        records
            .iter()
            .any(|r| &LowerName::from(&r.name) == name && r.rtype == *rtype)
    });
    stale
}

/// Which profile's /24 contains `ip`. Every profile subnet is a /24, and
/// inbound-VPN peers are allocated inside their profile's.
fn profile_for(profiles: &[ProfileNet], ip: Ipv4Addr) -> Option<&ProfileNet> {
    profiles
        .iter()
        .find(|p| p.gateway.octets()[..3] == ip.octets()[..3])
}

fn read_snapshot(cfgs: &Configs) -> Result<NetSnapshot, Error> {
    let mut snapshot = NetSnapshot::default();

    // Profile → interface section, zone, inbound-VPN interface.
    let mut zones: Vec<(String, Vec<String>)> = Vec::new();
    cfgs["firewall"].each::<FirewallZone, Error>(|_, zone| {
        zones.push((zone.name.clone(), zone.network.clone()));
    })?;
    cfgs["firewall"].each::<FirewallForwarding, Error>(|_, fwd| {
        snapshot.reach.insert((fwd.src, fwd.dest));
    })?;
    cfgs["startwrt"].each::<crate::profiles::UciProfile, Error>(|_, profile| {
        let iface = profile.interface.clone();
        let Some((gateway, device)) = cfgs["network"].sections.iter().find_map(|s| {
            if s.name().as_deref() != Some(iface.as_str()) {
                return None;
            }
            let net = s.get::<NetworkInterface>().ok()?;
            Some((net.ipaddr?, net.device))
        }) else {
            return;
        };
        let zone = zones
            .iter()
            .find(|(_, networks)| networks.iter().any(|n| n == &iface))
            .map(|(name, _)| name.clone())
            .unwrap_or_else(|| format!("vlan_{iface}"));
        let wg_name = format!("wg_{iface}");
        let wg_device = cfgs["network"]
            .sections
            .iter()
            .any(|s| s.name().as_deref() == Some(wg_name.as_str()))
            .then_some(wg_name.clone());
        snapshot.profiles.push(ProfileNet {
            interface: iface,
            device,
            gateway,
            zone,
            wg_device,
        });
    })?;

    // Permitted LAN devices and their static reservations.
    cfgs["dhcp"].each::<DhcpHost, Error>(|_, host| {
        if host._allow_dns_inject.as_deref() == Some("1") {
            snapshot.allowed_macs.insert(host.mac.to_uppercase());
        }
        if let Some(ip) = host.ip.as_deref().and_then(|ip| ip.parse().ok()) {
            snapshot.reserved.insert(host.mac.to_uppercase(), ip);
        }
    })?;

    // `wireguard_wg_<iface>` peer sections: public key, PSK, tunnel /32.
    for p in &snapshot.profiles {
        let Some(wg) = &p.wg_device else { continue };
        let peer_ty = format!("wireguard_{wg}");
        for section in &cfgs["network"].sections {
            if section.ty() != peer_ty {
                continue;
            }
            let mut pubkey = None;
            let mut psk = None;
            let mut ip = None;
            for line in &section.lines {
                match line {
                    Line::Option { option, value, .. } => match &*option.as_str() {
                        "public_key" => pubkey = Some(value.as_str().to_string()),
                        "preshared_key" => psk = Some(value.as_str().to_string()),
                        _ => {}
                    },
                    Line::List { list, item, .. } if list.as_str() == "allowed_ips" => {
                        if let Some(v4) = item
                            .as_str()
                            .split('/')
                            .next()
                            .and_then(|s| s.parse::<Ipv4Addr>().ok())
                        {
                            ip = Some(v4);
                        }
                    }
                    _ => {}
                }
            }
            let (Some(pubkey), Some(psk), Some(ip)) = (pubkey, psk, ip) else {
                continue;
            };
            use base64::Engine;
            let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&psk) else {
                continue;
            };
            let Ok(psk) = <[u8; 32]>::try_from(bytes) else {
                continue;
            };
            snapshot
                .wg_peers
                .insert(ip, (pubkey, derive_tsig_key(&psk), p.interface.clone()));
        }
    }
    Ok(snapshot)
}

/// The tiered `pre_update` policy. Every record is validated before any
/// ownership is claimed.
fn policy(
    directory: &SyncMutex<Directory>,
    src: IpAddr,
    updates: &[Record],
    tsig_ok: bool,
) -> ResponseCode {
    directory.mutate(|d| {
        // The client sees only a generic failure; the divert's 20/s limit
        // bounds this log.
        let refuse = |why: String| {
            tracing::warn!("DNS UPDATE from {src} refused: {why}");
            ResponseCode::Refused
        };
        let Some(owner) = d.by_ip.get(&src).map(|i| i.owner.clone()) else {
            return refuse(
                "source holds no known address assignment with the DNS-injection \
                 permission"
                    .into(),
            );
        };
        for rec in updates {
            let name = LowerName::from(&rec.name);
            // dnsmasq is authoritative for `lan.`.
            if lan_zone().zone_of(&name) {
                return refuse(format!("{name} is inside the reserved `lan.` zone"));
            }
            let rtype = rec.record_type();
            match rec.dns_class {
                DNSClass::IN => {
                    if tsig_ok {
                        // Signed tier.
                        if !matches!(
                            rtype,
                            RecordType::A | RecordType::AAAA | RecordType::CNAME | RecordType::TXT
                        ) {
                            return refuse(format!("record type {rtype} is not injectable"));
                        }
                    } else {
                        // Unsigned tier: a name may point only at its source.
                        let rdata_is_src = match &rec.data {
                            RData::A(a) => IpAddr::V4((*a).into()) == src,
                            RData::AAAA(a) => IpAddr::V6((*a).into()) == src,
                            _ => false,
                        };
                        if !rdata_is_src {
                            return refuse(format!(
                                "{name}: an unsigned update may only point a name at \
                                 its own source address"
                            ));
                        }
                    }
                    if d.owners
                        .get(&(name.clone(), rtype))
                        .is_some_and(|o| *o != owner)
                    {
                        return refuse(format!("{name} is owned by another device"));
                    }
                }
                // Deleting an unheld name is a no-op the store ignores.
                DNSClass::ANY if rtype == RecordType::ANY => {
                    if d.owners.iter().any(|((n, _), o)| *n == name && *o != owner) {
                        return refuse(format!("{name} is owned by another device"));
                    }
                }
                DNSClass::ANY | DNSClass::NONE => {
                    if d.owners
                        .get(&(name.clone(), rtype))
                        .is_some_and(|o| *o != owner)
                    {
                        return refuse(format!("{name} is owned by another device"));
                    }
                }
                _ => {}
            }
        }
        // A `NONE`-class delete may leave the rrset populated; its claim
        // stays until the sweep sees it empty.
        for rec in updates {
            let name = LowerName::from(&rec.name);
            let rtype = rec.record_type();
            match rec.dns_class {
                DNSClass::IN => {
                    d.owners.insert((name, rtype), owner.clone());
                }
                DNSClass::ANY if rtype == RecordType::ANY => {
                    d.owners.retain(|(n, _), _| *n != name);
                }
                DNSClass::ANY => {
                    d.owners.remove(&(name, rtype));
                }
                _ => {}
            }
        }
        ResponseCode::NoError
    })
}

fn lan_zone() -> LowerName {
    LowerName::from(Name::from_ascii("lan.").expect("static valid name"))
}

// ── Listeners ──────────────────────────────────────────────

/// Rebinds listeners to the current profile set. A failed bind is retried
/// next refresh.
async fn sync_listeners(di: &Arc<DnsInject>, profiles: &[ProfileNet]) {
    let mut listeners = di.listeners.lock().await;
    let (keep, drop): (Vec<_>, Vec<_>) = std::mem::take(&mut *listeners)
        .into_iter()
        .partition(|l| profiles.contains(&l.net));
    *listeners = keep;
    for l in drop {
        l.shutdown.cancel();
        if tokio::time::timeout(SHUTDOWN_TIMEOUT, l.task)
            .await
            .is_err()
        {
            tracing::warn!(
                "dns-inject listener for {} did not shut down in time",
                l.net.interface
            );
        }
    }
    for p in profiles {
        if listeners.iter().any(|l| &l.net == p) {
            continue;
        }
        match bind_listener(di.injector.clone(), p) {
            Ok(l) => listeners.push(l),
            Err(e) => {
                tracing::warn!("dns-inject bind for {} failed: {e}", p.interface);
            }
        }
    }
}

fn bind_listener(injector: Arc<DnsInjector>, p: &ProfileNet) -> Result<Listener, Error> {
    // The miss path forwards to the profile's own dnsmasq.
    let catalog = forwarding_catalog(vec![SocketAddr::from((p.gateway, 53))], FORWARD_TIMEOUT)?;
    let mut server = Server::new(InjectingHandler::new(injector, catalog));
    server.register_socket(bind_device_udp(p.gateway, DNS_UPDATE_PORT_LAN, &p.device)?);
    if let Some(wg) = &p.wg_device {
        // Best-effort: the wg interface can lag its UCI section.
        match bind_device_udp(p.gateway, DNS_UPDATE_PORT_WG, wg) {
            Ok(socket) => server.register_socket(socket),
            Err(e) => tracing::warn!("dns-inject wg bind on {wg} failed: {e}"),
        }
    }
    let shutdown = server.shutdown_token().clone();
    let iface = p.interface.clone();
    let task = tokio::spawn(async move {
        if let Err(e) = server.block_until_done().await {
            tracing::warn!("dns-inject listener for {iface} exited: {e}");
        }
    })
    .into();
    Ok(Listener {
        net: p.clone(),
        shutdown,
        task,
    })
}

/// A UDP socket bound to exactly one kernel device.
fn bind_device_udp(
    addr: Ipv4Addr,
    port: u16,
    device: &str,
) -> Result<tokio::net::UdpSocket, Error> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )
    .with_kind(ErrorKind::Network)?;
    socket
        .bind_device(Some(device.as_bytes()))
        .with_kind(ErrorKind::Network)?;
    socket.set_nonblocking(true).with_kind(ErrorKind::Network)?;
    socket
        .bind(&SocketAddrV4::new(addr, port).into())
        .with_kind(ErrorKind::Network)?;
    tokio::net::UdpSocket::from_std(socket.into()).with_kind(ErrorKind::Network)
}

// ── Answer plane ───────────────────────────────────────────

async fn run_render(di: Arc<DnsInject>) {
    let mut rx = di.render_rx.clone();
    // The refresher's wake after its first pass writes the baseline files.
    let mut last: HashMap<String, String> = HashMap::new();
    loop {
        let records = rx.borrow_and_update().clone();
        if let Err(e) = render_all(&di, &records, &mut last).await {
            tracing::warn!("dns-inject render failed: {e}");
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}

/// Renders every profile's addn-hosts file. Unchanged content is neither
/// written nor signalled.
async fn render_all(
    di: &Arc<DnsInject>,
    records: &[InjectedRecord],
    last: &mut HashMap<String, String>,
) -> Result<(), Error> {
    let (profiles, reach, by_ip) = di.directory.peek(|d| {
        (
            d.profiles.clone(),
            d.reach.clone(),
            d.by_ip
                .iter()
                .map(|(ip, i)| (*ip, i.profile.clone()))
                .collect::<BTreeMap<IpAddr, String>>(),
        )
    });
    for p in &profiles {
        let content = profile_hosts_content(p, &profiles, &reach, &by_ip, records);
        if last.get(&p.interface).map(String::as_str) == Some(content.as_str()) {
            continue;
        }
        let path = inject_hosts_path(&p.interface);
        // In place, never tmp + rename: dnsmasq's ujail holds a bind mount on
        // this inode.
        tokio::fs::write(&path, &content)
            .await
            .with_kind(ErrorKind::Filesystem)?;
        signal_dnsmasq_instance(&format!("dns_{}", p.interface)).await;
        last.insert(p.interface.clone(), content);
    }
    // Profiles that vanished take their files with them.
    let live: BTreeSet<&String> = profiles.iter().map(|p| &p.interface).collect();
    let stale: Vec<String> = last
        .keys()
        .filter(|iface| !live.contains(iface))
        .cloned()
        .collect();
    for iface in stale {
        let _ = tokio::fs::remove_file(inject_hosts_path(&iface)).await;
        last.remove(&iface);
    }
    Ok(())
}

/// The addn-hosts content profile `p` may see: A/AAAA records whose source
/// profile `p`'s zone can reach. Sorted and deduped.
fn profile_hosts_content(
    p: &ProfileNet,
    profiles: &[ProfileNet],
    reach: &BTreeSet<(String, String)>,
    by_ip: &BTreeMap<IpAddr, String>,
    records: &[InjectedRecord],
) -> String {
    // By the source's subnet, else by the directory.
    let source_profile = |r: &InjectedRecord| -> Option<String> {
        match r.source {
            IpAddr::V4(v4) => profile_for(profiles, v4)
                .map(|sp| sp.interface.clone())
                .or_else(|| by_ip.get(&r.source).cloned()),
            _ => by_ip.get(&r.source).cloned(),
        }
    };
    let mut lines: Vec<String> = records
        .iter()
        .filter(|r| matches!(r.rdata, RData::A(_) | RData::AAAA(_)))
        .filter(|r| {
            let Some(source) = source_profile(r) else {
                return false;
            };
            source == p.interface
                || profiles
                    .iter()
                    .find(|sp| sp.interface == source)
                    .is_some_and(|sp| reach.contains(&(p.zone.clone(), sp.zone.clone())))
        })
        .map(|r| format!("{} {}\n", r.rdata, r.name.to_utf8().trim_end_matches('.')))
        .collect();
    lines.sort();
    lines.dedup();
    lines.concat()
}

/// SIGHUPs one dnsmasq instance through procd. dnsmasq runs in its own PID
/// namespace, so its pidfile holds `1`.
async fn signal_dnsmasq_instance(instance: &str) {
    if let Err(e) = tokio::process::Command::new("ubus")
        .args([
            "call",
            "service",
            "signal",
            &format!(r#"{{"name":"dnsmasq","instance":"{instance}","signal":1}}"#),
        ])
        .invoke(ErrorKind::Network.into())
        .await
    {
        tracing::warn!("SIGHUP of dnsmasq instance {instance} failed: {e}");
    }
}

/// Truncates, never removes: a running dnsmasq instance holds a bind mount on
/// each file.
async fn purge_rendered_files() {
    let Ok(mut dir) = tokio::fs::read_dir("/tmp").await else {
        return;
    };
    while let Ok(Some(entry)) = dir.next_entry().await {
        if entry
            .file_name()
            .to_str()
            .is_some_and(|n| n.starts_with(INJECT_FILE_PREFIX))
        {
            let _ = tokio::fs::write(entry.path(), b"").await;
        }
    }
}

// ── RPC ────────────────────────────────────────────────────

pub fn dns<C: CtrlContext>() -> ParentHandler<C> {
    ParentHandler::new().subcommand(
        "injected-list",
        from_fn_async_local(injected_list)
            .with_display_serializable()
            .with_call_remote::<CliContext>(),
    )
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InjectedDnsRecord {
    pub name: String,
    pub rtype: String,
    pub value: String,
    pub ttl: u32,
    /// The injecting device's address, when known.
    pub source: Option<String>,
    /// Owning LAN device MAC (uppercase); absent for a WireGuard peer.
    pub owner_mac: Option<String>,
    /// Owning inbound-VPN peer public key; absent for a LAN device.
    pub owner_peer: Option<String>,
    /// Display name of the owning LAN device, when one is known.
    pub device_name: Option<String>,
    /// Profile interface whose subnet the record was injected from.
    pub profile: Option<String>,
}

/// The injected records, for the UI.
#[instrument(skip_all)]
pub async fn injected_list(ctx: ServerContext) -> Result<Vec<InjectedDnsRecord>, Error> {
    let Some(di) = DNS_INJECT.get() else {
        return Ok(Vec::new());
    };
    let arena = Arena::new();
    let cfgs = parse_all(ctx.uci_root(), &arena, &["dhcp"]).await?;
    let names = crate::port_control::device_display_names(&cfgs["dhcp"]).unwrap_or_default();
    let owners = di.directory.peek(|d| d.owners.clone());
    let profiles = di.directory.peek(|d| {
        d.by_ip
            .iter()
            .map(|(ip, i)| (*ip, i.profile.clone()))
            .collect::<BTreeMap<IpAddr, String>>()
    });
    Ok(di
        .injector
        .list()
        .into_iter()
        .map(|r| {
            let (name, rtype, value, ttl, source) = r.to_parts();
            let owner = owners.get(&(LowerName::from(&r.name), r.rtype));
            let (owner_mac, owner_peer) = match owner {
                Some(Owner::Mac(mac)) => (Some(mac.clone()), None),
                Some(Owner::WgPeer(pk)) => (None, Some(pk.clone())),
                None => (None, None),
            };
            InjectedDnsRecord {
                name,
                rtype,
                value,
                ttl,
                profile: source.and_then(|ip| profiles.get(&ip).cloned()),
                source: source.map(|ip| ip.to_string()),
                device_name: owner_mac.as_ref().and_then(|mac| names.get(mac).cloned()),
                owner_mac,
                owner_peer,
            }
        })
        .collect())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetDnsInjectionReq {
    pub mac: String,
    pub allow: bool,
}

/// Sets a device's DNS-injection permission on its DHCP host entry and
/// rewrites the per-profile dnsmasq instances.
#[instrument(skip_all)]
pub async fn set_dns_injection<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<SetDnsInjectionReq>,
) -> Result<(), Error> {
    // A bad MAC in a `config host` section can stop dnsmasq loading the file.
    if !crate::published_ports::validate_mac(&req.mac) {
        return Err(Error::new(
            eyre!("invalid mac: {}", req.mac),
            ErrorKind::InvalidValue,
        ));
    }
    let mac_upper = req.mac.to_uppercase();
    let allow = req.allow;
    let log_failure = |err: &Error| {
        crate::activity::log(
            "device",
            "dns-injection",
            false,
            &format!("Failed to update DNS injection for {mac_upper}"),
            Some(&err.to_string()),
        );
    };
    let written =
        crate::devices::upsert_dhcp_host(&ctx.uci_root(), &req.mac, move |host, existed| {
            if !existed && !allow {
                return false; // nothing to do: absent = denied
            }
            host._allow_dns_inject = allow.then(|| "1".to_string());
            true
        })
        .await
        .inspect_err(|err| log_failure(err))?;
    if written {
        rewrite_instances(&ctx).await.inspect_err(log_failure)?;
        crate::activity::log(
            "device",
            "dns-injection",
            true,
            &format!(
                "{} DNS injection for {mac_upper}",
                if req.allow { "Enabled" } else { "Disabled" }
            ),
            None,
        );
        // A reload, not a SIGHUP: instances may have been created or removed.
        if ctx.effectful() {
            crate::devices::reload_dnsmasq();
        }
        // A revocation drops the device's records on this pass.
        if let Some(di) = DNS_INJECT.get() {
            di.invalidate();
        }
    }
    Ok(())
}

/// Rewrites the per-profile dnsmasq sections to match the inject-permitted
/// set.
async fn rewrite_instances<C: CtrlContext>(ctx: &C) -> Result<(), Error> {
    let uci_root = ctx.uci_root();
    let mut retries = 4;
    loop {
        let uci_root = uci_root.clone();
        // `true` = a concurrent-write conflict worth retrying.
        let conflicted = uci_task(move || async move {
            let arena = Arena::new();
            let mut cfgs = parse_all(
                &uci_root,
                &arena,
                &["startwrt", "network", "dhcp", "firewall"],
            )
            .await?;
            crate::profiles::rewrite_all_dns_forwarding(&mut cfgs)?;
            match dump_all(&uci_root, cfgs).await {
                Ok(()) => Ok(false),
                Err(uciedit::Error::Conflict { .. }) => Ok(true),
                Err(e) => Err(e.into()),
            }
        })
        .await?;
        if !conflicted {
            return Ok(());
        }
        if retries == 0 {
            return Err(Error::new(
                eyre!("persistent UCI write conflict rewriting dnsmasq instances"),
                ErrorKind::UciEdit,
            ));
        }
        retries -= 1;
    }
}

#[cfg(test)]
mod tests {
    use hickory_server::proto::rr::rdata::{A, CNAME};

    use super::*;

    fn fqdn(s: &str) -> Name {
        let mut n = Name::from_utf8(s).unwrap();
        n.set_fqdn(true);
        n
    }

    fn a_record(name: &str, addr: Ipv4Addr) -> Record {
        Record::from_rdata(fqdn(name), 300, RData::A(A::from(addr)))
    }

    fn injected(name: &str, addr: Ipv4Addr, source: Ipv4Addr) -> InjectedRecord {
        InjectedRecord {
            name: fqdn(name),
            rtype: RecordType::A,
            rdata: RData::A(A::from(addr)),
            ttl: 300,
            source: IpAddr::V4(source),
        }
    }

    fn lan_ip(host: u8) -> Ipv4Addr {
        Ipv4Addr::new(192, 168, 1, host)
    }

    /// A directory with one permitted LAN device and one WireGuard peer.
    fn directory() -> SyncMutex<Directory> {
        let mut d = Directory::default();
        d.by_ip.insert(
            IpAddr::V4(lan_ip(50)),
            Injector {
                owner: Owner::Mac("AA:BB:CC:DD:EE:FF".into()),
                profile: "lan".into(),
            },
        );
        d.by_ip.insert(
            IpAddr::V4(lan_ip(51)),
            Injector {
                owner: Owner::Mac("11:22:33:44:55:66".into()),
                profile: "lan".into(),
            },
        );
        d.by_ip.insert(
            IpAddr::V4(lan_ip(200)),
            Injector {
                owner: Owner::WgPeer("pubkey1".into()),
                profile: "lan".into(),
            },
        );
        SyncMutex::new(d)
    }

    #[tokio::test]
    async fn unsigned_tier_is_publish_yourself_only() {
        let dir = directory();
        let src = IpAddr::V4(lan_ip(50));
        assert_eq!(
            policy(&dir, src, &[a_record("nas.example.com", lan_ip(50))], false),
            ResponseCode::NoError,
            "rdata == source is the permitted shape"
        );
        assert_eq!(
            policy(&dir, src, &[a_record("nas.example.com", lan_ip(99))], false),
            ResponseCode::Refused,
            "pointing a name at someone else needs a signature"
        );
        let cname = Record::from_rdata(
            fqdn("alias.example.com"),
            300,
            RData::CNAME(CNAME(fqdn("nas.example.com"))),
        );
        assert_eq!(
            policy(&dir, src, &[cname], false),
            ResponseCode::Refused,
            "unsigned tier is A/AAAA only"
        );
    }

    #[tokio::test]
    async fn signed_tier_allows_any_rdata_and_more_types() {
        let dir = directory();
        let src = IpAddr::V4(lan_ip(200));
        assert_eq!(
            policy(&dir, src, &[a_record("svc.example.com", lan_ip(7))], true),
            ResponseCode::NoError,
            "signed: any rdata"
        );
        let cname = Record::from_rdata(
            fqdn("alias.example.com"),
            300,
            RData::CNAME(CNAME(fqdn("svc.example.com"))),
        );
        assert_eq!(policy(&dir, src, &[cname], true), ResponseCode::NoError);
        let srv = Record::from_rdata(
            fqdn("_x._tcp.example.com"),
            300,
            RData::SRV(hickory_server::proto::rr::rdata::SRV::new(
                0,
                0,
                443,
                fqdn("svc.example.com"),
            )),
        );
        assert_eq!(
            policy(&dir, src, &[srv], true),
            ResponseCode::Refused,
            "even signed injection is limited to the admin-path types"
        );
    }

    #[tokio::test]
    async fn unknown_source_is_refused() {
        let dir = directory();
        assert_eq!(
            policy(
                &dir,
                IpAddr::V4(lan_ip(9)),
                &[a_record("nas.example.com", lan_ip(9))],
                false
            ),
            ResponseCode::Refused
        );
    }

    #[tokio::test]
    async fn lan_names_are_reserved() {
        let dir = directory();
        assert_eq!(
            policy(
                &dir,
                IpAddr::V4(lan_ip(50)),
                &[a_record("nas.lan", lan_ip(50))],
                false
            ),
            ResponseCode::Refused,
            "dnsmasq is authoritative for lan."
        );
        assert_eq!(
            policy(
                &dir,
                IpAddr::V4(lan_ip(200)),
                &[a_record("nas.lan", lan_ip(7))],
                true
            ),
            ResponseCode::Refused,
            "reserved even for the signed tier"
        );
    }

    #[tokio::test]
    async fn name_ownership_is_first_come() {
        let dir = directory();
        let first = IpAddr::V4(lan_ip(50));
        let second = IpAddr::V4(lan_ip(51));
        assert_eq!(
            policy(
                &dir,
                first,
                &[a_record("nas.example.com", lan_ip(50))],
                false
            ),
            ResponseCode::NoError
        );
        assert_eq!(
            policy(
                &dir,
                second,
                &[a_record("nas.example.com", lan_ip(51))],
                false
            ),
            ResponseCode::Refused,
            "a held name refuses a different identity"
        );
        assert_eq!(
            policy(
                &dir,
                first,
                &[a_record("nas.example.com", lan_ip(50))],
                false
            ),
            ResponseCode::NoError,
            "the owner may re-assert"
        );
        // A whole-name delete from the non-owner is refused; from the owner
        // it releases the claim, so the second device may then take the name.
        let delete = |name: &str| {
            let mut r = Record::update0(fqdn(name), 0, RecordType::ANY);
            r.dns_class = DNSClass::ANY;
            r
        };
        assert_eq!(
            policy(&dir, second, &[delete("nas.example.com")], false),
            ResponseCode::Refused
        );
        assert_eq!(
            policy(&dir, first, &[delete("nas.example.com")], false),
            ResponseCode::NoError
        );
        assert_eq!(
            policy(
                &dir,
                second,
                &[a_record("nas.example.com", lan_ip(51))],
                false
            ),
            ResponseCode::NoError,
            "released name is claimable again"
        );
    }

    #[tokio::test]
    async fn refused_message_claims_nothing() {
        let dir = directory();
        let src = IpAddr::V4(lan_ip(50));
        // Second record invalid (rdata != src), so the whole message refuses
        // — and the valid first record must not have claimed its name.
        let updates = [
            a_record("good.example.com", lan_ip(50)),
            a_record("bad.example.com", lan_ip(99)),
        ];
        assert_eq!(policy(&dir, src, &updates, false), ResponseCode::Refused);
        assert!(
            dir.peek(|d| d.owners.is_empty()),
            "refusal leaves no ownership trace"
        );
    }

    fn snapshot_with(macs: &[&str]) -> NetSnapshot {
        let mut s = NetSnapshot::default();
        for mac in macs {
            s.allowed_macs.insert(mac.to_string());
        }
        s
    }

    #[tokio::test]
    async fn sweep_reaps_revoked_and_moved_owners() {
        let mac = "AA:BB:CC:DD:EE:FF";
        let mut d = Directory::default();
        let name = LowerName::from(&fqdn("nas.example.com"));
        d.owners
            .insert((name.clone(), RecordType::A), Owner::Mac(mac.into()));
        let records = vec![injected("nas.example.com", lan_ip(50), lan_ip(50))];

        // Permission revoked → reaped.
        let stale = sweep_owners(&mut d, &snapshot_with(&[]), &HashMap::new(), &[], &records);
        assert_eq!(stale, vec![(name.clone(), RecordType::A)]);
        assert!(d.owners.is_empty());

        // Permitted and holding the address (lease) → kept.
        d.owners
            .insert((name.clone(), RecordType::A), Owner::Mac(mac.into()));
        let leases = HashMap::from([(mac.to_string(), lan_ip(50).to_string())]);
        assert!(sweep_owners(&mut d, &snapshot_with(&[mac]), &leases, &[], &records).is_empty());
        assert_eq!(d.owners.len(), 1);

        // Address moved to a different lease → reaped: DHCP is free to hand
        // the old address to someone else.
        let moved = HashMap::from([(mac.to_string(), lan_ip(77).to_string())]);
        assert_eq!(
            sweep_owners(&mut d, &snapshot_with(&[mac]), &moved, &[], &records),
            vec![(name.clone(), RecordType::A)]
        );

        // A WireGuard owner lives exactly as long as its peer config.
        d.owners
            .insert((name.clone(), RecordType::A), Owner::WgPeer("pk".into()));
        let mut with_peer = snapshot_with(&[]);
        with_peer
            .wg_peers
            .insert(lan_ip(200), ("pk".into(), [0u8; 32], "lan".into()));
        let wg_records = vec![injected("nas.example.com", lan_ip(200), lan_ip(200))];
        assert!(sweep_owners(&mut d, &with_peer, &HashMap::new(), &[], &wg_records).is_empty());
        assert_eq!(
            sweep_owners(
                &mut d,
                &snapshot_with(&[]),
                &HashMap::new(),
                &[],
                &wg_records
            ),
            vec![(name, RecordType::A)]
        );
    }

    /// A static-IP device is admitted on its neighbor entry alone; the sweep
    /// counts that entry too.
    #[tokio::test]
    async fn sweep_keeps_owner_known_only_from_neighbors() {
        let mac = "AA:BB:CC:DD:EE:FF";
        let mut d = Directory::default();
        let name = LowerName::from(&fqdn("nas.example.com"));
        d.owners
            .insert((name.clone(), RecordType::A), Owner::Mac(mac.into()));
        let records = vec![injected("nas.example.com", lan_ip(50), lan_ip(50))];
        // Lowercase, as `ip neigh` prints it: the match is case-insensitive.
        let neighbors = vec![crate::devices::ArpEntry {
            ip: lan_ip(50).to_string(),
            mac: mac.to_lowercase(),
            interface: "br-lan.1".into(),
            state: "REACHABLE".into(),
        }];
        assert!(
            sweep_owners(
                &mut d,
                &snapshot_with(&[mac]),
                &HashMap::new(),
                &neighbors,
                &records
            )
            .is_empty(),
            "a neighbor entry holds the address as surely as a lease does"
        );
        assert_eq!(d.owners.len(), 1);
    }

    #[tokio::test]
    async fn ownership_does_not_outlive_records() {
        let mac = "AA:BB:CC:DD:EE:FF";
        let mut d = Directory::default();
        d.owners.insert(
            (LowerName::from(&fqdn("gone.example.com")), RecordType::A),
            Owner::Mac(mac.into()),
        );
        let leases = HashMap::from([(mac.to_string(), lan_ip(50).to_string())]);
        // No record backs the claim (the client withdrew it): released, but
        // not reported stale — there is nothing to delete from the store.
        assert!(sweep_owners(&mut d, &snapshot_with(&[mac]), &leases, &[], &[]).is_empty());
        assert!(d.owners.is_empty());
    }

    fn profile(iface: &str, third_octet: u8, zone: &str) -> ProfileNet {
        ProfileNet {
            interface: iface.into(),
            device: format!("br-lan.{third_octet}"),
            gateway: Ipv4Addr::new(192, 168, third_octet, 1),
            zone: zone.into(),
            wg_device: None,
        }
    }

    /// A `wireguard_wg_<iface>` peer section yields a directory entry keyed
    /// by its tunnel /32, carrying the TSIG key derived from its PSK.
    #[tokio::test]
    async fn snapshot_derives_wg_peer_tsig_keys() {
        use base64::Engine;
        let dir = tempfile::tempdir().unwrap();
        let psk = [7u8; 32];
        let psk_b64 = base64::engine::general_purpose::STANDARD.encode(psk);
        std::fs::write(
            dir.path().join("startwrt"),
            "config profile lan\n\toption fullname 'Admin'\n\toption interface 'lan'\n\toption vlan_tag '1'\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("network"),
            format!(
                "config interface 'lan'\n\toption proto 'static'\n\toption ipaddr '192.168.1.1'\n\toption device 'br-lan.1'\n\n\
                 config interface 'wg_lan'\n\toption proto 'wireguard'\n\n\
                 config wireguard_wg_lan\n\toption public_key 'peer-pub'\n\toption preshared_key '{psk_b64}'\n\tlist allowed_ips '10.59.0.2/32'\n\n\
                 config wireguard_wg_lan\n\toption public_key 'broken'\n\toption preshared_key 'not-base64!'\n\tlist allowed_ips '10.59.0.3/32'\n"
            ),
        )
        .unwrap();
        std::fs::write(dir.path().join("dhcp"), "").unwrap();
        std::fs::write(dir.path().join("firewall"), "").unwrap();

        let arena = Arena::new();
        let cfgs = parse_all(
            dir.path(),
            &arena,
            &["startwrt", "network", "dhcp", "firewall"],
        )
        .await
        .unwrap();
        let snapshot = read_snapshot(&cfgs).unwrap();

        let p = snapshot
            .profiles
            .iter()
            .find(|p| p.interface == "lan")
            .unwrap();
        assert_eq!(p.wg_device.as_deref(), Some("wg_lan"));

        let (pubkey, key, profile) = snapshot
            .wg_peers
            .get(&Ipv4Addr::new(10, 59, 0, 2))
            .expect("peer keyed by its tunnel /32");
        assert_eq!(pubkey, "peer-pub");
        assert_eq!(profile, "lan");
        assert_eq!(
            *key,
            derive_tsig_key(&psk),
            "the key the listener verifies against derives from the peer's PSK \
             exactly as the client derives its signing key"
        );
        assert!(
            !snapshot.wg_peers.contains_key(&Ipv4Addr::new(10, 59, 0, 3)),
            "a peer whose PSK cannot decode contributes no key"
        );
    }

    #[tokio::test]
    async fn rendered_hosts_follow_lan_access() {
        let profiles = vec![
            profile("lan", 1, "lan"),
            profile("guest", 101, "vlan_guest"),
            profile("iot", 102, "vlan_iot"),
        ];
        // guest → lan is forwarded (guest may reach lan); iot reaches nobody.
        let reach = BTreeSet::from([("vlan_guest".into(), "lan".into())]);
        let by_ip = BTreeMap::new();
        let records = vec![
            injected("nas.example.com", lan_ip(50), lan_ip(50)),
            InjectedRecord {
                name: fqdn("txt.example.com"),
                rtype: RecordType::TXT,
                rdata: RData::TXT(hickory_server::proto::rr::rdata::TXT::new(vec!["x".into()])),
                ttl: 300,
                source: IpAddr::V4(lan_ip(50)),
            },
        ];
        let content =
            |p: &ProfileNet| profile_hosts_content(p, &profiles, &reach, &by_ip, &records);
        assert_eq!(
            content(&profiles[0]),
            "192.168.1.50 nas.example.com\n",
            "a profile always sees its own records; TXT cannot appear in a hosts file"
        );
        assert_eq!(
            content(&profiles[1]),
            "192.168.1.50 nas.example.com\n",
            "guest reaches lan, so the name resolves there"
        );
        assert_eq!(
            content(&profiles[2]),
            "",
            "iot is firewalled away from lan: resolving the name would only leak it"
        );
    }

    /// The rendered files encode the directory, so a refresh wakes the
    /// renderer even when no record changed.
    #[tokio::test]
    async fn refresh_wakes_render_without_record_change() {
        let dir = tempfile::tempdir().unwrap();
        for f in ["startwrt", "network", "dhcp", "firewall"] {
            std::fs::write(dir.path().join(f), "").unwrap();
        }
        let di = DnsInject::new(dir.path().to_path_buf());
        let mut rx = di.render_rx.clone();
        rx.borrow_and_update();
        assert!(!rx.has_changed().unwrap());

        refresh(&di).await.unwrap();
        assert!(
            rx.has_changed().unwrap(),
            "a refresh with unchanged records must still wake the renderer"
        );
        rx.borrow_and_update();

        refresh(&di).await.unwrap();
        assert!(
            rx.has_changed().unwrap(),
            "every pass wakes; the renderer's content diff is what keeps quiet passes free"
        );
    }
}
