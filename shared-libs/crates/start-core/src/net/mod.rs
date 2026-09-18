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

const _: () = {
    let evaluation_order = [
        TUNNEL_REPLY_RULE_PRIORITY,
        DIVERT_RULE_PRIORITY,
        MAIN_RULE_PRIORITY,
        REPLY_RULE_PRIORITY,
        SOURCE_RULE_PRIORITY,
        SERVICE_OUTBOUND_RULE_PRIORITY,
        SERVICE_OUTBOUND_REJECT_RULE_PRIORITY,
        WG_ENCAP_RULE_PRIORITY,
        DEFAULT_OUTBOUND_RULE_PRIORITY,
        DEFAULT_OUTBOUND_REJECT_RULE_PRIORITY,
    ];
    let mut i = 1;
    while i < evaluation_order.len() {
        assert!(evaluation_order[i - 1] < evaluation_order[i]);
        i += 1;
    }
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
    use super::rule_at_priority;

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
