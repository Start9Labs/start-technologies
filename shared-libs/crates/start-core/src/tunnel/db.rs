use std::collections::{BTreeMap, BTreeSet};
use std::net::{Ipv4Addr, SocketAddrV4, SocketAddrV6};
use std::path::PathBuf;
use std::time::Duration;

use axum::extract::ws;
use clap::Parser;
use imbl::OrdMap;
use imbl_value::InternedString;
use ipnet::Ipv4Net;
use itertools::Itertools;
use patch_db::Dump;
use patch_db::json_ptr::{JsonPointer, ROOT};
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use ts_rs::TS;

use crate::GatewayId;
use crate::auth::AuthKeys;
use crate::context::CliContext;
use crate::db::model::public::NetworkInterfaceInfo;
use crate::prelude::*;
use crate::rpc_continuations::{Guid, RpcContinuation};
use crate::tunnel::context::TunnelContext;
use crate::tunnel::migrations;
use crate::tunnel::web::WebserverInfo;
use crate::tunnel::wg::{WgServer, WgSubnetConfig};
use crate::util::serde::{HandlerExtSerde, apply_expr};

#[derive(Default, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct TunnelDatabase {
    #[serde(default)]
    #[ts(skip)]
    pub migrations: BTreeSet<InternedString>,
    pub webserver: WebserverInfo,
    pub password: Option<String>,
    /// Same key as the StartOS private db, so a 1.1.x db upgrades by serde
    /// default (empty — everyone signs in again) with no migration.
    #[serde(default)]
    pub session_pubkeys: AuthKeys,
    #[ts(as = "std::collections::BTreeMap::<GatewayId, NetworkInterfaceInfo>")]
    pub gateways: OrdMap<GatewayId, NetworkInterfaceInfo>,
    pub wg: WgServer,
    pub port_forwards: PortForwards,
    /// IPv6 GUA firewall pinholes: inbound to a client's own global address is
    /// accepted (no NAT — the GUA is directly routable), keyed by the exposed
    /// `[GUA]:port`. The v4 analogue is a `PortForward::Dnat`.
    #[serde(default)]
    pub pinholes6: Pinholes6,
    #[serde(default)]
    pub dns_records: DnsRecords,
    #[serde(default)]
    pub http_redirects: HttpRedirects,
}

impl TunnelDatabase {
    pub fn init() -> Self {
        let mut db = Self {
            migrations: migrations::MIGRATIONS
                .iter()
                .map(|m| m.name().into())
                .collect(),
            ..Default::default()
        };
        db.wg.subnets.0.insert(
            Ipv4Net::new_assert([10, 59, rand::random(), 1].into(), 24),
            WgSubnetConfig {
                name: "Default Subnet".into(),
                ..Default::default()
            },
        );
        db
    }
}

impl Model<TunnelDatabase> {
    /// Prune forwards whose target is no longer a known client. Returns the
    /// surviving sources, the dropped SNI routes, and the dropped SNI fallbacks,
    /// which the caller must unregister from the in-memory demux dataplane.
    pub fn gc_forwards(&mut self) -> Result<GcForwards, Error> {
        let mut keep_sources = BTreeSet::new();
        let mut dropped_sni: Vec<(SocketAddrV4, String, SocketAddrV4)> = Vec::new();
        let mut dropped_fallbacks: Vec<(SocketAddrV4, SocketAddrV4)> = Vec::new();
        let mut keep_targets = BTreeSet::new();
        for (_, cfg) in self.as_wg().as_subnets().as_entries()? {
            keep_targets.extend(cfg.as_clients().keys()?);
        }
        self.as_port_forwards_mut().mutate(|pf| {
            Ok(pf.0.retain(|k, v| {
                let keep = match v {
                    PortForward::Dnat { target, .. } => keep_targets.contains(target.ip()),
                    PortForward::Sni { routes, fallback } => {
                        for (h, r) in routes.iter() {
                            if !keep_targets.contains(r.target.ip()) {
                                dropped_sni.push((*k, h.clone(), r.target));
                            }
                        }
                        routes.retain(|_, r| keep_targets.contains(r.target.ip()));
                        if let Some(f) = fallback {
                            if !keep_targets.contains(f.target.ip()) {
                                dropped_fallbacks.push((*k, f.target));
                                *fallback = None;
                            }
                        }
                        !routes.is_empty() || fallback.is_some()
                    }
                };
                if keep {
                    keep_sources.insert(*k);
                }
                keep
            }))
        })?;
        Ok(GcForwards {
            keep_sources,
            dropped_sni,
            dropped_fallbacks,
        })
    }
}

/// The result of [`Model::<TunnelDatabase>::gc_forwards`]: surviving sources plus
/// the dataplane demux entries (SNI routes and hostname-less fallbacks) whose
/// target is no longer a known client and so must be unregistered.
pub struct GcForwards {
    pub keep_sources: BTreeSet<SocketAddrV4>,
    pub dropped_sni: Vec<(SocketAddrV4, String, SocketAddrV4)>,
    pub dropped_fallbacks: Vec<(SocketAddrV4, SocketAddrV4)>,
}

#[test]
fn export_bindings_tunnel_db() {
    use crate::tunnel::api::*;
    use crate::tunnel::auth::{AddKeyParams, RemoveKeyParams, SetPasswordParams};

    TunnelDatabase::export_all_to("bindings/tunnel").unwrap();
    SubnetParams::export_all_to("bindings/tunnel").unwrap();
    AddSubnetParams::export_all_to("bindings/tunnel").unwrap();
    SetSubnetDnsParams::export_all_to("bindings/tunnel").unwrap();
    AddDeviceParams::export_all_to("bindings/tunnel").unwrap();
    RemoveDeviceParams::export_all_to("bindings/tunnel").unwrap();
    ListDevicesParams::export_all_to("bindings/tunnel").unwrap();
    ShowConfigParams::export_all_to("bindings/tunnel").unwrap();
    AddPortForwardParams::export_all_to("bindings/tunnel").unwrap();
    RemovePortForwardParams::export_all_to("bindings/tunnel").unwrap();
    UpdatePortForwardLabelParams::export_all_to("bindings/tunnel").unwrap();
    SetPortForwardEnabledParams::export_all_to("bindings/tunnel").unwrap();
    AddPinholeParams::export_all_to("bindings/tunnel").unwrap();
    RemovePinholeParams::export_all_to("bindings/tunnel").unwrap();
    UpdatePinholeLabelParams::export_all_to("bindings/tunnel").unwrap();
    SetPinholeEnabledParams::export_all_to("bindings/tunnel").unwrap();
    SetDnsInjectionParams::export_all_to("bindings/tunnel").unwrap();
    SetAutoPortForwardParams::export_all_to("bindings/tunnel").unwrap();
    SetSubnetWanParams::export_all_to("bindings/tunnel").unwrap();
    SetSubnetIpv6Params::export_all_to("bindings/tunnel").unwrap();
    SetDeviceWanParams::export_all_to("bindings/tunnel").unwrap();
    SetDeviceKindParams::export_all_to("bindings/tunnel").unwrap();
    AddDnsRecordParams::export_all_to("bindings/tunnel").unwrap();
    RemoveDnsRecordParams::export_all_to("bindings/tunnel").unwrap();
    DnsRecordEntry::export_all_to("bindings/tunnel").unwrap();
    HttpRedirects::export_all_to("bindings/tunnel").unwrap();
    SetHttpRedirectEnabledParams::export_all_to("bindings/tunnel").unwrap();
    HttpRedirectStatus::export_all_to("bindings/tunnel").unwrap();
    AddKeyParams::export_all_to("bindings/tunnel").unwrap();
    RemoveKeyParams::export_all_to("bindings/tunnel").unwrap();
    SetPasswordParams::export_all_to("bindings/tunnel").unwrap();
}

/// One external-port forward: an nftables DNAT or an SNI-demultiplexed shared
/// port. Mutually exclusive for a given external address.
#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PortForward {
    Dnat {
        target: SocketAddrV4,
        label: Option<String>,
        #[serde(default = "default_true")]
        enabled: bool,
        /// Contiguous ports forwarded (a PCP PORT_SET range); `1` for single-port.
        #[serde(default = "default_one")]
        count: u16,
        /// Gateway-created (PCP/UPnP) vs user-added. Drives the UI Manual/Automatic split.
        #[serde(default)]
        auto: bool,
    },
    Sni {
        /// hostname (lowercase; may be `*.suffix`) -> route.
        routes: BTreeMap<String, SniRoute>,
        /// Hostname-less catch-all for this shared external port. Traffic whose
        /// SNI matches no `routes` entry — or that carries no SNI (bare-IP TLS,
        /// non-TLS) — is spliced here instead of being dropped. `None` closes the
        /// port to unmatched traffic. This lets a bare public IP and named
        /// domains share one external port, the bare IP acting as the fallback.
        #[serde(default)]
        fallback: Option<SniRoute>,
    },
}

/// One SNI-demultiplexed hostname route on a shared external port.
#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SniRoute {
    pub target: SocketAddrV4,
    pub label: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Gateway-created (PCP) vs user-added. Drives the UI Manual/Automatic split.
    #[serde(default)]
    pub auto: bool,
}

fn default_true() -> bool {
    true
}

fn default_one() -> u16 {
    1
}

/// A DNS record served by the tunnel (injected via RFC 2136 or added manually).
/// `value` is the rdata as text: an IP for A/AAAA, a name for CNAME, etc.
#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DnsRecordEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub rtype: String,
    pub value: String,
    pub ttl: u32,
    /// The device IP that injected this, or `null` for a manual record.
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
pub struct DnsRecords(pub Vec<DnsRecordEntry>);

impl PortForward {
    /// Number of contiguous external ports this forward occupies: a DNAT spans
    /// its `count`; an SNI-demuxed forward holds the single shared port.
    pub fn port_span(&self) -> u16 {
        match self {
            PortForward::Dnat { count, .. } => (*count).max(1),
            PortForward::Sni { .. } => 1,
        }
    }
}

/// Per-IPv4 HTTP→HTTPS redirect state. The tunnel runs a redirect on port 80 of
/// every public IPv4 by default; this records the addresses where the user has
/// turned it off (absence = on). The redirect also yields to any port-forward
/// occupying port 80 on that IP, so the two never fight over the port.
#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HttpRedirects {
    #[serde(default)]
    #[ts(type = "string[]")]
    pub disabled: BTreeSet<Ipv4Addr>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
pub struct PortForwards(pub BTreeMap<SocketAddrV4, PortForward>);
impl Map for PortForwards {
    type Key = SocketAddrV4;
    type Value = PortForward;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Self::key_string(key)
    }
    fn key_string(key: &Self::Key) -> Result<InternedString, Error> {
        Ok(InternedString::from_display(key))
    }
}

/// One IPv6 GUA firewall entry, keyed by the exposed `[GUA]:external_port`. The
/// destination is always the same GUA. When `internal_port` is `None` (or equals
/// the key's port) it's a pure firewall pinhole — `ct state new accept`, no NAT.
/// When it differs it's a port-only DNAT to `[GUA]:internal_port` (e.g. 80→443),
/// the v6 analogue of a `PortForward::Dnat`.
#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Pinhole {
    pub label: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Contiguous ports opened, counting up from the key's port; `1` for single.
    #[serde(default = "default_one")]
    pub count: u16,
    /// Destination port on the GUA; `None` means "same as the external (key) port"
    /// — a pure pinhole. A different value makes this a port-DNAT.
    #[serde(default)]
    pub internal_port: Option<u16>,
    /// Gateway-created (PCP) vs user-added. Drives the UI Manual/Automatic split.
    #[serde(default)]
    pub auto: bool,
}

impl Pinhole {
    /// The destination port on the GUA: `internal_port` if remapped, else the
    /// external (key) port. `external` is `key.port()`.
    pub fn internal_port(&self, external: u16) -> u16 {
        self.internal_port.unwrap_or(external)
    }
    /// Whether this entry does port translation (DNAT) rather than a pure pinhole.
    pub fn is_dnat(&self, external: u16) -> bool {
        self.internal_port(external) != external
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
pub struct Pinholes6(pub BTreeMap<SocketAddrV6, Pinhole>);
impl Map for Pinholes6 {
    type Key = SocketAddrV6;
    type Value = Pinhole;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Self::key_string(key)
    }
    fn key_string(key: &Self::Key) -> Result<InternedString, Error> {
        Ok(InternedString::from_display(key))
    }
}
impl Pinholes6 {
    /// The key of an existing pinhole on the same GUA whose port span overlaps
    /// `[gua.port(), gua.port() + count - 1]`, if any. An exact key match is
    /// excluded (idempotent re-assert / same-port collision, handled by callers).
    pub fn overlapping(&self, gua: SocketAddrV6, count: u16) -> Option<SocketAddrV6> {
        let ip = *gua.ip();
        let new_lo = gua.port();
        let new_hi = new_lo.saturating_add(count.saturating_sub(1));
        // Pinholes on one GUA never overlap, so only the nearest entry starting
        // at or before new_hi (skipping an exact re-assert of `gua`) can reach
        // into [new_lo, new_hi].
        self.0
            .range(..=SocketAddrV6::new(ip, new_hi, 0, 0))
            .rev()
            .take_while(|(key, _)| key.ip() == &ip)
            .find(|(key, _)| **key != gua)
            .and_then(|(key, ph)| {
                let hi = key.port().saturating_add(ph.count.max(1).saturating_sub(1));
                (hi >= new_lo).then_some(*key)
            })
    }
}
impl PortForwards {
    /// The source of an existing forward on the same external IP whose port span
    /// overlaps `[source.port(), source.port() + count - 1]`, if any. An exact
    /// `source` match is excluded — callers treat that as an idempotent
    /// re-assert (auto) or a same-port collision (manual); this catches the case
    /// ranges introduce, where two *different* start ports cover shared ports.
    pub fn overlapping(&self, source: SocketAddrV4, count: u16) -> Option<SocketAddrV4> {
        let ip = *source.ip();
        let new_lo = source.port();
        let new_hi = new_lo.saturating_add(count.saturating_sub(1));
        // Forwards on one external IP never overlap, so only the nearest entry
        // starting at or before new_hi (skipping an exact re-assert of `source`)
        // can reach into [new_lo, new_hi].
        self.0
            .range(..=SocketAddrV4::new(ip, new_hi))
            .rev()
            .take_while(|(src, _)| src.ip() == &ip)
            .find(|(src, _)| **src != source)
            .and_then(|(src, pf)| {
                let hi = src.port().saturating_add(pf.port_span().saturating_sub(1));
                (hi >= new_lo).then_some(*src)
            })
    }

    /// Whether any forward on `addr`'s IP has a port span covering `addr.port()`.
    /// Used to keep the port-80 HTTP redirect mutually exclusive with forwards:
    /// the redirect yields when a forward already occupies the port.
    pub fn occupied(&self, addr: SocketAddrV4) -> bool {
        // Only the nearest forward starting at or before `addr` can cover it
        // (forwards on one IP never overlap).
        self.0.range(..=addr).next_back().is_some_and(|(src, pf)| {
            src.ip() == addr.ip()
                && src.port().saturating_add(pf.port_span().saturating_sub(1)) >= addr.port()
        })
    }
}

pub fn db_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "dump",
            from_fn_async(cli_dump)
                .with_display_serializable()
                .with_about("about.filter-query-db-display-tables-records"),
        )
        .subcommand(
            "dump",
            from_fn_async(dump)
                .with_metadata("admin", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "subscribe",
            from_fn_async(subscribe)
                .with_metadata("get_signer", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "apply",
            from_fn_async(cli_apply)
                .no_display()
                .with_about("about.update-db-record"),
        )
        .subcommand(
            "apply",
            from_fn_async(apply)
                .with_metadata("admin", Value::Bool(true))
                .no_cli(),
        )
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CliDumpParams {
    #[arg(long = "pointer", short = 'p', help = "help.arg.json-pointer")]
    pointer: Option<JsonPointer>,
    #[arg(help = "help.arg.database-path")]
    path: Option<PathBuf>,
}

#[instrument(skip_all)]
async fn cli_dump(
    HandlerArgs {
        context,
        parent_method,
        method,
        params: CliDumpParams { pointer, path },
        ..
    }: HandlerArgs<CliContext, CliDumpParams>,
) -> Result<Dump, RpcError> {
    let dump = if let Some(path) = path {
        PatchDb::open(path).await?.dump(&ROOT).await
    } else {
        let method = parent_method.into_iter().chain(method).join(".");
        from_value::<Dump>(
            context
                .call_remote::<TunnelContext>(&method, imbl_value::json!({ "pointer": pointer }))
                .await?,
        )?
    };

    Ok(dump)
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct DumpParams {
    #[arg(long = "pointer", short = 'p', help = "help.arg.json-pointer")]
    #[ts(type = "string | null")]
    pointer: Option<JsonPointer>,
}

pub async fn dump(ctx: TunnelContext, DumpParams { pointer }: DumpParams) -> Result<Dump, Error> {
    Ok(ctx
        .db
        .dump(&pointer.as_ref().map_or(ROOT, |p| p.borrowed()))
        .await)
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CliApplyParams {
    #[arg(help = "help.arg.db-apply-expr")]
    expr: String,
    #[arg(help = "help.arg.database-path")]
    path: Option<PathBuf>,
}

#[instrument(skip_all)]
async fn cli_apply(
    HandlerArgs {
        context,
        parent_method,
        method,
        params: CliApplyParams { expr, path },
        ..
    }: HandlerArgs<CliContext, CliApplyParams>,
) -> Result<(), RpcError> {
    if let Some(path) = path {
        PatchDb::open(path)
            .await?
            .apply_function(|db| {
                let res = apply_expr(
                    serde_json::to_value(patch_db::Value::from(db))
                        .with_kind(ErrorKind::Deserialization)?
                        .into(),
                    &expr,
                )?;

                Ok::<_, Error>((
                    to_value(
                        &serde_json::from_value::<TunnelDatabase>(res.clone().into()).with_ctx(
                            |_| {
                                (
                                    crate::ErrorKind::Deserialization,
                                    "result does not match database model",
                                )
                            },
                        )?,
                    )?,
                    (),
                ))
            })
            .await
            .result?;
    } else {
        let method = parent_method.into_iter().chain(method).join(".");
        context
            .call_remote::<TunnelContext>(&method, imbl_value::json!({ "expr": expr }))
            .await?;
    }

    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ApplyParams {
    #[arg(help = "help.arg.db-apply-expr")]
    expr: String,
    #[arg(help = "help.arg.database-path")]
    path: Option<PathBuf>,
}

pub async fn apply(ctx: TunnelContext, ApplyParams { expr, .. }: ApplyParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            let res = apply_expr(
                serde_json::to_value(patch_db::Value::from(db.clone()))
                    .with_kind(ErrorKind::Deserialization)?
                    .into(),
                &expr,
            )?;

            db.ser(
                &serde_json::from_value::<TunnelDatabase>(res.clone().into()).with_ctx(|_| {
                    (
                        crate::ErrorKind::Deserialization,
                        "result does not match database model",
                    )
                })?,
            )
        })
        .await
        .result
}

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeParams {
    #[ts(type = "string | null")]
    pointer: Option<JsonPointer>,
    #[ts(skip)]
    #[serde(rename = "__Auth_signer")]
    signer: Option<InternedString>,
}

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeRes {
    #[ts(type = "{ id: number; value: unknown }")]
    pub dump: Dump,
    pub guid: Guid,
}

pub async fn subscribe(
    ctx: TunnelContext,
    SubscribeParams { pointer, signer }: SubscribeParams,
) -> Result<SubscribeRes, Error> {
    let (dump, mut sub) = ctx
        .db
        .dump_and_sub(pointer.unwrap_or_else(|| ROOT.to_owned()))
        .await;
    let guid = Guid::new();
    ctx.rpc_continuations
        .add(
            guid.clone(),
            RpcContinuation::ws_authed(
                &ctx,
                signer,
                |mut ws| async move {
                    if let Err(e) = async {
                        loop {
                            tokio::select! {
                                rev = sub.recv() => {
                                    if let Some(rev) = rev {
                                        ws.send(ws::Message::Text(
                                            serde_json::to_string(&rev)
                                                .with_kind(ErrorKind::Serialization)?
                                                .into(),
                                        ))
                                        .await
                                        .with_kind(ErrorKind::Network)?;
                                    } else {
                                        return ws.normal_close("complete").await;
                                    }
                                }
                                msg = ws.recv() => {
                                    if msg.transpose().with_kind(ErrorKind::Network)?.is_none() {
                                        return Ok(())
                                    }
                                }
                            }
                        }
                    }
                    .await
                    {
                        if !crate::util::net::is_ws_reset_without_close(&e) {
                            tracing::error!("Error in db websocket: {e}");
                            tracing::debug!("{e:?}");
                        }
                    }
                },
                Duration::from_secs(30),
            ),
        )
        .await;

    Ok(SubscribeRes { dump, guid })
}

#[test]
fn sni_and_dnat_persistence_round_trip() {
    use crate::tunnel::migrations::{PortForwardKind, TunnelMigration};

    let route = SniRoute {
        target: "10.59.0.2:443".parse().unwrap(),
        label: None,
        enabled: true,
        auto: true,
    };
    let mut routes = BTreeMap::new();
    routes.insert("id.example.com".to_string(), route);
    let sni = PortForward::Sni {
        routes,
        fallback: None,
    };

    let sni_json = serde_json::to_value(&sni).unwrap();
    eprintln!("SNI serialized: {sni_json}");
    assert_eq!(sni_json["kind"], serde_json::json!("sni"));
    let sni_back: PortForward = serde_json::from_value(sni_json).unwrap();
    match &sni_back {
        PortForward::Sni { routes, .. } => {
            let r = routes.get("id.example.com").expect("route present");
            assert_eq!(r.target, "10.59.0.2:443".parse().unwrap());
            assert_eq!(r.label, None);
            assert!(r.enabled);
        }
        other => panic!("expected Sni, got {other:?}"),
    }

    let dnat = PortForward::Dnat {
        target: "10.59.0.2:443".parse().unwrap(),
        label: None,
        enabled: true,
        count: 1,
        auto: false,
    };
    let dnat_json = serde_json::to_value(&dnat).unwrap();
    eprintln!("DNAT serialized: {dnat_json}");
    assert_eq!(dnat_json["kind"], serde_json::json!("dnat"));
    let dnat_back: PortForward = serde_json::from_value(dnat_json).unwrap();
    assert!(matches!(dnat_back, PortForward::Dnat { count: 1, .. }));

    // Legacy entry with no `kind` field, run through the m_01 migration.
    let mut legacy: imbl_value::Value = imbl_value::json!({
        "portForwards": {
            "1.2.3.4:443": {
                "target": "10.59.0.2:443",
                "label": null,
                "enabled": true,
                "count": 1
            }
        }
    });
    PortForwardKind.action(&mut legacy).unwrap();
    eprintln!("Migrated legacy: {legacy}");
    let migrated_entry = legacy["portForwards"]["1.2.3.4:443"].clone();
    let migrated: PortForward =
        serde_json::from_value(serde_json::to_value(&migrated_entry).unwrap()).unwrap();
    assert!(
        matches!(migrated, PortForward::Dnat { count: 1, .. }),
        "migrated legacy entry should be Dnat, got {migrated:?}"
    );

    // Whole PortForwards map mixing a migrated dnat and a new sni entry.
    let mixed = serde_json::json!({
        "1.2.3.4:443": {
            "kind": "dnat",
            "target": "10.59.0.2:443",
            "label": null,
            "enabled": true,
            "count": 1
        },
        "5.6.7.8:443": {
            "kind": "sni",
            "routes": {
                "id.example.com": {
                    "target": "10.59.0.2:443",
                    "label": null,
                    "enabled": true
                }
            }
        }
    });
    let map: PortForwards = serde_json::from_value(mixed).unwrap();
    assert_eq!(map.0.len(), 2);
    let dnat_e = map.0.get(&"1.2.3.4:443".parse().unwrap()).unwrap();
    assert!(matches!(dnat_e, PortForward::Dnat { .. }));
    let sni_e = map.0.get(&"5.6.7.8:443".parse().unwrap()).unwrap();
    match sni_e {
        PortForward::Sni { routes, .. } => {
            let r = routes.get("id.example.com").unwrap();
            assert!(r.enabled);
        }
        other => panic!("expected Sni, got {other:?}"),
    }
}

#[test]
fn sni_fallback_serde_backward_compat() {
    // Legacy SNI JSON (written before the `fallback` field existed) must
    // deserialize with `fallback: None` — this is why no migration is needed.
    let legacy = serde_json::json!({
        "kind": "sni",
        "routes": {
            "id.example.com": {
                "target": "10.59.0.2:443", "label": null, "enabled": true, "auto": true
            }
        }
    });
    let pf: PortForward = serde_json::from_value(legacy).unwrap();
    match pf {
        PortForward::Sni { fallback, .. } => assert!(fallback.is_none()),
        other => panic!("expected Sni, got {other:?}"),
    }

    // A fallback round-trips through serde.
    let with_fb = PortForward::Sni {
        routes: BTreeMap::new(),
        fallback: Some(SniRoute {
            target: "10.59.0.3:443".parse().unwrap(),
            label: Some("PCP".to_string()),
            enabled: true,
            auto: true,
        }),
    };
    let back: PortForward =
        serde_json::from_value(serde_json::to_value(&with_fb).unwrap()).unwrap();
    match back {
        PortForward::Sni {
            fallback: Some(f), ..
        } => assert_eq!(f.target, "10.59.0.3:443".parse().unwrap()),
        other => panic!("expected Sni with fallback, got {other:?}"),
    }
}

#[test]
fn port_forward_overlap_detection() {
    let dnat = |target: &str, count: u16| PortForward::Dnat {
        target: target.parse().unwrap(),
        label: None,
        enabled: true,
        count,
        auto: false,
    };
    let src = |s: &str| s.parse::<SocketAddrV4>().unwrap();

    let mut map = BTreeMap::new();
    // An existing 10-port range 8000..=8009 on WAN IP 1.2.3.4.
    map.insert(src("1.2.3.4:8000"), dnat("10.0.0.2:8000", 10));
    // An SNI forward occupying the single port 443.
    map.insert(
        src("1.2.3.4:443"),
        PortForward::Sni {
            routes: BTreeMap::new(),
            fallback: None,
        },
    );
    let forwards = PortForwards(map);

    // A single port inside the existing range overlaps it.
    assert_eq!(
        forwards.overlapping(src("1.2.3.4:8005"), 1),
        Some(src("1.2.3.4:8000")),
    );
    // A new range straddling the end of the existing range overlaps.
    assert_eq!(
        forwards.overlapping(src("1.2.3.4:8009"), 5),
        Some(src("1.2.3.4:8000")),
    );
    // A range that swallows the single SNI port overlaps it.
    assert_eq!(
        forwards.overlapping(src("1.2.3.4:440"), 8),
        Some(src("1.2.3.4:443")),
    );
    // An adjacent, disjoint range (8010..=8019) does not overlap.
    assert_eq!(forwards.overlapping(src("1.2.3.4:8010"), 10), None);
    // The same span on a different WAN IP does not overlap.
    assert_eq!(forwards.overlapping(src("5.6.7.8:8005"), 1), None);
    // The exact same source key is excluded (collision / re-assert, not overlap).
    assert_eq!(forwards.overlapping(src("1.2.3.4:8000"), 10), None);
}

#[test]
fn port_forward_occupied_gates_http_redirect() {
    let dnat = |target: &str, count: u16| PortForward::Dnat {
        target: target.parse().unwrap(),
        label: None,
        enabled: true,
        count,
        auto: false,
    };
    let src = |s: &str| s.parse::<SocketAddrV4>().unwrap();

    let mut map = BTreeMap::new();
    map.insert(src("1.2.3.4:80"), dnat("10.0.0.2:80", 1));
    map.insert(
        src("5.6.7.8:443"),
        PortForward::Sni {
            routes: BTreeMap::new(),
            fallback: None,
        },
    );
    // A range on 9.9.9.9 spanning 78..=82, which swallows port 80.
    map.insert(src("9.9.9.9:78"), dnat("10.0.0.2:78", 5));
    let pf = PortForwards(map);

    // A single-port forward exactly on :80 occupies it.
    assert!(pf.occupied(src("1.2.3.4:80")));
    // A range that covers 80 occupies it.
    assert!(pf.occupied(src("9.9.9.9:80")));
    // An IP whose only forward is on 443 leaves :80 free for the redirect.
    assert!(!pf.occupied(src("5.6.7.8:80")));
    // A :80 forward on one IP does not occupy :80 on a different IP.
    assert!(!pf.occupied(src("2.2.2.2:80")));
    // A neighboring port outside every span is free.
    assert!(!pf.occupied(src("9.9.9.9:90")));
}

#[test]
fn pinhole_persistence_round_trip() {
    let gua = |s: &str| s.parse::<SocketAddrV6>().unwrap();
    let ph = Pinhole {
        label: Some("Home Assistant".into()),
        enabled: true,
        count: 1,
        internal_port: None,
        auto: false,
    };
    let json = serde_json::to_value(&ph).unwrap();
    let back: Pinhole = serde_json::from_value(json).unwrap();
    assert_eq!(back.label.as_deref(), Some("Home Assistant"));
    assert!(back.enabled);
    assert_eq!(back.count, 1);
    assert!(!back.auto);
    // No remap → pure pinhole; internal port resolves to the external (key) port.
    assert_eq!(back.internal_port(8443), 8443);
    assert!(!back.is_dnat(8443));

    // A gateway (PCP) entry deserialized from a bare map: defaults fill enabled/count.
    let map: Pinholes6 = serde_json::from_value(serde_json::json!({
        "[2001:db8::1]:8443": { "label": "PCP", "auto": true }
    }))
    .unwrap();
    let e = map.0.get(&gua("[2001:db8::1]:8443")).unwrap();
    assert!(e.enabled);
    assert_eq!(e.count, 1);
    assert!(e.auto);
    assert_eq!(e.internal_port, None);

    // A port-DNAT entry (80 → 443, the v6 HTTP-redirect case).
    let redirect = Pinhole {
        label: Some("HTTP redirect".into()),
        enabled: true,
        count: 1,
        internal_port: Some(443),
        auto: false,
    };
    let back: Pinhole = serde_json::from_value(serde_json::to_value(&redirect).unwrap()).unwrap();
    assert_eq!(back.internal_port(80), 443);
    assert!(back.is_dnat(80));
}

#[test]
fn pinhole_overlap_detection() {
    let gua = |s: &str| s.parse::<SocketAddrV6>().unwrap();
    let ph = |count: u16| Pinhole {
        label: None,
        enabled: true,
        count,
        internal_port: None,
        auto: false,
    };
    let mut map = BTreeMap::new();
    map.insert(gua("[2001:db8::1]:8000"), ph(10));
    let holes = Pinholes6(map);
    // Overlaps the 8000..=8009 range.
    assert_eq!(
        holes.overlapping(gua("[2001:db8::1]:8005"), 1),
        Some(gua("[2001:db8::1]:8000"))
    );
    // Adjacent, disjoint (8010) does not overlap.
    assert_eq!(holes.overlapping(gua("[2001:db8::1]:8010"), 1), None);
    // Same span on a different GUA does not overlap.
    assert_eq!(holes.overlapping(gua("[2001:db8::2]:8005"), 1), None);
    // The exact key is excluded (collision / re-assert, not overlap).
    assert_eq!(holes.overlapping(gua("[2001:db8::1]:8000"), 10), None);
}
