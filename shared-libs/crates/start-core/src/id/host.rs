use std::path::Path;
use std::str::FromStr;

use imbl_value::InternedString;
use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

use crate::{Id, InvalidId};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[ts(type = "string")]
pub struct HostId(Id);
impl FromStr for HostId {
    type Err = InvalidId;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Id::try_from(s.to_owned())?))
    }
}
impl From<Id> for HostId {
    fn from(id: Id) -> Self {
        Self(id)
    }
}
impl From<HostId> for Id {
    fn from(value: HostId) -> Self {
        value.0
    }
}
impl From<HostId> for InternedString {
    fn from(value: HostId) -> Self {
        value.0.into()
    }
}
impl std::fmt::Display for HostId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0)
    }
}
impl std::ops::Deref for HostId {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}
impl AsRef<str> for HostId {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}
impl<'de> Deserialize<'de> for HostId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(HostId(Deserialize::deserialize(deserializer)?))
    }
}
impl HostId {
    /// The server host's id — the host owning the OS's own bindings (the
    /// StartOS UI) rather than a package's.
    pub fn admin() -> Self {
        Self(Id::try_from("admin").expect("valid id"))
    }
}
impl AsRef<Path> for HostId {
    fn as_ref(&self) -> &Path {
        self.0.as_ref().as_ref()
    }
}
