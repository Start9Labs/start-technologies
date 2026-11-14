use std::collections::{BTreeMap, BTreeSet};

use axum::Router;
use axum::extract::Request;
use axum::http::{StatusCode, Uri};
use axum::response::Redirect;
use futures::future::ready;
use imbl_value::InternedString;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, Server, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::context::CliContext;
use crate::middleware::auth::Auth;
use crate::middleware::auth::session::RpcSessionUpgrader;
use crate::middleware::cors;
use crate::net::http::request_authority;
use crate::net::static_server::{bad_request, not_found, server_error};
use crate::prelude::*;
use crate::registry::context::RegistryContext;
use crate::registry::device_info::DeviceInfoMiddleware;
use crate::registry::os::index::OsIndex;
use crate::registry::package::index::{PackageIndex, ReaderParams};
use crate::registry::signer::SignerInfo;
use crate::rpc_continuations::{Guid, RpcContinuations};
use crate::s9pk::manifest::LocaleString;
use crate::util::DataUrl;
use crate::util::serde::HandlerExtSerde;

pub mod admin;
pub mod asset;
mod cli_identity;
mod compat;
pub mod context;
pub mod db;
pub mod device_info;
pub mod info;
pub mod metrics;
mod migrations;
pub mod mirror;
pub mod os;
pub mod package;
pub mod signer;
pub mod sync;

#[cfg(test)]
pub(crate) mod tests;

#[derive(Debug, Default, Deserialize, Serialize, HasModel)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct RegistryDatabase {
    #[serde(default)]
    pub server_identity_key: crate::sign::instance::StoredIdentityKey,
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

#[derive(Debug, Default, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct FullIndex {
    pub name: Option<String>,
    pub icon: Option<DataUrl<'static>>,
    pub description: Option<LocaleString>,
    pub package: PackageIndex,
    pub os: OsIndex,
    pub signers: BTreeMap<Guid, SignerInfo>,
    #[serde(default)]
    pub trust_signers: BTreeSet<Guid>,
}

rpc_toolkit::reflect_ts!(FullIndex);
rpc_toolkit::ts_export!(FullIndex, namespaces = [""]);

pub async fn get_full_index(
    ctx: RegistryContext,
    params: ReaderParams,
) -> Result<FullIndex, Error> {
    let db = ctx.db.peek().await;
    let reader = params.reader(&db)?;
    let mut index = db.into_index();
    index.as_package_mut().retain_visible(reader.as_ref())?;
    index.de()
}

pub fn registry_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "index",
            from_fn_async(get_full_index)
                .with_metadata("authenticated", Value::Bool(false))
                .with_metadata("get_signer", Value::Bool(true))
                .with_display_serializable()
                .with_about("about.list-registry-info-packages")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "info",
            info::info_api::<C>().with_about("about.commands-registry-info"),
        )
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

pub(crate) fn registry_server(ctx: RegistryContext) -> rpc_toolkit::HttpServer<RegistryContext> {
    Server::new(move || ready(Ok(ctx.clone())), registry_api())
        .middleware(Auth::new().with_local_auth().with_signature_auth())
        .middleware(DeviceInfoMiddleware::new())
}

async fn marketplace_redirect(req: Request) -> Result<Redirect, StatusCode> {
    let scheme = req
        .headers()
        .get("x-forwarded-proto")
        .map(|value| value.to_str())
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .or(req.uri().scheme_str())
        .unwrap_or("http");
    if !matches!(scheme, "http" | "https") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let authority = request_authority(&req).ok_or(StatusCode::BAD_REQUEST)?;
    let registry = Uri::builder()
        .scheme(scheme)
        .authority(authority)
        .path_and_query("/")
        .build()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let mut marketplace = url::Url::parse("https://marketplace.start9.com/").unwrap();
    marketplace
        .query_pairs_mut()
        .append_pair("registry", &registry.to_string());
    Ok(Redirect::temporary(marketplace.as_str()))
}

pub fn registry_router(ctx: RegistryContext) -> Router {
    use axum::extract as x;
    use axum::routing::{any, get};
    let server = registry_server(ctx.clone());
    let continuation = get({
        let ctx = ctx.clone();
        move |x::Path(path): x::Path<String>, ws: axum::extract::ws::WebSocketUpgrade| async move {
            match Guid::from(&path) {
                None => {
                    tracing::debug!("No Guid Path");
                    bad_request()
                }
                Some(guid) => match ctx.rpc_continuations.get_ws_handler(&guid).await {
                    Some(cont) => cont.on_upgrade(ws, guid),
                    _ => not_found(),
                },
            }
        }
    });
    Router::new()
        .route("/", get(marketplace_redirect))
        .route("/rpc/{*path}", any(server.clone()).layer(cors::layer()))
        .route(RpcSessionUpgrader::PATH, RpcSessionUpgrader::route(server))
        .route(
            &format!("/{}/{{*path}}", package::icon::ICON_PATH),
            get({
                let ctx = ctx.clone();
                move |x::Path(path): x::Path<String>, request: x::Request| async move {
                    package::icon::stored_icon(&ctx, &path, request).await
                }
            }),
        )
        .route(
            &format!("{}/{{*path}}", RpcSessionUpgrader::PATH),
            continuation.clone(),
        )
        .route(
            &format!("{}/{{*path}}", RpcContinuations::LEGACY_WS_PATH),
            continuation,
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
