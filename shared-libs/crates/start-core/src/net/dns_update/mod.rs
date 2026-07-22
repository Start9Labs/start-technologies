//! Best-effort RFC 2136 (DNS UPDATE) client. When a private domain is enabled
//! on a gateway, push an `A` record to the gateway's DNS server so other LAN
//! devices can resolve it; withdraw it when the domain is disabled.
//!
//! The gateway requires a TSIG signature (RFC 8945) keyed off the WireGuard PSK
//! it shares with us, so each UPDATE is signed with a key derived from the PSK
//! NetworkManager holds for that gateway's interface and bound to our address on
//! it. A gateway that doesn't accept RFC 2136 just means the domain only
//! resolves on StartOS's own resolver, as before; a gateway with no PSK (e.g. a
//! plain router) gets an unsigned, best-effort update.

pub mod rfc2136;

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hickory_server::proto::op::update_message::{append, delete_rrset};
use hickory_server::proto::op::{Message, ResponseCode};
use hickory_server::proto::rr::rdata::tsig::TsigAlgorithm;
use hickory_server::proto::rr::rdata::{A, AAAA};
use hickory_server::proto::rr::{Name, RData, Record, RecordSet, RecordType, TSigner};
use hkdf::Hkdf;
use imbl::OrdMap;
use imbl_value::InternedString;
use sha2::Sha256;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{interval, timeout};

use crate::GatewayId;
use crate::db::model::Database;
use crate::db::model::public::NetworkInterfaceInfo;
use crate::hostname::ServerHostname;
use crate::net::port_map::candidate_gateways;
use crate::net::utils::ipv6_is_link_local;
use crate::prelude::*;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::sync::Watch;

const DNS_PORT: u16 = 53;
const RECORD_TTL: u32 = 300;
const QUERY_TIMEOUT: Duration = Duration::from_secs(2);
/// Re-assert desired records so they survive a gateway DNS restart.
const REFRESH_INTERVAL: Duration = Duration::from_secs(180);

const TSIG_INFO: &[u8] = b"startos-dns-update-v1";
/// TSIG time window (seconds); both ends run NTP so 5 min is ample.
pub(crate) const TSIG_FUDGE: u16 = 300;

/// Fixed TSIG key name shared by signer (server) and verifier (gateway); a pure
/// key identifier, built identically on both sides.
pub(crate) fn tsig_key_name() -> Name {
    Name::from_ascii("startos-dns-update.").expect("static valid name")
}

/// Per-device TSIG HMAC key derived from the WireGuard PSK. Both sides derive it
/// identically; a sandboxed service can't read the root-only PSK, so it can't
/// forge a valid signature.
pub(crate) fn derive_tsig_key(psk: &[u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    Hkdf::<Sha256>::new(None, psk)
        .expand(TSIG_INFO, &mut out)
        .expect("32 <= 255*32 output bytes");
    out
}

/// HMAC-SHA256 TSIG signer/verifier for a derived key.
pub(crate) fn tsig_signer(key: [u8; 32]) -> TSigner {
    TSigner::new(
        key.to_vec(),
        TsigAlgorithm::HmacSha256,
        tsig_key_name(),
        TSIG_FUDGE,
    )
    .expect("HmacSha256 supported; static name valid")
}

/// (gateway this target belongs to, DNS server to update, our address on that
/// subnet / the A record value). The gateway id resolves the TSIG key.
type Target = (GatewayId, IpAddr, IpAddr);

/// Resolves a gateway's WireGuard PSK (async D-Bus call into NetworkManager), so
/// the controller can derive that gateway's TSIG signing key. `Ok(Some)` is a
/// real key, `Ok(None)` is a definitively keyless gateway (cacheable), and
/// `Err(())` is a transient lookup failure (NOT cached, so the next tick retries
/// instead of permanently downgrading an expected-signed gateway to unsigned).
type PskLookup = Arc<
    dyn Fn(GatewayId) -> Pin<Box<dyn Future<Output = Result<Option<[u8; 32]>, ()>> + Send>>
        + Send
        + Sync,
>;

enum Command {
    Add {
        fqdn: InternedString,
        gateways: BTreeSet<GatewayId>,
    },
    Gc {
        rm: BTreeSet<InternedString>,
    },
}

/// Mirrors [`crate::net::dns::DnsController`]'s private-domain API so the
/// net-service reconcile can drive both in lock-step.
#[derive(Clone)]
pub struct DnsUpdateController {
    req: mpsc::UnboundedSender<Command>,
}

impl DnsUpdateController {
    pub fn new(
        mut net_iface: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
        psk: PskLookup,
    ) -> Self {
        let (req, mut recv) = mpsc::unbounded_channel::<Command>();
        tokio::spawn(async move {
            let mut desired: BTreeMap<InternedString, BTreeSet<GatewayId>> = BTreeMap::new();
            // Currently-published targets, kept so we can withdraw stale ones.
            let mut active: BTreeMap<InternedString, BTreeSet<Target>> = BTreeMap::new();
            // Per-gateway TSIG signer (or `None` for keyless gateways), cleared
            // on a network change since a re-imported config can rotate the PSK.
            let mut signers: BTreeMap<GatewayId, Option<TSigner>> = BTreeMap::new();
            let mut ifaces = net_iface.read_and_mark_seen();
            let mut refresh = interval(REFRESH_INTERVAL);
            refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    cmd = recv.recv() => match cmd {
                        Some(Command::Add { fqdn, gateways }) => {
                            let changed = desired.get(&fqdn) != Some(&gateways);
                            desired.insert(fqdn.clone(), gateways);
                            if changed {
                                reconcile_one(&fqdn, &desired, &mut active, &net_iface, &ifaces, &psk, &mut signers).await;
                            }
                        }
                        Some(Command::Gc { rm }) => {
                            for fqdn in rm {
                                desired.remove(&fqdn);
                                reconcile_one(&fqdn, &desired, &mut active, &net_iface, &ifaces, &psk, &mut signers).await;
                            }
                        }
                        None => break,
                    },
                    _ = net_iface.changed() => {
                        ifaces = net_iface.read();
                        signers.clear();
                        for fqdn in desired.keys().cloned().collect::<Vec<_>>() {
                            reconcile_one(&fqdn, &desired, &mut active, &net_iface, &ifaces, &psk, &mut signers).await;
                        }
                    }
                    _ = refresh.tick() => {
                        for (fqdn, targets) in &active {
                            if let Ok(name) = fqdn_to_name(fqdn) {
                                let mut outcomes: BTreeMap<&GatewayId, bool> = BTreeMap::new();
                                for (gw, server, ip) in targets {
                                    if recently_failed(&net_iface, gw) {
                                        continue;
                                    }
                                    let signer = signer_for(gw, &psk, &mut signers).await;
                                    let ok = apply(&name, *server, *ip, signer.as_ref()).await;
                                    *outcomes.entry(gw).or_default() |= ok;
                                }
                                for (gw, ok) in outcomes {
                                    report(&net_iface, gw, ok);
                                }
                            }
                        }
                    }
                }
            }
        });
        Self { req }
    }

    pub fn add(&self, fqdn: InternedString, gateways: BTreeSet<GatewayId>) {
        self.req.send(Command::Add { fqdn, gateways }).ok();
    }

    pub fn gc(&self, rm: BTreeSet<InternedString>) {
        if !rm.is_empty() {
            self.req.send(Command::Gc { rm }).ok();
        }
    }
}

/// The live WireGuard gateways among `ifaces`. A `.local` record makes sense to
/// inject only here: over a plain LAN gateway a client resolves `.local` by mDNS
/// multicast, but a WireGuard client (e.g. on StartTunnel) can't — Android in
/// particular excludes VPN connections from mDNS — so the tunnel's resolver must
/// answer it instead.
fn wireguard_gateways(ifaces: &OrdMap<GatewayId, NetworkInterfaceInfo>) -> BTreeSet<GatewayId> {
    ifaces
        .iter()
        .filter(|(_, info)| info.is_wireguard())
        .map(|(gw, _)| gw.clone())
        .collect()
}

/// Continuously inject the server's mDNS `.local` name over every WireGuard
/// gateway — and only those — via RFC 2136, so clients on the tunnel (which
/// can't do mDNS over a VPN) still resolve `<hostname>.local` to the server's
/// address on that tunnel. The gateway's own per-device policy still applies: an
/// UPDATE to a gateway with DNS injection disabled is refused, and the `.local`
/// name falls back to a manual record there. Re-derives the target gateways on
/// every network change and the name on every hostname change.
pub fn spawn_server_mdns_injection(
    db: TypedPatchDb<Database>,
    mut net_iface: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    dns_update: DnsUpdateController,
) -> NonDetachingJoinHandle<()> {
    tokio::spawn(async move {
        let mut hostname_sub = db
            .subscribe(
                "/public/serverInfo/hostname"
                    .parse()
                    .expect("valid pointer"),
            )
            .await;
        // The name currently asserted as desired, so a hostname change withdraws
        // the old one instead of leaving it to be re-asserted forever.
        let mut current: Option<InternedString> = None;
        loop {
            // A failed hostname read just skips this pass; we still wait on both
            // signals below, so a network change is never missed while it's unset.
            match ServerHostname::load(db.peek().await.as_public().as_server_info()) {
                Ok(h) => {
                    let name = h.local_domain_name();
                    let gateways = net_iface.peek(wireguard_gateways);
                    if let Some(prev) = &current {
                        if *prev != name || gateways.is_empty() {
                            dns_update.gc(std::iter::once(prev.clone()).collect());
                            current = None;
                        }
                    }
                    if !gateways.is_empty() {
                        // Idempotent; also picks up a gateway added to / removed from the set.
                        dns_update.add(name.clone(), gateways);
                        current = Some(name);
                    }
                }
                Err(e) => {
                    tracing::error!("mDNS injection: could not read server hostname: {e}");
                    tracing::debug!("{e:?}");
                }
            }
            tokio::select! {
                _ = net_iface.changed() => {}
                _ = hostname_sub.recv() => {}
            }
        }
    })
    .into()
}

/// The (resolver, our-ip) pairs for a private domain on one gateway: one per
/// subnet, pointing at our address there (published as an A record for IPv4, an
/// AAAA for IPv6) and sent to a same-family resolver on that subnet. The caller
/// pairs each with its gateway id.
fn targets_for(info: &NetworkInterfaceInfo) -> Vec<(IpAddr, IpAddr)> {
    let Some(ip_info) = &info.ip_info else {
        return Vec::new();
    };
    let resolvers: Vec<IpAddr> = candidate_gateways(info)
        .into_iter()
        .map(|(g, _)| g)
        .collect();
    let mut out = Vec::new();
    for subnet in &ip_info.subnets {
        let our_ip = subnet.addr();
        // A link-local v6 address isn't usefully resolvable, so don't publish it.
        if let IpAddr::V6(v6) = our_ip {
            if ipv6_is_link_local(v6) {
                continue;
            }
        }
        // A same-family resolver on our subnet (the NM gateway).
        let server = resolvers
            .iter()
            .copied()
            .find(|r| r.is_ipv4() == our_ip.is_ipv4() && subnet.contains(r));
        if let Some(server) = server {
            out.push((server, our_ip));
        }
    }
    out
}

/// Resolve (and cache) a gateway's TSIG signer. `None` for a gateway with no
/// PSK, whose update then goes out unsigned (best-effort). A transient lookup
/// failure returns `None` for this attempt but is NOT cached, so the next
/// reconcile / refresh tick retries rather than wedging the gateway on unsigned.
async fn signer_for(
    gw: &GatewayId,
    psk: &PskLookup,
    cache: &mut BTreeMap<GatewayId, Option<TSigner>>,
) -> Option<TSigner> {
    if let Some(signer) = cache.get(gw) {
        return signer.clone();
    }
    match psk(gw.clone()).await {
        Ok(key) => {
            let signer = key.map(|key| tsig_signer(derive_tsig_key(&key)));
            cache.insert(gw.clone(), signer.clone());
            signer
        }
        Err(()) => None,
    }
}

/// Feed a round's UPDATE outcomes back as the gateway's `dns_update`
/// capability verdict: supported if any of the gateway's resolvers accepted
/// the update (a gateway can have a resolver per address family, and e.g.
/// StartTunnel only accepts v4-sourced updates). Aggregating per round keeps
/// a per-target report from flapping the verdict — which would rewrite the
/// watch, retrigger a reconcile, and spin. `set_verdict`'s trust windows then
/// make a repeat of the same verdict a no-op, so a steady state doesn't
/// retrigger the `net_iface` watch on every refresh tick.
/// A gateway whose last UPDATE failed inside the negative trust window — the
/// same dead-gateway skip the port-map client applies, so a refusing resolver
/// is retried on the verdict's staleness cadence, not every refresh tick.
fn recently_failed(
    interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    gw: &GatewayId,
) -> bool {
    let now = chrono::Utc::now();
    interfaces
        .read()
        .get(gw)
        .map_or(false, |i| i.dns_update.fresh(now) == Some(false))
}

fn report(
    interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    gw: &GatewayId,
    supported: bool,
) {
    let now = chrono::Utc::now();
    interfaces.send_if_modified(|m| {
        m.get_mut(gw).map_or(false, |info| {
            crate::net::port_map::set_verdict(&mut info.dns_update, supported, now)
        })
    });
}

async fn reconcile_one(
    fqdn: &InternedString,
    desired: &BTreeMap<InternedString, BTreeSet<GatewayId>>,
    active: &mut BTreeMap<InternedString, BTreeSet<Target>>,
    interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    ifaces: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    psk: &PskLookup,
    signers: &mut BTreeMap<GatewayId, Option<TSigner>>,
) {
    let Ok(name) = fqdn_to_name(fqdn) else {
        return;
    };
    let want: BTreeSet<Target> = desired
        .get(fqdn)
        .into_iter()
        .flatten()
        .filter_map(|gw| ifaces.get(gw).map(|info| (gw, info)))
        .flat_map(|(gw, info)| {
            targets_for(info)
                .into_iter()
                .map(move |(server, ip)| (gw.clone(), server, ip))
        })
        .collect();
    let had = active.remove(fqdn).unwrap_or_default();
    for (gw, server, ip) in had.difference(&want) {
        let signer = signer_for(gw, psk, signers).await;
        withdraw(&name, *server, *ip, signer.as_ref()).await;
    }
    let mut outcomes: BTreeMap<&GatewayId, bool> = BTreeMap::new();
    for (gw, server, ip) in &want {
        if recently_failed(interfaces, gw) {
            continue;
        }
        let signer = signer_for(gw, psk, signers).await;
        let ok = apply(&name, *server, *ip, signer.as_ref()).await;
        *outcomes.entry(gw).or_default() |= ok;
    }
    for (gw, ok) in outcomes {
        report(interfaces, gw, ok);
    }
    if !want.is_empty() {
        active.insert(fqdn.clone(), want);
    }
}

fn fqdn_to_name(fqdn: &str) -> Result<Name, Error> {
    let mut name = Name::from_utf8(fqdn).with_kind(ErrorKind::ParseUrl)?;
    name.set_fqdn(true);
    Ok(name)
}

/// Zone for an RFC 2136 update: the FQDN's parent is a valid origin, and our
/// own servers don't enforce it strictly.
fn zone_of(fqdn: &Name) -> Name {
    let base = fqdn.base_name();
    if base.is_root() { fqdn.clone() } else { base }
}

/// A record for an IPv4 address, AAAA for an IPv6 one.
fn record_type_for(ip: IpAddr) -> RecordType {
    match ip {
        IpAddr::V4(_) => RecordType::A,
        IpAddr::V6(_) => RecordType::AAAA,
    }
}

/// Whether both messages of the replace (delete + add) were accepted.
async fn apply(fqdn: &Name, server: IpAddr, ip: IpAddr, signer: Option<&TSigner>) -> bool {
    let zone = zone_of(fqdn);
    let rtype = record_type_for(ip);
    let rdata = match ip {
        IpAddr::V4(v4) => RData::A(A::from(v4)),
        IpAddr::V6(v6) => RData::AAAA(AAAA::from(v6)),
    };
    // Replace: drop any existing rrset of this type for the name, then add ours.
    let delete = delete_rrset(Record::update0(fqdn.clone(), 0, rtype), zone.clone(), false);
    let mut rrset = RecordSet::new(fqdn.clone(), rtype, 0);
    rrset.insert(Record::from_rdata(fqdn.clone(), RECORD_TTL, rdata), 0);
    let add = append(rrset, zone, false, false);
    for msg in [delete, add] {
        if let Err(e) = send(server, ip, &msg, signer).await {
            crate::dev_log!(debug, "RFC 2136 update of {fqdn} on {server} failed: {e}");
            return false;
        }
    }
    tracing::debug!("published {fqdn} -> {ip} via RFC 2136 on {server}");
    true
}

async fn withdraw(fqdn: &Name, server: IpAddr, ip: IpAddr, signer: Option<&TSigner>) {
    let msg = delete_rrset(
        Record::update0(fqdn.clone(), 0, record_type_for(ip)),
        zone_of(fqdn),
        false,
    );
    if let Err(e) = send(server, ip, &msg, signer).await {
        crate::dev_log!(debug, "RFC 2136 delete of {fqdn} on {server} failed: {e}");
    }
}

async fn send(
    server: IpAddr,
    local_ip: IpAddr,
    message: &Message,
    signer: Option<&TSigner>,
) -> Result<(), Error> {
    // Sign with the gateway's TSIG key (derived from the WG PSK) when we have
    // one; `finalize` mutates, so sign a clone.
    let bytes = match signer {
        Some(signer) => {
            let mut signed = message.clone();
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            signed
                .finalize(signer, now)
                .map_err(|e| Error::new(eyre!("TSIG sign DNS UPDATE: {e}"), ErrorKind::Network))?;
            signed.to_vec()
        }
        None => message.to_vec(),
    }
    .map_err(|e| Error::new(eyre!("encode DNS UPDATE: {e}"), ErrorKind::Network))?;
    // Bind to our address on the gateway so the server authorizes us by source IP.
    let socket = UdpSocket::bind(SocketAddr::new(local_ip, 0))
        .await
        .with_kind(ErrorKind::Network)?;
    socket
        .connect(SocketAddr::new(server, DNS_PORT))
        .await
        .with_kind(ErrorKind::Network)?;
    socket.send(&bytes).await.with_kind(ErrorKind::Network)?;
    let mut buf = [0u8; 1232];
    let n = timeout(QUERY_TIMEOUT, socket.recv(&mut buf))
        .await
        .map_err(|_| Error::new(eyre!("timed out"), ErrorKind::Network))?
        .with_kind(ErrorKind::Network)?;
    let resp = Message::from_vec(&buf[..n])
        .map_err(|e| Error::new(eyre!("decode DNS response: {e}"), ErrorKind::Network))?;
    match resp.metadata.response_code {
        // NXRRSet on a delete (nothing to remove) is fine.
        ResponseCode::NoError | ResponseCode::NXRRSet => Ok(()),
        other => Err(Error::new(
            eyre!("DNS UPDATE refused: {other}"),
            ErrorKind::Network,
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use imbl::OrdMap;
    use imbl_value::InternedString;

    use super::{recently_failed, wireguard_gateways};
    use crate::GatewayId;
    use crate::db::model::public::{
        CapabilityVerdict, IpInfo, NetworkInterfaceInfo, NetworkInterfaceType,
    };
    use crate::util::sync::Watch;

    fn iface(device_type: Option<NetworkInterfaceType>) -> NetworkInterfaceInfo {
        NetworkInterfaceInfo {
            ip_info: device_type.map(|device_type| {
                Arc::new(IpInfo {
                    device_type: Some(device_type),
                    ..Default::default()
                })
            }),
            ..Default::default()
        }
    }

    fn gw(id: &str) -> GatewayId {
        GatewayId::from(InternedString::intern(id))
    }

    #[test]
    fn only_wireguard_gateways_are_selected() {
        let ifaces: OrdMap<GatewayId, NetworkInterfaceInfo> = [
            (gw("wg0"), iface(Some(NetworkInterfaceType::Wireguard))),
            (gw("eth0"), iface(Some(NetworkInterfaceType::Ethernet))),
            (gw("wlan0"), iface(Some(NetworkInterfaceType::Wireless))),
            (gw("wg1"), iface(Some(NetworkInterfaceType::Wireguard))),
            // A gateway with no ip_info (down) is never WireGuard.
            (gw("down"), iface(None)),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            wireguard_gateways(&ifaces),
            [gw("wg0"), gw("wg1")].into_iter().collect()
        );
    }

    #[test]
    fn recent_failure_gates_retries() {
        let ifaces = Watch::new(OrdMap::from_iter([(
            gw("wg0"),
            iface(Some(NetworkInterfaceType::Wireguard)),
        )]));
        let set = |v: CapabilityVerdict| {
            ifaces.send_if_modified(|m| {
                m.get_mut(&gw("wg0"))
                    .map_or(false, |i: &mut NetworkInterfaceInfo| {
                        let changed = i.dns_update != v;
                        i.dns_update = v;
                        changed
                    })
            });
        };

        // Never probed: attempt.
        assert!(!recently_failed(&ifaces, &gw("wg0")));
        assert!(!recently_failed(&ifaces, &gw("missing")));
        // Fresh success: attempt.
        set(CapabilityVerdict::supported(true));
        assert!(!recently_failed(&ifaces, &gw("wg0")));
        // Fresh failure: skip.
        set(CapabilityVerdict::supported(false));
        assert!(recently_failed(&ifaces, &gw("wg0")));
        // Stale failure (past the 5-minute negative window): retry.
        set(CapabilityVerdict {
            supported: Some(false),
            at: Some(chrono::Utc::now() - chrono::TimeDelta::minutes(6)),
        });
        assert!(!recently_failed(&ifaces, &gw("wg0")));
    }

    #[test]
    fn no_wireguard_gateways_selects_nothing() {
        let ifaces: OrdMap<GatewayId, NetworkInterfaceInfo> =
            [(gw("eth0"), iface(Some(NetworkInterfaceType::Ethernet)))]
                .into_iter()
                .collect();
        assert!(wireguard_gateways(&ifaces).is_empty());
    }
}
