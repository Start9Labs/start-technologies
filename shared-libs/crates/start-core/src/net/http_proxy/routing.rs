use std::collections::BTreeMap;
use std::sync::Arc;

use super::{
    ChainRoute, ChainTarget, HttpProxyBinding, HttpProxyChain, HttpProxyGateway, HttpProxySockets,
};
use crate::Error;
use crate::net::dns::DnsController;
use crate::net::request_origin::RequestOrigin;
use crate::net::vhost::ProxyTarget;

pub(crate) struct BindingRoutes {
    routes: BTreeMap<HttpProxyGateway, ChainRoute>,
    pub target: Arc<GatewayRoutes>,
}

impl std::fmt::Debug for BindingRoutes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BindingRoutes")
            .field("routes", &self.routes)
            .finish()
    }
}

struct ScopedChain {
    public_only: bool,
    target: Arc<ChainTarget>,
}

pub(crate) struct GatewayRoutes {
    dns: Arc<DnsController>,
    entries: BTreeMap<HttpProxyGateway, ScopedChain>,
}

impl GatewayRoutes {
    pub fn select(&self, origin: &RequestOrigin) -> Result<Option<Arc<ChainTarget>>, Error> {
        let source = origin
            .peer_ip
            .map(|ip| self.dns.requester(ip))
            .transpose()?
            .flatten();
        // Service-origin traffic selects its package gateway, independently of configured chains.
        let gateway = source
            .map(|package_id| HttpProxyGateway::UrlPlugin { package_id })
            .or_else(|| {
                origin
                    .gateway
                    .clone()
                    .map(|gateway_id| HttpProxyGateway::Network { gateway_id })
            });
        Ok(gateway
            .as_ref()
            .and_then(|gateway| self.entries.get(gateway))
            .filter(|entry| !entry.public_only || origin.public)
            .map(|entry| entry.target.clone()))
    }

    fn matches(&self, entries: &BTreeMap<HttpProxyGateway, ScopedChain>) -> bool {
        self.entries.len() == entries.len()
            && self.entries.iter().all(|(gateway, entry)| {
                entries.get(gateway).is_some_and(|other| {
                    entry.public_only == other.public_only
                        && Arc::ptr_eq(&entry.target, &other.target)
                })
            })
    }
}

impl BindingRoutes {
    pub fn new(dns: Arc<DnsController>) -> Self {
        Self {
            routes: BTreeMap::new(),
            target: Arc::new(GatewayRoutes {
                dns,
                entries: BTreeMap::new(),
            }),
        }
    }

    pub async fn reconcile(
        &mut self,
        sockets: &HttpProxySockets,
        binding: &HttpProxyBinding,
        policies: &[HttpProxyChain],
        upstream: &ProxyTarget,
    ) {
        self.routes.retain(|gateway, _| {
            policies
                .iter()
                .any(|policy| &policy.gateway == gateway && !policy.chain.is_empty())
        });
        let providers = sockets.providers.read();
        let mut entries = BTreeMap::new();
        for policy in policies.iter().filter(|policy| !policy.chain.is_empty()) {
            let previous = self.routes.remove(&policy.gateway);
            let route = if previous
                .as_ref()
                .is_some_and(|route| route.matches(&policy.chain, &providers, upstream))
            {
                previous.unwrap()
            } else {
                drop(previous);
                ChainRoute::new(
                    sockets,
                    binding,
                    &policy.gateway,
                    &policy.chain,
                    upstream.clone(),
                )
                .await
            };
            entries.insert(
                policy.gateway.clone(),
                ScopedChain {
                    public_only: policy.public_only,
                    target: route.target.clone(),
                },
            );
            self.routes.insert(policy.gateway.clone(), route);
        }
        if !self.target.matches(&entries) {
            self.target = Arc::new(GatewayRoutes {
                dns: self.target.dns.clone(),
                entries,
            });
        }
    }
}
