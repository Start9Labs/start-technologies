use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use clap::Parser;
use clap::builder::ValueParserFactory;
use futures::future::{BoxFuture, Shared};
use futures::{FutureExt, StreamExt};
use imbl_value::InternedString;
use instant_acme::{Error as AcmeError, LetsEncrypt};
use itertools::Itertools;
use regex::Regex;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::crypto::CryptoProvider;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::server::ClientHello;
use tokio_rustls::rustls::sign::CertifiedKey;
use url::Url;

use crate::GatewayId;
use crate::context::{CliContext, RpcContext};
use crate::db::model::public::AcmeSettings;
use crate::db::model::{Database, DatabaseModel};
use crate::db::{DbAccess, DbAccessByKey, DbAccessMut};
use crate::error::ErrorData;
use crate::net::gateway::{
    CheckPortParams, CheckPortRes, CheckPortV6Res, check_port, check_port_v6,
};
use crate::net::ssl::{cert_is_unexpired, should_use_cert};
use crate::net::tls::{SingleCertResolver, TlsHandler, TlsHandlerAction};
use crate::net::vhost::ACME_CHALLENGE_PORT;
use crate::net::web_server::Accept;
use crate::prelude::*;
use crate::util::FromStrParser;
use crate::util::serde::{Der64, HandlerExtSerde};
use crate::util::sync::{SyncMutex, Watch};

mod order;

pub const ACME_TLS_ALPN_NAME: &[u8] = b"acme-tls/1";

/// Names with an order in flight. Written only by the order path, which starts
/// an order only for a name a vhost holds a provider for — so membership here
/// is what makes a challenge answerable, and it must stay that way.
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
    /// Set once reported, so retries stay silent; dropped with the entry.
    reported: bool,
}

/// Told the SANs of a failed order and why, to surface to the operator.
pub type ReportOrderFailure =
    Arc<dyn Fn(BTreeSet<InternedString>, String) -> BoxFuture<'static, ()> + Send + Sync>;

/// Cooldown for failures without a server-supplied `Retry-After`. Long
/// enough to break the per-connection retry loop.
const DEFAULT_FAILURE_BACKOFF: Duration = Duration::from_secs(60);

/// Cap on server-supplied `Retry-After` so a misbehaving directory
/// can't indefinitely silence cert issuance.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

fn retry_after_from_order_error(err: &AcmeError) -> Option<Duration> {
    match err {
        AcmeError::RateLimited { retry_after, .. } => retry_after.map(|deadline| {
            deadline
                .duration_since(SystemTime::now())
                .unwrap_or_default()
                .min(MAX_RETRY_AFTER)
        }),
        AcmeError::Timeout(Some(deadline)) => Some(
            deadline
                .saturating_duration_since(Instant::now())
                .min(MAX_RETRY_AFTER),
        ),
        _ => None,
    }
}

pub struct AcmeTlsHandler<M: HasModel, S> {
    pub db: TypedPatchDb<M>,
    pub acme_cache: AcmeTlsAlpnCache,
    pub crypto_provider: Arc<CryptoProvider>,
    pub get_provider: S,
    pub in_progress: Watch<BTreeMap<BTreeSet<InternedString>, OrderEntry>>,
    pub report_failure: Option<ReportOrderFailure>,
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
        let cached = store
            .as_certs()
            .as_idx(&provider.0)
            .and_then(|p| p.as_idx(JsonKey::new_ref(san_info)))
            .and_then(|cert| cert.de().log_err());
        if let Some(cert) = &cached {
            if usable(cert) {
                self.in_progress
                    .send_if_modified(|map| map.remove(san_info).is_some());
                return certified_key(cert, &self.crypto_provider).log_err();
            }
        }

        let renewing = || {
            certified_key(
                cached.as_ref().filter(|c| unexpired(c))?,
                &self.crypto_provider,
            )
            .log_err()
        };

        let Some(contact) = <M as DbAccessByKey<AcmeSettings>>::access_by_key(&peek, &provider)
            .and_then(|settings| settings.as_contact().de().log_err())
        else {
            return renewing();
        };
        drop(peek);

        let cache_entries = san_info
            .iter()
            .cloned()
            .map(|d| (d, Watch::new(None)))
            .collect::<BTreeMap<_, _>>();

        enum Action {
            Await(Shared<BoxFuture<'static, Option<CertifiedKey>>>),
            Backoff,
        }
        let action = self.in_progress.send_modify(|map| {
            let entry = map.entry(san_info.clone()).or_default();

            // Resolved failures live in `backoff_until`, not `in_flight`.
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
            let report_failure = self.report_failure.clone();
            let acme_cache = self.acme_cache.clone();
            let db = self.db.clone();
            let in_progress = self.in_progress.clone();
            let san_info_clone = san_info.clone();
            let cache_entries_clone = cache_entries.clone();
            let contact_clone = contact.clone();
            let crypto_provider = self.crypto_provider.clone();

            let fut = async move {
                acme_cache.mutate(|c| {
                    c.extend(
                        cache_entries_clone
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone())),
                    );
                });

                let res = tokio::time::timeout(Duration::from_secs(120), async {
                    order::order(
                        |identifier, cert| {
                            let domain =
                                InternedString::from_display(&identifier.authorized(false));
                            if let Some(entry) = cache_entries_clone.get(&domain) {
                                entry.send(Some(Arc::new(cert)));
                            }
                            Ok(())
                        },
                        provider_clone.0.as_str(),
                        &san_info_clone,
                        &AcmeCertCache(&db),
                        &contact_clone,
                        &crypto_provider,
                        Box::new(order::AcmeHttpClient::new()?),
                    )
                    .await
                })
                .await;

                acme_cache.mutate(|c| c.retain(|c, _| !cache_entries_clone.contains_key(c)));

                let (cert, failure) = match res {
                    Ok(Ok(cert)) => (Some(cert), None),
                    Ok(Err(e)) => {
                        let retry_after = retry_after_from_order_error(&e);
                        tracing::warn!("ACME order failed for {san_info_clone:?}: {e}");
                        tracing::debug!("{e:?}");
                        (
                            None,
                            Some((
                                retry_after.unwrap_or(DEFAULT_FAILURE_BACKOFF),
                                e.to_string(),
                            )),
                        )
                    }
                    Err(_) => {
                        tracing::warn!("ACME order timed out for {san_info_clone:?} after 120s");
                        (
                            None,
                            Some((
                                DEFAULT_FAILURE_BACKOFF,
                                t!("acme.order-timed-out").to_string(),
                            )),
                        )
                    }
                };

                if let Some((d, error)) = failure {
                    let report = in_progress.send_modify(|map| {
                        let Some(entry) = map.get_mut(&san_info_clone) else {
                            return false;
                        };
                        entry.in_flight = None;
                        entry.backoff_until = Some(Instant::now() + d);
                        tracing::info!("ACME order for {san_info_clone:?} backing off for {d:?}");
                        std::mem::replace(&mut entry.reported, true) == false
                    });
                    if report {
                        if let Some(report) = &report_failure {
                            report(san_info_clone.clone(), error).await;
                        }
                    }
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
            Action::Backoff => None,
        }
        .or_else(renewing)
    }
}

fn usable(cert: &AcmeCert) -> bool {
    cert.fullchain
        .first()
        .and_then(|certificate| should_use_cert(&certificate.0).log_err())
        .unwrap_or(false)
}

fn unexpired(cert: &AcmeCert) -> bool {
    cert.fullchain
        .get(0)
        .and_then(|c| cert_is_unexpired(&c.0).log_err())
        .unwrap_or(false)
}

fn certified_key(
    cert: &AcmeCert,
    crypto_provider: &CryptoProvider,
) -> Result<CertifiedKey, ErrorData> {
    Ok(CertifiedKey::from_der(
        cert.fullchain
            .iter()
            .map(|c| Ok(CertificateDer::from(c.0.to_der()?)))
            .collect::<Result<_, Error>>()?,
        PrivateKeyDer::from(PrivatePkcs8KeyDer::from(
            cert.key
                .0
                .private_key_to_pkcs8()
                .with_kind(ErrorKind::OpenSsl)?,
        )),
        crypto_provider,
    )
    .with_kind(ErrorKind::TLSInit)?)
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
            let Some(cert) = self.acme_cache.peek(|c| c.get(domain).cloned()) else {
                tracing::debug!("no ACME challenge in flight for {domain}");
                return None;
            };
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
    pub accounts: BTreeMap<JsonKey<Vec<String>>, Der64<pkcs8::Document>>,
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

/// Standalone ACME authority for caller-authorized SAN sets.
#[derive(Default, Deserialize, Serialize, HasModel)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[model = "Model<Self>"]
pub struct BootstrapAcme {
    pub providers: BTreeMap<JsonKey<BTreeSet<InternedString>>, AcmeProvider>,
    pub settings: BTreeMap<AcmeProvider, AcmeSettings>,
    pub cache: AcmeCertStore,
}

impl BootstrapAcme {
    /// The caller supplies only enabled SAN/provider bindings owned by the bootstrap host.
    pub fn capture(
        db: &DatabaseModel,
        providers: BTreeMap<JsonKey<BTreeSet<InternedString>>, AcmeProvider>,
    ) -> Result<Self, Error> {
        let mut snapshot = Self {
            providers,
            ..Self::default()
        };
        let store = <Database as DbAccess<AcmeCertStore>>::access(db);
        for (sans, provider) in &snapshot.providers {
            if let Some(settings) =
                <Database as DbAccessByKey<AcmeSettings>>::access_by_key(db, provider)
            {
                let settings = settings.de()?;
                let contacts = JsonKey::new(settings.contact.clone());
                if let Some(account) = store.as_accounts().as_idx(&contacts) {
                    snapshot.cache.accounts.insert(contacts, account.de()?);
                }
                snapshot.settings.insert(provider.clone(), settings);
            }
            if let Some(cert) = store
                .as_certs()
                .as_idx(&provider.0)
                .and_then(|certs| certs.as_idx(sans))
            {
                snapshot
                    .cache
                    .certs
                    .entry(provider.0.clone())
                    .or_default()
                    .insert(sans.clone(), cert.de()?);
            }
        }
        Ok(snapshot)
    }

    /// Retains authoritative settings, accounts and healthy cached identities.
    pub(crate) fn merge_cache_into(&self, db: &mut DatabaseModel) -> Result<(), Error> {
        for (sans, provider) in &self.providers {
            let Some(expected) = self.settings.get(provider) else {
                continue;
            };
            let Some(current) =
                <Database as DbAccessByKey<AcmeSettings>>::access_by_key(db, provider)
            else {
                continue;
            };
            if serde_json::to_value(current.de()?).with_kind(ErrorKind::Serialization)?
                != serde_json::to_value(expected).with_kind(ErrorKind::Serialization)?
            {
                continue;
            }
            let store = <Database as DbAccessMut<AcmeCertStore>>::access_mut(db);
            let contacts = JsonKey::new(expected.contact.clone());
            if store.as_accounts().as_idx(&contacts).is_none() {
                if let Some(account) = self.cache.accounts.get(&contacts) {
                    store.as_accounts_mut().insert(&contacts, account)?;
                }
            }
            let current = store
                .as_certs()
                .as_idx(&provider.0)
                .and_then(|certs| certs.as_idx(sans))
                .map(|cert| cert.de())
                .transpose()?;
            if current.as_ref().is_some_and(usable) {
                continue;
            }
            if let Some(cert) = self
                .cache
                .certs
                .get(&provider.0)
                .and_then(|certs| certs.get(sans))
                .filter(|cert| usable(cert))
            {
                store
                    .as_certs_mut()
                    .upsert(&provider.0, || Ok(BTreeMap::new()))?
                    .insert(sans, cert)?;
            }
        }
        Ok(())
    }

    /// Materializes a mutable RAM cache; callers persist renewed records at handoff.
    pub async fn materialize(self) -> Result<TypedPatchDb<Self>, Error> {
        let db = TypedPatchDb::load_unchecked(patch_db::PatchDb::in_memory());
        db.mutate(move |db: &mut Model<Self>| db.ser(&self))
            .await
            .result?;
        Ok(db)
    }
}

impl DbAccess<AcmeCertStore> for BootstrapAcme {
    fn access(db: &Model<Self>) -> &Model<AcmeCertStore> {
        db.as_cache()
    }
}
impl DbAccessMut<AcmeCertStore> for BootstrapAcme {
    fn access_mut(db: &mut Model<Self>) -> &mut Model<AcmeCertStore> {
        db.as_cache_mut()
    }
}
impl DbAccessByKey<AcmeSettings> for BootstrapAcme {
    type Key<'a> = &'a AcmeProvider;
    fn access_by_key<'a>(
        db: &'a Model<Self>,
        key: &AcmeProvider,
    ) -> Option<&'a Model<AcmeSettings>> {
        db.as_settings().as_idx(key)
    }
}

#[derive(Clone)]
pub struct BootstrapAcmeProvider(pub TypedPatchDb<BootstrapAcme>);

impl GetAcmeProvider for BootstrapAcmeProvider {
    async fn get_provider<'a, 'b: 'a>(
        &'b self,
        san_info: &'a BTreeSet<InternedString>,
    ) -> Option<impl AsRef<AcmeProvider> + Send + 'b> {
        self.0
            .peek()
            .await
            .as_providers()
            .as_idx(JsonKey::new_ref(san_info))?
            .de()
            .log_err()
    }
}

pub type AcmeCert = crate::net::ssl::CertificateIdentity;

pub struct AcmeCertCache<'a, M: HasModel>(pub &'a TypedPatchDb<M>);
impl<'a, M> AcmeCertCache<'a, M>
where
    M: HasModel<Model = Model<M>> + DbAccessMut<AcmeCertStore> + Send + Sync,
{
    async fn read_account(&self, contacts: &[&str]) -> Result<Option<Vec<u8>>, ErrorData> {
        let contacts = JsonKey::new(contacts.into_iter().map(|s| (*s).to_owned()).collect_vec());
        let peek = self.0.peek().await;
        let Some(account) = M::access(&peek).as_accounts().as_idx(&contacts) else {
            return Ok(None);
        };
        Ok(Some(account.de()?.0.into_vec()))
    }

    async fn write_account(&self, contacts: &[&str], contents: &[u8]) -> Result<(), ErrorData> {
        let contacts = JsonKey::new(contacts.into_iter().map(|s| (*s).to_owned()).collect_vec());
        let key = pkcs8::Document::try_from(contents).with_kind(ErrorKind::Deserialization)?;
        self.0
            .mutate(|db| {
                M::access_mut(db)
                    .as_accounts_mut()
                    .insert(&contacts, &Der64::new(key))
            })
            .await
            .result?;
        Ok(())
    }

    async fn write_certificate(
        &self,
        sans: &BTreeSet<InternedString>,
        directory_url: &str,
        cert: &AcmeCert,
    ) -> Result<(), ErrorData> {
        tracing::info!("Saving new certificate for {sans:?}");
        let identifiers = JsonKey::new_ref(sans);
        let directory_url = directory_url
            .parse::<Url>()
            .with_kind(ErrorKind::ParseUrl)?;
        self.0
            .mutate(|db| {
                M::access_mut(db)
                    .as_certs_mut()
                    .upsert(&directory_url, || Ok(BTreeMap::new()))?
                    .insert(identifiers, cert)
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
        .subcommand(
            "check-challenge",
            from_fn_async(check_challenge)
                .with_display_serializable()
                .with_about("about.check-acme-challenge-reachability")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct CheckChallengeParams {
    #[arg(help = "help.arg.fqdn")]
    pub fqdn: InternedString,
    #[arg(help = "help.arg.gateway-id")]
    pub gateway: GatewayId,
    #[arg(long, help = "help.arg.acme-provider")]
    pub acme: AcmeProvider,
}

rpc_toolkit::reflect_ts!(CheckChallengeParams);
rpc_toolkit::ts_export!(CheckChallengeParams, namespaces = [""]);

/// Reachability of the port a certificate authority validates on. A null leg
/// went unprobed.
#[derive(Debug, Clone, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct CheckChallengeRes {
    pub port: Option<CheckPortRes>,
    pub port_v6: Option<CheckPortV6Res>,
}

rpc_toolkit::reflect_ts!(CheckChallengeRes);
rpc_toolkit::ts_export!(CheckChallengeRes, namespaces = [""]);

/// Probes the challenge port. `None` when the authority's certificate has life left.
pub async fn check_challenge(
    ctx: RpcContext,
    CheckChallengeParams {
        fqdn,
        gateway,
        acme,
    }: CheckChallengeParams,
) -> Result<Option<CheckChallengeRes>, Error> {
    if !challenge_pending(&ctx.db.peek().await, &fqdn, &acme)? {
        return Ok(None);
    }

    let params = CheckPortParams {
        port: ACME_CHALLENGE_PORT,
        gateway,
    };
    let (port_res, port_v6_res) = tokio::join!(
        check_port(ctx.clone(), params.clone()),
        check_port_v6(ctx.clone(), params),
    );

    Ok(Some(CheckChallengeRes {
        port: port_res.log_err(),
        port_v6: port_v6_res.log_err().flatten(),
    }))
}

/// True from the start of the renewal window, not from expiry.
fn challenge_pending(
    db: &DatabaseModel,
    fqdn: &InternedString,
    acme: &AcmeProvider,
) -> Result<bool, Error> {
    let sans: BTreeSet<InternedString> = [fqdn.clone()].into_iter().collect();
    let Some(cert) = db
        .as_private()
        .as_key_store()
        .as_acme()
        .as_certs()
        .as_idx(&acme.0)
        .and_then(|p| p.as_idx(JsonKey::new_ref(&sans)))
    else {
        return Ok(true);
    };
    Ok(!usable(&cert.de()?))
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, VisitFields)]
#[visit(ts(type = "z.string()"))]
pub struct AcmeProvider(pub Url);

rpc_toolkit::reflect_ts!(AcmeProvider);
impl FromStr for AcmeProvider {
    type Err = <Url as FromStr>::Err;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "letsencrypt" => LetsEncrypt::Production.url().parse(),
            "letsencrypt-staging" => LetsEncrypt::Staging.url().parse(),
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

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
pub struct InitAcmeParams {
    #[arg(long, help = "help.arg.acme-provider")]
    pub provider: AcmeProvider,
    #[arg(long, help = "help.arg.acme-contact")]
    pub contact: Vec<String>,
}

rpc_toolkit::reflect_ts!(InitAcmeParams);
rpc_toolkit::ts_export!(InitAcmeParams, namespaces = [""]);

lazy_static::lazy_static! {
    static ref EMAIL_REGEX: Regex =
        Regex::new(r"^[^@\s]+@[^@\s]+\.[^@\s]+$").unwrap();
}

fn invalid_contact_err(contact: &str) -> Error {
    Error::new(
        eyre!("{}", t!("acme.invalid-contact", contact = contact)),
        ErrorKind::InvalidRequest,
    )
}

// RFC 8555 §7.3 requires contact URLs; Let's Encrypt accepts `mailto:`.
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

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
pub struct RemoveAcmeParams {
    #[arg(long, help = "help.arg.acme-provider")]
    pub provider: AcmeProvider,
}

rpc_toolkit::reflect_ts!(RemoveAcmeParams);
rpc_toolkit::ts_export!(RemoveAcmeParams, namespaces = [""]);

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
    use std::time::{Duration, SystemTime};

    use instant_acme::{Error as AcmeError, Problem};

    use super::{
        MAX_RETRY_AFTER, normalize_contact, retry_after_from_order_error, validate_contact,
    };

    fn rate_limit(retry_after: Option<SystemTime>) -> AcmeError {
        AcmeError::RateLimited {
            retry_after,
            problem: serde_json::from_value(serde_json::json!({
                "type": "urn:ietf:params:acme:error:rateLimited",
                "detail": "Please visit: https://portal.letsencrypt.org/sfe/v1/unpause?jwt=test-token",
                "status": 429,
            })).unwrap(),
        }
    }

    #[test]
    fn rate_limited_with_retry_after_passes_through() {
        let err = rate_limit(Some(SystemTime::now() + Duration::from_secs(120)));
        let duration = retry_after_from_order_error(&err).unwrap();
        assert!(duration <= Duration::from_secs(120));
        assert!(duration > Duration::from_secs(115));
    }

    #[test]
    fn rate_limited_retry_after_capped_at_max() {
        let err = rate_limit(Some(
            SystemTime::now() + Duration::from_secs(365 * 24 * 60 * 60),
        ));
        assert_eq!(retry_after_from_order_error(&err), Some(MAX_RETRY_AFTER));
    }

    #[test]
    fn rate_limited_past_deadline_returns_zero() {
        assert_eq!(
            retry_after_from_order_error(&rate_limit(Some(SystemTime::UNIX_EPOCH))),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn rate_limited_without_retry_after_returns_none() {
        assert_eq!(retry_after_from_order_error(&rate_limit(None)), None);
    }

    #[test]
    fn non_429_problem_returns_none() {
        let problem: Problem = serde_json::from_value(serde_json::json!({"status":503})).unwrap();
        assert_eq!(retry_after_from_order_error(&AcmeError::Api(problem)), None);
    }

    #[test]
    fn polling_retry_deadline_is_capped_and_past_deadlines_allow_retry() {
        let err = AcmeError::Timeout(Some(std::time::Instant::now() + MAX_RETRY_AFTER * 2));
        assert_eq!(retry_after_from_order_error(&err), Some(MAX_RETRY_AFTER));
        let err = AcmeError::Timeout(Some(std::time::Instant::now() - Duration::from_secs(1)));
        assert_eq!(retry_after_from_order_error(&err), Some(Duration::ZERO));
        assert_eq!(
            retry_after_from_order_error(&AcmeError::Timeout(None)),
            None
        );
    }

    #[test]
    fn non_http_errors_return_none() {
        assert_eq!(retry_after_from_order_error(&AcmeError::Crypto), None);
    }

    #[test]
    fn notification_preserves_unpause_link_in_every_locale() {
        let error = rate_limit(None).to_string();
        for locale in ["en_US", "de_DE", "es_ES", "fr_FR", "pl_PL"] {
            let message =
                crate::prelude::t!("acme.order-failed-message", locale = locale, error = error);
            assert!(
                message.contains("https://portal.letsencrypt.org/sfe/v1/unpause?jwt=test-token"),
                "{locale}: {message}"
            );
        }
    }

    #[test]
    fn validate_contact_accepts_mailto_uri() {
        assert!(validate_contact("mailto:admin@example.com").is_ok());
        assert!(validate_contact("mailto:user.name+tag@sub.example.co.uk").is_ok());
    }

    #[test]
    fn validate_contact_rejects_bare_email() {
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

#[cfg(test)]
mod bootstrap_tests {
    use patch_db::ModelExt;

    use super::*;
    use crate::net::ssl::{CertBranding, gen_nistp256, make_root_cert};
    use crate::util::serde::Pem;

    fn names(name: &str) -> BTreeSet<InternedString> {
        [InternedString::from(name)].into_iter().collect()
    }

    fn certificate(days_left: u64) -> AcmeCert {
        let key = gen_nistp256().unwrap();
        let start = SystemTime::now() - Duration::from_secs((10 * 364 - days_left) * 86400);
        let cert = make_root_cert(&key, &CertBranding::start_os("test"), start).unwrap();
        AcmeCert {
            key: Der64(key),
            fullchain: vec![Pem(cert)],
        }
    }

    fn provider() -> AcmeProvider {
        "https://ca.invalid/directory".parse().unwrap()
    }

    fn snapshot(cert: AcmeCert) -> BootstrapAcme {
        let provider = provider();
        BootstrapAcme {
            providers: [(JsonKey::new(names("unlock.example.com")), provider.clone())]
                .into_iter()
                .collect(),
            settings: [(
                provider.clone(),
                AcmeSettings {
                    contact: vec!["mailto:admin@example.com".into()],
                },
            )]
            .into_iter()
            .collect(),
            cache: AcmeCertStore {
                accounts: BTreeMap::new(),
                certs: [(
                    provider.0,
                    [(JsonKey::new(names("unlock.example.com")), cert)]
                        .into_iter()
                        .collect(),
                )]
                .into_iter()
                .collect(),
            },
        }
    }

    fn handler(
        db: TypedPatchDb<BootstrapAcme>,
    ) -> AcmeTlsHandler<BootstrapAcme, BootstrapAcmeProvider> {
        AcmeTlsHandler {
            get_provider: BootstrapAcmeProvider(db.clone()),
            db,
            acme_cache: Arc::new(SyncMutex::new(BTreeMap::new())),
            crypto_provider: Arc::new(tokio_rustls::rustls::crypto::ring::default_provider()),
            in_progress: Watch::new(BTreeMap::new()),
            report_failure: None,
        }
    }

    #[test]
    fn bootstrap_handoff_preserves_healthy_cache_and_replaces_due_certificates() {
        for (days_left, replace) in [(10, true), (100, false)] {
            let account =
                crate::account::AccountInfo::new("cache-test-password", SystemTime::now(), None)
                    .unwrap();
            let mut db = Model::new(&Database::init(&account, false, None, None).unwrap()).unwrap();
            let renewed = snapshot(certificate(365));
            db.as_public_mut()
                .as_server_info_mut()
                .as_network_mut()
                .as_acme_mut()
                .ser(&renewed.settings)
                .unwrap();
            let existing = snapshot(certificate(days_left));
            let sans = JsonKey::new(names("unlock.example.com"));
            let expected = if replace {
                &renewed.cache.certs[&provider().0][&sans]
            } else {
                &existing.cache.certs[&provider().0][&sans]
            }
            .key
            .0
            .public_key_to_der()
            .unwrap();
            <Database as DbAccessMut<AcmeCertStore>>::access_mut(&mut db)
                .ser(&existing.cache)
                .unwrap();
            renewed.merge_cache_into(&mut db).unwrap();
            let stored = <Database as DbAccess<AcmeCertStore>>::access(&db)
                .as_certs()
                .as_idx(&provider().0)
                .unwrap()
                .as_idx(&sans)
                .unwrap()
                .de()
                .unwrap();
            assert_eq!(stored.key.0.public_key_to_der().unwrap(), expected);
        }
    }

    #[test]
    fn bootstrap_handoff_cannot_override_changed_provider_settings() {
        let account =
            crate::account::AccountInfo::new("cache-test-password", SystemTime::now(), None)
                .unwrap();
        let mut db = Model::new(&Database::init(&account, false, None, None).unwrap()).unwrap();
        let renewed = snapshot(certificate(365));
        db.as_public_mut()
            .as_server_info_mut()
            .as_network_mut()
            .as_acme_mut()
            .ser(&BTreeMap::from([(
                provider(),
                AcmeSettings {
                    contact: vec!["mailto:replacement@example.com".into()],
                },
            )]))
            .unwrap();
        renewed.merge_cache_into(&mut db).unwrap();
        let stored = <Database as DbAccess<AcmeCertStore>>::access(&db)
            .de()
            .unwrap();
        assert!(stored.accounts.is_empty());
        assert!(stored.certs.is_empty());
    }

    #[tokio::test]
    async fn snapshot_limits_authority_and_reuses_the_account_and_certificate_cache() {
        let mut original = snapshot(certificate(100));
        let contacts = JsonKey::new(vec!["mailto:admin@example.com".into()]);
        let account = gen_nistp256().unwrap().private_key_to_pkcs8().unwrap();
        original.cache.accounts.insert(
            contacts.clone(),
            Der64(pkcs8::Document::try_from(account.as_slice()).unwrap()),
        );
        original.cache.accounts.insert(
            JsonKey::new(vec!["mailto:other@example.com".into()]),
            Der64(
                pkcs8::Document::try_from(
                    gen_nistp256()
                        .unwrap()
                        .private_key_to_pkcs8()
                        .unwrap()
                        .as_slice(),
                )
                .unwrap(),
            ),
        );
        original
            .cache
            .certs
            .get_mut(&provider().0)
            .unwrap()
            .insert(JsonKey::new(names("other.example.com")), certificate(100));
        original.cache.certs.insert(
            "https://other-ca.invalid/directory".parse().unwrap(),
            [(JsonKey::new(names("unlock.example.com")), certificate(100))]
                .into_iter()
                .collect(),
        );
        let db = Model::<Database>::from_value(imbl_value::json!({
            "private": {"keyStore": {"acme": original.cache}},
            "public": {"serverInfo": {"network": {"acme": original.settings}}},
        }));
        let captured = BootstrapAcme::capture(&db, original.providers).unwrap();
        assert_eq!(captured.settings.len(), 1);
        assert_eq!(captured.cache.accounts.len(), 1);
        assert_eq!(captured.cache.certs.len(), 1);
        assert_eq!(captured.cache.certs[&provider().0].len(), 1);
        let serialized = serde_json::to_vec(&captured).unwrap();
        let db = serde_json::from_slice::<BootstrapAcme>(&serialized)
            .unwrap()
            .materialize()
            .await
            .unwrap();
        let cache = AcmeCertCache(&db);
        assert_eq!(
            cache
                .read_account(&["mailto:admin@example.com"])
                .await
                .unwrap()
                .unwrap(),
            account
        );
        let tls = handler(db.clone());
        assert!(tls.get_cert(&names("unlock.example.com")).await.is_some());
        assert!(tls.get_cert(&names("other.example.com")).await.is_none());
        let replacement = certificate(200);
        let replacement_der = replacement.fullchain[0].0.to_der().unwrap();
        cache
            .write_certificate(
                &names("unlock.example.com"),
                provider().as_ref(),
                &replacement,
            )
            .await
            .unwrap();
        cache
            .write_account(&["mailto:admin@example.com"], &account)
            .await
            .unwrap();
        let handoff = db.peek().await.de().unwrap();
        let restored =
            serde_json::from_slice::<BootstrapAcme>(&serde_json::to_vec(&handoff).unwrap())
                .unwrap()
                .materialize()
                .await
                .unwrap();
        assert_eq!(
            restored
                .peek()
                .await
                .as_cache()
                .as_certs()
                .as_idx(&provider().0)
                .unwrap()
                .as_idx(JsonKey::new_ref(&names("unlock.example.com")))
                .unwrap()
                .de()
                .unwrap()
                .fullchain[0]
                .0
                .to_der()
                .unwrap(),
            replacement_der
        );
        assert_eq!(
            AcmeCertCache(&restored)
                .read_account(&["mailto:admin@example.com"])
                .await
                .unwrap()
                .unwrap(),
            account
        );
    }

    #[tokio::test]
    async fn renewal_backoff_serves_unexpired_cache_but_never_expired_cache() {
        for (days, expected) in [(10, true), (0, false)] {
            let tls = handler(snapshot(certificate(days)).materialize().await.unwrap());
            let sans = names("unlock.example.com");
            tls.in_progress.send_modify(|map| {
                map.insert(
                    sans.clone(),
                    OrderEntry {
                        backoff_until: Some(Instant::now() + Duration::from_secs(60)),
                        ..OrderEntry::default()
                    },
                );
            });
            assert_eq!(tls.get_cert(&sans).await.is_some(), expected);
            assert!(tls.acme_cache.peek(|cache| cache.is_empty()));
        }
    }

    #[tokio::test]
    async fn missing_settings_still_serves_unexpired_cache_without_starting_an_order() {
        let mut snapshot = snapshot(certificate(10));
        snapshot.settings.clear();
        let tls = handler(snapshot.materialize().await.unwrap());
        assert!(tls.get_cert(&names("unlock.example.com")).await.is_some());
        assert!(tls.in_progress.peek(|map| map.is_empty()));
    }
}
