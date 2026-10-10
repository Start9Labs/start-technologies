use std::collections::BTreeMap;

use clap::Parser;
use imbl_value::InternedString;
use itertools::Itertools;
use rpc_toolkit::{Context, Empty, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

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
                .no_cli(),
        )
        .subcommand(
            "set-description",
            from_fn_async(cli_set_description)
                .no_display()
                .with_about("about.set-registry-description"),
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

#[derive(Debug, Default, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct RegistryInfo {
    pub name: Option<String>,
    pub icon: Option<DataUrl<'static>>,
    /// Markdown, shown above the registry's services.
    pub description: Option<LocaleString>,
    pub categories: BTreeMap<InternedString, Category>,
}

rpc_toolkit::reflect_ts!(RegistryInfo);
rpc_toolkit::ts_export!(RegistryInfo, namespaces = [""]);

pub async fn get_info(ctx: RegistryContext) -> Result<RegistryInfo, Error> {
    let peek = ctx.db.peek().await.into_index();
    Ok(RegistryInfo {
        name: peek.as_name().de()?,
        icon: peek.as_icon().de()?,
        description: peek.as_description().de()?,
        categories: peek.as_package().as_categories().de()?,
    })
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct SetNameParams {
    #[arg(help = "help.arg.registry-name")]
    pub name: String,
}

rpc_toolkit::reflect_ts!(SetNameParams);
rpc_toolkit::ts_export!(SetNameParams, namespaces = [""]);

pub async fn set_name(
    ctx: RegistryContext,
    SetNameParams { name }: SetNameParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| db.as_index_mut().as_name_mut().ser(&Some(name)))
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SetDescriptionParams {
    #[serde(deserialize_with = "Option::deserialize")]
    #[visit(
        input_wire = "Option<LocaleString>",
        type_attributes(visit::input_wire)
    )]
    pub description: Option<LocaleString>,
}

rpc_toolkit::reflect_ts!(SetDescriptionParams);
rpc_toolkit::ts_export!(SetDescriptionParams, namespaces = [""]);

pub async fn set_description(
    ctx: RegistryContext,
    SetDescriptionParams { description }: SetDescriptionParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| db.as_index_mut().as_description_mut().ser(&description))
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct CliSetDescriptionParams {
    #[arg(
        help = "help.arg.registry-description",
        required_unless_present = "clear",
        conflicts_with = "clear"
    )]
    pub description: Option<LocaleString>,
    #[arg(long, help = "help.arg.clear-registry-description")]
    pub clear: bool,
}

rpc_toolkit::reflect_ts!(CliSetDescriptionParams);
rpc_toolkit::ts_export!(CliSetDescriptionParams, namespaces = [""]);

pub async fn cli_set_description(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        params: CliSetDescriptionParams { description, .. },
        ..
    }: HandlerArgs<CliContext, CliSetDescriptionParams>,
) -> Result<(), Error> {
    ctx.call_remote::<RegistryContext>(
        &parent_method.into_iter().chain(method).join("."),
        imbl_value::json!({ "description": description }),
    )
    .await?;
    Ok(())
}

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SetIconParams {
    #[serde(deserialize_with = "Option::deserialize")]
    #[visit(
        input_wire = "Option<DataUrl<'static>>",
        type_attributes(visit::input_wire)
    )]
    pub icon: Option<DataUrl<'static>>,
}

rpc_toolkit::reflect_ts!(SetIconParams);
rpc_toolkit::ts_export!(SetIconParams, namespaces = [""]);

pub async fn set_icon(
    ctx: RegistryContext,
    SetIconParams { icon }: SetIconParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| db.as_index_mut().as_icon_mut().ser(&icon))
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct CliSetIconParams {
    #[arg(
        help = "help.arg.icon-source",
        required_unless_present = "clear",
        conflicts_with = "clear"
    )]
    pub icon: Option<String>,
    #[arg(long, help = "help.arg.clear-registry-icon")]
    pub clear: bool,
}

rpc_toolkit::reflect_ts!(CliSetIconParams);
rpc_toolkit::ts_export!(CliSetIconParams, namespaces = [""]);

pub async fn cli_set_icon(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        params: CliSetIconParams { icon, .. },
        ..
    }: HandlerArgs<CliContext, CliSetIconParams>,
) -> Result<(), Error> {
    let data_url = if let Some(icon) = icon {
        Some(if icon.starts_with("data:") {
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
        })
    } else {
        None
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_setters_require_value_or_clear() {
        for value in [
            "icon.png",
            "file://icon.png",
            "https://example.com/icon.png",
            "data:image/png;base64,aWNvbg==",
        ] {
            let params = CliSetIconParams::try_parse_from(["set-icon", value]).unwrap();
            assert_eq!(params.icon.as_deref(), Some(value));
            assert!(!params.clear);
        }
        let params = CliSetIconParams::try_parse_from(["set-icon", "--clear"]).unwrap();
        assert!(params.icon.is_none());
        assert!(params.clear);
        assert!(CliSetIconParams::try_parse_from(["set-icon"]).is_err());
        assert!(CliSetIconParams::try_parse_from(["set-icon", "icon.png", "--clear"]).is_err());
        assert!(CliSetIconParams::try_parse_from(["set-icon", "--clear", "icon.png"]).is_err());

        for value in ["Description", "", r#"{"en_US":"Description"}"#] {
            let params =
                CliSetDescriptionParams::try_parse_from(["set-description", value]).unwrap();
            assert_eq!(params.description, Some(value.parse().unwrap()));
            assert!(!params.clear);
        }
        let params =
            CliSetDescriptionParams::try_parse_from(["set-description", "--clear"]).unwrap();
        assert!(params.description.is_none());
        assert!(params.clear);
        assert!(CliSetDescriptionParams::try_parse_from(["set-description"]).is_err());
        assert!(
            CliSetDescriptionParams::try_parse_from(["set-description", "Description", "--clear"])
                .is_err()
        );
        assert!(
            CliSetDescriptionParams::try_parse_from(["set-description", "--clear", "Description"])
                .is_err()
        );
        assert!(SetNameParams::try_parse_from(["set-name", "--clear"]).is_err());
    }

    #[cfg(feature = "ts")]
    #[test]
    fn rpc_setter_bindings_require_nullable_values() {
        use rpc_toolkit::ts::{Direction, TSVisitor};

        let mut visitor = TSVisitor::new();
        visitor.with_direction(Direction::Input, |visitor| {
            visitor.append_type::<SetIconParams>();
            visitor.append_type::<SetDescriptionParams>();
        });
        let declarations = visitor.into_declarations().unwrap();
        for (field, target) in [
            ("icon", "DataUrlInput"),
            ("description", "LocaleStringInput"),
        ] {
            assert!(
                declarations.contains(&format!("\"{field}\":")),
                "{declarations}"
            );
            assert!(
                !declarations.contains(&format!("\"{field}\"?:")),
                "{declarations}"
            );
            assert!(
                declarations.contains(&format!("{target}|null")),
                "{declarations}"
            );
        }
    }

    #[test]
    fn rpc_setters_require_explicit_nullable_values() {
        let icon = "data:image/png;base64,aWNvbg==";
        let params: SetIconParams =
            imbl_value::from_value(imbl_value::json!({ "icon": icon })).unwrap();
        assert_eq!(
            serde_json::to_value(params).unwrap(),
            serde_json::json!({ "icon": icon })
        );
        let params: SetIconParams =
            imbl_value::from_value(imbl_value::json!({ "icon": null })).unwrap();
        assert!(params.icon.is_none());
        assert_eq!(
            serde_json::to_value(params).unwrap(),
            serde_json::json!({ "icon": null })
        );
        assert!(imbl_value::from_value::<SetIconParams>(imbl_value::json!({})).is_err());
        assert!(
            imbl_value::from_value::<SetIconParams>(imbl_value::json!({ "icon": "icon.png" }))
                .is_err()
        );
        assert!(imbl_value::from_value::<SetIconParams>(imbl_value::json!({ "icon": 1 })).is_err());

        for description in [
            serde_json::json!("Description"),
            serde_json::json!(""),
            serde_json::json!({ "en_US": "Description" }),
        ] {
            let value = serde_json::json!({ "description": description });
            let params: SetDescriptionParams = serde_json::from_value(value.clone()).unwrap();
            assert!(params.description.is_some());
            assert_eq!(serde_json::to_value(params).unwrap(), value);
        }
        let params: SetDescriptionParams =
            imbl_value::from_value(imbl_value::json!({ "description": null })).unwrap();
        assert!(params.description.is_none());
        assert_eq!(
            serde_json::to_value(params).unwrap(),
            serde_json::json!({ "description": null })
        );
        assert!(imbl_value::from_value::<SetDescriptionParams>(imbl_value::json!({})).is_err());
        assert!(
            imbl_value::from_value::<SetDescriptionParams>(imbl_value::json!({ "description": 1 }))
                .is_err()
        );
        assert!(
            imbl_value::from_value::<SetNameParams>(imbl_value::json!({ "name": null })).is_err()
        );
    }
}
