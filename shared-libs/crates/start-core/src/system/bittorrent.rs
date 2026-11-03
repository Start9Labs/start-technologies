use clap::Parser;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::context::{CliContext, RpcContext};
use crate::db::model::Database;
use crate::prelude::*;
use crate::util::serde::HandlerExtSerde;

pub(crate) fn api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "enabled",
            from_fn(enabled)
                .with_display_serializable()
                .with_about("about.system.bittorrent-enabled")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-seeding",
            from_fn_async(set_seeding)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.system.set-seeding")
                .with_call_remote::<CliContext>(),
        )
}

fn enabled(ctx: RpcContext) -> Result<bool, Error> {
    Ok(ctx.torrents.downloads_enabled())
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
struct SetSeedingParams {
    #[arg(long, required = true, action = clap::ArgAction::Set, help = "help.arg.seeding-enabled")]
    enabled: bool,
}

rpc_toolkit::reflect_ts!(SetSeedingParams);

async fn set_seeding(ctx: RpcContext, params: SetSeedingParams) -> Result<(), Error> {
    set_preference(&ctx.db, params.enabled).await
}

async fn set_preference(db: &TypedPatchDb<Database>, enabled: bool) -> Result<(), Error> {
    db.mutate(|db| {
        db.as_public_mut()
            .as_server_info_mut()
            .as_registry_seeding_mut()
            .ser(&enabled)?;
        Ok(())
    })
    .await
    .result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preference_updates_the_authoritative_database_field() -> Result<(), Error> {
        let db = TypedPatchDb::<Database>::load_unchecked(PatchDb::in_memory());
        for value in [true, false] {
            set_preference(&db, value).await?;
            assert_eq!(
                db.peek()
                    .await
                    .as_public()
                    .as_server_info()
                    .as_registry_seeding()
                    .de()?,
                value
            );
        }
        Ok(())
    }

    #[test]
    fn seeding_requests_require_an_explicit_boolean() {
        assert!(serde_json::from_value::<SetSeedingParams>(serde_json::json!({})).is_err());
        assert!(SetSeedingParams::try_parse_from(["set-seeding"]).is_err());
        for value in ["true", "false"] {
            let params =
                SetSeedingParams::try_parse_from(["set-seeding", "--enabled", value]).unwrap();
            assert_eq!(params.enabled, value == "true");
        }
    }
}
