//! Secret handling while the caller excludes concurrent swap/core-handler changes.
//!
//! Child tools inherit the zero core limit. Exec resets dumpability and requires
//! a non-piped core handler throughout secret-bearing subprocess execution.

use std::path::Path;

use crate::prelude::*;
use crate::util::io::TmpDir;

pub(crate) struct ProcessSecretMemory(());
pub struct InitramfsMemory(ProcessSecretMemory);

fn unsafe_memory() -> Error {
    Error::new(eyre!(t!("fde.memory.unsafe-environment")), ErrorKind::Bios)
}

fn check_kernel_state(swaps: &str, core_pattern: &str) -> Result<(), Error> {
    if swaps.lines().skip(1).any(|line| !line.trim().is_empty()) || core_pattern.starts_with('|') {
        return Err(unsafe_memory());
    }
    Ok(())
}

pub(crate) fn require_tmpfs(directory: &Path) -> Result<(), Error> {
    #[cfg(target_os = "linux")]
    {
        let fs = nix::sys::statfs::statfs(directory).with_kind(ErrorKind::Filesystem)?;
        if fs.filesystem_type() != nix::sys::statfs::TMPFS_MAGIC {
            return Err(unsafe_memory());
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = directory;
        Err(unsafe_memory())
    }
}

impl ProcessSecretMemory {
    #[cfg(target_os = "linux")]
    fn check_environment() -> Result<(), Error> {
        check_kernel_state(
            &std::fs::read_to_string("/proc/swaps")?,
            &std::fs::read_to_string("/proc/sys/kernel/core_pattern")?,
        )
    }

    pub(crate) fn protect() -> Result<Self, Error> {
        #[cfg(target_os = "linux")]
        {
            Self::check_environment()?;
            let limit = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) } != 0
                || unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } != 0
            {
                return Err(Error::new(std::io::Error::last_os_error(), ErrorKind::Bios));
            }
            let protected = Self(());
            protected.check()?;
            Ok(protected)
        }
        #[cfg(not(target_os = "linux"))]
        Err(unsafe_memory())
    }

    pub(crate) fn check(&self) -> Result<(), Error> {
        #[cfg(target_os = "linux")]
        {
            Self::check_environment()?;
            let mut limit = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if unsafe { libc::getrlimit(libc::RLIMIT_CORE, &mut limit) } != 0 {
                return Err(Error::new(std::io::Error::last_os_error(), ErrorKind::Bios));
            }
            if limit.rlim_cur != 0
                || limit.rlim_max != 0
                || unsafe { libc::prctl(libc::PR_GET_DUMPABLE, 0, 0, 0, 0) } != 0
            {
                return Err(unsafe_memory());
            }
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        Err(unsafe_memory())
    }
}

impl InitramfsMemory {
    /// Protects this helper and child tools before bootstrap secrets enter memory.
    pub fn protect(runtime: &Path) -> Result<Self, Error> {
        Self::check_filesystems(runtime)?;
        let protected = Self(ProcessSecretMemory::protect()?);
        protected.check(runtime)?;
        Ok(protected)
    }

    fn check_filesystems(runtime: &Path) -> Result<(), Error> {
        for directory in [runtime, Path::new(TmpDir::BASE)] {
            require_tmpfs(directory)?;
        }
        Ok(())
    }

    pub fn check(&self, runtime: &Path) -> Result<(), Error> {
        Self::check_filesystems(runtime)?;
        self.0.check()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_swap_and_piped_dump_handlers_reject_secret_release() {
        let header = "Filename Type Size Used Priority\n";
        assert!(check_kernel_state(header, "core\n").is_ok());
        assert!(
            check_kernel_state(&format!("{header}/swapfile file 1024 0 -2\n"), "core\n").is_err()
        );
        assert!(check_kernel_state(header, "|/usr/lib/systemd/systemd-coredump %P\n").is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires RAM-backed /var/tmp/startos, no swap or piped core handler; changes process limits"]
    fn initramfs_protection_survives_exec_and_rechecks_dumpability() {
        std::fs::create_dir_all(TmpDir::BASE).unwrap();
        let temp = tempfile::tempdir_in("/dev/shm").unwrap();
        let protected = InitramfsMemory::protect(temp.path()).unwrap();
        let output = std::process::Command::new("sh")
            .args(["-c", "ulimit -Sc; ulimit -Hc"])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"0\n0\n");
        assert_eq!(unsafe { libc::prctl(libc::PR_GET_DUMPABLE, 0, 0, 0, 0) }, 0);
        assert_eq!(unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 1, 0, 0, 0) }, 0);
        assert!(protected.check(temp.path()).is_err());
    }
}
