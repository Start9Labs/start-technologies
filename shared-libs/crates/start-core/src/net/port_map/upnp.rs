//! UPnP IGD client helpers — the fallback behind PCP/NAT-PMP (see
//! [`crate::net::port_map`]). Discovery binds to the gateway interface's local
//! address so the SSDP M-SEARCH leaves via that gateway, covering a home router
//! and a StartTunnel IGD over WireGuard (see [`crate::tunnel::forward::igd`]).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use igd_next::aio::Gateway;
use igd_next::aio::tokio::{Tokio, search_gateway};
use igd_next::{PortMappingProtocol, SearchOptions};

use crate::net::port_map::pcp::hostname::validate_hostname;
use crate::net::port_map::server::igd::{
    ADD_HOSTNAME_ACTION, DELETE_HOSTNAME_ACTION, WANIP_SERVICE,
};
use crate::prelude::*;

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(4);
/// IGD SOAP control calls are unbounded in igd-next; without this a gateway that
/// accepts TCP but never answers would wedge the single-threaded port-map daemon.
const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);
/// `0` requests an indefinite lease; the controller re-asserts periodically.
const LEASE_DURATION: u32 = 0;
const DESCRIPTION: &str = "StartOS";

fn search_options(local_ip: Ipv4Addr) -> SearchOptions {
    SearchOptions {
        bind_addr: SocketAddr::new(IpAddr::V4(local_ip), 0),
        timeout: Some(DISCOVERY_TIMEOUT),
        single_search_timeout: Some(DISCOVERY_TIMEOUT),
        ..Default::default()
    }
}

/// Discover the IGD reachable from `local_ip` (SSDP M-SEARCH out that interface).
pub async fn discover(local_ip: Ipv4Addr) -> Result<Gateway<Tokio>, Error> {
    search_gateway(search_options(local_ip)).await.map_err(|e| {
        Error::new(
            eyre!("UPnP gateway discovery failed: {e}"),
            ErrorKind::Network,
        )
    })
}

/// Map `external_port` -> `local_ip:internal_port` for `protocol` on `gateway`.
pub async fn add_port(
    gateway: &Gateway<Tokio>,
    protocol: PortMappingProtocol,
    external_port: u16,
    local_ip: Ipv4Addr,
    internal_port: u16,
) -> Result<(), Error> {
    let call = gateway.add_port(
        protocol,
        external_port,
        SocketAddr::new(IpAddr::V4(local_ip), internal_port),
        LEASE_DURATION,
        DESCRIPTION,
    );
    match tokio::time::timeout(CONTROL_TIMEOUT, call).await {
        Ok(r) => {
            r.map_err(|e| Error::new(eyre!("UPnP AddPortMapping failed: {e}"), ErrorKind::Network))
        }
        Err(_) => Err(Error::new(
            eyre!("UPnP AddPortMapping timed out"),
            ErrorKind::Network,
        )),
    }
}

/// Remove the mapping for `protocol` and `external_port`; a missing mapping is
/// not an error.
pub async fn remove_port(
    gateway: &Gateway<Tokio>,
    protocol: PortMappingProtocol,
    external_port: u16,
) -> Result<(), Error> {
    let call = gateway.remove_port(protocol, external_port);
    match tokio::time::timeout(CONTROL_TIMEOUT, call).await {
        Ok(Ok(())) | Ok(Err(igd_next::RemovePortError::NoSuchPortMapping)) => Ok(()),
        Ok(Err(e)) => Err(Error::new(
            eyre!("UPnP DeletePortMapping failed: {e}"),
            ErrorKind::Network,
        )),
        Err(_) => Err(Error::new(
            eyre!("UPnP DeletePortMapping timed out"),
            ErrorKind::Network,
        )),
    }
}

/// Lease we request for a hostname mapping; the server clamps to its own max
/// (3600s) and the controller re-asserts every refresh tick, well within it.
const HOSTNAME_LEASE_SECONDS: u32 = 3600;

/// Whether `gateway` advertises the Start9 hostname vendor action. A plain
/// lookup: `discover()` already parsed the SCPD into `control_schema`, so
/// capability detection costs no extra round trip.
pub fn supports_hostname(gateway: &Gateway<Tokio>) -> bool {
    gateway.control_schema.contains_key(ADD_HOSTNAME_ACTION)
}

/// SOAP envelope for [`ADD_HOSTNAME_ACTION`], shaped like igd-next's
/// `format_add_port_mapping_message` (which the server's parser is tested
/// against). The caller has validated the hostname to `[A-Za-z0-9.*-]`
/// ([`add_hostname_mapping`]), so interpolation needs no XML escaping.
pub(crate) fn add_hostname_body(
    external_port: u16,
    local_ip: Ipv4Addr,
    internal_port: u16,
    hostname: &str,
) -> String {
    format!(
        r#"<?xml version="1.0"?>
<s:Envelope s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/" xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
<s:Body>
<u:{ADD_HOSTNAME_ACTION} xmlns:u="{service}">
<NewRemoteHost></NewRemoteHost>
<NewExternalPort>{external_port}</NewExternalPort>
<NewProtocol>TCP</NewProtocol>
<NewInternalPort>{internal_port}</NewInternalPort>
<NewInternalClient>{local_ip}</NewInternalClient>
<NewEnabled>1</NewEnabled>
<NewPortMappingDescription>{DESCRIPTION}</NewPortMappingDescription>
<NewLeaseDuration>{HOSTNAME_LEASE_SECONDS}</NewLeaseDuration>
<NewHostname>{hostname}</NewHostname>
</u:{ADD_HOSTNAME_ACTION}>
</s:Body>
</s:Envelope>"#,
        service = WANIP_SERVICE,
    )
}

/// SOAP envelope for [`DELETE_HOSTNAME_ACTION`]. Carries the internal port so
/// the server can reconstruct the peer-scoped route target it is deleting.
pub(crate) fn delete_hostname_body(
    external_port: u16,
    internal_port: u16,
    hostname: &str,
) -> String {
    format!(
        r#"<?xml version="1.0"?>
<s:Envelope s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/" xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
<s:Body>
<u:{DELETE_HOSTNAME_ACTION} xmlns:u="{service}">
<NewRemoteHost></NewRemoteHost>
<NewExternalPort>{external_port}</NewExternalPort>
<NewProtocol>TCP</NewProtocol>
<NewInternalPort>{internal_port}</NewInternalPort>
<NewHostname>{hostname}</NewHostname>
</u:{DELETE_HOSTNAME_ACTION}>
</s:Body>
</s:Envelope>"#,
        service = WANIP_SERVICE,
    )
}

/// POST a vendor-action SOAP request to `gateway`'s control endpoint.
/// igd-next's own `perform_request` is private, so this hand-rolls the same
/// HTTP shape (`SOAPAction: "<service>#<action>"`, text/xml body).
async fn vendor_control_call(
    gateway: &Gateway<Tokio>,
    action: &str,
    body: String,
) -> Result<(), Error> {
    let url = format!("http://{}{}", gateway.addr, gateway.control_url);
    let soap_action = format!("\"{WANIP_SERVICE}#{action}\"");
    let call = async {
        // no_proxy: this is a LAN control call; reqwest otherwise honors
        // HTTP_PROXY/ALL_PROXY, which igd-next's transport ignores — a proxy
        // env would break (or leak) only the vendor actions.
        let resp = reqwest::Client::builder()
            .no_proxy()
            .build()
            .with_kind(ErrorKind::Network)?
            .post(&url)
            .header("SOAPAction", soap_action)
            .header(reqwest::header::CONTENT_TYPE, "text/xml; charset=\"utf-8\"")
            .body(body)
            .send()
            .await
            .map_err(|e| Error::new(eyre!("UPnP {action} failed: {e}"), ErrorKind::Network))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        // Require the action's own <...Response> element, not just a 2xx — an
        // intercepting middlebox answering 200 is not a created mapping.
        if status.is_success() && text.contains(&format!("{action}Response")) {
            Ok(())
        } else {
            // A fault body carries <errorCode>; surface it for diagnostics.
            let code = text
                .split("<errorCode>")
                .nth(1)
                .and_then(|t| t.split("</errorCode>").next())
                .unwrap_or("unknown");
            Err(Error::new(
                eyre!("UPnP {action} failed: HTTP {status}, UPnP error {code}"),
                ErrorKind::Network,
            ))
        }
    };
    match tokio::time::timeout(CONTROL_TIMEOUT, call).await {
        Ok(r) => r,
        Err(_) => Err(Error::new(
            eyre!("UPnP {action} timed out"),
            ErrorKind::Network,
        )),
    }
}

/// Bind `hostname` on `external_port` -> `local_ip:internal_port` via the
/// Start9 vendor action. The gateway SNI-demuxes the shared external port; the
/// granted lease is finite, so the caller must re-assert before expiry.
pub async fn add_hostname_mapping(
    gateway: &Gateway<Tokio>,
    external_port: u16,
    local_ip: Ipv4Addr,
    internal_port: u16,
    hostname: &str,
) -> Result<(), Error> {
    // Nothing upstream character-validates a configured domain, and the
    // envelope interpolates it unescaped — reject here rather than emit
    // malformed XML the server can only 402.
    if !validate_hostname(hostname) {
        return Err(Error::new(
            eyre!("invalid hostname for SNI mapping: {hostname:?}"),
            ErrorKind::InvalidRequest,
        ));
    }
    vendor_control_call(
        gateway,
        ADD_HOSTNAME_ACTION,
        add_hostname_body(external_port, local_ip, internal_port, hostname),
    )
    .await
}

/// Remove the SNI route for `hostname` on `external_port` targeting
/// `internal_port` on the caller.
pub async fn remove_hostname_mapping(
    gateway: &Gateway<Tokio>,
    external_port: u16,
    internal_port: u16,
    hostname: &str,
) -> Result<(), Error> {
    if !validate_hostname(hostname) {
        return Err(Error::new(
            eyre!("invalid hostname for SNI mapping: {hostname:?}"),
            ErrorKind::InvalidRequest,
        ));
    }
    vendor_control_call(
        gateway,
        DELETE_HOSTNAME_ACTION,
        delete_hostname_body(external_port, internal_port, hostname),
    )
    .await
}

/// Whether `ip` is a routable public IPv4 worth reporting. A gateway behind
/// CGNAT/double-NAT reports a private external IP, useless for clearnet, so the
/// caller falls back to an echoip probe.
pub(crate) fn is_wan_candidate(ip: Ipv4Addr) -> bool {
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.octets()[0] == 0)
}

/// External IPv4 of an already-discovered `gateway` — the same as
/// [`get_external_ipv4`] but reusing the caller's discovery, so one SSDP round
/// yields both the UPnP capability verdict and the WAN address.
pub async fn external_ipv4(gateway: &Gateway<Tokio>) -> Result<Option<Ipv4Addr>, Error> {
    match tokio::time::timeout(CONTROL_TIMEOUT, gateway.get_external_ip()).await {
        Ok(Ok(IpAddr::V4(ip))) if is_wan_candidate(ip) => Ok(Some(ip)),
        Ok(Ok(_)) => Ok(None),
        Ok(Err(e)) => Err(Error::new(
            eyre!("UPnP GetExternalIPAddress failed: {e}"),
            ErrorKind::Network,
        )),
        Err(_) => Err(Error::new(
            eyre!("UPnP GetExternalIPAddress timed out"),
            ErrorKind::Network,
        )),
    }
}

/// External IPv4 of the IGD reachable from `local_ip` (UPnP
/// `GetExternalIPAddress`). `Ok(None)` means no usable public address — a
/// private/CGNAT result is discarded so the caller falls back to an echoip query.
pub async fn get_external_ipv4(local_ip: Ipv4Addr) -> Result<Option<Ipv4Addr>, Error> {
    external_ipv4(&discover(local_ip).await?).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_external_ips_trigger_echoip_fallback() {
        // Anything not routable on the public Internet must be rejected so the
        // caller falls back to an echoip query.
        for ip in [
            "10.0.0.1",
            "10.255.1.2",
            "172.16.0.1",
            "172.31.255.254",
            "192.168.1.1",
            "169.254.1.1", // link-local
            "127.0.0.1",   // loopback
            "0.0.0.0",     // unspecified
            "255.255.255.255",
        ] {
            assert!(
                !is_wan_candidate(ip.parse().unwrap()),
                "{ip} should be rejected as non-public"
            );
        }
    }

    #[test]
    fn public_external_ips_are_accepted() {
        for ip in ["1.2.3.4", "8.8.8.8", "1.1.1.1", "93.184.216.34"] {
            assert!(
                is_wan_candidate(ip.parse().unwrap()),
                "{ip} should be accepted as public"
            );
        }
    }
}
