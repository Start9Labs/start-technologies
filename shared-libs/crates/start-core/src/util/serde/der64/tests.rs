use ed25519_dalek::{SigningKey, VerifyingKey};
use openssl::pkey::PKey;
use serde::{Deserialize, Serialize};

use super::*;
use crate::util::serde::{IoFormat, Pem};

#[test]
fn der64_spki_is_standard_base64_with_padding() {
    let key = SigningKey::from_bytes(&[2; 32]).verifying_key();
    let expected = "MCowBQYDK2VwAyEAgTl3Dqh9F19Wo1Rmw0x+zMuNipG07jeiXfYPW4/Js5Q=";
    assert_eq!(Der64(key).to_string(), expected);
    assert_eq!(
        serde_json::to_string(&Der64(key)).unwrap(),
        format!("\"{expected}\"")
    );
    assert_eq!(expected.parse::<Der64<VerifyingKey>>().unwrap().0, key);
    assert_eq!(
        expected
            .trim_end_matches('=')
            .parse::<Der64<VerifyingKey>>()
            .unwrap()
            .0,
        key
    );
    for input in [
        "MCowBQYDK2VwAyEAgTl3Dqh9F19Wo1Rmw0x-zMuNipG07jeiXfYPW4_Js5Q",
        "MCowBQYDK2VwAyEAgTl3Dqh9F19Wo1Rmw0x-zMuNipG07jeiXfYPW4_Js5Q=",
    ] {
        let decoded = input.parse::<Der64<VerifyingKey>>().unwrap();
        assert_eq!(decoded.0, key);
        assert_eq!(decoded.to_string(), expected);
    }
}

#[test]
fn der64_signing_keys_roundtrip_in_storage_formats() {
    let key = SigningKey::from_bytes(&[42; 32]);
    for format in [IoFormat::Json, IoFormat::Cbor] {
        let encoded = format.to_vec(&Der64(key.clone())).unwrap();
        let decoded: Der64<SigningKey> = format.from_slice(&encoded).unwrap();
        assert_eq!(decoded.0, key);
    }
}

#[test]
fn der64_reads_legacy_pem_and_writes_canonical_der() {
    let key = SigningKey::from_bytes(&[42; 32]);
    let private_pem = serde_json::to_string(&Pem(key.clone())).unwrap();
    let public_pem = serde_json::to_string(&Pem(key.verifying_key())).unwrap();
    let private: Der64<SigningKey> = serde_json::from_str(&private_pem).unwrap();
    let public: Der64<VerifyingKey> = serde_json::from_str(&public_pem).unwrap();
    assert_eq!(private.0, key);
    assert_eq!(public.0, key.verifying_key());
    assert!(!serde_json::to_string(&private).unwrap().contains("BEGIN"));
    assert!(!serde_json::to_string(&public).unwrap().contains("BEGIN"));
}

#[test]
fn der64_pkcs8_interoperates_with_openssl() {
    for key in [
        PKey::generate_ed25519().unwrap(),
        crate::net::ssl::gen_nistp256().unwrap(),
    ] {
        let encoded = Der64(key.clone()).to_string();
        let decoded: Der64<PKey<Private>> = encoded.parse().unwrap();
        assert!(decoded.public_eq(&key));
        let legacy = Pem(key.clone()).to_string();
        let decoded: Der64<PKey<Private>> = legacy.parse().unwrap();
        assert!(decoded.public_eq(&key));
        assert_eq!(decoded.to_string(), encoded);
    }
}

#[test]
fn der64_field_adapter_matches_wrapper() {
    #[derive(Deserialize, Serialize)]
    struct Field {
        #[serde(with = "crate::util::serde::der64")]
        key: SigningKey,
    }
    let key = SigningKey::from_bytes(&[42; 32]);
    let value = serde_json::to_value(Field { key: key.clone() }).unwrap();
    assert_eq!(
        value["key"],
        serde_json::to_value(Der64(key.clone())).unwrap()
    );
    let decoded: Field = serde_json::from_value(value).unwrap();
    assert_eq!(decoded.key, key);
}

#[test]
fn der64_account_documents_preserve_sec1_bytes() {
    let key = crate::net::ssl::gen_nistp256().unwrap();
    let sec1 = key.ec_key().unwrap().private_key_to_der().unwrap();
    let document = pkcs8::Document::try_from(sec1.as_slice()).unwrap();
    let legacy =
        der::pem::encode_string("EC PRIVATE KEY", der::pem::LineEnding::LF, &sec1).unwrap();
    let decoded: Der64<pkcs8::Document> = legacy.parse().unwrap();
    assert_eq!(decoded.0.as_bytes(), sec1);
    let encoded = serde_json::to_string(&Der64(document)).unwrap();
    let decoded: Der64<pkcs8::Document> = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded.0.as_bytes(), sec1);
}

#[derive(Deserialize, Serialize, HasModel)]
#[model = "Model<Self>"]
struct AuthDatabase {
    keys: crate::auth::AuthKeys,
}

#[tokio::test]
async fn der64_auth_keys_normalize_during_typed_database_load() {
    use patch_db::json_ptr::ROOT;

    use crate::auth::Session;
    let key =
        crate::sign::AnyVerifyingKey::Ed25519(SigningKey::from_bytes(&[42; 32]).verifying_key());
    let session = Session {
        name: Some("existing sign-in".into()),
        logged_in: chrono::Utc::now(),
        last_active: chrono::Utc::now(),
        user_agent: Some("existing client".into()),
    };
    let db = PatchDb::in_memory();
    db.put(
        &ROOT,
        &imbl_value::json!({ "keys": { key.to_string(): session } }),
    )
    .await
    .unwrap();
    let db = TypedPatchDb::<AuthDatabase>::load(db).await.unwrap();
    let peek = db.peek().await;
    assert!(peek.as_keys().as_idx(&key.to_string().into()).is_none());
    let stored = peek
        .as_keys()
        .as_idx(&key.interned_der64())
        .unwrap()
        .de()
        .unwrap();
    assert_eq!(stored.name, session.name);
    assert_eq!(stored.logged_in, session.logged_in);
    assert_eq!(stored.last_active, session.last_active);
    assert_eq!(stored.user_agent, session.user_agent);
}

#[test]
fn der64_rejects_invalid_encoding_and_wrong_der_type() {
    for value in [
        "",
        "not base64!",
        "AA",
        "-----BEGIN PRIVATE KEY-----\ninvalid\n-----END PRIVATE KEY-----",
    ] {
        assert!(value.parse::<Der64<SigningKey>>().is_err(), "{value}");
    }
    let key = SigningKey::from_bytes(&[42; 32]);
    assert!(
        Der64(key.clone())
            .to_string()
            .parse::<Der64<VerifyingKey>>()
            .is_err()
    );
    assert!(
        Der64(key.verifying_key())
            .to_string()
            .parse::<Der64<SigningKey>>()
            .is_err()
    );
}
