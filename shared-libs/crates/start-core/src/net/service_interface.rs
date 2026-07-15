use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6};

use imbl_value::InternedString;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::prelude::*;
use crate::{ActionId, GatewayId, HostId, PackageId, ServiceInterfaceId};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct HostnameInfo {
    pub ssl: bool,
    pub public: bool,
    pub hostname: InternedString,
    pub port: Option<u16>,
    pub metadata: HostnameMetadata,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
#[serde(rename_all_fields = "camelCase")]
#[serde(tag = "kind")]
pub enum HostnameMetadata {
    Ipv4 {
        gateway: GatewayId,
    },
    Ipv6 {
        gateway: GatewayId,
        scope_id: u32,
    },
    Mdns {
        gateways: BTreeSet<GatewayId>,
    },
    PrivateDomain {
        gateways: BTreeSet<GatewayId>,
    },
    PublicDomain {
        gateway: GatewayId,
    },
    Plugin {
        package_id: PackageId,
        remove_action: Option<ActionId>,
        overflow_actions: Vec<ActionId>,
        #[ts(type = "unknown")]
        #[serde(default)]
        info: Value,
    },
}

impl HostnameInfo {
    pub fn to_socket_addr(&self) -> Option<SocketAddr> {
        let ip = self.hostname.parse().ok()?;
        Some(SocketAddr::new(ip, self.port?))
    }

    pub fn to_san_hostname(&self) -> InternedString {
        self.hostname.clone()
    }

    /// True for the always-on internal interfaces — loopback (`lo`) and the
    /// `lxcbr0` bridge (`HOST_IP`). These are how the host and other containers
    /// reach the service; they are never operator-disablable, and a binding with
    /// no exported interface is restricted to them.
    pub fn is_internal(&self) -> bool {
        match self.hostname.parse::<IpAddr>() {
            Ok(IpAddr::V4(v4)) => v4.is_loopback() || v4 == Ipv4Addr::from(crate::HOST_IP),
            Ok(IpAddr::V6(v6)) => v6.is_loopback(),
            Err(_) => false,
        }
    }

    /// If this address is an IPv6 **global unicast** address (a GUA — not
    /// loopback / ULA / link-local), return it as a `SocketAddrV6`. GUAs are the
    /// only addresses that carry the Disabled / LAN / LAN+WAN tri-state.
    pub fn gua(&self) -> Option<SocketAddrV6> {
        if !matches!(self.metadata, HostnameMetadata::Ipv6 { .. }) {
            return None;
        }
        let ip = self.hostname.parse::<Ipv6Addr>().ok()?;
        if crate::net::utils::ipv6_is_local(ip) {
            return None;
        }
        Some(SocketAddrV6::new(ip, self.port?, 0, 0))
    }
}

impl HostnameMetadata {
    pub fn is_ip(&self) -> bool {
        matches!(self, Self::Ipv4 { .. } | Self::Ipv6 { .. })
    }

    pub fn gateways(&self) -> Box<dyn Iterator<Item = &GatewayId> + '_> {
        match self {
            Self::Ipv4 { gateway }
            | Self::Ipv6 { gateway, .. }
            | Self::PublicDomain { gateway } => Box::new(std::iter::once(gateway)),
            Self::PrivateDomain { gateways } | Self::Mdns { gateways } => Box::new(gateways.iter()),
            Self::Plugin { .. } => Box::new(std::iter::empty()),
        }
    }
}

// TEMPORARY beta.9 compat: `packageId` was `Option`, `null`/absent meant the OS UI — map to `start-os`. Remove after beta.10.
fn deserialize_os_ui_package_id<'de, D>(deserializer: D) -> Result<PackageId, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<PackageId>::deserialize(deserializer)?.unwrap_or_else(PackageId::start_os))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PluginHostnameInfo {
    /// [`PackageId::start_os`] identifies the server's own host (the StartOS UI).
    #[serde(
        default = "PackageId::start_os",
        deserialize_with = "deserialize_os_ui_package_id"
    )]
    pub package_id: PackageId,
    pub host_id: HostId,
    pub internal_port: u16,
    pub ssl: bool,
    pub public: bool,
    #[ts(type = "string")]
    pub hostname: InternedString,
    pub port: Option<u16>,
    #[ts(type = "unknown")]
    #[serde(default)]
    pub info: Value,
}

impl PluginHostnameInfo {
    /// Convert to a `HostnameInfo` with `Plugin` metadata, using the given plugin package ID.
    pub fn to_hostname_info(
        &self,
        plugin_package: &PackageId,
        remove_action: Option<ActionId>,
        overflow_actions: Vec<ActionId>,
    ) -> HostnameInfo {
        HostnameInfo {
            ssl: self.ssl,
            public: self.public,
            hostname: self.hostname.clone(),
            port: self.port,
            metadata: HostnameMetadata::Plugin {
                package_id: plugin_package.clone(),
                info: self.info.clone(),
                remove_action,
                overflow_actions,
            },
        }
    }

    /// Check if a `HostnameInfo` with Plugin metadata matches this `PluginHostnameInfo`
    /// (comparing address fields only, not row_actions).
    pub fn matches_hostname_info(&self, h: &HostnameInfo, plugin_package: &PackageId) -> bool {
        match &h.metadata {
            HostnameMetadata::Plugin {
                package_id, info, ..
            } => {
                package_id == plugin_package
                    && h.ssl == self.ssl
                    && h.public == self.public
                    && h.hostname == self.hostname
                    && h.port == self.port
                    && *info == self.info
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct GatewayInfo {
    pub id: GatewayId,
    pub name: InternedString,
    pub public: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct ServiceInterface {
    pub id: ServiceInterfaceId,
    pub name: String,
    pub description: String,
    pub masked: bool,
    pub address_info: AddressInfo,
    #[serde(rename = "type")]
    pub interface_type: ServiceInterfaceType,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ServiceInterfaceType {
    Ui,
    P2p,
    Api,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AddressInfo {
    pub username: Option<String>,
    pub host_id: HostId,
    pub internal_port: u16,
    #[ts(type = "string | null")]
    pub scheme: Option<InternedString>,
    #[ts(type = "string | null")]
    pub ssl_scheme: Option<InternedString>,
    pub suffix: String,
}

/// The single restricted service interface a port-range binding may export.
///
/// Unlike [`ServiceInterface`], a range interface is always `api`-typed and
/// carries no `masked` / `username` / `path` / `query` and no per-address
/// [`AddressInfo`] — its address is the host plus the range's external port
/// span, taken from the [`RangeBindInfo`](crate::net::host::binding::RangeBindInfo)
/// it lives under. `scheme` is an optional transport prefix (e.g. `tcp` for
/// bitcoin ZMQ endpoints); most ranges (coturn RTP, FTP data) omit it.
#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct RangeServiceInterface {
    pub id: ServiceInterfaceId,
    pub name: String,
    pub description: String,
    #[ts(type = "string | null")]
    pub scheme: Option<InternedString>,
}
