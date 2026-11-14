use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6};
use std::ops::RangeInclusive;
use std::str::FromStr;

use clap::Parser;
use clap::builder::ValueParserFactory;
use imbl_value::json;
use itertools::Itertools;
use patch_db::Dump;
use patch_db::json_ptr::JsonPointer;
use rpc_toolkit::{
    Context, Empty, HandlerArgs, HandlerExt, ParentHandler, from_fn_async, from_fn_async_local,
};
use serde::{Deserialize, Serialize};

use crate::context::{CliContext, RpcContext};
use crate::db::prelude::Map;
use crate::hostname::ServerHostname;
use crate::net::forward::{AvailablePorts, START9_BRIDGE_IFACE};
use crate::net::host::HostApiKind;
use crate::net::service_interface::{
    HostnameInfo, HostnameMetadata, RangeServiceInterface, ServiceInterface,
};
use crate::net::vhost::AlpnInfo;
use crate::prelude::*;
use crate::util::FromStrParser;
use crate::util::serde::{HandlerExtSerde, MaybeUtf8String, display_serializable};
use crate::util::tui::choose_custom_display;
use crate::{GatewayId, HostId, ServiceInterfaceId};

mod reassign;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct BindId {
    pub id: HostId,
    pub internal_port: u16,
}

rpc_toolkit::reflect_ts!(BindId);
rpc_toolkit::ts_export!(BindId, namespaces = [""]);
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

#[derive(Debug, Default, Clone, Deserialize, Serialize, VisitFields, HasModel)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct DerivedAddressInfo {
    /// Public IP opt-ins.
    pub enabled: BTreeSet<SocketAddr>,
    /// Named-address and private-IP opt-outs.
    pub disabled: BTreeSet<(InternedString, u16)>,
    /// Named-address opt-ins retained across discovery changes.
    #[serde(default)]
    pub enabled_names: BTreeSet<(InternedString, u16)>,
    /// Binding policy projected for standalone address consumers.
    #[serde(default)]
    pub default_off: bool,
    /// IPv6 global-unicast WAN opt-ins, projected into `HostnameInfo.public`.
    #[serde(default)]
    pub gua_wan: BTreeSet<SocketAddrV6>,
    /// User override: enable these private IPs regardless of their mDNS address.
    #[serde(default)]
    pub lan_enabled: BTreeSet<(InternedString, u16)>,
    /// Plaintext private addresses requiring consent, keyed by hostname, port and gateway.
    #[serde(default)]
    pub insecure: BTreeSet<(InternedString, u16, GatewayId)>,
    /// Persisted consent; ordinary address changes must not grant insecure access.
    /// Address removal retains consent; explicit disabling or gateway removal revokes it.
    #[serde(default)]
    pub insecure_enabled: BTreeSet<(InternedString, u16, GatewayId)>,
    /// Discovered address candidates.
    pub available: BTreeSet<HostnameInfo>,
    /// Effective address selections shared by network enforcement and clients.
    #[serde(default)]
    pub effective: BTreeSet<HostnameInfo>,
}

rpc_toolkit::reflect_ts!(DerivedAddressInfo);
rpc_toolkit::ts_export!(DerivedAddressInfo, namespaces = [""]);

fn override_key(address: &HostnameInfo) -> (InternedString, u16) {
    (address.hostname.clone(), address.port.unwrap_or_default())
}

/// Insecure hostname rows contain one gateway.
pub(super) fn insecure_key(address: &HostnameInfo) -> Option<(InternedString, u16, GatewayId)> {
    Some((
        address.hostname.clone(),
        address.port?,
        address.metadata.gateways().next()?.clone(),
    ))
}

/// Does `mdns` resolve to `ip`?
fn mdns_covers(mdns: &HostnameInfo, ip: &HostnameInfo) -> bool {
    let gateway = match &ip.metadata {
        HostnameMetadata::Ipv4 { gateway } | HostnameMetadata::Ipv6 { gateway, .. } => gateway,
        _ => return false,
    };
    mdns.port == ip.port
        && matches!(
            &mdns.metadata,
            HostnameMetadata::Mdns { gateways } if gateways.contains(gateway)
        )
}

impl Model<DerivedAddressInfo> {
    pub fn mutate_available(
        &mut self,
        update: impl FnOnce(&mut BTreeSet<HostnameInfo>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.mutate(|addresses| {
            update(&mut addresses.available)?;
            addresses.refresh_effective();
            Ok(())
        })
    }
}

impl DerivedAddressInfo {
    /// Override mutations must refresh selections before publication.
    pub(crate) fn refresh_effective(&mut self) {
        self.effective = self.enabled().into_iter().cloned().collect();
    }

    /// Keeps legacy SDK address readers aligned with the effective selections.
    pub(super) fn project_service_selections(&mut self) {
        let enabled: BTreeSet<_> = self.enabled().into_iter().cloned().collect();
        self.effective = enabled.clone();
        let opted_names: BTreeSet<_> = enabled
            .iter()
            .filter(|a| insecure_key(a).is_some_and(|k| self.insecure_enabled.contains(&k)))
            .map(override_key)
            .collect();
        self.available.retain(|a| {
            let restricted = insecure_key(a).is_some_and(|k| self.insecure.contains(&k))
                || opted_names.contains(&override_key(a));
            !restricted || enabled.contains(a)
        });
        self.disabled.retain(|key| !opted_names.contains(key));
        if self.default_off {
            let named_on: BTreeSet<_> = enabled
                .iter()
                .filter(|a| !a.metadata.is_ip())
                .map(override_key)
                .collect();
            let named_off: Vec<_> = self
                .available
                .iter()
                .filter(|a| !a.metadata.is_ip() && !a.is_internal())
                .map(override_key)
                .filter(|key| !named_on.contains(key))
                .collect();
            self.disabled.extend(named_off);
        }
        let lan_ips: Vec<_> = self
            .available
            .iter()
            .filter(|a| a.metadata.is_ip() && !a.public && !a.is_internal())
            .map(|a| (override_key(a), enabled.contains(a)))
            .collect();
        for (key, on) in lan_ips {
            self.set_lan_ip(key, on);
        }
    }

    /// The UI and direct forwarding must agree on enabled/public address exposure.
    /// An enabled plain-port public domain publishes its gateway GUAs in that state.
    /// Public forwarding behind a Local or Disabled row is a correctness defect.
    /// A new TLS row of a public domain stays off unless that domain was enabled
    /// here and no row of it was disabled.
    /// A plain domain groups its port's addresses, so the group stays on only if
    /// everything public on the gateway was on, a Local GUA counting as public-off.
    /// An operator action grouping addresses enables them before this runs.
    pub(super) fn resolve_address_groups(&mut self, previous: &DerivedAddressInfo) {
        let is_domain =
            |a: &&HostnameInfo| matches!(a.metadata, HostnameMetadata::PublicDomain { .. });
        let enabled = previous.enabled();
        let disabled_domains: BTreeSet<_> = previous
            .available
            .iter()
            .filter(is_domain)
            .filter(|a| !enabled.contains(a))
            .map(|a| a.hostname.clone())
            .collect();
        let enabled_domains: BTreeSet<_> = enabled
            .iter()
            .copied()
            .filter(is_domain)
            .map(|a| a.hostname.clone())
            .collect();
        let mut all_public = BTreeMap::<(GatewayId, bool), bool>::new();
        for address in previous
            .available
            .iter()
            .filter(|a| (a.public && a.metadata.is_ip()) || a.gua().is_some() || is_domain(a))
        {
            // `gua_wan` is current; a GUA's `public` flag waits for `set_available`.
            let public = address.gua().map_or_else(
                || enabled.contains(address),
                |gua| previous.gua_wan.contains(&gua) && previous.public_ip_on(address),
            );
            for gateway in address.metadata.gateways() {
                *all_public
                    .entry((gateway.clone(), address.ssl))
                    .or_insert(true) &= public;
            }
        }
        let mut wan_groups = BTreeMap::new();
        let mut new_tls = Vec::new();
        for address in &self.available {
            let (HostnameMetadata::PublicDomain { gateway }, Some(port)) =
                (&address.metadata, address.port)
            else {
                continue;
            };
            if !address.ssl {
                let all_on = all_public
                    .get(&(gateway.clone(), false))
                    .or_else(|| all_public.get(&(gateway.clone(), true)))
                    .is_some_and(|all| *all);
                wan_groups.insert((gateway.clone(), port), all_on);
            } else if !previous.available.contains(address) {
                let key = override_key(address);
                if !self.enabled_names.contains(&key) {
                    new_tls.push((
                        key,
                        !disabled_domains.contains(&address.hostname)
                            && enabled_domains.contains(&address.hostname),
                    ));
                }
            }
        }
        for (key, on) in new_tls {
            self.set_name_enabled(key, on);
        }
        for ((gateway, port), on) in wan_groups {
            set_nonssl_wan_group(self, &gateway, port, on);
        }
    }

    /// Changing defaults preserves served addresses, not undiscovered candidates.
    pub(super) fn set_available(&mut self, available: BTreeSet<HostnameInfo>, default_off: bool) {
        if default_off && !self.default_off {
            let selected: Vec<_> = self.enabled().into_iter().cloned().collect();
            for address in selected {
                if !address.metadata.is_ip() {
                    self.set_name_enabled(override_key(&address), true);
                } else if !address.public && !address.is_internal() {
                    self.set_lan_ip(override_key(&address), true);
                }
            }
        }
        self.default_off = default_off;
        self.available = available
            .into_iter()
            .filter(|address| !default_off || !address.is_internal())
            .map(|mut address| {
                if let Some(gua) = address.gua() {
                    address.public = self.gua_wan.contains(&gua);
                }
                address
            })
            .collect();
        self.refresh_effective();
    }

    /// Clears saved opt-ins; recomputing addresses does not restore them.
    pub(crate) fn disable_all(&mut self) {
        self.enabled.clear();
        self.enabled_names.clear();
        self.gua_wan.clear();
        self.lan_enabled.clear();
        self.insecure_enabled.clear();
        self.disabled
            .extend(self.available.iter().map(override_key));
        self.refresh_effective();
    }

    fn public_ip_on(&self, address: &HostnameInfo) -> bool {
        address
            .to_socket_addr()
            .is_some_and(|sa| self.enabled.contains(&sa))
    }

    /// A LAN IP inherits consent from its gateway's `.local` row.
    fn insecure_consented(&self, address: &HostnameInfo) -> bool {
        let Some((_, port, gateway)) = insecure_key(address).filter(|k| self.insecure.contains(k))
        else {
            return true;
        };
        self.insecure_enabled
            .contains(&(address.hostname.clone(), port, gateway.clone()))
            || (address.metadata.is_ip()
                && self.available.iter().any(|mdns| {
                    mdns_covers(mdns, address)
                        && self.insecure_enabled.contains(&(
                            mdns.hostname.clone(),
                            port,
                            gateway.clone(),
                        ))
                }))
    }

    /// Consent overrides the `disabled` entry its name shares with other gateways' rows.
    fn switched_on(&self, address: &HostnameInfo) -> bool {
        insecure_key(address).is_some_and(|k| self.insecure_enabled.contains(&k))
            || self.name_on(&override_key(address))
    }

    /// A private plain-port domain shares one consent state with its gateway IPs.
    fn insecure_allowed(&self, address: &HostnameInfo) -> bool {
        let key = override_key(address);
        if !self.insecure_consented(address) {
            return false;
        }
        if !address.ssl
            && (address.metadata.is_ip()
                || matches!(address.metadata, HostnameMetadata::PrivateDomain { .. }))
        {
            for gateway in address
                .metadata
                .gateways()
                .filter(|g| has_nonssl_private_domain(self, g, key.1))
            {
                if nonssl_lan_group(self, gateway, key.1)
                    .iter()
                    .any(|member| !self.insecure_consented(member))
                {
                    return false;
                }
            }
        }
        true
    }

    /// An override decides; without one the mDNS address resolving to it does.
    pub(super) fn lan_ip_on(&self, address: &HostnameInfo) -> bool {
        if !self.insecure_allowed(address) {
            return false;
        }
        let key = override_key(address);
        if self.lan_enabled.contains(&key) {
            return true;
        }
        if self.disabled.contains(&key) {
            return false;
        }
        let mut names = self
            .available
            .iter()
            .filter(|mdns| mdns_covers(mdns, address))
            .peekable();
        (!self.default_off || names.peek().is_some()) && names.all(|mdns| self.switched_on(mdns))
    }

    fn name_on(&self, key: &(InternedString, u16)) -> bool {
        !self.disabled.contains(key) && (!self.default_off || self.enabled_names.contains(key))
    }

    pub(super) fn set_name_enabled(&mut self, key: (InternedString, u16), enabled: bool) {
        if enabled {
            self.disabled.remove(&key);
            self.enabled_names.insert(key);
        } else {
            self.enabled_names.remove(&key);
            self.disabled.insert(key);
        }
    }

    /// Public GUAs remain reachable independently of the mDNS switch.
    fn mdns_on(&self, mdns: &HostnameInfo) -> bool {
        (self.insecure_allowed(mdns) && self.switched_on(mdns))
            || (!mdns.ssl
                && self
                    .available
                    .iter()
                    .any(|ip| !ip.public && mdns_covers(mdns, ip) && self.lan_ip_on(ip)))
    }

    fn revoke_insecure(&mut self, address: &HostnameInfo) {
        if let Some(port) = address.port {
            for gateway in address.metadata.gateways() {
                self.insecure_enabled
                    .remove(&(address.hostname.clone(), port, gateway.clone()));
            }
        }
    }

    fn set_lan_ip(&mut self, key: (InternedString, u16), enabled: bool) {
        if enabled {
            self.disabled.remove(&key);
            self.lan_enabled.insert(key);
        } else {
            self.lan_enabled.remove(&key);
            self.disabled.insert(key);
        }
    }

    /// Includes explicit consent for temporarily unavailable addresses.
    pub(crate) fn has_enabled_intent(&self) -> bool {
        !self.enabled.is_empty()
            || self.enabled_names.iter().any(|key| self.name_on(key))
            || !self.lan_enabled.is_empty()
            || !self.insecure_enabled.is_empty()
            || !self.enabled().is_empty()
    }

    /// Effective addresses after binding defaults, saved overrides and per-gateway plaintext consent.
    /// Internal addresses remain reachable; public sockets require explicit opt-in.
    pub fn enabled(&self) -> BTreeSet<&HostnameInfo> {
        self.available
            .iter()
            .filter(|h| {
                if h.is_internal() {
                    // lo / lxcbr0 are always reachable and never operator-disablable.
                    true
                } else if h.metadata.is_ip() {
                    if h.public {
                        self.public_ip_on(h)
                    } else {
                        self.lan_ip_on(h)
                    }
                } else if matches!(h.metadata, HostnameMetadata::Mdns { .. }) {
                    self.mdns_on(h)
                } else {
                    self.insecure_allowed(h) && self.switched_on(h)
                }
            })
            .collect()
    }

    /// Moves native address overrides; plugin endpoints retain their ports.
    pub fn rekey_port(&mut self, old: u16, new: u16) {
        if old == new {
            return;
        }
        let plugin_keys: BTreeSet<_> = self
            .available
            .iter()
            .filter(|address| matches!(address.metadata, HostnameMetadata::Plugin { .. }))
            .map(override_key)
            .collect();
        self.enabled = std::mem::take(&mut self.enabled)
            .into_iter()
            .map(|mut sa| {
                if sa.port() == old {
                    sa.set_port(new);
                }
                sa
            })
            .collect();
        for overrides in [
            &mut self.disabled,
            &mut self.lan_enabled,
            &mut self.enabled_names,
        ] {
            *overrides = std::mem::take(overrides)
                .into_iter()
                .map(|(h, p)| {
                    let port = if p == old && !plugin_keys.contains(&(h.clone(), p)) {
                        new
                    } else {
                        p
                    };
                    (h, port)
                })
                .collect();
        }
        for overrides in [&mut self.insecure, &mut self.insecure_enabled] {
            *overrides = std::mem::take(overrides)
                .into_iter()
                .map(|(h, p, g)| (h, if p == old { new } else { p }, g))
                .collect();
        }
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

    pub(super) fn rekey_name(
        &mut self,
        old: (InternedString, u16),
        new: (InternedString, u16),
    ) -> bool {
        if old == new {
            return false;
        }
        let mut changed = false;
        if self.enabled_names.remove(&old) {
            self.set_name_enabled(new.clone(), true);
            changed = true;
        } else if self.disabled.remove(&old) {
            self.set_name_enabled(new.clone(), false);
            changed = true;
        }
        for overrides in [&mut self.insecure, &mut self.insecure_enabled] {
            *overrides = std::mem::take(overrides)
                .into_iter()
                .map(|(hostname, port, gateway)| {
                    if (&hostname, port) == (&old.0, old.1) {
                        changed = true;
                        (new.0.clone(), new.1, gateway)
                    } else {
                        (hostname, port, gateway)
                    }
                })
                .collect();
        }
        changed
    }

    /// Drops IP overrides whose available addresses belong exclusively to this gateway.
    pub fn forget_gateway(&mut self, gateway: &GatewayId) {
        self.insecure.retain(|(_, _, g)| g != gateway);
        self.insecure_enabled.retain(|(_, _, g)| g != gateway);
        let available = &self.available;
        for address in available
            .iter()
            .filter(|a| a.metadata.is_ip() && a.metadata.gateways().any(|g| g == gateway))
            .filter(|address| {
                !available.iter().any(|other| {
                    other.metadata.is_ip()
                        && override_key(other) == override_key(address)
                        && !other.metadata.gateways().any(|g| g == gateway)
                })
            })
        {
            if let Some(sa) = address.to_socket_addr() {
                self.enabled.remove(&sa);
            }
            if let Some(gua) = address.gua() {
                self.gua_wan.remove(&gua);
            }
            let key = override_key(address);
            self.disabled.remove(&key);
            self.lan_enabled.remove(&key);
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, HasModel, VisitFields)]
#[model = "Model<Self>"]
pub struct Bindings(pub BTreeMap<u16, BindInfo>);

rpc_toolkit::reflect_ts!(Bindings);
rpc_toolkit::ts_export!(Bindings, namespaces = [""]);

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

/// A contiguous binding keyed by its internal start port in [`BindingRanges`].
/// External ports map to internal ports by offset.
#[derive(Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct RangeBindInfo {
    pub enabled: bool,
    #[serde(default)]
    pub transport: crate::net::transport::Transport,
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

rpc_toolkit::reflect_ts!(RangeBindInfo);
rpc_toolkit::ts_export!(RangeBindInfo, namespaces = [""]);

#[derive(Debug, Default, Deserialize, Serialize, HasModel, VisitFields)]
#[model = "Model<Self>"]
pub struct BindingRanges(pub BTreeMap<u16, RangeBindInfo>);

rpc_toolkit::reflect_ts!(BindingRanges);
rpc_toolkit::ts_export!(BindingRanges, namespaces = [""]);

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

/// The inclusive internal port span a claim of `count` ports from `start` covers.
/// `count - 1` is taken first — `start + count` overflows u16 for a span ending
/// at 65535.
pub fn internal_span(start: u16, count: u16) -> Result<RangeInclusive<u16>, Error> {
    count
        .checked_sub(1)
        .and_then(|last| start.checked_add(last))
        .map(|end| start..=end)
        .ok_or_else(|| {
            Error::new(
                eyre!("a span of {count} ports from {start} does not fit in the port space"),
                ErrorKind::InvalidRequest,
            )
        })
}

pub fn overlap_error(claim: &RangeInclusive<u16>, existing: &RangeInclusive<u16>) -> Error {
    fn describe(span: &RangeInclusive<u16>) -> String {
        if span.start() == span.end() {
            format!("port {}", span.start())
        } else {
            format!("port range {}-{}", span.start(), span.end())
        }
    }
    Error::new(
        eyre!(
            "internal {} overlaps {}, which this host already binds",
            describe(claim),
            describe(existing)
        ),
        ErrorKind::InvalidRequest,
    )
}

impl Bindings {
    /// The span of the first enabled single-port binding inside `claim`.
    pub fn enabled_overlap(&self, claim: &RangeInclusive<u16>) -> Option<RangeInclusive<u16>> {
        self.range(claim.clone())
            .find(|(_, info)| info.enabled)
            .map(|(port, _)| *port..=*port)
    }
}

impl BindingRanges {
    /// The span of the first enabled range overlapping `claim`. `rebinding` is
    /// the key of the entry an idempotent re-bind re-affirms, which must not
    /// conflict with itself.
    pub fn enabled_overlap(
        &self,
        claim: &RangeInclusive<u16>,
        rebinding: Option<u16>,
    ) -> Result<Option<RangeInclusive<u16>>, Error> {
        for (start, info) in self.iter() {
            if !info.enabled || Some(*start) == rebinding {
                continue;
            }
            let existing = internal_span(*start, info.number_of_ports)?;
            if existing.start() <= claim.end() && claim.start() <= existing.end() {
                return Ok(Some(existing));
            }
        }
        Ok(None)
    }
}

impl RangeBindInfo {
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Hand the whole span back to the pool. `number_of_ports` is at least 2,
    /// so subtract before adding — `start + count` overflows a u16 at 65535.
    pub fn release(&self, available_ports: &mut AvailablePorts) {
        available_ports
            .free(self.external_start_port..=self.external_start_port + (self.number_of_ports - 1));
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

#[derive(Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
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

rpc_toolkit::reflect_ts!(BindInfo);
rpc_toolkit::ts_export!(BindInfo, namespaces = [""]);

/// Unallocated candidates; absent ports are pending rather than protocol defaults.
#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct BindingPreview {
    pub options: BindOptions,
    pub addresses: DerivedAddressInfo,
}

rpc_toolkit::reflect_ts!(BindingPreview);
rpc_toolkit::ts_export!(BindingPreview, namespaces = [""]);

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct NetInfo {
    pub assigned_port: Option<u16>,
    pub assigned_ssl_port: Option<u16>,
}

rpc_toolkit::reflect_ts!(NetInfo);
rpc_toolkit::ts_export!(NetInfo, namespaces = [""]);
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

    fn ensure_selectable(&self, address: &HostnameInfo) -> Result<(), Error> {
        if self.options.default_off && !self.enabled {
            return Err(Error::new(
                eyre!("{}", t!("net.host.binding.address-unavailable")),
                ErrorKind::InvalidRequest,
            ));
        }
        validate_available_address(&self.addresses, address)
    }

    pub(crate) fn set_address_enabled(
        &mut self,
        address: &HostnameInfo,
        enabled: bool,
    ) -> Result<(), Error> {
        self.ensure_selectable(address)?;
        set_address_enabled_with_consent(&mut self.addresses, address, enabled, false)
    }

    pub(crate) fn set_gua_wan(&mut self, address: &HostnameInfo, wan: bool) -> Result<(), Error> {
        self.ensure_selectable(address)?;
        if wan && !self.options.allows_public_address(address.ssl) {
            return Err(Error::new(
                eyre!("{}", t!("net.host.binding.insecure-public")),
                ErrorKind::InvalidRequest,
            ));
        }
        let gua = address.gua().ok_or_else(|| {
            Error::new(
                eyre!("{}", t!("net.host.binding.not-global-unicast")),
                ErrorKind::InvalidRequest,
            )
        })?;
        set_gua_wan_on(&mut self.addresses, address, gua, wan);
        Ok(())
    }

    pub(crate) fn unallocated(options: BindOptions) -> Self {
        Self {
            enabled: true,
            options,
            net: NetInfo {
                assigned_port: None,
                assigned_ssl_port: None,
            },
            addresses: DerivedAddressInfo::default(),
            interfaces: BTreeMap::new(),
        }
    }

    pub fn new(
        available_ports: &mut AvailablePorts,
        options: BindOptions,
        privileged: bool,
    ) -> Result<Self, Error> {
        let assigned_ssl_port = options
            .preferred_ssl_port()
            .map(|preferred| available_ports.alloc_preferred([preferred], true, privileged))
            .transpose()?;
        let assigned_port = options
            .wants_plain_port()
            .then(|| {
                available_ports.alloc_preferred(
                    [options.preferred_external_port],
                    false,
                    privileged,
                )
            })
            .transpose()?;

        Ok(Self {
            net: NetInfo {
                assigned_port,
                assigned_ssl_port,
            },
            ..Self::unallocated(options)
        })
    }
    pub fn update(
        self,
        available_ports: &mut AvailablePorts,
        options: BindOptions,
        privileged: bool,
    ) -> Result<Self, Error> {
        let Self {
            net: held,
            addresses,
            interfaces,
            ..
        } = self;
        // Free both up front so each leg can reclaim the number it already holds —
        // it is in the user's address book and keys their per-address overrides.
        available_ports.free(held.assigned_port.into_iter().chain(held.assigned_ssl_port));
        let assigned_ssl_port = options
            .preferred_ssl_port()
            .map(|preferred| {
                available_ports.alloc_preferred(
                    held.assigned_ssl_port.into_iter().chain([preferred]),
                    true,
                    privileged,
                )
            })
            .transpose()?;
        let assigned_port = options
            .wants_plain_port()
            .then(|| {
                available_ports.alloc_preferred(
                    held.assigned_port
                        .into_iter()
                        .chain([options.preferred_external_port]),
                    false,
                    privileged,
                )
            })
            .transpose()?;

        Ok(Self {
            enabled: true,
            options,
            net: NetInfo {
                assigned_port,
                assigned_ssl_port,
            },
            addresses,
            interfaces,
        })
    }
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Inverse of [`BindInfo::new`]. Unlike [`BindInfo::update`], which reclaims
    /// the same numbers to keep the user's address book stable, this returns
    /// them to the pool for anyone.
    pub fn release(&self, available_ports: &mut AvailablePorts) {
        available_ports.free(
            self.net
                .assigned_port
                .into_iter()
                .chain(self.net.assigned_ssl_port),
        );
    }
}

/// A protocol safe to expose on enabled public addresses.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct Security {
    /// Whether TLS precedes application bytes.
    pub ssl: bool,
}

rpc_toolkit::reflect_ts!(Security);
rpc_toolkit::ts_export!(Security, namespaces = [""]);

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase", try_from = "CompatibleBindOptions")]
#[visit(type_attributes(serde::try_from))]
pub struct BindOptions {
    /// Selects direct-forward transports; HTTP proxies and TLS listeners use TCP.
    #[serde(default)]
    pub transport: crate::net::transport::Transport,
    /// Zero requests dynamic allocation; it never reserves port zero.
    pub preferred_external_port: u16,
    pub add_ssl: Option<AddSslOptions>,
    pub secure: Option<Security>,
    /// Proxy protocol shared by every exposed leg of this binding.
    pub proxy: Option<ProxyOptions>,
    /// New address groups require opt-in; existing selections persist.
    /// Inactive bindings clear selections and reject opt-ins.
    /// Internal endpoints remain local and are omitted from opt-in candidates.
    #[serde(default, alias = "publicOnly")]
    pub default_off: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, VisitVariants)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum ProxyOptions {
    /// Relays raw TCP streams without parsing application messages.
    Tcp,
    Http {
        /// Adds `X-Forwarded-For` and, on the SSL leg, `X-Forwarded-Proto: https`.
        /// Client-supplied `X-Forwarded-*` headers are stripped on both legs.
        #[serde(default)]
        add_x_forwarded_headers: bool,
        /// Validates every request before forwarding. Basic auth sets `X-Forwarded-User`.
        #[serde(default)]
        auth: Option<ProxyAuth>,
    },
}

rpc_toolkit::reflect_ts!(ProxyOptions);
rpc_toolkit::ts_export!(ProxyOptions, namespaces = [""]);

#[derive(Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
struct CompatibleBindOptions {
    #[serde(default)]
    transport: crate::net::transport::Transport,
    preferred_external_port: u16,
    add_ssl: Option<LegacyProxySslOptions>,
    secure: Option<Security>,
    proxy: Option<ProxyOptions>,
    #[serde(default, alias = "publicOnly")]
    default_off: bool,
}

rpc_toolkit::reflect_ts!(CompatibleBindOptions);

#[derive(Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
struct LegacyProxySslOptions {
    #[serde(flatten)]
    ssl: AddSslOptions,
    #[serde(default)]
    add_x_forwarded_headers: bool,
    #[serde(default)]
    auth: Option<ProxyAuth>,
}

rpc_toolkit::reflect_ts!(LegacyProxySslOptions);

impl TryFrom<CompatibleBindOptions> for BindOptions {
    type Error = &'static str;

    fn try_from(options: CompatibleBindOptions) -> Result<Self, Self::Error> {
        let mut proxy = options.proxy;
        let add_ssl = options.add_ssl.map(|legacy| {
            if proxy.is_none() && (legacy.add_x_forwarded_headers || legacy.auth.is_some()) {
                proxy = Some(ProxyOptions::Http {
                    add_x_forwarded_headers: legacy.add_x_forwarded_headers,
                    auth: legacy.auth,
                });
            }
            legacy.ssl
        });
        let options = Self {
            default_off: options.default_off,
            transport: options.transport,
            preferred_external_port: options.preferred_external_port,
            add_ssl,
            secure: options.secure,
            proxy,
        };
        if !options
            .transport
            .protocols()
            .contains(&crate::net::transport::TransportProtocol::Tcp)
            && (options.proxy.is_some()
                || options.add_ssl.is_some()
                || options.secure.is_some_and(|s| s.ssl))
        {
            return Err("HTTP proxying and TLS require TCP transport");
        }
        if options.serves_own_tls() && matches!(options.proxy, Some(ProxyOptions::Http { .. })) {
            return Err("HTTP proxying requires TLS termination for a TLS container");
        }
        Ok(options)
    }
}

rpc_toolkit::reflect_ts!(BindOptions);
rpc_toolkit::ts_export!(BindOptions, namespaces = [""]);

impl BindOptions {
    /// OS-terminated plaintext stays private, including IPv6 GUAs.
    pub fn allows_public_address(&self, ssl: bool) -> bool {
        ssl || self.secure.is_some()
    }

    pub(crate) fn server_ui() -> Self {
        Self {
            default_off: false,
            transport: crate::net::transport::Transport::tcp(),
            preferred_external_port: 80,
            add_ssl: Some(AddSslOptions {
                preferred_external_port: super::SERVER_UI_SSL_PORT,
                alpn: Some(AlpnInfo(vec![
                    MaybeUtf8String("h2".into()),
                    MaybeUtf8String("http/1.1".into()),
                ])),
                upstream_cert_validation: None,
            }),
            secure: None,
            proxy: None,
        }
    }

    /// The container terminates TLS itself; the OS fronts its port with an
    /// SNI-passthrough listener that pipes the raw TLS stream through.
    pub fn serves_own_tls(&self) -> bool {
        self.secure.map_or(false, |s| s.ssl) && self.add_ssl.is_none()
    }

    /// The `add_ssl` options for a binding StartOS rewraps: one whose container
    /// serves its own TLS behind a listener of ours that terminates the client's.
    pub fn rewrap(&self) -> Option<&AddSslOptions> {
        self.add_ssl
            .as_ref()
            .filter(|_| self.secure.map_or(false, |s| s.ssl))
    }

    /// Preferred external port for the TLS-carrying listener: the OS's own when
    /// it terminates (`add_ssl`), otherwise the binding's, since a self-TLS
    /// binding has no second port to take one from.
    pub fn preferred_ssl_port(&self) -> Option<u16> {
        self.add_ssl
            .as_ref()
            .map(|s| s.preferred_external_port)
            .or_else(|| {
                self.serves_own_tls()
                    .then_some(self.preferred_external_port)
            })
    }

    /// A plaintext external port exists unless the container itself speaks TLS
    /// — two listening ports both carrying TLS makes no sense.
    pub fn wants_plain_port(&self) -> bool {
        !self.secure.map_or(false, |s| s.ssl)
    }
}

/// How the OS reverse proxy validates the container's TLS certificate when it
/// rewraps SSL (`add_ssl` set AND `secure.ssl == true`, so the OS terminates
/// the client's TLS and initiates a fresh TLS connection to the container).
/// Absent (`None`) means validate against the StartOS root CA — the default.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum UpstreamCertValidation {
    /// Do not validate the container's certificate at all. Use when the
    /// container serves a self-signed cert on the trusted internal bridge.
    Disable,
    /// Validate against this PEM-encoded certificate (or chain) instead of the
    /// StartOS root CA.
    Certificate(String),
}

rpc_toolkit::reflect_ts!(UpstreamCertValidation);
rpc_toolkit::ts_export!(UpstreamCertValidation, namespaces = [""]);

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct AddSslOptions {
    pub preferred_external_port: u16,
    /// The application protocols StartOS answers a client with, from those it
    /// asked for. Unset answers with whatever it asked for.
    #[serde(default, deserialize_with = "legacy_alpn::deserialize")]
    #[visit(input_wire = "Option<legacy_alpn::CompatibleAlpnInfo>")]
    #[visit(type_attributes(visit::input_wire))]
    pub alpn: Option<AlpnInfo>,
    /// Certificate validation for the OS→container TLS leg when rewrapping.
    /// `None` (the default) validates against the StartOS root CA.
    #[serde(default)]
    pub upstream_cert_validation: Option<UpstreamCertValidation>,
}

rpc_toolkit::reflect_ts!(AddSslOptions);
rpc_toolkit::ts_export!(AddSslOptions, namespaces = [""]);

mod legacy_alpn {
    use serde::{Deserialize, Deserializer};

    use crate::net::vhost::AlpnInfo;
    use crate::util::serde::MaybeUtf8String;

    #[derive(Deserialize, visit_rs::VisitVariants)]
    #[serde(rename_all = "camelCase")]
    pub(super) enum LegacyAlpnInfo {
        Reflect,
        Specified(Vec<MaybeUtf8String>),
    }

    rpc_toolkit::reflect_ts!(LegacyAlpnInfo);

    #[derive(Deserialize, visit_rs::VisitVariants)]
    #[serde(untagged)]
    pub(super) enum CompatibleAlpnInfo {
        Current(Vec<MaybeUtf8String>),
        Legacy(LegacyAlpnInfo),
    }

    rpc_toolkit::reflect_ts!(CompatibleAlpnInfo);

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Option<AlpnInfo>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(
            match Option::<CompatibleAlpnInfo>::deserialize(deserializer)? {
                None | Some(CompatibleAlpnInfo::Legacy(LegacyAlpnInfo::Reflect)) => None,
                Some(CompatibleAlpnInfo::Current(protocols))
                | Some(CompatibleAlpnInfo::Legacy(LegacyAlpnInfo::Specified(protocols))) => {
                    Some(AlpnInfo(protocols))
                }
            },
        )
    }
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
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, VisitVariants)]
#[serde(rename_all = "camelCase", tag = "type")]
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

rpc_toolkit::reflect_ts!(ProxyAuth);
rpc_toolkit::ts_export!(ProxyAuth, namespaces = [""]);

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct BasicCredential {
    pub username: String,
    pub password: String,
}

rpc_toolkit::reflect_ts!(BasicCredential);
rpc_toolkit::ts_export!(BasicCredential, namespaces = [""]);

pub fn binding<C: Context, Kind: HostApiKind>()
-> ParentHandler<C, Kind::Params, Kind::InheritedParams> {
    ParentHandler::<C, Kind::Params, Kind::InheritedParams>::new()
        .subcommand(
            "reassign-port",
            from_fn_async(reassign::reassign_port::<Kind>)
                .with_metadata("sync_db", Value::Bool(true))
                .with_inherited(Kind::inheritance)
                .no_display()
                .with_about("about.reassign-binding-port")
                .with_call_remote::<CliContext>(),
        )
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
                    table.add_row(row![bc =>
                        "INTERNAL PORT",
                        "ENABLED",
                        "EXTERNAL PORT",
                        "EXTERNAL SSL PORT",
                        "BRIDGE",
                        "BRIDGE SSL",
                    ]);
                    for (internal, info) in res.iter() {
                        let bridge = |ssl: bool| {
                            bridge_address(&info.addresses, ssl)
                                .map_or_else(|| "N/A".to_owned(), |a| a.to_string())
                        };
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
                            bridge(false),
                            bridge(true),
                        ]);
                    }

                    print_table(&table)?;

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
                .no_cli(),
        )
        .subcommand(
            "set-address-enabled",
            from_fn_async_local(cli_set_address_enabled::<Kind>)
                .no_ts()
                .with_inherited(Kind::inheritance)
                .no_display()
                .with_about("about.set-address-enabled-for-binding"),
        )
        .subcommand(
            "set-range-address-enabled",
            from_fn_async(set_range_address_enabled::<Kind>)
                .with_metadata("sync_db", Value::Bool(true))
                .with_inherited(Kind::inheritance)
                .no_cli(),
        )
        .subcommand(
            "set-range-address-enabled",
            from_fn_async_local(cli_set_range_address_enabled::<Kind>)
                .no_ts()
                .with_inherited(Kind::inheritance)
                .no_display()
                .with_about("about.set-range-address-enabled-for-binding"),
        )
        .subcommand(
            "set-gua-wan",
            from_fn_async(set_gua_wan::<Kind>)
                .with_metadata("sync_db", Value::Bool(true))
                .with_inherited(Kind::inheritance)
                .no_cli(),
        )
        .subcommand(
            "set-gua-wan",
            from_fn_async_local(cli_set_gua_wan::<Kind>)
                .no_ts()
                .with_inherited(Kind::inheritance)
                .no_display()
                .with_about("about.set-gua-wan-for-binding"),
        )
}

/// The row `sdk.host.getBridgeAddress` resolves: IPv4 on the `lxcbr0` gateway.
fn bridge_address(addresses: &DerivedAddressInfo, ssl: bool) -> Option<SocketAddr> {
    addresses
        .available
        .iter()
        .find(|a| {
            a.ssl == ssl
                && matches!(&a.metadata, HostnameMetadata::Ipv4 { gateway } if gateway.as_str() == START9_BRIDGE_IFACE)
        })
        .and_then(HostnameInfo::to_socket_addr)
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

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct BindingSetAddressEnabledParams {
    pub(crate) internal_port: u16,
    pub(crate) address: HostnameInfo,
    pub(crate) enabled: Option<bool>,
    /// Accepts readable and alterable plaintext traffic, including passwords.
    #[serde(default)]
    pub(crate) allow_insecure: bool,
}

rpc_toolkit::reflect_ts!(BindingSetAddressEnabledParams);
rpc_toolkit::ts_export!(BindingSetAddressEnabledParams, namespaces = [""]);

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct CliBindingSetAddressEnabledParams {
    #[arg(help = "help.arg.internal-port")]
    internal_port: u16,
    #[arg(long, help = "help.arg.address")]
    address: String,
    #[arg(long, help = "help.arg.binding-enabled")]
    enabled: Option<bool>,
    #[arg(long, help = "help.arg.allow-insecure-address")]
    allow_insecure: bool,
}

// Plain-port domain aliases share their gateway's exposure; WAN consent dominates GUA LAN intent.

/// Is there a non-SSL public domain at this `gateway` + `port`? Without one the
/// WAN IPv4 and GUA toggle independently.
pub(crate) fn has_nonssl_public_domain(
    addresses: &DerivedAddressInfo,
    gateway: &GatewayId,
    port: u16,
) -> bool {
    addresses.available.iter().any(|a| {
        !a.ssl
            && a.port == Some(port)
            && matches!(&a.metadata, HostnameMetadata::PublicDomain { gateway: gw } if gw == gateway)
    })
}

/// Is there a non-SSL private domain at this `gateway` + `port`? The LAN mirror
/// of [`has_nonssl_public_domain`].
pub(crate) fn has_nonssl_private_domain(
    addresses: &DerivedAddressInfo,
    gateway: &GatewayId,
    port: u16,
) -> bool {
    addresses.available.iter().any(|a| {
        !a.ssl
            && a.port == Some(port)
            && matches!(&a.metadata, HostnameMetadata::PrivateDomain { gateways } if gateways.contains(gateway))
    })
}

/// Is the LAN level (private domain or bare LAN IPv4) currently on at
/// `gateway`+`port`? Excludes the GUA.
fn nonssl_lan_on(addresses: &DerivedAddressInfo, gateway: &GatewayId, port: u16) -> bool {
    let enabled = addresses.enabled();
    addresses.available.iter().any(|a| {
        if a.ssl || a.port != Some(port) {
            return false;
        }
        match &a.metadata {
            HostnameMetadata::PrivateDomain { gateways } if gateways.contains(gateway) => {
                enabled.contains(a)
            }
            HostnameMetadata::Ipv4 { gateway: gw } if !a.public && gw == gateway => {
                enabled.contains(a)
            }
            _ => false,
        }
    })
}

/// Re-derive every GUA's reachability at `gateway`+`port` from its stored WAN
/// opt-in (`gua_wan`, projected to `HostnameInfo.public`) and the LAN level. The
/// WAN opt-in is an operator preference — this only READS it, never clobbers it,
/// so a LAN-level change can't un-publish a GUA. A public GUA (in `gua_wan`) is
/// reachable on WAN and LAN (public wins); otherwise it is local (on while the
/// LAN level is up, else off). Only [`set_nonssl_wan_group`]/`set_gua_wan` write
/// `gua_wan`.
fn resolve_nonssl_gua(addresses: &mut DerivedAddressInfo, gateway: &GatewayId, port: u16) {
    let lan_on = nonssl_lan_on(addresses, gateway, port);
    let guas: Vec<(SocketAddrV6, InternedString)> = addresses
        .available
        .iter()
        .filter_map(|a| {
            if a.ssl || a.port != Some(port) {
                return None;
            }
            match &a.metadata {
                HostnameMetadata::Ipv6 { gateway: gw, .. } if gw == gateway => {
                    a.gua().map(|g| (g, a.hostname.clone()))
                }
                _ => None,
            }
        })
        .collect();
    for (g, host) in guas {
        let key = (host, port);
        let sa = SocketAddr::V6(g);
        if addresses.gua_wan.contains(&g) {
            // Public (operator WAN opt-in): reachable on WAN and LAN.
            addresses.enabled.insert(sa);
            addresses.disabled.remove(&key);
            addresses.lan_enabled.remove(&key);
        } else {
            // Local: LAN-only, following the LAN level.
            addresses.enabled.remove(&sa);
            let local = lan_on || addresses.lan_enabled.contains(&key);
            addresses.set_lan_ip(key, local);
        }
    }
}

/// Set the non-SSL WAN level at `gateway`+`port` — the public domain(s) and the
/// bare WAN IPv4 — to `enabled`, then resolve the shared GUA. On a non-SSL port
/// there is no SNI, so these share the same packets and move as one. Callers gate
/// on a public domain being present ([`has_nonssl_public_domain`]).
pub(crate) fn set_nonssl_wan_group(
    addresses: &mut DerivedAddressInfo,
    gateway: &GatewayId,
    port: u16,
    enabled: bool,
) {
    let mut domain_keys = Vec::new();
    let mut ipv4 = Vec::new();
    let mut guas = Vec::new();
    for a in &addresses.available {
        if a.ssl || a.port != Some(port) {
            continue;
        }
        match &a.metadata {
            HostnameMetadata::PublicDomain { gateway: gw } if gw == gateway => {
                domain_keys.push((a.hostname.clone(), port));
            }
            HostnameMetadata::Ipv4 { gateway: gw } if a.public && gw == gateway => {
                if let Some(sa) = a.to_socket_addr() {
                    ipv4.push(sa);
                }
            }
            HostnameMetadata::Ipv6 { gateway: gw, .. } if gw == gateway => {
                if let Some(g) = a.gua() {
                    guas.push(g);
                }
            }
            _ => {}
        }
    }
    for key in domain_keys {
        addresses.set_name_enabled(key, enabled);
    }
    for sa in ipv4 {
        if enabled {
            addresses.enabled.insert(sa);
        } else {
            addresses.enabled.remove(&sa);
        }
    }
    // The public domain flips the GUA's WAN opt-in (the stored `gua_wan` /
    // `public` flag) with it; `resolve_nonssl_gua` then derives its reachability.
    for g in guas {
        if enabled {
            addresses.gua_wan.insert(g);
        } else {
            addresses.gua_wan.remove(&g);
        }
    }
    resolve_nonssl_gua(addresses, gateway, port);
}

fn nonssl_lan_group(
    addresses: &DerivedAddressInfo,
    gateway: &GatewayId,
    port: u16,
) -> BTreeSet<HostnameInfo> {
    let mut gateways = BTreeSet::from([gateway.clone()]);
    loop {
        let previous = gateways.len();
        for a in &addresses.available {
            if a.ssl || a.port != Some(port) {
                continue;
            }
            if let HostnameMetadata::PrivateDomain {
                gateways: domain_gateways,
            } = &a.metadata
            {
                if !gateways.is_disjoint(domain_gateways) {
                    gateways.extend(domain_gateways.iter().cloned());
                }
            }
        }
        if gateways.len() == previous {
            break;
        }
    }
    addresses
        .available
        .iter()
        .filter(|a| !a.ssl && !a.public && a.port == Some(port))
        .filter(|a| match &a.metadata {
            HostnameMetadata::PrivateDomain {
                gateways: domain_gateways,
            } => !gateways.is_disjoint(domain_gateways),
            HostnameMetadata::Ipv4 { gateway } | HostnameMetadata::Ipv6 { gateway, .. } => {
                gateways.contains(gateway)
            }
            _ => false,
        })
        .cloned()
        .collect()
}

/// Sets inseparable private domains and IPs; enabling never grants insecure consent.
pub(crate) fn set_nonssl_lan_group(
    addresses: &mut DerivedAddressInfo,
    gateway: &GatewayId,
    port: u16,
    enabled: bool,
) {
    let members = nonssl_lan_group(addresses, gateway, port);
    let gateways: BTreeSet<_> = members
        .iter()
        .flat_map(|a| a.metadata.gateways())
        .cloned()
        .collect();
    for a in &members {
        if !enabled {
            addresses.revoke_insecure(a);
        }
        let key = override_key(a);
        if a.metadata.is_ip() {
            addresses.set_lan_ip(key, enabled);
        } else {
            addresses.set_name_enabled(key, enabled);
        }
    }
    for gateway in gateways {
        resolve_nonssl_gua(addresses, &gateway, port);
    }
}

// Applies consent to the native address group.
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
        // Non-SSL: a dual-stack public domain links IPv4 and GUA, so when one is
        // present move the whole {IPv4, domain, GUA} group together (no domain =>
        // v4 and gua stay independent).
        if !address.ssl {
            if let HostnameMetadata::Ipv4 { gateway } | HostnameMetadata::Ipv6 { gateway, .. } =
                &address.metadata
            {
                let port = sa.port();
                if has_nonssl_public_domain(addresses, gateway, port) {
                    set_nonssl_wan_group(addresses, gateway, port, enabled);
                }
            }
        }
    } else {
        let port = address.port.unwrap_or(if address.ssl { 443 } else { 80 });
        let key = (address.hostname.clone(), port);
        if address.metadata.is_ip() {
            addresses.set_lan_ip(key, enabled);
        } else {
            addresses.set_name_enabled(key, enabled);
        }
        // Non-SSL: a domain ties the v4 and v6 sides together (no SNI). A public
        // domain moves the WAN level; a private domain (or the bare LAN IPv4, when
        // a private domain links them) moves the LAN level.
        if !address.ssl {
            match &address.metadata {
                HostnameMetadata::PublicDomain { gateway } => {
                    set_nonssl_wan_group(addresses, gateway, port, enabled);
                }
                HostnameMetadata::PrivateDomain { gateways } => {
                    for gateway in gateways {
                        set_nonssl_lan_group(addresses, gateway, port, enabled);
                    }
                }
                HostnameMetadata::Ipv4 { gateway } | HostnameMetadata::Ipv6 { gateway, .. } => {
                    if has_nonssl_private_domain(addresses, gateway, port) {
                        set_nonssl_lan_group(addresses, gateway, port, enabled);
                    }
                }
                // Off takes every LAN IP it resolves to off with it.
                HostnameMetadata::Mdns { gateways } if !enabled => {
                    for gateway in gateways {
                        if has_nonssl_private_domain(addresses, gateway, port) {
                            set_nonssl_lan_group(addresses, gateway, port, false);
                        }
                    }
                    let held: Vec<_> = addresses
                        .available
                        .iter()
                        .filter(|ip| !ip.public && mdns_covers(address, ip))
                        .map(override_key)
                        .collect();
                    for key in held {
                        addresses.lan_enabled.remove(&key);
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn validate_available_address(
    addresses: &DerivedAddressInfo,
    address: &HostnameInfo,
) -> Result<(), Error> {
    if !addresses.available.contains(address) {
        return Err(Error::new(
            eyre!("{}", t!("net.host.binding.address-unavailable")),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

pub(super) fn set_address_enabled_with_consent(
    addresses: &mut DerivedAddressInfo,
    address: &HostnameInfo,
    enabled: bool,
    allow_insecure: bool,
) -> Result<(), Error> {
    validate_available_address(addresses, address)?;
    let mut affected = BTreeSet::from([address.clone()]);
    if !address.ssl {
        if matches!(address.metadata, HostnameMetadata::Mdns { .. }) {
            affected.extend(
                addresses
                    .available
                    .iter()
                    .filter(|ip| !ip.public && mdns_covers(address, ip))
                    .cloned(),
            );
        }
        for member in affected.clone().iter().filter(|a| {
            !a.public
                && (a.metadata.is_ip()
                    || matches!(a.metadata, HostnameMetadata::PrivateDomain { .. }))
        }) {
            let port = member.port.unwrap_or(80);
            for gateway in member
                .metadata
                .gateways()
                .filter(|g| has_nonssl_private_domain(addresses, g, port))
            {
                affected.extend(nonssl_lan_group(addresses, gateway, port));
            }
        }
    }
    let insecure: BTreeSet<_> = affected
        .iter()
        .filter_map(insecure_key)
        .filter(|key| addresses.insecure.contains(key))
        .collect();
    if enabled && !allow_insecure && !insecure.is_empty() {
        return Err(Error::new(
            eyre!("{}", t!("net.host.binding.insecure-consent-required")),
            ErrorKind::InvalidRequest,
        ));
    }
    for member in &affected {
        if !enabled {
            addresses.revoke_insecure(member);
        } else if let Some(key) = insecure_key(member).filter(|k| insecure.contains(k)) {
            addresses.insecure_enabled.insert(key);
        }
    }
    if insecure_key(address).is_some_and(|k| addresses.insecure.contains(&k)) {
        for member in &affected {
            if member.metadata.is_ip() {
                addresses.set_lan_ip(override_key(member), enabled);
            }
        }
        return Ok(());
    }
    set_address_enabled_on(addresses, address, enabled)
}

/// Toggle one address of a single-port binding (keyed by its internal port).
/// Port ranges use [`set_range_address_enabled`] — they live in a separate DB
/// subtree, so the API distinguishes the two rather than probing both.
pub async fn set_address_enabled<Kind: HostApiKind>(
    ctx: RpcContext,
    params: BindingSetAddressEnabledParams,
    inheritance: Kind::Inheritance,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| set_address_enabled_in::<Kind>(db, params, &inheritance))
        .await
        .result
}

pub(crate) fn set_address_enabled_in<Kind: HostApiKind>(
    db: &mut crate::db::model::DatabaseModel,
    BindingSetAddressEnabledParams {
        internal_port,
        address,
        enabled,
        allow_insecure,
    }: BindingSetAddressEnabledParams,
    inheritance: &Kind::Inheritance,
) -> Result<(), Error> {
    let enabled = enabled.unwrap_or(true);
    if !enabled && address.is_internal() {
        return Err(Error::new(
            eyre!("loopback / bridge (internal) addresses cannot be disabled"),
            ErrorKind::InvalidRequest,
        ));
    }
    Kind::host_for(inheritance, db)?
        .as_bindings_mut()
        .mutate(|bindings| {
            let binding = bindings
                .get_mut(&internal_port)
                .or_not_found(internal_port)?;
            binding.ensure_selectable(&address)?;
            set_address_enabled_with_consent(
                &mut binding.addresses,
                &address,
                enabled,
                allow_insecure,
            )
        })?;
    refresh_addresses::<Kind>(db, inheritance)
}

/// Selects a port-range address through the shared plaintext-consent owner.
pub async fn set_range_address_enabled<Kind: HostApiKind>(
    ctx: RpcContext,
    BindingSetAddressEnabledParams {
        internal_port,
        address,
        enabled,
        allow_insecure,
    }: BindingSetAddressEnabledParams,
    inheritance: Kind::Inheritance,
) -> Result<(), Error> {
    let enabled = enabled.unwrap_or(true);
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
                    set_address_enabled_with_consent(
                        &mut range.addresses,
                        &address,
                        enabled,
                        allow_insecure,
                    )
                })?;
            refresh_addresses::<Kind>(db, &inheritance)
        })
        .await
        .result?;
    Ok(())
}

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct BindingSetGuaWanParams {
    pub(crate) internal_port: u16,
    pub(crate) address: HostnameInfo,
    pub(crate) wan: bool,
}

rpc_toolkit::reflect_ts!(BindingSetGuaWanParams);
rpc_toolkit::ts_export!(BindingSetGuaWanParams, namespaces = [""]);

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct CliBindingSetGuaWanParams {
    #[arg(help = "help.arg.internal-port")]
    internal_port: u16,
    #[arg(long, help = "help.arg.address")]
    address: String,
    #[arg(long, help = "help.arg.gua-wan")]
    wan: bool,
}

fn set_gua_wan_on(
    addresses: &mut DerivedAddressInfo,
    address: &HostnameInfo,
    gua: SocketAddrV6,
    wan: bool,
) {
    let gateway = match &address.metadata {
        HostnameMetadata::Ipv6 { gateway, .. } if !address.ssl => Some(gateway),
        _ => None,
    };
    match gateway {
        Some(gateway) if has_nonssl_public_domain(addresses, gateway, gua.port()) => {
            set_nonssl_wan_group(addresses, gateway, gua.port(), wan);
        }
        Some(gateway) => {
            if wan {
                addresses.gua_wan.insert(gua);
            } else {
                addresses.gua_wan.remove(&gua);
            }
            resolve_nonssl_gua(addresses, gateway, gua.port());
        }
        None => {
            if wan {
                addresses.gua_wan.insert(gua);
                addresses.enabled.insert(SocketAddr::V6(gua));
            } else {
                addresses.gua_wan.remove(&gua);
                addresses.enabled.remove(&SocketAddr::V6(gua));
            }
        }
    }
}

/// Selects WAN exposure for a global-unicast IPv6 address; linked public domains follow.
pub async fn set_gua_wan<Kind: HostApiKind>(
    ctx: RpcContext,
    params: BindingSetGuaWanParams,
    inheritance: Kind::Inheritance,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| set_gua_wan_in::<Kind>(db, params, &inheritance))
        .await
        .result
}

pub(crate) fn set_gua_wan_in<Kind: HostApiKind>(
    db: &mut crate::db::model::DatabaseModel,
    BindingSetGuaWanParams {
        internal_port,
        address,
        wan,
    }: BindingSetGuaWanParams,
    inheritance: &Kind::Inheritance,
) -> Result<(), Error> {
    Kind::host_for(inheritance, db)?
        .as_bindings_mut()
        .mutate(|bindings| {
            bindings
                .get_mut(&internal_port)
                .or_not_found(internal_port)?
                .set_gua_wan(&address, wan)
        })?;
    refresh_addresses::<Kind>(db, inheritance)
}

fn refresh_addresses<Kind: HostApiKind>(
    db: &mut crate::db::model::DatabaseModel,
    inheritance: &Kind::Inheritance,
) -> Result<(), Error> {
    let hostname = ServerHostname::load(db.as_public().as_server_info())?;
    let gateways = db
        .as_public()
        .as_server_info()
        .as_network()
        .as_gateways()
        .de()?;
    let ports = db.as_private().as_available_ports().de()?;
    Kind::host_for(inheritance, db)?.update_addresses(&hostname, &gateways, &ports)
}

async fn cli_set_address_enabled<Kind: HostApiKind>(
    HandlerArgs {
        context,
        parent_method,
        method,
        params,
        inherited_params,
        raw_params,
        ..
    }: HandlerArgs<CliContext, CliBindingSetAddressEnabledParams, Kind::Inheritance>,
) -> Result<(), Error> {
    call_with_address::<Kind>(
        &context,
        parent_method.into_iter().chain(method).join("."),
        raw_params,
        &inherited_params,
        params.internal_port,
        "bindings",
        &params.address,
    )
    .await
}

async fn cli_set_range_address_enabled<Kind: HostApiKind>(
    HandlerArgs {
        context,
        parent_method,
        method,
        params,
        inherited_params,
        raw_params,
        ..
    }: HandlerArgs<CliContext, CliBindingSetAddressEnabledParams, Kind::Inheritance>,
) -> Result<(), Error> {
    call_with_address::<Kind>(
        &context,
        parent_method.into_iter().chain(method).join("."),
        raw_params,
        &inherited_params,
        params.internal_port,
        "bindingRanges",
        &params.address,
    )
    .await
}

async fn cli_set_gua_wan<Kind: HostApiKind>(
    HandlerArgs {
        context,
        parent_method,
        method,
        params,
        inherited_params,
        raw_params,
        ..
    }: HandlerArgs<CliContext, CliBindingSetGuaWanParams, Kind::Inheritance>,
) -> Result<(), Error> {
    call_with_address::<Kind>(
        &context,
        parent_method.into_iter().chain(method).join("."),
        raw_params,
        &inherited_params,
        params.internal_port,
        "bindings",
        &params.address,
    )
    .await
}

async fn call_with_address<Kind: HostApiKind>(
    ctx: &CliContext,
    method: String,
    mut params: Value,
    inheritance: &Kind::Inheritance,
    internal_port: u16,
    binding_collection: &'static str,
    address: &str,
) -> Result<(), Error> {
    let resolved = if address.trim_start().starts_with('{') {
        serde_json::from_str(address).with_kind(ErrorKind::Deserialization)?
    } else {
        let mut pointer: JsonPointer = Kind::host_pointer(inheritance)?;
        pointer.push_end(binding_collection);
        pointer.push_end(&internal_port.to_string());
        pointer.push_end("addresses");
        pointer.push_end("available");
        let dump: Dump = from_value(
            ctx.call_remote::<RpcContext>(
                "db.dump",
                json!({ "pointer": AsRef::<str>::as_ref(&pointer) }),
            )
            .await?,
        )?;
        let available: BTreeSet<HostnameInfo> =
            from_value::<Option<_>>(dump.value)?.or_not_found(internal_port)?;
        let (hostname, port) = parse_address(address);
        let candidates = available
            .iter()
            .filter(|a| a.hostname == hostname && port.is_none_or(|p| served_port(a) == p))
            .collect::<Vec<_>>();
        match &candidates[..] {
            [] => {
                return Err(Error::new(
                    eyre!(
                        "{}\n{}",
                        t!(
                            "net.host.binding.address-not-found",
                            address = address,
                            port = internal_port
                        ),
                        available
                            .iter()
                            .map(|a| format!("  {}", describe(a)))
                            .join("\n"),
                    ),
                    ErrorKind::NotFound,
                ));
            }
            [one] => HostnameInfo::clone(one),
            _ => HostnameInfo::clone(
                choose_custom_display(
                    &t!(
                        "net.host.binding.choose-address",
                        address = address,
                        port = internal_port
                    ),
                    &candidates,
                    |a| describe(a),
                )
                .await?,
            ),
        }
    };
    params["address"] = to_value(&resolved)?;
    ctx.call_remote::<RpcContext>(&method, params).await?;
    Ok(())
}

fn parse_address(address: &str) -> (InternedString, Option<u16>) {
    if let Ok(ip) = address.parse::<IpAddr>() {
        return (InternedString::from_display(&ip), None);
    }
    if let Ok(addr) = address.parse::<SocketAddr>() {
        return (InternedString::from_display(&addr.ip()), Some(addr.port()));
    }
    if let Some(Ok(ip)) = address
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .map(str::parse::<Ipv6Addr>)
    {
        return (InternedString::from_display(&ip), None);
    }
    match address
        .rsplit_once(':')
        .map(|(host, port)| (host, port.parse()))
    {
        Some((host, Ok(port))) => (InternedString::intern(host), Some(port)),
        _ => (InternedString::intern(address), None),
    }
}

/// The port a `None` stands for, as [`set_address_enabled_on`] keys it.
fn served_port(address: &HostnameInfo) -> u16 {
    address.port.unwrap_or(if address.ssl { 443 } else { 80 })
}

fn describe(address: &HostnameInfo) -> String {
    let host = if address.hostname.parse::<Ipv6Addr>().is_ok() {
        format!("[{}]", address.hostname)
    } else {
        address.hostname.to_string()
    };
    let kind = t!(match &address.metadata {
        HostnameMetadata::Ipv4 { .. } => "net.host.binding.address-kind-ipv4",
        HostnameMetadata::Ipv6 { .. } => "net.host.binding.address-kind-ipv6",
        HostnameMetadata::Mdns { .. } => "net.host.binding.address-kind-mdns",
        HostnameMetadata::PrivateDomain { .. } => "net.host.binding.address-kind-private-domain",
        HostnameMetadata::PublicDomain { .. } => "net.host.binding.address-kind-public-domain",
        HostnameMetadata::Plugin { .. } => "net.host.binding.address-kind-plugin",
    });
    let gateways = address.metadata.gateways().join(", ");
    format!(
        "{host}:{}  {kind}{}, {}{}",
        served_port(address),
        if gateways.is_empty() {
            String::new()
        } else {
            format!(
                " {}",
                t!("net.host.binding.address-via", gateways = gateways)
            )
        },
        t!(if address.public {
            "net.host.binding.address-public"
        } else {
            "net.host.binding.address-private"
        }),
        if address.ssl {
            format!(", {}", t!("net.host.binding.address-ssl"))
        } else {
            String::new()
        },
    )
}

#[cfg(test)]
mod default_off_tests;

#[cfg(test)]
mod test {
    use super::*;
    use crate::GatewayId;
    use crate::net::service_interface::{HostnameInfo, HostnameMetadata};
    use crate::util::serde::MaybeUtf8String;

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
    fn server_ui_declares_tcp() {
        assert_eq!(
            BindOptions::server_ui().transport,
            crate::net::transport::Transport::tcp(),
        );
    }

    #[test]
    fn binding_transport_defaults_both_and_round_trips_explicit_choices() {
        use crate::net::transport::Transport;

        let legacy = serde_json::json!({"preferredExternalPort": 5353});
        let options: BindOptions = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(options.transport, Transport::default());
        for (wire, transport, expected) in [
            (
                serde_json::json!("tcp"),
                Transport::tcp(),
                serde_json::json!(["tcp"]),
            ),
            (
                serde_json::json!("udp"),
                Transport::udp(),
                serde_json::json!(["udp"]),
            ),
            (
                serde_json::json!(["udp", "tcp"]),
                Transport::default(),
                serde_json::json!(["tcp", "udp"]),
            ),
        ] {
            let mut input = legacy.clone();
            input["transport"] = wire;
            let options: BindOptions = serde_json::from_value(input).unwrap();
            assert_eq!(options.transport, transport);
            assert_eq!(
                serde_json::to_value(options).unwrap()["transport"],
                expected
            );
        }
        let range: RangeBindInfo = serde_json::from_value(serde_json::json!({
            "enabled": true, "externalStartPort": 42000, "numberOfPorts": 10,
        }))
        .unwrap();
        assert_eq!(range.transport, Transport::default());
        assert!(
            serde_json::from_value::<BindOptions>(serde_json::json!({
                "preferredExternalPort": 5353, "transport": "sctp",
            }))
            .is_err()
        );
    }

    #[test]
    fn udp_bindings_reject_stream_proxy_and_tls_options() {
        for options in [
            serde_json::json!({"proxy": {"type": "tcp"}}),
            serde_json::json!({"proxy": {"type": "http"}}),
            serde_json::json!({"addSsl": {"preferredExternalPort": 443}}),
            serde_json::json!({"secure": {"ssl": true}}),
        ] {
            for transport in [serde_json::json!("udp"), serde_json::json!(["udp", "udp"])] {
                let mut input = options.clone();
                input["preferredExternalPort"] = 5353.into();
                input["transport"] = transport;
                assert!(serde_json::from_value::<BindOptions>(input).is_err());
            }
        }
    }

    fn opts(preferred: u16, add_ssl: Option<u16>, ssl: Option<bool>) -> BindOptions {
        BindOptions {
            default_off: false,
            transport: Default::default(),
            preferred_external_port: preferred,
            add_ssl: add_ssl.map(|preferred_external_port| AddSslOptions {
                preferred_external_port,
                alpn: None,
                upstream_cert_validation: None,
            }),
            secure: ssl.map(|ssl| Security { ssl }),
            proxy: None,
        }
    }

    fn add_ssl_with_alpn(alpn: serde_json::Value) -> AddSslOptions {
        serde_json::from_value(serde_json::json!({
            "preferredExternalPort": 443,
            "alpn": alpn,
        }))
        .unwrap()
    }

    #[test]
    fn binding_creation_reports_dynamic_port_exhaustion_for_either_leg() {
        for options in [opts(0, None, Some(false)), opts(0, None, Some(true))] {
            let mut ports = AvailablePorts::new();
            ports.try_alloc_range(49152, 16384, true).unwrap();
            let error = BindInfo::new(&mut ports, options, true).unwrap_err();
            assert_eq!(error.kind, ErrorKind::Network);
        }
    }

    #[test]
    fn legacy_ssl_policy_normalizes_into_binding_http_proxy() {
        for (headers, auth) in [
            (true, serde_json::Value::Null),
            (
                false,
                serde_json::json!({ "type": "bearer", "tokens": ["secret"] }),
            ),
            (
                true,
                serde_json::json!({ "type": "basic", "credentials": [{ "username": "alice", "password": "secret" }] }),
            ),
        ] {
            let legacy = serde_json::json!({
                "preferredExternalPort": 8080,
                "secure": null,
                "addSsl": { "preferredExternalPort": 8443, "addXForwardedHeaders": headers, "auth": auth },
            });
            let options: BindOptions = serde_json::from_value(legacy).unwrap();
            let serialized = serde_json::to_value(&options).unwrap();
            assert_eq!(serialized["proxy"]["type"], "http");
            assert_eq!(serialized["proxy"]["addXForwardedHeaders"], headers);
            assert_eq!(serialized["proxy"]["auth"]["type"], auth["type"]);
            assert!(serialized["addSsl"].get("auth").is_none());
            assert!(serialized["addSsl"].get("addXForwardedHeaders").is_none());
            assert_eq!(serialized["addSsl"]["preferredExternalPort"], 8443);
            let round_trip: BindOptions = serde_json::from_value(serialized.clone()).unwrap();
            assert_eq!(serde_json::to_value(round_trip).unwrap(), serialized);
        }
    }

    #[test]
    fn http_proxy_is_independent_of_ssl_and_rejects_passthrough() {
        let plain = serde_json::json!({
            "preferredExternalPort": 8080,
            "addSsl": null,
            "secure": null,
            "proxy": { "type": "http" },
        });
        let options: BindOptions = serde_json::from_value(plain.clone()).unwrap();
        assert!(matches!(
            options.proxy,
            Some(ProxyOptions::Http {
                add_x_forwarded_headers: false,
                auth: None
            })
        ));
        let mut passthrough = plain;
        passthrough["secure"] = serde_json::json!({ "ssl": true });
        assert!(serde_json::from_value::<BindOptions>(passthrough.clone()).is_err());
        passthrough["addSsl"] = serde_json::json!({ "preferredExternalPort": 8443 });
        assert!(serde_json::from_value::<BindOptions>(passthrough).is_ok());
        let raw: BindOptions = serde_json::from_value(serde_json::json!({
            "preferredExternalPort": 8333,
            "addSsl": { "preferredExternalPort": 8443, "addXForwardedHeaders": false },
        }))
        .unwrap();
        assert!(raw.proxy.is_none());
    }

    #[test]
    fn tcp_proxy_round_trips_plain_and_tls_passthrough_without_http_policy() {
        for secure in [serde_json::Value::Null, serde_json::json!({"ssl": true})] {
            let input = serde_json::json!({
                "transport": ["tcp"],
                "preferredExternalPort": 6881,
                "addSsl": null,
                "secure": secure,
                "proxy": {"type": "tcp"},
            });
            let options: BindOptions = serde_json::from_value(input.clone()).unwrap();
            assert_eq!(options.proxy, Some(ProxyOptions::Tcp));
            assert!(!options.default_off);
            let mut expected = input;
            expected["defaultOff"] = serde_json::json!(false);
            assert_eq!(serde_json::to_value(options).unwrap(), expected);
        }
    }

    #[test]
    fn bridge_address_is_the_lxcbr0_row_not_a_lookalike() {
        let row = |ssl, hostname: &str, port, metadata| HostnameInfo {
            ssl,
            public: false,
            hostname: InternedString::intern(hostname),
            port: Some(port),
            metadata,
        };
        let bridge = || HostnameMetadata::Ipv4 {
            gateway: START9_BRIDGE_IFACE.parse().unwrap(),
        };
        let addresses = DerivedAddressInfo {
            available: [
                row(
                    false,
                    "10.0.3.1",
                    1,
                    HostnameMetadata::Plugin {
                        package_id: "decoy".parse().unwrap(),
                        remove_action: None,
                        overflow_actions: vec![],
                        info: Value::Null,
                    },
                ),
                row(
                    false,
                    "192.168.1.5",
                    8080,
                    HostnameMetadata::Ipv4 {
                        gateway: "eth0".parse().unwrap(),
                    },
                ),
                row(false, "10.0.3.1", 8080, bridge()),
                row(true, "10.0.3.1", 8443, bridge()),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        let addr = |ssl| bridge_address(&addresses, ssl).map(|a| a.to_string());
        assert_eq!(addr(false).as_deref(), Some("10.0.3.1:8080"));
        assert_eq!(addr(true).as_deref(), Some("10.0.3.1:8443"));
    }

    #[test]
    fn add_ssl_accepts_current_and_legacy_alpn_formats() {
        let protocols = AlpnInfo(vec![
            MaybeUtf8String(b"h2".to_vec()),
            MaybeUtf8String(b"http/1.1".to_vec()),
        ]);

        for json in [
            serde_json::json!(["h2", "http/1.1"]),
            serde_json::json!({ "specified": ["h2", "http/1.1"] }),
        ] {
            let options = add_ssl_with_alpn(json);
            assert_eq!(options.alpn, Some(protocols.clone()));
            assert_eq!(
                serde_json::to_value(options).unwrap()["alpn"],
                serde_json::json!(["h2", "http/1.1"]),
            );
        }

        for json in [serde_json::Value::Null, serde_json::json!("reflect")] {
            let options = add_ssl_with_alpn(json);
            assert_eq!(options.alpn, None);
            assert_eq!(
                serde_json::to_value(options).unwrap()["alpn"],
                serde_json::Value::Null,
            );
        }
    }

    #[test]
    fn tls_carrying_ports_are_ssl_ports() {
        let mut ports = AvailablePorts::new();
        let privileged = false;

        // plaintext: one forwarded port
        let plain = BindInfo::new(&mut ports, opts(8080, None, Some(false)), privileged).unwrap();
        assert_eq!(plain.net.assigned_port, Some(8080));
        assert_eq!(plain.net.assigned_ssl_port, None);
        assert!(!ports.is_ssl(8080));

        // we terminate TLS in front of a plaintext container: both ports
        let add_ssl = BindInfo::new(&mut ports, opts(8081, Some(8444), None), privileged).unwrap();
        assert_eq!(add_ssl.net.assigned_port, Some(8081));
        assert_eq!(add_ssl.net.assigned_ssl_port, Some(8444));
        assert!(ports.is_ssl(8444));

        // we rewrap the container's TLS: the ssl port only
        let rewrap =
            BindInfo::new(&mut ports, opts(8082, Some(8445), Some(true)), privileged).unwrap();
        assert_eq!(rewrap.net.assigned_port, None);
        assert_eq!(rewrap.net.assigned_ssl_port, Some(8445));

        // the container serves its own TLS: the ssl port only, fronted by
        // our SNI-passthrough listener
        let own_tls = BindInfo::new(&mut ports, opts(8083, None, Some(true)), privileged).unwrap();
        assert_eq!(own_tls.net.assigned_port, None);
        assert_eq!(own_tls.net.assigned_ssl_port, Some(8083));
        assert!(ports.is_ssl(8083));
    }

    #[test]
    fn changing_how_a_port_is_served_keeps_its_number() {
        let mut ports = AvailablePorts::new();
        let privileged = false;
        let plain = BindInfo::new(&mut ports, opts(8080, None, Some(false)), privileged).unwrap();
        assert_eq!(plain.net.assigned_port, Some(8080));

        let own_tls = plain
            .update(&mut ports, opts(8080, None, Some(true)), privileged)
            .unwrap();
        assert_eq!(own_tls.net.assigned_port, None);
        assert_eq!(own_tls.net.assigned_ssl_port, Some(8080));
        assert!(ports.is_ssl(8080));

        // handing termination to us keeps the port
        let add_ssl = own_tls
            .update(&mut ports, opts(8080, Some(8080), Some(true)), privileged)
            .unwrap();
        assert_eq!(add_ssl.net.assigned_ssl_port, Some(8080));
        assert!(ports.is_ssl(8080));

        // and back down to plaintext
        let plain = add_ssl
            .update(&mut ports, opts(8080, None, Some(false)), privileged)
            .unwrap();
        assert_eq!(plain.net.assigned_port, Some(8080));
        assert_eq!(plain.net.assigned_ssl_port, None);
        assert!(!ports.is_ssl(8080));
    }

    /// Retiring has to hand both external ports back. Disabling — all
    /// `clearBindings` does — keeps them claimed for the life of the server.
    #[test]
    fn release_frees_both_ports() {
        let mut ports = AvailablePorts::new();
        let privileged = false;

        let bind = BindInfo::new(&mut ports, opts(8081, Some(8444), None), privileged).unwrap();
        assert_eq!(bind.net.assigned_port, Some(8081));
        assert_eq!(bind.net.assigned_ssl_port, Some(8444));
        // not vacuous: both are held before the release
        assert_eq!(ports.try_alloc(8081, false, privileged), None);
        assert_eq!(ports.try_alloc(8444, true, privileged), None);

        bind.release(&mut ports);

        assert_eq!(ports.try_alloc(8081, false, privileged), Some(8081));
        assert_eq!(ports.try_alloc(8444, true, privileged), Some(8444));
    }

    #[test]
    fn release_leaves_other_bindings_alone() {
        let mut ports = AvailablePorts::new();
        let privileged = false;
        let neighbor =
            BindInfo::new(&mut ports, opts(9000, None, Some(false)), privileged).unwrap();
        let bind = BindInfo::new(&mut ports, opts(8080, None, Some(false)), privileged).unwrap();
        assert_eq!(bind.net.assigned_ssl_port, None);

        bind.release(&mut ports);

        assert_eq!(ports.try_alloc(8080, false, privileged), Some(8080));
        assert_eq!(ports.try_alloc(9000, false, privileged), None);
        assert_eq!(neighbor.net.assigned_port, Some(9000));
    }

    /// The whole span, inclusive — and a range ending at 65535 must not
    /// overflow u16 on the way back to the pool.
    #[test]
    fn release_frees_the_whole_range_span() {
        let mut ports = AvailablePorts::new();
        let privileged = false;
        ports.try_alloc_range(65534, 2, privileged).unwrap();
        let range = RangeBindInfo {
            enabled: true,
            transport: Default::default(),
            external_start_port: 65534,
            number_of_ports: 2,
            addresses: Default::default(),
            interface: None,
        };
        assert_eq!(ports.try_alloc(65535, false, privileged), None);

        range.release(&mut ports);

        // succeeds only if both 65534 and 65535 came back
        ports.try_alloc_range(65534, 2, privileged).unwrap();
    }

    #[test]
    fn a_rebind_does_not_migrate_onto_a_freed_preferred_port() {
        let mut ports = AvailablePorts::new();
        let privileged = false;
        // someone else holds 8080, so this binding lands elsewhere
        assert_eq!(ports.try_alloc(8080, false, privileged), Some(8080));
        let squatted =
            BindInfo::new(&mut ports, opts(8080, None, Some(false)), privileged).unwrap();
        let assigned = squatted.net.assigned_port.unwrap();
        assert_ne!(assigned, 8080);

        ports.free([8080]);
        let again = squatted
            .update(&mut ports, opts(8080, None, Some(false)), privileged)
            .unwrap();
        assert_eq!(again.net.assigned_port, Some(assigned));
    }

    /// The admin UI rebinds on every boot, so `update` is what hands 443 back.
    #[test]
    fn the_os_ui_keeps_its_well_known_ports_across_rebinds() {
        let mut ports = AvailablePorts::new();
        let privileged = true;
        // exactly what `NetController::os_bindings` asks for
        let admin = opts(80, Some(443), None);

        let ui = BindInfo::new(&mut ports, admin.clone(), privileged).unwrap();
        assert_eq!(ui.net.assigned_port, Some(80));
        assert_eq!(ui.net.assigned_ssl_port, Some(443));
        assert!(ports.is_ssl(443));

        let ui = ui.update(&mut ports, admin, privileged).unwrap();
        assert_eq!(ui.net.assigned_port, Some(80));
        assert_eq!(ui.net.assigned_ssl_port, Some(443));
        assert!(ports.is_ssl(443));
    }

    /// `Public::init` plants the binding already holding 443 but no plaintext
    /// port, so `os_bindings` only ever reaches it through `update` — the shape
    /// the test above skips by starting from `new`, which is how the UI came to
    /// ship on an ephemeral port through 0.4.0.
    #[test]
    fn the_os_ui_claims_80_from_the_seeded_binding() {
        let mut ports = AvailablePorts::new();
        let admin = opts(80, Some(443), None);
        let seeded = BindInfo {
            enabled: false,
            options: admin.clone(),
            net: NetInfo {
                assigned_port: None,
                assigned_ssl_port: Some(443),
            },
            addresses: DerivedAddressInfo::default(),
            interfaces: BTreeMap::new(),
        };

        let ui = seeded.update(&mut ports, admin.clone(), true).unwrap();
        assert_eq!(ui.net.assigned_port, Some(80));
        assert_eq!(ui.net.assigned_ssl_port, Some(443));

        let ui = ui.update(&mut ports, admin, true).unwrap();
        assert_eq!(ui.net.assigned_port, Some(80));
        assert_eq!(ui.net.assigned_ssl_port, Some(443));
    }

    /// A binding that grows a second leg must not hand the port it already has to
    /// the new one — each leg keeps its own.
    #[test]
    fn gaining_a_leg_does_not_carry_the_other_leg_s_port() {
        let mut ports = AvailablePorts::new();
        let privileged = false;
        let plain = BindInfo::new(&mut ports, opts(8080, None, Some(false)), privileged).unwrap();
        assert_eq!(plain.net.assigned_port, Some(8080));

        let both = plain
            .update(&mut ports, opts(8080, Some(8443), None), privileged)
            .unwrap();
        assert_eq!(both.net.assigned_port, Some(8080));
        assert_eq!(both.net.assigned_ssl_port, Some(8443));
    }

    /// The seeded admin binding is the same shape from the other side, and its
    /// plaintext leg must want 80 outright — not 443 with 80 as the fallback the
    /// ssl leg's head start happens to force.
    #[test]
    fn the_os_ui_plaintext_leg_wants_80_even_when_443_is_free() {
        let admin = opts(80, Some(443), None);
        let seeded = || BindInfo {
            enabled: false,
            options: admin.clone(),
            net: NetInfo {
                assigned_port: None,
                assigned_ssl_port: Some(443),
            },
            addresses: DerivedAddressInfo::default(),
            interfaces: BTreeMap::new(),
        };

        let mut ports = AvailablePorts::new();
        let ui = seeded().update(&mut ports, admin.clone(), true).unwrap();
        assert_eq!(ui.net.assigned_port, Some(80));
        assert_eq!(ui.net.assigned_ssl_port, Some(443));

        // 443 already taken, so the ssl leg cannot clear it out of the plain leg's way.
        let mut squatted = AvailablePorts::new();
        assert_eq!(squatted.try_alloc(443, true, true), Some(443));
        let ui = seeded().update(&mut squatted, admin, true).unwrap();
        assert_eq!(ui.net.assigned_port, Some(80));
    }

    /// Why the drift needs `v0_4_0_2` rather than healing itself: `update`
    /// keeps the port it already holds even for a privileged claimant whose
    /// preferred port is free.
    #[test]
    fn a_drifted_os_ui_port_does_not_heal_on_rebind() {
        let mut ports = AvailablePorts::new();
        ports.set_ssl(443, true);
        ports.set_ssl(55543, false);
        let admin = opts(80, Some(443), None);
        let drifted = BindInfo {
            enabled: false,
            options: admin.clone(),
            net: NetInfo {
                assigned_port: Some(55543),
                assigned_ssl_port: Some(443),
            },
            addresses: DerivedAddressInfo::default(),
            interfaces: BTreeMap::new(),
        };

        let ui = drifted.update(&mut ports, admin, true).unwrap();
        assert_eq!(ui.net.assigned_port, Some(55543));
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
    fn nonssl_wan_group_moves_domain_v4_gua_together() {
        let gw = GatewayId::from(InternedString::intern("wg1"));
        let mk = |ssl, public, host: &str, meta| HostnameInfo {
            ssl,
            public,
            hostname: InternedString::intern(host),
            port: Some(42000),
            metadata: meta,
        };
        let mut info = DerivedAddressInfo::default();
        info.available.insert(mk(
            false,
            true,
            "turn.start9.dev",
            HostnameMetadata::PublicDomain {
                gateway: gw.clone(),
            },
        ));
        info.available.insert(mk(
            false,
            true,
            "64.23.194.12",
            HostnameMetadata::Ipv4 {
                gateway: gw.clone(),
            },
        ));
        info.available.insert(mk(
            false,
            false,
            "2001:db8::1",
            HostnameMetadata::Ipv6 {
                gateway: gw.clone(),
                scope_id: 0,
            },
        ));

        assert!(has_nonssl_public_domain(&info, &gw, 42000));
        assert!(!has_nonssl_public_domain(&info, &gw, 9999));

        let v4_sa: SocketAddr = "64.23.194.12:42000".parse().unwrap();
        let gua_v6: SocketAddrV6 = "[2001:db8::1]:42000".parse().unwrap();
        let dom_key = (InternedString::intern("turn.start9.dev"), 42000u16);

        // Enable: domain un-disabled, v4 enabled, GUA published (gua_wan+enabled).
        set_nonssl_wan_group(&mut info, &gw, 42000, true);
        assert!(!info.disabled.contains(&dom_key));
        assert!(info.enabled.contains(&v4_sa));
        assert!(info.gua_wan.contains(&gua_v6));
        assert!(info.enabled.contains(&SocketAddr::V6(gua_v6)));

        // Disable: all three off together.
        set_nonssl_wan_group(&mut info, &gw, 42000, false);
        assert!(info.disabled.contains(&dom_key));
        assert!(!info.enabled.contains(&v4_sa));
        assert!(!info.gua_wan.contains(&gua_v6));
        assert!(!info.enabled.contains(&SocketAddr::V6(gua_v6)));
    }

    #[test]
    fn public_domain_exposure_keeps_refreshed_gua_rows_enabled_and_public() {
        let domain = HostnameInfo {
            ssl: false,
            public: true,
            hostname: InternedString::intern("smp.example.com"),
            port: Some(5223),
            metadata: HostnameMetadata::PublicDomain {
                gateway: GatewayId::from(InternedString::intern("eth0")),
            },
        };
        let gua = HostnameInfo {
            ssl: false,
            ..ipv6_addr("2001:db8::1", 5223)
        };
        let mut info = DerivedAddressInfo::default();
        info.available.insert(domain.clone());
        info.disabled.insert(override_key(&gua));
        refresh(&mut info, BTreeSet::from([domain.clone(), gua.clone()]));
        let published = info.available.iter().find(|a| a.gua().is_some()).unwrap();
        assert!(published.public);
        assert!(info.enabled().contains(published));

        let next_gua = HostnameInfo {
            ssl: false,
            ..ipv6_addr("2001:db8::2", 5223)
        };
        refresh(
            &mut info,
            BTreeSet::from([domain.clone(), next_gua.clone()]),
        );
        let published = info.available.iter().find(|a| a.gua().is_some()).unwrap();
        assert!(published.public);
        assert!(info.enabled().contains(published));

        set_nonssl_wan_group(
            &mut info,
            &GatewayId::from(InternedString::intern("eth0")),
            5223,
            false,
        );
        refresh(&mut info, BTreeSet::from([domain, next_gua]));
        assert!(
            info.available
                .iter()
                .all(|a| a.gua().is_none() || !a.public)
        );
    }

    fn refresh(info: &mut DerivedAddressInfo, available: BTreeSet<HostnameInfo>) {
        let previous = info.clone();
        info.set_available(available.clone(), false);
        info.resolve_address_groups(&previous);
        info.set_available(available, false);
    }

    fn domain_row(hostname: &str, ssl: bool, port: u16) -> HostnameInfo {
        HostnameInfo {
            ssl,
            public: true,
            hostname: InternedString::intern(hostname),
            port: Some(port),
            metadata: HostnameMetadata::PublicDomain {
                gateway: GatewayId::from(InternedString::intern("eth0")),
            },
        }
    }

    fn plain_wan_rows(port: u16) -> [HostnameInfo; 2] {
        [
            HostnameInfo {
                ssl: false,
                public: true,
                hostname: InternedString::intern("203.0.113.1"),
                port: Some(port),
                metadata: HostnameMetadata::Ipv4 {
                    gateway: GatewayId::from(InternedString::intern("eth0")),
                },
            },
            HostnameInfo {
                ssl: false,
                ..ipv6_addr("2001:db8::1", port)
            },
        ]
    }

    fn assert_wan_group_off(info: &DerivedAddressInfo) {
        assert!(info.gua_wan.is_empty());
        assert!(info.enabled.is_empty());
        assert!(info.enabled().iter().all(|a| !a.public));
    }

    #[test]
    fn a_domain_disabled_over_tls_keeps_the_wan_group_off_on_the_same_port() {
        let tls = domain_row("smp.example.com", true, 5223);
        let mut info = DerivedAddressInfo::default();
        info.available.insert(tls.clone());
        info.disabled.insert(override_key(&tls));
        info.enabled.insert("203.0.113.1:5223".parse().unwrap());
        let refreshed: BTreeSet<_> = plain_wan_rows(5223)
            .into_iter()
            .chain([domain_row("smp.example.com", false, 5223)])
            .collect();

        refresh(&mut info, refreshed.clone());
        assert_wan_group_off(&info);

        refresh(&mut info, refreshed);
        assert_wan_group_off(&info);
    }

    #[test]
    fn a_domain_disabled_over_tls_stays_off_on_a_new_plain_port() {
        let tls = domain_row("smp.example.com", true, 5224);
        let plain = domain_row("smp.example.com", false, 5223);
        let mut info = DerivedAddressInfo::default();
        info.available.insert(tls.clone());
        info.disabled.insert(override_key(&tls));
        let refreshed: BTreeSet<_> = plain_wan_rows(5223)
            .into_iter()
            .chain([plain.clone()])
            .collect();

        refresh(&mut info, refreshed.clone());
        assert_wan_group_off(&info);
        assert!(info.disabled.contains(&override_key(&plain)));

        let gateway = GatewayId::from(InternedString::intern("eth0"));
        set_nonssl_wan_group(&mut info, &gateway, 5223, true);
        refresh(&mut info, refreshed);
        assert!(info.enabled().contains(&plain));
        assert_eq!(info.gua_wan.len(), 1);
    }

    #[test]
    fn a_disabled_domain_turns_off_an_enabled_domain_that_can_no_longer_be_disjoint() {
        let on = domain_row("a.example.com", true, 5223);
        let off = domain_row("b.example.com", true, 5223);
        let mut info = DerivedAddressInfo::default();
        info.available.extend([on, off.clone()]);
        info.disabled.insert(override_key(&off));

        refresh(
            &mut info,
            plain_wan_rows(5223)
                .into_iter()
                .chain([
                    domain_row("a.example.com", false, 5223),
                    domain_row("b.example.com", false, 5223),
                ])
                .collect(),
        );
        assert_wan_group_off(&info);
    }

    #[test]
    fn a_disabled_tls_row_leaves_its_domains_existing_plain_row_alone() {
        let tls = domain_row("smp.example.com", true, 443);
        let plain = domain_row("smp.example.com", false, 5223);
        let mut info = DerivedAddressInfo::default();
        info.available.extend([tls.clone(), plain.clone()]);
        info.disabled.insert(override_key(&tls));

        refresh(&mut info, BTreeSet::from([tls, plain.clone()]));
        assert!(info.enabled().contains(&plain));
    }

    #[test]
    fn a_domain_disabled_on_a_plain_port_stays_off_over_tls() {
        let plain = domain_row("smp.example.com", false, 5223);
        let tls = domain_row("smp.example.com", true, 5224);
        let mut info = DerivedAddressInfo::default();
        info.available.insert(plain.clone());
        info.disabled.insert(override_key(&plain));

        refresh(&mut info, BTreeSet::from([tls.clone()]));
        assert!(!info.enabled().contains(&tls));
    }

    #[test]
    fn an_enabled_domain_stays_enabled_when_its_tls_port_moves() {
        let mut info = DerivedAddressInfo::default();
        info.available
            .insert(domain_row("smp.example.com", true, 5224));
        let moved = domain_row("smp.example.com", true, 5225);

        refresh(&mut info, BTreeSet::from([moved.clone()]));
        assert!(info.enabled().contains(&moved));
    }

    #[test]
    fn a_domain_with_no_earlier_row_stays_off() {
        let tls = domain_row("smp.example.com", true, 5224);
        let plain = domain_row("smp.example.com", false, 5223);
        let mut info = DerivedAddressInfo::default();

        refresh(&mut info, BTreeSet::from([tls.clone(), plain.clone()]));
        assert!(!info.enabled().contains(&tls));
        assert!(!info.enabled().contains(&plain));
    }

    #[test]
    fn an_enabled_domain_stays_enabled_when_its_gateway_changes() {
        let mut info = DerivedAddressInfo::default();
        info.available
            .insert(domain_row("smp.example.com", true, 5224));
        let moved = HostnameInfo {
            metadata: HostnameMetadata::PublicDomain {
                gateway: GatewayId::from(InternedString::intern("wg1")),
            },
            ..domain_row("smp.example.com", true, 5224)
        };

        refresh(&mut info, BTreeSet::from([moved.clone()]));
        assert!(info.enabled().contains(&moved));
    }

    fn public_v4(port: u16) -> HostnameInfo {
        plain_wan_rows(port)[0].clone()
    }

    #[test]
    fn a_new_plain_domain_follows_public_addresses_that_are_all_on() {
        let bare = "203.0.113.1:5223".parse().unwrap();
        let plain = domain_row("smp.example.com", false, 5223);
        let mut info = DerivedAddressInfo::default();
        info.available.insert(public_v4(5223));
        info.enabled.insert(bare);

        refresh(
            &mut info,
            plain_wan_rows(5223)
                .into_iter()
                .chain([plain.clone()])
                .collect(),
        );
        assert!(info.enabled().contains(&plain));
        assert!(info.enabled.contains(&bare));
        assert_eq!(info.gua_wan.len(), 1);
    }

    #[test]
    fn a_new_plain_domain_follows_public_addresses_that_are_all_off() {
        let plain = domain_row("smp.example.com", false, 5223);
        let mut info = DerivedAddressInfo::default();
        info.available.insert(public_v4(5223));

        refresh(
            &mut info,
            plain_wan_rows(5223)
                .into_iter()
                .chain([plain.clone()])
                .collect(),
        );
        assert_wan_group_off(&info);
        assert!(!info.enabled().contains(&plain));
    }

    #[test]
    fn a_new_plain_domain_turns_off_public_addresses_that_were_mixed() {
        let off = domain_row("off.example.com", false, 5223);
        let plain = domain_row("smp.example.com", false, 5223);
        let mut info = DerivedAddressInfo::default();
        info.available.extend([public_v4(5223), off.clone()]);
        info.enabled.insert("203.0.113.1:5223".parse().unwrap());
        info.disabled.insert(override_key(&off));

        refresh(
            &mut info,
            plain_wan_rows(5223)
                .into_iter()
                .chain([off, plain.clone()])
                .collect(),
        );
        assert_wan_group_off(&info);
        assert!(!info.enabled().contains(&plain));
    }

    #[test]
    fn a_domain_enabled_over_tls_turns_off_on_a_plain_port_beside_a_local_gua() {
        let plain = domain_row("smp.example.com", false, 5223);
        let mut info = DerivedAddressInfo::default();
        info.available.extend([
            domain_row("smp.example.com", true, 5223),
            ipv6_addr("2001:db8::1", 5223),
        ]);

        refresh(
            &mut info,
            plain_wan_rows(5223)
                .into_iter()
                .chain([plain.clone()])
                .collect(),
        );
        assert_wan_group_off(&info);
        assert!(!info.enabled().contains(&plain));
    }

    #[test]
    fn an_enabled_plain_domain_beside_a_local_gua_turns_off() {
        let plain = domain_row("smp.example.com", false, 5223);
        let rows: BTreeSet<_> = plain_wan_rows(5223)
            .into_iter()
            .chain([plain.clone()])
            .collect();
        let mut info = DerivedAddressInfo::default();
        info.available = rows.clone();
        info.enabled.insert("203.0.113.1:5223".parse().unwrap());

        refresh(&mut info, rows);
        assert_wan_group_off(&info);
        assert!(!info.enabled().contains(&plain));
    }

    #[test]
    fn a_renumbered_gua_joins_a_plain_group_that_was_all_on() {
        let plain = domain_row("smp.example.com", false, 5223);
        let old_gua = HostnameInfo {
            ssl: false,
            public: true,
            ..ipv6_addr("2001:db8::1", 5223)
        };
        let new_gua = HostnameInfo {
            ssl: false,
            ..ipv6_addr("2001:db8::2", 5223)
        };
        let mut info = DerivedAddressInfo::default();
        info.available.extend([plain.clone(), old_gua.clone()]);
        info.enabled.insert(old_gua.to_socket_addr().unwrap());
        info.gua_wan.insert(old_gua.gua().unwrap());

        refresh(&mut info, BTreeSet::from([plain.clone(), new_gua.clone()]));
        assert!(info.enabled().contains(&plain));
        assert!(info.gua_wan.contains(&new_gua.gua().unwrap()));
        assert!(info.enabled.contains(&new_gua.to_socket_addr().unwrap()));
    }

    #[test]
    fn a_local_gua_is_public_off_so_public_ipv4_beside_it_is_mixed() {
        let plain = domain_row("smp.example.com", false, 5223);
        let mut info = DerivedAddressInfo::default();
        info.available.extend(plain_wan_rows(5223));
        info.enabled.insert("203.0.113.1:5223".parse().unwrap());

        refresh(
            &mut info,
            plain_wan_rows(5223)
                .into_iter()
                .chain([plain.clone()])
                .collect(),
        );
        assert_wan_group_off(&info);
        assert!(!info.enabled().contains(&plain));
    }

    #[test]
    fn a_disabled_public_gua_keeps_a_tls_to_plain_group_off() {
        let gua = HostnameInfo {
            public: true,
            ..ipv6_addr("2001:db8::1", 5223)
        };
        let mut info = DerivedAddressInfo::default();
        info.available
            .extend([domain_row("smp.example.com", true, 5223), gua.clone()]);
        info.gua_wan.insert(gua.gua().unwrap());
        set_address_enabled_on(&mut info, &gua, true).unwrap();
        set_address_enabled_on(&mut info, &gua, false).unwrap();
        assert!(info.gua_wan.contains(&gua.gua().unwrap()));
        assert!(!info.enabled().contains(&gua));

        refresh(
            &mut info,
            BTreeSet::from([
                domain_row("smp.example.com", false, 5223),
                HostnameInfo { ssl: false, ..gua },
            ]),
        );
        assert_wan_group_off(&info);
    }

    #[test]
    fn a_domain_enabled_over_tls_stays_on_a_plain_port_with_public_bare_ips() {
        let plain = domain_row("smp.example.com", false, 5223);
        let tls_bare = "203.0.113.1:5224".parse().unwrap();
        let mut info = DerivedAddressInfo::default();
        info.available.extend([
            domain_row("smp.example.com", true, 5224),
            HostnameInfo {
                ssl: true,
                public: true,
                hostname: InternedString::intern("203.0.113.1"),
                port: Some(5224),
                metadata: HostnameMetadata::Ipv4 {
                    gateway: GatewayId::from(InternedString::intern("eth0")),
                },
            },
        ]);
        info.enabled.insert(tls_bare);

        refresh(
            &mut info,
            plain_wan_rows(5223)
                .into_iter()
                .chain([plain.clone()])
                .collect(),
        );
        assert!(info.enabled().contains(&plain));
        assert!(info.enabled.contains(&"203.0.113.1:5223".parse().unwrap()));
        assert_eq!(info.gua_wan.len(), 1);
    }

    #[test]
    fn toggling_any_plain_group_member_moves_the_whole_group() {
        let plain = domain_row("smp.example.com", false, 5223);
        let [v4, gua] = plain_wan_rows(5223);
        let gua = HostnameInfo {
            public: true,
            ..gua
        };
        let gateway = GatewayId::from(InternedString::intern("eth0"));
        for member in [&plain, &v4, &gua] {
            let mut info = DerivedAddressInfo::default();
            info.available
                .extend([plain.clone(), v4.clone(), gua.clone()]);
            set_nonssl_wan_group(&mut info, &gateway, 5223, false);

            set_address_enabled_on(&mut info, member, true).unwrap();
            assert!(info.enabled().contains(&plain));
            assert!(info.enabled.contains(&v4.to_socket_addr().unwrap()));
            assert!(info.gua_wan.contains(&gua.gua().unwrap()));

            set_address_enabled_on(&mut info, member, false).unwrap();
            assert!(!info.enabled().contains(&plain));
            assert!(!info.enabled.contains(&v4.to_socket_addr().unwrap()));
            assert!(!info.gua_wan.contains(&gua.gua().unwrap()));
        }
    }

    #[test]
    fn a_new_public_domain_does_not_preempt_sibling_reconciliation() {
        let domain = HostnameInfo {
            ssl: false,
            public: true,
            hostname: InternedString::intern("smp.example.com"),
            port: Some(5223),
            metadata: HostnameMetadata::PublicDomain {
                gateway: GatewayId::from(InternedString::intern("eth0")),
            },
        };
        let gua = HostnameInfo {
            ssl: false,
            ..ipv6_addr("2001:db8::1", 5223)
        };
        let mut info = DerivedAddressInfo::default();
        refresh(&mut info, BTreeSet::from([domain, gua]));
        assert!(info.gua_wan.is_empty());
        assert!(info.enabled.is_empty());
        assert!(
            info.available
                .iter()
                .all(|a| a.gua().is_none() || !a.public)
        );
    }

    #[test]
    fn a_public_domain_does_not_publish_other_exposures() {
        let domain = HostnameInfo {
            ssl: false,
            public: true,
            hostname: InternedString::intern("smp.example.com"),
            port: Some(5223),
            metadata: HostnameMetadata::PublicDomain {
                gateway: GatewayId::from(InternedString::intern("eth0")),
            },
        };
        let other_port = HostnameInfo {
            ssl: false,
            ..ipv6_addr("2001:db8::1", 443)
        };
        let other_gateway = HostnameInfo {
            ssl: false,
            metadata: HostnameMetadata::Ipv6 {
                gateway: GatewayId::from(InternedString::intern("eth1")),
                scope_id: 0,
            },
            ..ipv6_addr("2001:db8::2", 5223)
        };
        let mut info = DerivedAddressInfo::default();
        info.available.insert(domain.clone());
        refresh(
            &mut info,
            BTreeSet::from([domain.clone(), other_port, other_gateway]),
        );
        assert!(
            info.available
                .iter()
                .all(|a| a.gua().is_none() || !a.public)
        );

        refresh(
            &mut info,
            BTreeSet::from([
                HostnameInfo {
                    ssl: true,
                    ..domain
                },
                ipv6_addr("2001:db8::1", 5223),
            ]),
        );
        assert!(
            info.available
                .iter()
                .all(|a| a.gua().is_none() || !a.public)
        );
    }

    #[test]
    fn shared_gua_public_wins_over_local() {
        let gw = GatewayId::from(InternedString::intern("wg1"));
        let mk = |ssl, public, host: &str, meta| HostnameInfo {
            ssl,
            public,
            hostname: InternedString::intern(host),
            port: Some(42000),
            metadata: meta,
        };
        let mut info = DerivedAddressInfo::default();
        info.available.insert(mk(
            false,
            true,
            "pub.example.com",
            HostnameMetadata::PublicDomain {
                gateway: gw.clone(),
            },
        ));
        info.available.insert(mk(
            false,
            true,
            "64.23.194.12",
            HostnameMetadata::Ipv4 {
                gateway: gw.clone(),
            },
        ));
        info.available.insert(mk(
            false,
            false,
            "priv.local",
            HostnameMetadata::PrivateDomain {
                gateways: BTreeSet::from([gw.clone()]),
            },
        ));
        info.available.insert(mk(
            false,
            false,
            "10.0.0.5",
            HostnameMetadata::Ipv4 {
                gateway: gw.clone(),
            },
        ));
        info.available.insert(mk(
            false,
            false,
            "2001:db8::1",
            HostnameMetadata::Ipv6 {
                gateway: gw.clone(),
                scope_id: 0,
            },
        ));

        let gua_v6: SocketAddrV6 = "[2001:db8::1]:42000".parse().unwrap();
        let gua_key = (InternedString::intern("2001:db8::1"), 42000u16);

        // WAN level on -> GUA public (public is inclusive of LAN, so it wins).
        set_nonssl_wan_group(&mut info, &gw, 42000, true);
        assert!(
            info.gua_wan.contains(&gua_v6),
            "public domain on => GUA public"
        );

        // WAN level off, LAN level still on -> GUA drops to local, not off.
        set_nonssl_wan_group(&mut info, &gw, 42000, false);
        assert!(!info.gua_wan.contains(&gua_v6), "GUA no longer public");
        assert!(
            !info.disabled.contains(&gua_key),
            "GUA is local (on), not off, because the LAN level is still up"
        );
    }

    #[test]
    fn nonssl_lan_group_moves_private_v4_gua_together() {
        let gw = GatewayId::from(InternedString::intern("wg1"));
        let mk = |host: &str, meta| HostnameInfo {
            ssl: false,
            public: false,
            hostname: InternedString::intern(host),
            port: Some(42000),
            metadata: meta,
        };
        let mut info = DerivedAddressInfo::default();
        info.available.insert(mk(
            "priv.local",
            HostnameMetadata::PrivateDomain {
                gateways: BTreeSet::from([gw.clone()]),
            },
        ));
        info.available.insert(mk(
            "10.0.0.5",
            HostnameMetadata::Ipv4 {
                gateway: gw.clone(),
            },
        ));
        info.available.insert(mk(
            "2001:db8::1",
            HostnameMetadata::Ipv6 {
                gateway: gw.clone(),
                scope_id: 0,
            },
        ));

        let priv_key = (InternedString::intern("priv.local"), 42000u16);
        let lan_key = (InternedString::intern("10.0.0.5"), 42000u16);
        let gua_key = (InternedString::intern("2001:db8::1"), 42000u16);

        // Disable the LAN level (nothing on the WAN side to keep the GUA) -> off.
        set_nonssl_lan_group(&mut info, &gw, 42000, false);
        assert!(info.disabled.contains(&priv_key));
        assert!(info.disabled.contains(&lan_key));
        assert!(info.disabled.contains(&gua_key), "GUA off");

        // Re-enable -> private domain + LAN IPv4 on, GUA local (on, not public).
        set_nonssl_lan_group(&mut info, &gw, 42000, true);
        assert!(!info.disabled.contains(&priv_key));
        assert!(!info.disabled.contains(&lan_key));
        assert!(!info.disabled.contains(&gua_key));
        assert!(
            !info
                .gua_wan
                .contains(&"[2001:db8::1]:42000".parse().unwrap()),
            "GUA is local, not public"
        );
    }

    #[test]
    fn lan_change_preserves_a_stored_wan_gua() {
        let gw = GatewayId::from(InternedString::intern("wg1"));
        let mk = |host: &str, meta| HostnameInfo {
            ssl: false,
            public: false,
            hostname: InternedString::intern(host),
            port: Some(42000),
            metadata: meta,
        };
        let mut info = DerivedAddressInfo::default();
        info.available.insert(mk(
            "priv.local",
            HostnameMetadata::PrivateDomain {
                gateways: BTreeSet::from([gw.clone()]),
            },
        ));
        info.available.insert(mk(
            "10.0.0.5",
            HostnameMetadata::Ipv4 {
                gateway: gw.clone(),
            },
        ));
        info.available.insert(mk(
            "2001:db8::1",
            HostnameMetadata::Ipv6 {
                gateway: gw.clone(),
                scope_id: 0,
            },
        ));
        let gua_v6: SocketAddrV6 = "[2001:db8::1]:42000".parse().unwrap();
        // Operator opted this GUA into WAN directly (stored preference) — there is
        // no public domain to link it.
        info.gua_wan.insert(gua_v6);
        info.enabled.insert(SocketAddr::V6(gua_v6));

        // A LAN-level change must NOT un-publish the GUA's stored WAN opt-in.
        set_nonssl_lan_group(&mut info, &gw, 42000, false);
        assert!(
            info.gua_wan.contains(&gua_v6),
            "a LAN change must not clobber the GUA's stored WAN opt-in"
        );
        set_nonssl_lan_group(&mut info, &gw, 42000, true);
        assert!(
            info.gua_wan.contains(&gua_v6),
            "still WAN after LAN re-enabled"
        );
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

    fn lan_fixture(ssl: bool) -> (DerivedAddressInfo, HostnameInfo, [HostnameInfo; 2]) {
        let eth = GatewayId::from(InternedString::intern("eth0"));
        let wifi = GatewayId::from(InternedString::intern("wlan0"));
        let mk = |host: &str, metadata| HostnameInfo {
            ssl,
            public: false,
            hostname: InternedString::intern(host),
            port: Some(8443),
            metadata,
        };
        let mdns = mk(
            "server.local",
            HostnameMetadata::Mdns {
                gateways: BTreeSet::from([eth.clone(), wifi.clone()]),
            },
        );
        let ips = [
            mk("192.0.2.10", HostnameMetadata::Ipv4 { gateway: eth }),
            mk("198.51.100.10", HostnameMetadata::Ipv4 { gateway: wifi }),
        ];
        let mut info = DerivedAddressInfo::default();
        info.available.insert(mdns.clone());
        info.available.extend(ips.iter().cloned());
        (info, mdns, ips)
    }

    #[test]
    fn a_lan_ip_is_disabled_on_its_own() {
        let (mut info, mdns, [eth, wifi]) = lan_fixture(true);

        set_address_enabled_on(&mut info, &eth, false).unwrap();

        assert!(info.enabled().contains(&mdns));
        assert!(!info.enabled().contains(&eth));
        assert!(info.enabled().contains(&wifi));
    }

    #[test]
    fn a_lan_ip_without_an_override_follows_its_mdns_address() {
        let (mut info, mdns, [eth, wifi]) = lan_fixture(true);

        set_address_enabled_on(&mut info, &mdns, false).unwrap();
        assert!(info.enabled().is_empty());

        set_address_enabled_on(&mut info, &eth, true).unwrap();
        set_address_enabled_on(&mut info, &mdns, false).unwrap();
        assert!(!info.enabled().contains(&mdns));
        assert!(info.enabled().contains(&eth));
        assert!(!info.enabled().contains(&wifi));

        let reassigned = HostnameInfo {
            hostname: InternedString::intern("198.51.100.11"),
            ..wifi.clone()
        };
        info.available.remove(&eth);
        info.available.remove(&wifi);
        info.available.insert(reassigned.clone());
        assert!(!info.enabled().contains(&reassigned));

        info.available.insert(eth.clone());
        assert!(info.enabled().contains(&eth));

        set_address_enabled_on(&mut info, &mdns, true).unwrap();
        assert!(info.enabled().contains(&mdns));
        assert!(info.enabled().contains(&reassigned));
    }

    #[test]
    fn an_enabled_lan_ip_serves_its_mdns_address_on_a_nonssl_port() {
        let (mut info, mdns, [eth, wifi]) = lan_fixture(false);

        set_address_enabled_on(&mut info, &mdns, false).unwrap();
        assert!(info.enabled().is_empty());

        set_address_enabled_on(&mut info, &eth, true).unwrap();
        assert!(info.enabled().contains(&mdns));
        assert!(info.enabled().contains(&eth));
        assert!(!info.enabled().contains(&wifi));

        set_address_enabled_on(&mut info, &mdns, false).unwrap();
        assert!(info.enabled().is_empty());

        set_address_enabled_on(&mut info, &mdns, true).unwrap();
        assert!(info.enabled().contains(&mdns));
        assert!(info.enabled().contains(&eth));
        assert!(info.enabled().contains(&wifi));
    }

    #[test]
    fn disabling_mdns_on_a_nonssl_port_takes_a_linked_lan_group_with_it() {
        let (mut info, mdns, [eth, wifi]) = lan_fixture(false);
        let domain = HostnameInfo {
            hostname: InternedString::intern("priv.lan"),
            metadata: HostnameMetadata::PrivateDomain {
                gateways: BTreeSet::from([GatewayId::from(InternedString::intern("eth0"))]),
            },
            ..eth.clone()
        };
        info.available.insert(domain.clone());

        set_address_enabled_on(&mut info, &mdns, false).unwrap();
        assert!(info.enabled().is_empty());

        set_address_enabled_on(&mut info, &mdns, true).unwrap();
        assert!(info.enabled().contains(&mdns));
        assert!(info.enabled().contains(&wifi));
        assert!(!info.enabled().contains(&eth));
        assert!(!info.enabled().contains(&domain));
    }

    #[test]
    fn an_enabled_public_gua_leaves_mdns_disabled() {
        let (mut info, mdns, _) = lan_fixture(false);
        let gua = HostnameInfo {
            public: true,
            hostname: InternedString::intern("2001:db8::10"),
            metadata: HostnameMetadata::Ipv6 {
                gateway: GatewayId::from(InternedString::intern("eth0")),
                scope_id: 0,
            },
            ..mdns.clone()
        };
        info.available.insert(gua.clone());
        info.enabled.insert(gua.to_socket_addr().unwrap());

        set_address_enabled_on(&mut info, &mdns, false).unwrap();

        assert!(!info.enabled().contains(&mdns));
        assert!(info.enabled().contains(&gua));
    }

    #[test]
    fn insecure_consent_enables_only_the_selected_private_address() {
        let (mut info, mdns, [eth, wifi]) = lan_fixture(false);
        info.insecure = info.available.iter().filter_map(insecure_key).collect();
        assert!(info.enabled().is_empty());
        assert!(set_address_enabled_with_consent(&mut info, &eth, true, false).is_err());
        assert!(info.enabled().is_empty());
        set_address_enabled_with_consent(&mut info, &eth, true, true).unwrap();
        assert!(info.enabled().contains(&eth));
        assert!(info.enabled().contains(&mdns));
        assert!(!info.enabled().contains(&wifi));
        let saved = serde_json::to_value(&info).unwrap();
        info = serde_json::from_value(saved).unwrap();
        assert!(info.enabled().contains(&eth));
        set_address_enabled_with_consent(&mut info, &mdns, false, false).unwrap();
        assert!(info.enabled().is_empty());
        assert!(info.insecure_enabled.is_empty());
        set_address_enabled_with_consent(&mut info, &mdns, true, true).unwrap();
        assert!(info.enabled().contains(&eth));
        assert!(info.enabled().contains(&wifi));
        assert!(info.enabled().contains(&mdns));
    }

    #[test]
    fn a_lan_ip_assigned_later_follows_its_consented_local_name() {
        let (mut info, mdns, [eth, wifi]) = lan_fixture(false);
        let on = |gateway: &str| HostnameInfo {
            metadata: HostnameMetadata::Mdns {
                gateways: BTreeSet::from([GatewayId::from(InternedString::intern(gateway))]),
            },
            ..mdns.clone()
        };
        let (name, trusted) = (on("eth0"), on("wlan0"));
        info.available = BTreeSet::from([name.clone(), eth.clone(), trusted.clone(), wifi.clone()]);
        info.insecure = [&name, &eth].into_iter().filter_map(insecure_key).collect();
        set_address_enabled_with_consent(&mut info, &name, true, true).unwrap();
        set_address_enabled_with_consent(&mut info, &trusted, false, false).unwrap();
        let renumbered = HostnameInfo {
            hostname: InternedString::intern("192.0.2.20"),
            ..eth.clone()
        };
        info.available.remove(&eth);
        info.insecure.remove(&insecure_key(&eth).unwrap());
        info.available.insert(renumbered.clone());
        info.insecure.extend(insecure_key(&renumbered));
        assert!(info.enabled().contains(&name));
        assert!(info.enabled().contains(&renumbered));
        assert!(!info.enabled().contains(&trusted));
        assert!(!info.enabled().contains(&wifi));

        let mut view = info.clone();
        view.project_service_selections();
        assert!(view.disabled.contains(&override_key(&wifi)));
        assert!(view.lan_enabled.contains(&override_key(&renumbered)));

        set_address_enabled_with_consent(&mut info, &renumbered, false, false).unwrap();
        assert!(!info.enabled().contains(&renumbered));
        assert!(info.enabled().contains(&name));
    }

    #[test]
    fn service_view_disables_unconsented_default_off_names_for_legacy_readers() {
        let (mut info, mdns, [eth, _]) = lan_fixture(false);
        let domain = HostnameInfo {
            hostname: InternedString::intern("media.lan"),
            metadata: HostnameMetadata::PrivateDomain {
                gateways: BTreeSet::from([GatewayId::from(InternedString::intern("eth0"))]),
            },
            ..eth.clone()
        };
        info.available.insert(domain.clone());
        info.default_off = true;
        assert!(info.enabled().is_empty());
        let mut view = info.clone();
        view.project_service_selections();
        assert!(view.disabled.contains(&override_key(&mdns)));
        assert!(view.disabled.contains(&override_key(&domain)));

        info.set_name_enabled(override_key(&domain), true);
        assert!(info.enabled().contains(&domain));
        assert!(!info.enabled().contains(&mdns));
        let mut view = info.clone();
        view.project_service_selections();
        assert!(!view.disabled.contains(&override_key(&domain)));
        assert!(view.disabled.contains(&override_key(&mdns)));
    }

    #[test]
    fn insecure_private_domain_consent_covers_its_inseparable_lan_group() {
        let (mut info, _, [eth, wifi]) = lan_fixture(false);
        let gua = HostnameInfo {
            hostname: InternedString::intern("2001:db8::10"),
            metadata: HostnameMetadata::Ipv6 {
                gateway: GatewayId::from(InternedString::intern("eth0")),
                scope_id: 0,
            },
            ..eth.clone()
        };
        let domain = HostnameInfo {
            hostname: InternedString::intern("media.lan"),
            metadata: HostnameMetadata::PrivateDomain {
                gateways: BTreeSet::from([GatewayId::from(InternedString::intern("eth0"))]),
            },
            ..eth.clone()
        };
        info.available.extend([gua.clone(), domain.clone()]);
        info.insecure = info.available.iter().filter_map(insecure_key).collect();
        info.disabled.insert(override_key(&domain));
        assert!(set_address_enabled_with_consent(&mut info, &domain, true, false).is_err());
        set_address_enabled_with_consent(&mut info, &domain, true, true).unwrap();
        for member in [&eth, &gua, &domain] {
            assert!(info.enabled().contains(member));
        }
        assert!(!info.enabled().contains(&wifi));
        assert!(info.gua_wan.is_empty());
        let untrusted = std::mem::take(&mut info.insecure);
        assert!(info.enabled().contains(&domain));
        assert!(info.enabled().contains(&eth));
        info.insecure = untrusted;
        set_address_enabled_with_consent(&mut info, &gua, false, false).unwrap();
        assert!(info.enabled().is_empty());
    }

    #[test]
    fn disabling_a_trusted_name_revokes_prior_consent_on_all_its_gateways() {
        let (mut info, mdns, _) = lan_fixture(false);
        info.insecure_enabled = mdns
            .metadata
            .gateways()
            .map(|g| (mdns.hostname.clone(), mdns.port.unwrap(), g.clone()))
            .collect();
        assert_eq!(info.insecure_enabled.len(), 2);
        set_address_enabled_with_consent(&mut info, &mdns, false, false).unwrap();
        assert!(info.insecure_enabled.is_empty());
        info.insecure = info.available.iter().filter_map(insecure_key).collect();
        assert!(info.enabled().is_empty());
    }

    #[test]
    fn plugin_address_changes_refresh_effective_selections() {
        let mut model = Model::new(&DerivedAddressInfo::default()).unwrap();
        let row = HostnameInfo {
            ssl: false,
            public: false,
            hostname: InternedString::intern("service.onion"),
            port: Some(80),
            metadata: HostnameMetadata::Plugin {
                package_id: "tor".parse().unwrap(),
                remove_action: None,
                overflow_actions: vec![],
                info: Value::Null,
            },
        };
        model
            .mutate_available(|available| {
                available.insert(row.clone());
                Ok(())
            })
            .unwrap();
        assert_eq!(model.de().unwrap().effective, BTreeSet::from([row]));
        model
            .mutate_available(|available| {
                available.clear();
                Ok(())
            })
            .unwrap();
        assert!(model.de().unwrap().effective.is_empty());
    }

    #[test]
    fn os_plaintext_cannot_become_public_but_self_secured_protocols_can() {
        assert!(!opts(8080, Some(8443), None).allows_public_address(false));
        assert!(opts(8080, Some(8443), None).allows_public_address(true));
        assert!(opts(8333, None, Some(false)).allows_public_address(false));
        assert!(opts(8443, None, Some(true)).allows_public_address(true));
    }

    #[test]
    fn stale_or_forged_address_metadata_cannot_bypass_consent() {
        let (mut info, _, [eth, _]) = lan_fixture(false);
        info.insecure = info.available.iter().filter_map(insecure_key).collect();
        let forged = HostnameInfo {
            public: true,
            ..eth
        };
        assert!(set_address_enabled_with_consent(&mut info, &forged, true, true).is_err());
        assert!(info.enabled().is_empty());
        let request: BindingSetAddressEnabledParams = serde_json::from_value(serde_json::json!({
            "internalPort": 5223, "address": forged, "enabled": true,
        }))
        .unwrap();
        assert!(!request.allow_insecure);
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
        info.lan_enabled
            .insert((InternedString::intern("192.0.2.10"), 49152));

        let consent = (
            InternedString::intern("media.lan"),
            49152,
            "eth0".parse().unwrap(),
        );
        info.insecure.insert(consent.clone());
        info.insecure_enabled.insert(consent);
        info.rekey_port(49152, 5000);
        let moved = (
            InternedString::intern("media.lan"),
            5000,
            "eth0".parse().unwrap(),
        );
        assert!(info.insecure.contains(&moved));
        assert!(info.insecure_enabled.contains(&moved));
        info.forget_gateway(&"eth0".parse().unwrap());
        assert!(info.insecure.is_empty());
        assert!(info.insecure_enabled.is_empty());

        assert_eq!(
            info.lan_enabled,
            BTreeSet::from([(InternedString::intern("192.0.2.10"), 5000)])
        );

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

    #[test]
    fn forget_gateway_drops_only_that_gateways_overrides() {
        let gw = |id: &str| GatewayId::from(InternedString::intern(id));
        let ip = |host: &str, metadata: HostnameMetadata| HostnameInfo {
            ssl: false,
            public: true,
            hostname: InternedString::intern(host),
            port: Some(3478),
            metadata,
        };
        let mut info = DerivedAddressInfo::default();
        info.available.extend([
            ip(
                "64.23.194.12",
                HostnameMetadata::Ipv4 { gateway: gw("wg0") },
            ),
            ip(
                "2604:a880:4:1d0:0:3:145f:5",
                HostnameMetadata::Ipv6 {
                    gateway: gw("wg0"),
                    scope_id: 4,
                },
            ),
            ip("10.59.88.5", HostnameMetadata::Ipv4 { gateway: gw("wg0") }),
            ip(
                "73.34.87.9",
                HostnameMetadata::Ipv4 {
                    gateway: gw("eno0"),
                },
            ),
            ip(
                "demo.local",
                HostnameMetadata::Mdns {
                    gateways: BTreeSet::from([gw("wg0"), gw("eno0")]),
                },
            ),
        ]);
        info.enabled.extend([
            "64.23.194.12:3478".parse::<SocketAddr>().unwrap(),
            "[2604:a880:4:1d0:0:3:145f:5]:3478".parse().unwrap(),
            "73.34.87.9:3478".parse().unwrap(),
        ]);
        info.gua_wan
            .insert("[2604:a880:4:1d0:0:3:145f:5]:3478".parse().unwrap());
        info.disabled
            .insert((InternedString::intern("demo.local"), 3478));
        info.lan_enabled
            .insert((InternedString::intern("10.59.88.5"), 3478));

        info.forget_gateway(&gw("wg0"));

        assert_eq!(
            info.enabled,
            BTreeSet::from(["73.34.87.9:3478".parse().unwrap()])
        );
        assert!(info.gua_wan.is_empty());
        assert!(info.lan_enabled.is_empty());
        assert_eq!(
            info.disabled,
            BTreeSet::from([(InternedString::intern("demo.local"), 3478)])
        );
    }

    #[test]
    fn forget_gateway_preserves_overrides_shared_with_another_gateway() {
        let mut info = DerivedAddressInfo::default();
        let enabled = BTreeSet::from(["64.23.194.12:3478".parse().unwrap()]);
        let gua_wan = BTreeSet::from(["[2001:db8::1]:3478".parse().unwrap()]);
        let disabled = BTreeSet::from([(InternedString::intern("10.0.0.1"), 3478)]);
        let lan_enabled = BTreeSet::from([(InternedString::intern("10.0.0.2"), 3478)]);
        info.enabled = enabled.clone();
        info.gua_wan = gua_wan.clone();
        info.disabled = disabled.clone();
        info.lan_enabled = lan_enabled.clone();
        for gateway in ["wg0", "eno0"] {
            let gateway = GatewayId::from(InternedString::intern(gateway));
            for (hostname, public) in [
                ("64.23.194.12", true),
                ("10.0.0.1", false),
                ("10.0.0.2", false),
                ("2001:db8::1", true),
            ] {
                info.available.insert(HostnameInfo {
                    ssl: false,
                    public,
                    hostname: InternedString::intern(hostname),
                    port: Some(3478),
                    metadata: if hostname.contains(':') {
                        HostnameMetadata::Ipv6 {
                            gateway: gateway.clone(),
                            scope_id: 4,
                        }
                    } else {
                        HostnameMetadata::Ipv4 {
                            gateway: gateway.clone(),
                        }
                    },
                });
            }
        }

        info.forget_gateway(&GatewayId::from(InternedString::intern("wg0")));

        assert_eq!(info.enabled, enabled);
        assert_eq!(info.gua_wan, gua_wan);
        assert_eq!(info.disabled, disabled);
        assert_eq!(info.lan_enabled, lan_enabled);
    }

    fn range_info(external_start_port: u16, number_of_ports: u16, enabled: bool) -> RangeBindInfo {
        RangeBindInfo {
            enabled,
            transport: Default::default(),
            external_start_port,
            number_of_ports,
            addresses: Default::default(),
            interface: None,
        }
    }

    #[test]
    fn internal_span_covers_every_port_and_rejects_one_past_the_end() {
        assert_eq!(internal_span(42000, 500).unwrap(), 42000..=42499);
        assert_eq!(internal_span(65534, 2).unwrap(), 65534..=65535);
        assert_eq!(internal_span(8080, 1).unwrap(), 8080..=8080);
        assert!(internal_span(65535, 2).is_err());
        assert!(internal_span(0, 0).is_err());
    }

    #[test]
    fn only_enabled_bindings_occupy_their_port() {
        let mut ports = AvailablePorts::new();
        let live = BindInfo::new(&mut ports, opts(28332, None, Some(false)), false).unwrap();
        let mut dormant = BindInfo::new(&mut ports, opts(28333, None, Some(false)), false).unwrap();
        dormant.disable();
        let bindings = Bindings([(28332, live), (28333, dormant)].into_iter().collect());

        assert_eq!(
            bindings.enabled_overlap(&(28330..=28334)),
            Some(28332..=28332)
        );
        assert_eq!(bindings.enabled_overlap(&(28333..=28340)), None);
        assert_eq!(bindings.enabled_overlap(&(28333..=28333)), None);
    }

    #[test]
    fn a_range_does_not_conflict_with_the_entry_it_reaffirms() {
        let ranges = BindingRanges(
            [
                (42000, range_info(42000, 500, true)),
                (50000, range_info(50000, 10, false)),
            ]
            .into_iter()
            .collect(),
        );

        // Re-affirming 42000 skips itself but a second range still collides.
        assert_eq!(
            ranges
                .enabled_overlap(&(42000..=42499), Some(42000))
                .unwrap(),
            None
        );
        assert_eq!(
            ranges.enabled_overlap(&(42499..=42600), None).unwrap(),
            Some(42000..=42499)
        );
        assert_eq!(
            ranges.enabled_overlap(&(42500..=42600), None).unwrap(),
            None
        );
        // A dormant range never conflicts.
        assert_eq!(
            ranges.enabled_overlap(&(50005..=50020), None).unwrap(),
            None
        );
    }
}
