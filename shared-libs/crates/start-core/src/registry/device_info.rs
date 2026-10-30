use std::collections::BTreeMap;
use std::ops::Deref;
use std::path::Path;

use axum::extract::Request;
use axum::response::Response;
use exver::{Version, VersionRange};
use http::HeaderValue;
use imbl_value::InternedString;
use patch_db::ModelExt;
use rpc_toolkit::yajrc::RpcMethod;
use rpc_toolkit::{Middleware, RpcRequest, RpcResponse};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::context::RpcContext;
use crate::prelude::*;
use crate::registry::context::RegistryContext;
use crate::registry::os::index::OsVersionInfoMap;
use crate::registry::os::version::GetOsVersionParams;
use crate::registry::package::get::{
    GetPackageParams, GetPackageResponse, GetPackageResponseFull, PackageDetailLevel,
};
use crate::registry::package::icon::{icons_dir, load_icon};
use crate::registry::package::index::PackageVersionInfo;
use crate::s9pk::manifest::LocaleString;
use crate::util::VersionString;
use crate::util::lshw::LshwDevice;
use crate::version::VersionT;

pub const DEVICE_INFO_HEADER: &str = "X-StartOS-Device-Info";

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub os: OsInfo,
    pub hardware: Option<HardwareInfo>,
}

rpc_toolkit::reflect_ts!(DeviceInfo);
impl DeviceInfo {
    pub async fn load(ctx: &RpcContext) -> Result<Self, Error> {
        Ok(Self {
            os: OsInfo::from(ctx),
            hardware: Some(HardwareInfo::load(ctx).await?),
        })
    }
}
impl DeviceInfo {
    pub fn to_header_value(&self) -> HeaderValue {
        let mut url: Url = "http://localhost".parse().unwrap();
        let mut qp = url.query_pairs_mut();
        qp.append_pair("os.version", &self.os.version.to_string())
            .append_pair("os.compat", &self.os.compat.to_string())
            .append_pair("os.platform", &*self.os.platform);
        if let Some(lang) = self.os.language.as_deref() {
            qp.append_pair("os.language", lang);
        }
        drop(qp);

        HeaderValue::from_str(url.query().unwrap_or_default()).unwrap()
    }
    pub fn from_header_value(header: &HeaderValue) -> Result<Self, Error> {
        let query: BTreeMap<_, _> = form_urlencoded::parse(header.as_bytes()).collect();
        let has_hw_info = query.keys().any(|k| k.starts_with("hardware."));
        let version = query
            .get("os.version")
            .or_not_found("os.version")?
            .parse()?;
        Ok(Self {
            os: OsInfo {
                compat: query.get("os.compat").or_not_found("os.compat")?.parse()?,
                platform: query
                    .get("os.platform")
                    .or_not_found("os.platform")?
                    .deref()
                    .into(),
                language: query
                    .get("os.language")
                    .map(|v| v.deref())
                    .map(InternedString::intern),
                version,
            },
            hardware: has_hw_info
                .then(|| {
                    Ok::<_, Error>(HardwareInfo {
                        arch: query
                            .get("hardware.arch")
                            .or_not_found("hardware.arch")?
                            .parse()?,
                        ram: query
                            .get("hardware.ram")
                            .or_not_found("hardware.ram")?
                            .parse()?,
                        devices: None,
                    })
                })
                .transpose()?,
        })
    }
    pub fn filter_for_hardware(
        &self,
        method: &str,
        params: Value,
        res: &mut Value,
    ) -> Result<(), Error> {
        match method {
            "package.get" => {
                let params: Model<GetPackageParams> = ModelExt::from_value(params);

                let other = params.as_other_versions().de()?;
                if params.as_id().transpose_ref().is_some() {
                    if other.unwrap_or_default() == PackageDetailLevel::Full {
                        self.filter_package_get_full(ModelExt::value_as_mut(res))?;
                    } else {
                        self.filter_package_get(ModelExt::value_as_mut(res))?;
                    }
                } else {
                    for (_, v) in res.as_object_mut().into_iter().flat_map(|o| o.iter_mut()) {
                        if other.unwrap_or_default() == PackageDetailLevel::Full {
                            self.filter_package_get_full(ModelExt::value_as_mut(v))?;
                        } else {
                            self.filter_package_get(ModelExt::value_as_mut(v))?;
                        }
                    }
                }

                Ok(())
            }
            "os.version.get" => self.filter_os_version(ModelExt::value_as_mut(res)),
            _ => Ok(()),
        }
    }

    fn filter_package_versions(
        &self,
        versions: &mut Model<BTreeMap<VersionString, PackageVersionInfo>>,
    ) -> Result<(), Error> {
        let beta_10: Version = "0.4.0-beta.10".parse()?;

        versions.retain(|_, info| info.for_device(self))?;

        // beta.10 dropped the manifest `alerts` field; older clients require it present.
        if self.os.version < beta_10 {
            for (_, info) in versions.as_entries_mut()? {
                info.as_value_mut()["alerts"] = imbl_value::json!({});
            }
        }

        Ok(())
    }

    fn filter_package_get(&self, res: &mut Model<GetPackageResponse>) -> Result<(), Error> {
        self.filter_package_versions(res.as_best_mut())
    }

    fn filter_package_get_full(
        &self,
        res: &mut Model<GetPackageResponseFull>,
    ) -> Result<(), Error> {
        self.filter_package_versions(res.as_best_mut())?;
        self.filter_package_versions(res.as_other_versions_mut())
    }

    fn filter_os_version(&self, res: &mut Model<OsVersionInfoMap>) -> Result<(), Error> {
        res.retain(|_, info| {
            let source_version = info.as_source_version().de()?;
            Ok(self.os.version.satisfies(&source_version))
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct OsInfo {
    #[visit(wire = "VersionString")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub version: Version,
    pub compat: VersionRange,
    pub platform: InternedString,
    pub language: Option<InternedString>,
}

rpc_toolkit::reflect_ts!(OsInfo);
impl From<&RpcContext> for OsInfo {
    fn from(_: &RpcContext) -> Self {
        Self {
            version: crate::version::Current::default().semver(),
            compat: crate::version::Current::default().compat().clone(),
            platform: InternedString::intern(&*crate::PLATFORM),
            language: Some(InternedString::intern(&*rust_i18n::locale())),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    pub arch: InternedString,
    pub ram: u64,
    pub devices: Option<Vec<LshwDevice>>,
}

rpc_toolkit::reflect_ts!(HardwareInfo);
impl HardwareInfo {
    pub async fn load(ctx: &RpcContext) -> Result<Self, Error> {
        let s = ctx.db.peek().await.into_public().into_server_info();
        Ok(Self {
            arch: InternedString::intern(&*crate::ARCH),
            ram: s.as_ram().de()?,
            devices: Some(s.as_devices().de()?),
        })
    }
}

#[derive(Deserialize)]
pub struct Metadata {
    #[serde(default)]
    get_device_info: bool,
}

#[derive(Clone)]
pub struct DeviceInfoMiddleware {
    device_info_header: Option<HeaderValue>,
    device_info: Option<DeviceInfo>,
    req: Option<RpcRequest>,
    legacy_signatures: bool,
    legacy_pem: bool,
    legacy_os_versions: bool,
    inline_icons: bool,
    legacy_os_notes: bool,
}
impl DeviceInfoMiddleware {
    pub fn new() -> Self {
        Self {
            device_info_header: None,
            device_info: None,
            req: None,
            legacy_signatures: false,
            legacy_pem: true,
            legacy_os_versions: false,
            inline_icons: false,
            legacy_os_notes: false,
        }
    }

    fn prepare_request(
        &mut self,
        startwrt: bool,
        metadata: Metadata,
        request: &mut RpcRequest,
    ) -> Result<(), Error> {
        self.legacy_signatures = startwrt;
        self.legacy_pem = true;
        self.legacy_os_notes = false;
        if let Some(header) = &self.device_info_header {
            let device_info = if metadata.get_device_info {
                Some(DeviceInfo::from_header_value(header)?)
            } else {
                DeviceInfo::from_header_value(header).ok()
            };
            if let Some(device_info) = device_info {
                self.legacy_signatures |= reads_legacy_signatures(&device_info.os.version)?;
                self.legacy_pem = startwrt || predates_0_4_0_5(&device_info.os.version)?;
                self.legacy_os_notes = self.legacy_pem;
                self.inline_icons = reads_inline_icons(&device_info.os.version)?;
                if metadata.get_device_info {
                    request.params["__DeviceInfo_device_info"] = to_value(&device_info)?;
                    self.device_info = Some(device_info);
                }
            }
        } else if request.method.as_str() == "os.version.get" {
            let params: GetOsVersionParams = from_value(request.params.clone())?;
            if let Some(version) = params.source_version {
                self.legacy_signatures |= reads_legacy_signatures(&version)?;
                self.legacy_pem = startwrt || predates_0_4_0_5(&version)?;
                self.legacy_os_notes = self.legacy_pem;
            }
        }
        if request.method.as_str().starts_with("db.") {
            self.legacy_pem = false;
            self.legacy_signatures = false;
        }
        self.legacy_os_versions =
            !startwrt && self.legacy_signatures && request.method.as_str() == "os.version.get";
        self.req = Some(request.clone());
        Ok(())
    }
}

fn predates_0_4_0_5(version: &Version) -> Result<bool, Error> {
    Ok(version < &"0.4.0.5".parse::<Version>()?)
}

fn project_legacy_os_notes(method: &str, res: &mut Value) -> Result<(), Error> {
    let versions = match method {
        "os.version.get" => res,
        "os.index" => &mut res["versions"],
        "index" => &mut res["os"]["versions"],
        _ => return Ok(()),
    };
    for (_, info) in versions
        .as_object_mut()
        .into_iter()
        .flat_map(|o| o.iter_mut())
    {
        let notes: LocaleString = from_value(info["releaseNotes"].clone())?;
        info["releaseNotes"] = Value::String(notes.localized_for("en_US").into());
    }
    Ok(())
}

fn reads_legacy_signatures(version: &Version) -> Result<bool, Error> {
    Ok(version < &"0.4.0.3".parse::<Version>()?)
}

/// Whether a StartOS version needs icons inline as data URLs.
fn reads_inline_icons(version: &Version) -> Result<bool, Error> {
    Ok(version < &"0.4.0.3".parse::<Version>()?)
}

/// Replaces stored icon names with their data URLs in a response carrying package versions.
pub async fn inline_icons(
    dir: &Path,
    method: &str,
    params: &Value,
    res: &mut Value,
) -> Result<(), Error> {
    let infos: Vec<&mut Value> = match method {
        "package.get" => {
            let responses: Vec<&mut Value> = if params["id"].is_null() {
                res.as_object_mut()
                    .into_iter()
                    .flat_map(|o| o.iter_mut().map(|(_, v)| v))
                    .collect()
            } else {
                vec![res]
            };
            responses
                .into_iter()
                .filter_map(|r| r.as_object_mut())
                .flat_map(|r| {
                    r.iter_mut()
                        .filter(|(k, _)| matches!(&**k, "best" | "otherVersions"))
                        .map(|(_, v)| v)
                })
                .filter_map(|v| v.as_object_mut())
                .flat_map(|v| v.iter_mut().map(|(_, info)| info))
                .collect()
        }
        "package.index" => package_index_versions(res),
        "index" => package_index_versions(&mut res["package"]),
        _ => return Ok(()),
    };
    let mut slots: Vec<&mut Value> = Vec::new();
    for info in infos.into_iter().filter_map(|i| i.as_object_mut()) {
        for (k, v) in info.iter_mut() {
            match &**k {
                "icon" => slots.push(v),
                "dependencyMetadata" => slots.extend(
                    v.as_object_mut()
                        .into_iter()
                        .flat_map(|d| d.iter_mut())
                        .filter_map(|(_, dep)| dep.as_object_mut())
                        .flat_map(|dep| dep.iter_mut().filter(|(k, _)| &**k == "icon"))
                        .map(|(_, icon)| icon),
                ),
                _ => (),
            }
        }
    }
    let mut loaded: BTreeMap<InternedString, Value> = BTreeMap::new();
    for slot in slots {
        let Some(path) = slot.as_str().filter(|s| !s.starts_with("data:")) else {
            continue;
        };
        let path = InternedString::intern(path);
        let data_url = match loaded.get(&path) {
            Some(data_url) => data_url.clone(),
            None => {
                let data_url = Value::String(load_icon(dir, &path).await?.into());
                loaded.insert(path, data_url.clone());
                data_url
            }
        };
        *slot = data_url;
    }
    Ok(())
}

fn package_index_versions(index: &mut Value) -> Vec<&mut Value> {
    index["packages"]
        .as_object_mut()
        .into_iter()
        .flat_map(|p| p.iter_mut())
        .filter_map(|(_, package)| package["versions"].as_object_mut())
        .flat_map(|v| v.iter_mut().map(|(_, info)| info))
        .collect()
}

impl Middleware<RegistryContext> for DeviceInfoMiddleware {
    type Metadata = Metadata;
    async fn process_http_request(
        &mut self,
        _: &RegistryContext,
        request: &mut Request,
    ) -> Result<(), Response> {
        self.device_info_header = request.headers_mut().remove(DEVICE_INFO_HEADER);
        Ok(())
    }
    async fn process_rpc_request(
        &mut self,
        ctx: &RegistryContext,
        metadata: Self::Metadata,
        request: &mut RpcRequest,
    ) -> Result<(), RpcResponse> {
        self.prepare_request(ctx.startwrt, metadata, request)
            .map_err(|e| RpcResponse::from_result(Err(e)))
    }
    async fn process_rpc_response(
        &mut self,
        ctx: &RegistryContext,
        response: &mut RpcResponse,
    ) -> () {
        if let (Some(req), Some(device_info), Ok(res)) =
            (&self.req, &self.device_info, &mut response.result)
        {
            if let Err(e) =
                device_info.filter_for_hardware(req.method.as_str(), req.params.clone(), res)
            {
                response.result = Err(e).map_err(From::from);
            }
        }
        if let (true, Some(req), Ok(res)) = (self.inline_icons, &self.req, &mut response.result) {
            if let Err(e) = inline_icons(
                &icons_dir(&ctx.datadir),
                req.method.as_str(),
                &req.params,
                res,
            )
            .await
            {
                response.result = Err(e).map_err(From::from);
            }
        }
        if let (true, Some(req), Ok(res)) = (self.legacy_os_notes, &self.req, &mut response.result)
        {
            if let Err(e) = project_legacy_os_notes(req.method.as_str(), res) {
                response.result = Err(e).map_err(From::from);
            }
        }
        if let (true, Ok(res)) = (self.legacy_signatures, &mut response.result) {
            crate::sign::compat::downgrade_signatures(res);
            if self.legacy_os_versions {
                drop_unsigned_os_versions(res);
            }
        }
        if let (true, Ok(res)) = (self.legacy_pem, &mut response.result) {
            super::compat::pem_wire(res);
        }
    }
}

fn drop_unsigned_os_versions(versions: &mut Value) {
    if let Some(versions) = versions.as_object_mut() {
        versions.retain(|_, info| {
            ["iso", "squashfs", "img"].iter().all(|kind| {
                info[*kind].as_object().map_or(true, |assets| {
                    assets.values().all(|asset| {
                        asset["signatures"]
                            .as_object()
                            .is_some_and(|sigs| !sigs.is_empty())
                    })
                })
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use imbl_value::json;

    use super::*;

    fn request(method: &str, params: Value) -> RpcRequest {
        from_value(json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }))
            .unwrap()
    }

    #[test]
    fn list_and_single_package_queries_select_signers_for_the_device_asset() {
        use crate::registry::asset::RegistryAsset;
        use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;

        let mut arm: RegistryAsset<MerkleArchiveCommitment> = serde_json::from_str(include_str!(
            "../install/fixtures/production-bitcoind-31.1-26.json"
        ))
        .unwrap();
        let keyrings = crate::trusted_keys::initial_trusted_keys();
        let alpha = arm
            .signatures
            .keys()
            .find(|key| keyrings["start9-alpha"].public_keys.contains(key))
            .unwrap()
            .clone();
        let mut x86 = arm.clone();
        assert!(x86.signatures.remove(&alpha).is_some());
        arm.signatures.retain(|key, _| key == &alpha);
        let response = json!({"best": {"31.1:26": {
            "osVersion": "0.4.0",
            "s9pks": [[{"arch": ["aarch64"]}, arm], [{"arch": ["x86_64"]}, x86]],
        }}});
        for (arch, expected) in [("x86_64", x86), ("aarch64", arm)] {
            let device = DeviceInfo {
                os: OsInfo {
                    version: "0.4.1".parse().unwrap(),
                    compat: "*".parse().unwrap(),
                    platform: arch.into(),
                    language: None,
                },
                hardware: Some(HardwareInfo {
                    arch: arch.into(),
                    ram: 8_000_000_000,
                    devices: Some(vec![]),
                }),
            };
            for id in [Some("bitcoind"), None] {
                let mut filtered = if id.is_some() {
                    response.clone()
                } else {
                    json!({"bitcoind": response})
                };
                device
                    .filter_for_hardware("package.get", json!({"id": id}), &mut filtered)
                    .unwrap();
                let package = if id.is_some() {
                    &filtered
                } else {
                    &filtered["bitcoind"]
                };
                let assets = package["best"]["31.1:26"]["s9pks"].as_array().unwrap();
                assert_eq!(assets.len(), 1);
                assert_eq!(
                    assets[0][1]["signatures"],
                    to_value(&expected.signatures).unwrap()
                );
            }
        }
    }

    #[test]
    fn pem_responses_follow_client_capability() {
        for (header, startwrt, legacy) in [
            (None, false, true),
            (
                Some("os.version=0.4.0.4&os.compat=*&os.platform=x86_64"),
                false,
                true,
            ),
            (
                Some("os.version=0.4.0.5&os.compat=*&os.platform=x86_64"),
                false,
                false,
            ),
            (
                Some("os.version=0.4.0.5&os.compat=*&os.platform=x86_64"),
                true,
                true,
            ),
        ] {
            let mut middleware = DeviceInfoMiddleware::new();
            middleware.device_info_header = header.map(HeaderValue::from_static);
            middleware
                .prepare_request(
                    startwrt,
                    Metadata {
                        get_device_info: false,
                    },
                    &mut request("index", json!({})),
                )
                .unwrap();
            assert_eq!(middleware.legacy_pem, legacy);
        }
        for (version, legacy) in [("0.4.0.4", true), ("0.4.0.5", false)] {
            let mut middleware = DeviceInfoMiddleware::new();
            middleware
                .prepare_request(
                    false,
                    Metadata {
                        get_device_info: false,
                    },
                    &mut request("os.version.get", json!({ "sourceVersion": version })),
                )
                .unwrap();
            assert_eq!(middleware.legacy_pem, legacy);
        }
    }

    #[test]
    fn raw_database_responses_keep_the_stored_encoding() {
        for method in ["db.dump", "db.subscribe"] {
            let mut middleware = DeviceInfoMiddleware::new();
            middleware
                .prepare_request(
                    true,
                    Metadata {
                        get_device_info: false,
                    },
                    &mut request(method, json!({})),
                )
                .unwrap();
            assert!(!middleware.legacy_pem);
            assert!(!middleware.legacy_signatures);
        }
    }

    #[test]
    fn os_note_projection_follows_client_version_not_language() {
        for (header, source, startwrt, legacy) in [
            (None, None, false, false),
            (None, Some("0.4.0.4"), false, true),
            (None, Some("0.4.0.5"), false, false),
            (
                Some("os.version=0.4.0.4&os.compat=*&os.platform=x86_64&os.language=pl_PL"),
                None,
                false,
                true,
            ),
            (
                Some("os.version=0.4.0.5&os.compat=*&os.platform=x86_64&os.language=pl_PL"),
                Some("0.4.0.4"),
                false,
                false,
            ),
            (None, None, true, false),
            (None, Some("0.1.0"), true, true),
            (
                Some("os.version=0.1.0&os.compat=*&os.platform=spacemit,k1-x_muse-pi-pro"),
                None,
                true,
                true,
            ),
        ] {
            let mut middleware = DeviceInfoMiddleware::new();
            middleware.device_info_header = header.map(HeaderValue::from_static);
            middleware
                .prepare_request(
                    startwrt,
                    Metadata {
                        get_device_info: true,
                    },
                    &mut request("os.version.get", json!({ "sourceVersion": source })),
                )
                .unwrap();
            assert_eq!(
                middleware.legacy_os_notes, legacy,
                "{:?} {:?}",
                header, source
            );
        }
    }

    #[test]
    fn headerless_index_queries_preserve_maps_for_cli_promotion() {
        for startwrt in [false, true] {
            for method in ["index", "os.index", "os.version.get"] {
                let mut middleware = DeviceInfoMiddleware::new();
                middleware
                    .prepare_request(
                        startwrt,
                        Metadata {
                            get_device_info: false,
                        },
                        &mut request(method, json!({})),
                    )
                    .unwrap();
                assert!(!middleware.legacy_os_notes);
            }
        }
    }

    #[test]
    fn legacy_notes_project_every_os_response_without_changing_stored_maps() {
        let stored = json!({
            "0.4.0.5": {
                "releaseNotes": { "pl_PL": "Polskie informacje", "en_US": "English notes" },
                "headline": "A release",
            },
            "0.4.0.4": { "releaseNotes": "Legacy notes" },
        });
        for method in ["os.version.get", "os.index", "index"] {
            let mut response = match method {
                "os.version.get" => stored.clone(),
                "os.index" => json!({ "versions": stored }),
                _ => json!({ "os": { "versions": stored }, "package": { "unchanged": true } }),
            };
            project_legacy_os_notes(method, &mut response).unwrap();
            let versions = match method {
                "os.version.get" => &response,
                "os.index" => &response["versions"],
                _ => {
                    assert_eq!(response["package"], json!({ "unchanged": true }));
                    &response["os"]["versions"]
                }
            };
            assert_eq!(
                versions["0.4.0.5"]["releaseNotes"].as_str(),
                Some("English notes")
            );
            assert_eq!(versions["0.4.0.5"]["headline"].as_str(), Some("A release"));
            assert_eq!(
                versions["0.4.0.4"]["releaseNotes"].as_str(),
                Some("Legacy notes")
            );
            assert!(stored["0.4.0.5"]["releaseNotes"].is_object());
        }
        let mut unrelated = stored.clone();
        project_legacy_os_notes("package.index", &mut unrelated).unwrap();
        assert_eq!(unrelated, stored);
    }

    #[test]
    fn diagnostic_update_signatures_follow_source_version_without_device_info() {
        for (version, legacy) in [("0.4.0.2", true), ("0.4.0.3", false), ("0.4.0.4", false)] {
            let mut middleware = DeviceInfoMiddleware::new();
            middleware
                .prepare_request(
                    false,
                    Metadata {
                        get_device_info: true,
                    },
                    &mut request("os.version.get", json!({ "sourceVersion": version })),
                )
                .unwrap();
            assert_eq!(middleware.legacy_signatures, legacy, "{version}");
            assert_eq!(middleware.legacy_os_versions, legacy, "{version}");
        }
    }

    #[test]
    fn headerless_registry_queries_keep_modern_labels_unless_startwrt() {
        for startwrt in [false, true] {
            for (method, params) in [
                ("os.version.get", json!({})),
                ("package.get", json!({ "sourceVersion": "0.4.0.2" })),
            ] {
                let mut middleware = DeviceInfoMiddleware::new();
                middleware
                    .prepare_request(
                        startwrt,
                        Metadata {
                            get_device_info: true,
                        },
                        &mut request(method, params),
                    )
                    .unwrap();
                assert_eq!(middleware.legacy_signatures, startwrt);
            }
        }
    }

    #[test]
    fn legacy_os_version_filter_follows_device_version_and_method() {
        for (header, method, expected) in [
            (
                "os.version=0.4.0.2&os.compat=*&os.platform=x86_64",
                "os.version.get",
                true,
            ),
            (
                "os.version=0.4.0.3&os.compat=*&os.platform=x86_64",
                "os.version.get",
                false,
            ),
            (
                "os.version=0.4.0.2&os.compat=*&os.platform=x86_64",
                "package.get",
                false,
            ),
        ] {
            let mut middleware = DeviceInfoMiddleware::new();
            middleware.device_info_header = Some(HeaderValue::from_static(header));
            middleware
                .prepare_request(
                    false,
                    Metadata {
                        get_device_info: false,
                    },
                    &mut request(method, json!({})),
                )
                .unwrap();
            assert_eq!(middleware.legacy_os_versions, expected, "{header} {method}");
        }
    }

    #[test]
    fn startwrt_keeps_os_versions_without_signatures() {
        for header in [
            None,
            Some("os.version=0.1.0&os.compat=^0.1.0&os.platform=spacemit,k1-x_muse-pi-pro"),
        ] {
            for params in [json!({}), json!({ "sourceVersion": "0.1.0" })] {
                let mut middleware = DeviceInfoMiddleware::new();
                middleware.device_info_header = header.map(HeaderValue::from_static);
                middleware
                    .prepare_request(
                        true,
                        Metadata {
                            get_device_info: false,
                        },
                        &mut request("os.version.get", params),
                    )
                    .unwrap();
                assert!(middleware.legacy_signatures);
                assert!(!middleware.legacy_os_versions);
            }
        }
    }

    #[test]
    fn drops_os_versions_with_an_unsigned_asset() {
        let signed = json!({ "signatures": { "signer": "sig" } });
        let unsigned = json!({ "signatures": {} });
        let mut res = json!({
            "0.4.0.3": { "squashfs": { "x86_64": signed }, "iso": { "x86_64": signed }, "img": {} },
            "0.4.0.4": { "squashfs": { "x86_64": signed, "aarch64": unsigned }, "iso": {}, "img": {} },
        });
        drop_unsigned_os_versions(&mut res);
        assert_eq!(
            res.as_object()
                .unwrap()
                .keys()
                .map(|k| &**k)
                .collect::<Vec<_>>(),
            ["0.4.0.3"]
        );
    }

    #[test]
    fn device_info_version_takes_precedence_over_update_source() {
        let mut middleware = DeviceInfoMiddleware::new();
        middleware.device_info_header = Some(HeaderValue::from_static(
            "os.version=0.4.0.4&os.compat=*&os.platform=x86_64",
        ));
        middleware
            .prepare_request(
                false,
                Metadata {
                    get_device_info: true,
                },
                &mut request("os.version.get", json!({ "sourceVersion": "0.4.0.2" })),
            )
            .unwrap();
        assert!(!middleware.legacy_signatures);
        assert!(middleware.device_info.is_some());
    }

    fn stored(dir: &Path) -> (Value, Value) {
        let icon = crate::util::DataUrl::from_vec("image/png", b"icon".to_vec());
        let stored = crate::registry::package::icon::store_icon(dir, &icon).unwrap();
        (to_value(&stored).unwrap(), to_value(&icon).unwrap())
    }

    fn versions(name: &Value) -> Value {
        json!({
            "1.0.0:0": {
                "icon": name,
                "dependencyMetadata": { "dep": { "icon": name }, "bare": { "title": null } },
            },
        })
    }

    #[tokio::test]
    async fn inlines_icons_for_every_method_carrying_package_versions() {
        let dir = tempfile::tempdir().unwrap();
        let (name, data_url) = stored(dir.path());
        let inlined = |info: &Value| {
            assert_eq!(info["icon"], data_url);
            assert_eq!(info["dependencyMetadata"]["dep"]["icon"], data_url);
            assert!(info["dependencyMetadata"]["bare"].get("icon").is_none());
        };

        let mut one = json!({ "best": versions(&name), "otherVersions": versions(&name) });
        inline_icons(dir.path(), "package.get", &json!({ "id": "pkg" }), &mut one)
            .await
            .unwrap();
        inlined(&one["best"]["1.0.0:0"]);
        inlined(&one["otherVersions"]["1.0.0:0"]);

        let mut all = json!({ "pkg": { "best": versions(&name) } });
        inline_icons(dir.path(), "package.get", &json!({ "id": null }), &mut all)
            .await
            .unwrap();
        inlined(&all["pkg"]["best"]["1.0.0:0"]);

        let index = json!({ "packages": { "pkg": { "versions": versions(&name) } } });
        let mut package_index = index.clone();
        inline_icons(dir.path(), "package.index", &json!({}), &mut package_index)
            .await
            .unwrap();
        inlined(&package_index["packages"]["pkg"]["versions"]["1.0.0:0"]);

        let mut full = json!({ "package": index });
        inline_icons(dir.path(), "index", &json!({}), &mut full)
            .await
            .unwrap();
        inlined(&full["package"]["packages"]["pkg"]["versions"]["1.0.0:0"]);
    }

    #[tokio::test]
    async fn other_methods_are_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let (name, _) = stored(dir.path());
        let mut res = json!({ "icon": name });
        let before = res.clone();
        inline_icons(dir.path(), "os.version.get", &json!({}), &mut res)
            .await
            .unwrap();
        assert_eq!(res, before);
    }

    #[test]
    fn only_startos_before_0_4_0_3_needs_inline_icons() {
        let prepared = |header: Option<&'static str>, method: &str| {
            let mut middleware = DeviceInfoMiddleware::new();
            middleware.device_info_header = header.map(HeaderValue::from_static);
            middleware
                .prepare_request(
                    false,
                    Metadata {
                        get_device_info: false,
                    },
                    &mut request(method, json!({})),
                )
                .unwrap();
            middleware.inline_icons
        };
        const LEGACY: &str = "os.version=0.4.0.2&os.compat=*&os.platform=x86_64";
        const CURRENT: &str = "os.version=0.4.0.3&os.compat=*&os.platform=x86_64";
        assert!(prepared(Some(LEGACY), "package.index"));
        assert!(!prepared(Some(CURRENT), "package.index"));
        assert!(!prepared(None, "package.index"));
    }
}
