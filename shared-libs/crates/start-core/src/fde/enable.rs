//! Pending enablement retains DEFAULT until durable compatible EFI activation.
//! Initial administrator installation is distinct from account password replacement.

use serde::{Deserialize, Serialize};

use crate::account::{AccountInfo, LuksCredentials};
use crate::db::model::{Database, DatabaseModel};
use crate::disk::fde::{AdministratorInstallationPlan, DefaultKeyslots, LuksPool, VolumeKeys};
use crate::prelude::*;
use crate::util::sync::SyncRwLock;

/// Retains original DEFAULT authority across administrator installation and later password changes.
#[derive(Clone, Deserialize, Serialize)]
#[serde(
    tag = "stage",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum AdministratorSetup {
    Installing {
        plan: AdministratorInstallationPlan,
        target_hash: String,
    },
    Installed {
        default_slots: DefaultKeyslots,
    },
}

impl AdministratorSetup {
    fn default_slots(&self) -> DefaultKeyslots {
        match self {
            Self::Installing { plan, .. } => plan.original_default_slots(),
            Self::Installed { default_slots } => default_slots.clone(),
        }
    }
}

fn invalid_state() -> Error {
    Error::new(
        eyre!(t!("error.invalid-request")),
        ErrorKind::InvalidRequest,
    )
}

async fn persist_setup(
    db: &TypedPatchDb<Database>,
    setup: &AdministratorSetup,
) -> Result<(), Error> {
    db.mutate(|db| set_setup(db, setup)).await.result
}

fn set_setup(db: &mut DatabaseModel, setup: &AdministratorSetup) -> Result<(), Error> {
    db.as_private_mut()
        .as_fde_enrollment_mut()
        .mutate(|enrollment| {
            enrollment.as_mut().ok_or_else(invalid_state)?.administrator = Some(setup.clone());
            Ok(())
        })
}

/// Re-authenticates the current password; the caller holds `password_changes`.
/// The caller verifies firmware enrollment before administrator installation.
pub(crate) async fn prepare_administrator(
    db: &TypedPatchDb<Database>,
    account: &SyncRwLock<AccountInfo>,
    pool: &LuksPool,
    password: &str,
) -> Result<(), Error> {
    let memory = pool.protect_secret_handling()?;
    crate::disk::fde::validate_new_password(password)?;
    let snapshot = db.peek().await;
    crate::auth::check_password(&snapshot.as_private().as_password().de()?, password)?;
    let enrollment = snapshot
        .as_private()
        .as_fde_enrollment()
        .de()?
        .ok_or_else(invalid_state)?;
    let mut setup = match enrollment.administrator {
        Some(setup) => setup,
        None => {
            if snapshot.as_private().as_luks_credentials().de()?.is_some() {
                return Err(invalid_state());
            }
            let defaults = pool.identify_default_slots().await?;
            let setup = AdministratorSetup::Installing {
                plan: pool.plan_administrator_installation(&defaults).await?,
                target_hash: snapshot.as_private().as_password().de()?,
            };
            persist_setup(db, &setup).await?;
            setup
        }
    };
    let keys = pending_keys(pool, &setup).await?;
    loop {
        match setup {
            AdministratorSetup::Installing { plan, target_hash } => {
                if crate::auth::check_password(&target_hash, password).is_err() {
                    pool.abort_administrator_installation(&plan, &keys).await?;
                    setup = AdministratorSetup::Installing {
                        plan: pool
                            .plan_administrator_installation(&plan.original_default_slots())
                            .await?,
                        target_hash: db.peek().await.as_private().as_password().de()?,
                    };
                    persist_setup(db, &setup).await?;
                    continue;
                }
                pool.install_administrator(&plan, password, &keys).await?;
                let admin = plan.new_admin_slots();
                setup = AdministratorSetup::Installed {
                    default_slots: plan.original_default_slots(),
                };
                db.mutate(|db| {
                    let private = db.as_private_mut();
                    private
                        .as_luks_credentials_mut()
                        .ser(&Some(LuksCredentials::Stable {
                            admin: admin.clone(),
                        }))?;
                    set_setup(db, &setup)
                })
                .await
                .result?;
            }
            AdministratorSetup::Installed { .. } => {
                let result = crate::account::password::change_encrypted_password(
                    db, account, pool, &keys, password,
                )
                .await;
                crate::account::password::finish_password_change(db, password, result).await?;
                return memory.check();
            }
        }
    }
}

/// Restores missing volatile unlock keys from persisted pending-enable DEFAULT authority.
/// Protected boots require retained unlock keys.
pub(crate) async fn restore_pending_keys(
    db: &TypedPatchDb<Database>,
    guid: &str,
) -> Result<(), Error> {
    let snapshot = db.peek().await;
    let Some(enrollment) = snapshot.as_private().as_fde_enrollment().de()? else {
        return Ok(());
    };
    let Some(setup) = enrollment.administrator else {
        return Ok(());
    };
    let pool = LuksPool::system(guid)?;
    let memory = pool.protect_secret_handling()?;
    pending_keys(&pool, &setup).await?;
    memory.check()
}

/// The caller protects memory and holds persisted pending-enable authority.
pub(crate) async fn pending_keys(
    pool: &LuksPool,
    setup: &AdministratorSetup,
) -> Result<VolumeKeys, Error> {
    match pool.cached_unlock_keys().await? {
        Some(keys) => Ok(keys),
        None => {
            require_pending_recovery(super::initramfs::BootMode::current().await?)?;
            let keys = pool.capture_default_keys(&setup.default_slots()).await?;
            pool.cache_unlock_keys(&keys).await?;
            Ok(keys)
        }
    }
}

fn require_pending_recovery(boot: super::initramfs::BootMode) -> Result<(), Error> {
    match boot {
        super::initramfs::BootMode::Ordinary => Ok(()),
        super::initramfs::BootMode::Protected => Err(Error::new(
            eyre!(t!("account.volume-keys-unavailable")),
            ErrorKind::DiskManagement,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_boot_never_recovers_pending_keys_through_default() {
        require_pending_recovery(super::super::initramfs::BootMode::Ordinary).unwrap();
        assert!(require_pending_recovery(super::super::initramfs::BootMode::Protected).is_err());
    }
}
