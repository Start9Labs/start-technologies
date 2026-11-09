use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::backup::target::{BackupTargetId, catalog};
use crate::context::RpcContext;
use crate::db::model::DatabaseModel;
use crate::disk::mount::guard::{TmpMountGuard, WeakMountGuard};
use crate::prelude::*;
use crate::shared_volumes::mounts::PinnedDirectory;
use crate::shared_volumes::source::{self, SourceDefinition};
use crate::shared_volumes::{
    SharedVolumeAvailability, SharedVolumeId, SharedVolumeInspection, SharedVolumeSource,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum SourceId {
    Volume(SharedVolumeId),
    Target(BackupTargetId),
}

impl SourceId {
    fn definition(&self, db: &DatabaseModel) -> Result<SourceDefinition, Error> {
        match self {
            Self::Volume(id) => SourceDefinition::read(db, id),
            Self::Target(id) => Ok(catalog::BackupTargetSource::read(db, id)?.definition),
        }
    }
}

#[derive(Debug)]
pub(super) enum DirectoryContents {
    Folder,
    Target(catalog::TargetContents),
}

#[derive(Clone)]
pub(super) struct RegisteredSource {
    pub id: SourceId,
    pub definition: SourceDefinition,
    pub inspection: SharedVolumeInspection,
    pub check_unmounted: bool,
}

pub(super) fn snapshot(db: &DatabaseModel) -> Result<BTreeMap<SourceId, RegisteredSource>, Error> {
    let mut sources = BTreeMap::new();
    let mut insert =
        |id: SourceId, inspection: SharedVolumeInspection, required: bool| -> Result<(), Error> {
            let definition = id.definition(db)?;
            sources.insert(
                id.clone(),
                RegisteredSource {
                    id,
                    definition,
                    inspection,
                    check_unmounted: required,
                },
            );
            Ok(())
        };
    for (id, volume) in db.as_public().as_shared_volumes().de()? {
        insert(
            SourceId::Volume(id),
            volume.inspection,
            !volume.grants.is_empty(),
        )?;
    }
    for (id, target) in db.as_public().as_backup_targets().de()? {
        insert(SourceId::Target(id), target.inspection, true)?;
    }
    Ok(sources)
}

pub(super) fn directory_inspector<'a>(
    sources: impl Iterator<Item = &'a RegisteredSource>,
) -> Option<&'a RegisteredSource> {
    sources.min_by_key(|source| !source.needs_contents())
}

impl RegisteredSource {
    // Cold CIFS checks require a granted volume or a registered target.
    pub fn needs_inspection(&self, startup: bool, restart: bool) -> bool {
        let cifs = matches!(self.definition.source(), SharedVolumeSource::Cifs { .. });
        (!cifs || self.check_unmounted)
            && (startup
                || restart
                || self.inspection.availability == SharedVolumeAvailability::Unknown)
    }

    pub fn retryable(&self) -> bool {
        self.check_unmounted
            && matches!(self.definition.source(), SharedVolumeSource::Cifs { .. })
            && self.inspection.availability == SharedVolumeAvailability::Unavailable
    }

    pub fn needs_contents(&self) -> bool {
        matches!(self.id, SourceId::Target(_))
            && catalog::needs_content_validation(&self.inspection)
    }

    pub async fn check_directory(
        &self,
        root: PathBuf,
        server_id: String,
    ) -> Result<DirectoryContents, Error> {
        let relative = self.definition.subpath().unwrap_or_default();
        let directory =
            tokio::task::spawn_blocking(move || PinnedDirectory::open(&root, &relative))
                .await
                .with_kind(ErrorKind::Unknown)??;
        if self.needs_contents() {
            Ok(DirectoryContents::Target(
                catalog::read_contents(directory.path(), &server_id).await?,
            ))
        } else {
            Ok(DirectoryContents::Folder)
        }
    }

    pub async fn inspect(&self, ctx: RpcContext) -> Result<(), Error> {
        match &self.id {
            SourceId::Volume(id) => {
                super::super::inspect(ctx, super::super::SharedVolumeParams { id: id.clone() })
                    .await
                    .map(|_| ())
            }
            SourceId::Target(id) => {
                let database = ctx.db.clone();
                drop(ctx);
                catalog::refresh(&database, id).await
            }
        }
    }

    pub async fn retry(&self, ctx: RpcContext) -> Result<(), Error> {
        if let SharedVolumeSource::Cifs { hostname, .. } = self.definition.source() {
            if crate::disk::mount::filesystem::cifs::reachable(hostname).await {
                self.inspect(ctx).await?;
            }
        }
        Ok(())
    }

    pub async fn existing(&self) -> Result<Vec<MountedSource>, Error> {
        let mounts = match &self.id {
            SourceId::Volume(id) => {
                TmpMountGuard::existing_all(&self.definition.filesystem(id)).await?
            }
            SourceId::Target(id) => {
                TmpMountGuard::existing_all(&catalog::filesystem(id, &self.definition)).await?
            }
        };
        Ok(mounts
            .into_iter()
            .map(|mount| MountedSource {
                source: self.clone(),
                mount,
            })
            .collect())
    }

    pub async fn record(
        &self,
        ctx: &RpcContext,
        mount: Option<&WeakMountGuard>,
        inspection: SharedVolumeInspection,
    ) -> Result<(), Error> {
        ctx.db
            .mutate(|db| match &self.id {
                SourceId::Volume(id) => {
                    source::write_checked(db, id, &self.definition, mount, inspection).map(|_| ())
                }
                SourceId::Target(id) => {
                    catalog::write_inspection(db, id, &self.definition, mount, inspection, None)
                }
            })
            .await
            .result
    }
}

pub(super) struct MountedSource {
    pub source: RegisteredSource,
    pub mount: WeakMountGuard,
}

impl MountedSource {
    pub fn relative_path(&self) -> std::path::PathBuf {
        self.source.definition.subpath().unwrap_or_default()
    }

    pub async fn record(
        &self,
        ctx: &RpcContext,
        inspection: SharedVolumeInspection,
    ) -> Result<(), Error> {
        self.source.record(ctx, Some(&self.mount), inspection).await
    }

    pub async fn record_directory(
        &self,
        ctx: &RpcContext,
        inspection: SharedVolumeInspection,
        directory: &DirectoryContents,
    ) -> Result<(), Error> {
        match (&self.source.id, directory) {
            (SourceId::Target(id), DirectoryContents::Target(contents)) => {
                ctx.db
                    .mutate(|db| {
                        catalog::write_inspection(
                            db,
                            id,
                            &self.source.definition,
                            Some(&self.mount),
                            inspection,
                            Some(contents),
                        )
                    })
                    .await
                    .result
            }
            _ => self.record(ctx, inspection).await,
        }
    }
}

#[cfg(test)]
mod tests;
