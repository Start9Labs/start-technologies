use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use clap::{Parser, ValueEnum};
use exver::{ExtendedVersion, VersionRange};
use imbl_value::{InternedString, json};
use itertools::Itertools;
use patch_db::ModelExt;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tracing::warn;

use crate::PackageId;
use crate::context::CliContext;
use crate::prelude::*;
use crate::progress::{FullProgressTracker, ProgressUnits};
use crate::registry::context::RegistryContext;
use crate::registry::device_info::DeviceInfo;
use crate::registry::package::index::{PackageIndex, PackageVersionInfo, ReaderParams};
use crate::s9pk::manifest::{HardwareRequirements, LocaleString};
use crate::s9pk::merkle_archive::source::ArchiveSource;
use crate::s9pk::v2::SIG_CONTEXT;
use crate::util::VersionString;
use crate::util::io::{TrackingIO, create_file, delete_file, rename, to_tmp_path};
use crate::util::serde::{WithIoFormat, display_serializable};
use crate::util::tui::{choose, choose_custom_display};

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Deserialize,
    Serialize,
    VisitVariants,
    ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum PackageDetailLevel {
    None,
    Short,
    Full,
}

rpc_toolkit::reflect_ts!(PackageDetailLevel);
rpc_toolkit::ts_export!(PackageDetailLevel, namespaces = [""]);
impl Default for PackageDetailLevel {
    fn default() -> Self {
        Self::Short
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct PackageInfoShort {
    pub release_notes: LocaleString,
    /// `PackageMetadata::default_locale`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_locale: Option<InternedString>,
    /// A newer same-flavor revision covers this release's hardware.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub superseded: bool,
}

rpc_toolkit::reflect_ts!(PackageInfoShort);
rpc_toolkit::ts_export!(PackageInfoShort, namespaces = [""]);

#[derive(Debug, Deserialize, Serialize, VisitFields, Parser, HasModel)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
#[model = "Model<Self>"]
pub struct GetPackageParams {
    #[arg(help = "help.arg.package-id")]
    pub id: Option<PackageId>,
    #[arg(long, short = 'v', help = "help.arg.target-version-range")]
    pub target_version: Option<VersionRange>,
    #[arg(long, help = "help.arg.source-version")]
    pub source_version: Option<VersionString>,
    #[visit(ts(skip))]
    #[arg(skip)]
    #[serde(rename = "__DeviceInfo_device_info")]
    pub device_info: Option<DeviceInfo>,
    #[arg(default_value = "none", help = "help.arg.other-versions-detail")]
    pub other_versions: Option<PackageDetailLevel>,
    #[arg(long, help = "help.arg.all-revisions")]
    #[serde(default)]
    pub all_revisions: bool,
    #[command(flatten)]
    #[serde(flatten)]
    pub reader: ReaderParams,
}

rpc_toolkit::reflect_ts!(GetPackageParams);
rpc_toolkit::ts_export!(GetPackageParams, namespaces = [""]);

#[derive(Debug, Deserialize, Serialize, VisitFields, HasModel)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct GetPackageResponse {
    pub categories: BTreeSet<InternedString>,
    /// When this registry first published any version it still carries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_published_at: Option<DateTime<Utc>>,
    pub best: BTreeMap<VersionString, PackageVersionInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_versions: Option<BTreeMap<VersionString, PackageInfoShort>>,
}

rpc_toolkit::reflect_ts!(GetPackageResponse);
rpc_toolkit::ts_export!(GetPackageResponse, namespaces = [""]);
impl GetPackageResponse {
    pub fn tables(self) -> Vec<prettytable::Table> {
        use prettytable::*;

        let mut res = Vec::with_capacity(self.best.len());

        for (version, info) in self.best {
            let mut table = info.table(&version);

            let lesser_versions: BTreeMap<_, _> = self
                .other_versions
                .clone()
                .into_iter()
                .flatten()
                .filter(|(v, _)| **v < *version)
                .collect();

            if !lesser_versions.is_empty() {
                table.add_row(row![bc => "OLDER VERSIONS"]);
                table.add_row(row![bc => "VERSION", "RELEASE NOTES"]);
                for (version, info) in lesser_versions {
                    table.add_row(row![
                        AsRef::<str>::as_ref(&version),
                        &info.release_notes.localized(info.default_locale.as_deref())
                    ]);
                }
            }

            res.push(table);
        }

        res
    }
}

#[derive(Debug, Deserialize, Serialize, VisitFields, HasModel)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct GetPackageResponseFull {
    pub categories: BTreeSet<InternedString>,
    /// When this registry first published any version it still carries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_published_at: Option<DateTime<Utc>>,
    pub best: BTreeMap<VersionString, PackageVersionInfo>,
    pub other_versions: BTreeMap<VersionString, PackageVersionInfo>,
}

rpc_toolkit::reflect_ts!(GetPackageResponseFull);
rpc_toolkit::ts_export!(GetPackageResponseFull, namespaces = [""]);
impl GetPackageResponseFull {
    pub fn tables(self) -> Vec<prettytable::Table> {
        let mut res = Vec::with_capacity(self.best.len());

        let all: BTreeMap<_, _> = self
            .best
            .into_iter()
            .chain(self.other_versions.into_iter())
            .collect();

        for (version, info) in all {
            res.push(info.table(&version));
        }

        res
    }
}

pub type GetPackagesResponse = BTreeMap<PackageId, GetPackageResponse>;
pub type GetPackagesResponseFull = BTreeMap<PackageId, GetPackageResponseFull>;

#[derive(Debug, Deserialize, Serialize, VisitVariants)]
#[serde(untagged)]
pub enum GetPackageResult {
    PackageFull(GetPackageResponseFull),
    Package(GetPackageResponse),
    PackagesFull(GetPackagesResponseFull),
    Packages(GetPackagesResponse),
}

rpc_toolkit::reflect_ts!(GetPackageResult);

fn get_matching_models(
    db: &Model<PackageIndex>,
    GetPackageParams {
        id,
        source_version,
        device_info,
        target_version,
        ..
    }: &GetPackageParams,
) -> Result<Vec<(PackageId, ExtendedVersion, Model<PackageVersionInfo>)>, Error> {
    if let Some(id) = id {
        if let Some(pkg) = db.as_packages().as_idx(id) {
            vec![(id.clone(), pkg)]
        } else {
            vec![]
        }
    } else {
        db.as_packages().as_entries()?
    }
    .iter()
    .map(|(k, v)| {
        Ok(v.as_versions()
            .as_entries()?
            .into_iter()
            .map(|(v, info)| {
                let ev = ExtendedVersion::from(v);
                Ok::<_, Error>(
                    if target_version.as_ref().map_or(true, |tv| ev.satisfies(tv))
                        && source_version.as_ref().map_or(Ok(true), |source_version| {
                            Ok::<_, Error>(
                                source_version.satisfies(
                                    &info
                                        .as_source_version()
                                        .de()?
                                        .unwrap_or(VersionRange::any()),
                                ),
                            )
                        })?
                    {
                        let mut info = info.clone();
                        if let Some(device_info) = &device_info {
                            if info.for_device(device_info)? {
                                Some((k.clone(), ev, info))
                            } else {
                                None
                            }
                        } else {
                            Some((k.clone(), ev, info))
                        }
                    } else {
                        None
                    },
                )
            })
            .flatten_ok())
    })
    .flatten_ok()
    .map(|res| res.and_then(|a| a))
    .collect()
}

fn covers(newer: &HardwareRequirements, older: &HardwareRequirements) -> bool {
    newer.arch.as_ref().map_or(true, |n| {
        older.arch.as_ref().is_some_and(|o| o.is_subset(n))
    }) && newer
        .ram
        .map_or(true, |n| older.ram.is_some_and(|o| n <= o))
        && newer.device.iter().all(|d| older.device.contains(d))
}

fn hide_superseded_revisions(
    best: Option<&BTreeMap<VersionString, Model<PackageVersionInfo>>>,
    other: &mut BTreeMap<VersionString, Model<PackageVersionInfo>>,
) -> Result<(), Error> {
    let hidden = superseded_revisions(best, other)?;
    other.retain(|v, _| !hidden.contains(v));
    Ok(())
}

/// Older revisions whose upstream version and flavor a newer revision covers on all their hardware.
fn superseded_revisions(
    best: Option<&BTreeMap<VersionString, Model<PackageVersionInfo>>>,
    other: &BTreeMap<VersionString, Model<PackageVersionInfo>>,
) -> Result<BTreeSet<VersionString>, Error> {
    let group = |v: &VersionString| (v.flavor().map(str::to_owned), v.upstream().clone());
    let hardware = |info: &Model<PackageVersionInfo>| {
        from_value::<Vec<(HardwareRequirements, Value)>>(info.as_s9pks().clone().into())
            .map(|s9pks| s9pks.into_iter().map(|(hw, _)| hw).collect_vec())
    };
    let mut newer: BTreeMap<_, Vec<Vec<HardwareRequirements>>> = BTreeMap::new();
    for (v, info) in best.into_iter().flatten() {
        newer.entry(group(v)).or_default().push(hardware(info)?);
    }
    let mut hidden = BTreeSet::new();
    for (v, info) in other.iter().rev() {
        let hw = hardware(info)?;
        let seen = newer.entry(group(v)).or_default();
        if seen
            .iter()
            .any(|n| hw.iter().all(|o| n.iter().any(|req| covers(req, o))))
        {
            hidden.insert(v.clone());
        }
        seen.push(hw);
    }
    Ok(hidden)
}

pub async fn get_package(ctx: RegistryContext, params: GetPackageParams) -> Result<Value, Error> {
    let peek = ctx.db.peek().await;
    let reader = params.reader.reader(&peek)?;
    let mut index = peek.as_index().as_package().clone();
    index.retain_visible(reader.as_ref())?;
    let result = select_packages(&index, &params)?;
    if let Some(id) = &params.id {
        if params.target_version.is_some() {
            let created_at = Utc::now().to_rfc3339();
            let pkg_id = id.to_string();
            let version = from_value::<GetPackageResponse>(result.clone())?
                .best
                .keys()
                .last()
                .map(ToString::to_string);
            tokio::task::spawn_blocking(move || {
                ctx.metrics_db.mutate(|conn| {
                    if let Err(e) = conn.execute(
                        "INSERT INTO package_request (created_at, pkg_id, version) VALUES (?1, ?2, ?3)",
                        params![created_at, pkg_id, version],
                    ) {
                        warn!("failed to record package request metric: {e}");
                    }
                });
            });
        }
    }
    Ok(result)
}

type Releases = BTreeMap<PackageId, BTreeMap<VersionString, Model<PackageVersionInfo>>>;

/// Splits releases into each package's best per flavor and the rest.
fn partition_best(
    matching: Vec<(PackageId, ExtendedVersion, Model<PackageVersionInfo>)>,
) -> (Releases, Releases) {
    let mut best: Releases = Default::default();
    let mut other: Releases = Default::default();
    for (id, version, info) in matching {
        let package_best = best.entry(id.clone()).or_default();
        let package_other = other.entry(id.clone()).or_default();
        if package_best.keys().all(|k| !(**k > version)) {
            for worse_version in package_best
                .keys()
                .filter(|k| ***k < version)
                .cloned()
                .collect_vec()
            {
                if let Some(info) = package_best.remove(&worse_version) {
                    package_other.insert(worse_version, info);
                }
            }
            package_best.insert(version.into(), info);
        } else {
            package_other.insert(version.into(), info);
        }
    }
    (best, other)
}

fn categories(
    index: &Model<PackageIndex>,
    id: &PackageId,
) -> Result<BTreeSet<InternedString>, Error> {
    index
        .as_packages()
        .as_idx(id)
        .map(|p| p.as_categories().de())
        .transpose()
        .map(Option::unwrap_or_default)
}

fn first_published_at(
    index: &Model<PackageIndex>,
    id: &PackageId,
) -> Result<Option<DateTime<Utc>>, Error> {
    let Some(package) = index.as_packages().as_idx(id) else {
        return Ok(None);
    };
    package
        .as_versions()
        .as_entries()?
        .into_iter()
        .map(|(_, info)| info.as_s9pks().de())
        .flatten_ok()
        .map_ok(|(_, asset)| asset.published_at)
        .process_results(|times| times.min())
}

/// One package as StartOS lists it in its registry cache.
#[derive(Debug, Deserialize, Serialize, VisitFields, HasModel)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct PackageListing {
    pub categories: BTreeSet<InternedString>,
    /// When this registry first published any version it still carries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_published_at: Option<DateTime<Utc>>,
    /// The newest release of each flavor this server can run, plus the update target.
    pub releases: BTreeMap<VersionString, PackageVersionInfo>,
    /// Every release this server can run.
    pub versions: BTreeMap<VersionString, ListedVersion>,
    /// Newest release of the installed flavor that the installed version can migrate to and this
    /// server can install.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_target: Option<VersionString>,
}

rpc_toolkit::reflect_ts!(PackageListing);

#[derive(Debug, Default, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct ListedVersion {
    /// A newer same-flavor revision covers this release's hardware.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub superseded: bool,
}

rpc_toolkit::reflect_ts!(ListedVersion);

/// The release as the index saves it, without s9pk access.
fn listed_release(info: &Model<PackageVersionInfo>) -> Value {
    let mut release = info.as_value().clone();
    if let Some(fields) = release.as_object_mut() {
        if let Some(s9pks) = fields.remove("s9pks") {
            fields.insert("s9pks".into(), without_access(s9pks));
        }
    }
    release
}

/// `PackageVersionInfo::strip_access`, leaving public assets' values untouched.
fn without_access(mut s9pks: Value) -> Value {
    const ACCESS: [&str; 2] = ["private", "readers"];
    let restricted: Vec<usize> = s9pks
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter(|(_, entry)| {
            entry
                .get(1)
                .is_some_and(|asset| ACCESS.iter().any(|key| asset.get(*key).is_some()))
        })
        .map(|(i, _)| i)
        .collect();
    for i in restricted {
        if let Some(asset) = s9pks
            .get_mut(i)
            .and_then(|entry| entry.get_mut(1))
            .and_then(Value::as_object_mut)
        {
            for key in ACCESS {
                asset.remove(key);
            }
        }
    }
    s9pks
}

/// A package's cache row, absent when the device can run none of its releases.
pub(crate) fn listing(
    index: &Model<PackageIndex>,
    id: &PackageId,
    device: &DeviceInfo,
    update_target: Option<&VersionString>,
) -> Result<Option<Model<PackageListing>>, Error> {
    let params = GetPackageParams {
        id: Some(id.clone()),
        target_version: None,
        source_version: None,
        device_info: Some(device.clone()),
        other_versions: None,
        all_revisions: true,
        reader: Default::default(),
    };
    let (mut best, mut other) = partition_best(get_matching_models(index, &params)?);
    let Some(mut releases) = best.remove(id) else {
        return Ok(None);
    };
    let other = other.remove(id).unwrap_or_default();
    let superseded = superseded_revisions(Some(&releases), &other)?;
    let versions: imbl_value::InOMap<_, _> = releases
        .keys()
        .chain(other.keys())
        .map(|version| {
            Ok::<_, Error>((
                InternedString::from_display(version),
                to_value(&ListedVersion {
                    superseded: superseded.contains(version),
                })?,
            ))
        })
        .try_collect()?;
    if let Some(target) = update_target {
        if let Some(info) = other.get(target) {
            releases.insert(target.clone(), info.clone());
        }
    }
    let package = index
        .as_packages()
        .as_idx(id)
        .or_not_found(lazy_format!("package {id}"))?;
    let mut row = imbl_value::InOMap::new();
    row.insert(
        "categories".into(),
        package.as_categories().as_value().clone(),
    );
    if let Some(published) = first_published_at(index, id)? {
        row.insert("firstPublishedAt".into(), to_value(&published)?);
    }
    row.insert(
        "releases".into(),
        releases
            .iter()
            .map(|(version, info)| (InternedString::from_display(version), listed_release(info)))
            .collect::<imbl_value::InOMap<_, _>>()
            .into(),
    );
    row.insert("versions".into(), versions.into());
    if let Some(target) = update_target {
        row.insert("updateTarget".into(), to_value(target)?);
    }
    Ok(Some(Model::from_value(row.into())))
}

/// Newest release of the installed flavor that the installed version can migrate to and the
/// device can install.
pub(crate) fn update_target(
    index: &Model<PackageIndex>,
    id: &PackageId,
    installed: &VersionString,
    device: &DeviceInfo,
) -> Result<Option<VersionString>, Error> {
    let params = GetPackageParams {
        id: Some(id.clone()),
        target_version: None,
        source_version: Some(installed.clone()),
        device_info: Some(device.clone()),
        other_versions: None,
        all_revisions: false,
        reader: Default::default(),
    };
    Ok(get_matching_models(index, &params)?
        .into_iter()
        .map(|(_, version, _)| VersionString::from(version))
        .filter(|version| {
            version.flavor() == installed.flavor()
                && version.partial_cmp(installed) == Some(std::cmp::Ordering::Greater)
        })
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)))
}

/// Details retain superseded release notes independently of the selected target range.
pub(crate) fn select_package_details(
    index: &Model<PackageIndex>,
    id: &PackageId,
    target_version: Option<&VersionRange>,
    device: &DeviceInfo,
) -> Result<GetPackageResponse, Error> {
    let params = GetPackageParams {
        id: Some(id.clone()),
        target_version: None,
        source_version: None,
        device_info: Some(device.clone()),
        other_versions: None,
        all_revisions: true,
        reader: Default::default(),
    };
    let matching = get_matching_models(index, &params)?;
    let (newest, mut older) = partition_best(matching.clone());
    let superseded = superseded_revisions(newest.get(id), &older.remove(id).unwrap_or_default())?;
    let (mut selected, _) = partition_best(
        matching
            .iter()
            .filter(|(_, version, _)| target_version.is_none_or(|range| version.satisfies(range)))
            .cloned()
            .collect(),
    );
    let selected = selected.remove(id).unwrap_or_default();
    let other_versions = matching
        .into_iter()
        .filter(|(_, version, _)| !selected.contains_key(&VersionString::from(version.clone())))
        .map(|(_, version, info)| {
            let version = VersionString::from(version);
            let mut short: PackageInfoShort = from_value(info.into())?;
            short.superseded = superseded.contains(&version);
            Ok((version, short))
        })
        .collect::<Result<_, Error>>()?;
    let best = selected
        .into_iter()
        .map(|(version, info)| {
            let mut info = info.de()?;
            info.strip_access();
            Ok((version, info))
        })
        .collect::<Result<_, Error>>()?;
    Ok(GetPackageResponse {
        categories: categories(index, id)?,
        first_published_at: first_published_at(index, id)?,
        best,
        other_versions: Some(other_versions),
    })
}

pub(crate) fn select_packages(
    index: &Model<PackageIndex>,
    params: &GetPackageParams,
) -> Result<Value, Error> {
    let categories_for = |id: &PackageId| categories(index, id);
    let first_published_for = |id: &PackageId| first_published_at(index, id);
    let (mut best, mut other) = partition_best(get_matching_models(index, params)?);
    if !params.all_revisions {
        for (id, package_other) in &mut other {
            hide_superseded_revisions(best.get(id), package_other)?;
        }
    }
    if let Some(id) = &params.id {
        let categories = categories_for(id)?;
        let first_published_at = first_published_for(id)?;
        let best: BTreeMap<VersionString, PackageVersionInfo> = best
            .remove(id)
            .unwrap_or_default()
            .into_iter()
            .map(|(k, i)| Ok::<_, Error>((k, i.de()?)))
            .try_collect()?;
        let other = other.remove(id).unwrap_or_default();
        match params.other_versions.unwrap_or_default() {
            PackageDetailLevel::None => to_value(&GetPackageResult::Package(GetPackageResponse {
                first_published_at,
                categories,
                best,
                other_versions: None,
            })),
            PackageDetailLevel::Short => to_value(&GetPackageResult::Package(GetPackageResponse {
                first_published_at,
                categories,
                best,
                other_versions: Some(
                    other
                        .into_iter()
                        .map(|(k, i)| from_value(i.into()).map(|v| (k, v)))
                        .try_collect()?,
                ),
            })),
            PackageDetailLevel::Full => {
                to_value(&GetPackageResult::PackageFull(GetPackageResponseFull {
                    first_published_at,
                    categories,
                    best,
                    other_versions: other
                        .into_iter()
                        .map(|(k, i)| Ok::<_, Error>((k, i.de()?)))
                        .try_collect()?,
                }))
            }
        }
    } else {
        match params.other_versions.unwrap_or_default() {
            PackageDetailLevel::None => to_value(&GetPackageResult::Packages(
                best.into_iter()
                    .map(|(id, best)| {
                        let categories = categories_for(&id)?;
                        let first_published_at = first_published_for(&id)?;
                        Ok::<_, Error>((
                            id,
                            GetPackageResponse {
                                first_published_at,
                                categories,
                                best: best
                                    .into_iter()
                                    .map(|(k, i)| Ok::<_, Error>((k, i.de()?)))
                                    .try_collect()?,
                                other_versions: None,
                            },
                        ))
                    })
                    .try_collect::<_, GetPackagesResponse, _>()?,
            )),
            PackageDetailLevel::Short => to_value(&GetPackageResult::Packages(
                best.into_iter()
                    .map(|(id, best)| {
                        let categories = categories_for(&id)?;
                        let first_published_at = first_published_for(&id)?;
                        let other = other.remove(&id).unwrap_or_default();
                        Ok::<_, Error>((
                            id,
                            GetPackageResponse {
                                first_published_at,
                                categories,
                                best: best
                                    .into_iter()
                                    .map(|(k, i)| Ok::<_, Error>((k, i.de()?)))
                                    .try_collect()?,
                                other_versions: Some(
                                    other
                                        .into_iter()
                                        .map(|(k, i)| from_value(i.into()).map(|v| (k, v)))
                                        .try_collect()?,
                                ),
                            },
                        ))
                    })
                    .try_collect::<_, GetPackagesResponse, _>()?,
            )),
            PackageDetailLevel::Full => to_value(&GetPackageResult::PackagesFull(
                best.into_iter()
                    .map(|(id, best)| {
                        let categories = categories_for(&id)?;
                        let first_published_at = first_published_for(&id)?;
                        let other = other.remove(&id).unwrap_or_default();
                        Ok::<_, Error>((
                            id,
                            GetPackageResponseFull {
                                first_published_at,
                                categories,
                                best: best
                                    .into_iter()
                                    .map(|(k, i)| Ok::<_, Error>((k, i.de()?)))
                                    .try_collect()?,
                                other_versions: other
                                    .into_iter()
                                    .map(|(k, i)| Ok::<_, Error>((k, i.de()?)))
                                    .try_collect()?,
                            },
                        ))
                    })
                    .try_collect::<_, GetPackagesResponseFull, _>()?,
            )),
        }
    }
}

pub fn display_package_info(
    params: WithIoFormat<GetPackageParams>,
    info: Value,
) -> Result<(), Error> {
    if let Some(format) = params.format {
        return display_serializable(format, info);
    }

    if let Some(_) = params.rest.id {
        if params.rest.other_versions.unwrap_or_default() == PackageDetailLevel::Full {
            for table in from_value::<GetPackageResponseFull>(info)?.tables() {
                print_table(&table)?;
                println!();
            }
        } else {
            for table in from_value::<GetPackageResponse>(info)?.tables() {
                print_table(&table)?;
                println!();
            }
        }
    } else {
        if params.rest.other_versions.unwrap_or_default() == PackageDetailLevel::Full {
            for (_, package) in from_value::<GetPackagesResponseFull>(info)? {
                for table in package.tables() {
                    print_table(&table)?;
                    println!();
                }
            }
        } else {
            for (_, package) in from_value::<GetPackagesResponse>(info)? {
                for table in package.tables() {
                    print_table(&table)?;
                    println!();
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize, Serialize, VisitFields, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct CliDownloadParams {
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
    #[arg(long, short = 'v', help = "help.arg.target-version-range")]
    pub target_version: Option<VersionRange>,
    #[arg(short, long, help = "help.arg.destination-path")]
    pub dest: Option<PathBuf>,
    #[arg(long, short, help = "help.arg.architecture")]
    pub arch: Option<InternedString>,
}

rpc_toolkit::reflect_ts!(CliDownloadParams);

/// Private downloads omit supplemental endorsements.
pub async fn cli_download(
    ctx: CliContext,
    CliDownloadParams {
        ref id,
        target_version,
        dest,
        arch,
    }: CliDownloadParams,
) -> Result<(), Error> {
    let progress_tracker = FullProgressTracker::new();
    let mut fetching_progress = progress_tracker.add_phase("Fetching".into(), Some(1));
    let download_progress = progress_tracker.add_phase("Downloading".into(), Some(100));
    let mut verify_progress = progress_tracker.add_phase("Verifying".into(), Some(10));

    let progress = progress_tracker.progress_bar_task("Downloading S9PK...");

    fetching_progress.start();
    let mut res: GetPackageResponse = from_value(
        ctx.call_remote::<RegistryContext>(
            "package.get",
            json!({
                "id": &id,
                "targetVersion": &target_version,
            }),
        )
        .await?,
    )?;
    let PackageVersionInfo {
        s9pks: mut s9pk, ..
    } = match res.best.len() {
        0 => {
            return Err(Error::new(
                eyre!(
                    "{}",
                    t!(
                        "registry.package.get.version-not-found",
                        id = id,
                        version = target_version.unwrap_or(VersionRange::Any)
                    )
                ),
                ErrorKind::NotFound,
            ));
        }
        1 => res.best.pop_first().unwrap().1,
        _ => {
            let choices = res.best.keys().cloned().collect::<Vec<_>>();
            let version = choose(
                &format!("Multiple flavors of {id} available. Choose a version to download:"),
                &choices,
            )
            .await?;
            res.best.remove(version).unwrap()
        }
    };
    if let Some(arch) = &arch {
        s9pk.retain(|(hw, _)| {
            hw.arch
                .as_ref()
                .map_or(true, |arches| arches.contains(arch))
        });
    }
    let s9pk = match s9pk.len() {
        0 => {
            return Err(Error::new(
                eyre!(
                    "{}",
                    t!(
                        "registry.package.get.version-not-found",
                        id = id,
                        version = target_version.unwrap_or(VersionRange::Any)
                    )
                ),
                ErrorKind::NotFound,
            ));
        }
        1 => s9pk.pop().unwrap().1,
        _ => {
            let (_, asset) = choose_custom_display(
                &format!(concat!(
                    "Multiple packages with different hardware requirements found. ",
                    "Choose a file to download:"
                )),
                &s9pk,
                |(hw, _)| {
                    use std::fmt::Write;
                    let mut res = String::new();
                    if let Some(arch) = &hw.arch {
                        write!(
                            &mut res,
                            "{}: {}",
                            if arch.len() == 1 {
                                "Architecture"
                            } else {
                                "Architectures"
                            },
                            arch.iter().join(", ")
                        )
                        .unwrap();
                    }
                    if !hw.device.is_empty() {
                        if !res.is_empty() {
                            write!(&mut res, "; ").unwrap();
                        }
                        write!(
                            &mut res,
                            "{}: {}",
                            if hw.device.len() == 1 {
                                "Device"
                            } else {
                                "Devices"
                            },
                            hw.device.iter().map(|d| &d.description).join(", ")
                        )
                        .unwrap();
                    }
                    if let Some(ram) = hw.ram {
                        if !res.is_empty() {
                            write!(&mut res, "; ").unwrap();
                        }
                        write!(
                            &mut res,
                            "RAM >={:.2}GiB",
                            ram as f64 / (1024.0 * 1024.0 * 1024.0)
                        )
                        .unwrap();
                    }

                    res
                },
            )
            .await?;
            asset.clone()
        }
    };
    s9pk.validate(SIG_CONTEXT, s9pk.all_signers())?;
    fetching_progress.complete();

    let dest = dest.unwrap_or_else(|| Path::new(".").join(id).with_extension("s9pk"));
    let dest_tmp = to_tmp_path(&dest)?;
    let (mut parsed, source) = s9pk
        .download_to(&dest_tmp, ctx.client.clone(), download_progress)
        .await?;
    if let Some(size) = source.size().await {
        verify_progress.set_total(size);
    }
    verify_progress.set_units(Some(ProgressUnits::Bytes));
    let plaintext = if source.is_encrypted() {
        Some(to_tmp_path(&dest_tmp)?)
    } else {
        None
    };
    let out: Box<dyn AsyncWrite + Unpin + Send> = match &plaintext {
        Some(path) => Box::new(create_file(path).await?),
        None => Box::new(tokio::io::sink()),
    };
    let mut progress_sink = verify_progress.writer(out);
    parsed
        .serialize(&mut TrackingIO::new(0, &mut progress_sink), true)
        .await?;
    let (mut out, mut verify_progress) = progress_sink.into_inner();
    out.shutdown().await?;
    verify_progress.complete();

    source.inner().wait_for_buffered().await?;
    match plaintext {
        Some(path) => {
            drop((parsed, source));
            delete_file(&dest_tmp).await?;
            rename(path, dest).await?;
        }
        None => rename(dest_tmp, dest).await?,
    }

    progress_tracker.complete();
    progress.await.unwrap();

    println!("{}", t!("registry.package.get.download-complete"));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn versions(entries: &[&str]) -> BTreeMap<VersionString, Model<PackageVersionInfo>> {
        entries.iter().map(|v| version(v, Vec::new())).collect()
    }

    fn version(v: &str, s9pks: Vec<Value>) -> (VersionString, Model<PackageVersionInfo>) {
        let s9pks: Vec<_> = s9pks.into_iter().map(|hw| json!([hw, null])).collect();
        (
            v.parse().unwrap(),
            json!({ "releaseNotes": v, "s9pks": s9pks }).into(),
        )
    }

    fn hidden(best: Vec<(&str, Vec<Value>)>, other: Vec<(&str, Vec<Value>)>) -> BTreeSet<String> {
        let best: BTreeMap<_, _> = best.into_iter().map(|(v, hw)| version(v, hw)).collect();
        let mut other: BTreeMap<_, _> = other.into_iter().map(|(v, hw)| version(v, hw)).collect();
        let all: BTreeSet<_> = other.keys().map(|v| v.to_string()).collect();
        hide_superseded_revisions(Some(&best), &mut other).unwrap();
        all.into_iter()
            .filter(|v| !other.keys().any(|k| k.to_string() == *v))
            .collect()
    }

    #[test]
    fn hide_superseded_revisions_keeps_revision_followed_by_narrower_hardware() {
        let both = json!({ "arch": ["x86_64", "aarch64"] });
        let x86 = json!({ "arch": ["x86_64"] });
        let other = vec![
            ("1.0.0:0", vec![both.clone()]),
            ("1.0.0:1", vec![x86.clone()]),
        ];
        assert!(hidden(vec![], other).is_empty());
        let other = vec![("1.0.0:0", vec![both])];
        assert!(hidden(vec![("1.0.0:1", vec![x86])], other).is_empty());
    }

    #[test]
    fn hide_superseded_revisions_hides_once_hardware_is_restored() {
        let both = json!({ "arch": ["x86_64", "aarch64"] });
        let x86 = json!({ "arch": ["x86_64"] });
        let other = vec![
            ("1.0.0:0", vec![both.clone()]),
            ("1.0.0:1", vec![x86]),
            ("1.0.0:2", vec![both]),
        ];
        assert_eq!(
            hidden(vec![], other),
            BTreeSet::from(["1.0.0:0".to_owned(), "1.0.0:1".to_owned()]),
        );
    }

    #[test]
    fn hide_superseded_revisions_compares_each_hardware_field() {
        let any = json!({});
        let gpu = json!({ "device": [{ "description": "GPU", "class": "display" }] });
        for (older, newer, hides) in [
            (json!({ "ram": 4 }), json!({ "ram": 8 }), false),
            (json!({ "ram": 8 }), json!({ "ram": 4 }), true),
            (any.clone(), json!({ "arch": ["x86_64"] }), false),
            (json!({ "arch": ["x86_64"] }), any.clone(), true),
            (any.clone(), gpu.clone(), false),
            (gpu, any, true),
        ] {
            let other = vec![
                ("1.0.0:0", vec![older.clone()]),
                ("1.0.0:1", vec![newer.clone()]),
            ];
            assert_eq!(
                !hidden(vec![], other).is_empty(),
                hides,
                "{older} then {newer}"
            );
        }
    }

    #[test]
    fn hide_superseded_revisions_needs_every_older_s9pk_covered() {
        let x86 = json!({ "arch": ["x86_64"] });
        let arm = json!({ "arch": ["aarch64"] });
        let other = vec![("1.0.0:0", vec![x86.clone(), arm]), ("1.0.0:1", vec![x86])];
        assert!(hidden(vec![], other).is_empty());
    }

    fn assert_versions(
        actual: BTreeMap<VersionString, Model<PackageVersionInfo>>,
        expected: &[&str],
    ) {
        let values = |entries: BTreeMap<VersionString, Model<PackageVersionInfo>>| {
            entries
                .into_iter()
                .map(|(version, info)| (version, Value::from(info)))
                .collect::<BTreeMap<_, _>>()
        };
        assert_eq!(values(actual), values(versions(expected)));
    }

    #[test]
    fn hide_superseded_revisions_orders_revisions_numerically() {
        let mut other = versions(&["1.0.0:2", "1.0.0:10"]);

        hide_superseded_revisions(None, &mut other).unwrap();

        assert_versions(other, &["1.0.0:10"]);
    }

    #[test]
    fn hide_superseded_revisions_keeps_flavors_and_upstreams_separate() {
        let mut other = versions(&[
            "1.0.0:2",
            "1.0.0:10",
            "2.0.0:1",
            "2.0.0:3",
            "#alpha:1.0.0:1",
            "#alpha:1.0.0:2",
            "#beta:1.0.0:1",
            "#alpha:1.0.0-rc.1:1",
            "#alpha:1.0.0-rc.1:2",
        ]);

        hide_superseded_revisions(None, &mut other).unwrap();

        assert_versions(
            other,
            &[
                "1.0.0:10",
                "2.0.0:3",
                "#alpha:1.0.0:2",
                "#beta:1.0.0:1",
                "#alpha:1.0.0-rc.1:2",
            ],
        );
    }

    #[test]
    fn hide_superseded_revisions_uses_best_without_changing_it() {
        let best = versions(&["2.0.0:10", "#alpha:1.0.0:10"]);
        let mut other = versions(&[
            "2.0.0:2",
            "1.0.0:2",
            "1.0.0:10",
            "#alpha:1.0.0:2",
            "#beta:1.0.0:2",
        ]);

        hide_superseded_revisions(Some(&best), &mut other).unwrap();

        assert_versions(other, &["1.0.0:10", "#beta:1.0.0:2"]);
        assert_versions(best, &["2.0.0:10", "#alpha:1.0.0:10"]);
    }

    #[test]
    fn hide_superseded_revisions_accepts_empty_maps() {
        let best = BTreeMap::new();
        let mut other = BTreeMap::new();

        hide_superseded_revisions(None, &mut other).unwrap();
        assert!(other.is_empty());
        hide_superseded_revisions(Some(&best), &mut other).unwrap();
        assert!(other.is_empty());
        hide_superseded_revisions(Some(&versions(&["1.0.0:10"])), &mut other).unwrap();
        assert!(other.is_empty());
    }

    #[test]
    fn all_revisions_json_defaults_to_false_and_accepts_true() {
        for input in [json!({}), json!({ "allRevisions": false })] {
            let params: GetPackageParams = from_value(input).unwrap();
            assert!(!params.all_revisions);
            assert_eq!(to_value(&params).unwrap()["allRevisions"], json!(false));
        }

        let params: GetPackageParams = from_value(json!({ "allRevisions": true })).unwrap();
        assert!(params.all_revisions);
        assert_eq!(to_value(&params).unwrap()["allRevisions"], json!(true));
    }

    #[test]
    fn all_revisions_cli_defaults_to_false_and_supports_flag() {
        let params = GetPackageParams::try_parse_from(["get"]).unwrap();
        assert!(!params.all_revisions);
        assert_eq!(params.other_versions, Some(PackageDetailLevel::None));

        let params = GetPackageParams::try_parse_from(["get", "--all-revisions"]).unwrap();
        assert!(params.all_revisions);
    }

    #[test]
    fn listing_keeps_the_newest_release_per_flavor_its_update_target_and_every_version() {
        use crate::registry::package::index::tests::version_info_signed_by;

        let (_, info) = version_info_signed_by(1);
        let info = to_value(&info).unwrap();
        let index: Model<PackageIndex> = json!({
            "categories": {},
            "packages": { "demo": {
                "authorized": {},
                "categories": ["tools"],
                "versions": {
                    "1.0.0:0": info,
                    "1.0.0:1": info,
                    "2.0.0:0": info,
                    "#alt:1.0.0:0": info,
                },
            } },
        })
        .into();
        let device = DeviceInfo {
            os: crate::registry::device_info::OsInfo {
                version: "0.4.0".parse().unwrap(),
                compat: "*".parse().unwrap(),
                platform: "x86_64".into(),
                language: None,
            },
            hardware: None,
        };
        let id: PackageId = "demo".parse().unwrap();
        let keys = |versions: Vec<&VersionString>| -> BTreeSet<String> {
            versions.into_iter().map(ToString::to_string).collect()
        };
        let set = |versions: &[&str]| -> BTreeSet<String> {
            versions.iter().map(|v| (*v).to_owned()).collect()
        };

        let newest = listing(&index, &id, &device, None)
            .unwrap()
            .unwrap()
            .de()
            .unwrap();
        assert_eq!(newest.categories, ["tools".into()].into());
        assert!(newest.update_target.is_none());
        assert_eq!(
            keys(newest.releases.keys().collect()),
            set(&["#alt:1.0.0:0", "2.0.0:0"])
        );
        let release = &newest.releases[&"2.0.0:0".parse::<VersionString>().unwrap()];
        assert_eq!(
            to_value(&release.metadata.description).unwrap(),
            info["description"]
        );
        assert_eq!(
            keys(newest.versions.keys().collect()),
            set(&["#alt:1.0.0:0", "1.0.0:0", "1.0.0:1", "2.0.0:0"])
        );
        assert!(newest.versions[&"1.0.0:0".parse::<VersionString>().unwrap()].superseded);
        assert!(!newest.versions[&"1.0.0:1".parse::<VersionString>().unwrap()].superseded);

        let target: VersionString = "1.0.0:1".parse().unwrap();
        let updating = listing(&index, &id, &device, Some(&target))
            .unwrap()
            .unwrap()
            .de()
            .unwrap();
        assert_eq!(updating.update_target, Some(target));
        assert_eq!(
            keys(updating.releases.keys().collect()),
            set(&["#alt:1.0.0:0", "1.0.0:1", "2.0.0:0"])
        );

        let installed: VersionString = "1.0.0:0".parse().unwrap();
        assert_eq!(
            update_target(&index, &id, &installed, &device).unwrap(),
            Some("2.0.0:0".parse().unwrap())
        );
    }

    #[test]
    fn exact_details_keep_other_versions_and_superseded_release_notes() {
        use crate::registry::package::index::tests::version_info_signed_by;

        let (_, info) = version_info_signed_by(1);
        let info = to_value(&info).unwrap();
        let index: Model<PackageIndex> = json!({
            "categories": {},
            "packages": { "demo": {
                "authorized": {},
                "categories": ["tools"],
                "versions": {
                    "1.0.0:0": info,
                    "1.0.0:1": info,
                    "2.0.0:0": info,
                    "#alt:1.0.0:0": info,
                },
            } },
        })
        .into();
        let id: PackageId = "demo".parse().unwrap();
        let device = DeviceInfo {
            os: crate::registry::device_info::OsInfo {
                version: "0.4.0".parse().unwrap(),
                compat: "*".parse().unwrap(),
                platform: "x86_64".into(),
                language: None,
            },
            hardware: None,
        };
        let target = "=1.0.0:1".parse().unwrap();
        let details = select_package_details(&index, &id, Some(&target), &device).unwrap();
        let version = |value: &str| value.parse::<VersionString>().unwrap();
        assert_eq!(details.best.len(), 1);
        assert!(details.best.contains_key(&version("1.0.0:1")));
        let other = details.other_versions.unwrap();
        assert_eq!(other.len(), 3);
        assert!(other[&version("1.0.0:0")].superseded);
        assert!(!other[&version("2.0.0:0")].superseded);
        assert_eq!(
            to_value(&other[&version("1.0.0:0")]).unwrap()["releaseNotes"],
            info["releaseNotes"]
        );
        assert!(other.contains_key(&version("#alt:1.0.0:0")));
    }

    #[test]
    fn listed_release_is_the_saved_release_without_access() {
        use crate::registry::package::index::tests::version_info_signed_by;

        let (_, mut info) = version_info_signed_by(1);
        for (_, asset) in &mut info.s9pks {
            asset.readers.insert(crate::rpc_continuations::Guid::new());
        }
        let release: PackageVersionInfo =
            from_value(listed_release(&Model::new(&info).unwrap())).unwrap();
        info.strip_access();
        assert_eq!(to_value(&release).unwrap(), to_value(&info).unwrap());
    }

    #[test]
    fn first_published_at_is_the_earliest_asset_of_any_version() {
        use crate::registry::package::index::tests::version_info_signed_by;

        let (_, mut first) = version_info_signed_by(1);
        first.s9pks[0].1.published_at = "2026-01-01T00:00:00Z".parse().unwrap();
        let (_, mut second) = version_info_signed_by(2);
        second.s9pks[0].1.published_at = "2026-03-01T00:00:00Z".parse().unwrap();
        let index: Model<PackageIndex> = json!({
            "categories": {},
            "packages": { "demo": {
                "authorized": {},
                "categories": [],
                "versions": {
                    "1.0.0:0": to_value(&first).unwrap(),
                    "2.0.0:0": to_value(&second).unwrap(),
                },
            } },
        })
        .into();
        let expected = json!("2026-01-01T00:00:00Z");

        let one = GetPackageParams::try_parse_from(["get", "demo"]).unwrap();
        let one = select_packages(&index, &one).unwrap();
        assert_eq!(one["firstPublishedAt"], expected);

        let all = GetPackageParams::try_parse_from(["get"]).unwrap();
        let all = select_packages(&index, &all).unwrap();
        assert_eq!(all["demo"]["firstPublishedAt"], expected);
    }
}

#[test]
fn check_matching_info_short() {
    use crate::registry::package::index::PackageMetadata;
    use crate::s9pk::manifest::Description;

    let lang_map =
        |s: &str| LocaleString::LanguageMap([("en".into(), s.into())].into_iter().collect());

    let info = PackageVersionInfo {
        metadata: PackageMetadata {
            title: "Test Package".into(),
            default_locale: None,
            description: Description {
                short: lang_map("A short description"),
                long: lang_map("A longer description of the test package"),
            },
            release_notes: lang_map("Initial release"),
            pre_download_alert: None,
            git_hash: None,
            license: "MIT".into(),
            package_repo: "https://github.com/example/wrapper".parse().unwrap(),
            upstream_repo: "https://github.com/example/upstream".parse().unwrap(),
            marketing_url: Some("https://example.com".parse().unwrap()),
            donation_url: None,
            os_version: exver::Version::new([0, 3, 6], []),
            sdk_version: None,
            hardware_acceleration: false,
            userspace_filesystems: false,
            virtual_networking: false,
            hardware_virtualization: false,
            plugins: BTreeSet::new(),
            satisfies: BTreeSet::new(),
        },
        icon: Some(crate::registry::package::icon::PackageIcon::Stored(
            "blake3/icon.png".into(),
        )),
        dependency_metadata: BTreeMap::new(),
        source_version: None,
        s9pks: Vec::new(),
    };
    from_value::<PackageInfoShort>(to_value(&info).unwrap()).unwrap();
}

#[test]
fn rpc_result_surrogate_preserves_all_package_selection_shapes() {
    let minimal = serde_json::json!({ "categories": ["tools"], "best": {} });
    let with_versions =
        serde_json::json!({ "categories": ["tools"], "best": {}, "otherVersions": {} });
    for fixture in [minimal, with_versions.clone()] {
        let package = serde_json::from_value::<GetPackageResponse>(fixture.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(to_value(&GetPackageResult::Package(package)).unwrap()).unwrap(),
            fixture
        );
        let packages_fixture = serde_json::json!({ "demo": fixture });
        let packages =
            serde_json::from_value::<GetPackagesResponse>(packages_fixture.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(to_value(&GetPackageResult::Packages(packages)).unwrap()).unwrap(),
            packages_fixture
        );
    }
    let full = serde_json::from_value::<GetPackageResponseFull>(with_versions.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(to_value(&GetPackageResult::PackageFull(full)).unwrap()).unwrap(),
        with_versions
    );
    let full_map = serde_json::json!({ "demo": with_versions });
    let packages = serde_json::from_value::<GetPackagesResponseFull>(full_map.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(to_value(&GetPackageResult::PackagesFull(packages)).unwrap()).unwrap(),
        full_map
    );
}
