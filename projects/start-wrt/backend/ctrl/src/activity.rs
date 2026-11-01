use std::ops::Deref;
use std::sync::{Mutex, MutexGuard};

use clap::Parser;
use rpc_toolkit::{from_fn_async, HandlerExt as _, ParentHandler};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::prelude::*;
use crate::utils::HandlerExtSerde;
use crate::{CliContext, ServerContext};

pub const ACTIVITY_DB: &str = "/var/lib/startwrt/activity.db";
pub(crate) const MAX_ENTRIES: usize = 500;

const ACTIVITY_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS activity (
    id INTEGER PRIMARY KEY,
    timestamp TEXT NOT NULL,
    category TEXT NOT NULL,
    action TEXT NOT NULL,
    success INTEGER NOT NULL,
    summary TEXT NOT NULL,
    error TEXT
);";

static DB: Mutex<Option<Connection>> = Mutex::new(None);

struct Db(MutexGuard<'static, Option<Connection>>);

impl Deref for Db {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        self.0.as_ref().expect("opened by db()")
    }
}

fn db() -> Result<Db, Error> {
    #[cfg(test)]
    let path = ":memory:";
    #[cfg(not(test))]
    let path = ACTIVITY_DB;
    let mut slot = DB
        .lock()
        .map_err(|e| Error::new(eyre!("lock poisoned: {e}"), ErrorKind::Incoherent))?;
    ensure_open(&mut slot, path)?;
    Ok(Db(slot))
}

fn ensure_open(slot: &mut Option<Connection>, path: &str) -> Result<(), Error> {
    if slot.is_none() {
        *slot = Some(open_db(path).map_err(|e| {
            Error::new(
                eyre!("failed to open activity database: {e}"),
                ErrorKind::Filesystem,
            )
        })?);
    }
    Ok(())
}

fn open_db(path: &str) -> Result<Connection, Box<dyn std::error::Error>> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(ACTIVITY_SCHEMA)?;
    Ok(conn)
}

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
pub struct ActivityEntry {
    pub id: i64,
    pub timestamp: String,
    pub category: String,
    pub action: String,
    pub success: bool,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
rpc_toolkit::reflect_ts!(ActivityEntry);

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct ActivityListResponse {
    pub entries: Vec<ActivityEntry>,
    pub total: usize,
}
rpc_toolkit::reflect_ts!(ActivityListResponse);

#[derive(Debug, Deserialize, Serialize, Default, Parser, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ActivityListParams {
    #[serde(default)]
    #[clap(long)]
    pub offset: Option<usize>,
    #[serde(default)]
    #[clap(long)]
    pub limit: Option<usize>,
}
rpc_toolkit::reflect_ts!(ActivityListParams);

#[derive(Debug, Deserialize, Serialize, Parser, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ActivityDeleteParams {
    #[clap(long)]
    pub id: i64,
}
rpc_toolkit::reflect_ts!(ActivityDeleteParams);

/// Log an activity entry. Best-effort — never propagates errors.
pub fn log(category: &str, action: &str, success: bool, summary: &str, error: Option<&str>) {
    let _ = log_inner(category, action, success, summary, error);
}

fn log_inner(
    category: &str,
    action: &str,
    success: bool,
    summary: &str,
    error: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Emit to syslog via tracing.
    // The startwrt-activity prefix makes these entries easy to find: `logread | grep startwrt-activity`
    let status = if success { "OK" } else { "FAIL" };
    let err_suffix = error.map(|e| format!(": {e}")).unwrap_or_default();
    tracing::info!(target: "activity", "startwrt-activity [{category}.{action}] {status} {summary}{err_suffix}");

    // Insert into SQLite
    let conn = db().map_err(|e| e.to_string())?;
    insert_entry(&conn, category, action, success, summary, error)?;
    Ok(())
}

/// Insert one entry into an open connection and enforce the entry cap. Shared
/// by the global logger and `log_to` (which targets an arbitrary DB file).
fn insert_entry(
    conn: &Connection,
    category: &str,
    action: &str,
    success: bool,
    summary: &str,
    error: Option<&str>,
) -> rusqlite::Result<()> {
    let timestamp = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO activity (timestamp, category, action, success, summary, error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![timestamp, category, action, success, summary, error],
    )?;

    // Enforce cap
    conn.execute(
        "DELETE FROM activity WHERE id <= (
            SELECT id FROM activity ORDER BY id DESC LIMIT 1 OFFSET ?1
        )",
        [MAX_ENTRIES],
    )?;

    Ok(())
}

/// Records an entry in the database at an arbitrary path, creating it as
/// needed. Best-effort.
pub fn log_to(
    db_path: &str,
    category: &str,
    action: &str,
    success: bool,
    summary: &str,
    error: Option<&str>,
) {
    let _ = log_to_inner(db_path, category, action, success, summary, error);
}

fn log_to_inner(
    db_path: &str,
    category: &str,
    action: &str,
    success: bool,
    summary: &str,
    error: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = if success { "OK" } else { "FAIL" };
    let err_suffix = error.map(|e| format!(": {e}")).unwrap_or_default();
    tracing::info!(target: "activity", "startwrt-activity(eMMC) [{category}.{action}] {status} {summary}{err_suffix}");

    let conn = open_db(db_path)?;
    insert_entry(&conn, category, action, success, summary, error)?;
    Ok(())
}

/// Convenience: log success or failure from a Result, return it unchanged.
pub fn log_result<T, E: std::fmt::Display>(
    category: &str,
    action: &str,
    subject: &str,
    result: Result<T, E>,
) -> Result<T, E> {
    match &result {
        Ok(_) => log(
            category,
            action,
            true,
            &capitalize_action_summary(action, subject),
            None,
        ),
        Err(e) => log(
            category,
            action,
            false,
            &format!("Failed to {} {}", action.replace('-', " "), subject),
            Some(&e.to_string()),
        ),
    }
    result
}

fn capitalize_action_summary(action: &str, subject: &str) -> String {
    let verb = match action {
        "created" => "Created",
        "updated" => "Updated",
        "deleted" => "Deleted",
        "configured" => "Configured",
        "enabled" => "Enabled",
        "disabled" => "Disabled",
        "blocked" => "Blocked",
        "unblocked" => "Unblocked",
        "forgotten" => "Forgot",
        "added" => "Added",
        "peer-added" => "Added",
        "peer-deleted" => "Deleted",
        "downloaded" => "Downloaded",
        "restored" => "Restored",
        "recovered" => "Recovered",
        _ => action,
    };
    format!("{verb} {subject}")
}

// ── RPC Handlers ────────────────────────────────────────────────

#[instrument(skip_all)]
pub(crate) async fn list(
    _ctx: ServerContext,
    params: ActivityListParams,
) -> Result<ActivityListResponse, Error> {
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(50);

    let conn = db()?;

    let total: usize = conn
        .query_row("SELECT COUNT(*) FROM activity", [], |row| row.get(0))
        .map_err(|e| {
            Error::new(
                eyre!("failed to count activity: {e}"),
                ErrorKind::Filesystem,
            )
        })?;

    let mut stmt = conn
        .prepare(
            "SELECT id, timestamp, category, action, success, summary, error
             FROM activity ORDER BY id DESC LIMIT ?1 OFFSET ?2",
        )
        .map_err(|e| Error::new(eyre!("failed to prepare query: {e}"), ErrorKind::Filesystem))?;

    let entries = stmt
        .query_map(rusqlite::params![limit, offset], |row| {
            Ok(ActivityEntry {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                category: row.get(2)?,
                action: row.get(3)?,
                success: row.get::<_, i32>(4)? != 0,
                summary: row.get(5)?,
                error: row.get(6)?,
            })
        })
        .map_err(|e| {
            Error::new(
                eyre!("failed to query activity: {e}"),
                ErrorKind::Filesystem,
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            Error::new(
                eyre!("failed to read activity row: {e}"),
                ErrorKind::Filesystem,
            )
        })?;

    Ok(ActivityListResponse { entries, total })
}

#[instrument(skip_all)]
async fn delete(
    _ctx: ServerContext,
    ActivityDeleteParams { id }: ActivityDeleteParams,
) -> Result<(), Error> {
    let conn = db()?;
    conn.execute("DELETE FROM activity WHERE id = ?1", [id])
        .map_err(|e| {
            Error::new(
                eyre!("failed to delete activity: {e}"),
                ErrorKind::Filesystem,
            )
        })?;
    if conn.changes() == 0 {
        return Err(Error::new(
            eyre!("Activity entry not found"),
            ErrorKind::NotFound,
        ));
    }
    Ok(())
}

#[instrument(skip_all)]
async fn clear(_ctx: ServerContext) -> Result<(), Error> {
    let conn = db()?;
    conn.execute("DELETE FROM activity", []).map_err(|e| {
        Error::new(
            eyre!("failed to clear activity: {e}"),
            ErrorKind::Filesystem,
        )
    })?;
    Ok(())
}

pub fn activity<C: rpc_toolkit::Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async(list)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "delete",
            from_fn_async(delete)
                .no_display()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "clear",
            from_fn_async(clear)
                .no_display()
                .with_call_remote::<CliContext>(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_open_is_retried() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, "").unwrap();
        let blocked = file.join("activity.db");
        let mut slot = None;
        assert!(ensure_open(&mut slot, blocked.to_str().unwrap()).is_err());
        assert!(slot.is_none());
        let path = dir.path().join("activity.db");
        ensure_open(&mut slot, path.to_str().unwrap()).unwrap();
        assert!(slot.is_some());
    }

    #[test]
    fn open_db_creates_the_directory_and_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/activity.db");
        let conn = open_db(path.to_str().unwrap()).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM activity", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
