use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use clap::builder::ValueParserFactory;
use exver::VersionRange;
use patch_db::ModelExt;
use rust_i18n::t;

use crate::db::model::DatabaseModel;
use crate::db::model::package::{
    CurrentDependencies, CurrentDependencyInfo, CurrentDependencyKind, ManifestPreference,
    TaskEntry,
};
use crate::dependencies::DepInfo;
use crate::disk::mount::filesystem::bind::{Bind, FileType};
use crate::disk::mount::filesystem::idmapped::IdMap;
use crate::disk::mount::filesystem::{FileSystem, MountType};
use crate::disk::mount::util::{is_mountpoint, unmount};
use crate::lxc::service_filesystem;
use crate::s9pk::S9pk;
use crate::s9pk::manifest::Manifest;
use crate::service::effects::callbacks::CallbackHandler;
use crate::service::effects::prelude::*;
use crate::service::rpc::CallbackId;
use crate::status::health_check::NamedHealthCheckResult;
use crate::util::{FromStrParser, VersionString};
use crate::volume::data_dir;
use crate::{DATA_DIR, HealthCheckId, PackageId, ReplayId, VolumeId};

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct MountTarget {
    package_id: PackageId,
    volume_id: VolumeId,
    subpath: Option<PathBuf>,
    readonly: bool,
    #[serde(default)]
    idmap: Vec<IdMap>,
    #[serde(skip_deserializing)]
    #[visit(ts(skip))]
    filetype: FileType,
}

rpc_toolkit::reflect_ts!(MountTarget);
rpc_toolkit::ts_export!(MountTarget, namespaces = [""]);

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct MountParams {
    location: PathBuf,
    target: MountTarget,
}

rpc_toolkit::reflect_ts!(MountParams);
rpc_toolkit::ts_export!(MountParams, namespaces = [""]);

fn relative_components(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::ParentDir => out.push(component.as_os_str()),
            _ => {}
        }
    }
    out
}

async fn confined_join(base: &Path, rel: &Path) -> Result<PathBuf, Error> {
    let base = crate::util::io::canonicalize(base, false).await?;
    let joined = crate::util::io::canonicalize(base.join(relative_components(rel)), false).await?;
    if !joined.starts_with(&base) {
        return Err(Error::new(
            eyre!("mount paths must stay within their base directory"),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(joined)
}

pub async fn mount(
    context: EffectContext,
    MountParams {
        location,
        target:
            MountTarget {
                package_id,
                volume_id,
                subpath,
                readonly,
                idmap,
                filetype,
            },
    }: MountParams,
) -> Result<(), Error> {
    let context = context.deref()?;
    let volume = data_dir(DATA_DIR, &package_id, &volume_id);
    if tokio::fs::metadata(&volume).await.is_err() {
        return Err(Error::new(
            eyre!(
                "{}",
                t!(
                    "service.effects.dependency.volume-missing",
                    package = package_id,
                    volume = volume_id
                )
            ),
            ErrorKind::NotFound,
        ));
    }
    let source = confined_join(&volume, subpath.as_deref().unwrap_or(Path::new(""))).await?;
    let rootfs = context
        .seed
        .persistent_container
        .lxc_container
        .get()
        .or_not_found("lxc container")?
        .rootfs_dir()
        .to_owned();
    let mountpoint = confined_join(&rootfs, &location).await?;

    if is_mountpoint(&mountpoint).await? {
        unmount(&mountpoint, true).await?;
    }

    service_filesystem(
        Bind::new(source).with_type(filetype).recursive(true),
        &idmap,
    )
    .mount(
        mountpoint,
        if readonly {
            MountType::ReadOnly
        } else {
            MountType::ReadWrite
        },
    )
    .await?;

    Ok(())
}

pub async fn get_installed_packages(context: EffectContext) -> Result<BTreeSet<PackageId>, Error> {
    context
        .deref()?
        .seed
        .ctx
        .db
        .peek()
        .await
        .into_public()
        .into_package_data()
        .keys()
}

#[derive(Debug, Clone, Deserialize, Serialize, visit_rs::VisitVariants)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[serde(rename_all_fields = "camelCase")]
pub enum DependencyRequirement {
    Running {
        id: PackageId,
        health_checks: BTreeSet<HealthCheckId>,
        version_range: VersionRange,
    },
    Exists {
        id: PackageId,
        version_range: VersionRange,
    },
}

rpc_toolkit::reflect_ts!(DependencyRequirement);
rpc_toolkit::ts_export!(DependencyRequirement, namespaces = [""]);
// filebrowser:exists,bitcoind:running:foo+bar+baz
impl FromStr for DependencyRequirement {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once(':') {
            Some((id, "e")) | Some((id, "exists")) => Ok(Self::Exists {
                id: id.parse()?,
                version_range: "*".parse()?, // TODO
            }),
            Some((id, rest)) => {
                let health_checks = match rest.split_once(':') {
                    Some(("r", rest)) | Some(("running", rest)) => rest
                        .split('+')
                        .map(|id| id.parse().map_err(Error::from))
                        .collect(),
                    Some((kind, _)) => Err(Error::new(
                        eyre!(
                            "{}",
                            t!(
                                "service.effects.dependency.unknown-dependency-kind",
                                kind = kind
                            )
                        ),
                        ErrorKind::InvalidRequest,
                    )),
                    None => match rest {
                        "r" | "running" => Ok(BTreeSet::new()),
                        kind => Err(Error::new(
                            eyre!(
                                "{}",
                                t!(
                                    "service.effects.dependency.unknown-dependency-kind",
                                    kind = kind
                                )
                            ),
                            ErrorKind::InvalidRequest,
                        )),
                    },
                }?;
                Ok(Self::Running {
                    id: id.parse()?,
                    health_checks,
                    version_range: "*".parse()?, // TODO
                })
            }
            None => Ok(Self::Running {
                id: s.parse()?,
                health_checks: BTreeSet::new(),
                version_range: "*".parse()?, // TODO
            }),
        }
    }
}
impl ValueParserFactory for DependencyRequirement {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        FromStrParser::new()
    }
}
#[derive(Deserialize, Serialize, Parser, visit_rs::VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "camelCase")]
pub struct SetDependenciesParams {
    dependencies: Vec<DependencyRequirement>,
}

rpc_toolkit::reflect_ts!(SetDependenciesParams);
rpc_toolkit::ts_export!(SetDependenciesParams, namespaces = [""]);
fn required_base(info: &DepInfo) -> Option<(CurrentDependencyKind, VersionRange)> {
    if info.optional {
        return None;
    }
    Some((info.kind.clone()?, info.version_range.clone()?))
}

fn narrow_to_base(
    base: &DepInfo,
    kind: CurrentDependencyKind,
    version_range: VersionRange,
) -> (CurrentDependencyKind, VersionRange) {
    let kind = match (&base.kind, kind) {
        (
            Some(CurrentDependencyKind::Running {
                health_checks: base,
            }),
            CurrentDependencyKind::Running { mut health_checks },
        ) => {
            health_checks.extend(base.iter().cloned());
            CurrentDependencyKind::Running { health_checks }
        }
        (Some(CurrentDependencyKind::Running { health_checks }), CurrentDependencyKind::Exists) => {
            CurrentDependencyKind::Running {
                health_checks: health_checks.clone(),
            }
        }
        (_, kind) => kind,
    };
    let version_range = if let Some(base) = &base.version_range {
        let outside_base =
            VersionRange::and(version_range.clone(), VersionRange::not(base.clone()));
        if outside_base.satisfiable() {
            VersionRange::and(base.clone(), version_range)
        } else {
            version_range
        }
    } else {
        version_range
    };
    (kind, version_range)
}

async fn dependency_info(
    s9pk: &S9pk,
    dep_id: &PackageId,
    kind: CurrentDependencyKind,
    version_range: VersionRange,
) -> Result<CurrentDependencyInfo, Error> {
    Ok(CurrentDependencyInfo {
        title: s9pk.dependency_metadata(dep_id).await?.map(|m| m.title),
        kind,
        version_range,
    })
}

pub(crate) async fn required_base_dependencies(s9pk: &S9pk) -> Result<CurrentDependencies, Error> {
    let mut deps = BTreeMap::new();
    for (id, info) in &s9pk.as_manifest().dependencies.0 {
        if let Some((kind, version_range)) = required_base(info) {
            deps.insert(
                id.clone(),
                dependency_info(s9pk, id, kind, version_range).await?,
            );
        }
    }
    Ok(CurrentDependencies(deps))
}

pub async fn set_dependencies(
    context: EffectContext,
    SetDependenciesParams { dependencies }: SetDependenciesParams,
) -> Result<(), Error> {
    let context = context.deref()?;
    let id = &context.seed.id;
    let s9pk = &context.seed.persistent_container.s9pk;
    let mut deps = required_base_dependencies(s9pk).await?.0;
    for dependency in dependencies {
        let (dep_id, kind, version_range) = match dependency {
            DependencyRequirement::Exists { id, version_range } => {
                (id, CurrentDependencyKind::Exists, version_range)
            }
            DependencyRequirement::Running {
                id,
                health_checks,
                version_range,
            } => (
                id,
                CurrentDependencyKind::Running { health_checks },
                version_range,
            ),
        };
        let (kind, version_range) =
            if let Some(base) = s9pk.as_manifest().dependencies.0.get(&dep_id) {
                narrow_to_base(base, kind, version_range)
            } else {
                (kind, version_range)
            };
        if let Some(info) = deps.get_mut(&dep_id) {
            info.kind = kind;
            info.version_range = version_range;
        } else {
            deps.insert(
                dep_id.clone(),
                dependency_info(s9pk, &dep_id, kind, version_range).await?,
            );
        }
    }
    context
        .seed
        .ctx
        .db
        .mutate(|db| {
            let pde = db
                .as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(id)
                .or_not_found(id)?;
            let deps = CurrentDependencies(deps);
            pde.as_current_dependencies_mut().ser(&deps)?;
            Ok(())
        })
        .await
        .result
}

pub async fn get_dependencies(context: EffectContext) -> Result<Vec<DependencyRequirement>, Error> {
    let context = context.deref()?;
    let id = &context.seed.id;
    let db = context.seed.ctx.db.peek().await;
    let data = db
        .as_public()
        .as_package_data()
        .as_idx(id)
        .or_not_found(id)?
        .as_current_dependencies()
        .de()?;

    Ok(data
        .0
        .into_iter()
        .map(|(id, current_dependency_info)| {
            let CurrentDependencyInfo {
                version_range,
                kind,
                ..
            } = current_dependency_info;
            match kind {
                CurrentDependencyKind::Exists => {
                    DependencyRequirement::Exists { id, version_range }
                }
                CurrentDependencyKind::Running { health_checks } => {
                    DependencyRequirement::Running {
                        id,
                        health_checks,
                        version_range,
                    }
                }
            }
        })
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser, visit_rs::VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct CheckDependenciesParam {
    package_ids: Option<Vec<PackageId>>,
}

rpc_toolkit::reflect_ts!(CheckDependenciesParam);
rpc_toolkit::ts_export!(CheckDependenciesParam, namespaces = [""]);
#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct CheckDependenciesResult {
    package_id: PackageId,
    title: Option<String>,
    installed_version: Option<VersionString>,
    satisfies: BTreeSet<VersionString>,
    is_running: bool,
    tasks: BTreeMap<ReplayId, TaskEntry>,
    health_checks: BTreeMap<HealthCheckId, NamedHealthCheckResult>,
}

rpc_toolkit::reflect_ts!(CheckDependenciesResult);
rpc_toolkit::ts_export!(CheckDependenciesResult, namespaces = [""]);
pub async fn check_dependencies(
    context: EffectContext,
    CheckDependenciesParam { package_ids }: CheckDependenciesParam,
) -> Result<Vec<CheckDependenciesResult>, Error> {
    let context = context.deref()?;
    let db = context.seed.ctx.db.peek().await;
    let manifest = context.seed.persistent_container.s9pk.as_manifest();
    dependency_statuses(
        &db,
        &context.seed.id,
        manifest.metadata.default_locale.as_deref(),
        package_ids,
    )
}

/// Titles use the declaring package's default locale.
fn dependency_statuses(
    db: &DatabaseModel,
    package_id: &PackageId,
    default_locale: Option<&str>,
    package_ids: Option<Vec<PackageId>>,
) -> Result<Vec<CheckDependenciesResult>, Error> {
    let pde = db
        .as_public()
        .as_package_data()
        .as_idx(package_id)
        .or_not_found(package_id)?;
    let current_dependencies = pde.as_current_dependencies().de()?;
    let tasks = pde.as_tasks().de()?;
    let package_dependency_info: Vec<_> = package_ids
        .unwrap_or_else(|| current_dependencies.0.keys().cloned().collect())
        .into_iter()
        .filter_map(|x| {
            let info = current_dependencies.0.get(&x)?;
            Some((x, info))
        })
        .collect();
    let mut results = Vec::with_capacity(package_dependency_info.len());

    for (package_id, dependency_info) in package_dependency_info {
        let title = dependency_info
            .title
            .clone()
            .map(|title| title.localized(default_locale));
        let tasks = tasks
            .iter()
            .filter(|(_, v)| v.task.package_id == package_id)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let Some(package) = db.as_public().as_package_data().as_idx(&package_id) else {
            results.push(CheckDependenciesResult {
                package_id,
                title,
                installed_version: None,
                satisfies: BTreeSet::new(),
                is_running: false,
                tasks,
                health_checks: Default::default(),
            });
            continue;
        };
        let manifest = package.as_state_info().as_manifest(ManifestPreference::New);
        let installed_version = manifest.as_version().de()?.into_version();
        let satisfies = manifest.as_metadata().as_satisfies().de()?;
        let installed_version = Some(installed_version.clone().into());
        let is_running = package
            .as_status_info()
            .as_started()
            .transpose_ref()
            .is_some();
        let health_checks = package.as_status_info().as_health().de()?;
        results.push(CheckDependenciesResult {
            package_id,
            title,
            installed_version,
            satisfies,
            is_running,
            tasks,
            health_checks,
        });
    }
    Ok(results)
}

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct GetServiceManifestParams {
    pub package_id: PackageId,

    #[arg(skip)]
    pub callback: Option<CallbackId>,
}

rpc_toolkit::reflect_ts!(GetServiceManifestParams);
rpc_toolkit::ts_export!(GetServiceManifestParams, namespaces = [""]);

pub async fn get_service_manifest(
    context: EffectContext,
    GetServiceManifestParams {
        package_id,
        callback,
    }: GetServiceManifestParams,
) -> Result<Option<Manifest>, Error> {
    use crate::db::model::package::PackageState;

    let context = context.deref()?;

    let ptr = format!("/public/packageData/{}/stateInfo", package_id)
        .parse()
        .expect("valid json pointer");
    let mut watch = context.seed.ctx.db.watch(ptr).await.typed::<PackageState>();

    let state = watch.peek_and_mark_seen()?;
    let manifest = if state.as_value().is_null() {
        None
    } else {
        Some(state.as_manifest(ManifestPreference::Old).de()?)
    };

    if let Some(callback) = callback {
        let callback = callback.register(&context.seed.persistent_container);
        context.seed.ctx.callbacks.add_get_service_manifest(
            package_id.clone(),
            watch,
            CallbackHandler::new(&context, callback),
        );
    }

    Ok(manifest)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn dependency_titles_use_the_declaring_package_default_with_or_without_the_dependency() {
        let db = Model::from_value(imbl_value::json!({
            "public": { "packageData": {
                "caller": {
                    "currentDependencies": {
                        "installed": {
                            "title": { "de_DE": "Deutsch", "pl_PL": "Polski" },
                            "kind": "exists", "versionRange": "*",
                        },
                        "missing": {
                            "title": { "de_DE": "Deutsch", "pl_PL": "Polski" },
                            "kind": "exists", "versionRange": "*",
                        },
                    },
                    "tasks": {},
                },
                "installed": {
                    "stateInfo": {
                        "state": "installed",
                        "manifest": {
                            "version": "1.0.0:0", "satisfies": [], "defaultLocale": "pl_PL",
                        },
                    },
                    "statusInfo": { "started": null, "health": {} },
                },
            }},
        }));
        let caller = "caller".parse().unwrap();
        let results = dependency_statuses(&db, &caller, Some("de_DE"), None).unwrap();
        assert_eq!(results.len(), 2);
        for result in results {
            assert_eq!(result.title.as_deref(), Some("Deutsch"));
            assert_eq!(
                result.installed_version.is_some(),
                &*result.package_id == "installed"
            );
        }
    }

    #[test]
    fn required_manifest_requirement_is_the_static_fallback() {
        let base: crate::dependencies::DepInfo = serde_json::from_str(
            r#"{"description":null,"optional":false,"versionRange":">=31.1:17","kind":"running","healthChecks":["bitcoind"],"metadata":{"title":"Bitcoin","icon":"https://example.com/icon.png"}}"#,
        )
        .unwrap();
        let (kind, range) = required_base(&base).unwrap();
        assert_eq!(range.to_string(), ">=31.1:17");
        assert!(
            matches!(kind, CurrentDependencyKind::Running { health_checks } if health_checks.contains(&"bitcoind".parse().unwrap()))
        );

        let mut optional = base.clone();
        optional.optional = true;
        assert!(required_base(&optional).is_none());
        let mut legacy = base;
        legacy.kind = None;
        assert!(required_base(&legacy).is_none());
    }

    #[test]
    fn runtime_cannot_broaden_an_enabled_optional_dependency() {
        let base: DepInfo = serde_json::from_str(
            r#"{"description":null,"optional":true,"versionRange":">=31.1:17","kind":"running","healthChecks":["ready"],"metadata":{"title":"Bitcoin","icon":"https://example.com/icon.png"}}"#,
        )
        .unwrap();
        assert!(required_base(&base).is_none());
        let (kind, range) =
            narrow_to_base(&base, CurrentDependencyKind::Exists, VersionRange::any());
        assert_eq!(range.to_string(), ">=31.1:17");
        assert!(
            matches!(kind, CurrentDependencyKind::Running { health_checks } if health_checks.contains(&"ready".parse().unwrap()))
        );
    }

    #[test]
    fn runtime_running_health_checks_add_to_the_manifest_base() {
        let base: DepInfo = serde_json::from_str(
            r#"{"description":null,"optional":false,"versionRange":">=31.1:17","kind":"running","healthChecks":["ready"],"metadata":{"title":"Bitcoin","icon":"https://example.com/icon.png"}}"#,
        )
        .unwrap();
        let (kind, range) = narrow_to_base(
            &base,
            CurrentDependencyKind::Running {
                health_checks: ["rpc".parse().unwrap()].into(),
            },
            ">=32:0".parse().unwrap(),
        );
        assert_eq!(range.to_string(), ">=32:0");
        assert!(
            matches!(kind, CurrentDependencyKind::Running { health_checks } if health_checks == ["ready".parse().unwrap(), "rpc".parse().unwrap()].into())
        );

        let already_narrow: VersionRange = ">=31.1:17 && >=32:0".parse().unwrap();
        let expected = already_narrow.to_string();
        let (_, preserved) = narrow_to_base(&base, CurrentDependencyKind::Exists, already_narrow);
        assert_eq!(preserved.to_string(), expected);
    }

    #[test]
    fn relative_components_drops_roots_and_dots() {
        assert_eq!(
            relative_components(Path::new("/a/b/./c")),
            PathBuf::from("a/b/c")
        );
        assert_eq!(relative_components(Path::new("")), PathBuf::new());
        assert_eq!(
            relative_components(Path::new("a/../b")),
            PathBuf::from("a/../b")
        );
    }

    #[tokio::test]
    async fn confined_join_keeps_paths_under_the_base() {
        let tmp = PathBuf::from(format!("/tmp/confined-join-test-{}", std::process::id()));
        let base = tmp.join("base");
        tokio::fs::create_dir_all(base.join("inner")).await.unwrap();
        tokio::fs::create_dir_all(tmp.join("outside"))
            .await
            .unwrap();
        #[cfg(unix)]
        tokio::fs::symlink(tmp.join("outside"), base.join("link"))
            .await
            .unwrap();
        let canonical_base = tokio::fs::canonicalize(&base).await.unwrap();

        assert_eq!(
            confined_join(&base, Path::new("inner")).await.unwrap(),
            canonical_base.join("inner")
        );
        // a tail that does not exist yet is fine, as long as it is plain names
        assert_eq!(
            confined_join(&base, Path::new("inner/new/dir"))
                .await
                .unwrap(),
            canonical_base.join("inner/new/dir")
        );
        assert_eq!(
            confined_join(&base, Path::new("")).await.unwrap(),
            canonical_base
        );
        // a `..` that resolves back inside is fine
        assert_eq!(
            confined_join(&base, Path::new("inner/../inner"))
                .await
                .unwrap(),
            canonical_base.join("inner")
        );
        assert!(confined_join(&base, Path::new("../outside")).await.is_err());
        assert!(
            confined_join(&base, Path::new("missing/../../outside"))
                .await
                .is_err()
        );
        #[cfg(unix)]
        {
            assert!(confined_join(&base, Path::new("link")).await.is_err());
            assert!(confined_join(&base, Path::new("link/sub")).await.is_err());
        }

        tokio::fs::remove_dir_all(&tmp).await.ok();
    }

    fn idmap(from_id: u32, to_id: u32, range: u32) -> IdMap {
        IdMap {
            from_id,
            to_id,
            range,
        }
    }

    #[tokio::test]
    #[ignore = "requires root + mount privileges"]
    async fn idmap_host_side_remaps_uids() {
        use std::os::unix::fs::MetadataExt;

        tokio::fs::create_dir_all(crate::disk::mount::guard::TMP_MOUNTPOINT)
            .await
            .unwrap();

        // stand-in for the startbox `unshare-userns` applet (see which_self_exe):
        // unshare a userns, print `ready`, block on stdin while the parent maps it.
        let helper = PathBuf::from(format!("/tmp/idmap-it-helper-{}", std::process::id()));
        tokio::fs::write(
            &helper,
            "#!/bin/sh\nexec unshare -U sh -c 'echo ready; exec cat'\n",
        )
        .await
        .unwrap();
        std::fs::set_permissions(&helper, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        unsafe { std::env::set_var("STARTOS_TEST_USERNS_HELPER", &helper) };

        let base = PathBuf::from(format!("/tmp/idmap-it-{}", std::process::id()));
        let src = base.join("src");
        let target = base.join("target");
        tokio::fs::create_dir_all(&src).await.unwrap();
        tokio::fs::create_dir_all(&target).await.unwrap();
        for (name, uid) in [("by0", 0u32), ("by5", 5), ("by1000", 1000)] {
            tokio::fs::write(src.join(name), b"x").await.unwrap();
            std::os::unix::fs::chown(src.join(name), Some(uid), Some(uid)).unwrap();
        }

        service_filesystem(
            Bind::new(&src)
                .with_type(FileType::Directory)
                .recursive(true),
            &[idmap(0, 1000, 1)],
        )
        .mount(&target, MountType::ReadWrite)
        .await
        .unwrap();

        let u0 = std::fs::metadata(target.join("by0")).unwrap().uid();
        let u5 = std::fs::metadata(target.join("by5")).unwrap().uid();
        let u1000 = std::fs::metadata(target.join("by1000")).unwrap().uid();
        unmount(&target, true).await.ok();
        tokio::fs::remove_dir_all(&base).await.ok();
        tokio::fs::remove_file(&helper).await.ok();
        unsafe { std::env::remove_var("STARTOS_TEST_USERNS_HELPER") };

        // override: on-disk 0 -> 0:1000 lifted by the base offset -> host 101000
        assert_eq!(
            u0, 101000,
            "overridden on-disk 0 should surface as host 101000"
        );
        // base preserved: on-disk 5 keeps its base mapping -> host 100005
        assert_eq!(
            u5, 100005,
            "non-overridden on-disk 5 should keep base -> 100005"
        );
        // collision: base target of on-disk 1000 == override target, so it drops
        assert_eq!(u1000, 65534, "colliding on-disk 1000 should drop to nobody");
    }
}
