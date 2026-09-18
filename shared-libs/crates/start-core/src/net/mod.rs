use rpc_toolkit::{Context, HandlerExt, ParentHandler};

pub mod acme;
pub mod dns;
pub mod dns_update;
pub mod domain_redirect;
pub mod forward;
pub mod gateway;
pub mod host;
pub mod http;
pub mod keys;
pub mod mdns;
pub mod net_controller;
pub mod port_map;
pub mod service_interface;
pub mod socks;
pub mod ssl;
pub mod static_server;
pub mod tls;
pub mod transparent;
pub mod tunnel;
pub mod utils;
pub mod vhost;
pub mod web_server;
pub mod wifi;

/// `ip rule` priorities; policy-routing.md derives every order asserted below.
const TUNNEL_REPLY_RULE_PRIORITY: u32 = 48;
const DIVERT_RULE_PRIORITY: u32 = 49;
const MAIN_RULE_PRIORITY: u32 = 50;
const REPLY_RULE_PRIORITY: u32 = 51;
const SOURCE_RULE_PRIORITY: u32 = 60;
const SERVICE_OUTBOUND_RULE_PRIORITY: u32 = 70;
const SERVICE_OUTBOUND_REJECT_RULE_PRIORITY: u32 = 71;
const WG_ENCAP_RULE_PRIORITY: u32 = 74;
const DEFAULT_OUTBOUND_RULE_PRIORITY: u32 = 75;
const DEFAULT_OUTBOUND_REJECT_RULE_PRIORITY: u32 = 76;
const AUTO_MAIN_RULE_PRIORITY: u32 = 1000;
const AUTO_DEFAULT_RULE_PRIORITY: u32 = 1100;

const _: () = {
    const fn before(rule: u32, others: &[u32]) {
        let mut i = 0;
        while i < others.len() {
            assert!(rule < others[i]);
            i += 1;
        }
    }
    const fn apart(rule: u32, others: &[u32]) {
        let mut i = 0;
        while i < others.len() {
            assert!(rule != others[i]);
            i += 1;
        }
    }

    // A rule precedes those that match some of its packets and route them elsewhere.
    before(
        TUNNEL_REPLY_RULE_PRIORITY,
        &[
            MAIN_RULE_PRIORITY,
            SERVICE_OUTBOUND_RULE_PRIORITY,
            SERVICE_OUTBOUND_REJECT_RULE_PRIORITY,
            DEFAULT_OUTBOUND_RULE_PRIORITY,
            DEFAULT_OUTBOUND_REJECT_RULE_PRIORITY,
            AUTO_MAIN_RULE_PRIORITY,
        ],
    );
    before(
        DIVERT_RULE_PRIORITY,
        &[
            MAIN_RULE_PRIORITY,
            SERVICE_OUTBOUND_RULE_PRIORITY,
            SERVICE_OUTBOUND_REJECT_RULE_PRIORITY,
            DEFAULT_OUTBOUND_RULE_PRIORITY,
            DEFAULT_OUTBOUND_REJECT_RULE_PRIORITY,
            AUTO_MAIN_RULE_PRIORITY,
        ],
    );
    before(
        MAIN_RULE_PRIORITY,
        &[
            REPLY_RULE_PRIORITY,
            SOURCE_RULE_PRIORITY,
            SERVICE_OUTBOUND_RULE_PRIORITY,
            SERVICE_OUTBOUND_REJECT_RULE_PRIORITY,
            DEFAULT_OUTBOUND_RULE_PRIORITY,
            DEFAULT_OUTBOUND_REJECT_RULE_PRIORITY,
        ],
    );
    before(
        REPLY_RULE_PRIORITY,
        &[
            SERVICE_OUTBOUND_RULE_PRIORITY,
            SERVICE_OUTBOUND_REJECT_RULE_PRIORITY,
            DEFAULT_OUTBOUND_RULE_PRIORITY,
            DEFAULT_OUTBOUND_REJECT_RULE_PRIORITY,
            AUTO_MAIN_RULE_PRIORITY,
        ],
    );
    before(
        SOURCE_RULE_PRIORITY,
        &[
            DEFAULT_OUTBOUND_RULE_PRIORITY,
            DEFAULT_OUTBOUND_REJECT_RULE_PRIORITY,
            AUTO_MAIN_RULE_PRIORITY,
        ],
    );
    before(
        SERVICE_OUTBOUND_RULE_PRIORITY,
        &[
            SERVICE_OUTBOUND_REJECT_RULE_PRIORITY,
            DEFAULT_OUTBOUND_RULE_PRIORITY,
            DEFAULT_OUTBOUND_REJECT_RULE_PRIORITY,
            AUTO_MAIN_RULE_PRIORITY,
        ],
    );
    before(
        SERVICE_OUTBOUND_REJECT_RULE_PRIORITY,
        &[DEFAULT_OUTBOUND_RULE_PRIORITY, AUTO_MAIN_RULE_PRIORITY],
    );
    before(
        WG_ENCAP_RULE_PRIORITY,
        &[
            DEFAULT_OUTBOUND_RULE_PRIORITY,
            DEFAULT_OUTBOUND_REJECT_RULE_PRIORITY,
        ],
    );
    before(
        DEFAULT_OUTBOUND_RULE_PRIORITY,
        &[
            DEFAULT_OUTBOUND_REJECT_RULE_PRIORITY,
            AUTO_MAIN_RULE_PRIORITY,
        ],
    );
    before(
        DEFAULT_OUTBOUND_REJECT_RULE_PRIORITY,
        &[AUTO_MAIN_RULE_PRIORITY],
    );

    // Reconciliation tells these apart by priority alone.
    apart(
        SOURCE_RULE_PRIORITY,
        &[
            TUNNEL_REPLY_RULE_PRIORITY,
            DIVERT_RULE_PRIORITY,
            REPLY_RULE_PRIORITY,
            SERVICE_OUTBOUND_RULE_PRIORITY,
        ],
    );
    apart(REPLY_RULE_PRIORITY, &[DIVERT_RULE_PRIORITY]);
    apart(
        WG_ENCAP_RULE_PRIORITY,
        &[
            TUNNEL_REPLY_RULE_PRIORITY,
            DIVERT_RULE_PRIORITY,
            REPLY_RULE_PRIORITY,
        ],
    );
};

/// The rest of an `ip rule show` line at this priority.
fn rule_at_priority(line: &str, priority: u32) -> Option<&str> {
    let (column, rest) = line.trim_start().split_once(':')?;
    (column.parse() == Ok(priority)).then_some(rest)
}

pub fn net_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "acme",
            acme::acme_api::<C>().with_about("about.setup-acme-certificate"),
        )
        .subcommand(
            "dns",
            dns::dns_api::<C>().with_about("about.manage-query-dns"),
        )
        .subcommand(
            "forward",
            forward::forward_api::<C>().with_about("about.manage-port-forwards"),
        )
        .subcommand(
            "gateway",
            gateway::gateway_api::<C>().with_about("about.view-edit-gateway-configs"),
        )
        .subcommand(
            "tunnel",
            tunnel::tunnel_api::<C>().with_about("about.manage-tunnels"),
        )
        .subcommand(
            "ssl",
            ssl::ssl_api::<C>().with_about("about.manage-ssl-certificates"),
        )
        .subcommand(
            "vhost",
            vhost::vhost_api::<C>().with_about("about.manage-ssl-vhost-proxy"),
        )
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    #[test]
    fn ladder_satisfies_the_routing_invariants() {
        let unshare = |args: &[&str]| {
            let mut cmd = Command::new("unshare");
            cmd.arg("-rn").args(args);
            cmd
        };
        if !unshare(&[
            "bash",
            "-c",
            "ip link show lo && sysctl -n net.ipv4.ip_forward",
        ])
        .output()
        .is_ok_and(|o| o.status.success())
        {
            eprintln!("skipped: needs unprivileged user namespaces, iproute2, and sysctl");
            return;
        }
        let output = unshare(&["bash", "-c", include_str!("policy_routing_model.sh")])
            .env("TUNNEL_REPLY", TUNNEL_REPLY_RULE_PRIORITY.to_string())
            .env("DIVERT", DIVERT_RULE_PRIORITY.to_string())
            .env("MAIN", MAIN_RULE_PRIORITY.to_string())
            .env("REPLY", REPLY_RULE_PRIORITY.to_string())
            .env("SOURCE", SOURCE_RULE_PRIORITY.to_string())
            .env(
                "SERVICE_OUTBOUND",
                SERVICE_OUTBOUND_RULE_PRIORITY.to_string(),
            )
            .env(
                "SERVICE_OUTBOUND_REJECT",
                SERVICE_OUTBOUND_REJECT_RULE_PRIORITY.to_string(),
            )
            .env("WG_ENCAP", WG_ENCAP_RULE_PRIORITY.to_string())
            .env(
                "DEFAULT_OUTBOUND",
                DEFAULT_OUTBOUND_RULE_PRIORITY.to_string(),
            )
            .env(
                "DEFAULT_OUTBOUND_REJECT",
                DEFAULT_OUTBOUND_REJECT_RULE_PRIORITY.to_string(),
            )
            .env("AUTO_MAIN", AUTO_MAIN_RULE_PRIORITY.to_string())
            .env("AUTO_DEFAULT", AUTO_DEFAULT_RULE_PRIORITY.to_string())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    #[test]
    fn rule_at_priority_matches_the_whole_priority_column() {
        assert_eq!(
            rule_at_priority("51:\tfrom all fwmark 0x3ec lookup 1004", 51),
            Some("\tfrom all fwmark 0x3ec lookup 1004")
        );
        assert_eq!(rule_at_priority("510:\tfrom all lookup main", 51), None);
        assert_eq!(rule_at_priority("5:\tfrom all lookup main", 51), None);
        assert_eq!(rule_at_priority("from all lookup main", 51), None);
    }
}
