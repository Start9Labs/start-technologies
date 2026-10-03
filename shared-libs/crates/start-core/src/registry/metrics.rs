use clap::Parser;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use visit_rs::ts::TS;

use crate::context::CliContext;
use crate::prelude::*;
use crate::registry::context::RegistryContext;
use crate::util::serde::{HandlerExtSerde, WithIoFormat, display_serializable};

pub fn metrics_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "summary",
            from_fn_async(get_summary)
                .with_metadata("admin", Value::Bool(true))
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| display_summary(handle.params, result))
                .with_about("about.get-metrics-summary")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "users",
            from_fn_async(get_users)
                .with_metadata("admin", Value::Bool(true))
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| display_users(handle.params, result))
                .with_about("about.get-metrics-users")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "downloads",
            from_fn_async(get_downloads)
                .with_metadata("admin", Value::Bool(true))
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| display_downloads(handle.params, result))
                .with_about("about.get-metrics-downloads")
                .with_call_remote::<CliContext>(),
        )
}

// --- summary ---

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CountEntry {
    pub label: String,
    pub count: u64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MetricsSummary {
    pub total_checkins: u64,
    pub unique_servers: u64,
    pub total_package_requests: u64,
    pub by_arch: Vec<CountEntry>,
    pub by_os_version: Vec<CountEntry>,
}

pub async fn get_summary(ctx: RegistryContext) -> Result<MetricsSummary, Error> {
    tokio::task::spawn_blocking(move || {
        ctx.metrics_db.peek(|conn| {
            let total_checkins: u64 = conn
                .query_row("SELECT COUNT(*) FROM user_activity", [], |row| row.get(0))
                .with_kind(ErrorKind::Database)?;

            let unique_servers: u64 = conn
                .query_row(
                    "SELECT COUNT(DISTINCT server_id) FROM user_activity",
                    [],
                    |row| row.get(0),
                )
                .with_kind(ErrorKind::Database)?;

            let total_package_requests: u64 = conn
                .query_row("SELECT COUNT(*) FROM package_request", [], |row| row.get(0))
                .with_kind(ErrorKind::Database)?;

            let by_arch = query_count_entries(
                conn,
                "SELECT COALESCE(arch, 'unknown'), COUNT(*) FROM user_activity GROUP BY arch ORDER BY COUNT(*) DESC",
            )?;

            let by_os_version = query_count_entries(
                conn,
                "SELECT COALESCE(os_version, 'unknown'), COUNT(*) FROM user_activity GROUP BY os_version ORDER BY COUNT(*) DESC",
            )?;

            Ok(MetricsSummary {
                total_checkins,
                unique_servers,
                total_package_requests,
                by_arch,
                by_os_version,
            })
        })
    })
    .await
    .with_kind(ErrorKind::Unknown)?
}

fn display_summary<T>(params: WithIoFormat<T>, summary: MetricsSummary) -> Result<(), Error> {
    use prettytable::*;

    if let Some(format) = params.format {
        return display_serializable(format, summary);
    }

    println!("Total check-ins:       {}", summary.total_checkins);
    println!("Unique servers:        {}", summary.unique_servers);
    println!("Total package requests: {}", summary.total_package_requests);

    if !summary.by_arch.is_empty() {
        println!();
        let mut table = Table::new();
        table.add_row(row![bc => "ARCHITECTURE", "COUNT"]);
        for entry in &summary.by_arch {
            table.add_row(row![&entry.label, entry.count]);
        }
        table.print_tty(false)?;
    }

    if !summary.by_os_version.is_empty() {
        println!();
        let mut table = Table::new();
        table.add_row(row![bc => "OS VERSION", "COUNT"]);
        for entry in &summary.by_os_version {
            table.add_row(row![&entry.label, entry.count]);
        }
        table.print_tty(false)?;
    }

    Ok(())
}

// --- users ---

#[derive(Debug, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GetUsersParams {
    /// Start of time range (RFC 3339)
    #[ts(type = "string | null")]
    #[arg(long, help = "help.arg.metrics-after")]
    pub after: Option<String>,
    /// End of time range (RFC 3339)
    #[ts(type = "string | null")]
    #[arg(long, help = "help.arg.metrics-before")]
    pub before: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UsersResponse {
    pub unique_servers: u64,
    pub total_checkins: u64,
}

pub async fn get_users(
    ctx: RegistryContext,
    GetUsersParams { after, before }: GetUsersParams,
) -> Result<UsersResponse, Error> {
    tokio::task::spawn_blocking(move || {
        ctx.metrics_db.peek(|conn| {
            let (where_clause, params) = time_range_where(&after, &before);

            let unique_servers: u64 = conn
                .query_row(
                    &format!("SELECT COUNT(DISTINCT server_id) FROM user_activity{where_clause}"),
                    rusqlite::params_from_iter(&params),
                    |row| row.get(0),
                )
                .with_kind(ErrorKind::Database)?;

            let total_checkins: u64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM user_activity{where_clause}"),
                    rusqlite::params_from_iter(&params),
                    |row| row.get(0),
                )
                .with_kind(ErrorKind::Database)?;

            Ok(UsersResponse {
                unique_servers,
                total_checkins,
            })
        })
    })
    .await
    .with_kind(ErrorKind::Unknown)?
}

fn display_users<T>(params: WithIoFormat<T>, response: UsersResponse) -> Result<(), Error> {
    if let Some(format) = params.format {
        return display_serializable(format, response);
    }

    println!("Unique servers: {}", response.unique_servers);
    println!("Total check-ins: {}", response.total_checkins);

    Ok(())
}

// --- downloads ---

#[derive(Debug, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GetDownloadsParams {
    /// Filter by package ID
    #[ts(type = "string | null")]
    #[arg(long, help = "help.arg.metrics-pkg-id")]
    pub pkg_id: Option<String>,
    /// Filter by version
    #[ts(type = "string | null")]
    #[arg(long, help = "help.arg.metrics-version")]
    pub version: Option<String>,
    /// Start of time range (RFC 3339)
    #[ts(type = "string | null")]
    #[arg(long, help = "help.arg.metrics-after")]
    pub after: Option<String>,
    /// End of time range (RFC 3339)
    #[ts(type = "string | null")]
    #[arg(long, help = "help.arg.metrics-before")]
    pub before: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PackageVersionCount {
    pub pkg_id: String,
    pub version: String,
    pub count: u64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DownloadsResponse {
    pub total_requests: u64,
    pub by_package: Vec<CountEntry>,
    pub by_package_version: Vec<PackageVersionCount>,
}

pub async fn get_downloads(
    ctx: RegistryContext,
    GetDownloadsParams {
        pkg_id,
        version,
        after,
        before,
    }: GetDownloadsParams,
) -> Result<DownloadsResponse, Error> {
    tokio::task::spawn_blocking(move || {
        ctx.metrics_db.peek(|conn| {
            let (where_clause, params) =
                downloads_where(&pkg_id, &version, &after, &before);

            let total_requests: u64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM package_request{where_clause}"),
                    rusqlite::params_from_iter(&params),
                    |row| row.get(0),
                )
                .with_kind(ErrorKind::Database)?;

            let by_package = query_count_entries_with_params(
                conn,
                &format!(
                    "SELECT pkg_id, COUNT(*) FROM package_request{where_clause} GROUP BY pkg_id ORDER BY COUNT(*) DESC"
                ),
                &params,
            )?;

            let by_package_version = {
                let mut stmt = conn
                    .prepare(&format!(
                        "SELECT pkg_id, COALESCE(version, 'unknown'), COUNT(*) FROM package_request{where_clause} GROUP BY pkg_id, version ORDER BY pkg_id, COUNT(*) DESC"
                    ))
                    .with_kind(ErrorKind::Database)?;
                let rows = stmt
                    .query_map(rusqlite::params_from_iter(&params), |row| {
                        Ok(PackageVersionCount {
                            pkg_id: row.get(0)?,
                            version: row.get(1)?,
                            count: row.get(2)?,
                        })
                    })
                    .with_kind(ErrorKind::Database)?;
                rows.map(|r| r.with_kind(ErrorKind::Database))
                    .collect::<Result<Vec<_>, _>>()?
            };

            Ok(DownloadsResponse {
                total_requests,
                by_package,
                by_package_version,
            })
        })
    })
    .await
    .with_kind(ErrorKind::Unknown)?
}

fn display_downloads(
    params: WithIoFormat<GetDownloadsParams>,
    response: DownloadsResponse,
) -> Result<(), Error> {
    use prettytable::*;

    if let Some(format) = params.format {
        return display_serializable(format, response);
    }

    println!("Total requests: {}", response.total_requests);

    if !response.by_package.is_empty() {
        println!();
        let mut table = Table::new();
        table.add_row(row![bc => "PACKAGE", "COUNT"]);
        for entry in &response.by_package {
            table.add_row(row![&entry.label, entry.count]);
        }
        table.print_tty(false)?;
    }

    if !response.by_package_version.is_empty() {
        println!();
        let mut table = Table::new();
        table.add_row(row![bc => "PACKAGE", "VERSION", "COUNT"]);
        for entry in &response.by_package_version {
            table.add_row(row![&entry.pkg_id, &entry.version, entry.count]);
        }
        table.print_tty(false)?;
    }

    Ok(())
}

// --- helpers ---

fn query_count_entries(conn: &rusqlite::Connection, sql: &str) -> Result<Vec<CountEntry>, Error> {
    query_count_entries_with_params(conn, sql, &[])
}

fn query_count_entries_with_params(
    conn: &rusqlite::Connection,
    sql: &str,
    params: &[String],
) -> Result<Vec<CountEntry>, Error> {
    let mut stmt = conn.prepare(sql).with_kind(ErrorKind::Database)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params), |row| {
            Ok(CountEntry {
                label: row.get(0)?,
                count: row.get(1)?,
            })
        })
        .with_kind(ErrorKind::Database)?;
    rows.map(|r| r.with_kind(ErrorKind::Database)).collect()
}

fn time_range_where(after: &Option<String>, before: &Option<String>) -> (String, Vec<String>) {
    let mut conditions = Vec::new();
    let mut params = Vec::new();

    if let Some(a) = after {
        params.push(a.clone());
        conditions.push(format!("created_at >= ?{}", params.len()));
    }
    if let Some(b) = before {
        params.push(b.clone());
        conditions.push(format!("created_at < ?{}", params.len()));
    }

    let clause = if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    };

    (clause, params)
}

fn downloads_where(
    pkg_id: &Option<String>,
    version: &Option<String>,
    after: &Option<String>,
    before: &Option<String>,
) -> (String, Vec<String>) {
    let mut conditions = Vec::new();
    let mut params = Vec::new();

    if let Some(id) = pkg_id {
        params.push(id.clone());
        conditions.push(format!("pkg_id = ?{}", params.len()));
    }
    if let Some(v) = version {
        params.push(v.clone());
        conditions.push(format!("version = ?{}", params.len()));
    }
    if let Some(a) = after {
        params.push(a.clone());
        conditions.push(format!("created_at >= ?{}", params.len()));
    }
    if let Some(b) = before {
        params.push(b.clone());
        conditions.push(format!("created_at < ?{}", params.len()));
    }

    let clause = if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    };

    (clause, params)
}
