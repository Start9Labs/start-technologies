use chrono::Utc;
use clap::{Parser, ValueEnum};
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::PackageId;
use crate::context::CliContext;
use crate::prelude::*;
use crate::registry::context::RegistryContext;
use crate::util::VersionString;
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
        .subcommand(
            "installs",
            from_fn_async(get_installs)
                .with_metadata("admin", Value::Bool(true))
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| display_installs(handle.params, result))
                .with_about("about.get-metrics-installs")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "subscriptions",
            from_fn_async(get_subscriptions)
                .with_metadata("admin", Value::Bool(true))
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| {
                    display_subscriptions(handle.params, result)
                })
                .with_about("about.get-metrics-subscriptions")
                .with_call_remote::<CliContext>(),
        )
}

// --- recording ---

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct RecordInstallParams {
    pub id: PackageId,
    pub version: VersionString,
}

rpc_toolkit::reflect_ts!(RecordInstallParams);
rpc_toolkit::ts_export!(RecordInstallParams, namespaces = [""]);

/// Counts a completed install or update. Answers the same whether or not the version exists.
pub async fn record_install(
    ctx: RegistryContext,
    RecordInstallParams { id, version }: RecordInstallParams,
) -> Result<(), Error> {
    let listed = ctx
        .db
        .peek()
        .await
        .as_index()
        .as_package()
        .as_packages()
        .as_idx(&id)
        .map_or(false, |package| {
            package.as_versions().as_idx(&version).is_some()
        });
    if listed {
        record(
            ctx,
            "INSERT INTO package_install (created_at, pkg_id, version) VALUES (?1, ?2, ?3)",
            vec![Some(id.to_string()), Some(version.to_string())],
        );
    }
    Ok(())
}

pub(crate) fn record_subscription(ctx: &RegistryContext, os_version: Option<String>) {
    record(
        ctx.clone(),
        "INSERT INTO index_subscription (created_at, os_version) VALUES (?1, ?2)",
        vec![os_version],
    );
}

/// Inserts a row stamped with the current time ahead of `values`.
fn record(ctx: RegistryContext, sql: &'static str, values: Vec<Option<String>>) {
    tokio::task::spawn_blocking(move || {
        ctx.metrics_db.mutate(|conn| {
            let row = std::iter::once(Some(Utc::now().to_rfc3339())).chain(values);
            if let Err(e) = conn.execute(sql, rusqlite::params_from_iter(row)) {
                warn!("failed to record metric: {e}");
            }
        });
    });
}

// --- shared ---

#[derive(Debug, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct CountEntry {
    pub label: String,
    pub count: u64,
}

rpc_toolkit::reflect_ts!(CountEntry);
rpc_toolkit::ts_export!(CountEntry, namespaces = [""]);

#[derive(Clone, Copy, Debug, Deserialize, Serialize, VisitVariants, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum MetricsInterval {
    Day,
    Week,
    Month,
}

rpc_toolkit::reflect_ts!(MetricsInterval);
rpc_toolkit::ts_export!(MetricsInterval, namespaces = [""]);

impl MetricsInterval {
    /// A row's period: its UTC date, the Monday starting its week, or `YYYY-MM`.
    fn period(self) -> &'static str {
        match self {
            Self::Day => "substr(created_at, 1, 10)",
            Self::Week => "date(substr(created_at, 1, 10), 'weekday 0', '-6 days')",
            Self::Month => "substr(created_at, 1, 7)",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct PeriodCount {
    pub period: String,
    pub count: u64,
}

rpc_toolkit::reflect_ts!(PeriodCount);
rpc_toolkit::ts_export!(PeriodCount, namespaces = [""]);

#[derive(Debug, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct PackageVersionCount {
    pub pkg_id: String,
    pub version: String,
    pub count: u64,
}

rpc_toolkit::reflect_ts!(PackageVersionCount);
rpc_toolkit::ts_export!(PackageVersionCount, namespaces = [""]);

#[derive(Default)]
struct Filter {
    conditions: Vec<String>,
    params: Vec<String>,
}

impl Filter {
    fn range(after: &Option<String>, before: &Option<String>) -> Self {
        Self::default()
            .with("created_at >=", after)
            .with("created_at <", before)
    }

    fn with(mut self, condition: &str, value: &Option<String>) -> Self {
        if let Some(value) = value {
            self.params.push(value.clone());
            self.conditions
                .push(format!("{condition} ?{}", self.params.len()));
        }
        self
    }

    fn clause(&self) -> String {
        if self.conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", self.conditions.join(" AND "))
        }
    }
}

fn count(conn: &rusqlite::Connection, sql: &str, filter: &Filter) -> Result<u64, Error> {
    conn.query_row(
        &format!("{sql}{}", filter.clause()),
        rusqlite::params_from_iter(&filter.params),
        |row| row.get(0),
    )
    .with_kind(ErrorKind::Database)
}

fn count_entries(
    conn: &rusqlite::Connection,
    table: &str,
    label: &str,
    filter: &Filter,
) -> Result<Vec<CountEntry>, Error> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT COALESCE({label}, 'unknown'), COUNT(*) FROM {table}{} GROUP BY 1 ORDER BY 2 DESC",
            filter.clause()
        ))
        .with_kind(ErrorKind::Database)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(&filter.params), |row| {
            Ok(CountEntry {
                label: row.get(0)?,
                count: row.get(1)?,
            })
        })
        .with_kind(ErrorKind::Database)?;
    rows.map(|r| r.with_kind(ErrorKind::Database)).collect()
}

fn package_version_counts(
    conn: &rusqlite::Connection,
    table: &str,
    filter: &Filter,
) -> Result<Vec<PackageVersionCount>, Error> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT pkg_id, COALESCE(version, 'unknown'), COUNT(*) FROM {table}{} GROUP BY pkg_id, version ORDER BY pkg_id, COUNT(*) DESC",
            filter.clause()
        ))
        .with_kind(ErrorKind::Database)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(&filter.params), |row| {
            Ok(PackageVersionCount {
                pkg_id: row.get(0)?,
                version: row.get(1)?,
                count: row.get(2)?,
            })
        })
        .with_kind(ErrorKind::Database)?;
    rows.map(|r| r.with_kind(ErrorKind::Database)).collect()
}

fn series(
    conn: &rusqlite::Connection,
    table: &str,
    filter: &Filter,
    interval: Option<MetricsInterval>,
) -> Result<Vec<PeriodCount>, Error> {
    let Some(interval) = interval else {
        return Ok(Vec::new());
    };
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {}, COUNT(*) FROM {table}{} GROUP BY 1 ORDER BY 1",
            interval.period(),
            filter.clause()
        ))
        .with_kind(ErrorKind::Database)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(&filter.params), |row| {
            Ok(PeriodCount {
                period: row.get(0)?,
                count: row.get(1)?,
            })
        })
        .with_kind(ErrorKind::Database)?;
    rows.map(|r| r.with_kind(ErrorKind::Database)).collect()
}

async fn query<T: Send + 'static>(
    ctx: RegistryContext,
    f: impl FnOnce(&rusqlite::Connection) -> Result<T, Error> + Send + 'static,
) -> Result<T, Error> {
    tokio::task::spawn_blocking(move || ctx.metrics_db.peek(f))
        .await
        .with_kind(ErrorKind::Unknown)?
}

fn print_counts(heading: &str, entries: &[CountEntry]) -> Result<(), Error> {
    use prettytable::*;

    if entries.is_empty() {
        return Ok(());
    }
    println!();
    let mut table = Table::new();
    table.add_row(row![bc => heading, "COUNT"]);
    for entry in entries {
        table.add_row(row![&entry.label, entry.count]);
    }
    print_table(&table)
}

fn print_package_versions(entries: &[PackageVersionCount]) -> Result<(), Error> {
    use prettytable::*;

    if entries.is_empty() {
        return Ok(());
    }
    println!();
    let mut table = Table::new();
    table.add_row(row![bc => "PACKAGE", "VERSION", "COUNT"]);
    for entry in entries {
        table.add_row(row![&entry.pkg_id, &entry.version, entry.count]);
    }
    print_table(&table)
}

fn print_series(series: &[PeriodCount]) -> Result<(), Error> {
    use prettytable::*;

    if series.is_empty() {
        return Ok(());
    }
    println!();
    let mut table = Table::new();
    table.add_row(row![bc => "PERIOD", "COUNT"]);
    for point in series {
        table.add_row(row![&point.period, point.count]);
    }
    print_table(&table)
}

// --- summary ---

#[derive(Debug, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSummary {
    pub total_checkins: u64,
    pub unique_servers: u64,
    pub total_package_requests: u64,
    pub total_installs: u64,
    pub total_subscriptions: u64,
    pub by_arch: Vec<CountEntry>,
    pub by_os_version: Vec<CountEntry>,
}

rpc_toolkit::reflect_ts!(MetricsSummary);
rpc_toolkit::ts_export!(MetricsSummary, namespaces = [""]);

pub async fn get_summary(ctx: RegistryContext) -> Result<MetricsSummary, Error> {
    query(ctx, |conn| {
        let all = Filter::default();
        Ok(MetricsSummary {
            total_checkins: count(conn, "SELECT COUNT(*) FROM user_activity", &all)?,
            unique_servers: count(
                conn,
                "SELECT COUNT(DISTINCT server_id) FROM user_activity",
                &all,
            )?,
            total_package_requests: count(conn, "SELECT COUNT(*) FROM package_request", &all)?,
            total_installs: count(conn, "SELECT COUNT(*) FROM package_install", &all)?,
            total_subscriptions: count(conn, "SELECT COUNT(*) FROM index_subscription", &all)?,
            by_arch: count_entries(conn, "user_activity", "arch", &all)?,
            by_os_version: count_entries(conn, "user_activity", "os_version", &all)?,
        })
    })
    .await
}

fn display_summary<T>(params: WithIoFormat<T>, summary: MetricsSummary) -> Result<(), Error> {
    if let Some(format) = params.format {
        return display_serializable(format, summary);
    }

    println!("Total check-ins:        {}", summary.total_checkins);
    println!("Unique servers:         {}", summary.unique_servers);
    println!("Total package requests: {}", summary.total_package_requests);
    println!("Total installs:         {}", summary.total_installs);
    println!("Total subscriptions:    {}", summary.total_subscriptions);
    print_counts("ARCHITECTURE", &summary.by_arch)?;
    print_counts("OS VERSION", &summary.by_os_version)
}

// --- users ---

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct GetUsersParams {
    /// Start of time range (RFC 3339)
    #[arg(long, help = "help.arg.metrics-after")]
    pub after: Option<String>,
    /// End of time range (RFC 3339)
    #[arg(long, help = "help.arg.metrics-before")]
    pub before: Option<String>,
    /// Also report each period in the range
    #[arg(long, help = "help.arg.metrics-interval")]
    #[serde(default)]
    pub interval: Option<MetricsInterval>,
}

rpc_toolkit::reflect_ts!(GetUsersParams);
rpc_toolkit::ts_export!(GetUsersParams, namespaces = [""]);

#[derive(Debug, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct PeriodUsers {
    pub period: String,
    pub unique_servers: u64,
    pub checkins: u64,
}

rpc_toolkit::reflect_ts!(PeriodUsers);
rpc_toolkit::ts_export!(PeriodUsers, namespaces = [""]);

#[derive(Debug, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct UsersResponse {
    pub unique_servers: u64,
    pub total_checkins: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub series: Vec<PeriodUsers>,
}

rpc_toolkit::reflect_ts!(UsersResponse);
rpc_toolkit::ts_export!(UsersResponse, namespaces = [""]);

pub async fn get_users(
    ctx: RegistryContext,
    GetUsersParams {
        after,
        before,
        interval,
    }: GetUsersParams,
) -> Result<UsersResponse, Error> {
    query(ctx, move |conn| {
        let filter = Filter::range(&after, &before);
        let series = match interval {
            None => Vec::new(),
            Some(interval) => {
                let mut stmt = conn
                    .prepare(&format!(
                        "SELECT {}, COUNT(DISTINCT server_id), COUNT(*) FROM user_activity{} GROUP BY 1 ORDER BY 1",
                        interval.period(),
                        filter.clause()
                    ))
                    .with_kind(ErrorKind::Database)?;
                let rows = stmt
                    .query_map(rusqlite::params_from_iter(&filter.params), |row| {
                        Ok(PeriodUsers {
                            period: row.get(0)?,
                            unique_servers: row.get(1)?,
                            checkins: row.get(2)?,
                        })
                    })
                    .with_kind(ErrorKind::Database)?;
                rows.map(|r| r.with_kind(ErrorKind::Database))
                    .collect::<Result<_, _>>()?
            }
        };
        Ok(UsersResponse {
            unique_servers: count(
                conn,
                "SELECT COUNT(DISTINCT server_id) FROM user_activity",
                &filter,
            )?,
            total_checkins: count(conn, "SELECT COUNT(*) FROM user_activity", &filter)?,
            series,
        })
    })
    .await
}

fn display_users<T>(params: WithIoFormat<T>, response: UsersResponse) -> Result<(), Error> {
    use prettytable::*;

    if let Some(format) = params.format {
        return display_serializable(format, response);
    }

    println!("Unique servers: {}", response.unique_servers);
    println!("Total check-ins: {}", response.total_checkins);

    if !response.series.is_empty() {
        println!();
        let mut table = Table::new();
        table.add_row(row![bc => "PERIOD", "UNIQUE SERVERS", "CHECK-INS"]);
        for point in &response.series {
            table.add_row(row![&point.period, point.unique_servers, point.checkins]);
        }
        print_table(&table)?;
    }

    Ok(())
}

// --- downloads and installs ---

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct GetDownloadsParams {
    /// Filter by package ID
    #[arg(long, help = "help.arg.metrics-pkg-id")]
    pub pkg_id: Option<String>,
    /// Filter by version
    #[arg(long, help = "help.arg.metrics-version")]
    pub version: Option<String>,
    /// Start of time range (RFC 3339)
    #[arg(long, help = "help.arg.metrics-after")]
    pub after: Option<String>,
    /// End of time range (RFC 3339)
    #[arg(long, help = "help.arg.metrics-before")]
    pub before: Option<String>,
    /// Also report each period in the range
    #[arg(long, help = "help.arg.metrics-interval")]
    #[serde(default)]
    pub interval: Option<MetricsInterval>,
}

rpc_toolkit::reflect_ts!(GetDownloadsParams);
rpc_toolkit::ts_export!(GetDownloadsParams, namespaces = [""]);

impl GetDownloadsParams {
    fn filter(&self) -> Filter {
        Filter::range(&self.after, &self.before)
            .with("pkg_id =", &self.pkg_id)
            .with("version =", &self.version)
    }
}

#[derive(Debug, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct DownloadsResponse {
    pub total_requests: u64,
    pub by_package: Vec<CountEntry>,
    pub by_package_version: Vec<PackageVersionCount>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub series: Vec<PeriodCount>,
}

rpc_toolkit::reflect_ts!(DownloadsResponse);
rpc_toolkit::ts_export!(DownloadsResponse, namespaces = [""]);

pub async fn get_downloads(
    ctx: RegistryContext,
    params: GetDownloadsParams,
) -> Result<DownloadsResponse, Error> {
    query(ctx, move |conn| {
        let filter = params.filter();
        Ok(DownloadsResponse {
            total_requests: count(conn, "SELECT COUNT(*) FROM package_request", &filter)?,
            by_package: count_entries(conn, "package_request", "pkg_id", &filter)?,
            by_package_version: package_version_counts(conn, "package_request", &filter)?,
            series: series(conn, "package_request", &filter, params.interval)?,
        })
    })
    .await
}

fn display_downloads(
    params: WithIoFormat<GetDownloadsParams>,
    response: DownloadsResponse,
) -> Result<(), Error> {
    if let Some(format) = params.format {
        return display_serializable(format, response);
    }

    println!("Total requests: {}", response.total_requests);
    print_counts("PACKAGE", &response.by_package)?;
    print_package_versions(&response.by_package_version)?;
    print_series(&response.series)
}

#[derive(Debug, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct InstallsResponse {
    pub total_installs: u64,
    pub by_package: Vec<CountEntry>,
    pub by_package_version: Vec<PackageVersionCount>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub series: Vec<PeriodCount>,
}

rpc_toolkit::reflect_ts!(InstallsResponse);
rpc_toolkit::ts_export!(InstallsResponse, namespaces = [""]);

pub async fn get_installs(
    ctx: RegistryContext,
    params: GetDownloadsParams,
) -> Result<InstallsResponse, Error> {
    query(ctx, move |conn| {
        let filter = params.filter();
        Ok(InstallsResponse {
            total_installs: count(conn, "SELECT COUNT(*) FROM package_install", &filter)?,
            by_package: count_entries(conn, "package_install", "pkg_id", &filter)?,
            by_package_version: package_version_counts(conn, "package_install", &filter)?,
            series: series(conn, "package_install", &filter, params.interval)?,
        })
    })
    .await
}

fn display_installs(
    params: WithIoFormat<GetDownloadsParams>,
    response: InstallsResponse,
) -> Result<(), Error> {
    if let Some(format) = params.format {
        return display_serializable(format, response);
    }

    println!("Total installs: {}", response.total_installs);
    print_counts("PACKAGE", &response.by_package)?;
    print_package_versions(&response.by_package_version)?;
    print_series(&response.series)
}

// --- subscriptions ---

#[derive(Debug, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionsResponse {
    pub total_subscriptions: u64,
    pub by_os_version: Vec<CountEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub series: Vec<PeriodCount>,
}

rpc_toolkit::reflect_ts!(SubscriptionsResponse);
rpc_toolkit::ts_export!(SubscriptionsResponse, namespaces = [""]);

pub async fn get_subscriptions(
    ctx: RegistryContext,
    GetUsersParams {
        after,
        before,
        interval,
    }: GetUsersParams,
) -> Result<SubscriptionsResponse, Error> {
    query(ctx, move |conn| {
        let filter = Filter::range(&after, &before);
        Ok(SubscriptionsResponse {
            total_subscriptions: count(conn, "SELECT COUNT(*) FROM index_subscription", &filter)?,
            by_os_version: count_entries(conn, "index_subscription", "os_version", &filter)?,
            series: series(conn, "index_subscription", &filter, interval)?,
        })
    })
    .await
}

fn display_subscriptions(
    params: WithIoFormat<GetUsersParams>,
    response: SubscriptionsResponse,
) -> Result<(), Error> {
    if let Some(format) = params.format {
        return display_serializable(format, response);
    }

    println!("Total subscriptions: {}", response.total_subscriptions);
    print_counts("OS VERSION", &response.by_os_version)?;
    print_series(&response.series)
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use super::*;

    fn db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE package_install (id INTEGER PRIMARY KEY, created_at TEXT NOT NULL, pkg_id TEXT NOT NULL, version TEXT NOT NULL);",
        )
        .unwrap();
        for (at, id) in [
            ("2026-11-02T23:59:59.999999999+00:00", "nextcloud"),
            ("2026-11-03T00:00:00+00:00", "nextcloud"),
            ("2026-11-08T23:59:59+00:00", "gitea"),
            ("2026-11-09T12:00:00+00:00", "gitea"),
            ("2026-12-01T00:00:00+00:00", "nextcloud"),
        ] {
            conn.execute(
                "INSERT INTO package_install (created_at, pkg_id, version) VALUES (?1, ?2, '1.0:0')",
                params![at, id],
            )
            .unwrap();
        }
        conn
    }

    fn periods(interval: MetricsInterval, filter: &Filter) -> Vec<(String, u64)> {
        series(&db(), "package_install", filter, Some(interval))
            .unwrap()
            .into_iter()
            .map(|p| (p.period, p.count))
            .collect()
    }

    #[test]
    fn series_buckets_by_utc_day_monday_week_and_month() {
        let all = Filter::default();
        let p = |s: &str, n| (s.to_owned(), n);
        assert_eq!(
            periods(MetricsInterval::Day, &all),
            [
                p("2026-11-02", 1),
                p("2026-11-03", 1),
                p("2026-11-08", 1),
                p("2026-11-09", 1),
                p("2026-12-01", 1),
            ]
        );
        assert_eq!(
            periods(MetricsInterval::Week, &all),
            [p("2026-11-02", 3), p("2026-11-09", 1), p("2026-11-30", 1)]
        );
        assert_eq!(
            periods(MetricsInterval::Month, &all),
            [p("2026-11", 4), p("2026-12", 1)]
        );
    }

    #[test]
    fn filters_combine_range_and_package() {
        let filter = Filter::range(&Some("2026-11-03T00:00:00+00:00".into()), &None)
            .with("pkg_id =", &Some("nextcloud".into()));
        assert_eq!(
            count(&db(), "SELECT COUNT(*) FROM package_install", &filter).unwrap(),
            2
        );
        assert!(
            series(&db(), "package_install", &filter, None)
                .unwrap()
                .is_empty()
        );
    }
}
