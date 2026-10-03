use serde::{Deserialize, Serialize};
use visit_rs::ts::TS;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum StartStop {
    Start,
    Stop,
}

impl StartStop {
    pub fn is_start(&self) -> bool {
        matches!(self, StartStop::Start)
    }
}
