use std::fs::File;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tracing::Subscriber;
use tracing_appender::non_blocking::{NonBlocking, NonBlockingBuilder, WorkerGuard};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry, reload};

lazy_static! {
    pub static ref LOGGER: StartOSLogger = StartOSLogger::init();
}

/// Contested by the appender thread and `set_logfile`, never Tokio workers.
#[derive(Clone, Default)]
struct SharedLogFile(Arc<Mutex<Option<File>>>);

/// Writes from the appender thread, never a Tokio worker.
struct TeeWriter {
    file: SharedLogFile,
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Ok(mut guard) = self.file.0.lock() {
            if let Some(f) = guard.as_mut() {
                let _ = f.write_all(buf);
            }
        }
        let _ = io::stderr().write_all(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Ok(mut guard) = self.file.0.lock() {
            if let Some(f) = guard.as_mut() {
                let _ = f.flush();
            }
        }
        let _ = io::stderr().flush();
        Ok(())
    }
}

const LOG_TARGETS: [&str; 2] = [env!("CARGO_CRATE_NAME"), "start_os"];

/// Verbosity accepted by the server's runtime logging control.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Deserialize,
    Serialize,
    clap::ValueEnum,
    visit_rs::VisitVariants,
)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

rpc_toolkit::reflect_ts!(LogLevel);

impl From<LogLevel> for tracing::Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => Self::ERROR,
            LogLevel::Warn => Self::WARN,
            LogLevel::Info => Self::INFO,
            LogLevel::Debug => Self::DEBUG,
            LogLevel::Trace => Self::TRACE,
        }
    }
}

#[derive(Clone)]
struct LogFilter {
    reload: reload::Handle<EnvFilter, Registry>,
}

impl LogFilter {
    fn set_level(&self, level: LogLevel) -> Result<(), reload::Error> {
        let level = tracing::Level::from(level);
        let directives = LOG_TARGETS
            .map(|target| format!("{target}={level}"))
            .join(",");
        self.reload.reload(EnvFilter::new(directives))
    }
}

#[derive(Clone)]
pub struct StartOSLogger {
    logfile: SharedLogFile,
    filter: LogFilter,
    _guard: Arc<Mutex<Option<WorkerGuard>>>,
}

impl StartOSLogger {
    pub fn enable(&self) {}

    pub fn set_logfile(&self, logfile: Option<File>) {
        *self.logfile.0.lock().unwrap() = logfile;
    }

    /// Sets StartOS backend verbosity until the daemon restarts.
    pub fn set_log_level(&self, level: LogLevel) -> Result<(), reload::Error> {
        self.filter.set_level(level)
    }

    fn base_subscriber(writer: NonBlocking, startup: EnvFilter) -> (impl Subscriber, LogFilter) {
        use tracing_error::ErrorLayer;
        use tracing_subscriber::fmt;
        use tracing_subscriber::prelude::*;

        let (filter, reload) = reload::Layer::new(startup);
        let fmt_layer = fmt::layer()
            .with_writer(writer)
            .with_line_number(true)
            .with_file(true)
            .with_target(true)
            // Colored log messages contain ANSI escapes.
            .with_ansi_sanitization(false)
            .with_filter(filter);

        let sub = tracing_subscriber::registry().with(fmt_layer);

        #[cfg(feature = "console-subscriber")]
        let sub = sub.with(console_subscriber::spawn());

        let sub = sub.with(ErrorLayer::default());

        (sub, LogFilter { reload })
    }

    fn init() -> Self {
        let logfile = SharedLogFile::default();
        let (writer, guard) = NonBlockingBuilder::default()
            .lossy(true)
            .buffered_lines_limit(65_536)
            .thread_name("startos-logger".into())
            .finish(TeeWriter {
                file: logfile.clone(),
            });
        let startup = EnvFilter::builder()
            .with_default_directive(format!("{}=info", LOG_TARGETS[0]).parse().unwrap())
            .from_env_lossy();
        let (subscriber, filter) = Self::base_subscriber(writer, startup);
        subscriber.init();
        color_eyre::install().unwrap_or_else(|_| tracing::warn!("tracing too many times"));

        StartOSLogger {
            logfile,
            filter,
            _guard: Arc::new(Mutex::new(Some(guard))),
        }
    }
}

#[cfg(test)]
#[path = "logger/tests.rs"]
mod tests;
