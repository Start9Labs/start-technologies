use std::collections::BTreeSet;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWrite};
use tokio::process::Command;

use crate::tunnel::wg::WIREGUARD_INTERFACE_NAME;
use crate::util::Invoke;
use crate::util::io::maybe_open_file;
use crate::{Error, ErrorKind};

const BACKENDS: &[(&str, &str)] = &[
    ("iptables-legacy", "iptables-legacy-save"),
    ("iptables-nft", "iptables-nft-save"),
];
const SAVED_RULESET: &str = "/etc/iptables/rules.v4";
const CMD_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RULESET_BYTES: u64 = 4 * 1024 * 1024;
const MAX_DISPLAY_CHAINS: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
struct BetaFilterFinding {
    beta_chains: BTreeSet<String>,
    has_wg_new_rule: bool,
}

struct BoundedWriter {
    buf: Vec<u8>,
    limit: usize,
    exceeded: bool,
}

impl BoundedWriter {
    fn new(limit: usize) -> Self {
        Self {
            buf: Vec::new(),
            limit,
            exceeded: false,
        }
    }
}

impl AsyncWrite for BoundedWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.buf.len().saturating_add(data.len()) > self.limit {
            self.exceeded = true;
            return Poll::Ready(Err(std::io::Error::other("output exceeds size limit")));
        }
        self.buf.extend_from_slice(data);
        Poll::Ready(Ok(data.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// Logs one warning if leftover 0.4.0 pre-release iptables `FORWARD DROP` rules are detected.
pub(super) async fn warn_if_blocking() {
    if let Some(msg) = detect_warning(
        BACKENDS,
        &[Path::new(SAVED_RULESET)],
        CMD_TIMEOUT,
        MAX_RULESET_BYTES,
    )
    .await
    {
        tracing::warn!("{msg}");
    }
}

async fn detect_warning(
    backends: &[(&str, &str)],
    saved_paths: &[&Path],
    timeout: Duration,
    max_ruleset_bytes: u64,
) -> Option<String> {
    let mut live = Vec::new();
    for &(label, bin) in backends {
        if let Some(f) = read_backend(bin, max_ruleset_bytes, timeout)
            .await
            .as_deref()
            .and_then(detect_blocking_beta_filter)
        {
            live.push((label.to_owned(), f));
        }
    }
    let mut saved = Vec::new();
    for path in saved_paths {
        if let Some(f) = read_ruleset_file(path, max_ruleset_bytes, timeout)
            .await
            .as_deref()
            .and_then(detect_blocking_beta_filter)
        {
            saved.push((path.display().to_string(), f));
        }
    }
    format_warning(&live, &saved)
}

fn is_not_found_error(err: &Error) -> bool {
    err.source.chain().any(|e| {
        e.downcast_ref::<std::io::Error>()
            .is_some_and(|i| i.kind() == std::io::ErrorKind::NotFound)
    })
}

async fn read_backend(bin: &str, max_bytes: u64, timeout: Duration) -> Option<String> {
    let limit = usize::try_from(max_bytes).unwrap_or(usize::MAX);
    let mut writer = BoundedWriter::new(limit);
    let res = Command::new(bin)
        .timeout(Some(timeout))
        .output_to(&mut writer)
        .invoke(ErrorKind::Network)
        .await;
    if writer.exceeded {
        tracing::debug!("skipping oversized legacy iptables output for {bin}");
        return None;
    }
    match res {
        Ok(_) => Some(String::from_utf8_lossy(&writer.buf).into_owned()),
        Err(err) => {
            if !is_not_found_error(&err) {
                tracing::debug!("skipping legacy iptables check for {bin}: {}", err.source);
            }
            None
        }
    }
}

async fn read_ruleset_file(path: &Path, max_bytes: u64, timeout: Duration) -> Option<String> {
    match tokio::time::timeout(timeout, read_ruleset_file_inner(path, max_bytes)).await {
        Ok(res) => res,
        Err(_) => {
            tracing::debug!("skipping saved ruleset {} (timed out)", path.display());
            None
        }
    }
}

async fn read_ruleset_file_inner(path: &Path, max_bytes: u64) -> Option<String> {
    match tokio::fs::metadata(path).await {
        Ok(m) if !m.is_file() => return None,
        Ok(m) if m.len() > max_bytes => {
            tracing::debug!("skipping oversized saved ruleset {}", path.display());
            return None;
        }
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::debug!("skipping saved ruleset {}: {e}", path.display());
            return None;
        }
    }
    let file = maybe_open_file(path)
        .await
        .inspect_err(|e| tracing::debug!("skipping saved ruleset {}: {e}", path.display()))
        .ok()??;
    let mut buf = Vec::new();
    match file
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut buf)
        .await
    {
        Ok(_) if buf.len() as u64 <= max_bytes => Some(String::from_utf8_lossy(&buf).into_owned()),
        Ok(_) => {
            tracing::debug!("skipping oversized saved ruleset {}", path.display());
            None
        }
        Err(e) => {
            tracing::debug!("skipping saved ruleset {}: {e}", path.display());
            None
        }
    }
}

fn detect_blocking_beta_filter(save_output: &str) -> Option<BetaFilterFinding> {
    let (mut in_filter, mut forward_drop, mut has_wg_new_rule) = (false, false, false);
    let (mut declared_chains, mut referenced_chains) = (BTreeSet::new(), BTreeSet::new());
    for line in save_output.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        } else if let Some(table) = line.strip_prefix('*') {
            in_filter = table == "filter";
            if in_filter {
                (forward_drop, has_wg_new_rule) = (false, false);
                declared_chains.clear();
                referenced_chains.clear();
            }
        } else if !in_filter {
            continue;
        } else if line == "COMMIT" {
            in_filter = false;
        } else if let Some(rest) = line.strip_prefix(':') {
            let mut parts = rest.split_whitespace();
            match (parts.next(), parts.next()) {
                (Some("FORWARD"), policy) => forward_drop = policy == Some("DROP"),
                (Some(c), _)
                    if is_beta_filter_chain_for(c, "INPUT")
                        || is_beta_filter_chain_for(c, "FORWARD") =>
                {
                    declared_chains.insert(c.to_owned());
                }
                _ => (),
            }
        } else {
            let tokens: Vec<&str> = strip_counter(line).split_whitespace().collect();
            has_wg_new_rule |= is_beta_wg_forward_rule(&tokens);
            if let Some(target) = beta_hook_jump(&tokens) {
                referenced_chains.insert(target.to_owned());
            }
        }
    }
    let beta_chains: BTreeSet<String> = declared_chains
        .intersection(&referenced_chains)
        .cloned()
        .collect();
    (forward_drop && (!beta_chains.is_empty() || has_wg_new_rule)).then_some(BetaFilterFinding {
        beta_chains,
        has_wg_new_rule,
    })
}

fn is_beta_filter_chain_for(name: &str, hook: &str) -> bool {
    name.strip_prefix('F')
        .and_then(|s| s.split_once('_'))
        .is_some_and(|(hash, suffix)| {
            hash.len() == 15
                && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
                && suffix == hook
        })
}

fn strip_counter(line: &str) -> &str {
    line.strip_prefix('[')
        .and_then(|r| r.split_once(']'))
        .filter(|(c, _)| {
            c.split_once(':').is_some_and(|(p, b)| {
                !p.is_empty()
                    && !b.is_empty()
                    && p.bytes().chain(b.bytes()).all(|d| d.is_ascii_digit())
            })
        })
        .map_or(line, |(_, rest)| rest.trim_start())
}

fn beta_hook_jump<'a>(tokens: &[&'a str]) -> Option<&'a str> {
    match tokens {
        [
            "-A" | "-I",
            hook @ ("INPUT" | "FORWARD"),
            "-j" | "--jump",
            target,
        ] if is_beta_filter_chain_for(target, hook) => Some(*target),
        _ => None,
    }
}

fn is_beta_wg_forward_rule(tokens: &[&str]) -> bool {
    let mut rest = match tokens {
        ["-A" | "-I", "FORWARD", tail @ ..] => tail,
        _ => return false,
    };
    let (mut wg_in, mut state_new, mut accept) = (false, false, false);
    while !rest.is_empty() {
        match rest {
            ["-i" | "--in-interface", iface, tail @ ..] if *iface == WIREGUARD_INTERFACE_NAME => {
                (wg_in, rest) = (true, tail);
            }
            ["-m", "state", "--state", "NEW", tail @ ..]
            | ["-m", "conntrack", "--ctstate", "NEW", tail @ ..] => {
                (state_new, rest) = (true, tail)
            }
            ["-j" | "--jump", "ACCEPT", tail @ ..] => (accept, rest) = (true, tail),
            _ => return false,
        }
    }
    wg_in && state_new && accept
}

fn format_warning(
    live: &[(String, BetaFilterFinding)],
    saved: &[(String, BetaFilterFinding)],
) -> Option<String> {
    if live.is_empty() && saved.is_empty() {
        return None;
    }
    let describe = |items: &[(String, BetaFilterFinding)]| {
        items
            .iter()
            .map(|(label, f)| {
                let mut parts = Vec::new();
                if !f.beta_chains.is_empty() {
                    let mut shown: Vec<_> = f
                        .beta_chains
                        .iter()
                        .take(MAX_DISPLAY_CHAINS)
                        .cloned()
                        .collect();
                    if f.beta_chains.len() > MAX_DISPLAY_CHAINS {
                        shown.push(format!(
                            "+{} more",
                            f.beta_chains.len() - MAX_DISPLAY_CHAINS
                        ));
                    }
                    parts.push(format!("0.4.0 pre-release chains: {}", shown.join(", ")));
                }
                if f.has_wg_new_rule {
                    parts.push(format!(
                        "0.4.0 pre-release {WIREGUARD_INTERFACE_NAME} NEW accept rule"
                    ));
                }
                format!("{label} ({})", parts.join("; "))
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let cleanup_steps = |items: &[(String, BetaFilterFinding)]| {
        let mut steps = Vec::new();
        if items.iter().any(|(_, f)| !f.beta_chains.is_empty()) {
            steps.push(
                "remove the matched filter `F*_` chains (and inspect `nat` for matching `F*_` chains)"
                    .to_owned(),
            );
        }
        if items.iter().any(|(_, f)| f.has_wg_new_rule) {
            steps.push(format!(
                "remove the `-A FORWARD -i {WIREGUARD_INTERFACE_NAME} -m state/conntrack --state/--ctstate NEW -j ACCEPT` rule"
            ));
        }
        steps.join(" and ")
    };
    if !live.is_empty() {
        let extra = if saved.is_empty() {
            String::new()
        } else {
            format!("; also persisted in {}", describe(saved))
        };
        Some(format!(
            "Detected leftover 0.4.0 pre-release StartTunnel iptables rules under a FORWARD DROP policy in {}{extra}. \
             These rules may drop new port forwards added after upgrading to StartTunnel 1.x. \
             StartTunnel 1.x uses nftables (`table ip startos`) and does not modify iptables automatically. \
             Confirm whether these legacy rules are dropping new forwards and whether ACCEPT is the intended FORWARD policy on this host; \
             only then restore the FORWARD policy to ACCEPT, {}, \
             and check any saved ruleset (such as `{SAVED_RULESET}`) that restores them on boot.",
            describe(live),
            cleanup_steps(live),
        ))
    } else {
        Some(format!(
            "Detected leftover 0.4.0 pre-release StartTunnel iptables rules under a FORWARD DROP policy in saved ruleset(s): {}. \
             If restored on boot, these rules may drop new port forwards added after upgrading to StartTunnel 1.x. \
             StartTunnel 1.x uses nftables (`table ip startos`) and does not modify iptables files automatically. \
             Inspect what ruleset is loaded at boot, and in that saved file only if unintended, \
             update its FORWARD policy and {}.",
            describe(saved),
            cleanup_steps(saved),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::ResultExt;

    const BETA_UPGRADED_RULESET: &str = "\
# Generated by iptables-save v1.8.9 (legacy)
*nat
:PREROUTING ACCEPT [0:0]
:F0123456789abcde_PREROUTING - [0:0]
-A PREROUTING -j F0123456789abcde_PREROUTING
COMMIT
*filter
:INPUT ACCEPT [0:0]
:FORWARD DROP [42:2520]
:OUTPUT ACCEPT [0:0]
:F0123456789abcde_ACCEPT - [0:0]
:F0123456789abcde_FORWARD - [0:0]
:F0123456789abcde_INPUT - [0:0]
-A INPUT -j F0123456789abcde_INPUT
-A FORWARD -i wg-start-tunnel -m state --state NEW -j ACCEPT
-A FORWARD -m state --state ESTABLISHED,RELATED -j ACCEPT
-A FORWARD -j F0123456789abcde_FORWARD
-A F0123456789abcde_FORWARD -d 10.59.0.2/32 -p tcp -m tcp --dport 8080 -m state --state NEW -j ACCEPT
COMMIT
";

    #[test]
    fn detects_beta_upgraded_ruleset_and_formats_actionable_warning() {
        let finding = detect_blocking_beta_filter(BETA_UPGRADED_RULESET).expect("must detect");
        assert!(finding.has_wg_new_rule);
        assert_eq!(
            finding.beta_chains,
            BTreeSet::from([
                "F0123456789abcde_FORWARD".to_owned(),
                "F0123456789abcde_INPUT".to_owned(),
            ])
        );

        let warning = format_warning(&[("iptables-legacy".to_owned(), finding)], &[])
            .expect("must format warning");
        assert!(warning.contains("iptables-legacy"));
        assert!(warning.contains("FORWARD DROP policy"));
        assert!(warning.contains("F0123456789abcde_FORWARD"));
        assert!(warning.contains("wg-start-tunnel"));
        assert!(warning.contains("may drop new port forwards"));
        assert!(warning.contains("Confirm whether these legacy rules are dropping new forwards"));
        assert!(warning.contains("ACCEPT is the intended FORWARD policy"));
        assert!(warning.contains("/etc/iptables/rules.v4"));
    }

    #[test]
    fn detects_beta_wg_rule_without_forward_chains_and_with_conntrack_or_counters() {
        let ruleset = "\
*filter
:INPUT ACCEPT [0:0]
:FORWARD DROP [1:60]
:OUTPUT ACCEPT [0:0]
[12:720] -A FORWARD -i wg-start-tunnel -m conntrack --ctstate NEW -j ACCEPT
[99:8000] -A FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
COMMIT
";
        let finding = detect_blocking_beta_filter(ruleset).expect("must detect");
        assert!(finding.beta_chains.is_empty());
        assert!(finding.has_wg_new_rule);

        let warning = format_warning(&[("iptables-nft".to_owned(), finding)], &[])
            .expect("must format warning");
        assert!(warning.contains("NEW -j ACCEPT"));
        assert!(!warning.contains("F*_"));
    }

    #[test]
    fn beta_chains_under_forward_accept_are_non_blocking() {
        let non_blocking =
            BETA_UPGRADED_RULESET.replace(":FORWARD DROP [42:2520]", ":FORWARD ACCEPT [0:0]");
        assert_eq!(detect_blocking_beta_filter(&non_blocking), None);
    }

    #[test]
    fn unrelated_user_firewall_and_unreferenced_beta_names_are_not_flagged() {
        let ufw_docker_ruleset = "\
*filter
:INPUT DROP [0:0]
:FORWARD DROP [10:600]
:OUTPUT ACCEPT [0:0]
:DOCKER - [0:0]
:DOCKER-USER - [0:0]
:ufw-before-forward - [0:0]
:ufw-user-forward - [0:0]
:F0123456789abcd_FORWARD - [0:0]
:F0123456789abcdef_FORWARD - [0:0]
:F0123456789abcde_CUSTOM - [0:0]
:F0123456789abcde_FORWARD - [0:0]
:F0123456789abcde_INPUT - [0:0]
-A FORWARD -j DOCKER-USER
-A FORWARD -j ufw-before-forward
-A FORWARD -j ufw-user-forward
-A FORWARD -s 10.0.0.0/8 -j F0123456789abcde_FORWARD
-A FORWARD -j F0123456789abcde_INPUT
-A FORWARD ! -i wg-start-tunnel -m state --state NEW -j ACCEPT
-A FORWARD -i wg-start-tunnel -d 10.59.0.2/32 -p tcp --dport 443 -m conntrack --ctstate NEW -j ACCEPT
-A FORWARD -s 192.168.1.0/24 -m comment --comment \"-i wg-start-tunnel -m state --state NEW -j ACCEPT -j F0123456789abcde_FORWARD\" -j DROP
-A FORWARD -m comment --comment \"-j\" -j DROP
COMMIT
";
        assert_eq!(detect_blocking_beta_filter(ufw_docker_ruleset), None);
    }

    #[test]
    fn preserves_filter_across_trailing_tables_and_resets_on_repeated_filter() {
        let with_trailing_mangle =
            format!("{BETA_UPGRADED_RULESET}*mangle\n:FORWARD ACCEPT [0:0]\nCOMMIT\n");
        assert!(detect_blocking_beta_filter(&with_trailing_mangle).is_some());

        let with_clean_second_filter = format!(
            "{BETA_UPGRADED_RULESET}*filter\n:INPUT ACCEPT [0:0]\n:FORWARD ACCEPT [0:0]\n:OUTPUT ACCEPT [0:0]\nCOMMIT\n"
        );
        assert_eq!(detect_blocking_beta_filter(&with_clean_second_filter), None);
    }

    #[test]
    fn truncates_excessive_beta_chain_list_in_warning() {
        let mut chains = BTreeSet::new();
        for i in 0..8 {
            chains.insert(format!("F{i:015x}_FORWARD"));
        }
        let finding = BetaFilterFinding {
            beta_chains: chains,
            has_wg_new_rule: false,
        };
        let warning = format_warning(&[("iptables-nft".to_owned(), finding)], &[])
            .expect("must format warning");
        assert!(warning.contains("iptables-nft"));
        assert!(warning.contains("+2 more"));
        assert!(warning.contains("inspect `nat` for matching `F*_` chains"));
        assert!(!warning.contains("wg-start-tunnel"));
    }

    #[test]
    fn classifies_not_found_errors_distinct_from_other_failures() {
        let not_found: Error = Result::<(), _>::Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "missing binary",
        ))
        .with_ctx(|_| (ErrorKind::Network, "iptables-legacy-save"))
        .unwrap_err();
        let perm_denied: Error = Result::<(), _>::Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "permission denied",
        ))
        .with_ctx(|_| (ErrorKind::Network, "iptables-legacy-save"))
        .unwrap_err();

        assert!(is_not_found_error(&not_found));
        assert!(!is_not_found_error(&perm_denied));
    }

    #[tokio::test]
    async fn missing_binaries_and_absent_saved_ruleset_return_no_warning() {
        let dir = tempfile::tempdir().unwrap();
        let absent = dir.path().join("rules.v4");
        let missing_backends = [
            (
                "iptables-legacy",
                "__nonexistent_start_tunnel_iptables_legacy_save__",
            ),
            (
                "iptables-nft",
                "__nonexistent_start_tunnel_iptables_nft_save__",
            ),
        ];

        let warning = detect_warning(
            &missing_backends,
            &[absent.as_path()],
            CMD_TIMEOUT,
            MAX_RULESET_BYTES,
        )
        .await;
        assert_eq!(warning, None);
    }

    #[tokio::test]
    async fn timed_out_backend_returns_none_promptly() {
        let dir = tempfile::tempdir().unwrap();
        let slow_bin = dir.path().join("slow-iptables-save");
        std::fs::write(&slow_bin, "#!/bin/sh\nexec sleep 5\n").unwrap();
        std::fs::set_permissions(&slow_bin, std::fs::Permissions::from_mode(0o755)).unwrap();

        let start = std::time::Instant::now();
        let res = read_backend(
            &slow_bin.to_string_lossy(),
            MAX_RULESET_BYTES,
            Duration::from_millis(100),
        )
        .await;
        assert_eq!(res, None);
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn oversized_backend_output_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let big_bin = dir.path().join("big-iptables-save");
        std::fs::write(
            &big_bin,
            format!("#!/bin/sh\ncat <<'EOF'\n{BETA_UPGRADED_RULESET}EOF\n"),
        )
        .unwrap();
        std::fs::set_permissions(&big_bin, std::fs::Permissions::from_mode(0o755)).unwrap();

        let bin = big_bin.to_string_lossy();
        assert_eq!(read_backend(&bin, 64, CMD_TIMEOUT).await, None);
        assert!(
            read_backend(&bin, MAX_RULESET_BYTES, CMD_TIMEOUT)
                .await
                .is_some()
        );
    }

    #[tokio::test]
    async fn oversized_saved_ruleset_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let big = dir.path().join("rules.v4");
        std::fs::write(&big, BETA_UPGRADED_RULESET).unwrap();

        assert_eq!(read_ruleset_file(&big, 64, CMD_TIMEOUT).await, None);
        assert!(
            read_ruleset_file(&big, MAX_RULESET_BYTES, CMD_TIMEOUT)
                .await
                .is_some()
        );
    }

    #[tokio::test]
    async fn detects_live_backend_and_saved_ruleset_fixtures_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let fake_legacy = dir.path().join("fake-iptables-legacy-save");
        let fake_failing = dir.path().join("fake-iptables-nft-save");
        let saved_blocking = dir.path().join("rules.v4");
        let saved_clean = dir.path().join("rules-clean.v4");

        std::fs::write(
            &fake_legacy,
            format!("#!/bin/sh\ncat <<'EOF'\n{BETA_UPGRADED_RULESET}EOF\n"),
        )
        .unwrap();
        std::fs::set_permissions(&fake_legacy, std::fs::Permissions::from_mode(0o755)).unwrap();

        std::fs::write(
            &fake_failing,
            "#!/bin/sh\necho 'permission denied' >&2\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake_failing, std::fs::Permissions::from_mode(0o755)).unwrap();

        std::fs::write(&saved_blocking, BETA_UPGRADED_RULESET).unwrap();
        std::fs::write(
            &saved_clean,
            "*filter\n:INPUT ACCEPT [0:0]\n:FORWARD ACCEPT [0:0]\n:OUTPUT ACCEPT [0:0]\nCOMMIT\n",
        )
        .unwrap();

        let legacy_bin = fake_legacy.to_string_lossy().into_owned();
        let failing_bin = fake_failing.to_string_lossy().into_owned();
        let backends = [
            ("iptables-legacy", legacy_bin.as_str()),
            ("iptables-nft", failing_bin.as_str()),
        ];

        let combined = detect_warning(
            &backends,
            &[saved_blocking.as_path()],
            CMD_TIMEOUT,
            MAX_RULESET_BYTES,
        )
        .await
        .expect("must warn");
        assert!(combined.contains("iptables-legacy"));
        assert!(combined.contains(&saved_blocking.display().to_string()));
        assert!(!combined.contains("iptables-nft"));

        let clean_only = detect_warning(
            &[("iptables-nft", failing_bin.as_str())],
            &[saved_clean.as_path(), dir.path()],
            CMD_TIMEOUT,
            MAX_RULESET_BYTES,
        )
        .await;
        assert_eq!(clean_only, None);

        let saved_only = detect_warning(
            &[],
            &[saved_blocking.as_path()],
            CMD_TIMEOUT,
            MAX_RULESET_BYTES,
        )
        .await
        .expect("must warn on saved blocking ruleset");
        assert!(saved_only.contains(&saved_blocking.display().to_string()));
        assert!(saved_only.contains("restored on boot"));
        assert!(saved_only.contains("may drop new port forwards"));
        assert!(saved_only.contains("Inspect what ruleset is loaded at boot"));
        assert!(!saved_only.contains("restore the FORWARD policy to ACCEPT"));
    }
}
