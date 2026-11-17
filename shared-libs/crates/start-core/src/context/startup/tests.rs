use std::os::unix::fs::PermissionsExt;
use std::time::SystemTime;

use super::*;
use crate::account::AccountInfo;
use crate::db::model::Database;
use crate::sign::fingerprint::fingerprint;

async fn startup_identity_at(config: &Path) -> Result<Option<SigningKey>, Error> {
    super::startup_identity_at(&config.join("id.key.pem")).await
}

#[tokio::test]
async fn fresh_setup_keeps_one_private_identity_across_contexts_account_creation_and_reboot() {
    let config = tempfile::tempdir().unwrap();
    let first = startup_identity_at(config.path()).await.unwrap().unwrap();
    let account =
        AccountInfo::new_with_identity("password", SystemTime::now(), None, first.clone()).unwrap();
    let db = crate::prelude::TypedPatchDb::<Database>::load_unchecked(
        crate::prelude::PatchDb::open(config.path().join("account.db"))
            .await
            .unwrap(),
    );
    db.put(
        &patch_db::json_ptr::ROOT,
        &Database::init(&account, false, None, None).unwrap(),
    )
    .await
    .unwrap();
    let loaded = AccountInfo::load(&db.peek().await).unwrap();
    assert_eq!(loaded.server_identity_key.to_bytes(), first.to_bytes());
    std::fs::write(config.path().join("disk.guid"), "installed").unwrap();
    let rebooted = startup_identity_at(config.path()).await.unwrap().unwrap();
    let before = StartupRpc::new(Some(first))
        .identity()
        .unwrap()
        .verifying_key()
        .await
        .unwrap();
    let after = StartupRpc::new(Some(rebooted))
        .identity()
        .unwrap()
        .verifying_key()
        .await
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        fingerprint(&before),
        fingerprint(&AnySigningKey::Ed25519(loaded.server_identity_key).verifying_key())
    );
    let path = config.path().join("id.key.pem");
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[tokio::test]
async fn setup_keeps_its_identity_when_installation_replaces_the_config_mount() {
    let config = tempfile::tempdir().unwrap();
    let key = startup_identity_at(config.path()).await.unwrap().unwrap();
    let startup = StartupRpc::new(Some(key.clone()));
    let path = config.path().join("id.key.pem");
    std::fs::remove_file(&path).unwrap();
    let carried = startup.key_at(&path).unwrap();
    assert_eq!(carried.to_bytes(), key.to_bytes());
    persist_identity_at(&path, &carried).await.unwrap();
    assert_eq!(
        startup_identity_at(config.path())
            .await
            .unwrap()
            .unwrap()
            .to_bytes(),
        key.to_bytes()
    );
}

#[tokio::test]
async fn init_setup_and_diagnostic_contexts_advertise_the_same_persisted_key() {
    use crate::context::config::ServerConfig;
    use crate::context::{DiagnosticContext, InitContext, SetupContext};
    use crate::middleware::auth::signature::SignatureAuthContext;
    use crate::net::dns::DnsController;
    use crate::util::sync::Watch;

    let config_dir = tempfile::tempdir().unwrap();
    let key = startup_identity_at(config_dir.path())
        .await
        .unwrap()
        .unwrap();
    let expected = key.verifying_key();
    let (_, server) = crate::setup::test::context();
    let config = ServerConfig::default();
    let dns = std::sync::Arc::new(DnsController::new(Watch::new(Default::default())));
    let init = InitContext::init(&config, dns.clone(), Some(key.clone()))
        .await
        .unwrap();
    let setup =
        SetupContext::init(&server, config.clone(), dns.clone(), Some(key.clone())).unwrap();
    let diagnostic = DiagnosticContext::init(
        &config,
        None,
        Error::new(eyre!("identity fixture"), ErrorKind::Unknown),
        dns,
        Some(key),
    )
    .await
    .unwrap();
    for identity in [
        init.server_identity(),
        setup.server_identity(),
        diagnostic.server_identity(),
    ] {
        assert_eq!(
            identity.unwrap().verifying_key().await.unwrap(),
            crate::sign::AnyVerifyingKey::Ed25519(expected)
        );
    }
}

#[tokio::test]
async fn migration_waits_for_the_existing_account_key_instead_of_advertising_a_replacement() {
    let config = tempfile::tempdir().unwrap();
    std::fs::write(config.path().join("disk.guid"), "installed").unwrap();
    let pending = startup_identity_at(config.path()).await.unwrap();
    assert!(pending.is_none());
    let startup = StartupRpc::new(pending);
    let path = config.path().join("id.key.pem");
    assert!(startup.key_at(&path).is_err());
    assert!(!path.exists());
    let account = AccountInfo::new("password", SystemTime::now(), None).unwrap();
    persist_identity_at(&path, &account.server_identity_key)
        .await
        .unwrap();
    assert_eq!(
        startup.key_at(&path).unwrap().to_bytes(),
        account.server_identity_key.to_bytes()
    );
    assert_eq!(
        startup_identity_at(config.path())
            .await
            .unwrap()
            .unwrap()
            .to_bytes(),
        account.server_identity_key.to_bytes()
    );
}

#[tokio::test]
async fn restoring_an_account_replaces_the_copy_and_keeps_its_original_key() {
    let config = tempfile::tempdir().unwrap();
    let setup = startup_identity_at(config.path()).await.unwrap().unwrap();
    let restored = AccountInfo::new("password", SystemTime::now(), None).unwrap();
    assert_ne!(setup.to_bytes(), restored.server_identity_key.to_bytes());
    let path = config.path().join("id.key.pem");
    persist_identity_at(&path, &restored.server_identity_key)
        .await
        .unwrap();
    std::fs::write(config.path().join("disk.guid"), "restored").unwrap();
    assert_eq!(
        startup_identity_at(config.path())
            .await
            .unwrap()
            .unwrap()
            .to_bytes(),
        restored.server_identity_key.to_bytes()
    );
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[tokio::test]
async fn malformed_unconfigured_identity_is_not_replaced_with_a_new_key() {
    let config = tempfile::tempdir().unwrap();
    let path = config.path().join("id.key.pem");
    std::fs::write(&path, "not a private key").unwrap();
    assert!(startup_identity_at(config.path()).await.is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "not a private key");
    std::fs::write(config.path().join("disk.guid"), "installed").unwrap();
    assert!(startup_identity_at(config.path()).await.unwrap().is_none());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "not a private key");
}
