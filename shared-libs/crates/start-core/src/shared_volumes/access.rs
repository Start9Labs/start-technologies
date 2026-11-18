use std::collections::BTreeMap;
use std::sync::{Arc, Weak};

use tokio::sync::{Mutex, RwLock};

use super::mounts::SharedVolumeMounts;
use super::{SharedVolumeGrant, SharedVolumeId};
use crate::PackageId;
use crate::lxc::ContainerId;
use crate::prelude::*;
use crate::service::{Service, ServiceRuntime};
use crate::util::sync::SyncMutex;

struct Exposure {
    service: Weak<Service>,
    runtime: ServiceRuntime,
    mounts: Arc<Mutex<SharedVolumeMounts>>,
    volumes: BTreeMap<SharedVolumeId, SharedVolumeGrant>,
}

impl Exposure {
    fn admit(&mut self, volume: &SharedVolumeId, readonly: bool) {
        self.volumes
            .entry(volume.clone())
            .and_modify(|access| access.readonly &= readonly)
            .or_insert(SharedVolumeGrant { readonly });
    }
}

pub(crate) struct ServiceExposures {
    package_id: PackageId,
    incarnations: BTreeMap<ContainerId, Exposure>,
}

impl ServiceExposures {
    fn new(package_id: PackageId) -> Self {
        Self {
            package_id,
            incarnations: BTreeMap::new(),
        }
    }

    fn prune(&mut self) {
        self.incarnations.retain(|_, exposure| {
            exposure.service.strong_count() > 0
                || !exposure.runtime.is_retired()
                || !exposure
                    .mounts
                    .try_lock()
                    .is_ok_and(|mounts| mounts.is_empty())
        });
    }

    pub(crate) fn record(
        &mut self,
        volume: &SharedVolumeId,
        readonly: bool,
        service: &Arc<Service>,
    ) -> Result<(), Error> {
        self.prune();
        let container = service.lxc_container()?;
        if !self.incarnations.contains_key(container.guid.as_ref()) {
            self.incarnations.insert(
                (*container.guid).clone(),
                Exposure {
                    service: Arc::downgrade(service),
                    runtime: service.runtime_control()?,
                    mounts: service.shared_volume_mounts().clone(),
                    volumes: BTreeMap::new(),
                },
            );
        }
        self.incarnations
            .get_mut(container.guid.as_ref())
            .unwrap()
            .admit(volume, readonly);
        Ok(())
    }

    pub(crate) async fn capture(
        &mut self,
        volume: &SharedVolumeId,
        grant: Option<&SharedVolumeGrant>,
    ) -> Vec<Revocation> {
        self.prune();
        let mut revocations = Vec::new();
        for (container_id, exposure) in &self.incarnations {
            let Some(access) = exposure.volumes.get(volume) else {
                continue;
            };
            if grant.is_some_and(|grant| grant.allows(access.readonly))
                && !exposure.runtime.payload_is_stopped()
            {
                continue;
            }
            exposure.mounts.lock().await.block();
            revocations.push(Revocation {
                package_id: self.package_id.clone(),
                container_id: container_id.clone(),
                runtime: exposure.runtime.clone(),
                mounts: exposure.mounts.clone(),
            });
        }
        revocations
    }
}

pub(crate) struct Revocation {
    pub(crate) package_id: PackageId,
    pub(crate) container_id: ContainerId,
    pub(crate) runtime: ServiceRuntime,
    mounts: Arc<Mutex<SharedVolumeMounts>>,
}

impl Revocation {
    pub(crate) async fn complete(&self) -> Result<(), Error> {
        self.runtime.revoke().await?;
        self.mounts.lock().await.clear().await
    }
}

/// Published restrictions exclude every incompatible admitted incarnation.
/// Admitted access survives SDK unmounts and Service destruction until full retirement.
#[derive(Default)]
pub(crate) struct SharedVolumeAccess {
    pub(crate) management: RwLock<()>,
    services: SyncMutex<BTreeMap<PackageId, Arc<Mutex<ServiceExposures>>>>,
}

impl SharedVolumeAccess {
    pub(crate) fn service(&self, package: &PackageId) -> Arc<Mutex<ServiceExposures>> {
        self.services.mutate(|services| {
            services
                .entry(package.clone())
                .or_insert_with(|| Arc::new(Mutex::new(ServiceExposures::new(package.clone()))))
                .clone()
        })
    }

    pub(crate) fn services(&self) -> Vec<(PackageId, Arc<Mutex<ServiceExposures>>)> {
        self.services.peek(|services| {
            services
                .iter()
                .map(|(id, exposures)| (id.clone(), exposures.clone()))
                .collect()
        })
    }
}

#[cfg(test)]
mod tests;
