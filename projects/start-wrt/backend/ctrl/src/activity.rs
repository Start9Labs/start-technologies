use crate::prelude::*;
use crate::utils::HandlerExtSerde;
use crate::{CliContext, ServerContext};
use clap::Parser;
use rpc_toolkit::{from_fn_async, HandlerExt as _, ParentHandler};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::{LazyLock, Mutex};
use tracing::instrument;

const ACTIVITY_DIR: &str = "/etc/startwrt";
#[cfg(not(test))]
const ACTIVITY_DB: &str = "/etc/startwrt/activity.db";
const MAX_ENTRIES: usize = 500;

const ACTIVITY_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS activity (
    id INTEGER PRIMARY KEY,
    timestamp TEXT NOT NULL,
    category TEXT NOT NULL,
    action TEXT NOT NULL,
    success INTEGER NOT NULL,
    summary TEXT NOT NULL,
    error TEXT
);";

static DB: LazyLock<Mutex<Connection>> = LazyLock::new(|| {
    let _ = std::fs::create_dir_all(ACTIVITY_DIR);
    #[cfg(test)]
    let conn = Connection::open_in_memory().expect("failed to open in-memory activity database");
    #[cfg(not(test))]
    let conn = Connection::open(ACTIVITY_DB).expect("failed to open activity database");
    conn.execute_batch(ACTIVITY_SCHEMA)
        .expect("failed to initialize activity table");
    Mutex::new(conn)
});

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityListResponse {
    pub entries: Vec<ActivityEntry>,
    pub total: usize,
}

#[derive(Debug, Deserialize, Serialize, Default, Parser)]
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

#[derive(Debug, Deserialize, Serialize, Parser)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ActivityDeleteParams {
    #[clap(long)]
    pub id: i64,
}

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
    let conn = DB.lock().map_err(|e| format!("lock poisoned: {e}"))?;
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

/// Log an activity entry into a database at an arbitrary path, creating the
/// file and schema if needed. The setup flasher uses this to record an entry
/// straight into the freshly-flashed eMMC DB (`<merged>/etc/startwrt/activity.db`),
/// because the flash runs from the microSD whose own activity DB is discarded.
/// Best-effort — never panics or propagates.
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

    if let Some(parent) = std::path::Path::new(db_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(db_path)?;
    conn.execute_batch(ACTIVITY_SCHEMA)?;
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
async fn list(
    _ctx: ServerContext,
    params: ActivityListParams,
) -> Result<ActivityListResponse, Error> {
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(50);

    let conn = DB
        .lock()
        .map_err(|e| Error::new(eyre!("lock poisoned: {e}"), ErrorKind::Incoherent))?;

    let total: usize = conn
        .query_row("SELECT COUNT(*) FROM activity", [], |row| row.get(0))
        .map_err(|e| Error::new(eyre!("failed to count activity: {e}"), ErrorKind::Filesystem))?;

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
        .map_err(|e| Error::new(eyre!("failed to query activity: {e}"), ErrorKind::Filesystem))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::new(eyre!("failed to read activity row: {e}"), ErrorKind::Filesystem))?;

    Ok(ActivityListResponse { entries, total })
}

#[instrument(skip_all)]
async fn delete(
    _ctx: ServerContext,
    ActivityDeleteParams { id }: ActivityDeleteParams,
) -> Result<(), Error> {
    let conn = DB
        .lock()
        .map_err(|e| Error::new(eyre!("lock poisoned: {e}"), ErrorKind::Incoherent))?;
    conn.execute("DELETE FROM activity WHERE id = ?1", [id])
        .map_err(|e| Error::new(eyre!("failed to delete activity: {e}"), ErrorKind::Filesystem))?;
    if conn.changes() == 0 {
        return Err(Error::new(eyre!("Activity entry not found"), ErrorKind::NotFound));
    }
    Ok(())
}

#[instrument(skip_all)]
async fn clear(_ctx: ServerContext) -> Result<(), Error> {
    let conn = DB
        .lock()
        .map_err(|e| Error::new(eyre!("lock poisoned: {e}"), ErrorKind::Incoherent))?;
    conn.execute("DELETE FROM activity", [])
        .map_err(|e| Error::new(eyre!("failed to clear activity: {e}"), ErrorKind::Filesystem))?;
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
