//! Root's initial-user-namespace keyring retains keys until reboot, without a timeout.
//! Opening hardens the whole root user keyring against inherited possessor access.
//! Callers retain OS root credentials and protect transient userspace key copies.

use std::ffi::{CStr, CString};
use std::io;

use zeroize::Zeroizing;

use super::{Error, ErrorKind, VolumeId, metadata_error};

const KEY_USR_ALL: libc::c_long = 0x003f0000;

pub(super) struct RootKeyring(libc::c_long);

fn checked(result: libc::c_long) -> Result<libc::c_long, Error> {
    if result < 0 {
        Err(Error::new(
            io::Error::last_os_error(),
            ErrorKind::DiskManagement,
        ))
    } else {
        Ok(result)
    }
}

fn owner_only(id: libc::c_long) -> Result<(), Error> {
    checked(unsafe { libc::syscall(libc::SYS_keyctl, libc::KEYCTL_SETPERM, id, KEY_USR_ALL) })?;
    Ok(())
}

fn description(id: &VolumeId) -> Result<CString, Error> {
    CString::new(format!("startos-fde:{}:{}", id.mapper_name, id.uuid))
        .map_err(|_| metadata_error())
}

impl RootKeyring {
    pub(super) fn open() -> Result<Self, Error> {
        if unsafe { libc::geteuid() } != 0 {
            return Err(metadata_error());
        }
        let id = checked(unsafe {
            libc::syscall(
                libc::SYS_keyctl,
                libc::KEYCTL_GET_KEYRING_ID,
                libc::KEY_SPEC_USER_KEYRING as libc::c_long,
                1 as libc::c_long,
            )
        })?;
        owner_only(id)?;
        let process = checked(unsafe {
            libc::syscall(
                libc::SYS_keyctl,
                libc::KEYCTL_GET_KEYRING_ID,
                libc::KEY_SPEC_PROCESS_KEYRING as libc::c_long,
                1 as libc::c_long,
            )
        })?;
        checked(unsafe { libc::syscall(libc::SYS_keyctl, libc::KEYCTL_LINK, id, process) })?;
        Ok(Self(id))
    }

    fn find(&self, description: &CStr) -> Result<Option<libc::c_long>, Error> {
        let id = unsafe {
            libc::syscall(
                libc::SYS_keyctl,
                libc::KEYCTL_SEARCH,
                self.0,
                c"user".as_ptr(),
                description.as_ptr(),
                0 as libc::c_long,
            )
        };
        if id < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::ENOKEY) {
            return Ok(None);
        }
        Ok(Some(checked(id)?))
    }

    fn require_owner(id: libc::c_long, description: &CStr) -> Result<(), Error> {
        let size = checked(unsafe {
            libc::syscall(
                libc::SYS_keyctl,
                libc::KEYCTL_DESCRIBE,
                id,
                0 as libc::c_long,
                0 as libc::c_long,
            )
        })? as usize;
        let mut buffer = vec![0u8; size];
        let copied = checked(unsafe {
            libc::syscall(
                libc::SYS_keyctl,
                libc::KEYCTL_DESCRIBE,
                id,
                buffer.as_mut_ptr(),
                buffer.len(),
            )
        })? as usize;
        if copied != size {
            return Err(metadata_error());
        }
        let value = CStr::from_bytes_with_nul(&buffer).map_err(|_| metadata_error())?;
        let value = value.to_str().map_err(|_| metadata_error())?;
        let fields: Vec<_> = value.splitn(5, ';').collect();
        if fields.len() != 5
            || fields[0] != "user"
            || fields[1] != "0"
            || fields[4].as_bytes() != description.to_bytes()
        {
            return Err(metadata_error());
        }
        Ok(())
    }

    pub(super) fn store(&self, id: &VolumeId, bytes: &[u8]) -> Result<(), Error> {
        let description = description(id)?;
        if let Some(existing) = self.find(&description)? {
            Self::require_owner(existing, &description)?;
            owner_only(existing)?;
        }
        let key = checked(unsafe {
            libc::syscall(
                libc::SYS_add_key,
                c"user".as_ptr(),
                description.as_ptr(),
                bytes.as_ptr(),
                bytes.len(),
                self.0,
            )
        })?;
        owner_only(key)?;
        Ok(())
    }

    pub(super) fn load(&self, id: &VolumeId) -> Result<Option<Zeroizing<Vec<u8>>>, Error> {
        let description = description(id)?;
        let Some(key) = self.find(&description)? else {
            return Ok(None);
        };
        Self::require_owner(key, &description)?;
        owner_only(key)?;
        let size = checked(unsafe {
            libc::syscall(
                libc::SYS_keyctl,
                libc::KEYCTL_READ,
                key,
                0 as libc::c_long,
                0 as libc::c_long,
            )
        })? as usize;
        if size == 0 {
            return Err(metadata_error());
        }
        let mut bytes = Zeroizing::new(vec![0u8; size]);
        let copied = checked(unsafe {
            libc::syscall(
                libc::SYS_keyctl,
                libc::KEYCTL_READ,
                key,
                bytes.as_mut_ptr(),
                bytes.len(),
            )
        })? as usize;
        if copied != size {
            return Err(metadata_error());
        }
        Ok(Some(bytes))
    }

    #[cfg(test)]
    pub(super) fn remove(&self, id: &VolumeId) -> Result<(), Error> {
        if let Some(key) = self.find(&description(id)?)? {
            checked(unsafe { libc::syscall(libc::SYS_keyctl, libc::KEYCTL_UNLINK, key, self.0) })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::{Invoke, new_guid};

    const PAYLOAD: &[u8] = b"kernel-keyring-test-payload";
    #[test]
    #[ignore = "controlled unprivileged subprocess for the managed keyring fixture"]
    fn kernel_keyring_unprivileged_process() {
        let id: libc::c_long = std::env::var("FDE_KEYRING_SERIAL")
            .unwrap()
            .parse()
            .unwrap();
        let mut bytes = [0u8; 128];
        let result = unsafe {
            libc::syscall(
                libc::SYS_keyctl,
                libc::KEYCTL_READ,
                id,
                bytes.as_mut_ptr(),
                bytes.len(),
            )
        };
        assert_eq!(result, -1);
        assert_eq!(
            io::Error::last_os_error().raw_os_error(),
            Some(libc::EACCES)
        );
    }

    #[test]
    #[ignore = "controlled producer subprocess for the managed keyring fixture"]
    fn kernel_keyring_store_process() {
        checked(unsafe {
            libc::syscall(
                libc::SYS_keyctl,
                libc::KEYCTL_JOIN_SESSION_KEYRING,
                0 as libc::c_long,
            )
        })
        .unwrap();
        let id = VolumeId {
            mapper_name: "kernel-fixture".into(),
            uuid: std::env::var("FDE_KEYRING_UUID").unwrap(),
        };
        RootKeyring::open().unwrap().store(&id, PAYLOAD).unwrap();
    }

    #[tokio::test]
    #[ignore = "requires OS-root kernel keyrings in an isolated managed guest"]
    async fn kernel_keys_outlive_producer_session_and_deny_unprivileged_reads() {
        let id = VolumeId {
            mapper_name: "kernel-fixture".into(),
            uuid: new_guid().to_string(),
        };
        let ring = RootKeyring::open().unwrap();
        assert!(ring.load(&id).unwrap().is_none());
        let binary = std::env::current_exe().unwrap();
        tokio::process::Command::new(&binary)
            .args([
                "--ignored",
                "--exact",
                "disk::fde::keyring::tests::kernel_keyring_store_process",
            ])
            .env("FDE_KEYRING_UUID", &id.uuid)
            .invoke(ErrorKind::DiskManagement)
            .await
            .unwrap();
        assert_eq!(ring.load(&id).unwrap().unwrap().as_slice(), PAYLOAD);
        let serial = ring.find(&description(&id).unwrap()).unwrap().unwrap();
        tokio::process::Command::new(&binary)
            .args([
                "--ignored",
                "--exact",
                "disk::fde::keyring::tests::kernel_keyring_unprivileged_process",
            ])
            .env("FDE_KEYRING_SERIAL", serial.to_string())
            .uid(65534)
            .gid(65534)
            .invoke(ErrorKind::DiskManagement)
            .await
            .unwrap();
        ring.remove(&id).unwrap();
        assert!(ring.load(&id).unwrap().is_none());
    }
}
