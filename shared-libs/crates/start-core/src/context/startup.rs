use std::path::{Path, PathBuf};

use ed25519_dalek::SigningKey;

use crate::developer::load_signing_key;
use crate::middleware::auth::signature::NonceCache;
use crate::prelude::*;
use crate::sign::session::ServerIdentity;
use crate::sign::{AnySigningKey, Signer};
use crate::util::Invoke;
use crate::util::io::{delete_file, write_file_atomic, write_file_private_atomic};
use crate::util::serde::Pem;
use crate::util::sync::SyncMutex;

pub(crate) const IDENTITY_PATH: &str = "/media/startos/config/id.key.pem";

fn setup_identity_path_at(identity: &Path) -> PathBuf {
    identity.with_file_name("setup.key.pem")
}

fn configured(identity: &Path) -> Result<bool, Error> {
    identity
        .with_file_name("disk.guid")
        .try_exists()
        .with_kind(ErrorKind::Filesystem)
}

/// The database owns an installed server's identity; config exposes the same key before data import.
/// Setup keys remain separate until completion; imported account keys cannot seed fresh accounts.
/// Both config copies are unencrypted and root-only.
pub(crate) async fn startup_identity() -> Result<Option<SigningKey>, Error> {
    startup_identity_at(Path::new(IDENTITY_PATH)).await
}

async fn startup_identity_at(identity: &Path) -> Result<Option<SigningKey>, Error> {
    let configured = configured(identity)?;
    let path = if configured {
        identity.to_owned()
    } else {
        setup_identity_path_at(identity)
    };
    if path.try_exists().with_kind(ErrorKind::Filesystem)? {
        return match load_signing_key(&path) {
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
    persist_identity_at(&path, &key).await?;
    Ok(Some(key))
}

pub(crate) struct DiagnosticIdentity {
    key: SigningKey,
    temporary: bool,
}

impl DiagnosticIdentity {
    pub(crate) fn temporary(key: SigningKey) -> Self {
        Self {
            key,
            temporary: true,
        }
    }
}

impl From<SigningKey> for DiagnosticIdentity {
    fn from(key: SigningKey) -> Self {
        Self {
            key,
            temporary: false,
        }
    }
}

/// An unavailable persistent key permits only a marked, unpersisted recovery identity.
pub(crate) async fn diagnostic_identity() -> DiagnosticIdentity {
    diagnostic_identity_at(Path::new(IDENTITY_PATH)).await
}

async fn diagnostic_identity_at(path: &Path) -> DiagnosticIdentity {
    match startup_identity_at(path).await.log_err().flatten() {
        Some(key) => key.into(),
        None => DiagnosticIdentity::temporary(
            crate::db::model::private::generate_server_identity_key().0,
        ),
    }
}

/// Carries the live session's admitted identity, replacing preserved setup keys.
pub(crate) async fn persist_setup_identity(key: &SigningKey) -> Result<(), Error> {
    persist_setup_identity_at(Path::new(IDENTITY_PATH), key).await
}

async fn persist_setup_identity_at(path: &Path, key: &SigningKey) -> Result<(), Error> {
    persist_identity_at(&setup_identity_path_at(path), key).await
}

pub(crate) async fn mirror_configured_identity(key: &SigningKey) -> Result<(), Error> {
    mirror_configured_identity_at(Path::new(IDENTITY_PATH), key).await
}

async fn mirror_configured_identity_at(path: &Path, key: &SigningKey) -> Result<(), Error> {
    if configured(path)? {
        persist_identity_at(path, key).await?;
    }
    Ok(())
}

/// `disk.guid` commits the account identity; incomplete setup continues using its separate key.
pub(crate) async fn complete_identity(key: &SigningKey, disk_guid: &str) -> Result<(), Error> {
    complete_identity_at(Path::new(IDENTITY_PATH), key, disk_guid).await
}

async fn complete_identity_at(path: &Path, key: &SigningKey, disk_guid: &str) -> Result<(), Error> {
    persist_identity_at(path, key).await?;
    write_file_atomic(path.with_file_name("disk.guid"), disk_guid).await?;
    sync_config(path).await?;
    delete_file(setup_identity_path_at(path)).await?;
    sync_config(path).await
}

pub(crate) async fn forget_identity() -> Result<(), Error> {
    tokio::process::Command::new("/usr/lib/startos/scripts/forget-server-identity")
        .invoke(ErrorKind::Filesystem)
        .await?;
    Ok(())
}

async fn persist_identity_at(path: &Path, key: &SigningKey) -> Result<(), Error> {
    write_file_private_atomic(path, Pem::new_ref(key).to_string()).await?;
    sync_config(path).await
}

async fn sync_config(path: &Path) -> Result<(), Error> {
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
    temporary: bool,
    pub nonces: SyncMutex<NonceCache>,
}

impl StartupRpc {
    pub fn new(identity: Option<SigningKey>) -> Self {
        Self {
            identity,
            temporary: false,
            nonces: SyncMutex::new(NonceCache::default()),
        }
    }

    pub fn diagnostic(identity: DiagnosticIdentity) -> Self {
        Self {
            temporary: identity.temporary,
            ..Self::new(Some(identity.key))
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

    pub fn identity(&self) -> Result<ServerIdentity, Error> {
        let signer = self
            .key()
            .map(|key| Signer::from(AnySigningKey::Ed25519(key)))?;
        Ok(ServerIdentity {
            signer,
            temporary: self.temporary,
        })
    }

    pub async fn clock_synced() -> bool {
        crate::init::check_time_is_synchronized()
            .await
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests;
