use std::collections::BTreeMap;
use std::sync::{Arc, Weak};

use tokio::sync::{Mutex, RwLock};

use super::mounts::SharedVolumeMounts;
use super::{SharedVolumeGrant, SharedVolumeId};
use crate::PackageId;
use crate::prelude::*;
use crate::util::sync::SyncMutex;

/// Grant changes deny future preparation and retire incompatible staging sources.
/// Revocation is best-effort: existing bindings, aliases and open handles can remain usable.
#[derive(Default)]
pub(crate) struct SharedVolumeAccess {
    pub(crate) management: RwLock<()>,
    stages: SyncMutex<BTreeMap<PackageId, Vec<Weak<Mutex<SharedVolumeMounts>>>>>,
}

impl SharedVolumeAccess {
    pub(crate) fn register(&self, package: &PackageId, mounts: &Arc<Mutex<SharedVolumeMounts>>) {
        self.stages.mutate(|stages| {
            let stages = stages.entry(package.clone()).or_default();
            stages.retain(|stage| stage.strong_count() > 0);
            stages.push(Arc::downgrade(mounts));
        });
    }

    fn live_stages(&self) -> BTreeMap<PackageId, Vec<Arc<Mutex<SharedVolumeMounts>>>> {
        self.stages.mutate(|stages| {
            let mut live = BTreeMap::new();
            stages.retain(|package, stages| {
                let mounts: Vec<_> = stages.iter().filter_map(Weak::upgrade).collect();
                stages.retain(|stage| stage.strong_count() > 0);
                if mounts.is_empty() {
                    false
                } else {
                    live.insert(package.clone(), mounts);
                    true
                }
            });
            live
        })
    }

    pub(crate) async fn reconcile_grant(
        &self,
        package: &PackageId,
        volume: &SharedVolumeId,
        grant: Option<&SharedVolumeGrant>,
    ) -> Result<(), Error> {
        let mut errors = ErrorCollection::new();
        for mounts in self.live_stages().remove(package).unwrap_or_default() {
            errors.handle(mounts.lock().await.reconcile_grant(volume, grant).await);
        }
        errors.into_result()
    }

    pub(crate) async fn remove_volume(&self, volume: &SharedVolumeId) -> Result<(), Error> {
        let mut errors = ErrorCollection::new();
        for mounts in self.live_stages().into_values().flatten() {
            errors.handle(mounts.lock().await.reconcile_grant(volume, None).await);
        }
        errors.into_result()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn registry_tracks_staging_lifetimes_without_owning_them() {
        let registry = SharedVolumeAccess::default();
        let package = "consumer".parse().unwrap();
        let first = Arc::new(Mutex::new(SharedVolumeMounts::default()));
        let second = Arc::new(Mutex::new(SharedVolumeMounts::default()));
        registry.register(&package, &first);
        registry.register(&package, &second);
        assert_eq!(Arc::strong_count(&first), 1);
        assert_eq!(registry.live_stages()[&package].len(), 2);
        drop(first);
        assert_eq!(registry.live_stages()[&package].len(), 1);
        registry
            .reconcile_grant(&package, &SharedVolumeId::new(), None)
            .await
            .unwrap();
        assert!(second.lock().await.check_allowed().is_ok());
        drop(second);
        assert!(registry.live_stages().is_empty());
    }
}
