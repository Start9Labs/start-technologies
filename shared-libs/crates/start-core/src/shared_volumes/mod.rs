use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::PackageId;
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
            Err(Error::new(
                eyre!(t!("shared-volumes.access-denied")),
                ErrorKind::Authorization,
            ))
        }
    }
}

pub fn handler<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("create", from_fn_async(create).no_cli())
        .subcommand("rename", from_fn_async(rename).no_cli())
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
            tokio::fs::create_dir_all(local_path(crate::DATA_DIR, &id)).await?;
            (SharedVolumeSource::Local, None)
        }
        CreateSharedVolumeSource::Cifs(Cifs {
            hostname,
            path,
            username,
            password,
        }) => (
            SharedVolumeSource::Cifs {
                hostname,
                path: Path::new("/").join(path),
            },
            Some(SharedVolumeCredentials { username, password }),
        ),
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
                    availability: SharedVolumeAvailability::Unknown,
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
