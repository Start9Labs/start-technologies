use super::*;

mod follow_tests;

#[derive(Clone, Default)]
struct Logs(Arc<Mutex<Vec<u8>>>);

impl Write for Logs {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn emit_events() {
    tracing::trace!(target: "start_core::net::vhost", "core trace");
    tracing::debug!(target: "start_core::net::vhost", "core debug");
    tracing::info!(target: "start_core::net::vhost", "core info");
    tracing::warn!(target: "start_core::net::vhost", "core warn");
    tracing::error!(target: "start_core::net::vhost", "core error");
    tracing::trace!(target: "start_os", "os trace");
    tracing::debug!(target: "start_os", "os debug");
    tracing::info!(target: "start_os", "os info");
    tracing::warn!(target: "start_os", "os warn");
    tracing::error!(target: "start_os", "os error");
    tracing::trace!(target: "tungstenite::protocol", "transport trace");
    tracing::debug!(target: "h2::codec::framed_write", "transport debug");
    tracing::info!(target: "dependency", "dependency info");
    tracing::warn!(target: "dependency", "dependency warn");
    tracing::error!(target: "dependency", "dependency error");
}

fn capture(startup: &str, run: impl FnOnce(&LogFilter)) -> String {
    let logs = Logs::default();
    let (writer, guard) = NonBlockingBuilder::default().finish(logs.clone());
    let (subscriber, filter) = StartOSLogger::base_subscriber(writer, EnvFilter::new(startup));
    tracing::subscriber::with_default(subscriber, || run(&filter));
    drop(guard);
    String::from_utf8(logs.0.lock().unwrap().clone()).unwrap()
}

#[test]
fn runtime_levels_apply_live_to_both_startos_targets_and_exclude_dependencies() {
    let mut expected = Vec::new();
    let text = capture("trace", |filter| {
        for (level, events) in [
            (LogLevel::Error, vec!["core error", "os error"]),
            (
                LogLevel::Warn,
                vec!["core warn", "core error", "os warn", "os error"],
            ),
            (
                LogLevel::Info,
                vec![
                    "core info",
                    "core warn",
                    "core error",
                    "os info",
                    "os warn",
                    "os error",
                ],
            ),
            (
                LogLevel::Debug,
                vec![
                    "core debug",
                    "core info",
                    "core warn",
                    "core error",
                    "os debug",
                    "os info",
                    "os warn",
                    "os error",
                ],
            ),
            (
                LogLevel::Trace,
                vec![
                    "core trace",
                    "core debug",
                    "core info",
                    "core warn",
                    "core error",
                    "os trace",
                    "os debug",
                    "os info",
                    "os warn",
                    "os error",
                ],
            ),
        ] {
            filter.set_level(level).unwrap();
            emit_events();
            expected.extend(events);
        }
    });
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), expected.len(), "{text}");
    for (line, message) in lines.iter().zip(expected) {
        assert!(line.contains(message), "{text}");
    }
}

#[test]
fn a_new_logger_uses_the_startup_filter_instead_of_the_previous_runtime_level() {
    let selected = capture("start_core=error,dependency=warn", |filter| {
        filter.set_level(LogLevel::Info).unwrap();
        emit_events();
    });
    assert_eq!(selected.lines().count(), 6, "{selected}");
    assert!(selected.contains("core info"), "{selected}");
    assert!(selected.contains("os info"), "{selected}");
    assert!(!selected.contains("dependency"), "{selected}");
    let restarted = capture("start_core=error,dependency=warn", |_| emit_events());
    assert_eq!(restarted.lines().count(), 3, "{restarted}");
    assert!(restarted.contains("core error"), "{restarted}");
    assert!(restarted.contains("dependency warn"), "{restarted}");
    assert!(!restarted.contains("core info"), "{restarted}");
    assert!(!restarted.contains("os error"), "{restarted}");
}
