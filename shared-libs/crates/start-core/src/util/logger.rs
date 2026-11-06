use std::fs::File;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use lazy_static::lazy_static;
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

#[derive(Clone)]
struct TraceFilter {
    startup: EnvFilter,
    reload: reload::Handle<EnvFilter, Registry>,
}

impl TraceFilter {
    fn set_trace(&self, enabled: bool) -> Result<(), reload::Error> {
        self.reload.reload(if enabled {
            EnvFilter::new("trace")
        } else {
            self.startup.clone()
        })
    }
}

#[derive(Clone)]
pub struct StartOSLogger {
    logfile: SharedLogFile,
    filter: TraceFilter,
    _guard: Arc<Mutex<Option<WorkerGuard>>>,
}

impl StartOSLogger {
    pub fn enable(&self) {}

    pub fn set_logfile(&self, logfile: Option<File>) {
        *self.logfile.0.lock().unwrap() = logfile;
    }

    /// Enables process-wide trace output; disabling restores the startup filter.
    pub fn set_trace(&self, enabled: bool) -> Result<(), reload::Error> {
        self.filter.set_trace(enabled)
    }

    fn base_subscriber(writer: NonBlocking, startup: EnvFilter) -> (impl Subscriber, TraceFilter) {
        use tracing_error::ErrorLayer;
        use tracing_subscriber::fmt;
        use tracing_subscriber::prelude::*;

        let (filter, reload) = reload::Layer::new(startup.clone());
        let fmt_layer = fmt::layer()
            .with_writer(writer)
            .with_line_number(true)
            .with_file(true)
            .with_target(true)
            // 0.3.20's ANSI-injection guard (tokio-rs/tracing#3368) escapes ESC
            // unconditionally, mangling our colored logs (#3369); opt out — these
            // are our own trusted logs. Toggle added in 0.3.23 (#3484).
            .with_ansi_sanitization(false)
            .with_filter(filter);

        let sub = tracing_subscriber::registry().with(fmt_layer);

        #[cfg(feature = "console-subscriber")]
        let sub = sub.with(console_subscriber::spawn());

        let sub = sub.with(ErrorLayer::default());

        (sub, TraceFilter { startup, reload })
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
            .with_default_directive(
                format!("{}=info", std::module_path!().split("::").next().unwrap())
                    .parse()
                    .unwrap(),
            )
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
