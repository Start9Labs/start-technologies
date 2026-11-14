use std::collections::{BTreeSet, HashSet};
use std::future::Future;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::Request;
use chrono::TimeDelta;
use futures::FutureExt;
use futures::future::BoxFuture;
use http::header::USER_AGENT;
use http::{Extensions, HeaderMap, HeaderValue};
use itertools::Itertools;
use reqwest::Client;
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{Context, Middleware, RpcRequest, RpcResponse};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::auth::{AuthKeys, LoginContext};
use crate::context::{CliContext, RpcContext};
use crate::middleware::auth::DbContext;
use crate::net::host::binding::BindInfo;
use crate::net::http::request_authority;
use crate::net::request_origin::RequestOrigin;
use crate::prelude::*;
use crate::rpc_continuations::OpenAuthedContinuations;
use crate::sign::commitment::Commitment;
use crate::sign::commitment::request::RequestCommitment;
use crate::sign::{AnySignature, AnySigningKey, AnyVerifyingKey, Signer};
use crate::util::sync::SyncMutex;

pub const AUTH_SIG_HEADER: &str = "X-Start-Auth-Sig";

/// RPC-metadata fields understood by [`SignatureAuth`] when layered over an
/// [`RpcContext`]. `login` marks enrollment: encrypted admission or a request
/// signature proves possession of the key, independently of prior registration.
#[derive(Deserialize)]
pub struct LoginMetadata {
    #[serde(default)]
    pub login: bool,
}

/// Identifies password-enrollment requests for login throttling.
pub trait SignatureAuthMetadata {
    /// Whether the request enrolls a signer by password.
    fn is_login(&self) -> bool {
        false
    }
}

impl SignatureAuthMetadata for LoginMetadata {
    fn is_login(&self) -> bool {
        self.login
    }
}

/// Returns the enrolled key, or `None` for signed login requests.
pub fn check_enrolled(
    pubkey: Option<&AnyVerifyingKey>,
    login: bool,
    is_enrolled: impl FnOnce(&InternedString) -> Result<bool, Error>,
) -> Result<Option<InternedString>, Error> {
    let Some(pubkey) = pubkey else {
        return Err(Error::new(
            eyre!("{}", t!("middleware.auth.unauthorized")),
            ErrorKind::Authorization,
        ));
    };
    if login {
        return Ok(None);
    }
    let key = pubkey.interned_der64();
    if is_enrolled(&key)? {
        Ok(Some(key))
    } else {
        Err(Error::new(
            eyre!("{}", t!("middleware.auth.key-not-authorized")),
            ErrorKind::Authorization,
        ))
    }
}

/// Admission capabilities independent of enrolled authorization or database availability.
pub trait SignatureAuthContext: Context + SessionContext {
    fn server_identity(&self) -> Result<Signer, Error>;
    /// Acquires admission replay policy without retaining the authentication owner.
    fn replay_check(&self) -> RequestReplayCheck;
    fn with_secure_session(self, session: crate::middleware::auth::session::SecureSession) -> Self
    where
        Self: Sized;
    fn mutate_nonce_cache<F: FnOnce(&mut NonceCache) -> T, T>(&self, f: F) -> T;
    /// Whether the clock can be trusted; untrusted clocks use nonce LRU eviction.
    fn clock_synced(&self) -> impl Future<Output = bool> + Send;
    /// Signing identities allowed for the request's ingress.
    /// JSON message checks supply the connection's `RequestOrigin` extension.
    fn sig_context(
        &self,
        extensions: &Extensions,
    ) -> impl Future<Output = impl IntoIterator<Item = Result<impl AsRef<str> + Send, Error>> + Send>
    + Send;
}

/// Live enrolled-key policy and lifecycle over the authoritative database.
pub trait SignatureAuthorizationContext: SignatureAuthContext + DbContext {
    type AdditionalMetadata: DeserializeOwned + SignatureAuthMetadata + Send;
    type CheckPubkeyRes: Send;
    /// How far a key's persisted `last_active` may lag its latest request.
    /// Zero persists every request.
    const LAST_ACTIVE_RESOLUTION: TimeDelta = TimeDelta::zero();
    /// Revocation-scoped continuations keyed by the signer's DER64 identity.
    fn open_authed_continuations(&self) -> &OpenAuthedContinuations<Option<InternedString>>;
    /// In-memory enrolled keys that are never persisted (kiosk mode), if the
    /// context supports them.
    fn ephemeral_auth_keys(&self) -> Option<&SyncMutex<AuthKeys>> {
        None
    }
    /// Remove `keys` from this context's persisted signer store. Store
    /// removal only — never call directly: unenrollment goes through
    /// [`Self::unenroll`] or [`HasUnenrolledKeys::unenroll`] so the keys'
    /// open continuations die with them.
    fn remove_enrolled_keys(
        db: &mut Model<Self::Database>,
        keys: &BTreeSet<InternedString>,
    ) -> Result<(), Error>;
    fn check_pubkey(
        &self,
        db: &Model<Self::Database>,
        pubkey: Option<&AnyVerifyingKey>,
        metadata: Self::AdditionalMetadata,
    ) -> Result<Self::CheckPubkeyRes, Error>;
    fn post_auth_hook(
        &self,
        check_pubkey_res: Self::CheckPubkeyRes,
        request: &RpcRequest,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    /// Unenroll `keys`, however they were enrolled: kills their open
    /// continuations and removes them from the ephemeral and persisted
    /// signer stores. Not meant to be overridden — call sites already inside
    /// a db transaction use [`HasUnenrolledKeys::unenroll`] directly.
    fn unenroll(
        &self,
        keys: impl IntoIterator<Item = InternedString>,
    ) -> impl Future<Output = Result<HasUnenrolledKeys, Error>> + Send
    where
        Self: Sized,
    {
        let keys: BTreeSet<_> = keys.into_iter().collect();
        async move {
            let continuations = self.open_authed_continuations();
            let ephemeral = self.ephemeral_auth_keys();
            self.db()
                .mutate(|db| {
                    HasUnenrolledKeys::unenroll::<Self>(continuations, ephemeral, db, keys)
                })
                .await
                .result
        }
    }
}

/// Proof that a set of auth keys was unenrolled — removed from the persisted
/// and ephemeral signer stores with any continuations they opened killed.
/// Obtained via [`SignatureAuthorizationContext::unenroll`], or [`Self::unenroll`]
/// from inside a db transaction.
#[derive(Serialize, Deserialize, VisitFields)]
pub struct HasUnenrolledKeys(());

rpc_toolkit::reflect_ts!(HasUnenrolledKeys);
impl HasUnenrolledKeys {
    /// For call sites already inside a db transaction. Taking the
    /// continuations map as a parameter is the point: unenrollment cannot be
    /// expressed without handing over the kill handle. The kills are not
    /// transactional: if the mutation is later discarded, the keys stay
    /// enrolled but their continuations are already dead — erring on the
    /// side of dropping a reconnectable session, never the reverse.
    pub fn unenroll<C: SignatureAuthorizationContext>(
        continuations: &OpenAuthedContinuations<Option<InternedString>>,
        ephemeral: Option<&SyncMutex<AuthKeys>>,
        db: &mut Model<C::Database>,
        keys: impl IntoIterator<Item = InternedString>,
    ) -> Result<Self, Error> {
        let keys: BTreeSet<_> = keys.into_iter().collect();
        for key in &keys {
            continuations.kill(&Some(key.clone()))
        }
        if let Some(ephemeral) = ephemeral {
            ephemeral.mutate(|map| {
                for key in &keys {
                    map.0.remove(&**key);
                }
            });
        }
        C::remove_enrolled_keys(db, &keys)?;
        Ok(HasUnenrolledKeys(()))
    }
}

impl SignatureAuthContext for RpcContext {
    fn server_identity(&self) -> Result<Signer, Error> {
        RpcContext::server_identity(self)
    }
    fn replay_check(&self) -> RequestReplayCheck {
        RpcContext::replay_check(self)
    }
    fn with_secure_session(self, session: crate::middleware::auth::session::SecureSession) -> Self {
        RpcContext::with_secure_session(self, session)
    }
    fn mutate_nonce_cache<F: FnOnce(&mut NonceCache) -> T, T>(&self, f: F) -> T {
        self.auth_sig_nonce_cache.mutate(f)
    }
    async fn clock_synced(&self) -> bool {
        self.db
            .peek()
            .await
            .as_public()
            .as_server_info()
            .as_ntp_synced()
            .de()
            .unwrap_or(false)
    }
    async fn sig_context(
        &self,
        extensions: &Extensions,
    ) -> impl IntoIterator<Item = Result<impl AsRef<str> + Send, Error>> + Send {
        let origin = RequestOrigin::from_extensions(extensions);
        let binding = self
            .db
            .peek()
            .await
            .into_public()
            .into_server_info()
            .into_network()
            .into_host()
            .into_bindings()
            .into_idx(&80)
            .map(|b| b.de())
            .transpose();
        std::iter::once(
            binding.map(|binding| startos_signing_identities(binding.as_ref(), &origin)),
        )
        .flatten_ok()
    }
}

impl SignatureAuthorizationContext for RpcContext {
    type AdditionalMetadata = LoginMetadata;
    type CheckPubkeyRes = Option<InternedString>;
    fn open_authed_continuations(&self) -> &OpenAuthedContinuations<Option<InternedString>> {
        &self.open_authed_continuations
    }
    fn ephemeral_auth_keys(&self) -> Option<&SyncMutex<AuthKeys>> {
        Some(&self.ephemeral_auth_keys)
    }
    fn remove_enrolled_keys(
        db: &mut Model<Self::Database>,
        keys: &BTreeSet<InternedString>,
    ) -> Result<(), Error> {
        let auth_keys = db.as_private_mut().as_sessions_mut().as_pubkeys_mut();
        for key in keys {
            auth_keys.remove(key)?;
        }
        Ok(())
    }
    fn check_pubkey(
        &self,
        db: &Model<Self::Database>,
        pubkey: Option<&AnyVerifyingKey>,
        metadata: Self::AdditionalMetadata,
    ) -> Result<Self::CheckPubkeyRes, Error> {
        check_enrolled(pubkey, metadata.login, |key| {
            self.auth_key_is_enrolled(db, key)
        })
    }
    async fn post_auth_hook(&self, key: Self::CheckPubkeyRes, _: &RpcRequest) -> Result<(), Error> {
        if let Some(key) = key {
            crate::auth::touch_last_active(self, &key).await?;
        }
        Ok(())
    }
}

fn startos_signing_identities(
    binding: Option<&BindInfo>,
    origin: &RequestOrigin,
) -> BTreeSet<InternedString> {
    let mut identities = BTreeSet::new();
    if let Some(binding) = binding.filter(|binding| binding.enabled) {
        for address in binding.enabled_addresses() {
            if !origin.is_internal() {
                // Public identities remain valid privately for hairpin access.
                if address.is_internal()
                    || (origin.public && !address.public)
                    || !origin
                        .gateway
                        .as_ref()
                        .is_some_and(|gateway| address.metadata.gateways().any(|g| g == gateway))
                {
                    continue;
                }
            }
            let identity = match address.metadata {
                crate::net::service_interface::HostnameMetadata::Ipv6 { .. } => {
                    InternedString::from_display(&lazy_format!("[{}]", address.hostname))
                }
                _ => address.hostname.clone(),
            };
            identities.insert(identity);
        }
    }
    if origin.peer_ip.is_some_and(|ip| ip.is_loopback()) {
        identities.insert("localhost".into());
    }
    identities
}

/// Formats an IP as `url::Url::host_str` renders it.
pub fn url_host_str(ip: IpAddr) -> InternedString {
    match ip {
        IpAddr::V4(ip) => InternedString::from_display(&ip),
        IpAddr::V6(ip) => InternedString::from_display(&lazy_format!("[{ip}]")),
    }
}

/// A loopback signing identity for requests from a loopback peer.
fn loopback_identity(request: &Request) -> Option<InternedString> {
    if !RequestOrigin::from_extensions(request.extensions())
        .peer_ip
        .is_some_and(|ip| ip.is_loopback())
    {
        return None;
    }
    let ip: IpAddr = request_authority(request)?
        .host()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse()
        .ok()?;
    ip.is_loopback().then(|| url_host_str(ip))
}

/// Names the server answers to on the request's ingress.
pub(crate) fn request_identities<'a, C: SignatureAuthContext>(
    context: &'a C,
    request: &'a Request,
) -> impl Future<Output = Result<Vec<InternedString>, Error>> + Send + 'a {
    let identities = signing_identities(context, request.extensions(), loopback_identity(request));
    let host = request_authority(request).map(|authority| authority.host().to_owned());
    async move {
        let mut identities = identities.await?;
        if let Some(host) = host {
            if let Some(index) = identities.iter().position(|identity| &**identity == host) {
                identities.swap(0, index);
            }
        }
        Ok(identities)
    }
}

async fn signing_identities<C: SignatureAuthContext>(
    context: &C,
    ingress: &Extensions,
    loopback: Option<InternedString>,
) -> Result<Vec<InternedString>, Error> {
    let mut identities = context
        .sig_context(ingress)
        .await
        .into_iter()
        .map(|identity| identity.map(|identity| InternedString::intern(identity.as_ref())))
        .collect::<Result<Vec<_>, _>>()?;
    identities.extend(loopback);
    Ok(identities)
}

pub trait SigningContext {
    fn signing_key(&self) -> Result<Signer, Error>;

    /// Local credentials take precedence; uninitialized identities permit anonymous calls.
    fn rpc_signer(&self, headers: &http::HeaderMap) -> Result<Option<Signer>, Error> {
        if headers.contains_key(http::header::AUTHORIZATION) {
            return Ok(None);
        }
        match self.signing_key() {
            Ok(signer) => Ok(Some(signer)),
            Err(error) if error.kind == ErrorKind::Uninitialized => Ok(None),
            Err(error) => Err(error),
        }
    }
}

impl SigningContext for CliContext {
    fn signing_key(&self) -> Result<Signer, Error> {
        Ok(self.id_key()?.clone())
    }
}

impl SigningContext for RpcContext {
    fn signing_key(&self) -> Result<Signer, Error> {
        Ok(Signer::Key(AnySigningKey::Ed25519(
            self.account
                .peek(|account| account.server_identity_key.clone()),
        )))
    }
}

#[derive(Deserialize)]
pub struct Metadata<Additional> {
    #[serde(flatten)]
    additional: Additional,
    #[serde(default)]
    get_signer: bool,
    #[serde(default)]
    get_user_agent: bool,
    #[serde(default)]
    get_peer: bool,
}

#[derive(Clone)]
pub struct SignatureAuth {
    signer: Option<Result<(AnyVerifyingKey, InternedString), RpcError>>,
    user_agent: Option<HeaderValue>,
    origin: RequestOrigin,
}
impl SignatureAuth {
    pub fn new() -> Self {
        Self {
            signer: None,
            user_agent: None,
            origin: RequestOrigin::default(),
        }
    }
}

const SIG_TIME_GRACE_PERIOD_SECS: u64 = 60;

const MAX_UNTIMED_NONCES: usize = 1024 * 1024;

/// Retains nonces for 60 seconds with a trusted clock.
/// Otherwise bounds storage by evicting the oldest submitted timestamps.
#[derive(Default)]
pub struct NonceCache {
    seen: HashSet<u64>,
    order: BTreeSet<(i64, u64)>,
}
impl NonceCache {
    fn evict_oldest(&mut self) {
        let (_, nonce) = self.order.pop_first().unwrap();
        self.seen.remove(&nonce);
    }

    fn handle_nonce(&mut self, nonce: u64, timestamp: i64, now: Option<i64>) -> Result<(), Error> {
        if let Some(now) = now {
            while let Some(&(timestamp, _)) = self.order.iter().next() {
                if now.saturating_sub(timestamp) > SIG_TIME_GRACE_PERIOD_SECS as i64 {
                    self.evict_oldest();
                } else {
                    break;
                }
            }
        }
        // Replay errors must not trigger key-revocation logout.
        if !self.seen.insert(nonce) {
            return Err(Error::new(
                eyre!("{}", t!("middleware.auth.replay-attack-detected")),
                ErrorKind::InvalidSignature,
            ));
        }
        if now.is_none() {
            while self.order.len() >= MAX_UNTIMED_NONCES {
                self.evict_oldest();
            }
        }
        self.order.insert((timestamp, nonce));
        Ok(())
    }
}

/// Verify the [`AUTH_SIG_HEADER`] on an incoming request: signature against
/// each of the context's sig-context strings, timestamp within 30s, nonce not
/// replayed, and the body hash matching the commitment (the body is buffered
/// back into the request). Returns the verified signer.
pub async fn verify_request_signature<C: SignatureAuthContext>(
    context: &C,
    request: &mut Request,
) -> Result<AnyVerifyingKey, Error> {
    verify_request_signature_with_address(context, request)
        .await
        .map(|(signer, _)| signer)
}

async fn verify_request_signature_with_address<C: SignatureAuthContext>(
    context: &C,
    request: &mut Request,
) -> Result<(AnyVerifyingKey, InternedString), Error> {
    let origin = RequestOrigin::from_extensions(request.extensions());
    request.extensions_mut().insert(origin);
    let header = SignatureHeader::from_header(
        request
            .headers()
            .get(AUTH_SIG_HEADER)
            .or_not_found(AUTH_SIG_HEADER)
            .with_kind(ErrorKind::InvalidRequest)?,
    )?;

    let limit = request
        .extensions()
        .get::<rpc_toolkit::RequestBodyLimit>()
        .copied()
        .unwrap_or_default();
    ensure_code!(
        header.commitment.size <= limit.0.get() as u64,
        ErrorKind::InvalidRequest,
        "{}",
        t!("middleware.auth.request-body-too-large")
    );

    let address = {
        let identities = request_identities(context, request).await?;
        let address = verified_address(&header, identities.iter().map(|c| &**c));
        let Some(address) = address else {
            tracing::debug!(
                signer = ?header.signer,
                contexts = ?identities,
                "request signature failed verification for every known server identity"
            );
            return Err(Error::new(
                eyre!("{}", t!("middleware.auth.invalid-request-signature")),
                ErrorKind::Authorization,
            ));
        };
        address
    };
    let SignatureHeader {
        commitment, signer, ..
    } = header;

    accept_nonce(context, &commitment).await?;

    let mut body = Vec::with_capacity(commitment.size as usize);
    commitment.copy_to(request, &mut body).await?;
    *request.body_mut() = Body::from(body);

    Ok((signer, address))
}

/// Live replay and clock policy without retaining the authentication owner.
#[derive(Clone)]
pub struct RequestReplayCheck(
    Arc<dyn Fn(RequestCommitment) -> BoxFuture<'static, Result<(), Error>> + Send + Sync>,
);

impl RequestReplayCheck {
    /// Additional owners captured by the context factory must be weak.
    pub fn new<S, C>(
        owner: &Arc<S>,
        context: impl Fn(Arc<S>) -> Option<C> + Clone + Send + Sync + 'static,
    ) -> Self
    where
        S: Send + Sync + 'static,
        C: SignatureAuthContext + 'static,
    {
        let owner = Arc::downgrade(owner);
        Self(Arc::new(move |commitment| {
            let owner = owner.clone();
            let context = context.clone();
            async move {
                let context = owner.upgrade().and_then(context).ok_or_else(|| {
                    Error::new(
                        eyre!("{}", t!("middleware.auth.invalid-request-signature")),
                        ErrorKind::InvalidSignature,
                    )
                })?;
                accept_nonce(&context, &commitment).await
            }
            .boxed()
        }))
    }

    /// The caller must verify identity and complete body before consuming the nonce.
    pub fn check(&self, commitment: RequestCommitment) -> BoxFuture<'static, Result<(), Error>> {
        (self.0)(commitment)
    }
}

#[cfg(test)]
#[path = "signature/replay_tests.rs"]
mod replay_tests;

async fn accept_nonce<C: SignatureAuthContext>(
    context: &C,
    commitment: &RequestCommitment,
) -> Result<(), Error> {
    let now = if context.clock_synced().await {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_else(|e| e.duration().as_secs() as i64 * -1);
        if now.abs_diff(commitment.timestamp) > SIG_TIME_GRACE_PERIOD_SECS / 2 {
            return Err(Error::new(
                eyre!("{}", t!("middleware.auth.timestamp-not-within-30s")),
                ErrorKind::InvalidSignature,
            ));
        }
        Some(now)
    } else {
        None
    };
    context.mutate_nonce_cache(|n| n.handle_nonce(commitment.nonce, commitment.timestamp, now))
}

fn verified_address(
    header: &SignatureHeader,
    contexts: impl IntoIterator<Item = impl AsRef<str>>,
) -> Option<InternedString> {
    contexts.into_iter().find_map(|context| {
        verify_request(
            &header.signer,
            &header.commitment,
            context.as_ref(),
            &header.signature,
        )
        .is_ok()
        .then(|| InternedString::from(context.as_ref()))
    })
}

pub use crate::sign::commitment::request::RequestProof as SignatureHeader;

impl SignatureHeader {
    pub async fn sign(signer: &Signer, body: &[u8], context: &str) -> Result<Self, Error> {
        let commitment = RequestCommitment::for_bytes(body);
        let signature = signer
            .sign_message(&request_signing_message(&commitment, context)?)
            .await?;
        Ok(Self {
            commitment,
            signer: signer.verifying_key().await?,
            signature,
        })
    }
}

const REQUEST_AUTH_TAG: &[u8] = b"Start-Auth-Sig v2\0";
const REQUEST_AUTH_TAG_V1: &[u8] = b"Start-Auth-Sig v1\0";

/// HTTP authentication wire constants projected for generated clients.
pub fn wire_profile() -> serde_json::Value {
    serde_json::json!({ "tag": REQUEST_AUTH_TAG, "header": AUTH_SIG_HEADER })
}

fn request_signing_message(
    commitment: &RequestCommitment,
    context: &str,
) -> Result<Vec<u8>, Error> {
    crate::sign::signing_message(REQUEST_AUTH_TAG, commitment, context)
}

/// The v1 layout, verified for clients that predate v2: the tag, the commitment, then the context.
fn request_signing_message_v1(commitment: &RequestCommitment, context: &str) -> Vec<u8> {
    let mut msg = REQUEST_AUTH_TAG_V1.to_vec();
    crate::sign::append_commitment(&mut msg, commitment);
    msg.extend_from_slice(context.as_bytes());
    msg
}

pub fn verify_request(
    key: &AnyVerifyingKey,
    commitment: &RequestCommitment,
    context: &str,
    signature: &AnySignature,
) -> Result<(), Error> {
    request_signing_message(commitment, context)
        .and_then(|message| verify_auth_message(key, signature, &message))
        .or_else(|_| {
            verify_auth_message(
                key,
                signature,
                &request_signing_message_v1(commitment, context),
            )
        })
}

fn verify_auth_message(
    key: &AnyVerifyingKey,
    signature: &AnySignature,
    message: &[u8],
) -> Result<(), Error> {
    match (key, signature) {
        (AnyVerifyingKey::Ed25519(key), AnySignature::Ed25519(signature)) => {
            key.verify_strict(message, signature)?;
            Ok(())
        }
        (_, AnySignature::Ed25519ph(_)) => Err(Error::new(
            eyre!("{}", t!("sign.unknown-signature-algorithm")),
            ErrorKind::InvalidSignature,
        )),
    }
}

impl<C: SignatureAuthorizationContext> Middleware<C> for SignatureAuth {
    type Metadata = Metadata<C::AdditionalMetadata>;
    async fn process_http_request(
        &mut self,
        context: &C,
        request: &mut Request,
    ) -> Result<(), axum::response::Response> {
        self.user_agent = request.headers().get(USER_AGENT).cloned();
        self.origin = RequestOrigin::from_extensions(request.extensions());
        if request
            .extensions()
            .get::<rpc_toolkit::WebSocketRequest>()
            .is_none()
            && request.headers().contains_key(AUTH_SIG_HEADER)
        {
            self.signer = Some(
                verify_request_signature_with_address(context, request)
                    .await
                    .map_err(RpcError::from),
            );
        }
        Ok(())
    }
    async fn process_rpc_request(
        &mut self,
        context: &C,
        metadata: Self::Metadata,
        request: &mut RpcRequest,
    ) -> Result<(), RpcResponse> {
        async {
            let verified = match context.secure_session() {
                Some(session) => Some((session.peer().clone(), session.address().clone())),
                None => self.signer.take().transpose()?,
            };
            let signer = verified.as_ref().map(|(signer, _)| signer);
            let login = metadata.additional.is_login();
            let db = context.db().peek().await;
            let res = context.check_pubkey(&db, signer, metadata.additional)?;
            if metadata.get_signer {
                if let Some(signer) = signer {
                    crate::middleware::params_object(&mut request.params)?
                        .insert("__Auth_signer".into(), to_value(signer)?);
                }
            }
            if metadata.get_user_agent {
                if let Some(user_agent) = self.user_agent.as_ref().and_then(|h| h.to_str().ok()) {
                    crate::middleware::params_object(&mut request.params)?
                        .insert("__Auth_userAgent".into(), to_value(&user_agent)?);
                }
            }
            if metadata.get_peer {
                if let Some(peer) = self.origin.peer_ip {
                    request.params["__Auth_peer"] = to_value(&peer)?;
                }
            }
            if login {
                if let Some((_, address)) = verified {
                    super::login_rate_limit::check(address, &self.origin)?;
                }
            }
            context.post_auth_hook(res, request).await?;
            Ok(())
        }
        .await
        .map_err(|e: Error| rpc_toolkit::RpcResponse::from_result(Err(e)))
    }
}

pub(crate) fn rpc_request_body(method: &str, params: Value) -> Result<String, RpcError> {
    use rpc_toolkit::yajrc::{GenericRpcMethod, Id, RpcRequest};
    serde_json::to_string(&RpcRequest {
        id: Some(Id::Number(0.into())),
        method: GenericRpcMethod::<_, _, Value>::new(method),
        params,
    })
    .map_err(Into::into)
}

pub async fn call_http(
    client: Client,
    url: Url,
    headers: HeaderMap,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    let body = rpc_request_body(method, params)?;
    send_http(client, url, headers, body, usize::MAX).await
}

pub(crate) async fn send_http(
    client: Client,
    url: Url,
    headers: HeaderMap,
    body: String,
    max_response_bytes: usize,
) -> Result<Value, RpcError> {
    use reqwest::Method;
    use reqwest::header::{ACCEPT, CONTENT_LENGTH, CONTENT_TYPE};
    use rpc_toolkit::RpcResponse;
    let req = client
        .request(Method::POST, url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .header(CONTENT_LENGTH, body.len())
        .headers(headers);
    let mut res = req.body(body).send().await?;

    if !res.status().is_success() {
        let status = res.status();
        let txt = res.text().await?;
        let mut res = Err(Error::new(
            eyre!("{}", status.canonical_reason().unwrap_or(status.as_str())),
            ErrorKind::Network,
        ));
        if !txt.is_empty() {
            res = res.with_ctx(|_| (ErrorKind::Network, txt));
        }
        return res.map_err(From::from);
    }

    match res
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        Some("application/json") => {
            let mut bytes = Vec::new();
            while let Some(chunk) = res.chunk().await? {
                if chunk.len() > max_response_bytes - bytes.len() {
                    return Err(Error::new(
                        eyre!("{}", t!("util.net.secure.invalid-message")),
                        ErrorKind::Network,
                    )
                    .into());
                }
                bytes.extend_from_slice(&chunk);
            }
            serde_json::from_slice::<RpcResponse>(&bytes)
                .with_kind(ErrorKind::Deserialization)?
                .result
        }
        _ => Err(Error::new(
            eyre!("{}", t!("middleware.auth.unknown-content-type")),
            ErrorKind::Network,
        )
        .into()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use http::HeaderValue;

    use super::*;
    use crate::sign::AnySigningKey;

    /// TypeScript-generated proof for seed 0102…1f20 and `BODY`.
    const JS_PRODUCED_HEADER: &str = "commitment=request&timestamp=1792644825&nonce=18095015305127205807&size=59&blake3=95o3MZRDgMasjyEKb6h2qMb1JFOs45lZdiY2qeXDRQY&signer=MCowBQYDK2VwAyEAebVWLo_mVPlAeLES6KmLp5AfhTrmlb7X4OORC60ElmQ&signature=MEkwBQYDK2VwBEBBctoB4q6Ez-Iuc8w7CBw6gz5ysJYD6F2fpz7Cb7A9mX96rUFD-IPWzjs3mt-Y6oBqDfYVVC-cVXbkU9UTGIsC";
    const JS_PRODUCED_HEADER_V1: &str = "timestamp=1784746349&nonce=5184603501117103523&size=59&blake3=95o3MZRDgMasjyEKb6h2qMb1JFOs45lZdiY2qeXDRQY&signer=MCowBQYDK2VwAyEAebVWLo_mVPlAeLES6KmLp5AfhTrmlb7X4OORC60ElmQ&signature=MEkwBQYDK2VwBEA0QgkL7GFycl72l-Bt076252XjpFG7aLngdxeIs-hNXpCkcJUtq9Xsfm_YV68U0JtK2qLMG2KPSfAVTcg8ntMH";
    const BODY: &[u8] = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"server.echo\",\"params\":{}}";

    #[test]
    fn clockless_nonce_eviction_bounds_both_indexes_without_admitting_retained_replays() {
        let mut cache = NonceCache {
            seen: (0..MAX_UNTIMED_NONCES as u64).collect(),
            order: (0..MAX_UNTIMED_NONCES as u64)
                .map(|nonce| (0, nonce))
                .collect(),
        };
        cache.handle_nonce(u64::MAX, 0, None).unwrap();
        assert_eq!(cache.seen.len(), MAX_UNTIMED_NONCES);
        assert_eq!(cache.order.len(), MAX_UNTIMED_NONCES);
        assert!(!cache.seen.contains(&0));
        assert!(cache.seen.contains(&u64::MAX));
        assert!(cache.handle_nonce(1, 0, None).is_err());
        assert!(cache.seen.contains(&1));
        assert_eq!(cache.order.len(), MAX_UNTIMED_NONCES);
    }

    #[test]
    fn timed_nonce_eviction_keeps_both_indexes_consistent() {
        let mut cache = NonceCache::default();
        cache.handle_nonce(1, 10, Some(10)).unwrap();
        cache.handle_nonce(2, 71, Some(71)).unwrap();
        assert_eq!(cache.seen, HashSet::from([2]));
        assert_eq!(cache.order, BTreeSet::from([(71, 2)]));
    }

    #[test]
    fn verifies_js_produced_signature_headers() {
        for raw in [JS_PRODUCED_HEADER, JS_PRODUCED_HEADER_V1] {
            let header = SignatureHeader::from_header(&HeaderValue::from_static(raw))
                .expect("header parses");
            assert_eq!(header.commitment.size, BODY.len() as u64);
            assert_eq!(header.commitment.blake3.0, *blake3::hash(BODY).as_bytes());
            verify_request(
                &header.signer,
                &header.commitment,
                "start-9.local",
                &header.signature,
            )
            .expect("signature verifies with the signing context");
            verify_request(
                &header.signer,
                &header.commitment,
                "other-host.local",
                &header.signature,
            )
            .expect_err("signature does not verify under a different context");
        }
    }

    #[test]
    fn loopback_identity_is_the_addressed_loopback_ip() {
        let addressed_to = |host: &str| {
            let mut request = http::Request::builder()
                .header(http::header::HOST, host)
                .body(Body::empty())
                .unwrap();
            request
                .extensions_mut()
                .insert(crate::net::web_server::TcpMetadata {
                    peer_addr: "127.0.0.1:1234".parse().unwrap(),
                    local_addr: "127.0.0.1:8989".parse().unwrap(),
                });
            loopback_identity(&request)
        };
        assert_eq!(
            addressed_to("127.1.1.19:8989").as_deref(),
            Some("127.1.1.19")
        );
        assert_eq!(addressed_to("[::1]:8080").as_deref(), Some("[::1]"));
        assert_eq!(addressed_to("192.168.1.50:8989"), None);
        assert_eq!(addressed_to("localhost:8989"), None);
    }

    #[test]
    fn remote_requests_cannot_supply_loopback_signing_identities() {
        let mut request = http::Request::builder()
            .header(http::header::HOST, "127.8.9.10:80")
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(crate::net::web_server::TcpMetadata {
                peer_addr: "203.0.113.7:1234".parse().unwrap(),
                local_addr: "192.168.1.2:80".parse().unwrap(),
            });
        assert_eq!(loopback_identity(&request), None);
    }

    #[tokio::test]
    async fn login_address_is_the_verified_configured_identity() {
        let key = Signer::Key(AnySigningKey::Ed25519(
            ed25519_dalek::SigningKey::from_bytes(&[7; 32]),
        ));
        let header = SignatureHeader::sign(&key, BODY, "server.example")
            .await
            .unwrap();
        assert_eq!(
            verified_address(&header, ["other.example", "server.example"]).as_deref(),
            Some("server.example")
        );
        let unconfigured = SignatureHeader::sign(&key, BODY, "attacker.example")
            .await
            .unwrap();
        assert_eq!(
            verified_address(&unconfigured, ["other.example", "server.example"]),
            None
        );
    }

    fn ingress_binding() -> BindInfo {
        use crate::net::host::binding::{AddSslOptions, BindOptions, DerivedAddressInfo, NetInfo};
        use crate::net::service_interface::{
            AddressInfo, HostnameInfo, HostnameMetadata, ServiceInterface, ServiceInterfaceType,
        };

        let address = |hostname: &str, public, metadata| HostnameInfo {
            hostname: hostname.into(),
            public,
            metadata,
            ssl: true,
            port: Some(443),
        };
        let domain = |hostname, public, gateway: &str| {
            address(
                hostname,
                public,
                if public {
                    HostnameMetadata::PublicDomain {
                        gateway: gateway.parse().unwrap(),
                    }
                } else {
                    HostnameMetadata::PrivateDomain {
                        gateways: [gateway.parse().unwrap()].into_iter().collect(),
                    }
                },
            )
        };
        let ip = |hostname: &str, public, gateway: &str| {
            address(
                hostname,
                public,
                if hostname.contains(':') {
                    HostnameMetadata::Ipv6 {
                        gateway: gateway.parse().unwrap(),
                        scope_id: 0,
                    }
                } else {
                    HostnameMetadata::Ipv4 {
                        gateway: gateway.parse().unwrap(),
                    }
                },
            )
        };
        let interface = ServiceInterface {
            id: "admin-ui".parse().unwrap(),
            name: "StartOS UI".into(),
            description: String::new(),
            masked: false,
            address_info: AddressInfo {
                username: None,
                host_id: crate::HostId::admin(),
                internal_port: 80,
                scheme: Some("http".into()),
                ssl_scheme: Some("https".into()),
                suffix: String::new(),
            },
            interface_type: ServiceInterfaceType::Ui,
            preferred_launcher_address: None,
        };
        BindInfo {
            enabled: true,
            options: BindOptions {
                preferred_external_port: 80,
                secure: None,
                proxy: None,
                add_ssl: Some(AddSslOptions {
                    preferred_external_port: 443,
                    alpn: None,
                    upstream_cert_validation: Default::default(),
                }),
            },
            net: NetInfo {
                assigned_port: Some(80),
                assigned_ssl_port: Some(443),
            },
            interfaces: [(interface.id.clone(), interface)].into_iter().collect(),
            addresses: DerivedAddressInfo {
                available: [
                    domain("public.example", true, "eth0"),
                    domain("private.example", false, "eth0"),
                    domain("other.example", true, "eth1"),
                    domain("dual.example", true, "eth0"),
                    domain("dual.example", false, "eth1"),
                    address(
                        "start-9.local",
                        false,
                        HostnameMetadata::Mdns {
                            gateways: ["eth0".parse().unwrap()].into_iter().collect(),
                        },
                    ),
                    ip("192.168.1.2", false, "eth0"),
                    ip("203.0.113.2", true, "eth0"),
                    ip("2001:db8:1::2", false, "eth0"),
                    ip("2001:db8:1::3", true, "eth0"),
                    ip("127.0.0.1", false, "lo"),
                    ip("10.0.3.1", false, "lxcbr0"),
                    address(
                        "service.onion",
                        true,
                        HostnameMetadata::Plugin {
                            package_id: "tor".parse().unwrap(),
                            remove_action: None,
                            overflow_actions: Vec::new(),
                            info: Value::Null,
                        },
                    ),
                ]
                .into_iter()
                .collect(),
                enabled: [
                    "203.0.113.2:443".parse().unwrap(),
                    "[2001:db8:1::3]:443".parse().unwrap(),
                ]
                .into_iter()
                .collect(),
                ..Default::default()
            },
        }
    }

    fn ingress(gateway: &str, public: bool) -> RequestOrigin {
        RequestOrigin {
            peer_ip: Some(
                if public {
                    "198.51.100.7"
                } else {
                    "192.168.1.7"
                }
                .parse()
                .unwrap(),
            ),
            local_ip: Some("192.168.1.2".parse().unwrap()),
            gateway: Some(gateway.parse().unwrap()),
            public,
        }
    }

    #[test]
    fn signing_identities_follow_ingress_gateway_and_public_exposure() {
        let binding = ingress_binding();
        for (origin, expected) in [
            (
                ingress("eth0", true),
                vec![
                    "public.example",
                    "dual.example",
                    "203.0.113.2",
                    "[2001:db8:1::3]",
                ],
            ),
            (ingress("eth1", true), vec!["other.example"]),
            (
                ingress("eth0", false),
                vec![
                    "public.example",
                    "private.example",
                    "dual.example",
                    "start-9.local",
                    "192.168.1.2",
                    "203.0.113.2",
                    "[2001:db8:1::2]",
                    "[2001:db8:1::3]",
                ],
            ),
            (
                ingress("eth1", false),
                vec!["other.example", "dual.example"],
            ),
            (ingress("eth2", false), vec![]),
        ] {
            let actual = startos_signing_identities(Some(&binding), &origin);
            assert_eq!(
                actual,
                expected.into_iter().map(InternedString::from).collect(),
                "{origin:?}"
            );
        }
        let mut unknown = ingress("eth0", true);
        unknown.gateway = None;
        assert!(startos_signing_identities(Some(&binding), &unknown).is_empty());
    }

    #[test]
    fn signing_identities_respect_binding_and_address_enablement() {
        let mut binding = ingress_binding();
        let origin = ingress("eth0", false);
        binding
            .addresses
            .disabled
            .insert(("private.example".into(), 443));
        binding
            .addresses
            .enabled
            .remove(&"203.0.113.2:443".parse().unwrap());
        let identities = startos_signing_identities(Some(&binding), &origin);
        assert!(!identities.contains("private.example"));
        assert!(!identities.contains("203.0.113.2"));
        assert!(identities.contains("public.example"));
        binding.enabled = false;
        assert!(startos_signing_identities(Some(&binding), &origin).is_empty());
        binding.enabled = true;
        binding.interfaces.clear();
        assert!(startos_signing_identities(Some(&binding), &origin).is_empty());
    }

    #[test]
    fn internal_ingress_retains_plugin_names_but_localhost_requires_loopback() {
        let binding = ingress_binding();
        let bridge_v6 = crate::net::forward::START9_BRIDGE_V6_ADDR.to_string();
        for (destination, peer, gateway, loopback) in [
            ("127.0.0.1", "127.0.0.1", "lo", true),
            ("::1", "::1", "lo", true),
            ("10.0.3.1", "10.0.3.2", "lxcbr0", false),
            (bridge_v6.as_str(), "fd00::2", "lxcbr0", false),
        ] {
            let mut origin = ingress(gateway, false);
            origin.local_ip = Some(destination.parse().unwrap());
            origin.peer_ip = Some(peer.parse().unwrap());
            let identities = startos_signing_identities(Some(&binding), &origin);
            assert!(identities.contains("service.onion"));
            assert!(identities.contains("public.example"));
            assert!(identities.contains("private.example"));
            assert!(identities.contains("other.example"));
            assert_eq!(identities.contains("localhost"), loopback);
        }
        assert!(
            !startos_signing_identities(Some(&binding), &ingress("eth0", true))
                .contains("service.onion")
        );
        assert!(
            !startos_signing_identities(Some(&binding), &ingress("eth0", false))
                .contains("service.onion")
        );
        let mut loopback = ingress("lo", false);
        loopback.peer_ip = Some("127.0.0.1".parse().unwrap());
        loopback.local_ip = loopback.peer_ip;
        assert_eq!(
            startos_signing_identities(None, &loopback),
            [InternedString::from("localhost")].into_iter().collect()
        );
    }

    #[tokio::test]
    async fn signed_non_login_requests_reject_identities_from_other_ingress_scopes() {
        let binding = ingress_binding();
        let key = Signer::Key(AnySigningKey::Ed25519(
            ed25519_dalek::SigningKey::from_bytes(&[7; 32]),
        ));
        for (hostname, origin, accepted) in [
            ("public.example", ingress("eth0", true), true),
            ("public.example", ingress("eth1", true), false),
            ("private.example", ingress("eth0", true), false),
            ("private.example", ingress("eth0", false), true),
            ("localhost", ingress("eth0", true), false),
            ("127.0.0.1", ingress("eth0", true), false),
            ("service.onion", ingress("eth0", true), false),
        ] {
            let header = SignatureHeader::sign(&key, BODY, hostname).await.unwrap();
            assert_eq!(
                verified_address(&header, startos_signing_identities(Some(&binding), &origin))
                    .is_some(),
                accepted,
                "{hostname} via {origin:?}"
            );
        }
    }

    #[test]
    fn rejects_unknown_commitment_type() {
        let header = JS_PRODUCED_HEADER.replacen("commitment=request", "commitment=bogus", 1);
        assert!(SignatureHeader::from_header(&HeaderValue::from_str(&header).unwrap()).is_err());
    }

    #[tokio::test]
    async fn compact_header_round_trip() {
        let key = Signer::Key(AnySigningKey::Ed25519(
            ed25519_dalek::SigningKey::from_bytes(&[2; 32]),
        ));
        let header = SignatureHeader::sign(&key, BODY, "start-9.local")
            .await
            .expect("signs");
        let value = header.to_header();
        assert!(
            !value.to_str().unwrap().contains("BEGIN"),
            "compact form must not be PEM-armored"
        );
        let query: BTreeMap<_, _> = form_urlencoded::parse(value.as_bytes()).collect();
        assert_eq!(
            query["signer"],
            "MCowBQYDK2VwAyEAgTl3Dqh9F19Wo1Rmw0x-zMuNipG07jeiXfYPW4_Js5Q"
        );
        for field in ["signer", "signature"] {
            assert!(!query[field].contains(['+', '/', '=']));
        }
        let parsed = SignatureHeader::from_header(&value).expect("compact header parses");
        verify_request(
            &parsed.signer,
            &parsed.commitment,
            "start-9.local",
            &parsed.signature,
        )
        .expect("compact round trip verifies");
        let (AnyVerifyingKey::Ed25519(vk), AnySignature::Ed25519(sig)) =
            (&parsed.signer, &parsed.signature)
        else {
            panic!("request signatures are pure Ed25519");
        };
        assert!(
            vk.verify_strict(
                &request_signing_message_v1(&parsed.commitment, "start-9.local"),
                sig
            )
            .is_err(),
            "clients sign the v2 layout"
        );
    }
}
