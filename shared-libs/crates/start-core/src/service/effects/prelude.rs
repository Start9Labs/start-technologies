pub use clap::Parser;
pub use serde::{Deserialize, Serialize};
pub use ts_rs::TS;

pub use crate::prelude::*;
use crate::rpc_continuations::Guid;
pub(super) use crate::service::effects::context::EffectContext;

// Identifies the procedure an effect call belongs to. The container runtime
// sets it to the calling procedure's id; `action run --event-id` sets it to
// the id `get-input` returned.
// A service treats a call carrying the id of a procedure it is running as part
// of that procedure, exempt from its conflicts.
// Not a doc comment: clap prints one as the about text of every command that
// flattens this.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct EventId {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[arg(long, help = "help.arg.event-id")]
    pub event_id: Option<Guid>,
}
impl EventId {
    /// A fresh id for a call made outside any procedure.
    pub fn or_new(self) -> Guid {
        self.event_id.unwrap_or_default()
    }
}
