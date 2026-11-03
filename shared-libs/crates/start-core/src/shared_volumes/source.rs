use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::context::RpcContext;
use crate::disk::mount::filesystem::bind::Bind;
use crate::disk::mount::filesystem::block_dev::BlockDev;
use crate::disk::mount::filesystem::cifs::{Cifs, SharedCifs};
use crate::disk::mount::filesystem::idmapped::IdMapped;
use crate::disk::mount::filesystem::syscall::open_beneath;
use crate::disk::mount::filesystem::{MountType, ReadOnly, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
use crate::prelude::*;
use crate::shared_volumes::mounts::{PreparedSource, confined_relative};
use crate::shared_volumes::{
    SharedVolumeAvailability, SharedVolumeId, SharedVolumeSource, local_path,
};

pub async fn create_local_directory(path: &Path) -> Result<(), Error> {
    tokio::fs::create_dir_all(path).await?;
    std::os::unix::fs::chown(
        path,
        Some(crate::lxc::BASE_IDMAP.to_id),
        Some(crate::lxc::BASE_IDMAP.to_id),
    )?;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o777)).await?;
    Ok(())
}

pub async fn open_source(
    ctx: &RpcContext,
    id: &SharedVolumeId,
    mount_type: MountType,
) -> Result<PreparedSource, Error> {
    let db = ctx.db.peek().await;
    let source = db
        .as_public()
        .as_shared_volumes()
        .as_idx(id)
        .or_not_found(id)?
        .as_source()
        .de()?;
    let (mounted, subpath) = match source {
        SharedVolumeSource::Local => {
            let path = local_path(crate::DATA_DIR, id);
            (open_local(&path, mount_type).await, None)
        }
        SharedVolumeSource::Disk { logicalname, path } => {
            (open_external(&logicalname, mount_type).await, Some(path))
        }
        SharedVolumeSource::Cifs { hostname, path } => {
            let credentials = db
                .as_private()
                .as_shared_volume_credentials()
                .as_idx(id)
                .or_not_found(id)?
                .de()?;
            (
                TmpMountGuard::mount(
                    &SharedCifs(Cifs {
                        hostname,
                        path,
                        username: credentials.username,
                        password: credentials.password,
                    }),
                    mount_type,
                )
                .await,
                None,
            )
        }
    };
    let result = match mounted {
        Ok(guard) => PreparedSource::open(guard, subpath).await,
        Err(error) => Err(error),
    };
    let availability = if result.is_ok() {
        SharedVolumeAvailability::Available
    } else {
        SharedVolumeAvailability::Unavailable
    };
    record_availability(ctx, id, availability).await?;
    source_result(result)
}

fn source_result(result: Result<PreparedSource, Error>) -> Result<PreparedSource, Error> {
    result.with_ctx(|error| (error.kind, t!("shared-volumes.source-unavailable")))
}

pub(crate) async fn record_availability(
    ctx: &RpcContext,
    id: &SharedVolumeId,
    availability: SharedVolumeAvailability,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            if let Some(volume) = db.as_public_mut().as_shared_volumes_mut().as_idx_mut(id) {
                volume.as_availability_mut().ser(&availability)?;
            }
            Ok(())
        })
        .await
        .result
}

pub(super) fn external_partition(
    disks: &[crate::disk::util::DiskInfo],
    logicalname: &Path,
) -> Result<PathBuf, Error> {
    disks
        .iter()
        .flat_map(|disk| &disk.partitions)
        .find(|partition| {
            partition.logicalname == logicalname || partition.stable_path == logicalname
        })
        .map(|partition| partition.logicalname.clone())
        .ok_or_else(|| {
            Error::new(
                eyre!(t!("shared-volumes.invalid-drive")),
                ErrorKind::InvalidRequest,
            )
        })
}

pub(super) async fn prepare_external_source(
    device: PathBuf,
    path: PathBuf,
) -> Result<SharedVolumeSource, Error> {
    let path = confined_relative(&path)?;
    let logicalname = BlockDev::new(device).persistent_path().await?;
    if !path.as_os_str().is_empty() {
        let drive = TmpMountGuard::mount(&BlockDev::new(&logicalname), ReadOnly).await?;
        let root = drive.path().to_owned();
        let relative = path.clone();
        let existing =
            tokio::task::spawn_blocking(move || open_beneath(&root, &relative).map(drop))
                .await
                .with_kind(ErrorKind::Unknown)?;
        drive.unmount().await?;
        if let Err(error) = existing {
            if !error
                .source
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            {
                return Err(error);
            }
            let drive = TmpMountGuard::mount(&BlockDev::new(&logicalname), ReadWrite).await?;
            let root = drive.path().to_owned();
            let relative = path.clone();
            let result =
                tokio::task::spawn_blocking(move || create_directory_beneath(&root, &relative))
                    .await
                    .with_kind(ErrorKind::Unknown)?;
            let unmount = drive.unmount().await;
            result?;
            unmount?;
        }
    }
    Ok(SharedVolumeSource::Disk { logicalname, path })
}

fn create_directory_beneath(root: &Path, path: &Path) -> Result<(), Error> {
    use nix::errno::Errno;
    use nix::sys::stat::{Mode, mkdirat};
    let relative = confined_relative(path)?;
    let mut parent = PathBuf::new();
    for name in relative.iter() {
        let directory = open_beneath(root, &parent)?;
        match mkdirat(&directory, name, Mode::from_bits_truncate(0o755)) {
            Ok(()) | Err(Errno::EEXIST) => {}
            Err(error) => return Err(Error::new(error, ErrorKind::Filesystem)),
        }
        parent.push(name);
    }
    open_beneath(root, &relative)?;
    Ok(())
}

async fn open_external(device: &Path, mount_type: MountType) -> Result<TmpMountGuard, Error> {
    TmpMountGuard::mount(
        &IdMapped::new(BlockDev::new(device), vec![crate::lxc::BASE_IDMAP]),
        mount_type,
    )
    .await
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

#[cfg(test)]
mod tests;
