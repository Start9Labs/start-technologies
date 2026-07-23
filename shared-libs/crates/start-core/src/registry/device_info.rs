use std::collections::BTreeMap;
use std::ops::Deref;

use axum::extract::Request;
use axum::response::Response;
use exver::{Version, VersionRange};
use http::HeaderValue;
use imbl_value::InternedString;
use patch_db::ModelExt;
use rpc_toolkit::yajrc::RpcMethod;
use rpc_toolkit::{Middleware, RpcRequest, RpcResponse};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use url::Url;

use crate::context::RpcContext;
use crate::prelude::*;
use crate::registry::context::RegistryContext;
use crate::registry::os::index::OsVersionInfoMap;
use crate::registry::package::get::{
    GetPackageParams, GetPackageResponse, GetPackageResponseFull, PackageDetailLevel,
};
use crate::registry::package::index::PackageVersionInfo;
use crate::util::VersionString;
use crate::util::lshw::LshwDevice;
use crate::version::VersionT;

pub const DEVICE_INFO_HEADER: &str = "X-StartOS-Device-Info";

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub os: OsInfo,
    pub hardware: Option<HardwareInfo>,
}
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

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OsInfo {
    #[ts(as = "VersionString")]
    pub version: Version,
    #[ts(type = "string")]
    pub compat: VersionRange,
    pub platform: InternedString,
    pub language: Option<InternedString>,
}
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

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    #[ts(type = "string")]
    pub arch: InternedString,
    #[ts(type = "number")]
    pub ram: u64,
    pub devices: Option<Vec<LshwDevice>>,
}
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
}
impl DeviceInfoMiddleware {
    pub fn new() -> Self {
        Self {
            device_info_header: None,
            device_info: None,
            req: None,
        }
    }
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
        _: &RegistryContext,
        metadata: Self::Metadata,
        request: &mut RpcRequest,
    ) -> Result<(), RpcResponse> {
        async move {
            if metadata.get_device_info {
                if let Some(device_info) = &self.device_info_header {
                    let device_info = DeviceInfo::from_header_value(device_info)?;
                    request.params["__DeviceInfo_device_info"] = to_value(&device_info)?;
                    self.device_info = Some(device_info);
                    self.req = Some(request.clone());
                }
            }

            Ok::<_, Error>(())
        }
        .await
        .map_err(|e| RpcResponse::from_result(Err(e)))
    }
    async fn process_rpc_response(
        &mut self,
        _: &RegistryContext,
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
    }
}
