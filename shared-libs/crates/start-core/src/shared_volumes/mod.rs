use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::PackageId;

pub mod mounts;
pub mod source;
#[cfg(target_os = "linux")]
pub(crate) mod watch;
use crate::context::RpcContext;
use crate::disk::mount::filesystem::cifs::Cifs;
use crate::prelude::*;
use crate::rpc_continuations::Guid;

/// A 32-character uppercase base32 volume identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, VisitFields)]
#[serde(try_from = "String", into = "String")]
#[visit(type_attributes(serde::try_from, serde::into))]
pub struct SharedVolumeId(Guid);

rpc_toolkit::reflect_ts!(SharedVolumeId);
rpc_toolkit::ts_export!(SharedVolumeId, namespaces = [""]);

impl SharedVolumeId {
    pub fn new() -> Self {
        Self(Guid::new())
    }
}

impl Default for SharedVolumeId {
    fn default() -> Self {
        Self::new()
    }
}

impl TryFrom<String> for SharedVolumeId {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl FromStr for SharedVolumeId {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Guid::from_str(value).map(Self).map_err(|error| Error {
            kind: ErrorKind::InvalidRequest,
            ..error
        })
    }
}

impl From<SharedVolumeId> for String {
    fn from(value: SharedVolumeId) -> Self {
        value.0.to_string()
    }
}

impl std::fmt::Display for SharedVolumeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for SharedVolumeId {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, VisitVariants)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SharedVolumeSource {
    /// Managed Btrfs subvolume on the encrypted data partition.
    Local,
    Disk {
        logicalname: PathBuf,
        #[serde(default)]
        path: PathBuf,
        /// The drive as reported when the source was created, for display.
        #[serde(default)]
        drive: Option<SharedVolumeDrive>,
    },
    Cifs {
        hostname: String,
        path: PathBuf,
        #[serde(default)]
        username: String,
    },
}

rpc_toolkit::reflect_ts!(SharedVolumeSource);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SharedVolumeDrive {
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub label: Option<String>,
    pub capacity: u64,
}

rpc_toolkit::reflect_ts!(SharedVolumeDrive);

impl SharedVolumeDrive {
    /// The partition label, else the vendor and model.
    pub fn display_name(&self) -> Option<String> {
        self.label
            .as_ref()
            .filter(|label| !label.trim().is_empty())
            .cloned()
            .or_else(|| {
                let model = [self.vendor.as_deref(), self.model.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" ");
                (!model.is_empty()).then_some(model)
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SharedVolumeGrant {
    pub readonly: bool,
}

rpc_toolkit::reflect_ts!(SharedVolumeGrant);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum SharedVolumeAvailability {
    #[default]
    Unknown,
    Available,
    Unavailable,
}

rpc_toolkit::reflect_ts!(SharedVolumeAvailability);

#[derive(Debug, Clone, Serialize, Deserialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct SharedVolume {
    pub name: String,
    pub source: SharedVolumeSource,
    pub grants: BTreeMap<PackageId, SharedVolumeGrant>,
    #[serde(flatten)]
    pub inspection: SharedVolumeInspection,
}

impl SharedVolume {
    pub(crate) fn new(name: String, source: SharedVolumeSource) -> Self {
        Self {
            name,
            source,
            grants: BTreeMap::new(),
            inspection: SharedVolumeInspection::default(),
        }
    }
}

pub(crate) fn managed_local_paths(
    database: &crate::db::model::DatabaseModel,
) -> Result<Vec<PathBuf>, Error> {
    let mut paths = Vec::new();
    for (id, volume) in database.as_public().as_shared_volumes().as_entries()? {
        if matches!(volume.as_source().de()?, SharedVolumeSource::Local) {
            paths.push(local_path(crate::PACKAGE_DATA, &id));
        }
    }
    for (id, target) in database.as_public().as_backup_targets().as_entries()? {
        if matches!(target.as_source().de()?, SharedVolumeSource::Local) {
            paths.push(crate::backup::target::catalog::local_path(
                crate::PACKAGE_DATA,
                &id,
            ));
        }
    }
    Ok(paths)
}

/// Restores pending local deletions as ungranted registrations for retry.
pub(crate) fn recover_pending_deletions(
    volumes: &mut BTreeMap<SharedVolumeId, SharedVolume>,
    private: &Value,
) -> Result<(), Error> {
    if let Some(saved) = private.get("retainedSharedVolumes") {
        let pending: BTreeMap<SharedVolumeId, String> = from_value(saved.clone())?;
        for (id, name) in pending {
            volumes
                .entry(id)
                .or_insert_with(|| SharedVolume::new(name, SharedVolumeSource::Local));
        }
    }
    Ok(())
}

rpc_toolkit::reflect_ts!(SharedVolume);
rpc_toolkit::ts_export!(SharedVolume, namespaces = [""]);

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedVolumeCredentials {
    pub password: Option<String>,
}

impl SharedVolumeCredentials {
    pub(crate) fn update_password(&mut self, password: Option<String>) {
        if let Some(password) = password {
            self.password = (!password.is_empty()).then_some(password);
        }
    }
}

impl std::fmt::Debug for SharedVolumeCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SharedVolumeCredentials([redacted])")
    }
}

/// Managed shared-volume storage beneath the mounted package-data partition.
pub fn local_directory(package_data: impl AsRef<Path>) -> PathBuf {
    package_data.as_ref().join("shared-volumes")
}

/// The root must be the mounted package-data partition.
pub fn local_path(package_data: impl AsRef<Path>, id: &SharedVolumeId) -> PathBuf {
    local_directory(package_data).join(id.as_ref())
}

impl SharedVolume {
    pub fn authorize(
        &self,
        capable: bool,
        package: &PackageId,
        readonly: bool,
    ) -> Result<(), Error> {
        let grant = capable.then(|| self.grants.get(package)).flatten();
        if grant.is_some_and(|grant| readonly || !grant.readonly) {
            Ok(())
        } else {
            Err(access_denied())
        }
    }
}

pub(crate) fn access_denied() -> Error {
    Error::new(
        eyre!(t!("shared-volumes.access-denied")),
        ErrorKind::Authorization,
    )
}

pub fn handler<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("create", from_fn_async(create).no_cli())
        .subcommand("rename", from_fn_async(rename).no_cli())
        .subcommand("update-cifs", from_fn_async(update_cifs).no_cli())
        .subcommand("set-grant", from_fn_async(set_grant).no_cli())
        .subcommand("remove", from_fn_async(remove).no_cli())
        .subcommand("inspect", from_fn_async(inspect).no_cli())
}

#[derive(Deserialize, Serialize, VisitVariants)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CreateSharedVolumeSource {
    Local,
    Disk {
        logicalname: PathBuf,
        #[serde(default)]
        path: PathBuf,
    },
    Cifs(Cifs),
}

rpc_toolkit::reflect_ts!(CreateSharedVolumeSource);

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct CreateSharedVolumeParams {
    pub name: String,
    pub source: CreateSharedVolumeSource,
}

rpc_toolkit::reflect_ts!(CreateSharedVolumeParams);

pub(crate) fn validate_name(name: &str) -> Result<(), Error> {
    if name.trim().is_empty() {
        return Err(Error::new(
            eyre!(t!("shared-volumes.name-required")),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

pub async fn create(
    ctx: RpcContext,
    CreateSharedVolumeParams { name, source }: CreateSharedVolumeParams,
) -> Result<SharedVolumeId, Error> {
    validate_name(&name)?;
    let id = SharedVolumeId::new();
    let (source, credentials) = match source {
        CreateSharedVolumeSource::Local => {
            source::create_local_directory(&local_path(crate::PACKAGE_DATA, &id)).await?;
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
        matches!(source, SharedVolumeSource::Local).then(|| local_path(crate::PACKAGE_DATA, &id));
    let publication = ctx
        .db
        .mutate(|db| {
            if let Some(credentials) = &credentials {
                db.as_private_mut()
                    .as_shared_volume_credentials_mut()
                    .insert(&id, credentials)?;
            }
            db.as_public_mut()
                .as_shared_volumes_mut()
                .insert(&id, &SharedVolume::new(name, source))?;
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
pub struct RenameSharedVolumeParams {
    pub id: SharedVolumeId,
    pub name: String,
}

rpc_toolkit::reflect_ts!(RenameSharedVolumeParams);

pub async fn rename(
    ctx: RpcContext,
    RenameSharedVolumeParams { id, name }: RenameSharedVolumeParams,
) -> Result<(), Error> {
    validate_name(&name)?;
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_shared_volumes_mut()
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
pub struct UpdateCifsSharedVolumeParams {
    pub id: SharedVolumeId,
    pub hostname: String,
    pub path: PathBuf,
    pub username: String,
    /// Omitted keeps the stored password; empty clears it.
    #[serde(default)]
    pub password: Option<String>,
}

rpc_toolkit::reflect_ts!(UpdateCifsSharedVolumeParams);

pub async fn update_cifs(
    ctx: RpcContext,
    UpdateCifsSharedVolumeParams {
        id,
        hostname,
        path,
        username,
        password,
    }: UpdateCifsSharedVolumeParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            let volume = db
                .as_public()
                .as_shared_volumes()
                .as_idx(&id)
                .or_not_found(&id)?
                .de()?;
            if !matches!(volume.source, SharedVolumeSource::Cifs { .. }) {
                return Err(Error::new(
                    eyre!(t!("shared-volumes.not-cifs")),
                    ErrorKind::InvalidRequest,
                ));
            }
            let mut credentials = db
                .as_private()
                .as_shared_volume_credentials()
                .as_idx(&id)
                .map(|credentials| credentials.de())
                .transpose()?
                .unwrap_or_default();
            credentials.update_password(password);
            let (source, credentials) = source::prepare_cifs_source(Cifs {
                hostname,
                path,
                username,
                password: credentials.password,
            })?;
            db.as_private_mut()
                .as_shared_volume_credentials_mut()
                .insert(&id, &credentials)?;
            let volume = db
                .as_public_mut()
                .as_shared_volumes_mut()
                .as_idx_mut(&id)
                .or_not_found(&id)?;
            volume.as_source_mut().ser(&source)?;
            volume.mutate(|volume| {
                volume.inspection = SharedVolumeInspection::default();
                Ok(())
            })
        })
        .await
        .result
}

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SetSharedVolumeGrantParams {
    pub id: SharedVolumeId,
    pub package_id: PackageId,
    pub grant: Option<SharedVolumeGrant>,
}

rpc_toolkit::reflect_ts!(SetSharedVolumeGrantParams);

pub async fn set_grant(
    ctx: RpcContext,
    SetSharedVolumeGrantParams {
        id,
        package_id,
        grant,
    }: SetSharedVolumeGrantParams,
) -> Result<(), Error> {
    use crate::db::model::package::ManifestPreference;
    let service = ctx.services.get(&package_id).await;
    let mut mounts = if let Some(service) = service.as_ref() {
        Some(service.shared_volume_mounts().lock().await)
    } else {
        None
    };
    ctx.db
        .mutate(|db| {
            if grant.is_some()
                && !db
                    .as_public()
                    .as_package_data()
                    .as_idx(&package_id)
                    .or_not_found(&package_id)?
                    .as_state_info()
                    .as_manifest(ManifestPreference::Old)
                    .as_shared_volumes()
                    .de()?
            {
                return Err(access_denied());
            }
            let volume = db
                .as_public_mut()
                .as_shared_volumes_mut()
                .as_idx_mut(&id)
                .or_not_found(&id)?;
            if let Some(grant) = &grant {
                volume.as_grants_mut().insert(&package_id, grant)?;
            } else {
                volume.as_grants_mut().remove(&package_id)?;
            }
            Ok(())
        })
        .await
        .result?;
    if let Some(mounts) = &mut mounts {
        mounts.reconcile_grant(&id, grant.as_ref())?;
    }
    Ok(())
}

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SharedVolumeParams {
    pub id: SharedVolumeId,
}

rpc_toolkit::reflect_ts!(SharedVolumeParams);

/// Revokes local grants before detachment; failed deletion keeps the registration.
pub async fn remove(
    ctx: RpcContext,
    SharedVolumeParams { id }: SharedVolumeParams,
) -> Result<(), Error> {
    let (source, packages) = ctx.db.mutate(|db| prepare_removal(db, &id)).await.result?;
    let mut errors = ErrorCollection::new();
    for package_id in &packages {
        let service = ctx.services.get(package_id).await;
        if let Some(service) = service.as_ref() {
            errors.handle(
                service
                    .shared_volume_mounts()
                    .lock()
                    .await
                    .reconcile_grant(&id, None),
            );
        }
    }
    errors.into_result()?;
    if matches!(source, Some(SharedVolumeSource::Local)) {
        delete_local_volume(&ctx.db, &id, Path::new(crate::PACKAGE_DATA)).await?;
    }
    Ok(())
}

fn prepare_removal(
    db: &mut crate::db::model::DatabaseModel,
    id: &SharedVolumeId,
) -> Result<(Option<SharedVolumeSource>, BTreeSet<PackageId>), Error> {
    let source = db
        .as_public()
        .as_shared_volumes()
        .as_idx(id)
        .map(|volume| volume.as_source().de())
        .transpose()?;
    if matches!(source, Some(SharedVolumeSource::Local)) {
        db.as_public_mut()
            .as_shared_volumes_mut()
            .as_idx_mut(id)
            .unwrap()
            .as_grants_mut()
            .ser(&BTreeMap::new())?;
    } else {
        remove_definition(db, id)?;
    }
    Ok((source, db.as_public().as_package_data().keys()?))
}

async fn delete_local_volume(
    database: &TypedPatchDb<crate::db::model::Database>,
    id: &SharedVolumeId,
    package_data: &Path,
) -> Result<(), Error> {
    use crate::disk::mount::filesystem::ReadOnly;
    use crate::disk::mount::filesystem::bind::Bind;
    use crate::disk::mount::guard::TmpMountGuard;
    let path = local_path(package_data, id);
    let _unused = TmpMountGuard::reserve_unmounted(&Bind::new(&path), ReadOnly).await?;
    crate::util::btrfs::delete_volume_root(&path).await?;
    database.mutate(|db| remove_definition(db, id)).await.result
}

fn remove_definition(
    db: &mut crate::db::model::DatabaseModel,
    id: &SharedVolumeId,
) -> Result<(), Error> {
    db.as_public_mut().as_shared_volumes_mut().remove(id)?;
    db.as_private_mut()
        .as_shared_volume_credentials_mut()
        .remove(id)?;
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, HasModel, VisitFields)]
#[serde(default, rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct SharedVolumeInspection {
    pub availability: SharedVolumeAvailability,
    pub available: Option<u64>,
    pub error: Option<String>,
    pub checked_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl SharedVolumeInspection {
    pub(crate) fn observed(
        availability: SharedVolumeAvailability,
        available: Option<u64>,
        error: Option<String>,
    ) -> Self {
        Self {
            availability,
            available,
            error,
            checked_at: Some(chrono::Utc::now()),
        }
    }

    pub(crate) fn available(bytes: Option<u64>) -> Self {
        Self::observed(SharedVolumeAvailability::Available, bytes, None)
    }

    pub(crate) fn available_capacity(result: Result<u64, String>) -> Self {
        match result {
            Ok(bytes) => Self::available(Some(bytes)),
            Err(error) => Self::observed(SharedVolumeAvailability::Available, None, Some(error)),
        }
    }

    pub(crate) fn unavailable(error: String) -> Self {
        Self::observed(SharedVolumeAvailability::Unavailable, None, Some(error))
    }
}

rpc_toolkit::reflect_ts!(SharedVolumeInspection);

pub async fn inspect(
    ctx: RpcContext,
    SharedVolumeParams { id }: SharedVolumeParams,
) -> Result<SharedVolumeInspection, Error> {
    use crate::disk::mount::filesystem::ReadOnly;
    use crate::disk::mount::guard::GenericMountGuard;
    if let Ok(guard) = source::open_source(&ctx, &id, ReadOnly).await {
        guard.unmount().await?;
    }
    ctx.db
        .peek()
        .await
        .as_public()
        .as_shared_volumes()
        .as_idx(&id)
        .or_not_found(&id)?
        .as_inspection()
        .de()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_base32_guids_not_paths_or_names() {
        let id = SharedVolumeId::new();
        assert!(Guid::from(id.as_ref()).is_some());
        let fixed = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
        assert_eq!(fixed.parse::<SharedVolumeId>().unwrap().as_ref(), fixed);
        let encoded = serde_json::to_string(&id).unwrap();
        assert_eq!(
            serde_json::from_str::<SharedVolumeId>(&encoded).unwrap(),
            id
        );
        for value in [
            "../other",
            "name",
            "/etc",
            "",
            "00000000-0000-0000-0000-00000000000/",
            "7cb12a88-783d-4909-9923-d5eb47c7e01f",
            "abcdefghijklmnopqrstuvwxyz234567",
            "ABCDEFGHIJKLMNOPQRSTUVWXYZ234568",
            "ABCDEFGHIJKLMNOPQRSTUVWXYZ23456",
            "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567A",
            "ABCDEFGHIJKLMNOPQRSTUVWXYZ23456/",
        ] {
            assert!(value.parse::<SharedVolumeId>().is_err());
            assert!(serde_json::from_value::<SharedVolumeId>(serde_json::json!(value)).is_err());
        }
        assert_eq!(
            local_path("/data", &id).parent().unwrap(),
            Path::new("/data/shared-volumes")
        );
    }

    #[test]
    fn external_sources_without_a_path_keep_the_filesystem_root() {
        let source = serde_json::json!({"kind":"disk", "logicalname":"/dev/disk/by-uuid/photos"});
        let stored: SharedVolumeSource = serde_json::from_value(source.clone()).unwrap();
        let created: CreateSharedVolumeSource = serde_json::from_value(source).unwrap();
        assert!(
            matches!(stored, SharedVolumeSource::Disk { path, .. } if path.as_os_str().is_empty())
        );
        assert!(
            matches!(created, CreateSharedVolumeSource::Disk { path, .. } if path.as_os_str().is_empty())
        );
    }

    #[test]
    fn removal_revokes_local_grants_before_deletion_and_unregisters_external_sources() {
        use crate::db::model::DatabaseModel;
        let local = SharedVolumeId::new();
        let cifs = SharedVolumeId::new();
        let disk = SharedVolumeId::new();
        let mut db = DatabaseModel::from(to_value(&serde_json::json!({
            "public": { "sharedVolumes": {
                local.to_string(): {"name":"Photos", "source":{"kind":"local"}, "grants":{"consumer":{"readonly":false}}},
                cifs.to_string(): {"name":"Remote", "source":{"kind":"cifs","hostname":"nas","path":"/share"}, "grants":{}},
                disk.to_string(): {"name":"Drive", "source":{"kind":"disk","logicalname":"/dev/disk/by-uuid/aabbccdd-0000-4000-8000-0123456789ab"}, "grants":{}}
            }, "packageData": {} },
            "private": { "sharedVolumeCredentials": {
                cifs.to_string(): {"password":"secret"}
            } }
        })).unwrap());
        let (source, _) = prepare_removal(&mut db, &local).unwrap();
        assert!(matches!(source, Some(SharedVolumeSource::Local)));
        prepare_removal(&mut db, &cifs).unwrap();
        prepare_removal(&mut db, &disk).unwrap();
        let reloaded = DatabaseModel::from(to_value(&db).unwrap());
        let volume = reloaded
            .as_public()
            .as_shared_volumes()
            .as_idx(&local)
            .unwrap();
        assert_eq!(volume.as_name().de().unwrap(), "Photos");
        assert!(volume.as_grants().de().unwrap().is_empty());
        assert!(
            reloaded
                .as_private()
                .as_shared_volume_credentials()
                .de()
                .unwrap()
                .is_empty()
        );
        assert!(
            reloaded.as_public().as_shared_volumes().keys().unwrap() == BTreeSet::from([local])
        );
    }

    #[test]
    fn boot_maintenance_selects_only_managed_local_roots_from_both_catalogs() {
        let volume = SharedVolumeId::new();
        let target = crate::backup::target::BackupTargetId::new();
        let remote = SharedVolumeId::new();
        let external = crate::backup::target::BackupTargetId::new();
        let database = crate::db::model::DatabaseModel::from(imbl_value::json!({
            "public": {
                "sharedVolumes": {
                    volume.to_string(): {"source": {"kind": "local"}},
                    remote.to_string(): {"source": {"kind": "cifs", "hostname": "nas", "path": "/files"}}
                },
                "backupTargets": {
                    target.to_string(): {"source": {"kind": "local"}},
                    external.to_string(): {"source": {"kind": "disk", "logicalname": "/dev/disk/by-uuid/external"}}
                }
            }
        }));
        let paths: BTreeSet<_> = managed_local_paths(&database)
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(
            paths,
            BTreeSet::from([
                Path::new(crate::PACKAGE_DATA)
                    .join("shared-volumes")
                    .join(volume.as_ref()),
                Path::new(crate::PACKAGE_DATA)
                    .join("backup-targets")
                    .join(target.as_ref()),
            ])
        );
    }

    #[test]
    fn interrupted_deletions_recover_as_named_ungranted_local_entries() {
        let id = SharedVolumeId::new();
        let mut volumes = BTreeMap::new();
        recover_pending_deletions(
            &mut volumes,
            &imbl_value::json!({
                "retainedSharedVolumes": { id.to_string(): "Photos" }
            }),
        )
        .unwrap();
        assert_eq!(volumes.len(), 1);
        let volume = &volumes[&id];
        assert_eq!(volume.name, "Photos");
        assert!(matches!(volume.source, SharedVolumeSource::Local));
        assert!(volume.grants.is_empty());
        assert_eq!(
            volume.inspection.availability,
            SharedVolumeAvailability::Unknown
        );
    }

    async fn deletion_database(
        root: &Path,
        id: &SharedVolumeId,
    ) -> TypedPatchDb<crate::db::model::Database> {
        use patch_db::json_ptr::ROOT;
        let mut volume = SharedVolume::new("Photos".into(), SharedVolumeSource::Local);
        volume.grants.insert(
            "consumer".parse().unwrap(),
            SharedVolumeGrant { readonly: false },
        );
        let model = crate::db::model::DatabaseModel::from(imbl_value::json!({
            "public": { "sharedVolumes": {id.to_string(): to_value(&volume).unwrap()}, "packageData": {} },
            "private": { "sharedVolumeCredentials": {} }
        }));
        let database = TypedPatchDb::<crate::db::model::Database>::load_unchecked(
            patch_db::PatchDb::open(root.join("state.db"))
                .await
                .unwrap(),
        );
        database.put(&ROOT, &model).await.unwrap();
        database
    }

    #[tokio::test]
    async fn failed_local_deletion_preserves_registration_without_grants_for_retry() {
        let root = tempfile::tempdir().unwrap();
        let id = SharedVolumeId::new();
        let database = deletion_database(root.path(), &id).await;
        database
            .mutate(|db| prepare_removal(db, &id))
            .await
            .result
            .unwrap();
        let path = local_path(root.path(), &id);
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&path, b"not a directory").await.unwrap();
        assert!(
            delete_local_volume(&database, &id, root.path())
                .await
                .is_err()
        );
        let snapshot = database.peek().await;
        let stored = snapshot
            .as_public()
            .as_shared_volumes()
            .as_idx(&id)
            .unwrap()
            .de()
            .unwrap();
        assert_eq!(stored.name, "Photos");
        assert!(stored.grants.is_empty());
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"not a directory");
        tokio::fs::remove_file(&path).await.unwrap();
        tokio::fs::create_dir(&path).await.unwrap();
        tokio::fs::write(path.join("contents"), b"data")
            .await
            .unwrap();
        delete_local_volume(&database, &id, root.path())
            .await
            .unwrap();
        assert!(!path.exists());
        assert!(
            database
                .peek()
                .await
                .as_public()
                .as_shared_volumes()
                .as_idx(&id)
                .is_none()
        );
    }

    #[tokio::test]
    #[ignore = "requires BTRFS_TEST_VOLUMES_DIR and Btrfs subvolume permissions"]
    async fn local_deletion_rejects_active_sources_then_deletes_before_unregistering() {
        use crate::disk::mount::filesystem::ReadOnly;
        use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
        let fixture = crate::util::btrfs::tests::directory();
        let id = SharedVolumeId::new();
        let database = deletion_database(fixture.path(), &id).await;
        let path = local_path(fixture.path(), &id);
        source::create_local_directory(&path).await.unwrap();
        tokio::fs::write(path.join("sentinel"), b"managed contents")
            .await
            .unwrap();
        let filesystem = source::SourceFilesystem::Local(path.clone());
        let held = TmpMountGuard::mount(&filesystem, ReadOnly).await.unwrap();
        database
            .mutate(|db| prepare_removal(db, &id))
            .await
            .result
            .unwrap();
        assert!(
            delete_local_volume(&database, &id, fixture.path())
                .await
                .is_err()
        );
        let snapshot = database.peek().await;
        assert!(
            snapshot
                .as_public()
                .as_shared_volumes()
                .as_idx(&id)
                .unwrap()
                .as_grants()
                .de()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            tokio::fs::read(path.join("sentinel")).await.unwrap(),
            b"managed contents"
        );
        held.unmount().await.unwrap();
        delete_local_volume(&database, &id, fixture.path())
            .await
            .unwrap();
        assert!(!path.exists());
        assert!(
            database
                .peek()
                .await
                .as_public()
                .as_shared_volumes()
                .as_idx(&id)
                .is_none()
        );
    }

    #[test]
    fn grants_never_override_capability_or_escalate_writes() {
        let package = "consumer".parse().unwrap();
        let mut volume = SharedVolume {
            name: "Photos".into(),
            source: SharedVolumeSource::Local,
            grants: BTreeMap::new(),
            inspection: SharedVolumeInspection::default(),
        };
        assert!(volume.authorize(true, &package, true).is_err());
        volume
            .grants
            .insert(package.clone(), SharedVolumeGrant { readonly: true });
        assert!(volume.authorize(true, &package, true).is_ok());
        assert!(volume.authorize(true, &package, false).is_err());
        assert!(volume.authorize(false, &package, true).is_err());
        volume
            .grants
            .insert(package.clone(), SharedVolumeGrant { readonly: false });
        assert!(volume.authorize(true, &package, false).is_ok());
        assert!(volume.authorize(false, &package, false).is_err());
        assert!(
            volume
                .authorize(true, &"other".parse().unwrap(), true)
                .is_err()
        );
        volume.name = "Renamed".into();
        assert!(volume.authorize(true, &package, false).is_ok());
        volume.grants.remove(&package);
        assert!(volume.authorize(true, &package, true).is_err());
    }
}
