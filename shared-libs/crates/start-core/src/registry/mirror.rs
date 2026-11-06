use std::collections::BTreeMap;
use std::sync::{Arc, Weak};

use patch_db::json_ptr::JsonPointer;
use patch_db::{ModelExt, PatchDb};

use crate::prelude::*;
use crate::registry::RegistryDatabase;
use crate::registry::package::index::Reader;
use crate::sign::AnyVerifyingKey;
use crate::util::future::NonDetachingJoinHandle;

/// `None` selects the shared public mirror.
pub type IndexMirrors = tokio::sync::Mutex<BTreeMap<Option<InternedString>, Weak<IndexMirror>>>;

pub struct IndexMirror {
    pub db: PatchDb,
    _sync: NonDetachingJoinHandle<()>,
}

impl IndexMirror {
    async fn spawn(
        main: &TypedPatchDb<RegistryDatabase>,
        key: Option<AnyVerifyingKey>,
    ) -> Result<Arc<Self>, Error> {
        let (dump, mut sub) = main.dump_and_sub(JsonPointer::default()).await;
        let mut root = dump.value;
        let visible = visible_root(&root, key.as_ref())?;
        let db = PatchDb::in_memory();
        db.put(&JsonPointer::<&str>::default(), &visible).await?;
        let mirror = db.clone();
        let sync = tokio::spawn(async move {
            while let Some(rev) = sub.recv().await {
                let res = async {
                    patch_db::json_patch::patch(&mut root, &rev.patch)
                        .map_err(|e| Error::new(eyre!("{e}"), ErrorKind::Database))?;
                    let next = visible_root(&root, key.as_ref())?;
                    mirror.put(&JsonPointer::<&str>::default(), &next).await?;
                    Ok::<_, Error>(())
                }
                .await;
                if let Err(e) = res {
                    tracing::error!("Error mirroring registry index: {e}");
                    tracing::debug!("{e:?}");
                    break;
                }
            }
        });
        Ok(Arc::new(Self {
            db,
            _sync: sync.into(),
        }))
    }
}

/// Public-only readers share the public mirror until they resubscribe.
pub fn mirror_key(
    db: &Model<RegistryDatabase>,
    key: Option<AnyVerifyingKey>,
) -> Result<Option<AnyVerifyingKey>, Error> {
    Ok(match Reader::resolve(db, key.as_ref())? {
        Some(reader)
            if reader.admin || db.as_index().as_package().grants_private_access(&reader)? =>
        {
            key
        }
        _ => None,
    })
}

pub async fn index_mirror(
    mirrors: &IndexMirrors,
    main: &TypedPatchDb<RegistryDatabase>,
    key: Option<AnyVerifyingKey>,
) -> Result<Arc<IndexMirror>, Error> {
    let identity = key.as_ref().map(|key| key.interned_der64());
    let mut mirrors = mirrors.lock().await;
    if let Some(mirror) = mirrors.get(&identity).and_then(Weak::upgrade) {
        return Ok(mirror);
    }
    let mirror = IndexMirror::spawn(main, key).await?;
    mirrors.retain(|_, m| m.strong_count() > 0);
    mirrors.insert(identity, Arc::downgrade(&mirror));
    Ok(mirror)
}

fn visible_root(root: &Value, key: Option<&AnyVerifyingKey>) -> Result<Value, Error> {
    let db: Model<RegistryDatabase> = root.clone().into();
    let reader = Reader::resolve(&db, key)?;
    let mut index = db.into_index();
    index.as_package_mut().retain_visible(reader.as_ref())?;
    Ok(imbl_value::json!({ "index": index.into_value() }))
}

#[cfg(test)]
mod tests {
    use imbl_value::json;

    use super::*;
    use crate::rpc_continuations::Guid;

    fn key() -> AnyVerifyingKey {
        crate::sign::AnySigningKey::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[7; 32]))
            .verifying_key()
    }

    fn package(private: bool, reader: &Guid) -> Value {
        json!({
            "authorized": {},
            "categories": [],
            "versions": { "1.0.0:0": { "releaseNotes": "notes" } },
            "private": private,
            "readers": [reader],
        })
    }

    fn root(private: bool, reader: &Guid, admins: &[&Guid]) -> Value {
        json!({
            "migrations": [],
            "admins": admins,
            "index": {
                "name": null,
                "icon": null,
                "description": null,
                "os": { "versions": {} },
                "signers": {
                    reader.as_ref(): { "name": "reader", "contact": [], "keys": [key()] },
                },
                "package": {
                    "categories": {},
                    "packages": {
                        "public": package(false, reader),
                        "secret": package(private, reader),
                    },
                },
            },
        })
    }

    fn packages(root: &Value) -> Vec<String> {
        root["index"]["package"]["packages"]
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.to_string())
            .collect()
    }

    #[test]
    fn visible_root_hides_private_packages_from_everyone_else() {
        let reader = Guid::new();
        let private = root(true, &reader, &[]);
        let visible = visible_root(&private, None).unwrap();
        assert_eq!(packages(&visible), ["public"]);
        assert!(visible.get("admins").is_none());
        assert_eq!(
            packages(
                &visible_root(
                    &private,
                    Some(
                        &crate::sign::AnySigningKey::Ed25519(
                            ed25519_dalek::SigningKey::from_bytes(&[8; 32])
                        )
                        .verifying_key()
                    )
                )
                .unwrap()
            ),
            ["public"]
        );
        assert_eq!(
            packages(&visible_root(&private, Some(&key())).unwrap()),
            ["public", "secret"]
        );
        assert_eq!(
            packages(&visible_root(&root(false, &reader, &[]), None).unwrap()),
            ["public", "secret"]
        );
    }

    #[test]
    fn only_admins_and_private_readers_get_their_own_mirror() {
        let reader = Guid::new();
        let stranger =
            crate::sign::AnySigningKey::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[8; 32]))
                .verifying_key();
        let mirror_key_in =
            |root: Value, key: Option<AnyVerifyingKey>| mirror_key(&root.into(), key).unwrap();
        assert_eq!(mirror_key_in(root(true, &reader, &[]), None), None);
        assert_eq!(
            mirror_key_in(root(true, &reader, &[]), Some(stranger)),
            None
        );
        assert_eq!(mirror_key_in(root(false, &reader, &[]), Some(key())), None);
        assert_eq!(
            mirror_key_in(root(true, &reader, &[]), Some(key())),
            Some(key())
        );
        assert_eq!(
            mirror_key_in(root(false, &reader, &[&reader]), Some(key())),
            Some(key())
        );
    }

    #[tokio::test]
    async fn mirror_rechecks_grants_admin_status_and_enrolled_keys() {
        let reader = Guid::new();
        let permitted = root(true, &reader, &[]);
        let mut denied = permitted.clone();
        denied["index"]["package"]["packages"]["secret"]["readers"] = json!([]);
        let mut admin = denied.clone();
        admin["admins"] = json!([reader]);
        let mut revoked = permitted.clone();
        revoked["index"]["signers"][reader.as_ref()]["keys"] = json!([]);

        let main = TypedPatchDb::<RegistryDatabase>::load_unchecked(PatchDb::in_memory());
        main.put(&JsonPointer::<&str>::default(), &denied)
            .await
            .unwrap();
        let mirrors = IndexMirrors::default();
        let own = index_mirror(&mirrors, &main, Some(key())).await.unwrap();
        let (dump, mut sub) = own.db.dump_and_sub(JsonPointer::default()).await;
        let mut view = dump.value;
        assert_eq!(packages(&view), ["public"]);

        for (state, private_visible) in [
            (permitted.clone(), true),
            (denied.clone(), false),
            (admin, true),
            (denied, false),
            (permitted, true),
            (revoked, false),
        ] {
            main.put(&JsonPointer::<&str>::default(), &state)
                .await
                .unwrap();
            let revision = tokio::time::timeout(std::time::Duration::from_secs(5), sub.recv())
                .await
                .unwrap()
                .unwrap();
            patch_db::json_patch::patch(&mut view, &revision.patch).unwrap();
            assert_eq!(
                packages(&view).contains(&"secret".to_owned()),
                private_visible
            );
        }
    }

    #[tokio::test]
    async fn mirror_follows_visibility_changes() {
        let reader = Guid::new();
        let main = TypedPatchDb::<RegistryDatabase>::load_unchecked(PatchDb::in_memory());
        main.put(&JsonPointer::<&str>::default(), &root(true, &reader, &[]))
            .await
            .unwrap();
        let mirrors = IndexMirrors::default();
        let public = index_mirror(&mirrors, &main, None).await.unwrap();
        let shared = index_mirror(&mirrors, &main, None).await.unwrap();
        assert!(Arc::ptr_eq(&public, &shared));
        let own = index_mirror(&mirrors, &main, Some(key())).await.unwrap();
        let (dump, mut sub) = public.db.dump_and_sub("/index".parse().unwrap()).await;
        assert_eq!(packages(&json!({ "index": dump.value })), ["public"]);
        assert_eq!(
            packages(&own.db.dump(&JsonPointer::<&str>::default()).await.value),
            ["public", "secret"]
        );

        main.mutate(|db| {
            db.as_index_mut()
                .as_package_mut()
                .as_packages_mut()
                .as_idx_mut(&"secret".parse().unwrap())
                .unwrap()
                .as_access_mut()
                .as_private_mut()
                .ser(&false)
        })
        .await
        .result
        .unwrap();
        sub.recv().await.unwrap();
        assert_eq!(
            packages(&public.db.dump(&JsonPointer::<&str>::default()).await.value),
            ["public", "secret"]
        );

        drop((public, shared));
        let fresh = index_mirror(&mirrors, &main, None).await.unwrap();
        assert_eq!(mirrors.lock().await.len(), 2);
        drop(fresh);
    }
}
