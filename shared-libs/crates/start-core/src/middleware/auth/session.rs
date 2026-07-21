use std::borrow::Borrow;
use std::collections::BTreeSet;
use std::ops::Deref;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::Request;
use axum::response::Response;
use basic_cookies::Cookie;
use chrono::Utc;
use http::HeaderValue;
use http::header::{COOKIE, USER_AGENT};
use rpc_toolkit::yajrc::INTERNAL_ERROR;
use rpc_toolkit::{Middleware, RpcRequest, RpcResponse};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::auth::{Sessions, check_password, write_shadow};
use crate::context::RpcContext;
use crate::middleware::auth::DbContext;
use crate::prelude::*;
use crate::rpc_continuations::OpenAuthedContinuations;
use crate::util::sync::SyncMutex;

pub trait SessionAuthContext: DbContext {
    fn ephemeral_sessions(&self) -> &SyncMutex<Sessions>;
    fn open_authed_continuations(&self) -> &OpenAuthedContinuations<Option<InternedString>>;
    fn access_sessions(db: &mut Model<Self::Database>) -> &mut Model<Sessions>;
    fn check_password(db: &Model<Self::Database>, password: &str) -> Result<(), Error>;
    #[allow(unused_variables)]
    fn post_login_hook(&self, password: &str) -> impl Future<Output = Result<(), Error>> + Send {
        async { Ok(()) }
    }
}

impl SessionAuthContext for RpcContext {
    fn ephemeral_sessions(&self) -> &SyncMutex<Sessions> {
        &self.ephemeral_sessions
    }
    fn open_authed_continuations(&self) -> &OpenAuthedContinuations<Option<InternedString>> {
        &self.open_authed_continuations
    }
    fn access_sessions(db: &mut Model<Self::Database>) -> &mut Model<Sessions> {
        db.as_private_mut().as_sessions_mut()
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

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginRes {
    pub session: InternedString,
}

pub trait AsLogoutSessionId {
    fn as_logout_session_id(self) -> InternedString;
}

/// Will need to know when we have logged out from a route
#[derive(Serialize, Deserialize)]
pub struct HasLoggedOutSessions(());

impl HasLoggedOutSessions {
    pub async fn new<C: SessionAuthContext>(
        sessions: impl IntoIterator<Item = impl AsLogoutSessionId>,
        ctx: &C,
    ) -> Result<Self, Error> {
        let to_log_out: BTreeSet<_> = sessions
            .into_iter()
            .map(|s| s.as_logout_session_id())
            .collect();
        for sid in &to_log_out {
            ctx.open_authed_continuations().kill(&Some(sid.clone()))
        }
        ctx.ephemeral_sessions().mutate(|s| {
            for sid in &to_log_out {
                s.0.remove(sid);
            }
        });
        ctx.db()
            .mutate(|db| {
                let sessions = C::access_sessions(db);
                for sid in &to_log_out {
                    sessions.remove(sid)?;
                }

                Ok(())
            })
            .await
            .result?;
        Ok(HasLoggedOutSessions(()))
    }
}

/// When we have a need to create a new session,
/// Or when we are using internal valid authenticated service.
#[derive(Debug, Clone)]
pub struct HashSessionToken {
    hashed: InternedString,
    token: InternedString,
}
impl HashSessionToken {
    pub fn new() -> Self {
        Self::from_token(InternedString::intern(
            base32::encode(
                base32::Alphabet::Rfc4648 { padding: false },
                &rand::random::<[u8; 16]>(),
            )
            .to_lowercase(),
        ))
    }

    pub fn from_token(token: InternedString) -> Self {
        let hashed = Self::hash(&*token);
        Self { hashed, token }
    }

    pub fn from_cookie(cookie: &Cookie) -> Self {
        Self::from_token(InternedString::intern(cookie.get_value()))
    }

    pub fn from_header(header: Option<&HeaderValue>) -> Result<Self, Error> {
        if let Some(cookie_header) = header {
            let cookies = Cookie::parse(
                cookie_header
                    .to_str()
                    .with_kind(crate::ErrorKind::Authorization)?,
            )
            .with_kind(crate::ErrorKind::Authorization)?;
            if let Some(session) = cookies.iter().find(|c| c.get_name() == "session") {
                return Ok(Self::from_cookie(session));
            }
        }
        Err(Error::new(
            eyre!("{}", t!("middleware.auth.unauthorized")),
            crate::ErrorKind::Authorization,
        ))
    }

    pub fn to_login_res(&self) -> LoginRes {
        LoginRes {
            session: self.token.clone(),
        }
    }

    pub fn hashed(&self) -> &InternedString {
        &self.hashed
    }

    fn hash(token: &str) -> InternedString {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        InternedString::intern(
            base32::encode(
                base32::Alphabet::Rfc4648 { padding: false },
                hasher.finalize().as_slice(),
            )
            .to_lowercase(),
        )
    }
}
impl AsLogoutSessionId for HashSessionToken {
    fn as_logout_session_id(self) -> InternedString {
        self.hashed
    }
}
impl PartialEq for HashSessionToken {
    fn eq(&self, other: &Self) -> bool {
        self.hashed == other.hashed
    }
}
impl Eq for HashSessionToken {}
impl PartialOrd for HashSessionToken {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.hashed.partial_cmp(&other.hashed)
    }
}
impl Ord for HashSessionToken {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.hashed.cmp(&other.hashed)
    }
}
impl Borrow<str> for HashSessionToken {
    fn borrow(&self) -> &str {
        &*self.hashed
    }
}

pub struct ValidSessionToken(pub HashSessionToken);
impl ValidSessionToken {
    pub async fn from_header<C: SessionAuthContext>(
        header: Option<&HeaderValue>,
        ctx: &C,
    ) -> Result<Self, Error> {
        if let Some(cookie_header) = header {
            let cookies = Cookie::parse(
                cookie_header
                    .to_str()
                    .with_kind(crate::ErrorKind::Authorization)?,
            )
            .with_kind(crate::ErrorKind::Authorization)?;
            if let Some(cookie) = cookies.iter().find(|c| c.get_name() == "session") {
                if let Ok(s) = Self::from_session(HashSessionToken::from_cookie(cookie), ctx).await
                {
                    return Ok(s);
                }
            }
        }
        Err(Error::new(
            eyre!("{}", t!("middleware.auth.unauthorized")),
            crate::ErrorKind::Authorization,
        ))
    }

    pub async fn from_session<C: SessionAuthContext>(
        session_token: HashSessionToken,
        ctx: &C,
    ) -> Result<Self, Error> {
        let session_hash = session_token.hashed();
        if !ctx.ephemeral_sessions().mutate(|s| {
            if let Some(session) = s.0.get_mut(session_hash) {
                session.last_active = Utc::now();
                true
            } else {
                false
            }
        }) {
            ctx.db()
                .mutate(|db| {
                    C::access_sessions(db)
                        .as_idx_mut(session_hash)
                        .ok_or_else(|| {
                            Error::new(
                                eyre!("{}", t!("middleware.auth.unauthorized")),
                                crate::ErrorKind::Authorization,
                            )
                        })?
                        .mutate(|s| {
                            s.last_active = Utc::now();
                            Ok(())
                        })
                })
                .await
                .result?;
        }
        Ok(Self(session_token))
    }
}

#[derive(Deserialize)]
pub struct Metadata {
    #[serde(default)]
    login: bool,
    #[serde(default)]
    get_session: bool,
}

const LOGIN_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(20);
const LOGIN_RATE_LIMIT_MAX_ATTEMPTS: usize = 3;

#[derive(Clone)]
pub struct SessionAuth {
    rate_limiter: Arc<SyncMutex<(usize, Instant)>>,
    is_login: bool,
    cookie: Option<HeaderValue>,
    set_cookie: Option<HeaderValue>,
    user_agent: Option<HeaderValue>,
}
impl SessionAuth {
    pub fn new() -> Self {
        Self {
            rate_limiter: Arc::new(SyncMutex::new((0, Instant::now()))),
            is_login: false,
            cookie: None,
            set_cookie: None,
            user_agent: None,
        }
    }
}

impl<C: SessionAuthContext> Middleware<C> for SessionAuth {
    type Metadata = Metadata;
    async fn process_http_request(&mut self, _: &C, request: &mut Request) -> Result<(), Response> {
        self.cookie = request.headers().get(COOKIE).cloned();
        self.user_agent = request.headers().get(USER_AGENT).cloned();
        Ok(())
    }
    async fn process_rpc_request(
        &mut self,
        context: &C,
        metadata: Self::Metadata,
        request: &mut RpcRequest,
    ) -> Result<(), RpcResponse> {
        async {
            if metadata.login {
                self.is_login = true;
                self.rate_limiter.mutate(|(count, time)| {
                    if time.elapsed() >= LOGIN_RATE_LIMIT_WINDOW {
                        *count = 0;
                    }
                    if *count >= LOGIN_RATE_LIMIT_MAX_ATTEMPTS {
                        Err(Error::new(
                            eyre!("{}", t!("middleware.auth.rate-limited-login")),
                            crate::ErrorKind::RateLimited,
                        ))
                    } else {
                        *count += 1;
                        *time = Instant::now();
                        Ok(())
                    }
                })?;
                if let Some(user_agent) = self.user_agent.as_ref().and_then(|h| h.to_str().ok()) {
                    request.params["__Auth_userAgent"] =
                        Value::String(Arc::new(user_agent.to_owned()))
                }
            } else {
                let ValidSessionToken(s) =
                    ValidSessionToken::from_header(self.cookie.as_ref(), context).await?;
                if metadata.get_session {
                    request.params["__Auth_session"] =
                        Value::String(Arc::new(s.hashed().deref().to_owned()));
                }
            }
            Ok::<_, Error>(())
        }
        .await
        .map_err(|e| RpcResponse::from_result(Err(e)))
    }
    async fn process_rpc_response(&mut self, _: &C, response: &mut RpcResponse) {
        if self.is_login {
            if response.result.is_ok() {
                let res = std::mem::replace(&mut response.result, Err(INTERNAL_ERROR));
                response.result = async {
                    let res = res?;
                    let login_res = from_value::<LoginRes>(res.clone())?;
                    self.set_cookie = Some(
                        HeaderValue::from_str(&format!(
                        "session={}; Path=/; SameSite=Strict; Expires=Fri, 31 Dec 9999 23:59:59 GMT;",
                        login_res.session
                    ))
                        .with_kind(crate::ErrorKind::Network)?,
                    );

                    Ok(res)
                }
                .await;
            }
        }
    }
    async fn process_http_response(&mut self, _: &C, response: &mut Response) {
        if let Some(set_cookie) = self.set_cookie.take() {
            response.headers_mut().insert("set-cookie", set_cookie);
        }
    }
}
