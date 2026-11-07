use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use pkcs8::EncodePrivateKey;
use tokio::io::AsyncWriteExt;
use zeroize::Zeroizing;

use super::{AnySigningKey, Signer};
use crate::prelude::*;
use crate::util::io::AtomicFile;

/// Private server identity filename within the instance's persistent directory.
pub const IDENTITY_FILE: &str = "rpc-identity.pem";

const PRIVATE_MODE: u32 = 0o600;

/// Loads a persistent private server identity, creating it atomically on first startup.
/// Invalid existing contents fail startup rather than replacing the identity.
pub async fn load(directory: &Path) -> Result<Signer, Error> {
    tokio::fs::create_dir_all(directory)
        .await
        .with_kind(ErrorKind::Filesystem)?;
    let path = directory.join(IDENTITY_FILE);
    let contents = match tokio::fs::read_to_string(&path).await {
        Ok(contents) => Zeroizing::new(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let key = AnySigningKey::generate();
            let contents = key
                .to_pkcs8_pem(pkcs8::LineEnding::LF)
                .with_kind(ErrorKind::Serialization)?;
            let temporary = directory.join(format!(".{IDENTITY_FILE}.{}", crate::util::new_guid()));
            let mut file = AtomicFile::new(&path, Some(&temporary)).await?;
            file.set_permissions(std::fs::Permissions::from_mode(PRIVATE_MODE))
                .await
                .with_kind(ErrorKind::Filesystem)?;
            file.write_all(contents.as_bytes())
                .await
                .with_kind(ErrorKind::Filesystem)?;
            file.save_no_replace().await?;
            Zeroizing::new(
                tokio::fs::read_to_string(&path)
                    .await
                    .with_kind(ErrorKind::Filesystem)?,
            )
        }
        Err(error) => return Err(Error::new(error, ErrorKind::Filesystem)),
    };
    let file = tokio::fs::File::open(&path)
        .await
        .with_kind(ErrorKind::Filesystem)?;
    file.set_permissions(std::fs::Permissions::from_mode(PRIVATE_MODE))
        .await
        .with_kind(ErrorKind::Filesystem)?;
    file.sync_all().await.with_kind(ErrorKind::Filesystem)?;
    tokio::fs::File::open(directory)
        .await
        .with_kind(ErrorKind::Filesystem)?
        .sync_all()
        .await
        .with_kind(ErrorKind::Filesystem)?;
    Ok(contents.parse::<AnySigningKey>()?.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn identity_is_private_and_stable_across_loads() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("server");
        let first = load(&dir).await.unwrap().verifying_key().await.unwrap();
        let path = dir.join(IDENTITY_FILE);
        let saved = tokio::fs::read(&path).await.unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            load(&dir).await.unwrap().verifying_key().await.unwrap(),
            first
        );
        assert_eq!(tokio::fs::read(&path).await.unwrap(), saved);
    }

    #[tokio::test]
    async fn concurrent_first_startups_share_one_complete_identity() {
        let temp = tempfile::tempdir().unwrap();
        let keys = futures::future::join_all((0..16).map(|_| async {
            load(temp.path())
                .await
                .unwrap()
                .verifying_key()
                .await
                .unwrap()
        }))
        .await;
        assert!(keys.iter().all(|key| key == &keys[0]));
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn invalid_existing_identity_is_not_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(IDENTITY_FILE);
        tokio::fs::write(&path, b"not a private key").await.unwrap();
        assert!(load(temp.path()).await.is_err());
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"not a private key");
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn existing_key_permissions_are_tightened_without_rotation() {
        let temp = tempfile::tempdir().unwrap();
        let key = AnySigningKey::generate();
        let path = temp.path().join(IDENTITY_FILE);
        tokio::fs::write(
            &path,
            key.to_pkcs8_pem(pkcs8::LineEnding::LF).unwrap().as_bytes(),
        )
        .await
        .unwrap();
        tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .await
            .unwrap();
        assert_eq!(
            load(temp.path())
                .await
                .unwrap()
                .verifying_key()
                .await
                .unwrap(),
            key.verifying_key()
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[tokio::test]
    async fn separate_instance_directories_do_not_share_identity() {
        let temp = tempfile::tempdir().unwrap();
        let first = load(&temp.path().join("one"))
            .await
            .unwrap()
            .verifying_key()
            .await
            .unwrap();
        let second = load(&temp.path().join("two"))
            .await
            .unwrap()
            .verifying_key()
            .await
            .unwrap();
        assert_ne!(first, second);
    }
}
