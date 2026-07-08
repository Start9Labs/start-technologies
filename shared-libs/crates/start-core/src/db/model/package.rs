use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use exver::VersionRange;
use imbl_value::InternedString;
use patch_db::HasModel;
use patch_db::json_ptr::JsonPointer;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::net::host::Hosts;
use crate::prelude::*;
use crate::progress::FullProgress;
use crate::s9pk::manifest::{LocaleString, Manifest};
use crate::status::StatusInfo;
use crate::util::DataUrl;
use crate::util::serde::{Pem, is_partial_of};
use crate::{ActionId, GatewayId, HealthCheckId, HostId, PackageId, ReplayId};

#[derive(Debug, Default, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct AllPackageData(pub BTreeMap<PackageId, PackageDataEntry>);
impl Map for AllPackageData {
    type Key = PackageId;
    type Value = PackageDataEntry;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Ok(key)
    }
    fn key_string(key: &Self::Key) -> Result<InternedString, Error> {
        Ok(key.clone().into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ManifestPreference {
    Old,
    New,
}

#[derive(Debug, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "state")]
#[model = "Model<Self>"]
#[ts(export)]
pub enum PackageState {
    Installing(InstallingState),
    Restoring(InstallingState),
    Updating(UpdatingState),
    Installed(InstalledState),
    Removing(InstalledState),
}
impl PackageState {
    pub fn expect_installed(&self) -> Result<&InstalledState, Error> {
        match self {
            Self::Installed(a) => Ok(a),
            _ => Err(Error::new(
                eyre!(
                    "Package {} is not in installed state",
                    self.as_manifest(ManifestPreference::Old).id
                ),
                ErrorKind::InvalidRequest,
            )),
        }
    }
    pub fn expect_removing(&self) -> Result<&InstalledState, Error> {
        match self {
            Self::Removing(a) => Ok(a),
            _ => Err(Error::new(
                eyre!(
                    "Package {} is not in removing state",
                    self.as_manifest(ManifestPreference::Old).id
                ),
                ErrorKind::InvalidRequest,
            )),
        }
    }
    pub fn into_installing_info(self) -> Option<InstallingInfo> {
        match self {
            Self::Installing(InstallingState { installing_info })
            | Self::Restoring(InstallingState { installing_info }) => Some(installing_info),
            Self::Updating(UpdatingState {
                installing_info, ..
            }) => Some(installing_info),
            Self::Installed(_) | Self::Removing(_) => None,
        }
    }
    pub fn as_installing_info(&self) -> Option<&InstallingInfo> {
        match self {
            Self::Installing(InstallingState { installing_info })
            | Self::Restoring(InstallingState { installing_info }) => Some(installing_info),
            Self::Updating(UpdatingState {
                installing_info, ..
            }) => Some(installing_info),
            Self::Installed(_) | Self::Removing(_) => None,
        }
    }
    pub fn as_installing_info_mut(&mut self) -> Option<&mut InstallingInfo> {
        match self {
            Self::Installing(InstallingState { installing_info })
            | Self::Restoring(InstallingState { installing_info }) => Some(installing_info),
            Self::Updating(UpdatingState {
                installing_info, ..
            }) => Some(installing_info),
            Self::Installed(_) | Self::Removing(_) => None,
        }
    }
    pub fn into_manifest(self, preference: ManifestPreference) -> Manifest {
        match self {
            Self::Installing(InstallingState {
                installing_info: InstallingInfo { new_manifest, .. },
            })
            | Self::Restoring(InstallingState {
                installing_info: InstallingInfo { new_manifest, .. },
            }) => new_manifest,
            Self::Updating(UpdatingState { manifest, .. })
                if preference == ManifestPreference::Old =>
            {
                manifest
            }
            Self::Updating(UpdatingState {
                installing_info: InstallingInfo { new_manifest, .. },
                ..
            }) => new_manifest,
            Self::Installed(InstalledState { manifest })
            | Self::Removing(InstalledState { manifest }) => manifest,
        }
    }
    pub fn as_manifest(&self, preference: ManifestPreference) -> &Manifest {
        match self {
            Self::Installing(InstallingState {
                installing_info: InstallingInfo { new_manifest, .. },
            })
            | Self::Restoring(InstallingState {
                installing_info: InstallingInfo { new_manifest, .. },
            }) => new_manifest,
            Self::Updating(UpdatingState { manifest, .. })
                if preference == ManifestPreference::Old =>
            {
                manifest
            }
            Self::Updating(UpdatingState {
                installing_info: InstallingInfo { new_manifest, .. },
                ..
            }) => new_manifest,
            Self::Installed(InstalledState { manifest })
            | Self::Removing(InstalledState { manifest }) => manifest,
        }
    }
    pub fn as_manifest_mut(&mut self, preference: ManifestPreference) -> &mut Manifest {
        match self {
            Self::Installing(InstallingState {
                installing_info: InstallingInfo { new_manifest, .. },
            })
            | Self::Restoring(InstallingState {
                installing_info: InstallingInfo { new_manifest, .. },
            }) => new_manifest,
            Self::Updating(UpdatingState { manifest, .. })
                if preference == ManifestPreference::Old =>
            {
                manifest
            }
            Self::Updating(UpdatingState {
                installing_info: InstallingInfo { new_manifest, .. },
                ..
            }) => new_manifest,
            Self::Installed(InstalledState { manifest })
            | Self::Removing(InstalledState { manifest }) => manifest,
        }
    }
}
impl Model<PackageState> {
    pub fn expect_installed(&self) -> Result<&Model<InstalledState>, Error> {
        match self.as_match() {
            PackageStateMatchModelRef::Installed(a) => Ok(a),
            _ => Err(Error::new(
                eyre!(
                    "Package {} is not in installed state",
                    self.as_manifest(ManifestPreference::Old).as_id().de()?
                ),
                ErrorKind::InvalidRequest,
            )),
        }
    }
    pub fn into_installing_info(self) -> Option<Model<InstallingInfo>> {
        match self.into_match() {
            PackageStateMatchModel::Installing(s) | PackageStateMatchModel::Restoring(s) => {
                Some(s.into_installing_info())
            }
            PackageStateMatchModel::Updating(s) => Some(s.into_installing_info()),
            PackageStateMatchModel::Installed(_) | PackageStateMatchModel::Removing(_) => None,
            PackageStateMatchModel::Error(_) => None,
        }
    }
    pub fn as_installing_info(&self) -> Option<&Model<InstallingInfo>> {
        match self.as_match() {
            PackageStateMatchModelRef::Installing(s) | PackageStateMatchModelRef::Restoring(s) => {
                Some(s.as_installing_info())
            }
            PackageStateMatchModelRef::Updating(s) => Some(s.as_installing_info()),
            PackageStateMatchModelRef::Installed(_) | PackageStateMatchModelRef::Removing(_) => {
                None
            }
            PackageStateMatchModelRef::Error(_) => None,
        }
    }
    pub fn as_installing_info_mut(&mut self) -> Option<&mut Model<InstallingInfo>> {
        match self.as_match_mut() {
            PackageStateMatchModelMut::Installing(s) | PackageStateMatchModelMut::Restoring(s) => {
                Some(s.as_installing_info_mut())
            }
            PackageStateMatchModelMut::Updating(s) => Some(s.as_installing_info_mut()),
            PackageStateMatchModelMut::Installed(_) | PackageStateMatchModelMut::Removing(_) => {
                None
            }
            PackageStateMatchModelMut::Error(_) => None,
        }
    }
    pub fn into_manifest(self, preference: ManifestPreference) -> Model<Manifest> {
        match self.into_match() {
            PackageStateMatchModel::Installing(s) | PackageStateMatchModel::Restoring(s) => {
                s.into_installing_info().into_new_manifest()
            }
            PackageStateMatchModel::Updating(s) if preference == ManifestPreference::Old => {
                s.into_manifest()
            }
            PackageStateMatchModel::Updating(s) => s.into_installing_info().into_new_manifest(),
            PackageStateMatchModel::Installed(s) | PackageStateMatchModel::Removing(s) => {
                s.into_manifest()
            }
            PackageStateMatchModel::Error(_) => Value::Null.into(),
        }
    }
    pub fn as_manifest(&self, preference: ManifestPreference) -> &Model<Manifest> {
        match self.as_match() {
            PackageStateMatchModelRef::Installing(s) | PackageStateMatchModelRef::Restoring(s) => {
                s.as_installing_info().as_new_manifest()
            }
            PackageStateMatchModelRef::Updating(s) if preference == ManifestPreference::Old => {
                s.as_manifest()
            }
            PackageStateMatchModelRef::Updating(s) => s.as_installing_info().as_new_manifest(),
            PackageStateMatchModelRef::Installed(s) | PackageStateMatchModelRef::Removing(s) => {
                s.as_manifest()
            }
            PackageStateMatchModelRef::Error(_) => (&Value::Null).into(),
        }
    }
    pub fn as_manifest_mut(
        &mut self,
        preference: ManifestPreference,
    ) -> Result<&mut Model<Manifest>, Error> {
        Ok(match self.as_match_mut() {
            PackageStateMatchModelMut::Installing(s) | PackageStateMatchModelMut::Restoring(s) => {
                s.as_installing_info_mut().as_new_manifest_mut()
            }
            PackageStateMatchModelMut::Updating(s) if preference == ManifestPreference::Old => {
                s.as_manifest_mut()
            }
            PackageStateMatchModelMut::Updating(s) => {
                s.as_installing_info_mut().as_new_manifest_mut()
            }
            PackageStateMatchModelMut::Installed(s) | PackageStateMatchModelMut::Removing(s) => {
                s.as_manifest_mut()
            }
            PackageStateMatchModelMut::Error(_) => {
                return Err(Error::new(
                    eyre!("could not determine package state to get manifest"),
                    ErrorKind::Database,
                ));
            }
        })
    }
}

#[derive(Debug, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct InstallingState {
    pub installing_info: InstallingInfo,
}

#[derive(Debug, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct UpdatingState {
    pub manifest: Manifest,
    pub s9pk: PathBuf,
    pub installing_info: InstallingInfo,
}

#[derive(Debug, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct InstalledState {
    pub manifest: Manifest,
}

#[derive(Debug, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct InstallingInfo {
    pub new_manifest: Manifest,
    pub progress: FullProgress,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum AllowedStatuses {
    OnlyRunning,
    OnlyStopped,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum ActionAccess {
    Public,
    Dependent,
    User,
}
impl Default for ActionAccess {
    fn default() -> Self {
        Self::User
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct ActionMetadata {
    /// A human-readable name
    pub name: String,
    /// A detailed description of what the action will do
    pub description: String,
    /// Presents as an alert prior to executing the action. Should be used sparingly but important if the action could have harmful, unintended consequences
    pub warning: Option<String>,
    #[serde(default)]
    /// One of: "enabled", "hidden", or { disabled: "" }
    ///   - "enabled" - the action is available be run
    ///   - "hidden" - the action cannot be seen or run
    ///   - { disabled: "example explanation" } means the action is visible but cannot be run. Replace "example explanation" with a reason why the action is disable to prevent user confusion.
    pub visibility: ActionVisibility,
    /// One of: "only-stopped", "only-running", "all"
    ///   - "only-stopped" - the action can only be run when the service is stopped
    ///   - "only-running" - the action can only be run when the service is running
    ///   - "any" - the action can only be run regardless of the service's status
    pub allowed_statuses: AllowedStatuses,
    pub has_input: bool,
    /// If provided, this action will be nested under a header of this value, along with other actions of the same group
    pub group: Option<String>,
    /// Who is allowed to invoke this action directly via `effects.action.run`.
    ///   - "public" — any installed package
    ///   - "dependent" — only services that declare this package as a current dependency
    ///   - "user" — only the user (other services must create a task). Default when omitted.
    /// Services that lack direct access can always queue a task with `effects.action.createTask`.
    #[ts(optional)]
    pub access: Option<ActionAccess>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
#[serde(rename_all_fields = "camelCase")]
pub enum ActionVisibility {
    Hidden,
    Disabled(String),
    Enabled,
}
impl Default for ActionVisibility {
    fn default() -> Self {
        Self::Enabled
    }
}

#[derive(Debug, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct PackageDataEntry {
    pub state_info: PackageState,
    pub s9pk: PathBuf,
    pub status_info: StatusInfo,
    #[ts(type = "string | null")]
    pub registry: Option<Url>,
    #[ts(type = "string")]
    pub developer_key: Pem<ed25519_dalek::VerifyingKey>,
    pub icon: DataUrl<'static>,
    #[ts(type = "string | null")]
    pub last_backup: Option<DateTime<Utc>>,
    pub current_dependencies: CurrentDependencies,
    pub actions: BTreeMap<ActionId, ActionMetadata>,
    pub tasks: BTreeMap<ReplayId, TaskEntry>,
    pub hosts: Hosts,
    #[ts(type = "string[]")]
    pub store_exposed_dependents: Vec<JsonPointer>,
    #[ts(type = "string | null")]
    pub outbound_gateway: Option<GatewayId>,
    #[serde(default)]
    pub plugin: PackagePlugin,
}
impl AsRef<PackageDataEntry> for PackageDataEntry {
    fn as_ref(&self) -> &PackageDataEntry {
        self
    }
}

#[derive(Debug, Default, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct PackagePlugin {
    pub url: Option<UrlPluginRegistration>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UrlPluginRegistration {
    pub table_action: ActionId,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct CurrentDependencies(pub BTreeMap<PackageId, CurrentDependencyInfo>);
impl CurrentDependencies {
    pub fn map(
        mut self,
        transform: impl Fn(
            BTreeMap<PackageId, CurrentDependencyInfo>,
        ) -> BTreeMap<PackageId, CurrentDependencyInfo>,
    ) -> Self {
        self.0 = transform(self.0);
        self
    }
}
impl Map for CurrentDependencies {
    type Key = PackageId;
    type Value = CurrentDependencyInfo;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Ok(key)
    }
    fn key_string(key: &Self::Key) -> Result<InternedString, Error> {
        Ok(key.clone().into())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, TS, HasModel)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct CurrentDependencyInfo {
    pub title: Option<LocaleString>,
    pub icon: Option<DataUrl<'static>>,
    #[serde(flatten)]
    pub kind: CurrentDependencyKind,
    #[ts(type = "string")]
    pub version_range: VersionRange,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "kind")]
pub enum CurrentDependencyKind {
    Exists,
    #[serde(rename_all = "camelCase")]
    Running {
        #[serde(default)]
        #[ts(type = "string[]")]
        health_checks: BTreeSet<HealthCheckId>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, TS, HasModel)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
#[model = "Model<Self>"]
pub struct TaskEntry {
    pub task: Task,
    pub active: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS, HasModel)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
#[model = "Model<Self>"]
pub struct Task {
    pub package_id: PackageId,
    pub action_id: ActionId,
    #[serde(default)]
    pub severity: TaskSeverity,
    #[ts(optional)]
    pub reason: Option<String>,
    #[ts(optional)]
    pub when: Option<TaskTrigger>,
    #[ts(optional)]
    pub input: Option<TaskInput>,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum TaskSeverity {
    Optional,
    Important,
    Critical,
}
impl Default for TaskSeverity {
    fn default() -> Self {
        TaskSeverity::Important
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TaskTrigger {
    #[serde(default)]
    pub once: bool,
    pub condition: TaskCondition,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum TaskCondition {
    InputNotMatches,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "kind")]
pub enum TaskInput {
    Partial {
        #[ts(type = "Record<string, unknown>[]")]
        accept: Vec<Value>,
        #[ts(type = "Record<string, unknown>")]
        set: Value,
    },
}
// Accepts both the current `{ accept, set }` shape and the legacy `{ value }`
// shape emitted by s9pks built against the pre-2.0 SDK.
impl<'de> Deserialize<'de> for TaskInput {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case", tag = "kind")]
        enum Repr {
            Partial {
                #[serde(default)]
                accept: Option<Vec<Value>>,
                #[serde(default)]
                set: Option<Value>,
                #[serde(default)]
                value: Option<Value>,
            },
        }
        let Repr::Partial { accept, set, value } = Repr::deserialize(deserializer)?;
        match (accept, set, value) {
            (Some(accept), Some(set), _) => Ok(Self::Partial { accept, set }),
            (_, _, Some(value)) => Ok(Self::Partial {
                accept: vec![value.clone()],
                set: value,
            }),
            _ => Err(serde::de::Error::custom(
                "task input requires `accept` and `set`, or the legacy `value`",
            )),
        }
    }
}
impl TaskInput {
    pub fn matches(&self, input: Option<&Value>) -> bool {
        match self {
            Self::Partial { accept, .. } => match input {
                None => false,
                Some(full) => accept.iter().any(|a| is_partial_of(a, full)),
            },
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct InterfaceAddressMap(pub BTreeMap<HostId, InterfaceAddresses>);
impl Map for InterfaceAddressMap {
    type Key = HostId;
    type Value = InterfaceAddresses;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Ok(key)
    }
    fn key_string(key: &Self::Key) -> Result<InternedString, Error> {
        Ok(key.clone().into())
    }
}

#[derive(Debug, Deserialize, Serialize, HasModel)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct InterfaceAddresses {
    pub tor_address: Option<String>,
    pub lan_address: Option<String>,
}

#[cfg(test)]
mod task_input_tests {
    use serde_json::json;

    use super::TaskInput;

    #[test]
    fn legacy_value_shape_normalizes_to_accept_set() {
        let input: TaskInput =
            serde_json::from_value(json!({ "kind": "partial", "value": { "a": 1 } })).unwrap();
        assert_eq!(
            serde_json::to_value(&input).unwrap(),
            json!({ "kind": "partial", "accept": [{ "a": 1 }], "set": { "a": 1 } }),
        );
    }

    #[test]
    fn accept_set_shape_round_trips() {
        let wire = json!({
            "kind": "partial",
            "accept": [{ "a": 1 }, { "a": 2 }],
            "set": { "a": 1 },
        });
        let input: TaskInput = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(&input).unwrap(), wire);
    }

    #[test]
    fn rejects_empty_partial() {
        assert!(serde_json::from_value::<TaskInput>(json!({ "kind": "partial" })).is_err());
    }
}
