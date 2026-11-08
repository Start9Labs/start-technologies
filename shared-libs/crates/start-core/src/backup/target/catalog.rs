use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::BackupTargetId;
use crate::context::RpcContext;
use crate::db::model::DatabaseModel;
use crate::disk::mount::filesystem::bind::Bind;
use crate::disk::mount::filesystem::block_dev::BlockDev;
use crate::disk::mount::filesystem::cifs::Cifs;
use crate::disk::mount::filesystem::{MountType, ReadOnly};
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard, WeakMountGuard};
use crate::disk::util::{StartOsRecoveryInfo, has_legacy_backup, recovery_info};
use crate::prelude::*;
use crate::shared_volumes::mounts::PreparedSource;
use crate::shared_volumes::source::{self, PreparedFilesystem, SourceDefinition};
use crate::shared_volumes::{
    CreateSharedVolumeParams, CreateSharedVolumeSource, SharedVolumeInspection, SharedVolumeSource,
};

/// A registered backup destination and its last source observation.
#[derive(Debug, Clone, Serialize, Deserialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct BackupTarget {
    pub name: String,
    pub source: SharedVolumeSource,
    #[serde(flatten)]
    pub inspection: SharedVolumeInspection,
    #[serde(default)]
    pub start_os: BTreeMap<String, StartOsRecoveryInfo>,
    #[serde(default)]
    pub legacy_backup: bool,
}

rpc_toolkit::reflect_ts!(BackupTarget);
rpc_toolkit::ts_export!(BackupTarget, namespaces = [""]);

pub(crate) type TargetMount = Arc<PreparedSource>;

#[derive(Debug)]
pub(crate) struct BackupTargetSource {
    id: BackupTargetId,
    pub name: String,
    pub source: SharedVolumeSource,
    pub(crate) definition: SourceDefinition,
}

pub(crate) fn filesystem(
    id: &BackupTargetId,
    definition: &SourceDefinition,
) -> PreparedFilesystem<BlockDev<PathBuf>, Cifs> {
    match definition.source() {
        SharedVolumeSource::Local => PreparedFilesystem::Local(local_path(crate::DATA_DIR, id)),
        SharedVolumeSource::Disk { logicalname, .. } => {
            PreparedFilesystem::Disk(BlockDev::new(logicalname.clone()))
        }
        SharedVolumeSource::Cifs { .. } => PreparedFilesystem::Cifs(definition.cifs().unwrap()),
    }
}

pub fn local_path(data_dir: impl AsRef<Path>, id: &BackupTargetId) -> PathBuf {
    data_dir.as_ref().join("backup-targets").join(id.as_ref())
}

impl BackupTargetSource {
    pub(crate) fn read(db: &DatabaseModel, id: &BackupTargetId) -> Result<Self, Error> {
        let target = db
            .as_public()
            .as_backup_targets()
            .as_idx(id)
            .or_not_found(id)?
            .de()?;
        let source = target.source;
        let credentials = if matches!(source, SharedVolumeSource::Cifs { .. }) {
            Some(
                db.as_private()
                    .as_backup_target_credentials()
                    .as_idx(id)
                    .or_not_found(id)?
                    .de()?,
            )
        } else {
            None
        };
        Ok(Self {
            id: id.clone(),
            name: target.name,
            definition: SourceDefinition::new(source.clone(), credentials),
            source,
        })
    }

    pub(crate) async fn mount(&self, mount_type: MountType) -> Result<TargetMount, Error> {
        let mount =
            TmpMountGuard::mount(&filesystem(&self.id, &self.definition), mount_type).await?;
        let prepared = PreparedSource::open(mount, self.definition.subpath()).await?;
        if !prepared.mount().is_active() {
            return Err(Error::new(
                eyre!(t!("shared-volumes.source-unavailable")),
                ErrorKind::Filesystem,
            ));
        }
        Ok(Arc::new(prepared))
    }
}

pub async fn create(
    ctx: RpcContext,
    CreateSharedVolumeParams { name, source }: CreateSharedVolumeParams,
) -> Result<BackupTargetId, Error> {
    crate::shared_volumes::validate_name(&name)?;
    let id = BackupTargetId::new();
    let (source, credentials) = match source {
        CreateSharedVolumeSource::Local => {
            let path = local_path(crate::DATA_DIR, &id);
            tokio::fs::create_dir_all(&path).await?;
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).await?;
            (SharedVolumeSource::Local, None)
        }
        CreateSharedVolumeSource::Disk { logicalname, path } => (
            source::prepare_disk_source(&ctx, logicalname, path).await?,
            None,
        ),
        CreateSharedVolumeSource::Cifs(cifs) => {
            let (source, credentials) = source::prepare_cifs_source(cifs)?;
            (source, Some(credentials))
        }
    };
    let directory =
        matches!(source, SharedVolumeSource::Local).then(|| local_path(crate::DATA_DIR, &id));
    let publication = ctx
        .db
        .mutate(|db| {
            if let Some(credentials) = credentials {
                db.as_private_mut()
                    .as_backup_target_credentials_mut()
                    .insert(&id, &credentials)?;
            }
            db.as_public_mut().as_backup_targets_mut().insert(
                &id,
                &BackupTarget {
                    name,
                    source,
                    inspection: Default::default(),
                    start_os: BTreeMap::new(),
                    legacy_backup: false,
                },
            )?;
            Ok(())
        })
        .await
        .result;
    if let Some(directory) = directory {
        source::finish_directory_publication(&directory, publication).await?;
    } else {
        publication?;
    }
    Ok(id)
}

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct TargetParams {
    pub id: BackupTargetId,
}
rpc_toolkit::reflect_ts!(TargetParams);

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct RenameTargetParams {
    pub id: BackupTargetId,
    pub name: String,
}
rpc_toolkit::reflect_ts!(RenameTargetParams);

pub async fn rename(
    ctx: RpcContext,
    RenameTargetParams { id, name }: RenameTargetParams,
) -> Result<(), Error> {
    crate::shared_volumes::validate_name(&name)?;
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_backup_targets_mut()
                .as_idx_mut(&id)
                .or_not_found(&id)?
                .as_name_mut()
                .ser(&name)
        })
        .await
        .result
}

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCifsTargetParams {
    pub id: BackupTargetId,
    pub hostname: String,
    pub path: PathBuf,
    pub username: String,
    /// Omitted keeps the stored password; empty clears it.
    #[serde(default)]
    pub password: Option<String>,
}
rpc_toolkit::reflect_ts!(UpdateCifsTargetParams);

pub async fn update_cifs(ctx: RpcContext, params: UpdateCifsTargetParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| update_cifs_definition(db, params))
        .await
        .result
}

fn update_cifs_definition(
    db: &mut DatabaseModel,
    UpdateCifsTargetParams {
        id,
        hostname,
        path,
        username,
        password,
    }: UpdateCifsTargetParams,
) -> Result<(), Error> {
    ensure_cifs(db, &id)?;
    let mut credentials = db
        .as_private()
        .as_backup_target_credentials()
        .as_idx(&id)
        .or_not_found(&id)?
        .de()?;
    credentials.update_password(password);
    let (source, credentials) = source::prepare_cifs_source(Cifs {
        hostname,
        path,
        username,
        password: credentials.password,
    })?;
    db.as_private_mut()
        .as_backup_target_credentials_mut()
        .insert(&id, &credentials)?;
    db.as_public_mut()
        .as_backup_targets_mut()
        .as_idx_mut(&id)
        .or_not_found(&id)?
        .mutate(|target| {
            target.source = source;
            target.inspection = Default::default();
            target.start_os.clear();
            target.legacy_backup = false;
            Ok(())
        })
}

pub(super) fn ensure_cifs(db: &DatabaseModel, id: &BackupTargetId) -> Result<(), Error> {
    let source = db
        .as_public()
        .as_backup_targets()
        .as_idx(id)
        .or_not_found(id)?
        .as_source()
        .de()?;
    if !matches!(source, SharedVolumeSource::Cifs { .. }) {
        return Err(Error::new(
            eyre!(t!("shared-volumes.not-cifs")),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

pub async fn remove(ctx: RpcContext, TargetParams { id }: TargetParams) -> Result<(), Error> {
    let db = ctx.db.peek().await;
    let source = db
        .as_public()
        .as_backup_targets()
        .as_idx(&id)
        .map(|target| target.as_source().de())
        .transpose()?;
    let local = matches!(source, None | Some(SharedVolumeSource::Local));
    let path = local_path(crate::DATA_DIR, &id);
    let _unused = if local {
        Some(TmpMountGuard::reserve_unmounted(&Bind::new(&path), ReadOnly).await?)
    } else {
        None
    };
    ctx.db
        .mutate(|db| {
            db.as_public_mut().as_backup_targets_mut().remove(&id)?;
            db.as_private_mut()
                .as_backup_target_credentials_mut()
                .remove(&id)?;
            Ok(())
        })
        .await
        .result?;
    if local {
        crate::util::io::delete_dir(&path).await?;
    }
    Ok(())
}

pub async fn inspect(
    ctx: RpcContext,
    TargetParams { id }: TargetParams,
) -> Result<SharedVolumeInspection, Error> {
    refresh(&ctx.db, &id).await?;
    ctx.db
        .peek()
        .await
        .as_public()
        .as_backup_targets()
        .as_idx(&id)
        .or_not_found(&id)?
        .as_inspection()
        .de()
}

#[derive(Debug)]
pub(crate) struct TargetContents {
    start_os: BTreeMap<String, StartOsRecoveryInfo>,
    legacy_backup: bool,
}

pub(crate) async fn read_contents(path: &Path, server_id: &str) -> Result<TargetContents, Error> {
    Ok(TargetContents {
        start_os: recovery_info(path).await?,
        legacy_backup: has_legacy_backup(path, server_id).await,
    })
}

pub(super) async fn inspect_contents(
    path: &Path,
    server_id: &str,
) -> Result<(SharedVolumeInspection, TargetContents), Error> {
    let capacity = source::measure_mount(path)
        .await
        .map_err(source::public_error_message);
    let contents = read_contents(path, server_id).await?;
    Ok((
        SharedVolumeInspection::available_capacity(capacity),
        contents,
    ))
}

pub(crate) async fn refresh(
    database: &TypedPatchDb<crate::db::model::Database>,
    id: &BackupTargetId,
) -> Result<(), Error> {
    let db = database.peek().await;
    if db.as_public().as_backup_targets().as_idx(id).is_none() {
        return Ok(());
    }
    let target = BackupTargetSource::read(&db, id)?;
    let server_id = db.as_public().as_server_info().as_id().de()?;
    let mut guard = None;
    let result = async {
        guard = Some(target.mount(ReadOnly).await?);
        inspect_contents(guard.as_ref().unwrap().path(), &server_id).await
    }
    .await;
    let (inspection, metadata) = match result {
        Ok((inspection, contents)) => (inspection, Some(contents)),
        Err(error) => (
            SharedVolumeInspection::unavailable(source::public_error_message(error)),
            None,
        ),
    };
    let mount = guard.as_ref().map(|guard| guard.mount().downgrade());
    let published = database
        .mutate(|db| {
            write_inspection(
                db,
                id,
                &target.definition,
                mount.as_ref(),
                inspection,
                metadata.as_ref(),
            )
        })
        .await
        .result;
    let cleanup = match guard {
        Some(guard) => guard.unmount().await,
        None => Ok(()),
    };
    published?;
    cleanup
}

pub(crate) fn needs_content_validation(inspection: &SharedVolumeInspection) -> bool {
    inspection.availability != crate::shared_volumes::SharedVolumeAvailability::Available
}

pub(crate) fn write_inspection(
    db: &mut DatabaseModel,
    id: &BackupTargetId,
    definition: &SourceDefinition,
    mount: Option<&WeakMountGuard>,
    inspection: SharedVolumeInspection,
    metadata: Option<&TargetContents>,
) -> Result<(), Error> {
    if db.as_public().as_backup_targets().as_idx(id).is_none() {
        return Ok(());
    }
    if BackupTargetSource::read(db, id)?.definition != *definition
        || mount.is_some_and(|mount| mount.active_path().is_none())
    {
        return Ok(());
    }
    let previous = db
        .as_public()
        .as_backup_targets()
        .as_idx(id)
        .or_not_found(id)?
        .as_inspection()
        .de()?;
    // Availability recovery requires revalidating target contents.
    if inspection.availability == crate::shared_volumes::SharedVolumeAvailability::Available
        && needs_content_validation(&previous)
        && metadata.is_none()
    {
        return Ok(());
    }
    db.as_public_mut()
        .as_backup_targets_mut()
        .as_idx_mut(id)
        .or_not_found(id)?
        .mutate(|target| {
            target.inspection = inspection;
            if let Some(contents) = metadata {
                target.start_os = contents.start_os.clone();
                target.legacy_backup = contents.legacy_backup;
            }
            Ok(())
        })
}

pub(crate) fn initialize(db: &mut DatabaseModel) -> Result<(), Error> {
    let mut targets = db.as_public().as_backup_targets().de()?;
    let mut credentials = db.as_private().as_backup_target_credentials().de()?;
    let previous = db.as_private().as_cifs().de()?;
    for (number, cifs) in previous.0 {
        let id = format!("cifs-{number}").parse::<BackupTargetId>()?;
        if targets.contains_key(&id) {
            continue;
        }
        let name = cifs_name(&cifs);
        let (source, password) = source::cifs_definition(cifs);
        credentials.insert(id.clone(), password);
        targets.insert(
            id,
            BackupTarget {
                name,
                source,
                inspection: Default::default(),
                start_os: BTreeMap::new(),
                legacy_backup: false,
            },
        );
    }
    for target in targets.values_mut() {
        target.inspection = Default::default();
    }
    db.as_public_mut().as_backup_targets_mut().ser(&targets)?;
    db.as_private_mut()
        .as_backup_target_credentials_mut()
        .ser(&credentials)?;
    db.as_private_mut()
        .as_cifs_mut()
        .ser(&super::cifs::CifsTargets::default())
}

#[cfg(test)]
mod tests;

pub(super) fn cifs_name(cifs: &Cifs) -> String {
    format!("{}:{}", cifs.hostname, cifs.path.display())
}
