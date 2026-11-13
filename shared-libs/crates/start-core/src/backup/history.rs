use chrono::{DateTime, Utc};
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use super::BackupReport;
use super::target::BackupTargetId;
use crate::context::RpcContext;
use crate::db::model::DatabaseModel;
use crate::prelude::*;
use crate::rpc_continuations::Guid;

/// A completed background backup with its original target name and final report.
#[derive(Debug, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct BackupRun {
    pub id: String,
    pub target_id: BackupTargetId,
    pub target_name: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub report: BackupReport,
}

rpc_toolkit::reflect_ts!(BackupRun);
rpc_toolkit::ts_export!(BackupRun, namespaces = [""]);

impl BackupRun {
    pub(super) fn finished(
        target_id: BackupTargetId,
        target_name: String,
        started_at: DateTime<Utc>,
        report: BackupReport,
    ) -> Self {
        Self {
            id: Guid::new().to_string(),
            target_id,
            target_name,
            started_at,
            completed_at: Utc::now(),
            report,
        }
    }
}

pub(super) fn record(db: &mut DatabaseModel, run: &BackupRun) -> Result<(), Error> {
    db.as_private_mut()
        .as_backup_history_mut()
        .insert(&run.id, run)?;
    Ok(())
}

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct HistoryParams {
    pub offset: usize,
    pub limit: usize,
}
rpc_toolkit::reflect_ts!(HistoryParams);

#[derive(Debug, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct BackupHistory {
    pub runs: Vec<BackupRun>,
    pub total: usize,
}
rpc_toolkit::reflect_ts!(BackupHistory);

pub fn handler<C: Context>() -> ParentHandler<C> {
    ParentHandler::new().subcommand("list", from_fn_async(list).no_cli())
}

pub async fn list(ctx: RpcContext, params: HistoryParams) -> Result<BackupHistory, Error> {
    page(&ctx.db.peek().await, params)
}

fn page(
    db: &DatabaseModel,
    HistoryParams { offset, limit }: HistoryParams,
) -> Result<BackupHistory, Error> {
    let mut runs: Vec<_> = db
        .as_private()
        .as_backup_history()
        .de()?
        .into_values()
        .collect();
    runs.sort_by(|a, b| {
        b.started_at
            .cmp(&a.started_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    let total = runs.len();
    Ok(BackupHistory {
        runs: runs.into_iter().skip(offset).take(limit).collect(),
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_reports_page_newest_first_and_keep_the_original_target_name() {
        let mut db = DatabaseModel::from(
            to_value(&serde_json::json!({"private": {"backupHistory": {}}})).unwrap(),
        );
        for (started, name, error) in [
            ("2026-11-01T01:00:00Z", "Original", None),
            ("2026-11-02T01:00:00Z", "Renamed", Some("disk disconnected")),
        ] {
            let report = serde_json::from_value(serde_json::json!({"server": {"attempted": true, "error": error}, "packages": {}, "sharedVolumes": {}})).unwrap();
            let run = BackupRun::finished(
                "cifs-7".parse().unwrap(),
                name.into(),
                started.parse().unwrap(),
                report,
            );
            record(&mut db, &run).unwrap();
        }
        let db = DatabaseModel::from(to_value(&db).unwrap());
        let first = page(
            &db,
            HistoryParams {
                offset: 0,
                limit: 1,
            },
        )
        .unwrap();
        assert_eq!(first.total, 2);
        assert_eq!(first.runs[0].target_name, "Renamed");
        assert_eq!(
            first.runs[0].report.server.error.as_deref(),
            Some("disk disconnected")
        );
        let second = page(
            &db,
            HistoryParams {
                offset: 1,
                limit: 1,
            },
        )
        .unwrap();
        assert_eq!(second.total, 2);
        assert_eq!(second.runs[0].target_name, "Original");
        assert!(
            page(
                &db,
                HistoryParams {
                    offset: usize::MAX,
                    limit: usize::MAX
                }
            )
            .unwrap()
            .runs
            .is_empty()
        );
        assert!(
            page(
                &db,
                HistoryParams {
                    offset: 0,
                    limit: 0
                }
            )
            .unwrap()
            .runs
            .is_empty()
        );
    }
}
