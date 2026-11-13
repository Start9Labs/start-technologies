use std::collections::{BTreeMap, BTreeSet};

use exver::VersionRange;
use rust_i18n::t;
use serde::{Deserialize, Serialize};

use crate::PackageId;
use crate::context::RpcContext;
use crate::db::model::DatabaseModel;
use crate::db::model::package::{
    AllPackageData, ManifestPreference, PackageDataEntry, PackageState,
};
use crate::prelude::*;
use crate::registry::package::index::PackageMetadata;
use crate::s9pk::S9pk;
use crate::service::action::{conditional_task_targets, reset_task_conditions};
use crate::status::DesiredStatus;
use crate::util::VersionString;
use crate::util::data_url::DataUrl;

/// A pin forbids fallback and retains presentation after the provider is removed.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct DependencySelection {
    pub provider_id: PackageId,
    #[serde(default)]
    pub pin: Option<ProviderPin>,
}
rpc_toolkit::reflect_ts!(DependencySelection);
rpc_toolkit::ts_export!(DependencySelection, namespaces = [""]);

/// Presentation retained while the pinned provider is absent.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct ProviderPin {
    pub title: String,
    pub icon: DataUrl<'static>,
}
rpc_toolkit::reflect_ts!(ProviderPin);
rpc_toolkit::ts_export!(ProviderPin, namespaces = [""]);

impl From<PackageId> for DependencySelection {
    fn from(provider_id: PackageId) -> Self {
        Self {
            provider_id,
            pin: None,
        }
    }
}

/// Observed providers use active manifests; staged updates do not change the dependency contract.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct DependencyBinding {
    pub provider_id: Option<PackageId>,
    pub versions: BTreeSet<VersionString>,
    pub candidates: BTreeMap<PackageId, BTreeSet<VersionString>>,
}

rpc_toolkit::reflect_ts!(DependencyBinding);
rpc_toolkit::ts_export!(DependencyBinding, namespaces = [""]);

/// Cross-package claims are direct; the provider's own version never counts toward another ID.
pub fn provided_versions(
    metadata: &PackageMetadata,
    id: &PackageId,
    version: &VersionString,
    dependency: &PackageId,
) -> BTreeSet<VersionString> {
    if id == dependency {
        metadata
            .satisfies
            .iter()
            .cloned()
            .chain([version.clone()])
            .collect()
    } else {
        metadata
            .satisfies_packages
            .get(dependency)
            .cloned()
            .unwrap_or_default()
    }
}

pub fn compatible(versions: &BTreeSet<VersionString>, range: &VersionRange) -> bool {
    range.satisfied_by_release(
        &versions
            .iter()
            .map(|version| (**version).clone())
            .collect::<Vec<_>>(),
    )
}

fn bindings_for(
    packages: &AllPackageData,
    owner: &PackageDataEntry,
) -> BTreeMap<PackageId, DependencyBinding> {
    owner
        .current_dependencies
        .0
        .iter()
        .map(|(dependency, requirement)| {
            let candidates: BTreeMap<_, _> = packages
                .0
                .iter()
                .filter_map(|(id, package)| {
                    let manifest = match &package.state_info {
                        PackageState::Installed(state) => &state.manifest,
                        PackageState::Updating(state) => &state.manifest,
                        _ => return None,
                    };
                    let versions = provided_versions(
                        &manifest.metadata,
                        &manifest.id,
                        &manifest.version,
                        dependency,
                    );
                    compatible(&versions, &requirement.version_range)
                        .then(|| (id.clone(), versions))
                })
                .collect();
            let selection = owner.dependency_selections.get(dependency);
            let preferred = selection
                .map(|selection| &selection.provider_id)
                .filter(|id| candidates.contains_key(*id));
            let provider_id = if selection.is_some_and(|selection| selection.pin.is_some()) {
                preferred.cloned()
            } else {
                preferred
                    .or_else(|| {
                        owner
                            .dependency_bindings
                            .get(dependency)
                            .and_then(|binding| binding.provider_id.as_ref())
                            .filter(|id| candidates.contains_key(*id))
                    })
                    .cloned()
                    .or_else(|| {
                        (candidates.len() == 1).then(|| candidates.keys().next().unwrap().clone())
                    })
            };
            let versions = provider_id
                .as_ref()
                .and_then(|id| candidates.get(id))
                .cloned()
                .unwrap_or_default();
            (
                dependency.clone(),
                DependencyBinding {
                    provider_id,
                    versions,
                    candidates,
                },
            )
        })
        .collect()
}

pub fn initialize_bindings(db: &mut DatabaseModel, owner: &PackageId) -> Result<(), Error> {
    let packages = db.as_public().as_package_data().de()?;
    let Some(package) = packages.0.get(owner) else {
        return Ok(());
    };
    let bindings = bindings_for(&packages, package);
    let previous_targets = conditional_task_targets(db, owner)?;
    db.as_public_mut()
        .as_package_data_mut()
        .as_idx_mut(owner)
        .or_not_found(owner)?
        .mutate(|package| {
            package.dependency_bindings = bindings;
            Ok(())
        })?;
    reset_task_conditions(db, owner, previous_targets).map(|_| ())
}

pub fn initialize_new_bindings(db: &mut DatabaseModel, owner: &PackageId) -> Result<(), Error> {
    let packages = db.as_public().as_package_data().de()?;
    let package = packages.0.get(owner).or_not_found(owner)?;
    let bindings = bindings_for(&packages, package);
    db.as_public_mut()
        .as_package_data_mut()
        .as_idx_mut(owner)
        .or_not_found(owner)?
        .mutate(|package| {
            for (id, binding) in bindings {
                package.dependency_bindings.entry(id).or_insert(binding);
            }
            Ok(())
        })
}

/// Declared dependencies return their effective provider, or `None` while unresolved.
/// Administrative, self, and undeclared IDs stay physical.
pub fn find_package(
    db: &DatabaseModel,
    owner: &PackageId,
    requested: &PackageId,
) -> Result<Option<PackageId>, Error> {
    if owner == requested || requested.is_start_os() {
        return Ok(Some(requested.clone()));
    }
    let package = db
        .as_public()
        .as_package_data()
        .as_idx(owner)
        .or_not_found(owner)?;
    if !package
        .as_current_dependencies()
        .de()?
        .0
        .contains_key(requested)
    {
        return Ok(Some(requested.clone()));
    }
    Ok(package
        .as_dependency_bindings()
        .as_idx(requested)
        .map(|binding| binding.as_provider_id().de())
        .transpose()?
        .flatten())
}

pub fn resolve_package(
    db: &DatabaseModel,
    owner: &PackageId,
    requested: &PackageId,
) -> Result<PackageId, Error> {
    find_package(db, owner, requested)?.ok_or_else(|| {
        Error::new(
            eyre!(
                "{}",
                t!("dependencies.providers.unresolved", dependency = requested)
            ),
            ErrorKind::NotFound,
        )
    })
}

fn identities(
    bindings: &BTreeMap<PackageId, DependencyBinding>,
) -> BTreeMap<PackageId, Option<PackageId>> {
    bindings
        .iter()
        .map(|(id, binding)| (id.clone(), binding.provider_id.clone()))
        .collect()
}

/// Binding identities change after the old container exits, including its mounts and callbacks.
/// Returns whether a busy service still needs reconciliation.
pub async fn reconcile(ctx: &RpcContext) -> Result<bool, Error> {
    let packages = ctx.db.peek().await.as_public().as_package_data().de()?;
    let mut retry = false;
    for (id, package) in &packages.0 {
        if !refreshable(package) || bindings_for(&packages, package) == package.dependency_bindings
        {
            continue;
        }
        if let Some(completed) = ctx
            .services
            .try_reconcile_dependencies(ctx, id)
            .await
            .log_err()
        {
            retry |= !completed;
        }
    }
    Ok(retry)
}

/// Bindings remain unchanged during backups and updates.
fn refreshable(package: &PackageDataEntry) -> bool {
    matches!(package.state_info, PackageState::Installed(_))
        && !matches!(
            package.status_info.desired,
            DesiredStatus::BackingUp { .. } | DesiredStatus::Updating { .. }
        )
}

pub(crate) fn refresh_bindings(db: &mut DatabaseModel, owner: &PackageId) -> Result<bool, Error> {
    let packages = db.as_public().as_package_data().de()?;
    let Some(package) = packages.0.get(owner) else {
        return Ok(false);
    };
    if !refreshable(package) {
        return Ok(false);
    }
    let desired = bindings_for(&packages, package);
    if identities(&desired) != identities(&package.dependency_bindings) {
        return Ok(true);
    }
    initialize_bindings(db, owner)?;
    Ok(false)
}

pub async fn initialize(ctx: &RpcContext) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            for id in db.as_public().as_package_data().keys()? {
                initialize_bindings(db, &id)?;
            }
            Ok(())
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SelectProviderParams {
    pub id: PackageId,
    pub dependency_id: PackageId,
    pub provider_id: Option<PackageId>,
    /// Locks to this ID without fallback.
    #[serde(default)]
    pub locked: bool,
}
rpc_toolkit::reflect_ts!(SelectProviderParams);

fn validate_selection(
    packages: &AllPackageData,
    params: &SelectProviderParams,
) -> Result<(), Error> {
    let owner = packages.0.get(&params.id).or_not_found(&params.id)?;
    let bindings = bindings_for(packages, owner);
    let binding = bindings
        .get(&params.dependency_id)
        .or_not_found(&params.dependency_id)?;
    if let Some(provider) = &params.provider_id {
        let unlocking = !params.locked
            && owner
                .dependency_selections
                .get(&params.dependency_id)
                .is_some_and(|selection| {
                    selection.provider_id == *provider && selection.pin.is_some()
                });
        if !binding.candidates.contains_key(provider) && !unlocking {
            return Err(Error::new(
                eyre!("{}", t!("dependencies.providers.incompatible")),
                ErrorKind::InvalidRequest,
            ));
        }
    }
    Ok(())
}

async fn provider_pin(ctx: &RpcContext, provider: &PackageId) -> Result<ProviderPin, Error> {
    let path = ctx
        .db
        .peek()
        .await
        .as_public()
        .as_package_data()
        .as_idx(provider)
        .or_not_found(provider)?
        .as_s9pk()
        .de()?;
    let s9pk = S9pk::open(path, Some(provider)).await?;
    Ok(ProviderPin {
        title: s9pk.as_manifest().metadata.title.to_string(),
        icon: s9pk.icon_data_url().await?,
    })
}

/// Locks constrain automatic fallback, not explicit provider selection.
pub async fn select_provider(ctx: RpcContext, params: SelectProviderParams) -> Result<(), Error> {
    validate_selection(
        &ctx.db.peek().await.as_public().as_package_data().de()?,
        &params,
    )?;
    let pin = match (&params.provider_id, params.locked) {
        (Some(provider), true) => Some(provider_pin(&ctx, provider).await?),
        _ => None,
    };
    ctx.db
        .mutate(|db| {
            validate_selection(&db.as_public().as_package_data().de()?, &params)?;
            db.as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(&params.id)
                .or_not_found(&params.id)?
                .mutate(|package| {
                    if let Some(provider) = params.provider_id {
                        package.dependency_selections.insert(
                            params.dependency_id,
                            DependencySelection {
                                provider_id: provider,
                                pin,
                            },
                        );
                    } else {
                        package.dependency_selections.remove(&params.dependency_id);
                    }
                    Ok(())
                })
        })
        .await
        .result
}

/// Captures the latest presentation before uninstall removes the provider archive.
pub(crate) async fn refresh_pins(ctx: &RpcContext, provider: &PackageId) -> Result<(), Error> {
    let packages = ctx.db.peek().await.as_public().as_package_data().de()?;
    if !packages.0.values().any(|owner| {
        owner
            .dependency_selections
            .values()
            .any(|selection| selection.provider_id == *provider && selection.pin.is_some())
    }) {
        return Ok(());
    }
    let pin = provider_pin(ctx, provider).await?;
    ctx.db
        .mutate(|db| refresh_cached_pins(db, provider, &pin))
        .await
        .result
}

fn refresh_cached_pins(
    db: &mut DatabaseModel,
    provider: &PackageId,
    pin: &ProviderPin,
) -> Result<(), Error> {
    for owner in db.as_public().as_package_data().keys()? {
        db.as_public_mut()
            .as_package_data_mut()
            .as_idx_mut(&owner)
            .or_not_found(&owner)?
            .as_dependency_selections_mut()
            .mutate(|selections| {
                for selection in selections.values_mut().filter(|selection| {
                    selection.provider_id == *provider && selection.pin.is_some()
                }) {
                    selection.pin = Some(pin.clone());
                }
                Ok(())
            })?;
    }
    Ok(())
}

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct PreviewProviderChangeParams {
    pub id: PackageId,
    pub version: Option<VersionString>,
    #[serde(default)]
    pub satisfies: BTreeSet<VersionString>,
    #[serde(default)]
    pub satisfies_packages: BTreeMap<PackageId, BTreeSet<VersionString>>,
}
rpc_toolkit::reflect_ts!(PreviewProviderChangeParams);

fn preview_change(
    mut packages: AllPackageData,
    params: PreviewProviderChangeParams,
) -> Vec<PackageId> {
    if let Some(version) = params.version {
        if let Some(package) = packages.0.get_mut(&params.id) {
            let manifest = package.state_info.as_manifest_mut(ManifestPreference::Old);
            manifest.version = version;
            manifest.metadata.satisfies = params.satisfies;
            manifest.metadata.satisfies_packages = params.satisfies_packages;
        }
    } else {
        packages.0.remove(&params.id);
    }
    packages
        .0
        .iter()
        .filter_map(|(owner, package)| {
            let desired = bindings_for(&packages, package);
            package
                .dependency_bindings
                .iter()
                .any(|(dependency, binding)| {
                    binding.provider_id.as_ref() == Some(&params.id)
                        && desired
                            .get(dependency)
                            .and_then(|binding| binding.provider_id.as_ref())
                            != Some(&params.id)
                })
                .then(|| owner.clone())
        })
        .collect()
}

pub async fn preview_provider_change(
    ctx: RpcContext,
    params: PreviewProviderChangeParams,
) -> Result<Vec<PackageId>, Error> {
    Ok(preview_change(
        ctx.db.peek().await.as_public().as_package_data().de()?,
        params,
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn packages() -> AllPackageData {
        serde_json::from_value(json!({
            "consumer": entry("consumer", "1.0:0", json!({}), json!({"target": {"title":null,"kind":"exists","versionRange": ">=2:0 && <3:0"}})),
            "provider": entry("provider", "99:0", json!({"target":["2.1:0"]}), json!({}))
        })).unwrap()
    }
    fn entry(
        id: &str,
        version: &str,
        claims: serde_json::Value,
        deps: serde_json::Value,
    ) -> serde_json::Value {
        json!({"stateInfo":{"state":"installed","manifest":{
            "id":id,"version":version,"canMigrateTo":"*","canMigrateFrom":"*","title":id,
            "description":{"short":"", "long":""},"releaseNotes":"","gitHash":null,"license":"MIT",
            "upstreamRepo":"https://example.com","marketingUrl":null,"donationUrl":null,"sdkVersion":null,
            "satisfies":[],"satisfiesPackages":claims,"images":{},"volumes":[],"dependencies":{}
        }},"s9pk":"/tmp/package.s9pk","statusInfo":{"health":{},"error":null,"started":null,"desired":{"main":"stopped"}},
        "registry":null,"developerKey":crate::util::serde::Der64::new(ed25519_dalek::SigningKey::from_bytes(&[1;32]).verifying_key()),"lastBackup":null,
        "currentDependencies":deps,"actions":{},"tasks":{},"hosts":{},"storeExposedDependents":[],"outboundGateway":null})
    }
    fn resolve(packages: &AllPackageData) -> DependencyBinding {
        bindings_for(packages, &packages.0["consumer"])["target"].clone()
    }
    #[test]
    fn lifecycle_operations_defer_binding_changes_until_completion() {
        let consumer = "consumer".parse().unwrap();
        let mut initial = db(packages());
        initialize_bindings(&mut initial, &consumer).unwrap();
        let binding = initial
            .as_public()
            .as_package_data()
            .as_idx(&consumer)
            .unwrap()
            .as_dependency_bindings()
            .de()
            .unwrap();
        for desired in [
            DesiredStatus::Stopped.backing_up(),
            DesiredStatus::Stopped.updating(),
        ] {
            let mut packages = initial.as_public().as_package_data().de().unwrap();
            packages.0.remove("provider");
            packages.0.get_mut("consumer").unwrap().status_info.desired = desired;
            assert!(!refreshable(&packages.0["consumer"]));
            let mut db = db(packages);
            assert!(!refresh_bindings(&mut db, &consumer).unwrap());
            assert_eq!(
                db.as_public()
                    .as_package_data()
                    .as_idx(&consumer)
                    .unwrap()
                    .as_dependency_bindings()
                    .de()
                    .unwrap(),
                binding
            );
            db.as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(&consumer)
                .unwrap()
                .as_status_info_mut()
                .as_desired_mut()
                .ser(&DesiredStatus::Stopped)
                .unwrap();
            assert!(refreshable(
                &db.as_public()
                    .as_package_data()
                    .as_idx(&consumer)
                    .unwrap()
                    .de()
                    .unwrap()
            ));
            assert!(refresh_bindings(&mut db, &consumer).unwrap());
            assert_eq!(
                db.as_public()
                    .as_package_data()
                    .as_idx(&consumer)
                    .unwrap()
                    .as_dependency_bindings()
                    .de()
                    .unwrap(),
                binding
            );
        }
    }

    fn pin(title: &str) -> ProviderPin {
        ProviderPin {
            title: title.into(),
            icon: DataUrl::from_vec("image/png", vec![1, 2, 3]),
        }
    }

    #[test]
    fn pinned_provider_never_falls_back_and_returns_when_reinstalled() {
        let mut packages = packages();
        packages.0.insert(
            "alternative".parse().unwrap(),
            serde_json::from_value(entry(
                "alternative",
                "3:0",
                json!({"target":["2.2:0"]}),
                json!({}),
            ))
            .unwrap(),
        );
        packages
            .0
            .get_mut("consumer")
            .unwrap()
            .dependency_selections
            .insert(
                "target".parse().unwrap(),
                DependencySelection {
                    provider_id: "provider".parse().unwrap(),
                    pin: Some(pin("Chosen provider")),
                },
            );
        assert_eq!(
            resolve(&packages).provider_id,
            Some("provider".parse().unwrap())
        );
        assert!(
            validate_selection(
                &packages,
                &SelectProviderParams {
                    id: "consumer".parse().unwrap(),
                    dependency_id: "target".parse().unwrap(),
                    provider_id: Some("alternative".parse().unwrap()),
                    locked: false,
                }
            )
            .is_ok()
        );
        let provider = packages.0.remove("provider").unwrap();
        assert!(resolve(&packages).provider_id.is_none());
        assert_eq!(resolve(&packages).candidates.len(), 1);
        let params = SelectProviderParams {
            id: "consumer".parse().unwrap(),
            dependency_id: "target".parse().unwrap(),
            provider_id: Some("provider".parse().unwrap()),
            locked: false,
        };
        assert!(validate_selection(&packages, &params).is_ok());
        assert!(
            validate_selection(
                &packages,
                &SelectProviderParams {
                    locked: true,
                    ..params
                }
            )
            .is_err()
        );
        packages.0.insert("provider".parse().unwrap(), provider);
        assert_eq!(
            resolve(&packages).provider_id,
            Some("provider".parse().unwrap())
        );
        packages
            .0
            .get_mut("provider")
            .unwrap()
            .state_info
            .as_manifest_mut(ManifestPreference::Old)
            .metadata
            .satisfies_packages
            .clear();
        assert!(resolve(&packages).provider_id.is_none());
        packages
            .0
            .get_mut("consumer")
            .unwrap()
            .dependency_selections
            .get_mut("target")
            .unwrap()
            .pin = None;
        assert_eq!(
            resolve(&packages).provider_id,
            Some("alternative".parse().unwrap())
        );
    }

    #[test]
    fn pinned_presentation_is_refreshed_and_survives_provider_removal_and_serialization() {
        let mut packages = packages();
        let selections = &mut packages
            .0
            .get_mut("consumer")
            .unwrap()
            .dependency_selections;
        selections.insert(
            "target".parse().unwrap(),
            DependencySelection {
                provider_id: "provider".parse().unwrap(),
                pin: Some(pin("Old title")),
            },
        );
        selections.insert(
            "unlocked".parse().unwrap(),
            "provider".parse::<PackageId>().unwrap().into(),
        );
        let mut db = db(packages);
        let updated = pin("Updated title");
        refresh_cached_pins(&mut db, &"provider".parse().unwrap(), &updated).unwrap();
        db.as_public_mut()
            .as_package_data_mut()
            .remove(&"provider".parse().unwrap())
            .unwrap();
        let stored: AllPackageData = serde_json::from_value(
            serde_json::to_value(db.as_public().as_package_data().de().unwrap()).unwrap(),
        )
        .unwrap();
        let selection = &stored.0["consumer"].dependency_selections["target"];
        assert_eq!(selection.pin.as_ref(), Some(&updated));
        assert_eq!(selection.provider_id, "provider".parse().unwrap());
        assert!(
            stored.0["consumer"].dependency_selections["unlocked"]
                .pin
                .is_none()
        );
        assert!(resolve(&stored).provider_id.is_none());
    }

    #[test]
    fn initialization_materializes_legacy_collections_without_losing_saved_intent() {
        let mut value = serde_json::to_value(packages()).unwrap();
        for field in ["dependencySelections", "dependencyBindings"] {
            value["consumer"].as_object_mut().unwrap().remove(field);
        }
        let mut db = DatabaseModel::from(imbl_value::json!({ "public": { "packageData": value } }));
        let consumer = "consumer".parse().unwrap();
        initialize_bindings(&mut db, &consumer).unwrap();
        let package = db.as_public().as_package_data().as_idx(&consumer).unwrap();
        assert!(package.as_dependency_selections().de().unwrap().is_empty());
        assert_eq!(
            package.as_status_info().as_desired().de().unwrap(),
            DesiredStatus::Stopped
        );
        db.as_public_mut()
            .as_package_data_mut()
            .as_idx_mut(&consumer)
            .unwrap()
            .as_dependency_selections_mut()
            .insert(
                &"target".parse().unwrap(),
                &DependencySelection::from("unavailable".parse::<PackageId>().unwrap()),
            )
            .unwrap();
        initialize_bindings(&mut db, &consumer).unwrap();
        assert_eq!(
            db.as_public()
                .as_package_data()
                .as_idx(&consumer)
                .unwrap()
                .as_dependency_selections()
                .de()
                .unwrap()["target"]
                .provider_id,
            "unavailable".parse().unwrap()
        );
        assert_eq!(
            find_package(&db, &consumer, &"target".parse().unwrap()).unwrap(),
            Some("provider".parse().unwrap())
        );
    }

    #[test]
    fn optional_resolution_preserves_absence_and_physical_ids() {
        let mut packages = packages();
        let requirement = packages.0["consumer"].current_dependencies.0["target"].clone();
        for id in ["consumer", "start-os"] {
            packages
                .0
                .get_mut("consumer")
                .unwrap()
                .current_dependencies
                .0
                .insert(id.parse().unwrap(), requirement.clone());
        }
        let mut db = db(packages);
        let consumer = "consumer".parse().unwrap();
        let target = "target".parse().unwrap();
        assert_eq!(find_package(&db, &consumer, &target).unwrap(), None);
        assert_eq!(
            resolve_package(&db, &consumer, &target).unwrap_err().kind,
            ErrorKind::NotFound
        );
        for id in ["consumer", "start-os", "absent"] {
            let physical = id.parse().unwrap();
            assert_eq!(
                find_package(&db, &consumer, &physical).unwrap(),
                Some(physical.clone())
            );
            assert_eq!(
                resolve_package(&db, &consumer, &physical).unwrap(),
                physical
            );
        }
        initialize_bindings(&mut db, &consumer).unwrap();
        assert_eq!(
            find_package(&db, &consumer, &target).unwrap(),
            Some("provider".parse().unwrap())
        );
        db.as_public_mut()
            .as_package_data_mut()
            .remove(&"provider".parse().unwrap())
            .unwrap();
        initialize_bindings(&mut db, &consumer).unwrap();
        assert_eq!(find_package(&db, &consumer, &target).unwrap(), None);
        assert_eq!(
            resolve_package(&db, &consumer, &target).unwrap_err().kind,
            ErrorKind::NotFound
        );
    }

    #[test]
    fn ambiguous_dependency_is_absent_for_optional_resolution() {
        let mut packages = packages();
        packages.0.insert(
            "alternative".parse().unwrap(),
            serde_json::from_value(entry(
                "alternative",
                "3:0",
                json!({"target":["2.2:0"]}),
                json!({}),
            ))
            .unwrap(),
        );
        let mut db = db(packages);
        let consumer = "consumer".parse().unwrap();
        let target = "target".parse().unwrap();
        initialize_bindings(&mut db, &consumer).unwrap();
        assert_eq!(find_package(&db, &consumer, &target).unwrap(), None);
        assert_eq!(
            resolve_package(&db, &consumer, &target).unwrap_err().kind,
            ErrorKind::NotFound
        );
    }

    #[test]
    fn provider_changes_reset_task_conditions_and_preserve_once_semantics() {
        use crate::service::action::update_tasks_for_service;

        let mut packages = packages();
        packages.0.insert(
            "alternative".parse().unwrap(),
            serde_json::from_value(entry(
                "alternative",
                "3:0",
                json!({"target":["2.2:0"]}),
                json!({}),
            ))
            .unwrap(),
        );
        packages
            .0
            .get_mut("consumer")
            .unwrap()
            .dependency_selections
            .insert(
                "target".parse().unwrap(),
                "provider".parse::<PackageId>().unwrap().into(),
            );
        let mut db = db(packages);
        let consumer = "consumer".parse().unwrap();
        let target = "target".parse().unwrap();
        let provider: PackageId = "provider".parse().unwrap();
        let alternative: PackageId = "alternative".parse().unwrap();
        let action = "configure".parse().unwrap();
        let task_id = "configure".parse().unwrap();
        let once_id = "once".parse().unwrap();
        initialize_bindings(&mut db, &consumer).unwrap();
        db.as_public_mut()
            .as_package_data_mut()
            .as_idx_mut(&consumer)
            .unwrap()
            .as_tasks_mut()
            .ser(
                &serde_json::from_value(json!({
                    "configure": { "active": false, "task": {
                        "packageId": "target", "actionId": "configure",
                        "when": { "condition": "input-not-matches", "once": false },
                        "input": { "kind": "partial", "accept": [{ "enabled": true }], "set": {} }
                    } },
                    "once": { "active": true, "task": {
                        "packageId": "target", "actionId": "configure",
                        "when": { "condition": "input-not-matches", "once": true },
                        "input": { "kind": "partial", "accept": [{ "enabled": true }], "set": {} }
                    } }
                }))
                .unwrap(),
            )
            .unwrap();
        let tasks = |db: &DatabaseModel| {
            db.as_public()
                .as_package_data()
                .as_idx(&consumer)
                .unwrap()
                .as_tasks()
                .de()
                .unwrap()
        };
        db.as_public_mut()
            .as_package_data_mut()
            .as_idx_mut(&consumer)
            .unwrap()
            .as_dependency_selections_mut()
            .insert(&target, &DependencySelection::from(alternative.clone()))
            .unwrap();
        assert!(refresh_bindings(&mut db, &consumer).unwrap());
        assert!(!tasks(&db)[&task_id].active);
        assert_eq!(
            find_package(&db, &consumer, &target).unwrap(),
            Some(provider.clone())
        );
        initialize_bindings(&mut db, &consumer).unwrap();
        assert!(tasks(&db)[&task_id].active);
        update_tasks_for_service(
            &mut db,
            &provider,
            &action,
            &imbl_value::json!({"enabled":true}),
            false,
        )
        .unwrap();
        assert!(tasks(&db)[&task_id].active);
        update_tasks_for_service(
            &mut db,
            &alternative,
            &action,
            &imbl_value::json!({"enabled":false}),
            false,
        )
        .unwrap();
        assert!(tasks(&db)[&task_id].active);
        update_tasks_for_service(
            &mut db,
            &alternative,
            &action,
            &imbl_value::json!({"enabled":true}),
            false,
        )
        .unwrap();
        assert!(!tasks(&db)[&task_id].active);
        assert_eq!(tasks(&db)[&task_id].task.package_id, target);
        assert!(tasks(&db)[&once_id].active);
        update_tasks_for_service(
            &mut db,
            &alternative,
            &action,
            &imbl_value::json!({"enabled":true}),
            true,
        )
        .unwrap();
        assert!(!tasks(&db).contains_key(&once_id));
        for id in [&provider, &alternative] {
            db.as_public_mut().as_package_data_mut().remove(id).unwrap();
        }
        initialize_bindings(&mut db, &consumer).unwrap();
        assert_eq!(find_package(&db, &consumer, &target).unwrap(), None);
        assert!(tasks(&db)[&task_id].active);
        assert!(!tasks(&db).contains_key(&once_id));
        update_tasks_for_service(
            &mut db,
            &alternative,
            &action,
            &imbl_value::json!({"enabled":true}),
            false,
        )
        .unwrap();
        assert!(tasks(&db)[&task_id].active);
    }

    #[test]
    fn unique_provider_uses_package_specific_versions() {
        let packages = packages();
        let binding = resolve(&packages);
        assert_eq!(binding.provider_id, Some("provider".parse().unwrap()));
        assert_eq!(binding.versions, BTreeSet::from(["2.1:0".parse().unwrap()]));
    }
    #[test]
    fn unrelated_versions_and_transitive_claims_do_not_fulfill_dependency() {
        let mut packages = packages();
        let provider = packages.0.get_mut("provider").unwrap();
        let manifest = provider.state_info.as_manifest_mut(ManifestPreference::Old);
        manifest.version = "2.5:0".parse().unwrap();
        manifest.metadata.satisfies_packages = BTreeMap::from([(
            "other".parse().unwrap(),
            BTreeSet::from(["2.1:0".parse().unwrap()]),
        )]);
        assert!(resolve(&packages).provider_id.is_none());
    }
    #[test]
    fn ambiguity_requires_selection_and_unavailable_selection_falls_back() {
        let mut packages = packages();
        packages.0.insert(
            "alternative".parse().unwrap(),
            serde_json::from_value(entry(
                "alternative",
                "3:0",
                json!({"target":["2.2:0"]}),
                json!({}),
            ))
            .unwrap(),
        );
        assert!(resolve(&packages).provider_id.is_none());
        packages
            .0
            .get_mut("consumer")
            .unwrap()
            .dependency_selections
            .insert(
                "target".parse().unwrap(),
                "provider".parse::<PackageId>().unwrap().into(),
            );
        assert_eq!(
            resolve(&packages).provider_id,
            Some("provider".parse().unwrap())
        );
        packages.0.remove("provider");
        assert_eq!(
            resolve(&packages).provider_id,
            Some("alternative".parse().unwrap())
        );
    }
    #[test]
    fn new_candidates_do_not_displace_a_working_binding() {
        let mut packages = packages();
        let binding = resolve(&packages);
        packages
            .0
            .get_mut("consumer")
            .unwrap()
            .dependency_bindings
            .insert("target".parse().unwrap(), binding);
        packages.0.insert(
            "alternative".parse().unwrap(),
            serde_json::from_value(entry(
                "alternative",
                "3:0",
                json!({"target":["2.2:0"]}),
                json!({}),
            ))
            .unwrap(),
        );
        assert_eq!(
            resolve(&packages).provider_id,
            Some("provider".parse().unwrap())
        );
    }
    fn db(packages: AllPackageData) -> DatabaseModel {
        DatabaseModel::from(imbl_value::json!({ "public": { "packageData": packages } }))
    }

    #[test]
    fn refresh_keeps_effective_identity_until_container_replacement() {
        let mut db = db(packages());
        let consumer = "consumer".parse().unwrap();
        let target = "target".parse().unwrap();
        initialize_bindings(&mut db, &consumer).unwrap();
        db.as_public_mut()
            .as_package_data_mut()
            .remove(&"provider".parse().unwrap())
            .unwrap();
        assert!(refresh_bindings(&mut db, &consumer).unwrap());
        assert_eq!(
            resolve_package(&db, &consumer, &target).unwrap(),
            "provider".parse().unwrap()
        );
        initialize_bindings(&mut db, &consumer).unwrap();
        assert_eq!(
            resolve_package(&db, &consumer, &target).unwrap_err().kind,
            ErrorKind::NotFound
        );
        assert_eq!(
            db.as_public()
                .as_package_data()
                .as_idx(&consumer)
                .unwrap()
                .as_status_info()
                .as_desired()
                .de()
                .unwrap(),
            DesiredStatus::Stopped
        );
    }

    #[test]
    fn saved_preference_returns_after_sole_provider_fallback() {
        let mut packages = packages();
        let alternative = serde_json::from_value(entry(
            "alternative",
            "1:0",
            json!({"target":["2.2:0"]}),
            json!({}),
        ))
        .unwrap();
        packages
            .0
            .insert("alternative".parse().unwrap(), alternative);
        packages
            .0
            .get_mut("consumer")
            .unwrap()
            .dependency_selections
            .insert(
                "target".parse().unwrap(),
                "provider".parse::<PackageId>().unwrap().into(),
            );
        let original = packages.0.remove("provider").unwrap();
        let fallback = resolve(&packages);
        assert_eq!(fallback.provider_id, Some("alternative".parse().unwrap()));
        packages
            .0
            .get_mut("consumer")
            .unwrap()
            .dependency_bindings
            .insert("target".parse().unwrap(), fallback);
        packages.0.insert("provider".parse().unwrap(), original);
        assert_eq!(
            resolve(&packages).provider_id,
            Some("provider".parse().unwrap())
        );
    }

    #[test]
    fn previews_include_consumers_of_an_alias_and_preserve_compatible_claims() {
        let mut packages = packages();
        let binding = resolve(&packages);
        packages
            .0
            .get_mut("consumer")
            .unwrap()
            .dependency_bindings
            .insert("target".parse().unwrap(), binding);
        let params = |claims| PreviewProviderChangeParams {
            id: "provider".parse().unwrap(),
            version: Some("100:0".parse().unwrap()),
            satisfies: BTreeSet::new(),
            satisfies_packages: claims,
        };
        let kept: AllPackageData =
            serde_json::from_value(serde_json::to_value(&packages).unwrap()).unwrap();
        assert!(
            preview_change(
                kept,
                params(BTreeMap::from([(
                    "target".parse().unwrap(),
                    BTreeSet::from(["2.5:0".parse().unwrap()])
                )]))
            )
            .is_empty()
        );
        assert_eq!(
            preview_change(packages, params(BTreeMap::new())),
            vec!["consumer".parse::<PackageId>().unwrap()]
        );
    }

    #[test]
    fn same_id_uses_own_aliases_and_other_claims_cannot_veto_it() {
        let mut packages = packages();
        let provider = packages.0.get_mut("provider").unwrap();
        let manifest = provider.state_info.as_manifest_mut(ManifestPreference::Old);
        manifest.metadata.satisfies = BTreeSet::from(["2.3:0".parse().unwrap()]);
        let logical: PackageId = "provider".parse().unwrap();
        let requirement = packages
            .0
            .get_mut("consumer")
            .unwrap()
            .current_dependencies
            .0
            .remove("target")
            .unwrap();
        packages
            .0
            .get_mut("consumer")
            .unwrap()
            .current_dependencies
            .0
            .insert(logical.clone(), requirement);
        let binding = bindings_for(&packages, &packages.0["consumer"])[&logical].clone();
        assert_eq!(binding.provider_id, Some(logical));
        assert!(binding.versions.contains(&"99:0".parse().unwrap()));
        assert!(binding.versions.contains(&"2.3:0".parse().unwrap()));
    }

    #[test]
    fn staged_claim_withdrawal_does_not_displace_active_provider() {
        let mut packages = packages();
        let provider = packages.0.get_mut("provider").unwrap();
        let old = provider
            .state_info
            .as_manifest(ManifestPreference::Old)
            .clone();
        let mut staged = old.clone();
        staged.metadata.satisfies_packages.clear();
        provider.state_info = PackageState::Updating(crate::db::model::package::UpdatingState {
            manifest: old,
            s9pk: "/tmp/new.s9pk".into(),
            installing_info: crate::db::model::package::InstallingInfo {
                new_manifest: staged,
                progress: crate::progress::FullProgress::new(),
            },
        });
        assert_eq!(
            resolve(&packages).provider_id,
            Some("provider".parse().unwrap())
        );
    }

    #[test]
    fn stopped_provider_counts_but_installing_and_removing_do_not() {
        let mut packages = packages();
        assert!(resolve(&packages).provider_id.is_some());
        let provider = packages.0.get_mut("provider").unwrap();
        provider.state_info = PackageState::Removing(crate::db::model::package::InstalledState {
            manifest: provider
                .state_info
                .as_manifest(ManifestPreference::Old)
                .clone(),
        });
        assert!(resolve(&packages).provider_id.is_none());
    }
}
