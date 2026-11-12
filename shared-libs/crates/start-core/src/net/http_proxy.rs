use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::model::package::{AllPackageData, PackageDataEntry};
use crate::prelude::*;
use crate::s9pk::manifest::LocaleString;
use crate::{GatewayId, HostId, PackageId};

mod routing;
mod runtime;
pub(crate) use routing::{BindingRoutes, GatewayRoutes};
pub(crate) use runtime::{ChainRoute, ChainTarget, HttpProxyMounts, HttpProxySockets};

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
/// A provider/profile pair occurring once in a gateway's ordered chain.
pub struct HttpProxyStage {
    pub package_id: PackageId,
    pub profile_id: InternedString,
}

rpc_toolkit::reflect_ts!(HttpProxyStage);
rpc_toolkit::ts_export!(HttpProxyStage, namespaces = [""]);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
/// The protected binding, shared by its exported interfaces.
pub struct HttpProxyBinding {
    pub package_id: PackageId,
    pub host_id: HostId,
    /// The container-side port passed to `bindPort`.
    pub internal_port: u16,
}

rpc_toolkit::reflect_ts!(HttpProxyBinding);
rpc_toolkit::ts_export!(HttpProxyBinding, namespaces = [""]);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, VisitVariants)]
#[serde(tag = "kind", rename_all = "kebab-case")]
/// The ingress gateway, independent of chain order and public-only applicability.
pub enum HttpProxyGateway {
    Network {
        #[serde(rename = "gatewayId")]
        gateway_id: GatewayId,
    },
    UrlPlugin {
        #[serde(rename = "packageId")]
        package_id: PackageId,
    },
}

rpc_toolkit::reflect_ts!(HttpProxyGateway);
rpc_toolkit::ts_export!(HttpProxyGateway, namespaces = [""]);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
/// Operator intent for one binding × gateway. Missing policies have empty chains.
pub struct HttpProxyChain {
    pub gateway: HttpProxyGateway,
    pub chain: Vec<HttpProxyStage>,
    /// Bypasses private clients. URL-plugin gateways require false.
    #[serde(default)]
    pub public_only: bool,
}

rpc_toolkit::reflect_ts!(HttpProxyChain);
rpc_toolkit::ts_export!(HttpProxyChain, namespaces = [""]);

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
/// Identified within its provider by binding, gateway and profile, independently of chain order.
pub struct HttpProxyAttachment {
    pub binding: HttpProxyBinding,
    pub gateway: HttpProxyGateway,
    pub profile_id: InternedString,
    pub ingress: HttpProxyEndpoint,
    pub next: HttpProxyEndpoint,
    pub protocols: BTreeSet<HttpProxyProtocol>,
}

rpc_toolkit::reflect_ts!(HttpProxyAttachment);
rpc_toolkit::ts_export!(HttpProxyAttachment, namespaces = [""]);

impl HttpProxyBinding {
    pub fn socket_name(&self, gateway: &HttpProxyGateway, profile: &str) -> String {
        let mut hash = blake3::Hasher::new();
        for field in [self.package_id.as_ref(), self.host_id.as_ref(), profile] {
            hash.update(&(field.len() as u64).to_le_bytes());
            hash.update(field.as_bytes());
        }
        let (kind, id) = match gateway {
            HttpProxyGateway::Network { gateway_id } => {
                (b"network".as_slice(), gateway_id.as_str())
            }
            HttpProxyGateway::UrlPlugin { package_id } => {
                (b"url-plugin".as_slice(), package_id.as_ref())
            }
        };
        hash.update(kind);
        hash.update(&(id.len() as u64).to_le_bytes());
        hash.update(id.as_bytes());
        hash.update(&self.internal_port.to_le_bytes());
        format!("{}.sock", &hash.finalize().to_hex()[..32])
    }

    pub fn socket_path(
        &self,
        root: &Path,
        directory: &str,
        gateway: &HttpProxyGateway,
        profile: &str,
    ) -> PathBuf {
        root.join(directory)
            .join(self.socket_name(gateway, profile))
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
            attachments.extend(host_attachments(
                packages, provider, package_id, host_id, host,
            ));
        }
    }
    attachments
}

pub fn host_attachments(
    packages: &AllPackageData,
    provider: &PackageId,
    package_id: &PackageId,
    host_id: &HostId,
    host: &crate::net::host::Host,
) -> Vec<HttpProxyAttachment> {
    let mut attachments = Vec::new();
    for (internal_port, bind) in &host.bindings.0 {
        if !bind.enabled || bind.options.proxy.is_none() {
            continue;
        }
        for policy in &bind.proxy_chains {
            for stage in policy
                .chain
                .iter()
                .filter(|stage| &stage.package_id == provider)
            {
                let binding = HttpProxyBinding {
                    package_id: package_id.clone(),
                    host_id: host_id.clone(),
                    internal_port: *internal_port,
                };
                let protocols = chain_protocols(packages, &policy.chain);
                attachments.push(HttpProxyAttachment {
                    ingress: HttpProxyEndpoint::Unix {
                        path: binding.socket_path(
                            Path::new(CONTAINER_SOCKET_DIR),
                            "in",
                            &policy.gateway,
                            &stage.profile_id,
                        ),
                    },
                    next: HttpProxyEndpoint::Unix {
                        path: binding.socket_path(
                            Path::new(CONTAINER_SOCKET_DIR),
                            "next",
                            &policy.gateway,
                            &stage.profile_id,
                        ),
                    },
                    binding,
                    gateway: policy.gateway.clone(),
                    profile_id: stage.profile_id.clone(),
                    protocols,
                });
            }
        }
    }
    attachments
}

#[derive(Debug, Clone, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
/// Replaces one gateway's chain without changing the binding's other gateways.
pub struct SetHttpProxyChainParams {
    pub internal_port: u16,
    pub policy: HttpProxyChain,
}

rpc_toolkit::reflect_ts!(SetHttpProxyChainParams);
rpc_toolkit::ts_export!(SetHttpProxyChainParams, namespaces = [""]);

pub async fn set_chain<Kind: crate::net::host::HostApiKind>(
    ctx: crate::context::RpcContext,
    SetHttpProxyChainParams {
        internal_port,
        policy,
    }: SetHttpProxyChainParams,
    inheritance: Kind::Inheritance,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            let packages = db.as_public().as_package_data().de()?;
            Kind::host_for(&inheritance, db)?
                .as_bindings_mut()
                .mutate(|bindings| {
                    let binding = bindings
                        .get_mut(&internal_port)
                        .or_not_found(internal_port)?;
                    replace_chain(binding, &packages, policy)
                })
        })
        .await
        .result
}

fn replace_chain(
    binding: &mut crate::net::host::binding::BindInfo,
    packages: &AllPackageData,
    policy: HttpProxyChain,
) -> Result<(), Error> {
    let invalid =
        if policy.public_only && matches!(policy.gateway, HttpProxyGateway::UrlPlugin { .. }) {
            Some(t!("net.http-proxy.public-only-network"))
        } else if !chain_is_unique(&policy.chain) {
            Some(t!("net.http-proxy.duplicate-stage"))
        } else if !policy.chain.is_empty() && binding.options.proxy.is_none() {
            Some(t!("net.http-proxy.requires-http"))
        } else if !policy.chain.is_empty() && chain_protocols(packages, &policy.chain).is_empty() {
            Some(t!("net.http-proxy.incompatible-chain"))
        } else {
            None
        };
    if let Some(message) = invalid {
        return Err(Error::new(eyre!("{message}"), ErrorKind::InvalidRequest));
    }
    binding
        .proxy_chains
        .retain(|entry| entry.gateway != policy.gateway);
    binding.proxy_chains.push(policy);
    binding
        .proxy_chains
        .sort_by(|a, b| a.gateway.cmp(&b.gateway));
    Ok(())
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
