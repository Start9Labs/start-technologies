use patch_db::json_ptr::ROOT;

use super::*;
use crate::backup::os::OsBackup;
use crate::db::model::Database;

#[tokio::test]
async fn server_identity_key_retains_the_persisted_key_and_fingerprint() {
    let account = AccountInfo::new("identity-test", SystemTime::now(), None).unwrap();
    let initial = Database::init(&account, false, None, None).unwrap();
    let raw = PatchDb::in_memory();
    raw.put(&ROOT, &initial).await.unwrap();
    let stored = raw.dump(&ROOT).await.value["private"]["developerKey"].clone();
    assert!(stored.is_string());
    assert!(
        raw.dump(&ROOT).await.value["private"]
            .as_object()
            .unwrap()
            .get("serverIdentityKey")
            .is_none()
    );
    let db = TypedPatchDb::<Database>::load(raw).await.unwrap();
    let loaded = AccountInfo::load(&db.peek().await).unwrap();
    assert_eq!(
        loaded.server_identity_key.to_bytes(),
        account.server_identity_key.to_bytes()
    );
    assert_eq!(
        loaded.server_identity_key.verifying_key(),
        account.server_identity_key.verifying_key()
    );
    db.mutate(|db| loaded.save(db)).await.result.unwrap();
    assert_eq!(
        db.dump(&ROOT).await.value["private"]["developerKey"],
        stored
    );
}

#[test]
fn server_identity_key_retains_the_backup_format_and_value() {
    let backup = OsBackup {
        account: AccountInfo::new("identity-test", SystemTime::now(), None).unwrap(),
        ui: Value::Null,
    };
    let encoded = to_value(&backup).unwrap();
    assert_eq!(encoded["version"], imbl_value::json!(2));
    assert!(encoded["compat_s9pk_key"].is_string());
    assert!(
        !encoded
            .as_object()
            .unwrap()
            .contains_key("server_identity_key")
    );
    let restored: OsBackup = from_value(encoded).unwrap();
    assert_eq!(
        restored.account.server_identity_key.to_bytes(),
        backup.account.server_identity_key.to_bytes()
    );
}
