use std::collections::BTreeMap;

use clap::Parser;
use imbl_value::InternedString;
use itertools::Itertools;
use rpc_toolkit::{Context, Empty, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::context::CliContext;
use crate::prelude::*;
use crate::registry::context::RegistryContext;
use crate::registry::package::index::Category;
use crate::s9pk::manifest::LocaleString;
use crate::util::DataUrl;
use crate::util::serde::{HandlerExtSerde, WithIoFormat};

pub fn info_api<C: Context>() -> ParentHandler<C, WithIoFormat<Empty>> {
    ParentHandler::<C, WithIoFormat<Empty>>::new()
        .root_handler(
            from_fn_async(get_info)
                .with_metadata("authenticated", Value::Bool(false))
                .with_display_serializable()
                .with_about("about.display-registry-info")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-name",
            from_fn_async(set_name)
                .with_metadata("admin", Value::Bool(true))
                .no_display()
                .with_about("about.set-registry-name")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-description",
            from_fn_async(set_description)
                .with_metadata("admin", Value::Bool(true))
                .no_display()
                .with_about("about.set-registry-description")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-icon",
            from_fn_async(set_icon)
                .with_metadata("admin", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "set-icon",
            from_fn_async(cli_set_icon)
                .no_display()
                .with_about("about.set-registry-icon"),
        )
}

#[derive(Debug, Default, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RegistryInfo {
    pub name: Option<String>,
    pub icon: Option<DataUrl<'static>>,
    /// Markdown, shown above the registry's services.
    pub description: Option<LocaleString>,
    pub categories: BTreeMap<InternedString, Category>,
}

pub async fn get_info(ctx: RegistryContext) -> Result<RegistryInfo, Error> {
    let peek = ctx.db.peek().await.into_index();
    Ok(RegistryInfo {
        name: peek.as_name().de()?,
        icon: peek.as_icon().de()?,
        description: peek.as_description().de()?,
        categories: peek.as_package().as_categories().de()?,
    })
}

#[derive(Debug, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SetNameParams {
    #[arg(help = "help.arg.registry-name")]
    pub name: String,
}

pub async fn set_name(
    ctx: RegistryContext,
    SetNameParams { name }: SetNameParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| db.as_index_mut().as_name_mut().ser(&Some(name)))
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SetDescriptionParams {
    #[arg(help = "help.arg.registry-description")]
    pub description: LocaleString,
}

pub async fn set_description(
    ctx: RegistryContext,
    SetDescriptionParams { description }: SetDescriptionParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_index_mut()
                .as_description_mut()
                .ser(&Some(description))
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SetIconParams {
    pub icon: DataUrl<'static>,
}

pub async fn set_icon(
    ctx: RegistryContext,
    SetIconParams { icon }: SetIconParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| db.as_index_mut().as_icon_mut().ser(&Some(icon)))
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CliSetIconParams {
    #[arg(help = "help.arg.icon-source")]
    pub icon: String,
}

pub async fn cli_set_icon(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        params: CliSetIconParams { icon },
        ..
    }: HandlerArgs<CliContext, CliSetIconParams>,
) -> Result<(), Error> {
    let data_url = if icon.starts_with("data:") {
        icon.parse::<DataUrl<'static>>()
            .with_kind(ErrorKind::ParseUrl)?
    } else if icon.starts_with("https://") || icon.starts_with("http://") {
        let res = ctx
            .client
            .get(&icon)
            .send()
            .await
            .with_kind(ErrorKind::Network)?;
        DataUrl::from_response(res).await?
    } else {
        let path = icon.strip_prefix("file://").unwrap_or(&icon);
        DataUrl::from_path(path).await?
    };
    ctx.call_remote::<RegistryContext>(
        &parent_method.into_iter().chain(method).join("."),
        imbl_value::json!({
            "icon": data_url,
        }),
    )
    .await?;
    Ok(())
}
