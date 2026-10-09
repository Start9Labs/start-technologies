use std::convert::Infallible;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::process::Stdio;
use std::str::FromStr;
use std::time::{Duration, UNIX_EPOCH};

use axum::extract::ws;
use chrono::{DateTime, Utc};
use clap::builder::ValueParserFactory;
use clap::{Args, FromArgMatches, Parser};
use color_eyre::eyre::eyre;
use futures::stream::BoxStream;
use futures::{Future, Stream, StreamExt, TryStreamExt};
use itertools::Itertools;
use rpc_toolkit::ts::TS;
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{
    CallRemote, Context, Empty, HandlerArgs, HandlerExt, HandlerFor, ParentHandler, from_fn_async,
};
use serde::de::{self, DeserializeOwned};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio_stream::wrappers::LinesStream;
use tokio_tungstenite::tungstenite::Message;
use tracing::instrument;

use crate::PackageId;
use crate::context::{CliContext, RpcContext};
use crate::error::ResultExt;
use crate::prelude::*;
use crate::rpc_continuations::{Guid, RpcContinuation, RpcContinuations};
use crate::util::net::WebSocket;
use crate::util::serde::Reversible;
use crate::util::{FromStrParser, Invoke};

#[pin_project::pin_project]
pub struct LogStream {
    _child: Option<Child>,
    #[pin]
    entries: BoxStream<'static, Result<JournalctlEntry, Error>>,
}
impl Deref for LogStream {
    type Target = BoxStream<'static, Result<JournalctlEntry, Error>>;
    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}
impl DerefMut for LogStream {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entries
    }
}
impl Stream for LogStream {
    type Item = Result<JournalctlEntry, Error>;
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.project();
        Stream::poll_next(this.entries, cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.entries.size_hint()
    }
}

#[instrument(skip_all)]
async fn ws_handler(
    first_entry: Option<LogEntry>,
    mut logs: LogStream,
    mut stream: WebSocket,
) -> Result<(), Error> {
    if let Some(first_entry) = first_entry {
        stream
            .send(ws::Message::Text(
                serde_json::to_string(&first_entry)
                    .with_kind(ErrorKind::Serialization)?
                    .into(),
            ))
            .await
            .with_kind(ErrorKind::Network)?;
    }

    loop {
        tokio::select! {
            entry = logs.try_next() => {
                if let Some(entry) = entry? {
                    let (_, log_entry) = entry.log_entry()?;
                    stream
                        .send(ws::Message::Text(
                            serde_json::to_string(&log_entry)
                                .with_kind(ErrorKind::Serialization)?
                                .into(),
                        ))
                        .await
                        .with_kind(ErrorKind::Network)?;
                } else {
                    return stream.normal_close("complete").await;
                }
            },
            msg = stream.recv() => {
                if msg.transpose().with_kind(crate::ErrorKind::Network)?.is_none() {
                    return Ok(())
                }
            }
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct LogResponse {
    #[visit(wire = "Vec<LogEntry>")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub entries: Reversible<LogEntry>,
    start_cursor: Option<String>,
    end_cursor: Option<String>,
}

rpc_toolkit::reflect_ts!(LogResponse);
rpc_toolkit::ts_export!(LogResponse, namespaces = [""]);
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct LogFollowResponse {
    start_cursor: Option<String>,
    guid: Guid,
}

rpc_toolkit::reflect_ts!(LogFollowResponse);
rpc_toolkit::ts_export!(LogFollowResponse, namespaces = [""]);

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    #[visit(ts(type = "string"), wire = "rpc_toolkit::ts::Unknown")]
    #[visit(opaque, type_attributes(visit::wire))]
    timestamp: DateTime<Utc>,
    message: String,
    boot_id: String,
}

rpc_toolkit::reflect_ts!(LogEntry);
rpc_toolkit::ts_export!(LogEntry, namespaces = [""]);
impl std::fmt::Display for LogEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{} {}",
            self.timestamp
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            self.message
        )
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JournalctlEntry {
    #[serde(rename = "__REALTIME_TIMESTAMP")]
    pub timestamp: String,
    #[serde(rename = "MESSAGE")]
    #[serde(deserialize_with = "deserialize_log_message")]
    pub message: String,
    #[serde(rename = "__CURSOR")]
    pub cursor: String,
    #[serde(rename = "_BOOT_ID")]
    pub boot_id: String,
}
impl JournalctlEntry {
    fn log_entry(self) -> Result<(String, LogEntry), Error> {
        Ok((
            self.cursor,
            LogEntry {
                timestamp: DateTime::<Utc>::from(
                    UNIX_EPOCH + Duration::from_micros(self.timestamp.parse::<u64>()?),
                ),
                message: self.message,
                boot_id: self.boot_id,
            },
        ))
    }
}

fn deserialize_log_message<'de, D: serde::de::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<String, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = String;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "a parsable string")
        }
        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v.to_owned())
        }
        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(v)
        }
        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(String::new())
        }
        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            String::from_utf8(
                std::iter::repeat_with(|| seq.next_element::<u8>().transpose())
                    .take_while(|a| a.is_some())
                    .flatten()
                    .collect::<Result<Vec<u8>, _>>()?,
            )
            .map(|s| s.to_owned())
            .map_err(serde::de::Error::custom)
        }
    }
    deserializer.deserialize_any(Visitor)
}

/// Defining how we are going to filter on a journalctl cli log.
/// Kernal: (-k --dmesg                 Show kernel message log from the current boot)
/// Unit: ( -u --unit=UNIT             Show logs from the specified unit
///     --user-unit=UNIT        Show logs from the specified user unit))
/// System: Unit is startd, but we also filter on the comm
/// Container: Filtering containers, like podman/docker is done by filtering on the CONTAINER_NAME
#[derive(Debug, Clone)]
pub enum LogSource {
    Kernel,
    Unit(&'static str),
    Package(PackageId),
}

pub const SYSTEM_UNIT: &str = "startd";

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct PackageIdParams {
    #[arg(help = "help.arg.package-id")]
    id: PackageId,
}

rpc_toolkit::reflect_ts!(PackageIdParams);

#[derive(Debug, Clone)]
pub enum BootIdentifier {
    Index(i32),
    Id(String),
}
impl FromStr for BootIdentifier {
    type Err = Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.parse() {
            Ok(i) => Self::Index(i),
            Err(_) => Self::Id(s.to_owned()),
        })
    }
}
impl ValueParserFactory for BootIdentifier {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        Self::Parser::new()
    }
}
impl Serialize for BootIdentifier {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Index(i) => serializer.serialize_i32(*i),
            Self::Id(i) => serializer.serialize_str(i),
        }
    }
}
impl<'de> Deserialize<'de> for BootIdentifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = BootIdentifier;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "a string or integer")
            }
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Self::Value::Id(v.to_owned()))
            }
            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Self::Value::Id(v))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Self::Value::Index(v as i32))
            }
            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Self::Value::Index(v as i32))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Self::Value::Index(v as i32))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}
impl From<BootIdentifier> for String {
    fn from(value: BootIdentifier) -> Self {
        match value {
            BootIdentifier::Index(i) => i.to_string(),
            BootIdentifier::Id(i) => i,
        }
    }
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct LogsParams<Extra: FromArgMatches + Args = Empty> {
    #[command(flatten)]
    #[serde(flatten)]
    extra: Extra,
    #[arg(short = 'l', long = "limit", help = "help.arg.log-limit")]
    limit: Option<usize>,
    #[arg(
        short = 'c',
        long = "cursor",
        conflicts_with = "follow",
        help = "help.arg.log-cursor"
    )]
    cursor: Option<String>,
    #[arg(short = 'b', long = "boot", help = "help.arg.log-boot")]
    #[serde(default)]
    #[visit(
        ts(type = "number | string | null"),
        wire = "Option<rpc_toolkit::ts::Unknown>"
    )]
    #[visit(opaque, type_attributes(visit::wire))]
    boot: Option<BootIdentifier>,
    #[arg(
        short = 'B',
        long = "before",
        conflicts_with = "follow",
        help = "help.arg.log-before"
    )]
    #[serde(default)]
    before: bool,
}

rpc_toolkit::reflect_ts!(impl [Extra: FromArgMatches + Args] for LogsParams<Extra> where [Extra: rpc_toolkit::ts::TS]);
rpc_toolkit::ts_export!(LogsParams<Empty>, name = "LogsParams", namespaces = [""]);

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CliLogsParams<Extra: FromArgMatches + Args = Empty> {
    #[command(flatten)]
    #[serde(flatten)]
    rpc_params: LogsParams<Extra>,
    #[arg(short = 'f', long = "follow", help = "help.arg.log-follow")]
    #[serde(default)]
    follow: bool,
}

#[allow(private_bounds)]
pub fn logs<
    C: Context + AsRef<RpcContinuations>,
    Extra: FromArgMatches + Serialize + DeserializeOwned + Args + TS + Send + Sync + 'static,
>(
    source: impl for<'a> LogSourceFn<'a, C, Extra>,
) -> ParentHandler<C, LogsParams<Extra>> {
    ParentHandler::new()
        .root_handler(logs_nofollow::<C, Extra>(source.clone()).no_cli())
        .subcommand(
            "follow",
            logs_follow::<C, Extra>(source)
                .with_inherited(|params, _| params)
                .no_cli(),
        )
}

pub async fn cli_logs<RemoteContext, Extra>(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        params: CliLogsParams { rpc_params, follow },
        ..
    }: HandlerArgs<CliContext, CliLogsParams<Extra>>,
) -> Result<(), RpcError>
where
    CliContext: CallRemote<RemoteContext>,
    Extra: FromArgMatches + Args + Serialize + Send + Sync,
{
    let method = parent_method
        .into_iter()
        .chain(method)
        .chain(follow.then_some("follow"))
        .join(".");

    if follow {
        let res = from_value::<LogFollowResponse>(
            ctx.call_remote::<RemoteContext>(&method, to_value(&rpc_params)?)
                .await?,
        )?;

        let mut stream = ctx.ws_continuation(res.guid).await?;
        while let Some(log) = stream.try_next().await? {
            if let Message::Text(log) = log {
                println!("{}", serde_json::from_str::<LogEntry>(&log)?);
            }
        }
    } else {
        let res = from_value::<LogResponse>(
            ctx.call_remote::<RemoteContext>(&method, to_value(&rpc_params)?)
                .await?,
        )?;

        for entry in res.entries.iter() {
            println!("{}", entry);
        }
    }

    Ok(())
}

trait LogSourceFn<'a, Context, Extra>: Clone + Send + Sync + 'static {
    type Fut: Future<Output = Result<LogSource, Error>> + Send + 'a;
    fn call(&self, ctx: &'a Context, extra: Extra) -> Self::Fut;
}

impl<'a, C: Context, Extra, F, Fut> LogSourceFn<'a, C, Extra> for F
where
    F: Fn(&'a C, Extra) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<LogSource, Error>> + Send + 'a,
{
    type Fut = Fut;
    fn call(&self, ctx: &'a C, extra: Extra) -> Self::Fut {
        self(ctx, extra)
    }
}

fn logs_nofollow<C, Extra>(
    f: impl for<'a> LogSourceFn<'a, C, Extra>,
) -> impl HandlerFor<
    C,
    Params = LogsParams<Extra>,
    InheritedParams = Empty,
    Ok = LogResponse,
    Err = Error,
> + rpc_toolkit::ts::HandlerTSBindings
where
    C: Context,
    Extra: FromArgMatches + Args + TS + Send + Sync + 'static,
{
    from_fn_async(
        move |HandlerArgs {
                  context,
                  params:
                      LogsParams {
                          extra,
                          limit,
                          cursor,
                          boot,
                          before,
                      },
                  ..
              }: HandlerArgs<C, LogsParams<Extra>>| {
            let f = f.clone();
            async move {
                fetch_logs(
                    f.call(&context, extra).await?,
                    limit,
                    cursor,
                    boot.map(String::from),
                    before,
                )
                .await
            }
        },
    )
}

fn logs_follow<
    C: Context + AsRef<RpcContinuations>,
    Extra: FromArgMatches + Args + Send + Sync + 'static,
>(
    f: impl for<'a> LogSourceFn<'a, C, Extra>,
) -> impl HandlerFor<
    C,
    Params = Empty,
    InheritedParams = LogsParams<Extra>,
    Ok = LogFollowResponse,
    Err = Error,
> + rpc_toolkit::ts::HandlerTSBindings {
    from_fn_async(
        move |HandlerArgs {
                  context,
                  inherited_params:
                      LogsParams {
                          extra,
                          cursor,
                          limit,
                          boot,
                          ..
                      },
                  ..
              }: HandlerArgs<C, Empty, LogsParams<Extra>>| {
            let f = f.clone();
            async move {
                let src = f.call(&context, extra).await?;
                follow_logs(context, src, cursor, limit, boot.map(String::from)).await
            }
        },
    )
}

async fn get_package_id(
    _: &RpcContext,
    PackageIdParams { id }: PackageIdParams,
) -> Result<LogSource, Error> {
    Ok(LogSource::Package(id))
}

pub fn package_logs() -> ParentHandler<RpcContext, LogsParams<PackageIdParams>> {
    logs::<RpcContext, PackageIdParams>(get_package_id)
}

pub async fn journalctl(
    id: LogSource,
    limit: Option<usize>,
    cursor: Option<&str>,
    boot: Option<&str>,
    before: bool,
    follow: bool,
) -> Result<LogStream, Error> {
    let mut cmd = gen_journalctl_command(&id);

    if let Some(limit) = limit {
        cmd.arg(format!("--lines={}", limit));
    }

    if let Some(cursor) = cursor {
        cmd.arg(&format!("--after-cursor={}", cursor));
        if before {
            cmd.arg("--reverse");
        }
    }

    if let Some(boot) = boot {
        cmd.arg(format!("--boot={boot}"));
    } else {
        cmd.arg("--boot=all");
    }

    let deserialized_entries = String::from_utf8(cmd.invoke(ErrorKind::Journald).await?)?
        .lines()
        .map(serde_json::from_str::<JournalctlEntry>)
        .filter_map(|e| e.ok())
        .collect::<Vec<_>>();

    if follow {
        let mut follow_cmd = gen_journalctl_command(&id);
        follow_cmd.arg("-f");
        if let Some(last) = deserialized_entries.last() {
            follow_cmd.arg(format!("--after-cursor={}", last.cursor));
            follow_cmd.arg("--lines=all");
        } else {
            follow_cmd.arg("--lines=0");
        }
        let mut child = follow_cmd.stdout(Stdio::piped()).spawn()?;
        let out = BufReader::new(child.stdout.take().ok_or_else(|| {
            Error::new(
                eyre!("{}", t!("logs.no-stdout-available")),
                crate::ErrorKind::Journald,
            )
        })?);

        let journalctl_entries = LinesStream::new(out.lines());

        let follow_deserialized_entries = journalctl_entries
            .map_err(|e| Error::new(e, crate::ErrorKind::Journald))
            .try_filter_map(|s| {
                futures::future::ready(Ok(serde_json::from_str::<JournalctlEntry>(&s).ok()))
            });

        let entries = futures::stream::iter(deserialized_entries)
            .map(Ok)
            .chain(follow_deserialized_entries)
            .boxed();
        Ok(LogStream {
            _child: Some(child),
            entries,
        })
    } else {
        let entries = futures::stream::iter(deserialized_entries).map(Ok).boxed();

        Ok(LogStream {
            _child: None,
            entries,
        })
    }
}

fn gen_journalctl_command(id: &LogSource) -> Command {
    let mut cmd = Command::new("journalctl");

    cmd.kill_on_drop(true);

    cmd.arg("--output=json");
    cmd.arg("--output-fields=MESSAGE");
    match id {
        LogSource::Kernel => {
            cmd.arg("-k");
        }
        LogSource::Unit(id) => {
            cmd.arg("-u");
            cmd.arg(id);
        }
        LogSource::Package(id) => {
            cmd.arg("-u")
                .arg("container-runtime.service")
                .arg("-D")
                .arg(Path::new("/media/startos/data/package-data/logs").join(id));
        }
    };
    cmd
}

#[instrument(skip_all)]
pub async fn fetch_logs(
    id: LogSource,
    limit: Option<usize>,
    cursor: Option<String>,
    boot: Option<String>,
    before: bool,
) -> Result<LogResponse, Error> {
    let limit = limit.unwrap_or(50);
    let mut stream = journalctl(
        id,
        Some(limit),
        cursor.as_deref(),
        boot.as_deref(),
        before,
        false,
    )
    .await?;

    let mut entries = Vec::with_capacity(limit);
    let mut start_cursor = None;

    if let Some(first) = tokio::time::timeout(Duration::from_secs(1), stream.try_next())
        .await
        .ok()
        .transpose()?
        .flatten()
    {
        let (cursor, entry) = first.log_entry()?;
        start_cursor = Some(cursor);
        entries.push(entry);
    }

    let (mut end_cursor, entries) = stream
        .try_fold(
            (start_cursor.clone(), entries),
            |(_, mut acc), entry| async move {
                let (cursor, entry) = entry.log_entry()?;
                acc.push(entry);
                Ok((Some(cursor), acc))
            },
        )
        .await?;
    let mut entries = Reversible::new(entries);
    // reverse again so output is always in increasing chronological order
    if cursor.is_some() && before {
        entries.reverse();
        std::mem::swap(&mut start_cursor, &mut end_cursor);
    }
    Ok(LogResponse {
        entries,
        start_cursor,
        end_cursor,
    })
}

#[instrument(skip_all)]
pub async fn follow_logs<Context: AsRef<RpcContinuations>>(
    ctx: Context,
    id: LogSource,
    cursor: Option<String>,
    limit: Option<usize>,
    boot: Option<String>,
) -> Result<LogFollowResponse, Error> {
    let limit = if cursor.is_some() {
        None
    } else {
        Some(limit.unwrap_or(50))
    };
    let mut stream = journalctl(id, limit, cursor.as_deref(), boot.as_deref(), false, true).await?;

    let mut start_cursor = None;
    let mut first_entry = None;

    if let Some(first) = tokio::time::timeout(Duration::from_secs(1), stream.try_next())
        .await
        .ok()
        .transpose()?
        .flatten()
    {
        let (cursor, entry) = first.log_entry()?;
        start_cursor = Some(cursor);
        first_entry = Some(entry);
    }

    let guid = Guid::new();
    ctx.as_ref()
        .add(
            guid.clone(),
            RpcContinuation::ws(
                move |socket| async move {
                    if let Err(e) = ws_handler(first_entry, stream, socket).await {
                        tracing::error!(
                            "{}",
                            t!("logs.error-in-log-stream", error = e.to_string())
                        );
                    }
                },
                Duration::from_secs(30),
            ),
        )
        .await;
    Ok(LogFollowResponse { start_cursor, guid })
}

#[cfg(test)]
mod tests {
    use rpc_toolkit::ts::{Direction, TSVisitor};

    use super::*;

    #[test]
    fn log_boot_binding_matches_nullable_serde() {
        for input in [
            serde_json::json!({}),
            serde_json::json!({"boot": null}),
            serde_json::json!({"boot": -1}),
            serde_json::json!({"boot": "boot-id"}),
        ] {
            let params: LogsParams = serde_json::from_value(input.clone()).unwrap();
            let output = serde_json::to_value(params).unwrap();
            assert_eq!(
                output["boot"],
                input.get("boot").cloned().unwrap_or_default()
            );
        }
        assert!(serde_json::from_value::<LogsParams>(serde_json::json!({"boot": true})).is_err());
        for direction in [Direction::Input, Direction::Output] {
            let mut visitor = TSVisitor::new();
            visitor.with_direction(direction, |visitor| visitor.append_type::<LogsParams>());
            let schema = visitor.into_module("Logs").unwrap();
            let field = if direction == Direction::Input {
                "\"boot\"?:(number | string | null)"
            } else {
                "\"boot\":(number | string | null)"
            };
            assert!(schema.contains(field), "{schema}");
        }
    }
}
