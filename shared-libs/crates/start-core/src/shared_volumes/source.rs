use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::context::RpcContext;
use crate::disk::mount::filesystem::MountType;
use crate::disk::mount::filesystem::bind::Bind;
use crate::disk::mount::filesystem::cifs::{Cifs, SharedCifs};
use crate::disk::mount::guard::TmpMountGuard;
use crate::prelude::*;
use crate::shared_volumes::{
    SharedVolumeAvailability, SharedVolumeId, SharedVolumeSource, local_path,
};

pub async fn create_local_directory(path: &Path) -> Result<(), Error> {
    tokio::fs::create_dir_all(path).await?;
    std::os::unix::fs::chown(path, Some(100000), Some(100000))?;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o777)).await?;
    Ok(())
}

pub async fn open_source(
    ctx: &RpcContext,
    id: &SharedVolumeId,
    mount_type: MountType,
) -> Result<TmpMountGuard, Error> {
    let db = ctx.db.peek().await;
    let source = db
        .as_public()
        .as_shared_volumes()
        .as_idx(id)
        .or_not_found(id)?
        .as_source()
        .de()?;
    let result = match source {
        SharedVolumeSource::Local => {
            let path = local_path(crate::DATA_DIR, id);
            open_local(&path, mount_type).await
        }
        SharedVolumeSource::Cifs { hostname, path } => {
            let credentials = db
                .as_private()
                .as_shared_volume_credentials()
                .as_idx(id)
                .or_not_found(id)?
                .de()?;
            TmpMountGuard::mount(
                &SharedCifs(Cifs {
                    hostname,
                    path,
                    username: credentials.username,
                    password: credentials.password,
                }),
                mount_type,
            )
            .await
        }
    };
    let availability = if result.is_ok() {
        SharedVolumeAvailability::Available
    } else {
        SharedVolumeAvailability::Unavailable
    };
    ctx.db
        .mutate(|db| {
            if let Some(volume) = db.as_public_mut().as_shared_volumes_mut().as_idx_mut(id) {
                volume.as_availability_mut().ser(&availability)?;
            }
            Ok(())
        })
        .await
        .result?;
    result.map_err(|error| Error::new(eyre!(t!("shared-volumes.source-unavailable")), error.kind))
}

async fn open_local(path: &Path, mount_type: MountType) -> Result<TmpMountGuard, Error> {
    if !tokio::fs::metadata(path).await?.is_dir() {
        return Err(Error::new(
            eyre!(t!("shared-volumes.source-unavailable")),
            ErrorKind::Filesystem,
        ));
    }
    TmpMountGuard::mount(&Bind::new(path), mount_type).await
}
