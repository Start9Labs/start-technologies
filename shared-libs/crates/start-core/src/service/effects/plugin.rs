use serde::{Deserialize, Serialize};
use visit_rs::ts::TS;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum PluginId {
    UrlV0,
}
