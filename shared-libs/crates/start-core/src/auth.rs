use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use clap::Parser;
use color_eyre::eyre::eyre;
use imbl_value::{InternedString, json};
use itertools::Itertools;
use josekit::jwk::Jwk;
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{CallRemote, Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tracing::instrument;
use ts_rs::TS;

use crate::context::{CliContext, RpcContext};
use crate::middleware::auth::signature::{HasUnenrolledKeys, SignatureAuthContext};
use crate::prelude::*;
use crate::sign::AnyVerifyingKey;
use crate::util::crypto::EncryptedWire;
use crate::util::io::create_file_mod;
use crate::util::serde::{HandlerExtSerde, WithIoFormat, display_serializable};
use crate::util::sync::SyncMutex;
use crate::{Error, ResultExt, ensure_code};

/// The server's enrolled auth keys, keyed by their PEM encoding. Each enrolled
/// key is a sign-in: it carries the same metadata a session used to (when it
/// was created, when it was last used, and the user agent that enrolled it).
#[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
pub struct AuthKeys(pub BTreeMap<InternedString, Session>);
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

/// Contexts where a password can enroll an auth key (login), plus access to
/// the enrolled-key store that login writes and the session list reads.
pub trait LoginContext: SignatureAuthContext {
    /// The persisted enrolled-key store. Enrollment happens via login; never
    /// remove a key directly — unenrollment goes through
    /// [`SignatureAuthContext::unenroll`] so a revoked key's open
    /// continuations die with it.
    fn access_auth_keys(db: &mut Model<Self::Database>) -> &mut Model<AuthKeys>;
    fn check_password(db: &Model<Self::Database>, password: &str) -> Result<(), Error>;
    #[allow(unused_variables)]
    fn post_login_hook(&self, password: &str) -> impl Future<Output = Result<(), Error>> + Send {
        async { Ok(()) }
    }
}
impl LoginContext for RpcContext {
    fn access_auth_keys(db: &mut Model<Self::Database>) -> &mut Model<AuthKeys> {
        db.as_private_mut().as_session_pubkeys_mut()
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
    Ok(())
}

#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum PasswordType {
    EncryptedWire(EncryptedWire),
    String(String),
}
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
pub fn auth<C: Context, AC: LoginContext>() -> ParentHandler<C>
where
    CliContext: CallRemote<AC>,
{
    ParentHandler::new()
        .subcommand(
            "login",
            from_fn_async(login_impl::<AC>)
                .with_metadata("login", Value::Bool(true))
                .with_metadata("get_signer", Value::Bool(true))
                .with_metadata("get_user_agent", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "login",
            from_fn_async(cli_login::<AC>)
                .no_display()
                .with_about("about.login-new-auth-session"),
        )
        .subcommand(
            "logout",
            from_fn_async(logout::<AC>)
                .with_metadata("get_signer", Value::Bool(true))
                .no_display()
                .with_about("about.logout-current-auth-session")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "session",
            session::<C, AC>().with_about("about.list-or-kill-auth-sessions"),
        )
        .subcommand(
            "reset-password",
            from_fn_async(reset_password_impl).no_cli(),
        )
        .subcommand(
            "reset-password",
            from_fn_async(cli_reset_password)
                .no_display()
                .with_about("about.reset-password"),
        )
        .subcommand(
            "get-pubkey",
            from_fn_async(get_pubkey)
                .with_metadata("authenticated", Value::Bool(false))
                .no_display()
                .with_about("about.get-pubkey-from-server")
                .with_call_remote::<CliContext>(),
        )
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
async fn cli_login<C: LoginContext>(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        ..
    }: HandlerArgs<CliContext>,
) -> Result<(), RpcError>
where
    CliContext: CallRemote<C>,
{
    let password = if let Ok(password) = std::env::var("PASSWORD") {
        password
    } else {
        rpassword::prompt_password("Password: ")?
    };

    if ctx.id_key().is_err() {
        let secret = ed25519_dalek::SigningKey::generate(&mut crate::util::crypto::os_rng());
        crate::developer::write_signing_key(&secret, &ctx.id_key_path).await?;
    }
    let pubkey = ctx
        .id_key()
        .map(|k| AnyVerifyingKey::Ed25519(k.into()).to_string())?;

    ctx.call_remote::<C>(
        &parent_method.into_iter().chain(method).join("."),
        json!({
            "password": password,
            "pubkey": pubkey,
            "metadata": {
                "platforms": ["cli"],
            },
        }),
    )
    .await?;

    Ok(())
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

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LoginParams {
    password: String,
    #[ts(skip)]
    #[serde(rename = "__Auth_userAgent")] // from Auth middleware
    user_agent: Option<String>,
    /// The PEM-encoded public key to enroll on a successful login. The login
    /// request itself is signed with the matching secret key, so enrollment
    /// proves possession.
    pubkey: AnyVerifyingKey,
    /// The key the request was actually signed with, injected by the auth
    /// middleware. Enforced to equal `pubkey`, so a login can only enroll the
    /// key that proved possession.
    #[ts(skip)]
    #[serde(rename = "__Auth_signer")] // from Auth middleware
    signer: AnyVerifyingKey,
    /// Enroll in memory only, never persisted (kiosk mode, which re-enrolls
    /// on every browser restart and would otherwise accumulate keys).
    #[serde(default)]
    ephemeral: bool,
}

const LOGIN_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(20);
const LOGIN_RATE_LIMIT_MAX_ATTEMPTS: usize = 3;
static LOGIN_RATE_LIMITER: LazyLock<SyncMutex<(usize, Instant)>> =
    LazyLock::new(|| SyncMutex::new((0, Instant::now())));

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

    LOGIN_RATE_LIMITER.mutate(|(count, time)| {
        if time.elapsed() >= LOGIN_RATE_LIMIT_WINDOW {
            *count = 0;
        }
        if *count >= LOGIN_RATE_LIMIT_MAX_ATTEMPTS {
            Err(Error::new(
                eyre!("{}", t!("middleware.auth.rate-limited-login")),
                ErrorKind::RateLimited,
            ))
        } else {
            *count += 1;
            *time = Instant::now();
            Ok(())
        }
    })?;

    let now = Utc::now();
    let pubkey = pubkey.to_string();
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
            keys.0.insert(InternedString::intern(&pubkey), record);
        });
    } else {
        ctx.db()
            .mutate(|db| {
                C::check_password(db, &password)?;
                C::access_auth_keys(db).insert(&InternedString::intern(&pubkey), &record)?;
                Ok(())
            })
            .await
            .result?;
    }

    ctx.post_login_hook(&password).await?;

    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct LogoutParams {
    #[ts(skip)]
    #[serde(rename = "__Auth_signer")] // from Auth middleware
    signer: InternedString,
}

pub async fn logout<C: SignatureAuthContext>(
    ctx: C,
    LogoutParams { signer }: LogoutParams,
) -> Result<Option<HasUnenrolledKeys>, Error> {
    Ok(Some(ctx.unenroll([signer]).await?))
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Session {
    /// A friendly name for the key, if one was assigned at enrollment (e.g.
    /// tunnel device keys). UI-enrolled keys are unnamed.
    #[serde(default)]
    pub name: Option<InternedString>,
    #[serde(default)]
    #[ts(type = "string")]
    pub logged_in: DateTime<Utc>,
    #[serde(default)]
    #[ts(type = "string")]
    pub last_active: DateTime<Utc>,
    #[serde(default)]
    pub user_agent: Option<String>,
}

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SessionList {
    #[ts(type = "string | null")]
    current: Option<InternedString>,
    sessions: AuthKeys,
}

pub fn session<C: Context, AC: LoginContext>() -> ParentHandler<C>
where
    CliContext: CallRemote<AC>,
{
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async(list::<AC>)
                .with_metadata("get_signer", Value::Bool(true))
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| display_sessions(handle.params, result))
                .with_about("about.display-all-auth-sessions")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "kill",
            from_fn_async(kill::<AC>)
                .no_display()
                .with_about("about.terminate-auth-sessions")
                .with_call_remote::<CliContext>(),
        )
}

fn display_sessions(params: WithIoFormat<ListParams>, arg: SessionList) -> Result<(), Error> {
    use prettytable::*;

    if let Some(format) = params.format {
        return display_serializable(format, arg);
    }

    let mut table = Table::new();
    table.add_row(row![bc =>
        "ID",
        "LOGGED IN",
        "LAST ACTIVE",
        "USER AGENT",
    ]);
    for (id, session) in arg.sessions.0 {
        let mut row = row![
            &id,
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
    table.print_tty(false)?;
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ListParams {
    #[arg(skip)]
    #[ts(skip)]
    #[serde(rename = "__Auth_signer")] // from Auth middleware
    signer: Option<InternedString>,
}

// #[command(display(display_sessions))]
#[instrument(skip_all)]
pub async fn list<C: LoginContext>(
    ctx: C,
    ListParams { signer }: ListParams,
) -> Result<SessionList, Error> {
    let mut sessions = C::access_auth_keys(&mut ctx.db().peek().await).de()?;
    if let Some(ephemeral) = ctx.ephemeral_auth_keys() {
        ephemeral.peek(|e| {
            sessions
                .0
                .extend(e.0.iter().map(|(k, v)| (k.clone(), v.clone())))
        });
    }
    Ok(SessionList {
        current: signer,
        sessions,
    })
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct KillParams {
    #[arg(help = "help.arg.session-ids")]
    ids: Vec<String>,
}

#[instrument(skip_all)]
pub async fn kill<C: SignatureAuthContext>(
    ctx: C,
    KillParams { ids }: KillParams,
) -> Result<(), Error> {
    ctx.unenroll(ids.into_iter().map(InternedString::from))
        .await?;
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ResetPasswordParams {
    #[arg(help = "help.arg.old-password")]
    old_password: Option<PasswordType>,
    #[arg(help = "help.arg.new-password")]
    new_password: Option<PasswordType>,
}

#[instrument(skip_all)]
async fn cli_reset_password(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        ..
    }: HandlerArgs<CliContext>,
) -> Result<(), RpcError> {
    let old_password = rpassword::prompt_password(&t!("auth.prompt-current-password"))?;

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
        imbl_value::json!({ "old-password": old_password, "new-password": new_password }),
    )
    .await?;

    Ok(())
}

#[instrument(skip_all)]
pub async fn reset_password_impl(
    ctx: RpcContext,
    ResetPasswordParams {
        old_password,
        new_password,
    }: ResetPasswordParams,
) -> Result<(), Error> {
    let old_password = old_password.unwrap_or_default().decrypt(&ctx)?;
    let new_password = new_password.unwrap_or_default().decrypt(&ctx)?;

    let account = ctx.account.mutate(|account| {
        if !argon2::verify_encoded(&account.password, old_password.as_bytes())
            .with_kind(crate::ErrorKind::IncorrectPassword)?
        {
            return Err(Error::new(
                eyre!("{}", t!("auth.password-incorrect")),
                crate::ErrorKind::IncorrectPassword,
            ));
        }
        account.set_password(&new_password)?;
        Ok(account.clone())
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
