use std::collections::BTreeSet;
use std::sync::Arc;

use crate::ActionId;
use crate::net::host::{all_hosts, host_for};
use crate::net::service_interface::{HostnameMetadata, PluginHostnameInfo};
use crate::service::Service;
use crate::service::effects::plugin::PluginId;
use crate::service::effects::prelude::*;

fn require_url_plugin(context: &Arc<Service>) -> Result<(), Error> {
    if !context
        .seed
        .persistent_container
        .s9pk
        .as_manifest()
        .metadata
        .plugins
        .contains(&PluginId::UrlV0)
    {
        return Err(Error::new(
            eyre!(
                "{}",
                t!("net.plugin.manifest-missing-plugin", plugin = "url-v0")
            ),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UrlPluginRegisterParams {
    pub table_action: ActionId,
}

pub async fn register(
    context: EffectContext,
    UrlPluginRegisterParams { table_action }: UrlPluginRegisterParams,
) -> Result<(), Error> {
    use crate::db::model::package::UrlPluginRegistration;

    let context = context.deref()?;
    require_url_plugin(&context)?;
    let plugin_id = context.seed.id.clone();

    context
        .seed
        .ctx
        .db
        .mutate(|db| {
            db.as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(&plugin_id)
                .or_not_found(&plugin_id)?
                .as_plugin_mut()
                .as_url_mut()
                .ser(&Some(UrlPluginRegistration { table_action }))?;
            Ok(())
        })
        .await
        .result?;

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UrlPluginExportUrlParams {
    pub hostname_info: PluginHostnameInfo,
    pub remove_action: Option<ActionId>,
    pub overflow_actions: Vec<ActionId>,
}

pub async fn export_url(
    context: EffectContext,
    UrlPluginExportUrlParams {
        hostname_info,
        remove_action,
        overflow_actions,
    }: UrlPluginExportUrlParams,
) -> Result<(), Error> {
    let context = context.deref()?;
    require_url_plugin(&context)?;
    let plugin_id = context.seed.id.clone();

    let entry = hostname_info.to_hostname_info(&plugin_id, remove_action, overflow_actions);

    context
        .seed
        .ctx
        .db
        .mutate(|db| {
            let host = host_for(db, &hostname_info.package_id, &hostname_info.host_id)?;
            host.as_bindings_mut()
                .as_idx_mut(&hostname_info.internal_port)
                .or_not_found(t!(
                    "net.plugin.binding-not-found",
                    binding = format!(
                        "{}:{}:{}",
                        hostname_info.package_id,
                        hostname_info.host_id,
                        hostname_info.internal_port
                    )
                ))?
                .as_addresses_mut()
                .as_available_mut()
                .mutate(|available: &mut BTreeSet<_>| {
                    available.insert(entry);
                    Ok(())
                })?;
            Ok(())
        })
        .await
        .result?;

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UrlPluginClearUrlsParams {
    pub except: BTreeSet<PluginHostnameInfo>,
}

pub async fn clear_urls(
    context: EffectContext,
    UrlPluginClearUrlsParams { except }: UrlPluginClearUrlsParams,
) -> Result<(), Error> {
    let context = context.deref()?;
    require_url_plugin(&context)?;
    let plugin_id = context.seed.id.clone();

    context
        .seed
        .ctx
        .db
        .mutate(|db| {
            for host in all_hosts(db) {
                let host = host?;
                for (_, bind) in host.as_bindings_mut().as_entries_mut()? {
                    bind.as_addresses_mut().as_available_mut().mutate(
                        |available: &mut BTreeSet<_>| {
                            available.retain(|h| {
                                match &h.metadata {
                                    HostnameMetadata::Plugin { package_id, .. }
                                        if package_id == &plugin_id =>
                                    {
                                        // Keep if it matches any entry in the except list
                                        except
                                            .iter()
                                            .any(|e| e.matches_hostname_info(h, &plugin_id))
                                    }
                                    _ => true,
                                }
                            });
                            Ok(())
                        },
                    )?;
                }
            }
            Ok(())
        })
        .await
        .result?;

    Ok(())
}
