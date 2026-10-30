use std::collections::BTreeMap;

use clap::Parser;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::context::{CliContext, RpcContext};
use crate::prelude::*;
use crate::registry::signer::AcceptSigners;
use crate::sign::AnyVerifyingKey;
pub use crate::trusted_keys::initial_signers;
use crate::trusted_keys::{legacy_add, legacy_remove, legacy_update, os_signers};
use crate::util::serde::HandlerExtSerde;

pub type TrustedOsUpdateSigners = BTreeMap<InternedString, TrustedOsUpdateSigner>;

#[derive(Clone, Debug, Deserialize, Serialize, HasModel, VisitFields, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct TrustedOsUpdateSigner {
    pub label: String,
}

rpc_toolkit::reflect_ts!(TrustedOsUpdateSigner);
rpc_toolkit::ts_export!(TrustedOsUpdateSigner, namespaces = [""]);

pub fn initial_signer_policy() -> AcceptSigners {
    AcceptSigners::Any(
        initial_signers()
            .into_keys()
            .map(|key| AcceptSigners::Signer(key.parse().unwrap()))
            .collect(),
    )
}

pub fn api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "add",
            from_fn_async(add)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.add-signer")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "get",
            from_fn_async(get)
                .with_display_serializable()
                .with_about("about.get-os-update-signer")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "list",
            from_fn_async(list)
                .with_display_serializable()
                .with_about("about.list-signers")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "update",
            from_fn_async(update)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.edit-signer")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.remove-signer")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct OsUpdateSignerKeyParams {
    #[arg(allow_hyphen_values = true, help = "help.arg.signer-key")]
    key: AnyVerifyingKey,
}

rpc_toolkit::reflect_ts!(OsUpdateSignerKeyParams);
rpc_toolkit::ts_export!(OsUpdateSignerKeyParams, namespaces = [""]);

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct SetOsUpdateSignerParams {
    #[arg(allow_hyphen_values = true, help = "help.arg.signer-key")]
    key: AnyVerifyingKey,
    #[arg(long, help = "help.arg.signer-name")]
    label: String,
}

rpc_toolkit::reflect_ts!(SetOsUpdateSignerParams);
rpc_toolkit::ts_export!(SetOsUpdateSignerParams, namespaces = [""]);

fn signer_not_found() -> Error {
    Error::new(
        eyre!("{}", t!("update.signers.not-found")),
        ErrorKind::NotFound,
    )
}

fn get_signer(
    signers: &TrustedOsUpdateSigners,
    key: &AnyVerifyingKey,
) -> Result<TrustedOsUpdateSigner, Error> {
    signers
        .get(&key.interned_der64())
        .cloned()
        .ok_or_else(signer_not_found)
}

pub async fn add(ctx: RpcContext, params: SetOsUpdateSignerParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_trusted_keys_mut()
                .mutate(|keys| legacy_add(keys, params.key, params.label))
        })
        .await
        .result
}

pub async fn get(
    ctx: RpcContext,
    params: OsUpdateSignerKeyParams,
) -> Result<TrustedOsUpdateSigner, Error> {
    get_signer(&list(ctx).await?, &params.key)
}

pub async fn list(ctx: RpcContext) -> Result<TrustedOsUpdateSigners, Error> {
    Ok(os_signers(
        &ctx.db
            .peek()
            .await
            .into_public()
            .into_server_info()
            .into_trusted_keys()
            .de()?,
    ))
}

pub async fn update(ctx: RpcContext, params: SetOsUpdateSignerParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_trusted_keys_mut()
                .mutate(|keys| legacy_update(keys, &params.key, params.label))
        })
        .await
        .result
}

pub async fn remove(ctx: RpcContext, params: OsUpdateSignerKeyParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_trusted_keys_mut()
                .mutate(|keys| legacy_remove(keys, &params.key))
        })
        .await
        .result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_crud_uses_shared_trusted_keys() {
        let canonical = initial_signers().into_keys().next().unwrap();
        let key: AnyVerifyingKey = canonical.replace('\n', "\r\n").parse().unwrap();
        let mut keys = crate::trusted_keys::TrustedKeys::new();
        assert!(legacy_update(&mut keys, &key, "missing".into()).is_err());
        assert!(legacy_remove(&mut keys, &key).is_err());
        legacy_add(&mut keys, key.clone(), "custom signer".into()).unwrap();
        assert_eq!(
            get_signer(&os_signers(&keys), &key).unwrap().label,
            "custom signer"
        );
        assert!(legacy_add(&mut keys, key.clone(), "duplicate".into()).is_err());
        legacy_update(&mut keys, &key, "renamed".into()).unwrap();
        assert_eq!(
            get_signer(&os_signers(&keys), &key).unwrap().label,
            "renamed"
        );
        legacy_remove(&mut keys, &key).unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn initial_trust_contains_only_the_three_approved_keys() {
        let signers = initial_signers();
        assert_eq!(signers.len(), 3);
        for (label, body) in [
            (
                "drbonez",
                "MCowBQYDK2VwAyEAgpcI3hOExv1njCt+zJoYBartdyk4SKiqSIy3+9tcpOI=",
            ),
            (
                "matthill",
                "MCowBQYDK2VwAyEAfeubYt/ozIfZspUZdmu2Fa6sK2psMnHVGxuVolo8u40=",
            ),
            (
                "Start9",
                "MCowBQYDK2VwAyEAJdDg62o3oHIh4lXECn9gqORe+cCBbH/67MWjMxIMzDg=",
            ),
        ] {
            let key: AnyVerifyingKey =
                format!("-----BEGIN PUBLIC KEY-----\n{body}\n-----END PUBLIC KEY-----\n")
                    .parse()
                    .unwrap();
            assert_eq!(get_signer(&signers, &key).unwrap().label, label);
        }
        for body in [
            "MCowBQYDK2VwAyEAqPjOeMD2CB3UEASoICgfJlfrO5ygjOSBCCNRyDBs75A=",
            "MCowBQYDK2VwAyEASv7fB+rs6FRL+pk7KXaoXONNn26dRkwMB9LXUqmXXLM=",
        ] {
            let key: AnyVerifyingKey =
                format!("-----BEGIN PUBLIC KEY-----\n{body}\n-----END PUBLIC KEY-----\n")
                    .parse()
                    .unwrap();
            assert!(get_signer(&signers, &key).is_err());
        }
    }

    #[test]
    fn initial_start9_key_matches_repository_openpgp_primary_key() {
        let home = tempfile::tempdir().unwrap();
        let output = std::process::Command::new("gpg")
            .arg("--homedir")
            .arg(home.path())
            .args([
                "--batch",
                "--no-options",
                "--with-colons",
                "--with-key-data",
                "--show-keys",
                concat!(env!("CARGO_MANIFEST_DIR"), "/../../../apt/start9.gpg"),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let records = String::from_utf8(output.stdout).unwrap();
        let primary = records
            .lines()
            .find(|line| line.starts_with("pub:"))
            .unwrap();
        assert_eq!(primary.split(':').nth(3), Some("22"));
        let point = records
            .lines()
            .find(|line| line.starts_with("pkd:1:"))
            .unwrap()
            .split(':')
            .nth(3)
            .unwrap();
        let bytes = hex::decode(point.strip_prefix("40").unwrap()).unwrap();
        let key = AnyVerifyingKey::Ed25519(
            ed25519_dalek::VerifyingKey::from_bytes(&bytes.try_into().unwrap()).unwrap(),
        );
        assert_eq!(
            get_signer(&initial_signers(), &key).unwrap().label,
            "Start9"
        );
    }

    #[test]
    fn cli_requires_a_valid_key_and_label_for_writes() {
        let key = initial_signers().into_keys().next().unwrap();
        let parsed =
            SetOsUpdateSignerParams::try_parse_from(["add", key.as_ref(), "--label", "test"])
                .unwrap();
        assert_eq!(parsed.key.interned_der64(), key);
        assert_eq!(parsed.label, "test");
        assert!(SetOsUpdateSignerParams::try_parse_from(["add", key.as_ref()]).is_err());
        assert!(
            SetOsUpdateSignerParams::try_parse_from(["add", "invalid", "--label", "test"]).is_err()
        );
        assert!(OsUpdateSignerKeyParams::try_parse_from(["get", key.as_ref()]).is_ok());
    }
}
