//! Shared server-side RFC 2136 (DNS UPDATE) handling, used by both StartTunnel
//! (in this crate) and StartWRT's `startwrt-ctrld` (which imports this crate).
//!
//! [`DnsInjector`] is an in-memory store of injected DNS records with four
//! hooks: an authorizer per source IP, a TSIG key lookup (the per-device key
//! derived from its WireGuard PSK), a `pre_update` policy that sees an
//! UPDATE's records and its [`UpdateAuth`] before they mutate the store, and
//! an `on_change` notifier. [`InjectingHandler`] wraps a forwarding
//! `RequestHandler`: an injected-name `Query` is answered locally, an
//! authorized `Update` mutates the store, everything else is forwarded.
//!
//! UPDATE authentication is **TSIG** (RFC 8945), HMAC keyed off the device's
//! root-only WG PSK. The handler hands the verdict to `pre_update`, which
//! decides what an unsigned UPDATE may do.
//! TSIG proves the signer holds that PSK (no forgery) but not freshness: there's
//! no anti-replay state, so a captured signature replays within `TSIG_FUDGE`.
//! That's bounded to records the sending device may already mutate and is
//! idempotent (the client re-asserts every few minutes), so it's accepted as a
//! limitation rather than tracked. Manual CRUD via [`DnsInjector::upsert`] /
//! [`DnsInjector::delete`] is an admin path and bypasses both the authorizer and
//! TSIG.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use hickory_server::net::runtime::Time;
use hickory_server::net::xfer::Protocol;
use hickory_server::proto::op::{Header, HeaderCounts, Message, Metadata, OpCode, ResponseCode};
use hickory_server::proto::rr::rdata::{CNAME, TXT};
use hickory_server::proto::rr::{DNSClass, LowerName, Name, RData, Record, RecordType};
use hickory_server::server::{Request, RequestHandler, ResponseHandler, ResponseInfo};
use hickory_server::zone_handler::{Catalog, MessageResponseBuilder};

use crate::prelude::*;
use crate::util::sync::SyncMutex;

/// One injected record. Kept Rust-only; each gateway maps it to its own
/// serializable view for the API/UI.
#[derive(Clone, Debug, PartialEq)]
pub struct InjectedRecord {
    pub name: Name,
    pub rtype: RecordType,
    pub rdata: RData,
    pub ttl: u32,
    /// Device IP that injected this; unspecified for manual entries.
    pub source: IpAddr,
}

impl InjectedRecord {
    /// Render to the text form gateways persist and show; `source` is `None`
    /// for a manual record.
    pub fn to_parts(&self) -> (String, String, String, u32, Option<IpAddr>) {
        let source = if self.source.is_unspecified() {
            None
        } else {
            Some(self.source)
        };
        (
            self.name.to_utf8().trim_end_matches('.').to_string(),
            self.rtype.to_string(),
            self.rdata.to_string(),
            self.ttl,
            source,
        )
    }

    /// Parse the text form back into a record; pass `Ipv4Addr::UNSPECIFIED`
    /// as `source` for a manual record.
    pub fn from_parts(
        name: &str,
        rtype: &str,
        value: &str,
        ttl: u32,
        source: IpAddr,
    ) -> Result<Self, Error> {
        let mut n = Name::from_utf8(name).with_kind(ErrorKind::ParseUrl)?;
        n.set_fqdn(true);
        let (rtype, rdata) = parse_rdata(rtype, value)?;
        Ok(Self {
            name: n,
            rtype,
            rdata,
            ttl,
            source,
        })
    }
}

fn parse_rdata(rtype: &str, value: &str) -> Result<(RecordType, RData), Error> {
    let invalid =
        |what: &str| Error::new(eyre!("invalid {what}: {value}"), ErrorKind::InvalidRequest);
    Ok(match rtype.to_ascii_uppercase().as_str() {
        "A" => (
            RecordType::A,
            RData::A(
                value
                    .parse::<Ipv4Addr>()
                    .map_err(|_| invalid("A record"))?
                    .into(),
            ),
        ),
        "AAAA" => (
            RecordType::AAAA,
            RData::AAAA(
                value
                    .parse::<Ipv6Addr>()
                    .map_err(|_| invalid("AAAA record"))?
                    .into(),
            ),
        ),
        "CNAME" => (
            RecordType::CNAME,
            RData::CNAME(CNAME(
                Name::from_utf8(value).with_kind(ErrorKind::ParseUrl)?,
            )),
        ),
        "TXT" => (
            RecordType::TXT,
            RData::TXT(TXT::new(vec![value.to_string()])),
        ),
        other => {
            return Err(Error::new(
                eyre!("unsupported DNS record type: {other}"),
                ErrorKind::InvalidRequest,
            ));
        }
    })
}

/// What vouches for an UPDATE's source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UpdateAuth {
    /// A valid TSIG from the source's key.
    pub tsig: bool,
    /// Arrived over TCP, whose handshake proves the source address.
    pub tcp: bool,
}

type Authorizer = Box<dyn Fn(IpAddr) -> bool + Send + Sync>;
/// The per-device derived TSIG key for a source IP, or `None` if it isn't an
/// allowed DNS-injection device.
type KeyLookup = Box<dyn Fn(IpAddr) -> Option<[u8; 32]> + Send + Sync>;
/// Fired with a full snapshot after a mutation that changed the store.
type OnChange = Box<dyn Fn(Vec<InjectedRecord>) + Send + Sync>;
/// Runs on an authorized UPDATE's records before they mutate the store.
/// Anything but `NoError` refuses the whole message.
type PreUpdate = Box<dyn Fn(IpAddr, &[Record], UpdateAuth) -> ResponseCode + Send + Sync>;

pub struct DnsInjector {
    records: SyncMutex<BTreeMap<LowerName, Vec<InjectedRecord>>>,
    authorize: Authorizer,
    tsig_key: KeyLookup,
    on_change: OnChange,
    pre_update: PreUpdate,
}

impl DnsInjector {
    pub fn new(
        initial: Vec<InjectedRecord>,
        authorize: impl Fn(IpAddr) -> bool + Send + Sync + 'static,
        tsig_key: impl Fn(IpAddr) -> Option<[u8; 32]> + Send + Sync + 'static,
        on_change: impl Fn(Vec<InjectedRecord>) + Send + Sync + 'static,
        pre_update: impl Fn(IpAddr, &[Record], UpdateAuth) -> ResponseCode + Send + Sync + 'static,
    ) -> Arc<Self> {
        let mut records: BTreeMap<LowerName, Vec<InjectedRecord>> = BTreeMap::new();
        for r in initial {
            records.entry(LowerName::from(&r.name)).or_default().push(r);
        }
        Arc::new(Self {
            records: SyncMutex::new(records),
            authorize: Box::new(authorize),
            tsig_key: Box::new(tsig_key),
            on_change: Box::new(on_change),
            pre_update: Box::new(pre_update),
        })
    }

    /// Verify the RFC 8945 TSIG on a raw UPDATE request from `src`: rejects an
    /// absent/invalid/out-of-window signature, or a src with no injection key.
    /// The key is derived from the device's WireGuard PSK, which a sandboxed
    /// service can't read — so it can't forge a signature for the server's IP.
    fn verify_tsig(&self, src: IpAddr, request_bytes: &[u8]) -> bool {
        let Some(key) = (self.tsig_key)(src) else {
            return false;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        match super::tsig_signer(key).verify_message_byte(request_bytes, None, true) {
            Ok((_, _, valid)) => valid.contains(&now),
            Err(_) => false,
        }
    }

    /// Every injected record, in stable order.
    pub fn list(&self) -> Vec<InjectedRecord> {
        self.records
            .peek(|m| m.values().flatten().cloned().collect())
    }

    fn notify(&self) {
        (self.on_change)(self.list());
    }

    /// Manually add or replace a record (admin action; skips authorization).
    pub fn upsert(&self, record: InjectedRecord) {
        let changed = self.records.mutate(|m| {
            let v = m.entry(LowerName::from(&record.name)).or_default();
            if v.contains(&record) {
                return false;
            }
            v.retain(|r| !(r.rtype == record.rtype && r.rdata == record.rdata));
            v.push(record);
            true
        });
        if changed {
            self.notify();
        }
    }

    /// Manually delete records for a name (optionally a single type).
    pub fn delete(&self, name: &Name, rtype: Option<RecordType>) {
        let changed = self.records.mutate(|m| {
            let key = LowerName::from(name);
            match rtype {
                None => m.remove(&key).is_some(),
                Some(rt) => {
                    let Some(v) = m.get_mut(&key) else {
                        return false;
                    };
                    let before = v.len();
                    v.retain(|r| r.rtype != rt);
                    let changed = v.len() != before;
                    if v.is_empty() {
                        m.remove(&key);
                    }
                    changed
                }
            }
        });
        if changed {
            self.notify();
        }
    }

    fn lookup(&self, name: &LowerName, rtype: RecordType) -> Vec<Record> {
        self.records.peek(|m| {
            m.get(name)
                .into_iter()
                .flatten()
                .filter(|r| rtype == RecordType::ANY || r.rtype == rtype)
                .map(|r| Record::from_rdata(r.name.clone(), r.ttl, r.rdata.clone()))
                .collect()
        })
    }

    /// Whether any record (of any type) exists for `name`; distinguishes an
    /// authoritative NODATA from a name we don't serve.
    fn contains_name(&self, name: &LowerName) -> bool {
        self.records.peek(|m| m.contains_key(name))
    }

    /// Verifies and applies a raw UPDATE message; `msg` is its parse.
    fn update(&self, src: IpAddr, request: &[u8], msg: &Message, tcp: bool) -> ResponseCode {
        let auth = UpdateAuth {
            tsig: self.verify_tsig(src, request),
            tcp,
        };
        self.apply_update(src, &msg.authorities, auth)
    }

    /// The wire response to a raw UPDATE message from `src`.
    pub fn answer_update(&self, src: IpAddr, request: &[u8], tcp: bool) -> Result<Vec<u8>, Error> {
        let response = match Message::from_vec(request) {
            Ok(msg) => {
                let code = self.update(src, request, &msg, tcp);
                let mut response = Message::error_msg(msg.metadata.id, OpCode::Update, code);
                response.queries = msg.queries;
                response
            }
            Err(_) => Message::error_msg(
                request
                    .get(..2)
                    .map_or(0, |b| u16::from_be_bytes([b[0], b[1]])),
                OpCode::Update,
                ResponseCode::FormErr,
            ),
        };
        response
            .to_vec()
            .map_err(|e| Error::new(eyre!("encode DNS UPDATE response: {e}"), ErrorKind::Network))
    }

    /// Applies an UPDATE's records (RFC 2136 §2.5) once the authorizer and
    /// `pre_update` admit them.
    fn apply_update(&self, src: IpAddr, updates: &[Record], auth: UpdateAuth) -> ResponseCode {
        if !(self.authorize)(src) {
            return ResponseCode::Refused;
        }
        let code = (self.pre_update)(src, updates, auth);
        if code != ResponseCode::NoError {
            return code;
        }
        let changed = self.records.mutate(|m| {
            let mut changed = false;
            for rec in updates {
                let key = LowerName::from(&rec.name);
                match rec.dns_class {
                    // Add to an RRset.
                    DNSClass::IN => {
                        let record = InjectedRecord {
                            name: rec.name.clone(),
                            rtype: rec.record_type(),
                            rdata: rec.data.clone(),
                            ttl: rec.ttl,
                            source: src,
                        };
                        let v = m.entry(key).or_default();
                        if !v.contains(&record) {
                            v.retain(|r| !(r.rtype == record.rtype && r.rdata == record.rdata));
                            v.push(record);
                            changed = true;
                        }
                    }
                    // Delete an RRset (a whole type, or every type for the name).
                    DNSClass::ANY => {
                        if rec.record_type() == RecordType::ANY {
                            changed |= m.remove(&key).is_some();
                        } else if let Some(v) = m.get_mut(&key) {
                            let before = v.len();
                            v.retain(|r| r.rtype != rec.record_type());
                            changed |= v.len() != before;
                            if v.is_empty() {
                                m.remove(&key);
                            }
                        }
                    }
                    // Delete one specific record.
                    DNSClass::NONE => {
                        if let Some(v) = m.get_mut(&key) {
                            let rdata = rec.data.clone();
                            let before = v.len();
                            v.retain(|r| r.rdata != rdata);
                            changed |= v.len() != before;
                            if v.is_empty() {
                                m.remove(&key);
                            }
                        }
                    }
                    _ => {}
                }
            }
            changed
        });
        if changed {
            self.notify();
        }
        ResponseCode::NoError
    }
}

/// Wraps a forwarding `Catalog` with injected-record answering and authorized
/// RFC 2136 UPDATE handling. Non-injected queries fall through to the `Catalog`
/// (a `ForwardZoneHandler` pointed at upstream / dnsmasq).
pub struct InjectingHandler {
    injector: Arc<DnsInjector>,
    forwarder: Catalog,
}

impl InjectingHandler {
    pub fn new(injector: Arc<DnsInjector>, forwarder: Catalog) -> Self {
        Self {
            injector,
            forwarder,
        }
    }
}

fn header_with_code(request: &Request, code: ResponseCode) -> Metadata {
    let mut header = Metadata::response_from_request(&request.metadata);
    header.recursion_available = true;
    header.response_code = code;
    header
}

fn fallback_info(header: Metadata) -> ResponseInfo {
    Header {
        metadata: header,
        counts: HeaderCounts::default(),
    }
    .into()
}

#[async_trait::async_trait]
impl RequestHandler for InjectingHandler {
    async fn handle_request<R: ResponseHandler, T: Time>(
        &self,
        request: &Request,
        mut response_handle: R,
    ) -> ResponseInfo {
        match request.metadata.op_code {
            OpCode::Update => {
                // MessageRequest hides the authority section the update RRs
                // live in.
                let code = match Message::from_vec(request.as_slice()) {
                    Ok(msg) => self.injector.update(
                        request.src().ip(),
                        request.as_slice(),
                        &msg,
                        request.protocol() != Protocol::Udp,
                    ),
                    Err(_) => ResponseCode::FormErr,
                };
                let header = header_with_code(request, code);
                response_handle
                    .send_response(
                        MessageResponseBuilder::from_message_request(&*request).build(
                            header,
                            [],
                            [],
                            [],
                            [],
                        ),
                    )
                    .await
                    .unwrap_or_else(|_| fallback_info(header))
            }
            OpCode::Query => {
                let req = request.request_info().ok();
                let answers = req
                    .as_ref()
                    .map(|req| {
                        self.injector
                            .lookup(req.query.name(), req.query.query_type())
                    })
                    .unwrap_or_default();
                // Authoritative for any injected name: serve records or NODATA.
                // Forwarding a held name's missing-type query lets upstream NXDOMAIN
                // poison it (RFC 8020) — e.g. an AAAA probe for an A-only domain.
                let known = req
                    .as_ref()
                    .is_some_and(|req| self.injector.contains_name(req.query.name()));
                if answers.is_empty() && !known {
                    return self
                        .forwarder
                        .handle_request::<R, T>(request, response_handle)
                        .await;
                }
                let header = header_with_code(request, ResponseCode::NoError);
                response_handle
                    .send_response(
                        MessageResponseBuilder::from_message_request(&*request).build(
                            header,
                            &answers,
                            [],
                            [],
                            [],
                        ),
                    )
                    .await
                    .unwrap_or_else(|_| fallback_info(header))
            }
            _ => {
                self.forwarder
                    .handle_request::<R, T>(request, response_handle)
                    .await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGNED: UpdateAuth = UpdateAuth {
        tsig: true,
        tcp: false,
    };

    fn injector() -> Arc<DnsInjector> {
        DnsInjector::new(
            Vec::new(),
            |_| true,
            |_| None,
            |_| {},
            |_, _, _| ResponseCode::NoError,
        )
    }

    /// The policy StartTunnel installs: a valid TSIG or nothing.
    fn tsig_required(_: IpAddr, _: &[Record], auth: UpdateAuth) -> ResponseCode {
        if auth.tsig {
            ResponseCode::NoError
        } else {
            ResponseCode::Refused
        }
    }

    fn fqdn(s: &str) -> LowerName {
        let mut n = Name::from_utf8(s).unwrap();
        n.set_fqdn(true);
        LowerName::from(&n)
    }

    /// An A-only injected name must answer NODATA (not forward) for an AAAA
    /// query: the name is held, so `contains_name` is true even though the
    /// queried type is absent. Forwarding would let upstream NXDOMAIN poison it.
    // tokio runtime required: DnsInjector's SyncMutex spawns a lock watchdog.
    #[tokio::test]
    async fn held_name_is_nodata_not_forwarded() {
        let inj = injector();
        inj.upsert(
            InjectedRecord::from_parts(
                "scrunge.goop",
                "A",
                "10.59.0.2",
                300,
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            )
            .unwrap(),
        );
        let q = fqdn("scrunge.goop");
        assert_eq!(inj.lookup(&q, RecordType::A).len(), 1, "A query resolves");
        assert!(
            inj.lookup(&q, RecordType::AAAA).is_empty(),
            "no AAAA record"
        );
        assert!(inj.contains_name(&q), "held name -> handler answers NODATA");
        assert!(
            !inj.contains_name(&fqdn("nope.goop")),
            "unknown name -> handler forwards"
        );
    }

    #[test]
    fn tsig_key_derivation_is_deterministic() {
        let a = super::super::derive_tsig_key(&[1u8; 32]);
        assert_eq!(
            a,
            super::super::derive_tsig_key(&[1u8; 32]),
            "same psk -> same key"
        );
        assert_ne!(
            a,
            super::super::derive_tsig_key(&[2u8; 32]),
            "different psk -> different key"
        );
    }

    // A signed UPDATE built the way the server signs it must verify; an unsigned
    // one, a wrong key, or a src with no key must be rejected.
    #[tokio::test]
    async fn tsig_gates_updates() {
        use hickory_server::proto::op::update_message::append;
        use hickory_server::proto::rr::RecordSet;
        use hickory_server::proto::rr::rdata::A;

        let build = |sign_key: Option<[u8; 32]>| {
            let mut name = Name::from_utf8("host.example.com").unwrap();
            name.set_fqdn(true);
            let mut rrset = RecordSet::new(name.clone(), RecordType::A, 0);
            rrset.insert(
                Record::from_rdata(
                    name.clone(),
                    300,
                    RData::A(A::from(Ipv4Addr::new(10, 59, 0, 2))),
                ),
                0,
            );
            let mut msg = append(rrset, name.base_name(), false, false);
            if let Some(key) = sign_key {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                msg.finalize(&super::super::tsig_signer(key), now).unwrap();
            }
            msg.to_vec().unwrap()
        };

        let src = IpAddr::V4(Ipv4Addr::new(10, 59, 0, 2));
        let key_a = super::super::derive_tsig_key(&[7u8; 32]);
        let key_b = super::super::derive_tsig_key(&[9u8; 32]);
        let signed = build(Some(key_a));
        let unsigned = build(None);

        let inj = DnsInjector::new(
            Vec::new(),
            |_| true,
            move |ip| (ip == src).then_some(key_a),
            |_| {},
            tsig_required,
        );
        assert!(
            inj.verify_tsig(src, &signed),
            "valid TSIG from the right key accepted"
        );
        assert!(!inj.verify_tsig(src, &unsigned), "unsigned UPDATE rejected");
        assert!(
            !inj.verify_tsig(IpAddr::V4(Ipv4Addr::new(10, 59, 0, 9)), &signed),
            "no key for src rejected"
        );

        let inj_b = DnsInjector::new(
            Vec::new(),
            |_| true,
            move |_| Some(key_b),
            |_| {},
            tsig_required,
        );
        assert!(!inj_b.verify_tsig(src, &signed), "wrong key rejected");
    }

    fn a_record(name: &str, addr: [u8; 4]) -> Record {
        use hickory_server::proto::rr::rdata::A;
        let mut n = Name::from_utf8(name).unwrap();
        n.set_fqdn(true);
        Record::from_rdata(n, 300, RData::A(A::from(Ipv4Addr::from(addr))))
    }

    /// An UPDATE that changes nothing does not fire `on_change`.
    #[tokio::test]
    async fn noop_updates_do_not_notify() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let fired = Arc::new(AtomicUsize::new(0));
        let count = fired.clone();
        let inj = DnsInjector::new(
            Vec::new(),
            |_| true,
            |_| None,
            move |_| {
                count.fetch_add(1, Ordering::SeqCst);
            },
            |_, _, _| ResponseCode::NoError,
        );
        let src = IpAddr::V4(Ipv4Addr::new(10, 59, 0, 2));
        let rec = a_record("host.example.com", [10, 59, 0, 2]);

        assert_eq!(
            inj.apply_update(src, std::slice::from_ref(&rec), UpdateAuth::default()),
            ResponseCode::NoError
        );
        assert_eq!(fired.load(Ordering::SeqCst), 1, "first assert notifies");
        assert_eq!(
            inj.apply_update(src, std::slice::from_ref(&rec), UpdateAuth::default()),
            ResponseCode::NoError
        );
        assert_eq!(fired.load(Ordering::SeqCst), 1, "re-assert is a no-op");

        let other = a_record("host.example.com", [10, 59, 0, 3]);
        assert_eq!(
            inj.apply_update(src, &[other], UpdateAuth::default()),
            ResponseCode::NoError
        );
        assert_eq!(fired.load(Ordering::SeqCst), 2, "a real change notifies");

        let mut name = Name::from_utf8("host.example.com").unwrap();
        name.set_fqdn(true);
        inj.delete(&name, Some(RecordType::AAAA));
        assert_eq!(
            fired.load(Ordering::SeqCst),
            2,
            "deleting nothing is a no-op"
        );
        inj.delete(&name, None);
        assert_eq!(fired.load(Ordering::SeqCst), 3, "a real delete notifies");
    }

    /// The response carries the request's id, zone and verdict; garbage gets
    /// FORMERR under the id it starts with.
    #[tokio::test]
    async fn answer_update_echoes_id_and_zone() {
        use hickory_server::proto::op::update_message::append;
        use hickory_server::proto::rr::RecordSet;

        let inj = DnsInjector::new(
            Vec::new(),
            |_| true,
            |_| None,
            |_| {},
            |_, _, auth: UpdateAuth| {
                if auth.tcp {
                    ResponseCode::NoError
                } else {
                    ResponseCode::Refused
                }
            },
        );
        let mut name = Name::from_utf8("host.example.com").unwrap();
        name.set_fqdn(true);
        let mut rrset = RecordSet::new(name.clone(), RecordType::A, 0);
        rrset.insert(a_record("host.example.com", [10, 59, 0, 2]), 0);
        let request = append(rrset, name.base_name(), false, false);
        let bytes = request.to_vec().unwrap();
        let src = IpAddr::V4(Ipv4Addr::new(10, 59, 0, 2));

        let reply = Message::from_vec(&inj.answer_update(src, &bytes, true).unwrap()).unwrap();
        assert_eq!(reply.metadata.id, request.metadata.id);
        assert_eq!(reply.metadata.op_code, OpCode::Update);
        assert_eq!(reply.metadata.response_code, ResponseCode::NoError);
        assert_eq!(reply.queries, request.queries, "zone section echoed");
        assert_eq!(inj.list().len(), 1);

        let reply = Message::from_vec(&inj.answer_update(src, &bytes, false).unwrap()).unwrap();
        assert_eq!(reply.metadata.response_code, ResponseCode::Refused);

        let reply =
            Message::from_vec(&inj.answer_update(src, &[0x12, 0x34, 0xff], true).unwrap()).unwrap();
        assert_eq!(reply.metadata.id, 0x1234);
        assert_eq!(reply.metadata.response_code, ResponseCode::FormErr);
    }

    /// A `pre_update` refusal reaches the client as that code and leaves the
    /// store untouched and un-notified.
    #[tokio::test]
    async fn pre_update_refusal_leaves_store_untouched() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let fired = Arc::new(AtomicUsize::new(0));
        let count = fired.clone();
        let inj = DnsInjector::new(
            Vec::new(),
            |_| true,
            |_| None,
            move |_| {
                count.fetch_add(1, Ordering::SeqCst);
            },
            |_, _, _| ResponseCode::Refused,
        );
        let src = IpAddr::V4(Ipv4Addr::new(10, 59, 0, 2));
        let rec = a_record("host.example.com", [10, 59, 0, 2]);
        assert_eq!(inj.apply_update(src, &[rec], SIGNED), ResponseCode::Refused);
        assert!(inj.list().is_empty(), "store untouched");
        assert_eq!(fired.load(Ordering::SeqCst), 0, "no notify");
    }

    /// With StartTunnel's policy installed, every src × signed combination
    /// yields the response code `handle_request` used to.
    #[tokio::test]
    async fn tunnel_policy_matches_old_tsig_refusal() {
        let src = IpAddr::V4(Ipv4Addr::new(10, 59, 0, 2));
        let inj = DnsInjector::new(
            Vec::new(),
            move |ip| ip == src,
            |_| None,
            |_| {},
            tsig_required,
        );
        let rec = a_record("host.example.com", [10, 59, 0, 2]);
        assert_eq!(
            inj.apply_update(src, std::slice::from_ref(&rec), UpdateAuth::default()),
            ResponseCode::Refused,
            "authorized but unsigned -> refused"
        );
        assert!(inj.list().is_empty());
        assert_eq!(
            inj.apply_update(
                IpAddr::V4(Ipv4Addr::new(10, 59, 0, 9)),
                std::slice::from_ref(&rec),
                SIGNED
            ),
            ResponseCode::Refused,
            "unauthorized -> refused even when signed"
        );
        assert_eq!(
            inj.apply_update(src, std::slice::from_ref(&rec), SIGNED),
            ResponseCode::NoError,
            "authorized and signed -> applied"
        );
        assert_eq!(inj.list().len(), 1);
    }
}
