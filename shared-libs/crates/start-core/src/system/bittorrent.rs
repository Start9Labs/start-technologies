use clap::Parser;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::context::{CliContext, RpcContext};
use crate::db::model::Database;
use crate::net::service_interface::ServiceInterface;
use crate::prelude::*;
use crate::registry::bittorrent::TorrentPreferences;
use crate::util::serde::HandlerExtSerde;

mod incoming;
pub(crate) use incoming::configure_incoming;

pub(crate) fn api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "config",
            from_fn(config)
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
}

/// Configured transport availability and the native incoming peer interface.
#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
struct BitTorrentConfig {
    available: bool,
    interface: ServiceInterface,
}

rpc_toolkit::reflect_ts!(BitTorrentConfig);
rpc_toolkit::ts_export!(BitTorrentConfig, namespaces = [""]);

fn config(ctx: RpcContext) -> Result<BitTorrentConfig, Error> {
    Ok(BitTorrentConfig {
        available: ctx.torrents.available(),
        interface: incoming::interface(),
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preferences_cascade_atomically_in_the_authoritative_database() -> Result<(), Error> {
        let db = TypedPatchDb::<Database>::load_unchecked(PatchDb::in_memory());
        db.put(
            &patch_db::json_ptr::ROOT,
            &imbl_value::json!({"public": {"serverInfo": {
                "bittorrent": to_value(&TorrentPreferences::default())?,
                "network": {"host": {"bindings": {}}}
            }}}),
        )
        .await?;
        let enabled = update_preferences(
            &db,
            SetPreferencesParams {
                seeding: Some(true),
                ..Default::default()
            },
        )
        .await?;
        assert_eq!(
            enabled,
            TorrentPreferences {
                downloads: true,
                seeding: true
            }
        );
        let disabled = update_preferences(
            &db,
            SetPreferencesParams {
                downloads: Some(false),
                ..Default::default()
            },
        )
        .await?;
        assert_eq!(
            disabled,
            TorrentPreferences {
                downloads: false,
                seeding: false
            }
        );
        assert_eq!(
            db.peek()
                .await
                .as_public()
                .as_server_info()
                .as_bittorrent()
                .de()?,
            disabled
        );
        assert_eq!(
            update_preferences(
                &db,
                SetPreferencesParams {
                    downloads: Some(true),
                    ..Default::default()
                },
            )
            .await?,
            TorrentPreferences::default()
        );
        Ok(())
    }

    #[tokio::test]
    async fn address_rows_are_the_ingress_opt_in_and_either_parent_clears_them() -> Result<(), Error>
    {
        use std::sync::Arc;

        use crate::db::model::public::{GatewayType, IpInfo, NetworkInterfaceInfo};
        use crate::net::forward::AvailablePorts;
        use crate::net::host::Host;

        for disable_downloads in [false, true] {
            let gateway = NetworkInterfaceInfo {
                gateway_type: GatewayType::InboundOutbound,
                ip_info: Some(Arc::new(IpInfo {
                    subnets: ["192.168.1.2/24".parse::<ipnet::IpNet>().unwrap()]
                        .into_iter()
                        .collect(),
                    wan_ip: Some("203.0.113.45".parse().unwrap()),
                    ..Default::default()
                })),
                ..Default::default()
            };
            let db = TypedPatchDb::<Database>::load_unchecked(PatchDb::in_memory());
            db.put(&patch_db::json_ptr::ROOT, &imbl_value::json!({
                "public": {"serverInfo": {
                    "hostname": "server",
                    "bittorrent": to_value(&TorrentPreferences::default())?,
                    "network": {"host": to_value(&Host::default())?, "gateways": {"eth0": to_value(&gateway)?}}
                }},
                "private": {"availablePorts": to_value(&AvailablePorts::new())?}
            })).await?;
            assert!(
                db.mutate(|db| incoming::configure_in(db, true))
                    .await
                    .result?
                    .is_none()
            );
            let binding = db
                .peek()
                .await
                .as_public()
                .as_server_info()
                .as_network()
                .as_host()
                .as_bindings()
                .as_idx(&6881)
                .unwrap()
                .de()?;
            assert!(!binding.enabled);
            assert!(binding.addresses.enabled().is_empty());
            assert_eq!(binding.addresses.available.len(), 1);
            let address = binding.addresses.available.first().unwrap().clone();
            assert_eq!(address.hostname, InternedString::intern("203.0.113.45"));
            assert!(address.public);
            update_preferences(
                &db,
                SetPreferencesParams {
                    seeding: Some(true),
                    ..Default::default()
                },
            )
            .await?;
            assert!(
                db.mutate(|db| incoming::configure_in(db, true))
                    .await
                    .result?
                    .is_none()
            );
            db.mutate(|db| {
                db.as_public_mut()
                    .as_server_info_mut()
                    .as_network_mut()
                    .as_host_mut()
                    .as_bindings_mut()
                    .as_idx_mut(&6881)
                    .unwrap()
                    .mutate(|binding| binding.set_address_enabled(&address, true))
            })
            .await
            .result?;
            assert!(
                db.mutate(|db| incoming::configure_in(db, true))
                    .await
                    .result?
                    .is_some()
            );
            update_preferences(
                &db,
                SetPreferencesParams {
                    downloads: disable_downloads.then_some(false),
                    seeding: (!disable_downloads).then_some(false),
                },
            )
            .await?;
            let binding = db
                .peek()
                .await
                .as_public()
                .as_server_info()
                .as_network()
                .as_host()
                .as_bindings()
                .as_idx(&6881)
                .unwrap()
                .de()?;
            assert!(!binding.enabled);
            assert!(binding.addresses.enabled().is_empty());
            assert!(
                db.mutate(|db| incoming::configure_in(db, true))
                    .await
                    .result?
                    .is_none()
            );
            update_preferences(
                &db,
                SetPreferencesParams {
                    downloads: Some(true),
                    seeding: Some(true),
                },
            )
            .await?;
            assert!(
                db.mutate(|db| incoming::configure_in(db, true))
                    .await
                    .result?
                    .is_none()
            );
            assert!(
                db.peek()
                    .await
                    .as_public()
                    .as_server_info()
                    .as_network()
                    .as_host()
                    .as_bindings()
                    .as_idx(&6881)
                    .unwrap()
                    .de()?
                    .addresses
                    .enabled()
                    .is_empty()
            );
        }
        Ok(())
    }

    #[test]
    fn preference_flags_require_explicit_booleans() {
        for flag in ["--downloads", "--seeding"] {
            assert!(SetPreferencesParams::try_parse_from(["set-preferences", flag]).is_err());
            assert!(
                SetPreferencesParams::try_parse_from(["set-preferences", flag, "false"]).is_ok()
            );
        }
    }
}
