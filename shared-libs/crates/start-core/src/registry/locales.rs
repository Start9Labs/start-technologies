use std::collections::{BTreeMap, BTreeSet};

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

/// `IndexLocales` loaded at startup and recomputed after each index revision.
pub struct IndexLocaleTracker {
    locales: watch::Receiver<IndexLocales>,
    _task: NonDetachingJoinHandle<()>,
}

impl IndexLocaleTracker {
    pub async fn spawn(db: TypedPatchDb<RegistryDatabase>) -> Result<Self, Error> {
        let mut sub = db.subscribe(super::db::INDEX.clone()).await;
        let initial = IndexLocales::of(db.peek().await.as_index())?;
        let (send, locales) = watch::channel(initial);
        let task = tokio::spawn(async move {
            while sub.recv().await.is_some() {
                while sub.try_recv().is_ok() {}
                match IndexLocales::of(db.peek().await.as_index()) {
                    Ok(next) => {
                        send.send_if_modified(|current| {
                            let changed = *current != next;
                            *current = next;
                            changed
                        });
                    }
                    Err(e) => {
                        tracing::error!("Error tracking registry index locales: {e}");
                        tracing::debug!("{e:?}");
                    }
                }
            }
        });
        Ok(Self {
            locales,
            _task: task.into(),
        })
    }

    pub fn current(&self) -> watch::Ref<'_, IndexLocales> {
        self.locales.borrow()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use imbl_value::json;
    use patch_db::PatchDb;
    use patch_db::json_ptr::JsonPointer;

    use super::*;

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
        let languages = |tracker: &IndexLocaleTracker| {
            let current = tracker.current();
            let mut all = current.unpackaged.clone();
            all.extend(current.packages.values().flatten().cloned());
            all.into_iter().map(|l| l.to_string()).collect::<Vec<_>>()
        };
        assert_eq!(languages(&tracker), ["en_US", "pl_PL"]);

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
        tokio::time::timeout(Duration::from_secs(5), async {
            while languages(&tracker) != ["en_US", "ja_JP", "pl_PL"] {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            tracker.current().packages[&"pkg".parse::<PackageId>().unwrap()]
                .iter()
                .map(|l| &**l)
                .collect::<Vec<_>>(),
            ["en_US", "ja_JP"]
        );

        db.put(
            &"/index/package/packages".parse::<JsonPointer>().unwrap(),
            &json!({}),
        )
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while languages(&tracker) != ["en_US", "pl_PL"] {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
}
