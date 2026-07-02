use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use axum::Router;

use crate::net::static_server::ui_router;
use crate::tunnel::context::TunnelContext;

pub mod api;
pub mod auth;
pub mod context;
pub mod db;
pub mod dns;
pub mod forward;
pub(crate) mod migrations;
pub mod update;
pub mod web;
pub mod wg;
pub mod wg6;

pub const TUNNEL_DEFAULT_PORT: u16 = 5960;
pub const TUNNEL_DEFAULT_LISTEN: SocketAddr = SocketAddr::V4(SocketAddrV4::new(
    Ipv4Addr::new(127, 0, 59, 60),
    TUNNEL_DEFAULT_PORT,
));

pub fn tunnel_router(ctx: TunnelContext) -> Router {
    ui_router(ctx)
}
