use std::fmt::Display;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use digest::Digest;
use lazy_format::lazy_format;
use sha2::Sha256;
use tokio::process::Command;

use super::FileSystem;
use crate::disk::mount::filesystem::default_mount_command;
use crate::prelude::*;
use crate::util::Invoke;

pub struct EcryptFS<EncryptedDir: AsRef<Path>, Key: AsRef<str>> {
    encrypted_dir: EncryptedDir,
    key: Key,
}
impl<EncryptedDir: AsRef<Path>, Key: AsRef<str>> EcryptFS<EncryptedDir, Key> {
    pub fn new(encrypted_dir: EncryptedDir, key: Key) -> Self {
        EcryptFS { encrypted_dir, key }
    }
}
impl<EncryptedDir: AsRef<Path> + Send + Sync, Key: AsRef<str> + Send + Sync> FileSystem
    for EcryptFS<EncryptedDir, Key>
{
    fn mount_type(&self) -> Option<impl AsRef<str>> {
        Some("ecryptfs")
    }
    async fn source(&self) -> Result<Option<impl AsRef<Path>>, Error> {
        Ok(Some(&self.encrypted_dir))
    }
    fn mount_options(&self) -> impl IntoIterator<Item = impl Display> {
        [
            Box::new(lazy_format!(
                "key=passphrase:passphrase_passwd={}",
                self.key.as_ref()
            )) as Box<dyn Display>,
            Box::new("ecryptfs_cipher=aes"),
            Box::new("ecryptfs_key_bytes=32"),
            Box::new("ecryptfs_passthrough=n"),
            Box::new("ecryptfs_enable_filename_crypto=y"),
            Box::new("no_sig_cache"),
        ]
    }
    async fn mount<P: AsRef<Path> + Send>(
        &self,
        mountpoint: P,
        mount_type: super::MountType,
    ) -> Result<(), Error> {
        self.pre_mount(mountpoint.as_ref(), mount_type).await?;
        Command::new("mount")
            .args(
                default_mount_command(self, mountpoint, mount_type)
                    .await?
                    .get_args(),
            )
            .input(Some(&mut std::io::Cursor::new(b"\n")))
            .invoke(crate::ErrorKind::Filesystem)
            .await?;
        Ok(())
    }
    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        let mut sha = Sha256::new();
        sha.update("EcryptFS");
        sha.update(
            tokio::fs::canonicalize(self.encrypted_dir.as_ref())
                .await
                .with_ctx(|_| {
                    (
                        crate::ErrorKind::Filesystem,
                        self.encrypted_dir.as_ref().display().to_string(),
                    )
                })?
                .as_os_str()
                .as_bytes(),
        );
        Ok(sha.finalize())
    }
}
