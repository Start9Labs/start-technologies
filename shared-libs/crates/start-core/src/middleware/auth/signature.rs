use std::collections::BTreeMap;
use std::future::Future;
use std::net::IpAddr;
use std::sync::LazyLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::Request;
use chrono::Utc;
use http::{HeaderMap, HeaderValue};
use reqwest::Client;
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{Middleware, RpcRequest, RpcResponse};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio::sync::Mutex;
use url::Url;

use crate::context::{CliContext, RpcContext};
use crate::middleware::auth::DbContext;
use crate::prelude::*;
use crate::sign::commitment::Commitment;
use crate::sign::commitment::request::RequestCommitment;
use crate::sign::{AnySignature, AnySigningKey, AnyVerifyingKey, SignatureScheme};
use crate::util::iter::TransposeResultIterExt;
use crate::util::serde::Base64;

pub const AUTH_SIG_HEADER: &str = "X-StartOS-Auth-Sig";

/// RPC-metadata fields understood by [`SignatureAuth`] when layered over an
/// [`RpcContext`]. `login` marks the enrollment endpoint: the request must
/// still be signed (proving possession of the key being enrolled), but the
/// key need not be registered yet.
#[derive(Deserialize)]
pub struct LoginMetadata {
    #[serde(default)]
    pub login: bool,
}

pub trait SignatureAuthContext: DbContext {
    type AdditionalMetadata: DeserializeOwned + Send;
    type CheckPubkeyRes: Send;
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
}

impl SignatureAuthContext for RpcContext {
    type AdditionalMetadata = LoginMetadata;
    type CheckPubkeyRes = Option<AnyVerifyingKey>;
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
                        info.lan_ip
                            .iter()
                            .copied()
                            .chain(info.wan_ip.map(IpAddr::V4))
                            .map(|ip| InternedString::intern(url_host_str(ip)))
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
                .collect::<Vec<_>>()
        })
    }
    fn check_pubkey(
        &self,
        db: &Model<Self::Database>,
        pubkey: Option<&AnyVerifyingKey>,
        metadata: Self::AdditionalMetadata,
    ) -> Result<Self::CheckPubkeyRes, Error> {
        let Some(pubkey) = pubkey else {
            return Err(Error::new(
                eyre!("{}", t!("middleware.auth.unauthorized")),
                ErrorKind::Authorization,
            ));
        };
        if metadata.login {
            return Ok(None);
        }
        let key = InternedString::intern(pubkey.to_string());
        if self
            .ephemeral_auth_keys
            .peek(|keys| keys.0.contains_key(&*key))
        {
            return Ok(Some(pubkey.clone()));
        }
        if db
            .as_private()
            .as_session_pubkeys()
            .de()?
            .0
            .contains_key(&*key)
        {
            return Ok(Some(pubkey.clone()));
        }

        Err(Error::new(
            eyre!("{}", t!("middleware.auth.key-not-authorized")),
            ErrorKind::Authorization,
        ))
    }
    async fn post_auth_hook(&self, key: Self::CheckPubkeyRes, _: &RpcRequest) -> Result<(), Error> {
        if let Some(key) = key {
            let key = InternedString::intern(key.to_string());
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
fn url_host_str(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    }
}

pub trait SigningContext {
    fn signing_key(&self) -> Result<AnySigningKey, Error>;
}

impl SigningContext for CliContext {
    fn signing_key(&self) -> Result<AnySigningKey, Error> {
        Ok(AnySigningKey::Ed25519(self.developer_key()?.clone()))
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
}

#[derive(Clone)]
pub struct SignatureAuth {
    signer: Option<Result<AnyVerifyingKey, RpcError>>,
}
impl SignatureAuth {
    pub fn new() -> Self {
        Self { signer: None }
    }
}

static NONCE_CACHE: LazyLock<Mutex<BTreeMap<Instant, u64>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

async fn handle_nonce(nonce: u64) -> Result<(), Error> {
    let mut cache = NONCE_CACHE.lock().await;
    if cache.values().any(|n| *n == nonce) {
        return Err(Error::new(
            eyre!("{}", t!("middleware.auth.replay-attack-detected")),
            ErrorKind::Authorization,
        ));
    }
    while let Some(entry) = cache.first_entry() {
        if entry.key().elapsed() > Duration::from_secs(60) {
            entry.remove_entry();
        } else {
            break;
        }
    }
    cache.insert(Instant::now(), nonce);
    Ok(())
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

    let mut verified = false;
    for sig_context in context.sig_context().await {
        let sig_context = sig_context?;
        if verify_request(&signer, &commitment, sig_context.as_ref(), &signature).is_ok()
            || verify_request_legacy(&signer, &commitment, sig_context.as_ref(), &signature).is_ok()
        {
            verified = true;
            break;
        }
    }
    if !verified {
        return Err(Error::new(
            eyre!("{}", t!("middleware.auth.no-valid-sig-context")),
            ErrorKind::Authorization,
        ));
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_else(|e| e.duration().as_secs() as i64 * -1);
    if (now - commitment.timestamp).abs() > 30 {
        return Err(Error::new(
            eyre!("{}", t!("middleware.auth.timestamp-not-within-30s")),
            ErrorKind::InvalidSignature,
        ));
    }
    handle_nonce(commitment.nonce).await?;

    let mut body = Vec::with_capacity(commitment.size as usize);
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
        url.query_pairs_mut()
            .append_pair("signer", &self.signer.to_string());
        url.query_pairs_mut()
            .append_pair("signature", &self.signature.to_string());
        HeaderValue::from_str(url.query().unwrap_or_default()).unwrap()
    }
    pub fn from_header(header: &HeaderValue) -> Result<Self, Error> {
        let query: BTreeMap<_, _> = form_urlencoded::parse(header.as_bytes()).collect();
        Ok(Self {
            commitment: RequestCommitment::from_query(&header)?,
            signer: query.get("signer").or_not_found("signer")?.parse()?,
            signature: query.get("signature").or_not_found("signature")?.parse()?,
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
const REQUEST_AUTH_TAG: &[u8] = b"StartOS RPC Auth v1\0";

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

/// Pre-0.4.0-beta.11 request signatures: Ed25519ph with the server identity
/// carried in the dom2 context parameter. Still accepted so deployed CLI and
/// tunnel-device clients keep working; new signatures use [`verify_request`].
fn verify_request_legacy(
    key: &AnyVerifyingKey,
    commitment: &RequestCommitment,
    context: &str,
    signature: &AnySignature,
) -> Result<(), Error> {
    key.scheme()
        .verify_commitment(key, commitment, context, signature)
}

impl<C: SignatureAuthContext> Middleware<C> for SignatureAuth {
    type Metadata = Metadata<C::AdditionalMetadata>;
    async fn process_http_request(
        &mut self,
        context: &C,
        request: &mut Request,
    ) -> Result<(), axum::response::Response> {
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

    /// Generated by the TypeScript client (`lib/auth/signature.ts`, pure
    /// Ed25519 via @noble/curves) for secret key 0102…1f20, context
    /// "start-9.local", and the JSON body below. Guards the byte-level
    /// contract between the browser signer and this verifier.
    const JS_PRODUCED_HEADER: &str = "timestamp=1784678775&nonce=15138308865896296388&size=59&blake3=95o3MZRDgMasjyEKb6h2qMb1JFOs45lZdiY2qeXDRQY&signer=-----BEGIN+PUBLIC+KEY-----%0AMCowBQYDK2VwAyEAebVWLo%2FmVPlAeLES6KmLp5AfhTrmlb7X4OORC60ElmQ%3D%0A-----END+PUBLIC+KEY-----%0A&signature=-----BEGIN+SIGNATURE-----%0AMEkwBQYDK2VwBEACcEsaI6InKoVf%2BBB27cXMtYw1DxZIgnGaYmfIM%2BWucOEcCfxl%0Asld0e7pTCSqxhKMmTSdP9QmNOSxwjSgUyHgK%0A-----END+SIGNATURE-----%0A";
    /// Same key/body/context, signed with the legacy Ed25519ph scheme.
    const JS_LEGACY_HEADER: &str = "timestamp=1784677724&nonce=934644805336935159&size=59&blake3=95o3MZRDgMasjyEKb6h2qMb1JFOs45lZdiY2qeXDRQY&signer=-----BEGIN+PUBLIC+KEY-----%0AMCowBQYDK2VwAyEAebVWLo%2FmVPlAeLES6KmLp5AfhTrmlb7X4OORC60ElmQ%3D%0A-----END+PUBLIC+KEY-----%0A&signature=-----BEGIN+SIGNATURE-----%0AMEkwBQYDK2VwBEAbL92MluAwidhMHo1HE49s3U7tcVwo%2FdaHPrmP2WryD4pvXNAR%0AJ4y94b%2BSDYXgBa5PVoOqcXRisLORxl0lFpMB%0A-----END+SIGNATURE-----%0A";
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

    #[test]
    fn legacy_scheme_still_accepted() {
        let header = SignatureHeader::from_header(&HeaderValue::from_static(JS_LEGACY_HEADER))
            .expect("header parses");
        verify_request_legacy(
            &header.signer,
            &header.commitment,
            "start-9.local",
            &header.signature,
        )
        .expect("legacy signature verifies");
        verify_request(
            &header.signer,
            &header.commitment,
            "start-9.local",
            &header.signature,
        )
        .expect_err("legacy signature is not valid under the new scheme");
    }
}
