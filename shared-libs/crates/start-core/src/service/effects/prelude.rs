pub use clap::Parser;
pub use serde::{Deserialize, Serialize};
pub use ts_rs::TS;

pub use crate::prelude::*;
use crate::rpc_continuations::Guid;
pub(super) use crate::service::effects::context::EffectContext;

/// The event id of the procedure making an effect call, which the container
/// runtime sets on every call. A message sent to a service under the id of a
/// handler that service is running skips that handler's conflicts.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventId {
    #[serde(default)]
    pub event_id: Guid,
}
