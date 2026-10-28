use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, visit_rs::VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum StartStop {
    Start,
    Stop,
}

rpc_toolkit::reflect_ts!(StartStop);

impl StartStop {
    pub fn is_start(&self) -> bool {
        matches!(self, StartStop::Start)
    }
}
