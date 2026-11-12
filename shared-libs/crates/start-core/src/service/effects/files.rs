use std::path::PathBuf;

use crate::rpc_continuations::Guid;
use crate::s9pk::manifest::Manifest;
use crate::service::action_files::{UploadLimits, UploadedFile, invalid};
use crate::service::effects::prelude::*;
use crate::{DATA_DIR, VolumeId};

#[derive(Debug, Clone, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct PrepareUploadParams {
    /// A volume declared by this service's manifest.
    pub volume: VolumeId,
    /// Volume-relative file or existing directory; directories retain the uploaded basename.
    pub path: String,
    #[serde(flatten)]
    pub limits: UploadLimits,
    /// Installed file permissions; defaults to 0600.
    pub mode: Option<u32>,
    /// Logical service UID; defaults to 0.
    pub uid: Option<u32>,
    /// Logical service GID; defaults to 0.
    pub gid: Option<u32>,
    #[serde(flatten)]
    #[visit(opaque, ts(skip))]
    event: EventId,
}
rpc_toolkit::reflect_ts!(PrepareUploadParams);

#[derive(Debug, Clone, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct PrepareDownloadParams {
    /// A volume declared by this service's manifest.
    pub volume: VolumeId,
    /// Volume-relative regular file with no symlinks or traversal. Keep it stable until cleanup.
    pub path: String,
    /// Download Content-Type; defaults to application/octet-stream.
    pub mime: Option<String>,
    /// Suggested download name; defaults to the path's final component.
    pub filename: Option<String>,
    #[serde(flatten)]
    #[visit(opaque, ts(skip))]
    event: EventId,
}
rpc_toolkit::reflect_ts!(PrepareDownloadParams);

#[derive(Debug, Clone, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct ResolveUploadParams {
    pub id: Guid,
    #[serde(flatten)]
    #[visit(opaque, ts(skip))]
    event: EventId,
}
rpc_toolkit::reflect_ts!(ResolveUploadParams);

fn volume_root(manifest: &Manifest, volume: &VolumeId) -> Result<PathBuf, Error> {
    if !manifest.volumes.contains(volume) {
        return Err(invalid());
    }
    Ok(crate::volume::data_dir(DATA_DIR, &manifest.id, volume))
}

fn event_id(event: EventId) -> Result<Guid, Error> {
    event.event_id.ok_or_else(invalid)
}

pub async fn prepare_upload(
    context: EffectContext,
    params: PrepareUploadParams,
) -> Result<Guid, Error> {
    let service = context.deref()?;
    let root = volume_root(
        service.seed.persistent_container.s9pk.as_manifest(),
        &params.volume,
    )?;
    service
        .seed
        .action_files
        .prepare_upload(
            &service.seed.ctx.rpc_continuations,
            event_id(params.event)?,
            &root,
            params.path,
            params.limits,
            params.mode,
            params.uid,
            params.gid,
        )
        .await
}

pub async fn prepare_download(
    context: EffectContext,
    params: PrepareDownloadParams,
) -> Result<Guid, Error> {
    let service = context.deref()?;
    let root = volume_root(
        service.seed.persistent_container.s9pk.as_manifest(),
        &params.volume,
    )?;
    service.seed.action_files.prepare_download(
        event_id(params.event)?,
        &root,
        &params.path,
        params.mime,
        params.filename,
    )
}

pub async fn get_upload_limits(
    context: EffectContext,
    params: ResolveUploadParams,
) -> Result<UploadLimits, Error> {
    let service = context.deref()?;
    service
        .seed
        .action_files
        .get_upload_limits(&event_id(params.event)?, &params.id)
}

pub async fn resolve_upload(
    context: EffectContext,
    params: ResolveUploadParams,
) -> Result<UploadedFile, Error> {
    let service = context.deref()?;
    service
        .seed
        .action_files
        .resolve_upload(&event_id(params.event)?, &params.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparation_uses_only_a_declared_manifest_volume() {
        let manifest: Manifest = serde_json::from_str(r#"{
            "id": "test", "version": "1.0.0:0",
            "canMigrateTo": "1.0.0:0", "canMigrateFrom": "1.0.0:0",
            "title": "Test", "description": {"short": "Test", "long": "Test"},
            "releaseNotes": "Test", "license": "MIT",
            "packageRepo": "https://example.com/package", "upstreamRepo": "https://example.com/upstream",
            "images": {}, "volumes": ["main"]
        }"#).unwrap();
        assert_eq!(
            volume_root(&manifest, &"main".parse().unwrap()).unwrap(),
            crate::volume::data_dir(DATA_DIR, &manifest.id, &"main".parse().unwrap())
        );
        assert!(volume_root(&manifest, &"missing".parse().unwrap()).is_err());
    }

    #[test]
    fn upload_limits_are_flattened_and_omit_to_unbounded() {
        let omitted: PrepareUploadParams =
            serde_json::from_str(r#"{"volume":"main","path":"."}"#).unwrap();
        assert_eq!(omitted.limits.max_size, None);
        for max in [0, 3] {
            let params: PrepareUploadParams = serde_json::from_value(
                serde_json::json!({"volume":"main","path":"directory/","maxSize":max}),
            )
            .unwrap();
            assert_eq!(params.limits.max_size, Some(max));
            let encoded = serde_json::to_value(params).unwrap();
            assert_eq!(encoded["maxSize"], max);
            assert!(encoded.get("limits").is_none());
        }
        assert!(
            serde_json::from_value::<PrepareUploadParams>(
                serde_json::json!({"volume":"main","path":"file","maxSize":-1})
            )
            .is_err()
        );
    }

    #[test]
    fn transfer_effects_require_the_implicit_procedure_event() {
        let params: PrepareUploadParams =
            serde_json::from_str(r#"{"volume":"main","path":"file"}"#).unwrap();
        assert!(event_id(params.event).is_err());
        let event = Guid::new();
        let params: ResolveUploadParams = serde_json::from_str(&format!(
            r#"{{"id":"{}","eventId":"{}"}}"#,
            Guid::new(),
            event
        ))
        .unwrap();
        assert_eq!(event_id(params.event).unwrap(), event);
    }
}
