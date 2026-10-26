use std::collections::BTreeSet;
use std::net::IpAddr;
use std::path::Path;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, TimeDelta, Utc};
use clap::Parser;
use imbl_value::imbl::OrdMap;
use imbl_value::{json, InternedString, Value};
use itertools::Itertools;
use patch_db::value::index::Index;
use patch_db::{HasModel, ModelExt, PatchDb};
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{from_fn_async, CallRemote, Empty, HandlerArgs, HandlerExt, ParentHandler};
use serde::{Deserialize, Serialize};
use startos::auth::{AuthKeys, LoginContext, Session};
use startos::middleware::auth::local::LocalAuthContext;
use startos::middleware::auth::signature::{
    check_enrolled, url_host_str, HasUnenrolledKeys, LoginMetadata, NonceCache,
    SignatureAuthContext,
};
use startos::middleware::auth::DbContext;
use startos::prelude::{Model, TypedPatchDb};
use startos::rpc_continuations::OpenAuthedContinuations;
use startos::sign::AnyVerifyingKey;
use startos::util::sync::SyncMutex;
use tokio::io::AsyncWriteExt;
use tracing::instrument;

use crate::prelude::*;
use crate::utils::DeserializeStdin;
use crate::{captive, CliContext, CtrlContext, ServerContext};

pub const LOCAL_AUTH_COOKIE_PATH: &str = "/run/startwrt/rpc.authcookie";

pub(crate) const AUTH_DB_PATH: &str = "/etc/startwrt/auth.db";

const SHADOW_PATH: &str = "/etc/shadow";

/// Written by dnsmasq's ntp hotplug once ntpd reports a valid time.
const TIME_VALID_PATH: &str = "/var/state/dnsmasqsec";

const EXPIRED_KEY_SWEEP: Duration = Duration::from_secs(60 * 60);

/// A sign-in ends this long after login, however often it is used.
const SIGN_IN_LIFETIME: TimeDelta = TimeDelta::hours(24);

/// How far ahead of a trusted clock a login time may lie.
const LOGIN_CLOCK_SKEW: TimeDelta = TimeDelta::minutes(5);

/// A clock not yet trusted judges age alone.
fn sign_in_ended(logged_in: DateTime<Utc>, now: DateTime<Utc>, clock_trusted: bool) -> bool {
    now - logged_in > SIGN_IN_LIFETIME || (clock_trusted && logged_in - now > LOGIN_CLOCK_SKEW)
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthDatabase {
    #[serde(default)]
    pub session_pubkeys: AuthKeys,
}
impl HasModel for AuthDatabase {
    type Model = Model<Self>;
}

trait AuthDatabaseModel {
    fn as_session_pubkeys(&self) -> &Model<AuthKeys>;
    fn as_session_pubkeys_mut(&mut self) -> &mut Model<AuthKeys>;
}
impl AuthDatabaseModel for Model<AuthDatabase> {
    fn as_session_pubkeys(&self) -> &Model<AuthKeys> {
        self.transmute_ref(|v| {
            SESSION_PUBKEYS
                .index_into(v)
                .unwrap_or(&patch_db::value::NULL)
        })
    }
    fn as_session_pubkeys_mut(&mut self) -> &mut Model<AuthKeys> {
        self.transmute_mut(|v| SESSION_PUBKEYS.index_or_insert(v))
    }
}
const SESSION_PUBKEYS: &str = "sessionPubkeys";

/// The enabled DDNS hostname as of the ddns config's modification time.
#[derive(Default)]
struct DdnsIdentity {
    modified: Option<SystemTime>,
    hostname: Option<InternedString>,
}

pub struct AuthState {
    db: TypedPatchDb<AuthDatabase>,
    nonce_cache: SyncMutex<NonceCache>,
    open_authed_continuations: OpenAuthedContinuations<Option<InternedString>>,
    ddns: SyncMutex<DdnsIdentity>,
}

impl AuthState {
    /// Enrolls nothing and persists nothing.
    pub fn in_memory() -> Self {
        Self::new(TypedPatchDb::load_unchecked(PatchDb::in_memory()))
    }

    pub async fn open() -> Result<Self, Error> {
        if let Some(dir) = Path::new(AUTH_DB_PATH).parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        let db = PatchDb::open(AUTH_DB_PATH).await.map_err(|e| {
            Error::new(
                eyre!("Failed to open {AUTH_DB_PATH}: {e}"),
                ErrorKind::Filesystem,
            )
        })?;
        Ok(Self::new(
            TypedPatchDb::load_or_init(db, || async { Ok(AuthDatabase::default()) }).await?,
        ))
    }

    fn new(db: TypedPatchDb<AuthDatabase>) -> Self {
        Self {
            db,
            nonce_cache: SyncMutex::new(NonceCache::default()),
            open_authed_continuations: OpenAuthedContinuations::new(),
            ddns: SyncMutex::new(DdnsIdentity::default()),
        }
    }
}

impl ServerContext {
    /// Re-reads the ddns config only when its modification time changes.
    pub(crate) async fn ddns_identity(&self) -> Option<InternedString> {
        let path = self.uci_root().join("ddns");
        let modified = tokio::fs::metadata(&path)
            .await
            .ok()
            .and_then(|meta| meta.modified().ok());
        let cached = self
            .auth
            .ddns
            .peek(|cache| (cache.modified == modified).then(|| cache.hostname.clone()));
        if let Some(hostname) = cached {
            return hostname;
        }
        let hostname = crate::wan::ddns_get(self.clone())
            .await
            .ok()
            .and_then(enabled_ddns_hostname);
        self.auth.ddns.mutate(|cache| {
            *cache = DdnsIdentity {
                modified,
                hostname: hostname.clone(),
            }
        });
        hostname
    }
}

impl AsRef<OpenAuthedContinuations<Option<InternedString>>> for ServerContext {
    fn as_ref(&self) -> &OpenAuthedContinuations<Option<InternedString>> {
        &self.auth.open_authed_continuations
    }
}

impl DbContext for ServerContext {
    type Database = AuthDatabase;
    fn db(&self) -> &TypedPatchDb<Self::Database> {
        &self.auth.db
    }
}

impl SignatureAuthContext for ServerContext {
    type AdditionalMetadata = LoginMetadata;
    type CheckPubkeyRes = Option<InternedString>;
    /// Routine requests never write to flash.
    const LAST_ACTIVE_RESOLUTION: TimeDelta = TimeDelta::hours(1);
    fn mutate_nonce_cache<F: FnOnce(&mut NonceCache) -> T, T>(&self, f: F) -> T {
        self.auth.nonce_cache.mutate(f)
    }
    async fn clock_synced(&self) -> bool {
        tokio::fs::metadata(TIME_VALID_PATH).await.is_ok()
    }
    fn open_authed_continuations(&self) -> &OpenAuthedContinuations<Option<InternedString>> {
        &self.auth.open_authed_continuations
    }
    fn remove_enrolled_keys(
        db: &mut Model<Self::Database>,
        keys: &BTreeSet<InternedString>,
    ) -> Result<(), startos::Error> {
        let auth_keys = db.as_session_pubkeys_mut();
        for key in keys {
            auth_keys.remove(key)?;
        }
        Ok(())
    }
    async fn sig_context(
        &self,
        _: &http::Extensions,
    ) -> impl IntoIterator<Item = Result<impl AsRef<str> + Send, startos::Error>> + Send {
        signing_identities(interface_addresses(), self.ddns_identity().await)
            .into_iter()
            .map(Ok)
    }
    fn check_pubkey(
        &self,
        db: &Model<Self::Database>,
        pubkey: Option<&AnyVerifyingKey>,
        metadata: Self::AdditionalMetadata,
    ) -> Result<Self::CheckPubkeyRes, startos::Error> {
        let now = Utc::now();
        let clock_trusted = Path::new(TIME_VALID_PATH).exists();
        check_enrolled(pubkey, metadata.login, |key| {
            Ok(db
                .as_session_pubkeys()
                .de()?
                .0
                .get(&**key)
                .is_some_and(|session| !sign_in_ended(session.logged_in, now, clock_trusted)))
        })
    }
    async fn post_auth_hook(
        &self,
        key: Self::CheckPubkeyRes,
        _: &rpc_toolkit::RpcRequest,
    ) -> Result<(), startos::Error> {
        if let Some(key) = key {
            startos::auth::touch_last_active(self, &key).await?;
        }
        Ok(())
    }
}

impl LocalAuthContext for ServerContext {
    const LOCAL_AUTH_COOKIE_PATH: &str = LOCAL_AUTH_COOKIE_PATH;
    const LOCAL_AUTH_COOKIE_OWNERSHIP: &str = "root:root";
}

impl LoginContext for ServerContext {
    fn access_auth_keys(db: &mut Model<Self::Database>) -> &mut Model<AuthKeys> {
        db.as_session_pubkeys_mut()
    }
    fn check_password(_: &Model<Self::Database>, password: &str) -> Result<(), startos::Error> {
        check_password_sync(Path::new(SHADOW_PATH), password).map_err(|e| {
            crate::activity::log("auth", "login", false, "Login failed", Some(&e.to_string()));
            e.into()
        })
    }
    fn sign_in_expired(session: &Session, now: DateTime<Utc>) -> bool {
        sign_in_ended(session.logged_in, now, true)
    }
    async fn post_login_hook(&self, _: &str) -> Result<(), startos::Error> {
        crate::activity::log("auth", "login", true, "Logged in", None);
        Ok(())
    }
}

/// Unenrolls expired keys for the daemon's lifetime.
pub async fn reap_idle_keys(ctx: ServerContext) {
    loop {
        if let Err(e) = startos::auth::reap_idle_keys(&ctx).await {
            tracing::error!("idle key sweep failed: {e}");
        }
        tokio::time::sleep(EXPIRED_KEY_SWEEP).await;
    }
}

/// Lowercased; `None` while DDNS is disabled.
fn enabled_ddns_hostname(ddns: crate::wan::WanDdnsResponse) -> Option<InternedString> {
    ddns.enabled
        .then_some(ddns.hostname)
        .flatten()
        .map(|hostname| InternedString::intern(hostname.to_ascii_lowercase()))
}

/// Every name and address a browser may sign for.
fn signing_identities(
    addresses: impl IntoIterator<Item = IpAddr>,
    ddns: Option<InternedString>,
) -> BTreeSet<InternedString> {
    std::iter::once(InternedString::intern(crate::ssl::ROUTER_HOSTNAME))
        .chain(addresses.into_iter().map(url_host_str))
        .chain(ddns)
        .collect()
}

/// Excludes loopback, which start-core grants to loopback peers alone, and
/// link-local IPv6, which a URL can only name with a zone.
fn interface_addresses() -> Vec<IpAddr> {
    let Ok(addrs) = nix::ifaddrs::getifaddrs() else {
        return Vec::new();
    };
    addrs
        .filter_map(|ifaddr| {
            let addr = ifaddr.address?;
            if let Some(v4) = addr.as_sockaddr_in() {
                Some(IpAddr::V4(v4.ip()))
            } else {
                addr.as_sockaddr_in6()
                    .map(|v6| v6.ip())
                    .filter(|ip| !ip.is_unicast_link_local())
                    .map(IpAddr::V6)
            }
        })
        .filter(|ip| !ip.is_unspecified() && !ip.is_loopback())
        .collect()
}

/// `None` when the account has no password or is locked.
fn get_shadow_hash(shadow: &Path, username: &str) -> Result<Option<String>, Error> {
    let shadow_content = std::fs::read_to_string(shadow).map_err(|e| {
        Error::new(
            eyre!("Failed to read {}: {}", shadow.display(), e),
            ErrorKind::Filesystem,
        )
    })?;

    for line in shadow_content.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 2 && parts[0] == username {
            let hash = parts[1];
            if hash.is_empty() || hash == "*" || hash == "!" || hash == "x" {
                return Ok(None);
            }
            return Ok(Some(hash.to_string()));
        }
    }

    Ok(None)
}

async fn get_root_hash(shadow: &Path) -> Result<Option<String>, Error> {
    let shadow = shadow.to_owned();
    tokio::task::spawn_blocking(move || get_shadow_hash(&shadow, "root")).await?
}

const MIN_PASSWORD_LEN: usize = 12;

fn validate_password_length(password: &str) -> Result<(), Error> {
    if password.len() < MIN_PASSWORD_LEN {
        return Err(Error::new(
            eyre!("password must be at least {MIN_PASSWORD_LEN} characters"),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

/// `STARTWRT_DEV_PASSWORD` replaces the shadow check for development without root.
fn check_password_sync(shadow: &Path, password: &str) -> Result<(), Error> {
    if let Ok(dev_password) = std::env::var("STARTWRT_DEV_PASSWORD") {
        tracing::warn!("DEV MODE: Using development password authentication");
        if password == dev_password {
            return Ok(());
        }
        return Err(Error::new(
            eyre!("Incorrect password"),
            ErrorKind::IncorrectPassword,
        ));
    }

    let Some(hash) = get_shadow_hash(shadow, "root")? else {
        return Err(Error::new(
            eyre!("admin password is not set"),
            ErrorKind::Uninitialized,
        ));
    };
    if pwhash::unix::verify(password, &hash) {
        Ok(())
    } else {
        Err(Error::new(
            eyre!("Incorrect password"),
            ErrorKind::IncorrectPassword,
        ))
    }
}

pub async fn check_password(shadow: &Path, password: &str) -> Result<(), Error> {
    let shadow = shadow.to_owned();
    let password = password.to_owned();
    tokio::task::spawn_blocking(move || check_password_sync(&shadow, &password)).await?
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct VerifyPasswordParams {
    pub password: String,
}
rpc_toolkit::reflect_ts!(VerifyPasswordParams);

#[instrument(skip_all)]
pub async fn verify_password_impl(
    _ctx: ServerContext,
    VerifyPasswordParams { password }: VerifyPasswordParams,
) -> Result<(), Error> {
    check_password(Path::new(SHADOW_PATH), &password).await
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct ResetPasswordParams {
    pub old_password: String,
    pub new_password: String,
    #[arg(skip)]
    #[visit(ts(skip))]
    #[serde(rename = "__Auth_signer")]
    pub signer: Option<InternedString>,
}
rpc_toolkit::reflect_ts!(ResetPasswordParams);

async fn update_shadow_hash(
    shadow_path: &Path,
    username: &str,
    new_hash: &str,
) -> Result<(), Error> {
    let shadow_content = tokio::fs::read_to_string(shadow_path).await.map_err(|e| {
        Error::new(
            eyre!("Failed to read {}: {e}", shadow_path.display()),
            ErrorKind::Filesystem,
        )
    })?;

    let mut found = false;
    let new_content = shadow_content
        .lines()
        .map(|line| {
            if line.starts_with(&format!("{username}:")) {
                found = true;
                let mut parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    parts[1] = new_hash;
                }
                parts.join(":")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if !found {
        return Err(Error::new(
            eyre!("User '{username}' not found in /etc/shadow"),
            ErrorKind::NotFound,
        ));
    }

    let new_content = if new_content.ends_with('\n') {
        new_content
    } else {
        format!("{new_content}\n")
    };

    use std::os::unix::fs::PermissionsExt;
    let mut file = startos::util::io::AtomicFile::new(shadow_path, None::<&Path>)
        .await
        .map_err(Error::from)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .await
        .map_err(|e| {
            Error::new(
                eyre!("Failed to set shadow file permissions: {e}"),
                ErrorKind::Filesystem,
            )
        })?;
    file.write_all(new_content.as_bytes()).await.map_err(|e| {
        Error::new(
            eyre!("Failed to write shadow file: {e}"),
            ErrorKind::Filesystem,
        )
    })?;
    file.save().await.map_err(Error::from)?;

    Ok(())
}

async fn unenroll_all_except(
    ctx: &ServerContext,
    keep: Option<&InternedString>,
) -> Result<(), Error> {
    let continuations = ctx.open_authed_continuations();
    let ephemeral = ctx.ephemeral_auth_keys();
    ctx.auth
        .db
        .mutate(|db| {
            let revoked = db
                .as_session_pubkeys()
                .de()?
                .0
                .into_keys()
                .filter(|key| Some(key) != keep);
            HasUnenrolledKeys::unenroll::<ServerContext>(continuations, ephemeral, db, revoked)
        })
        .await
        .result?;
    Ok(())
}

#[instrument(skip_all)]
pub async fn reset_password_impl(
    ctx: ServerContext,
    params: ResetPasswordParams,
) -> Result<(), Error> {
    reset_password(&ctx, Path::new(SHADOW_PATH), params).await
}

/// Keeps the requesting key enrolled and unenrolls every other.
async fn reset_password(
    ctx: &ServerContext,
    shadow: &Path,
    ResetPasswordParams {
        old_password,
        new_password,
        signer,
    }: ResetPasswordParams,
) -> Result<(), Error> {
    check_password(shadow, &old_password).await?;
    validate_password_length(&new_password)?;

    let new_hash = pwhash::sha512_crypt::hash(&new_password).map_err(|e| {
        Error::new(
            eyre!("Failed to hash password: {e}"),
            ErrorKind::PasswordHashGeneration,
        )
    })?;

    let result = update_shadow_hash(shadow, "root", &new_hash).await;
    match &result {
        Ok(()) => crate::activity::log(
            "auth",
            "password-changed",
            true,
            "Changed admin password",
            None,
        ),
        Err(e) => crate::activity::log(
            "auth",
            "password-changed",
            false,
            "Failed to change admin password",
            Some(&e.to_string()),
        ),
    }
    result?;
    unenroll_all_except(ctx, signer.as_ref()).await
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
    let old_password = rpassword::prompt_password("Current Password: ")?;

    ctx.call_remote(
        "auth.verify-password",
        OrdMap::new(),
        json!({ "password": old_password }),
        Empty {},
    )
    .await?;

    let new_password = rpassword::prompt_password("New Password: ")?;
    let confirm = rpassword::prompt_password("Confirm Password: ")?;

    if new_password != confirm {
        return Err(Error::new(eyre!("Passwords do not match"), ErrorKind::InvalidRequest).into());
    }

    ctx.call_remote(
        &parent_method.into_iter().chain(method).join("."),
        OrdMap::new(),
        json!({
            "oldPassword": old_password,
            "newPassword": new_password,
        }),
        Empty {},
    )
    .await?;

    println!("Password changed successfully.");
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct CheckInitializedRes {
    pub initialized: bool,
}
rpc_toolkit::reflect_ts!(CheckInitializedRes);

#[instrument(skip_all)]
pub async fn check_initialized_impl(_ctx: ServerContext) -> Result<CheckInitializedRes, Error> {
    let has_password = get_root_hash(Path::new(SHADOW_PATH)).await?;
    Ok(CheckInitializedRes {
        initialized: has_password.is_some(),
    })
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SetInitialPasswordParams {
    pub password: String,
    /// IANA timezone name, e.g. "America/New_York". An unknown name is skipped.
    #[serde(default)]
    pub timezone: Option<String>,
}
rpc_toolkit::reflect_ts!(SetInitialPasswordParams);

#[instrument(skip_all)]
pub async fn set_initial_password_impl(
    ctx: ServerContext,
    SetInitialPasswordParams { password, timezone }: SetInitialPasswordParams,
) -> Result<(), Error> {
    set_initial_password(&ctx, Path::new(SHADOW_PATH), &password).await?;

    if let Some(timezone) = timezone {
        let params = crate::system::SetTimezoneParams { timezone };
        if let Err(e) = crate::system::set_timezone(ctx, DeserializeStdin(params)).await {
            tracing::warn!("failed to set the setup timezone: {e}");
        }
    }

    captive::disable_captive_portal().await.map_err(|e| {
        tracing::error!("failed to disable captive portal: {e}");
        e
    })?;

    Ok(())
}

/// Unenrolls every key.
async fn set_initial_password(
    ctx: &ServerContext,
    shadow: &Path,
    password: &str,
) -> Result<(), Error> {
    if get_root_hash(shadow).await?.is_some() {
        return Err(Error::new(
            eyre!("admin password is already set"),
            ErrorKind::InvalidRequest,
        ));
    }

    validate_password_length(password)?;

    let new_hash = pwhash::sha512_crypt::hash(password).map_err(|e| {
        Error::new(
            eyre!("failed to hash password: {e}"),
            ErrorKind::PasswordHashGeneration,
        )
    })?;
    update_shadow_hash(shadow, "root", &new_hash).await?;
    unenroll_all_except(ctx, None).await
}

pub fn auth<C: CtrlContext>() -> ParentHandler<C> {
    startos::auth::auth::<C, ServerContext, CliContext>()
        .subcommand(
            "verify-password",
            from_fn_async(verify_password_impl).no_cli(),
        )
        .subcommand(
            "set-password",
            from_fn_async(reset_password_impl)
                .with_metadata("get_signer", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "set-password",
            from_fn_async(cli_reset_password)
                .no_display()
                .with_about("Reset root password"),
        )
        .subcommand(
            "check-initialized",
            from_fn_async(check_initialized_impl)
                .with_metadata("authenticated", Value::Bool(false))
                .no_cli(),
        )
        .subcommand(
            "set-initial-password",
            from_fn_async(set_initial_password_impl)
                .with_metadata("authenticated", Value::Bool(false))
                .no_cli(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAST_ACTIVE_RESOLUTION: TimeDelta =
        <ServerContext as SignatureAuthContext>::LAST_ACTIVE_RESOLUTION;

    #[test]
    fn validate_password_length_rejects_short() {
        assert!(validate_password_length("shortpass12").is_err());
        assert!(validate_password_length("").is_err());
    }

    #[test]
    fn validate_password_length_accepts_min_and_above() {
        assert!(validate_password_length("exactly12chr").is_ok());
        assert!(validate_password_length("a-longer-passphrase").is_ok());
    }

    #[test]
    fn signing_identities_cover_router_name_addresses_and_ddns() {
        let identities = signing_identities(
            ["192.168.0.1".parse().unwrap(), "fd00::1".parse().unwrap()],
            Some("home.example.com".into()),
        );
        let expected: BTreeSet<InternedString> =
            ["router.lan", "192.168.0.1", "[fd00::1]", "home.example.com"]
                .into_iter()
                .map(InternedString::intern)
                .collect();
        assert_eq!(identities, expected);
    }

    #[test]
    fn enabled_ddns_hostname_is_lowercased() {
        let ddns = |enabled| crate::wan::WanDdnsResponse {
            enabled,
            provider: crate::wan::DdnsProvider::Cloudflare,
            hostname: Some("Home.Example.COM".into()),
            username: None,
            password: None,
            token: None,
            zone: None,
        };
        assert_eq!(
            enabled_ddns_hostname(ddns(true)).as_deref(),
            Some("home.example.com")
        );
        assert_eq!(enabled_ddns_hostname(ddns(false)), None);
    }

    #[tokio::test]
    async fn logout_requires_a_signing_key() {
        let ctx = ServerContext::default();
        let key = InternedString::intern("key");
        ctx.auth
            .db
            .put(
                &patch_db::json_ptr::ROOT,
                &AuthDatabase {
                    session_pubkeys: AuthKeys(
                        [(key.clone(), startos::auth::Session::default())]
                            .into_iter()
                            .collect(),
                    ),
                },
            )
            .await
            .unwrap();
        let Err(err) =
            startos::auth::logout(ctx.clone(), imbl_value::from_value(json!({})).unwrap()).await
        else {
            panic!("logout without a signing key succeeded");
        };
        assert_eq!(err.kind, startos::ErrorKind::InvalidRequest);
        startos::auth::logout(
            ctx.clone(),
            imbl_value::from_value(json!({ "__Auth_signer": key })).unwrap(),
        )
        .await
        .unwrap();
        let remaining = ctx.auth.db.peek().await.as_session_pubkeys().de().unwrap();
        assert!(remaining.0.is_empty());
    }

    async fn enroll(ctx: &ServerContext, keys: &[&str]) {
        let now = Utc::now();
        let keys: Vec<_> = keys.iter().map(|key| (*key, now)).collect();
        enroll_logged_in(ctx, &keys).await;
    }

    async fn enroll_logged_in(ctx: &ServerContext, keys: &[(&str, DateTime<Utc>)]) {
        let keys = keys
            .iter()
            .map(|(key, logged_in)| {
                let session = Session {
                    logged_in: *logged_in,
                    last_active: Utc::now(),
                    ..Default::default()
                };
                (InternedString::intern(*key), session)
            })
            .collect();
        ctx.auth
            .db
            .put(
                &patch_db::json_ptr::ROOT,
                &AuthDatabase {
                    session_pubkeys: AuthKeys(keys),
                },
            )
            .await
            .unwrap();
    }

    async fn enrolled(ctx: &ServerContext) -> BTreeSet<InternedString> {
        ctx.auth
            .db
            .peek()
            .await
            .as_session_pubkeys()
            .de()
            .unwrap()
            .0
            .into_keys()
            .collect()
    }

    fn write_shadow(dir: &tempfile::TempDir, root_hash: &str) -> std::path::PathBuf {
        let shadow = dir.path().join("shadow");
        std::fs::write(
            &shadow,
            format!("root:{root_hash}:0:0:99999:7:::\ndaemon:*:0:0:99999:7:::\n"),
        )
        .unwrap();
        shadow
    }

    fn hash(password: &str) -> String {
        pwhash::sha512_crypt::hash(password).unwrap()
    }

    fn reset_params(signer: Option<&str>) -> ResetPasswordParams {
        ResetPasswordParams {
            old_password: "old-admin-password".into(),
            new_password: "new-admin-password".into(),
            signer: signer.map(InternedString::intern),
        }
    }

    #[test]
    fn login_needs_a_root_password() {
        let dir = tempfile::TempDir::new().unwrap();
        for root_hash in ["", "*", "!", "x"] {
            let shadow = write_shadow(&dir, root_hash);
            let Err(err) = check_password_sync(&shadow, "any-password-at-all") else {
                panic!("a password matched root hash {root_hash:?}");
            };
            assert_eq!(err.kind, ErrorKind::Uninitialized);
        }
    }

    #[tokio::test]
    async fn set_password_unenrolls_every_other_key() {
        let ctx = ServerContext::default();
        enroll(&ctx, &["caller", "other-a", "other-b"]).await;
        let dir = tempfile::TempDir::new().unwrap();
        let shadow = write_shadow(&dir, &hash("old-admin-password"));

        reset_password(&ctx, &shadow, reset_params(Some("caller")))
            .await
            .unwrap();

        assert_eq!(
            enrolled(&ctx).await,
            BTreeSet::from([InternedString::intern("caller")])
        );
        check_password_sync(&shadow, "new-admin-password").unwrap();
    }

    #[tokio::test]
    async fn local_set_password_unenrolls_every_key() {
        let ctx = ServerContext::default();
        enroll(&ctx, &["browser", "cli"]).await;
        let dir = tempfile::TempDir::new().unwrap();
        let shadow = write_shadow(&dir, &hash("old-admin-password"));

        reset_password(&ctx, &shadow, reset_params(None))
            .await
            .unwrap();

        assert!(enrolled(&ctx).await.is_empty());
    }

    #[tokio::test]
    async fn failed_password_write_unenrolls_nothing() {
        let ctx = ServerContext::default();
        enroll(&ctx, &["caller", "other"]).await;
        let dir = tempfile::TempDir::new().unwrap();
        let shadow = write_shadow(&dir, &hash("old-admin-password"));
        let blocked_write = startos::util::io::to_tmp_path(&shadow).unwrap();
        std::fs::create_dir(blocked_write).unwrap();

        reset_password(&ctx, &shadow, reset_params(Some("caller")))
            .await
            .unwrap_err();

        assert_eq!(enrolled(&ctx).await.len(), 2);
        check_password_sync(&shadow, "old-admin-password").unwrap();
    }

    #[tokio::test]
    async fn initial_password_unenrolls_every_key() {
        let ctx = ServerContext::default();
        enroll(&ctx, &["before-setup-a", "before-setup-b"]).await;
        let dir = tempfile::TempDir::new().unwrap();
        let shadow = write_shadow(&dir, "");

        set_initial_password(&ctx, &shadow, "initial-admin-password")
            .await
            .unwrap();

        assert!(enrolled(&ctx).await.is_empty());
        check_password_sync(&shadow, "initial-admin-password").unwrap();
    }

    #[tokio::test]
    async fn requests_are_refused_a_day_after_login() {
        let ctx = ServerContext::default();
        let pubkey = |seed| {
            use ed25519_dalek::pkcs8::EncodePublicKey;
            ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
                .verifying_key()
                .to_public_key_pem(Default::default())
                .unwrap()
                .parse::<AnyVerifyingKey>()
                .unwrap()
        };
        let (expired, live) = (pubkey(1), pubkey(2));
        let (expired_pem, live_pem) = (expired.to_string(), live.to_string());
        let now = Utc::now();
        enroll_logged_in(
            &ctx,
            &[
                (
                    expired_pem.as_str(),
                    now - TimeDelta::hours(24) - TimeDelta::seconds(1),
                ),
                (
                    live_pem.as_str(),
                    now - TimeDelta::hours(23) - TimeDelta::minutes(59),
                ),
            ],
        )
        .await;
        let db = ctx.auth.db.peek().await;
        let check = |key: &AnyVerifyingKey| {
            ctx.check_pubkey(&db, Some(key), LoginMetadata { login: false })
        };

        let Err(err) = check(&expired) else {
            panic!("a key signed in over a day ago was accepted");
        };
        assert_eq!(err.kind, startos::ErrorKind::Authorization);
        assert_eq!(check(&live).unwrap(), Some(live.interned_pem()));
    }

    #[test]
    fn only_a_trusted_clock_ends_a_future_login() {
        let now = Utc::now();
        assert!(!sign_in_ended(now + TimeDelta::days(3), now, false));
        assert!(sign_in_ended(now + TimeDelta::days(3), now, true));
        assert!(!sign_in_ended(now + TimeDelta::minutes(1), now, true));
    }

    #[tokio::test]
    async fn sweep_unenrolls_keys_a_day_after_login() {
        let ctx = ServerContext::default();
        let now = Utc::now();
        enroll_logged_in(
            &ctx,
            &[
                ("in-use", now - TimeDelta::hours(25)),
                ("fresh", now - TimeDelta::hours(1)),
                ("future", now + TimeDelta::hours(1)),
            ],
        )
        .await;
        let continuations = &ctx.auth.open_authed_continuations;
        ctx.auth
            .db
            .mutate(|db| startos::auth::reap_idle_keys_in::<ServerContext>(continuations, None, db))
            .await
            .result
            .unwrap();
        assert_eq!(
            enrolled(&ctx).await,
            BTreeSet::from([InternedString::intern("fresh")])
        );
    }

    #[tokio::test]
    async fn last_active_persists_only_once_stale() {
        let ctx = ServerContext::default();
        ctx.auth
            .db
            .put(&patch_db::json_ptr::ROOT, &AuthDatabase::default())
            .await
            .unwrap();
        let key = InternedString::intern("key");
        let enrolled = Utc::now() - TimeDelta::minutes(5);
        ctx.auth
            .db
            .mutate(|db| {
                db.as_session_pubkeys_mut().insert(
                    &key,
                    &startos::auth::Session {
                        logged_in: enrolled,
                        last_active: enrolled,
                        ..Default::default()
                    },
                )
            })
            .await
            .result
            .unwrap();
        let request = rpc_toolkit::RpcRequest {
            id: None,
            method: rpc_toolkit::yajrc::GenericRpcMethod::new("x".into()),
            params: Value::Null,
        };
        let last_active = |ctx: &ServerContext| {
            let ctx = ctx.clone();
            let key = key.clone();
            async move {
                ctx.auth
                    .db
                    .peek()
                    .await
                    .as_session_pubkeys()
                    .de()
                    .unwrap()
                    .0[&key]
                    .last_active
            }
        };

        ctx.post_auth_hook(Some(key.clone()), &request)
            .await
            .unwrap();
        assert_eq!(last_active(&ctx).await, enrolled);

        let stale = Utc::now() - LAST_ACTIVE_RESOLUTION - TimeDelta::minutes(1);
        ctx.auth
            .db
            .mutate(|db| {
                db.as_session_pubkeys_mut().mutate(|keys| {
                    keys.0.get_mut(&key).unwrap().last_active = stale;
                    Ok(())
                })
            })
            .await
            .result
            .unwrap();
        ctx.post_auth_hook(Some(key.clone()), &request)
            .await
            .unwrap();
        assert!(last_active(&ctx).await > stale + LAST_ACTIVE_RESOLUTION);
    }
}
