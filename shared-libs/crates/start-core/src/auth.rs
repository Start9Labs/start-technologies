use std::borrow::{Borrow, BorrowMut};
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::Path;

use chrono::{DateTime, TimeDelta, Utc};
use clap::Parser;
use color_eyre::eyre::eyre;
use futures::future::BoxFuture;
use imbl_value::imbl::OrdMap;
use imbl_value::{InternedString, json};
use itertools::Itertools;
use josekit::jwk::Jwk;
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{
    CallRemote, Context, Empty, HandlerArgs, HandlerExt, ParentHandler, from_fn_async,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tracing::instrument;

use crate::context::{CliContext, RpcContext};
use crate::mcp::{EFFECT, Effect};
use crate::middleware::auth::signature::{HasUnenrolledKeys, SignatureAuthorizationContext};
use crate::prelude::*;
use crate::rpc_continuations::{OpenAuthedContinuations, RevocationSubscription};
use crate::sign::{AnyVerifyingKey, Signer};
use crate::util::crypto::EncryptedWire;
use crate::util::io::create_file_mod;
use crate::util::serde::{HandlerExtSerde, display_serializable};
use crate::util::sync::SyncMutex;
use crate::{Error, ResultExt, ensure_code};

/// Enrolled sign-ins indexed by canonical DER64 SPKI.
#[derive(Debug, Clone, Default, Serialize, VisitFields)]
pub struct AuthKeys(pub BTreeMap<InternedString, Session>);

impl<'de> Deserialize<'de> for AuthKeys {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        crate::sign::canonicalize_verifying_keys(BTreeMap::deserialize(deserializer)?)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

rpc_toolkit::reflect_ts!(AuthKeys);
impl Borrow<BTreeMap<InternedString, Session>> for AuthKeys {
    fn borrow(&self) -> &BTreeMap<InternedString, Session> {
        &self.0
    }
}
impl BorrowMut<BTreeMap<InternedString, Session>> for AuthKeys {
    fn borrow_mut(&mut self) -> &mut BTreeMap<InternedString, Session> {
        &mut self.0
    }
}
impl AuthKeys {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }
}
impl Map for AuthKeys {
    type Key = InternedString;
    type Value = Session;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Ok(key)
    }
    fn key_string(key: &Self::Key) -> Result<InternedString, Error> {
        Ok(key.clone())
    }
}

/// The server's sessions, by credential type.
#[derive(Debug, Clone, Default, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct Sessions {
    pub pubkeys: AuthKeys,
    pub tokens: TokenSessions,
}

/// Bearer-token sessions indexed by token hash.
pub type TokenSessions = BTreeMap<InternedString, Session>;

rpc_toolkit::reflect_ts!(Sessions);
rpc_toolkit::ts_export!(Sessions, namespaces = [""]);

/// Contexts where a password can enroll an auth key (login), plus access to
/// the enrolled-key store that login writes and the key list reads.
pub trait LoginContext: SignatureAuthorizationContext {
    /// The persisted enrolled-key store. Enrollment happens via login; never
    /// remove a key directly — unenrollment goes through
    /// [`SignatureAuthorizationContext::unenroll`] so a revoked key's open
    /// continuations die with it.
    fn access_auth_keys(db: &mut Model<Self::Database>) -> &mut Model<AuthKeys>;
    /// The bearer-token store, keyed by token hash. `None` where tokens are unsupported.
    /// Never remove a token directly; [`revoke_tokens`] kills its continuations.
    #[allow(unused_variables)]
    fn access_tokens(db: &mut Model<Self::Database>) -> Option<&mut Model<TokenSessions>> {
        None
    }
    /// Current key membership under the context's enrollment and expiry policy.
    fn auth_key_is_enrolled(
        &self,
        db: &Model<Self::Database>,
        key: &InternedString,
    ) -> Result<bool, Error> {
        Ok(self
            .ephemeral_auth_keys()
            .is_some_and(|keys| keys.peek(|keys| keys.0.contains_key(&**key)))
            || Self::access_auth_keys(&mut db.clone())
                .as_idx(key)
                .is_some())
    }
    /// Current bearer membership in the context's authoritative token store.
    fn auth_token_is_enrolled(&self, db: &Model<Self::Database>, token: &InternedString) -> bool {
        Self::access_tokens(&mut db.clone()).is_some_and(|tokens| tokens.as_idx(token).is_some())
    }
    /// Database-owned password hash. External credential stores return `None`.
    #[allow(unused_variables)]
    fn password_hash(db: &Model<Self::Database>) -> Result<Option<String>, Error> {
        Ok(None)
    }
    fn check_password(db: &Model<Self::Database>, password: &str) -> Result<(), Error> {
        check_password(&Self::password_hash(db)?.unwrap_or_default(), password)
    }
    /// Whether a sign-in has ended, judged by a trusted clock. Defaults to
    /// [`MAX_IDLE`] without use.
    fn sign_in_expired(session: &Session, now: DateTime<Utc>) -> bool {
        now - session.last_active > MAX_IDLE
    }
    #[allow(unused_variables)]
    fn post_login_hook(&self, password: &str) -> impl Future<Output = Result<(), Error>> + Send {
        async { Ok(()) }
    }
    /// Observes each correctly signed login with its outcome.
    #[allow(unused_variables)]
    fn login_attempt_hook(
        &self,
        attempt: &LoginAttempt,
        result: Result<(), &Error>,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
}

/// Where a login came from.
#[derive(Debug, Clone, Default)]
pub struct LoginAttempt {
    /// The client's address, through any relay that recorded it.
    pub peer: Option<IpAddr>,
    pub user_agent: Option<String>,
}

/// Registers revocation before rechecking live credential membership; retains no grants or snapshot.
/// An absent subject retains the caller's independently authenticated local-credential scope.
pub async fn continuation_subscription<C: LoginContext>(
    context: &C,
    subject: Option<InternedString>,
) -> Result<RevocationSubscription, Error> {
    let subscription = context
        .open_authed_continuations()
        .subscribe_to_kill(subject.clone());
    if let Some(subject) = subject {
        let db = context.db().peek().await;
        if !context.auth_key_is_enrolled(&db, &subject)?
            && !context.auth_token_is_enrolled(&db, &subject)
        {
            return Err(Error::new(
                eyre!("{}", t!("middleware.auth.key-not-authorized")),
                crate::ErrorKind::Authorization,
            ));
        }
    }
    Ok(subscription)
}

/// Removes bearer tokens and kills the continuations they opened. For call sites
/// already inside a db transaction.
pub fn revoke_tokens<C: LoginContext>(
    continuations: &OpenAuthedContinuations<Option<InternedString>>,
    db: &mut Model<C::Database>,
    tokens: impl IntoIterator<Item = InternedString>,
) -> Result<(), Error> {
    let Some(store) = C::access_tokens(db) else {
        return Ok(());
    };
    for token in tokens {
        continuations.kill(&Some(token.clone()));
        store.remove(&token)?;
    }
    Ok(())
}

impl LoginContext for RpcContext {
    fn access_auth_keys(db: &mut Model<Self::Database>) -> &mut Model<AuthKeys> {
        db.as_private_mut().as_sessions_mut().as_pubkeys_mut()
    }
    fn access_tokens(db: &mut Model<Self::Database>) -> Option<&mut Model<TokenSessions>> {
        Some(db.as_private_mut().as_sessions_mut().as_tokens_mut())
    }
    fn password_hash(db: &Model<Self::Database>) -> Result<Option<String>, Error> {
        Ok(Some(db.as_private().as_password().de()?))
    }
    async fn post_login_hook(&self, password: &str) -> Result<(), Error> {
        if tokio::fs::metadata("/media/startos/config/overlay/etc/shadow")
            .await
            .is_err()
        {
            write_shadow(&password).await?;
        }
        Ok(())
    }
}

pub async fn write_shadow(password: &str) -> Result<(), Error> {
    let hash: String = sha_crypt::sha512_simple(password, &sha_crypt::Sha512Params::default())
        .map_err(|e| Error::new(eyre!("{e:?}"), ErrorKind::Serialization))?;
    let shadow_contents = tokio::fs::read_to_string("/etc/shadow").await?;
    let mut shadow_file =
        create_file_mod("/media/startos/config/overlay/etc/shadow", 0o640).await?;
    for line in shadow_contents.lines() {
        match line.split_once(":") {
            Some((user, rest)) if user == "start9" || user == "kiosk" => {
                let (_, rest) = rest.split_once(":").ok_or_else(|| {
                    Error::new(
                        eyre!("{}", t!("auth.malformed-etc-shadow")),
                        ErrorKind::ParseSysInfo,
                    )
                })?;
                shadow_file
                    .write_all(format!("{user}:{hash}:{rest}\n").as_bytes())
                    .await?;
            }
            _ => {
                shadow_file.write_all(line.as_bytes()).await?;
                shadow_file.write_all(b"\n").await?;
            }
        }
    }
    shadow_file.sync_all().await?;
    tokio::fs::copy("/media/startos/config/overlay/etc/shadow", "/etc/shadow").await?;

    // MOK enrollment requires the password established during setup.
    crate::util::mok::enroll_mok(std::path::Path::new("/"))
        .await
        .log_err();

    Ok(())
}

/// The start9 user's crypt(3) hash in `<root>/etc/shadow`. `None` when the user is absent or locked.
pub(crate) async fn start9_password_hash(root: &Path) -> Result<Option<String>, Error> {
    let shadow = tokio::fs::read_to_string(root.join("etc/shadow")).await?;
    Ok(shadow
        .lines()
        .find_map(|line| line.strip_prefix("start9:"))
        .and_then(|rest| rest.split(':').next())
        .filter(|hash| hash.starts_with('$'))
        .map(str::to_owned))
}

/// Accepts SHA-512 crypt hashes, the format [`write_shadow`] writes.
pub(crate) fn check_shadow_password(hash: &str, password: &str) -> Result<(), Error> {
    sha_crypt::sha512_check(password, hash).map_err(|_| {
        Error::new(
            eyre!("{}", t!("auth.password-incorrect")),
            ErrorKind::IncorrectPassword,
        )
    })
}

#[derive(Clone, Serialize, Deserialize, VisitVariants)]
#[serde(untagged)]
pub enum PasswordType {
    EncryptedWire(EncryptedWire),
    String(String),
}

rpc_toolkit::reflect_ts!(PasswordType);
rpc_toolkit::ts_export!(PasswordType, namespaces = [""]);
impl PasswordType {
    /// Unwrapped startup secrets require encrypted admission; legacy JWE remains valid.
    pub fn decrypt_in_session(
        self,
        context: &impl crate::middleware::auth::session::SessionContext,
        current_secret: impl AsRef<Jwk>,
    ) -> Result<String, Error> {
        if matches!(self, Self::String(_)) && context.secure_session().is_none() {
            return Err(Error::new(
                eyre!(t!("auth.encrypted-session-required")),
                ErrorKind::InvalidRequest,
            ));
        }
        self.decrypt(current_secret)
    }

    pub fn decrypt(self, current_secret: impl AsRef<Jwk>) -> Result<String, Error> {
        match self {
            PasswordType::String(x) => Ok(x),
            PasswordType::EncryptedWire(x) => x.decrypt(current_secret).ok_or_else(|| {
                Error::new(
                    color_eyre::eyre::eyre!("{}", t!("auth.couldnt-decode-password")),
                    crate::ErrorKind::Unknown,
                )
            }),
        }
    }
}
impl Default for PasswordType {
    fn default() -> Self {
        PasswordType::String(String::default())
    }
}
pub(crate) struct RedactedPassword;

impl std::fmt::Debug for RedactedPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<REDACTED_PASSWORD>")
    }
}
impl std::fmt::Debug for PasswordType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&RedactedPassword, f)
    }
}

impl std::str::FromStr for PasswordType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match serde_json::from_str(s) {
            Ok(a) => a,
            Err(_) => PasswordType::String(s.to_string()),
        })
    }
}
#[cfg(test)]
mod password_tests;

/// A CLI whose identity key `auth login` enrolls.
pub trait IdKeyContext: Context {
    fn id_key(&self) -> Result<&Signer, Error>;
    /// Where a missing key is generated.
    fn id_key_path(&self) -> &Path;
}
impl IdKeyContext for CliContext {
    fn id_key(&self) -> Result<&Signer, Error> {
        CliContext::id_key(self)
    }
    fn id_key_path(&self) -> &Path {
        &self.id_key_path
    }
}

/// Login, logout and session handlers for the server context `AC`, with the
/// CLI side on `CC`. Product-specific password handlers chain onto the result.
pub fn auth<C: Context, AC: LoginContext, CC: IdKeyContext + CallRemote<AC>>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "login",
            from_fn_async(login_impl::<AC>)
                .with_metadata(EFFECT, Effect::Destructive.into())
                .with_metadata("login", Value::Bool(true))
                .with_metadata(crate::mcp::MCP, Value::Bool(false))
                .with_metadata("get_signer", Value::Bool(true))
                .with_metadata("get_user_agent", Value::Bool(true))
                .with_metadata("get_peer", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "login",
            from_fn_async(cli_login::<AC, CC>)
                .no_display()
                .with_about("about.login-new-auth-session"),
        )
        .subcommand(
            "logout",
            from_fn_async(logout::<AC>)
                .with_metadata(EFFECT, Effect::Destructive.into())
                .with_metadata("get_signer", Value::Bool(true))
                .with_metadata(crate::mcp::MCP, Value::Bool(false))
                .no_display()
                .with_about("about.logout-current-auth-session")
                .with_call_remote::<CC>(),
        )
        .subcommand(
            "key",
            key::<C, AC, CC>().with_about("about.commands-authorized-keys"),
        )
}

fn key<C: Context, AC: LoginContext, CC: CallRemote<AC>>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "add",
            from_fn_async(add_key::<AC>)
                .with_metadata(EFFECT, Effect::Destructive.into())
                .with_metadata("sync_db", Value::Bool(true))
                .with_metadata(crate::mcp::MCP, Value::Bool(false))
                .no_display()
                .with_about("about.add-new-authorized-key")
                .with_call_remote::<CC>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove_key::<AC>)
                .with_metadata(EFFECT, Effect::Destructive.into())
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.remove-authorized-key")
                .with_call_remote::<CC>(),
        )
        .subcommand(
            "list",
            from_fn_async(list_keys::<AC>)
                .with_metadata(EFFECT, Effect::Read.into())
                .with_metadata("get_signer", Value::Bool(true))
                .with_display_serializable()
                .with_custom_display_fn(|HandlerArgs { params, .. }, res| {
                    if let Some(format) = params.format {
                        return display_serializable(format, res);
                    }
                    display_sessions(&res.keys.0, res.current.as_ref())
                })
                .with_about("about.list-authorized-keys")
                .with_call_remote::<CC>(),
        )
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct AddKeyParams {
    #[arg(help = "help.arg.auth-key")]
    pub key: AnyVerifyingKey,
    #[arg(long, help = "help.arg.auth-key-name")]
    #[serde(default)]
    pub name: Option<InternedString>,
    #[arg(long, help = "help.arg.user-agent")]
    #[serde(default)]
    pub user_agent: Option<String>,
}

rpc_toolkit::reflect_ts!(AddKeyParams);
rpc_toolkit::ts_export!(AddKeyParams, namespaces = [""]);

/// Enrolls a key without a password or the login rate limit.
pub async fn add_key<C: LoginContext>(
    ctx: C,
    AddKeyParams {
        key,
        name,
        user_agent,
    }: AddKeyParams,
) -> Result<(), Error> {
    let now = Utc::now();
    let record = Session {
        name,
        logged_in: now,
        last_active: now,
        user_agent,
    };
    ctx.db()
        .mutate(|db| {
            C::access_auth_keys(db).insert(&key.interned_der64(), &record)?;
            Ok(())
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct RemoveKeyParams {
    #[arg(required = true, help = "help.arg.session-pubkeys")]
    pub keys: Vec<AnyVerifyingKey>,
}

rpc_toolkit::reflect_ts!(RemoveKeyParams);
rpc_toolkit::ts_export!(RemoveKeyParams, namespaces = [""]);

pub async fn remove_key<C: LoginContext>(
    ctx: C,
    RemoveKeyParams { keys }: RemoveKeyParams,
) -> Result<(), Error> {
    ctx.unenroll(keys.iter().map(AnyVerifyingKey::interned_der64))
        .await?;
    Ok(())
}

/// Bearer-token commands; a token is a named session.
pub fn token<C: Context, AC: LoginContext>() -> ParentHandler<C>
where
    CliContext: CallRemote<AC>,
{
    ParentHandler::new()
        .subcommand(
            "create",
            from_fn_async(create_token::<AC>)
                .with_metadata(EFFECT, Effect::Write.into())
                .with_metadata(crate::mcp::MCP, Value::Bool(false))
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| {
                    if let Some(format) = handle.params.format {
                        return display_serializable(format, result);
                    }
                    println!("{}", result.token);
                    Ok(())
                })
                .with_about("about.create-auth-token")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "list",
            from_fn_async(list_tokens::<AC>)
                .with_metadata(EFFECT, Effect::Read.into())
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| {
                    if let Some(format) = handle.params.format {
                        return display_serializable(format, result);
                    }
                    display_sessions(&result, None)
                })
                .with_about("about.list-auth-tokens")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove_tokens::<AC>)
                .with_metadata(EFFECT, Effect::Destructive.into())
                .no_display()
                .with_about("about.remove-auth-tokens")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CreateTokenParams {
    #[arg(long, help = "help.arg.token-name")]
    pub name: InternedString,
}

rpc_toolkit::reflect_ts!(CreateTokenParams);
rpc_toolkit::ts_export!(CreateTokenParams, namespaces = [""]);

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct CreatedToken {
    /// The session id the token authenticates as, for `auth.token.remove`.
    pub id: InternedString,
    /// The bearer secret. It is not stored and cannot be retrieved again.
    pub token: String,
}

rpc_toolkit::reflect_ts!(CreatedToken);
rpc_toolkit::ts_export!(CreatedToken, namespaces = [""]);

#[instrument(skip_all)]
pub async fn create_token<C: LoginContext>(
    ctx: C,
    CreateTokenParams { name }: CreateTokenParams,
) -> Result<CreatedToken, Error> {
    let (token, id) = crate::middleware::auth::bearer::new_token();
    let now = Utc::now();
    let record = Session {
        name: Some(name),
        logged_in: now,
        last_active: now,
        user_agent: None,
    };
    ctx.db()
        .mutate(|db| {
            C::access_tokens(db)
                .ok_or_else(|| {
                    Error::new(
                        eyre!("{}", t!("auth.tokens-unsupported")),
                        ErrorKind::InvalidRequest,
                    )
                })?
                .insert(&id, &record)
        })
        .await
        .result?;
    Ok(CreatedToken { id, token })
}

/// Keys and tokens unused this long expire by default.
pub const MAX_IDLE: TimeDelta = TimeDelta::days(30);

fn expired_keys<C: LoginContext>(
    sessions: &BTreeMap<InternedString, Session>,
    now: DateTime<Utc>,
) -> Vec<InternedString> {
    sessions
        .iter()
        .filter(|(_, session)| C::sign_in_expired(session, now))
        .map(|(key, _)| key.clone())
        .collect()
}

/// Unenrolls keys and revokes tokens whose sign-in has expired inside an open
/// db transaction. The caller has established that the clock is trustworthy.
pub fn reap_idle_keys_in<C: LoginContext>(
    continuations: &OpenAuthedContinuations<Option<InternedString>>,
    ephemeral: Option<&SyncMutex<AuthKeys>>,
    db: &mut Model<C::Database>,
) -> Result<(), Error> {
    let now = Utc::now();
    let expired = expired_keys::<C>(&C::access_auth_keys(db).de()?.0, now);
    HasUnenrolledKeys::unenroll::<C>(continuations, ephemeral, db, expired)?;
    let expired = C::access_tokens(db)
        .map(|tokens| tokens.de())
        .transpose()?
        .map_or_else(Vec::new, |tokens| expired_keys::<C>(&tokens, now));
    revoke_tokens::<C>(continuations, db, expired)
}

/// Unenrolls keys and revokes tokens whose sign-in has expired. Waits for a
/// trusted clock, so a wrong boot-time clock cannot reap live sign-ins.
pub async fn reap_idle_keys<C: LoginContext>(ctx: &C) -> Result<(), Error> {
    if !ctx.clock_synced().await {
        return Ok(());
    }
    let continuations = ctx.open_authed_continuations();
    let ephemeral = ctx.ephemeral_auth_keys();
    ctx.db()
        .mutate(|db| reap_idle_keys_in::<C>(continuations, ephemeral, db))
        .await
        .result
}

/// Records a request by an enrolled key, persisting the time at most once per
/// [`SignatureAuthorizationContext::LAST_ACTIVE_RESOLUTION`].
pub async fn touch_last_active<C: LoginContext>(
    ctx: &C,
    key: &InternedString,
) -> Result<(), Error> {
    let now = Utc::now();
    if let Some(ephemeral) = ctx.ephemeral_auth_keys() {
        let found = ephemeral.mutate(|keys| {
            keys.0
                .get_mut(key)
                .map(|session| session.last_active = now)
                .is_some()
        });
        if found {
            return Ok(());
        }
    }
    persist_last_active(ctx, pubkey_store::<C>, key, now).await
}

/// Records a request by a bearer token, persisting the time at most once per
/// [`SignatureAuthorizationContext::LAST_ACTIVE_RESOLUTION`].
pub async fn touch_token<C: LoginContext>(ctx: &C, token: &InternedString) -> Result<(), Error> {
    persist_last_active(ctx, C::access_tokens, token, Utc::now()).await
}

fn pubkey_store<C: LoginContext>(db: &mut Model<C::Database>) -> Option<&mut Model<AuthKeys>> {
    Some(C::access_auth_keys(db))
}

async fn persist_last_active<C: LoginContext, S>(
    ctx: &C,
    store: fn(&mut Model<C::Database>) -> Option<&mut Model<S>>,
    key: &InternedString,
    now: DateTime<Utc>,
) -> Result<(), Error>
where
    S: Serialize
        + DeserializeOwned
        + Borrow<BTreeMap<InternedString, Session>>
        + BorrowMut<BTreeMap<InternedString, Session>>,
{
    if !C::LAST_ACTIVE_RESOLUTION.is_zero() {
        let mut peek = ctx.db().peek().await;
        let fresh = store(&mut peek)
            .map(|sessions| sessions.de())
            .transpose()?
            .and_then(|sessions| {
                sessions
                    .borrow()
                    .get(key)
                    .map(|session| now - session.last_active < C::LAST_ACTIVE_RESOLUTION)
            })
            .unwrap_or(false);
        if fresh {
            return Ok(());
        }
    }
    ctx.db()
        .mutate(|db| {
            let Some(sessions) = store(db) else {
                return Ok(());
            };
            sessions.mutate(|sessions| {
                if let Some(session) = sessions.borrow_mut().get_mut(key) {
                    session.last_active = now;
                }
                Ok(())
            })
        })
        .await
        .result
}

#[test]
fn gen_pwd() {
    println!(
        "{:?}",
        argon2::hash_encoded(
            b"testing1234",
            &rand::random::<[u8; 16]>()[..],
            &argon2::Config::rfc9106_low_mem()
        )
        .unwrap()
    )
}

#[instrument(skip_all)]
async fn cli_login<AC: LoginContext, CC: IdKeyContext + CallRemote<AC>>(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        ..
    }: HandlerArgs<CC>,
) -> Result<(), RpcError> {
    login::<AC, CC>(&ctx, &parent_method.into_iter().chain(method).join(".")).await?;
    Ok(())
}

/// Enrolls the identity key, generating one first if there is none. Boxed so
/// a `CallRemote` impl can await it without its future containing itself.
pub fn login<'a, AC: LoginContext, CC: IdKeyContext + CallRemote<AC>>(
    ctx: &'a CC,
    method: &'a str,
) -> BoxFuture<'a, Result<(), Error>> {
    Box::pin(async move {
        <CC as CallRemote<AC>>::call_remote(
            ctx,
            method,
            OrdMap::new(),
            login_params(ctx).await?,
            Empty {},
        )
        .await?;

        Ok(())
    })
}

/// Prompts for the password and returns `auth.login` params enrolling the
/// identity key, generating it first when it cannot be loaded.
pub async fn login_params<C: IdKeyContext>(ctx: &C) -> Result<Value, Error> {
    let password = crate::util::tui::read_password("PASSWORD", "Password: ")?;

    if ctx.id_key().is_err() {
        let secret = ed25519_dalek::SigningKey::generate(&mut crate::util::crypto::os_rng());
        crate::developer::write_signing_key(&secret, ctx.id_key_path()).await?;
    }
    let pubkey = ctx.id_key()?.verifying_key().await?.to_string();

    Ok(json!({
        "password": password,
        "pubkey": pubkey,
        "metadata": {
            "platforms": ["cli"],
        },
    }))
}

pub fn check_password(hash: &str, password: &str) -> Result<(), Error> {
    ensure_code!(
        argon2::verify_encoded(&hash, password.as_bytes()).map_err(|_| {
            Error::new(
                eyre!("{}", t!("auth.password-incorrect")),
                crate::ErrorKind::IncorrectPassword,
            )
        })?,
        crate::ErrorKind::IncorrectPassword,
        t!("auth.password-incorrect")
    );
    Ok(())
}

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct LoginParams {
    password: String,
    #[visit(ts(skip))]
    #[serde(rename = "__Auth_userAgent")]
    user_agent: Option<String>,
    /// The PEM-encoded public key to enroll on a successful login. The login
    /// request itself is signed with the matching secret key, so enrollment
    /// proves possession.
    pubkey: AnyVerifyingKey,
    /// The key the request was actually signed with, injected by the auth
    /// middleware. Enforced to equal `pubkey`, so a login can only enroll the
    /// key that proved possession.
    #[visit(ts(skip))]
    #[serde(rename = "__Auth_signer")]
    signer: AnyVerifyingKey,
    #[visit(ts(skip))]
    #[serde(rename = "__Auth_peer", default)]
    peer: Option<IpAddr>,
    /// Enroll in memory only, never persisted (kiosk mode, which re-enrolls
    /// on every browser restart and would otherwise accumulate keys).
    #[serde(default)]
    ephemeral: bool,
}

rpc_toolkit::reflect_ts!(LoginParams);
rpc_toolkit::ts_export!(LoginParams, namespaces = [""]);

#[instrument(skip_all)]
pub async fn login_impl<C: LoginContext>(
    ctx: C,
    LoginParams {
        password,
        user_agent,
        pubkey,
        signer,
        peer,
        ephemeral,
    }: LoginParams,
) -> Result<(), Error> {
    if signer != pubkey {
        return Err(Error::new(
            eyre!("{}", t!("middleware.auth.enrolled-key-mismatch")),
            ErrorKind::InvalidRequest,
        ));
    }

    let attempt = LoginAttempt {
        peer,
        user_agent: user_agent.clone(),
    };
    let result = enroll(&ctx, &password, &pubkey, user_agent, ephemeral).await;
    ctx.login_attempt_hook(&attempt, result.as_ref().map(|_| ()))
        .await;
    result
}

/// Returns the verified database hash; holds no database lock while hashing.
async fn verify_password<C: LoginContext>(
    ctx: &C,
    password: &str,
) -> Result<Option<String>, Error> {
    let db = ctx.db().peek().await;
    let password = password.to_owned();
    tokio::task::spawn_blocking(move || {
        C::check_password(&db, &password)?;
        C::password_hash(&db)
    })
    .await
    .with_kind(ErrorKind::Unknown)?
}

/// Enrollment rejects a replaced database password before inserting persistent or ephemeral keys.
fn check_verified_password<C: LoginContext>(
    db: &Model<C::Database>,
    verified_hash: &Option<String>,
) -> Result<(), Error> {
    ensure_code!(
        &C::password_hash(db)? == verified_hash,
        ErrorKind::IncorrectPassword,
        t!("auth.password-incorrect")
    );
    Ok(())
}

async fn enroll<C: LoginContext>(
    ctx: &C,
    password: &str,
    pubkey: &AnyVerifyingKey,
    user_agent: Option<String>,
    ephemeral: bool,
) -> Result<(), Error> {
    let now = Utc::now();
    let pubkey = pubkey.interned_der64();
    let record = Session {
        name: None,
        logged_in: now,
        last_active: now,
        user_agent,
    };
    let ephemeral_keys = if ephemeral {
        Some(ctx.ephemeral_auth_keys().ok_or_else(|| {
            Error::new(
                eyre!("{}", t!("middleware.auth.ephemeral-unsupported")),
                ErrorKind::InvalidRequest,
            )
        })?)
    } else {
        None
    };
    let verified_hash = verify_password::<C>(ctx, password).await?;
    ctx.db()
        .mutate(|db| {
            check_verified_password::<C>(db, &verified_hash)?;
            if let Some(ephemeral_keys) = ephemeral_keys {
                ephemeral_keys.mutate(|keys| {
                    keys.0.insert(pubkey.clone(), record);
                });
            } else {
                C::access_auth_keys(db).insert(&pubkey, &record)?;
            }
            Ok(())
        })
        .await
        .result?;

    ctx.post_login_hook(password).await
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct LogoutParams {
    #[arg(skip)]
    #[visit(ts(skip))]
    #[serde(rename = "__Auth_signer")]
    signer: Option<InternedString>,
}

rpc_toolkit::reflect_ts!(LogoutParams);

pub async fn logout<C: LoginContext>(
    ctx: C,
    LogoutParams { signer }: LogoutParams,
) -> Result<Option<HasUnenrolledKeys>, Error> {
    let Some(signer) = signer else {
        return Err(Error::new(
            eyre!("{}", t!("auth.local-logout-unnecessary")),
            ErrorKind::InvalidRequest,
        ));
    };
    let unenrolled = ctx.unenroll([signer.clone()]).await?;
    let continuations = ctx.open_authed_continuations();
    ctx.db()
        .mutate(|db| revoke_tokens::<C>(continuations, db, [signer]))
        .await
        .result?;
    Ok(Some(unenrolled))
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// A friendly name for the key, if one was assigned at enrollment (e.g.
    /// tunnel device keys). UI-enrolled keys are unnamed.
    #[serde(default)]
    pub name: Option<InternedString>,
    #[serde(default)]
    pub logged_in: DateTime<Utc>,
    #[serde(default)]
    pub last_active: DateTime<Utc>,
    #[serde(default)]
    pub user_agent: Option<String>,
}

rpc_toolkit::reflect_ts!(Session);
rpc_toolkit::ts_export!(Session, namespaces = [""]);

/// Enrolled keys, including ephemeral ones, and the caller's own.
#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct KeyList {
    current: Option<InternedString>,
    keys: AuthKeys,
}

rpc_toolkit::reflect_ts!(KeyList);
rpc_toolkit::ts_export!(KeyList, namespaces = [""]);

fn display_sessions<'a>(
    sessions: impl IntoIterator<Item = (&'a InternedString, &'a Session)>,
    current: Option<&InternedString>,
) -> Result<(), Error> {
    use prettytable::*;

    let mut table = Table::new();
    table.add_row(row![bc =>
        "ID",
        "NAME",
        "LOGGED IN",
        "LAST ACTIVE",
        "USER AGENT",
    ]);
    for (id, session) in sessions {
        let mut row = row![
            id,
            session.name.as_deref().unwrap_or("N/A"),
            &format!("{}", session.logged_in),
            &format!("{}", session.last_active),
            session.user_agent.as_deref().unwrap_or("N/A"),
        ];
        if Some(id) == current {
            row.iter_mut()
                .map(|c| c.style(Attr::ForegroundColor(color::GREEN)))
                .collect::<()>()
        }
        table.add_row(row);
    }
    print_table(&table)?;
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ListKeysParams {
    #[arg(skip)]
    #[visit(ts(skip))]
    #[serde(rename = "__Auth_signer")]
    signer: Option<InternedString>,
}

rpc_toolkit::reflect_ts!(ListKeysParams);

#[instrument(skip_all)]
pub async fn list_keys<C: LoginContext>(
    ctx: C,
    ListKeysParams { signer }: ListKeysParams,
) -> Result<KeyList, Error> {
    let mut keys = C::access_auth_keys(&mut ctx.db().peek().await).de()?;
    if let Some(ephemeral) = ctx.ephemeral_auth_keys() {
        ephemeral.peek(|e| {
            keys.0
                .extend(e.0.iter().map(|(k, v)| (k.clone(), v.clone())))
        });
    }
    Ok(KeyList {
        current: signer,
        keys,
    })
}

#[instrument(skip_all)]
pub async fn list_tokens<C: LoginContext>(ctx: C) -> Result<TokenSessions, Error> {
    C::access_tokens(&mut ctx.db().peek().await)
        .map(|tokens| tokens.de())
        .transpose()
        .map(Option::unwrap_or_default)
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct RemoveTokensParams {
    #[arg(required = true, help = "help.arg.session-tokens")]
    tokens: Vec<InternedString>,
}

rpc_toolkit::reflect_ts!(RemoveTokensParams);
rpc_toolkit::ts_export!(RemoveTokensParams, namespaces = [""]);

#[instrument(skip_all)]
pub async fn remove_tokens<C: LoginContext>(
    ctx: C,
    RemoveTokensParams { tokens }: RemoveTokensParams,
) -> Result<(), Error> {
    let continuations = ctx.open_authed_continuations();
    ctx.db()
        .mutate(|db| revoke_tokens::<C>(continuations, db, tokens))
        .await
        .result
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ResetPasswordParams {
    #[arg(help = "help.arg.new-password")]
    new_password: Option<PasswordType>,
}

rpc_toolkit::reflect_ts!(ResetPasswordParams);
rpc_toolkit::ts_export!(ResetPasswordParams, namespaces = [""]);

#[instrument(skip_all)]
pub(crate) async fn cli_reset_password(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        ..
    }: HandlerArgs<CliContext>,
) -> Result<(), RpcError> {
    let new_password = {
        let new_password = rpassword::prompt_password(&t!("auth.prompt-new-password"))?;
        if new_password != rpassword::prompt_password(&t!("auth.prompt-confirm"))? {
            return Err(Error::new(
                eyre!("{}", t!("auth.passwords-do-not-match")),
                crate::ErrorKind::IncorrectPassword,
            )
            .into());
        }
        new_password
    };

    ctx.call_remote::<RpcContext>(
        &parent_method.into_iter().chain(method).join("."),
        to_value(&ResetPasswordParams {
            new_password: Some(PasswordType::String(new_password)),
        })?,
    )
    .await?;

    Ok(())
}

#[instrument(skip_all)]
pub async fn reset_password_impl(
    ctx: RpcContext,
    ResetPasswordParams { new_password }: ResetPasswordParams,
) -> Result<(), Error> {
    let new_password = new_password.unwrap_or_default().decrypt(&ctx)?;

    let account = ctx.account.mutate(|account| {
        account.set_password(&new_password)?;
        Ok::<_, Error>(account.clone())
    })?;
    ctx.db.mutate(|d| account.save(d)).await.result?;
    write_shadow(&new_password).await?;
    Ok(())
}

#[instrument(skip_all)]
pub async fn get_pubkey(ctx: RpcContext) -> Result<Jwk, RpcError> {
    let secret = <RpcContext as AsRef<Jwk>>::as_ref(&ctx).clone();
    let pub_key = secret.to_public_key()?;
    Ok(pub_key)
}

#[cfg(test)]
mod password_snapshot_tests {
    use super::*;
    use crate::tunnel::context::TunnelContext;
    use crate::tunnel::db::TunnelDatabase;

    #[test]
    fn startos_password_replacement_invalidates_verified_snapshot() {
        let mut db = Model::<crate::db::model::Database>::from(imbl_value::ord_json!({
            "private": { "password": "verified" }
        }));
        let verified_hash = RpcContext::password_hash(&db).unwrap();
        check_verified_password::<RpcContext>(&db, &verified_hash).unwrap();
        db.as_private_mut()
            .as_password_mut()
            .ser(&"replacement".to_owned())
            .unwrap();
        assert_eq!(
            check_verified_password::<RpcContext>(&db, &verified_hash)
                .unwrap_err()
                .kind,
            ErrorKind::IncorrectPassword
        );
    }

    #[test]
    fn tunnel_password_replacement_or_removal_invalidates_verified_snapshot() {
        let mut db = Model::<TunnelDatabase>::from(imbl_value::ord_json!({
            "password": "verified"
        }));
        let verified_hash = TunnelContext::password_hash(&db).unwrap();
        check_verified_password::<TunnelContext>(&db, &verified_hash).unwrap();
        for password in [Some("replacement".to_owned()), None] {
            db.as_password_mut().ser(&password).unwrap();
            assert_eq!(
                check_verified_password::<TunnelContext>(&db, &verified_hash)
                    .unwrap_err()
                    .kind,
                ErrorKind::IncorrectPassword
            );
        }
    }
}

#[cfg(test)]
mod shadow_tests {
    use super::*;
    use crate::util::io::{TmpDir, write_file_atomic};

    #[tokio::test]
    async fn start9_password_hash_ignores_locked_and_unset_passwords() {
        for (field, expected) in [
            ("$6$salt$hash", Some("$6$salt$hash")),
            ("!$y$j9T$factory", None),
            ("!", None),
            ("*", None),
            ("", None),
        ] {
            let root = TmpDir::new().await.unwrap();
            write_file_atomic(
                root.join("etc/shadow"),
                format!("start9x:$6$other:1::::::\nstart9:{field}:19000:0:99999:7:::\n"),
            )
            .await
            .unwrap();
            assert_eq!(
                start9_password_hash(&root).await.unwrap().as_deref(),
                expected,
                "{field:?}",
            );
        }
    }
}
