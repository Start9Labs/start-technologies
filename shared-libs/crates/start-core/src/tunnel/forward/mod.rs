//! The tunnel's gateway-side inbound forwarding: nft DNAT + external-IP
//! resolution ([`igd`]), the PCP [`GatewayBackend`](crate::net::port_map::server::GatewayBackend)
//! implementation ([`pcp`]), and the SNI demultiplexer ([`sni`]).

pub mod igd;
pub mod pcp;
pub mod pinhole;
pub mod sni;
