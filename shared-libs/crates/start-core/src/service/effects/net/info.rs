use std::net::Ipv4Addr;

use imbl::OrdMap;
use patch_db::TypedDbWatch;
use patch_db::json_ptr::JsonPointer;
use tokio::process::Command;

use crate::db::model::public::NetworkInterfaceInfo;
use crate::service::effects::callbacks::CallbackHandler;
use crate::service::effects::prelude::*;
use crate::service::rpc::CallbackId;
use crate::util::Invoke;
use crate::{GatewayId, PackageId};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GetContainerIpParams {
    package_id: Option<PackageId>,

    callback: Option<CallbackId>,
}

pub async fn get_container_ip(
    context: EffectContext,
    GetContainerIpParams {
        package_id,
        callback,
    }: GetContainerIpParams,
) -> Result<Option<Ipv4Addr>, Error> {
    let context = context.deref()?;

    if let Some(package_id) = package_id.filter(|id| id != &context.seed.id) {
        if let Some(callback) = callback {
            // ip is static for the lifetime of the container, so callback unnecessary for self
            let callback = callback.register(&context.seed.persistent_container);
            context
                .seed
                .ctx
                .callbacks
                .add_get_container_ip(package_id.clone(), CallbackHandler::new(&context, callback));
        }
        let Some(svc) = &*context.seed.ctx.services.get(&package_id).await else {
            return Ok(None);
        };

        let Some(lxc) = svc.seed.persistent_container.lxc_container.get() else {
            return Ok(None);
        };
        let res = lxc.ip().await?;

        Ok(Some(res))
    } else {
        let Some(lxc) = context.seed.persistent_container.lxc_container.get() else {
            return Ok(None);
        };
        lxc.ip().await.map(Some)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GetOutboundGatewayParams {
    callback: Option<CallbackId>,
}

pub async fn get_outbound_gateway(
    context: EffectContext,
    GetOutboundGatewayParams { callback }: GetOutboundGatewayParams,
) -> Result<GatewayId, Error> {
    let context = context.deref()?;
    let ctx = &context.seed.ctx;

    // Resolve the effective gateway; DB watches are created atomically
    // with each read to avoid race conditions.
    let (gw, pkg_watch, os_watch, gateways_watch) =
        resolve_outbound_gateway(ctx, &context.seed.id).await?;

    if let Some(callback) = callback {
        let callback = callback.register(&context.seed.persistent_container);
        context.seed.ctx.callbacks.add_get_outbound_gateway(
            context.seed.id.clone(),
            pkg_watch,
            os_watch,
            gateways_watch,
            CallbackHandler::new(&context, callback),
        );
    }

    Ok(gw)
}

async fn resolve_outbound_gateway(
    ctx: &crate::context::RpcContext,
    package_id: &PackageId,
) -> Result<
    (
        GatewayId,
        TypedDbWatch<Option<GatewayId>>,
        Option<TypedDbWatch<Option<GatewayId>>>,
        Option<TypedDbWatch<OrdMap<GatewayId, NetworkInterfaceInfo>>>,
    ),
    Error,
> {
    // 1. Package-specific outbound gateway — subscribe before reading
    let pkg_ptr: JsonPointer = format!("/public/packageData/{}/outboundGateway", package_id)
        .parse()
        .expect("valid json pointer");
    let mut pkg_watch = ctx.db.watch(pkg_ptr).await;
    let pkg_gw: Option<GatewayId> = imbl_value::from_value(pkg_watch.peek_and_mark_seen()?)?;

    if let Some(gw) = pkg_gw {
        return Ok((gw, pkg_watch.typed(), None, None));
    }

    // 2. OS-level default outbound — subscribe before reading
    let os_ptr: JsonPointer = "/public/serverInfo/network/defaultOutbound"
        .parse()
        .expect("valid json pointer");
    let mut os_watch = ctx.db.watch(os_ptr).await;
    let default_outbound: Option<GatewayId> =
        imbl_value::from_value(os_watch.peek_and_mark_seen()?)?;

    if let Some(gw) = default_outbound {
        return Ok((gw, pkg_watch.typed(), Some(os_watch.typed()), None));
    }

    // 3. Fall through to main routing table — watch gateways for changes
    let gw_ptr: JsonPointer = "/public/serverInfo/network/gateways"
        .parse()
        .expect("valid json pointer");
    let mut gateways_watch = ctx.db.watch(gw_ptr).await;
    gateways_watch.peek_and_mark_seen()?;

    let gw = default_route_interface().await?;
    Ok((
        gw,
        pkg_watch.typed(),
        Some(os_watch.typed()),
        Some(gateways_watch.typed()),
    ))
}

/// Parses `ip route show table main` for the default route's `dev` field.
async fn default_route_interface() -> Result<GatewayId, Error> {
    let output = Command::new("ip")
        .arg("route")
        .arg("show")
        .arg("table")
        .arg("main")
        .invoke(ErrorKind::Network)
        .await?;
    let text = String::from_utf8_lossy(&output);
    for line in text.lines() {
        if line.starts_with("default ") {
            let mut parts = line.split_whitespace();
            while let Some(tok) = parts.next() {
                if tok == "dev" {
                    if let Some(dev) = parts.next() {
                        return Ok(dev.parse().unwrap());
                    }
                }
            }
        }
    }
    Err(Error::new(
        eyre!("no default route found in main routing table"),
        ErrorKind::Network,
    ))
}
