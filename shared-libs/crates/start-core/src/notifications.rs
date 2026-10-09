use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use clap::Parser;
use clap::builder::ValueParserFactory;
use color_eyre::eyre::eyre;
use imbl_value::InternedString;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::PackageId;
use crate::backup::BackupReport;
use crate::context::{CliContext, RpcContext};
use crate::db::model::DatabaseModel;
use crate::prelude::*;
use crate::util::FromStrParser;
use crate::util::serde::{HandlerExtSerde, const_true};

// #[command(subcommands(list, delete, delete_before, create))]
pub fn notification<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async(list)
                .with_display_serializable()
                .with_about("about.list-notifications")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove)
                .no_display()
                .with_about("about.remove-notification-for-ids")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove-before",
            from_fn_async(remove_before)
                .no_display()
                .with_about("about.remove-notifications-before-id")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "mark-seen",
            from_fn_async(mark_seen)
                .no_display()
                .with_about("about.mark-notifications-seen")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "mark-seen-before",
            from_fn_async(mark_seen_before)
                .no_display()
                .with_about("about.mark-notifications-seen-before-id")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "mark-unseen",
            from_fn_async(mark_unseen)
                .no_display()
                .with_about("about.mark-notifications-unseen")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "create",
            from_fn_async(create)
                .no_display()
                .with_about("about.persist-new-notification")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ListNotificationParams {
    #[arg(help = "help.arg.notification-before-id")]
    #[visit(ts(type = "number | null"), wire = "Option<rpc_toolkit::ts::Unknown>")]
    #[visit(opaque, type_attributes(visit::wire))]
    before: Option<u32>,
    #[arg(help = "help.arg.notification-limit")]
    #[visit(ts(type = "number | null"), wire = "Option<rpc_toolkit::ts::Unknown>")]
    #[visit(opaque, type_attributes(visit::wire))]
    limit: Option<usize>,
}

rpc_toolkit::reflect_ts!(ListNotificationParams);
rpc_toolkit::ts_export!(ListNotificationParams, namespaces = [""]);
// #[command(display(display_serializable))]
#[instrument(skip_all)]
pub async fn list(
    ctx: RpcContext,
    ListNotificationParams { before, limit }: ListNotificationParams,
) -> Result<Vec<NotificationWithId>, Error> {
    ctx.db
        .mutate(|db| {
            let limit = limit.unwrap_or(40);
            match before {
                None => {
                    let records = db
                        .as_private()
                        .as_notifications()
                        .as_entries()?
                        .into_iter()
                        .rev()
                        .take(limit);
                    let notifs = records
                        .into_iter()
                        .map(|(id, notification)| {
                            Ok(NotificationWithId {
                                id,
                                notification: notification.de()?,
                            })
                        })
                        .collect::<Result<Vec<NotificationWithId>, Error>>()?;
                    Ok(notifs)
                }
                Some(before) => {
                    let records = db
                        .as_private()
                        .as_notifications()
                        .as_entries()?
                        .into_iter()
                        .filter(|(id, _)| *id < before)
                        .rev()
                        .take(limit);
                    records
                        .into_iter()
                        .map(|(id, notification)| {
                            Ok(NotificationWithId {
                                id,
                                notification: notification.de()?,
                            })
                        })
                        .collect()
                }
            }
        })
        .await
        .result
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ModifyNotificationParams {
    #[arg(help = "help.arg.notification-ids")]
    #[visit(ts(type = "number[]"), wire = "rpc_toolkit::ts::Unknown")]
    #[visit(opaque, type_attributes(visit::wire))]
    ids: Vec<u32>,
}

rpc_toolkit::reflect_ts!(ModifyNotificationParams);
rpc_toolkit::ts_export!(ModifyNotificationParams, namespaces = [""]);

pub async fn remove(
    ctx: RpcContext,
    ModifyNotificationParams { ids }: ModifyNotificationParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            let n = db.as_private_mut().as_notifications_mut();
            for id in ids {
                n.remove(&id)?;
            }
            let mut unread = 0;
            for (_, n) in n.as_entries()? {
                if !n.as_seen().de()? {
                    unread += 1;
                }
            }
            db.as_public_mut()
                .as_server_info_mut()
                .as_unread_notification_count_mut()
                .ser(&unread)?;
            Ok(())
        })
        .await
        .result
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ModifyNotificationBeforeParams {
    #[arg(help = "help.arg.notification-before-id")]
    #[visit(ts(type = "number"), wire = "rpc_toolkit::ts::Unknown")]
    #[visit(opaque, type_attributes(visit::wire))]
    before: u32,
}

rpc_toolkit::reflect_ts!(ModifyNotificationBeforeParams);
rpc_toolkit::ts_export!(ModifyNotificationBeforeParams, namespaces = [""]);

pub async fn remove_before(
    ctx: RpcContext,
    ModifyNotificationBeforeParams { before }: ModifyNotificationBeforeParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            let n = db.as_private_mut().as_notifications_mut();
            for id in n.keys()?.range(..before) {
                n.remove(&id)?;
            }
            let mut unread = 0;
            for (_, n) in n.as_entries()? {
                if !n.as_seen().de()? {
                    unread += 1;
                }
            }
            db.as_public_mut()
                .as_server_info_mut()
                .as_unread_notification_count_mut()
                .ser(&unread)?;
            Ok(())
        })
        .await
        .result
}

pub async fn mark_seen(
    ctx: RpcContext,
    ModifyNotificationParams { ids }: ModifyNotificationParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            let n = db.as_private_mut().as_notifications_mut();
            for id in ids {
                n.as_idx_mut(&id)
                    .or_not_found(lazy_format!("Notification #{id}"))?
                    .as_seen_mut()
                    .ser(&true)?;
            }
            let mut unread = 0;
            for (_, n) in n.as_entries()? {
                if !n.as_seen().de()? {
                    unread += 1;
                }
            }
            db.as_public_mut()
                .as_server_info_mut()
                .as_unread_notification_count_mut()
                .ser(&unread)?;
            Ok(())
        })
        .await
        .result
}

pub async fn mark_seen_before(
    ctx: RpcContext,
    ModifyNotificationBeforeParams { before }: ModifyNotificationBeforeParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            let n = db.as_private_mut().as_notifications_mut();
            for id in n.keys()?.range(..=before) {
                n.as_idx_mut(&id)
                    .or_not_found(lazy_format!("Notification #{id}"))?
                    .as_seen_mut()
                    .ser(&true)?;
            }
            let mut unread = 0;
            for (_, n) in n.as_entries()? {
                if !n.as_seen().de()? {
                    unread += 1;
                }
            }
            db.as_public_mut()
                .as_server_info_mut()
                .as_unread_notification_count_mut()
                .ser(&unread)?;
            Ok(())
        })
        .await
        .result
}

pub async fn mark_unseen(
    ctx: RpcContext,
    ModifyNotificationParams { ids }: ModifyNotificationParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            let n = db.as_private_mut().as_notifications_mut();
            for id in ids {
                n.as_idx_mut(&id)
                    .or_not_found(lazy_format!("Notification #{id}"))?
                    .as_seen_mut()
                    .ser(&false)?;
            }
            let mut unread = 0;
            for (_, n) in n.as_entries()? {
                if !n.as_seen().de()? {
                    unread += 1;
                }
            }
            db.as_public_mut()
                .as_server_info_mut()
                .as_unread_notification_count_mut()
                .ser(&unread)?;
            Ok(())
        })
        .await
        .result
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CreateParams {
    #[arg(long, short, help = "help.arg.package-id")]
    package: Option<PackageId>,
    #[arg(help = "help.arg.notification-level")]
    level: NotificationLevel,
    #[arg(help = "help.arg.notification-title")]
    title: String,
    #[arg(help = "help.arg.notification-message")]
    message: String,
}

rpc_toolkit::reflect_ts!(CreateParams);

pub async fn create(
    ctx: RpcContext,
    CreateParams {
        package,
        level,
        title,
        message,
    }: CreateParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| notify(db, package, level, title, message, ()))
        .await
        .result
}

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, VisitVariants,
)]
#[serde(rename_all = "camelCase")]
pub enum NotificationLevel {
    Success,
    Info,
    Warning,
    Error,
}

rpc_toolkit::reflect_ts!(NotificationLevel);
rpc_toolkit::ts_export!(NotificationLevel, namespaces = [""]);
impl fmt::Display for NotificationLevel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NotificationLevel::Success => write!(f, "success"),
            NotificationLevel::Info => write!(f, "info"),
            NotificationLevel::Warning => write!(f, "warning"),
            NotificationLevel::Error => write!(f, "error"),
        }
    }
}
impl ValueParserFactory for NotificationLevel {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        FromStrParser::new()
    }
}

pub struct InvalidNotificationLevel(String);
impl From<InvalidNotificationLevel> for crate::Error {
    fn from(val: InvalidNotificationLevel) -> Self {
        Error::new(
            eyre!("{}", t!("notifications.invalid-level", level = val.0)),
            ErrorKind::ParseDbField,
        )
    }
}
impl FromStr for NotificationLevel {
    type Err = InvalidNotificationLevel;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            s if s == "success" => Ok(NotificationLevel::Success),
            s if s == "info" => Ok(NotificationLevel::Info),
            s if s == "warning" => Ok(NotificationLevel::Warning),
            s if s == "error" => Ok(NotificationLevel::Error),
            s => Err(InvalidNotificationLevel(s.to_string())),
        }
    }
}
impl fmt::Display for InvalidNotificationLevel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Invalid Notification Level: {}", self.0)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notifications(pub BTreeMap<u32, Notification>);
impl Notifications {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }
}
impl Map for Notifications {
    type Key = u32;
    type Value = Notification;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Self::key_string(key)
    }
    fn key_string(key: &Self::Key) -> Result<InternedString, Error> {
        Ok(InternedString::from_display(key))
    }
}

#[derive(Debug, Serialize, Deserialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct Notification {
    pub package_id: Option<PackageId>,
    #[visit(ts(type = "string"), wire = "rpc_toolkit::ts::Unknown")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub created_at: DateTime<Utc>,
    pub code: u32,
    pub level: NotificationLevel,
    pub title: String,
    pub message: String,
    pub data: Value,
    #[serde(default = "const_true")]
    pub seen: bool,
}

rpc_toolkit::reflect_ts!(Notification);
rpc_toolkit::ts_export!(Notification, namespaces = [""]);

#[derive(Debug, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct NotificationWithId {
    id: u32,
    #[serde(flatten)]
    notification: Notification,
}

rpc_toolkit::reflect_ts!(NotificationWithId);
rpc_toolkit::ts_export!(NotificationWithId, namespaces = [""]);

pub trait NotificationType:
    serde::Serialize + for<'de> serde::Deserialize<'de> + std::fmt::Debug
{
    const CODE: u32;
}

impl NotificationType for () {
    const CODE: u32 = 0;
}
impl NotificationType for BackupReport {
    const CODE: u32 = 1;
}
impl NotificationType for String {
    const CODE: u32 = 2;
}

#[instrument(skip(subtype, db))]
pub fn notify<T: NotificationType>(
    db: &mut DatabaseModel,
    package_id: Option<PackageId>,
    level: NotificationLevel,
    title: String,
    message: String,
    subtype: T,
) -> Result<(), Error> {
    let data = to_value(&subtype)?;
    db.as_public_mut()
        .as_server_info_mut()
        .as_unread_notification_count_mut()
        .mutate(|c| {
            *c += 1;
            Ok(())
        })?;
    let id = db
        .as_private()
        .as_notifications()
        .keys()?
        .into_iter()
        .max()
        .map_or(0, |id| id + 1);
    db.as_private_mut().as_notifications_mut().insert(
        &id,
        &Notification {
            package_id,
            created_at: Utc::now(),
            code: T::CODE,
            level,
            title,
            message,
            data,
            seen: false,
        },
    )?;
    Ok(())
}

#[test]
fn serialization() {
    println!(
        "{}",
        serde_json::json!({ "test": "abcdefg", "num": 32, "nested": { "inner": null, "xyz": [0,2,4]}})
    )
}
