use std::ops::Deref;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
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
/// Opens each entry's syslog line.
pub const SYSLOG_TAG: &str = "startwrt-activity";
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
static QUIESCED: AtomicUsize = AtomicUsize::new(0);
pub(crate) struct QuiesceGuard<'a>(&'a AtomicUsize);
impl Drop for QuiesceGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
fn quiesce_slot<'a>(q: &'a AtomicUsize, db: &Mutex<Option<Connection>>) -> QuiesceGuard<'a> {
    q.fetch_add(1, Ordering::SeqCst);
    *db.lock().unwrap_or_else(|e| e.into_inner()) = None;
    QuiesceGuard(q)
}
pub(crate) fn quiesce() -> QuiesceGuard<'static> {
    quiesce_slot(&QUIESCED, &DB)
}

pub struct ActivitySnapshot(Option<Vec<u8>>);
pub async fn snapshot(root: &Path) -> Result<ActivitySnapshot, Error> {
    match tokio::fs::read(root.join(ACTIVITY_DB.trim_start_matches('/'))).await {
        Ok(bytes) => Ok(ActivitySnapshot(Some(bytes))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ActivitySnapshot(None)),
        Err(e) => Err(e.into()),
    }
}
impl ActivitySnapshot {
    pub async fn restore(self, root: &Path) -> Result<(), Error> {
        let rel = ACTIVITY_DB.trim_start_matches('/');
        for base in [rel, crate::migrate::LEGACY_ACTIVITY_DB] {
            for suffix in ["", "-journal", "-wal", "-shm"] {
                startos::util::io::delete_file(root.join(format!("{base}{suffix}"))).await?;
            }
        }
        if let Some(bytes) = self.0 {
            startos::util::io::write_file_atomic(root.join(rel), &bytes).await?;
        }
        Ok(())
    }
}

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
    ensure_open(&mut slot, path, &QUIESCED)?;
    Ok(Db(slot))
}

fn ensure_open(slot: &mut Option<Connection>, path: &str, q: &AtomicUsize) -> Result<(), Error> {
    if q.load(Ordering::SeqCst) > 0 {
        let err = eyre!("activity database is quiesced");
        return Err(Error::new(err, ErrorKind::Filesystem));
    }
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
    let status = if success { "OK" } else { "FAIL" };
    let err_suffix = error.map(|e| format!(": {e}")).unwrap_or_default();
    tracing::info!(target: "activity", "{SYSLOG_TAG} [{category}.{action}] {status} {summary}{err_suffix}");

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
    tracing::info!(target: "activity", "{SYSLOG_TAG}(eMMC) [{category}.{action}] {status} {summary}{err_suffix}");

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
        let quiesced = AtomicUsize::new(0);
        let mut slot = None;
        assert!(ensure_open(&mut slot, blocked.to_str().unwrap(), &quiesced).is_err());
        assert!(slot.is_none());
        let path = dir.path().join("activity.db");
        ensure_open(&mut slot, path.to_str().unwrap(), &quiesced).unwrap();
        assert!(slot.is_some());
    }

    #[test]
    fn quiesce_closes_connection_and_blocks_reopen_until_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("activity.db");
        let path = path.to_str().unwrap();
        let quiesced = AtomicUsize::new(0);
        let db = Mutex::new(None);
        {
            let mut slot = db.lock().unwrap();
            ensure_open(&mut slot, path, &quiesced).unwrap();
            assert!(slot.is_some());
        }
        let g1 = quiesce_slot(&quiesced, &db);
        assert!(db.lock().unwrap().is_none());
        assert!(ensure_open(&mut db.lock().unwrap(), path, &quiesced).is_err());
        let g2 = quiesce_slot(&quiesced, &db);
        drop(g1);
        assert!(ensure_open(&mut db.lock().unwrap(), path, &quiesced).is_err());
        drop(g2);
        ensure_open(&mut db.lock().unwrap(), path, &quiesced).unwrap();
        assert!(db.lock().unwrap().is_some());
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

    #[tokio::test]
    async fn snapshot_restore_preserves_routers_db_even_through_symlink_or_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let rel = ACTIVITY_DB.trim_start_matches('/');
        let db_path = root.join(rel);
        log_to(
            db_path.to_str().unwrap(),
            "system",
            "boot",
            true,
            "Router own log",
            None,
        );

        let before = snapshot(root).await.unwrap();

        let stale_dir = tempfile::tempdir().unwrap();
        let stale_db = stale_dir.path().join("activity.db");
        log_to(
            stale_db.to_str().unwrap(),
            "system",
            "boot",
            true,
            "Stale backup log",
            None,
        );
        let stale_bytes = std::fs::read(&stale_db).unwrap();

        std::os::unix::fs::symlink("startwrt", root.join("var/lib/x")).unwrap();
        std::fs::write(root.join("var/lib/x/activity.db"), &stale_bytes).unwrap();
        std::fs::create_dir_all(root.join("etc/startwrt")).unwrap();
        for base in [rel, crate::migrate::LEGACY_ACTIVITY_DB] {
            for suffix in ["-journal", "-wal", "-shm"] {
                std::fs::write(root.join(format!("{base}{suffix}")), b"sidecar").unwrap();
            }
        }
        std::fs::write(root.join(crate::migrate::LEGACY_ACTIVITY_DB), &stale_bytes).unwrap();

        before.restore(root).await.unwrap();

        let read_summaries = |p: &Path| -> Vec<String> {
            let conn = Connection::open(p).unwrap();
            let mut stmt = conn.prepare("SELECT summary FROM activity").unwrap();
            stmt.query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(read_summaries(&db_path), vec!["Router own log"]);
        for base in [rel, crate::migrate::LEGACY_ACTIVITY_DB] {
            for suffix in ["-journal", "-wal", "-shm"] {
                assert!(!root.join(format!("{base}{suffix}")).exists());
            }
        }
        assert!(!root.join(crate::migrate::LEGACY_ACTIVITY_DB).exists());

        let before = snapshot(root).await.unwrap();
        let victim = root.join("etc/victim");
        std::fs::write(&victim, b"untouched").unwrap();
        std::fs::remove_file(&db_path).unwrap();
        std::os::unix::fs::symlink(&victim, &db_path).unwrap();

        before.restore(root).await.unwrap();

        assert_eq!(std::fs::read(&victim).unwrap(), b"untouched");
        assert!(!std::fs::symlink_metadata(&db_path)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(read_summaries(&db_path), vec!["Router own log"]);

        let fresh = tempfile::tempdir().unwrap();
        let fresh_root = fresh.path();
        let before = snapshot(fresh_root).await.unwrap();
        std::fs::create_dir_all(fresh_root.join("var/lib/startwrt")).unwrap();
        std::fs::create_dir_all(fresh_root.join("etc/startwrt")).unwrap();
        std::fs::write(fresh_root.join(rel), &stale_bytes).unwrap();
        std::fs::write(fresh_root.join(format!("{rel}-wal")), b"wal").unwrap();
        std::fs::write(
            fresh_root.join(crate::migrate::LEGACY_ACTIVITY_DB),
            &stale_bytes,
        )
        .unwrap();
        std::fs::write(
            fresh_root.join(format!("{}-journal", crate::migrate::LEGACY_ACTIVITY_DB)),
            b"journal",
        )
        .unwrap();

        before.restore(fresh_root).await.unwrap();

        assert!(!fresh_root.join(rel).exists());
        assert!(!fresh_root.join(format!("{rel}-wal")).exists());
        assert!(!fresh_root.join(crate::migrate::LEGACY_ACTIVITY_DB).exists());
        assert!(!fresh_root
            .join(format!("{}-journal", crate::migrate::LEGACY_ACTIVITY_DB))
            .exists());
    }
}
