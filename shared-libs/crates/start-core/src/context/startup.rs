use crate::middleware::auth::signature::NonceCache;
use crate::sign::{AnySigningKey, Signer};
use crate::util::sync::SyncMutex;

/// Possession admission for a startup context without an enrolled-key database.
pub(crate) struct StartupRpc {
    pub identity: Signer,
    pub nonces: SyncMutex<NonceCache>,
}

impl StartupRpc {
    pub fn new() -> Self {
        Self {
            identity: AnySigningKey::generate().into(),
            nonces: SyncMutex::new(NonceCache::default()),
        }
    }

    pub async fn clock_synced() -> bool {
        crate::init::check_time_is_synchronized()
            .await
            .unwrap_or(false)
    }
}
