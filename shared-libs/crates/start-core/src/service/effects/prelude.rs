pub use clap::Parser;
pub use serde::{Deserialize, Serialize};
pub use ts_rs::TS;

pub use crate::prelude::*;
use crate::rpc_continuations::Guid;
pub(super) use crate::service::effects::context::EffectContext;

// The event id of the procedure making an effect call, which the container
// runtime sets on every call. `action run` takes it as `--event-id` to answer
// the form an earlier `get-input` opened.
// A message sent to a service under the id of a handler that service is
// running skips that handler's conflicts. A doc comment here becomes the
// about text of every command that flattens this.
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
