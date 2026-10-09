use clap::Parser;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use ts_rs::TS;

use crate::context::RpcContext;
use crate::prelude::*;
use crate::{Error, PackageId};

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ControlParams {
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct StartParams {
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
    #[arg(long, help = "help.arg.force-start")]
    #[serde(default)]
    pub force: bool,
}

#[instrument(skip_all)]
pub async fn start(ctx: RpcContext, StartParams { id, force }: StartParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            let entry = db
                .as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(&id)
                .or_not_found(&id)?;
            if !force && entry.has_blocking_task(&id)? {
                return Err(Error::new(
                    eyre!("{}", t!("control.start-critical-task", id = id)),
                    ErrorKind::InvalidRequest,
                ));
            }
            entry.as_status_info_mut().start()
        })
        .await
        .result?;

    Ok(())
}

pub async fn stop(ctx: RpcContext, ControlParams { id }: ControlParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(&id)
                .or_not_found(&id)?
                .as_status_info_mut()
                .stop()
        })
        .await
        .result?;

    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ForceStopParams {
    pub id: PackageId,
    #[ts(type = "string")]
    pub force_stop_at: chrono::DateTime<chrono::Utc>,
}

pub async fn force_stop(
    ctx: RpcContext,
    ForceStopParams { id, force_stop_at }: ForceStopParams,
) -> Result<(), Error> {
    ctx.services.force_stop(&ctx, &id, force_stop_at).await
}

pub async fn restart(ctx: RpcContext, ControlParams { id }: ControlParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(&id)
                .or_not_found(&id)?
                .as_status_info_mut()
                .restart()
        })
        .await
        .result?;

    Ok(())
}
