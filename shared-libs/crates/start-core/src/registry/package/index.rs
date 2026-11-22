use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::u32;

use chrono::Utc;
use clap::Parser;
use exver::{Version, VersionRange};
use imbl_value::InternedString;
use patch_db::ModelExt;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::PackageId;
use crate::db::model::package::CurrentDependencyKind;
use crate::prelude::*;
use crate::registry::RegistryDatabase;
use crate::registry::asset::RegistryAsset;
use crate::registry::context::RegistryContext;
use crate::registry::device_info::DeviceInfo;
use crate::registry::package::icon::{PackageIcon, store_icon_async};
use crate::registry::signer::SignerInfo;
use crate::rpc_continuations::Guid;
use crate::s9pk::S9pk;
use crate::s9pk::git_hash::GitHash;
use crate::s9pk::manifest::{Description, HardwareRequirements, LocaleString, current_version};
use crate::s9pk::merkle_archive::source::FileSource;
#[cfg(test)]
use crate::s9pk::merkle_archive::source::encrypted::KeyStreamInfo;
use crate::service::effects::plugin::PluginId;
#[cfg(test)]
use crate::sign::AnySignature;
use crate::sign::AnyVerifyingKey;
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::util::VersionString;

#[derive(Debug, Default, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct PackageIndex {
    pub categories: BTreeMap<InternedString, Category>,
    pub packages: BTreeMap<PackageId, PackageInfo>,
}

rpc_toolkit::reflect_ts!(PackageIndex);
rpc_toolkit::ts_export!(PackageIndex, namespaces = [""]);

impl PackageIndex {
    pub fn retain_registered_signatures(&mut self, signers: &BTreeMap<Guid, SignerInfo>) {
        for package in self.packages.values_mut() {
            for version in package.versions.values_mut() {
                version.retain_registered_signatures(signers);
            }
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct PackageInfo {
    pub authorized: BTreeMap<Guid, VersionRange>,
    pub versions: BTreeMap<VersionString, PackageVersionInfo>,
    pub categories: BTreeSet<InternedString>,
}

rpc_toolkit::reflect_ts!(PackageInfo);
rpc_toolkit::ts_export!(PackageInfo, namespaces = [""]);

/// The identities a request's key holds on this registry.
#[derive(Clone, Debug, Default)]
pub struct Reader {
    pub admin: bool,
    pub signer: Option<Guid>,
    /// Entries of `FullIndex.readers` holding the key.
    pub reader_ids: BTreeSet<Guid>,
}

impl Reader {
    /// None for a missing key or one the registry does not know.
    pub fn resolve(
        db: &Model<RegistryDatabase>,
        key: Option<&AnyVerifyingKey>,
    ) -> Result<Option<Self>, Error> {
        let Some(key) = key else {
            return Ok(None);
        };
        let signer = db.as_index().as_signers().find_signer(key)?;
        let reader_ids = db.as_index().as_readers().ids_for(key)?;
        if signer.is_none() && reader_ids.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            admin: match &signer {
                Some(signer) => db.as_admins().de()?.contains(signer),
                None => false,
            },
            signer,
            reader_ids,
        }))
    }

    fn is_admin(reader: Option<&Self>) -> bool {
        reader.is_some_and(|reader| reader.admin)
    }

    /// Publishers see every s9pk of their package.
    fn sees(reader: Option<&Self>, publisher: bool, access: &S9pkAccess) -> bool {
        !access.private
            || publisher
            || reader.is_some_and(|reader| {
                reader.admin || !reader.reader_ids.is_disjoint(&access.readers)
            })
    }
}

/// The access fields of a stored `RegistryAsset`.
#[derive(Deserialize)]
struct S9pkAccess {
    #[serde(default, deserialize_with = "is_present")]
    private: bool,
    #[serde(default)]
    readers: BTreeSet<Guid>,
}

fn is_present<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    Ok(!Value::deserialize(deserializer)?.is_null())
}

impl S9pkAccess {
    fn of(entry: &Value) -> Result<Self, Error> {
        from_value(entry[1].clone())
    }
}

impl Model<PackageVersionInfo> {
    /// Drops the s9pks the requester cannot see, and reader lists for everyone but admins and
    /// publishers.
    ///
    /// False when the version had s9pks and none remain.
    pub(crate) fn retain_visible(
        &mut self,
        reader: Option<&Reader>,
        publisher: bool,
    ) -> Result<bool, Error> {
        let Some(s9pks) = ModelExt::as_value_mut(self.as_s9pks_mut()).as_array_mut() else {
            return Ok(true);
        };
        if s9pks.is_empty() {
            return Ok(true);
        }
        let mut visible = imbl::Vector::new();
        for mut entry in std::mem::take(s9pks) {
            if Reader::sees(reader, publisher, &S9pkAccess::of(&entry)?) {
                if !(publisher || Reader::is_admin(reader)) {
                    if let Some(asset) = entry[1].as_object_mut() {
                        asset.remove("readers");
                    }
                }
                visible.push_back(entry);
            }
        }
        *s9pks = visible;
        Ok(!s9pks.is_empty())
    }

    pub fn is_visible_to(&self, reader: Option<&Reader>, publisher: bool) -> Result<bool, Error> {
        self.clone().retain_visible(reader, publisher)
    }

    fn grants_private_access(&self, reader: &Reader, publisher: bool) -> Result<bool, Error> {
        for entry in ModelExt::as_value(self.as_s9pks())
            .as_array()
            .into_iter()
            .flatten()
        {
            let access = S9pkAccess::of(entry)?;
            if access.private && Reader::sees(Some(reader), publisher, &access) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl Model<PackageInfo> {
    pub fn is_publisher(&self, reader: Option<&Reader>) -> Result<bool, Error> {
        match reader.and_then(|reader| reader.signer.as_ref()) {
            Some(signer) => self.as_authorized().contains_key(signer),
            None => Ok(false),
        }
    }

    /// Whether the requester administers the registry or publishes this package.
    pub fn is_managed_by(&self, reader: Option<&Reader>) -> Result<bool, Error> {
        Ok(Reader::is_admin(reader) || self.is_publisher(reader)?)
    }

    /// Drops what the requester cannot see. False when versions existed and none remain.
    fn retain_visible(&mut self, reader: Option<&Reader>) -> Result<bool, Error> {
        let publisher = self.is_publisher(reader)?;
        let versions = self.as_versions_mut();
        if versions.keys()?.is_empty() {
            return Ok(true);
        }
        versions.retain(|_, version| version.retain_visible(reader, publisher))?;
        Ok(!versions.keys()?.is_empty())
    }

    pub fn is_visible_to(&self, reader: Option<&Reader>) -> Result<bool, Error> {
        self.clone().retain_visible(reader)
    }

    pub(crate) fn for_each_locale_string(
        &mut self,
        f: &mut impl FnMut(&mut LocaleString),
    ) -> Result<(), Error> {
        for (_, version) in self.as_versions_mut().as_entries_mut()? {
            version.for_each_locale_string(f)?;
        }
        Ok(())
    }
}

impl Model<PackageIndex> {
    pub fn visible_package(
        &self,
        id: &PackageId,
        reader: Option<&Reader>,
    ) -> Result<Option<&Model<PackageInfo>>, Error> {
        match self.as_packages().as_idx(id) {
            Some(info) if info.is_visible_to(reader)? => Ok(Some(info)),
            _ => Ok(None),
        }
    }

    /// Whether the requester sees any private s9pk.
    pub fn grants_private_access(&self, reader: &Reader) -> Result<bool, Error> {
        for (_, info) in self.as_packages().as_entries()? {
            let publisher = info.is_publisher(Some(reader))?;
            for (_, version) in info.as_versions().as_entries()? {
                if version.grants_private_access(reader, publisher)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Drops the s9pks, versions, and packages the requester cannot see, and reader lists for
    /// everyone but admins and each package's publishers.
    pub fn retain_visible(&mut self, reader: Option<&Reader>) -> Result<(), Error> {
        self.as_packages_mut()
            .retain(|_, info| info.retain_visible(reader))
    }

    pub fn s9pks_with(
        &self,
        commitment: &MerkleArchiveCommitment,
    ) -> Result<Vec<RegistryAsset<MerkleArchiveCommitment>>, Error> {
        let mut found = Vec::new();
        for (_, package) in self.as_packages().as_entries()? {
            for (_, version) in package.as_versions().as_entries()? {
                found.extend(
                    version
                        .as_s9pks()
                        .de()?
                        .into_iter()
                        .map(|(_, asset)| asset)
                        .filter(|asset| &asset.commitment == commitment),
                );
            }
        }
        Ok(found)
    }

    pub fn de_visible_to(&self, reader: Option<&Reader>) -> Result<PackageIndex, Error> {
        let mut index = self.clone();
        index.retain_visible(reader)?;
        index.de()
    }
}

#[derive(Debug, Default, Deserialize, Serialize, VisitFields, Parser)]
#[group(skip)]
pub struct ReaderParams {
    #[arg(skip)]
    #[visit(ts(skip))]
    #[serde(rename = "__Auth_signer")]
    pub signer: Option<AnyVerifyingKey>,
}

rpc_toolkit::reflect_ts!(ReaderParams);

impl ReaderParams {
    pub fn reader(&self, db: &Model<RegistryDatabase>) -> Result<Option<Reader>, Error> {
        Reader::resolve(db, self.signer.as_ref())
    }
}

#[derive(Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct Category {
    pub name: LocaleString,
}

rpc_toolkit::reflect_ts!(Category);
rpc_toolkit::ts_export!(Category, namespaces = [""]);

#[derive(Debug, Deserialize, Serialize, HasModel, PartialEq, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct DependencyMetadata {
    pub title: Option<LocaleString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<PackageIcon>,
    pub description: Option<LocaleString>,
    pub optional: bool,
    #[serde(default)]
    pub version_range: Option<VersionRange>,
    #[serde(flatten)]
    pub kind: Option<CurrentDependencyKind>,
}

rpc_toolkit::reflect_ts!(DependencyMetadata);
rpc_toolkit::ts_export!(DependencyMetadata, namespaces = [""]);

fn placeholder_url() -> Url {
    "https://example.com".parse().unwrap()
}

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PreDownloadAlertWhen {
    pub source_version: VersionRange,
}

rpc_toolkit::reflect_ts!(PreDownloadAlertWhen);
rpc_toolkit::ts_export!(PreDownloadAlertWhen, namespaces = [""]);

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PreDownloadAlert {
    pub message: LocaleString,
    pub when: PreDownloadAlertWhen,
}

rpc_toolkit::reflect_ts!(PreDownloadAlert);
rpc_toolkit::ts_export!(PreDownloadAlert, namespaces = [""]);

#[derive(Clone, Debug, Deserialize, Serialize, HasModel, VisitFields, PartialEq)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct PackageMetadata {
    pub title: InternedString,
    /// Locale this package's `LocaleString`s fall back to when a requested one is missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_locale: Option<InternedString>,
    pub description: Description,
    pub release_notes: LocaleString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_download_alert: Option<PreDownloadAlert>,
    pub git_hash: Option<GitHash>,
    pub license: InternedString,
    #[serde(default = "placeholder_url")]
    pub package_repo: Url,
    pub upstream_repo: Url,
    pub marketing_url: Option<Url>,
    pub donation_url: Option<Url>,
    #[serde(default = "current_version")]
    pub os_version: Version,
    pub sdk_version: Option<Version>,
    #[serde(default)]
    pub hardware_acceleration: bool,
    /// Grants access to `/dev/fuse`.
    #[serde(default)]
    pub userspace_filesystems: bool,
    /// Grants access to `/dev/net/tun`.
    #[serde(default)]
    pub virtual_networking: bool,
    /// Grants /dev/kvm when present. The opening process must run as container root.
    #[serde(default)]
    pub hardware_virtualization: bool,
    #[serde(default)]
    pub plugins: BTreeSet<PluginId>,
    #[serde(default)]
    pub satisfies: BTreeSet<VersionString>,
}

rpc_toolkit::reflect_ts!(PackageMetadata);

#[derive(Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct PackageVersionInfo {
    #[serde(flatten)]
    pub metadata: PackageMetadata,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<PackageIcon>,
    pub dependency_metadata: BTreeMap<PackageId, DependencyMetadata>,
    pub source_version: Option<VersionRange>,
    pub s9pks: Vec<(HardwareRequirements, RegistryAsset<MerkleArchiveCommitment>)>,
}

rpc_toolkit::reflect_ts!(PackageVersionInfo);
rpc_toolkit::ts_export!(PackageVersionInfo, namespaces = [""]);
impl PackageVersionInfo {
    pub async fn from_s9pk<S: FileSource + Clone>(
        s9pk: &S9pk<S>,
        urls: Vec<Url>,
        icons: &Path,
    ) -> Result<Self, Error> {
        let manifest = s9pk.as_manifest();
        let icon = Some(store_icon_async(icons, s9pk.icon_data_url().await?).await?);
        let mut dependency_metadata = BTreeMap::new();
        for (id, info) in &manifest.dependencies.0 {
            let dep_meta = s9pk.dependency_metadata(id).await?;
            dependency_metadata.insert(
                id.clone(),
                DependencyMetadata {
                    title: dep_meta.map(|m| m.title),
                    icon: match s9pk.dependency_icon_data_url(id).await? {
                        Some(icon) => Some(store_icon_async(icons, icon).await?),
                        None => None,
                    },
                    description: info.description.clone(),
                    optional: info.optional,
                    version_range: info.version_range.clone(),
                    kind: info.kind.clone(),
                },
            );
        }
        Ok(Self {
            metadata: manifest.metadata.clone(),
            icon,
            dependency_metadata,
            source_version: Some(manifest.can_migrate_from.clone()),
            s9pks: vec![(
                manifest.hardware_requirements.clone(),
                RegistryAsset {
                    published_at: Utc::now(),
                    urls,
                    commitment: s9pk.as_archive().commitment().await?,
                    signatures: s9pk.signatures().await?,
                    private: None,
                    readers: BTreeSet::new(),
                },
            )],
        })
    }
    /// Authoritative indexes retain signatures from keys in `FullIndex.signers`.
    pub fn retain_registered_signatures(&mut self, signers: &BTreeMap<Guid, SignerInfo>) {
        let keys: BTreeSet<&AnyVerifyingKey> = signers.values().flat_map(|s| &s.keys).collect();
        for (_, asset) in &mut self.s9pks {
            asset.signatures.retain(|key, _| keys.contains(key));
        }
    }
    /// A matching s9pk takes the other's URLs and access when replacing URLs, and keeps its own
    /// access otherwise.
    pub fn merge_with(&mut self, other: Self, replace_urls: bool) -> Result<(), Error> {
        if self.metadata != other.metadata {
            return Err(Error::new(
                color_eyre::eyre::eyre!("{}", t!("registry.package.index.metadata-mismatch")),
                ErrorKind::InvalidRequest,
            ));
        }
        if self.icon != other.icon {
            return Err(Error::new(
                color_eyre::eyre::eyre!("{}", t!("registry.package.index.icon-mismatch")),
                ErrorKind::InvalidRequest,
            ));
        }
        if self.dependency_metadata != other.dependency_metadata {
            return Err(Error::new(
                color_eyre::eyre::eyre!(
                    "{}",
                    t!("registry.package.index.dependency-metadata-mismatch")
                ),
                ErrorKind::InvalidRequest,
            ));
        }
        for (hw_req, asset) in other.s9pks {
            if let Some((_, matching)) = self
                .s9pks
                .iter_mut()
                .find(|(h, s)| s.commitment == asset.commitment && *h == hw_req)
            {
                matching.merge_signatures(&asset.commitment, asset.signatures);
                if replace_urls {
                    matching.urls = asset.urls;
                    matching.private = asset.private;
                    matching.readers = asset.readers;
                } else {
                    for url in asset.urls {
                        if matching.urls.contains(&url) {
                            continue;
                        }
                        matching.urls.push(url);
                    }
                }
            } else {
                if let Some((h, matching)) = self.s9pks.iter_mut().find(|(h, _)| *h == hw_req) {
                    *matching = asset;
                    *h = hw_req;
                } else {
                    self.s9pks.push((hw_req, asset));
                }
            }
        }
        self.s9pks.sort_by_key(|(h, _)| h.specificity_desc());
        Ok(())
    }
    /// Removes every s9pk's keystream and reader list.
    pub fn strip_access(&mut self) {
        for (_, asset) in &mut self.s9pks {
            asset.private = None;
            asset.readers.clear();
        }
    }
    pub fn table(self, version: &VersionString) -> prettytable::Table {
        use prettytable::*;

        let mut table = Table::new();

        table.add_row(row![bc => &self.metadata.title]);
        table.add_row(row![br -> "VERSION", AsRef::<str>::as_ref(version)]);
        table.add_row(row![br -> "RELEASE NOTES", &self.metadata.release_notes.localized(self.metadata.default_locale.as_deref())]);
        table.add_row(
            row![br -> "ABOUT", &textwrap::wrap(&self.metadata.description.short.localized(self.metadata.default_locale.as_deref()), 80).join("\n")],
        );
        table.add_row(row![
            br -> "DESCRIPTION",
            &textwrap::wrap(&self.metadata.description.long.localized(self.metadata.default_locale.as_deref()), 80).join("\n")
        ]);
        table.add_row(row![br -> "GIT HASH", self.metadata.git_hash.as_deref().unwrap_or("N/A")]);
        table.add_row(row![br -> "LICENSE", &self.metadata.license]);
        table.add_row(row![br -> "PACKAGE REPO", &self.metadata.package_repo.to_string()]);
        table.add_row(row![br -> "SERVICE REPO", &self.metadata.upstream_repo.to_string()]);
        table.add_row(row![br -> "WEBSITE", self.metadata.marketing_url.as_ref().map_or("N/A".to_owned(), |u| u.to_string())]);

        table
    }
}
impl Model<PackageVersionInfo> {
    /// Filters this package version for compatibility with the given device.
    /// Returns false if the package is incompatible (should be removed).
    /// Modifies s9pks in place to only include compatible variants.
    pub fn for_device(&mut self, device_info: &DeviceInfo) -> Result<bool, Error> {
        if !self
            .as_metadata()
            .as_os_version()
            .de()?
            .satisfies(&device_info.os.compat)
        {
            return Ok(false);
        }
        if let Some(hw) = &device_info.hardware {
            self.as_s9pks_mut().mutate(|s9pks| {
                s9pks.retain(|(hw_req, _)| hw_req.is_compatible(hw));
                if hw.devices.is_some() {
                    s9pks.sort_by_key(|(req, _)| req.specificity_desc());
                } else {
                    s9pks.sort_by_key(|(req, _)| {
                        let (dev, arch, ram) = req.specificity_desc();
                        (u32::MAX - dev, arch, ram)
                    });
                }
                Ok(())
            })?;

            if ModelExt::as_value(self.as_s9pks())
                .as_array()
                .map_or(true, |s| s.is_empty())
            {
                return Ok(false);
            }

            if let Some(locale) = device_info.os.language.as_deref() {
                self.localize_for(Some(locale))?;
            }
        }

        Ok(true)
    }

    /// Resolves every `LocaleString` with `LocaleString::localize_for` and the declared default.
    pub fn localize_for(&mut self, locale: Option<&str>) -> Result<(), Error> {
        let default = self.as_metadata().as_default_locale().de()?;
        self.for_each_locale_string(&mut |s| s.localize_for(locale, default.as_deref()))
    }

    pub(crate) fn for_each_locale_string(
        &mut self,
        f: &mut impl FnMut(&mut LocaleString),
    ) -> Result<(), Error> {
        for (_, dependency) in self.as_dependency_metadata_mut().as_entries_mut()? {
            dependency.mutate(|d| {
                d.title
                    .iter_mut()
                    .chain(&mut d.description)
                    .for_each(&mut *f);
                Ok(())
            })?;
        }
        let metadata = self.as_metadata_mut();
        metadata.as_description_mut().mutate(|d| {
            f(&mut d.short);
            f(&mut d.long);
            Ok(())
        })?;
        metadata.as_release_notes_mut().mutate(|r| Ok(f(r)))?;
        metadata.as_pre_download_alert_mut().mutate(|alert| {
            if let Some(alert) = alert {
                f(&mut alert.message);
            }
            Ok(())
        })
    }
}

pub async fn get_package_index(
    ctx: RegistryContext,
    params: ReaderParams,
) -> Result<PackageIndex, Error> {
    let db = ctx.db.peek().await;
    let reader = params.reader(&db)?;
    db.as_index().as_package().de_visible_to(reader.as_ref())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn private_asset(readers: &[&Guid]) -> Value {
        let (_, mut info) = version_info_signed_by(1);
        let asset = &mut info.s9pks[0].1;
        asset.private = Some(KeyStreamInfo {
            key: crate::util::serde::Base64([1; 32]),
            iv: crate::util::serde::Base64([2; 16]),
        });
        asset.readers = readers.iter().map(|r| (*r).clone()).collect();
        to_value(&asset).unwrap()
    }

    fn reader(admin: bool, signer: Option<&Guid>, reader_ids: &[&Guid]) -> Reader {
        Reader {
            admin,
            signer: signer.cloned(),
            reader_ids: reader_ids.iter().map(|r| (*r).clone()).collect(),
        }
    }

    #[test]
    fn private_s9pks_admit_admins_publishers_and_readers() {
        let reader_id = Guid::new();
        let publisher = Guid::new();
        let (_, public) = version_info_signed_by(1);
        let public = to_value(&public).unwrap();
        let mut private = public.clone();
        private["s9pks"][0][1] = private_asset(&[&reader_id]);
        let mut mixed = public.clone();
        mixed["s9pks"] = imbl_value::json!([
            public["s9pks"][0].clone(),
            [{ "arch": ["aarch64"] }, private_asset(&[&reader_id])],
        ]);
        let mut empty = public.clone();
        empty["s9pks"] = imbl_value::json!([]);
        let index: Model<PackageIndex> = imbl_value::json!({
            "categories": {}, "packages": {
                "public": {"authorized": {}, "categories": [], "versions": {"1.0.0:0": public}},
                "mixed": {"authorized": {}, "categories": [], "versions": {
                    "1.0.0:0": mixed, "2.0.0:0": private.clone(), "3.0.0:0": empty,
                }},
                "secret": {"authorized": {publisher.as_ref(): "*"}, "categories": [],
                    "versions": {"1.0.0:0": private}},
                "unreleased": {"authorized": {}, "categories": [], "versions": {}},
            }
        })
        .into();
        let visible =
            |reader: Option<Reader>| index.de_visible_to(reader.as_ref()).unwrap().packages;
        let summary = |reader: Option<Reader>| {
            visible(reader)
                .into_iter()
                .map(|(id, package)| {
                    let versions = package
                        .versions
                        .into_iter()
                        .map(|(v, info)| {
                            let readers = info
                                .s9pks
                                .iter()
                                .map(|(_, a)| a.readers.len())
                                .collect::<Vec<_>>();
                            (v.to_string(), readers)
                        })
                        .collect::<Vec<_>>();
                    (id.to_string(), versions)
                })
                .collect::<BTreeMap<_, _>>()
        };
        let anonymous = summary(None);
        assert_eq!(
            anonymous.keys().collect::<Vec<_>>(),
            ["mixed", "public", "unreleased"]
        );
        assert_eq!(
            anonymous["mixed"],
            [("1.0.0:0".into(), vec![0]), ("3.0.0:0".into(), vec![])]
        );
        assert_eq!(
            summary(Some(reader(false, None, &[&Guid::new()]))),
            anonymous
        );
        let as_reader = summary(Some(reader(false, None, &[&reader_id])));
        assert_eq!(as_reader.len(), 4);
        assert_eq!(
            as_reader["mixed"],
            [
                ("1.0.0:0".into(), vec![0, 0]),
                ("2.0.0:0".into(), vec![0]),
                ("3.0.0:0".into(), vec![]),
            ]
        );
        let as_publisher = summary(Some(reader(false, Some(&publisher), &[])));
        assert_eq!(as_publisher["secret"], [("1.0.0:0".into(), vec![1])]);
        assert_eq!(as_publisher["mixed"], anonymous["mixed"]);
        let as_admin = summary(Some(reader(true, Some(&Guid::new()), &[])));
        assert_eq!(as_admin["secret"], [("1.0.0:0".into(), vec![1])]);
        assert_eq!(as_admin["mixed"][0].1, vec![0, 1]);

        let secret = &visible(Some(reader(false, None, &[&reader_id])))["secret"];
        assert!(
            secret.versions.values().next().unwrap().s9pks[0]
                .1
                .private
                .is_some()
        );

        assert!(
            !index
                .grants_private_access(&reader(false, None, &[&Guid::new()]))
                .unwrap()
        );
        for grantee in [
            reader(false, None, &[&reader_id]),
            reader(false, Some(&publisher), &[]),
            reader(true, Some(&Guid::new()), &[]),
        ] {
            assert!(index.grants_private_access(&grantee).unwrap());
        }
    }

    pub(crate) fn version_info_signed_by(seed: u8) -> (AnyVerifyingKey, PackageVersionInfo) {
        use ed25519_dalek::Signer;

        let key = ed25519_dalek::SigningKey::from_bytes(&[seed; 32]);
        let mut info: PackageVersionInfo = serde_json::from_value(serde_json::json!({
            "title": "Test Package",
            "description": { "short": "Short", "long": "Long" },
            "releaseNotes": "Notes",
            "gitHash": null,
            "license": "MIT",
            "upstreamRepo": "https://example.com/upstream",
            "marketingUrl": null,
            "donationUrl": null,
            "sdkVersion": null,
            "dependencyMetadata": {},
            "sourceVersion": null,
            "s9pks": [[
                { "device": [], "ram": null, "arch": ["x86_64"] },
                {
                    "publishedAt": "2026-01-01T00:00:00Z",
                    "urls": ["https://example.com/test.s9pk"],
                    "commitment": {
                        "rootSighash": "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc",
                        "rootMaxsize": 380,
                    },
                    "signatures": {},
                },
            ]],
        }))
        .unwrap();
        let verifying_key = AnyVerifyingKey::Ed25519(key.verifying_key());
        info.s9pks[0].1.signatures.insert(
            verifying_key.clone(),
            AnySignature::Ed25519ph(key.sign(b"test")),
        );
        (verifying_key, info)
    }

    #[test]
    fn merging_the_same_s9pk_adds_its_signatures() {
        let (developer, mut info) = version_info_signed_by(1);
        let (cosigner, cosigned) = version_info_signed_by(2);
        info.merge_with(cosigned, true).unwrap();

        assert_eq!(
            info.s9pks[0].1.signatures.keys().collect::<BTreeSet<_>>(),
            [&developer, &cosigner].into()
        );
    }

    #[test]
    fn re_adding_an_s9pk_replaces_its_access_and_mirroring_keeps_it() {
        let reader_id = Guid::new();
        let (_, mut stored) = version_info_signed_by(1);
        stored.s9pks[0].1 = from_value(private_asset(&[&reader_id])).unwrap();
        let (_, public) = version_info_signed_by(1);

        let (_, mut mirrored) = version_info_signed_by(1);
        mirrored.s9pks[0].1.urls = vec!["https://mirror.example.com/test.s9pk".parse().unwrap()];
        let mut info = from_value::<PackageVersionInfo>(to_value(&stored).unwrap()).unwrap();
        info.merge_with(mirrored, false).unwrap();
        assert!(info.s9pks[0].1.private.is_some());
        assert_eq!(info.s9pks[0].1.readers, [reader_id].into());
        assert_eq!(info.s9pks[0].1.urls.len(), 2);

        info.merge_with(public, true).unwrap();
        assert!(info.s9pks[0].1.private.is_none());
        assert!(info.s9pks[0].1.readers.is_empty());
    }

    pub(crate) fn signers_with(
        keys: impl IntoIterator<Item = AnyVerifyingKey>,
    ) -> BTreeMap<Guid, SignerInfo> {
        [(
            Guid::new(),
            SignerInfo {
                name: "Publisher".into(),
                contact: vec![],
                keys: keys.into_iter().collect(),
                gpg_exports: BTreeSet::new(),
            },
        )]
        .into()
    }

    #[test]
    fn unregistered_signatures_are_dropped() {
        let (publisher, mut info) = version_info_signed_by(1);
        let (_, ci_signed) = version_info_signed_by(2);
        info.merge_with(ci_signed, true).unwrap();

        info.retain_registered_signatures(&signers_with([publisher.clone()]));

        assert_eq!(
            info.s9pks[0].1.signatures.keys().collect::<BTreeSet<_>>(),
            [&publisher].into()
        );
    }

    #[test]
    fn registry_dependency_metadata_preserves_base_requirement() {
        let metadata: DependencyMetadata = serde_json::from_str(
            r#"{"title":"Bitcoin","icon":null,"description":null,"optional":false,"versionRange":">=31.1:17","kind":"running","healthChecks":["bitcoind"]}"#,
        )
        .unwrap();
        assert!(
            matches!(metadata.kind, Some(CurrentDependencyKind::Running { ref health_checks }) if health_checks.contains(&"bitcoind".parse().unwrap()))
        );
        let json = serde_json::to_value(&metadata).unwrap();
        assert_eq!(json["kind"], "running");
        assert_eq!(json["healthChecks"], serde_json::json!(["bitcoind"]));
        assert_eq!(json["versionRange"], ">=31.1:17");

        let old: DependencyMetadata = serde_json::from_str(
            r#"{"title":"Bitcoin","icon":null,"description":null,"optional":false}"#,
        )
        .unwrap();
        assert!(old.kind.is_none());
        assert!(old.version_range.is_none());
    }

    #[test]
    fn old_manifest_defaults_hardware_virtualization_to_false() {
        let metadata = PackageMetadata {
            title: "Test Package".into(),
            default_locale: None,
            description: Description {
                short: LocaleString::Translated("Short".into()),
                long: LocaleString::Translated("Long".into()),
            },
            release_notes: LocaleString::Translated("Notes".into()),
            pre_download_alert: Some(PreDownloadAlert {
                message: LocaleString::Translated("Back up before updating".into()),
                when: PreDownloadAlertWhen {
                    source_version: ">=1.0.0:0".parse().unwrap(),
                },
            }),
            git_hash: None,
            license: "MIT".into(),
            package_repo: "https://example.com/package".parse().unwrap(),
            upstream_repo: "https://example.com/upstream".parse().unwrap(),
            marketing_url: None,
            donation_url: None,
            os_version: current_version(),
            sdk_version: None,
            hardware_acceleration: false,
            userspace_filesystems: false,
            virtual_networking: false,
            hardware_virtualization: true,
            plugins: BTreeSet::new(),
            satisfies: BTreeSet::new(),
        };
        let mut old_manifest = serde_json::to_value(metadata).unwrap();
        assert_eq!(
            old_manifest["preDownloadAlert"]["message"],
            "Back up before updating"
        );
        old_manifest
            .as_object_mut()
            .unwrap()
            .remove("hardwareVirtualization");
        old_manifest
            .as_object_mut()
            .unwrap()
            .remove("preDownloadAlert");

        let parsed: PackageMetadata = serde_json::from_value(old_manifest).unwrap();

        assert!(!parsed.hardware_virtualization);
        assert!(parsed.pre_download_alert.is_none());
    }

    #[test]
    fn pre_download_alert_round_trips() {
        let alert = PreDownloadAlert {
            message: LocaleString::LanguageMap(
                [
                    ("en_US".into(), "Back up before updating".into()),
                    ("fr_FR".into(), "Sauvegardez avant la mise à jour".into()),
                ]
                .into_iter()
                .collect(),
            ),
            when: PreDownloadAlertWhen {
                source_version: ">=1.0.0:0 && <2.0.0:0".parse().unwrap(),
            },
        };
        let encoded = serde_json::to_value(&alert).unwrap();
        assert_eq!(encoded["when"]["sourceVersion"], ">=1.0.0:0 <2.0.0:0");
        assert_eq!(
            encoded["message"]["fr_FR"],
            "Sauvegardez avant la mise à jour"
        );
        let mut localized = serde_json::from_value::<PreDownloadAlert>(encoded).unwrap();
        assert_eq!(localized, alert);
        localized.message.localize_for(Some("fr_FR"), None);
        assert_eq!(
            localized.message,
            LocaleString::Translated("Sauvegardez avant la mise à jour".into())
        );
    }
}
