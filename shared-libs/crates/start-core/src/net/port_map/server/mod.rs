//! Reusable server-side PCP (RFC 6887 + the HOSTNAME and PORT_SET extensions).
//!
//! Security model: a MAP forces the target to the *requesting* peer's own
//! address, so a peer can only forward to itself; authorization is delegated to
//! [`GatewayBackend::is_known_client`].

pub mod igd;

use std::future::Future;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4};
use std::sync::Arc;

use crate::net::port_map::pcp::capability::encode_start9_capability_option;
use crate::net::port_map::pcp::hostname::{
    RESULT_UNSUPP_HOSTNAME, encode_hostname_option, parse_hostname_options,
};
use crate::net::port_map::pcp::portset::{PortSet, encode_port_set_option, parse_port_set_options};
use crate::tunnel::forward::sni::SniDemux;

/// Standard PCP server port (RFC 6887).
pub const PCP_PORT: u16 = 5351;
const PCP_VERSION: u8 = 2;
const OPCODE_ANNOUNCE: u8 = 0;
const OPCODE_MAP: u8 = 1;
const RESPONSE_BIT: u8 = 0x80;
const MAP_REQUEST_LEN: usize = 60;
const MAP_RESPONSE_LEN: usize = 60;
const HEADER_LEN: usize = 24;
/// Cap the lease we grant; the client re-asserts well within this.
const MAX_LIFETIME_SECONDS: u32 = 3600;

// PCP result codes (RFC 6887 §7.4).
const SUCCESS: u8 = 0;
const UNSUPP_VERSION: u8 = 1;
const NOT_AUTHORIZED: u8 = 2;
const MALFORMED_REQUEST: u8 = 3;
const UNSUPP_OPCODE: u8 = 4;
const MALFORMED_OPTION: u8 = 6;
const NO_RESOURCES: u8 = 8;
const CANNOT_PROVIDE_EXTERNAL: u8 = 11;

/// PCP protocol field value for TCP (the only transport the SNI demux handles).
const PROTO_TCP: u8 = 6;
/// Largest PORT_SET range we will grant in one MAP (RFC 7753 lets us grant
/// fewer than requested; the client skips the range if it can't get them all).
const MAX_PORT_SET: u16 = 1024;

/// Per-gateway I/O and forward backend for the shared PCP server.
pub trait GatewayBackend: Send + Sync {
    /// Create or refresh a forward of `count` contiguous ports from `source`
    /// (the external address) to `target`, on behalf of `peer`. `lifetime` is
    /// the granted lease in seconds for a PCP mapping (renewed by the client
    /// before it lapses), or `None` for a permanent forward (manual / UPnP).
    /// `Err(code)` is the UPnP/IGD error code (e.g. 718 ConflictInMappingEntry);
    /// PCP maps any error to NO_RESOURCES.
    fn add_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        count: u16,
        peer: Ipv4Addr,
        lifetime: Option<u32>,
    ) -> impl Future<Output = Result<(), u16>> + Send;

    /// Remove the peer's forward to `(peer, internal_port)`, if any (PCP
    /// identifies a mapping by its target).
    fn remove_forward(&self, peer: Ipv4Addr, internal_port: u16)
    -> impl Future<Output = ()> + Send;

    /// Remove the forward at external address `source` if owned by `peer` (UPnP
    /// IGD identifies a mapping by its external port). Returns whether a
    /// peer-owned forward was removed; `false` means "no such mapping", reported
    /// without revealing other peers' mappings.
    fn remove_forward_by_source(
        &self,
        source: SocketAddrV4,
        peer: Ipv4Addr,
    ) -> impl Future<Output = bool> + Send;

    /// The external (WAN) IPv4 the gateway routes `peer`'s egress out of, or
    /// `None` if unknown.
    fn external_ipv4(&self, peer: Ipv4Addr) -> impl Future<Output = Option<Ipv4Addr>> + Send;

    /// Whether `peer` is a client this gateway will create mappings for.
    fn is_known_client(&self, peer: Ipv4Addr) -> impl Future<Output = bool> + Send;

    /// Whether `gua` is a global IPv6 address this gateway has delegated to a
    /// client — the authorization check for a v6 pinhole. Default: no v6 support.
    fn is_known_gua(&self, _gua: Ipv6Addr) -> impl Future<Output = bool> + Send {
        async { false }
    }

    /// Open or refresh a firewall pinhole for `count` contiguous ports at
    /// `[gua]:external_port`, delivered to `[gua]:internal_port` on the same GUA.
    /// `internal_port == external_port` is a pure pinhole (no NAT); a different
    /// value is a port-only DNAT (e.g. 80→443). `lifetime` is the granted PCP
    /// lease in seconds, or `None` for a permanent pinhole (manual).
    /// `Err(code)` maps to NO_RESOURCES. Default: v6 unsupported.
    fn add_pinhole(
        &self,
        _gua: Ipv6Addr,
        _external_port: u16,
        _internal_port: u16,
        _count: u16,
        _lifetime: Option<u32>,
    ) -> impl Future<Output = Result<(), u16>> + Send {
        async { Err(0) }
    }

    /// Remove the pinhole at `[gua]:external_port`, if owned by `gua`. Default: no-op.
    fn remove_pinhole(
        &self,
        _gua: Ipv6Addr,
        _external_port: u16,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// The SNI demultiplexer used for HOSTNAME-bound shared-port mappings.
    fn sni(&self) -> &Arc<SniDemux>;

    /// Register SNI-demuxed hostname routes on `source` (the shared external
    /// address) to `target`, owned by `target`. `lifetime` is `None` for a
    /// permanent (DB-backed) binding. Default impl is dataplane-only; the tunnel
    /// overrides it to also persist the routes.
    fn add_sni_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
        lifetime: Option<u32>,
    ) -> impl Future<Output = Result<(), u8>> + Send {
        async move {
            self.sni()
                .register(*source.ip(), source.port(), hostnames, target, lifetime)
        }
    }

    /// Remove the SNI routes for `hostnames` on `source` owned by `target`.
    fn remove_sni_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        hostnames: &[String],
    ) -> impl Future<Output = ()> + Send {
        async move {
            self.sni()
                .unregister(*source.ip(), source.port(), hostnames, target);
        }
    }
}

fn ipv4_mapped(ip: Ipv4Addr) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[10] = 0xff;
    out[11] = 0xff;
    out[12..16].copy_from_slice(&ip.octets());
    out
}

/// Header-only result-code response, for version/opcode errors raised before
/// the request body is trusted.
fn error_response(opcode: u8, result: u8, epoch: u32) -> Vec<u8> {
    let mut r = vec![0u8; HEADER_LEN];
    r[0] = PCP_VERSION;
    r[1] = RESPONSE_BIT | (opcode & 0x7f);
    r[3] = result;
    r[8..12].copy_from_slice(&epoch.to_be_bytes());
    r
}

/// An ANNOUNCE response carrying the Start9 capability marker, so a client can
/// confirm this gateway speaks the HOSTNAME extension before emitting it.
fn announce_response(epoch: u32) -> Vec<u8> {
    let mut r = vec![0u8; HEADER_LEN];
    r[0] = PCP_VERSION;
    r[1] = RESPONSE_BIT | OPCODE_ANNOUNCE;
    r[3] = SUCCESS;
    r[8..12].copy_from_slice(&epoch.to_be_bytes());
    encode_start9_capability_option(&mut r);
    r
}

/// A MAP response, echoing the request's nonce/protocol/internal port.
fn map_response(
    result: u8,
    req: &[u8],
    internal_port: u16,
    external_port: u16,
    external_ip: Ipv4Addr,
    lifetime: u32,
    epoch: u32,
) -> Vec<u8> {
    let mut r = vec![0u8; MAP_RESPONSE_LEN];
    r[0] = PCP_VERSION;
    r[1] = RESPONSE_BIT | OPCODE_MAP;
    r[3] = result;
    r[4..8].copy_from_slice(&lifetime.to_be_bytes());
    r[8..12].copy_from_slice(&epoch.to_be_bytes());
    r[24..36].copy_from_slice(&req[24..36]);
    r[36] = req[36];
    r[40..42].copy_from_slice(&internal_port.to_be_bytes());
    r[42..44].copy_from_slice(&external_port.to_be_bytes());
    r[44..60].copy_from_slice(&ipv4_mapped(external_ip));
    r
}

/// A MAP response for a v6 pinhole: identical to [`map_response`] but the
/// external-address field carries the full GUA rather than a v4-mapped address.
fn map_response6(
    result: u8,
    req: &[u8],
    internal_port: u16,
    external_port: u16,
    gua: Ipv6Addr,
    lifetime: u32,
    epoch: u32,
) -> Vec<u8> {
    let mut r = map_response(
        result,
        req,
        internal_port,
        external_port,
        Ipv4Addr::UNSPECIFIED,
        lifetime,
        epoch,
    );
    r[44..60].copy_from_slice(&gua.octets());
    r
}

/// A MAP response echoing the granted HOSTNAME options. The base response is
/// 32-bit aligned, so the appended options stay aligned.
fn map_response_with_hostnames(
    result: u8,
    req: &[u8],
    internal_port: u16,
    external_port: u16,
    external_ip: Ipv4Addr,
    lifetime: u32,
    epoch: u32,
    hostnames: &[String],
) -> Vec<u8> {
    let mut r = map_response(
        result,
        req,
        internal_port,
        external_port,
        external_ip,
        lifetime,
        epoch,
    );
    for name in hostnames {
        encode_hostname_option(&mut r, name);
    }
    r
}

/// A MAP response echoing the granted PORT_SET (RFC 7753): the opcode's
/// external port is the first port of the range; the option carries its size.
fn map_response_with_port_set(
    result: u8,
    req: &[u8],
    internal_port: u16,
    external_port: u16,
    external_ip: Ipv4Addr,
    lifetime: u32,
    epoch: u32,
    granted: u16,
) -> Vec<u8> {
    let mut r = map_response(
        result,
        req,
        internal_port,
        external_port,
        external_ip,
        lifetime,
        epoch,
    );
    encode_port_set_option(
        &mut r,
        &PortSet {
            size: granted,
            first_internal_port: internal_port,
            parity: false,
        },
    );
    r
}

/// Handle one PCP datagram from `peer`, returning the response bytes to send
/// back (or `None` to stay silent). `epoch` is the server's seconds-since-start.
pub async fn handle<B: GatewayBackend + ?Sized>(
    backend: &B,
    peer: Ipv4Addr,
    req: &[u8],
    epoch: u32,
) -> Option<Vec<u8>> {
    if req.len() < HEADER_LEN {
        return None;
    }
    let opcode = req[1] & 0x7f;
    if req[1] & RESPONSE_BIT != 0 {
        return None;
    }
    if req[0] != PCP_VERSION {
        return Some(error_response(opcode, UNSUPP_VERSION, epoch));
    }
    // Answer ANNOUNCE for any peer (the marker only reveals "I speak HOSTNAME");
    // it must precede the MAP-only check, which would otherwise reject opcode 0.
    if opcode == OPCODE_ANNOUNCE {
        tracing::debug!("PCP ANNOUNCE from {peer}: replying with Start9 capability marker");
        return Some(announce_response(epoch));
    }
    if opcode != OPCODE_MAP {
        return Some(error_response(opcode, UNSUPP_OPCODE, epoch));
    }
    if req.len() < MAP_REQUEST_LEN {
        return Some(error_response(opcode, MALFORMED_REQUEST, epoch));
    }

    if !backend.is_known_client(peer).await {
        return Some(map_response(
            NOT_AUTHORIZED,
            req,
            0,
            0,
            Ipv4Addr::UNSPECIFIED,
            0,
            epoch,
        ));
    }

    let lifetime = u32::from_be_bytes([req[4], req[5], req[6], req[7]]);
    let internal_port = u16::from_be_bytes([req[40], req[41]]);
    let suggested_external_port = u16::from_be_bytes([req[42], req[43]]);

    let Some(external_ip) = backend.external_ipv4(peer).await else {
        return Some(map_response(
            CANNOT_PROVIDE_EXTERNAL,
            req,
            internal_port,
            0,
            Ipv4Addr::UNSPECIFIED,
            0,
            epoch,
        ));
    };
    let external_port = if suggested_external_port != 0 {
        suggested_external_port
    } else {
        internal_port
    };

    if internal_port == 0 {
        return Some(map_response(
            MALFORMED_REQUEST,
            req,
            internal_port,
            external_port,
            external_ip,
            0,
            epoch,
        ));
    }

    // HOSTNAME options mean a SNI-demuxed binding on a shared external port,
    // handled by the SNI demux dataplane rather than a forward.
    let hostnames = match parse_hostname_options(req.get(MAP_REQUEST_LEN..).unwrap_or(&[])) {
        Ok(h) => h,
        Err(()) => {
            return Some(map_response(
                MALFORMED_OPTION,
                req,
                internal_port,
                external_port,
                external_ip,
                0,
                epoch,
            ));
        }
    };
    if !hostnames.is_empty() {
        if req[36] != PROTO_TCP {
            return Some(map_response(
                RESULT_UNSUPP_HOSTNAME,
                req,
                internal_port,
                external_port,
                external_ip,
                0,
                epoch,
            ));
        }
        // Force the route target to the requesting peer's own address.
        let target = SocketAddrV4::new(peer, internal_port);
        if lifetime == 0 {
            backend
                .remove_sni_forward(
                    SocketAddrV4::new(external_ip, external_port),
                    target,
                    &hostnames,
                )
                .await;
            return Some(map_response_with_hostnames(
                SUCCESS,
                req,
                internal_port,
                external_port,
                external_ip,
                0,
                epoch,
                &hostnames,
            ));
        }
        let granted = lifetime.min(MAX_LIFETIME_SECONDS);
        return match backend
            .add_sni_forward(
                SocketAddrV4::new(external_ip, external_port),
                target,
                &hostnames,
                Some(granted),
            )
            .await
        {
            Ok(()) => Some(map_response_with_hostnames(
                SUCCESS,
                req,
                internal_port,
                external_port,
                external_ip,
                granted,
                epoch,
                &hostnames,
            )),
            Err(code) => Some(map_response(
                code,
                req,
                internal_port,
                external_port,
                external_ip,
                0,
                epoch,
            )),
        };
    }

    // PCP PORT_SET extension (RFC 7753): map a contiguous range in one request.
    let port_set = match parse_port_set_options(req.get(MAP_REQUEST_LEN..).unwrap_or(&[])) {
        Ok(ps) => ps,
        Err(()) => {
            return Some(map_response(
                MALFORMED_OPTION,
                req,
                internal_port,
                external_port,
                external_ip,
                0,
                epoch,
            ));
        }
    };
    if let Some(ps) = port_set {
        if ps.size == 0 {
            return Some(map_response(
                MALFORMED_OPTION,
                req,
                internal_port,
                external_port,
                external_ip,
                0,
                epoch,
            ));
        }
        if ps.size > 1 {
            let granted = ps.size.min(MAX_PORT_SET);
            let source = SocketAddrV4::new(external_ip, external_port);
            let target = SocketAddrV4::new(peer, internal_port);
            if lifetime == 0 {
                backend.remove_forward(peer, internal_port).await;
                return Some(map_response_with_port_set(
                    SUCCESS,
                    req,
                    internal_port,
                    external_port,
                    external_ip,
                    0,
                    epoch,
                    granted,
                ));
            }
            let granted_lifetime = lifetime.min(MAX_LIFETIME_SECONDS);
            return match backend
                .add_forward(source, target, granted, peer, Some(granted_lifetime))
                .await
            {
                Ok(()) => Some(map_response_with_port_set(
                    SUCCESS,
                    req,
                    internal_port,
                    external_port,
                    external_ip,
                    granted_lifetime,
                    epoch,
                    granted,
                )),
                Err(_) => Some(map_response(
                    NO_RESOURCES,
                    req,
                    internal_port,
                    external_port,
                    external_ip,
                    0,
                    epoch,
                )),
            };
        }
    }

    // Lifetime 0 deletes the mapping (RFC 6887 §15).
    if lifetime == 0 {
        backend.remove_forward(peer, internal_port).await;
        return Some(map_response(
            SUCCESS,
            req,
            internal_port,
            external_port,
            external_ip,
            0,
            epoch,
        ));
    }

    // Secure: force the target to the requesting peer's own address.
    let source = SocketAddrV4::new(external_ip, external_port);
    let target = SocketAddrV4::new(peer, internal_port);
    let granted = lifetime.min(MAX_LIFETIME_SECONDS);
    match backend
        .add_forward(source, target, 1, peer, Some(granted))
        .await
    {
        Ok(()) => Some(map_response(
            SUCCESS,
            req,
            internal_port,
            external_port,
            external_ip,
            granted,
            epoch,
        )),
        // The external port is taken by another mapping; the client may retry.
        Err(_) => Some(map_response(
            NO_RESOURCES,
            req,
            internal_port,
            external_port,
            external_ip,
            0,
            epoch,
        )),
    }
}

/// Handle one PCP datagram received on the v6 socket from `peer` (the sender's
/// own GUA). A MAP opens a firewall pinhole for that GUA — the target is forced
/// to the source, so a client can only expose itself. `internal_port` and the
/// suggested external port may differ (a port-DNAT, e.g. the 80→443 redirect).
/// SNI/PORT_SET options don't apply to v6 pinholes and are ignored.
pub async fn handle6<B: GatewayBackend + ?Sized>(
    backend: &B,
    peer: Ipv6Addr,
    req: &[u8],
    epoch: u32,
) -> Option<Vec<u8>> {
    if req.len() < HEADER_LEN {
        return None;
    }
    let opcode = req[1] & 0x7f;
    if req[1] & RESPONSE_BIT != 0 {
        return None;
    }
    if req[0] != PCP_VERSION {
        return Some(error_response(opcode, UNSUPP_VERSION, epoch));
    }
    if opcode == OPCODE_ANNOUNCE {
        tracing::debug!("PCP ANNOUNCE from {peer}: replying with Start9 capability marker");
        return Some(announce_response(epoch));
    }
    if opcode != OPCODE_MAP {
        return Some(error_response(opcode, UNSUPP_OPCODE, epoch));
    }
    if req.len() < MAP_REQUEST_LEN {
        return Some(error_response(opcode, MALFORMED_REQUEST, epoch));
    }

    if !backend.is_known_gua(peer).await {
        return Some(map_response6(
            NOT_AUTHORIZED,
            req,
            0,
            0,
            Ipv6Addr::UNSPECIFIED,
            0,
            epoch,
        ));
    }

    let lifetime = u32::from_be_bytes([req[4], req[5], req[6], req[7]]);
    let internal_port = u16::from_be_bytes([req[40], req[41]]);
    let suggested_external_port = u16::from_be_bytes([req[42], req[43]]);
    let external_port = if suggested_external_port != 0 {
        suggested_external_port
    } else {
        internal_port
    };

    if internal_port == 0 {
        return Some(map_response6(
            MALFORMED_REQUEST,
            req,
            internal_port,
            external_port,
            peer,
            0,
            epoch,
        ));
    }

    // Lifetime 0 deletes the pinhole (RFC 6887 §15).
    if lifetime == 0 {
        backend.remove_pinhole(peer, external_port).await;
        return Some(map_response6(
            SUCCESS,
            req,
            internal_port,
            external_port,
            peer,
            0,
            epoch,
        ));
    }

    let granted = lifetime.min(MAX_LIFETIME_SECONDS);
    match backend
        .add_pinhole(peer, external_port, internal_port, 1, Some(granted))
        .await
    {
        Ok(()) => Some(map_response6(
            SUCCESS,
            req,
            internal_port,
            external_port,
            peer,
            granted,
            epoch,
        )),
        Err(_) => Some(map_response6(
            NO_RESOURCES,
            req,
            internal_port,
            external_port,
            peer,
            0,
            epoch,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map_request(nonce: [u8; 12], lifetime: u32, internal: u16, external: u16) -> Vec<u8> {
        let mut r = vec![0u8; MAP_REQUEST_LEN];
        r[0] = PCP_VERSION;
        r[1] = OPCODE_MAP;
        r[4..8].copy_from_slice(&lifetime.to_be_bytes());
        r[24..36].copy_from_slice(&nonce);
        r[36] = 6; // TCP
        r[40..42].copy_from_slice(&internal.to_be_bytes());
        r[42..44].copy_from_slice(&external.to_be_bytes());
        r
    }

    #[test]
    fn map_response_echoes_nonce_and_encodes_external() {
        let nonce = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let req = map_request(nonce, 3600, 8443, 443);
        let resp = map_response(
            SUCCESS,
            &req,
            8443,
            443,
            Ipv4Addr::new(203, 0, 113, 7),
            3600,
            42,
        );
        assert_eq!(resp.len(), MAP_RESPONSE_LEN);
        assert_eq!(resp[0], PCP_VERSION);
        assert_eq!(resp[1], RESPONSE_BIT | OPCODE_MAP);
        assert_eq!(resp[3], SUCCESS);
        assert_eq!(
            u32::from_be_bytes([resp[4], resp[5], resp[6], resp[7]]),
            3600
        );
        assert_eq!(
            u32::from_be_bytes([resp[8], resp[9], resp[10], resp[11]]),
            42
        );
        assert_eq!(&resp[24..36], &nonce);
        assert_eq!(resp[36], 6);
        assert_eq!(u16::from_be_bytes([resp[40], resp[41]]), 8443);
        assert_eq!(u16::from_be_bytes([resp[42], resp[43]]), 443);
        assert_eq!(&resp[44..60], &ipv4_mapped(Ipv4Addr::new(203, 0, 113, 7)));
    }

    #[test]
    fn ipv4_mapped_is_rfc_format() {
        assert_eq!(
            ipv4_mapped(Ipv4Addr::new(192, 0, 2, 1)),
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 192, 0, 2, 1]
        );
    }

    #[test]
    fn error_response_carries_code() {
        let r = error_response(OPCODE_MAP, UNSUPP_VERSION, 7);
        assert_eq!(r.len(), HEADER_LEN);
        assert_eq!(r[0], PCP_VERSION);
        assert_eq!(r[1], RESPONSE_BIT | OPCODE_MAP);
        assert_eq!(r[3], UNSUPP_VERSION);
    }

    #[test]
    fn announce_response_carries_marker() {
        use crate::net::port_map::pcp::capability::has_start9_capability;
        let r = announce_response(42);
        assert_eq!(r.len(), HEADER_LEN + 8);
        assert_eq!(r[0], PCP_VERSION);
        assert_eq!(r[1], RESPONSE_BIT | OPCODE_ANNOUNCE);
        assert_eq!(r[3], SUCCESS);
        assert_eq!(u32::from_be_bytes([r[8], r[9], r[10], r[11]]), 42);
        assert!(has_start9_capability(&r[HEADER_LEN..]));
    }

    struct Stub(Arc<SniDemux>);
    impl GatewayBackend for Stub {
        fn add_forward(
            &self,
            _: SocketAddrV4,
            _: SocketAddrV4,
            _: u16,
            _: Ipv4Addr,
            _: Option<u32>,
        ) -> impl Future<Output = Result<(), u16>> + Send {
            async { Ok(()) }
        }
        fn remove_forward(&self, _: Ipv4Addr, _: u16) -> impl Future<Output = ()> + Send {
            async {}
        }
        fn remove_forward_by_source(
            &self,
            _: SocketAddrV4,
            _: Ipv4Addr,
        ) -> impl Future<Output = bool> + Send {
            async { false }
        }
        fn external_ipv4(&self, _: Ipv4Addr) -> impl Future<Output = Option<Ipv4Addr>> + Send {
            async { Some(Ipv4Addr::new(203, 0, 113, 1)) }
        }
        fn is_known_client(&self, _: Ipv4Addr) -> impl Future<Output = bool> + Send {
            async { false }
        }
        fn sni(&self) -> &Arc<SniDemux> {
            &self.0
        }
    }

    // ANNOUNCE is answered with the marker for ANY peer (is_known_client false),
    // and is NOT swallowed by the MAP-only path as UNSUPP_OPCODE.
    #[tokio::test]
    async fn handle_announce_returns_marker_unauthed() {
        use crate::net::port_map::pcp::capability::has_start9_capability;
        let stub = Stub(SniDemux::new());
        let mut req = vec![0u8; HEADER_LEN];
        req[0] = PCP_VERSION;
        req[1] = OPCODE_ANNOUNCE;
        let resp = handle(&stub, Ipv4Addr::new(10, 59, 0, 2), &req, 7)
            .await
            .expect("ANNOUNCE answered");
        assert_eq!(resp[1], RESPONSE_BIT | OPCODE_ANNOUNCE);
        assert_eq!(resp[3], SUCCESS);
        assert!(has_start9_capability(&resp[HEADER_LEN..]));
    }

    #[tokio::test]
    async fn handle_rejects_unknown_opcode() {
        let stub = Stub(SniDemux::new());
        let mut req = vec![0u8; HEADER_LEN];
        req[0] = PCP_VERSION;
        req[1] = 3; // not ANNOUNCE(0) nor MAP(1)
        let resp = handle(&stub, Ipv4Addr::LOCALHOST, &req, 0).await.unwrap();
        assert_eq!(resp[3], UNSUPP_OPCODE);
    }

    struct RecordingStub {
        sni: Arc<SniDemux>,
        forward_lifetime: std::sync::Mutex<Option<Option<u32>>>,
    }
    impl GatewayBackend for RecordingStub {
        fn add_forward(
            &self,
            _: SocketAddrV4,
            _: SocketAddrV4,
            _: u16,
            _: Ipv4Addr,
            lifetime: Option<u32>,
        ) -> impl Future<Output = Result<(), u16>> + Send {
            *self.forward_lifetime.lock().unwrap() = Some(lifetime);
            async { Ok(()) }
        }
        fn remove_forward(&self, _: Ipv4Addr, _: u16) -> impl Future<Output = ()> + Send {
            async {}
        }
        fn remove_forward_by_source(
            &self,
            _: SocketAddrV4,
            _: Ipv4Addr,
        ) -> impl Future<Output = bool> + Send {
            async { false }
        }
        fn external_ipv4(&self, _: Ipv4Addr) -> impl Future<Output = Option<Ipv4Addr>> + Send {
            async { Some(Ipv4Addr::new(203, 0, 113, 1)) }
        }
        fn is_known_client(&self, _: Ipv4Addr) -> impl Future<Output = bool> + Send {
            async { true }
        }
        fn sni(&self) -> &Arc<SniDemux> {
            &self.sni
        }
    }

    // A MAP asking for more than the server cap is granted the capped lease, and
    // that SAME capped value is handed to the forward backend (so the tunnel
    // stamps its lease with the real grant, not the client's larger request).
    #[tokio::test]
    async fn map_grants_and_forwards_capped_lifetime() {
        let stub = RecordingStub {
            sni: SniDemux::new(),
            forward_lifetime: std::sync::Mutex::new(None),
        };
        let req = map_request([1u8; 12], 7200, 8443, 443); // 7200 > MAX 3600
        let resp = handle(&stub, Ipv4Addr::new(10, 59, 0, 2), &req, 5)
            .await
            .expect("answered");
        assert_eq!(resp[3], SUCCESS);
        assert_eq!(
            u32::from_be_bytes([resp[4], resp[5], resp[6], resp[7]]),
            MAX_LIFETIME_SECONDS
        );
        assert_eq!(
            *stub.forward_lifetime.lock().unwrap(),
            Some(Some(MAX_LIFETIME_SECONDS))
        );
    }

    // A lifetime-0 MAP is a delete: it removes rather than adds, so the backend
    // forward — and thus any lease — is never stamped.
    #[tokio::test]
    async fn map_lifetime_zero_does_not_add() {
        let stub = RecordingStub {
            sni: SniDemux::new(),
            forward_lifetime: std::sync::Mutex::new(None),
        };
        let req = map_request([2u8; 12], 0, 8443, 443);
        let resp = handle(&stub, Ipv4Addr::new(10, 59, 0, 2), &req, 5)
            .await
            .expect("answered");
        assert_eq!(resp[3], SUCCESS);
        assert_eq!(*stub.forward_lifetime.lock().unwrap(), None);
    }

    struct V6Stub {
        sni: Arc<SniDemux>,
        known: bool,
        pinholes: std::sync::Mutex<Vec<(Ipv6Addr, u16, u16, u16)>>,
        removed: std::sync::Mutex<Vec<(Ipv6Addr, u16)>>,
    }
    impl V6Stub {
        fn new(known: bool) -> Self {
            Self {
                sni: SniDemux::new(),
                known,
                pinholes: std::sync::Mutex::new(Vec::new()),
                removed: std::sync::Mutex::new(Vec::new()),
            }
        }
    }
    impl GatewayBackend for V6Stub {
        fn add_forward(
            &self,
            _: SocketAddrV4,
            _: SocketAddrV4,
            _: u16,
            _: Ipv4Addr,
            _: Option<u32>,
        ) -> impl Future<Output = Result<(), u16>> + Send {
            async { Ok(()) }
        }
        fn remove_forward(&self, _: Ipv4Addr, _: u16) -> impl Future<Output = ()> + Send {
            async {}
        }
        fn remove_forward_by_source(
            &self,
            _: SocketAddrV4,
            _: Ipv4Addr,
        ) -> impl Future<Output = bool> + Send {
            async { false }
        }
        fn external_ipv4(&self, _: Ipv4Addr) -> impl Future<Output = Option<Ipv4Addr>> + Send {
            async { None }
        }
        fn is_known_client(&self, _: Ipv4Addr) -> impl Future<Output = bool> + Send {
            async { false }
        }
        fn is_known_gua(&self, _: Ipv6Addr) -> impl Future<Output = bool> + Send {
            let known = self.known;
            async move { known }
        }
        fn add_pinhole(
            &self,
            gua: Ipv6Addr,
            external_port: u16,
            internal_port: u16,
            count: u16,
            _lifetime: Option<u32>,
        ) -> impl Future<Output = Result<(), u16>> + Send {
            self.pinholes
                .lock()
                .unwrap()
                .push((gua, external_port, internal_port, count));
            async { Ok(()) }
        }
        fn remove_pinhole(
            &self,
            gua: Ipv6Addr,
            external_port: u16,
        ) -> impl Future<Output = ()> + Send {
            self.removed.lock().unwrap().push((gua, external_port));
            async {}
        }
        fn sni(&self) -> &Arc<SniDemux> {
            &self.sni
        }
    }

    const TEST_GUA: Ipv6Addr = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0x50);

    // A pure pinhole: no suggested external port → external == internal, no NAT.
    #[tokio::test]
    async fn handle6_opens_pinhole_for_known_gua() {
        let stub = V6Stub::new(true);
        let req = map_request([9u8; 12], 3600, 8443, 0);
        let resp = handle6(&stub, TEST_GUA, &req, 1).await.expect("answered");
        assert_eq!(resp[3], SUCCESS);
        assert_eq!(u16::from_be_bytes([resp[40], resp[41]]), 8443); // internal
        assert_eq!(u16::from_be_bytes([resp[42], resp[43]]), 8443); // external
        assert_eq!(&resp[44..60], &TEST_GUA.octets()); // GUA, not v4-mapped
        assert_eq!(
            *stub.pinholes.lock().unwrap(),
            vec![(TEST_GUA, 8443, 8443, 1)]
        );
    }

    // A suggested external port different from internal → the 80→443 redirect.
    #[tokio::test]
    async fn handle6_honors_suggested_external_port() {
        let stub = V6Stub::new(true);
        let req = map_request([9u8; 12], 3600, 443, 80);
        let resp = handle6(&stub, TEST_GUA, &req, 1).await.expect("answered");
        assert_eq!(resp[3], SUCCESS);
        assert_eq!(u16::from_be_bytes([resp[40], resp[41]]), 443); // internal
        assert_eq!(u16::from_be_bytes([resp[42], resp[43]]), 80); // external
        assert_eq!(*stub.pinholes.lock().unwrap(), vec![(TEST_GUA, 80, 443, 1)]);
    }

    #[tokio::test]
    async fn handle6_rejects_unknown_gua() {
        let stub = V6Stub::new(false);
        let req = map_request([9u8; 12], 3600, 8443, 0);
        let resp = handle6(&stub, TEST_GUA, &req, 1).await.expect("answered");
        assert_eq!(resp[3], NOT_AUTHORIZED);
        assert!(stub.pinholes.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn handle6_lifetime_zero_removes() {
        let stub = V6Stub::new(true);
        let req = map_request([9u8; 12], 0, 443, 80);
        let resp = handle6(&stub, TEST_GUA, &req, 1).await.expect("answered");
        assert_eq!(resp[3], SUCCESS);
        assert_eq!(*stub.removed.lock().unwrap(), vec![(TEST_GUA, 80)]);
        assert!(stub.pinholes.lock().unwrap().is_empty());
    }
}
