//! The `system.boot` activity entry: one per boot, with the firmware version
//! and what ended the run before it, when the router recorded that.

use std::io::Write;
use std::path::Path;

/// The daemon's own account of its run, kept across reboots.
const RUN_STATE: &str = "/var/lib/startwrt/run-state";
/// Exists once this boot has its entry. In RAM.
const BOOT_LOGGED: &str = "/var/run/startwrt-boot-logged";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    /// Initializing; a run that ends here ended for an unknown reason.
    Starting,
    Running,
    Stopped,
    Restarting,
    Restoring,
}

impl RunState {
    fn as_str(self) -> &'static str {
        match self {
            RunState::Starting => "starting",
            RunState::Running => "running",
            RunState::Stopped => "stopped",
            RunState::Restarting => "restarting",
            RunState::Restoring => "restoring",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s.trim() {
            "starting" => RunState::Starting,
            "running" => RunState::Running,
            "stopped" => RunState::Stopped,
            "restarting" => RunState::Restarting,
            "restoring" => RunState::Restoring,
            _ => return None,
        })
    }

    /// Why a run that ended in this state led to a boot.
    fn reboot_cause(self) -> Option<&'static str> {
        match self {
            RunState::Starting => None,
            RunState::Running => Some("a power loss or crash"),
            RunState::Stopped => Some("a clean shutdown"),
            RunState::Restarting => Some("a restart from StartWRT"),
            RunState::Restoring => Some("restoring a backup"),
        }
    }
}

/// Logs `system.boot` on the daemon's first start in a boot, then marks the
/// run as starting. An applied firmware update takes precedence over the
/// recorded state.
pub fn record_start(update_applied: bool) {
    if let Some(summary) = start_at(Path::new(RUN_STATE), Path::new(BOOT_LOGGED), update_applied) {
        crate::activity::log("system", "boot", true, &summary, None);
    }
}

/// Marks the run as fully started.
pub fn record_running() {
    write_state(Path::new(RUN_STATE), RunState::Running);
}

/// Marks the run as stopped on request, unless a reboot's cause is already
/// recorded.
pub fn record_stop() {
    stop_at(Path::new(RUN_STATE));
}

/// Records why the router is about to reboot.
pub fn record_reboot(cause: RunState) {
    write_state(Path::new(RUN_STATE), cause);
}

fn start_at(run_state: &Path, boot_logged: &Path, update_applied: bool) -> Option<String> {
    let previous = read_state(run_state);
    write_state(run_state, RunState::Starting);
    if boot_logged.exists() {
        return None;
    }
    if let Err(e) = std::fs::write(boot_logged, "") {
        tracing::warn!("failed to mark this boot as logged: {e}");
    }
    let cause = if update_applied {
        Some("a firmware update")
    } else {
        previous.and_then(RunState::reboot_cause)
    };
    let mut summary = format!(
        "Booted v{} ({})",
        env!("CARGO_PKG_VERSION"),
        short_hash(env!("STARTWRT_GIT_HASH"))
    );
    if let Some(cause) = cause {
        summary.push_str(" after ");
        summary.push_str(cause);
    }
    Some(summary)
}

fn stop_at(path: &Path) {
    if read_state(path) == Some(RunState::Running) {
        write_state(path, RunState::Stopped);
    }
}

fn read_state(path: &Path) -> Option<RunState> {
    RunState::parse(&std::fs::read_to_string(path).ok()?)
}

/// Durable before it returns: the state must outlive a power loss right after.
fn write_state(path: &Path, state: RunState) {
    if let Err(e) = write_durably(path, state.as_str()) {
        tracing::warn!("failed to record run state {}: {e}", state.as_str());
    }
}

fn write_durably(path: &Path, contents: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("/"));
    std::fs::create_dir_all(dir)?;
    let tmp = path.with_extension("tmp");
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&tmp, path)?;
    std::fs::File::open(dir)?.sync_all()
}

/// A commit hash shortened to 9 digits, keeping any `-modified` mark.
fn short_hash(hash: &str) -> String {
    let (commit, mark) = match hash.split_once('-') {
        Some((commit, mark)) => (commit, Some(mark)),
        None => (hash, None),
    };
    if commit.len() <= 9 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return hash.to_string();
    }
    match mark {
        Some(mark) => format!("{}-{mark}", &commit[..9]),
        None => commit[..9].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boot(dir: &Path, update_applied: bool) -> Option<String> {
        start_at(
            &dir.join("run-state"),
            &dir.join("boot-logged"),
            update_applied,
        )
    }

    fn new_boot(dir: &Path) {
        std::fs::remove_file(dir.join("boot-logged")).unwrap();
    }

    fn cause(summary: Option<String>) -> Option<String> {
        summary
            .expect("a boot entry")
            .split_once(" after ")
            .map(|(_, cause)| cause.to_string())
    }

    #[test]
    fn each_boot_names_how_the_previous_run_ended() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("run-state");

        assert_eq!(cause(boot(dir.path(), false)), None);
        assert_eq!(read_state(&state), Some(RunState::Starting));

        new_boot(dir.path());
        assert_eq!(
            cause(boot(dir.path(), false)),
            None,
            "never finished starting"
        );

        write_durably(&state, "running").unwrap();
        new_boot(dir.path());
        assert_eq!(
            cause(boot(dir.path(), false)).as_deref(),
            Some("a power loss or crash")
        );

        write_durably(&state, "running").unwrap();
        stop_at(&state);
        new_boot(dir.path());
        assert_eq!(
            cause(boot(dir.path(), false)).as_deref(),
            Some("a clean shutdown")
        );

        write_durably(&state, "restarting").unwrap();
        stop_at(&state);
        new_boot(dir.path());
        assert_eq!(
            cause(boot(dir.path(), false)).as_deref(),
            Some("a restart from StartWRT")
        );

        write_durably(&state, "restoring").unwrap();
        new_boot(dir.path());
        assert_eq!(
            cause(boot(dir.path(), false)).as_deref(),
            Some("restoring a backup")
        );

        write_durably(&state, "running").unwrap();
        new_boot(dir.path());
        assert_eq!(
            cause(boot(dir.path(), true)).as_deref(),
            Some("a firmware update")
        );
    }

    #[test]
    fn a_daemon_restart_within_a_boot_logs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(boot(dir.path(), false).is_some());
        assert_eq!(boot(dir.path(), false), None);
        assert_eq!(
            read_state(&dir.path().join("run-state")),
            Some(RunState::Starting)
        );
    }

    #[test]
    fn unreadable_state_names_no_cause() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("run-state"), "garbage").unwrap();
        assert_eq!(cause(boot(dir.path(), false)), None);
    }

    #[test]
    fn hashes_shorten_to_nine_digits() {
        assert_eq!(
            short_hash("a64bbe71ed041f2864c40588275425a8b9e500a0"),
            "a64bbe71e"
        );
        assert_eq!(
            short_hash("a64bbe71ed041f2864c40588275425a8b9e500a0-modified"),
            "a64bbe71e-modified"
        );
        assert_eq!(
            short_hash("@wrt/activity-log-gaps"),
            "@wrt/activity-log-gaps"
        );
        assert_eq!(short_hash("unknown"), "unknown");
    }
}
