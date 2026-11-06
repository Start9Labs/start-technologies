use std::path::{Path, PathBuf};

use imbl_value::InternedString;
use openssl::pkey::{PKey, Private};
use openssl::x509::X509;
use patch_db::Value;
use serde::{Deserialize, Serialize};
use ssh_key::private::Ed25519Keypair;
use tokio::io::AsyncWriteExt;

use crate::account::AccountInfo;
use crate::context::RpcContext;
use crate::hostname::{ServerHostname, generate_hostname, generate_id};
use crate::prelude::*;
use crate::util::io::{AtomicFile, delete_dir, dir_copy, dir_size, rename};
use crate::util::serde::{Base32, Base64, IoFormat, Pem};

pub struct OsBackup {
    pub account: AccountInfo,
    pub ui: Value,
}

pub(crate) async fn backup_system(ctx: &RpcContext, destination: &Path) -> Result<(), Error> {
    let mut os_backup_file =
        AtomicFile::new(destination.join("os-backup.json"), None::<PathBuf>).await?;
    os_backup_file
        .write_all(&system_metadata(ctx).await?)
        .await?;
    os_backup_file.save().await?;

    let old = destination.join("luks.old");
    delete_dir(&old).await?;
    let backup = destination.join("luks");
    if tokio::fs::metadata(&backup).await.is_ok() {
        rename(&backup, &old).await?;
    }
    let source = Path::new("/media/startos/config/luks");
    if tokio::fs::metadata(source).await.is_ok() {
        dir_copy(source, &backup, None).await?;
    }
    Ok(())
}

pub(crate) async fn system_logical_size(ctx: &RpcContext) -> Result<u64, Error> {
    let metadata_bytes = system_metadata(ctx).await?.len() as u64;
    let luks = Path::new("/media/startos/config/luks");
    let luks_bytes = if tokio::fs::metadata(luks).await.is_ok() {
        dir_size(luks, None).await?
    } else {
        0
    };
    // The completed backup keeps the previous LUKS copy beside the current one.
    Ok(metadata_bytes.saturating_add(luks_bytes.saturating_mul(2)))
}

async fn system_metadata(ctx: &RpcContext) -> Result<Vec<u8>, Error> {
    let ui = ctx.db.peek().await.into_public().into_ui().de()?;
    IoFormat::Json.to_vec(&OsBackup {
        account: ctx.account.peek(Clone::clone),
        ui,
    })
}
impl<'de> Deserialize<'de> for OsBackup {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let tagged = OsBackupSerDe::deserialize(deserializer)?;
        Ok(match tagged.version {
            0 => patch_db::value::from_value::<OsBackupV0>(tagged.rest)
                .map_err(serde::de::Error::custom)?
                .project()
                .map_err(serde::de::Error::custom)?,
            1 => patch_db::value::from_value::<OsBackupV1>(tagged.rest)
                .map_err(serde::de::Error::custom)?
                .project()
                .map_err(serde::de::Error::custom)?,
            2 => patch_db::value::from_value::<OsBackupV2>(tagged.rest)
                .map_err(serde::de::Error::custom)?
                .project()
                .map_err(serde::de::Error::custom)?,
            v => {
                return Err(serde::de::Error::custom(&format!(
                    "Unknown backup version {v}"
                )));
            }
        })
    }
}
impl Serialize for OsBackup {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        OsBackupSerDe {
            version: 2,
            rest: patch_db::value::to_value(&OsBackupV2::unproject(self))
                .map_err(serde::ser::Error::custom)?,
        }
        .serialize(serializer)
    }
}

#[derive(Deserialize, Serialize)]
struct OsBackupSerDe {
    #[serde(default)]
    version: usize,
    #[serde(flatten)]
    rest: Value,
}

/// V0
#[derive(Deserialize)]
#[serde(rename = "kebab-case")]
struct OsBackupV0 {
    #[allow(dead_code)] // parsed for format-validation; StartOS no longer restores the tor key
    tor_key: Base32<[u8; 64]>, // Base32 Encoded Ed25519 Expanded Secret Key
    root_ca_key: Pem<PKey<Private>>, // PEM Encoded OpenSSL Key
    root_ca_cert: Pem<X509>,         // PEM Encoded OpenSSL X509 Certificate
    ui: Value,                       // JSON Value
}
impl OsBackupV0 {
    fn project(self) -> Result<OsBackup, Error> {
        Ok(OsBackup {
            account: AccountInfo {
                server_id: generate_id(),
                hostname: generate_hostname(),
                password: Default::default(),
                root_ca_key: self.root_ca_key.0,
                root_ca_cert: self.root_ca_cert.0,
                ssh_key: ssh_key::PrivateKey::random(
                    &mut crate::util::crypto::os_rng(),
                    ssh_key::Algorithm::Ed25519,
                )?,
                developer_key: ed25519_dalek::SigningKey::generate(
                    &mut crate::util::crypto::os_rng(),
                ),
            },
            ui: self.ui,
        })
    }
}

/// V1
#[derive(Deserialize, Serialize)]
#[serde(rename = "kebab-case")]
struct OsBackupV1 {
    server_id: String,               // uuidv4
    hostname: InternedString,        // embassy-<adjective>-<noun>
    net_key: Base64<[u8; 32]>,       // Ed25519 Secret Key
    root_ca_key: Pem<PKey<Private>>, // PEM Encoded OpenSSL Key
    root_ca_cert: Pem<X509>,         // PEM Encoded OpenSSL X509 Certificate
    ui: Value,                       // JSON Value
}
impl OsBackupV1 {
    fn project(self) -> Result<OsBackup, Error> {
        Ok(OsBackup {
            account: AccountInfo {
                server_id: self.server_id,
                hostname: ServerHostname::new(self.hostname)?,
                password: Default::default(),
                root_ca_key: self.root_ca_key.0,
                root_ca_cert: self.root_ca_cert.0,
                ssh_key: ssh_key::PrivateKey::from(Ed25519Keypair::from_seed(&self.net_key.0)),
                developer_key: ed25519_dalek::SigningKey::from_bytes(&self.net_key),
            },
            ui: self.ui,
        })
    }
}

/// V2
#[derive(Deserialize, Serialize)]
#[serde(rename = "kebab-case")]

struct OsBackupV2 {
    server_id: String, // uuidv4
    hostname: InternedString,
    root_ca_key: Pem<PKey<Private>>,   // PEM Encoded OpenSSL Key
    root_ca_cert: Pem<X509>,           // PEM Encoded OpenSSL X509 Certificate
    ssh_key: Pem<ssh_key::PrivateKey>, // PEM Encoded OpenSSH Key
    compat_s9pk_key: Pem<ed25519_dalek::SigningKey>, // PEM Encoded ED25519 Key
    ui: Value,                         // JSON Value
}
impl OsBackupV2 {
    fn project(self) -> Result<OsBackup, Error> {
        Ok(OsBackup {
            account: AccountInfo {
                server_id: self.server_id,
                hostname: ServerHostname::new(self.hostname)?,
                password: Default::default(),
                root_ca_key: self.root_ca_key.0,
                root_ca_cert: self.root_ca_cert.0,
                ssh_key: self.ssh_key.0,
                developer_key: self.compat_s9pk_key.0,
            },
            ui: self.ui,
        })
    }
    fn unproject(backup: &OsBackup) -> Self {
        Self {
            server_id: backup.account.server_id.clone(),
            hostname: (*backup.account.hostname).clone(),
            root_ca_key: Pem(backup.account.root_ca_key.clone()),
            root_ca_cert: Pem(backup.account.root_ca_cert.clone()),
            ssh_key: Pem(backup.account.ssh_key.clone()),
            compat_s9pk_key: Pem(backup.account.developer_key.clone()),
            ui: backup.ui.clone(),
        }
    }
}
