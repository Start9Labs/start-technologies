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
    #[arg(long, action = clap::ArgAction::Set, help = "help.arg.bittorrent-incoming")]
    incoming: Option<bool>,
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
        let preferences = preferences.update(params.downloads, params.seeding, params.incoming);
        stored.ser(&preferences)?;
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
                "bittorrent": to_value(&TorrentPreferences::default())?
            }}}),
        )
        .await?;
        let enabled = update_preferences(
            &db,
            SetPreferencesParams {
                seeding: Some(true),
                incoming: Some(true),
                ..Default::default()
            },
        )
        .await?;
        assert_eq!(
            enabled,
            TorrentPreferences {
                downloads: true,
                seeding: true,
                incoming: true
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
                seeding: false,
                incoming: false
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

    #[test]
    fn preference_flags_require_explicit_booleans() {
        for flag in ["--downloads", "--seeding", "--incoming"] {
            assert!(SetPreferencesParams::try_parse_from(["set-preferences", flag]).is_err());
            assert!(
                SetPreferencesParams::try_parse_from(["set-preferences", flag, "false"]).is_ok()
            );
        }
    }
}
