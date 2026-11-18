#[cfg(target_os = "linux")]
use std::time::Duration;

use crate::prelude::*;

#[cfg(target_os = "linux")]
pub(crate) struct LxcControl {
    kill: std::fs::File,
    events: tokio::io::unix::AsyncFd<std::fs::File>,
}

#[cfg(target_os = "linux")]
impl LxcControl {
    pub(crate) fn open(guid: &str) -> Result<Self, Error> {
        use nix::fcntl::{OFlag, openat};
        use nix::sys::stat::Mode;
        use tokio::io::Interest;

        let directory = crate::disk::mount::filesystem::syscall::open_beneath(
            std::path::Path::new("/sys/fs/cgroup"),
            &super::payload_cgroup(guid),
        )?;
        let kill = openat(
            &directory,
            "cgroup.kill",
            OFlag::O_WRONLY | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .with_kind(ErrorKind::Lxc)?;
        let events = openat(
            &directory,
            "cgroup.events",
            OFlag::O_RDONLY | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .with_kind(ErrorKind::Lxc)?;
        Ok(Self {
            kill: kill.into(),
            // File owns this descriptor until AsyncFd releases it.
            events: unsafe {
                tokio::io::unix::AsyncFd::register_with_interest(
                    std::fs::File::from(events),
                    Interest::PRIORITY,
                )
            }
            .with_kind(ErrorKind::Lxc)?,
        })
    }

    pub(crate) fn is_stopped(&self) -> Result<bool, Error> {
        use std::os::unix::fs::FileExt;
        let mut bytes = [0u8; 1024];
        let size = match self.events.get_ref().read_at(&mut bytes, 0) {
            Ok(size) => size,
            // The kernel permits removing a cgroup after its tasks have exited.
            Err(error) if matches!(error.raw_os_error(), Some(libc::ENODEV | libc::ENOENT)) => {
                return Ok(true);
            }
            Err(error) => return Err(error.into()),
        };
        population_state(&bytes[..size])
    }

    /// Success excludes every live process in this incarnation's payload subtree.
    /// Pinned cgroup files remain tied to that incarnation through concurrent teardown.
    pub(crate) async fn kill_and_wait(&self) -> Result<(), Error> {
        use std::io::Write;

        use tokio::io::Interest;

        if self.is_stopped()? {
            return Ok(());
        }
        let mut kill = &self.kill;
        if let Err(error) = kill.write_all(b"1") {
            if !self.is_stopped()? {
                return Err(error.into());
            }
        }
        tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                if self.is_stopped()? {
                    return Ok::<_, Error>(());
                }
                let mut ready = self
                    .events
                    .ready(Interest::PRIORITY)
                    .await
                    .with_kind(ErrorKind::Lxc)?;
                ready.clear_ready();
            }
        })
        .await
        .with_kind(ErrorKind::Timeout)?
    }
}

fn state_error() -> Error {
    Error::new(eyre!(t!("lxc.control.state-unavailable")), ErrorKind::Lxc)
}

#[cfg(target_os = "linux")]
fn population_state(bytes: &[u8]) -> Result<bool, Error> {
    for line in bytes.split(|byte| *byte == b'\n') {
        match line {
            b"populated 0" => return Ok(true),
            b"populated 1" => return Ok(false),
            _ => {}
        }
    }
    Err(state_error())
}

#[cfg(not(target_os = "linux"))]
pub(crate) struct LxcControl;

#[cfg(not(target_os = "linux"))]
impl LxcControl {
    pub(crate) fn open(_: &str) -> Result<Self, Error> {
        Err(state_error())
    }
    pub(crate) fn is_stopped(&self) -> Result<bool, Error> {
        Ok(true)
    }
    pub(crate) async fn kill_and_wait(&self) -> Result<(), Error> {
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn population_requires_an_explicit_kernel_state() {
        assert!(population_state(b"populated 0\nfrozen 1\n").unwrap());
        assert!(!population_state(b"populated 1\nfrozen 0\n").unwrap());
        for state in [
            b"".as_slice(),
            b"frozen 0\n",
            b"populated 9\n",
            b"populated 0 extra\n",
        ] {
            assert!(population_state(state).is_err());
        }
    }
}
