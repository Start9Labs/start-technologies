use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, visit_rs::VisitVariants,
)]
#[serde(rename_all = "kebab-case")]
pub enum PluginId {
    UrlV0,
}

rpc_toolkit::reflect_ts!(PluginId);
rpc_toolkit::ts_export!(PluginId, namespaces = [""]);
