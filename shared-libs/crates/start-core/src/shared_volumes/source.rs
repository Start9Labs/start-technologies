use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::context::RpcContext;
use crate::disk::mount::filesystem::bind::Bind;
use crate::disk::mount::filesystem::block_dev::BlockDev;
use crate::disk::mount::filesystem::cifs::{Cifs, SharedCifs};
use crate::disk::mount::filesystem::idmapped::IdMapped;
use crate::disk::mount::filesystem::syscall::open_beneath;
use crate::disk::mount::filesystem::{FileSystem, MountType, ReadOnly, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard, WeakMountGuard};
use crate::prelude::*;
use crate::shared_volumes::mounts::{PreparedSource, confined_relative};
use crate::shared_volumes::{
    SharedVolumeCredentials, SharedVolumeId, SharedVolumeInspection, SharedVolumeSource, local_path,
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

// The directory must be newly allocated for this publication.
pub(crate) async fn finish_directory_publication(
    path: &Path,
    result: Result<(), Error>,
) -> Result<(), Error> {
    if result.is_err() {
        crate::util::io::delete_dir(path).await.log_err();
    }
    result
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceDefinition {
    source: SharedVolumeSource,
    credentials: Option<SharedVolumeCredentials>,
}
impl SourceDefinition {
    pub(crate) fn read(
        db: &crate::db::model::DatabaseModel,
        id: &SharedVolumeId,
    ) -> Result<Self, Error> {
        let source = db
            .as_public()
            .as_shared_volumes()
            .as_idx(id)
            .or_not_found(id)?
            .as_source()
            .de()?;
        let credentials = if matches!(source, SharedVolumeSource::Cifs { .. }) {
            Some(
                db.as_private()
                    .as_shared_volume_credentials()
                    .as_idx(id)
                    .or_not_found(id)?
                    .de()?,
            )
        } else {
            None
        };
        Ok(Self::new(source, credentials))
    }

    pub(crate) fn new(
        mut source: SharedVolumeSource,
        credentials: Option<SharedVolumeCredentials>,
    ) -> Self {
        if let SharedVolumeSource::Disk { label, .. } = &mut source {
            *label = None;
        }
        Self {
            source,
            credentials,
        }
    }

    pub(crate) fn cifs(&self) -> Option<Cifs> {
        let SharedVolumeSource::Cifs {
            hostname,
            path,
            username,
        } = &self.source
        else {
            return None;
        };
        Some(Cifs {
            hostname: hostname.clone(),
            path: path.clone(),
            username: username.clone(),
            password: self.credentials.as_ref().unwrap().password.clone(),
        })
    }

    pub(crate) fn source(&self) -> &SharedVolumeSource {
        &self.source
    }

    pub(crate) fn subpath(&self) -> Option<PathBuf> {
        match &self.source {
            SharedVolumeSource::Disk { path, .. } => Some(path.clone()),
            _ => None,
        }
    }

    pub(super) fn filesystem(&self, id: &SharedVolumeId) -> SourceFilesystem {
        match &self.source {
            SharedVolumeSource::Local => {
                SourceFilesystem::Local(local_path(crate::PACKAGE_DATA, id))
            }
            SharedVolumeSource::Disk { logicalname, .. } => SourceFilesystem::external(logicalname),
            SharedVolumeSource::Cifs { .. } => {
                SourceFilesystem::Cifs(SharedCifs(self.cifs().unwrap()))
            }
        }
    }
}

pub(crate) enum PreparedFilesystem<D, C> {
    Local(PathBuf),
    Disk(D),
    Cifs(C),
}
pub(super) type SourceFilesystem = PreparedFilesystem<IdMapped<BlockDev<PathBuf>>, SharedCifs>;
impl SourceFilesystem {
    fn external(device: &Path) -> Self {
        Self::Disk(IdMapped::new(
            BlockDev::new(device.to_owned()),
            vec![crate::lxc::BASE_IDMAP],
        ))
    }
}
impl<D: FileSystem, C: FileSystem> FileSystem for PreparedFilesystem<D, C> {
    async fn mount<P: AsRef<Path> + Send>(
        &self,
        mountpoint: P,
        mount_type: MountType,
    ) -> Result<(), Error> {
        match self {
            Self::Local(path) => {
                if !tokio::fs::metadata(path).await?.is_dir() {
                    return Err(Error::new(
                        eyre!(t!("shared-volumes.source-unavailable")),
                        ErrorKind::Filesystem,
                    ));
                }
                Bind::new(path).mount(mountpoint, mount_type).await
            }
            Self::Disk(filesystem) => filesystem.mount(mountpoint, mount_type).await,
            Self::Cifs(filesystem) => filesystem.mount(mountpoint, mount_type).await,
        }
    }
    async fn check_mount(&self, mountpoint: &Path) -> Result<(), Error> {
        match self {
            Self::Cifs(filesystem) => filesystem.check_mount(mountpoint).await,
            Self::Disk(filesystem) => filesystem.check_mount(mountpoint).await,
            Self::Local(_) => Ok(()),
        }
    }
    async fn device(&self) -> Result<Option<PathBuf>, Error> {
        match self {
            Self::Disk(filesystem) => filesystem.device().await,
            Self::Cifs(filesystem) => filesystem.device().await,
            Self::Local(_) => Ok(None),
        }
    }
    async fn source_hash(&self) -> Result<digest::Output<sha2::Sha256>, Error> {
        match self {
            Self::Local(path) => Bind::new(path).source_hash().await,
            Self::Disk(filesystem) => filesystem.source_hash().await,
            Self::Cifs(filesystem) => filesystem.source_hash().await,
        }
    }
    async fn mount_hash(
        &self,
        mount_type: MountType,
    ) -> Result<digest::Output<sha2::Sha256>, Error> {
        match self {
            Self::Cifs(filesystem) => filesystem.mount_hash(mount_type).await,
            Self::Disk(filesystem) => filesystem.mount_hash(mount_type).await,
            Self::Local(path) => Bind::new(path).mount_hash(mount_type).await,
        }
    }
}

/// Capacity failures do not invalidate a successfully opened source.
pub async fn open_source(
    ctx: &RpcContext,
    id: &SharedVolumeId,
    mount_type: MountType,
) -> Result<PreparedSource, Error> {
    let definition = SourceDefinition::read(&ctx.db.peek().await, id)?;
    let filesystem = definition.filesystem(id);
    let subpath = definition.subpath();
    let mut prepared = None;
    let result = source_result(
        async {
            let mount = TmpMountGuard::mount(&filesystem, mount_type).await?;
            prepared = Some(PreparedSource::open(mount, subpath).await?);
            Ok(())
        }
        .await,
    );
    let inspection = match &result {
        Ok(()) => SharedVolumeInspection::available_capacity(
            measure_mount(prepared.as_ref().unwrap().path())
                .await
                .map_err(public_error_message),
        ),
        Err(error) => SharedVolumeInspection::unavailable(public_error_message(error)),
    };
    let mount = prepared.as_ref().map(|source| source.mount().downgrade());
    let accepted = ctx
        .db
        .mutate(|db| write_checked(db, id, &definition, mount.as_ref(), inspection))
        .await
        .result?;
    if !accepted
        || prepared
            .as_ref()
            .is_some_and(|source| !source.mount().is_active())
    {
        return source_result(Err(Error::new(
            eyre!(t!("shared-volumes.source-unavailable")),
            ErrorKind::Filesystem,
        )));
    }
    result.map(|_| prepared.unwrap())
}

pub(super) fn write_checked(
    db: &mut crate::db::model::DatabaseModel,
    id: &SharedVolumeId,
    definition: &SourceDefinition,
    mount: Option<&WeakMountGuard>,
    inspection: SharedVolumeInspection,
) -> Result<bool, Error> {
    if db.as_public().as_shared_volumes().as_idx(id).is_some()
        && SourceDefinition::read(db, id)? == *definition
        && mount.is_none_or(|mount| mount.active_path().is_some())
    {
        write_inspection(db, id, inspection)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub(crate) fn public_error_message(error: impl std::fmt::Display) -> String {
    static MOUNT_PATH: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(&format!(
            r#"{}/[^/\s:;,'"()]+"#,
            regex::escape(crate::disk::mount::guard::TMP_MOUNTPOINT)
        ))
        .unwrap()
    });
    MOUNT_PATH
        .replace_all(&error.to_string(), "[source]")
        .into_owned()
}

fn source_result<T>(result: Result<T, Error>) -> Result<T, Error> {
    result.with_ctx(|error| {
        (
            match error.kind {
                ErrorKind::InvalidRequest => ErrorKind::Filesystem,
                kind => kind,
            },
            t!("shared-volumes.source-unavailable"),
        )
    })
}

fn write_inspection(
    db: &mut crate::db::model::DatabaseModel,
    id: &SharedVolumeId,
    inspection: SharedVolumeInspection,
) -> Result<(), Error> {
    if let Some(volume) = db.as_public_mut().as_shared_volumes_mut().as_idx_mut(id) {
        if volume.as_inspection().de()? != inspection {
            // Flattened field serialization replaces the entire parent object.
            volume.mutate(|volume| {
                volume.inspection = inspection;
                Ok(())
            })?;
        }
    }
    Ok(())
}

pub(super) const SPACE_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub(crate) async fn measure_mount(path: &Path) -> Result<u64, Error> {
    tokio::time::timeout(SPACE_QUERY_TIMEOUT, crate::disk::util::get_available(path))
        .await
        .with_kind(ErrorKind::Timeout)
        .and_then(|result| result)
}

pub(crate) fn prepare_cifs_source(
    cifs: Cifs,
) -> Result<(SharedVolumeSource, SharedVolumeCredentials), Error> {
    cifs.validate()?;
    Ok(cifs_definition(cifs))
}

pub(crate) fn cifs_definition(cifs: Cifs) -> (SharedVolumeSource, SharedVolumeCredentials) {
    let Cifs {
        hostname,
        path,
        username,
        password,
    } = cifs;
    (
        SharedVolumeSource::Cifs {
            hostname,
            path: Path::new("/").join(path),
            username,
        },
        SharedVolumeCredentials { password },
    )
}

pub(crate) async fn prepare_disk_source(
    ctx: &RpcContext,
    logicalname: PathBuf,
    path: PathBuf,
) -> Result<SharedVolumeSource, Error> {
    let disks = crate::disk::util::list(&ctx.os_partitions, None).await?;
    let logicalname = tokio::fs::canonicalize(logicalname).await?;
    let (partition, label) = external_partition(&disks, &logicalname)?;
    crate::disk::format::refuse_system_drive(
        &ctx.os_partitions,
        &crate::disk::format::parent_drive(&partition).await?,
    )
    .await?;
    prepare_external_source(partition, path, label).await
}

pub(super) fn external_partition(
    disks: &[crate::disk::util::DiskInfo],
    logicalname: &Path,
) -> Result<(PathBuf, Option<String>), Error> {
    disks
        .iter()
        .flat_map(|disk| {
            disk.partitions
                .iter()
                .map(move |partition| (disk, partition))
        })
        .find(|(_, partition)| {
            partition.logicalname == logicalname || partition.stable_path == logicalname
        })
        .map(|(disk, partition)| {
            let model = [disk.vendor.as_deref(), disk.model.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            (
                partition.logicalname.clone(),
                partition
                    .label
                    .clone()
                    .or((!model.is_empty()).then_some(model)),
            )
        })
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
    label: Option<String>,
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
    Ok(SharedVolumeSource::Disk {
        logicalname,
        path,
        label,
    })
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

#[cfg(test)]
async fn open_external(device: &Path, mount_type: MountType) -> Result<TmpMountGuard, Error> {
    TmpMountGuard::mount(&SourceFilesystem::external(device), mount_type).await
}

#[cfg(test)]
mod tests;
