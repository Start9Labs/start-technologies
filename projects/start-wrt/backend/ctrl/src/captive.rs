use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Router;
use startos::net::http::request_authority;
use tokio::io::AsyncWriteExt;

use crate::prelude::*;
use crate::ssl::ROUTER_HOSTNAME;

const CAPTIVE_CONF_NAME: &str = "captive-portal.conf";
const CAPTIVE_CONTENT: &str = "\
    address=/#/192.168.0.1\n\
    address=/#/::\n\
    dhcp-option=114,http://192.168.0.1/\n";

/// Mirrors the presence of the dnsmasq drop-in.
static ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn is_active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Outside `inet fw4`, which fw4 reloads replace.
const BLOCK_RULESET: &str = "\
table inet startwrt_captive
delete table inet startwrt_captive
table inet startwrt_captive {
\tchain forward {
\t\ttype filter hook forward priority filter - 10; policy accept;
\t\tmeta l4proto tcp reject with tcp reset
\t\treject with icmpx type admin-prohibited
\t}
\tchain dstnat {
\t\ttype nat hook prerouting priority dstnat - 10; policy accept;
\t\tmeta l4proto { tcp, udp } th dport 53 redirect to :53
\t}
}
";

const UNBLOCK_RULESET: &str = "\
table inet startwrt_captive
delete table inet startwrt_captive
";

/// The dnsmasq instance serving the LAN and its conf-dir. Neither is a fixed
/// path: OpenWrt names both after the UCI section, e.g.
/// `/tmp/dnsmasq.cfg01411c.d/`.
fn find_dnsmasq_conf_dir() -> Result<(String, String), Error> {
    lan_instance(crate::devices::dnsmasq_instance_option("conf-dir")).ok_or_else(|| {
        Error::new(
            eyre!("no dnsmasq conf-dir found in /var/etc/dnsmasq.conf.*"),
            ErrorKind::NotFound,
        )
    })
}

/// The LAN profile's instance, else OpenWrt's unnamed one, which serves the
/// LAN while no profile instance exists.
fn lan_instance(mut instances: Vec<(String, String)>) -> Option<(String, String)> {
    instances.sort_by_key(|(name, _)| match name.as_str() {
        "dns_lan" => 0,
        name if name.starts_with("cfg") => 1,
        _ => 2,
    });
    instances.into_iter().next()
}

/// Check if the captive portal DNS hijack is currently active.
pub fn is_captive_portal_active() -> bool {
    find_dnsmasq_conf_dir()
        .map(|(_, dir)| Path::new(&dir).join(CAPTIVE_CONF_NAME).exists())
        .unwrap_or(false)
}

pub async fn enable_captive_portal() -> Result<(), Error> {
    apply_ruleset(BLOCK_RULESET).await?;
    enable_dns_hijack().await
}

/// Unblocks forwarding even when restoring DNS fails.
pub async fn disable_captive_portal() -> Result<(), Error> {
    let unblocked = apply_ruleset(UNBLOCK_RULESET).await;
    disable_dns_hijack().await?;
    unblocked
}

async fn enable_dns_hijack() -> Result<(), Error> {
    let (instance, dir) = find_dnsmasq_conf_dir()?;
    let conf = Path::new(&dir).join(CAPTIVE_CONF_NAME);

    if !Path::new(&dir).exists() {
        fs::create_dir_all(&dir)
            .map_err(|e| Error::new(eyre!("failed to create {dir}: {e}"), ErrorKind::Filesystem))?;
    }

    fs::write(&conf, CAPTIVE_CONTENT).map_err(|e| {
        Error::new(
            eyre!("failed to write {}: {e}", conf.display()),
            ErrorKind::Filesystem,
        )
    })?;
    ACTIVE.store(true, Ordering::Relaxed);

    restart_dnsmasq(&instance).await
}

async fn disable_dns_hijack() -> Result<(), Error> {
    let (instance, dir) = find_dnsmasq_conf_dir()?;
    let conf = Path::new(&dir).join(CAPTIVE_CONF_NAME);

    if conf.exists() {
        fs::remove_file(&conf).map_err(|e| {
            Error::new(
                eyre!("failed to remove {}: {e}", conf.display()),
                ErrorKind::Filesystem,
            )
        })?;
    }
    ACTIVE.store(false, Ordering::Relaxed);

    restart_dnsmasq(&instance).await
}

/// Check if the admin (root) password has been set in /etc/shadow.
///
/// Synchronous version of the check in auth.rs — reads /etc/shadow directly.
pub fn is_admin_password_set() -> Result<bool, Error> {
    let shadow = fs::read_to_string("/etc/shadow").map_err(|e| {
        Error::new(
            eyre!("failed to read /etc/shadow: {e}"),
            ErrorKind::Filesystem,
        )
    })?;
    Ok(root_password_set(&shadow))
}

pub(crate) fn root_password_set(shadow: &str) -> bool {
    shadow
        .lines()
        .map(|line| line.split(':').collect::<Vec<_>>())
        .find(|parts| parts.len() >= 2 && parts[0] == "root")
        .is_some_and(|parts| !matches!(parts[1], "" | "*" | "!" | "x"))
}

/// Evaluate current state and enable/disable captive portal accordingly.
///
/// - If no admin password is set → enable captive portal
/// - If admin password is set → disable captive portal
///
/// This is an async wrapper for use from the daemon's boot sequence.
pub async fn ensure_captive_portal_state() -> Result<(), Error> {
    let password_set = is_admin_password_set()?;
    let active = is_captive_portal_active();

    ACTIVE.store(active, Ordering::Relaxed);

    if password_set {
        apply_ruleset(UNBLOCK_RULESET).await?;
        if active {
            tracing::info!("admin password set, disabling captive portal");
            disable_dns_hijack().await?;
        }
    } else {
        apply_ruleset(BLOCK_RULESET).await?;
        if !active {
            tracing::info!("no admin password set, enabling captive portal");
            enable_dns_hijack().await?;
        }
    }

    Ok(())
}

/// `router.lan` and address literals name the router.
pub(crate) fn serves(host: &str) -> bool {
    host.eq_ignore_ascii_case(ROUTER_HOSTNAME)
        || host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(host)
            .parse::<IpAddr>()
            .is_ok()
}

/// Sends a request for any name but the router's own to `router.lan` while the
/// portal is active.
pub fn redirect_to_router(router: Router) -> Router {
    router.layer(axum::middleware::from_fn(
        |req: Request, next: Next| async move {
            match respond(is_active(), &req) {
                Some(response) => response,
                None => next.run(req).await,
            }
        },
    ))
}

fn respond(active: bool, req: &Request) -> Option<Response> {
    if !active || request_authority(req).is_some_and(|authority| serves(authority.host())) {
        return None;
    }
    Some(
        (
            http::StatusCode::FOUND,
            [(http::header::LOCATION, format!("http://{ROUTER_HOSTNAME}/"))],
        )
            .into_response(),
    )
}

async fn apply_ruleset(ruleset: &str) -> Result<(), Error> {
    let nft_err = |e: std::io::Error| Error::new(eyre!("nft: {e}"), ErrorKind::Network);
    let mut child = tokio::process::Command::new("nft")
        .args(["-f", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(nft_err)?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(ruleset.as_bytes()).await.map_err(nft_err)?;
    }
    let output = child.wait_with_output().await.map_err(nft_err)?;
    if !output.status.success() {
        return Err(Error::new(
            eyre!(
                "nft exited {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            ErrorKind::Network,
        ));
    }
    Ok(())
}

/// Reload ignores the drop-in, which lies outside the generated config. A
/// restart without an instance unregisters dnsmasq's interface triggers.
async fn restart_dnsmasq(instance: &str) -> Result<(), Error> {
    let status = crate::run_quiet_async(
        tokio::process::Command::new("/etc/init.d/dnsmasq").args(["restart", instance]),
    )
    .await
    .map_err(|e| {
        Error::new(
            eyre!("failed to restart dnsmasq instance {instance}: {e}"),
            ErrorKind::Network,
        )
    })?;

    if !status.success() {
        return Err(Error::new(
            eyre!(
                "dnsmasq instance {instance} restart failed (exit {})",
                status.code().unwrap_or(-1)
            ),
            ErrorKind::Network,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;

    use super::*;

    fn request(uri: &str, host: Option<&str>) -> Request {
        let mut req = Request::builder().uri(uri).body(Body::empty()).unwrap();
        if let Some(host) = host {
            req.headers_mut()
                .insert(http::header::HOST, host.parse().unwrap());
        }
        req
    }

    #[test]
    fn the_hijack_targets_the_lan_instance_whatever_the_listing_order() {
        let inst = |name: &str| (name.to_string(), format!("/tmp/dnsmasq.{name}.d"));
        assert_eq!(
            lan_instance(vec![inst("dns_guest"), inst("cfg01411c"), inst("dns_lan")]),
            Some(inst("dns_lan"))
        );
        assert_eq!(
            lan_instance(vec![inst("dns_guest"), inst("cfg01411c")]),
            Some(inst("cfg01411c"))
        );
        assert_eq!(
            lan_instance(vec![inst("cfg01411c")]),
            Some(inst("cfg01411c"))
        );
        assert_eq!(lan_instance(Vec::new()), None);
    }

    #[test]
    fn the_router_is_served_by_name_and_by_address() {
        for host in [
            "router.lan",
            "ROUTER.LAN",
            "router.lan:80",
            "192.168.0.1",
            "192.168.0.1:443",
            "[fd12:3456:789a::1]",
            "[fd12:3456:789a::1]:443",
        ] {
            assert!(respond(true, &request("/", Some(host))).is_none(), "{host}");
        }
    }

    #[test]
    fn every_other_name_is_sent_to_the_router() {
        for host in [
            "apple.com",
            "captive.apple.com",
            "connectivitycheck.gstatic.com:80",
            "www.msftconnecttest.com",
        ] {
            let response = respond(true, &request("/hotspot-detect.html?x=1", Some(host))).unwrap();
            assert_eq!(response.status(), http::StatusCode::FOUND, "{host}");
            assert_eq!(
                response.headers()[http::header::LOCATION],
                "http://router.lan/",
                "{host}"
            );
        }
        assert!(respond(true, &request("/", None)).is_some());
    }

    #[test]
    fn an_absolute_uri_outranks_the_host_header() {
        assert!(respond(true, &request("http://router.lan/rpc/v1", None)).is_none());
        assert!(respond(true, &request("http://apple.com/", Some("router.lan"))).is_some());
    }

    #[test]
    fn an_inactive_portal_serves_every_name() {
        assert!(respond(false, &request("/", Some("apple.com"))).is_none());
        assert!(respond(false, &request("/", None)).is_none());
    }

    #[test]
    fn sni_follows_the_same_rule() {
        assert!(serves("router.lan"));
        assert!(serves("192.168.0.1"));
        assert!(!serves("apple.com"));
        assert!(!serves("lan"));
        assert!(!serves(""));
    }

    #[test]
    fn root_hash_counts_as_set() {
        assert!(root_password_set(
            "root:$6$salt$hash:19000:0:99999:7:::\ndaemon:*:0:0:99999:7:::\n"
        ));
    }

    #[test]
    fn factory_shadow_has_no_password() {
        assert!(!root_password_set(
            "root:::0:99999:7:::\ndaemon:*:0:0:99999:7:::\n"
        ));
    }

    #[test]
    fn locked_root_has_no_password() {
        for hash in ["*", "!", "x"] {
            assert!(!root_password_set(&format!("root:{hash}:0:0:99999:7:::\n")));
        }
    }

    #[test]
    fn other_users_do_not_count() {
        assert!(!root_password_set("daemon:$6$salt$hash:0:0:99999:7:::\n"));
    }
}
