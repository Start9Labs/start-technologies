use std::collections::BTreeMap;

use crate::PackageId;
use crate::prelude::*;
use crate::service::effects::callbacks::CallbackHandler;
use crate::service::effects::prelude::*;
use crate::service::rpc::CallbackId;
use crate::shared_volumes::{SharedVolume, SharedVolumeAvailability, SharedVolumeId};

#[derive(Debug, Clone, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct GrantedSharedVolume {
    pub id: SharedVolumeId,
    pub name: String,
    pub readonly: bool,
    pub availability: SharedVolumeAvailability,
}

rpc_toolkit::reflect_ts!(GrantedSharedVolume);
rpc_toolkit::ts_export!(GrantedSharedVolume, namespaces = [""]);

#[derive(Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct GetSharedVolumesParams {
    pub callback: Option<CallbackId>,
}

rpc_toolkit::reflect_ts!(GetSharedVolumesParams);

fn granted_volumes(
    capable: bool,
    package: &PackageId,
    catalog: BTreeMap<SharedVolumeId, SharedVolume>,
) -> Result<Vec<GrantedSharedVolume>, Error> {
    if !capable {
        return Err(crate::shared_volumes::access_denied());
    }
    Ok(catalog
        .into_iter()
        .filter_map(|(id, volume)| {
            let grant = volume.grants.get(package)?;
            Some(GrantedSharedVolume {
                id,
                name: volume.name,
                readonly: grant.readonly,
                availability: volume.availability,
            })
        })
        .collect())
}

pub async fn get_shared_volumes(
    context: EffectContext,
    GetSharedVolumesParams { callback }: GetSharedVolumesParams,
) -> Result<Vec<GrantedSharedVolume>, Error> {
    let service = context.deref()?;
    let mut watch = service
        .seed
        .ctx
        .db
        .watch("/public/sharedVolumes".parse().unwrap())
        .await
        .typed::<BTreeMap<SharedVolumeId, SharedVolume>>();
    let result = granted_volumes(
        service
            .seed
            .persistent_container
            .s9pk
            .as_manifest()
            .shared_volumes,
        &service.seed.id,
        from_value(watch.as_mut().peek_and_mark_seen()?)?,
    )?;
    if let Some(callback) = callback {
        let callback = callback.register(&service.seed.persistent_container);
        service.seed.ctx.callbacks.add_get_shared_volumes(
            service.seed.id.clone(),
            watch,
            CallbackHandler::new(&service, callback),
        );
    }
    Ok(result)
}

#[derive(Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct MountSharedVolumeParams {
    pub id: SharedVolumeId,
    pub subcontainer: crate::rpc_continuations::Guid,
    pub mountpoint: std::path::PathBuf,
    pub subpath: Option<std::path::PathBuf>,
    pub readonly: bool,
}

rpc_toolkit::reflect_ts!(MountSharedVolumeParams);
rpc_toolkit::ts_export!(MountSharedVolumeParams, namespaces = [""]);

#[derive(Debug, Clone, Serialize, Deserialize, VisitVariants)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SharedVolumeMountResult {
    Mounted,
    Unavailable {
        reason: SharedVolumeUnavailableReason,
    },
}

rpc_toolkit::reflect_ts!(SharedVolumeMountResult);
rpc_toolkit::ts_export!(SharedVolumeMountResult, namespaces = [""]);

#[derive(Debug, Clone, Serialize, Deserialize, VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum SharedVolumeUnavailableReason {
    SourceUnavailable,
    Timeout,
}

rpc_toolkit::reflect_ts!(SharedVolumeUnavailableReason);

async fn authorize_mount(
    service: &crate::service::Service,
    id: &SharedVolumeId,
    readonly: bool,
) -> Result<(), Error> {
    service
        .seed
        .ctx
        .db
        .peek()
        .await
        .as_public()
        .as_shared_volumes()
        .as_idx(id)
        .ok_or_else(crate::shared_volumes::access_denied)?
        .de()?
        .authorize(
            service
                .seed
                .persistent_container
                .s9pk
                .as_manifest()
                .shared_volumes,
            &service.seed.id,
            readonly,
        )
}

pub async fn mount_shared_volume(
    context: EffectContext,
    MountSharedVolumeParams {
        id,
        subcontainer,
        mountpoint,
        subpath,
        readonly,
    }: MountSharedVolumeParams,
) -> Result<SharedVolumeMountResult, Error> {
    use crate::disk::mount::filesystem::syscall::MountNamespace;
    use crate::disk::mount::filesystem::{ReadOnly, ReadWrite};
    use crate::disk::mount::guard::GenericMountGuard;
    use crate::shared_volumes::mounts::{PreparedSource, PreparedTarget, confined_relative};
    let service = context.deref()?;
    let container = &service.seed.persistent_container;
    container.shared_volumes.lock().await.check_allowed()?;
    authorize_mount(&service, &id, readonly).await?;
    confined_relative(&mountpoint)?;
    if let Some(subpath) = &subpath {
        confined_relative(subpath)?;
    }
    container
        .subcontainers
        .lock()
        .await
        .get(&subcontainer)
        .or_not_found("subcontainer")?;
    let source = match crate::shared_volumes::source::open_source(
        &service.seed.ctx,
        &id,
        if readonly { ReadOnly } else { ReadWrite },
    )
    .await
    {
        Ok(source) => Ok(PreparedSource::open(source, subpath).await),
        Err(error) => Err(error),
    };
    let lxc = container
        .lxc_container
        .get()
        .or_not_found("lxc container")?;
    let pid = lxc.pid().await?;
    let namespace = std::sync::Arc::new(MountNamespace::for_process(pid)?);
    let root = {
        let subcontainers = container.subcontainers.lock().await;
        std::path::PathBuf::from(format!("/proc/{pid}/root")).join(
            subcontainers
                .get(&subcontainer)
                .or_not_found("subcontainer")?
                .overlay
                .path()
                .strip_prefix(lxc.rootfs_dir())
                .with_kind(ErrorKind::Incoherent)?,
        )
    };
    let target = PreparedTarget::open(root, mountpoint, namespace).await?;
    let mut mounts = container.shared_volumes.lock().await;
    mounts.check_allowed()?;
    authorize_mount(&service, &id, readonly).await?;
    let subcontainers = container.subcontainers.lock().await;
    subcontainers
        .get(&subcontainer)
        .or_not_found("subcontainer")?;
    let location = mounts.prepare(&subcontainer, target).await?;
    let source = match source {
        Ok(prepared) => prepared?,
        Err(error) => {
            if matches!(error.kind, ErrorKind::InvalidRequest) {
                return Err(error);
            }
            return Ok(SharedVolumeMountResult::Unavailable {
                reason: if matches!(error.kind, ErrorKind::Timeout) {
                    SharedVolumeUnavailableReason::Timeout
                } else {
                    SharedVolumeUnavailableReason::SourceUnavailable
                },
            });
        }
    };
    mounts.attach(&subcontainer, location, id, source, readonly)?;
    Ok(SharedVolumeMountResult::Mounted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_volumes::{SharedVolumeGrant, SharedVolumeSource};

    #[test]
    fn listing_preserves_offline_grants_without_exposing_other_volumes() {
        let package: PackageId = "consumer".parse().unwrap();
        let id = SharedVolumeId::new();
        let mut catalog = BTreeMap::new();
        catalog.insert(
            id.clone(),
            SharedVolume {
                name: "Photos".into(),
                source: SharedVolumeSource::Local,
                grants: [(package.clone(), SharedVolumeGrant { readonly: true })].into(),
                availability: SharedVolumeAvailability::Unavailable,
            },
        );
        catalog.insert(
            SharedVolumeId::new(),
            SharedVolume {
                name: "Private".into(),
                source: SharedVolumeSource::Local,
                grants: BTreeMap::new(),
                availability: SharedVolumeAvailability::Available,
            },
        );
        assert!(granted_volumes(false, &package, catalog.clone()).is_err());
        assert!(
            granted_volumes(true, &"other".parse().unwrap(), catalog.clone())
                .unwrap()
                .is_empty()
        );
        let listed = granted_volumes(true, &package, catalog.clone()).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, id);
        assert_eq!(listed[0].name, "Photos");
        assert!(listed[0].readonly);
        assert_eq!(
            listed[0].availability,
            SharedVolumeAvailability::Unavailable
        );
        let volume = catalog.get_mut(&id).unwrap();
        volume.name = "Renamed".into();
        volume.grants.get_mut(&package).unwrap().readonly = false;
        let listed = granted_volumes(true, &package, catalog.clone()).unwrap();
        assert_eq!(listed[0].id, id);
        assert_eq!(listed[0].name, "Renamed");
        assert!(!listed[0].readonly);
        catalog.get_mut(&id).unwrap().grants.clear();
        assert!(granted_volumes(true, &package, catalog).unwrap().is_empty());
    }
}
