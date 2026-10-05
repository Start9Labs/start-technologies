use std::str::FromStr;

use chrono::Utc;
use clap::builder::ValueParserFactory;

use crate::PackageId;
use crate::service::RebuildParams;
use crate::service::effects::prelude::*;
use crate::service::rpc::CallbackId;
use crate::status::StatusInfo;
use crate::util::FromStrParser;

pub async fn rebuild(context: EffectContext) -> Result<(), Error> {
    let seed = context.deref()?.seed.clone();
    let ctx = seed.ctx.clone();
    let id = seed.id.clone();
    drop(seed);
    tokio::spawn(async move {
        super::super::rebuild(ctx, RebuildParams { id })
            .await
            .log_err()
    });
    Ok(())
}

pub async fn restart(context: EffectContext) -> Result<(), Error> {
    let context = context.deref()?;
    let id = &context.seed.id;
    context
        .seed
        .ctx
        .db
        .mutate(|db| {
            db.as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(id)
                .or_not_found(id)?
                .as_status_info_mut()
                .restart()
        })
        .await
        .result?;
    Ok(())
}

pub async fn shutdown(context: EffectContext) -> Result<(), Error> {
    let context = context.deref()?;
    let id = &context.seed.id;
    context
        .seed
        .ctx
        .db
        .mutate(|db| {
            db.as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(id)
                .or_not_found(id)?
                .as_status_info_mut()
                .stop()
        })
        .await
        .result?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GetStatusParams {
    #[ts(optional)]
    pub package_id: Option<PackageId>,
    #[ts(optional)]
    #[arg(skip)]
    pub callback: Option<CallbackId>,
}

pub async fn get_status(
    context: EffectContext,
    GetStatusParams {
        package_id,
        callback,
    }: GetStatusParams,
) -> Result<Option<StatusInfo>, Error> {
    let context = context.deref()?;
    let id = package_id.unwrap_or_else(|| context.seed.id.clone());

    let ptr = format!("/public/packageData/{}/statusInfo", id)
        .parse()
        .expect("valid json pointer");
    let mut watch = context.seed.ctx.db.watch(ptr).await.typed::<StatusInfo>();

    let status = watch.peek_and_mark_seen()?.de().ok();

    if let Some(callback) = callback {
        let callback = callback.register(&context.seed.persistent_container);
        context.seed.ctx.callbacks.add_get_status(
            id.clone(),
            watch,
            super::callbacks::CallbackHandler::new(&context, callback),
        );
    }

    Ok(status)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SetMainStatusStatus {
    Running,
    Stopped,
}
impl FromStr for SetMainStatusStatus {
    type Err = color_eyre::eyre::Report;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            _ => Err(eyre!("unknown status {s}")),
        }
    }
}
impl ValueParserFactory for SetMainStatusStatus {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        FromStrParser::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SetMainStatus {
    status: SetMainStatusStatus,
}
pub async fn set_main_status(
    context: EffectContext,
    SetMainStatus { status }: SetMainStatus,
) -> Result<(), Error> {
    let context = context.deref()?;
    let id = &context.seed.id;
    context
        .seed
        .ctx
        .db
        .mutate(|db| {
            let s = db
                .as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(id)
                .or_not_found(id)?
                .as_status_info_mut();
            let prev = s.as_started_mut().replace(&match status {
                SetMainStatusStatus::Running => Some(Utc::now()),
                SetMainStatusStatus::Stopped => None,
            })?;
            if prev.is_none() && status == SetMainStatusStatus::Running {
                s.as_desired_mut().map_mutate(|s| Ok(s.started()))?;
            }

            Ok(())
        })
        .await
        .result?;
    Ok(())
}
