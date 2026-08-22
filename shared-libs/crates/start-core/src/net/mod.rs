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
