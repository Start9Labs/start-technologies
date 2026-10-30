use std::borrow::{Borrow, BorrowMut};
use std::collections::BTreeMap;
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
use crate::middleware::auth::signature::{HasUnenrolledKeys, SignatureAuthContext};
use crate::prelude::*;
use crate::rpc_continuations::OpenAuthedContinuations;
use crate::sign::{AnyVerifyingKey, Signer};
use crate::util::crypto::EncryptedWire;
use crate::util::io::create_file_mod;
use crate::util::serde::{HandlerExtSerde, WithIoFormat, display_serializable};
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
/// the enrolled-key store that login writes and the session list reads.
pub trait LoginContext: SignatureAuthContext {
    /// The persisted enrolled-key store. Enrollment happens via login; never
    /// remove a key directly — unenrollment goes through
    /// [`SignatureAuthContext::unenroll`] so a revoked key's open
    /// continuations die with it.
    fn access_auth_keys(db: &mut Model<Self::Database>) -> &mut Model<AuthKeys>;
    /// The bearer-token store, keyed by token hash. `None` where tokens are unsupported.
    /// Never remove a token directly; [`revoke_tokens`] kills its continuations.
    #[allow(unused_variables)]
    fn access_tokens(db: &mut Model<Self::Database>) -> Option<&mut Model<TokenSessions>> {
        None
    }
    fn check_password(db: &Model<Self::Database>, password: &str) -> Result<(), Error>;
    /// Whether a sign-in has ended, judged by a trusted clock. Defaults to
    /// [`MAX_IDLE`] without use.
    fn sign_in_expired(session: &Session, now: DateTime<Utc>) -> bool {
        now - session.last_active > MAX_IDLE
    }
    #[allow(unused_variables)]
    fn post_login_hook(&self, password: &str) -> impl Future<Output = Result<(), Error>> + Send {
        async { Ok(()) }
    }
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
    fn check_password(db: &Model<Self::Database>, password: &str) -> Result<(), Error> {
        check_password(&db.as_private().as_password().de()?, password)
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

#[derive(Clone, Serialize, Deserialize, VisitVariants)]
#[serde(untagged)]
pub enum PasswordType {
    EncryptedWire(EncryptedWire),
    String(String),
}

rpc_toolkit::reflect_ts!(PasswordType);
rpc_toolkit::ts_export!(PasswordType, namespaces = [""]);
impl PasswordType {
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
impl std::fmt::Debug for PasswordType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<REDACTED_PASSWORD>")?;
        Ok(())
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
                .with_metadata("login", Value::Bool(true))
                .with_metadata(crate::mcp::MCP, Value::Bool(false))
                .with_metadata("get_signer", Value::Bool(true))
                .with_metadata("get_user_agent", Value::Bool(true))
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
                .with_metadata("get_signer", Value::Bool(true))
                .with_metadata(crate::mcp::MCP, Value::Bool(false))
                .no_display()
                .with_about("about.logout-current-auth-session")
                .with_call_remote::<CC>(),
        )
        .subcommand(
            "session",
            session::<C, AC, CC>().with_about("about.list-or-kill-auth-sessions"),
        )
}

/// Bearer-token commands; a token is a named session, listed and revoked with `auth.session`.
pub fn token<C: Context, AC: LoginContext>() -> ParentHandler<C>
where
    CliContext: CallRemote<AC>,
{
    ParentHandler::new().subcommand(
        "create",
        from_fn_async(create_token::<AC>)
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
    /// The session id the token authenticates as, for `auth.session.kill`.
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
/// [`SignatureAuthContext::LAST_ACTIVE_RESOLUTION`].
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
/// [`SignatureAuthContext::LAST_ACTIVE_RESOLUTION`].
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
    let password = if let Ok(password) = std::env::var("PASSWORD") {
        password
    } else {
        rpassword::prompt_password("Password: ")?
    };

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
        ephemeral,
    }: LoginParams,
) -> Result<(), Error> {
    if signer != pubkey {
        return Err(Error::new(
            eyre!("{}", t!("middleware.auth.enrolled-key-mismatch")),
            ErrorKind::InvalidRequest,
        ));
    }

    let now = Utc::now();
    let pubkey = pubkey.interned_der64();
    let record = Session {
        name: None,
        logged_in: now,
        last_active: now,
        user_agent,
    };
    if ephemeral {
        let Some(ephemeral_keys) = ctx.ephemeral_auth_keys() else {
            return Err(Error::new(
                eyre!("{}", t!("middleware.auth.ephemeral-unsupported")),
                ErrorKind::InvalidRequest,
            ));
        };
        C::check_password(&ctx.db().peek().await, &password)?;
        ephemeral_keys.mutate(|keys| {
            keys.0.insert(pubkey.clone(), record);
        });
    } else {
        ctx.db()
            .mutate(|db| {
                C::check_password(db, &password)?;
                C::access_auth_keys(db).insert(&pubkey, &record)?;
                Ok(())
            })
            .await
            .result?;
    }

    ctx.post_login_hook(&password).await?;

    Ok(())
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

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SessionList {
    current: Option<InternedString>,
    sessions: Sessions,
}

rpc_toolkit::reflect_ts!(SessionList);
rpc_toolkit::ts_export!(SessionList, namespaces = [""]);

pub fn session<C: Context, AC: LoginContext, CC: CallRemote<AC>>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async(list::<AC>)
                .with_metadata("get_signer", Value::Bool(true))
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| display_sessions(handle.params, result))
                .with_about("about.display-all-auth-sessions")
                .with_call_remote::<CC>(),
        )
        .subcommand(
            "kill",
            from_fn_async(kill::<AC>)
                .no_display()
                .with_about("about.terminate-auth-sessions")
                .with_call_remote::<CC>(),
        )
}

fn display_sessions(params: WithIoFormat<ListParams>, arg: SessionList) -> Result<(), Error> {
    use prettytable::*;

    if let Some(format) = params.format {
        return display_serializable(format, arg);
    }

    let mut table = Table::new();
    table.add_row(row![bc =>
        "TYPE",
        "ID",
        "NAME",
        "LOGGED IN",
        "LAST ACTIVE",
        "USER AGENT",
    ]);
    let pubkeys = arg.sessions.pubkeys.0.into_iter().map(|s| ("pubkey", s));
    let tokens = arg.sessions.tokens.into_iter().map(|s| ("token", s));
    for (kind, (id, session)) in pubkeys.chain(tokens) {
        let mut row = row![
            kind,
            &id,
            session.name.as_deref().unwrap_or("N/A"),
            &format!("{}", session.logged_in),
            &format!("{}", session.last_active),
            session.user_agent.as_deref().unwrap_or("N/A"),
        ];
        if Some(id) == arg.current {
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
pub struct ListParams {
    #[arg(skip)]
    #[visit(ts(skip))]
    #[serde(rename = "__Auth_signer")]
    signer: Option<InternedString>,
}

rpc_toolkit::reflect_ts!(ListParams);

#[instrument(skip_all)]
pub async fn list<C: LoginContext>(
    ctx: C,
    ListParams { signer }: ListParams,
) -> Result<SessionList, Error> {
    let mut db = ctx.db().peek().await;
    let mut pubkeys = C::access_auth_keys(&mut db).de()?;
    if let Some(ephemeral) = ctx.ephemeral_auth_keys() {
        ephemeral.peek(|e| {
            pubkeys
                .0
                .extend(e.0.iter().map(|(k, v)| (k.clone(), v.clone())))
        });
    }
    let tokens = C::access_tokens(&mut db)
        .map(|tokens| tokens.de())
        .transpose()?
        .unwrap_or_default();
    Ok(SessionList {
        current: signer,
        sessions: Sessions { pubkeys, tokens },
    })
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct KillParams {
    #[arg(long = "pubkey", help = "help.arg.session-pubkeys")]
    #[serde(default)]
    pubkeys: Vec<AnyVerifyingKey>,
    #[arg(long = "token", help = "help.arg.session-tokens")]
    #[serde(default)]
    tokens: Vec<InternedString>,
}

rpc_toolkit::reflect_ts!(KillParams);
rpc_toolkit::ts_export!(KillParams, namespaces = [""]);

#[instrument(skip_all)]
pub async fn kill<C: LoginContext>(
    ctx: C,
    KillParams { pubkeys, tokens }: KillParams,
) -> Result<(), Error> {
    ctx.unenroll(pubkeys.iter().map(AnyVerifyingKey::interned_der64))
        .await?;
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
