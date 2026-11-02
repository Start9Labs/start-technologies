//! Portable discovery and progress messages exchanged with the backup filesystem helper.

use serde::{Deserialize, Serialize};

pub const MOUNT_COMMAND: &str = "mount";
pub const INSPECT_COMMAND: &str = "inspect";
pub const CONVERT_COMMAND: &str = "convert";

/// Unauthenticated discovery hints; opening the store validates its actual state.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "reflection", derive(visit_rs::VisitVariants))]
#[serde(rename_all = "camelCase")]
pub enum FormatStatus {
    Legacy,
    Current,
    Interrupted,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "reflection", derive(visit_rs::VisitVariants))]
#[serde(rename_all = "camelCase")]
pub enum ConversionPhase {
    Inspecting,
    Records,
    ScanningObjects,
    Objects,
    Publishing,
    Complete,
}

/// Counts describe this attempt's work; a resumed attempt inventories the remaining objects.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "reflection", derive(visit_rs::VisitFields))]
#[serde(rename_all = "camelCase")]
pub struct ConversionProgress {
    pub phase: ConversionPhase,
    pub done: u64,
    pub total: Option<u64>,
}

#[cfg(feature = "ts")]
rpc_toolkit::reflect_ts!(FormatStatus);
#[cfg(feature = "ts")]
rpc_toolkit::reflect_ts!(ConversionPhase);
#[cfg(feature = "ts")]
rpc_toolkit::reflect_ts!(ConversionProgress);

/// Maximum encoded progress line accepted by helper consumers.
pub const MAX_PROGRESS_LINE: usize = 4096;
