use std::collections::BTreeMap;

use crate::backup::target::{BackupTargetId, catalog};
use crate::context::RpcContext;
use crate::db::model::DatabaseModel;
use crate::disk::mount::guard::{TmpMountGuard, WeakMountGuard};
use crate::prelude::*;
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

#[derive(Clone)]
pub(super) struct RegisteredSource {
    pub id: SourceId,
    pub definition: SourceDefinition,
    pub inspection: SharedVolumeInspection,
    pub retryable: bool,
}

pub(super) fn snapshot(db: &DatabaseModel) -> Result<BTreeMap<SourceId, RegisteredSource>, Error> {
    let mut sources = BTreeMap::new();
    let mut insert =
        |id: SourceId, inspection: SharedVolumeInspection, required: bool| -> Result<(), Error> {
            let definition = id.definition(db)?;
            let retryable = required
                && matches!(definition.source(), SharedVolumeSource::Cifs { .. })
                && inspection.availability == SharedVolumeAvailability::Unavailable;
            sources.insert(
                id.clone(),
                RegisteredSource {
                    id,
                    definition,
                    inspection,
                    retryable,
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

impl RegisteredSource {
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
}

#[cfg(test)]
mod tests;
