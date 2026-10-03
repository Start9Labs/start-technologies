use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use visit_rs::ts::TS;

use crate::{Id, InvalidId};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[ts(type = "string")]
pub struct ActionId(Id);
impl FromStr for ActionId {
    type Err = InvalidId;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ActionId(Id::try_from(s.to_owned())?))
    }
}
impl AsRef<ActionId> for ActionId {
    fn as_ref(&self) -> &ActionId {
        self
    }
}
impl std::fmt::Display for ActionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0)
    }
}
impl AsRef<str> for ActionId {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}
impl AsRef<Path> for ActionId {
    fn as_ref(&self) -> &Path {
        self.0.as_ref().as_ref()
    }
}
impl<'de> Deserialize<'de> for ActionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        Ok(ActionId(serde::Deserialize::deserialize(deserializer)?))
    }
}
