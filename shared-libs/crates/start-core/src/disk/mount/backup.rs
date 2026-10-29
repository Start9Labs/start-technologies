use std::path::{Path, PathBuf};
use std::sync::Arc;

use color_eyre::eyre::eyre;
use tokio::io::AsyncWriteExt;
use tracing::instrument;
use zeroize::Zeroizing;

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
use crate::util::crypto::{unwrap_backup_key, wrap_backup_key};
use crate::util::io::AtomicFile;
use crate::util::serde::IoFormat;

#[derive(Clone)]
pub struct BackupMountGuard<G: GenericMountGuard> {
    backup_disk_mount_guard: Option<G>,
    encrypted_guard: Option<TmpMountGuard>,
    enc_key: Zeroizing<String>,
    unencrypted_metadata_path: PathBuf,
    pub unencrypted_metadata: BackupUnencryptedMetadata,
    pub metadata: BackupInfo,
}
impl<G: GenericMountGuard> BackupMountGuard<G> {
    /// Persists a new store's key before encrypted data can be written.
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
            match tokio::fs::read(&unencrypted_metadata_path).await {
                Ok(bytes) => IoFormat::Json.from_slice(&bytes)?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Default::default(),
                Err(e) => {
                    return Err(e).with_ctx(|_| {
                        (
                            crate::ErrorKind::Filesystem,
                            unencrypted_metadata_path.display().to_string(),
                        )
                    });
                }
            };
        let (enc_key, needs_wrap) = match (
            unencrypted_metadata.password_hash.as_ref(),
            unencrypted_metadata.wrapped_key.as_ref(),
        ) {
            (Some(hash), Some(wrapped_key)) => {
                check_password(hash, password)?;
                unwrap_backup_key(wrapped_key, password)?
            }
            (None, None) => {
                if tokio::fs::try_exists(&crypt_path).await? {
                    return Err(Error::new(
                        eyre!(t!("disk.mount.backup.invalid-key-metadata")),
                        crate::ErrorKind::Backup,
                    ));
                }
                (
                    base32::encode(
                        base32::Alphabet::Rfc4648 { padding: false },
                        &rand::random::<[u8; 32]>(),
                    ),
                    true,
                )
            }
            _ => {
                return Err(Error::new(
                    eyre!(t!("disk.mount.backup.invalid-key-metadata")),
                    crate::ErrorKind::Backup,
                ));
            }
        };
        let new_store = unencrypted_metadata.password_hash.is_none();
        if new_store {
            unencrypted_metadata.password_hash = Some(
                argon2::hash_encoded(
                    password.as_bytes(),
                    &rand::random::<[u8; 16]>(),
                    &argon2::Config::rfc9106_low_mem(),
                )
                .with_kind(crate::ErrorKind::PasswordHashGeneration)?,
            );
        }
        if needs_wrap {
            unencrypted_metadata.wrapped_key = Some(wrap_backup_key(&enc_key, password)?);
        }
        if new_store {
            tokio::fs::create_dir_all(&backup_dir).await?;
            save_unencrypted_metadata(&unencrypted_metadata_path, &unencrypted_metadata).await?;
        }
        Ok((unencrypted_metadata, enc_key))
    }

    /// Checks the password without mounting the encrypted filesystem.
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
        let enc_key = Zeroizing::new(enc_key);
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
            TmpMountGuard::mount(&BackupFS::new(&crypt_path, enc_key.as_str()), ReadWrite).await?;

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

        save_unencrypted_metadata(&unencrypted_metadata_path, &unencrypted_metadata).await?;

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
        let wrapped_key = wrap_backup_key(&self.enc_key, new_password)?;
        let password_hash = argon2::hash_encoded(
            new_password.as_bytes(),
            &rand::random::<[u8; 16]>(),
            &argon2::Config::rfc9106_low_mem(),
        )
        .with_kind(crate::ErrorKind::PasswordHashGeneration)?;
        self.unencrypted_metadata.password_hash = Some(password_hash);
        self.unencrypted_metadata.wrapped_key = Some(wrapped_key);
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
        save_unencrypted_metadata(&self.unencrypted_metadata_path, &self.unencrypted_metadata).await
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

async fn save_unencrypted_metadata(
    path: &Path,
    metadata: &BackupUnencryptedMetadata,
) -> Result<(), Error> {
    let mut file = AtomicFile::new(path, None::<PathBuf>).await?;
    file.write_all(&IoFormat::Json.to_vec(metadata)?).await?;
    file.save().await
}

impl<G: GenericMountGuard> std::fmt::Debug for BackupMountGuard<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackupMountGuard")
            .field("backup_disk_mount_guard", &self.backup_disk_mount_guard)
            .field("encrypted_guard", &self.encrypted_guard)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::Never;
    use crate::util::crypto::backup_key_tests::{KEY, LEGACY_PASSWORD, legacy_fixture};

    async fn legacy_metadata(path: &Path) -> BackupUnencryptedMetadata {
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        let metadata = BackupUnencryptedMetadata {
            password_hash: Some(
                argon2::hash_encoded(
                    LEGACY_PASSWORD.as_bytes(),
                    &[42; 16],
                    &argon2::Config::rfc9106_low_mem(),
                )
                .unwrap(),
            ),
            wrapped_key: Some(legacy_fixture(true)),
            ..Default::default()
        };
        save_unencrypted_metadata(path, &metadata).await.unwrap();
        metadata
    }

    #[tokio::test]
    async fn legacy_metadata_rewrap_persists_and_reopens() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join(BACKUP_DIR_NAME)
            .join("server")
            .join("unencrypted-metadata.json");
        legacy_metadata(&path).await;
        let original = tokio::fs::read(&path).await.unwrap();
        let (metadata, key) =
            BackupMountGuard::<Never>::load_metadata(directory.path(), "server", LEGACY_PASSWORD)
                .await
                .unwrap();
        assert!(key == KEY);
        assert!(metadata.wrapped_key.as_ref().unwrap().starts_with("v1:"));
        assert_eq!(tokio::fs::read(&path).await.unwrap(), original);
        save_unencrypted_metadata(&path, &metadata).await.unwrap();
        let (reopened, key) =
            BackupMountGuard::<Never>::load_metadata(directory.path(), "server", LEGACY_PASSWORD)
                .await
                .unwrap();
        assert!(key == KEY);
        assert_eq!(reopened.wrapped_key, metadata.wrapped_key);
    }

    #[tokio::test]
    async fn failed_metadata_load_preserves_backup() {
        let directory = tempfile::tempdir().unwrap();
        let backup_dir = directory.path().join(BACKUP_DIR_NAME).join("server");
        let path = backup_dir.join("unencrypted-metadata.json");
        let mut metadata = legacy_metadata(&path).await;
        let crypt = backup_dir.join("crypt");
        tokio::fs::create_dir(&crypt).await.unwrap();
        let marker = crypt.join("existing-backup");
        tokio::fs::write(&marker, b"retain").await.unwrap();
        let original = tokio::fs::read(&path).await.unwrap();
        assert!(
            BackupMountGuard::<Never>::load_metadata(directory.path(), "server", "wrong-password")
                .await
                .is_err()
        );
        assert_eq!(tokio::fs::read(&path).await.unwrap(), original);
        metadata.wrapped_key = Some("v2:invalid".to_owned());
        save_unencrypted_metadata(&path, &metadata).await.unwrap();
        let malformed = tokio::fs::read(&path).await.unwrap();
        assert!(
            BackupMountGuard::<Never>::load_metadata(directory.path(), "server", LEGACY_PASSWORD)
                .await
                .is_err()
        );
        assert_eq!(tokio::fs::read(&path).await.unwrap(), malformed);
        metadata.wrapped_key = None;
        save_unencrypted_metadata(&path, &metadata).await.unwrap();
        assert!(
            BackupMountGuard::<Never>::load_metadata(directory.path(), "server", LEGACY_PASSWORD)
                .await
                .is_err()
        );
        tokio::fs::remove_file(&path).await.unwrap();
        assert!(
            BackupMountGuard::<Never>::load_metadata(directory.path(), "server", LEGACY_PASSWORD)
                .await
                .is_err()
        );
        assert_eq!(tokio::fs::read(&marker).await.unwrap(), b"retain");
    }

    #[tokio::test]
    async fn new_metadata_and_password_changes_use_authenticated_wrapping() {
        let directory = tempfile::tempdir().unwrap();
        let (metadata, key) =
            BackupMountGuard::<Never>::load_metadata(directory.path(), "server", "first-password")
                .await
                .unwrap();
        assert!(metadata.wrapped_key.as_ref().unwrap().starts_with("v1:"));
        tokio::fs::create_dir(
            directory
                .path()
                .join(BACKUP_DIR_NAME)
                .join("server")
                .join("crypt"),
        )
        .await
        .unwrap();
        let (reopened, reopened_key) =
            BackupMountGuard::<Never>::load_metadata(directory.path(), "server", "first-password")
                .await
                .unwrap();
        assert!(reopened_key == key);
        assert_eq!(reopened.wrapped_key, metadata.wrapped_key);
        let mut guard = BackupMountGuard::<Never> {
            backup_disk_mount_guard: None,
            encrypted_guard: None,
            enc_key: Zeroizing::new(key.clone()),
            unencrypted_metadata_path: directory.path().join("unencrypted-metadata.json"),
            unencrypted_metadata: metadata,
            metadata: Default::default(),
        };
        assert!(!format!("{guard:?}").contains(&key));
        guard.change_password("second-password").unwrap();
        let wrapped = guard.unencrypted_metadata.wrapped_key.as_ref().unwrap();
        assert!(wrapped.starts_with("v1:"));
        assert!(unwrap_backup_key(wrapped, "second-password").unwrap().0 == key);
        assert!(unwrap_backup_key(wrapped, "first-password").is_err());
        check_password(
            guard.unencrypted_metadata.password_hash.as_ref().unwrap(),
            "second-password",
        )
        .unwrap();
    }
}
