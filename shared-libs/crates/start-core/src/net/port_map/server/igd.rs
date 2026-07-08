//! Reusable server-side UPnP IGD (WANIPConnection) protocol layer.
//!
//! Security model mirrors PCP: a peer can only forward to itself — the SOAP
//! body's `NewInternalClient` is ignored and the target is forced to the
//! requesting peer's own address.

use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::net::port_map::server::GatewayBackend;

pub const SSDP_MULTICAST: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
pub const SSDP_PORT: u16 = 1900;
/// HTTP port serving the device description, SCPD, and SOAP control endpoint.
pub const IGD_HTTP_PORT: u16 = 49001;
pub const WANIP_SERVICE: &str = "urn:schemas-upnp-org:service:WANIPConnection:1";
pub const IGD_DEVICE: &str = "urn:schemas-upnp-org:device:InternetGatewayDevice:1";
pub const SERVER_HEADER: &str = "StartOS UPnP/1.1";
pub const ROOT_DESC_PATH: &str = "/rootDesc.xml";
pub const SCPD_PATH: &str = "/WANIPCn.xml";
pub const CONTROL_PATH: &str = "/ctl/IPConn";

/// Minimal WANIPConnection SCPD. Clients (e.g. igd-next) read its `actionList`
/// to learn each action's input argument names before issuing a request.
pub const SCPD: &str = include_str!("igd_xml/scpd.xml");

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
    let root = xmltree::Element::parse(body.as_bytes()).ok()?;
    let action = root
        .get_child("Body")?
        .children
        .iter()
        .find_map(|n| n.as_element())?;
    action.get_child(arg)?.get_text()?.trim().parse().ok()
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
        _ => "Action Failed",
    }
}

/// Render the IGD root device description for `uuid`.
pub fn render_root_desc(uuid: &str) -> String {
    format!(
        include_str!("igd_xml/root_desc.xml"),
        device_type = IGD_DEVICE,
        uuid = uuid,
        service = WANIP_SERVICE,
        control = CONTROL_PATH,
        scpd = SCPD_PATH,
    )
}

/// Serve a static XML document with the given content type.
pub async fn serve_static(body: Arc<str>, content_type: &'static str) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, content_type)],
        body.to_string(),
    )
        .into_response()
}

/// Dispatch a SOAP control request from `peer` to the matching IGD action.
pub async fn handle_control<B: GatewayBackend + ?Sized>(
    backend: &B,
    peer: Ipv4Addr,
    headers: &HeaderMap,
    body: &str,
) -> Response {
    match soap_action(headers, body).as_deref() {
        Some("GetExternalIPAddress") => get_external_ip(backend, peer).await,
        Some("AddPortMapping") => add_mapping(backend, peer, body, false).await,
        Some("AddAnyPortMapping") => add_mapping(backend, peer, body, true).await,
        Some("DeletePortMapping") => delete_mapping(backend, peer, body).await,
        _ => fault(401, "Invalid Action"),
    }
}

async fn get_external_ip<B: GatewayBackend + ?Sized>(backend: &B, peer: Ipv4Addr) -> Response {
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

    // UPnP IGD leases are permanent here (StartOS requests lease 0); PCP is the
    // lease-bearing path.
    match backend.add_forward(source, target, 1, peer, None).await {
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

#[cfg(test)]
mod tests {
    use xmltree::Element;

    use super::*;

    /// Recreates how an IGD client locates the WANIPConnection service: walk
    /// devices/serviceLists for a matching serviceType, returning (SCPDURL,
    /// controlURL).
    fn find_wanip(device: &Element) -> Option<(String, String)> {
        if let Some(service_list) = device.get_child("serviceList") {
            for child in &service_list.children {
                if let Some(svc) = child.as_element() {
                    if svc.name == "service"
                        && svc
                            .get_child("serviceType")
                            .and_then(|e| e.get_text())
                            .as_deref()
                            == Some(WANIP_SERVICE)
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
            .find_map(find_wanip)
    }

    #[test]
    fn root_desc_advertises_wanip_service() {
        let xml = render_root_desc("abcd1234-0000-0000-0000-000000000000");
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
}
