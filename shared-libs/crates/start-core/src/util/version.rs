use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, visit_rs::VisitFields)]
#[visit(ts(type = "string"), wire = "rpc_toolkit::ts::Unknown")]
#[visit(ts(rename = "Version"))]
#[visit(opaque, type_attributes(visit::wire))]
pub struct VersionString {
    version: exver::ExtendedVersion,
    string: String,
}

rpc_toolkit::reflect_ts!(VersionString);
impl VersionString {
    pub fn as_str(&self) -> &str {
        self.string.as_str()
    }
    pub fn into_version(self) -> exver::ExtendedVersion {
        self.version
    }
}
impl std::fmt::Display for VersionString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.string)
    }
}
impl std::str::FromStr for VersionString {
    type Err = <exver::ExtendedVersion as FromStr>::Err;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(VersionString {
            string: s.to_owned(),
            version: s.parse()?,
        })
    }
}
impl From<exver::ExtendedVersion> for VersionString {
    fn from(v: exver::ExtendedVersion) -> Self {
        VersionString {
            string: v.to_string(),
            version: v,
        }
    }
}
impl From<VersionString> for exver::ExtendedVersion {
    fn from(v: VersionString) -> Self {
        v.version
    }
}
impl Default for VersionString {
    fn default() -> Self {
        Self::from(exver::ExtendedVersion::default())
    }
}
impl Deref for VersionString {
    type Target = exver::ExtendedVersion;
    fn deref(&self) -> &Self::Target {
        &self.version
    }
}
impl AsRef<exver::ExtendedVersion> for VersionString {
    fn as_ref(&self) -> &exver::ExtendedVersion {
        &self.version
    }
}
impl AsRef<str> for VersionString {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl PartialEq for VersionString {
    fn eq(&self, other: &VersionString) -> bool {
        self.version.eq(&other.version)
    }
}
impl Eq for VersionString {}
impl PartialOrd for VersionString {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.version.partial_cmp(&other.version)
    }
}
impl Ord for VersionString {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.version.partial_cmp(&other.version).unwrap_or_else(|| {
            match (self.version.flavor(), other.version.flavor()) {
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(_), None) => std::cmp::Ordering::Less,
                (a, b) => a.cmp(&b),
            }
        })
    }
}
impl Hash for VersionString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.version.hash(state)
    }
}
impl<'de> Deserialize<'de> for VersionString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let string = String::deserialize(deserializer)?;
        let version =
            exver::ExtendedVersion::from_str(&string).map_err(::serde::de::Error::custom)?;
        Ok(Self { string, version })
    }
}
impl Serialize for VersionString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.string.serialize(serializer)
    }
}
