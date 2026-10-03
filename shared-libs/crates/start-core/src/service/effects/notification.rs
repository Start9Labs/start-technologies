use crate::notifications::{NotificationLevel, notify};
use crate::service::effects::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CreateNotificationParams {
    pub level: NotificationLevel,
    pub title: String,
    pub message: String,
    /// Optional long-form, markdown-formatted body that the UI renders in a
    /// "View Details" modal when present (release notes, post-update
    /// changelogs, structured error reports). When omitted, the notification
    /// has no extra payload.
    #[serde(default)]
    #[ts(type = "string | null")]
    pub data: Option<String>,
}

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
