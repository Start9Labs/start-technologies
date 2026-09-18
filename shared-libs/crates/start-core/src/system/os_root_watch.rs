use std::fs::OpenOptions;
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::disk::OS_ROOT_MOUNT;

const INTERVAL: Duration = Duration::from_secs(30);
const FAILURES_BEFORE_CRASH: u32 = 4;

#[repr(align(4096))]
struct Sector([u8; 4096]);

fn os_root_device() -> Option<PathBuf> {
    std::fs::read_to_string("/proc/mounts")
        .ok()?
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?, fields.next()?))
        })
        .find(|(_, mount)| *mount == OS_ROOT_MOUNT)
        .map(|(source, _)| PathBuf::from(source))
}

fn read_direct(dev: &Path) -> std::io::Result<()> {
    let mut sector = Sector([0; 4096]);
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECT)
        .open(dev)?
        .read_exact_at(&mut sector.0, 0)
}

fn crash(reason: &str) {
    let _ = std::fs::write(
        "/dev/kmsg",
        format!("<3>startd: {reason}; crashing to reboot\n"),
    );
    for key in ["s", "c", "b"] {
        if let Err(e) = std::fs::write("/proc/sysrq-trigger", key) {
            tracing::error!("sysrq {key}: {e}");
        }
        std::thread::sleep(Duration::from_secs(5));
    }
}

/// Crashes the kernel once the OS root partition has been unreadable for
/// `FAILURES_BEFORE_CRASH` consecutive checks.
pub async fn watch_os_root() {
    let mut failures = 0;
    loop {
        tokio::time::sleep(INTERVAL).await;
        let Some(dev) = os_root_device() else {
            continue;
        };
        let path = dev.clone();
        let err = match tokio::time::timeout(
            INTERVAL,
            tokio::task::spawn_blocking(move || read_direct(&path)),
        )
        .await
        {
            Ok(Ok(Ok(()))) => {
                failures = 0;
                continue;
            }
            Ok(Ok(Err(e))) => e.to_string(),
            Ok(Err(e)) => e.to_string(),
            Err(_) => format!("no response within {}s", INTERVAL.as_secs()),
        };
        failures += 1;
        tracing::error!(
            "OS root partition {} unreadable ({failures}/{FAILURES_BEFORE_CRASH}): {err}",
            dev.display()
        );
        if failures >= FAILURES_BEFORE_CRASH {
            crash(&format!(
                "OS root partition {} unreadable for {failures} consecutive checks",
                dev.display()
            ));
        }
    }
}
