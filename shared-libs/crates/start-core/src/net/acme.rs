use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_acme::acme::{ACME_TLS_ALPN_NAME, AcmeError, Identifier};
use async_acme::rustls_helper::OrderError;
use clap::Parser;
use clap::builder::ValueParserFactory;
use futures::future::{BoxFuture, Shared};
use futures::{FutureExt, StreamExt};
use imbl_value::InternedString;
use itertools::Itertools;
use openssl::pkey::{PKey, Private};
use openssl::x509::X509;
use regex::Regex;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::crypto::CryptoProvider;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::server::ClientHello;
use tokio_rustls::rustls::sign::CertifiedKey;
use ts_rs::TS;
use url::Url;

use crate::context::{CliContext, RpcContext};
use crate::db::model::Database;
use crate::db::model::public::AcmeSettings;
use crate::db::{DbAccess, DbAccessByKey, DbAccessMut};
use crate::error::ErrorData;
use crate::net::ssl::should_use_cert;
use crate::net::tls::{SingleCertResolver, TlsHandler, TlsHandlerAction};
use crate::net::web_server::Accept;
use crate::prelude::*;
use crate::util::FromStrParser;
use crate::util::serde::{Pem, Pkcs8Doc};
use crate::util::sync::{SyncMutex, Watch};

pub type AcmeTlsAlpnCache =
    Arc<SyncMutex<BTreeMap<InternedString, Watch<Option<Arc<CertifiedKey>>>>>>;

/// Per-SAN order state: deduplicates concurrent in-flight ACME orders
/// and arms a cooldown after a failed one. The entry is removed when
/// `get_cert` next finds a valid cached cert in the DB, so the
/// success path doesn't need to clean up here.
#[derive(Default)]
pub struct OrderEntry {
    in_flight: Option<Shared<BoxFuture<'static, Option<CertifiedKey>>>>,
    /// On failure: server-supplied `Retry-After` (when present) or
    /// [`DEFAULT_FAILURE_BACKOFF`]. Inside this window `get_cert`
    /// returns `None` instead of starting a new order.
    backoff_until: Option<Instant>,
}

/// Cooldown for failures without a server-supplied `Retry-After`. Long
/// enough to break the per-connection retry loop.
const DEFAULT_FAILURE_BACKOFF: Duration = Duration::from_secs(60);

/// Cap on server-supplied `Retry-After` so a misbehaving directory
/// can't indefinitely silence cert issuance.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

/// `Some(retry_after)` (capped) for a 429, `None` otherwise.
fn retry_after_from_order_error(err: &OrderError) -> Option<Duration> {
    let OrderError::Acme(AcmeError::RateLimited { retry_after }) = err else {
        return None;
    };
    retry_after.map(|d| d.min(MAX_RETRY_AFTER))
}

pub struct AcmeTlsHandler<M: HasModel, S> {
    pub db: TypedPatchDb<M>,
    pub acme_cache: AcmeTlsAlpnCache,
    pub crypto_provider: Arc<CryptoProvider>,
    pub get_provider: S,
    pub in_progress: Watch<BTreeMap<BTreeSet<InternedString>, OrderEntry>>,
}
impl<M, S> AcmeTlsHandler<M, S>
where
    for<'a> M: DbAccessByKey<AcmeSettings, Key<'a> = &'a AcmeProvider>
        + DbAccessMut<AcmeCertStore>
        + HasModel<Model = Model<M>>
        + Send
        + Sync
        + 'static,
    S: GetAcmeProvider + Clone + Send + Sync,
{
    pub async fn get_cert(&self, san_info: &BTreeSet<InternedString>) -> Option<CertifiedKey> {
        let provider = self.get_provider.get_provider(san_info).await?;
        let provider = provider.as_ref();

        let peek = self.db.peek().await;
        let store = <M as DbAccess<AcmeCertStore>>::access(&peek);
        if let Some(cert) = store
            .as_certs()
            .as_idx(&provider.0)
            .and_then(|p| p.as_idx(JsonKey::new_ref(san_info)))
        {
            let cert = cert.de().log_err()?;
            if cert
                .fullchain
                .get(0)
                .and_then(|c| should_use_cert(&c.0).log_err())
                .unwrap_or(false)
            {
                // Cached cert is healthy; drop any stale order/backoff state.
                self.in_progress
                    .send_if_modified(|map| map.remove(san_info).is_some());
                return Some(
                    CertifiedKey::from_der(
                        cert.fullchain
                            .into_iter()
                            .map(|c| Ok(CertificateDer::from(c.to_der()?)))
                            .collect::<Result<_, Error>>()
                            .log_err()?,
                        PrivateKeyDer::from(PrivatePkcs8KeyDer::from(
                            cert.key.0.private_key_to_pkcs8().log_err()?,
                        )),
                        &*self.crypto_provider,
                    )
                    .log_err()?,
                );
            }
        }

        let contact = <M as DbAccessByKey<AcmeSettings>>::access_by_key(&peek, &provider)?
            .as_contact()
            .de()
            .log_err()?;
        drop(peek);

        let identifiers: Vec<_> = san_info
            .iter()
            .map(|d| match d.parse::<IpAddr>() {
                Ok(a) => Identifier::Ip(a),
                _ => Identifier::Dns((&**d).into()),
            })
            .collect::<Vec<_>>();

        let cache_entries = san_info
            .iter()
            .cloned()
            .map(|d| (d, Watch::new(None)))
            .collect::<BTreeMap<_, _>>();

        // Reuse an in-flight order, honour an active backoff, or start a
        // new order — all under the in_progress lock.
        enum Action {
            Await(Shared<BoxFuture<'static, Option<CertifiedKey>>>),
            Backoff,
        }
        let action = self.in_progress.send_modify(|map| {
            let entry = map.entry(san_info.clone()).or_default();

            // A resolved future's result is either in the DB (cached-cert
            // path would have hit) or in `backoff_until` — don't await it.
            if let Some(fut) = &entry.in_flight {
                if fut.peek().is_some() {
                    entry.in_flight = None;
                }
            }

            if let Some(fut) = &entry.in_flight {
                return Action::Await(fut.clone());
            }

            if let Some(until) = entry.backoff_until {
                if Instant::now() < until {
                    return Action::Backoff;
                }
            }

            let provider_clone = provider.clone();
            let acme_cache = self.acme_cache.clone();
            let db = self.db.clone();
            let in_progress = self.in_progress.clone();
            let san_info_clone = san_info.clone();
            let cache_entries_clone = cache_entries.clone();
            let identifiers_clone = identifiers.clone();
            let contact_clone = contact.clone();

            let fut = async move {
                acme_cache.mutate(|c| {
                    c.extend(
                        cache_entries_clone
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone())),
                    );
                });

                let res = tokio::time::timeout(
                    Duration::from_secs(120),
                    async_acme::rustls_helper::order(
                        |identifier, cert| {
                            let domain = InternedString::from_display(&identifier);
                            if let Some(entry) = cache_entries_clone.get(&domain) {
                                entry.send(Some(Arc::new(cert)));
                            }
                            Ok(())
                        },
                        provider_clone.0.as_str(),
                        &identifiers_clone,
                        Some(&AcmeCertCache(&db)),
                        &contact_clone,
                    ),
                )
                .await;

                acme_cache.mutate(|c| c.retain(|c, _| !cache_entries_clone.contains_key(c)));

                let (cert, backoff) = match res {
                    Ok(Ok(cert)) => (Some(cert), None),
                    Ok(Err(e)) => {
                        let retry_after = retry_after_from_order_error(&e);
                        tracing::warn!("ACME order failed for {san_info_clone:?}: {e}");
                        tracing::debug!("{e:?}");
                        (None, Some(retry_after.unwrap_or(DEFAULT_FAILURE_BACKOFF)))
                    }
                    Err(_) => {
                        tracing::warn!("ACME order timed out for {san_info_clone:?} after 120s");
                        (None, Some(DEFAULT_FAILURE_BACKOFF))
                    }
                };

                // Success: leave the entry alone; the next cached-cert
                // path call removes it. Failure: arm the cooldown.
                if let Some(d) = backoff {
                    in_progress.send_if_modified(|map| {
                        let Some(entry) = map.get_mut(&san_info_clone) else {
                            return false;
                        };
                        entry.in_flight = None;
                        entry.backoff_until = Some(Instant::now() + d);
                        tracing::info!("ACME order for {san_info_clone:?} backing off for {d:?}");
                        true
                    });
                }

                cert
            }
            .boxed()
            .shared();

            entry.in_flight = Some(fut.clone());
            Action::Await(fut)
        });

        match action {
            Action::Await(fut) => fut.await,
            // Inside cooldown: fall through to a self-signed cert from
            // `RootCaTlsHandler` instead of blocking the handshake.
            Action::Backoff => None,
        }
    }
}

pub trait GetAcmeProvider {
    fn get_provider<'a, 'b: 'a>(
        &'b self,
        san_info: &'a BTreeSet<InternedString>,
    ) -> impl Future<Output = Option<impl AsRef<AcmeProvider> + Send + 'b>> + Send + 'a;
}

impl<'a, A, M, S> TlsHandler<'a, A> for Arc<AcmeTlsHandler<M, S>>
where
    A: Accept + 'a,
    <A as Accept>::Metadata: Send + Sync,
    for<'m> M: DbAccessByKey<AcmeSettings, Key<'m> = &'m AcmeProvider>
        + DbAccessMut<AcmeCertStore>
        + HasModel<Model = Model<M>>
        + Send
        + Sync
        + 'static,
    S: GetAcmeProvider + Clone + Send + Sync,
{
    async fn get_config(
        &'a mut self,
        hello: &'a ClientHello<'a>,
        _: &'a <A as Accept>::Metadata,
    ) -> Option<TlsHandlerAction> {
        let domain = hello.server_name()?;
        if hello
            .alpn()
            .into_iter()
            .flatten()
            .any(|a| a == ACME_TLS_ALPN_NAME)
        {
            let cert = self
                .acme_cache
                .peek(|c| c.get(domain).cloned())
                .ok_or_else(|| {
                    Error::new(
                        eyre!("No challenge recv available for {domain}"),
                        ErrorKind::OpenSsl,
                    )
                })
                .log_err()?;
            tracing::info!("Waiting for verification cert for {domain}");
            let cert = cert
                .filter(|c| futures::future::ready(c.is_some()))
                .next()
                .await
                .flatten()?;
            tracing::info!("Verification cert received for {domain}");
            let mut cfg = ServerConfig::builder_with_provider(self.crypto_provider.clone())
                .with_safe_default_protocol_versions()
                .log_err()?
                .with_no_client_auth()
                .with_cert_resolver(Arc::new(SingleCertResolver(cert)));

            cfg.alpn_protocols = vec![ACME_TLS_ALPN_NAME.to_vec()];
            tracing::info!("performing ACME auth challenge");

            return Some(TlsHandlerAction::Tls(cfg));
        }

        let domains: BTreeSet<InternedString> = [domain.into()].into_iter().collect();

        let crypto_provider = self.crypto_provider.clone();
        if let Some(cert) = self.get_cert(&domains).await {
            return Some(TlsHandlerAction::Tls(
                ServerConfig::builder_with_provider(crypto_provider)
                    .with_safe_default_protocol_versions()
                    .log_err()?
                    .with_no_client_auth()
                    .with_cert_resolver(Arc::new(SingleCertResolver(Arc::new(cert)))),
            ));
        }

        None
    }
}

#[derive(Debug, Default, Deserialize, Serialize, HasModel)]
#[model = "Model<Self>"]
pub struct AcmeCertStore {
    pub accounts: BTreeMap<JsonKey<Vec<String>>, Pem<Pkcs8Doc>>,
    pub certs: BTreeMap<Url, BTreeMap<JsonKey<BTreeSet<InternedString>>, AcmeCert>>,
}
impl AcmeCertStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl DbAccess<AcmeCertStore> for Database {
    fn access<'a>(db: &'a Model<Self>) -> &'a Model<AcmeCertStore> {
        db.as_private().as_key_store().as_acme()
    }
}
impl DbAccessMut<AcmeCertStore> for Database {
    fn access_mut<'a>(db: &'a mut Model<Self>) -> &'a mut Model<AcmeCertStore> {
        db.as_private_mut().as_key_store_mut().as_acme_mut()
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AcmeCert {
    pub key: Pem<PKey<Private>>,
    pub fullchain: Vec<Pem<X509>>,
}

pub struct AcmeCertCache<'a, M: HasModel>(pub &'a TypedPatchDb<M>);
#[async_trait::async_trait]
impl<'a, M> async_acme::cache::AcmeCache for AcmeCertCache<'a, M>
where
    M: HasModel<Model = Model<M>> + DbAccessMut<AcmeCertStore> + Send + Sync,
{
    type Error = ErrorData;

    async fn read_account(&self, contacts: &[&str]) -> Result<Option<Vec<u8>>, Self::Error> {
        let contacts = JsonKey::new(contacts.into_iter().map(|s| (*s).to_owned()).collect_vec());
        let peek = self.0.peek().await;
        let Some(account) = M::access(&peek).as_accounts().as_idx(&contacts) else {
            return Ok(None);
        };
        Ok(Some(account.de()?.0.document.into_vec()))
    }

    async fn write_account(&self, contacts: &[&str], contents: &[u8]) -> Result<(), Self::Error> {
        let contacts = JsonKey::new(contacts.into_iter().map(|s| (*s).to_owned()).collect_vec());
        let key = Pkcs8Doc {
            tag: "EC PRIVATE KEY".into(),
            document: pkcs8::Document::try_from(contents).with_kind(ErrorKind::Pem)?,
        };
        self.0
            .mutate(|db| {
                M::access_mut(db)
                    .as_accounts_mut()
                    .insert(&contacts, &Pem::new(key))
            })
            .await
            .result?;
        Ok(())
    }

    async fn read_certificate(
        &self,
        identifiers: &[Identifier],
        directory_url: &str,
    ) -> Result<Option<(String, String)>, Self::Error> {
        let identifiers = JsonKey::new(
            identifiers
                .into_iter()
                .map(|d| match d {
                    Identifier::Dns(d) => d.into(),
                    Identifier::Ip(ip) => InternedString::from_display(ip),
                })
                .collect(),
        );
        let directory_url = directory_url
            .parse::<Url>()
            .with_kind(ErrorKind::ParseUrl)?;
        let peek = self.0.peek().await;
        let Some(cert) = M::access(&peek)
            .as_certs()
            .as_idx(&directory_url)
            .and_then(|a| a.as_idx(&identifiers))
        else {
            return Ok(None);
        };
        let cert = cert.de()?;
        if !cert
            .fullchain
            .get(0)
            .map(|c| should_use_cert(&c.0))
            .transpose()
            .map_err(Error::from)?
            .unwrap_or(false)
        {
            return Ok(None);
        }
        Ok(Some((
            String::from_utf8(
                cert.key
                    .0
                    .private_key_to_pem_pkcs8()
                    .with_kind(ErrorKind::OpenSsl)?,
            )
            .with_kind(ErrorKind::Utf8)?,
            cert.fullchain
                .into_iter()
                .map(|cert| {
                    String::from_utf8(cert.0.to_pem().with_kind(ErrorKind::OpenSsl)?)
                        .with_kind(ErrorKind::Utf8)
                })
                .collect::<Result<Vec<_>, _>>()?
                .join("\n"),
        )))
    }

    async fn write_certificate(
        &self,
        identifiers: &[Identifier],
        directory_url: &str,
        key_pem: &str,
        certificate_pem: &str,
    ) -> Result<(), Self::Error> {
        tracing::info!("Saving new certificate for {identifiers:?}");
        let identifiers = JsonKey::new(
            identifiers
                .into_iter()
                .map(|d| match d {
                    Identifier::Dns(d) => d.into(),
                    Identifier::Ip(ip) => InternedString::from_display(ip),
                })
                .collect(),
        );
        let directory_url = directory_url
            .parse::<Url>()
            .with_kind(ErrorKind::ParseUrl)?;
        let cert = AcmeCert {
            key: Pem(PKey::<Private>::private_key_from_pem(key_pem.as_bytes())
                .with_kind(ErrorKind::OpenSsl)?),
            fullchain: X509::stack_from_pem(certificate_pem.as_bytes())
                .with_kind(ErrorKind::OpenSsl)?
                .into_iter()
                .map(Pem)
                .collect(),
        };
        self.0
            .mutate(|db| {
                M::access_mut(db)
                    .as_certs_mut()
                    .upsert(&directory_url, || Ok(BTreeMap::new()))?
                    .insert(&identifiers, &cert)
            })
            .await
            .result?;

        Ok(())
    }
}

pub fn acme_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "init",
            from_fn_async(init)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.setup-acme-certificate-acquisition")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.remove-acme-certificate-acquisition-configuration")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
#[ts(type = "string")]
pub struct AcmeProvider(pub Url);
impl FromStr for AcmeProvider {
    type Err = <Url as FromStr>::Err;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "letsencrypt" => async_acme::acme::LETS_ENCRYPT_PRODUCTION_DIRECTORY.parse(),
            "letsencrypt-staging" => async_acme::acme::LETS_ENCRYPT_STAGING_DIRECTORY.parse(),
            s => s.parse(),
        }
        .map(|mut u: Url| {
            let path = u
                .path_segments()
                .into_iter()
                .flatten()
                .filter(|p| !p.is_empty())
                .map(|p| p.to_owned())
                .collect::<Vec<_>>();
            if let Ok(mut path_mut) = u.path_segments_mut() {
                path_mut.clear();
                path_mut.extend(path);
            }
            u
        })
        .map(Self)
    }
}
impl<'de> Deserialize<'de> for AcmeProvider {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        crate::util::serde::deserialize_from_str(deserializer)
    }
}
impl AsRef<str> for AcmeProvider {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}
impl AsRef<AcmeProvider> for AcmeProvider {
    fn as_ref(&self) -> &AcmeProvider {
        self
    }
}
impl ValueParserFactory for AcmeProvider {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        Self::Parser::new()
    }
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
pub struct InitAcmeParams {
    #[arg(long, help = "help.arg.acme-provider")]
    pub provider: AcmeProvider,
    #[arg(long, help = "help.arg.acme-contact")]
    pub contact: Vec<String>,
}

lazy_static::lazy_static! {
    // Pragmatic email match: local@domain, no whitespace, dot in domain.
    // RFC 5322 is far too permissive for what an ACME server actually accepts.
    static ref EMAIL_REGEX: Regex =
        Regex::new(r"^[^@\s]+@[^@\s]+\.[^@\s]+$").unwrap();
}

fn invalid_contact_err(contact: &str) -> Error {
    Error::new(
        eyre!("{}", t!("acme.invalid-contact", contact = contact)),
        ErrorKind::InvalidRequest,
    )
}

// RFC 8555 §7.3 requires `contact` entries to be URLs, and Let's Encrypt
// (the only provider start-os ships shorthand for) only supports `mailto:`.
// Without this gate, a bare email here gets stored verbatim and eventually
// fails the `newAccount` request with `unsupportedContact`.
fn validate_contact(contact: &str) -> Result<(), Error> {
    let url = contact
        .parse::<Url>()
        .map_err(|_| invalid_contact_err(contact))?;
    if url.scheme() != "mailto" {
        return Err(invalid_contact_err(contact));
    }
    if !EMAIL_REGEX.is_match(url.path()) {
        return Err(invalid_contact_err(contact));
    }
    Ok(())
}

// Accept either a full `mailto:` URI or a bare email — for the latter,
// prepend `mailto:` and re-validate so the stored value is always RFC 8555
// compliant.
fn normalize_contact(contact: String) -> Result<String, Error> {
    if validate_contact(&contact).is_ok() {
        return Ok(contact);
    }
    let prefixed = format!("mailto:{contact}");
    if validate_contact(&prefixed).is_ok() {
        return Ok(prefixed);
    }
    Err(invalid_contact_err(&contact))
}

pub async fn init(
    ctx: RpcContext,
    InitAcmeParams { provider, contact }: InitAcmeParams,
) -> Result<(), Error> {
    let contact = contact
        .into_iter()
        .map(normalize_contact)
        .collect::<Result<Vec<_>, _>>()?;
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_network_mut()
                .as_acme_mut()
                .insert(&provider, &AcmeSettings { contact })
        })
        .await
        .result?;
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
pub struct RemoveAcmeParams {
    #[arg(long, help = "help.arg.acme-provider")]
    pub provider: AcmeProvider,
}

pub async fn remove(
    ctx: RpcContext,
    RemoveAcmeParams { provider }: RemoveAcmeParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_network_mut()
                .as_acme_mut()
                .remove(&provider)
        })
        .await
        .result?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use async_acme::acme::{AcmeError, Identifier};
    use async_acme::rustls_helper::OrderError;

    use super::{
        MAX_RETRY_AFTER, normalize_contact, retry_after_from_order_error, validate_contact,
    };

    #[test]
    fn rate_limited_with_retry_after_passes_through() {
        let err = OrderError::Acme(AcmeError::RateLimited {
            retry_after: Some(Duration::from_secs(120)),
        });
        assert_eq!(
            retry_after_from_order_error(&err),
            Some(Duration::from_secs(120)),
        );
    }

    #[test]
    fn rate_limited_retry_after_capped_at_max() {
        // Pathological server: "please come back in a year".
        let err = OrderError::Acme(AcmeError::RateLimited {
            retry_after: Some(Duration::from_secs(365 * 24 * 60 * 60)),
        });
        assert_eq!(retry_after_from_order_error(&err), Some(MAX_RETRY_AFTER));
    }

    #[test]
    fn rate_limited_without_retry_after_returns_none() {
        let err = OrderError::Acme(AcmeError::RateLimited { retry_after: None });
        assert_eq!(retry_after_from_order_error(&err), None);
    }

    #[test]
    fn non_429_http_status_returns_none() {
        let err = OrderError::Acme(AcmeError::HttpStatus(503));
        assert_eq!(retry_after_from_order_error(&err), None);
    }

    #[test]
    fn non_http_errors_return_none() {
        let err = OrderError::TooManyAttemptsAuth(Identifier::Dns("example.test".into()));
        assert_eq!(retry_after_from_order_error(&err), None);
    }

    #[test]
    fn validate_contact_accepts_mailto_uri() {
        assert!(validate_contact("mailto:admin@example.com").is_ok());
        assert!(validate_contact("mailto:user.name+tag@sub.example.co.uk").is_ok());
    }

    #[test]
    fn validate_contact_rejects_bare_email() {
        // The validator itself is strict; the fallback in `normalize_contact`
        // is what auto-prefixes `mailto:`.
        assert!(validate_contact("admin@example.com").is_err());
    }

    #[test]
    fn validate_contact_rejects_non_mailto_scheme() {
        assert!(validate_contact("https://example.com/contact").is_err());
        assert!(validate_contact("tel:+15551234567").is_err());
    }

    #[test]
    fn validate_contact_rejects_mailto_without_valid_email() {
        assert!(validate_contact("mailto:").is_err());
        assert!(validate_contact("mailto:not-an-email").is_err());
        assert!(validate_contact("mailto:no-tld@localhost").is_err());
        assert!(validate_contact("mailto:has space@example.com").is_err());
        assert!(validate_contact("mailto:two@@example.com").is_err());
    }

    #[test]
    fn validate_contact_rejects_empty() {
        assert!(validate_contact("").is_err());
    }

    #[test]
    fn normalize_contact_passes_through_valid_mailto() {
        assert_eq!(
            normalize_contact("mailto:admin@example.com".into()).unwrap(),
            "mailto:admin@example.com",
        );
    }

    #[test]
    fn normalize_contact_prepends_mailto_for_bare_email() {
        assert_eq!(
            normalize_contact("admin@example.com".into()).unwrap(),
            "mailto:admin@example.com",
        );
    }

    #[test]
    fn normalize_contact_rejects_garbage_even_with_prefix() {
        assert!(normalize_contact("not an email".into()).is_err());
        assert!(normalize_contact("https://example.com".into()).is_err());
        assert!(normalize_contact("".into()).is_err());
    }
}
