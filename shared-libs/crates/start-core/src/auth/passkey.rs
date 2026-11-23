use std::collections::BTreeMap;
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use clap::Parser;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::Empty;
use crate::auth::{AuthKeys, Session};
use crate::context::rpc::WeakRpcContext;
use crate::context::{CliContext, RpcContext};
use crate::db::model::Database;
use crate::middleware::auth::signature::{HasUnenrolledKeys, SignatureAuthorizationContext};
use crate::prelude::*;
use crate::rpc_continuations::OpenAuthedContinuations;
use crate::sign::AnyVerifyingKey;
use crate::sign::session::PasskeyResolver;
use crate::util::serde::{HandlerExtSerde, WithIoFormat, display_serializable};
use crate::util::sync::SyncMutex;

pub mod sessions;

/// Keeps a passkey-signed handshake within its record limit.
const MAX_CREDENTIAL_ID_BYTES: usize = 255;

/// A persisted WebAuthn credential independent of browser session expiry.
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
    let verified_hash = crate::auth::verify_password::<RpcContext>(&ctx, &password).await?;
    ctx.db
        .mutate(|db| register(db, &verified_hash, &public_key, &passkey))
        .await
        .result
}

/// Rejects a password changed since it was verified.
fn register(
    db: &mut Model<Database>,
    verified_hash: &Option<String>,
    public_key: &AnyVerifyingKey,
    passkey: &Passkey,
) -> Result<(), Error> {
    crate::auth::check_verified_password::<RpcContext>(db, verified_hash)?;
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
/// The key lasts while it has connections and for [`sessions::GRACE`] after the last closes.
pub async fn enroll(ctx: RpcContext, params: EnrollPasskeySessionParams) -> Result<(), Error> {
    let ephemeral = &ctx.ephemeral_auth_keys;
    let tracked = &ctx.passkey_sessions;
    let owner = std::panic::AssertUnwindSafe(ctx.weak());
    ctx.db
        .mutate(|db| {
            let key = enroll_session(ephemeral, tracked, db, params)?;
            if let Some(ctx) = owner.upgrade() {
                sessions::expire_after_grace(&ctx, key);
            }
            Ok(())
        })
        .await
        .result
}

/// The caller holds the database mutation gate shared with revocation.
fn enroll_session(
    ephemeral: &SyncMutex<AuthKeys>,
    sessions: &SyncMutex<sessions::PasskeySessions>,
    db: &mut Model<Database>,
    EnrollPasskeySessionParams {
        pubkey,
        signer,
        user_agent,
    }: EnrollPasskeySessionParams,
) -> Result<InternedString, Error> {
    let passkey = signer.interned_der64();
    let Some(registered) = db.as_private().as_passkeys().as_idx(&passkey) else {
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
    ephemeral.mutate(|keys| keys.0.insert(key.clone(), session));
    sessions.mutate(|sessions| sessions.enroll(key.clone(), passkey, std::time::Instant::now()));
    Ok(key)
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
    sessions: &SyncMutex<sessions::PasskeySessions>,
    db: &mut Model<Database>,
    key: InternedString,
) -> Result<(), Error> {
    db.as_private_mut().as_passkeys_mut().remove(&key)?;
    let enrolled = sessions.mutate(|sessions| sessions.revoke_passkey(&key));
    HasUnenrolledKeys::unenroll::<RpcContext>(
        continuations,
        ephemeral,
        db,
        enrolled.into_iter().chain([key]),
    )?;
    Ok(())
}

/// Resolves the credential ID an encrypted connection's passkey signature carries.
pub(crate) fn resolver(ctx: WeakRpcContext) -> PasskeyResolver {
    Arc::new(move |credential_id| {
        let ctx = ctx.upgrade();
        Box::pin(async move { key_for_credential(&ctx?.db.peek().await, &credential_id) })
    })
}

fn key_for_credential(db: &Model<Database>, credential_id: &[u8]) -> Option<AnyVerifyingKey> {
    db.as_private()
        .as_passkeys()
        .de()
        .ok()?
        .into_iter()
        .find(|(_, passkey)| {
            URL_SAFE_NO_PAD
                .decode(&passkey.credential_id)
                .is_ok_and(|id| id == credential_id)
        })
        .and_then(|(key, _)| key.parse().ok())
}

#[cfg(test)]
mod tests {
    use patch_db::PatchDb;
    use patch_db::json_ptr::ROOT;

    use super::*;
    use crate::account::AccountInfo;
    use crate::auth::LoginContext;

    const PASSWORD: &str = "passkey-test";

    pub(super) async fn database() -> TypedPatchDb<Database> {
        let account = AccountInfo::new(PASSWORD, std::time::SystemTime::now(), None).unwrap();
        let raw = PatchDb::in_memory();
        raw.put(&ROOT, &Database::init(&account, false, None, None).unwrap())
            .await
            .unwrap();
        TypedPatchDb::load(raw).await.unwrap()
    }

    async fn current_hash(db: &TypedPatchDb<Database>) -> Option<String> {
        let hash = RpcContext::password_hash(&db.peek().await).unwrap();
        RpcContext::check_password(&db.peek().await, PASSWORD).unwrap();
        hash
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
        let hash = current_hash(&db).await;
        let (public_key, passkey) = passkey();
        let id = public_key.interned_der64();
        assert!(
            db.mutate(|db| register(db, &Some("replaced".into()), &public_key, &passkey))
                .await
                .result
                .is_err()
        );
        assert!(!is_registered(&db.peek().await, &id));
        db.mutate(|db| register(db, &hash, &public_key, &passkey))
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
        let mut recorded = sessions::PasskeySessions::default();
        recorded.enroll(session.clone(), id.clone(), std::time::Instant::now());
        let sessions = SyncMutex::new(recorded);
        db.mutate(|db| revoke(&continuations, Some(&ephemeral), &sessions, db, id.clone()))
            .await
            .result
            .unwrap();
        assert!(!is_registered(&db.peek().await, &id));
        assert!(ephemeral.peek(|keys| keys.0.is_empty()));
        assert!(sessions.mutate(|sessions| !sessions.connect(&session)));
    }

    fn enrollment(signer: &AnyVerifyingKey, seed: u8) -> EnrollPasskeySessionParams {
        EnrollPasskeySessionParams {
            pubkey: AnyVerifyingKey::Ed25519(
                ed25519_dalek::SigningKey::from_bytes(&[seed; 32]).verifying_key(),
            ),
            signer: signer.clone(),
            user_agent: None,
        }
    }

    #[tokio::test]
    async fn revoked_passkeys_cannot_publish_a_session_from_prior_authorization() {
        let db = database().await;
        let hash = current_hash(&db).await;
        let (public_key, passkey) = passkey();
        let id = public_key.interned_der64();
        db.mutate(|db| register(db, &hash, &public_key, &passkey))
            .await
            .result
            .unwrap();
        let admitted = db.peek().await;
        let continuations = OpenAuthedContinuations::new();
        let ephemeral = SyncMutex::new(AuthKeys::new());
        let sessions = SyncMutex::new(sessions::PasskeySessions::default());
        db.mutate(|db| revoke(&continuations, Some(&ephemeral), &sessions, db, id.clone()))
            .await
            .result
            .unwrap();
        assert!(is_registered(&admitted, &id));
        let params = enrollment(&public_key, 11);
        let key = params.pubkey.interned_der64();
        let result = db
            .mutate(|db| enroll_session(&ephemeral, &sessions, db, params))
            .await
            .result;
        assert_eq!(result.unwrap_err().kind, ErrorKind::Authorization);
        assert!(ephemeral.peek(|keys| keys.0.is_empty()));
        assert!(!sessions.mutate(|sessions| sessions.connect(&key)));
    }

    #[tokio::test]
    async fn passkey_removal_revokes_connected_and_unconnected_derived_keys() {
        let db = database().await;
        let hash = current_hash(&db).await;
        let (public_key, passkey) = passkey();
        let id = public_key.interned_der64();
        db.mutate(|db| register(db, &hash, &public_key, &passkey))
            .await
            .result
            .unwrap();
        let continuations = OpenAuthedContinuations::new();
        let ephemeral = SyncMutex::new(AuthKeys::new());
        let sessions = SyncMutex::new(sessions::PasskeySessions::default());
        let mut enrolled = Vec::new();
        for seed in [11, 12] {
            enrolled.push(
                db.mutate(|db| {
                    enroll_session(&ephemeral, &sessions, db, enrollment(&public_key, seed))
                })
                .await
                .result
                .unwrap(),
            );
        }
        assert!(sessions.mutate(|sessions| sessions.connect(&enrolled[0])));
        db.mutate(|db| revoke(&continuations, Some(&ephemeral), &sessions, db, id))
            .await
            .result
            .unwrap();
        assert!(ephemeral.peek(|keys| keys.0.is_empty()));
        for key in enrolled {
            assert!(!sessions.mutate(|sessions| sessions.connect(&key)));
        }
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

    #[tokio::test]
    async fn credential_ids_resolve_to_their_registered_passkey() {
        let db = database().await;
        let hash = current_hash(&db).await;
        let (public_key, passkey) = passkey();
        db.mutate(|db| register(db, &hash, &public_key, &passkey))
            .await
            .result
            .unwrap();
        let peek = db.peek().await;
        assert_eq!(key_for_credential(&peek, &[1, 2, 3]), Some(public_key));
        assert_eq!(key_for_credential(&peek, &[1, 2]), None);
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
