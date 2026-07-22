use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, OnceLock};

use chrono::{DateTime, Utc};
use exver::{Version, VersionRange};
use imbl::{OrdMap, OrdSet};
use imbl_value::InternedString;
use ipnet::IpNet;
use isocountry::CountryCode;
use itertools::Itertools;
use openssl::hash::MessageDigest;
use patch_db::{HasModel, Value};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use url::Url;

use crate::account::AccountInfo;
use crate::db::DbAccessByKey;
use crate::db::model::Database;
use crate::db::model::package::AllPackageData;
use crate::net::acme::AcmeProvider;
use crate::net::host::Host;
use crate::net::host::binding::{
    AddSslOptions, BindInfo, BindOptions, Bindings, DerivedAddressInfo, NetInfo,
};
use crate::net::vhost::{AlpnInfo, PassthroughInfo};
use crate::prelude::*;
use crate::progress::FullProgress;
use crate::system::{KeyboardOptions, SmtpValue};
use crate::util::cpupower::Governor;
use crate::util::lshw::LshwDevice;
use crate::util::serde::MaybeUtf8String;
use crate::version::{Current, VersionT};
use crate::{GatewayId, PLATFORM};

pub static DB_UI_SEED_CELL: OnceLock<&'static str> = OnceLock::new();

#[derive(Debug, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct Public {
    pub server_info: ServerInfo,
    pub package_data: AllPackageData,
    #[ts(type = "unknown")]
    pub ui: Value,
}
impl Public {
    pub fn init(
        account: &AccountInfo,
        kiosk: bool,
        language: Option<InternedString>,
        keyboard: Option<KeyboardOptions>,
    ) -> Result<Self, Error> {
        Ok(Self {
            server_info: ServerInfo {
                id: account.server_id.clone(),
                version: Current::default().semver(),
                name: account.hostname.name.clone(),
                hostname: (*account.hostname.hostname).clone(),
                last_backup: None,
                package_version_compat: Current::default().compat().clone(),
                post_init_migration_todos: BTreeMap::new(),
                network: NetworkInfo {
                    host: Host {
                        bindings: Bindings(
                            [(
                                80,
                                BindInfo {
                                    enabled: false,
                                    options: BindOptions {
                                        preferred_external_port: 80,
                                        add_ssl: Some(AddSslOptions {
                                            preferred_external_port: 443,
                                            add_x_forwarded_headers: false,
                                            alpn: Some(AlpnInfo::Specified(vec![
                                                MaybeUtf8String("h2".into()),
                                                MaybeUtf8String("http/1.1".into()),
                                            ])),
                                            auth: None,
                                            upstream_cert_validation: Default::default(),
                                        }),
                                        secure: None,
                                    },
                                    net: NetInfo {
                                        assigned_port: None,
                                        assigned_ssl_port: Some(443),
                                    },
                                    addresses: DerivedAddressInfo::default(),
                                    interfaces: BTreeMap::new(),
                                },
                            )]
                            .into_iter()
                            .collect(),
                        ),
                        binding_ranges: Default::default(),
                        public_domains: BTreeMap::new(),
                        private_domains: BTreeMap::new(),
                        port_forwards: BTreeSet::new(),
                    },
                    wifi: WifiInfo {
                        enabled: false,
                        ..Default::default()
                    },
                    gateways: OrdMap::new(),
                    acme: {
                        let mut acme: BTreeMap<AcmeProvider, AcmeSettings> = Default::default();
                        acme.insert(
                            "letsencrypt".parse()?,
                            AcmeSettings {
                                contact: Vec::new(),
                            },
                        );
                        #[cfg(feature = "dev")]
                        acme.insert(
                            "letsencrypt-staging".parse()?,
                            AcmeSettings {
                                contact: Vec::new(),
                            },
                        );
                        acme
                    },
                    dns: Default::default(),
                    default_outbound: None,
                    passthroughs: Vec::new(),
                },
                status_info: ServerStatus {
                    backup_progress: None,
                    update_progress: None,
                    shutting_down: false,
                    restarting: false,
                    restart: None,
                },
                unread_notification_count: 0,
                pubkey: ssh_key::PublicKey::from(&account.ssh_key)
                    .to_openssh()
                    .unwrap(),
                ca_fingerprint: account
                    .root_ca_cert
                    .digest(MessageDigest::sha256())
                    .unwrap()
                    .iter()
                    .map(|x| format!("{x:02X}"))
                    .join(":"),
                ntp_synced: false,
                zram: true,
                governor: None,
                smtp: None,
                echoip_urls: default_echoip_urls(),
                ram: 0,
                devices: Vec::new(),
                kiosk: Some(kiosk).filter(|_| &*PLATFORM != "raspberrypi"),
                language,
                keyboard,
            },
            package_data: AllPackageData::default(),
            ui: serde_json::from_str(*DB_UI_SEED_CELL.get().unwrap_or(&"null"))
                .with_kind(ErrorKind::Deserialization)?,
        })
    }
}

pub fn default_echoip_urls() -> Vec<Url> {
    vec![
        "https://ipconfig.io".parse().unwrap(),
        "https://ifconfig.co".parse().unwrap(),
    ]
}

#[derive(Debug, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct ServerInfo {
    pub id: String,
    pub name: InternedString,
    pub hostname: InternedString,
    #[ts(type = "string")]
    pub version: Version,
    #[ts(type = "string")]
    pub package_version_compat: VersionRange,
    #[ts(type = "Record<string, unknown>")]
    pub post_init_migration_todos: BTreeMap<Version, Value>,
    #[ts(type = "string | null")]
    pub last_backup: Option<DateTime<Utc>>,
    pub network: NetworkInfo,
    #[serde(default)]
    pub status_info: ServerStatus,
    #[ts(type = "number")]
    pub unread_notification_count: u64,
    pub pubkey: String,
    pub ca_fingerprint: String,
    #[serde(default)]
    pub ntp_synced: bool,
    #[serde(default)]
    pub zram: bool,
    pub governor: Option<Governor>,
    pub smtp: Option<SmtpValue>,
    #[serde(default = "default_echoip_urls")]
    #[ts(type = "string[]")]
    pub echoip_urls: Vec<Url>,
    #[ts(type = "number")]
    pub ram: u64,
    pub devices: Vec<LshwDevice>,
    pub kiosk: Option<bool>,
    pub language: Option<InternedString>,
    pub keyboard: Option<KeyboardOptions>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum RestartReason {
    Mdns,
    Language,
    Kiosk,
    Update,
}

#[derive(Debug, Default, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct NetworkInfo {
    pub wifi: WifiInfo,
    pub host: Host,
    #[ts(as = "BTreeMap::<GatewayId, NetworkInterfaceInfo>")]
    #[serde(default)]
    pub gateways: OrdMap<GatewayId, NetworkInterfaceInfo>,
    #[serde(default)]
    pub acme: BTreeMap<AcmeProvider, AcmeSettings>,
    #[serde(default)]
    pub dns: DnsSettings,
    #[serde(default)]
    #[ts(type = "string | null")]
    pub default_outbound: Option<GatewayId>,
    #[serde(default)]
    pub passthroughs: Vec<PassthroughInfo>,
}

#[derive(Debug, Default, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct DnsSettings {
    #[ts(type = "string[]")]
    pub dhcp_servers: VecDeque<SocketAddr>,
    #[ts(type = "string[] | null")]
    pub static_servers: Option<VecDeque<SocketAddr>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct NetworkInterfaceInfo {
    pub name: Option<InternedString>,
    pub secure: Option<bool>,
    pub ip_info: Option<Arc<IpInfo>>,
    // Pre-release dev DBs persisted this as `null` for auto-discovered gateways;
    // coerce absent/null to the default so those nodes still load.
    #[serde(default, rename = "type")]
    #[serde(deserialize_with = "deserialize_null_default")]
    #[ts(rename = "type")]
    pub gateway_type: GatewayType,
    #[serde(default)]
    pub port_map: GatewayPortMapCapabilities,
    /// The gateway's resolver accepted our last RFC 2136 DNS UPDATE — evidence
    /// from the update client (`net::dns_update`). A WireGuard gateway only
    /// serves the injected `<hostname>.local` while this is `supported`, so
    /// only then is the name listed on it.
    #[serde(default)]
    pub dns_update: CapabilityVerdict,
}

/// Whether the gateway reachable via this interface speaks each port-mapping
/// protocol, as last probed. Fed by the watcher's periodic probes and by
/// failure/success evidence from the port-map client, and synced to the db so a
/// chronically uncooperative gateway is visible (and skipped) instead of being
/// retried forever.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct GatewayPortMapCapabilities {
    pub pcp: CapabilityVerdict,
    pub nat_pmp: CapabilityVerdict,
    pub upnp: CapabilityVerdict,
    /// The PCP server answers ANNOUNCE with the Start9 capability marker, i.e.
    /// it honors OPTION_HOSTNAME (SNI demux).
    pub pcp_hostname: CapabilityVerdict,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct CapabilityVerdict {
    /// `None` = never probed.
    pub supported: Option<bool>,
    #[ts(type = "string | null")]
    pub at: Option<DateTime<Utc>>,
}

impl CapabilityVerdict {
    pub fn supported(supported: bool) -> Self {
        Self {
            supported: Some(supported),
            at: Some(Utc::now()),
        }
    }

    /// The verdict if it is still inside its trust window at `now` — a yes is
    /// trusted longer than a no (a gateway that once spoke a protocol rarely
    /// stops; a refusal deserves periodic re-probing).
    pub fn fresh(&self, now: DateTime<Utc>) -> Option<bool> {
        let (supported, at) = (self.supported?, self.at?);
        let ttl = if supported {
            chrono::TimeDelta::hours(1)
        } else {
            chrono::TimeDelta::minutes(5)
        };
        (now - at < ttl).then_some(supported)
    }
}

fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: Default + Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}
impl NetworkInterfaceInfo {
    pub fn secure(&self) -> bool {
        self.secure
            .unwrap_or_else(|| self.is_intrinsically_secure())
    }

    /// A WireGuard tunnel interface (e.g. a StartTunnel or StartWRT gateway).
    pub fn is_wireguard(&self) -> bool {
        matches!(
            self.ip_info.as_ref().and_then(|i| i.device_type),
            Some(NetworkInterfaceType::Wireguard)
        )
    }

    // lo and lxcbr0 (the only Loopback/Bridge interfaces on StartOS) never leave the
    // host, so insecure traffic such as plain HTTP defaults to permitted over them.
    fn is_intrinsically_secure(&self) -> bool {
        matches!(
            self.ip_info.as_ref().and_then(|i| i.device_type),
            Some(NetworkInterfaceType::Loopback | NetworkInterfaceType::Bridge)
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, TS, HasModel)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct IpInfo {
    #[ts(type = "string")]
    pub name: InternedString,
    pub scope_id: u32,
    pub device_type: Option<NetworkInterfaceType>,
    #[ts(type = "string[]")]
    pub subnets: OrdSet<IpNet>,
    #[ts(type = "string[]")]
    pub lan_ip: OrdSet<IpAddr>,
    pub wan_ip: Option<Ipv4Addr>,
    #[ts(type = "string[]")]
    pub ntp_servers: OrdSet<InternedString>,
    #[ts(type = "string[]")]
    pub dns_servers: OrdSet<IpAddr>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkInterfaceType {
    Ethernet,
    Wireless,
    Bridge,
    Wireguard,
    Loopback,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Deserialize,
    Serialize,
    TS,
    clap::ValueEnum,
)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum GatewayType {
    #[default]
    InboundOutbound,
    OutboundOnly,
}

#[derive(Debug, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct AcmeSettings {
    pub contact: Vec<String>,
}
impl DbAccessByKey<AcmeSettings> for Database {
    type Key<'a> = &'a AcmeProvider;
    fn access_by_key<'a>(
        db: &'a Model<Self>,
        key: Self::Key<'_>,
    ) -> Option<&'a Model<AcmeSettings>> {
        db.as_public()
            .as_server_info()
            .as_network()
            .as_acme()
            .as_idx(key)
    }
}

#[derive(Debug, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct DomainSettings {
    pub gateway: GatewayId,
}

#[derive(Debug, Default, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct ServerStatus {
    pub backup_progress: Option<FullProgress>,
    pub update_progress: Option<FullProgress>,
    #[serde(default)]
    pub shutting_down: bool,
    #[serde(default)]
    pub restarting: bool,
    #[serde(default)]
    pub restart: Option<RestartReason>,
}

#[derive(Debug, Default, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct WifiInfo {
    pub enabled: bool,
    pub interface: Option<GatewayId>,
    pub ssids: BTreeSet<String>,
    pub selected: Option<String>,
    #[ts(type = "string | null")]
    pub last_region: Option<CountryCode>,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ServerSpecs {
    pub cpu: String,
    pub disk: String,
    pub memory: String,
}

#[cfg(test)]
mod test {
    use super::*;

    fn gateway_type_of(type_field: serde_json::Value) -> GatewayType {
        serde_json::from_value::<NetworkInterfaceInfo>(serde_json::json!({ "type": type_field }))
            .unwrap()
            .gateway_type
    }

    #[test]
    fn gateway_type_defaults_and_tolerates_legacy_null() {
        // Absent `type` (fresh installs / interfaces older than the field) -> default.
        assert_eq!(
            serde_json::from_value::<NetworkInterfaceInfo>(serde_json::json!({}))
                .unwrap()
                .gateway_type,
            GatewayType::InboundOutbound
        );
        // Pre-release dev DBs persisted `type: null` for auto-discovered gateways;
        // it must still load (else the whole db fails to deserialize on boot).
        assert_eq!(
            gateway_type_of(serde_json::Value::Null),
            GatewayType::InboundOutbound
        );
        assert_eq!(
            gateway_type_of(serde_json::json!("inbound-outbound")),
            GatewayType::InboundOutbound
        );
        assert_eq!(
            gateway_type_of(serde_json::json!("outbound-only")),
            GatewayType::OutboundOnly
        );
    }
}
