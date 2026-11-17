use super::*;
use crate::account::{AccountInfo, LuksCredentials};
use crate::db::model::Database;
use crate::fde::enable::{AdministratorSetup, prepare_administrator};
use crate::prelude::{PatchDb, TypedPatchDb};
use crate::util::sync::SyncRwLock;

#[tokio::test]
#[ignore = "requires native cryptsetup, ukify, root user keyring and protected RAM in an exclusive managed guest"]
async fn real_pending_enable_luks1_recovers_initial_administrator_and_password_changes() {
    pending_enable("luks1").await;
}

#[tokio::test]
#[ignore = "requires native cryptsetup, ukify, root user keyring and protected RAM in an exclusive managed guest"]
async fn real_pending_enable_luks2_recovers_initial_administrator_and_password_changes() {
    pending_enable("luks2").await;
}

async fn lose_retained_keys(pool: &LuksPool) {
    let keys = pool.cached_unlock_keys().await.unwrap().unwrap();
    let ring = keyring::RootKeyring::open().unwrap();
    for key in &keys.volumes {
        ring.remove(&key.id).unwrap();
    }
    drop(keys);
    assert!(pool.cached_unlock_keys().await.unwrap().is_none());
}

fn block_shadow_writes() -> GeneralBoxedGuard {
    let directory = PathBuf::from("/media/startos/config/overlay/etc");
    let backup = directory.with_file_name("unavailable-etc");
    std::fs::rename(&directory, &backup).unwrap();
    std::fs::write(&directory, b"blocks the shadow parent").unwrap();
    GeneralBoxedGuard::new(move || {
        let _ = std::fs::remove_file(&directory);
        let _ = std::fs::rename(&backup, &directory);
    })
}

async fn pending_enable(format: &str) {
    let directory = tempfile::tempdir_in("/dev/shm").unwrap();
    let root = directory.path();
    let mut pool = format_real_pool_with_password(root, format, DEFAULT_PASSWORD).await;
    let memory = pool.protect_secret_handling().unwrap();
    let defaults = pool.identify_default_slots().await.unwrap();
    let initial_keys = pool.capture_default_keys(&defaults).await.unwrap();
    let key_hashes = initial_keys
        .volumes
        .each_ref()
        .map(|key| openssl::sha::sha256(&key.bytes));
    drop(initial_keys);
    let account = AccountInfo::new(NEW, std::time::SystemTime::now(), None).unwrap();
    let raw = PatchDb::open(root.join("account.db")).await.unwrap();
    let initial = Database::init(&account, false, None, None).unwrap();
    let db = TypedPatchDb::<Database>::load_or_init(raw, || async { Ok(initial) })
        .await
        .unwrap();
    let cache = SyncRwLock::new(account);
    crate::auth::write_shadow(NEW).await.unwrap();
    let enrollment = crate::fde::enrollment::Enrollment::generate(Path::new("/run"))
        .await
        .unwrap();
    db.mutate(|db| {
        db.as_private_mut()
            .as_fde_enrollment_mut()
            .ser(&Some(enrollment.clone()))
    })
    .await
    .result
    .unwrap();
    drop(enrollment);

    let failure = root.join("cryptsetup-failure");
    std::fs::write(&failure, format!("#!/bin/sh\ncase \"$*\" in *package-data*) if [ \"$1\" = luksAddKey ] && [ -e '{0}/fail-add' ]; then exit 5; fi; if [ \"$1\" = luksKillSlot ] && [ -e '{0}/fail-kill' ]; then exit 5; fi;; esac\nexec /usr/sbin/cryptsetup \"$@\"\n", root.display())).unwrap();
    std::fs::set_permissions(&failure, std::fs::Permissions::from_mode(0o700)).unwrap();
    pool.cryptsetup = failure;
    std::fs::write(root.join("fail-add"), b"").unwrap();
    assert!(
        prepare_administrator(&db, &cache, &pool, NEW)
            .await
            .is_err()
    );
    assert!(
        db.peek()
            .await
            .as_private()
            .as_luks_credentials()
            .de()
            .unwrap()
            .is_none()
    );
    let enrollment = db
        .peek()
        .await
        .as_private()
        .as_fde_enrollment()
        .de()
        .unwrap()
        .unwrap();
    let Some(AdministratorSetup::Installing { plan, .. }) = enrollment.administrator else {
        panic!("durable initial intent");
    };
    assert_eq!(plan.original_default_slots(), defaults);
    assert!(
        pool.metadata(&pool.devices[0])
            .await
            .unwrap()
            .slots
            .contains(&plan.allocation.target_slots[0])
    );
    assert!(
        !pool
            .metadata(&pool.devices[1])
            .await
            .unwrap()
            .slots
            .contains(&plan.allocation.target_slots[1])
    );
    drop(plan);
    std::fs::remove_file(root.join("fail-add")).unwrap();
    pool.verify_password(DEFAULT_PASSWORD).await.unwrap();
    assert_shadow(NEW, OTHER);

    crate::account::password::commit_password(
        &db,
        &cache,
        &AccountInfo::new(OTHER, std::time::SystemTime::now(), None)
            .unwrap()
            .password,
        None,
    )
    .await
    .unwrap();
    lose_retained_keys(&pool).await;
    drop(db);
    let db = TypedPatchDb::<Database>::load(PatchDb::open(root.join("account.db")).await.unwrap())
        .await
        .unwrap();
    prepare_administrator(&db, &cache, &pool, OTHER)
        .await
        .unwrap();
    let Some(LuksCredentials::Stable { admin }) = db
        .peek()
        .await
        .as_private()
        .as_luks_credentials()
        .de()
        .unwrap()
    else {
        panic!("installed administrator authority");
    };
    assert!(pool.accepts_admin_password(&admin, OTHER).await.unwrap());
    pool.verify_password(DEFAULT_PASSWORD).await.unwrap();
    pool.verify_password(MANUAL).await.unwrap();
    assert!(pool.verify_password(NEW).await.is_err());

    assert_shadow(OTHER, NEW);
    let retained = pool.cached_unlock_keys().await.unwrap().unwrap();
    std::fs::write(root.join("fail-kill"), b"").unwrap();
    let unavailable = block_shadow_writes();
    let result =
        crate::account::password::change_encrypted_password(&db, &cache, &pool, &retained, NEW)
            .await;
    let original_error = result.as_ref().err().unwrap().to_string();
    assert_eq!(
        crate::account::password::finish_password_change(&db, NEW, result)
            .await
            .unwrap_err()
            .to_string(),
        original_error
    );
    drop(unavailable);
    assert_shadow(OTHER, NEW);
    assert_eq!(
        prepare_administrator(&db, &cache, &pool, NEW)
            .await
            .unwrap_err()
            .to_string(),
        original_error
    );
    assert_shadow(NEW, OTHER);
    std::fs::remove_file(root.join("fail-kill")).unwrap();
    prepare_administrator(&db, &cache, &pool, NEW)
        .await
        .unwrap();
    drop(retained);
    let Some(LuksCredentials::Stable { admin: current }) = db
        .peek()
        .await
        .as_private()
        .as_luks_credentials()
        .de()
        .unwrap()
    else {
        panic!("replacement administrator authority");
    };
    assert_ne!(admin, current);
    let Some(AdministratorSetup::Installed { default_slots }) = db
        .peek()
        .await
        .as_private()
        .as_fde_enrollment()
        .de()
        .unwrap()
        .unwrap()
        .administrator
    else {
        panic!("independent DEFAULT authority");
    };
    assert_eq!(default_slots, defaults);
    lose_retained_keys(&pool).await;
    drop(db);
    let db = TypedPatchDb::<Database>::load(PatchDb::open(root.join("account.db")).await.unwrap())
        .await
        .unwrap();
    let setup = db
        .peek()
        .await
        .as_private()
        .as_fde_enrollment()
        .de()
        .unwrap()
        .unwrap()
        .administrator
        .unwrap();
    let retained = crate::fde::enable::pending_keys(&pool, &setup)
        .await
        .unwrap();
    let unavailable = block_shadow_writes();
    let result =
        crate::account::password::change_encrypted_password(&db, &cache, &pool, &retained, OTHER)
            .await;
    assert!(result.is_ok());
    assert!(
        crate::account::password::finish_password_change(&db, OTHER, result)
            .await
            .is_err()
    );
    drop(unavailable);
    assert_shadow(NEW, OTHER);
    let before_retry = db
        .peek()
        .await
        .as_private()
        .as_luks_credentials()
        .de()
        .unwrap();
    prepare_administrator(&db, &cache, &pool, OTHER)
        .await
        .unwrap();
    assert_shadow(OTHER, NEW);
    assert_eq!(
        retained
            .volumes
            .each_ref()
            .map(|key| openssl::sha::sha256(&key.bytes)),
        key_hashes
    );
    drop(retained);
    pool.verify_password(DEFAULT_PASSWORD).await.unwrap();
    pool.verify_password(MANUAL).await.unwrap();
    pool.verify_password(OTHER).await.unwrap();
    assert!(pool.verify_password(NEW).await.is_err());
    let Some(LuksCredentials::Stable { admin: after_retry }) = db
        .peek()
        .await
        .as_private()
        .as_luks_credentials()
        .de()
        .unwrap()
    else {
        panic!("stable retry");
    };
    let Some(LuksCredentials::Stable {
        admin: before_retry,
    }) = before_retry
    else {
        panic!("committed reset");
    };
    assert_eq!(after_retry, before_retry);
    lose_retained_keys(&pool).await;
    memory.check().unwrap();
}
