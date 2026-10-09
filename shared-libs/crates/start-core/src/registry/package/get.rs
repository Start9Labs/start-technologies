use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use chrono::Utc;
use clap::{Parser, ValueEnum};
use exver::{ExtendedVersion, VersionRange};
use imbl_value::{InternedString, json};
use itertools::Itertools;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::PackageId;
use crate::context::CliContext;
use crate::prelude::*;
use crate::progress::{FullProgressTracker, ProgressUnits};
use crate::registry::context::RegistryContext;
use crate::registry::device_info::DeviceInfo;
use crate::registry::package::index::{PackageIndex, PackageVersionInfo};
use crate::s9pk::manifest::{HardwareRequirements, LocaleString};
use crate::s9pk::merkle_archive::source::ArchiveSource;
use crate::s9pk::v2::SIG_CONTEXT;
use crate::util::VersionString;
use crate::util::io::{TrackingIO, to_tmp_path};
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
    #[visit(ts(type = "string | null"), wire = "Option<rpc_toolkit::ts::Unknown>")]
    #[arg(long, short = 'v', help = "help.arg.target-version-range")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub target_version: Option<VersionRange>,
    #[arg(long, help = "help.arg.source-version")]
    pub source_version: Option<VersionString>,
    #[visit(ts(skip), wire = "rpc_toolkit::ts::Unknown")]
    #[arg(skip)]
    #[serde(rename = "__DeviceInfo_device_info")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub device_info: Option<DeviceInfo>,
    #[arg(default_value = "none", help = "help.arg.other-versions-detail")]
    pub other_versions: Option<PackageDetailLevel>,
    #[arg(long, help = "help.arg.all-revisions")]
    #[serde(default)]
    pub all_revisions: bool,
}

rpc_toolkit::reflect_ts!(GetPackageParams);
rpc_toolkit::ts_export!(GetPackageParams, namespaces = [""]);

#[derive(Debug, Deserialize, Serialize, VisitFields, HasModel)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct GetPackageResponse {
    pub categories: BTreeSet<InternedString>,
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
                        &info.release_notes.localized()
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
    other.retain(|v, _| !hidden.contains(v));
    Ok(())
}

pub async fn get_package(ctx: RegistryContext, params: GetPackageParams) -> Result<Value, Error> {
    let peek = ctx.db.peek().await;
    let mut best: BTreeMap<PackageId, BTreeMap<VersionString, Model<PackageVersionInfo>>> =
        Default::default();
    let mut other: BTreeMap<PackageId, BTreeMap<VersionString, Model<PackageVersionInfo>>> =
        Default::default();
    for (id, version, info) in get_matching_models(&peek.as_index().as_package(), &params)? {
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
    if !params.all_revisions {
        for (id, package_other) in &mut other {
            hide_superseded_revisions(best.get(id), package_other)?;
        }
    }
    if let Some(id) = &params.id {
        if params.target_version.is_some() {
            let created_at = Utc::now().to_rfc3339();
            let pkg_id = id.to_string();
            let version = best
                .get(id)
                .and_then(|b| b.keys().last())
                .map(|v| v.to_string());
            let ctx = ctx.clone();
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
        let categories = peek
            .as_index()
            .as_package()
            .as_packages()
            .as_idx(id)
            .map(|p| p.as_categories().de())
            .transpose()?
            .unwrap_or_default();
        let best: BTreeMap<VersionString, PackageVersionInfo> = best
            .remove(id)
            .unwrap_or_default()
            .into_iter()
            .map(|(k, i)| Ok::<_, Error>((k, i.de()?)))
            .try_collect()?;
        let other = other.remove(id).unwrap_or_default();
        match params.other_versions.unwrap_or_default() {
            PackageDetailLevel::None => to_value(&GetPackageResult::Package(GetPackageResponse {
                categories,
                best,
                other_versions: None,
            })),
            PackageDetailLevel::Short => to_value(&GetPackageResult::Package(GetPackageResponse {
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
                        let categories = peek
                            .as_index()
                            .as_package()
                            .as_packages()
                            .as_idx(&id)
                            .map(|p| p.as_categories().de())
                            .transpose()?
                            .unwrap_or_default();
                        Ok::<_, Error>((
                            id,
                            GetPackageResponse {
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
                        let categories = peek
                            .as_index()
                            .as_package()
                            .as_packages()
                            .as_idx(&id)
                            .map(|p| p.as_categories().de())
                            .transpose()?
                            .unwrap_or_default();
                        let other = other.remove(&id).unwrap_or_default();
                        Ok::<_, Error>((
                            id,
                            GetPackageResponse {
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
                        let categories = peek
                            .as_index()
                            .as_package()
                            .as_packages()
                            .as_idx(&id)
                            .map(|p| p.as_categories().de())
                            .transpose()?
                            .unwrap_or_default();
                        let other = other.remove(&id).unwrap_or_default();
                        Ok::<_, Error>((
                            id,
                            GetPackageResponseFull {
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
                table.print_tty(false)?;
                println!();
            }
        } else {
            for table in from_value::<GetPackageResponse>(info)?.tables() {
                table.print_tty(false)?;
                println!();
            }
        }
    } else {
        if params.rest.other_versions.unwrap_or_default() == PackageDetailLevel::Full {
            for (_, package) in from_value::<GetPackagesResponseFull>(info)? {
                for table in package.tables() {
                    table.print_tty(false)?;
                    println!();
                }
            }
        } else {
            for (_, package) in from_value::<GetPackagesResponse>(info)? {
                for table in package.tables() {
                    table.print_tty(false)?;
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
    #[visit(ts(type = "string | null"), wire = "Option<rpc_toolkit::ts::Unknown>")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub target_version: Option<VersionRange>,
    #[arg(short, long, help = "help.arg.destination-path")]
    pub dest: Option<PathBuf>,
    #[arg(long, short, help = "help.arg.architecture")]
    #[visit(ts(type = "string | null"), wire = "Option<rpc_toolkit::ts::Unknown>")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub arch: Option<InternedString>,
}

rpc_toolkit::reflect_ts!(CliDownloadParams);

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
    let mut progress_sink = verify_progress.writer(tokio::io::sink());
    parsed
        .serialize(&mut TrackingIO::new(0, &mut progress_sink), true)
        .await?;
    progress_sink.into_inner().1.complete();

    source.wait_for_buffered().await?;
    tokio::fs::rename(dest_tmp, dest).await?;

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
}

#[test]
fn check_matching_info_short() {
    use crate::registry::package::index::PackageMetadata;
    use crate::s9pk::manifest::Description;
    use crate::util::DataUrl;

    let lang_map =
        |s: &str| LocaleString::LanguageMap([("en".into(), s.into())].into_iter().collect());

    let info = PackageVersionInfo {
        metadata: PackageMetadata {
            title: "Test Package".into(),
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
        icon: DataUrl::from_vec("image/png", vec![]),
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
