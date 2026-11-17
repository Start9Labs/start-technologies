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
async fn fresh_setup_keeps_one_private_identity_across_account_creation_completion_and_reboot() {
    let config = tempfile::tempdir().unwrap();
    let first = startup_identity_at(config.path()).await.unwrap().unwrap();
    let account =
        AccountInfo::new_with_identity("password", SystemTime::now(), None, first.clone()).unwrap();
    let raw = PatchDb::in_memory();
    raw.put(
        &patch_db::json_ptr::ROOT,
        &Database::init(&account, false, None, None).unwrap(),
    )
    .await
    .unwrap();
    let db = TypedPatchDb::<Database>::load(raw).await.unwrap();
    let loaded = AccountInfo::load(&db.peek().await).unwrap();
    assert_eq!(loaded.server_identity_key.to_bytes(), first.to_bytes());
    let path = config.path().join("id.key.pem");
    assert!(!path.exists());
    complete_identity_at(&path, &loaded.server_identity_key, "installed")
        .await
        .unwrap();
    assert!(!setup_identity_path_at(&path).exists());
    let rebooted = startup_identity_at(config.path()).await.unwrap().unwrap();
    let before = StartupRpc::new(Some(first)).identity().unwrap();
    let after = StartupRpc::new(Some(rebooted)).identity().unwrap();
    assert!(!before.temporary);
    assert!(!after.temporary);
    let before = before.signer.verifying_key().await.unwrap();
    let after = after.signer.verifying_key().await.unwrap();
    assert_eq!(before, after);
    assert_eq!(
        fingerprint(&before),
        fingerprint(&AnySigningKey::Ed25519(loaded.server_identity_key).verifying_key())
    );
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
    let setup_path = setup_identity_path_at(&path);
    std::fs::remove_file(&setup_path).unwrap();
    let carried = startup.key_at(&path).unwrap();
    assert_eq!(carried.to_bytes(), key.to_bytes());
    persist_setup_identity_at(&path, &carried).await.unwrap();
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
async fn preserve_reinstall_replaces_a_stale_setup_key_without_changing_the_saved_account_key() {
    let live_config = tempfile::tempdir().unwrap();
    let live_key = startup_identity_at(live_config.path())
        .await
        .unwrap()
        .unwrap();
    let startup = StartupRpc::new(Some(live_key.clone()));
    let installed_config = tempfile::tempdir().unwrap();
    let path = installed_config.path().join("id.key.pem");
    let old_account = AccountInfo::new("password", SystemTime::now(), None).unwrap();
    persist_identity_at(&path, &old_account.server_identity_key)
        .await
        .unwrap();
    persist_identity_at(
        &setup_identity_path_at(&path),
        &old_account.server_identity_key,
    )
    .await
    .unwrap();

    persist_setup_identity_at(&path, &startup.key_at(&path).unwrap())
        .await
        .unwrap();
    let installed = startup_identity_at(installed_config.path())
        .await
        .unwrap()
        .unwrap();
    let fresh =
        AccountInfo::new_with_identity("password", SystemTime::now(), None, installed).unwrap();
    assert_eq!(fresh.server_identity_key.to_bytes(), live_key.to_bytes());
    assert_ne!(
        fresh.server_identity_key.to_bytes(),
        old_account.server_identity_key.to_bytes()
    );
    assert_eq!(
        load_signing_key(&path).unwrap().to_bytes(),
        old_account.server_identity_key.to_bytes()
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
        key.into(),
    )
    .await
    .unwrap();
    for identity in [
        init.server_identity(),
        setup.server_identity(),
        diagnostic.server_identity(),
    ] {
        let identity = identity.unwrap();
        assert!(!identity.temporary);
        assert_eq!(
            identity.signer.verifying_key().await.unwrap(),
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
    mirror_configured_identity_at(&path, &account.server_identity_key)
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
async fn interrupted_restore_cannot_lend_an_imported_account_key_to_fresh_setup() {
    let config = tempfile::tempdir().unwrap();
    let setup = startup_identity_at(config.path()).await.unwrap().unwrap();
    let restored = AccountInfo::new("password", SystemTime::now(), None).unwrap();
    assert_ne!(setup.to_bytes(), restored.server_identity_key.to_bytes());
    let path = config.path().join("id.key.pem");
    mirror_configured_identity_at(&path, &restored.server_identity_key)
        .await
        .unwrap();
    assert!(!path.exists());
    persist_identity_at(&path, &restored.server_identity_key)
        .await
        .unwrap();
    assert_eq!(
        startup_identity_at(config.path())
            .await
            .unwrap()
            .unwrap()
            .to_bytes(),
        setup.to_bytes()
    );
    complete_identity_at(&path, &restored.server_identity_key, "restored")
        .await
        .unwrap();
    assert_eq!(
        startup_identity_at(config.path())
            .await
            .unwrap()
            .unwrap()
            .to_bytes(),
        restored.server_identity_key.to_bytes()
    );
    assert!(!setup_identity_path_at(&path).exists());
}

#[tokio::test]
async fn forgetting_the_data_drive_drops_both_identity_scopes_before_fresh_setup() {
    let config = tempfile::tempdir().unwrap();
    let first = startup_identity_at(config.path()).await.unwrap().unwrap();
    let path = config.path().join("id.key.pem");
    complete_identity_at(&path, &first, "installed")
        .await
        .unwrap();
    persist_identity_at(&setup_identity_path_at(&path), &first)
        .await
        .unwrap();
    tokio::process::Command::new("bash")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../projects/start-os/build/lib/scripts/forget-server-identity"
        ))
        .arg(config.path())
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    assert!(!path.exists());
    assert!(!setup_identity_path_at(&path).exists());
    assert!(!config.path().join("disk.guid").exists());
    let fresh = startup_identity_at(config.path()).await.unwrap().unwrap();
    assert_ne!(fresh.to_bytes(), first.to_bytes());
}

#[tokio::test]
async fn unavailable_diagnostic_keys_are_marked_temporary_and_never_persisted_or_adopted() {
    let config = tempfile::tempdir().unwrap();
    let path = config.path().join("id.key.pem");
    std::fs::write(config.path().join("disk.guid"), "installed").unwrap();
    let fallback = diagnostic_identity_at(&path).await;
    assert!(fallback.temporary);
    let peer = StartupRpc::diagnostic(fallback).identity().unwrap();
    assert!(peer.temporary);
    assert!(!path.exists());
    assert!(!setup_identity_path_at(&path).exists());
    std::fs::remove_file(config.path().join("disk.guid")).unwrap();
    std::fs::write(setup_identity_path_at(&path), "not a private key").unwrap();
    assert!(diagnostic_identity_at(&path).await.temporary);
    assert_eq!(
        std::fs::read_to_string(setup_identity_path_at(&path)).unwrap(),
        "not a private key"
    );
}

#[tokio::test]
async fn malformed_identity_copies_do_not_rotate_an_account_or_an_incomplete_setup() {
    let config = tempfile::tempdir().unwrap();
    let path = config.path().join("id.key.pem");
    let setup_path = setup_identity_path_at(&path);
    std::fs::write(&setup_path, "not a private key").unwrap();
    assert!(startup_identity_at(config.path()).await.is_err());
    assert_eq!(
        std::fs::read_to_string(&setup_path).unwrap(),
        "not a private key"
    );
    std::fs::write(config.path().join("disk.guid"), "installed").unwrap();
    std::fs::write(&path, "not an account key").unwrap();
    assert!(startup_identity_at(config.path()).await.unwrap().is_none());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "not an account key"
    );
}

#[tokio::test]
async fn ota_copy_accepts_both_native_private_key_writers_including_pkcs8_v2() {
    let config = tempfile::tempdir().unwrap();
    let runtime = config.path().join("runtime.pem");
    let persistent = config.path().join("id.key.pem");
    let key = crate::db::model::private::generate_server_identity_key().0;
    let upgrade = include_str!("../../../../../../projects/start-os/build/lib/scripts/upgrade");
    let (functions, _) = upgrade.split_once("\nif [ \"$UID\"").unwrap();
    let command = format!("{functions}\npreserve_server_identity \"$1\" \"$2\"");
    for runtime_writer in [true, false] {
        if runtime_writer {
            crate::developer::write_signing_key(&key, &runtime)
                .await
                .unwrap();
        } else {
            persist_identity_at(&runtime, &key).await.unwrap();
        }
        tokio::process::Command::new("bash")
            .arg("-ec")
            .arg(&command)
            .arg("fixture")
            .arg(&runtime)
            .arg(&persistent)
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        assert_eq!(
            load_signing_key(&persistent).unwrap().to_bytes(),
            key.to_bytes()
        );
        assert_eq!(
            std::fs::metadata(&persistent).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
