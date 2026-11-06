use super::*;

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

fn emit_events(phase: &str) {
    tracing::trace!(target: "start_core::net::vhost", phase, "own trace");
    tracing::debug!(target: "start_core::net::vhost", phase, "own debug");
    tracing::info!(target: "start_core::net::vhost", phase, "own info");
    tracing::trace!(target: "dependency", phase, "dependency trace");
    tracing::warn!(target: "dependency", phase, "dependency warning");
}

#[test]
fn trace_override_is_live_process_wide_and_restores_startup_directives() {
    let logs = Logs::default();
    let (writer, guard) = NonBlockingBuilder::default().finish(logs.clone());
    let (subscriber, filter) =
        StartOSLogger::base_subscriber(writer, EnvFilter::new("start_core=info,dependency=warn"));
    tracing::subscriber::with_default(subscriber, || {
        emit_events("startup");
        filter.set_trace(true).unwrap();
        emit_events("enabled");
        filter.set_trace(true).unwrap();
        emit_events("enabled-again");
        filter.set_trace(false).unwrap();
        emit_events("restored");
        filter.set_trace(false).unwrap();
        emit_events("restored-again");
    });
    drop(guard);
    let text = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    for phase in ["startup", "restored", "restored-again"] {
        let lines: Vec<_> = text
            .lines()
            .filter(|line| line.contains(&format!("\"{phase}\"")))
            .collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines.iter().any(|line| line.contains("own info")), "{text}");
        assert!(
            lines.iter().any(|line| line.contains("dependency warning")),
            "{text}"
        );
    }
    for phase in ["enabled", "enabled-again"] {
        let lines: Vec<_> = text
            .lines()
            .filter(|line| line.contains(&format!("\"{phase}\"")))
            .collect();
        assert_eq!(lines.len(), 5, "{text}");
        assert!(
            lines.iter().any(|line| line.contains("own trace")),
            "{text}"
        );
        assert!(
            lines.iter().any(|line| line.contains("dependency trace")),
            "{text}"
        );
    }
}

#[test]
fn disabling_the_override_preserves_explicit_startup_trace_logging() {
    let logs = Logs::default();
    let (writer, guard) = NonBlockingBuilder::default().finish(logs.clone());
    let (subscriber, filter) =
        StartOSLogger::base_subscriber(writer, EnvFilter::new("start_core=error,dependency=trace"));
    tracing::subscriber::with_default(subscriber, || {
        filter.set_trace(true).unwrap();
        filter.set_trace(false).unwrap();
        emit_events("restored");
    });
    drop(guard);
    let text = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert!(!text.contains("own trace"), "{text}");
    assert!(!text.contains("own info"), "{text}");
    assert!(text.contains("dependency trace"), "{text}");
}
