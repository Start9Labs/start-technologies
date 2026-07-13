use std::net::Ipv4Addr;

use color_eyre::eyre::eyre;
use reqwest::Url;
use tokio::process::Command;

use crate::prelude::*;
use crate::util::Invoke;

/// BLOCKING. Point a `*.local` URL at its resolved address, leaving every other URL alone.
///
/// Our binaries are statically linked against musl, whose `getaddrinfo` implements no NSS:
/// `/etc/nsswitch.conf` is ignored, so `mdns4_minimal` is never consulted and a `.local` name
/// cannot be resolved in-process. The lookup falls through to unicast DNS and dies after musl's
/// hard-coded 5s timeout, which is why `curl` resolves a name that `start-cli` cannot.
///
/// Rewriting the URL once here, at context build, means every consumer downstream — the HTTP
/// client and the websocket path alike — is handed an address rather than a name. The server's
/// cert carries IP SANs (see [`crate::net::ssl`]), so TLS still verifies.
#[cfg(target_os = "linux")]
pub fn pin_mdns_host(url: &mut Url) -> Result<(), Error> {
    let Some(hostname) = url
        .host_str()
        .map(|host| host.trim_end_matches('.').to_owned())
        .filter(|host| host.ends_with(".local"))
    else {
        return Ok(());
    };
    let ip = resolve_system(&hostname)?;
    url.set_ip_host(ip).map_err(|_| {
        Error::new(
            eyre!("Cannot point url at resolved address {ip}"),
            crate::ErrorKind::ParseUrl,
        )
    })
}

/// darwin's `getaddrinfo` resolves `.local` through mDNSResponder natively, so there is nothing
/// to work around: the binary is linked against the system libc, not musl.
#[cfg(not(target_os = "linux"))]
pub fn pin_mdns_host(_url: &mut Url) -> Result<(), Error> {
    Ok(())
}

/// BLOCKING. Resolve a hostname through the *system* resolver by shelling out to `getent`.
///
/// `getent` is a separate, dynamically linked binary, so it goes through the host's real NSS
/// stack — exactly the path `curl` and `ping` take — and honors whatever that machine is
/// configured for: mDNS, `/etc/hosts`, LDAP. Unlike [`resolve_mdns`] it needs nothing installed
/// beyond libc; `avahi-resolve-host-name` lives in `avahi-utils`, which mDNS on Linux does not
/// depend on and which most desktops don't have.
#[cfg(target_os = "linux")]
fn resolve_system(hostname: &str) -> Result<std::net::IpAddr, Error> {
    let unresolved = || {
        Error::new(
            eyre!("Failed to resolve hostname: {hostname}"),
            crate::ErrorKind::Network,
        )
    };
    let output = std::process::Command::new("getent")
        .arg("ahosts")
        .arg(hostname)
        .output()
        .with_ctx(|_| (crate::ErrorKind::Network, "getent ahosts"))?;
    if !output.status.success() {
        return Err(unresolved());
    }
    // One line per socket type, address first — take whichever the resolver preferred:
    //     192.168.8.170   STREAM demo.local
    //     192.168.8.170   DGRAM
    String::from_utf8(output.stdout)?
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .find_map(|addr| addr.parse().ok())
        .ok_or_else(unresolved)
}

pub async fn resolve_mdns(hostname: &str) -> Result<Ipv4Addr, Error> {
    Ok(String::from_utf8(
        Command::new("avahi-resolve-host-name")
            .kill_on_drop(true)
            .arg("-4")
            .arg(hostname)
            .invoke(crate::ErrorKind::Network)
            .await?,
    )?
    .split_once("\t")
    .ok_or_else(|| {
        Error::new(
            eyre!("Failed to resolve hostname: {}", hostname),
            crate::ErrorKind::Network,
        )
    })?
    .1
    .trim()
    .parse()?)
}
