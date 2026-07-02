use std::borrow::Borrow;
use std::path::Path;
use std::str::FromStr;

use imbl_value::InternedString;
use serde::{Deserialize, Serialize, Serializer};
use ts_rs::TS;

use crate::{Id, InvalidId, SYSTEM_ID};

lazy_static::lazy_static! {
    pub static ref SYSTEM_PACKAGE_ID: PackageId = PackageId(SYSTEM_ID.clone());
}
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, TS)]
#[ts(type = "string")]
pub struct PackageId(Id);
impl PackageId {
    /// The reserved package id identifying StartOS itself — the owner of the
    /// server's own host (the admin UI). Not installable as a package.
    pub fn start_os() -> Self {
        PackageId(Id::try_from("start-os").expect("valid id"))
    }
    pub fn is_start_os(&self) -> bool {
        &**self == "start-os"
    }
}
impl FromStr for PackageId {
    type Err = InvalidId;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(PackageId(Id::try_from(s.to_owned())?))
    }
}
impl From<Id> for PackageId {
    fn from(id: Id) -> Self {
        PackageId(id)
    }
}
impl From<PackageId> for Id {
    fn from(value: PackageId) -> Self {
        value.0
    }
}
impl From<PackageId> for InternedString {
    fn from(value: PackageId) -> Self {
        value.0.into()
    }
}
impl std::ops::Deref for PackageId {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl AsRef<PackageId> for PackageId {
    fn as_ref(&self) -> &PackageId {
        self
    }
}
impl std::fmt::Display for PackageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0)
    }
}
impl AsRef<str> for PackageId {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}
impl Borrow<str> for PackageId {
    fn borrow(&self) -> &str {
        self.0.as_ref()
    }
}
impl<'a> Borrow<str> for &'a PackageId {
    fn borrow(&self) -> &str {
        self.0.as_ref()
    }
}
impl AsRef<Path> for PackageId {
    fn as_ref(&self) -> &Path {
        self.0.as_ref().as_ref()
    }
}
impl<'de> Deserialize<'de> for PackageId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        Ok(PackageId(Deserialize::deserialize(deserializer)?))
    }
}
impl Serialize for PackageId {
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: Serializer,
    {
        Serialize::serialize(&self.0, serializer)
    }
}
