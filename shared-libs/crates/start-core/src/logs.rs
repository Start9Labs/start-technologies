use std::convert::Infallible;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::process::Stdio;
use std::str::FromStr;
use std::time::{Duration, UNIX_EPOCH};

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
    CallRemote, Context, Empty, HandlerArgs, HandlerExt, HandlerFor, ParentHandler, RpcStream,
    from_fn_async,
};
use serde::de::{self, DeserializeOwned};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio_stream::wrappers::LinesStream;
use tracing::instrument;

use crate::PackageId;
use crate::context::{CliContext, RpcContext};
use crate::error::ResultExt;
use crate::prelude::*;
use crate::rpc_continuations::{Guid, WebSocket, WebSocketPolicy};
use crate::util::serde::Reversible;
use crate::util::{FromStrParser, Invoke};

mod context;
pub use self::context::LogContext;
use self::context::LogEntries;

#[cfg(test)]
mod result_tests;
#[cfg(test)]
mod stream_tests;

/// Reports the followed process's failure after draining its output.
#[pin_project::pin_project]
pub struct LogStream {
    #[pin]
    entries: BoxStream<'static, Result<JournalctlEntry, Error>>,
}
impl LogStream {
    fn follow(mut child: Child, initial: Vec<JournalctlEntry>) -> Result<Self, Error> {
        let out = BufReader::new(child.stdout.take().ok_or_else(|| {
            Error::new(
                eyre!("{}", t!("logs.no-stdout-available")),
                crate::ErrorKind::Journald,
            )
        })?);
        let completed = futures::stream::once(async move {
            let output = child
                .wait_with_output()
                .await
                .with_kind(ErrorKind::Journald)?;
            crate::util::check_command_output(output, ErrorKind::Journald, "journalctl")?;
            Ok::<Option<JournalctlEntry>, Error>(None)
        })
        .try_filter_map(|entry| futures::future::ready(Ok(entry)));
        let entries = futures::stream::iter(initial)
            .map(Ok)
            .chain(
                LinesStream::new(out.lines())
                    .map_err(|e| Error::new(e, crate::ErrorKind::Journald))
                    .try_filter_map(|s| {
                        futures::future::ready(Ok(serde_json::from_str::<JournalctlEntry>(&s).ok()))
                    }),
            )
            .chain(completed)
            .boxed();
        Ok(Self { entries })
    }
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
async fn forward_log_socket(mut socket: WebSocket, entries: LogEntries) {
    let result = crate::rpc_continuations::forward_json_stream(&mut socket, entries).await;
    if let Err(error) = socket.close_result(result).await {
        if !crate::util::net::is_ws_reset_without_close(&error) {
            tracing::error!(
                "{}",
                t!("logs.error-in-log-stream", error = error.to_string())
            );
        }
    }
}

struct PreparedLogFollow {
    start_cursor: Option<String>,
    entries: LogEntries,
}

impl PreparedLogFollow {
    async fn new(mut source: LogStream) -> Result<Self, Error> {
        let first = tokio::time::timeout(Duration::from_secs(1), source.try_next())
            .await
            .ok()
            .transpose()?
            .flatten()
            .map(JournalctlEntry::log_entry)
            .transpose()?;
        let start_cursor = first.as_ref().map(|(cursor, _)| cursor.clone());
        let entries = futures::stream::iter(first.map(|(_, entry)| Ok(entry)))
            .chain(
                source
                    .map(|entry| entry.and_then(|entry| entry.log_entry().map(|(_, entry)| entry))),
            )
            .boxed();
        Ok(Self {
            start_cursor,
            entries,
        })
    }

    async fn subscribe(
        self,
        context: impl LogContext,
        signer: Option<InternedString>,
        policy: WebSocketPolicy,
        stream: Option<RpcStream<LogEntry>>,
    ) -> Result<LogFollowResponse, Error> {
        let guid = if let Some(stream) = stream {
            context.forward_logs(signer, stream, self.entries).await?;
            None
        } else {
            let guid = Guid::new();
            let continuation = context
                .log_continuation(signer, policy, self.entries)
                .await?;
            context.as_ref().add(guid.clone(), continuation).await;
            Some(guid)
        };
        Ok(LogFollowResponse {
            start_cursor: self.start_cursor,
            guid,
        })
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
    /// Present for ordinary replies; typed observations use same-ID events.
    guid: Option<Guid>,
}

rpc_toolkit::reflect_ts!(LogFollowResponse);
rpc_toolkit::ts_export!(LogFollowResponse, namespaces = [""]);

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
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
rpc_toolkit::impl_ts!(BootIdentifier => &format!("z.union([z.number().int().min({}).max({}), z.string()])", i32::MIN, i32::MAX), "z.ZodUnion<readonly [z.ZodNumber, z.ZodString]>");
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
                i32::try_from(v)
                    .map(Self::Value::Index)
                    .map_err(|_| E::invalid_value(de::Unexpected::Signed(v), &self))
            }
            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if v.is_finite()
                    && v.fract() == 0.0
                    && v >= f64::from(i32::MIN)
                    && v <= f64::from(i32::MAX)
                {
                    Ok(Self::Value::Index(v as i32))
                } else {
                    Err(E::invalid_value(de::Unexpected::Float(v), &self))
                }
            }
            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                i32::try_from(v)
                    .map(Self::Value::Index)
                    .map_err(|_| E::invalid_value(de::Unexpected::Unsigned(v), &self))
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
    boot: Option<BootIdentifier>,
    #[arg(
        short = 'B',
        long = "before",
        conflicts_with = "follow",
        help = "help.arg.log-before"
    )]
    #[serde(default)]
    before: bool,
    #[arg(skip)]
    #[visit(ts(skip))]
    #[serde(rename = "__Auth_signer")]
    signer: Option<InternedString>,
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
    C: LogContext,
    Extra: FromArgMatches + Serialize + DeserializeOwned + Args + TS + Send + Sync + 'static,
>(
    source: impl for<'a> LogSourceFn<'a, C, Extra>,
    policy: WebSocketPolicy,
) -> ParentHandler<C, LogsParams<Extra>> {
    ParentHandler::new()
        .root_handler(logs_nofollow::<C, Extra>(source.clone()).no_cli())
        .subcommand(
            "follow",
            logs_follow::<C, Extra>(source, policy)
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
    RemoteContext: crate::context::cli::CliWebSocketContext,
    Extra: FromArgMatches + Args + Serialize + Send + Sync,
{
    let method = parent_method
        .into_iter()
        .chain(method)
        .chain(follow.then_some("follow"))
        .join(".");

    if follow {
        let response =
            RemoteContext::stream_cli::<LogEntry, ()>(&ctx, &method, to_value(&rpc_params)?)
                .await?;
        let _: LogFollowResponse = from_value(response.initial)?;
        let mut entries = response.items;
        while let Some(entry) = entries.next().await? {
            println!("{}", entry);
        }
        entries.finish().await?;
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
                          ..
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

fn logs_follow<C: LogContext, Extra: FromArgMatches + Args + Send + Sync + 'static>(
    f: impl for<'a> LogSourceFn<'a, C, Extra>,
    policy: WebSocketPolicy,
) -> impl HandlerFor<
    C,
    Params = Empty,
    InheritedParams = LogsParams<Extra>,
    Ok = LogFollowResponse,
    Err = Error,
    Item = LogEntry,
    Final = (),
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
                          signer,
                          ..
                      },
                  rpc_stream,
                  ..
              }: HandlerArgs<C, Empty, LogsParams<Extra>, LogEntry>| {
            let f = f.clone();
            async move {
                let src = f.call(&context, extra).await?;
                follow_logs(
                    context,
                    src,
                    cursor,
                    limit,
                    boot.map(String::from),
                    policy,
                    signer,
                    rpc_stream,
                )
                .await
            }
        },
    )
    .with_metadata(crate::mcp::MCP, Value::Bool(false))
    .with_metadata("get_signer", Value::Bool(true))
}

async fn get_package_id(
    _: &RpcContext,
    PackageIdParams { id }: PackageIdParams,
) -> Result<LogSource, Error> {
    Ok(LogSource::Package(id))
}

pub fn package_logs() -> ParentHandler<RpcContext, LogsParams<PackageIdParams>> {
    logs::<RpcContext, PackageIdParams>(get_package_id, WebSocketPolicy::Authenticated)
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
        let child = follow_cmd.stdout(Stdio::piped()).spawn()?;
        LogStream::follow(child, deserialized_entries)
    } else {
        let entries = futures::stream::iter(deserialized_entries).map(Ok).boxed();

        Ok(LogStream { entries })
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
    let mut entries = Reversible::<LogEntry>::new(entries);
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
pub async fn follow_logs<Context: LogContext>(
    ctx: Context,
    id: LogSource,
    cursor: Option<String>,
    limit: Option<usize>,
    boot: Option<String>,
    policy: WebSocketPolicy,
    signer: Option<InternedString>,
    rpc_stream: Option<RpcStream<LogEntry>>,
) -> Result<LogFollowResponse, Error> {
    let limit = if cursor.is_some() {
        None
    } else {
        Some(limit.unwrap_or(50))
    };
    let source = journalctl(id, limit, cursor.as_deref(), boot.as_deref(), false, true).await?;
    PreparedLogFollow::new(source)
        .await?
        .subscribe(ctx, signer, policy, rpc_stream)
        .await
}

#[cfg(all(test, feature = "ts"))]
mod tests {
    use rpc_toolkit::ts::TSVisitor;

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
        let mut visitor = TSVisitor::new();
        visitor.declare::<LogsParams>("Logs");
        let schema = visitor.into_bindings(None, "./schemas").unwrap().schemas;
        assert!(
            schema.contains(
                "\"boot\": wire(z.union([z.number().int().min(-2147483648).max(2147483647), z.string()]).nullable().optional(), z.union([z.number().int().min(-2147483648).max(2147483647), z.string()]).nullable())"
            ),
            "{schema}"
        );
    }
}
