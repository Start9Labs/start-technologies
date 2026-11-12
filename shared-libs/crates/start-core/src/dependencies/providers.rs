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
use crate::service::action::{conditional_task_targets, reset_task_conditions};
use crate::status::DesiredStatus;
use crate::util::VersionString;

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
            // A saved preference returns when available; fallback never elects among several candidates.
            let provider_id = owner
                .dependency_selections
                .get(dependency)
                .filter(|id| candidates.contains_key(*id))
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
                });
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
pub async fn reconcile(ctx: &RpcContext) -> Result<(), Error> {
    let packages = ctx.db.peek().await.as_public().as_package_data().de()?;
    for (id, package) in &packages.0 {
        if !matches!(package.state_info, PackageState::Installed(_)) {
            continue;
        }
        let desired = bindings_for(&packages, package);
        if desired == package.dependency_bindings {
            continue;
        }
        ctx.services.reconcile_dependencies(ctx, id).await?;
    }
    Ok(())
}

pub(crate) fn refresh_bindings(db: &mut DatabaseModel, owner: &PackageId) -> Result<bool, Error> {
    let packages = db.as_public().as_package_data().de()?;
    let Some(package) = packages.0.get(owner) else {
        return Ok(false);
    };
    if !matches!(package.state_info, PackageState::Installed(_))
        || matches!(
            package.status_info.desired,
            DesiredStatus::BackingUp { .. } | DesiredStatus::Updating { .. }
        )
    {
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
}
rpc_toolkit::reflect_ts!(SelectProviderParams);

pub async fn select_provider(ctx: RpcContext, params: SelectProviderParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            let packages = db.as_public().as_package_data().de()?;
            let package = packages.0.get(&params.id).or_not_found(&params.id)?;
            let bindings = bindings_for(&packages, package);
            let binding = bindings
                .get(&params.dependency_id)
                .or_not_found(&params.dependency_id)?;
            if params
                .provider_id
                .as_ref()
                .is_some_and(|id| !binding.candidates.contains_key(id))
            {
                return Err(Error::new(
                    eyre!("{}", t!("dependencies.providers.incompatible")),
                    ErrorKind::InvalidRequest,
                ));
            }
            db.as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(&params.id)
                .or_not_found(&params.id)?
                .mutate(|package| {
                    if let Some(provider) = &params.provider_id {
                        package
                            .dependency_selections
                            .insert(params.dependency_id.clone(), provider.clone());
                    } else {
                        package.dependency_selections.remove(&params.dependency_id);
                    }
                    Ok(())
                })
        })
        .await
        .result
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
            .insert(&"target".parse().unwrap(), &"unavailable".parse().unwrap())
            .unwrap();
        initialize_bindings(&mut db, &consumer).unwrap();
        assert_eq!(
            db.as_public()
                .as_package_data()
                .as_idx(&consumer)
                .unwrap()
                .as_dependency_selections()
                .de()
                .unwrap()["target"],
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
            .insert("target".parse().unwrap(), "provider".parse().unwrap());
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
            .insert(&target, &alternative)
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
            .insert("target".parse().unwrap(), "provider".parse().unwrap());
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
            .insert("target".parse().unwrap(), "provider".parse().unwrap());
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
