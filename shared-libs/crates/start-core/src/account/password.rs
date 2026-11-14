use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::AccountInfo;
use crate::context::RpcContext;
use crate::db::model::Database;
use crate::disk::fde::{AdminKeyslots, KeyCredential, LuksPool, PasswordChangePlan, VolumeKeys};
use crate::prelude::*;
use crate::util::future::complete_owned_operation;
use crate::util::sync::SyncRwLock;

/// Persisted slot authority and recoverable account-password intent; never volume keys.
/// `Retiring` is committed atomically with the target account hash after both slots verify.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "stage",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LuksCredentials {
    Stable {
        admin: AdminKeyslots,
    },
    Installing {
        plan: PasswordChangePlan,
        target_hash: String,
    },
    Retiring {
        plan: PasswordChangePlan,
    },
}

pub(crate) async fn reset_password(
    ctx: RpcContext,
    password: Zeroizing<String>,
) -> Result<(), Error> {
    complete_owned_operation(async move {
        let _exclusive = ctx.password_changes.lock().await;
        change_password(&ctx, &password).await?;
        crate::auth::write_shadow(&password).await
    })
    .await
}

async fn commit_password(
    db: &TypedPatchDb<Database>,
    account: &SyncRwLock<AccountInfo>,
    hash: &str,
    credentials: Option<LuksCredentials>,
) -> Result<(), Error> {
    db.mutate(|db| {
        let private = db.as_private_mut();
        private.as_password_mut().ser(&hash.to_owned())?;
        private.as_luks_credentials_mut().ser(&credentials)
    })
    .await
    .result?;
    account.mutate(|account| account.password = hash.to_owned());
    Ok(())
}

async fn persist_credentials(
    db: &TypedPatchDb<Database>,
    credentials: &LuksCredentials,
) -> Result<(), Error> {
    db.mutate(|db| {
        db.as_private_mut()
            .as_luks_credentials_mut()
            .ser(&Some(credentials.clone()))
    })
    .await
    .result
}

async fn change_password(ctx: &RpcContext, password: &str) -> Result<(), Error> {
    let credentials = ctx
        .db
        .peek()
        .await
        .as_private()
        .as_luks_credentials()
        .de()?;
    let Some(_) = credentials else {
        return commit_password(
            &ctx.db,
            &ctx.account,
            &super::hash_password(password)?,
            None,
        )
        .await;
    };
    let pool = LuksPool::new(&ctx.disk_guid, "/media/startos/config")?;
    let memory = pool.protect_secret_handling()?;
    let keys = pool.cached_unlock_keys().await?.ok_or_else(|| {
        Error::new(
            eyre!(t!("account.volume-keys-unavailable")),
            ErrorKind::DiskManagement,
        )
    })?;
    change_encrypted_password(&ctx.db, &ctx.account, &pool, &keys, password).await?;
    memory.check()
}

/// Resumes persisted intent while the caller owns `password_changes` and protects transient keys.
pub(crate) async fn change_encrypted_password(
    db: &TypedPatchDb<Database>,
    account: &SyncRwLock<AccountInfo>,
    pool: &LuksPool,
    keys: &VolumeKeys,
    password: &str,
) -> Result<(), Error> {
    crate::disk::fde::validate_new_password(password)?;
    let mut credentials: LuksCredentials = db
        .peek()
        .await
        .as_private()
        .as_luks_credentials()
        .de()?
        .ok_or_else(|| {
            Error::new(
                eyre!(t!("error.invalid-request")),
                ErrorKind::InvalidRequest,
            )
        })?;
    loop {
        match credentials {
            LuksCredentials::Stable { admin } => {
                let current = db.peek().await.as_private().as_password().de()?;
                if crate::auth::check_password(&current, password).is_ok()
                    && pool.accepts_admin_password(&admin, password).await?
                {
                    return Ok(());
                }
                credentials = LuksCredentials::Installing {
                    plan: pool.plan_password_change(&admin).await?,
                    target_hash: super::hash_password(password)?,
                };
                persist_credentials(db, &credentials).await?;
            }
            LuksCredentials::Installing { plan, target_hash } => {
                if crate::auth::check_password(&target_hash, password).is_err() {
                    pool.abort_password_change(&plan, keys).await?;
                    credentials = LuksCredentials::Stable {
                        admin: plan.previous_admin_slots(),
                    };
                    persist_credentials(db, &credentials).await?;
                    continue;
                }
                pool.install_password(&plan, password, KeyCredential::VolumeKeys(keys))
                    .await?;
                credentials = LuksCredentials::Retiring { plan };
                commit_password(db, account, &target_hash, Some(credentials.clone())).await?;
            }
            LuksCredentials::Retiring { plan } => {
                let current = db.peek().await.as_private().as_password().de()?;
                let same_password = crate::auth::check_password(&current, password).is_ok();
                if same_password {
                    pool.retire_password(&plan, password).await?;
                } else {
                    pool.retire_password_with_keys(&plan, keys).await?;
                }
                credentials = LuksCredentials::Stable {
                    admin: plan.new_admin_slots(),
                };
                persist_credentials(db, &credentials).await?;
                if same_password {
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;
    use crate::hostname::ServerHostname;

    #[tokio::test]
    async fn password_commit_preserves_concurrent_identity_update() {
        let directory = tempfile::tempdir().unwrap();
        let account = AccountInfo::new("old-account-password", SystemTime::now(), None).unwrap();
        let raw = PatchDb::open(directory.path().join("account.db"))
            .await
            .unwrap();
        let initial = Database::init(&account, false, None, None).unwrap();
        let db = TypedPatchDb::<Database>::load_or_init(raw, || async { Ok(initial) })
            .await
            .unwrap();
        let cache = SyncRwLock::new(account);
        let hostname = ServerHostname::new_from_input("concurrent-hostname".into()).unwrap();
        db.mutate(|db| hostname.save(db.as_public_mut().as_server_info_mut()))
            .await
            .result
            .unwrap();
        let hash = super::super::hash_password("new-account-password").unwrap();
        commit_password(&db, &cache, &hash, None).await.unwrap();
        let stored = db.peek().await;
        assert_eq!(
            ServerHostname::load(stored.as_public().as_server_info())
                .unwrap()
                .as_ref(),
            hostname.as_ref()
        );
        crate::auth::check_password(
            &stored.as_private().as_password().de().unwrap(),
            "new-account-password",
        )
        .unwrap();
        assert_eq!(cache.read().password, hash);
        assert_eq!(
            stored
                .as_private()
                .as_key_store()
                .as_local_certs()
                .as_root_cert()
                .de()
                .unwrap()
                .0,
            cache.read().root_ca_cert
        );
    }

    #[tokio::test]
    async fn loading_legacy_account_materializes_missing_luks_state() {
        let directory = tempfile::tempdir().unwrap();
        let account = AccountInfo::new("legacy-account-password", SystemTime::now(), None).unwrap();
        let mut initial =
            serde_json::to_value(Database::init(&account, false, None, None).unwrap()).unwrap();
        initial["private"]
            .as_object_mut()
            .unwrap()
            .remove("luksCredentials");
        initial["private"]
            .as_object_mut()
            .unwrap()
            .remove("fdeEnrollment");
        let raw = PatchDb::open(directory.path().join("legacy.db"))
            .await
            .unwrap();
        raw.put(&patch_db::json_ptr::ROOT, &initial).await.unwrap();
        let db = TypedPatchDb::<Database>::load(raw).await.unwrap();
        assert!(
            db.peek()
                .await
                .as_private()
                .as_luks_credentials()
                .de()
                .unwrap()
                .is_none()
        );
        assert!(
            db.peek()
                .await
                .as_private()
                .as_fde_enrollment()
                .de()
                .unwrap()
                .is_none()
        );
        let value = serde_json::to_value(db.peek().await).unwrap();
        assert!(
            value["private"]
                .as_object()
                .unwrap()
                .contains_key("luksCredentials")
        );
        assert!(
            value["private"]
                .as_object()
                .unwrap()
                .contains_key("fdeEnrollment")
        );
    }
}
