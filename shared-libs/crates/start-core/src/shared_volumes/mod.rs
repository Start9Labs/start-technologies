use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::PackageId;

pub mod mounts;
pub mod source;
use crate::context::RpcContext;
use crate::disk::mount::filesystem::cifs::Cifs;
use crate::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, VisitFields)]
#[serde(try_from = "String", into = "String")]
#[visit(ts(type = "z.string().uuid()"))]
pub struct SharedVolumeId(String);

rpc_toolkit::reflect_ts!(SharedVolumeId);
rpc_toolkit::ts_export!(SharedVolumeId, namespaces = [""]);

impl SharedVolumeId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for SharedVolumeId {
    fn default() -> Self {
        Self::new()
    }
}

impl TryFrom<String> for SharedVolumeId {
    type Error = uuid::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(Self(uuid::Uuid::from_str(&value)?.to_string()))
    }
}

impl FromStr for SharedVolumeId {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned()).with_kind(ErrorKind::InvalidRequest)
    }
}

impl From<SharedVolumeId> for String {
    fn from(value: SharedVolumeId) -> Self {
        value.0
    }
}

impl std::fmt::Display for SharedVolumeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for SharedVolumeId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, VisitVariants)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SharedVolumeSource {
    Local,
    Cifs { hostname: String, path: PathBuf },
}

rpc_toolkit::reflect_ts!(SharedVolumeSource);

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
    #[serde(default)]
    pub availability: SharedVolumeAvailability,
}

rpc_toolkit::reflect_ts!(SharedVolume);
rpc_toolkit::ts_export!(SharedVolume, namespaces = [""]);

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedVolumeCredentials {
    pub username: String,
    pub password: Option<String>,
}

impl std::fmt::Debug for SharedVolumeCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SharedVolumeCredentials([redacted])")
    }
}

pub fn local_path(data_dir: impl AsRef<Path>, id: &SharedVolumeId) -> PathBuf {
    data_dir.as_ref().join("shared-volumes").join(id.as_ref())
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
        .subcommand("set-grant", from_fn_async(set_grant).no_cli())
        .subcommand("remove", from_fn_async(remove).no_cli())
        .subcommand("delete-data", from_fn_async(delete_data).no_cli())
        .subcommand("list-retained", from_fn_async(list_retained).no_cli())
        .subcommand("inspect", from_fn_async(inspect).no_cli())
}

#[derive(Deserialize, Serialize, VisitVariants)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CreateSharedVolumeSource {
    Local,
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

fn validate_name(name: &str) -> Result<(), Error> {
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
            source::create_local_directory(&local_path(crate::DATA_DIR, &id)).await?;
            (SharedVolumeSource::Local, None)
        }
        CreateSharedVolumeSource::Cifs(cifs) => {
            cifs.validate()?;
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
                },
                Some(SharedVolumeCredentials { username, password }),
            )
        }
    };
    ctx.db
        .mutate(|db| {
            if let Some(credentials) = &credentials {
                db.as_private_mut()
                    .as_shared_volume_credentials_mut()
                    .insert(&id, credentials)?;
            }
            db.as_public_mut().as_shared_volumes_mut().insert(
                &id,
                &SharedVolume {
                    name,
                    source,
                    grants: BTreeMap::new(),
                    availability: SharedVolumeAvailability::default(),
                },
            )?;
            Ok(())
        })
        .await
        .result?;
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

pub async fn remove(
    ctx: RpcContext,
    SharedVolumeParams { id }: SharedVolumeParams,
) -> Result<(), Error> {
    let packages = ctx
        .db
        .mutate(|db| remove_definition(db, &id))
        .await
        .result?;
    for package_id in &packages {
        let service = ctx.services.get(package_id).await;
        if let Some(service) = service.as_ref() {
            service
                .shared_volume_mounts()
                .lock()
                .await
                .reconcile_grant(&id, None)?;
        }
    }
    Ok(())
}

fn remove_definition(
    db: &mut crate::db::model::DatabaseModel,
    id: &SharedVolumeId,
) -> Result<BTreeSet<PackageId>, Error> {
    if let Some(volume) = db.as_public_mut().as_shared_volumes_mut().remove(id)? {
        let volume = volume.de()?;
        if matches!(volume.source, SharedVolumeSource::Local) {
            db.as_private_mut()
                .as_retained_shared_volumes_mut()
                .insert(id, &volume.name)?;
        }
    }
    db.as_private_mut()
        .as_shared_volume_credentials_mut()
        .remove(id)?;
    db.as_public().as_package_data().keys()
}

pub async fn list_retained(ctx: RpcContext) -> Result<BTreeMap<SharedVolumeId, String>, Error> {
    ctx.db
        .peek()
        .await
        .as_private()
        .as_retained_shared_volumes()
        .de()
}

pub async fn delete_data(
    ctx: RpcContext,
    SharedVolumeParams { id }: SharedVolumeParams,
) -> Result<(), Error> {
    if ctx
        .db
        .peek()
        .await
        .as_public()
        .as_shared_volumes()
        .contains_key(&id)?
    {
        return Err(Error::new(
            eyre!(t!("shared-volumes.remove-before-delete")),
            ErrorKind::InvalidRequest,
        ));
    }
    remove(ctx.clone(), SharedVolumeParams { id: id.clone() }).await?;
    match tokio::fs::remove_dir_all(local_path(crate::DATA_DIR, &id)).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    ctx.db
        .mutate(|db| {
            db.as_private_mut()
                .as_retained_shared_volumes_mut()
                .remove(&id)?;
            Ok(())
        })
        .await
        .result
}

#[derive(Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SharedVolumeInspection {
    pub availability: SharedVolumeAvailability,
    pub available: Option<u64>,
    pub error: Option<String>,
}

rpc_toolkit::reflect_ts!(SharedVolumeInspection);

pub async fn inspect(
    ctx: RpcContext,
    SharedVolumeParams { id }: SharedVolumeParams,
) -> Result<SharedVolumeInspection, Error> {
    use crate::disk::mount::filesystem::ReadOnly;
    use crate::disk::mount::guard::GenericMountGuard;
    match source::open_source(&ctx, &id, ReadOnly).await {
        Ok(guard) => {
            let available = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                crate::disk::util::get_available(guard.path()),
            )
            .await
            .with_kind(ErrorKind::Timeout)
            .and_then(|result| result);
            guard.unmount().await?;
            match available {
                Ok(available) => Ok(SharedVolumeInspection {
                    availability: SharedVolumeAvailability::Available,
                    available: Some(available),
                    error: None,
                }),
                Err(_) => {
                    source::record_availability(&ctx, &id, SharedVolumeAvailability::Unavailable)
                        .await?;
                    Ok(SharedVolumeInspection {
                        availability: SharedVolumeAvailability::Unavailable,
                        available: None,
                        error: Some(t!("shared-volumes.source-unavailable").to_string()),
                    })
                }
            }
        }
        Err(error) => Ok(SharedVolumeInspection {
            availability: SharedVolumeAvailability::Unavailable,
            available: None,
            error: Some(error.to_string()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_uuid_keys_not_paths_or_names() {
        let id = SharedVolumeId::new();
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
        ] {
            assert!(serde_json::from_value::<SharedVolumeId>(serde_json::json!(value)).is_err());
        }
        assert_eq!(
            local_path("/data", &id).parent().unwrap(),
            Path::new("/data/shared-volumes")
        );
    }

    #[test]
    fn removed_local_names_survive_reload_and_retries_but_cifs_credentials_do_not() {
        use crate::db::model::DatabaseModel;
        let local = SharedVolumeId::new();
        let cifs = SharedVolumeId::new();
        let mut db = DatabaseModel::from(to_value(&serde_json::json!({
            "public": { "sharedVolumes": {
                local.to_string(): {"name":"Photos", "source":{"kind":"local"}, "grants":{}},
                cifs.to_string(): {"name":"Remote", "source":{"kind":"cifs","hostname":"nas","path":"/share"}, "grants":{}}
            }, "packageData": {} },
            "private": { "sharedVolumeCredentials": {
                cifs.to_string(): {"username":"private", "password":"secret"}
            }, "retainedSharedVolumes": {} }
        })).unwrap());
        remove_definition(&mut db, &local).unwrap();
        remove_definition(&mut db, &local).unwrap();
        remove_definition(&mut db, &cifs).unwrap();
        let reloaded = DatabaseModel::from(to_value(&db).unwrap());
        assert_eq!(
            reloaded
                .as_private()
                .as_retained_shared_volumes()
                .de()
                .unwrap(),
            BTreeMap::from([(local, "Photos".to_owned())])
        );
        assert!(
            reloaded
                .as_private()
                .as_shared_volume_credentials()
                .de()
                .unwrap()
                .is_empty()
        );
        assert!(
            reloaded
                .as_public()
                .as_shared_volumes()
                .de()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn grants_never_override_capability_or_escalate_writes() {
        let package = "consumer".parse().unwrap();
        let mut volume = SharedVolume {
            name: "Photos".into(),
            source: SharedVolumeSource::Local,
            grants: BTreeMap::new(),
            availability: SharedVolumeAvailability::Unknown,
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
