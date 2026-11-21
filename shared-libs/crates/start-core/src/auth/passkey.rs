use std::collections::BTreeMap;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use clap::Parser;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::Empty;
use crate::auth::{AuthKeys, LoginContext, Session};
use crate::context::{CliContext, RpcContext};
use crate::db::model::Database;
use crate::middleware::auth::signature::{HasUnenrolledKeys, SignatureAuthorizationContext};
use crate::prelude::*;
use crate::rpc_continuations::OpenAuthedContinuations;
use crate::sign::AnyVerifyingKey;
use crate::util::serde::{HandlerExtSerde, WithIoFormat, display_serializable};
use crate::util::sync::SyncMutex;

/// The session key each passkey enrolled per device, keyed by the passkey's canonical DER64.
pub type PasskeySessions = BTreeMap<InternedString, BTreeMap<InternedString, InternedString>>;

/// Bound on the browser-chosen device label.
const MAX_DEVICE_ID_LEN: usize = 64;

/// WebAuthn's limit on credential ID length.
const MAX_CREDENTIAL_ID_BYTES: usize = 1023;

/// A WebAuthn credential that signs each encrypted RPC connection in place of a session key.
#[derive(Clone, Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct Passkey {
    pub name: String,
    /// Base64url credential ID, for the browser's `allowCredentials`.
    pub credential_id: String,
    pub added_at: DateTime<Utc>,
}

rpc_toolkit::reflect_ts!(Passkey);
rpc_toolkit::ts_export!(Passkey, namespaces = [""]);

/// What an unauthenticated browser needs to request and attribute a passkey signature.
#[derive(Clone, Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct PasskeyCredential {
    /// Base64url credential ID.
    pub credential_id: String,
    pub public_key: AnyVerifyingKey,
}

rpc_toolkit::reflect_ts!(PasskeyCredential);
rpc_toolkit::ts_export!(PasskeyCredential, namespaces = [""]);

pub fn passkey<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async(list)
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| display_passkeys(handle.params, result))
                .with_about("about.list-passkeys")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "add",
            from_fn_async(add)
                .with_metadata(crate::mcp::MCP, Value::Bool(false))
                .no_cli(),
        )
        .subcommand(
            "enroll",
            from_fn_async(enroll)
                .with_metadata("get_signer", Value::Bool(true))
                .with_metadata("get_user_agent", Value::Bool(true))
                .with_metadata(crate::mcp::MCP, Value::Bool(false))
                .no_cli(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove)
                .no_display()
                .with_about("about.remove-passkey")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "credentials",
            from_fn_async(credentials)
                .with_metadata("authenticated", Value::Bool(false))
                .with_metadata(crate::mcp::MCP, Value::Bool(false))
                .no_cli(),
        )
}

pub async fn list(ctx: RpcContext) -> Result<BTreeMap<InternedString, Passkey>, Error> {
    ctx.db.peek().await.as_private().as_passkeys().de()
}

fn display_passkeys(
    params: WithIoFormat<Empty>,
    passkeys: BTreeMap<InternedString, Passkey>,
) -> Result<(), Error> {
    use prettytable::*;

    if let Some(format) = params.format {
        return display_serializable(format, passkeys);
    }
    let mut table = Table::new();
    table.add_row(row![bc => "NAME", "PUBLIC KEY", "ADDED"]);
    for (key, passkey) in passkeys {
        table.add_row(row![passkey.name, key, passkey.added_at]);
    }
    print_table(&table)?;
    Ok(())
}

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct AddPasskeyParams {
    pub name: String,
    /// Base64url credential ID.
    pub credential_id: String,
    pub public_key: AnyVerifyingKey,
    /// The server password, confirming a credential that will outlive every session.
    pub password: String,
}

rpc_toolkit::reflect_ts!(AddPasskeyParams);
rpc_toolkit::ts_export!(AddPasskeyParams, namespaces = [""]);

fn validate_credential_id(credential_id: &str) -> Result<(), Error> {
    match URL_SAFE_NO_PAD.decode(credential_id) {
        Ok(id) if !id.is_empty() && id.len() <= MAX_CREDENTIAL_ID_BYTES => Ok(()),
        _ => Err(Error::new(
            eyre!("{}", t!("auth.passkey.invalid-credential-id")),
            ErrorKind::InvalidRequest,
        )),
    }
}

pub async fn add(
    ctx: RpcContext,
    AddPasskeyParams {
        name,
        credential_id,
        public_key,
        password,
    }: AddPasskeyParams,
) -> Result<(), Error> {
    validate_credential_id(&credential_id)?;
    let passkey = Passkey {
        name,
        credential_id,
        added_at: Utc::now(),
    };
    ctx.db
        .mutate(|db| register(db, &password, &public_key, &passkey))
        .await
        .result
}

fn register(
    db: &mut Model<Database>,
    password: &str,
    public_key: &AnyVerifyingKey,
    passkey: &Passkey,
) -> Result<(), Error> {
    RpcContext::check_password(db, password)?;
    db.as_private_mut()
        .as_passkeys_mut()
        .insert(&public_key.interned_der64(), passkey)?;
    Ok(())
}

pub(crate) fn is_registered(db: &Model<Database>, key: &InternedString) -> bool {
    db.as_private().as_passkeys().as_idx(key).is_some()
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct RemovePasskeyParams {
    #[arg(help = "help.arg.passkey-public-key")]
    pub public_key: AnyVerifyingKey,
}

rpc_toolkit::reflect_ts!(RemovePasskeyParams);
rpc_toolkit::ts_export!(RemovePasskeyParams, namespaces = [""]);

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct EnrollPasskeySessionParams {
    /// The browser's unpersisted session key.
    pub pubkey: AnyVerifyingKey,
    /// A stable random label for the browser; its previous session key for this passkey is revoked.
    pub device: InternedString,
    #[visit(ts(skip))]
    #[serde(rename = "__Auth_signer")]
    signer: AnyVerifyingKey,
    #[visit(ts(skip))]
    #[serde(rename = "__Auth_userAgent")]
    user_agent: Option<String>,
}

rpc_toolkit::reflect_ts!(EnrollPasskeySessionParams);
rpc_toolkit::ts_export!(EnrollPasskeySessionParams, namespaces = [""]);

/// Enrolls a session key in memory only, on a connection a registered passkey signed.
pub async fn enroll(
    ctx: RpcContext,
    EnrollPasskeySessionParams {
        pubkey,
        device,
        signer,
        user_agent,
    }: EnrollPasskeySessionParams,
) -> Result<(), Error> {
    if device.is_empty() || device.len() > MAX_DEVICE_ID_LEN {
        return Err(Error::new(
            eyre!("{}", t!("auth.passkey.invalid-device")),
            ErrorKind::InvalidRequest,
        ));
    }
    let passkey = signer.interned_der64();
    let peek = ctx.db.peek().await;
    let Some(registered) = peek.as_private().as_passkeys().as_idx(&passkey) else {
        return Err(Error::new(
            eyre!("{}", t!("auth.passkey.not-a-passkey")),
            ErrorKind::Authorization,
        ));
    };
    let now = Utc::now();
    let session = Session {
        name: Some(registered.as_name().de()?.into()),
        logged_in: now,
        last_active: now,
        user_agent,
    };
    let key = pubkey.interned_der64();
    ctx.ephemeral_auth_keys
        .mutate(|keys| keys.0.insert(key.clone(), session));
    let replaced = ctx
        .passkey_sessions
        .mutate(|sessions| record_session(sessions, passkey, device, key));
    if let Some(replaced) = replaced {
        ctx.unenroll([replaced]).await?;
    }
    Ok(())
}

/// Returns the device's previous, distinct session key for the passkey.
fn record_session(
    sessions: &mut PasskeySessions,
    passkey: InternedString,
    device: InternedString,
    key: InternedString,
) -> Option<InternedString> {
    sessions
        .entry(passkey)
        .or_default()
        .insert(device, key.clone())
        .filter(|previous| *previous != key)
}

/// Revokes the passkey, the session keys it enrolled, and their open connections and continuations.
pub async fn remove(
    ctx: RpcContext,
    RemovePasskeyParams { public_key }: RemovePasskeyParams,
) -> Result<(), Error> {
    let key = public_key.interned_der64();
    let continuations = ctx.open_authed_continuations();
    let ephemeral = ctx.ephemeral_auth_keys();
    let sessions = &ctx.passkey_sessions;
    ctx.db
        .mutate(|db| revoke(continuations, ephemeral, sessions, db, key))
        .await
        .result
}

fn revoke(
    continuations: &OpenAuthedContinuations<Option<InternedString>>,
    ephemeral: Option<&SyncMutex<AuthKeys>>,
    sessions: &SyncMutex<PasskeySessions>,
    db: &mut Model<Database>,
    key: InternedString,
) -> Result<(), Error> {
    db.as_private_mut().as_passkeys_mut().remove(&key)?;
    let enrolled = sessions
        .mutate(|sessions| sessions.remove(&key))
        .unwrap_or_default();
    HasUnenrolledKeys::unenroll::<RpcContext>(
        continuations,
        ephemeral,
        db,
        enrolled.into_values().chain([key]),
    )?;
    Ok(())
}

pub async fn credentials(ctx: RpcContext) -> Result<Vec<PasskeyCredential>, Error> {
    ctx.db
        .peek()
        .await
        .as_private()
        .as_passkeys()
        .de()?
        .into_iter()
        .map(|(key, passkey)| {
            Ok(PasskeyCredential {
                credential_id: passkey.credential_id,
                public_key: key.parse()?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use patch_db::PatchDb;
    use patch_db::json_ptr::ROOT;

    use super::*;
    use crate::account::AccountInfo;

    const PASSWORD: &str = "passkey-test";

    async fn database() -> TypedPatchDb<Database> {
        let account = AccountInfo::new(PASSWORD, std::time::SystemTime::now(), None).unwrap();
        let raw = PatchDb::in_memory();
        raw.put(&ROOT, &Database::init(&account, false, None, None).unwrap())
            .await
            .unwrap();
        TypedPatchDb::load(raw).await.unwrap()
    }

    fn passkey() -> (AnyVerifyingKey, Passkey) {
        let key = crate::sign::webauthn::test_util::p256_key();
        (
            AnyVerifyingKey::P256(*key.verifying_key()),
            Passkey {
                name: "laptop".into(),
                credential_id: URL_SAFE_NO_PAD.encode([1, 2, 3]),
                added_at: Utc::now(),
            },
        )
    }

    #[tokio::test]
    async fn passkeys_survive_logout_until_revoked_with_their_session_keys() {
        let db = database().await;
        let (public_key, passkey) = passkey();
        let id = public_key.interned_der64();
        assert!(
            db.mutate(|db| register(db, "wrong", &public_key, &passkey))
                .await
                .result
                .is_err()
        );
        assert!(!is_registered(&db.peek().await, &id));
        db.mutate(|db| register(db, PASSWORD, &public_key, &passkey))
            .await
            .result
            .unwrap();
        assert!(is_registered(&db.peek().await, &id));

        let continuations = OpenAuthedContinuations::new();
        db.mutate(|db| {
            HasUnenrolledKeys::unenroll::<RpcContext>(&continuations, None, db, [id.clone()])
                .map(|_| ())
        })
        .await
        .result
        .unwrap();
        assert!(is_registered(&db.peek().await, &id));

        let ephemeral = SyncMutex::new(AuthKeys::new());
        let session = InternedString::intern("session-key");
        ephemeral.mutate(|keys| {
            let now = Utc::now();
            keys.0.insert(
                session.clone(),
                Session {
                    name: None,
                    logged_in: now,
                    last_active: now,
                    user_agent: None,
                },
            )
        });
        let mut recorded = PasskeySessions::new();
        record_session(&mut recorded, id.clone(), "device".into(), session.clone());
        let sessions = SyncMutex::new(recorded);
        db.mutate(|db| revoke(&continuations, Some(&ephemeral), &sessions, db, id.clone()))
            .await
            .result
            .unwrap();
        assert!(!is_registered(&db.peek().await, &id));
        assert!(ephemeral.peek(|keys| keys.0.is_empty()));
        assert!(sessions.peek(|sessions| sessions.is_empty()));
    }

    #[test]
    fn a_device_replaces_only_its_own_session_key() {
        let mut sessions = PasskeySessions::new();
        let passkey = InternedString::intern("passkey");
        let record = |sessions: &mut PasskeySessions, device: &str, key: &str| {
            record_session(sessions, passkey.clone(), device.into(), key.into())
        };
        assert_eq!(record(&mut sessions, "laptop", "a"), None);
        assert_eq!(record(&mut sessions, "phone", "b"), None);
        assert_eq!(record(&mut sessions, "laptop", "a"), None);
        assert_eq!(record(&mut sessions, "laptop", "c"), Some("a".into()));
        assert_eq!(
            sessions[&passkey],
            BTreeMap::from([("laptop".into(), "c".into()), ("phone".into(), "b".into())])
        );
    }

    #[tokio::test]
    async fn databases_without_passkeys_load_empty() {
        let account = AccountInfo::new(PASSWORD, std::time::SystemTime::now(), None).unwrap();
        let mut value =
            serde_json::to_value(Database::init(&account, false, None, None).unwrap()).unwrap();
        value["private"].as_object_mut().unwrap().remove("passkeys");
        let raw = PatchDb::in_memory();
        raw.put(&ROOT, &value).await.unwrap();
        let db = TypedPatchDb::<Database>::load(raw).await.unwrap();
        assert!(
            db.peek()
                .await
                .as_private()
                .as_passkeys()
                .de()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn credential_ids_are_bounded_base64url() {
        validate_credential_id(&URL_SAFE_NO_PAD.encode([1, 2, 3])).unwrap();
        validate_credential_id(&URL_SAFE_NO_PAD.encode([7; MAX_CREDENTIAL_ID_BYTES])).unwrap();
        for invalid in [
            String::new(),
            "not base64!".into(),
            "AQID==".into(),
            URL_SAFE_NO_PAD.encode([7; MAX_CREDENTIAL_ID_BYTES + 1]),
        ] {
            assert!(validate_credential_id(&invalid).is_err(), "{invalid}");
        }
    }
}
