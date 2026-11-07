use axum::http::header::{HeaderName, HeaderValue};
use axum::routing::post;
use axum::Router;
use rpc_toolkit::{Context, Middleware, RpcResponse, Server};
use serde::{Deserialize, Serialize};
use startos::middleware::auth::session::RpcSessionUpgrader;
use startos::middleware::auth::Auth;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::ServerContext;

/// Firmware build stamp shared by RPC metadata, headers and system.info.
pub const GIT_HASH: &str = env!("STARTWRT_GIT_HASH");
/// HTTP compatibility name for the firmware build stamp.
pub const GIT_HASH_HEADER: HeaderName = HeaderName::from_static("x-startwrt-git-hash");

/// Firmware build identity attached to each RPC reply.
#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareMetadata {
    pub git_hash: String,
}
rpc_toolkit::reflect_ts!(FirmwareMetadata);
rpc_toolkit::ts_export!(FirmwareMetadata, namespaces = ["events"]);

#[derive(Clone)]
struct FirmwareVersion;

impl<C: Context> Middleware<C> for FirmwareVersion {
    type Metadata = rpc_toolkit::Empty;

    async fn process_rpc_response(&mut self, _: &C, response: &mut RpcResponse) {
        if let Err(error) = response.add_meta(FirmwareMetadata {
            git_hash: GIT_HASH.into(),
        }) {
            response.result = Err(error);
        }
    }
}

/// HTTP compatibility and optional encrypted RPC admission share one dispatch stack.
pub fn router(ctx: ServerContext) -> Router {
    let server = Server::new(
        move || std::future::ready(Ok(ctx.clone())),
        crate::main_api(),
    )
    .middleware(Auth::new().with_local_auth().with_signature_auth())
    .middleware(FirmwareVersion);
    Router::new()
        .route(
            "/rpc/v1",
            post(server.clone()).layer(SetResponseHeaderLayer::overriding(
                GIT_HASH_HEADER,
                HeaderValue::from_static(GIT_HASH),
            )),
        )
        .route(RpcSessionUpgrader::PATH, RpcSessionUpgrader::route(server))
}

#[cfg(test)]
pub(crate) mod tests;
