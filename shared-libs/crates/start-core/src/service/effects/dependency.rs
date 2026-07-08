use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::str::FromStr;

use clap::builder::ValueParserFactory;
use exver::VersionRange;
use rust_i18n::t;

use crate::db::model::package::{
    CurrentDependencies, CurrentDependencyInfo, CurrentDependencyKind, ManifestPreference,
    TaskEntry,
};
use crate::disk::mount::filesystem::bind::{Bind, FileType};
use crate::disk::mount::filesystem::idmapped::{IdMap, IdMapped};
use crate::disk::mount::filesystem::{FileSystem, MountType};
use crate::disk::mount::util::{is_mountpoint, unmount};
use crate::s9pk::manifest::Manifest;
use crate::service::effects::callbacks::CallbackHandler;
use crate::service::effects::prelude::*;
use crate::service::rpc::CallbackId;
use crate::status::health_check::NamedHealthCheckResult;
use crate::util::{FromStrParser, VersionString};
use crate::volume::data_dir;
use crate::{DATA_DIR, HealthCheckId, PackageId, ReplayId, VolumeId};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MountTarget {
    package_id: PackageId,
    volume_id: VolumeId,
    subpath: Option<PathBuf>,
    readonly: bool,
    #[serde(default)]
    idmap: Vec<IdMap>,
    #[serde(skip_deserializing)]
    #[ts(skip)]
    filetype: FileType,
}

/// The LXC base idmap every subcontainer mount carries: container ids 0..65535
/// map to host ids 100000..165535.
const BASE_IDMAP: IdMap = IdMap {
    from_id: 0,
    to_id: 100000,
    range: 65536,
};

/// The idmap to apply, host-side, for a pointer mount.
///
/// The SDK idmap is layered *on top of* the LXC base map (the SDK docs: "the
/// base LXC mapping is applied automatically"), and a list of idmaps composes
/// in sequence — each entry remaps the output of the entries before it. We
/// can't stack the layers as separate mounts (re-idmapping an already-idmapped
/// mount *replaces* the mapping, it doesn't compose — verified on a 6.15+
/// kernel), so the whole stack is collapsed into one equivalent idmap here.
///
/// Computed pointwise: for each on-disk id, start from its base mapping
/// (`k -> base + k`) and fold the idmaps in order (each is identity outside its
/// `from_id` range). The composition can be non-injective — overriding
/// `0 -> 1000` leaves both on-disk 0 and on-disk 1000 wanting target 1000 — and
/// a uid_map can't map two on-disk ids to one target, so colliding ids are
/// dropped to `nobody`, preferring an explicitly-remapped id over a passthrough.
/// See the proptest in the test module for the exact equivalence to composition.
fn pointer_idmap(custom: &[IdMap]) -> Vec<IdMap> {
    let base = BASE_IDMAP;
    if custom.is_empty() {
        return vec![base];
    }
    let len = base.range as usize;

    // Per on-disk id: its composed host target, and whether an idmap touched it.
    let mut host = vec![0u32; len];
    let mut explicit = vec![false; len];
    for d in 0..base.range {
        // base maps on-disk d -> container d (base.from_id is 0)
        let mut c = d;
        let mut touched = false;
        for m in custom {
            if c >= m.from_id && c - m.from_id < m.range {
                c = m.to_id.saturating_add(c - m.from_id);
                touched = true;
            }
        }
        host[d as usize] = base.to_id.saturating_add(c);
        explicit[d as usize] = touched;
    }

    // A uid_map target must be unique. On a collision, an explicit remap wins
    // over a passthrough; otherwise the lowest on-disk id wins.
    let mut owner: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for d in 0..base.range {
        match owner.get(&host[d as usize]) {
            None => {
                owner.insert(host[d as usize], d);
            }
            Some(&prev) if explicit[d as usize] && !explicit[prev as usize] => {
                owner.insert(host[d as usize], d);
            }
            Some(_) => {}
        }
    }
    let mut keep = vec![None; len];
    for (h, d) in owner {
        keep[d as usize] = Some(h);
    }

    // Coalesce contiguous survivors into extents.
    let mut out = Vec::new();
    let mut d = 0u32;
    while (d as usize) < len {
        if let Some(h0) = keep[d as usize] {
            let mut n = 1u32;
            while ((d + n) as usize) < len && keep[(d + n) as usize] == Some(h0.saturating_add(n)) {
                n += 1;
            }
            out.push(IdMap {
                from_id: d,
                to_id: h0,
                range: n,
            });
            d += n;
        } else {
            d += 1;
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MountParams {
    location: PathBuf,
    target: MountTarget,
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
    let subpath = subpath.unwrap_or_default();
    let subpath = subpath.strip_prefix("/").unwrap_or(&subpath);
    let source = data_dir(DATA_DIR, &package_id, &volume_id).join(subpath);
    let location = location.strip_prefix("/").unwrap_or(&location);
    let mountpoint = context
        .seed
        .persistent_container
        .lxc_container
        .get()
        .or_not_found("lxc container")?
        .rootfs_dir()
        .join(location);

    if is_mountpoint(&mountpoint).await? {
        unmount(&mountpoint, true).await?;
    }

    IdMapped::new(
        Bind::new(source).with_type(filetype).recursive(true),
        pointer_idmap(&idmap),
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

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[serde(rename_all_fields = "camelCase")]
#[ts(export)]
pub enum DependencyRequirement {
    Running {
        id: PackageId,
        health_checks: BTreeSet<HealthCheckId>,
        #[ts(type = "string")]
        version_range: VersionRange,
    },
    Exists {
        id: PackageId,
        #[ts(type = "string")]
        version_range: VersionRange,
    },
}
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
#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "camelCase")]
#[ts(export)]
pub struct SetDependenciesParams {
    dependencies: Vec<DependencyRequirement>,
}
pub async fn set_dependencies(
    context: EffectContext,
    SetDependenciesParams { dependencies }: SetDependenciesParams,
) -> Result<(), Error> {
    let context = context.deref()?;
    let id = &context.seed.id;

    let mut deps = BTreeMap::new();
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
        let info = CurrentDependencyInfo {
            title: context
                .seed
                .persistent_container
                .s9pk
                .dependency_metadata(&dep_id)
                .await?
                .map(|m| m.title),
            icon: context
                .seed
                .persistent_container
                .s9pk
                .dependency_icon_data_url(&dep_id)
                .await?,
            kind,
            version_range,
        };
        deps.insert(dep_id, info);
    }
    context
        .seed
        .ctx
        .db
        .mutate(|db| {
            db.as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(id)
                .or_not_found(id)?
                .as_current_dependencies_mut()
                .ser(&CurrentDependencies(deps))
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

#[derive(Debug, Clone, Serialize, Deserialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheckDependenciesParam {
    #[ts(optional)]
    package_ids: Option<Vec<PackageId>>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheckDependenciesResult {
    package_id: PackageId,
    title: Option<String>,
    installed_version: Option<VersionString>,
    satisfies: BTreeSet<VersionString>,
    is_running: bool,
    tasks: BTreeMap<ReplayId, TaskEntry>,
    health_checks: BTreeMap<HealthCheckId, NamedHealthCheckResult>,
}
pub async fn check_dependencies(
    context: EffectContext,
    CheckDependenciesParam { package_ids }: CheckDependenciesParam,
) -> Result<Vec<CheckDependenciesResult>, Error> {
    let context = context.deref()?;
    let db = context.seed.ctx.db.peek().await;
    let pde = db
        .as_public()
        .as_package_data()
        .as_idx(&context.seed.id)
        .or_not_found(&context.seed.id)?;
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
        let title = dependency_info.title.clone();
        let Some(package) = db.as_public().as_package_data().as_idx(&package_id) else {
            let tasks = tasks
                .iter()
                .filter(|(_, v)| v.task.package_id == package_id)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            results.push(CheckDependenciesResult {
                package_id,
                title: title.map(|t| t.localized()),
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
        let tasks = tasks
            .iter()
            .filter(|(_, v)| v.task.package_id == package_id)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        results.push(CheckDependenciesResult {
            package_id,
            title: title.map(|t| t.localized()),
            installed_version,
            satisfies,
            is_running,
            tasks,
            health_checks,
        });
    }
    Ok(results)
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GetServiceManifestParams {
    pub package_id: PackageId,
    #[ts(optional)]
    #[arg(skip)]
    pub callback: Option<CallbackId>,
}

pub async fn get_service_manifest(
    context: EffectContext,
    GetServiceManifestParams {
        package_id,
        callback,
    }: GetServiceManifestParams,
) -> Result<Manifest, Error> {
    use crate::db::model::package::PackageState;

    let context = context.deref()?;

    let ptr = format!("/public/packageData/{}/stateInfo", package_id)
        .parse()
        .expect("valid json pointer");
    let mut watch = context.seed.ctx.db.watch(ptr).await.typed::<PackageState>();

    let manifest = watch
        .peek_and_mark_seen()?
        .as_manifest(ManifestPreference::Old)
        .de()?;

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

    fn idmap(from_id: u32, to_id: u32, range: u32) -> IdMap {
        IdMap {
            from_id,
            to_id,
            range,
        }
    }

    #[test]
    fn empty_custom_is_just_the_base_map() {
        assert_eq!(pointer_idmap(&[]), vec![BASE_IDMAP]);
    }

    #[test]
    fn override_is_layered_on_top_of_the_base_map() {
        // mostro: on-disk 0 -> container 1000 (host 101000); every other id keeps
        // its base mapping (k -> 100000 + k), except on-disk 1000, whose base
        // target collides with the override and so is dropped (-> nobody).
        assert_eq!(
            pointer_idmap(&[idmap(0, 1000, 1)]),
            vec![
                idmap(0, 101000, 1),        // override: on-disk 0 -> container 1000
                idmap(1, 100001, 999),      // base: on-disk 1..999 -> container 1..999
                idmap(1001, 101001, 64535), // base: on-disk 1001..65535 (1000 dropped)
            ],
        );
    }

    #[test]
    fn chaining_idmaps_compose_sequentially() {
        // idmap 1 maps the OUTPUT range of idmap 0 (0->1000, then 1000->2000),
        // so they compose: on-disk 0 -> 1000 -> 2000 (host 102000), not 101000.
        let custom = [idmap(0, 1000, 1), idmap(1000, 2000, 1)];
        let flat = pointer_idmap(&custom);
        assert_eq!(
            apply(&flat, 0),
            Some(102000),
            "0 composes through both layers"
        );
        // an untouched id keeps its base mapping
        assert_eq!(apply(&flat, 5), Some(100005));
        // on-disk 1000 ALSO composes to 102000 (passthrough idmap 0, then 1000->2000),
        // colliding with on-disk 0 — a single uid_map can't hold both, so it drops.
        assert_eq!(compose_ref(&custom, 1000), Some(102000));
        assert_eq!(apply(&flat, 1000), None);
    }

    // Reference: the intended (possibly non-injective) composition — base, then
    // each idmap applied in sequence to the running value (identity out of range).
    fn compose_ref(custom: &[IdMap], on_disk: u32) -> Option<u32> {
        let base = BASE_IDMAP;
        if on_disk >= base.range {
            return None;
        }
        let mut c = on_disk;
        for m in custom {
            if c >= m.from_id && c - m.from_id < m.range {
                c = m.to_id.saturating_add(c - m.from_id);
            }
        }
        Some(base.to_id.saturating_add(c))
    }

    // Apply a flat uid_map to an on-disk id (None == nobody / unmapped).
    fn apply(map: &[IdMap], on_disk: u32) -> Option<u32> {
        map.iter().find_map(|m| {
            (on_disk >= m.from_id && on_disk - m.from_id < m.range)
                .then(|| m.to_id.saturating_add(on_disk - m.from_id))
        })
    }

    // A valid uid_map: non-overlapping source ranges AND non-overlapping target
    // ranges (an idmapped mount is a bijection between the two).
    fn is_valid_uid_map(map: &[IdMap]) -> bool {
        let overlaps = |mut v: Vec<(u32, u32)>| {
            v.sort();
            v.windows(2).any(|w| w[0].1 > w[1].0)
        };
        let froms: Vec<(u32, u32)> = map
            .iter()
            .map(|m| (m.from_id, m.from_id + m.range))
            .collect();
        let tos: Vec<(u32, u32)> = map.iter().map(|m| (m.to_id, m.to_id + m.range)).collect();
        !overlaps(froms) && !overlaps(tos)
    }

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config { cases: 48, ..proptest::test_runner::Config::default() })]

        /// `pointer_idmap` is exactly the injective restriction of the layered
        /// composition: it is a valid uid_map, never maps an on-disk id to
        /// anything other than what composition would, and preserves every
        /// target composition reaches uniquely — only genuine collisions (which
        /// a single uid_map cannot represent) drop to nobody.
        #[test]
        fn flatten_is_the_injective_restriction_of_composition(
            custom in proptest::collection::vec((0u32..60_000, 0u32..60_000, 1u32..2_000), 0..4usize),
        ) {
            let custom: Vec<IdMap> = custom.into_iter().map(|(f, t, r)| idmap(f, t, r)).collect();
            let flat = pointer_idmap(&custom);

            proptest::prop_assert!(is_valid_uid_map(&flat), "invalid uid_map: {flat:?}");

            let comp: Vec<Option<u32>> =
                (0..BASE_IDMAP.range).map(|d| compose_ref(&custom, d)).collect();
            let mut hits: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
            for c in comp.iter().flatten() {
                *hits.entry(*c).or_default() += 1;
            }

            for d in 0..BASE_IDMAP.range {
                let f = apply(&flat, d);
                let c = comp[d as usize];
                // never wrong: flatten agrees with composition or drops to nobody
                proptest::prop_assert!(f.is_none() || f == c, "d={d}: flat={f:?} compose={c:?}");
                // lossless where representable: a uniquely-reached target is kept
                if let Some(h) = c {
                    if hits[&h] == 1 {
                        proptest::prop_assert_eq!(f, Some(h), "d={} dropped a unique target", d);
                    }
                }
            }
        }
    }

    /// End-to-end check of the host-side mechanism the `mount-pointer` RPC
    /// effect runs: build the composed idmap and apply it with `IdMapped`
    /// (startd, init_user_ns), then read uids back through the mount. Requires
    /// root + a 6.15+ kernel, so it is ignored by default. Run with:
    ///   sudo <test-bin> --ignored --exact \
    ///     service::effects::dependency::test::idmap_host_side_remaps_uids --nocapture
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

        // exactly what mount() runs for a pointer mount with idmap [{0->1000}]
        IdMapped::new(
            Bind::new(&src)
                .with_type(FileType::Directory)
                .recursive(true),
            pointer_idmap(&[idmap(0, 1000, 1)]),
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
