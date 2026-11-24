use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Weak};

use patch_db::json_ptr::JsonPointer;
use patch_db::{ModelExt, PatchDb};
use tokio::sync::watch;

use crate::prelude::*;
use crate::registry::device_info::WireCompatibility;
use crate::registry::locales::IndexLocales;
use crate::registry::package::icon::icons_dir;
use crate::registry::package::index::Reader;
use crate::registry::{FullIndex, RegistryDatabase};
use crate::sign::AnyVerifyingKey;
use crate::util::future::NonDetachingJoinHandle;

pub type IndexMirrors =
    tokio::sync::Mutex<BTreeMap<(Option<InternedString>, IndexView), Weak<IndexMirror>>>;

/// Client-specific presentation of the index; equal views share one mirror.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct IndexView {
    pub wire: WireCompatibility,
    /// Unset keeps every translation.
    language: Option<ViewLanguage>,
    locales: BTreeSet<InternedString>,
}

/// Translation each `LocaleString` in a view resolves to.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ViewLanguage {
    Locale(InternedString),
    /// Each owner's declared default, for a language matching none in the index.
    Default,
}

impl IndexView {
    /// A language matching none of the reader's visible locales shares the default view.
    pub fn localized(
        db: &Model<RegistryDatabase>,
        locales: &IndexLocales,
        key: Option<&AnyVerifyingKey>,
        wire: WireCompatibility,
        language: &str,
    ) -> Result<Self, Error> {
        let reader = Reader::resolve(db, key)?;
        let visible = locales.visible_to(db, reader.as_ref())?;
        let language = match crate::best_locale(language, visible.iter().copied()) {
            Some(locale) => ViewLanguage::Locale(InternedString::intern(locale)),
            None => ViewLanguage::Default,
        };
        Ok(Self {
            wire,
            language: Some(language),
            locales: visible.into_iter().map(InternedString::intern).collect(),
        })
    }

    fn has_current_locales(
        &self,
        root: &Value,
        key: Option<&AnyVerifyingKey>,
    ) -> Result<bool, Error> {
        if self.language.is_none() {
            return Ok(true);
        }
        let db: Model<RegistryDatabase> = root.clone().into();
        let reader = Reader::resolve(&db, key)?;
        let locales = IndexLocales::of(db.as_index())?;
        let visible = locales
            .visible_to(&db, reader.as_ref())?
            .into_iter()
            .map(InternedString::intern)
            .collect::<BTreeSet<_>>();
        Ok(visible == self.locales)
    }
}

impl From<WireCompatibility> for IndexView {
    fn from(wire: WireCompatibility) -> Self {
        Self {
            wire,
            language: None,
            locales: BTreeSet::new(),
        }
    }
}

/// Localized mirrors retire when their visible locale inventory changes.
pub struct IndexMirror {
    pub db: PatchDb,
    completion: watch::Receiver<Option<Arc<Error>>>,
    _sync: NonDetachingJoinHandle<()>,
}

impl IndexMirror {
    async fn spawn(
        main: &TypedPatchDb<RegistryDatabase>,
        key: Option<AnyVerifyingKey>,
        view: IndexView,
        datadir: &Path,
    ) -> Result<Arc<Self>, Error> {
        let (dump, mut sub) = main.dump_and_sub(JsonPointer::default()).await;
        let mut root = dump.value;
        ensure_code!(
            view.has_current_locales(&root, key.as_ref())?,
            ErrorKind::Cancelled,
            "{}",
            rust_i18n::t!("error.cancelled")
        );
        let icons = icons_dir(datadir);
        let visible = projected_root(&root, key.as_ref(), &view, &icons).await?;
        let db = PatchDb::in_memory();
        db.put(&JsonPointer::<&str>::default(), &visible).await?;
        let mirror = db.clone();
        let (completed, completion) = watch::channel(None);
        let sync = tokio::spawn(async move {
            while let Some(rev) = sub.recv().await {
                let res = async {
                    patch_db::json_patch::patch(&mut root, &rev.patch)
                        .map_err(|e| Error::new(eyre!("{e}"), ErrorKind::Database))?;
                    if !view.has_current_locales(&root, key.as_ref())? {
                        return Ok(false);
                    }
                    let next = projected_root(&root, key.as_ref(), &view, &icons).await?;
                    mirror.put(&JsonPointer::<&str>::default(), &next).await?;
                    Ok::<_, Error>(true)
                }
                .await;
                match res {
                    Ok(true) => (),
                    Ok(false) => break,
                    Err(e) => {
                        tracing::error!("Error mirroring registry index: {e}");
                        tracing::debug!("{e:?}");
                        completed.send_replace(Some(Arc::new(e)));
                        break;
                    }
                }
            }
        });
        Ok(Arc::new(Self {
            db,
            completion,
            _sync: sync.into(),
        }))
    }

    pub async fn closed(&self) -> Result<(), Error> {
        let mut completion = self.completion.clone();
        let _ = completion.wait_for(|error| error.is_some()).await;
        match &*completion.borrow() {
            Some(error) => Err(error.clone_output()),
            None => Ok(()),
        }
    }

    fn is_running(&self) -> bool {
        self.completion.borrow().is_none() && self.completion.has_changed().is_ok()
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
    view: IndexView,
    datadir: &Path,
) -> Result<Arc<IndexMirror>, Error> {
    let identity = (key.as_ref().map(|key| key.interned_der64()), view.clone());
    let mut mirrors = mirrors.lock().await;
    if let Some(mirror) = mirrors
        .get(&identity)
        .and_then(Weak::upgrade)
        .filter(|m| m.is_running())
    {
        return Ok(mirror);
    }
    let mirror = IndexMirror::spawn(main, key, view, datadir).await?;
    mirrors.retain(|_, m| m.strong_count() > 0);
    mirrors.insert(identity, Arc::downgrade(&mirror));
    Ok(mirror)
}

async fn projected_root(
    root: &OrdValue,
    key: Option<&AnyVerifyingKey>,
    view: &IndexView,
    icons: &Path,
) -> Result<Value, Error> {
    let mut index = visible_index(root.clone().into(), key)?;
    match &view.language {
        Some(ViewLanguage::Locale(locale)) => index.localize_for(Some(locale))?,
        Some(ViewLanguage::Default) => index.localize_for(None)?,
        None => (),
    }
    let mut visible = imbl_value::json!({ "index": index.into_value() });
    view.wire
        .project(icons, "index", &Value::Null, &mut visible["index"])
        .await?;
    Ok(visible)
}

fn visible_index(
    db: Model<RegistryDatabase>,
    key: Option<&AnyVerifyingKey>,
) -> Result<Model<FullIndex>, Error> {
    let reader = Reader::resolve(&db, key)?;
    let mut index = db.into_index();
    index.retain_visible(reader.as_ref())?;
    Ok(index)
}

#[cfg(test)]
mod tests {
    mod compatibility;

    use super::*;
    use crate::rpc_continuations::Guid;

    fn key() -> AnyVerifyingKey {
        crate::sign::AnySigningKey::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[7; 32]))
            .verifying_key()
    }

    fn package(private: bool, reader: &Guid) -> OrdValue {
        let mut asset = imbl_value::ord_json!({ "readers": [reader] });
        if private {
            asset["private"] = imbl_value::ord_json!({
                "key": "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE",
                "iv": "AgICAgICAgICAgICAgICAg",
            });
        }
        imbl_value::ord_json!({
            "authorized": {},
            "categories": [],
            "versions": { "1.0.0:0": { "releaseNotes": "notes", "s9pks": [[{}, asset]] } },
        })
    }

    /// The reader key is both a registered signer and a reader-directory entry.
    fn root(private: bool, reader: &Guid, admins: &[&Guid]) -> OrdValue {
        imbl_value::ord_json!({
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
                "readers": {
                    reader.as_ref(): { "label": "reader", "key": key() },
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

    fn secret_s9pk(root: &mut OrdValue) -> &mut OrdValue {
        &mut root["index"]["package"]["packages"]["secret"]["versions"]["1.0.0:0"]["s9pks"][0][1]
    }

    fn packages<K: ObjectKind>(root: &GenericValue<K>) -> Vec<String> {
        imbl_value::ObjectMap::entries(root["index"]["package"]["packages"].as_object().unwrap())
            .map(|(k, _)| k.to_string())
            .collect()
    }

    async fn visible_root(root: &OrdValue, key: Option<&AnyVerifyingKey>) -> Value {
        projected_root(root, key, &IndexView::default(), Path::new(""))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn visible_root_hides_private_packages_from_everyone_else() {
        let reader = Guid::new();
        let private = root(true, &reader, &[]);
        let visible = visible_root(&private, None).await;
        assert_eq!(packages(&visible), ["public"]);
        assert!(visible.get("admins").is_none());
        let stranger =
            crate::sign::AnySigningKey::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[8; 32]))
                .verifying_key();
        assert_eq!(
            packages(&visible_root(&private, Some(&stranger)).await),
            ["public"]
        );
        assert_eq!(
            packages(&visible_root(&private, Some(&key())).await),
            ["public", "secret"]
        );
        assert_eq!(
            packages(&visible_root(&root(false, &reader, &[]), None).await),
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
            |root: OrdValue, key: Option<AnyVerifyingKey>| mirror_key(&root.into(), key).unwrap();
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
        secret_s9pk(&mut denied)["readers"] = imbl_value::ord_json!([]);
        let mut admin = denied.clone();
        admin["admins"] = imbl_value::ord_json!([reader]);
        let mut revoked = permitted.clone();
        revoked["index"]["readers"] = imbl_value::ord_json!({});

        let main = TypedPatchDb::<RegistryDatabase>::load_unchecked(PatchDb::in_memory());
        main.put(&JsonPointer::<&str>::default(), &denied)
            .await
            .unwrap();
        let mirrors = IndexMirrors::default();
        let own = index_mirror(
            &mirrors,
            &main,
            Some(key()),
            IndexView::default(),
            Path::new(""),
        )
        .await
        .unwrap();
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
    async fn localized_mirrors_retire_when_language_matching_changes() {
        for (language, before, after, expected) in [
            (
                "de_DE",
                json!({ "en_US": "English" }),
                json!({ "en_US": "English", "de_DE": "Deutsch" }),
                "Deutsch",
            ),
            (
                "en_GB",
                json!({ "en_US": "US English" }),
                json!({ "en_US": "US English", "en_GB": "GB English" }),
                "GB English",
            ),
            (
                "de_DE",
                json!({ "en_US": "English", "de_DE": "Deutsch" }),
                json!({ "en_US": "English" }),
                "English",
            ),
        ] {
            let main = TypedPatchDb::<RegistryDatabase>::load_unchecked(PatchDb::in_memory());
            main.put(
                &JsonPointer::<&str>::default(),
                &json!({
                    "admins": [],
                    "index": {
                        "description": before,
                        "defaultLocale": "en_US",
                        "package": { "categories": {}, "packages": {} },
                        "os": { "versions": {} },
                    },
                }),
            )
            .await
            .unwrap();
            let snapshot = main.peek().await;
            let view = IndexView::localized(
                &snapshot,
                &IndexLocales::of(snapshot.as_index()).unwrap(),
                None,
                WireCompatibility::default(),
                language,
            )
            .unwrap();
            let mirrors = IndexMirrors::default();
            let localized = index_mirror(&mirrors, &main, None, view.clone(), Path::new(""))
                .await
                .unwrap();
            let canonical =
                index_mirror(&mirrors, &main, None, IndexView::default(), Path::new(""))
                    .await
                    .unwrap();
            let (_, mut canonical_sub) = canonical.db.dump_and_sub("/index".parse().unwrap()).await;
            main.put(
                &"/index/description".parse::<JsonPointer>().unwrap(),
                &after,
            )
            .await
            .unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(5), localized.closed())
                .await
                .unwrap()
                .unwrap();
            assert!(!localized.is_running());
            tokio::time::timeout(std::time::Duration::from_secs(5), canonical_sub.recv())
                .await
                .unwrap()
                .unwrap();
            assert!(canonical.is_running());

            let stale = IndexMirror::spawn(&main, None, view, Path::new("")).await;
            assert!(matches!(stale, Err(error) if error.kind == ErrorKind::Cancelled));
            let snapshot = main.peek().await;
            let view = IndexView::localized(
                &snapshot,
                &IndexLocales::of(snapshot.as_index()).unwrap(),
                None,
                WireCompatibility::default(),
                language,
            )
            .unwrap();
            let fresh = index_mirror(&mirrors, &main, None, view, Path::new(""))
                .await
                .unwrap();
            assert!(!Arc::ptr_eq(&localized, &fresh));
            assert_eq!(
                fresh
                    .db
                    .dump(&"/index/description".parse::<JsonPointer>().unwrap())
                    .await
                    .value,
                json!(expected)
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
        let public = index_mirror(&mirrors, &main, None, IndexView::default(), Path::new(""))
            .await
            .unwrap();
        let shared = index_mirror(&mirrors, &main, None, IndexView::default(), Path::new(""))
            .await
            .unwrap();
        assert!(Arc::ptr_eq(&public, &shared));
        let own = index_mirror(
            &mirrors,
            &main,
            Some(key()),
            IndexView::default(),
            Path::new(""),
        )
        .await
        .unwrap();
        let (dump, mut sub) = public.db.dump_and_sub("/index".parse().unwrap()).await;
        assert_eq!(
            packages(&imbl_value::ord_json!({ "index": dump.value })),
            ["public"]
        );
        assert_eq!(
            packages(&own.db.dump(&JsonPointer::<&str>::default()).await.value),
            ["public", "secret"]
        );

        let mut published = root(true, &reader, &[]);
        secret_s9pk(&mut published)
            .as_object_mut()
            .unwrap()
            .remove("private");
        main.put(&JsonPointer::<&str>::default(), &published)
            .await
            .unwrap();
        sub.recv().await.unwrap();
        assert_eq!(
            packages(&public.db.dump(&JsonPointer::<&str>::default()).await.value),
            ["public", "secret"]
        );

        drop((public, shared));
        let fresh = index_mirror(&mirrors, &main, None, IndexView::default(), Path::new(""))
            .await
            .unwrap();
        assert_eq!(mirrors.lock().await.len(), 2);
        drop(fresh);
    }
}
