use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::model::package::{AllPackageData, PackageDataEntry};
use crate::prelude::*;
use crate::s9pk::manifest::LocaleString;
use crate::{HostId, PackageId};

mod runtime;
pub(crate) use runtime::{ChainRoute, ChainTarget, HttpProxySockets};

#[cfg(test)]
mod tests;

pub const CONTAINER_SOCKET_DIR: &str = "/media/startos/http-proxy";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, VisitVariants)]
#[serde(rename_all = "kebab-case")]
/// HTTP framing supported on the plugin's Unix stream sockets.
pub enum HttpProxyProtocol {
    Http1,
    Http2,
}

rpc_toolkit::reflect_ts!(HttpProxyProtocol);
rpc_toolkit::ts_export!(HttpProxyProtocol, namespaces = [""]);

impl HttpProxyProtocol {
    pub(crate) fn all() -> BTreeSet<Self> {
        BTreeSet::from([Self::Http1, Self::Http2])
    }

    pub fn alpn(&self) -> &'static [u8] {
        match self {
            Self::Http1 => b"http/1.1",
            Self::Http2 => b"h2",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
/// Provider-owned settings selectable in a binding's proxy chain.
pub struct HttpProxyProfile {
    pub id: InternedString,
    pub name: LocaleString,
    pub protocols: BTreeSet<HttpProxyProtocol>,
}

rpc_toolkit::reflect_ts!(HttpProxyProfile);
rpc_toolkit::ts_export!(HttpProxyProfile, namespaces = [""]);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
/// The provider's complete published profile list.
pub struct HttpProxyRegistration {
    pub profiles: Vec<HttpProxyProfile>,
}

rpc_toolkit::reflect_ts!(HttpProxyRegistration);
rpc_toolkit::ts_export!(HttpProxyRegistration, namespaces = [""]);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
/// A provider/profile pair occurring once in a binding's ordered chain.
pub struct HttpProxyStage {
    pub package_id: PackageId,
    pub profile_id: InternedString,
}

rpc_toolkit::reflect_ts!(HttpProxyStage);
rpc_toolkit::ts_export!(HttpProxyStage, namespaces = [""]);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
/// The protected binding, shared by all its exported interfaces and addresses.
pub struct HttpProxyBinding {
    pub package_id: PackageId,
    pub host_id: HostId,
    /// The container-side port passed to `bindPort`.
    pub internal_port: u16,
}

rpc_toolkit::reflect_ts!(HttpProxyBinding);
rpc_toolkit::ts_export!(HttpProxyBinding, namespaces = [""]);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, VisitVariants)]
#[serde(tag = "transport", rename_all = "camelCase")]
/// A socket path in the provider's container namespace.
pub enum HttpProxyEndpoint {
    Unix { path: PathBuf },
}

rpc_toolkit::reflect_ts!(HttpProxyEndpoint);
rpc_toolkit::ts_export!(HttpProxyEndpoint, namespaces = [""]);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
/// Identified within its provider by `binding` and `profileId`, independently of chain order.
pub struct HttpProxyAttachment {
    pub binding: HttpProxyBinding,
    pub profile_id: InternedString,
    pub ingress: HttpProxyEndpoint,
    pub next: HttpProxyEndpoint,
    pub protocols: BTreeSet<HttpProxyProtocol>,
}

rpc_toolkit::reflect_ts!(HttpProxyAttachment);
rpc_toolkit::ts_export!(HttpProxyAttachment, namespaces = [""]);

impl HttpProxyBinding {
    pub fn socket_name(&self, profile: &str) -> String {
        let mut hash = blake3::Hasher::new();
        for field in [self.package_id.as_ref(), self.host_id.as_ref(), profile] {
            hash.update(&(field.len() as u64).to_le_bytes());
            hash.update(field.as_bytes());
        }
        hash.update(&self.internal_port.to_le_bytes());
        format!("{}.sock", &hash.finalize().to_hex()[..32])
    }

    pub fn socket_path(&self, root: &Path, directory: &str, profile: &str) -> PathBuf {
        root.join(directory).join(self.socket_name(profile))
    }
}

pub fn profiles_are_valid(profiles: &[HttpProxyProfile]) -> bool {
    let mut ids = BTreeSet::new();
    profiles.iter().all(|profile| {
        !profile.id.is_empty() && !profile.protocols.is_empty() && ids.insert(&profile.id)
    })
}

pub fn chain_is_unique(chain: &[HttpProxyStage]) -> bool {
    let mut stages = BTreeSet::new();
    chain.iter().all(|stage| stages.insert(stage))
}

pub fn attachments_for(
    packages: &AllPackageData,
    provider: &PackageId,
) -> Vec<HttpProxyAttachment> {
    let mut attachments = Vec::new();
    for (package_id, package) in &packages.0 {
        for (host_id, host) in &package.hosts.0 {
            for (internal_port, bind) in &host.bindings.0 {
                if !bind.enabled || bind.options.proxy.is_none() {
                    continue;
                }
                for stage in bind
                    .proxy_chain
                    .iter()
                    .filter(|stage| &stage.package_id == provider)
                {
                    let binding = HttpProxyBinding {
                        package_id: package_id.clone(),
                        host_id: host_id.clone(),
                        internal_port: *internal_port,
                    };
                    let protocols = chain_protocols(packages, &bind.proxy_chain);
                    attachments.push(HttpProxyAttachment {
                        ingress: HttpProxyEndpoint::Unix {
                            path: binding.socket_path(
                                Path::new(CONTAINER_SOCKET_DIR),
                                "in",
                                &stage.profile_id,
                            ),
                        },
                        next: HttpProxyEndpoint::Unix {
                            path: binding.socket_path(
                                Path::new(CONTAINER_SOCKET_DIR),
                                "next",
                                &stage.profile_id,
                            ),
                        },
                        binding,
                        profile_id: stage.profile_id.clone(),
                        protocols,
                    });
                }
            }
        }
    }
    attachments
}

#[derive(Debug, Clone, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
/// Replaces the binding's operator-selected proxy chain.
pub struct SetHttpProxyChainParams {
    pub internal_port: u16,
    pub chain: Vec<HttpProxyStage>,
}

rpc_toolkit::reflect_ts!(SetHttpProxyChainParams);
rpc_toolkit::ts_export!(SetHttpProxyChainParams, namespaces = [""]);

pub async fn set_chain<Kind: crate::net::host::HostApiKind>(
    ctx: crate::context::RpcContext,
    SetHttpProxyChainParams {
        internal_port,
        chain,
    }: SetHttpProxyChainParams,
    inheritance: Kind::Inheritance,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            if !chain_is_unique(&chain) {
                return Err(Error::new(
                    eyre!("{}", t!("net.http-proxy.duplicate-stage")),
                    ErrorKind::InvalidRequest,
                ));
            }
            let packages = db.as_public().as_package_data().de()?;
            if !chain.is_empty() && chain_protocols(&packages, &chain).is_empty() {
                return Err(Error::new(
                    eyre!("{}", t!("net.http-proxy.incompatible-chain")),
                    ErrorKind::InvalidRequest,
                ));
            }
            Kind::host_for(&inheritance, db)?
                .as_bindings_mut()
                .mutate(|bindings| {
                    let binding = bindings
                        .get_mut(&internal_port)
                        .or_not_found(internal_port)?;
                    if !chain.is_empty() && binding.options.proxy.is_none() {
                        return Err(Error::new(
                            eyre!("{}", t!("net.http-proxy.requires-http")),
                            ErrorKind::InvalidRequest,
                        ));
                    }
                    binding.proxy_chain = chain;
                    Ok(())
                })
        })
        .await
        .result
}

pub fn profile_for<'a>(
    packages: &'a AllPackageData,
    stage: &HttpProxyStage,
) -> Option<&'a HttpProxyProfile> {
    registration_for(packages.0.get(&stage.package_id)?)?
        .profiles
        .iter()
        .find(|profile| profile.id == stage.profile_id)
}

fn registration_for(package: &PackageDataEntry) -> Option<&HttpProxyRegistration> {
    if !package
        .state_info
        .as_manifest(crate::db::model::package::ManifestPreference::Old)
        .metadata
        .plugins
        .contains(&crate::service::effects::plugin::PluginId::HttpProxyV0)
    {
        return None;
    }
    package.plugin.http_proxy.as_ref()
}

pub fn chain_protocols(
    packages: &AllPackageData,
    chain: &[HttpProxyStage],
) -> BTreeSet<HttpProxyProtocol> {
    common_protocols(
        chain
            .iter()
            .map(|stage| profile_for(packages, stage).map(|profile| &profile.protocols)),
    )
}

fn common_protocols<'a>(
    profiles: impl IntoIterator<Item = Option<&'a BTreeSet<HttpProxyProtocol>>>,
) -> BTreeSet<HttpProxyProtocol> {
    let mut protocols = HttpProxyProtocol::all();
    for profile in profiles {
        let Some(supported) = profile else {
            return BTreeSet::new();
        };
        protocols.retain(|protocol| supported.contains(protocol));
    }
    protocols
}
