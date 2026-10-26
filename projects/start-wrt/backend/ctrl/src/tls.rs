//! Certificates for the HTTPS listener. The local CA issues one per name the
//! router is reached by; Let's Encrypt issues one for the DDNS hostname when
//! that is enabled.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::net::IpAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::FutureExt;
use imbl_value::InternedString;
use itertools::Itertools;
use openssl::asn1::Asn1Time;
use openssl::x509::X509Ref;
use patch_db::value::index::Index;
use patch_db::ModelExt;
use rustls::crypto::CryptoProvider;
use rustls::server::ClientHello;
use rustls::sign::CertifiedKey;
use rustls::ServerConfig;
use serde::{Deserialize, Serialize};
use startos::db::model::public::AcmeSettings;
use startos::db::{DbAccess, DbAccessByKey, DbAccessMut};
use startos::net::acme::{
    AcmeCertStore, AcmeProvider, AcmeTlsHandler, GetAcmeProvider, ReportOrderFailure,
    ACME_TLS_ALPN_NAME,
};
use startos::net::ssl::{cert_is_unexpired, root_ca_config, CertStore};
use startos::net::tls::{SingleCertResolver, TlsHandler, TlsHandlerAction};
use startos::net::web_server::{extract, Accept, ExtractVisitor, TcpMetadata};
use startos::prelude::{HasModel, JsonKey, Model, PatchDb, TypedPatchDb};
use startos::util::serde::Pem;
use startos::util::sync::{SyncMutex, Watch};
use tokio::sync::Notify;
use visit_rs::Visit;

use crate::prelude::*;
use crate::ssl::{read_ca_chain, startwrt_branding, ROUTER_HOSTNAME};
use crate::wan::AcmeCertificateStatus;
use crate::{captive, ServerContext};

/// Holds private keys; the directory alone is readable by root.
const CERT_DIR: &str = "/etc/startwrt/certs";
const CERT_DB: &str = "certs.db";

const LETS_ENCRYPT: &str = "letsencrypt";

/// Between attempts while the DDNS hostname has no certificate. On its own
/// stays under Let's Encrypt's five failed validations per hostname an hour.
const ACME_RETRY: Duration = Duration::from_secs(15 * 60);
/// Between renewal checks once it has one.
const ACME_RECHECK: Duration = Duration::from_secs(60 * 60);

static ROUTER_CERTS: OnceLock<RouterCerts> = OnceLock::new();

/// Deriving `HasModel` emits an inherent `impl` on `Model`, allowed only in
/// start-core.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertDatabase {
    local: CertStore,
    #[serde(default)]
    acme: AcmeCertStore,
    #[serde(default)]
    acme_settings: BTreeMap<AcmeProvider, AcmeSettings>,
}
impl HasModel for CertDatabase {
    type Model = Model<Self>;
}

const LOCAL: &str = "local";
const ACME: &str = "acme";
const ACME_SETTINGS: &str = "acmeSettings";

fn field<'a, T>(db: &'a Model<CertDatabase>, key: &str) -> &'a Model<T> {
    db.transmute_ref(|v| key.index_into(v).unwrap_or(&patch_db::value::NULL))
}
fn field_mut<'a, T>(db: &'a mut Model<CertDatabase>, key: &str) -> &'a mut Model<T> {
    db.transmute_mut(|v| key.index_or_insert(v))
}
fn acme_settings(db: &Model<CertDatabase>) -> &Model<BTreeMap<AcmeProvider, AcmeSettings>> {
    field(db, ACME_SETTINGS)
}
fn acme_settings_mut(
    db: &mut Model<CertDatabase>,
) -> &mut Model<BTreeMap<AcmeProvider, AcmeSettings>> {
    field_mut(db, ACME_SETTINGS)
}

impl DbAccess<CertStore> for CertDatabase {
    fn access(db: &Model<Self>) -> &Model<CertStore> {
        field(db, LOCAL)
    }
}
impl DbAccessMut<CertStore> for CertDatabase {
    fn access_mut(db: &mut Model<Self>) -> &mut Model<CertStore> {
        field_mut(db, LOCAL)
    }
}
impl DbAccess<AcmeCertStore> for CertDatabase {
    fn access(db: &Model<Self>) -> &Model<AcmeCertStore> {
        field(db, ACME)
    }
}
impl DbAccessMut<AcmeCertStore> for CertDatabase {
    fn access_mut(db: &mut Model<Self>) -> &mut Model<AcmeCertStore> {
        field_mut(db, ACME)
    }
}
impl DbAccessByKey<AcmeSettings> for CertDatabase {
    type Key<'a> = &'a AcmeProvider;
    fn access_by_key<'a>(
        db: &'a Model<Self>,
        key: Self::Key<'_>,
    ) -> Option<&'a Model<AcmeSettings>> {
        acme_settings(db).as_idx(key)
    }
}

fn logged<T, E: std::fmt::Display>(res: Result<T, E>) -> Option<T> {
    res.map_err(|e| tracing::error!("{e}")).ok()
}

fn lets_encrypt() -> Result<AcmeProvider, Error> {
    LETS_ENCRYPT
        .parse()
        .map_err(|e| Error::new(e, ErrorKind::InvalidRequest))
}

/// The local CA from `/etc/ssl`, with no leaves.
fn local_store() -> Result<CertStore, Error> {
    let ((root_key, root_cert), (int_key, int_cert)) = read_ca_chain()?;
    Ok(CertStore {
        root_key: Pem::new(root_key),
        root_cert: Pem::new(root_cert),
        int_key: Pem::new(int_key),
        int_cert: Pem::new(int_cert),
        leaves: BTreeMap::new(),
    })
}

fn same_ca(a: &CertStore, b: &CertStore) -> Result<bool, Error> {
    Ok(a.root_cert.0.to_der()? == b.root_cert.0.to_der()?
        && a.int_cert.0.to_der()? == b.int_cert.0.to_der()?)
}

/// Keeps the stored leaves while the CA is unchanged, dropping expired ones.
fn reconcile_local(stored: CertStore, fresh: CertStore) -> Result<CertStore, Error> {
    if !same_ca(&stored, &fresh)? {
        return Ok(fresh);
    }
    let mut store = stored;
    store
        .leaves
        .retain(|_, leaf| cert_is_unexpired(&leaf.certs.nistp256).unwrap_or(false));
    Ok(store)
}

fn not_after(cert: &X509Ref) -> Result<DateTime<Utc>, Error> {
    let diff = Asn1Time::from_unix(0)?.diff(cert.not_after())?;
    DateTime::from_timestamp(i64::from(diff.days) * 86400 + i64::from(diff.secs), 0)
        .ok_or_else(|| Error::new(eyre!("certificate expiry out of range"), ErrorKind::OpenSsl))
}

/// The DDNS hostname alone, while ACME is on for it. Reads the DDNS name as
/// last refreshed by [`ServerContext::ddns_name`].
#[derive(Clone)]
struct DdnsAcmeProvider {
    ctx: ServerContext,
    provider: AcmeProvider,
}
impl GetAcmeProvider for DdnsAcmeProvider {
    async fn get_provider<'a, 'b: 'a>(
        &'b self,
        san_info: &'a BTreeSet<InternedString>,
    ) -> Option<impl AsRef<AcmeProvider> + Send + 'b> {
        let name = self.ctx.cached_ddns_name();
        let hostname = name.hostname.filter(|_| name.acme)?;
        (san_info.len() == 1 && san_info.contains(&hostname)).then(|| self.provider.clone())
    }
}

type Acme = Arc<AcmeTlsHandler<CertDatabase, DdnsAcmeProvider>>;

/// Called inline at the concrete database type, the compiler cannot prove
/// the ACME future `Send`.
fn acme_cert<'x, M, S>(
    acme: &'x Arc<AcmeTlsHandler<M, S>>,
    sans: &'x BTreeSet<InternedString>,
) -> impl Future<Output = Option<CertifiedKey>> + Send + 'x
where
    for<'m> M: DbAccessByKey<AcmeSettings, Key<'m> = &'m AcmeProvider>
        + DbAccessMut<AcmeCertStore>
        + HasModel<Model = Model<M>>
        + Send
        + Sync
        + 'static,
    S: GetAcmeProvider + Clone + Send + Sync,
{
    acme.get_cert(sans)
}

fn cached_acme_cert<'x, M, S>(
    acme: &'x Arc<AcmeTlsHandler<M, S>>,
    sans: &'x BTreeSet<InternedString>,
) -> impl Future<Output = Option<CertifiedKey>> + Send + 'x
where
    for<'m> M: DbAccessByKey<AcmeSettings, Key<'m> = &'m AcmeProvider>
        + DbAccessMut<AcmeCertStore>
        + HasModel<Model = Model<M>>
        + Send
        + Sync
        + 'static,
    S: GetAcmeProvider + Clone + Send + Sync,
{
    acme.get_cached_cert(sans)
}

fn acme_config<'a, A, M, S>(
    acme: &'a mut Arc<AcmeTlsHandler<M, S>>,
    hello: &'a ClientHello<'a>,
    metadata: &'a <A as Accept>::Metadata,
) -> impl Future<Output = Option<TlsHandlerAction>> + Send + 'a
where
    A: Accept + 'a,
    <A as Accept>::Metadata: Send + Sync,
    for<'m> M: DbAccessByKey<AcmeSettings, Key<'m> = &'m AcmeProvider>
        + DbAccessMut<AcmeCertStore>
        + HasModel<Model = Model<M>>
        + Send
        + Sync
        + 'static,
    S: GetAcmeProvider + Clone + Send + Sync + 'static,
{
    <Arc<AcmeTlsHandler<M, S>> as TlsHandler<'a, A>>::get_config(acme, hello, metadata)
}

#[derive(Clone)]
struct RouterCerts {
    db: TypedPatchDb<CertDatabase>,
    acme: Acme,
    provider: AcmeProvider,
    check: Arc<Notify>,
}

/// An unreadable store is set aside and replaced by an empty one.
async fn open_or_replace(
    path: &Path,
    provider: &AcmeProvider,
    ca: CertStore,
) -> Result<TypedPatchDb<CertDatabase>, Error> {
    match open_store(path, provider, ca.clone()).await {
        Ok(db) => Ok(db),
        Err(e) => {
            tracing::error!("{e}; starting a new certificate store");
            tokio::fs::rename(path, path.with_extension("db.unreadable")).await?;
            open_store(path, provider, ca).await
        }
    }
}

/// Keeps the stored leaves only while the CA is unchanged.
async fn open_store(
    path: &Path,
    provider: &AcmeProvider,
    ca: CertStore,
) -> Result<TypedPatchDb<CertDatabase>, Error> {
    let db = PatchDb::open(path).await.map_err(|e| {
        Error::new(
            eyre!("Failed to open {}: {e}", path.display()),
            ErrorKind::Filesystem,
        )
    })?;
    let fresh = ca.clone();
    let db = TypedPatchDb::<CertDatabase>::load_or_init(db, || async {
        Ok(CertDatabase {
            local: ca,
            acme: AcmeCertStore::new(),
            acme_settings: BTreeMap::new(),
        })
    })
    .await?;
    db.mutate(|db| {
        let local = <CertDatabase as DbAccessMut<CertStore>>::access_mut(db);
        let store = reconcile_local(local.de()?, fresh)?;
        local.ser(&store)?;
        let settings = acme_settings_mut(db);
        if settings.as_idx(provider).is_none() {
            settings.insert(
                provider,
                &AcmeSettings {
                    contact: Vec::new(),
                },
            )?;
        }
        Ok(())
    })
    .await
    .result?;
    Ok(db)
}

impl RouterCerts {
    async fn open(ctx: ServerContext, crypto_provider: Arc<CryptoProvider>) -> Result<Self, Error> {
        let dir = Path::new(CERT_DIR);
        tokio::fs::create_dir_all(dir).await?;
        tokio::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).await?;
        let path = dir.join(CERT_DB);
        let provider = lets_encrypt()?;
        let db = open_or_replace(&path, &provider, local_store()?).await?;

        let report: ReportOrderFailure =
            Arc::new(|sans: BTreeSet<InternedString>, reason: String| {
                crate::activity::log(
                    "wan",
                    "acme-failed",
                    false,
                    &format!(
                        "Failed to obtain a certificate for {}",
                        sans.iter().join(", ")
                    ),
                    Some(&reason),
                );
                async {}.boxed()
            });
        let acme = Arc::new(AcmeTlsHandler {
            db: db.clone(),
            acme_cache: Arc::new(SyncMutex::new(BTreeMap::new())),
            crypto_provider,
            get_provider: DdnsAcmeProvider {
                ctx,
                provider: provider.clone(),
            },
            in_progress: Watch::new(BTreeMap::new()),
            report_failure: Some(report),
        });
        Ok(Self {
            db,
            acme,
            provider,
            check: Arc::new(Notify::new()),
        })
    }

    async fn acme_status(&self, hostname: &InternedString) -> Option<AcmeCertificateStatus> {
        let sans: BTreeSet<InternedString> = [hostname.clone()].into();
        let peek = self.db.peek().await;
        let expires = <CertDatabase as DbAccess<AcmeCertStore>>::access(&peek)
            .as_certs()
            .as_idx(&self.provider.0)
            .and_then(|certs| certs.as_idx(JsonKey::new_ref(&sans)))
            .and_then(|cert| logged(cert.de()))
            .and_then(|cert| {
                let leaf = &cert.fullchain.first()?.0;
                cert_is_unexpired(leaf)
                    .unwrap_or(false)
                    .then(|| logged(not_after(leaf)))?
            });
        Some(match expires {
            Some(expires) => AcmeCertificateStatus::Issued { expires },
            None => match self.acme.last_failure(&sans) {
                Some(reason) => AcmeCertificateStatus::Failed { reason },
                None => AcmeCertificateStatus::Pending,
            },
        })
    }

    /// Obtains and renews the DDNS hostname's certificate for the daemon's
    /// lifetime.
    async fn maintain_acme(self, ctx: ServerContext) {
        loop {
            let name = ctx.ddns_name().await;
            let mut wait = ACME_RECHECK;
            if let (Some(hostname), true) = (name.hostname, name.acme) {
                let sans: BTreeSet<InternedString> = [hostname].into();
                if acme_cert(&self.acme, &sans).await.is_none() {
                    wait = ACME_RETRY;
                }
            }
            tokio::select! {
                _ = self.check.notified() => {}
                _ = tokio::time::sleep(wait) => {}
            }
        }
    }
}

/// `None` outside the daemon.
pub async fn acme_status(hostname: &str) -> Option<AcmeCertificateStatus> {
    let certs = ROUTER_CERTS.get()?;
    certs
        .acme_status(&InternedString::intern(hostname.to_ascii_lowercase()))
        .await
}

/// Wakes the ACME task to act on changed DDNS settings.
pub fn request_acme_certificate() {
    if let Some(certs) = ROUTER_CERTS.get() {
        certs.check.notify_one();
    }
}

/// Opens the certificate store and starts the ACME task.
pub async fn init(ctx: ServerContext) -> Result<RouterTlsHandler, Error> {
    let crypto_provider = Arc::new(rustls::crypto::ring::default_provider());
    let certs = RouterCerts::open(ctx.clone(), crypto_provider.clone()).await?;
    if ROUTER_CERTS.set(certs.clone()).is_ok() {
        tokio::spawn(certs.clone().maintain_acme(ctx.clone()));
    }
    Ok(RouterTlsHandler {
        db: certs.db,
        acme: certs.acme,
        ctx,
        crypto_provider,
    })
}

fn http_alpn() -> Vec<Vec<u8>> {
    vec![b"h2".to_vec(), b"http/1.1".to_vec()]
}

/// The local CA's certificate for one name.
async fn local_config(
    db: &TypedPatchDb<CertDatabase>,
    crypto_provider: &Arc<CryptoProvider>,
    hello: &ClientHello<'_>,
    name: InternedString,
) -> Option<TlsHandlerAction> {
    let mut cfg = root_ca_config(
        db,
        crypto_provider,
        &startwrt_branding(""),
        hello,
        [name].into(),
    )
    .await?;
    cfg.alpn_protocols = http_alpn();
    Some(TlsHandlerAction::Tls(cfg))
}

/// Serves `router.lan` and the DDNS hostname by name, and any other
/// connection by the router address it reached.
#[derive(Clone)]
pub struct RouterTlsHandler {
    db: TypedPatchDb<CertDatabase>,
    acme: Acme,
    ctx: ServerContext,
    crypto_provider: Arc<CryptoProvider>,
}

impl<'a, A> TlsHandler<'a, A> for RouterTlsHandler
where
    A: Accept + 'a,
    <A as Accept>::Metadata: Visit<ExtractVisitor<TcpMetadata>> + Clone + Send + Sync + 'static,
{
    async fn get_config(
        &'a mut self,
        hello: &'a ClientHello<'a>,
        metadata: &'a <A as Accept>::Metadata,
    ) -> Option<TlsHandlerAction> {
        if captive::is_active() && !hello.server_name().is_none_or(captive::serves) {
            return None;
        }
        let ddns = match hello.server_name() {
            Some(name) if !name.eq_ignore_ascii_case(ROUTER_HOSTNAME) => {
                self.ctx.ddns_name().await.hostname
            }
            _ => None,
        };
        let dialed = extract::<TcpMetadata, _>(metadata).map(|tcp| tcp.local_addr.ip());
        match served(hello.server_name(), ddns.as_ref(), dialed) {
            Served::Ddns(name) => {
                if hello
                    .alpn()
                    .into_iter()
                    .flatten()
                    .any(|a| a == ACME_TLS_ALPN_NAME)
                {
                    return acme_config::<A, _, _>(&mut self.acme, hello, metadata).await;
                }
                let sans = [name.clone()].into();
                if let Some(cert) = cached_acme_cert(&self.acme, &sans).await {
                    let mut cfg = ServerConfig::builder_with_provider(self.crypto_provider.clone())
                        .with_safe_default_protocol_versions()
                        .ok()?
                        .with_no_client_auth()
                        .with_cert_resolver(Arc::new(SingleCertResolver(Arc::new(cert))));
                    cfg.alpn_protocols = http_alpn();
                    Some(TlsHandlerAction::Tls(cfg))
                } else {
                    local_config(&self.db, &self.crypto_provider, hello, name).await
                }
            }
            Served::Local(name) => local_config(&self.db, &self.crypto_provider, hello, name).await,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Served {
    /// The DDNS hostname: its ACME certificate where one exists.
    Ddns(InternedString),
    Local(InternedString),
}

/// `router.lan` and the DDNS hostname are served by name. Any other name, or
/// none, is served by the router address the connection reached.
fn served(
    requested: Option<&str>,
    ddns: Option<&InternedString>,
    dialed: Option<IpAddr>,
) -> Served {
    let requested = requested
        .map(|name| InternedString::intern(name.trim_end_matches('.').to_ascii_lowercase()));
    match requested {
        Some(name) if &*name == ROUTER_HOSTNAME => Served::Local(name),
        Some(name) if ddns == Some(&name) => Served::Ddns(name),
        _ => Served::Local(match dialed {
            Some(ip) => InternedString::from_display(&ip.to_canonical()),
            None => InternedString::intern(ROUTER_HOSTNAME),
        }),
    }
}

#[cfg(test)]
mod tests {
    use openssl::pkey::PKey;
    use openssl::x509::X509;
    use startos::net::ssl::{gen_nistp256, make_int_cert, make_root_cert};

    use super::*;

    fn store() -> CertStore {
        let branding = startwrt_branding("test");
        let root_key = gen_nistp256().unwrap();
        let root_cert = make_root_cert(&root_key, &branding, std::time::SystemTime::now()).unwrap();
        let int_key = gen_nistp256().unwrap();
        let int_cert = make_int_cert((&root_key, &root_cert), &int_key, &branding).unwrap();
        CertStore {
            root_key: Pem::new(root_key),
            root_cert: Pem::new(root_cert),
            int_key: Pem::new(int_key),
            int_cert: Pem::new(int_cert),
            leaves: BTreeMap::new(),
        }
    }

    /// Same CA, separately loaded.
    fn copy(store: &CertStore) -> CertStore {
        let key = |k: &PKey<openssl::pkey::Private>| {
            PKey::private_key_from_der(&k.private_key_to_der().unwrap()).unwrap()
        };
        CertStore {
            root_key: Pem::new(key(&store.root_key.0)),
            root_cert: Pem::new(store.root_cert.0.clone()),
            int_key: Pem::new(key(&store.int_key.0)),
            int_cert: Pem::new(store.int_cert.0.clone()),
            leaves: BTreeMap::new(),
        }
    }

    async fn db_with(local: CertStore) -> TypedPatchDb<CertDatabase> {
        TypedPatchDb::load_or_init(PatchDb::in_memory(), || async {
            Ok::<_, startos::Error>(CertDatabase {
                local,
                acme: AcmeCertStore::new(),
                acme_settings: BTreeMap::new(),
            })
        })
        .await
        .unwrap()
    }

    async fn issue(db: &TypedPatchDb<CertDatabase>, name: &str) -> (Vec<String>, Vec<Vec<u8>>) {
        let hostnames: BTreeSet<InternedString> = [InternedString::intern(name)].into();
        let cert = db
            .mutate(|db| {
                <CertDatabase as DbAccessMut<CertStore>>::access_mut(db)
                    .cert_for(&hostnames, &startwrt_branding(""))
            })
            .await
            .result
            .unwrap();
        let sans = cert.leaf.certs.nistp256.subject_alt_names().unwrap();
        (
            sans.iter()
                .filter_map(|n| n.dnsname().map(str::to_string))
                .collect(),
            sans.iter()
                .filter_map(|n| n.ipaddress().map(<[u8]>::to_vec))
                .collect(),
        )
    }

    async fn stored(db: &TypedPatchDb<CertDatabase>) -> CertStore {
        <CertDatabase as DbAccess<CertStore>>::access(&db.peek().await)
            .de()
            .unwrap()
    }

    async fn router_with_acme(provider: AcmeProvider) -> RouterTlsHandler {
        let ctx = ServerContext {
            auth: Arc::new(crate::auth::AuthState::with_ddns_name(
                crate::auth::DdnsName {
                    hostname: Some(InternedString::intern("home.example.com")),
                    acme: true,
                },
            )),
            ..ServerContext::default()
        };
        let db = db_with(store()).await;
        db.mutate(|db| {
            acme_settings_mut(db).insert(
                &provider,
                &AcmeSettings {
                    contact: Vec::new(),
                },
            )
        })
        .await
        .result
        .unwrap();
        let crypto_provider = Arc::new(rustls::crypto::ring::default_provider());
        let acme = Arc::new(AcmeTlsHandler {
            db: db.clone(),
            acme_cache: Arc::new(SyncMutex::new(BTreeMap::new())),
            crypto_provider: crypto_provider.clone(),
            get_provider: DdnsAcmeProvider {
                ctx: ctx.clone(),
                provider,
            },
            in_progress: Watch::new(BTreeMap::new()),
            report_failure: None,
        });
        RouterTlsHandler {
            db,
            acme,
            ctx,
            crypto_provider,
        }
    }

    async fn resolved_certificate(router: &mut RouterTlsHandler, name: &str) -> X509 {
        use rustls::pki_types::ServerName;
        use rustls::server::Acceptor;
        use rustls::{ClientConfig, ClientConnection, RootCertStore};
        let client_config = ClientConfig::builder_with_provider(router.crypto_provider.clone())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(RootCertStore::empty())
            .with_no_client_auth();
        let wire_name = if name.ends_with('.') {
            format!("{}x", name.trim_end_matches('.'))
        } else {
            name.to_string()
        };
        let mut client = ClientConnection::new(
            Arc::new(client_config),
            ServerName::try_from(wire_name.clone()).unwrap(),
        )
        .unwrap();
        let mut bytes = Vec::new();
        client.write_tls(&mut bytes).unwrap();
        // rustls clients omit trailing dots from SNI.
        if name.ends_with('.') {
            let offset = bytes
                .windows(wire_name.len())
                .position(|part| part == wire_name.as_bytes())
                .unwrap();
            bytes[offset + wire_name.len() - 1] = b'.';
        }
        let mut acceptor = Acceptor::default();
        acceptor.read_tls(&mut std::io::Cursor::new(bytes)).unwrap();
        let accepted = acceptor.accept().unwrap().unwrap();
        let hello = accepted.client_hello();
        assert_eq!(hello.server_name(), Some(name));
        let metadata = TcpMetadata {
            local_addr: "192.168.20.1:443".parse().unwrap(),
            peer_addr: "192.168.20.2:12345".parse().unwrap(),
        };
        let action = tokio::time::timeout(
            Duration::from_millis(250),
            <RouterTlsHandler as TlsHandler<'_, tokio::net::TcpListener>>::get_config(
                router, &hello, &metadata,
            ),
        )
        .await
        .expect("HTTPS must not wait for an ACME order")
        .unwrap();
        let TlsHandlerAction::Tls(config) = action else {
            panic!("expected TLS");
        };
        assert_eq!(config.alpn_protocols, http_alpn());
        let cert = config.cert_resolver.resolve(hello).unwrap();
        X509::from_der(cert.cert[0].as_ref()).unwrap()
    }

    #[tokio::test]
    async fn ddns_handshake_uses_local_ca_while_acme_is_pending() {
        let stalled_directory = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let provider: AcmeProvider = format!(
            "http://{}/directory",
            stalled_directory.local_addr().unwrap()
        )
        .parse()
        .unwrap();
        let mut router = router_with_acme(provider).await;
        let cert = resolved_certificate(&mut router, "home.example.com").await;
        assert_eq!(
            cert.subject_alt_names().unwrap()[0].dnsname(),
            Some("home.example.com")
        );
        assert!(
            router.acme.in_progress.peek(|orders| orders.is_empty()),
            "only the maintenance task starts orders"
        );
    }

    #[tokio::test]
    async fn ddns_handshake_serves_cached_certificate_during_renewal_and_with_trailing_dot() {
        use openssl::hash::MessageDigest;
        use openssl::x509::extension::SubjectAlternativeName;
        use openssl::x509::X509NameBuilder;
        use startos::net::acme::AcmeCert;
        let stalled_directory = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let provider: AcmeProvider = format!(
            "http://{}/directory",
            stalled_directory.local_addr().unwrap()
        )
        .parse()
        .unwrap();
        let mut router = router_with_acme(provider.clone()).await;
        let key = gen_nistp256().unwrap();
        let mut subject = X509NameBuilder::new().unwrap();
        subject
            .append_entry_by_text("CN", "cached ACME certificate")
            .unwrap();
        let subject = subject.build();
        let mut cert = X509::builder().unwrap();
        cert.set_version(2).unwrap();
        cert.set_subject_name(&subject).unwrap();
        cert.set_issuer_name(&subject).unwrap();
        cert.set_pubkey(&key).unwrap();
        cert.set_not_before(&Asn1Time::from_unix(Utc::now().timestamp() - 60).unwrap())
            .unwrap();
        cert.set_not_after(&Asn1Time::days_from_now(5).unwrap())
            .unwrap();
        let san = SubjectAlternativeName::new()
            .dns("home.example.com")
            .build(&cert.x509v3_context(None, None))
            .unwrap();
        cert.append_extension(san).unwrap();
        cert.sign(&key, MessageDigest::sha256()).unwrap();
        let cert = cert.build();
        let sans: BTreeSet<InternedString> = [InternedString::intern("home.example.com")].into();
        router
            .db
            .mutate(|db| {
                <CertDatabase as DbAccessMut<AcmeCertStore>>::access_mut(db)
                    .as_certs_mut()
                    .upsert(&provider.0, || Ok(BTreeMap::new()))?
                    .insert(
                        JsonKey::new_ref(&sans),
                        &AcmeCert {
                            key: Pem(key),
                            fullchain: vec![Pem(cert.clone())],
                        },
                    )
            })
            .await
            .result
            .unwrap();
        for name in ["home.example.com.", "home.example.com"] {
            assert!(
                resolved_certificate(&mut router, name)
                    .await
                    .to_der()
                    .unwrap()
                    == cert.to_der().unwrap(),
                "must serve the cached ACME certificate for {name}"
            );
        }
        assert!(router.acme.in_progress.peek(|orders| orders.is_empty()));
    }

    #[test]
    fn served_names() {
        let ddns = InternedString::intern("home.example.com");
        let v4: IpAddr = "192.168.20.1".parse().unwrap();
        let mapped: IpAddr = "::ffff:192.168.20.1".parse().unwrap();
        let local = |name: &str| Served::Local(InternedString::intern(name));

        assert_eq!(
            served(Some("Router.LAN."), Some(&ddns), Some(v4)),
            local(ROUTER_HOSTNAME)
        );
        assert_eq!(
            served(Some("HOME.example.com."), Some(&ddns), Some(v4)),
            Served::Ddns(ddns.clone())
        );
        assert_eq!(
            served(Some("home.example.com"), None, Some(v4)),
            local("192.168.20.1")
        );
        assert_eq!(
            served(Some("attacker.example"), Some(&ddns), Some(v4)),
            local("192.168.20.1")
        );
        assert_eq!(
            served(None, Some(&ddns), Some(mapped)),
            local("192.168.20.1")
        );
        assert_eq!(
            served(None, None, Some("fd13:6d5c:b354:12::1".parse().unwrap())),
            local("fd13:6d5c:b354:12::1")
        );
        assert_eq!(served(None, None, None), local(ROUTER_HOSTNAME));
    }

    #[tokio::test]
    async fn leaf_names_only_the_name_asked_for() {
        let db = db_with(store()).await;
        assert_eq!(
            issue(&db, "192.168.20.1").await,
            (vec![], vec![vec![192, 168, 20, 1]])
        );
        assert_eq!(
            issue(&db, ROUTER_HOSTNAME).await,
            (vec![ROUTER_HOSTNAME.to_string()], vec![])
        );
    }

    #[tokio::test]
    async fn reconcile_keeps_leaves_under_the_same_ca() {
        let db = db_with(store()).await;
        issue(&db, "192.168.0.1").await;
        let stored = stored(&db).await;
        let fresh = copy(&stored);
        assert_eq!(reconcile_local(stored, fresh).unwrap().leaves.len(), 1);
    }

    #[tokio::test]
    async fn reconcile_drops_leaves_from_another_ca() {
        let db = db_with(store()).await;
        issue(&db, "192.168.0.1").await;
        let reconciled = reconcile_local(stored(&db).await, store()).unwrap();
        assert!(reconciled.leaves.is_empty());
    }

    #[tokio::test]
    async fn store_survives_reopen_and_replaces_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CERT_DB);
        let provider = lets_encrypt().unwrap();
        let base = store();

        let db = open_or_replace(&path, &provider, copy(&base))
            .await
            .unwrap();
        issue(&db, "192.168.0.1").await;
        drop(db);
        let db = open_or_replace(&path, &provider, copy(&base))
            .await
            .unwrap();
        assert_eq!(stored(&db).await.leaves.len(), 1);
        assert!(
            <CertDatabase as DbAccessByKey<AcmeSettings>>::access_by_key(
                &db.peek().await,
                &provider
            )
            .is_some()
        );
        drop(db);

        std::fs::write(&path, b"not a database").unwrap();
        let db = open_or_replace(&path, &provider, copy(&base))
            .await
            .unwrap();
        assert!(stored(&db).await.leaves.is_empty());
        assert!(path.with_extension("db.unreadable").exists());
    }

    #[tokio::test]
    async fn acme_settings_resolve_by_provider() {
        let db = db_with(store()).await;
        let provider = lets_encrypt().unwrap();
        db.mutate(|db| {
            acme_settings_mut(db).insert(
                &provider,
                &AcmeSettings {
                    contact: Vec::new(),
                },
            )
        })
        .await
        .result
        .unwrap();
        let peek = db.peek().await;
        assert!(
            <CertDatabase as DbAccessByKey<AcmeSettings>>::access_by_key(&peek, &provider)
                .is_some()
        );
    }
}
