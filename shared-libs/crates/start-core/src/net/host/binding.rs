use std::collections::{BTreeMap, BTreeSet};
use std::net::{SocketAddr, SocketAddrV6};
use std::str::FromStr;

use clap::Parser;
use clap::builder::ValueParserFactory;
use rpc_toolkit::{Context, Empty, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::context::{CliContext, RpcContext};
use crate::db::prelude::Map;
use crate::hostname::ServerHostname;
use crate::net::forward::AvailablePorts;
use crate::net::host::HostApiKind;
use crate::net::service_interface::{
    HostnameInfo, HostnameMetadata, RangeServiceInterface, ServiceInterface,
};
use crate::net::vhost::AlpnInfo;
use crate::prelude::*;
use crate::util::FromStrParser;
use crate::util::serde::{CliFromJsonString, HandlerExtSerde, display_serializable};
use crate::{HostId, ServiceInterfaceId};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct BindId {
    pub id: HostId,
    pub internal_port: u16,
}
impl ValueParserFactory for BindId {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        FromStrParser::new()
    }
}
impl FromStr for BindId {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (id, port) = s
            .split_once(":")
            .ok_or_else(|| Error::new(eyre!("expected <id>:<port>"), ErrorKind::ParseUrl))?;
        Ok(Self {
            id: id.parse()?,
            internal_port: port.parse()?,
        })
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, TS, HasModel)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
#[model = "Model<Self>"]
pub struct DerivedAddressInfo {
    /// User override: enable these addresses (only for public IP & port)
    pub enabled: BTreeSet<SocketAddr>,
    /// User override: disable these addresses (only for domains and private IP & port)
    pub disabled: BTreeSet<(InternedString, u16)>,
    /// User override: IPv6 global-unicast addresses opted into WAN exposure.
    /// Projected into `HostnameInfo.public` by `update_addresses`, so `public`
    /// stays the single source of truth for WAN reachability (P2P address
    /// selection, upstream pinholes). A GUA not in this set is LAN-only.
    #[serde(default)]
    pub gua_wan: BTreeSet<SocketAddrV6>,
    /// COMPUTED: NetServiceData::update — all possible addresses for this binding
    pub available: BTreeSet<HostnameInfo>,
}

impl DerivedAddressInfo {
    /// Returns addresses that are currently enabled after applying overrides.
    /// Default: public IPs (including WAN-exposed GUAs) are opt-in via `enabled`,
    /// everything else is on unless in `disabled`.
    pub fn enabled(&self) -> BTreeSet<&HostnameInfo> {
        self.available
            .iter()
            .filter(|h| {
                if h.is_internal() {
                    // lo / lxcbr0 are always reachable and never operator-disablable.
                    true
                } else if h.public && h.metadata.is_ip() {
                    // Public IPs: disabled by default, explicitly enabled via SocketAddr
                    h.to_socket_addr().map_or(
                        true, // should never happen, but would rather see them if it does
                        |sa| self.enabled.contains(&sa),
                    )
                } else {
                    !self
                        .disabled
                        .contains(&(h.hostname.clone(), h.port.unwrap_or_default())) // disablable addresses will always have a port
                }
            })
            .collect()
    }

    /// Move the port of every `enabled`/`disabled` override from `old` to `new`.
    /// A range keys all its overrides on `external_start_port`, so a service-
    /// driven resize that moves the span must carry the overrides with it — else
    /// a disabled WAN address silently re-enables under the recomputed defaults.
    pub fn rekey_port(&mut self, old: u16, new: u16) {
        if old == new {
            return;
        }
        self.enabled = std::mem::take(&mut self.enabled)
            .into_iter()
            .map(|mut sa| {
                if sa.port() == old {
                    sa.set_port(new);
                }
                sa
            })
            .collect();
        self.disabled = std::mem::take(&mut self.disabled)
            .into_iter()
            .map(|(h, p)| (h, if p == old { new } else { p }))
            .collect();
        self.gua_wan = std::mem::take(&mut self.gua_wan)
            .into_iter()
            .map(|mut sa| {
                if sa.port() == old {
                    sa.set_port(new);
                }
                sa
            })
            .collect();
    }
}

#[derive(Debug, Default, Deserialize, Serialize, HasModel, TS)]
#[model = "Model<Self>"]
#[ts(export)]
pub struct Bindings(pub BTreeMap<u16, BindInfo>);

impl Map for Bindings {
    type Key = u16;
    type Value = BindInfo;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Self::key_string(key)
    }
    fn key_string(key: &Self::Key) -> Result<InternedString, Error> {
        Ok(InternedString::from_display(key))
    }
}

impl std::ops::Deref for Bindings {
    type Target = BTreeMap<u16, BindInfo>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Bindings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Contiguous port-range binding (e.g. WebRTC/STUN/TURN RTP ranges).
///
/// Keyed by `internal_start_port` in [`BindingRanges`]. The range covers
/// `internal_start_port..(internal_start_port + number_of_ports)` and is
/// forwarded through a single iptables rule per protocol per gateway,
/// preserving the destination port number.
#[derive(Debug, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct RangeBindInfo {
    pub enabled: bool,
    pub external_start_port: u16,
    pub number_of_ports: u16,
    /// Reachable addresses for this range (LAN IPv4 / WAN IPv4 / mDNS /
    /// domains) with per-address enabled/disabled overrides — the same model as
    /// a single-port binding, but IPv4-only and non-SSL. COMPUTED by
    /// `update_addresses`; every entry uses `external_start_port` as its
    /// representative port. Public IPs are disabled by default (WAN is opt-in);
    /// LAN/mDNS/domains are enabled by default.
    #[serde(default)]
    pub addresses: DerivedAddressInfo,
    /// The single restricted `api` interface exported from this range, if any
    /// (`RangeOrigin.export`). Preserved across idempotent re-binds.
    #[serde(default)]
    pub interface: Option<RangeServiceInterface>,
}

#[derive(Debug, Default, Deserialize, Serialize, HasModel, TS)]
#[model = "Model<Self>"]
#[ts(export)]
pub struct BindingRanges(pub BTreeMap<u16, RangeBindInfo>);

impl Map for BindingRanges {
    type Key = u16;
    type Value = RangeBindInfo;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Self::key_string(key)
    }
    fn key_string(key: &Self::Key) -> Result<InternedString, Error> {
        Ok(InternedString::from_display(key))
    }
}

impl std::ops::Deref for BindingRanges {
    type Target = BTreeMap<u16, RangeBindInfo>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for BindingRanges {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl RangeBindInfo {
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Addresses actually served by this range. Analogous to
    /// [`BindInfo::enabled_addresses`]: a range with no exported interface is
    /// internal-only (lo / lxcbr0), its per-address overrides dormant.
    pub fn enabled_addresses(&self) -> BTreeSet<&HostnameInfo> {
        let enabled = self.addresses.enabled();
        if self.interface.is_none() {
            enabled.into_iter().filter(|a| a.is_internal()).collect()
        } else {
            enabled
        }
    }
}

#[derive(Debug, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct BindInfo {
    pub enabled: bool,
    pub options: BindOptions,
    pub net: NetInfo,
    pub addresses: DerivedAddressInfo,
    /// Service interfaces exported from this binding (`Origin.export`). A single
    /// binding (host + internal port) may back several interfaces (e.g. a `ui`
    /// and an `api` on the same port), so this is keyed by interface id.
    #[serde(default)]
    pub interfaces: BTreeMap<ServiceInterfaceId, ServiceInterface>,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NetInfo {
    pub assigned_port: Option<u16>,
    pub assigned_ssl_port: Option<u16>,
}
impl BindInfo {
    /// Addresses actually served by this binding. A binding with no exported
    /// service interface listens internally only (lo / lxcbr0) — the operator's
    /// per-address `enabled`/`disabled` overrides stay stored but dormant until
    /// an interface is exported, at which point they take effect again.
    pub fn enabled_addresses(&self) -> BTreeSet<&HostnameInfo> {
        let enabled = self.addresses.enabled();
        if self.interfaces.is_empty() {
            enabled.into_iter().filter(|a| a.is_internal()).collect()
        } else {
            enabled
        }
    }

    pub fn new(available_ports: &mut AvailablePorts, options: BindOptions) -> Result<Self, Error> {
        let mut assigned_port = None;
        let mut assigned_ssl_port = None;
        if let Some(ssl) = &options.add_ssl {
            assigned_ssl_port = available_ports
                .try_alloc(ssl.preferred_external_port, true)
                .or_else(|| Some(available_ports.alloc(true).ok()?));
        }
        if options
            .secure
            .map_or(true, |s| !(s.ssl && options.add_ssl.is_some()))
        {
            assigned_port = available_ports
                .try_alloc(options.preferred_external_port, false)
                .or_else(|| Some(available_ports.alloc(false).ok()?));
        }

        Ok(Self {
            enabled: true,
            options,
            net: NetInfo {
                assigned_port,
                assigned_ssl_port,
            },
            addresses: DerivedAddressInfo::default(),
            interfaces: BTreeMap::new(),
        })
    }
    pub fn update(
        self,
        available_ports: &mut AvailablePorts,
        options: BindOptions,
    ) -> Result<Self, Error> {
        let Self {
            net: mut lan,
            addresses,
            interfaces,
            ..
        } = self;
        if options
            .secure
            .map_or(true, |s| !(s.ssl && options.add_ssl.is_some()))
        // doesn't make sense to have 2 listening ports, both with ssl
        {
            lan.assigned_port = if let Some(port) = lan.assigned_port.take() {
                Some(port)
            } else if let Some(port) =
                available_ports.try_alloc(options.preferred_external_port, false)
            {
                Some(port)
            } else {
                Some(available_ports.alloc(false)?)
            };
        } else {
            if let Some(port) = lan.assigned_port.take() {
                available_ports.free([port]);
            }
        }
        if let Some(ssl) = &options.add_ssl {
            lan.assigned_ssl_port = if let Some(port) = lan.assigned_ssl_port.take() {
                Some(port)
            } else if let Some(port) = available_ports.try_alloc(ssl.preferred_external_port, true)
            {
                Some(port)
            } else {
                Some(available_ports.alloc(true)?)
            };
        } else {
            if let Some(port) = lan.assigned_ssl_port.take() {
                available_ports.free([port]);
            }
        }
        Ok(Self {
            enabled: true,
            options,
            net: lan,
            addresses,
            interfaces,
        })
    }
    pub fn disable(&mut self) {
        self.enabled = false;
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Security {
    pub ssl: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BindOptions {
    pub preferred_external_port: u16,
    pub add_ssl: Option<AddSslOptions>,
    pub secure: Option<Security>,
}

/// How the OS reverse proxy validates the container's TLS certificate when it
/// rewraps SSL (`add_ssl` set AND `secure.ssl == true`, so the OS terminates
/// the client's TLS and initiates a fresh TLS connection to the container).
/// Absent (`None`) means validate against the StartOS root CA — the default.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum UpstreamCertValidation {
    /// Do not validate the container's certificate at all. Use when the
    /// container serves a self-signed cert on the trusted internal bridge.
    Disable,
    /// Validate against this PEM-encoded certificate (or chain) instead of the
    /// StartOS root CA.
    Certificate(String),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AddSslOptions {
    pub preferred_external_port: u16,
    /// When `true`, the OS reverse proxy adds `X-Forwarded-Proto: https`
    /// and `X-Forwarded-For: <client-ip>` to incoming HTTP requests before
    /// forwarding them upstream. Setting this implies HTTP-aware proxying.
    #[serde(default)]
    pub add_x_forwarded_headers: bool,
    pub alpn: Option<AlpnInfo>,
    /// Certificate validation for the OS→container TLS leg when rewrapping.
    /// `None` (the default) validates against the StartOS root CA.
    #[serde(default)]
    #[ts(optional)]
    pub upstream_cert_validation: Option<UpstreamCertValidation>,
    /// Optional reverse-proxy auth gate. When set, the OS reverse proxy
    /// will validate the `Authorization` header on incoming HTTP requests
    /// against this configuration before forwarding them upstream.
    /// Unauthenticated requests get `401 Unauthorized` with an appropriate
    /// `WWW-Authenticate` challenge. For `Basic`, the authenticated
    /// username is forwarded to the upstream service as `X-Forwarded-User`.
    /// Setting this implies HTTP-aware proxying.
    #[serde(default)]
    pub auth: Option<ProxyAuth>,
}

/// Auth gate enforced by the OS reverse proxy on incoming requests.
///
/// - `Bearer { tokens }`: any of `tokens` is accepted as `Authorization: Bearer <token>`.
/// - `Basic  { credentials }`: any `(username, password)` pair in `credentials` is
///   accepted as `Authorization: Basic <base64(username:password)>`. The matched
///   `username` is forwarded upstream as `X-Forwarded-User`.
///
/// `realm` is the authentication realm advertised in the
/// `WWW-Authenticate` challenge sent on 401 responses (RFC 7235
/// §2.2). Defaults to `"StartOS"` when unset. Packages that share
/// credentials across multiple bindings should pick a stable realm
/// so that browsers reuse cached credentials across them.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase", tag = "type")]
#[ts(export)]
pub enum ProxyAuth {
    Bearer {
        tokens: Vec<String>,
        #[serde(default)]
        realm: Option<String>,
    },
    Basic {
        credentials: Vec<BasicCredential>,
        #[serde(default)]
        realm: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BasicCredential {
    pub username: String,
    pub password: String,
}

pub fn binding<C: Context, Kind: HostApiKind>()
-> ParentHandler<C, Kind::Params, Kind::InheritedParams> {
    ParentHandler::<C, Kind::Params, Kind::InheritedParams>::new()
        .subcommand(
            "list",
            from_fn_async(list_bindings::<Kind>)
                .with_inherited(Kind::inheritance)
                .with_display_serializable()
                .with_custom_display_fn(|HandlerArgs { params, .. }, res| {
                    use prettytable::*;

                    if let Some(format) = params.format {
                        return display_serializable(format, res);
                    }

                    let mut table = Table::new();
                    table.add_row(row![bc => "INTERNAL PORT", "ENABLED", "EXTERNAL PORT", "EXTERNAL SSL PORT"]);
                    for (internal, info) in res.iter() {
                        table.add_row(row![
                            internal,
                            info.enabled,
                            if let Some(port) = info.net.assigned_port {
                                port.to_string()
                            } else {
                                "N/A".to_owned()
                            },
                            if let Some(port) = info.net.assigned_ssl_port {
                                port.to_string()
                            } else {
                                "N/A".to_owned()
                            },
                        ]);
                    }

                    table.print_tty(false)?;

                    Ok(())
                })
                .with_about("about.list-bindings-for-host")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-address-enabled",
            from_fn_async(set_address_enabled::<Kind>)
                .with_metadata("sync_db", Value::Bool(true))
                .with_inherited(Kind::inheritance)
                .no_display()
                .with_about("about.set-address-enabled-for-binding")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-range-address-enabled",
            from_fn_async(set_range_address_enabled::<Kind>)
                .with_metadata("sync_db", Value::Bool(true))
                .with_inherited(Kind::inheritance)
                .no_display()
                .with_about("about.set-range-address-enabled-for-binding")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-gua-wan",
            from_fn_async(set_gua_wan::<Kind>)
                .with_metadata("sync_db", Value::Bool(true))
                .with_inherited(Kind::inheritance)
                .no_display()
                .with_about("about.set-gua-wan-for-binding")
                .with_call_remote::<CliContext>(),
        )
}

pub async fn list_bindings<Kind: HostApiKind>(
    ctx: RpcContext,
    _: Empty,
    inheritance: Kind::Inheritance,
) -> Result<Bindings, Error> {
    Kind::host_for(&inheritance, &mut ctx.db.peek().await)?
        .as_bindings()
        .de()
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BindingSetAddressEnabledParams {
    #[arg(help = "help.arg.internal-port")]
    internal_port: u16,
    #[arg(long, help = "help.arg.address")]
    #[ts(as = "HostnameInfo")]
    address: CliFromJsonString<HostnameInfo>,
    #[arg(long, help = "help.arg.binding-enabled")]
    enabled: Option<bool>,
}

/// Toggle one address on/off for a binding's `DerivedAddressInfo`. Public IPs
/// live in the `enabled` set (keyed by `SocketAddr`); domains and private IPs
/// live in the `disabled` set (keyed by `(hostname, port)`). Non-SSL Ipv4 ↔
/// PublicDomain on the same gateway+port are cascaded so they toggle together.
/// Shared by single-port bindings and port ranges (whose addresses all use
/// `external_start_port` as their port, so the same keying applies).
fn set_address_enabled_on(
    addresses: &mut DerivedAddressInfo,
    address: &HostnameInfo,
    enabled: bool,
) -> Result<(), Error> {
    if address.public && address.metadata.is_ip() {
        // Public IPs: toggle via SocketAddr in `enabled` set
        let sa = address.to_socket_addr().ok_or_else(|| {
            Error::new(
                eyre!("cannot convert address to socket addr"),
                ErrorKind::InvalidRequest,
            )
        })?;
        if enabled {
            addresses.enabled.insert(sa);
        } else {
            addresses.enabled.remove(&sa);
        }
        // Non-SSL Ipv4: cascade to PublicDomains on same gateway
        if !address.ssl {
            if let HostnameMetadata::Ipv4 { gateway } = &address.metadata {
                let port = sa.port();
                for a in &addresses.available {
                    if a.ssl {
                        continue;
                    }
                    if let HostnameMetadata::PublicDomain { gateway: gw } = &a.metadata {
                        if gw == gateway && a.port.unwrap_or(80) == port {
                            let k = (a.hostname.clone(), a.port.unwrap_or(80));
                            if enabled {
                                addresses.disabled.remove(&k);
                            } else {
                                addresses.disabled.insert(k);
                            }
                        }
                    }
                }
            }
        }
    } else {
        // Domains and private IPs: toggle via (host, port) in `disabled` set
        let port = address.port.unwrap_or(if address.ssl { 443 } else { 80 });
        let key = (address.hostname.clone(), port);
        if enabled {
            addresses.disabled.remove(&key);
        } else {
            addresses.disabled.insert(key);
        }
        // Non-SSL PublicDomain: cascade to Ipv4 + other PublicDomains on same gateway
        if !address.ssl {
            if let HostnameMetadata::PublicDomain { gateway } = &address.metadata {
                for a in &addresses.available {
                    if a.ssl {
                        continue;
                    }
                    match &a.metadata {
                        HostnameMetadata::Ipv4 { gateway: gw } if a.public && gw == gateway => {
                            if let Some(sa) = a.to_socket_addr() {
                                if sa.port() == port {
                                    if enabled {
                                        addresses.enabled.insert(sa);
                                    } else {
                                        addresses.enabled.remove(&sa);
                                    }
                                }
                            }
                        }
                        HostnameMetadata::PublicDomain { gateway: gw } if gw == gateway => {
                            let dp = a.port.unwrap_or(80);
                            if dp == port {
                                let k = (a.hostname.clone(), dp);
                                if enabled {
                                    addresses.disabled.remove(&k);
                                } else {
                                    addresses.disabled.insert(k);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(())
}

/// Toggle one address of a single-port binding (keyed by its internal port).
/// Port ranges use [`set_range_address_enabled`] — they live in a separate DB
/// subtree, so the API distinguishes the two rather than probing both.
pub async fn set_address_enabled<Kind: HostApiKind>(
    ctx: RpcContext,
    BindingSetAddressEnabledParams {
        internal_port,
        address,
        enabled,
    }: BindingSetAddressEnabledParams,
    inheritance: Kind::Inheritance,
) -> Result<(), Error> {
    let enabled = enabled.unwrap_or(true);
    let address = address.0;
    if !enabled && address.is_internal() {
        return Err(Error::new(
            eyre!("loopback / bridge (internal) addresses cannot be disabled"),
            ErrorKind::InvalidRequest,
        ));
    }
    ctx.db
        .mutate(|db| {
            Kind::host_for(&inheritance, db)?
                .as_bindings_mut()
                .mutate(|b| {
                    let bind = b.get_mut(&internal_port).or_not_found(internal_port)?;
                    set_address_enabled_on(&mut bind.addresses, &address, enabled)
                })?;
            let hostname = ServerHostname::load(db.as_public().as_server_info())?;
            let gateways = db
                .as_public()
                .as_server_info()
                .as_network()
                .as_gateways()
                .de()?;
            let ports = db.as_private().as_available_ports().de()?;
            Kind::host_for(&inheritance, db)?.update_addresses(&hostname, &gateways, &ports)
        })
        .await
        .result?;
    Ok(())
}

/// Toggle one address of a port-range binding (keyed by its internal start
/// port). The range counterpart of [`set_address_enabled`]; both share the
/// address-toggle logic in [`set_address_enabled_on`].
pub async fn set_range_address_enabled<Kind: HostApiKind>(
    ctx: RpcContext,
    BindingSetAddressEnabledParams {
        internal_port,
        address,
        enabled,
    }: BindingSetAddressEnabledParams,
    inheritance: Kind::Inheritance,
) -> Result<(), Error> {
    let enabled = enabled.unwrap_or(true);
    let address = address.0;
    if !enabled && address.is_internal() {
        return Err(Error::new(
            eyre!("loopback / bridge (internal) addresses cannot be disabled"),
            ErrorKind::InvalidRequest,
        ));
    }
    ctx.db
        .mutate(|db| {
            Kind::host_for(&inheritance, db)?
                .as_binding_ranges_mut()
                .mutate(|ranges| {
                    let range = ranges.get_mut(&internal_port).or_not_found(internal_port)?;
                    set_address_enabled_on(&mut range.addresses, &address, enabled)
                })?;
            let hostname = ServerHostname::load(db.as_public().as_server_info())?;
            let gateways = db
                .as_public()
                .as_server_info()
                .as_network()
                .as_gateways()
                .de()?;
            let ports = db.as_private().as_available_ports().de()?;
            Kind::host_for(&inheritance, db)?.update_addresses(&hostname, &gateways, &ports)
        })
        .await
        .result?;
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BindingSetGuaWanParams {
    #[arg(help = "help.arg.internal-port")]
    internal_port: u16,
    #[arg(long, help = "help.arg.address")]
    #[ts(as = "HostnameInfo")]
    address: CliFromJsonString<HostnameInfo>,
    #[arg(long, help = "help.arg.gua-wan")]
    wan: bool,
}

/// Opt a single IPv6 GUA on a binding into (or out of) WAN exposure. The flag
/// is projected into `HostnameInfo.public` by `update_addresses`, so the row's
/// enable/disable override set switches (`disabled` while local, `enabled`
/// while public) — the current on/off state is carried across the flip. Errors
/// if `address` is not an IPv6 global-unicast address.
pub async fn set_gua_wan<Kind: HostApiKind>(
    ctx: RpcContext,
    BindingSetGuaWanParams {
        internal_port,
        address,
        wan,
    }: BindingSetGuaWanParams,
    inheritance: Kind::Inheritance,
) -> Result<(), Error> {
    let address = address.0;
    let gua = address.gua().ok_or_else(|| {
        Error::new(
            eyre!("address is not an IPv6 global-unicast address"),
            ErrorKind::InvalidRequest,
        )
    })?;
    ctx.db
        .mutate(|db| {
            Kind::host_for(&inheritance, db)?
                .as_bindings_mut()
                .mutate(|b| {
                    let bind = b.get_mut(&internal_port).or_not_found(internal_port)?;
                    let addrs = &mut bind.addresses;
                    let sa = SocketAddr::V6(gua);
                    if wan {
                        let on = !addrs
                            .disabled
                            .contains(&(address.hostname.clone(), gua.port()));
                        addrs.gua_wan.insert(gua);
                        if on {
                            addrs.enabled.insert(sa);
                        } else {
                            addrs.enabled.remove(&sa);
                        }
                    } else {
                        addrs.gua_wan.remove(&gua);
                        addrs.enabled.remove(&sa);
                    }
                    Ok(())
                })?;
            let hostname = ServerHostname::load(db.as_public().as_server_info())?;
            let gateways = db
                .as_public()
                .as_server_info()
                .as_network()
                .as_gateways()
                .de()?;
            let ports = db.as_private().as_available_ports().de()?;
            Kind::host_for(&inheritance, db)?.update_addresses(&hostname, &gateways, &ports)
        })
        .await
        .result?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::GatewayId;
    use crate::net::service_interface::{HostnameInfo, HostnameMetadata};

    fn ipv6_addr(host: &str, port: u16) -> HostnameInfo {
        HostnameInfo {
            ssl: true,
            public: false,
            hostname: InternedString::intern(host),
            port: Some(port),
            metadata: HostnameMetadata::Ipv6 {
                gateway: GatewayId::from(InternedString::intern("eth0")),
                scope_id: 0,
            },
        }
    }

    #[test]
    fn gua_detection() {
        assert!(ipv6_addr("2001:db8::1", 443).gua().is_some()); // global unicast
        assert!(ipv6_addr("fd00::1", 443).gua().is_none()); // ULA
        assert!(ipv6_addr("fe80::1", 443).gua().is_none()); // link-local
        assert!(ipv6_addr("::1", 443).gua().is_none()); // loopback
        // An IPv4 address is never a GUA, even at a global-looking string.
        let v4 = HostnameInfo {
            hostname: InternedString::intern("1.2.3.4"),
            metadata: HostnameMetadata::Ipv4 {
                gateway: GatewayId::from(InternedString::intern("eth0")),
            },
            ..ipv6_addr("2001:db8::1", 443)
        };
        assert!(v4.gua().is_none());
    }

    #[test]
    fn gua_enabled_follows_public_flag() {
        let local = ipv6_addr("2001:db8::1", 443);
        let key = local.gua().unwrap();
        let mut info = DerivedAddressInfo::default();
        info.available.insert(local.clone());

        // A local (non-WAN) GUA follows the private rule: on unless disabled.
        assert!(info.enabled().contains(&local));
        info.disabled.insert((local.hostname.clone(), key.port()));
        assert!(!info.enabled().contains(&local));
        info.disabled.clear();

        // A WAN GUA carries public=true (projected from gua_wan by
        // update_addresses) and follows the public rule: opt-in via `enabled`.
        let public = HostnameInfo {
            public: true,
            ..local.clone()
        };
        info.available.clear();
        info.available.insert(public.clone());
        assert!(!info.enabled().contains(&public));
        info.enabled.insert(SocketAddr::V6(key));
        assert!(info.enabled().contains(&public));
    }

    #[test]
    fn rekey_port_carries_range_overrides() {
        use std::net::SocketAddr;
        let mut info = DerivedAddressInfo::default();
        let wan: SocketAddr = "1.2.3.4:49152".parse().unwrap();
        let unrelated: SocketAddr = "1.2.3.4:8443".parse().unwrap();
        info.enabled.insert(wan);
        info.enabled.insert(unrelated);
        info.disabled
            .insert((InternedString::intern("example.com"), 49152));

        // A range moving from external_start_port 49152 to 5000.
        info.rekey_port(49152, 5000);

        assert!(info.enabled.contains(&"1.2.3.4:5000".parse().unwrap()));
        assert!(!info.enabled.contains(&wan));
        assert!(info.enabled.contains(&unrelated)); // unrelated port untouched
        assert!(
            info.disabled
                .contains(&(InternedString::intern("example.com"), 5000))
        );
        assert!(
            !info
                .disabled
                .contains(&(InternedString::intern("example.com"), 49152))
        );
    }
}
