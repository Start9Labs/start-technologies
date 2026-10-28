use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::future::Future;
use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::Request;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::TimeDelta;
use http::header::USER_AGENT;
use http::{Extensions, HeaderMap, HeaderValue};
use itertools::Itertools;
use reqwest::Client;
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{Middleware, RpcRequest, RpcResponse};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::auth::AuthKeys;
use crate::context::{CliContext, RpcContext};
use crate::middleware::auth::DbContext;
use crate::net::host::binding::BindInfo;
use crate::net::http::request_authority;
use crate::net::request_origin::RequestOrigin;
use crate::prelude::*;
use crate::rpc_continuations::OpenAuthedContinuations;
use crate::sign::commitment::Commitment;
use crate::sign::commitment::request::RequestCommitment;
use crate::sign::{AnySignature, AnyVerifyingKey, Signer};
use crate::util::serde::{Base64, DerEncoding};
use crate::util::sync::SyncMutex;

pub const AUTH_SIG_HEADER: &str = "X-Start-Auth-Sig";

/// Maximum initial allocation from an untrusted commitment size.
const MAX_BODY_PREALLOC: u64 = 16 * 1024 * 1024;

/// RPC-metadata fields understood by [`SignatureAuth`] when layered over an
/// [`RpcContext`]. `login` marks the enrollment endpoint: the request must
/// still be signed (proving possession of the key being enrolled), but the
/// key need not be registered yet.
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

pub trait SignatureAuthContext: DbContext {
    type AdditionalMetadata: DeserializeOwned + SignatureAuthMetadata + Send;
    type CheckPubkeyRes: Send;
    /// How far a key's persisted `last_active` may lag its latest request.
    /// Zero persists every request.
    const LAST_ACTIVE_RESOLUTION: TimeDelta = TimeDelta::zero();
    fn mutate_nonce_cache<F: FnOnce(&mut NonceCache) -> T, T>(&self, f: F) -> T;
    /// Whether the clock can be trusted - otherwise nonces are LRU evicted
    fn clock_synced(&self) -> impl Future<Output = bool> + Send + Sync;
    /// Live continuations (REST/WS) registered under the signer that opened
    /// them, so revoking a signer kills anything it left open. Keyed by the
    /// signer's PEM (`None` for continuations opened without one).
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
    /// Signing identities allowed for the request's ingress.
    fn sig_context(
        &self,
        extensions: &Extensions,
    ) -> impl Future<Output = impl IntoIterator<Item = Result<impl AsRef<str> + Send, Error>> + Send>
    + Send;
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
/// Obtained via [`SignatureAuthContext::unenroll`], or [`Self::unenroll`]
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
    pub fn unenroll<C: SignatureAuthContext>(
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
    type AdditionalMetadata = LoginMetadata;
    type CheckPubkeyRes = Option<InternedString>;
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
    fn check_pubkey(
        &self,
        db: &Model<Self::Database>,
        pubkey: Option<&AnyVerifyingKey>,
        metadata: Self::AdditionalMetadata,
    ) -> Result<Self::CheckPubkeyRes, Error> {
        check_enrolled(pubkey, metadata.login, |key| {
            Ok(self
                .ephemeral_auth_keys
                .peek(|keys| keys.0.contains_key(&**key))
                || db
                    .as_private()
                    .as_sessions()
                    .as_pubkeys()
                    .as_idx(key)
                    .is_some())
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

pub trait SigningContext {
    fn signing_key(&self) -> Result<Signer, Error>;
}

impl SigningContext for CliContext {
    fn signing_key(&self) -> Result<Signer, Error> {
        Ok(self.id_key()?.clone())
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

/// Replay cache for the last 60s of nonces. `seen` gives O(1) membership;
/// `order` drives eviction, keyed by `(seen_at, nonce)` so two nonces recorded
/// at the same `Instant` can't collide and silently evict each other.
///
/// BEST EFFORT: security degrades when clock is not synced
#[derive(Default)]
pub struct NonceCache {
    seen: HashSet<u64>,
    order: BTreeSet<(i64, u64)>,
}
impl NonceCache {
    fn handle_nonce(&mut self, nonce: u64, timestamp: i64, now: Option<i64>) -> Result<(), Error> {
        if let Some(now) = now {
            while let Some(&(timestamp, n)) = self.order.iter().next() {
                if now.saturating_sub(timestamp) > SIG_TIME_GRACE_PERIOD_SECS as i64 {
                    self.order.remove(&(timestamp, n));
                    self.seen.remove(&n);
                } else {
                    break;
                }
            }
        } else {
            while self.order.len() > 1024 * 1024 {
                self.order.pop_first();
            }
        }
        // Not `Authorization`: clients treat 34 as key revocation and log out,
        // but a replay can be an innocent transport retransmit.
        if !self.seen.insert(nonce) {
            return Err(Error::new(
                eyre!("{}", t!("middleware.auth.replay-attack-detected")),
                ErrorKind::InvalidSignature,
            ));
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

    let address = {
        let sig_contexts = context
            .sig_context(request.extensions())
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let loopback = loopback_identity(request);
        let address = verified_address(
            &header,
            sig_contexts
                .iter()
                .map(|c| c.as_ref())
                .chain(loopback.as_deref()),
        );
        let Some(address) = address else {
            tracing::debug!(
                signer = ?header.signer,
                contexts = ?sig_contexts.iter().map(|c| c.as_ref()).collect::<Vec<_>>(),
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
    context.mutate_nonce_cache(|n| n.handle_nonce(commitment.nonce, commitment.timestamp, now))?;

    let mut body = Vec::with_capacity(commitment.size.min(MAX_BODY_PREALLOC) as usize);
    commitment.copy_to(request, &mut body).await?;
    *request.body_mut() = Body::from(body);

    Ok((signer, address))
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

pub struct SignatureHeader {
    pub commitment: RequestCommitment,
    pub signer: AnyVerifyingKey,
    pub signature: AnySignature,
}
impl SignatureHeader {
    pub fn to_header(&self) -> HeaderValue {
        let mut url: Url = "http://localhost".parse().unwrap();
        self.commitment.append_query(&mut url);
        url.query_pairs_mut().append_pair(
            "signer",
            &encode_header_der(&self.signer).expect("encode verifying key"),
        );
        url.query_pairs_mut().append_pair(
            "signature",
            &encode_header_der(&self.signature).expect("encode signature"),
        );
        HeaderValue::from_str(url.query().unwrap_or_default()).unwrap()
    }
    pub fn from_header(header: &HeaderValue) -> Result<Self, Error> {
        let query: BTreeMap<_, _> = form_urlencoded::parse(header.as_bytes()).collect();
        Ok(Self {
            commitment: RequestCommitment::from_query(&header)?,
            signer: decode_header_der(query.get("signer").or_not_found("signer")?)?,
            signature: decode_header_der(query.get("signature").or_not_found("signature")?)?,
        })
    }
    pub async fn sign(signer: &Signer, body: &[u8], context: &str) -> Result<Self, Error> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_else(|e| e.duration().as_secs() as i64 * -1);
        let nonce = rand::random();
        let commitment = RequestCommitment {
            timestamp,
            nonce,
            size: body.len() as u64,
            blake3: Base64(*blake3::hash(body).as_bytes()),
        };
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

fn encode_header_der(value: &impl DerEncoding) -> Result<String, Error> {
    Ok(URL_SAFE_NO_PAD.encode(
        value
            .to_der::<serde_json::Error>()
            .with_kind(ErrorKind::Serialization)?,
    ))
}

fn decode_header_der<T: DerEncoding>(value: &str) -> Result<T, Error> {
    let der = URL_SAFE_NO_PAD
        .decode(value)
        .with_kind(ErrorKind::Deserialization)?;
    T::from_der::<serde_json::Error>(&der).with_kind(ErrorKind::Deserialization)
}

const REQUEST_AUTH_TAG: &[u8] = b"Start-Auth-Sig v2\0";
const REQUEST_AUTH_TAG_V1: &[u8] = b"Start-Auth-Sig v1\0";

/// The message a request signature commits to, bound to the server identity
/// (hostname/IP/domain). Signed with pure Ed25519, so any WebCrypto client can produce it.
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
    match (key, signature) {
        (AnyVerifyingKey::Ed25519(key), AnySignature::Ed25519(signature)) => {
            match request_signing_message(commitment, context) {
                Ok(msg) if key.verify_strict(&msg, signature).is_ok() => Ok(()),
                _ => Ok(
                    key.verify_strict(&request_signing_message_v1(commitment, context), signature)?
                ),
            }
        }
        (_, AnySignature::Ed25519ph(_)) => Err(Error::new(
            eyre!("{}", t!("sign.unknown-signature-algorithm")),
            ErrorKind::InvalidSignature,
        )),
    }
}

impl<C: SignatureAuthContext> Middleware<C> for SignatureAuth {
    type Metadata = Metadata<C::AdditionalMetadata>;
    async fn process_http_request(
        &mut self,
        context: &C,
        request: &mut Request,
    ) -> Result<(), axum::response::Response> {
        self.user_agent = request.headers().get(USER_AGENT).cloned();
        if request.headers().contains_key(AUTH_SIG_HEADER) {
            self.signer = Some(
                verify_request_signature_with_address(context, request)
                    .await
                    .map_err(RpcError::from),
            );
        }
        self.origin = RequestOrigin::from_extensions(request.extensions());
        Ok(())
    }
    async fn process_rpc_request(
        &mut self,
        context: &C,
        metadata: Self::Metadata,
        request: &mut RpcRequest,
    ) -> Result<(), RpcResponse> {
        async {
            let verified = self.signer.take().transpose()?;
            let signer = verified.as_ref().map(|(signer, _)| signer);
            let login = metadata.additional.is_login();
            let db = context.db().peek().await;
            let res = context.check_pubkey(&db, signer, metadata.additional)?;
            // Injected only for an accepted signer: another method may still
            // admit the request after a rejected one.
            if metadata.get_signer {
                if let Some(signer) = signer {
                    request.params["__Auth_signer"] = to_value(signer)?;
                }
            }
            if metadata.get_user_agent {
                if let Some(user_agent) = self.user_agent.as_ref().and_then(|h| h.to_str().ok()) {
                    request.params["__Auth_userAgent"] = to_value(&user_agent)?;
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

pub async fn call_remote<Ctx: SigningContext + AsRef<Client>>(
    ctx: &Ctx,
    url: Url,
    headers: HeaderMap,
    sig_context: Option<&str>,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    let signer = sig_context.and_then(|c| Some((ctx.signing_key().ok()?, c)));
    call_remote_with_client(signer, ctx.as_ref().clone(), url, headers, method, params).await
}

/// Signs the request when given a signer and the server identity to bind it to.
pub(crate) async fn call_remote_with_client(
    signer: Option<(Signer, &str)>,
    client: Client,
    url: Url,
    headers: HeaderMap,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    use reqwest::Method;
    use reqwest::header::{ACCEPT, CONTENT_LENGTH, CONTENT_TYPE};
    use rpc_toolkit::RpcResponse;
    use rpc_toolkit::yajrc::{GenericRpcMethod, Id, RpcRequest};

    let rpc_req = RpcRequest {
        id: Some(Id::Number(0.into())),
        method: GenericRpcMethod::<_, _, Value>::new(method),
        params,
    };
    let body = serde_json::to_vec(&rpc_req)?;
    let mut req = client
        .request(Method::POST, url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .header(CONTENT_LENGTH, body.len())
        .headers(headers);
    if let Some((key, sig_ctx)) = signer {
        req = req.header(
            AUTH_SIG_HEADER,
            SignatureHeader::sign(&key, &body, sig_ctx)
                .await?
                .to_header(),
        );
    }
    let res = req.body(body).send().await?;

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
            serde_json::from_slice::<RpcResponse>(&*res.bytes().await?)
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
    use http::HeaderValue;

    use super::*;
    use crate::sign::AnySigningKey;

    /// Produced by the TypeScript client (`lib/auth/signature.ts`) for secret
    /// key 0102…1f20, context "start-9.local", and the JSON body below. Guards
    /// the byte-level contract between the browser signer and this verifier.
    const JS_PRODUCED_HEADER: &str = "commitment=request&timestamp=1792644825&nonce=18095015305127205807&size=59&blake3=95o3MZRDgMasjyEKb6h2qMb1JFOs45lZdiY2qeXDRQY&signer=MCowBQYDK2VwAyEAebVWLo_mVPlAeLES6KmLp5AfhTrmlb7X4OORC60ElmQ&signature=MEkwBQYDK2VwBEBBctoB4q6Ez-Iuc8w7CBw6gz5ysJYD6F2fpz7Cb7A9mX96rUFD-IPWzjs3mt-Y6oBqDfYVVC-cVXbkU9UTGIsC";
    /// The same inputs signed by the v1 client.
    const JS_PRODUCED_HEADER_V1: &str = "timestamp=1784746349&nonce=5184603501117103523&size=59&blake3=95o3MZRDgMasjyEKb6h2qMb1JFOs45lZdiY2qeXDRQY&signer=MCowBQYDK2VwAyEAebVWLo_mVPlAeLES6KmLp5AfhTrmlb7X4OORC60ElmQ&signature=MEkwBQYDK2VwBEA0QgkL7GFycl72l-Bt076252XjpFG7aLngdxeIs-hNXpCkcJUtq9Xsfm_YV68U0JtK2qLMG2KPSfAVTcg8ntMH";
    const BODY: &[u8] = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"server.echo\",\"params\":{}}";

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
                add_ssl: Some(AddSslOptions {
                    preferred_external_port: 443,
                    add_x_forwarded_headers: false,
                    alpn: None,
                    auth: None,
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
