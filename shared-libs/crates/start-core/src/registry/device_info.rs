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
    inline_icons_for: Option<RpcRequest>,
}
impl DeviceInfoMiddleware {
    pub fn new() -> Self {
        Self {
            device_info_header: None,
            device_info: None,
            req: None,
            legacy_signatures: false,
            inline_icons_for: None,
        }
    }

    fn prepare_request(
        &mut self,
        startwrt: bool,
        metadata: Metadata,
        request: &mut RpcRequest,
    ) -> Result<(), Error> {
        self.legacy_signatures = startwrt;
        if let Some(header) = &self.device_info_header {
            let device_info = if metadata.get_device_info {
                Some(DeviceInfo::from_header_value(header)?)
            } else {
                DeviceInfo::from_header_value(header).ok()
            };
            if let Some(device_info) = device_info {
                self.legacy_signatures |= reads_legacy_signatures(&device_info.os.version)?;
                if reads_inline_icons(&device_info.os.version)? {
                    self.inline_icons_for = Some(request.clone());
                }
                if metadata.get_device_info {
                    request.params["__DeviceInfo_device_info"] = to_value(&device_info)?;
                    self.device_info = Some(device_info);
                    self.req = Some(request.clone());
                }
            }
        } else if request.method.as_str() == "os.version.get" {
            let params: GetOsVersionParams = from_value(request.params.clone())?;
            if let Some(version) = params.source_version {
                self.legacy_signatures |= reads_legacy_signatures(&version)?;
            }
        }
        Ok(())
    }
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
        if let (Some(req), Ok(res)) = (&self.inline_icons_for, &mut response.result) {
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
        if let (true, Ok(res)) = (self.legacy_signatures, &mut response.result) {
            crate::sign::compat::downgrade_signatures(res);
        }
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
            middleware.inline_icons_for.is_some()
        };
        const LEGACY: &str = "os.version=0.4.0.2&os.compat=*&os.platform=x86_64";
        const CURRENT: &str = "os.version=0.4.0.3&os.compat=*&os.platform=x86_64";
        assert!(prepared(Some(LEGACY), "package.index"));
        assert!(!prepared(Some(CURRENT), "package.index"));
        assert!(!prepared(None, "package.index"));
    }
}
