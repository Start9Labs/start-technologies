use std::collections::{BTreeMap, BTreeSet};

use clap::Parser;
use imbl_value::InternedString;
use itertools::Itertools;
use rpc_toolkit::{Context, Empty, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::context::CliContext;
use crate::prelude::*;
use crate::registry::FullIndex;
use crate::registry::context::RegistryContext;
use crate::registry::package::index::Category;
use crate::rpc_continuations::Guid;
use crate::s9pk::manifest::LocaleString;
use crate::sign::AnyVerifyingKey;
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
            "set-trust-signers",
            from_fn_async(set_trust_signers)
                .with_metadata("admin", Value::Bool(true))
                .no_display()
                .with_about("about.set-registry-trust-signers")
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
    /// Proposed Marketplace signing keys and fallback labels, not an assertion of trust.
    #[serde(default)]
    pub trust_keys: BTreeMap<AnyVerifyingKey, String>,
}

rpc_toolkit::reflect_ts!(RegistryInfo);
rpc_toolkit::ts_export!(RegistryInfo, namespaces = [""]);

pub async fn get_info(ctx: RegistryContext) -> Result<RegistryInfo, Error> {
    let index: FullIndex = ctx.db.peek().await.into_index().de()?;
    let trust_keys = proposed_keys(&index);
    Ok(RegistryInfo {
        name: index.name,
        icon: index.icon,
        description: index.description,
        categories: index.package.categories,
        trust_keys,
    })
}

fn proposed_keys(index: &FullIndex) -> BTreeMap<AnyVerifyingKey, String> {
    index
        .trust_signers
        .iter()
        .filter_map(|id| index.signers.get(id))
        .flat_map(|signer| {
            signer
                .keys
                .iter()
                .map(|key| (key.clone(), signer.name.clone()))
        })
        .collect()
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct SetTrustSignersParams {
    #[arg(value_name = "SIGNER", help = "help.arg.registry-trust-signers", num_args = 0..)]
    pub signers: Vec<Guid>,
}

rpc_toolkit::reflect_ts!(SetTrustSignersParams);

async fn set_trust_signers(
    ctx: RegistryContext,
    params: SetTrustSignersParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_index_mut()
                .mutate(|index| select_trust_signers(index, params.signers.into_iter().collect()))
        })
        .await
        .result
}

fn select_trust_signers(index: &mut FullIndex, signers: BTreeSet<Guid>) -> Result<(), Error> {
    if signers.iter().any(|id| !index.signers.contains_key(id)) {
        return Err(Error::new(
            eyre!("{}", t!("registry.admin.unknown-signer")),
            ErrorKind::InvalidRequest,
        ));
    }
    index.trust_signers = signers;
    Ok(())
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
    fn registry_proposals_reject_malformed_keys_before_confirmation() {
        let mut value = serde_json::to_value(RegistryInfo::default()).unwrap();
        value["trustKeys"] = serde_json::json!({ "invalid key": "Publisher" });
        assert!(serde_json::from_value::<RegistryInfo>(value).is_err());
    }

    #[test]
    fn cli_trust_signers_accepts_a_selection_or_no_signers_to_clear() {
        let signer = Guid::new();
        let params =
            SetTrustSignersParams::try_parse_from(["set-trust-signers", signer.as_ref()]).unwrap();
        assert_eq!(params.signers, vec![signer]);
        assert!(
            SetTrustSignersParams::try_parse_from(["set-trust-signers"])
                .unwrap()
                .signers
                .is_empty()
        );
        assert!(SetTrustSignersParams::try_parse_from(["set-trust-signers", "invalid"]).is_err());
    }

    #[test]
    fn registry_proposes_only_the_selected_existing_signers() {
        use crate::registry::signer::SignerInfo;
        let first = Guid::new();
        let second = Guid::new();
        let mut index = FullIndex::default();
        let keys = crate::trusted_keys::initial_trusted_keys()[crate::trusted_keys::START9]
            .public_keys
            .clone();
        let first_key = keys.first().unwrap().clone();
        let second_key = keys.last().unwrap().clone();
        for (id, name, key) in [
            (first.clone(), "First", first_key.clone()),
            (second.clone(), "Second", second_key),
        ] {
            index.signers.insert(
                id,
                SignerInfo {
                    name: name.into(),
                    contact: vec![],
                    keys: BTreeSet::from([key]),
                },
            );
        }
        assert!(proposed_keys(&index).is_empty());
        select_trust_signers(&mut index, BTreeSet::from([first.clone()])).unwrap();
        assert_eq!(
            proposed_keys(&index),
            BTreeMap::from([(first_key, "First".into())])
        );
        assert!(select_trust_signers(&mut index, BTreeSet::from([Guid::new()])).is_err());
        assert_eq!(index.trust_signers, BTreeSet::from([first]));
        select_trust_signers(&mut index, BTreeSet::new()).unwrap();
        assert!(proposed_keys(&index).is_empty());
        let mut old_index = serde_json::to_value(&index).unwrap();
        old_index.as_object_mut().unwrap().remove("trustSigners");
        let restored: FullIndex = serde_json::from_value(old_index).unwrap();
        assert!(proposed_keys(&restored).is_empty());
    }

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
        use rpc_toolkit::ts::TSVisitor;

        let mut visitor = TSVisitor::new();
        visitor.append_type::<SetIconParams>();
        visitor.append_type::<SetDescriptionParams>();
        let schemas = visitor.into_bindings(None, "./schemas").unwrap().schemas;
        for (field, target) in [("icon", "DataUrl"), ("description", "LocaleString")] {
            assert!(
                schemas.contains(&format!("\"{field}\": {target}.nullable() }}")),
                "{schemas}"
            );
            assert!(
                !schemas.contains(&format!("\"{field}\": {target}.nullable().optional()")),
                "{schemas}"
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
