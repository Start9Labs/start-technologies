pub use clap::Parser;
pub use serde::{Deserialize, Serialize};

pub use crate::prelude::*;
use crate::rpc_continuations::Guid;
pub(super) use crate::service::effects::context::EffectContext;

// A running procedure's id grants conflict exemptions; `action run` uses `get-input`'s id.
// Keep this non-doc: clap renders struct docs as command descriptions.
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
