use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use exver::VersionRange;
use imbl_value::InternedString;
use patch_db::HasModel;
use patch_db::json_ptr::JsonPointer;
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::net::host::Hosts;
use crate::prelude::*;
use crate::progress::FullProgress;
use crate::s9pk::manifest::{LocaleString, Manifest};
use crate::status::StatusInfo;
use crate::util::DataUrl;
use crate::util::serde::{Pem, is_partial_of};
use crate::{ActionId, GatewayId, HealthCheckId, HostId, PackageId, ReplayId};

#[derive(Debug, Default, Deserialize, Serialize, VisitFields)]
pub struct AllPackageData(pub BTreeMap<PackageId, PackageDataEntry>);

rpc_toolkit::reflect_ts!(AllPackageData);
rpc_toolkit::ts_export!(AllPackageData, namespaces = [""]);
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

#[derive(Debug, Deserialize, Serialize, HasModel, VisitVariants)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "state")]
#[model = "Model<Self>"]
pub enum PackageState {
    Installing(InstallingState),
    Restoring(InstallingState),
    Updating(UpdatingState),
    Installed(InstalledState),
    Removing(InstalledState),
}

rpc_toolkit::reflect_ts!(PackageState);
rpc_toolkit::ts_export!(PackageState, namespaces = [""]);
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

#[derive(Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct InstallingState {
    pub installing_info: InstallingInfo,
}

rpc_toolkit::reflect_ts!(InstallingState);
rpc_toolkit::ts_export!(InstallingState, namespaces = [""]);

#[derive(Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct UpdatingState {
    pub manifest: Manifest,
    pub s9pk: PathBuf,
    pub installing_info: InstallingInfo,
}

rpc_toolkit::reflect_ts!(UpdatingState);
rpc_toolkit::ts_export!(UpdatingState, namespaces = [""]);

#[derive(Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct InstalledState {
    pub manifest: Manifest,
}

rpc_toolkit::reflect_ts!(InstalledState);
rpc_toolkit::ts_export!(InstalledState, namespaces = [""]);

#[derive(Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct InstallingInfo {
    pub new_manifest: Manifest,
    pub progress: FullProgress,
}

rpc_toolkit::reflect_ts!(InstallingInfo);
rpc_toolkit::ts_export!(InstallingInfo, namespaces = [""]);
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, VisitVariants)]
#[serde(rename_all = "kebab-case")]
pub enum AllowedStatuses {
    OnlyRunning,
    OnlyStopped,
    Any,
}

rpc_toolkit::reflect_ts!(AllowedStatuses);
rpc_toolkit::ts_export!(AllowedStatuses, namespaces = [""]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, VisitVariants)]
#[serde(rename_all = "kebab-case")]
pub enum ActionAccess {
    Public,
    Dependent,
    User,
}

rpc_toolkit::reflect_ts!(ActionAccess);
rpc_toolkit::ts_export!(ActionAccess, namespaces = [""]);
impl Default for ActionAccess {
    fn default() -> Self {
        Self::User
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, HasModel, VisitFields)]
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
    pub access: Option<ActionAccess>,
}

rpc_toolkit::reflect_ts!(ActionMetadata);

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, VisitVariants)]
#[serde(rename_all = "kebab-case")]
#[serde(rename_all_fields = "camelCase")]
pub enum ActionVisibility {
    Hidden,
    Disabled(String),
    Enabled,
}

rpc_toolkit::reflect_ts!(ActionVisibility);
rpc_toolkit::ts_export!(ActionVisibility, namespaces = [""]);
impl Default for ActionVisibility {
    fn default() -> Self {
        Self::Enabled
    }
}

#[derive(Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct PackageDataEntry {
    pub state_info: PackageState,
    pub s9pk: PathBuf,
    pub status_info: StatusInfo,
    #[visit(ts(type = "string | null"), wire = "Option<rpc_toolkit::ts::Unknown>")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub registry: Option<Url>,
    #[visit(ts(type = "string"), wire = "rpc_toolkit::ts::Unknown")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub developer_key: Pem<ed25519_dalek::VerifyingKey>,
    pub icon: DataUrl<'static>,
    #[visit(ts(type = "string | null"), wire = "Option<rpc_toolkit::ts::Unknown>")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub last_backup: Option<DateTime<Utc>>,
    pub current_dependencies: CurrentDependencies,
    pub actions: BTreeMap<ActionId, ActionMetadata>,
    pub tasks: BTreeMap<ReplayId, TaskEntry>,
    pub hosts: Hosts,
    #[visit(ts(type = "string[]"), wire = "rpc_toolkit::ts::Unknown")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub store_exposed_dependents: Vec<JsonPointer>,
    #[visit(ts(type = "string | null"), wire = "Option<rpc_toolkit::ts::Unknown>")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub outbound_gateway: Option<GatewayId>,
    #[serde(default)]
    pub plugin: PackagePlugin,
}

rpc_toolkit::reflect_ts!(PackageDataEntry);
rpc_toolkit::ts_export!(PackageDataEntry, namespaces = [""]);
impl AsRef<PackageDataEntry> for PackageDataEntry {
    fn as_ref(&self) -> &PackageDataEntry {
        self
    }
}

#[derive(Debug, Default, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct PackagePlugin {
    pub url: Option<UrlPluginRegistration>,
}

rpc_toolkit::reflect_ts!(PackagePlugin);
rpc_toolkit::ts_export!(PackagePlugin, namespaces = [""]);

#[derive(Debug, Clone, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct UrlPluginRegistration {
    pub table_action: ActionId,
}

rpc_toolkit::reflect_ts!(UrlPluginRegistration);
rpc_toolkit::ts_export!(UrlPluginRegistration, namespaces = [""]);

#[derive(Debug, Clone, Default, Deserialize, Serialize, VisitFields)]
pub struct CurrentDependencies(pub BTreeMap<PackageId, CurrentDependencyInfo>);

rpc_toolkit::reflect_ts!(CurrentDependencies);
rpc_toolkit::ts_export!(CurrentDependencies, namespaces = [""]);
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
impl CurrentDependencies {
    /// Whether tasks on the target count against the owning package.
    pub fn is_task_target(&self, owner: &PackageId, target: &PackageId) -> bool {
        target == owner || self.0.contains_key(target)
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

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields, HasModel)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct CurrentDependencyInfo {
    pub title: Option<LocaleString>,
    pub icon: Option<DataUrl<'static>>,
    #[serde(flatten)]
    pub kind: CurrentDependencyKind,
    pub version_range: VersionRange,
}

rpc_toolkit::reflect_ts!(CurrentDependencyInfo);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, VisitVariants)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "kind")]
pub enum CurrentDependencyKind {
    Exists,
    #[serde(rename_all = "camelCase")]
    Running {
        #[serde(default)]
        #[visit(ts(type = "string[]"), wire = "rpc_toolkit::ts::Unknown")]
        #[visit(opaque, type_attributes(visit::wire))]
        health_checks: BTreeSet<HealthCheckId>,
    },
}

rpc_toolkit::reflect_ts!(CurrentDependencyKind);

impl Model<PackageDataEntry> {
    /// Whether an active critical task on the package or a current dependency blocks starting.
    pub fn has_blocking_task(&self, id: &PackageId) -> Result<bool, Error> {
        let deps = self.as_current_dependencies().de()?;
        Ok(self.as_tasks().de()?.into_values().any(|t| {
            t.active
                && t.task.severity == TaskSeverity::Critical
                && deps.is_task_target(id, &t.task.package_id)
        }))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields, HasModel)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct TaskEntry {
    pub task: Task,
    pub active: bool,
}

rpc_toolkit::reflect_ts!(TaskEntry);
rpc_toolkit::ts_export!(TaskEntry, namespaces = [""]);

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields, HasModel)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[visit(ts(input_rename = "TaskParams"))]
pub struct Task {
    pub package_id: PackageId,
    pub action_id: ActionId,
    #[serde(default)]
    pub severity: TaskSeverity,

    pub reason: Option<String>,

    pub when: Option<TaskTrigger>,

    pub input: Option<TaskInput>,
}

rpc_toolkit::reflect_ts!(Task);
rpc_toolkit::ts_export!(Task, namespaces = [""]);

#[derive(Clone, Debug, Deserialize, Serialize, VisitVariants, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum TaskSeverity {
    Optional,
    Important,
    Critical,
}

rpc_toolkit::reflect_ts!(TaskSeverity);
rpc_toolkit::ts_export!(TaskSeverity, namespaces = [""]);
impl Default for TaskSeverity {
    fn default() -> Self {
        TaskSeverity::Important
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct TaskTrigger {
    #[serde(default)]
    pub once: bool,
    pub condition: TaskCondition,
}

rpc_toolkit::reflect_ts!(TaskTrigger);
rpc_toolkit::ts_export!(TaskTrigger, namespaces = [""]);

#[derive(Clone, Debug, Deserialize, Serialize, VisitVariants)]
#[serde(rename_all = "kebab-case")]
pub enum TaskCondition {
    InputNotMatches,
}

rpc_toolkit::reflect_ts!(TaskCondition);
rpc_toolkit::ts_export!(TaskCondition, namespaces = [""]);

#[derive(Clone, Debug, Serialize, VisitVariants)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "kind")]
#[visit(input_wire = "TaskInputRepr")]
#[visit(type_attributes(visit::input_wire))]
pub enum TaskInput {
    Partial {
        #[visit(
            ts(type = "Record<string, unknown>[]"),
            wire = "rpc_toolkit::ts::Unknown"
        )]
        #[visit(opaque, type_attributes(visit::wire))]
        accept: Vec<Value>,
        #[visit(
            ts(type = "Record<string, unknown>"),
            wire = "rpc_toolkit::ts::Unknown"
        )]
        #[visit(opaque, type_attributes(visit::wire))]
        set: Value,
    },
}

rpc_toolkit::reflect_ts!(TaskInput);
#[derive(Deserialize, VisitVariants)]
#[serde(rename_all = "kebab-case", tag = "kind")]
enum TaskInputRepr {
    Partial {
        #[serde(default)]
        accept: Option<Vec<Value>>,
        #[serde(default)]
        set: Option<Value>,
        #[serde(default)]
        value: Option<Value>,
    },
}

rpc_toolkit::reflect_ts!(TaskInputRepr);

impl<'de> Deserialize<'de> for TaskInput {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let TaskInputRepr::Partial { accept, set, value } =
            TaskInputRepr::deserialize(deserializer)?;
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
