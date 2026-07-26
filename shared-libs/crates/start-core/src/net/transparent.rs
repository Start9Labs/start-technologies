//! Source-preserving transparent egress for the SNI demux (RFC §4.6).
//!
//! A demux proxy reads the TLS ClientHello on a normal listener (the host is the
//! legitimate destination of the inbound flow), then originates the internal leg
//! with the *client's* source address via `IP_TRANSPARENT`, so the backend sees
//! the real peer rather than this host. Backend→client replies (addressed to the
//! client, transiting this host as the backend's gateway) are diverted back into
//! the proxy socket by policy routing.
//!
//! Datapath (`table ip`/`ip6 startos` + iproute2, mirrored per family):
//! - egress socket: `IP_TRANSPARENT`/`IPV6_TRANSPARENT`, bound to the client's
//!   `(ip, port)`; the client and backend legs are necessarily one family.
//! - `mangle_prerouting` `sni-divert`: an inbound packet matching a local
//!   `IP_TRANSPARENT` socket (i.e. a reply to such an egress) is marked with
//!   [`DIVERT_MARK`]. Only the inbound/reply direction is touched, so the
//!   proxy's own egress packets route to the backend normally.
//! - `ip [-6] rule fwmark DIVERT_MARK lookup DIVERT_TABLE priority 49` + `ip [-6]
//!   route add local 0.0.0.0/0`|`::/0` `dev lo table DIVERT_TABLE`: deliver the
//!   marked replies locally, into the transparent socket. v4 and v6 keep
//!   separate rule/route tables, so one number serves both.

use std::net::SocketAddr;

#[cfg(target_os = "linux")]
use tokio::net::TcpSocket;
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::sync::OnceCell;

#[cfg(target_os = "linux")]
use crate::net::utils::default_keepalive;
use crate::prelude::*;
use crate::util::Invoke;

/// Firewall mark for transparent-egress reply diversion. Outside the gateway's
/// per-interface `1000 + ifindex` mark space and its priority-50 rule.
pub const DIVERT_MARK: u32 = 0x0054_0001;
/// Dedicated routing table holding the local-delivery default for diverted
/// replies. Outside the gateway's `1000 + ifindex` table space.
pub const DIVERT_TABLE: u32 = 1344;

/// `mangle_prerouting` rule that marks inbound packets belonging to a local
/// `IP_TRANSPARENT` (SNI-demux) socket — the replies to a source-preserving
/// egress connection — so the priority-49 `ip rule` diverts them to the local
/// table and they reach the proxy socket instead of being forwarded back out.
/// Touches only the reply direction (the egress leg is in `output`, not here),
/// so it cannot misroute the proxy's own outbound packets. Spliced into the
/// gateway mangle reconcile so it survives that chain's flush; on hosts without
/// that reconcile (e.g. the tunnel) [`ensure_divert_infra`] adds it directly.
pub fn divert_mark_rule() -> String {
    [
        divert_mark_rule_family("ip"),
        divert_mark_rule_family("ip6"),
    ]
    .concat()
}

fn divert_mark_rule_family(family: &str) -> String {
    format!(
        "add rule {family} startos mangle_prerouting meta l4proto tcp socket transparent 1 meta mark set {DIVERT_MARK:#010x} comment \"sni-divert\"\n"
    )
}

/// Open the internal leg of a demuxed connection from the client's own source
/// address, so the backend sees the real peer. Requires `CAP_NET_ADMIN` (startd
/// runs as root). The reply path is set up by [`ensure_divert_infra`]. `client`
/// and `target` must be the same family — the source address is bound on the
/// socket that dials the target.
#[cfg(target_os = "linux")]
pub async fn transparent_connect(
    client: SocketAddr,
    target: SocketAddr,
) -> std::io::Result<TcpStream> {
    let sock = match (client, target) {
        (SocketAddr::V4(_), SocketAddr::V4(_)) => {
            let sock = TcpSocket::new_v4()?;
            // Must precede bind: permits binding a non-local (client) address.
            socket2::SockRef::from(&sock).set_ip_transparent_v4(true)?;
            sock
        }
        (SocketAddr::V6(_), SocketAddr::V6(_)) => {
            let sock = TcpSocket::new_v6()?;
            socket2::SockRef::from(&sock).set_ip_transparent_v6(true)?;
            sock
        }
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("transparent egress needs one family: client {client}, target {target}"),
            ));
        }
    };
    {
        let sref = socket2::SockRef::from(&sock);
        sref.set_reuse_address(true)?;
        if let Err(e) = sref.set_tcp_keepalive(&default_keepalive()) {
            tracing::debug!("transparent egress keepalive: {e}");
        }
    }
    sock.bind(client)?;
    sock.connect(target).await
}

/// `IP_TRANSPARENT` is Linux-only and the SNI demux runs only on the Linux
/// gateway, so this stub never executes off-Linux; it exists to keep the
/// cross-platform build (apple-darwin) compiling.
#[cfg(not(target_os = "linux"))]
pub async fn transparent_connect(
    _client: SocketAddr,
    _target: SocketAddr,
) -> std::io::Result<TcpStream> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "IP_TRANSPARENT transparent egress is Linux-only",
    ))
}

static DIVERT_INFRA: OnceCell<()> = OnceCell::const_new();

/// [`ensure_divert_infra`] serialized and run at most once per process (cached
/// only on success, so a transient failure is retried on the next call). Cheap
/// to call per connection from the local passthrough path. Serialization keeps
/// concurrent callers from racing the check-then-add commands (EEXIST).
pub async fn ensure_divert_infra_once() -> Result<(), Error> {
    DIVERT_INFRA
        .get_or_try_init(|| async { ensure_divert_infra().await.map(|_| ()) })
        .await
        .map(|_| ())
}

/// Serializes [`ensure_divert_infra`]'s check-then-add sequences: concurrent
/// callers (listener startup vs the periodic re-assert) would otherwise race
/// into EEXIST.
static DIVERT_ASSERT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Install the reply-path divert (idempotent): the iproute2 half (rule + table)
/// always, plus the nft `sni-divert` mark rule when absent — so hosts that run the
/// SNI demux but not the gateway mangle reconcile (e.g. the tunnel) still mark and
/// divert replies. Safe to call repeatedly; returns whether a missing piece had
/// to be (re-)added, so periodic callers can surface external flushes.
pub async fn ensure_divert_infra() -> Result<bool, Error> {
    let _guard = DIVERT_ASSERT.lock().await;
    let mut repaired = false;
    let table = DIVERT_TABLE.to_string();

    // Both families, and independently: `ip` and `ip -6` keep separate rule and
    // route tables, so the same DIVERT_TABLE/DIVERT_MARK numbers serve each.
    for (flag, default_route, family) in [("-4", "0.0.0.0/0", "ip"), ("-6", "::/0", "ip6")] {
        // Local-delivery default in the divert table: marked replies are delivered
        // to the local transparent socket instead of being forwarded back out.
        Command::new("ip")
            .args([
                flag,
                "route",
                "replace",
                "local",
                default_route,
                "dev",
                "lo",
                "table",
                &table,
            ])
            .invoke(ErrorKind::Network)
            .await?;

        // Policy rule at priority 49 — above the gateway's per-interface symmetric
        // -return rules at 50 — so diverted replies win.
        let rules = Command::new("ip")
            .args([flag, "rule", "list"])
            .invoke(ErrorKind::Network)
            .await
            .unwrap_or_default();
        if !String::from_utf8_lossy(&rules).contains(&format!("lookup {table}")) {
            Command::new("ip")
                .args([
                    flag,
                    "rule",
                    "add",
                    "fwmark",
                    &format!("{DIVERT_MARK:#x}"),
                    "lookup",
                    &table,
                    "priority",
                    "49",
                ])
                .invoke(ErrorKind::Network)
                .await?;
            repaired = true;
        }

        // nft `sni-divert` mark rule. The gateway reconcile owns (and re-adds) it on
        // hosts that run it; install it directly where nothing else does (the tunnel),
        // skipping if already present so we never duplicate or fight the reconcile.
        let chain = Command::new("nft")
            .args(["list", "chain", family, "startos", "mangle_prerouting"])
            .invoke(ErrorKind::Network)
            .await
            .unwrap_or_default();
        if !String::from_utf8_lossy(&chain).contains("sni-divert") {
            Command::new("nft")
                .args([
                    "add",
                    "rule",
                    family,
                    "startos",
                    "mangle_prerouting",
                    "meta",
                    "l4proto",
                    "tcp",
                    "socket",
                    "transparent",
                    "1",
                    "meta",
                    "mark",
                    "set",
                    &format!("{DIVERT_MARK:#010x}"),
                    "comment",
                    "sni-divert",
                ])
                .invoke(ErrorKind::Network)
                .await?;
            repaired = true;
        }
    }

    // rp_filter needs no loosening: a diverted reply's source is the backend,
    // routable via its own ingress interface, so even strict RPF (1) accepts it
    // (verified in a netns harness under both strict and loose).
    Ok(repaired)
}
