//! Secret handling while the caller excludes concurrent swap/core-handler changes.
//!
//! Child tools inherit the zero core limit. Exec resets dumpability and requires
//! a non-piped core handler throughout secret-bearing subprocess execution.

use std::path::Path;

use crate::db::model::DatabaseModel;
use crate::prelude::*;
use crate::util::io::TmpDir;

pub(crate) struct ProcessSecretMemory(());
pub struct InitramfsMemory(ProcessSecretMemory);

fn unsafe_memory() -> Error {
    Error::new(eyre!(t!("fde.memory.unsafe-environment")), ErrorKind::Bios)
}

/// Signing and disk authority suppress swap independently of the saved ZRAM preference.
pub(crate) fn requires_protection(db: &DatabaseModel) -> bool {
    let private = db.as_private();
    private.as_fde_enrollment().transpose_ref().is_some()
        || private.as_luks_credentials().transpose_ref().is_some()
}

/// Call before migrations or deserializing key-bearing records.
pub(crate) fn protect_authorities(
    db: &DatabaseModel,
) -> Result<Option<ProcessSecretMemory>, Error> {
    requires_protection(db)
        .then(ProcessSecretMemory::protect)
        .transpose()
}

fn check_kernel_state(swaps: &str, core_pattern: &str) -> Result<(), Error> {
    if crate::system::swap_devices(swaps).next().is_some() || core_pattern.starts_with('|') {
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
        assert!(
            check_kernel_state(
                &format!("{header}/dev/zram0 partition 1024 0 5\n"),
                "core\n"
            )
            .is_err()
        );
        assert!(check_kernel_state(header, "|/usr/lib/systemd/systemd-coredump %P\n").is_err());
    }

    #[test]
    fn ordinary_zram_preference_does_not_impose_fde_memory_policy() {
        let account = crate::account::AccountInfo::new(
            "ordinary-memory-test-password",
            std::time::SystemTime::now(),
            None,
        )
        .unwrap();
        let initial = crate::db::model::Database::init(&account, false, None, None).unwrap();
        let mut db = crate::db::prelude::Model::new(&initial).unwrap();
        for preference in [false, true] {
            db.as_public_mut()
                .as_server_info_mut()
                .as_zram_mut()
                .ser(&preference)
                .unwrap();
            assert!(!requires_protection(&db));
            assert!(protect_authorities(&db).unwrap().is_none());
        }
    }

    #[tokio::test]
    async fn untyped_authority_detection_does_not_decode_private_keys() {
        use patch_db::json_ptr::ROOT;

        use crate::db::model::Database;

        let directory = tempfile::tempdir().unwrap();
        let raw = PatchDb::open(directory.path().join("untyped.db"))
            .await
            .unwrap();
        for field in ["fdeEnrollment", "luksCredentials"] {
            let mut value = serde_json::json!({"private": {}});
            value["private"][field] = serde_json::json!({"malformed": "key-bearing record"});
            raw.put(&ROOT, &value).await.unwrap();
            let db = TypedPatchDb::<Database>::load_unchecked(raw.clone());
            assert!(requires_protection(&db.peek().await));
            assert!(TypedPatchDb::<Database>::load(raw.clone()).await.is_err());
        }
        raw.put(&ROOT, &serde_json::json!({"private": {}}))
            .await
            .unwrap();
        let db = TypedPatchDb::<Database>::load_unchecked(raw);
        assert!(!requires_protection(&db.peek().await));
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
