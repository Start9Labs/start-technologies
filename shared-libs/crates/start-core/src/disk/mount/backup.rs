use std::path::{Path, PathBuf};
use std::sync::Arc;

use color_eyre::eyre::eyre;
use tokio::io::AsyncWriteExt;
use tracing::instrument;

use super::guard::{GenericMountGuard, TmpMountGuard};
use super::util::sync_directory;
use crate::PackageId;
use crate::auth::check_password;
use crate::backup::target::BackupInfo;
use crate::disk::BACKUP_DIR_NAME;
use crate::disk::mount::filesystem::ReadWrite;
use crate::disk::mount::filesystem::backupfs::BackupFS;
use crate::disk::mount::guard::SubPath;
use crate::disk::util::BackupUnencryptedMetadata;
use crate::prelude::*;
use crate::util::crypto::{decrypt_slice, encrypt_slice};
use crate::util::io::AtomicFile;
use crate::util::serde::IoFormat;

#[derive(Clone, Debug)]
pub struct BackupMountGuard<G: GenericMountGuard> {
    backup_disk_mount_guard: Option<G>,
    encrypted_guard: Option<TmpMountGuard>,
    enc_key: String,
    unencrypted_metadata_path: PathBuf,
    pub unencrypted_metadata: BackupUnencryptedMetadata,
    pub metadata: BackupInfo,
}
impl<G: GenericMountGuard> BackupMountGuard<G> {
    #[instrument(skip_all)]
    pub async fn load_metadata(
        backup_disk_path: &Path,
        server_id: &str,
        password: &str,
    ) -> Result<(BackupUnencryptedMetadata, String), Error> {
        let backup_dir = backup_disk_path.join(BACKUP_DIR_NAME).join(server_id);
        let unencrypted_metadata_path = backup_dir.join("unencrypted-metadata.json");
        let crypt_path = backup_dir.join("crypt");
        let mut unencrypted_metadata: BackupUnencryptedMetadata =
            if tokio::fs::metadata(&unencrypted_metadata_path)
                .await
                .is_ok()
            {
                IoFormat::Json.from_slice(
                    &tokio::fs::read(&unencrypted_metadata_path)
                        .await
                        .with_ctx(|_| {
                            (
                                crate::ErrorKind::Filesystem,
                                unencrypted_metadata_path.display().to_string(),
                            )
                        })?,
                )?
            } else {
                crate::util::io::delete_dir(&crypt_path).await?;
                Default::default()
            };
        let enc_key = if let (Some(hash), Some(wrapped_key)) = (
            unencrypted_metadata.password_hash.as_ref(),
            unencrypted_metadata.wrapped_key.as_ref(),
        ) {
            let wrapped_key =
                base32::decode(base32::Alphabet::Rfc4648 { padding: true }, wrapped_key)
                    .ok_or_else(|| {
                        Error::new(
                            eyre!("failed to decode wrapped key"),
                            crate::ErrorKind::Backup,
                        )
                    })?;
            check_password(hash, password)?;
            String::from_utf8(decrypt_slice(wrapped_key, password))?
        } else {
            base32::encode(
                base32::Alphabet::Rfc4648 { padding: false },
                &rand::random::<[u8; 32]>()[..],
            )
        };
        if unencrypted_metadata.password_hash.is_none() {
            unencrypted_metadata.password_hash = Some(
                argon2::hash_encoded(
                    password.as_bytes(),
                    &rand::random::<[u8; 16]>()[..],
                    &argon2::Config::rfc9106_low_mem(),
                )
                .with_kind(crate::ErrorKind::PasswordHashGeneration)?,
            );
        }
        if unencrypted_metadata.wrapped_key.is_none() {
            unencrypted_metadata.wrapped_key = Some(base32::encode(
                base32::Alphabet::Rfc4648 { padding: true },
                &encrypt_slice(&enc_key, password),
            ));
        }
        Ok((unencrypted_metadata, enc_key))
    }

    /// Cheap up-front check that `password` matches the target's existing
    /// backup, reading only `unencrypted-metadata.json` — no encrypted mount or
    /// segment-log replay. `Ok` when the target has no backup yet (nothing to
    /// match against).
    #[instrument(skip_all)]
    pub async fn validate_password(
        backup_disk_path: &Path,
        server_id: &str,
        password: &str,
    ) -> Result<(), Error> {
        let unencrypted_metadata_path = backup_disk_path
            .join(BACKUP_DIR_NAME)
            .join(server_id)
            .join("unencrypted-metadata.json");
        let bytes = match tokio::fs::read(&unencrypted_metadata_path).await {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                return Err(e).with_ctx(|_| {
                    (
                        crate::ErrorKind::Filesystem,
                        unencrypted_metadata_path.display().to_string(),
                    )
                });
            }
        };
        let unencrypted_metadata: BackupUnencryptedMetadata = IoFormat::Json.from_slice(&bytes)?;
        if let Some(hash) = unencrypted_metadata.password_hash.as_ref() {
            check_password(hash, password)?;
        }
        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn mount(
        backup_disk_mount_guard: G,
        server_id: &str,
        password: &str,
    ) -> Result<Self, Error> {
        let backup_disk_path = backup_disk_mount_guard.path();
        let (unencrypted_metadata, enc_key) =
            Self::load_metadata(backup_disk_path, server_id, password).await?;
        let backup_dir = backup_disk_path.join(BACKUP_DIR_NAME).join(server_id);
        let unencrypted_metadata_path = backup_dir.join("unencrypted-metadata.json");
        let crypt_path = backup_dir.join("crypt");

        if tokio::fs::metadata(&crypt_path).await.is_err() {
            tokio::fs::create_dir_all(&crypt_path).await.with_ctx(|_| {
                (
                    crate::ErrorKind::Filesystem,
                    crypt_path.display().to_string(),
                )
            })?;
        }
        let encrypted_guard =
            TmpMountGuard::mount(&BackupFS::new(&crypt_path, &enc_key), ReadWrite).await?;

        let metadata_path = encrypted_guard.path().join("metadata.json");
        let metadata: BackupInfo = if tokio::fs::metadata(&metadata_path).await.is_ok() {
            IoFormat::Json.from_slice(&tokio::fs::read(&metadata_path).await.with_ctx(|_| {
                (
                    crate::ErrorKind::Filesystem,
                    metadata_path.display().to_string(),
                )
            })?)?
        } else {
            Default::default()
        };

        Ok(Self {
            backup_disk_mount_guard: Some(backup_disk_mount_guard),
            encrypted_guard: Some(encrypted_guard),
            enc_key,
            unencrypted_metadata_path,
            unencrypted_metadata,
            metadata,
        })
    }

    /// Root of the backup target drive — the parent of both the legacy
    /// `StartOSBackups` folder and the current `StartOSBackupsV2` folder.
    /// (`path()` instead returns the decrypted V2 crypt mount.)
    pub fn backup_disk_path(&self) -> &Path {
        self.backup_disk_mount_guard
            .as_ref()
            .expect("backup disk mount guard present")
            .path()
    }

    pub fn change_password(&mut self, new_password: &str) -> Result<(), Error> {
        self.unencrypted_metadata.password_hash = Some(
            argon2::hash_encoded(
                new_password.as_bytes(),
                &rand::random::<[u8; 16]>()[..],
                &argon2::Config::rfc9106_low_mem(),
            )
            .with_kind(crate::ErrorKind::PasswordHashGeneration)?,
        );
        self.unencrypted_metadata.wrapped_key = Some(base32::encode(
            base32::Alphabet::Rfc4648 { padding: false },
            &encrypt_slice(&self.enc_key, new_password),
        ));
        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn package_backup(
        self: &Arc<Self>,
        id: &PackageId,
    ) -> Result<SubPath<Arc<Self>>, Error> {
        let package_guard = SubPath::new(self.clone(), id);
        let package_path = package_guard.path();
        if tokio::fs::metadata(&package_path).await.is_err() {
            tokio::fs::create_dir_all(&package_path)
                .await
                .with_ctx(|_| {
                    (
                        crate::ErrorKind::Filesystem,
                        package_path.display().to_string(),
                    )
                })?;
        }
        Ok(package_guard)
    }

    #[instrument(skip_all)]
    pub async fn save(&self) -> Result<(), Error> {
        let metadata_path = self.path().join("metadata.json");
        let mut file = AtomicFile::new(&metadata_path, None::<PathBuf>).await?;
        file.write_all(&IoFormat::Json.to_vec(&self.metadata)?)
            .await?;
        file.save().await?;
        let mut file = AtomicFile::new(&self.unencrypted_metadata_path, None::<PathBuf>).await?;
        file.write_all(&IoFormat::Json.to_vec(&self.unencrypted_metadata)?)
            .await?;
        file.save().await?;
        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn save_and_unmount(self) -> Result<(), Error> {
        self.save().await?;
        self.unmount().await?;
        Ok(())
    }
}
impl<G: GenericMountGuard> GenericMountGuard for BackupMountGuard<G> {
    fn path(&self) -> &Path {
        if let Some(guard) = &self.encrypted_guard {
            guard.path()
        } else {
            unreachable!()
        }
    }
    async fn unmount(mut self) -> Result<(), Error> {
        if let Some(guard) = self.encrypted_guard.take() {
            sync_directory(guard.path()).await?;
            guard.unmount().await?;
        }
        if let Some(guard) = self.backup_disk_mount_guard.take() {
            guard.unmount().await?;
        }
        Ok(())
    }
}
impl<G: GenericMountGuard> Drop for BackupMountGuard<G> {
    fn drop(&mut self) {
        let first = self.encrypted_guard.take();
        let second = self.backup_disk_mount_guard.take();
        tokio::spawn(async move {
            if let Some(guard) = first {
                sync_directory(guard.path()).await.log_err();
                guard.unmount().await.log_err();
            }
            if let Some(guard) = second {
                guard.unmount().await.log_err();
            }
        });
    }
}
