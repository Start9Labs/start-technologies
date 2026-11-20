use std::collections::BTreeMap;
use std::path::Path;

use clap::Parser;
use clap::builder::ValueParserFactory;
use imbl_value::InternedString;
use rpc_toolkit::{Context, Empty, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tokio::fs::OpenOptions;
use tokio::process::Command;
use tracing::instrument;

use crate::context::{CliContext, RpcContext};
use crate::hostname::ServerHostname;
use crate::prelude::*;
use crate::util::io::create_file;
use crate::util::serde::{HandlerExtSerde, Pem, WithIoFormat, display_serializable};
use crate::util::{FromStrParser, Invoke};

pub const SSH_DIR: &str = "/home/start9/.ssh";

/// Authorized-key intent shared by SSH mutations and subscribed clients.
#[derive(Clone, Debug, Default, Deserialize, Serialize, VisitFields)]
pub struct SshKeys(BTreeMap<InternedString, WithTimeData<SshPubKey>>);

rpc_toolkit::reflect_ts!(SshKeys);
impl SshKeys {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }
}

impl Model<SshKeys> {
    /// Reauthorizing a fingerprint replaces its key without changing either timestamp.
    fn authorize(&mut self, key: SshPubKey) -> Result<SshKeyResponse, Error> {
        let fingerprint = InternedString::intern(key.0.fingerprint_md5());
        let key = match self.as_idx(&fingerprint) {
            Some(existing) => WithTimeData {
                value: key,
                ..existing.de()?
            },
            None => WithTimeData::new(key),
        };
        self.insert(&fingerprint, &key)?;
        Ok(SshKeyResponse::new(fingerprint, &key))
    }
}

impl From<BTreeMap<InternedString, WithTimeData<SshPubKey>>> for SshKeys {
    fn from(map: BTreeMap<InternedString, WithTimeData<SshPubKey>>) -> Self {
        Self(map)
    }
}
impl Map for SshKeys {
    type Key = InternedString;
    type Value = WithTimeData<SshPubKey>;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Ok(key)
    }
    fn key_string(key: &Self::Key) -> Result<InternedString, Error> {
        Ok(key.clone())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields)]
#[visit(opaque, ts(type = "z.string()"))]
pub struct SshPubKey(
    #[serde(serialize_with = "crate::util::serde::serialize_display")]
    #[serde(deserialize_with = "crate::util::serde::deserialize_from_str")]
    pub openssh_keys::PublicKey,
);

rpc_toolkit::reflect_ts!(SshPubKey);
impl ValueParserFactory for SshPubKey {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        FromStrParser::new()
    }
}

#[derive(serde::Serialize, serde::Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyResponse {
    pub alg: String,
    pub fingerprint: InternedString,
    pub hostname: String,
    pub created_at: String,
}

rpc_toolkit::reflect_ts!(SshKeyResponse);
rpc_toolkit::ts_export!(SshKeyResponse, namespaces = [""]);
impl SshKeyResponse {
    fn new(fingerprint: InternedString, key: &WithTimeData<SshPubKey>) -> Self {
        Self {
            alg: key.0.keytype().to_owned(),
            fingerprint,
            hostname: key.0.comment.clone().unwrap_or_default(),
            created_at: key.created_at.to_rfc3339(),
        }
    }
}
impl std::fmt::Display for SshKeyResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{} {} {} {}",
            self.created_at, self.alg, self.fingerprint, self.hostname
        )
    }
}

impl std::str::FromStr for SshPubKey {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse()
            .map(|pk| SshPubKey(pk))
            .with_kind(ErrorKind::ParseSshKey)
    }
}

pub fn ssh<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "add",
            from_fn_async(add)
                .no_display()
                .with_about("about.add-ssh-key")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove)
                .no_display()
                .with_about("about.remove-ssh-key")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "list",
            from_fn_async(list)
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| {
                    display_all_ssh_keys(handle.params, result)
                })
                .with_about("about.list-ssh-keys")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct SshAddParams {
    #[arg(help = "help.arg.ssh-public-key")]
    key: SshPubKey,
}

rpc_toolkit::reflect_ts!(SshAddParams);
rpc_toolkit::ts_export!(SshAddParams, namespaces = [""]);

#[instrument(skip_all)]
pub async fn add(
    ctx: RpcContext,
    SshAddParams { key }: SshAddParams,
) -> Result<SshKeyResponse, Error> {
    let (keys, res) = ctx
        .db
        .mutate(move |m| {
            let keys = m.as_public_mut().as_ssh_pubkeys_mut();
            let res = keys.authorize(key)?;
            Ok((keys.de()?, res))
        })
        .await
        .result?;
    sync_pubkeys(&keys, SSH_DIR).await?;
    Ok(res)
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct SshDeleteParams {
    #[arg(help = "help.arg.ssh-fingerprint")]
    fingerprint: InternedString,
}

rpc_toolkit::reflect_ts!(SshDeleteParams);
rpc_toolkit::ts_export!(SshDeleteParams, namespaces = [""]);

#[instrument(skip_all)]
pub async fn remove(
    ctx: RpcContext,
    SshDeleteParams { fingerprint }: SshDeleteParams,
) -> Result<(), Error> {
    let keys = ctx
        .db
        .mutate(|m| {
            let keys_ref = m.as_public_mut().as_ssh_pubkeys_mut();
            if keys_ref.remove(&fingerprint)?.is_some() {
                keys_ref.de()
            } else {
                Err(Error::new(
                    eyre!("{}", t!("ssh.key-not-found")),
                    ErrorKind::NotFound,
                ))
            }
        })
        .await
        .result?;
    sync_pubkeys(&keys, SSH_DIR).await
}

fn display_all_ssh_keys(
    params: WithIoFormat<Empty>,
    result: Vec<SshKeyResponse>,
) -> Result<(), Error> {
    use prettytable::*;

    if let Some(format) = params.format {
        return display_serializable(format, result);
    }

    let mut table = Table::new();
    table.add_row(row![bc =>
        "CREATED AT",
        "ALGORITHM",
        "FINGERPRINT",
        "HOSTNAME",
    ]);
    for key in result {
        let row = row![
            &format!("{}", key.created_at),
            &key.alg,
            &key.fingerprint,
            &key.hostname,
        ];
        table.add_row(row);
    }
    print_table(&table)?;

    Ok(())
}

#[instrument(skip_all)]
pub async fn list(ctx: RpcContext) -> Result<Vec<SshKeyResponse>, Error> {
    ctx.db
        .peek()
        .await
        .into_public()
        .into_ssh_pubkeys()
        .into_entries()?
        .into_iter()
        .map(|(fingerprint, key)| Ok(SshKeyResponse::new(fingerprint, &key.de()?)))
        .collect()
}

#[instrument(skip_all)]
pub async fn sync_keys<P: AsRef<Path>>(
    hostname: &ServerHostname,
    privkey: &Pem<ssh_key::PrivateKey>,
    pubkeys: &SshKeys,
    ssh_dir: P,
) -> Result<(), Error> {
    use tokio::io::AsyncWriteExt;

    let ssh_dir = ssh_dir.as_ref();
    if tokio::fs::metadata(ssh_dir).await.is_err() {
        tokio::fs::create_dir_all(ssh_dir).await?;
    }

    let id_alg = if privkey.0.algorithm().is_ed25519() {
        "id_ed25519"
    } else if privkey.0.algorithm().is_ecdsa() {
        "id_ecdsa"
    } else if privkey.0.algorithm().is_rsa() {
        "id_rsa"
    } else {
        "id_unknown"
    };

    let privkey_path = ssh_dir.join(id_alg);
    let mut f = OpenOptions::new()
        .create(true)
        .write(true)
        .mode(0o600)
        .open(&privkey_path)
        .await
        .with_ctx(|_| {
            (
                ErrorKind::Filesystem,
                lazy_format!("create {privkey_path:?}"),
            )
        })?;
    f.write_all(privkey.to_string().as_bytes()).await?;
    f.write_all(b"\n").await?;
    f.sync_all().await?;
    let mut f = create_file(ssh_dir.join(id_alg).with_extension("pub")).await?;
    f.write_all(
        (privkey
            .0
            .public_key()
            .to_openssh()
            .with_kind(ErrorKind::OpenSsh)?
            + " start9@"
            + hostname.as_ref())
        .as_bytes(),
    )
    .await?;
    f.write_all(b"\n").await?;
    f.sync_all().await?;

    sync_pubkeys(pubkeys, ssh_dir).await?;

    Command::new("chown")
        .arg("-R")
        .arg("start9:startos")
        .arg(ssh_dir)
        .invoke(ErrorKind::Filesystem)
        .await?;

    Ok(())
}

#[instrument(skip_all)]
pub async fn sync_pubkeys<P: AsRef<Path>>(pubkeys: &SshKeys, ssh_dir: P) -> Result<(), Error> {
    use tokio::io::AsyncWriteExt;

    let ssh_dir = ssh_dir.as_ref();
    if tokio::fs::metadata(ssh_dir).await.is_err() {
        tokio::fs::create_dir_all(ssh_dir).await?;
    }

    let mut f = create_file(ssh_dir.join("authorized_keys")).await?;
    for key in pubkeys.0.values() {
        f.write_all(key.0.to_key_format().as_bytes()).await?;
        f.write_all(b"\n").await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMcFf+jmv4w+tRbA1aXDvovqViM0NVRu86Hk5kcQItfI laptop";

    #[test]
    fn reauthorization_updates_the_comment_without_changing_timestamps() {
        let key: SshPubKey = KEY.parse().unwrap();
        let fingerprint = InternedString::intern(key.0.fingerprint_md5());
        let original = WithTimeData {
            value: key,
            created_at: "2020-01-01T00:00:00Z".parse().unwrap(),
            updated_at: "2021-01-01T00:00:00Z".parse().unwrap(),
        };
        let mut keys = Model::new(&SshKeys::from(BTreeMap::from([
            (fingerprint.clone(), original.clone()),
        ])))
        .unwrap();
        let response = keys
            .authorize(KEY.replace("laptop", "desktop").parse().unwrap())
            .unwrap();
        let saved = keys.as_idx(&fingerprint).unwrap().de().unwrap();
        assert_eq!(keys.de().unwrap().0.len(), 1);
        assert_eq!(response.fingerprint, fingerprint);
        assert_eq!(response.hostname, "desktop");
        assert_eq!(response.created_at, original.created_at.to_rfc3339());
        assert_eq!(saved.created_at, original.created_at);
        assert_eq!(saved.updated_at, original.updated_at);
        assert_eq!(saved.0.comment.as_deref(), Some("desktop"));
    }

    #[tokio::test]
    async fn reauthorizing_after_a_failed_sync_can_repair_authorized_keys() {
        let mut keys = Model::new(&SshKeys::new()).unwrap();
        let first = keys.authorize(KEY.parse().unwrap()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let ssh_dir = dir.path().join("ssh");
        tokio::fs::write(&ssh_dir, "not a directory").await.unwrap();
        assert!(sync_pubkeys(&keys.de().unwrap(), &ssh_dir).await.is_err());
        tokio::fs::remove_file(&ssh_dir).await.unwrap();
        let retry = keys.authorize(KEY.parse().unwrap()).unwrap();
        sync_pubkeys(&keys.de().unwrap(), &ssh_dir).await.unwrap();
        assert_eq!(first.created_at, retry.created_at);
        assert_eq!(keys.de().unwrap().0.len(), 1);
        assert_eq!(
            tokio::fs::read_to_string(ssh_dir.join("authorized_keys")).await.unwrap(),
            format!("{KEY}\n")
        );
    }
}
