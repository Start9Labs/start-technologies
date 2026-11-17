use std::path::Path;

use ed25519_dalek::SigningKey;

use crate::developer::load_signing_key;
use crate::middleware::auth::signature::NonceCache;
use crate::prelude::*;
use crate::sign::{AnySigningKey, Signer};
use crate::util::io::write_file_private_atomic;
use crate::util::serde::Pem;
use crate::util::sync::SyncMutex;

pub(crate) const IDENTITY_PATH: &str = "/media/startos/config/id.key.pem";
/// The database owns an installed server's identity; config exposes the same key before data import.
/// The config copy is unencrypted and root-only. Missing copies must not rotate installed identities.
pub(crate) async fn startup_identity() -> Result<Option<SigningKey>, Error> {
    startup_identity_at(Path::new(IDENTITY_PATH)).await
}

async fn startup_identity_at(path: &Path) -> Result<Option<SigningKey>, Error> {
    let configured = path
        .with_file_name("disk.guid")
        .try_exists()
        .with_kind(ErrorKind::Filesystem)?;
    if path.try_exists().with_kind(ErrorKind::Filesystem)? {
        return match load_signing_key(path) {
            Ok(key) => Ok(Some(key)),
            Err(error) if configured => {
                tracing::warn!("Boot identity unavailable until data import: {error}");
                Ok(None)
            }
            Err(error) => Err(error),
        };
    }
    if configured {
        return Ok(None);
    }
    let key = crate::db::model::private::generate_server_identity_key().0;
    persist_identity_at(path, &key).await?;
    Ok(Some(key))
}

pub(crate) async fn persist_identity(key: &SigningKey) -> Result<(), Error> {
    persist_identity_at(Path::new(IDENTITY_PATH), key).await
}

async fn persist_identity_at(path: &Path, key: &SigningKey) -> Result<(), Error> {
    write_file_private_atomic(path, Pem::new_ref(key).to_string()).await?;
    tokio::fs::File::open(path.parent().unwrap())
        .await
        .with_kind(ErrorKind::Filesystem)?
        .sync_all()
        .await
        .with_kind(ErrorKind::Filesystem)
}

/// Possession admission for a startup context without an enrolled-key database.
pub(crate) struct StartupRpc {
    // Setup retains its admitted identity across config-mount replacement.
    identity: Option<SigningKey>,
    pub nonces: SyncMutex<NonceCache>,
}

impl StartupRpc {
    pub fn new(identity: Option<SigningKey>) -> Self {
        Self {
            identity,
            nonces: SyncMutex::new(NonceCache::default()),
        }
    }

    pub fn key(&self) -> Result<SigningKey, Error> {
        self.key_at(Path::new(IDENTITY_PATH))
    }

    fn key_at(&self, path: &Path) -> Result<SigningKey, Error> {
        self.identity
            .clone()
            .map(Ok)
            .unwrap_or_else(|| load_signing_key(path))
    }

    pub fn identity(&self) -> Result<Signer, Error> {
        self.key().map(|key| AnySigningKey::Ed25519(key).into())
    }

    pub async fn clock_synced() -> bool {
        crate::init::check_time_is_synchronized()
            .await
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests;
