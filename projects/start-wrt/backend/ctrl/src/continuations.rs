use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::Path;
use axum::http::{Request, Response, StatusCode};
use axum::response::IntoResponse;
pub use startos::rpc_continuations::{Guid, RpcContinuation, RpcContinuations};

pub const DEFAULT_TTL: Duration = Duration::from_secs(60);
pub const MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;

fn server_error(e: startos::Error) -> Response<Body> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::from(e.to_string()))
        .unwrap()
}

/// Axum handler for `/rest/rpc/{guid}`.
pub async fn continuation_handler(
    Path(guid): Path<String>,
    axum::Extension(continuations): axum::Extension<Arc<RpcContinuations>>,
    request: Request<Body>,
) -> Response<Body> {
    let handler = match Guid::from(&guid) {
        Some(guid) => continuations.get_rest_handler(&guid).await,
        None => None,
    };
    match handler {
        Some(handler) => handler(request).await.unwrap_or_else(server_error),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
