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
pub(crate) const HOST_KEY_DIRECTORY: &str = "/etc/ssh";

pub(crate) fn is_installer_host_key(name: &std::ffi::OsStr) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let name = name.as_bytes();
    name.starts_with(b"ssh_host_") && name.ends_with(b"_key")
}

/// Includes and NSS expansion use the caller's actual prospective namespace.
/// The caller authenticates stable executable, library and runtime configuration inputs.
/// OpenSSL initializes before configuration output and can load configured providers.
pub(crate) async fn host_key_files(configuration: &Path) -> Result<Vec<std::path::PathBuf>, Error> {
    let output = Command::new("/usr/sbin/sshd")
        .args(["-G", "-f"])
        .arg(configuration)
        .current_dir("/")
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("LC_ALL", "C")
        .kill_on_drop(true)
        .invoke(ErrorKind::Filesystem)
        .await?;
    Ok(parse_host_key_files(&output))
}

fn parse_host_key_files(output: &[u8]) -> Vec<std::path::PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    output
        .split(|byte| *byte == b'\n')
        .filter_map(|line| line.strip_prefix(b"hostkey "))
        .map(|path| std::ffi::OsString::from_vec(path.to_vec()).into())
        .collect()
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SshKeys(BTreeMap<InternedString, WithTimeData<SshPubKey>>);
impl SshKeys {
    pub fn new() -> Self {
        Self(BTreeMap::new())
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

// #[command(subcommands(add, delete, list,))]
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
    let mut key = WithTimeData::new(key);
    let fingerprint = InternedString::intern(key.0.fingerprint_md5());
    let (keys, res) = ctx
        .db
        .mutate(move |m| {
            m.as_private_mut()
                .as_ssh_pubkeys_mut()
                .insert(&fingerprint, &key)?;

            Ok((
                m.as_private().as_ssh_pubkeys().de()?,
                SshKeyResponse {
                    alg: key.0.keytype().to_owned(),
                    fingerprint,
                    hostname: key.0.comment.take().unwrap_or_default(),
                    created_at: key.created_at.to_rfc3339(),
                },
            ))
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
            let keys_ref = m.as_private_mut().as_ssh_pubkeys_mut();
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
        .into_private()
        .into_ssh_pubkeys()
        .into_entries()?
        .into_iter()
        .map(|(fingerprint, key)| {
            let mut key = key.de()?;
            Ok(SshKeyResponse {
                alg: key.0.keytype().to_owned(),
                fingerprint,
                hostname: key.0.comment.take().unwrap_or_default(),
                created_at: key.created_at.to_rfc3339(),
            })
        })
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
    let encoded = zeroize::Zeroizing::new(
        crate::util::serde::PemEncoding::to_pem::<serde_json::Error>(&privkey.0)
            .with_kind(ErrorKind::OpenSsh)?,
    );
    let bytes = zeroize::Zeroizing::new([encoded.as_bytes(), b"\n"].concat());
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(&privkey_path)
        .await
        .with_ctx(|_| {
            (
                ErrorKind::Filesystem,
                lazy_format!("create {privkey_path:?}"),
            )
        })?;
    crate::util::io::write_secret_file(&file, bytes).await?;
    file.sync_all().await?;
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

    let mut f = create_file(ssh_dir.join("authorized_keys")).await?;
    for key in pubkeys.0.values() {
        f.write_all(key.0.to_key_format().as_bytes()).await?;
        f.write_all(b"\n").await?;
    }

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

    #[tokio::test]
    #[ignore = "requires native OpenSSH/OpenSSL and start9 identity"]
    async fn real_host_key_query_excludes_inherited_runtime_overrides() {
        use std::os::unix::ffi::OsStringExt;
        use std::path::PathBuf;

        const CHILD_CONFIG: &str = "STARTOS_TEST_SSH_QUERY_CONFIG";
        if let Some(configuration) = std::env::var_os(CHILD_CONFIG) {
            assert_eq!(
                host_key_files(Path::new(&configuration)).await.unwrap(),
                vec![
                    PathBuf::from("/home/start9/native-key"),
                    PathBuf::from("/private/with space"),
                    std::ffi::OsString::from_vec(b"/private/\xff".to_vec()).into(),
                ]
            );
            return;
        }

        let root = tempfile::tempdir().unwrap();
        let configuration = root.path().join("sshd_config");
        let included = root.path().join("included.conf");
        tokio::fs::write(&included, b"HostKey ~start9/native-key\n")
            .await
            .unwrap();
        let mut settings = format!(
            "Include {}\nHostKey \"/private/with space\"\n",
            included.display()
        )
        .into_bytes();
        settings.extend_from_slice(b"HostKey /private/\xff\n");
        tokio::fs::write(&configuration, settings).await.unwrap();
        let overrides = root.path().join("openssl.cnf");
        tokio::fs::write(&overrides, "config_diagnostics = 1\nopenssl_conf = init\n[init]\nalg_section = algorithms\n[algorithms]\ndefault_properties = fips=yes\n").await.unwrap();
        let control = Command::new("/usr/sbin/sshd")
            .args(["-G", "-f"])
            .arg(&configuration)
            .env("OPENSSL_CONF", &overrides)
            .output()
            .await
            .unwrap();
        assert!(
            !control.status.success(),
            "native OpenSSL override must distinguish the boundary"
        );
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "ssh::tests::real_host_key_query_excludes_inherited_runtime_overrides",
                "--nocapture",
            ])
            .env(CHILD_CONFIG, &configuration)
            .env("OPENSSL_CONF", &overrides)
            .env("OPENSSL_MODULES", root.path().join("missing-modules"))
            .env("PATH", root.path().join("missing-commands"))
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires native start9/startos system identities"]
    async fn real_private_key_sync_replaces_longer_contents_and_preserves_native_attributes() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("id_ed25519");
        tokio::fs::write(&path, vec![b'x'; 16384]).await.unwrap();
        tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .await
            .unwrap();
        xattr::set(&path, "user.startos-fixture", b"native-attributes").unwrap();
        let inode = tokio::fs::metadata(&path).await.unwrap().ino();
        let key = Pem(ssh_key::PrivateKey::random(
            &mut crate::util::crypto::os_rng(),
            ssh_key::Algorithm::Ed25519,
        )
        .unwrap());
        sync_keys(
            &ServerHostname::new("native-test".into()).unwrap(),
            &key,
            &SshKeys(BTreeMap::new()),
            root.path(),
        )
        .await
        .unwrap();
        let encoded = crate::util::serde::PemEncoding::to_pem::<serde_json::Error>(&key.0).unwrap();
        assert_eq!(
            tokio::fs::read(&path).await.unwrap(),
            [encoded.as_bytes(), b"\n"].concat()
        );
        assert_eq!(
            tokio::fs::metadata(&path)
                .await
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(tokio::fs::metadata(&path).await.unwrap().ino(), inode);
        assert_eq!(
            xattr::get(&path, "user.startos-fixture")
                .unwrap()
                .as_deref(),
            Some(b"native-attributes".as_slice())
        );
    }

    #[test]
    fn host_key_output_retains_spaces_and_non_utf8_paths() {
        use std::os::unix::ffi::OsStringExt;
        assert_eq!(parse_host_key_files(b"port 22\nhostkey /keys/with space\nhostcertificate /public/cert\nhostkey /keys/\xff\n"),
            vec![std::path::PathBuf::from("/keys/with space"), std::ffi::OsString::from_vec(b"/keys/\xff".to_vec()).into()]);
    }
}
