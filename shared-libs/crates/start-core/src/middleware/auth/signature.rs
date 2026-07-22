use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::future::Future;
use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::Request;
use chrono::Utc;
use http::header::USER_AGENT;
use http::{HeaderMap, HeaderValue};
use reqwest::Client;
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{Middleware, RpcRequest, RpcResponse};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::auth::AuthKeys;
use crate::context::{CliContext, RpcContext};
use crate::middleware::auth::DbContext;
use crate::prelude::*;
use crate::rpc_continuations::OpenAuthedContinuations;
use crate::sign::commitment::Commitment;
use crate::sign::commitment::request::RequestCommitment;
use crate::sign::{AnySignature, AnySigningKey, AnyVerifyingKey};
use crate::util::iter::TransposeResultIterExt;
use crate::util::serde::Base64;
use crate::util::sync::SyncMutex;

pub const AUTH_SIG_HEADER: &str = "X-Start-Auth-Sig";

/// Upper bound on how much we pre-reserve for a request body from the
/// attacker-controlled `commitment.size`. A forged self-signature can carry any
/// size, so an unbounded `with_capacity` would let one tiny request drive a huge
/// allocation before enrollment is ever checked; the verifier still enforces the
/// true size/hash as the body streams in, so a larger legitimate body just grows.
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

/// Shared body of the [`SignatureAuthContext::check_pubkey`] impls: reject an
/// unsigned request, let an as-yet-unenrolled key through the login endpoint,
/// and otherwise require the key to be enrolled per the context's `is_enrolled`
/// lookup. Returns the enrolled key so `post_auth_hook` needn't re-encode it.
pub(crate) fn check_enrolled(
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
    let key = pubkey.interned_pem();
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
    type AdditionalMetadata: DeserializeOwned + Send;
    type CheckPubkeyRes: Send;
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
    fn sig_context(
        &self,
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
#[derive(Serialize, Deserialize)]
pub struct HasUnenrolledKeys(());
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
        let auth_keys = db.as_private_mut().as_session_pubkeys_mut();
        for key in keys {
            auth_keys.remove(key)?;
        }
        Ok(())
    }
    async fn sig_context(
        &self,
    ) -> impl IntoIterator<Item = Result<impl AsRef<str> + Send, Error>> + Send {
        let peek = self.db.peek().await;
        self.account.peek(|a| {
            let ips: Vec<Result<InternedString, Error>> = match peek
                .as_public()
                .as_server_info()
                .as_network()
                .as_gateways()
                .de()
            {
                Ok(gateways) => gateways
                    .values()
                    .filter_map(|g| g.ip_info.clone())
                    .flat_map(|info| {
                        // The interface's own addresses (subnets), not its
                        // gateway (`lan_ip`), plus the public IP for clients
                        // reaching the server through a port forward.
                        info.subnets
                            .iter()
                            .map(|net| net.addr())
                            .chain(info.wan_ip.map(IpAddr::V4))
                            .map(|ip| url_host_str(ip))
                            .collect::<Vec<_>>()
                    })
                    .map(Ok)
                    .collect(),
                Err(e) => vec![Err(e)],
            };
            a.hostnames()
                .into_iter()
                .map(Ok)
                .chain(
                    peek.as_public()
                        .as_server_info()
                        .as_network()
                        .as_host()
                        .as_public_domains()
                        .keys()
                        .map(|k| k.into_iter())
                        .transpose(),
                )
                .chain(
                    peek.as_public()
                        .as_server_info()
                        .as_network()
                        .as_host()
                        .as_private_domains()
                        .keys()
                        .map(|k| k.into_iter())
                        .transpose(),
                )
                .chain(ips)
                // The loopback name, alongside the 127.0.0.1 / [::1] addresses
                // the loopback interface's subnets already contribute.
                .chain(std::iter::once(Ok(InternedString::intern("localhost"))))
                .collect::<Vec<_>>()
        })
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
                    .as_session_pubkeys()
                    .de()?
                    .0
                    .contains_key(&**key))
        })
    }
    async fn post_auth_hook(&self, key: Self::CheckPubkeyRes, _: &RpcRequest) -> Result<(), Error> {
        if let Some(key) = key {
            let ephemeral = self.ephemeral_auth_keys.mutate(|keys| {
                if let Some(entry) = keys.0.get_mut(&*key) {
                    entry.last_active = Utc::now();
                    true
                } else {
                    false
                }
            });
            if !ephemeral {
                self.db
                    .mutate(|db| {
                        db.as_private_mut().as_session_pubkeys_mut().mutate(|keys| {
                            if let Some(entry) = keys.0.get_mut(&*key) {
                                entry.last_active = Utc::now();
                            }
                            Ok(())
                        })
                    })
                    .await
                    .result?;
            }
        }
        Ok(())
    }
}

/// Format an IP the way `url::Url::host_str` (and `location.hostname`) renders
/// it, so signature contexts match regardless of how the server was addressed.
pub(crate) fn url_host_str(ip: IpAddr) -> InternedString {
    match ip {
        IpAddr::V4(ip) => InternedString::from_display(&ip),
        IpAddr::V6(ip) => InternedString::from_display(&lazy_format!("[{ip}]")),
    }
}

pub trait SigningContext {
    fn signing_key(&self) -> Result<AnySigningKey, Error>;
}

impl SigningContext for CliContext {
    fn signing_key(&self) -> Result<AnySigningKey, Error> {
        Ok(AnySigningKey::Ed25519(self.id_key()?.clone()))
    }
}

impl SigningContext for RpcContext {
    fn signing_key(&self) -> Result<AnySigningKey, Error> {
        Ok(AnySigningKey::Ed25519(
            self.account.peek(|a| a.developer_key.clone()),
        ))
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
    signer: Option<Result<AnyVerifyingKey, RpcError>>,
    user_agent: Option<HeaderValue>,
}
impl SignatureAuth {
    pub fn new() -> Self {
        Self {
            signer: None,
            user_agent: None,
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
    let SignatureHeader {
        commitment,
        signer,
        signature,
    } = SignatureHeader::from_header(
        request
            .headers()
            .get(AUTH_SIG_HEADER)
            .or_not_found(AUTH_SIG_HEADER)
            .with_kind(ErrorKind::InvalidRequest)?,
    )?;

    let sig_contexts = context
        .sig_context()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let verified = sig_contexts.iter().any(|sig_context| {
        verify_request(&signer, &commitment, sig_context.as_ref(), &signature).is_ok()
    });
    if !verified {
        tracing::debug!(
            ?signer,
            contexts = ?sig_contexts.iter().map(|c| c.as_ref()).collect::<Vec<_>>(),
            "request signature failed verification for every known server identity"
        );
        return Err(Error::new(
            eyre!("{}", t!("middleware.auth.invalid-request-signature")),
            ErrorKind::Authorization,
        ));
    }

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

    Ok(signer)
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
            &self.signer.to_base64_der().expect("encode verifying key"),
        );
        url.query_pairs_mut().append_pair(
            "signature",
            &self.signature.to_base64_der().expect("encode signature"),
        );
        HeaderValue::from_str(url.query().unwrap_or_default()).unwrap()
    }
    pub fn from_header(header: &HeaderValue) -> Result<Self, Error> {
        let query: BTreeMap<_, _> = form_urlencoded::parse(header.as_bytes()).collect();
        Ok(Self {
            commitment: RequestCommitment::from_query(&header)?,
            signer: AnyVerifyingKey::from_base64_der(query.get("signer").or_not_found("signer")?)?,
            signature: AnySignature::from_base64_der(
                query.get("signature").or_not_found("signature")?,
            )?,
        })
    }
    pub fn sign(signer: &AnySigningKey, body: &[u8], context: &str) -> Result<Self, Error> {
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
        let signature = sign_request(signer, &commitment, context)?;
        Ok(Self {
            commitment,
            signer: signer.verifying_key(),
            signature,
        })
    }
}

/// Protocol tag prefixed to request-auth signing messages: cross-protocol
/// separation so an RPC signature can never collide with a package/registry
/// signature (which use the Ed25519ph context parameter for the same job).
const REQUEST_AUTH_TAG: &[u8] = b"Start-Auth-Sig v1\0";

/// The message a request signature commits to: a fixed protocol tag, the
/// commitment, then the server identity (hostname/IP/domain) in the signed
/// bytes. Signed with pure Ed25519, so any WebCrypto client can produce it.
fn request_signing_message(commitment: &RequestCommitment, context: &str) -> Vec<u8> {
    use crate::sign::commitment::Digestable;

    struct Sink<'a>(&'a mut Vec<u8>);
    impl digest::Update for Sink<'_> {
        fn update(&mut self, data: &[u8]) {
            self.0.extend_from_slice(data);
        }
    }

    let mut msg = Vec::with_capacity(REQUEST_AUTH_TAG.len() + 56 + context.len());
    msg.extend_from_slice(REQUEST_AUTH_TAG);
    commitment.update(&mut Sink(&mut msg));
    msg.extend_from_slice(context.as_bytes());
    msg
}

pub fn sign_request(
    key: &AnySigningKey,
    commitment: &RequestCommitment,
    context: &str,
) -> Result<AnySignature, Error> {
    use ed25519_dalek::Signer;

    let msg = request_signing_message(commitment, context);
    match key {
        AnySigningKey::Ed25519(key) => Ok(AnySignature::Ed25519(key.sign(&msg))),
    }
}

pub fn verify_request(
    key: &AnyVerifyingKey,
    commitment: &RequestCommitment,
    context: &str,
    signature: &AnySignature,
) -> Result<(), Error> {
    let msg = request_signing_message(commitment, context);
    match (key, signature) {
        (AnyVerifyingKey::Ed25519(key), AnySignature::Ed25519(signature)) => {
            key.verify_strict(&msg, signature)?;
            Ok(())
        }
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
                verify_request_signature(context, request)
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
            let signer = self.signer.take().transpose()?;
            if metadata.get_signer {
                if let Some(signer) = &signer {
                    request.params["__Auth_signer"] = to_value(signer)?;
                }
            }
            if metadata.get_user_agent {
                if let Some(user_agent) = self.user_agent.as_ref().and_then(|h| h.to_str().ok()) {
                    request.params["__Auth_userAgent"] = to_value(&user_agent)?;
                }
            }
            let db = context.db().peek().await;
            let res = context.check_pubkey(&db, signer.as_ref(), metadata.additional)?;
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
    let mut req = ctx
        .as_ref()
        .request(Method::POST, url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .header(CONTENT_LENGTH, body.len())
        .headers(headers);
    if let (Some(sig_ctx), Ok(key)) = (sig_context, ctx.signing_key()) {
        req = req.header(
            AUTH_SIG_HEADER,
            SignatureHeader::sign(&key, &body, sig_ctx)?.to_header(),
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

    /// Generated with the TypeScript client's message layout
    /// (`lib/auth/signature.ts`) for secret key 0102…1f20, context
    /// "start-9.local", and the JSON body below. Guards the byte-level
    /// contract between the browser signer and this verifier.
    const JS_PRODUCED_HEADER: &str = "timestamp=1784746349&nonce=5184603501117103523&size=59&blake3=95o3MZRDgMasjyEKb6h2qMb1JFOs45lZdiY2qeXDRQY&signer=MCowBQYDK2VwAyEAebVWLo_mVPlAeLES6KmLp5AfhTrmlb7X4OORC60ElmQ&signature=MEkwBQYDK2VwBEA0QgkL7GFycl72l-Bt076252XjpFG7aLngdxeIs-hNXpCkcJUtq9Xsfm_YV68U0JtK2qLMG2KPSfAVTcg8ntMH";
    const BODY: &[u8] = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"server.echo\",\"params\":{}}";

    #[test]
    fn verifies_js_produced_signature_header() {
        let header = SignatureHeader::from_header(&HeaderValue::from_static(JS_PRODUCED_HEADER))
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

    /// The compact wire form (bare base64 DER, no PEM armor) round-trips and
    /// verifies.
    #[test]
    fn compact_header_round_trip() {
        let key = AnySigningKey::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[7; 32]));
        let header = SignatureHeader::sign(&key, BODY, "start-9.local").expect("signs");
        let value = header.to_header();
        assert!(
            !value.to_str().unwrap().contains("BEGIN"),
            "compact form must not be PEM-armored"
        );
        let parsed = SignatureHeader::from_header(&value).expect("compact header parses");
        verify_request(
            &parsed.signer,
            &parsed.commitment,
            "start-9.local",
            &parsed.signature,
        )
        .expect("compact round trip verifies");
    }
}
