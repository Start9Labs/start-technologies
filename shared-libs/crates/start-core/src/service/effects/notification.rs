use crate::notifications::{NotificationLevel, notify};
use crate::service::effects::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CreateNotificationParams {
    #[arg(help = "help.arg.notification-level")]
    pub level: NotificationLevel,
    #[arg(help = "help.arg.notification-title")]
    pub title: String,
    #[arg(help = "help.arg.notification-message")]
    pub message: String,
    /// Optional long-form, markdown-formatted body that the UI renders in a
    /// "View Details" modal when present (release notes, post-update
    /// changelogs, structured error reports). When omitted, the notification
    /// has no extra payload.
    #[arg(long, help = "help.arg.notification-data")]
    #[serde(default)]
    pub data: Option<String>,
}

rpc_toolkit::reflect_ts!(CreateNotificationParams);
rpc_toolkit::ts_export!(CreateNotificationParams, namespaces = [""]);

pub async fn create(
    context: EffectContext,
    CreateNotificationParams {
        level,
        title,
        message,
        data,
    }: CreateNotificationParams,
) -> Result<(), Error> {
    let context = context.deref()?;
    let package_id = context.seed.id.clone();

    context
        .seed
        .ctx
        .db
        .mutate(move |db| match data {
            None => notify(db, Some(package_id), level, title, message, ()),
            Some(data) => notify(db, Some(package_id), level, title, message, data),
        })
        .await
        .result?;
    Ok(())
}
