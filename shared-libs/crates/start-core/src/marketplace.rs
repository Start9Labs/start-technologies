use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use url::Url;

use crate::context::RpcContext;
use crate::prelude::*;
use crate::util::DataUrl;

const MANIFEST_URL: &str = "https://marketplace.start9.com/.well-known/startos/registries.json";

/// A registry Start9 lists, and the identity it is expected to present.
#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KnownRegistry {
    #[ts(type = "string")]
    pub url: Url,
    pub name: String,
    pub icon: Option<DataUrl<'static>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    registries: Vec<KnownRegistry>,
}

pub fn marketplace_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new().subcommand("known-registries", from_fn_async(known_registries).no_cli())
}

pub async fn known_registries(ctx: RpcContext) -> Result<Vec<KnownRegistry>, Error> {
    Ok(ctx
        .client
        .get(MANIFEST_URL)
        .send()
        .await?
        .error_for_status()?
        .json::<Manifest>()
        .await?
        .registries)
}
