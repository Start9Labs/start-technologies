//! UPnP IGD client helpers — the fallback behind PCP/NAT-PMP (see
//! [`crate::net::port_map`]). Discovery binds to the gateway interface's local
//! address so the SSDP M-SEARCH leaves via that gateway, covering a home router
//! and a StartTunnel IGD over WireGuard (see [`crate::tunnel::forward::igd`]).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use igd_next::aio::Gateway;
use igd_next::aio::tokio::{Tokio, search_gateway};
use igd_next::{PortMappingProtocol, SearchOptions};

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

/// Map `external_port` -> `local_ip:internal_port` (TCP) on `gateway`.
pub async fn add_port(
    gateway: &Gateway<Tokio>,
    external_port: u16,
    local_ip: Ipv4Addr,
    internal_port: u16,
) -> Result<(), Error> {
    let call = gateway.add_port(
        PortMappingProtocol::TCP,
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

/// Remove the TCP mapping for `external_port`; a missing mapping is not an error.
pub async fn remove_port(gateway: &Gateway<Tokio>, external_port: u16) -> Result<(), Error> {
    let call = gateway.remove_port(PortMappingProtocol::TCP, external_port);
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

/// External IPv4 of the IGD reachable from `local_ip` (UPnP
/// `GetExternalIPAddress`). `Ok(None)` means no usable public address — a
/// private/CGNAT result is discarded so the caller falls back to an echoip query.
pub async fn get_external_ipv4(local_ip: Ipv4Addr) -> Result<Option<Ipv4Addr>, Error> {
    let gateway = discover(local_ip).await?;
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
