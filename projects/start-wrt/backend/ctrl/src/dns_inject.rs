//! DNS injection: RFC 2136 UPDATE ingress for the router's resolver.
//!
//! The nft include `13-startwrt-dns-update-divert.nft` redirects UDP UPDATE
//! packets and every TCP connection arriving on a gateway's port 53 to the
//! per-profile listeners here. UDP queries stay with dnsmasq; a TCP
//! connection's queries pass through to it. Accepted records are rendered
//! into per-profile addn-hosts files that dnsmasq re-reads on SIGHUP.
//!
//! `policy` decides two tiers. A TSIG-signed UPDATE (an inbound WireGuard
//! peer, key derived from its PSK) may publish any A/AAAA/CNAME/TXT record.
//! An unsigned one (a LAN device with the permission) must arrive over TCP
//! and may publish A/AAAA records. Both are refused a name under `lan.` and
//! a name public DNS resolves anywhere but this router's WAN address. A name
//! belongs to the first owner that claims it.
//!
//! Listeners are `SO_BINDTODEVICE`-bound per profile, so the arrival
//! interface is the kernel's fact. Records, ownership and the directory live
//! in memory; clients re-assert within 180 s of a restart.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use futures::FutureExt;
use hickory_server::net::runtime::TokioRuntimeProvider;
use hickory_server::proto::op::ResponseCode;
use hickory_server::proto::rr::{DNSClass, LowerName, Name, RData, Record, RecordType};
use hickory_server::resolver::config::{
    ConnectionConfig, LookupIpStrategy, NameServerConfig, ResolveHosts, ResolverConfig,
    ResolverOpts,
};
use hickory_server::resolver::Resolver;
use hickory_server::server::Server;
use rpc_toolkit::{from_fn_async_local, HandlerExt as _, ParentHandler};
use serde::{Deserialize, Serialize};
use startos::net::dns_update::rfc2136::{
    DnsInjector, InjectedRecord, InjectingHandler, UpdateAuth,
};
use startos::net::dns_update::{derive_tsig_key, forwarding_catalog};
use startos::util::future::NonDetachingJoinHandle;
use startos::util::sync::SyncMutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
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
/// Idle limit on a TCP client, and on each exchange with dnsmasq.
const TCP_IDLE_TIMEOUT: Duration = Duration::from_secs(10);
/// Concurrent TCP clients per listening socket; more are closed on accept.
const TCP_MAX_CLIENTS: usize = 16;
/// Refusal log lines per second, box-wide.
const REFUSAL_LOG_RATE: u32 = 20;
const PUBLIC_LOOKUP_TIMEOUT: Duration = Duration::from_secs(3);
/// Special-use zones, which public DNS never delegates.
const PRIVATE_ZONES: &[&str] = &[
    "local.",
    "home.arpa.",
    "internal.",
    "test.",
    "invalid.",
    "localhost.",
    "example.",
];
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
    /// The router's WAN addresses.
    wan: BTreeSet<IpAddr>,
}

/// A name's standing in public DNS.
#[derive(Clone, Debug, PartialEq, Eq)]
enum PublicAnswer {
    /// NXDOMAIN, or no address records.
    Absent,
    Addrs(Vec<IpAddr>),
    Failed(String),
}

type PublicLookup = Arc<dyn Fn(Name) -> BoxFuture<'static, PublicAnswer> + Send + Sync>;

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
        let public = upstream_lookup().unwrap_or_else(|e| {
            tracing::warn!("dns-inject public resolver unavailable: {e}");
            let e = e.to_string();
            Arc::new(move |_| futures::future::ready(PublicAnswer::Failed(e.clone())).boxed())
        });
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
                move |src, updates: Vec<Record>, auth| {
                    let directory = policy_dir.clone();
                    let public = public.clone();
                    async move { policy(&directory, &public, src, &updates, auth).await }
                },
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
    let mut wan: BTreeSet<IpAddr> = crate::system::get_wan_ipv4()
        .await
        .ok()
        .flatten()
        .map(IpAddr::V4)
        .into_iter()
        .collect();
    wan.extend(
        crate::system::get_wan_ipv6s()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(IpAddr::V6),
    );

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
        d.wan = wan;
        sweep_owners(d, &snapshot, &leases, &neighbors, &di.injector.list())
    });
    // The daemon's default filter is `warn`.
    for (name, rtype) in stale {
        tracing::warn!(
            "DNS-inject sweep dropped {name} {rtype}: its owner no longer holds \
             the permission or the address it published from"
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
/// or the address it published from.
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
        let mut published = records
            .iter()
            .filter(|r| &LowerName::from(&r.name) == name && r.rtype == *rtype);
        let live = match owner {
            Owner::Mac(mac) => {
                snapshot.allowed_macs.contains(mac)
                    && published.all(|r| match r.source {
                        IpAddr::V4(source) => mac_holds(mac, source),
                        IpAddr::V6(_) => true,
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
async fn policy(
    directory: &SyncMutex<Directory>,
    public: &PublicLookup,
    src: IpAddr,
    updates: &[Record],
    auth: UpdateAuth,
) -> ResponseCode {
    // The client sees only a generic failure.
    let refuse = |why: String| {
        log_refusal(src, &why);
        ResponseCode::Refused
    };
    let wan = match directory.peek(|d| check(d, src, updates, auth).map(|_| d.wan.clone())) {
        Ok(wan) => wan,
        Err(why) => return refuse(why),
    };
    let added: BTreeMap<LowerName, &Name> = updates
        .iter()
        .filter(|r| r.dns_class == DNSClass::IN)
        .map(|r| (LowerName::from(&r.name), &r.name))
        .collect();
    for (lower, name) in added {
        if let Err(why) = publicly_claimable(public, &lower, name, &wan).await {
            return refuse(why);
        }
    }
    directory.mutate(|d| match check(d, src, updates, auth) {
        Ok(owner) => {
            claim(d, &owner, updates);
            ResponseCode::NoError
        }
        Err(why) => refuse(why),
    })
}

/// The update's owner, when the directory admits every record.
fn check(
    d: &Directory,
    src: IpAddr,
    updates: &[Record],
    auth: UpdateAuth,
) -> Result<Owner, String> {
    let owner = d.by_ip.get(&src).map(|i| i.owner.clone()).ok_or_else(|| {
        "source holds no known address assignment with the DNS-injection permission".to_string()
    })?;
    if !auth.tsig && !auth.tcp {
        return Err("an unsigned update must arrive over TCP".into());
    }
    let held_by_other = |name: &LowerName, rtype: RecordType| {
        d.owners
            .get(&(name.clone(), rtype))
            .is_some_and(|o| *o != owner)
    };
    for rec in updates {
        let name = LowerName::from(&rec.name);
        // dnsmasq is authoritative for `lan.`.
        if lan_zone().zone_of(&name) {
            return Err(format!("{name} is inside the reserved `lan.` zone"));
        }
        let rtype = rec.record_type();
        let held = match rec.dns_class {
            DNSClass::IN => {
                if !matches!(
                    rtype,
                    RecordType::A | RecordType::AAAA | RecordType::CNAME | RecordType::TXT
                ) {
                    return Err(format!("record type {rtype} is not injectable"));
                }
                if !auth.tsig && !matches!(rtype, RecordType::A | RecordType::AAAA) {
                    return Err(format!("record type {rtype} needs a signed update"));
                }
                held_by_other(&name, rtype)
            }
            // Deleting an unheld name is a no-op the store ignores.
            DNSClass::ANY if rtype == RecordType::ANY => {
                d.owners.iter().any(|((n, _), o)| *n == name && *o != owner)
            }
            DNSClass::ANY | DNSClass::NONE => held_by_other(&name, rtype),
            _ => false,
        };
        if held {
            return Err(format!("{name} is owned by another device"));
        }
    }
    Ok(owner)
}

/// Records what an admitted update claims and releases. A `NONE`-class delete
/// may leave the rrset populated; its claim stays until the sweep sees it
/// empty.
fn claim(d: &mut Directory, owner: &Owner, updates: &[Record]) {
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
}

/// Refuses a name public DNS resolves to anything but a WAN address. A failed
/// lookup refuses too.
async fn publicly_claimable(
    public: &PublicLookup,
    lower: &LowerName,
    name: &Name,
    wan: &BTreeSet<IpAddr>,
) -> Result<(), String> {
    if PRIVATE_ZONES
        .iter()
        .any(|z| LowerName::from(Name::from_ascii(z).expect("static valid name")).zone_of(lower))
    {
        return Ok(());
    }
    match public(name.clone()).await {
        PublicAnswer::Absent => Ok(()),
        PublicAnswer::Addrs(addrs) => match addrs.iter().find(|a| !wan.contains(a)) {
            None => Ok(()),
            Some(addr) => Err(format!(
                "{name} publicly resolves to {addr}, not this router"
            )),
        },
        PublicAnswer::Failed(e) => Err(format!("{name}: public lookup failed: {e}")),
    }
}

/// Resolves through the main dnsmasq instance, which serves no injected
/// records.
fn upstream_lookup() -> Result<PublicLookup, Error> {
    let mut config = ResolverConfig::from_parts(None, Vec::new(), Vec::new());
    config.add_name_server(NameServerConfig::new(
        Ipv4Addr::LOCALHOST.into(),
        true,
        vec![ConnectionConfig::udp(), ConnectionConfig::tcp()],
    ));
    let mut opts = ResolverOpts::default();
    opts.timeout = PUBLIC_LOOKUP_TIMEOUT;
    opts.attempts = 1;
    opts.ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
    opts.use_hosts_file = ResolveHosts::Never;
    let resolver = Resolver::builder_with_config(config, TokioRuntimeProvider::default())
        .with_options(opts)
        .build()
        .map_err(|e| Error::new(eyre!("{e}"), ErrorKind::Network))?;
    Ok(Arc::new(move |name| {
        let resolver = resolver.clone();
        async move {
            match resolver.lookup_ip(name).await {
                Ok(lookup) => PublicAnswer::Addrs(lookup.iter().collect()),
                Err(e) if e.is_nx_domain() || e.is_no_records_found() => PublicAnswer::Absent,
                Err(e) => PublicAnswer::Failed(e.to_string()),
            }
        }
        .boxed()
    }))
}

fn lan_zone() -> LowerName {
    LowerName::from(Name::from_ascii("lan.").expect("static valid name"))
}

fn log_refusal(src: IpAddr, why: &str) {
    static WINDOW: Mutex<Option<(Instant, u32)>> = Mutex::new(None);
    let mut window = WINDOW.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    let (start, count) = window.get_or_insert((now, 0));
    if now.duration_since(*start) >= Duration::from_secs(1) {
        (*start, *count) = (now, 0);
    }
    if *count < REFUSAL_LOG_RATE {
        *count += 1;
        tracing::warn!("DNS UPDATE from {src} refused: {why}");
    }
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
    // The profile's own dnsmasq.
    let upstream = SocketAddr::from((p.gateway, 53));
    let catalog = forwarding_catalog(vec![upstream], FORWARD_TIMEOUT)?;
    let mut server = Server::new(InjectingHandler::new(injector.clone(), catalog));
    server.register_socket(bind_device_udp(p.gateway, DNS_UPDATE_PORT_LAN, &p.device)?);
    let mut tcp = vec![bind_device_tcp(p.gateway, DNS_UPDATE_PORT_LAN, &p.device)?];
    if let Some(wg) = &p.wg_device {
        // Best-effort: the wg interface can lag its UCI section.
        let bound = bind_device_udp(p.gateway, DNS_UPDATE_PORT_WG, wg)
            .and_then(|udp| Ok((udp, bind_device_tcp(p.gateway, DNS_UPDATE_PORT_WG, wg)?)));
        match bound {
            Ok((udp, listener)) => {
                server.register_socket(udp);
                tcp.push(listener);
            }
            Err(e) => tracing::warn!("dns-inject wg bind on {wg} failed: {e}"),
        }
    }
    let shutdown = server.shutdown_token().clone();
    let tcp_shutdown = shutdown.clone();
    let iface = p.interface.clone();
    let task = tokio::spawn(async move {
        let tcp = futures::future::join_all(
            tcp.into_iter()
                .map(|l| serve_tcp(l, injector.clone(), upstream, tcp_shutdown.clone())),
        );
        let (udp, _) = tokio::join!(server.block_until_done(), tcp);
        if let Err(e) = udp {
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

/// A TCP listener bound to exactly one kernel device.
fn bind_device_tcp(addr: Ipv4Addr, port: u16, device: &str) -> Result<TcpListener, Error> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .with_kind(ErrorKind::Network)?;
    // A rebind must not wait out the last listener's TIME_WAIT connections.
    socket
        .set_reuse_address(true)
        .with_kind(ErrorKind::Network)?;
    socket
        .bind_device(Some(device.as_bytes()))
        .with_kind(ErrorKind::Network)?;
    socket.set_nonblocking(true).with_kind(ErrorKind::Network)?;
    socket
        .bind(&SocketAddrV4::new(addr, port).into())
        .with_kind(ErrorKind::Network)?;
    socket.listen(128).with_kind(ErrorKind::Network)?;
    TcpListener::from_std(socket.into()).with_kind(ErrorKind::Network)
}

/// Serves DNS-over-TCP clients until `shutdown`.
async fn serve_tcp(
    listener: TcpListener,
    injector: Arc<DnsInjector>,
    upstream: SocketAddr,
    shutdown: CancellationToken,
) {
    let slots = Arc::new(Semaphore::new(TCP_MAX_CLIENTS));
    // Dropping the set on shutdown aborts every client.
    let mut clients = JoinSet::new();
    loop {
        let accepted = tokio::select! {
            _ = shutdown.cancelled() => return,
            Some(_) = clients.join_next(), if !clients.is_empty() => continue,
            accepted = listener.accept() => accepted,
        };
        let (stream, peer) = match accepted {
            Ok(accepted) => accepted,
            Err(e) => {
                tracing::warn!("dns-inject TCP accept failed: {e}");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let Ok(slot) = slots.clone().try_acquire_owned() else {
            continue;
        };
        let injector = injector.clone();
        clients.spawn(async move {
            let _slot = slot;
            if let Err(e) = serve_tcp_client(stream, peer.ip(), &injector, upstream).await {
                tracing::debug!("dns-inject TCP client {peer}: {e}");
            }
        });
    }
}

/// Answers UPDATEs itself and relays every other message to `upstream`.
async fn serve_tcp_client(
    mut client: TcpStream,
    src: IpAddr,
    injector: &DnsInjector,
    upstream: SocketAddr,
) -> std::io::Result<()> {
    let mut dnsmasq: Option<TcpStream> = None;
    loop {
        let Some(request) = with_timeout(read_frame(&mut client)).await? else {
            return Ok(());
        };
        let response = if is_update(&request) {
            injector
                .answer_update(src, &request, true)
                .await
                .map_err(|e| std::io::Error::other(e.to_string()))?
        } else {
            relay(&mut dnsmasq, upstream, &request).await?
        };
        with_timeout(write_frame(&mut client, &response)).await?;
    }
}

/// One exchange with `upstream`, reusing the open connection. A reused one
/// that fails is replaced once.
async fn relay(
    conn: &mut Option<TcpStream>,
    upstream: SocketAddr,
    request: &[u8],
) -> std::io::Result<Vec<u8>> {
    let reused = conn.is_some();
    match relay_once(conn, upstream, request).await {
        Err(_) if reused => relay_once(conn, upstream, request).await,
        result => result,
    }
}

async fn relay_once(
    conn: &mut Option<TcpStream>,
    upstream: SocketAddr,
    request: &[u8],
) -> std::io::Result<Vec<u8>> {
    let mut stream = match conn.take() {
        Some(stream) => stream,
        None => with_timeout(TcpStream::connect(upstream)).await?,
    };
    with_timeout(write_frame(&mut stream, request)).await?;
    let response = with_timeout(read_frame(&mut stream))
        .await?
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::UnexpectedEof))?;
    *conn = Some(stream);
    Ok(response)
}

async fn with_timeout<T>(
    fut: impl std::future::Future<Output = std::io::Result<T>>,
) -> std::io::Result<T> {
    tokio::time::timeout(TCP_IDLE_TIMEOUT, fut)
        .await
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))?
}

/// A DNS message's opcode is UPDATE.
fn is_update(message: &[u8]) -> bool {
    message.get(2).is_some_and(|flags| (flags >> 3) & 0x0f == 5)
}

/// One length-prefixed DNS message; `None` on a clean close between messages.
async fn read_frame(stream: &mut TcpStream) -> std::io::Result<Option<Vec<u8>>> {
    let mut len = [0u8; 2];
    match stream.read_exact(&mut len).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let mut message = vec![0u8; usize::from(u16::from_be_bytes(len))];
    stream.read_exact(&mut message).await?;
    Ok(Some(message))
}

async fn write_frame(stream: &mut TcpStream, message: &[u8]) -> std::io::Result<()> {
    let len = u16::try_from(message.len())
        .map_err(|_| std::io::Error::other("DNS message exceeds 65535 bytes"))?;
    let mut framed = Vec::with_capacity(2 + message.len());
    framed.extend_from_slice(&len.to_be_bytes());
    framed.extend_from_slice(message);
    stream.write_all(&framed).await
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

    const TCP: UpdateAuth = UpdateAuth {
        tsig: false,
        tcp: true,
    };
    const SIGNED: UpdateAuth = UpdateAuth {
        tsig: true,
        tcp: false,
    };

    fn public(answer: PublicAnswer) -> PublicLookup {
        Arc::new(move |_| futures::future::ready(answer.clone()).boxed())
    }

    /// `policy` with public DNS answering nothing.
    async fn admit(
        dir: &SyncMutex<Directory>,
        src: IpAddr,
        updates: &[Record],
        auth: UpdateAuth,
    ) -> ResponseCode {
        policy(dir, &public(PublicAnswer::Absent), src, updates, auth).await
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
    async fn unsigned_tier_publishes_addresses_only() {
        let dir = directory();
        let src = IpAddr::V4(lan_ip(50));
        assert_eq!(
            admit(&dir, src, &[a_record("nas.example.com", lan_ip(99))], TCP).await,
            ResponseCode::NoError,
            "any address"
        );
        let cname = Record::from_rdata(
            fqdn("alias.example.com"),
            300,
            RData::CNAME(CNAME(fqdn("nas.example.com"))),
        );
        assert_eq!(
            admit(&dir, src, &[cname], TCP).await,
            ResponseCode::Refused,
            "unsigned tier is A/AAAA only"
        );
    }

    /// A name public DNS answers is claimable only where it points at the
    /// router's WAN. Special-use zones and deletes skip the lookup.
    #[tokio::test]
    async fn public_names_must_point_at_this_router() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let dir = directory();
        let wan = Ipv4Addr::new(203, 0, 113, 5);
        dir.mutate(|d| {
            d.wan.insert(IpAddr::V4(wan));
        });
        let src = IpAddr::V4(lan_ip(50));
        let publish = |name: &str| [a_record(name, lan_ip(50))];
        let run = |answer: PublicAnswer, updates: Vec<Record>| {
            let dir = &dir;
            async move { policy(dir, &public(answer), src, &updates, TCP).await }
        };

        assert_eq!(
            run(
                PublicAnswer::Addrs(vec![IpAddr::V4(Ipv4Addr::new(142, 250, 0, 1))]),
                publish("www.google.com").to_vec()
            )
            .await,
            ResponseCode::Refused,
            "a name that resolves elsewhere cannot be hijacked"
        );
        assert_eq!(
            run(
                PublicAnswer::Addrs(vec![IpAddr::V4(wan), IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))]),
                publish("mixed.example.com").to_vec()
            )
            .await,
            ResponseCode::Refused,
            "every public address must be the router's"
        );
        assert_eq!(
            run(
                PublicAnswer::Failed("timeout".into()),
                publish("unknown.example.com").to_vec()
            )
            .await,
            ResponseCode::Refused,
            "an unanswered lookup refuses"
        );
        assert_eq!(
            run(
                PublicAnswer::Addrs(vec![IpAddr::V4(wan)]),
                publish("home.example.com").to_vec()
            )
            .await,
            ResponseCode::NoError,
            "a name already routed to this router"
        );
        assert_eq!(
            run(PublicAnswer::Absent, publish("nextcloud.server").to_vec()).await,
            ResponseCode::NoError,
            "a name with no public records"
        );

        let asked = Arc::new(AtomicUsize::new(0));
        let counting: PublicLookup = {
            let asked = asked.clone();
            Arc::new(move |_| {
                asked.fetch_add(1, Ordering::SeqCst);
                futures::future::ready(PublicAnswer::Failed("offline".into())).boxed()
            })
        };
        for name in ["nas.local", "nas.home.arpa", "nas.internal"] {
            assert_eq!(
                policy(&dir, &counting, src, &publish(name), TCP).await,
                ResponseCode::NoError,
                "{name} never resolves publicly"
            );
        }
        let mut delete = Record::update0(fqdn("www.google.com"), 0, RecordType::ANY);
        delete.dns_class = DNSClass::ANY;
        assert_eq!(
            policy(&dir, &counting, src, &[delete], TCP).await,
            ResponseCode::NoError
        );
        assert_eq!(asked.load(Ordering::SeqCst), 0);
    }

    /// An unsigned UDP source is unproven, even one naming a WireGuard peer.
    #[tokio::test]
    async fn unsigned_udp_is_refused() {
        let dir = directory();
        for host in [50, 200] {
            assert_eq!(
                admit(
                    &dir,
                    IpAddr::V4(lan_ip(host)),
                    &[a_record("nas.example.com", lan_ip(host))],
                    UpdateAuth::default()
                )
                .await,
                ResponseCode::Refused
            );
        }
        assert!(dir.peek(|d| d.owners.is_empty()));
    }

    /// A TCP client's UPDATE is answered here with `tcp` set; its queries
    /// reach the upstream unchanged.
    #[tokio::test]
    async fn tcp_client_updates_locally_and_relays_queries() {
        use hickory_server::proto::op::update_message::append;
        use hickory_server::proto::op::{Message, Query};
        use hickory_server::proto::rr::RecordSet;

        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        let canned = b"upstream-answer".to_vec();
        let upstream_reply = canned.clone();
        let upstream_task = tokio::spawn(async move {
            let (mut conn, _) = upstream.accept().await.unwrap();
            let mut seen = Vec::new();
            while let Some(request) = read_frame(&mut conn).await.unwrap() {
                seen.push(request);
                write_frame(&mut conn, &upstream_reply).await.unwrap();
            }
            seen
        });

        let seen_auth = Arc::new(Mutex::new(None));
        let record_auth = seen_auth.clone();
        let injector = DnsInjector::new(
            Vec::new(),
            |_| true,
            |_| None,
            |_| {},
            move |_, _, auth| {
                *record_auth.lock().unwrap() = Some(auth);
                async { ResponseCode::NoError }
            },
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let shutdown = CancellationToken::new();
        let server = tokio::spawn(serve_tcp(
            listener,
            injector.clone(),
            upstream_addr,
            shutdown.clone(),
        ));

        let mut client = TcpStream::connect(addr).await.unwrap();
        let mut query = Message::query();
        query.add_query(Query::query(fqdn("other.example.com"), RecordType::A));
        let query = query.to_vec().unwrap();
        write_frame(&mut client, &query).await.unwrap();
        assert_eq!(read_frame(&mut client).await.unwrap().unwrap(), canned);

        let mut rrset = RecordSet::new(fqdn("nas.example.com"), RecordType::A, 0);
        rrset.insert(a_record("nas.example.com", lan_ip(50)), 0);
        let update = append(rrset, fqdn("example.com"), false, false);
        write_frame(&mut client, &update.to_vec().unwrap())
            .await
            .unwrap();
        let reply = Message::from_vec(&read_frame(&mut client).await.unwrap().unwrap()).unwrap();
        assert_eq!(reply.metadata.id, update.metadata.id);
        assert_eq!(reply.metadata.response_code, ResponseCode::NoError);
        assert_eq!(injector.list().len(), 1);
        assert_eq!(
            *seen_auth.lock().unwrap(),
            Some(UpdateAuth {
                tsig: false,
                tcp: true
            })
        );

        drop(client);
        assert_eq!(
            upstream_task.await.unwrap(),
            vec![query],
            "only the query reached the upstream"
        );
        shutdown.cancel();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn signed_tier_allows_any_rdata_and_more_types() {
        let dir = directory();
        let src = IpAddr::V4(lan_ip(200));
        assert_eq!(
            admit(&dir, src, &[a_record("svc.example.com", lan_ip(7))], SIGNED).await,
            ResponseCode::NoError,
            "signed: any rdata"
        );
        let cname = Record::from_rdata(
            fqdn("alias.example.com"),
            300,
            RData::CNAME(CNAME(fqdn("svc.example.com"))),
        );
        assert_eq!(
            admit(&dir, src, &[cname], SIGNED).await,
            ResponseCode::NoError
        );
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
            admit(&dir, src, &[srv], SIGNED).await,
            ResponseCode::Refused,
            "even signed injection is limited to the admin-path types"
        );
    }

    #[tokio::test]
    async fn unknown_source_is_refused() {
        let dir = directory();
        assert_eq!(
            admit(
                &dir,
                IpAddr::V4(lan_ip(9)),
                &[a_record("nas.example.com", lan_ip(9))],
                TCP
            )
            .await,
            ResponseCode::Refused
        );
    }

    #[tokio::test]
    async fn lan_names_are_reserved() {
        let dir = directory();
        assert_eq!(
            admit(
                &dir,
                IpAddr::V4(lan_ip(50)),
                &[a_record("nas.lan", lan_ip(50))],
                TCP
            )
            .await,
            ResponseCode::Refused,
            "dnsmasq is authoritative for lan."
        );
        assert_eq!(
            admit(
                &dir,
                IpAddr::V4(lan_ip(200)),
                &[a_record("nas.lan", lan_ip(7))],
                SIGNED
            )
            .await,
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
            admit(&dir, first, &[a_record("nas.example.com", lan_ip(50))], TCP).await,
            ResponseCode::NoError
        );
        assert_eq!(
            admit(
                &dir,
                second,
                &[a_record("nas.example.com", lan_ip(51))],
                TCP
            )
            .await,
            ResponseCode::Refused,
            "a held name refuses a different identity"
        );
        assert_eq!(
            admit(&dir, first, &[a_record("nas.example.com", lan_ip(50))], TCP).await,
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
            admit(&dir, second, &[delete("nas.example.com")], TCP).await,
            ResponseCode::Refused
        );
        assert_eq!(
            admit(&dir, first, &[delete("nas.example.com")], TCP).await,
            ResponseCode::NoError
        );
        assert_eq!(
            admit(
                &dir,
                second,
                &[a_record("nas.example.com", lan_ip(51))],
                TCP
            )
            .await,
            ResponseCode::NoError,
            "released name is claimable again"
        );
    }

    #[tokio::test]
    async fn refused_message_claims_nothing() {
        let dir = directory();
        let src = IpAddr::V4(lan_ip(50));
        // Second record invalid (unsigned CNAME), so the whole message
        // refuses — and the valid first record must not have claimed its name.
        let updates = [
            a_record("good.example.com", lan_ip(50)),
            Record::from_rdata(
                fqdn("bad.example.com"),
                300,
                RData::CNAME(CNAME(fqdn("good.example.com"))),
            ),
        ];
        assert_eq!(admit(&dir, src, &updates, TCP).await, ResponseCode::Refused);
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

    /// A record lives as long as its owner holds the address it published
    /// from, wherever the record points.
    #[tokio::test]
    async fn sweep_follows_the_publishing_address() {
        let mac = "AA:BB:CC:DD:EE:FF";
        let mut d = Directory::default();
        let name = LowerName::from(&fqdn("nas.example.com"));
        d.owners
            .insert((name.clone(), RecordType::A), Owner::Mac(mac.into()));
        let records = vec![injected("nas.example.com", lan_ip(99), lan_ip(50))];
        let holds = HashMap::from([(mac.to_string(), lan_ip(50).to_string())]);
        assert!(sweep_owners(&mut d, &snapshot_with(&[mac]), &holds, &[], &records).is_empty());
        let moved = HashMap::from([(mac.to_string(), lan_ip(99).to_string())]);
        assert_eq!(
            sweep_owners(&mut d, &snapshot_with(&[mac]), &moved, &[], &records),
            vec![(name, RecordType::A)],
            "holding the target address is not holding the source"
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
