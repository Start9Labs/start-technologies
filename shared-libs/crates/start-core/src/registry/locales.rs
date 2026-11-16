use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use patch_db::Dump;
use patch_db::json_ptr::ROOT;
use tokio::sync::watch;

use crate::PackageId;
use crate::prelude::*;
use crate::registry::package::index::Reader;
use crate::registry::{FullIndex, RegistryDatabase};
use crate::util::future::NonDetachingJoinHandle;

/// Locales the index carries translations for, grouped by owning package.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IndexLocales {
    unpackaged: BTreeSet<InternedString>,
    packages: BTreeMap<PackageId, BTreeSet<InternedString>>,
}

impl IndexLocales {
    pub fn of(index: &Model<FullIndex>) -> Result<Self, Error> {
        let mut index = index.clone();
        let mut locales = Self::default();
        index.for_each_unpackaged_locale_string(&mut |s| {
            locales.unpackaged.extend(s.languages().cloned())
        })?;
        for (id, package) in index.as_package_mut().as_packages_mut().as_entries_mut()? {
            let mut languages = BTreeSet::new();
            package.for_each_locale_string(&mut |s| languages.extend(s.languages().cloned()))?;
            locales.packages.insert(id, languages);
        }
        Ok(locales)
    }

    /// Locales outside packages, plus those of packages visible to the reader.
    pub fn visible_to(
        &self,
        db: &Model<RegistryDatabase>,
        reader: Option<&Reader>,
    ) -> Result<BTreeSet<&str>, Error> {
        let packages = db.as_index().as_package();
        let mut visible: BTreeSet<&str> = self.unpackaged.iter().map(|l| &**l).collect();
        for (id, languages) in &self.packages {
            if packages.visible_package(id, reader)?.is_some() {
                visible.extend(languages.iter().map(|l| &**l));
            }
        }
        Ok(visible)
    }
}

/// Database and locale metadata from one revision.
pub struct IndexLocaleSnapshot {
    pub source: Dump,
    pub locales: IndexLocales,
}

/// Subscription snapshots pair locale metadata with the same source revision.
pub struct IndexLocaleTracker {
    locales: watch::Receiver<Result<Arc<IndexLocaleSnapshot>, Arc<Error>>>,
    _task: NonDetachingJoinHandle<()>,
}

impl IndexLocaleTracker {
    pub async fn spawn(db: TypedPatchDb<RegistryDatabase>) -> Result<Self, Error> {
        let (source, mut sub) = db.dump_and_sub(ROOT.to_owned()).await;
        let database: Model<RegistryDatabase> = source.value.clone().into();
        let mut snapshot = Arc::new(IndexLocaleSnapshot {
            locales: IndexLocales::of(database.as_index())?,
            source,
        });
        let (send, locales) = watch::channel(Ok(snapshot.clone()));
        let task = tokio::spawn(async move {
            while sub.recv().await.is_some() {
                while sub.try_recv().is_ok() {}
                let source = db.dump(&ROOT).await;
                let next = if source.value["index"] == snapshot.source.value["index"] {
                    Ok(snapshot.locales.clone())
                } else {
                    let database: Model<RegistryDatabase> = source.value.clone().into();
                    IndexLocales::of(database.as_index())
                };
                match next {
                    Ok(locales) => {
                        snapshot = Arc::new(IndexLocaleSnapshot { source, locales });
                        send.send_replace(Ok(snapshot.clone()));
                    }
                    Err(e) => {
                        tracing::error!("Error tracking registry index locales: {e}");
                        tracing::debug!("{e:?}");
                        send.send_replace(Err(Arc::new(e)));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            locales,
            _task: task.into(),
        })
    }

    /// Returns matching database and locale snapshots at or after the requested revision.
    pub async fn snapshot(&self, sequence: u64) -> Result<Arc<IndexLocaleSnapshot>, Error> {
        let mut locales = self.locales.clone();
        let snapshot = locales
            .wait_for(|snapshot| match snapshot {
                Ok(snapshot) => snapshot.source.id >= sequence,
                Err(_) => true,
            })
            .await
            .with_kind(ErrorKind::Database)?;
        match &*snapshot {
            Ok(snapshot) => Ok(snapshot.clone()),
            Err(error) => Err(error.clone_output()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use imbl_value::json;
    use patch_db::PatchDb;
    use patch_db::json_ptr::JsonPointer;

    use super::*;
    use crate::registry::device_info::WireCompatibility;
    use crate::registry::mirror::{IndexMirrors, IndexView, index_mirror};

    fn version(notes: Value) -> Value {
        json!({
            "title": "Package",
            "description": { "short": "Short", "long": "Long" },
            "releaseNotes": notes,
            "dependencyMetadata": {},
        })
    }

    #[tokio::test]
    async fn tracker_follows_index_revisions() {
        let db = TypedPatchDb::<RegistryDatabase>::load_unchecked(PatchDb::in_memory());
        db.put(
            &JsonPointer::<&str>::default(),
            &json!({
                "admins": [],
                "index": {
                    "description": { "en_US": "Registry", "pl_PL": "Rejestr" },
                    "package": { "categories": {}, "packages": {} },
                    "os": { "versions": {} },
                },
            }),
        )
        .await
        .unwrap();
        let tracker = IndexLocaleTracker::spawn(db.clone()).await.unwrap();
        let languages = |snapshot: &IndexLocaleSnapshot| {
            let mut all = snapshot.locales.unpackaged.clone();
            all.extend(snapshot.locales.packages.values().flatten().cloned());
            all.into_iter().map(|l| l.to_string()).collect::<Vec<_>>()
        };
        let initial = tracker.snapshot(db.sequence().await).await.unwrap();
        assert_eq!(languages(&initial), ["en_US", "pl_PL"]);

        db.put(
            &"/index/package/packages/pkg"
                .parse::<JsonPointer>()
                .unwrap(),
            &json!({
                "authorized": {},
                "categories": [],
                "versions": {
                    "1.0.0:0": version(json!({ "en_US": "Notes", "ja_JP": "ノート" })),
                },
            }),
        )
        .await
        .unwrap();
        let added = tokio::time::timeout(
            Duration::from_secs(5),
            tracker.snapshot(db.sequence().await),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(languages(&added), ["en_US", "ja_JP", "pl_PL"]);
        assert_eq!(
            added.locales.packages[&"pkg".parse::<PackageId>().unwrap()]
                .iter()
                .map(|l| &**l)
                .collect::<Vec<_>>(),
            ["en_US", "ja_JP"]
        );
        let database: Model<RegistryDatabase> = added.source.value.clone().into();
        let view = IndexView::resolve(
            &database,
            &added.locales,
            None,
            WireCompatibility::default(),
            Some("ja_JP"),
        )
        .unwrap();
        let mirror = index_mirror(&IndexMirrors::default(), &db, None, view, Path::new(""))
            .await
            .unwrap();
        assert_eq!(
            mirror
                .db
                .dump(
                    &"/index/package/packages/pkg/versions/1.0.0:0/releaseNotes"
                        .parse::<JsonPointer>()
                        .unwrap(),
                )
                .await
                .value,
            json!("ノート")
        );

        let admins = json!([crate::rpc_continuations::Guid::new()]);
        db.put(&"/admins".parse::<JsonPointer>().unwrap(), &admins)
            .await
            .unwrap();
        let permissions = tokio::time::timeout(
            Duration::from_secs(5),
            tracker.snapshot(db.sequence().await),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(permissions.source.value["admins"], admins);
        assert_eq!(languages(&permissions), ["en_US", "ja_JP", "pl_PL"]);

        db.put(
            &"/index/package/packages".parse::<JsonPointer>().unwrap(),
            &json!({}),
        )
        .await
        .unwrap();
        let removed = tokio::time::timeout(
            Duration::from_secs(5),
            tracker.snapshot(db.sequence().await),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(languages(&removed), ["en_US", "pl_PL"]);
        assert!(
            removed.source.value["index"]["package"]["packages"]
                .as_object()
                .unwrap()
                .is_empty()
        );
    }
}
