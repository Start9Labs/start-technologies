use std::collections::BTreeMap;

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
use crate::middleware::auth::session::{
    AsLogoutSessionId, HasLoggedOutSessions, HashSessionToken, LoginRes, SessionAuthContext,
};
use crate::prelude::*;
use crate::util::crypto::EncryptedWire;
use crate::util::io::create_file_mod;
use crate::util::serde::{HandlerExtSerde, WithIoFormat, display_serializable};
use crate::{Error, ResultExt, ensure_code};

#[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
pub struct Sessions(pub BTreeMap<InternedString, Session>);
impl Sessions {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }
}
impl Map for Sessions {
    type Key = InternedString;
    type Value = Session;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Ok(key)
    }
    fn key_string(key: &Self::Key) -> Result<InternedString, Error> {
        Ok(key.clone())
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
pub fn auth<C: Context, AC: SessionAuthContext>() -> ParentHandler<C>
where
    CliContext: CallRemote<AC>,
{
    ParentHandler::new()
        .subcommand(
            "login",
            from_fn_async(login_impl::<AC>)
                .with_metadata("login", Value::Bool(true))
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
                .with_metadata("get_session", Value::Bool(true))
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
async fn cli_login<C: SessionAuthContext>(
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

    ctx.call_remote::<C>(
        &parent_method.into_iter().chain(method).join("."),
        json!({
            "password": password,
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
    #[serde(default)]
    ephemeral: bool,
}

#[instrument(skip_all)]
pub async fn login_impl<C: SessionAuthContext>(
    ctx: C,
    LoginParams {
        password,
        user_agent,
        ephemeral,
    }: LoginParams,
) -> Result<LoginRes, Error> {
    let tok = if ephemeral {
        C::check_password(&ctx.db().peek().await, &password)?;
        let hash_token = HashSessionToken::new();
        ctx.ephemeral_sessions().mutate(|s| {
            s.0.insert(
                hash_token.hashed().clone(),
                Session {
                    logged_in: Utc::now(),
                    last_active: Utc::now(),
                    user_agent,
                },
            )
        });
        Ok(hash_token.to_login_res())
    } else {
        ctx.db()
            .mutate(|db| {
                C::check_password(db, &password)?;
                let hash_token = HashSessionToken::new();
                C::access_sessions(db).insert(
                    hash_token.hashed(),
                    &Session {
                        logged_in: Utc::now(),
                        last_active: Utc::now(),
                        user_agent,
                    },
                )?;

                Ok(hash_token.to_login_res())
            })
            .await
            .result
    }?;

    ctx.post_login_hook(&password).await?;

    Ok(tok)
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct LogoutParams {
    #[ts(skip)]
    #[serde(rename = "__Auth_session")] // from Auth middleware
    session: InternedString,
}

pub async fn logout<C: SessionAuthContext>(
    ctx: C,
    LogoutParams { session }: LogoutParams,
) -> Result<Option<HasLoggedOutSessions>, Error> {
    Ok(Some(
        HasLoggedOutSessions::new(vec![HashSessionToken::from_token(session)], &ctx).await?,
    ))
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Session {
    #[ts(type = "string")]
    pub logged_in: DateTime<Utc>,
    #[ts(type = "string")]
    pub last_active: DateTime<Utc>,
    pub user_agent: Option<String>,
}

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SessionList {
    #[ts(type = "string | null")]
    current: Option<InternedString>,
    sessions: Sessions,
}

pub fn session<C: Context, AC: SessionAuthContext>() -> ParentHandler<C>
where
    CliContext: CallRemote<AC>,
{
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async(list::<AC>)
                .with_metadata("get_session", Value::Bool(true))
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
    #[serde(rename = "__Auth_session")] // from Auth middleware
    session: Option<InternedString>,
}

// #[command(display(display_sessions))]
#[instrument(skip_all)]
pub async fn list<C: SessionAuthContext>(
    ctx: C,
    ListParams { session, .. }: ListParams,
) -> Result<SessionList, Error> {
    let mut sessions = C::access_sessions(&mut ctx.db().peek().await).de()?;
    ctx.ephemeral_sessions().peek(|s| {
        sessions
            .0
            .extend(s.0.iter().map(|(k, v)| (k.clone(), v.clone())))
    });
    Ok(SessionList {
        current: session,
        sessions,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KillSessionId(InternedString);

impl KillSessionId {
    fn new(id: String) -> Self {
        Self(InternedString::from(id))
    }
}

impl AsLogoutSessionId for KillSessionId {
    fn as_logout_session_id(self) -> InternedString {
        self.0
    }
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
pub async fn kill<C: SessionAuthContext>(
    ctx: C,
    KillParams { ids }: KillParams,
) -> Result<(), Error> {
    HasLoggedOutSessions::new(ids.into_iter().map(KillSessionId::new), &ctx).await?;
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
