use std::collections::{BTreeMap, BTreeSet};

use axum::Router;
use futures::future::ready;
use imbl_value::InternedString;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, Server, from_fn_async};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::context::CliContext;
use crate::middleware::auth::Auth;
use crate::middleware::cors::Cors;
use crate::net::static_server::{bad_request, not_found, server_error};
use crate::prelude::*;
use crate::registry::context::RegistryContext;
use crate::registry::device_info::DeviceInfoMiddleware;
use crate::registry::os::index::OsIndex;
use crate::registry::package::index::PackageIndex;
use crate::registry::signer::SignerInfo;
use crate::rpc_continuations::Guid;
use crate::s9pk::manifest::LocaleString;
use crate::util::DataUrl;
use crate::util::serde::HandlerExtSerde;

pub mod admin;
pub mod asset;
pub mod context;
pub mod db;
pub mod device_info;
pub mod info;
pub mod metrics;
mod migrations;
pub mod os;
pub mod package;
pub mod signer;

#[derive(Debug, Default, Deserialize, Serialize, HasModel)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct RegistryDatabase {
    #[serde(default)]
    pub migrations: BTreeSet<InternedString>,
    pub admins: BTreeSet<Guid>,
    pub index: FullIndex,
}

impl RegistryDatabase {
    pub fn init() -> Self {
        Self {
            migrations: migrations::MIGRATIONS
                .iter()
                .map(|m| m.name().into())
                .collect(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct FullIndex {
    pub name: Option<String>,
    pub icon: Option<DataUrl<'static>>,
    pub description: Option<LocaleString>,
    pub package: PackageIndex,
    pub os: OsIndex,
    pub signers: BTreeMap<Guid, SignerInfo>,
}

pub async fn get_full_index(ctx: RegistryContext) -> Result<FullIndex, Error> {
    ctx.db.peek().await.into_index().de()
}

pub fn registry_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "index",
            from_fn_async(get_full_index)
                .with_display_serializable()
                .with_about("about.list-registry-info-packages")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "info",
            info::info_api::<C>().with_about("about.commands-registry-info"),
        )
        // set info and categories
        .subcommand(
            "os",
            os::os_api::<C>().with_about("about.commands-os-assets-versions"),
        )
        .subcommand(
            "package",
            package::package_api::<C>().with_about("about.commands-index-add-get-packages"),
        )
        .subcommand(
            "admin",
            admin::admin_api::<C>().with_about("about.commands-add-list-admins-signers"),
        )
        .subcommand(
            "db",
            db::db_api::<C>().with_about("about.commands-registry-db"),
        )
        .subcommand(
            "metrics",
            metrics::metrics_api::<C>().with_about("about.commands-registry-metrics"),
        )
}

pub fn registry_router(ctx: RegistryContext) -> Router {
    use axum::extract as x;
    use axum::routing::{any, get};
    Router::new()
        .route("/rpc/{*path}", {
            let ctx = ctx.clone();
            any(
                Server::new(move || ready(Ok(ctx.clone())), registry_api())
                    .middleware(Cors::new())
                    .middleware(Auth::new().with_local_auth().with_signature_auth())
                    .middleware(DeviceInfoMiddleware::new()),
            )
        })
        .route(
            "/ws/rpc/{*path}",
            get({
                let ctx = ctx.clone();
                move |x::Path(path): x::Path<String>,
                      ws: axum::extract::ws::WebSocketUpgrade| async move {
                    match Guid::from(&path) {
                        None => {
                            tracing::debug!("No Guid Path");
                            bad_request()
                        }
                        Some(guid) => match ctx.rpc_continuations.get_ws_handler(&guid).await {
                            Some(cont) => ws.on_upgrade(cont),
                            _ => not_found(),
                        },
                    }
                }
            }),
        )
        .route(
            "/rest/rpc/{*path}",
            any({
                let ctx = ctx.clone();
                move |request: x::Request| async move {
                    let path = request
                        .uri()
                        .path()
                        .strip_prefix("/rest/rpc/")
                        .unwrap_or_default();
                    match Guid::from(&path) {
                        None => {
                            tracing::debug!("No Guid Path");
                            bad_request()
                        }
                        Some(guid) => match ctx.rpc_continuations.get_rest_handler(&guid).await {
                            None => not_found(),
                            Some(cont) => cont(request).await.unwrap_or_else(server_error),
                        },
                    }
                }
            }),
        )
}
