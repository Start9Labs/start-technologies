use axum::response::Response;
use http::HeaderValue;
use rpc_toolkit::{Context, Middleware, RpcRequest, RpcResponse};
use serde::{Deserialize, Serialize};

use crate::context::RpcContext;
use crate::middleware::auth::DbContext;
use crate::prelude::*;
use crate::tunnel::context::TunnelContext;

#[derive(Deserialize)]
pub struct Metadata {
    #[serde(default)]
    sync_db: bool,
}

/// RPC reply metadata for live database synchronization.
#[derive(Debug, Default, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SyncDbMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Database revision the caller must observe before using the result.
    pub patch_sequence: Option<u64>,
}

rpc_toolkit::reflect_ts!(SyncDbMetadata);
rpc_toolkit::ts_export!(SyncDbMetadata, namespaces = [""]);

/// Database context for RPC synchronization metadata.
pub trait SyncDbContext: DbContext {
    /// Advances subscribers that suppress empty revisions.
    fn publish_sync_sequence(&self, _sequence: u64) {}
}

impl SyncDbContext for RpcContext {
    fn publish_sync_sequence(&self, sequence: u64) {
        self.sync_db.send_replace(sequence);
    }
}

impl SyncDbContext for TunnelContext {}

#[derive(Clone)]
pub struct SyncDb {
    sync_db: bool,
}
impl SyncDb {
    pub fn new() -> Self {
        Self { sync_db: false }
    }
}

impl<C: SyncDbContext> Middleware<C> for SyncDb {
    type Metadata = Metadata;

    async fn process_rpc_request(
        &mut self,
        _: &C,
        metadata: Self::Metadata,
        _: &mut RpcRequest,
    ) -> Result<(), RpcResponse> {
        self.sync_db = metadata.sync_db;
        Ok(())
    }

    async fn process_rpc_response(&mut self, context: &C, response: &mut RpcResponse) {
        if self.sync_db {
            let sequence = context.db().sequence().await;
            context.publish_sync_sequence(sequence);
            if let Err(error) = response.add_meta(SyncDbMetadata {
                patch_sequence: Some(sequence),
            }) {
                response.result = Err(error);
            }
        }
    }
}

/// Projects RPC metadata for HTTP clients that read `X-Patch-Sequence`.
#[derive(Clone, Default)]
pub struct SyncDbHeaderCompat {
    sequence: Option<u64>,
}

impl<C: Context> Middleware<C> for SyncDbHeaderCompat {
    type Metadata = rpc_toolkit::Empty;

    async fn process_rpc_response(&mut self, _: &C, response: &mut RpcResponse) {
        match response.read_meta::<SyncDbMetadata>() {
            Ok(metadata) => self.sequence = metadata.patch_sequence,
            Err(error) => response.result = Err(error),
        }
    }

    async fn process_http_response(&mut self, _: &C, response: &mut Response) {
        if let Some(sequence) = self.sequence {
            let previous = response
                .headers()
                .get("X-Patch-Sequence")
                .map(|value| value.to_str().unwrap().parse::<u64>().unwrap())
                .unwrap_or_default();
            response.headers_mut().insert(
                "X-Patch-Sequence",
                HeaderValue::from(sequence.max(previous)),
            );
        }
    }
}

#[cfg(test)]
mod tests;
