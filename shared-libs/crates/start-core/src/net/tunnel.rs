use std::time::Duration;

use clap::Parser;
use imbl_value::InternedString;
use patch_db::json_ptr::JsonPointer;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::GatewayId;
use crate::context::{CliContext, RpcContext};
use crate::db::model::public::{
    GatewayType, NetworkInfo, NetworkInterfaceInfo, NetworkInterfaceType,
};
use crate::net::host::all_hosts;
use crate::prelude::*;

pub fn tunnel_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "add",
            from_fn_async(add_tunnel)
                .with_about("about.add-new-tunnel")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove_tunnel)
                .no_display()
                .with_about("about.remove-tunnel")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "update",
            from_fn_async(update_tunnel)
                .no_display()
                .with_about("about.update-tunnel")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AddTunnelParams {
    #[arg(help = "help.arg.tunnel-name")]
    name: InternedString,
    #[arg(help = "help.arg.wireguard-config")]
    config: String,
    #[arg(help = "help.arg.gateway-type")]
    #[serde(default, rename = "type")]
    gateway_type: Option<GatewayType>,
    #[arg(long, help = "help.arg.set-as-default-outbound")]
    set_as_default_outbound: bool,
}

pub async fn add_tunnel(
    ctx: RpcContext,
    AddTunnelParams {
        name,
        config,
        gateway_type,
        set_as_default_outbound,
    }: AddTunnelParams,
) -> Result<GatewayId, Error> {
    // Caller may declare the type; fall back to auto-detection only when absent.
    // StartTunnel configs carry a marker => inbound/outbound; anything else
    // (e.g. a commercial VPN like Mullvad) => outbound-only.
    let gateway_type = gateway_type.unwrap_or_else(|| {
        let marker = crate::tunnel::wg::START_TUNNEL_MARKER.to_lowercase();
        if config.to_lowercase().contains(&marker) {
            GatewayType::InboundOutbound
        } else {
            GatewayType::OutboundOnly
        }
    });

    let ifaces = ctx.net_controller.net_iface.watcher.subscribe();
    let mut iface = GatewayId::from(InternedString::intern("wg0"));
    if !ifaces.send_if_modified(|i| {
        for id in 1..256 {
            if !i.contains_key(&iface) {
                i.insert(
                    iface.clone(),
                    NetworkInterfaceInfo {
                        name: Some(name),
                        secure: None,
                        ip_info: None,
                        gateway_type,
                        port_map: Default::default(),
                        dns_update: Default::default(),
                    },
                );
                return true;
            }
            iface = InternedString::from_display(&lazy_format!("wg{id}")).into();
        }
        false
    }) {
        return Err(Error::new(
            eyre!("too many wireguard interfaces"),
            ErrorKind::InvalidRequest,
        ));
    }

    let mut sub = ctx
        .db
        .subscribe(
            "/public/serverInfo/network/gateways"
                .parse::<JsonPointer>()
                .with_kind(ErrorKind::Database)?
                .join_end(iface.as_str())
                .join_end("ipInfo"),
        )
        .await;

    crate::net::gateway::add_wireguard_config(iface.as_str(), &config).await?;

    sub.recv().await;

    if set_as_default_outbound {
        ctx.db
            .mutate(|db| {
                db.as_public_mut()
                    .as_server_info_mut()
                    .as_network_mut()
                    .as_default_outbound_mut()
                    .ser(&Some(iface.clone()))
            })
            .await
            .result?;
    }

    // Wait for the sync loop to fully commit gateway state (addresses, hosts)
    // to the database, with a 15-second timeout.
    if tokio::time::timeout(Duration::from_secs(15), async {
        let mut watch = ctx
            .db
            .watch("/public/serverInfo/network".parse::<JsonPointer>().unwrap())
            .await
            .typed::<NetworkInfo>();
        loop {
            if watch
                .peek()?
                .as_gateways()
                .as_idx(&iface)
                .and_then(|g| g.as_ip_info().transpose_ref())
                .is_some()
            {
                break;
            }
            watch.changed().await?;
        }
        Ok::<_, Error>(())
    })
    .await
    .is_err()
    {
        tracing::warn!(
            "{}",
            t!(
                "net.tunnel.timeout-waiting-for-add",
                gateway = iface.as_str()
            )
        );
    }

    Ok(iface)
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
pub struct RemoveTunnelParams {
    #[arg(help = "help.arg.gateway-id")]
    id: GatewayId,
}
pub async fn remove_tunnel(
    ctx: RpcContext,
    RemoveTunnelParams { id }: RemoveTunnelParams,
) -> Result<(), Error> {
    let Some(existing) = ctx
        .db
        .peek()
        .await
        .into_public()
        .into_server_info()
        .into_network()
        .into_gateways()
        .into_idx(&id)
        .and_then(|e| e.into_ip_info().transpose())
    else {
        return Ok(());
    };

    if existing.as_deref().as_device_type().de()? != Some(NetworkInterfaceType::Wireguard) {
        return Err(Error::new(
            eyre!("network interface {id} is not a proxy"),
            ErrorKind::InvalidRequest,
        ));
    }

    ctx.db
        .mutate(|db| {
            let hostname = crate::hostname::ServerHostname::load(db.as_public().as_server_info())?;
            let gateways = db
                .as_public()
                .as_server_info()
                .as_network()
                .as_gateways()
                .de()?;
            let ports = db.as_private().as_available_ports().de()?;
            for host in all_hosts(db) {
                let host = host?;
                host.as_public_domains_mut()
                    .mutate(|p| Ok(p.retain(|_, v| v.gateway != id)))?;
                host.update_addresses(&hostname, &gateways, &ports)?;
            }

            Ok(())
        })
        .await
        .result?;

    ctx.net_controller.net_iface.delete_iface(&id).await?;

    ctx.db
        .mutate(|db| {
            let hostname = crate::hostname::ServerHostname::load(db.as_public().as_server_info())?;
            let gateways = db
                .as_public()
                .as_server_info()
                .as_network()
                .as_gateways()
                .de()?;
            let ports = db.as_private().as_available_ports().de()?;
            for host in all_hosts(db) {
                let host = host?;
                host.as_private_domains_mut().mutate(|d| {
                    for gateways in d.values_mut() {
                        gateways.remove(&id);
                    }
                    d.retain(|_, gateways| !gateways.is_empty());
                    Ok(())
                })?;
                host.update_addresses(&hostname, &gateways, &ports)?;
            }

            Ok(())
        })
        .await
        .result?;

    // Wait for the sync loop to fully commit gateway removal to the database,
    // with a 15-second timeout.
    if tokio::time::timeout(Duration::from_secs(15), async {
        let mut watch = ctx
            .db
            .watch("/public/serverInfo/network".parse::<JsonPointer>().unwrap())
            .await
            .typed::<NetworkInfo>();
        loop {
            if watch.peek()?.as_gateways().as_idx(&id).is_none() {
                break;
            }
            watch.changed().await?;
        }
        Ok::<_, Error>(())
    })
    .await
    .is_err()
    {
        tracing::warn!(
            "{}",
            t!(
                "net.tunnel.timeout-waiting-for-remove",
                gateway = id.as_str()
            )
        );
    }

    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
pub struct UpdateTunnelParams {
    #[arg(help = "help.arg.gateway-id")]
    id: GatewayId,
    #[arg(help = "help.arg.wireguard-config")]
    config: String,
}

/// Replace the WireGuard config behind an existing gateway interface in place,
/// keeping the gateway id and everything keyed to it (forwards, private/public
/// domains). Used to re-issue a config — e.g. one that now carries a `DNS =`
/// line. Applied via D-Bus Update2 + Device.Reapply, so the wg device is never
/// torn down and the gateway (and the request riding its tunnel) survives.
pub async fn update_tunnel(
    ctx: RpcContext,
    UpdateTunnelParams { id, config }: UpdateTunnelParams,
) -> Result<(), Error> {
    let Some(existing) = ctx
        .db
        .peek()
        .await
        .into_public()
        .into_server_info()
        .into_network()
        .into_gateways()
        .into_idx(&id)
        .and_then(|e| e.into_ip_info().transpose())
    else {
        return Err(Error::new(
            eyre!("unknown gateway: {id}"),
            ErrorKind::NotFound,
        ));
    };

    if existing.as_deref().as_device_type().de()? != Some(NetworkInterfaceType::Wireguard) {
        return Err(Error::new(
            eyre!("network interface {id} is not a proxy"),
            ErrorKind::InvalidRequest,
        ));
    }

    // Apply the new config in place (Update2 + Reapply) rather than deleting and
    // re-importing the connection. The wg device is never torn down, so updating
    // the gateway that carries this very request doesn't drop its own transport —
    // and if the handler is cancelled, the gateway is simply left on its old (or
    // new) config, never in a half-deleted state.
    crate::net::gateway::update_wireguard_config(id.as_str(), &config).await
}
