//! Reusable server-side UPnP IGD (WANIPConnection) protocol layer.
//!
//! Security model mirrors PCP: a peer can only forward to itself — the SOAP
//! body's `NewInternalClient` is ignored and the target is forced to the
//! requesting peer's own address. The endpoints are HTTP, so they also defend
//! against being driven from a victim's browser, on two fronts: every request
//! — control and static descriptions alike — must carry an IP-literal `Host`
//! ([`host_is_ip_literal`]), which defeats DNS rebinding (the only way a page
//! can *read* responses); and mutating actions must be named in the
//! `SOAPAction` header, which a cross-origin request can never carry, closing
//! the blind-write POST a page could otherwise aim directly at a guessed
//! gateway IP with no rebinding at all.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4};
use std::sync::Arc;

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::net::port_map::pcp::hostname::{
    RESULT_HOSTNAME_TAKEN, RESULT_UNSUPP_HOSTNAME, validate_hostname,
};
use crate::net::port_map::server::{GatewayBackend, MAX_LIFETIME_SECONDS, MappingEntry};

pub const SSDP_MULTICAST: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
pub const SSDP_PORT: u16 = 1900;
/// HTTP port serving the device description, SCPD, and SOAP control endpoint.
pub const IGD_HTTP_PORT: u16 = 49001;
/// Longest lease we grant a UPnP mapping; also what `NewLeaseDuration = 0`
/// ("permanent") means, per WANIPConnection:2's reading of 0 as one week.
pub const IGD_MAX_LEASE_SECONDS: u32 = 604_800;
pub const WANIP_SERVICE: &str = "urn:schemas-upnp-org:service:WANIPConnection:1";
/// Declared on the `WANDevice`. Carries no port-mapping actions, but a client
/// looking for an IGD identifies one by finding this service type in the device
/// description — miniupnpc's `UPNP_GetValidIGD` classifies a device without it
/// as `UPNP_UNKNOWN_DEVICE` and refuses to use it at all.
pub const WANCIF_SERVICE: &str = "urn:schemas-upnp-org:service:WANCommonInterfaceConfig:1";
pub const IGD_DEVICE: &str = "urn:schemas-upnp-org:device:InternetGatewayDevice:1";
pub const SERVER_HEADER: &str = "StartOS UPnP/1.1";
pub const ROOT_DESC_PATH: &str = "/rootDesc.xml";
pub const SCPD_PATH: &str = "/WANIPCn.xml";
pub const CIF_SCPD_PATH: &str = "/WANCfg.xml";
/// Both services share one control endpoint: actions are dispatched by name,
/// which is unambiguous across the two.
pub const CONTROL_PATH: &str = "/ctl/IPConn";
/// UPnP vendor action wire names for SNI hostname mappings.
pub const ADD_HOSTNAME_ACTION: &str = "X_START9_AddHostnameMapping";
pub const DELETE_HOSTNAME_ACTION: &str = "X_START9_DeleteHostnameMapping";

/// Minimal WANIPConnection SCPD. Clients (e.g. igd-next) read its `actionList`
/// to learn each action's input argument names before issuing a request.
pub const SCPD: &str = include_str!("igd_xml/scpd.xml");
/// Minimal WANCommonInterfaceConfig SCPD, served so the service declared in the
/// root description resolves rather than 404ing.
pub const CIF_SCPD: &str = include_str!("igd_xml/cif_scpd.xml");

/// Uptime reported by `GetStatusInfo`, measured from the first call — clients
/// use it only as a monotonic "how long has this connection been up".
fn uptime_seconds() -> u32 {
    static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    STARTED
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs()
        .try_into()
        .unwrap_or(u32::MAX)
}

/// Format a 16-byte slice as a stable, well-formed UUID/UDN string.
pub fn format_uuid(bytes: &[u8]) -> String {
    let mut b = [0u8; 16];
    for (i, slot) in b.iter_mut().enumerate() {
        *slot = bytes.get(i).copied().unwrap_or(0);
    }
    // We only need a stable, well-formed UDN, so the RFC 4122 variant/version
    // bits are left unset.
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0],
        b[1],
        b[2],
        b[3],
        b[4],
        b[5],
        b[6],
        b[7],
        b[8],
        b[9],
        b[10],
        b[11],
        b[12],
        b[13],
        b[14],
        b[15]
    )
}

/// Whether an SSDP `ST` (search target) matches this IGD.
pub fn st_matches(st: &str) -> bool {
    st == "ssdp:all"
        || st == "upnp:rootdevice"
        || st.contains("InternetGatewayDevice")
        || st.contains("WANIPConnection")
        || st.contains("WANConnectionDevice")
}

/// The SSDP M-SEARCH response advertising this IGD at `server_ip`.
pub fn ssdp_response(server_ip: Ipv4Addr, uuid: &str) -> String {
    let location = format!("http://{server_ip}:{IGD_HTTP_PORT}{ROOT_DESC_PATH}");
    format!(
        "HTTP/1.1 200 OK\r\n\
         CACHE-CONTROL: max-age=1800\r\n\
         EXT:\r\n\
         LOCATION: {location}\r\n\
         SERVER: {SERVER_HEADER}\r\n\
         ST: {IGD_DEVICE}\r\n\
         USN: uuid:{uuid}::{IGD_DEVICE}\r\n\
         \r\n"
    )
}

/// Extract a case-insensitive single-line header value from a raw HTTP message.
pub fn header_value(msg: &str, name: &str) -> Option<String> {
    let name = name.to_ascii_lowercase();
    msg.lines().find_map(|line| {
        let (k, v) = line.split_once(':')?;
        if k.trim().to_ascii_lowercase() == name {
            Some(v.trim().to_string())
        } else {
            None
        }
    })
}

/// The SOAP action being invoked, from the `SOAPAction` header (`"...#Action"`)
/// or, failing that, the first element under the SOAP `Body`.
fn soap_action(headers: &HeaderMap, body: &str) -> Option<String> {
    if let Some(h) = headers.get("SOAPAction").and_then(|v| v.to_str().ok()) {
        let h = h.trim().trim_matches('"');
        if let Some((_, action)) = h.rsplit_once('#') {
            return Some(action.to_string());
        }
    }
    let root = xmltree::Element::parse(body.as_bytes()).ok()?;
    let b = root.get_child("Body")?;
    b.children
        .iter()
        .find_map(|n| n.as_element().map(|e| e.name.clone()))
}

/// Read a `u16` argument by element name from anywhere in the SOAP body.
fn soap_u16(body: &str, arg: &str) -> Option<u16> {
    soap_arg(body, arg)
}

/// Read a `u32` argument by element name from anywhere in the SOAP body.
fn soap_u32(body: &str, arg: &str) -> Option<u32> {
    soap_arg(body, arg)
}

fn soap_raw_text(body: &str, arg: &str) -> Option<String> {
    let root = xmltree::Element::parse(body.as_bytes()).ok()?;
    let action = root
        .get_child("Body")?
        .children
        .iter()
        .find_map(|n| n.as_element())?;
    let element = action.get_child(arg)?;
    let open = format!("<{arg}>");
    let close = format!("</{arg}>");
    if let Some(start) = body.find(&open) {
        let raw = &body[start + open.len()..];
        let raw = &raw[..raw.find(&close)?];
        if raw.trim() != raw {
            return Some(raw.to_string());
        }
    } else if ![format!("<{arg}/>"), format!("<{arg} />")]
        .iter()
        .any(|tag| body.contains(tag))
    {
        return None;
    }
    Some(
        element
            .get_text()
            .map(|text| text.to_string())
            .unwrap_or_default(),
    )
}

fn soap_text(body: &str, arg: &str) -> Option<String> {
    Some(soap_raw_text(body, arg)?.trim().to_string())
}

fn soap_arg<T: std::str::FromStr>(body: &str, arg: &str) -> Option<T> {
    soap_text(body, arg)?.parse().ok()
}

fn ok(action: &str, inner: &str) -> Response {
    let body = format!(
        include_str!("igd_xml/ok.xml"),
        action = action,
        service = WANIP_SERVICE,
        inner = inner,
    );
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/xml; charset=\"utf-8\"")],
        body,
    )
        .into_response()
}

fn fault(code: u16, desc: &str) -> Response {
    let body = format!(include_str!("igd_xml/fault.xml"), code = code, desc = desc);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        [(header::CONTENT_TYPE, "text/xml; charset=\"utf-8\"")],
        body,
    )
        .into_response()
}

fn upnp_error_text(code: u16) -> &'static str {
    match code {
        402 => "Invalid Args",
        501 => "Action Failed",
        606 => "Action not authorized",
        714 => "NoSuchEntryInArray",
        718 => "ConflictInMappingEntry",
        725 => "OnlyPermanentLeasesSupported",
        800 => "HostnameTaken",
        801 => "HostnameNotSupported",
        _ => "Action Failed",
    }
}

fn sni_fault(code: u8) -> Response {
    let upnp = match code {
        RESULT_HOSTNAME_TAKEN => 800,
        RESULT_UNSUPP_HOSTNAME => 801,
        _ => 501,
    };
    fault(upnp, upnp_error_text(upnp))
}

/// Render the IGD root device description for `uuid`, identifying the gateway
/// as `product` — this is the name UPnP clients display for it, so each product
/// must pass its own rather than inherit whichever one the template was written
/// for.
pub fn render_root_desc(product: &str, uuid: &str) -> String {
    format!(
        include_str!("igd_xml/root_desc.xml"),
        device_type = IGD_DEVICE,
        product = product,
        uuid = uuid,
        service = WANIP_SERVICE,
        cif_service = WANCIF_SERVICE,
        control = CONTROL_PATH,
        scpd = SCPD_PATH,
        cif_scpd = CIF_SCPD_PATH,
    )
}

/// Serve a static XML document with the given content type.
///
/// Shares [`handle_control`]'s anti-rebinding `Host` gate: the device
/// description carries the product name and a stable UUID, which a rebound
/// page could otherwise read to fingerprint the network. Real clients fetch
/// these documents at the IP-literal URL that SSDP advertised.
pub async fn serve_static(
    headers: HeaderMap,
    body: Arc<str>,
    content_type: &'static str,
) -> Response {
    if !host_is_ip_literal(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, content_type)],
        body.to_string(),
    )
        .into_response()
}

/// Whether the `Host` header names an IP literal rather than a DNS name.
///
/// A legitimate UPnP client reaches this endpoint through the control URL it
/// read from the device description, whose host is always the gateway's own IP
/// (SSDP advertises `http://<ip>:<port>/…`), so it sends `Host: <ip>` or
/// `Host: <ip>:<port>`. A DNS-rebinding attacker's browser must keep the
/// attacker's domain in `Host` — that name is what makes the response read
/// same-origin with the malicious page — so a name-valued `Host` is the
/// attack's signature. Requiring an IP literal closes the rebinding oracle
/// (notably an otherwise well-known `GetExternalIPAddress` read that
/// deanonymizes the network's public IP) without the server needing to know its
/// own address. A missing `Host` is treated as a name (rejected): spec-
/// compliant HTTP/1.1 clients always send one, and a browser fetch always does.
fn host_is_ip_literal(headers: &HeaderMap) -> bool {
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
    else {
        return false;
    };
    // IPv6 literal is bracketed per RFC 3986: `[::1]` or `[::1]:port`.
    if let Some(rest) = host.strip_prefix('[') {
        return rest
            .split_once(']')
            .is_some_and(|(addr, _)| addr.parse::<Ipv6Addr>().is_ok());
    }
    // IPv4, with an optional `:port`.
    let addr = host.rsplit_once(':').map_or(host, |(h, _)| h);
    addr.parse::<Ipv4Addr>().is_ok()
}

/// Whether the action requires `SOAPAction` to prevent blind browser writes.
fn is_mutation(action: &str) -> bool {
    matches!(
        action,
        "AddPortMapping" | "AddAnyPortMapping" | "DeletePortMapping"
    ) || action == ADD_HOSTNAME_ACTION
        || action == DELETE_HOSTNAME_ACTION
}

/// Dispatch a SOAP control request from `peer` to the matching IGD action.
pub async fn handle_control<B: GatewayBackend + ?Sized>(
    backend: &B,
    peer: Ipv4Addr,
    headers: &HeaderMap,
    body: &str,
) -> Response {
    // An IP-literal Host blocks DNS rebinding.
    if !host_is_ip_literal(headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let action = soap_action(headers, body);
    // SOAPAction blocks browser simple-request writes.
    if action.as_deref().is_some_and(is_mutation) && !headers.contains_key("SOAPAction") {
        return StatusCode::FORBIDDEN.into_response();
    }
    match action.as_deref() {
        Some("GetExternalIPAddress") => get_external_ip(backend, peer).await,
        Some("AddPortMapping") => add_mapping(backend, peer, body, false).await,
        Some("AddAnyPortMapping") => add_mapping(backend, peer, body, true).await,
        Some("DeletePortMapping") => delete_mapping(backend, peer, body).await,
        Some("GetSpecificPortMappingEntry") => get_specific_mapping(backend, peer, body).await,
        Some("GetGenericPortMappingEntry") => get_generic_mapping(backend, peer, body).await,
        // Static descriptions of the WAN connection. A client uses these to
        // decide the gateway is a usable IGD before it will map anything:
        // miniupnpc calls GetStatusInfo and treats anything but `Connected` as
        // a disconnected IGD, refusing to continue.
        Some("GetStatusInfo") => ok(
            "GetStatusInfo",
            &format!(
                "<NewConnectionStatus>Connected</NewConnectionStatus>\
                 <NewLastConnectionError>ERROR_NONE</NewLastConnectionError>\
                 <NewUptime>{}</NewUptime>",
                uptime_seconds()
            ),
        ),
        Some("GetConnectionTypeInfo") => ok(
            "GetConnectionTypeInfo",
            "<NewConnectionType>IP_Routed</NewConnectionType>\
             <NewPossibleConnectionTypes>IP_Routed</NewPossibleConnectionTypes>",
        ),
        Some("GetNATRSIPStatus") => ok(
            "GetNATRSIPStatus",
            "<NewRSIPAvailable>0</NewRSIPAvailable><NewNATEnabled>1</NewNATEnabled>",
        ),
        // WANCommonInterfaceConfig. The link rates are unknown to us and
        // reported as 0, which the spec permits and clients treat as "unknown".
        Some("GetCommonLinkProperties") => ok(
            "GetCommonLinkProperties",
            "<NewWANAccessType>Ethernet</NewWANAccessType>\
             <NewLayer1UpstreamMaxBitRate>0</NewLayer1UpstreamMaxBitRate>\
             <NewLayer1DownstreamMaxBitRate>0</NewLayer1DownstreamMaxBitRate>\
             <NewPhysicalLinkStatus>Up</NewPhysicalLinkStatus>",
        ),
        Some(a) if a == ADD_HOSTNAME_ACTION => add_hostname_mapping(backend, peer, body).await,
        Some(a) if a == DELETE_HOSTNAME_ACTION => {
            delete_hostname_mapping(backend, peer, body).await
        }
        _ => fault(401, "Invalid Action"),
    }
}

/// `NewProtocol` from a SOAP body, normalized to the IGD spelling.
fn soap_protocol(body: &str) -> Option<&'static str> {
    match soap_arg::<String>(body, "NewProtocol")?
        .to_ascii_uppercase()
        .as_str()
    {
        "TCP" => Some("TCP"),
        "UDP" => Some("UDP"),
        _ => None,
    }
}

fn mapping_response(action: &str, entry: &MappingEntry) -> Response {
    ok(
        action,
        &format!(
            "<NewRemoteHost></NewRemoteHost>\
             <NewExternalPort>{}</NewExternalPort>\
             <NewProtocol>{}</NewProtocol>\
             <NewInternalPort>{}</NewInternalPort>\
             <NewInternalClient>{}</NewInternalClient>\
             <NewEnabled>1</NewEnabled>\
             <NewPortMappingDescription>{}</NewPortMappingDescription>\
             <NewLeaseDuration>{}</NewLeaseDuration>",
            entry.external_port,
            entry.protocol,
            entry.internal.port(),
            entry.internal.ip(),
            entry.description,
            entry.lease_seconds,
        ),
    )
}

/// Read back one mapping by external port + protocol. Clients call this right
/// after `AddPortMapping` to confirm the mapping took, so answering it wrongly
/// makes a successful mapping look like a failure.
async fn get_specific_mapping<B: GatewayBackend + ?Sized>(
    backend: &B,
    peer: Ipv4Addr,
    body: &str,
) -> Response {
    let (Some(external_port), Some(protocol)) =
        (soap_u16(body, "NewExternalPort"), soap_protocol(body))
    else {
        return fault(402, "Invalid Args");
    };
    if !backend.is_known_client(peer).await {
        return fault(606, "Action not authorized");
    }
    match backend
        .list_forwards(peer)
        .await
        .into_iter()
        .find(|e| e.external_port == external_port && e.protocol == protocol)
    {
        Some(entry) => mapping_response("GetSpecificPortMappingEntry", &entry),
        None => fault(714, "NoSuchEntryInArray"),
    }
}

/// Enumerate the peer's mappings by index; clients walk it until 714.
async fn get_generic_mapping<B: GatewayBackend + ?Sized>(
    backend: &B,
    peer: Ipv4Addr,
    body: &str,
) -> Response {
    let Some(index) = soap_arg::<usize>(body, "NewPortMappingIndex") else {
        return fault(402, "Invalid Args");
    };
    if !backend.is_known_client(peer).await {
        return fault(606, "Action not authorized");
    }
    match backend.list_forwards(peer).await.into_iter().nth(index) {
        Some(entry) => mapping_response("GetGenericPortMappingEntry", &entry),
        None => fault(714, "NoSuchEntryInArray"),
    }
}

async fn get_external_ip<B: GatewayBackend + ?Sized>(backend: &B, peer: Ipv4Addr) -> Response {
    // Answered for any client, as the standard specifies and as clients expect
    // during discovery. Authorization would buy nothing: a client reaches this
    // gateway through it, so the address is one it can determine for itself.
    // The rebinding gate in `handle_control` is what keeps a web page from
    // reading it on a client's behalf.
    match backend.external_ipv4(peer).await {
        Some(ip) => ok(
            "GetExternalIPAddress",
            &format!("<NewExternalIPAddress>{ip}</NewExternalIPAddress>"),
        ),
        None => fault(501, "Action Failed"),
    }
}

async fn add_mapping<B: GatewayBackend + ?Sized>(
    backend: &B,
    peer: Ipv4Addr,
    body: &str,
    any: bool,
) -> Response {
    let (Some(external_port), Some(internal_port)) = (
        soap_u16(body, "NewExternalPort"),
        soap_u16(body, "NewInternalPort"),
    ) else {
        return fault(402, "Invalid Args");
    };
    if external_port == 0 || internal_port == 0 {
        return fault(402, "Invalid Args");
    }
    if !backend.is_known_client(peer).await {
        return fault(606, "Action not authorized");
    }
    let Some(source_ip) = backend.external_ipv4(peer).await else {
        return fault(501, "Action Failed");
    };
    let source = SocketAddrV4::new(source_ip, external_port);
    // Secure mode: force the target to the requesting peer's own address.
    let target = SocketAddrV4::new(peer, internal_port);

    // IGD has no way to report a granted duration back on AddPortMapping, so
    // honor an explicit lease as-is, clamped. A lease of 0 ("as long as
    // possible") stays permanent rather than becoming WANIPConnection:2's week:
    // StartOS requests 0 and does not re-announce, so expiring it would drop a
    // working forward out from under a server that believes it permanent.
    let lifetime = match soap_u32(body, "NewLeaseDuration") {
        Some(0) | None => None,
        Some(secs) => Some(secs.min(IGD_MAX_LEASE_SECONDS)),
    };

    match backend.add_forward(source, target, 1, peer, lifetime).await {
        Ok(()) if any => ok(
            "AddAnyPortMapping",
            &format!("<NewReservedPort>{external_port}</NewReservedPort>"),
        ),
        Ok(()) => ok("AddPortMapping", ""),
        Err(code) => fault(code, upnp_error_text(code)),
    }
}

async fn delete_mapping<B: GatewayBackend + ?Sized>(
    backend: &B,
    peer: Ipv4Addr,
    body: &str,
) -> Response {
    let Some(external_port) = soap_u16(body, "NewExternalPort") else {
        return fault(402, "Invalid Args");
    };
    let Some(source_ip) = backend.external_ipv4(peer).await else {
        return fault(714, "NoSuchEntryInArray");
    };
    let source = SocketAddrV4::new(source_ip, external_port);

    // Owner-scoped so a peer can't delete (or probe for) another's mapping.
    if backend.remove_forward_by_source(source, peer).await {
        ok("DeletePortMapping", "")
    } else {
        fault(714, "NoSuchEntryInArray")
    }
}

async fn add_hostname_mapping<B: GatewayBackend + ?Sized>(
    backend: &B,
    peer: Ipv4Addr,
    body: &str,
) -> Response {
    let (Some(external_port), Some(internal_port)) = (
        soap_u16(body, "NewExternalPort"),
        soap_u16(body, "NewInternalPort"),
    ) else {
        return fault(402, "Invalid Args");
    };
    let (Some(remote_host), Some(_internal_client), Some(enabled), Some(_description)) = (
        soap_raw_text(body, "NewRemoteHost"),
        soap_text(body, "NewInternalClient"),
        soap_text(body, "NewEnabled"),
        soap_text(body, "NewPortMappingDescription"),
    ) else {
        return fault(402, "Invalid Args");
    };
    if !remote_host.is_empty() {
        return fault(801, upnp_error_text(801));
    }
    if external_port == 0
        || internal_port == 0
        || soap_protocol(body) != Some("TCP")
        || !matches!(enabled.as_str(), "1" | "true")
    {
        return fault(402, "Invalid Args");
    }
    let Some(hostname) = soap_raw_text(body, "NewHostname")
        .filter(|h| validate_hostname(h))
        .map(|h| h.to_ascii_lowercase())
    else {
        return fault(402, "Invalid Args");
    };
    if backend.sni().is_none() {
        return fault(801, upnp_error_text(801));
    }
    if !backend.is_known_client(peer).await {
        return fault(606, "Action not authorized");
    }
    let Some(source_ip) = backend.external_ipv4(peer).await else {
        return fault(501, "Action Failed");
    };
    let source = SocketAddrV4::new(source_ip, external_port);
    let target = SocketAddrV4::new(peer, internal_port);

    let Some(requested_lifetime) = soap_u32(body, "NewLeaseDuration") else {
        return fault(402, "Invalid Args");
    };
    let lifetime = if requested_lifetime > 0 {
        requested_lifetime.min(MAX_LIFETIME_SECONDS)
    } else {
        MAX_LIFETIME_SECONDS
    };
    match backend
        .add_sni_forward(
            source,
            target,
            std::slice::from_ref(&hostname),
            Some(lifetime),
        )
        .await
    {
        Ok(()) => ok(ADD_HOSTNAME_ACTION, ""),
        Err(code) => sni_fault(code),
    }
}

async fn delete_hostname_mapping<B: GatewayBackend + ?Sized>(
    backend: &B,
    peer: Ipv4Addr,
    body: &str,
) -> Response {
    let (Some(external_port), Some(internal_port), Some(remote_host)) = (
        soap_u16(body, "NewExternalPort"),
        soap_u16(body, "NewInternalPort"),
        soap_raw_text(body, "NewRemoteHost"),
    ) else {
        return fault(402, "Invalid Args");
    };
    if !remote_host.is_empty() {
        return fault(801, upnp_error_text(801));
    }
    if external_port == 0 || internal_port == 0 || soap_protocol(body) != Some("TCP") {
        return fault(402, "Invalid Args");
    }
    let Some(hostname) = soap_raw_text(body, "NewHostname")
        .filter(|h| validate_hostname(h))
        .map(|h| h.to_ascii_lowercase())
    else {
        return fault(402, "Invalid Args");
    };
    if backend.sni().is_none() {
        return fault(801, upnp_error_text(801));
    }
    if !backend.is_known_client(peer).await {
        return fault(606, "Action not authorized");
    }
    let Some(source_ip) = backend.external_ipv4(peer).await else {
        return fault(714, "NoSuchEntryInArray");
    };
    let source = SocketAddrV4::new(source_ip, external_port);
    let target = SocketAddrV4::new(peer, internal_port);
    backend
        .remove_sni_forward(source, target, std::slice::from_ref(&hostname))
        .await;
    ok(DELETE_HOSTNAME_ACTION, "")
}

#[cfg(test)]
mod tests {
    use xmltree::Element;

    use super::*;

    /// Recreates how an IGD client locates a service: walk devices/serviceLists
    /// for a matching serviceType, returning (SCPDURL, controlURL).
    fn find_service(device: &Element, service_type: &str) -> Option<(String, String)> {
        if let Some(service_list) = device.get_child("serviceList") {
            for child in &service_list.children {
                if let Some(svc) = child.as_element() {
                    if svc.name == "service"
                        && svc
                            .get_child("serviceType")
                            .and_then(|e| e.get_text())
                            .as_deref()
                            == Some(service_type)
                    {
                        return Some((
                            svc.get_child("SCPDURL")?.get_text()?.into_owned(),
                            svc.get_child("controlURL")?.get_text()?.into_owned(),
                        ));
                    }
                }
            }
        }
        let device_list = device.get_child("deviceList")?;
        device_list
            .children
            .iter()
            .filter_map(|c| c.as_element())
            .filter(|c| c.name == "device")
            .find_map(|d| find_service(d, service_type))
    }

    fn find_wanip(device: &Element) -> Option<(String, String)> {
        find_service(device, WANIP_SERVICE)
    }

    // A client identifies a gateway as an IGD by finding WANCommonInterfaceConfig
    // in the description. Without it miniupnpc — and everything built on it —
    // classifies us as UPNP_UNKNOWN_DEVICE and refuses to map anything at all,
    // so this service's presence is load-bearing, not decorative.
    #[test]
    fn root_desc_advertises_wan_common_interface_config() {
        let xml = render_root_desc("TestGateway", "abcd1234-0000-0000-0000-000000000000");
        let root = Element::parse(xml.as_bytes()).unwrap();
        let device = root.get_child("device").unwrap();
        let (scpd, control) =
            find_service(device, WANCIF_SERVICE).expect("WANCommonInterfaceConfig service");
        assert_eq!(scpd, CIF_SCPD_PATH);
        assert_eq!(control, CONTROL_PATH);
        // The SCPD it points at must parse, or a client fetching it gets junk.
        Element::parse(CIF_SCPD.as_bytes()).expect("CIF SCPD is well-formed");
    }

    // The description carries the name UPnP clients display for the gateway, so
    // it has to come from the caller — a hardcoded one means every product
    // advertises itself as whichever one the template was written for.
    #[test]
    fn root_desc_identifies_the_calling_product() {
        let xml = render_root_desc("StartWRT", "abcd1234-0000-0000-0000-000000000000");
        let root = Element::parse(xml.as_bytes()).unwrap();
        let device = root.get_child("device").unwrap();
        assert_eq!(
            device
                .get_child("friendlyName")
                .and_then(|e| e.get_text())
                .as_deref(),
            Some("StartWRT")
        );
        assert!(
            !xml.contains("StartTunnel"),
            "no other product's name may leak into the description:\n{xml}"
        );
    }

    #[test]
    fn protocol_argument_is_normalized() {
        let body = |p: &str| {
            format!(
                r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
<s:Body>
<u:GetSpecificPortMappingEntry xmlns:u="{WANIP_SERVICE}">
<NewProtocol>{p}</NewProtocol>
</u:GetSpecificPortMappingEntry>
</s:Body>
</s:Envelope>"#
            )
        };
        assert_eq!(soap_protocol(&body("TCP")), Some("TCP"));
        assert_eq!(soap_protocol(&body("udp")), Some("UDP"));
        assert_eq!(soap_protocol(&body("SCTP")), None);
    }

    #[test]
    fn root_desc_advertises_wanip_service() {
        let xml = render_root_desc("TestGateway", "abcd1234-0000-0000-0000-000000000000");
        let root = Element::parse(xml.as_bytes()).unwrap();
        let device = root.get_child("device").unwrap();
        let (scpd, control) = find_wanip(device).expect("WANIPConnection service");
        assert_eq!(scpd, SCPD_PATH);
        assert_eq!(control, CONTROL_PATH);
    }

    #[test]
    fn scpd_lists_input_args_for_add_port_mapping() {
        let scpd = Element::parse(SCPD.as_bytes()).unwrap();
        let action_list = scpd.get_child("actionList").unwrap();
        let mut actions = std::collections::HashMap::new();
        for child in &action_list.children {
            if let Some(a) = child.as_element() {
                let name = a
                    .get_child("name")
                    .unwrap()
                    .get_text()
                    .unwrap()
                    .into_owned();
                let ins: Vec<String> = a
                    .get_child("argumentList")
                    .map(|al| {
                        al.children
                            .iter()
                            .filter_map(|c| c.as_element())
                            .filter(|arg| {
                                arg.get_child("direction")
                                    .and_then(|d| d.get_text())
                                    .as_deref()
                                    == Some("in")
                            })
                            .filter_map(|arg| {
                                arg.get_child("name")?.get_text().map(|t| t.into_owned())
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                actions.insert(name, ins);
            }
        }
        assert!(actions.contains_key("GetExternalIPAddress"));
        assert!(actions.contains_key("DeletePortMapping"));
        let add = actions.get("AddPortMapping").expect("AddPortMapping");
        for arg in [
            "NewRemoteHost",
            "NewExternalPort",
            "NewProtocol",
            "NewInternalPort",
            "NewInternalClient",
            "NewEnabled",
            "NewPortMappingDescription",
            "NewLeaseDuration",
        ] {
            assert!(add.contains(&arg.to_string()), "missing {arg}");
        }
    }

    fn add_port_body() -> String {
        // Shaped like igd-next's `format_add_port_mapping_message`.
        r#"<?xml version="1.0"?>
<s:Envelope s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/" xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
<s:Body>
<u:AddPortMapping xmlns:u="urn:schemas-upnp-org:service:WANIPConnection:1">
<NewRemoteHost></NewRemoteHost>
<NewExternalPort>443</NewExternalPort>
<NewProtocol>TCP</NewProtocol>
<NewInternalPort>8443</NewInternalPort>
<NewInternalClient>10.59.1.5</NewInternalClient>
<NewEnabled>1</NewEnabled>
<NewPortMappingDescription>StartOS</NewPortMappingDescription>
<NewLeaseDuration>0</NewLeaseDuration>
</u:AddPortMapping>
</s:Body>
</s:Envelope>"#
            .to_string()
    }

    #[test]
    fn parses_action_and_ports_from_soap_body() {
        let body = add_port_body();
        assert_eq!(
            soap_action(&HeaderMap::new(), &body).as_deref(),
            Some("AddPortMapping")
        );
        assert_eq!(soap_u16(&body, "NewExternalPort"), Some(443));
        assert_eq!(soap_u16(&body, "NewInternalPort"), Some(8443));
        assert_eq!(soap_u16(&body, "NoSuchArg"), None);
    }

    fn host_headers(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, value.parse().unwrap());
        h
    }

    #[test]
    fn host_ip_literal_accepts_real_clients_rejects_rebinding() {
        // What SSDP advertises and real clients send back: IP literals.
        for host in [
            "192.168.1.1",
            "192.168.1.1:49001",
            "[fe80::1]",
            "[fe80::1]:49001",
        ] {
            assert!(
                host_is_ip_literal(&host_headers(host)),
                "should accept {host}"
            );
        }
        // DNS names — the rebinding signature — and a missing Host: rejected.
        for host in ["evil.com", "evil.com:49001", "gateway.local", ""] {
            assert!(
                !host_is_ip_literal(&host_headers(host)),
                "should reject {host}"
            );
        }
        assert!(
            !host_is_ip_literal(&HeaderMap::new()),
            "should reject missing Host"
        );
    }

    /// Backend that authorizes every peer, so a request that clears the
    /// browser-facing gates reaches a normal SOAP response.
    struct OpenStub;
    impl GatewayBackend for OpenStub {
        fn add_forward(
            &self,
            _: SocketAddrV4,
            _: SocketAddrV4,
            _: u16,
            _: Ipv4Addr,
            _: Option<u32>,
        ) -> impl std::future::Future<Output = Result<(), u16>> + Send {
            async { Ok(()) }
        }
        fn remove_forward(
            &self,
            _: Ipv4Addr,
            _: u16,
        ) -> impl std::future::Future<Output = ()> + Send {
            async {}
        }
        fn remove_forward_by_source(
            &self,
            _: SocketAddrV4,
            _: Ipv4Addr,
        ) -> impl std::future::Future<Output = bool> + Send {
            async { false }
        }
        fn external_ipv4(
            &self,
            _: Ipv4Addr,
        ) -> impl std::future::Future<Output = Option<Ipv4Addr>> + Send {
            async { Some(Ipv4Addr::new(203, 0, 113, 1)) }
        }
        fn is_known_client(&self, _: Ipv4Addr) -> impl std::future::Future<Output = bool> + Send {
            async { true }
        }
        fn sni(&self) -> Option<&Arc<crate::tunnel::forward::sni::SniDemux>> {
            None
        }
    }

    /// Backend that recognizes no peer, for the actions that answer regardless.
    struct UnknownPeerStub;
    impl GatewayBackend for UnknownPeerStub {
        fn add_forward(
            &self,
            _: SocketAddrV4,
            _: SocketAddrV4,
            _: u16,
            _: Ipv4Addr,
            _: Option<u32>,
        ) -> impl std::future::Future<Output = Result<(), u16>> + Send {
            async { Ok(()) }
        }
        fn remove_forward(
            &self,
            _: Ipv4Addr,
            _: u16,
        ) -> impl std::future::Future<Output = ()> + Send {
            async {}
        }
        fn remove_forward_by_source(
            &self,
            _: SocketAddrV4,
            _: Ipv4Addr,
        ) -> impl std::future::Future<Output = bool> + Send {
            async { false }
        }
        fn external_ipv4(
            &self,
            _: Ipv4Addr,
        ) -> impl std::future::Future<Output = Option<Ipv4Addr>> + Send {
            async { Some(Ipv4Addr::new(203, 0, 113, 1)) }
        }
        fn is_known_client(&self, _: Ipv4Addr) -> impl std::future::Future<Output = bool> + Send {
            async { false }
        }
        fn sni(&self) -> Option<&Arc<crate::tunnel::forward::sni::SniDemux>> {
            None
        }
    }

    fn external_ip_body() -> String {
        format!(
            r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
<s:Body><u:GetExternalIPAddress xmlns:u="{WANIP_SERVICE}"/></s:Body>
</s:Envelope>"#
        )
    }

    async fn body_string(resp: Response) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    /// Deliberate: clients read the external IP during discovery, before they
    /// are anything this gateway would map for, and refusing them there is what
    /// makes a client report the gateway unusable.
    #[tokio::test]
    async fn external_ip_answers_a_client_the_gateway_would_not_map_for() {
        let peer = Ipv4Addr::new(192, 168, 1, 5);
        let headers = host_headers("192.168.1.1:49001");
        let resp = handle_control(&UnknownPeerStub, peer, &headers, &external_ip_body()).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(body_string(resp).await.contains("203.0.113.1"));
    }

    fn status_info_body() -> String {
        format!(
            r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
<s:Body><u:GetStatusInfo xmlns:u="{WANIP_SERVICE}"/></s:Body>
</s:Envelope>"#
        )
    }

    #[tokio::test]
    async fn control_blocks_browser_shaped_requests() {
        let peer = Ipv4Addr::new(192, 168, 1, 5);
        let body = add_port_body();

        // Rebound Host (a DNS name): refused before any action runs.
        let resp = handle_control(&OpenStub, peer, &host_headers("evil.com"), &body).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let headers = host_headers("192.168.1.1:49001");
        let resp = handle_control(&OpenStub, peer, &headers, &body).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let mut headers = host_headers("192.168.1.1:49001");
        headers.insert(
            "SOAPAction",
            format!(r#""{WANIP_SERVICE}#AddPortMapping""#)
                .parse()
                .unwrap(),
        );
        let resp = handle_control(&OpenStub, peer, &headers, &body).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let headers = host_headers("192.168.1.1:49001");
        let resp = handle_control(&OpenStub, peer, &headers, &status_info_body()).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn browser_shaped_requests_cannot_reach_vendor_mutations() {
        for body in [
            add_hostname_body("0", "git.example.com"),
            delete_hostname_body("git.example.com"),
        ] {
            let stub = HostnameStub::new(true);

            let resp = handle_control(&stub, PEER, &host_headers("192.168.1.1:49001"), &body).await;
            assert_eq!(
                resp.status(),
                StatusCode::FORBIDDEN,
                "a mutation named only in the body must be refused"
            );
            assert!(
                stub.calls.lock().unwrap().is_empty(),
                "a refused request must not reach the backend"
            );

            let resp = control(&stub, PEER, &body).await;
            assert_eq!(
                resp.status(),
                StatusCode::OK,
                "the same body from a real client goes through"
            );
        }
    }

    #[tokio::test]
    async fn static_docs_share_the_host_gate() {
        let doc: Arc<str> = Arc::from("<root/>");
        let ok = serve_static(host_headers("192.168.1.1:49001"), doc.clone(), "text/xml").await;
        assert_eq!(ok.status(), StatusCode::OK);
        let rebound = serve_static(host_headers("evil.com"), doc, "text/xml").await;
        assert_eq!(rebound.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn soap_action_prefers_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "SOAPAction",
            r#""urn:schemas-upnp-org:service:WANIPConnection:1#DeletePortMapping""#
                .parse()
                .unwrap(),
        );
        assert_eq!(
            soap_action(&headers, "").as_deref(),
            Some("DeletePortMapping")
        );
    }

    #[test]
    fn response_and_fault_are_wellformed_xml() {
        let resp = ok(
            "GetExternalIPAddress",
            "<NewExternalIPAddress>1.2.3.4</NewExternalIPAddress>",
        );
        assert_eq!(resp.status(), StatusCode::OK);
        let f = fault(718, "ConflictInMappingEntry");
        assert_eq!(f.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn st_matches_igd_searches() {
        assert!(st_matches(IGD_DEVICE));
        assert!(st_matches("ssdp:all"));
        assert!(st_matches("upnp:rootdevice"));
        assert!(st_matches(WANIP_SERVICE));
        assert!(!st_matches(
            "urn:schemas-upnp-org:service:WANCommonInterfaceConfig:1"
        ));
    }

    #[test]
    fn uuid_is_stable_and_wellformed() {
        let bytes: Vec<u8> = (0u8..32).collect();
        let uuid = format_uuid(&bytes);
        assert_eq!(uuid, "00010203-0405-0607-0809-0a0b0c0d0e0f");
        assert_eq!(format_uuid(&bytes), format_uuid(&bytes));
    }

    use std::future::Future;
    use std::sync::Mutex;

    use crate::tunnel::forward::sni::SniDemux;

    const PEER: Ipv4Addr = Ipv4Addr::new(10, 59, 0, 2);
    const OTHER_PEER: Ipv4Addr = Ipv4Addr::new(10, 59, 0, 3);
    const EXT_IP: Ipv4Addr = Ipv4Addr::new(203, 0, 113, 1);

    /// Uses the real demux with an unprivileged external port.
    struct HostnameStub {
        sni: Arc<SniDemux>,
        known: bool,
        calls: Mutex<Vec<(SocketAddrV4, SocketAddrV4, Vec<String>, Option<u32>)>>,
        remove_calls: Mutex<Vec<(SocketAddrV4, SocketAddrV4, Vec<String>)>>,
    }
    impl HostnameStub {
        fn new(known: bool) -> Self {
            Self {
                sni: SniDemux::new(),
                known,
                calls: Mutex::new(Vec::new()),
                remove_calls: Mutex::new(Vec::new()),
            }
        }
    }
    impl GatewayBackend for HostnameStub {
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
            async { Some(EXT_IP) }
        }
        fn is_known_client(&self, _: Ipv4Addr) -> impl Future<Output = bool> + Send {
            let known = self.known;
            async move { known }
        }
        fn sni(&self) -> Option<&Arc<SniDemux>> {
            Some(&self.sni)
        }
        fn add_sni_forward(
            &self,
            source: SocketAddrV4,
            target: SocketAddrV4,
            hostnames: &[String],
            lifetime: Option<u32>,
        ) -> impl Future<Output = Result<(), u8>> + Send {
            self.calls
                .lock()
                .unwrap()
                .push((source, target, hostnames.to_vec(), lifetime));
            let res = self
                .sni
                .register(*source.ip(), source.port(), hostnames, target, lifetime);
            async move { res }
        }
        fn remove_sni_forward(
            &self,
            source: SocketAddrV4,
            target: SocketAddrV4,
            hostnames: &[String],
        ) -> impl Future<Output = ()> + Send {
            self.remove_calls
                .lock()
                .unwrap()
                .push((source, target, hostnames.to_vec()));
            self.sni
                .unregister(*source.ip(), source.port(), hostnames, target);
            async {}
        }
    }

    fn add_hostname_body(lease: &str, hostname: &str) -> String {
        format!(
            r#"<?xml version="1.0"?>
<s:Envelope s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/" xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
<s:Body>
<u:X_START9_AddHostnameMapping xmlns:u="{WANIP_SERVICE}">
<NewRemoteHost></NewRemoteHost>
<NewExternalPort>44300</NewExternalPort>
<NewProtocol>TCP</NewProtocol>
<NewInternalPort>8443</NewInternalPort>
<NewInternalClient>10.59.1.99</NewInternalClient>
<NewEnabled>1</NewEnabled>
<NewPortMappingDescription>StartOS</NewPortMappingDescription>
<NewLeaseDuration>{lease}</NewLeaseDuration>
<NewHostname>{hostname}</NewHostname>
</u:X_START9_AddHostnameMapping>
</s:Body>
</s:Envelope>"#
        )
    }

    fn delete_hostname_body(hostname: &str) -> String {
        format!(
            r#"<?xml version="1.0"?>
<s:Envelope s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/" xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
<s:Body>
<u:X_START9_DeleteHostnameMapping xmlns:u="{WANIP_SERVICE}">
<NewRemoteHost></NewRemoteHost>
<NewExternalPort>44300</NewExternalPort>
<NewProtocol>TCP</NewProtocol>
<NewInternalPort>8443</NewInternalPort>
<NewHostname>{hostname}</NewHostname>
</u:X_START9_DeleteHostnameMapping>
</s:Body>
</s:Envelope>"#
        )
    }

    async fn body_text(resp: Response) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn client_headers(body: &str) -> HeaderMap {
        let action = soap_action(&HeaderMap::new(), body).expect("body names an action");
        let mut headers = host_headers("192.168.1.1:49001");
        headers.insert(
            "SOAPAction",
            format!(r#""{WANIP_SERVICE}#{action}""#).parse().unwrap(),
        );
        headers
    }

    async fn control(stub: &HostnameStub, peer: Ipv4Addr, body: &str) -> Response {
        handle_control(stub, peer, &client_headers(body), body).await
    }

    #[test]
    fn scpd_lists_input_args_for_hostname_actions() {
        let scpd = Element::parse(SCPD.as_bytes()).unwrap();
        let action_list = scpd.get_child("actionList").unwrap();
        let mut actions = std::collections::HashMap::new();
        for child in &action_list.children {
            if let Some(a) = child.as_element() {
                let name = a
                    .get_child("name")
                    .unwrap()
                    .get_text()
                    .unwrap()
                    .into_owned();
                let ins: Vec<String> = a
                    .get_child("argumentList")
                    .map(|al| {
                        al.children
                            .iter()
                            .filter_map(|c| c.as_element())
                            .filter(|arg| {
                                arg.get_child("direction")
                                    .and_then(|d| d.get_text())
                                    .as_deref()
                                    == Some("in")
                            })
                            .filter_map(|arg| {
                                arg.get_child("name")?.get_text().map(|t| t.into_owned())
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                actions.insert(name, ins);
            }
        }
        let add = actions
            .get("X_START9_AddHostnameMapping")
            .expect("X_START9_AddHostnameMapping");
        assert_eq!(
            add,
            &[
                "NewRemoteHost",
                "NewExternalPort",
                "NewProtocol",
                "NewInternalPort",
                "NewInternalClient",
                "NewEnabled",
                "NewPortMappingDescription",
                "NewLeaseDuration",
                "NewHostname",
            ]
        );
        let del = actions
            .get("X_START9_DeleteHostnameMapping")
            .expect("X_START9_DeleteHostnameMapping");
        assert_eq!(
            del,
            &[
                "NewRemoteHost",
                "NewExternalPort",
                "NewProtocol",
                "NewInternalPort",
                "NewHostname",
            ]
        );
        let table = scpd.get_child("serviceStateTable").unwrap();
        assert!(
            table
                .children
                .iter()
                .filter_map(|c| c.as_element())
                .any(
                    |v| v.get_child("name").and_then(|n| n.get_text()).as_deref()
                        == Some("X_START9_Hostname")
                )
        );
    }

    #[test]
    fn client_envelopes_round_trip_through_server_parser() {
        let body = crate::net::port_map::upnp::add_hostname_body(
            443,
            Ipv4Addr::new(10, 59, 1, 5),
            8443,
            "git.example.com",
        );
        assert_eq!(
            soap_action(&HeaderMap::new(), &body).as_deref(),
            Some("X_START9_AddHostnameMapping")
        );
        assert_eq!(soap_u16(&body, "NewExternalPort"), Some(443));
        assert_eq!(soap_u16(&body, "NewInternalPort"), Some(8443));
        assert_eq!(soap_u16(&body, "NewLeaseDuration"), Some(3600));
        assert_eq!(
            soap_arg::<String>(&body, "NewHostname").as_deref(),
            Some("git.example.com")
        );

        let body = crate::net::port_map::upnp::delete_hostname_body(443, 8443, "git.example.com");
        assert_eq!(
            soap_action(&HeaderMap::new(), &body).as_deref(),
            Some("X_START9_DeleteHostnameMapping")
        );
        assert_eq!(soap_u16(&body, "NewExternalPort"), Some(443));
        assert_eq!(soap_u16(&body, "NewInternalPort"), Some(8443));
        assert_eq!(
            soap_arg::<String>(&body, "NewHostname").as_deref(),
            Some("git.example.com")
        );
    }

    #[tokio::test]
    async fn hostname_mapping_is_always_lease_bearing() {
        for (lease, granted) in [
            ("0", super::super::MAX_LIFETIME_SECONDS),
            ("600", 600),
            ("7200", super::super::MAX_LIFETIME_SECONDS),
            ("100000", super::super::MAX_LIFETIME_SECONDS),
        ] {
            let stub = HostnameStub::new(true);
            let resp = control(&stub, PEER, &add_hostname_body(lease, "git.example.com")).await;
            assert_eq!(resp.status(), StatusCode::OK, "lease {lease}");
            let calls = stub.calls.lock().unwrap();
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].3, Some(granted), "lease {lease}");
        }
    }

    #[tokio::test]
    async fn hostname_mapping_forces_target_to_peer() {
        let stub = HostnameStub::new(true);
        let resp = control(&stub, PEER, &add_hostname_body("0", "Git.Example.Com")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let calls = stub.calls.lock().unwrap();
        assert_eq!(calls[0].0, SocketAddrV4::new(EXT_IP, 44300));
        assert_eq!(calls[0].1, SocketAddrV4::new(PEER, 8443));
        assert_eq!(calls[0].2, vec!["git.example.com".to_string()]);
    }

    #[tokio::test]
    async fn hostname_actions_require_tcp_without_backend_mutation() {
        for valid_body in [
            add_hostname_body("0", "git.example.com"),
            delete_hostname_body("git.example.com"),
        ] {
            for body in [
                valid_body.replace("<NewProtocol>TCP</NewProtocol>", ""),
                valid_body.replace(
                    "<NewProtocol>TCP</NewProtocol>",
                    "<NewProtocol>UDP</NewProtocol>",
                ),
            ] {
                let stub = HostnameStub::new(true);
                let resp = control(&stub, PEER, &body).await;
                assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
                assert!(body_text(resp).await.contains("<errorCode>402</errorCode>"));
                assert!(stub.calls.lock().unwrap().is_empty());
                assert!(stub.remove_calls.lock().unwrap().is_empty());
            }
        }
    }

    #[tokio::test]
    async fn hostname_add_rejects_unsupported_or_missing_fields() {
        let valid = add_hostname_body("0", "git.example.com");
        for (body, code) in [
            (
                valid.replace(
                    "<NewRemoteHost></NewRemoteHost>",
                    "<NewRemoteHost>198.51.100.0/24</NewRemoteHost>",
                ),
                801,
            ),
            (
                valid.replace(
                    "<NewRemoteHost></NewRemoteHost>",
                    "<NewRemoteHost> </NewRemoteHost>",
                ),
                801,
            ),
            (
                valid.replace("<NewEnabled>1</NewEnabled>", "<NewEnabled>0</NewEnabled>"),
                402,
            ),
            (valid.replace("<NewEnabled>1</NewEnabled>", ""), 402),
            (
                valid.replace(
                    "<NewPortMappingDescription>StartOS</NewPortMappingDescription>",
                    "",
                ),
                402,
            ),
            (
                valid.replace("<NewLeaseDuration>0</NewLeaseDuration>", ""),
                402,
            ),
            (
                valid.replace(
                    "<NewLeaseDuration>0</NewLeaseDuration>",
                    "<NewLeaseDuration>invalid</NewLeaseDuration>",
                ),
                402,
            ),
        ] {
            let stub = HostnameStub::new(true);
            let resp = control(&stub, PEER, &body).await;
            assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
            assert!(
                body_text(resp)
                    .await
                    .contains(&format!("<errorCode>{code}</errorCode>"))
            );
            assert!(stub.calls.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn hostname_mapping_rejects_unknown_peer() {
        let stub = HostnameStub::new(false);
        let resp = control(&stub, PEER, &add_hostname_body("0", "git.example.com")).await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(body_text(resp).await.contains("<errorCode>606</errorCode>"));
        assert!(stub.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn hostname_mapping_rejects_malformed_hostname() {
        for bad in [
            "ex ample.com",
            ".example.com",
            "192.168.1.1",
            " git.example.com ",
            "",
        ] {
            let stub = HostnameStub::new(true);
            let resp = control(&stub, PEER, &add_hostname_body("0", bad)).await;
            assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR, "{bad:?}");
            assert!(
                body_text(resp).await.contains("<errorCode>402</errorCode>"),
                "{bad:?}"
            );
            assert!(stub.calls.lock().unwrap().is_empty(), "{bad:?}");
        }
    }

    #[tokio::test]
    async fn hostname_taken_by_another_target_faults_800() {
        let stub = HostnameStub::new(true);
        let resp = control(&stub, PEER, &add_hostname_body("0", "git.example.com")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let resp = control(
            &stub,
            OTHER_PEER,
            &add_hostname_body("0", "git.example.com"),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(body_text(resp).await.contains("<errorCode>800</errorCode>"));
        let resp = control(&stub, PEER, &add_hostname_body("0", "git.example.com")).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn hostname_delete_rejects_unknown_peer() {
        let stub = HostnameStub::new(false);
        let resp = control(&stub, PEER, &delete_hostname_body("git.example.com")).await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(body_text(resp).await.contains("<errorCode>606</errorCode>"));
    }

    #[tokio::test]
    async fn hostname_delete_is_owner_scoped() {
        let stub = HostnameStub::new(true);
        let resp = control(&stub, PEER, &add_hostname_body("0", "git.example.com")).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = control(&stub, OTHER_PEER, &delete_hostname_body("git.example.com")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let resp = control(
            &stub,
            OTHER_PEER,
            &add_hostname_body("0", "git.example.com"),
        )
        .await;
        assert!(
            body_text(resp).await.contains("<errorCode>800</errorCode>"),
            "route should still be held by the original owner"
        );

        let resp = control(&stub, PEER, &delete_hostname_body("git.example.com")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let resp = control(
            &stub,
            OTHER_PEER,
            &add_hostname_body("0", "git.example.com"),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn hostname_actions_fault_801_without_sni_dataplane() {
        struct NoSniStub;
        impl GatewayBackend for NoSniStub {
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
                async { Some(EXT_IP) }
            }
            fn is_known_client(&self, _: Ipv4Addr) -> impl Future<Output = bool> + Send {
                async { true }
            }
            fn sni(&self) -> Option<&Arc<SniDemux>> {
                None
            }
            fn add_sni_forward(
                &self,
                _: SocketAddrV4,
                _: SocketAddrV4,
                _: &[String],
                _: Option<u32>,
            ) -> impl Future<Output = Result<(), u8>> + Send {
                async { panic!("add_sni_forward reached on a backend without an SNI dataplane") }
            }
        }

        let stub = NoSniStub;
        let body = add_hostname_body("0", "git.example.com");
        let resp = handle_control(&stub, PEER, &client_headers(&body), &body).await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(body_text(resp).await.contains("<errorCode>801</errorCode>"));
        let body = delete_hostname_body("git.example.com");
        let resp = handle_control(&stub, PEER, &client_headers(&body), &body).await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(body_text(resp).await.contains("<errorCode>801</errorCode>"));
    }
}
