use super::{AnySigningKey, Signer};
use crate::prelude::*;
use crate::util::serde::Der64;

/// A missing identity is initialized durably before accepting handshakes.
pub type StoredIdentityKey = Option<Der64<AnySigningKey>>;

const IDENTITY_KEY: &str = "serverIdentityKey";

/// PatchDB owns the identity; malformed stored keys fail startup without rotation.
pub async fn load(db: &PatchDb) -> Result<Signer, Error> {
    let (_, key) = db
        .run_idempotent(|mut root| async move {
            let saved = root
                .as_object()
                .and_then(|object| object.get(IDENTITY_KEY))
                .cloned()
                .unwrap_or(Value::Null);
            if let Some(key) = from_value::<StoredIdentityKey>(saved)? {
                return Ok::<_, Error>((root, key.0));
            }
            let key = AnySigningKey::generate();
            root.as_object_mut()
                .ok_or_else(|| {
                    Error::new(
                        eyre!(rust_i18n::t!("sign.instance.invalid-database")),
                        ErrorKind::Database,
                    )
                })?
                .insert(IDENTITY_KEY.into(), to_value(&Der64::new_ref(&key))?);
            Ok((root, key))
        })
        .await?;
    Ok(key.into())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use patch_db::json_ptr::ROOT;

    use super::*;

    async fn open(path: &Path) -> PatchDb {
        let db = PatchDb::open(path).await.unwrap();
        if db.dump(&ROOT).await.value.is_null() {
            db.put(&ROOT, &imbl_value::json!({"sibling": {"preserve": true}}))
                .await
                .unwrap();
        }
        db
    }

    #[tokio::test]
    async fn identity_is_private_and_stable_after_reopening_patch_db() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("instance.db");
        let db = open(&path).await;
        let first = load(&db).await.unwrap().verifying_key().await.unwrap();
        assert_eq!(
            db.dump(&ROOT).await.value["sibling"]["preserve"],
            Value::Bool(true)
        );
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(db);
        let db = open(&path).await;
        assert_eq!(
            load(&db).await.unwrap().verifying_key().await.unwrap(),
            first
        );
    }

    #[tokio::test]
    async fn concurrent_initialization_persists_one_identity() {
        let temp = tempfile::tempdir().unwrap();
        let db = open(&temp.path().join("instance.db")).await;
        let keys = futures::future::join_all(
            (0..16).map(|_| async { load(&db).await.unwrap().verifying_key().await.unwrap() }),
        )
        .await;
        assert!(keys.iter().all(|key| key == &keys[0]));
        assert_eq!(
            load(&db).await.unwrap().verifying_key().await.unwrap(),
            keys[0]
        );
    }

    #[tokio::test]
    async fn malformed_database_key_is_not_rotated() {
        let temp = tempfile::tempdir().unwrap();
        let db = open(&temp.path().join("instance.db")).await;
        let pointer = "/serverIdentityKey"
            .parse::<patch_db::json_ptr::JsonPointer>()
            .unwrap();
        for invalid in [
            imbl_value::json!("not a key"),
            imbl_value::json!(42),
            imbl_value::json!({}),
        ] {
            db.put(&pointer, &invalid).await.unwrap();
            assert!(load(&db).await.is_err());
            assert_eq!(db.get_value(&pointer).await, invalid);
        }
    }

    async fn model_preserves_identity<T>()
    where
        T: patch_db::HasModel + Default + serde::Serialize + serde::de::DeserializeOwned,
    {
        let temp = tempfile::tempdir().unwrap();
        let raw = open(&temp.path().join("instance.db")).await;
        let mut old = to_value(&T::default()).unwrap();
        old.as_object_mut().unwrap().remove(IDENTITY_KEY);
        raw.put(&ROOT, &old).await.unwrap();
        let db = TypedPatchDb::<T>::load(raw).await.unwrap();
        let key = load(&db).await.unwrap().verifying_key().await.unwrap();
        let normalized = TypedPatchDb::<T>::load((*db).clone()).await.unwrap();
        assert_eq!(
            load(&normalized)
                .await
                .unwrap()
                .verifying_key()
                .await
                .unwrap(),
            key
        );
    }

    #[tokio::test]
    async fn registry_model_retains_the_identity_when_normalized() {
        model_preserves_identity::<crate::registry::RegistryDatabase>().await;
    }

    #[tokio::test]
    async fn tunnel_model_retains_the_identity_when_normalized() {
        model_preserves_identity::<crate::tunnel::db::TunnelDatabase>().await;
    }

    #[tokio::test]
    async fn independent_databases_get_independent_identities() {
        let temp = tempfile::tempdir().unwrap();
        let first = open(&temp.path().join("registry.db")).await;
        let second = open(&temp.path().join("tunnel.db")).await;
        assert_ne!(
            load(&first).await.unwrap().verifying_key().await.unwrap(),
            load(&second).await.unwrap().verifying_key().await.unwrap()
        );
    }
}
