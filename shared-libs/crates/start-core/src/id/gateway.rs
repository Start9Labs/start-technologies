use std::convert::Infallible;
use std::path::Path;
use std::str::FromStr;

use imbl_value::InternedString;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, visit_rs::VisitFields)]
#[visit(ts(type = "string"), wire = "rpc_toolkit::ts::Unknown")]
#[visit(opaque, type_attributes(visit::wire))]
pub struct GatewayId(InternedString);

rpc_toolkit::reflect_ts!(GatewayId);
rpc_toolkit::ts_export!(GatewayId, namespaces = ["", "tunnel"]);
impl GatewayId {
    pub fn as_str(&self) -> &str {
        &*self.0
    }
}
impl From<InternedString> for GatewayId {
    fn from(value: InternedString) -> Self {
        Self(value)
    }
}
impl From<GatewayId> for InternedString {
    fn from(value: GatewayId) -> Self {
        value.0
    }
}
impl FromStr for GatewayId {
    type Err = Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(GatewayId(InternedString::intern(s)))
    }
}
impl AsRef<GatewayId> for GatewayId {
    fn as_ref(&self) -> &GatewayId {
        self
    }
}
impl std::fmt::Display for GatewayId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0)
    }
}
impl AsRef<str> for GatewayId {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}
impl AsRef<Path> for GatewayId {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}
impl<'de> Deserialize<'de> for GatewayId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        Ok(GatewayId(serde::Deserialize::deserialize(deserializer)?))
    }
}
