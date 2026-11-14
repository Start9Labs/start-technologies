use clap::Parser;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::context::{CliContext, RpcContext};
use crate::db::model::Database;
use crate::net::host::binding::BindingPreview;
use crate::net::service_interface::{HostnameInfo, ServiceInterface};
use crate::prelude::*;
use crate::registry::bittorrent::{TorrentClient, TorrentPreferences};
use crate::util::serde::HandlerExtSerde;

mod incoming;
pub(crate) use incoming::{configure_incoming, retained_bindings};
#[cfg(test)]
mod tests;

pub(crate) fn api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "config",
            from_fn_async(config)
                .with_display_serializable()
                .with_about("about.system.bittorrent-config")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-preferences",
            from_fn_async(set_preferences)
                .with_metadata("sync_db", Value::Bool(true))
                .with_display_serializable()
                .with_about("about.system.set-bittorrent-preferences")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-address-enabled",
            from_fn_async(set_address_enabled)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "set-gua-wan",
            from_fn_async(set_gua_wan)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
}

/// Effective transport availability and incoming candidates without reserved resources.
#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
struct BitTorrentConfig {
    available: bool,
    interface: ServiceInterface,
    preview: Option<BindingPreview>,
}

rpc_toolkit::reflect_ts!(BitTorrentConfig);
rpc_toolkit::ts_export!(BitTorrentConfig, namespaces = [""]);

async fn config(ctx: RpcContext) -> Result<BitTorrentConfig, Error> {
    Ok(BitTorrentConfig {
        available: ctx.torrents.available(),
        interface: incoming::interface(),
        preview: incoming::preview(&ctx.db.peek().await)?,
    })
}

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
struct SelectAddressParams {
    address: HostnameInfo,
    enabled: bool,
}
rpc_toolkit::reflect_ts!(SelectAddressParams);

async fn set_address_enabled(
    ctx: RpcContext,
    params: SelectAddressParams,
) -> Result<HostnameInfo, Error> {
    let available = ctx.torrents.available();
    ctx.db
        .mutate(|db| incoming::select_address_in(db, available, params.address, params.enabled))
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
struct SelectGuaParams {
    address: HostnameInfo,
    wan: bool,
}
rpc_toolkit::reflect_ts!(SelectGuaParams);

async fn set_gua_wan(ctx: RpcContext, params: SelectGuaParams) -> Result<(), Error> {
    let available = ctx.torrents.available();
    ctx.db
        .mutate(|db| incoming::select_gua_in(db, available, params.address, params.wan))
        .await
        .result
}

#[derive(Debug, Default, Deserialize, Serialize, Parser, VisitFields)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
struct SetPreferencesParams {
    #[arg(long, action = clap::ArgAction::Set, help = "help.arg.bittorrent-downloads")]
    downloads: Option<bool>,
    #[arg(long, action = clap::ArgAction::Set, help = "help.arg.seeding-enabled")]
    seeding: Option<bool>,
}

rpc_toolkit::reflect_ts!(SetPreferencesParams);

async fn set_preferences(
    ctx: RpcContext,
    params: SetPreferencesParams,
) -> Result<TorrentPreferences, Error> {
    update_preferences(&ctx.db, params).await
}

/// Stops peer work on watcher failure without changing saved download or sharing intent.
pub(crate) async fn watcher_failed(
    db: &TypedPatchDb<Database>,
    torrents: &TorrentClient,
) -> Result<(), Error> {
    torrents.shutdown();
    db.mutate(|db| {
        db.as_public_mut()
            .as_server_info_mut()
            .as_status_info_mut()
            .as_bittorrent_watcher_failed_mut()
            .ser(&true)?;
        incoming::clear_selections(db)
    })
    .await
    .result
}

async fn update_preferences(
    db: &TypedPatchDb<Database>,
    params: SetPreferencesParams,
) -> Result<TorrentPreferences, Error> {
    db.mutate(|db| {
        let stored = db.as_public_mut().as_server_info_mut().as_bittorrent_mut();
        let preferences: TorrentPreferences = stored.de()?;
        let preferences = preferences.update(params.downloads, params.seeding);
        stored.ser(&preferences)?;
        if !preferences.seeding {
            incoming::clear_selections(db)?;
        }
        Ok(preferences)
    })
    .await
    .result
}
