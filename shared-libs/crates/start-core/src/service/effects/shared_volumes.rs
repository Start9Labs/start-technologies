use std::collections::BTreeMap;

use crate::PackageId;
use crate::db::model::DatabaseModel;
use crate::prelude::*;
use crate::service::effects::callbacks::CallbackHandler;
use crate::service::effects::prelude::*;
use crate::service::rpc::CallbackId;
use crate::shared_volumes::source::SourceDefinition;
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
                availability: volume.inspection.availability,
            })
        })
        .collect())
}

type MountInput = (
    SharedVolumeId,
    bool,
    SharedVolumeAvailability,
    SourceDefinition,
);

fn mount_inputs(package: &PackageId, db: &DatabaseModel) -> Result<Vec<MountInput>, Error> {
    let catalog = db.as_public().as_shared_volumes().de()?;
    catalog
        .into_iter()
        .filter_map(|(id, volume)| {
            let grant = volume.grants.get(package)?;
            Some(SourceDefinition::read(db, &id).map(|definition| {
                (
                    id,
                    grant.readonly,
                    volume.inspection.availability,
                    definition,
                )
            }))
        })
        .collect()
}

fn mount_inputs_changed(previous: &[MountInput], mut current: Vec<MountInput>) -> bool {
    for input in &mut current {
        if input.2 == SharedVolumeAvailability::Unknown {
            if let Some(known) = previous.iter().find(|known| known.0 == input.0) {
                input.2 = known.2.clone();
            }
        }
    }
    current != previous
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
        .watch(patch_db::json_ptr::ROOT.to_owned())
        .await
        .typed::<DatabaseModel>();
    let db = DatabaseModel::from(watch.as_mut().peek_and_mark_seen()?);
    let catalog = db.as_public().as_shared_volumes().de()?;
    let inputs = mount_inputs(&service.seed.id, &db)?;
    let result = granted_volumes(
        service
            .seed
            .persistent_container
            .s9pk
            .as_manifest()
            .shared_volumes,
        &service.seed.id,
        catalog,
    )?;
    if let Some(callback) = callback {
        let callback = callback.register(&service.seed.persistent_container);
        let package = service.seed.id.clone();
        service.seed.ctx.callbacks.add_get_shared_volumes(
            service.seed.id.clone(),
            watch,
            move |value| {
                mount_inputs(&package, &DatabaseModel::from(value.clone()))
                    .map_or(true, |current| mount_inputs_changed(&inputs, current))
            },
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
    use crate::shared_volumes::mounts::{PreparedTarget, confined_relative};
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
        Ok(source) => Ok(source.into_subpath(subpath).await),
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
    use crate::shared_volumes::{
        SharedVolumeCredentials, SharedVolumeDrive, SharedVolumeGrant, SharedVolumeSource,
    };

    #[test]
    fn consumers_are_notified_only_when_their_mount_inputs_change() {
        let package: PackageId = "consumer".parse().unwrap();
        let mine = SharedVolumeId::new();
        let other = SharedVolumeId::new();
        let mut catalog = BTreeMap::new();
        catalog.insert(
            mine.clone(),
            SharedVolume {
                name: "Music".into(),
                source: SharedVolumeSource::Cifs {
                    hostname: "nas".into(),
                    path: "/music".into(),
                    username: String::new(),
                },
                grants: [(package.clone(), SharedVolumeGrant { readonly: true })].into(),
                inspection: crate::shared_volumes::SharedVolumeInspection::available(None),
            },
        );
        catalog.insert(
            other.clone(),
            SharedVolume {
                name: "Private".into(),
                source: SharedVolumeSource::Local,
                grants: BTreeMap::new(),
                inspection: crate::shared_volumes::SharedVolumeInspection::available(None),
            },
        );
        let database = |catalog: &BTreeMap<SharedVolumeId, SharedVolume>| {
            DatabaseModel::from(
                to_value(&serde_json::json!({
                    "public": {"sharedVolumes": catalog},
                    "private": {"sharedVolumeCredentials": {
                        mine.to_string(): {"password": "old-password"}
                    }}
                }))
                .unwrap(),
            )
        };
        let inputs = mount_inputs(&package, &database(&catalog)).unwrap();
        let changed = |edit: &dyn Fn(&mut BTreeMap<SharedVolumeId, SharedVolume>)| {
            let mut edited = catalog.clone();
            edit(&mut edited);
            mount_inputs_changed(&inputs, mount_inputs(&package, &database(&edited)).unwrap())
        };
        assert!(!changed(
            &|c| c.get_mut(&mine).unwrap().name = "Songs".into()
        ));
        assert!(!changed(&|c| {
            c.get_mut(&other).unwrap().inspection.availability =
                SharedVolumeAvailability::Unavailable
        }));
        assert!(!changed(&|c| {
            c.get_mut(&mine).unwrap().inspection.availability = SharedVolumeAvailability::Unknown
        }));
        assert!(changed(&|c| {
            c.get_mut(&mine).unwrap().inspection.availability =
                SharedVolumeAvailability::Unavailable
        }));
        assert!(changed(&|c| {
            c.get_mut(&mine).unwrap().source = SharedVolumeSource::Cifs {
                hostname: "nas2".into(),
                path: "/music".into(),
                username: String::new(),
            }
        }));
        assert!(changed(&|c| {
            c.get_mut(&mine).unwrap().grants =
                [(package.clone(), SharedVolumeGrant { readonly: false })].into()
        }));
        assert!(changed(&|c| {
            c.get_mut(&other)
                .unwrap()
                .grants
                .insert(package.clone(), SharedVolumeGrant { readonly: true });
        }));
        assert!(!changed(&|c| {
            c.get_mut(&mine).unwrap().inspection =
                crate::shared_volumes::SharedVolumeInspection::available(Some(4096));
        }));
        let mut updated = database(&catalog);
        updated
            .as_private_mut()
            .as_shared_volume_credentials_mut()
            .as_idx_mut(&mine)
            .unwrap()
            .mutate(|credentials| {
                credentials.password = Some("new-password".into());
                Ok(())
            })
            .unwrap();
        assert!(mount_inputs_changed(
            &inputs,
            mount_inputs(&package, &updated).unwrap()
        ));
        updated
            .as_public_mut()
            .as_shared_volumes_mut()
            .as_idx_mut(&mine)
            .unwrap()
            .mutate(|volume| {
                volume.inspection = Default::default();
                Ok(())
            })
            .unwrap();
        assert!(mount_inputs_changed(
            &inputs,
            mount_inputs(&package, &updated).unwrap()
        ));
        assert!(
            !serde_json::to_string(&granted_volumes(true, &package, catalog.clone()).unwrap())
                .unwrap()
                .contains("password")
        );
        catalog.get_mut(&mine).unwrap().source = SharedVolumeSource::Disk {
            logicalname: "/dev/disk/by-uuid/photos".into(),
            path: "music".into(),
            drive: Some(SharedVolumeDrive {
                label: Some("Photos".into()),
                ..Default::default()
            }),
        };
        let inputs = mount_inputs(&package, &database(&catalog)).unwrap();
        if let SharedVolumeSource::Disk { drive, .. } = &mut catalog.get_mut(&mine).unwrap().source
        {
            drive.get_or_insert_default().label = Some("Renamed drive".into());
        }
        assert!(!mount_inputs_changed(
            &inputs,
            mount_inputs(&package, &database(&catalog)).unwrap()
        ));
    }

    #[test]
    fn password_only_edits_invalidate_consumers_without_disclosing_credentials() {
        let package: PackageId = "consumer".parse().unwrap();
        let id = SharedVolumeId::new();
        let other = SharedVolumeId::new();
        let catalog = BTreeMap::from([
            (
                id.clone(),
                SharedVolume {
                    name: "Music".into(),
                    source: SharedVolumeSource::Cifs {
                        hostname: "nas".into(),
                        path: "/music".into(),
                        username: "reader".into(),
                    },
                    grants: [(package.clone(), SharedVolumeGrant { readonly: true })].into(),
                    inspection: crate::shared_volumes::SharedVolumeInspection::available(None),
                },
            ),
            (
                other.clone(),
                SharedVolume {
                    name: "Private".into(),
                    source: SharedVolumeSource::Cifs {
                        hostname: "other-nas".into(),
                        path: "/private".into(),
                        username: "owner".into(),
                    },
                    grants: BTreeMap::new(),
                    inspection: crate::shared_volumes::SharedVolumeInspection::available(None),
                },
            ),
        ]);
        let mut db = DatabaseModel::from(imbl_value::json!({
            "public": { "sharedVolumes": to_value(&catalog).unwrap() },
            "private": { "sharedVolumeCredentials": to_value(&BTreeMap::from([
                (id.clone(), SharedVolumeCredentials { password: Some("old-secret".into()) }),
                (other.clone(), SharedVolumeCredentials { password: Some("other-old-secret".into()) }),
            ])).unwrap() }
        }));
        let previous = mount_inputs(&package, &db).unwrap();
        db.as_private_mut()
            .as_shared_volume_credentials_mut()
            .insert(
                &other,
                &SharedVolumeCredentials {
                    password: Some("unrelated-secret".into()),
                },
            )
            .unwrap();
        assert!(!mount_inputs_changed(
            &previous,
            mount_inputs(&package, &db).unwrap()
        ));
        db.as_private_mut()
            .as_shared_volume_credentials_mut()
            .insert(
                &id,
                &SharedVolumeCredentials {
                    password: Some("replacement-secret".into()),
                },
            )
            .unwrap();
        db.as_public_mut()
            .as_shared_volumes_mut()
            .as_idx_mut(&id)
            .unwrap()
            .mutate(|volume| {
                volume.inspection = Default::default();
                Ok(())
            })
            .unwrap();
        assert!(mount_inputs_changed(
            &previous,
            mount_inputs(&package, &db).unwrap()
        ));
        let listed = granted_volumes(
            true,
            &package,
            db.as_public().as_shared_volumes().de().unwrap(),
        )
        .unwrap();
        let public = to_value(&listed).unwrap();
        assert_eq!(public[0]["id"], to_value(&id).unwrap());
        assert!(public[0].get("source").is_none());
        assert!(public[0].get("password").is_none());
        assert!(!serde_json::to_string(&public).unwrap().contains("secret"));
    }

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
                inspection: crate::shared_volumes::SharedVolumeInspection {
                    availability: SharedVolumeAvailability::Unavailable,
                    ..Default::default()
                },
            },
        );
        catalog.insert(
            SharedVolumeId::new(),
            SharedVolume {
                name: "Private".into(),
                source: SharedVolumeSource::Local,
                grants: BTreeMap::new(),
                inspection: crate::shared_volumes::SharedVolumeInspection {
                    availability: SharedVolumeAvailability::Available,
                    ..Default::default()
                },
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
