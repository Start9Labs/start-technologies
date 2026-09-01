//! Source-preserving transparent egress for the SNI demux.
//!
//! Upstream sockets bind the client address. Policy routing returns backend
//! replies to the local transparent socket without terminating TLS.

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

/// Firewall mark for transparent reply diversion.
pub const DIVERT_MARK: u32 = 0x0054_0001;
/// Default policy-routing table for diverted replies.
pub const DIVERT_TABLE: u32 = 1344;

/// Host-specific reply-diversion policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DivertConfig {
    /// Routing table for the local-delivery default route.
    pub route_table: u32,
    /// Priority of the fwmark policy rule.
    pub rule_priority: u32,
    /// Whether matching preserves unrelated firewall-mark bits.
    pub masked_fwmark: bool,
    /// Whether this process manages the nftables marking rule.
    pub manage_nft: bool,
}

impl Default for DivertConfig {
    fn default() -> Self {
        DivertConfig {
            route_table: DIVERT_TABLE,
            rule_priority: 49,
            masked_fwmark: false,
            manage_nft: true,
        }
    }
}

static DIVERT_CONFIG: std::sync::OnceLock<DivertConfig> = std::sync::OnceLock::new();

/// Installs diversion policy before first use.
/// Returns the rejected value when policy was already installed.
pub fn set_divert_config(cfg: DivertConfig) -> Result<(), DivertConfig> {
    DIVERT_CONFIG.set(cfg)
}

fn divert_config() -> &'static DivertConfig {
    DIVERT_CONFIG.get_or_init(DivertConfig::default)
}

fn fwmark_arg(cfg: &DivertConfig) -> String {
    if cfg.masked_fwmark {
        format!("{DIVERT_MARK:#x}/{DIVERT_MARK:#x}")
    } else {
        format!("{DIVERT_MARK:#x}")
    }
}

/// Nftables rules marking transparent-socket replies for local delivery.
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

/// Opens an upstream connection bound to the client source address.
/// Both addresses must use the same IP family.
#[cfg(target_os = "linux")]
pub async fn transparent_connect(
    client: SocketAddr,
    target: SocketAddr,
) -> std::io::Result<TcpStream> {
    let sock = match (client, target) {
        (SocketAddr::V4(_), SocketAddr::V4(_)) => {
            let sock = TcpSocket::new_v4()?;
            // IP_TRANSPARENT must precede bind.
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

/// Unsupported non-Linux implementation for cross-platform builds.
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

/// Initializes diversion once, retrying after failures.
pub async fn ensure_divert_infra_once() -> Result<(), Error> {
    DIVERT_INFRA
        .get_or_try_init(|| async { ensure_divert_infra().await.map(|_| ()) })
        .await
        .map(|_| ())
}

static DIVERT_ASSERT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Reconciles policy-routing and optional nftables rules.
/// Returns whether any missing rule was restored.
pub async fn ensure_divert_infra() -> Result<bool, Error> {
    let _guard = DIVERT_ASSERT.lock().await;
    let cfg = divert_config();
    let mut repaired = false;
    let table = cfg.route_table.to_string();
    let priority = cfg.rule_priority.to_string();
    let fwmark = fwmark_arg(cfg);

    for (flag, default_route, family) in [("-4", "0.0.0.0/0", "ip"), ("-6", "::/0", "ip6")] {
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

        // Divert replies before per-interface symmetric-return rules.
        let rules = Command::new("ip")
            .args([flag, "rule", "list"])
            .invoke(ErrorKind::Network)
            .await
            .unwrap_or_default();
        if !String::from_utf8_lossy(&rules).contains(&format!("lookup {table}")) {
            Command::new("ip")
                .args([
                    flag, "rule", "add", "fwmark", &fwmark, "lookup", &table, "priority", &priority,
                ])
                .invoke(ErrorKind::Network)
                .await?;
            repaired = true;
        }

        // Install marking only when the host firewall does not own it.
        if !cfg.manage_nft {
            continue;
        }
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

    // Strict reverse-path filtering accepts backend-routable reply sources.
    Ok(repaired)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_legacy_behavior() {
        let cfg = DivertConfig::default();
        assert_eq!(cfg.route_table, DIVERT_TABLE);
        assert_eq!(cfg.rule_priority, 49);
        assert_eq!(fwmark_arg(&cfg), format!("{DIVERT_MARK:#x}"));
        assert!(cfg.manage_nft);
    }

    #[test]
    fn masked_fwmark_matches_or_set_marks() {
        let cfg = DivertConfig {
            masked_fwmark: true,
            ..DivertConfig::default()
        };
        assert_eq!(fwmark_arg(&cfg), "0x540001/0x540001");
    }
}
