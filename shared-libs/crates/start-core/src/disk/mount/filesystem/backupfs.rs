use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use backup_fs_protocol::{CONVERT_COMMAND, FormatStatus, INSPECT_COMMAND, MOUNT_COMMAND};
use digest::Digest;
use sha2::Sha256;
use tokio::process::Command;

use super::{FileSystem, MountType, ReadOnly};
use crate::prelude::*;
use crate::util::Invoke;

mod progress;

pub async fn convert(
    data_dir: &Path,
    key: &str,
    tracker: &crate::progress::FullProgressTracker,
) -> Result<(), Error> {
    let mut password = key.as_bytes();
    let mut sink = progress::ProgressSink::new(tracker);
    conversion_command(data_dir)
        .input(Some(&mut password))
        .output_to(&mut sink)
        .invoke(ErrorKind::Filesystem)
        .await?;
    Ok(())
}

pub struct BackupFS<DataDir: AsRef<Path>, Password: AsRef<str>> {
    data_dir: DataDir,
    password: Password,
}
impl<DataDir: AsRef<Path>, Password: AsRef<str>> BackupFS<DataDir, Password> {
    pub fn new(data_dir: DataDir, password: Password) -> Self {
        BackupFS { data_dir, password }
    }
}

fn conversion_command(data_dir: &Path) -> Command {
    let mut command = Command::new("startos-backup-fs");
    command
        .arg(CONVERT_COMMAND)
        .arg(data_dir)
        .env("RUST_LOG", "info");
    command
}

pub async fn inspect(data_dir: &Path) -> Result<FormatStatus, Error> {
    let output = Command::new("startos-backup-fs")
        .arg(INSPECT_COMMAND)
        .arg(data_dir)
        .invoke(ErrorKind::Filesystem)
        .await?;
    serde_json::from_slice(&output).with_kind(ErrorKind::Deserialization)
}

fn mount_command(data_dir: &Path, mountpoint: &Path, mount_type: MountType) -> Command {
    let mut command = Command::new("startos-backup-fs");
    command
        .arg(MOUNT_COMMAND)
        .arg(data_dir)
        .arg(mountpoint)
        .args(["--file-size-padding", "0.05", "-o", "allow_other"]);
    if mount_type == ReadOnly {
        command.arg("--readonly");
    }
    command
}

impl<DataDir: AsRef<Path> + Send + Sync, Password: AsRef<str> + Send + Sync> FileSystem
    for BackupFS<DataDir, Password>
{
    async fn mount<P: AsRef<Path> + Send>(
        &self,
        mountpoint: P,
        mount_type: MountType,
    ) -> Result<(), Error> {
        self.pre_mount(mountpoint.as_ref(), mount_type).await?;
        let mut password = self.password.as_ref().as_bytes();
        mount_command(self.data_dir.as_ref(), mountpoint.as_ref(), mount_type)
            .input(Some(&mut password))
            .capture(false)
            .invoke(ErrorKind::Filesystem)
            .await?;
        Ok(())
    }

    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        let mut sha = Sha256::new();
        sha.update("BackupFS");
        sha.update(
            tokio::fs::canonicalize(self.data_dir.as_ref())
                .await
                .with_ctx(|_| {
                    (
                        crate::ErrorKind::Filesystem,
                        self.data_dir.as_ref().display().to_string(),
                    )
                })?
                .as_os_str()
                .as_bytes(),
        );
        Ok(sha.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_argv_contains_only_the_operation_and_store_path() {
        let command = conversion_command(Path::new("/data"));
        let args: Vec<_> = command.as_std().get_args().collect();
        assert_eq!(
            args,
            [
                std::ffi::OsStr::new(CONVERT_COMMAND),
                std::ffi::OsStr::new("/data")
            ]
        );
    }

    #[test]
    fn mount_argv_contains_no_credentials_or_mount_table_options() {
        for mount_type in [
            MountType::ReadOnly,
            MountType::ReadWrite,
            MountType::BackupWrite,
        ] {
            let command = mount_command(Path::new("/data"), Path::new("/mount"), mount_type);
            let command = command.as_std();
            assert_eq!(command.get_program(), "startos-backup-fs");
            let args: Vec<_> = command
                .get_args()
                .map(|arg| arg.to_str().unwrap())
                .collect();
            assert_eq!(
                &args[..7],
                &[
                    "mount",
                    "/data",
                    "/mount",
                    "--file-size-padding",
                    "0.05",
                    "-o",
                    "allow_other"
                ]
            );
            assert_eq!(args.contains(&"--readonly"), mount_type == ReadOnly);
            assert_eq!(args.len(), if mount_type == ReadOnly { 8 } else { 7 });
            assert_eq!(command.get_envs().count(), 0);
        }
    }
}
