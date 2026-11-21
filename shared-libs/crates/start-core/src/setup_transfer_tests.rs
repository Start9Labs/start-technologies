use serde_json::{Value as JsonValue, json};

use super::*;

fn signing_record() -> JsonValue {
    json!({
        "image_key": "opaque-image-key",
        "image_certificate": "opaque-image-certificate",
        "manifest_key": "opaque-manifest-key",
        "policy_key": "opaque-policy-key",
        "approval_code": "pending-firmware-code",
        "futureField": {"preserved": true}
    })
}

fn source_database(administrator_stage: &str, credential_stage: &str) -> JsonValue {
    let slots = json!({"volumes": [
        {"id": {"mapperName": "STARTOS_SOURCE_main", "uuid": "source-main"}, "slots": [0]},
        {"id": {"mapperName": "STARTOS_SOURCE_package-data", "uuid": "source-packages"}, "slots": [0]}
    ]});
    let administrator = match administrator_stage {
        "installing" => json!({
            "stage": "installing",
            "plan": {"defaults": slots, "baseSlots": [[0], [0]], "targetSlots": [1, 1]},
            "targetHash": "uncommitted-administrator-hash"
        }),
        "installed" => json!({"stage": "installed", "defaultSlots": slots}),
        _ => unreachable!(),
    };
    let plan = json!({"admin": slots, "baseSlots": [[0], [0]], "targetSlots": [1, 1]});
    let credentials = match credential_stage {
        "stable" => json!({"stage": "stable", "admin": slots}),
        "installing" => {
            json!({"stage": "installing", "plan": plan, "targetHash": "uncommitted-password-hash"})
        }
        "retiring" => json!({"stage": "retiring", "plan": plan}),
        _ => unreachable!(),
    };
    serde_json::from_value::<crate::account::LuksCredentials>(credentials.clone()).unwrap();
    serde_json::from_value::<crate::fde::enable::AdministratorSetup>(administrator.clone())
        .unwrap();
    let mut enrollment = signing_record();
    enrollment["administrator"] = administrator;
    json!({
        "private": {
            "fdeEnrollment": enrollment,
            "luksCredentials": credentials,
            "password": "committed-account-hash",
            "unrelated": ["account-and-package-state"]
        },
        "public": {"unchanged": true}
    })
}

fn assert_reset(value: &JsonValue) {
    assert!(value["private"]["luksCredentials"].is_null());
    assert_eq!(value["private"]["fdeEnrollment"], signing_record());
    assert_eq!(value["private"]["password"], "committed-account-hash");
    assert_eq!(
        value["private"]["unrelated"],
        json!(["account-and-package-state"])
    );
    assert_eq!(value["public"], json!({"unchanged": true}));
}

#[test]
fn transferred_volume_reset_discards_every_slot_stage_without_decoding_signing_keys() {
    for administrator in ["installing", "installed"] {
        for credentials in ["stable", "installing", "retiring"] {
            let source = source_database(administrator, credentials);
            let mut model = DatabaseModel::from(to_value(&source).unwrap());
            reset_transferred_volume_authority(&mut model).unwrap();
            assert_reset(&serde_json::to_value(&model).unwrap());
            reset_transferred_volume_authority(&mut model).unwrap();
            assert_reset(&serde_json::to_value(&model).unwrap());
            assert!(!source["private"]["luksCredentials"].is_null());
            assert!(!source["private"]["fdeEnrollment"]["administrator"].is_null());
        }
    }
}

#[test]
fn ordinary_transfer_reset_does_not_create_enrollment() {
    for enrollment in [None, Some(JsonValue::Null)] {
        let mut value = json!({"private": {"password": "ordinary-account-hash"}});
        if let Some(enrollment) = enrollment {
            value["private"]["fdeEnrollment"] = enrollment;
        }
        let mut model = DatabaseModel::from(to_value(&value).unwrap());
        reset_transferred_volume_authority(&mut model).unwrap();
        let actual = serde_json::to_value(model).unwrap();
        assert!(actual["private"]["fdeEnrollment"].is_null());
        assert!(actual["private"]["luksCredentials"].is_null());
        assert_eq!(actual["private"]["password"], "ordinary-account-hash");
    }
}

#[tokio::test]
async fn copied_database_reset_persists_without_changing_the_attached_source() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("source");
    let destination_path = directory.path().join("destination");
    tokio::fs::create_dir(&source_path).await.unwrap();
    let source_value = source_database("installed", "retiring");
    let source = PatchDb::open(source_path.join("embassy.db")).await.unwrap();
    source.put(&ROOT, &source_value).await.unwrap();
    source.close().await;

    dir_copy(&source_path, &destination_path, None)
        .await
        .unwrap();
    let destination = PatchDb::open(destination_path.join("embassy.db"))
        .await
        .unwrap();
    let typed = TypedPatchDb::<Database>::load_unchecked(destination.clone());
    typed
        .mutate(reset_transferred_volume_authority)
        .await
        .result
        .unwrap();
    drop(typed);
    destination.close().await;

    let destination = PatchDb::open(destination_path.join("embassy.db"))
        .await
        .unwrap();
    assert_reset(&destination.get::<JsonValue, _, _>(&ROOT).await.unwrap());
    destination.close().await;
    let source = PatchDb::open(source_path.join("embassy.db")).await.unwrap();
    assert_eq!(
        source.get::<JsonValue, _, _>(&ROOT).await.unwrap(),
        source_value
    );
    source.close().await;
}
