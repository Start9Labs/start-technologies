use std::collections::BTreeMap;
use std::sync::Arc;

use imbl::OrdMap;
use serde::{Deserialize, Serialize};

use super::credentials::Credentials;
use crate::context::config::ServerConfig;
use crate::db::model::DatabaseModel;
use crate::db::model::public::NetworkInterfaceInfo;
use crate::hostname::ServerHostname;
use crate::net::acme::{AcmeTlsHandler, BootstrapAcme};
use crate::net::dns::DnsController;
use crate::net::dns_update::DnsUpdateController;
use crate::net::forward::{AvailablePorts, InterfacePortForwardController};
use crate::net::gateway::{NetworkInterfaceWatcher, WildcardListener};
use crate::net::host::Host;
use crate::net::net_controller::{
    HostBinds, HostExposurePlan, HostExposureResources, plan_host_exposure,
};
use crate::net::port_map::PortMapController;
use crate::net::ssl::{CertBranding, LocalCaTlsHandler, LocalIssuerCache};
use crate::net::vhost::VHostController;
use crate::net::web_server::WebServerAcceptorSetter;
use crate::prelude::*;
use crate::util::sync::{SyncMutex, Watch};
use crate::{GatewayId, HostId};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BootstrapNetwork {
    pub host: Host,
    pub gateways: OrdMap<GatewayId, NetworkInterfaceInfo>,
    pub ports: AvailablePorts,
    pub wifi: crate::db::model::public::WifiInfo,
}

impl BootstrapNetwork {
    pub fn capture(db: &DatabaseModel) -> Result<Self, Error> {
        let network = db.as_public().as_server_info().as_network();
        let mut host: Host = network.as_host().de()?;
        host.bindings.retain(|port, _| *port == 80);
        host.binding_ranges.clear();
        Ok(Self {
            host,
            gateways: network.as_gateways().de()?,
            ports: db.as_private().as_available_ports().de()?,
            wifi: network.as_wifi().de()?,
        })
    }
}

pub(crate) struct UnlockNetwork {
    pub host: Watch<Model<Host>>,
    pub acme: TypedPatchDb<BootstrapAcme>,
    _watcher: NetworkInterfaceWatcher,
    interfaces: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    tls_client: Arc<tokio_rustls::rustls::ClientConfig>,
    vhost: VHostController,
    forward: InterfacePortForwardController,
    dns: Arc<DnsController>,
    dns_update: DnsUpdateController,
    binds: HostBinds,
    hostname: ServerHostname,
    ports: AvailablePorts,
    mdns: Option<InternedString>,
}

impl UnlockNetwork {
    pub async fn new(
        credentials: &mut Credentials,
        dns: Arc<DnsController>,
        webserver: &WebServerAcceptorSetter<WildcardListener>,
        config: &ServerConfig,
    ) -> Result<Self, Error> {
        let issuer = credentials
            .local_issuer
            .take()
            .ok_or_else(super::initramfs::invalid_handoff)?;
        let BootstrapNetwork {
            host,
            gateways,
            ports,
            wifi,
        } = credentials
            .network
            .take()
            .ok_or_else(super::initramfs::invalid_handoff)?;
        if !host.bindings.contains_key(&80)
            || host.bindings.keys().any(|port| *port != 80)
            || !host.binding_ranges.is_empty()
        {
            return Err(super::initramfs::invalid_handoff());
        }
        let profiles = credentials.install_network_profiles().await?;
        let system_product_name = crate::firmware::system_product_name().await.log_err();
        crate::net::wifi::synchronize_network_manager(
            profiles,
            &wifi,
            system_product_name.as_deref(),
        )
        .await?;
        let watcher = NetworkInterfaceWatcher::from_seed(gateways, []);
        webserver.send_modify(|listener| listener.set_ip_info(watcher.subscribe()));
        dns.set_bootstrap_interfaces(watcher.subscribe());
        let acme = std::mem::take(&mut credentials.acme).materialize().await?;
        let crypto_provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
        let tls_client = Arc::new(crate::net::tls::client_config(
            crypto_provider.clone(),
            [&*issuer.root_cert.0],
        )?);
        let port_map = PortMapController::new(watcher.subscribe());
        let dns_update = DnsUpdateController::from_network_manager(watcher.subscribe());
        let vhost = VHostController::with_tls_handlers(
            watcher.subscribe(),
            crypto_provider.clone(),
            config
                .max_proxy_conns_per_target
                .unwrap_or(crate::net::vhost::MAX_PROXY_CONNS_PER_TARGET),
            port_map.clone(),
            {
                let acme = acme.clone();
                let local = LocalIssuerCache::new(issuer);
                let hostname = credentials.hostname.clone();
                let acme_cache = Arc::new(SyncMutex::new(BTreeMap::new()));
                move |get_provider| {
                    (
                        Arc::new(AcmeTlsHandler {
                            db: acme.clone(),
                            acme_cache: acme_cache.clone(),
                            crypto_provider: crypto_provider.clone(),
                            get_provider,
                            in_progress: Watch::new(BTreeMap::new()),
                            report_failure: None,
                        }),
                        LocalCaTlsHandler {
                            db: local.clone(),
                            crypto_provider: crypto_provider.clone(),
                            branding: CertBranding::start_os(hostname.as_ref()),
                        },
                    )
                }
            },
        );
        let mut network = Self {
            host: Watch::new(Model::new(&host)?),
            acme,
            interfaces: watcher.subscribe(),
            tls_client,
            forward: InterfacePortForwardController::new(watcher.subscribe(), port_map),
            dns_update,
            dns,
            _watcher: watcher,
            vhost,
            binds: HostBinds::default(),
            hostname: credentials.hostname.clone(),
            ports,
            mdns: None,
        };
        network.reconcile().await?;
        Ok(network)
    }

    async fn apply(
        &mut self,
        interfaces: &OrdMap<GatewayId, NetworkInterfaceInfo>,
        plan: HostExposurePlan,
    ) -> Result<(), Error> {
        self.binds
            .reconcile(
                HostExposureResources {
                    forward: &self.forward,
                    vhost: &self.vhost,
                    dns: &self.dns,
                    dns_update: &self.dns_update,
                },
                (None, HostId::admin()),
                interfaces,
                plan,
            )
            .await
    }

    pub async fn reconcile(&mut self) -> Result<(), Error> {
        let interfaces = self.interfaces.read_and_mark_seen();
        let mut host = self.host.read();
        host.update_addresses(&self.hostname, &interfaces, &self.ports)?;
        let host_data = host.de()?;
        let plan = plan_host_exposure(
            &host_data,
            &interfaces,
            std::net::Ipv4Addr::LOCALHOST,
            None,
            |_| self.tls_client.clone(),
        );
        self.host.send_replace(host);
        self.dns_update
            .reconcile_mdns(&mut self.mdns, &self.hostname, &interfaces);
        self.apply(&interfaces, plan).await
    }

    pub async fn changed(&mut self) {
        self.interfaces.changed().await;
    }

    pub async fn retire(&mut self) -> Result<(), Error> {
        self.apply(&self.interfaces.read(), HostExposurePlan::default())
            .await?;
        self.dns_update.gc(self.mdns.take().into_iter().collect());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::host::binding::{BindInfo, BindOptions, Bindings};

    #[test]
    fn snapshot_carries_the_ui_binding_without_other_server_ports_or_private_database() {
        let account = crate::account::AccountInfo::new(
            "account-test-password",
            std::time::SystemTime::now(),
            None,
        )
        .unwrap();
        let mut db =
            Model::new(&crate::db::model::Database::init(&account, false, None, None).unwrap())
                .unwrap();
        let mut ports = AvailablePorts::new();
        let bindings = [80, 8080]
            .into_iter()
            .map(|port| {
                (
                    port,
                    BindInfo::new(
                        &mut ports,
                        BindOptions {
                            preferred_external_port: port,
                            add_ssl: None,
                            secure: None,
                            proxy: None,
                        },
                        true,
                    )
                    .unwrap(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        db.as_public_mut()
            .as_server_info_mut()
            .as_network_mut()
            .as_host_mut()
            .as_bindings_mut()
            .ser(&Bindings(bindings))
            .unwrap();
        db.as_private_mut()
            .as_available_ports_mut()
            .ser(&ports)
            .unwrap();
        let snapshot = BootstrapNetwork::capture(&db).unwrap();
        assert_eq!(
            snapshot.host.bindings.keys().copied().collect::<Vec<_>>(),
            vec![80]
        );
        let encoded = serde_json::to_value(&snapshot).unwrap();
        assert!(encoded.get("private").is_none());
        assert!(encoded.get("password").is_none());
        assert!(snapshot.host.binding_ranges.is_empty());
    }
}
