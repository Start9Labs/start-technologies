use std::fmt::Debug;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};
use visit_rs::ts::TS;

use crate::util::VersionString;
use crate::{Id, InvalidId, PackageId};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[ts(type = "string")]
pub struct ImageId(Id);
impl AsRef<Path> for ImageId {
    fn as_ref(&self) -> &Path {
        self.0.as_ref().as_ref()
    }
}
impl std::fmt::Display for ImageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0)
    }
}
impl ImageId {
    pub fn for_package(&self, pkg_id: &PackageId, pkg_version: Option<&VersionString>) -> String {
        format!(
            "start9/{}/{}:{}",
            pkg_id,
            self.0,
            pkg_version.map(|v| { v.as_str() }).unwrap_or("latest")
        )
    }
}
impl FromStr for ImageId {
    type Err = InvalidId;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ImageId(Id::try_from(s.to_owned())?))
    }
}
impl<'de> Deserialize<'de> for ImageId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(ImageId(Deserialize::deserialize(deserializer)?))
    }
}
