use std::collections::BTreeSet;
use std::net::{Ipv4Addr, SocketAddr};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use clap::Parser;
use http::HeaderMap;
use http::header::AUTHORIZATION;
use imbl::OrdMap;
use imbl_value::InternedString;
use patch_db::PatchDb;
use patch_db::json_ptr::ROOT;
use reqwest::{Client, Proxy};
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{CallRemote, Context, Empty, RpcRequest};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Sender;
use tracing::instrument;
use url::Url;

use crate::context::config::{CONFIG_PATH, ContextConfig};
use crate::context::{CliContext, RpcContext};
use crate::middleware::auth::DbContext;
use crate::middleware::auth::local::{LocalAuthContext, dial_addr, is_loopback, local_auth_header};
use crate::middleware::auth::signature::{
    NonceCache, SignatureAuthContext, SignatureAuthMetadata, SignatureAuthorizationContext,
    SigningContext,
};
use crate::prelude::*;
use crate::registry::RegistryDatabase;
use crate::registry::device_info::{DEVICE_INFO_HEADER, DeviceInfo};
use crate::registry::migrations::run_migrations;
use crate::registry::mirror::IndexMirrors;
use crate::registry::package::icon::{icons_dir, referenced_icons, sweep_icons};
use crate::registry::package::promote::registry_rpc_url;
use crate::registry::signer::SignerInfo;
use crate::rpc_continuations::{OpenAuthedContinuations, RpcContinuations};
use crate::sign::AnyVerifyingKey;
use crate::util::io::append_file;
use crate::util::sync::SyncMutex;

const DEFAULT_REGISTRY_LISTEN: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::LOCALHOST), 5959);

#[derive(Debug, Clone, Default, Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "kebab-case")]
#[command(rename_all = "kebab-case")]
pub struct RegistryConfig {
    #[arg(short = 'c', long = "config", help = "help.arg.config-file-path")]
    pub config: Option<PathBuf>,
    #[arg(
        short = 'l',
        long = "listen",
        help = "help.arg.registry-listen-address"
    )]
    pub registry_listen: Option<SocketAddr>,
    #[arg(short = 'H', long = "hostname", help = "help.arg.registry-hostname")]
    pub registry_hostname: Vec<InternedString>,
    #[arg(short = 'p', long = "tor-proxy", help = "help.arg.tor-proxy-url")]
    pub tor_proxy: Option<Url>,
    #[arg(short = 'd', long = "datadir", help = "help.arg.data-directory")]
    pub datadir: Option<PathBuf>,
    #[arg(long, help = "help.arg.registry-startwrt")]
    #[serde(default)]
    pub startwrt: bool,
}
impl ContextConfig for RegistryConfig {
    fn next(&mut self) -> Option<PathBuf> {
        self.config.take()
    }
    fn merge_with(&mut self, mut other: Self) {
        self.registry_listen = self.registry_listen.take().or(other.registry_listen);
        self.registry_hostname.append(&mut other.registry_hostname);
        self.tor_proxy = self.tor_proxy.take().or(other.tor_proxy);
        self.datadir = self.datadir.take().or(other.datadir);
        self.startwrt |= other.startwrt;
    }
}

impl RegistryConfig {
    pub fn load(mut self) -> Result<Self, Error> {
        let path = self.next();
        self.load_path_rec(path)?;
        self.load_path_rec(Some(CONFIG_PATH))?;
        Ok(self)
    }
}

pub struct RegistryContextSeed {
    pub hostnames: Vec<InternedString>,
    pub listen: SocketAddr,
    pub db: TypedPatchDb<RegistryDatabase>,
    pub index_mirrors: IndexMirrors,
    pub datadir: PathBuf,
    pub startwrt: bool,
    pub rpc_continuations: RpcContinuations,
    pub open_authed_continuations: OpenAuthedContinuations<Option<InternedString>>,
    pub auth_sig_nonce_cache: SyncMutex<NonceCache>,
    pub client: Client,
    pub server_identity: crate::sign::Signer,
    pub shutdown: Sender<()>,
    pub metrics_db: SyncMutex<Connection>,
}

#[derive(Clone)]
pub struct RegistryContext(
    Arc<RegistryContextSeed>,
    Option<Arc<crate::middleware::auth::session::SecureSession>>,
);

impl SessionContext for RegistryContext {
    fn secure_session(&self) -> Option<&crate::middleware::auth::session::SecureSession> {
        self.1.as_deref()
    }
}
impl RegistryContext {
    pub(crate) fn replay_check(&self) -> crate::middleware::auth::signature::RequestReplayCheck {
        crate::middleware::auth::signature::RequestReplayCheck::new(&self.0, |seed| {
            Some(Self(seed, None))
        })
    }
    pub(crate) fn server_identity(&self) -> Result<crate::sign::Signer, Error> {
        Ok(self.server_identity.clone())
    }
    pub(crate) fn with_secure_session(
        mut self,
        session: crate::middleware::auth::session::SecureSession,
    ) -> Self {
        self.1 = Some(Arc::new(session));
        self
    }
}
impl RegistryContext {
    #[instrument(skip_all)]
    pub async fn init(config: &RegistryConfig) -> Result<Self, Error> {
        let (shutdown, _) = tokio::sync::broadcast::channel(1);
        let datadir = config
            .datadir
            .as_deref()
            .unwrap_or_else(|| Path::new("/var/lib/startos"))
            .to_owned();
        if tokio::fs::metadata(&datadir).await.is_err() {
            tokio::fs::create_dir_all(&datadir).await?;
        }
        let db_path = datadir.join("registry.db");
        let db = TypedPatchDb::<RegistryDatabase>::load_unchecked(PatchDb::open(&db_path).await?);
        if db.dump(&ROOT).await.value.is_null() {
            db.put(&ROOT, &RegistryDatabase::init()).await?;
        }
        db.mutate(|db| run_migrations(db, &datadir)).await.result?;
        let server_identity = crate::sign::instance::load(&db).await?;
        sweep_icons(
            &icons_dir(&datadir),
            &referenced_icons(db.peek().await.as_index().as_package())?,
        )
        .await?;

        Self::init_auth_cookie().await?;

        let tor_proxy_url = config
            .tor_proxy
            .clone()
            .map(Ok)
            .unwrap_or_else(|| "socks5h://tor.startos:9050".parse())?;
        let metrics_db_path = datadir.join("metrics.db");
        let metrics_db = Connection::open(&metrics_db_path).with_kind(ErrorKind::Database)?;
        metrics_db
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS user_activity (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    created_at TEXT NOT NULL,
                    server_id TEXT NOT NULL,
                    arch TEXT,
                    os_version TEXT
                );
                CREATE TABLE IF NOT EXISTS package_request (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    created_at TEXT NOT NULL,
                    pkg_id TEXT NOT NULL,
                    version TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_user_activity_created_at ON user_activity(created_at);
                CREATE INDEX IF NOT EXISTS idx_package_request_created_at ON package_request(created_at);
                CREATE INDEX IF NOT EXISTS idx_package_request_pkg_id ON package_request(pkg_id);",
            )
            .with_kind(ErrorKind::Database)?;
        let metrics_db = SyncMutex::new(metrics_db);
        if config.registry_hostname.is_empty() {
            return Err(Error::new(
                eyre!("{}", t!("registry.context.missing-hostname")),
                ErrorKind::NotFound,
            ));
        }
        Ok(Self(
            Arc::new(RegistryContextSeed {
                hostnames: config.registry_hostname.clone(),
                listen: config.registry_listen.unwrap_or(DEFAULT_REGISTRY_LISTEN),
                db,
                index_mirrors: IndexMirrors::default(),
                datadir,
                startwrt: config.startwrt,
                rpc_continuations: RpcContinuations::new(),
                open_authed_continuations: OpenAuthedContinuations::new(),
                auth_sig_nonce_cache: SyncMutex::new(Default::default()),
                client: Client::builder()
                    .proxy(Proxy::custom(move |url| {
                        if url.host_str().map_or(false, |h| h.ends_with(".onion")) {
                            Some(tor_proxy_url.clone())
                        } else {
                            None
                        }
                    }))
                    .build()
                    .with_kind(crate::ErrorKind::ParseUrl)?,
                server_identity,
                shutdown,
                metrics_db,
            }),
            None,
        ))
    }
}
impl AsRef<RpcContinuations> for RegistryContext {
    fn as_ref(&self) -> &RpcContinuations {
        &self.rpc_continuations
    }
}

impl Context for RegistryContext {}
impl Deref for RegistryContext {
    type Target = RegistryContextSeed;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

#[derive(Debug, Deserialize, Serialize, VisitFields, Parser)]
#[group(skip)]
pub struct RegistryUrlParams {
    #[arg(help = "help.arg.registry-url")]
    pub registry: Url,
}

rpc_toolkit::reflect_ts!(RegistryUrlParams);

pub(crate) async fn call_cli(
    ctx: &CliContext,
    url: Url,
    headers: HeaderMap,
    address: &str,
    method: &str,
    mut params: Value,
) -> Result<Value, RpcError> {
    use crate::middleware::auth::session::client;
    super::compat::pem_wire(&mut params);
    if let Some(signer) = ctx.rpc_signer(&headers)? {
        let request = ctx.rpc_client.capture(method, params)?;
        let connection = ctx
            .rpc_client
            .session(
                &signer,
                client::websocket_url(url.clone())?,
                address,
                headers,
            )
            .await?;
        for warning in super::cli_identity::observe(
            &ctx.id_key_path.with_file_name("registry-identities"),
            &url,
            address,
            &connection.peer,
        )
        .await
        {
            rpc_toolkit::output::advisory(format_args!("{}", warning));
        }
        connection.request(request).await
    } else {
        crate::middleware::auth::signature::call_http(
            ctx.client.clone(),
            url,
            headers,
            method,
            params,
        )
        .await
    }
}

impl CallRemote<RegistryContext> for CliContext {
    async fn call_remote(
        &self,
        mut method: &str,
        _: OrdMap<&'static str, Value>,
        params: Value,
        _: Empty,
    ) -> Result<Value, RpcError> {
        let local_auth = local_auth_header::<RegistryContext>().await;

        let (url, is_local) = if let Some(url) = &self.registry_url {
            let url = url.get()?.clone();
            let is_local = is_loopback(&url);
            (url, is_local)
        } else if local_auth.is_some() || !self.registry_hostname.is_empty() {
            let url: Url = format!(
                "http://{}",
                dial_addr(self.registry_listen.unwrap_or(DEFAULT_REGISTRY_LISTEN))
            )
            .parse()
            .map_err(Error::from)?;
            // The listen address identifies this machine, including non-loopback binds.
            (registry_rpc_url(&url)?, true)
        } else {
            return Err(Error::new(
                eyre!("{}", t!("registry.context.registry-required")),
                ErrorKind::InvalidRequest,
            )
            .into());
        };

        let mut headers = HeaderMap::new();
        if is_local {
            if let Some(auth) = local_auth {
                headers.insert(AUTHORIZATION, auth);
            }
        }

        method = method.strip_prefix("registry.").unwrap_or(method);
        let sig_context = self
            .registry_hostname
            .get(0)
            .cloned()
            .or_else(|| {
                self.registry_url
                    .as_ref()
                    .and_then(|url| url.selected_host())
                    .map(InternedString::intern)
            })
            .or_else(|| url.host_str().map(InternedString::intern));

        call_cli(
            self,
            url,
            headers,
            sig_context.as_deref().or_not_found("registry hostname")?,
            method,
            params,
        )
        .await
    }
}

/// Encrypted registry calls have no default reply deadline.
pub(crate) async fn call(
    client: &crate::context::rpc::ReloadableHttpClient,
    signer: &crate::sign::Signer,
    registry: &Url,
    headers: HeaderMap,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    use crate::middleware::auth::session::client as session_client;
    match super::sync::connection::connect(client, registry, signer, headers.clone()).await {
        Ok(mut socket) => {
            session_client::request(&mut socket, session_client::request_body(method, params)?)
                .await
        }
        Err(error) if super::sync::connection::websocket_unsupported(&error) => {
            call_legacy(client, signer, registry, headers, method, params).await
        }
        Err(error) => Err(error.into()),
    }
}

pub(super) async fn call_legacy(
    client: &crate::context::rpc::ReloadableHttpClient,
    signer: &crate::sign::Signer,
    registry: &Url,
    mut headers: HeaderMap,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    use crate::middleware::auth::signature::{
        AUTH_SIG_HEADER, SignatureHeader, rpc_request_body, send_http,
    };
    let body = rpc_request_body(method, params)?;
    let address = registry.host_str().or_not_found("registry hostname")?;
    headers.insert(
        AUTH_SIG_HEADER,
        SignatureHeader::sign(signer, body.as_bytes(), address)
            .await?
            .to_header(),
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        send_http(
            client.get_without_redirects(),
            registry_rpc_url(registry)?,
            headers,
            body,
            crate::util::net::secure::DEFAULT_RPC_RESPONSE_LIMIT,
        ),
    )
    .await
    .with_kind(ErrorKind::Network)?
}

impl CallRemote<RegistryContext, RegistryUrlParams> for RpcContext {
    async fn call_remote(
        &self,
        mut method: &str,
        metadata: OrdMap<&'static str, Value>,
        mut params: Value,
        RegistryUrlParams { registry }: RegistryUrlParams,
    ) -> Result<Value, RpcError> {
        super::compat::pem_wire(&mut params);
        let mut headers = HeaderMap::new();
        let mut device_info = None;
        if metadata
            .get("get_device_info")
            .and_then(|m| m.as_bool())
            .unwrap_or(false)
        {
            let di = DeviceInfo::load(self).await?;
            headers.insert(DEVICE_INFO_HEADER, di.to_header_value());
            device_info = Some(di);
        }

        method = method.strip_prefix("registry.").unwrap_or(method);
        if let Some(result) =
            super::sync::cached_call(self, &registry, method, &params, device_info.as_ref()).await?
        {
            return Ok(result);
        }

        let is_onion = registry.host_str().map_or(false, |h| h.ends_with(".onion"));
        let mut res = match call(
            &self.client,
            &self.signing_key()?,
            &registry,
            headers,
            method,
            params.clone(),
        )
        .await
        {
            Ok(res) => res,
            Err(e) if is_onion => return Err(onion_tor_hint(self, e).await),
            Err(e) => return Err(e),
        };

        if let Some(device_info) = device_info {
            device_info.filter_for_hardware(method, params, &mut res)?;
        }

        Ok(res)
    }
}

/// Onion registries require the Tor service's SOCKS proxy.
async fn onion_tor_hint(ctx: &RpcContext, original: RpcError) -> RpcError {
    let running = async {
        let tor_id = "tor".parse::<crate::PackageId>().ok()?;
        let peek = ctx.db.peek().await;
        let entry = peek.as_public().as_package_data().as_idx(&tor_id)?;
        Some(entry.as_status_info().as_started().de().ok()?.is_some())
    }
    .await;
    let msg = match running {
        Some(true) => return original,
        Some(false) => t!("registry.context.onion-tor-not-running"),
        None => t!("registry.context.onion-tor-not-installed"),
    };
    Error::new(eyre!("{msg}"), ErrorKind::Network).into()
}

#[derive(Deserialize)]
pub struct RegistryAuthMetadata {
    #[serde(default)]
    admin: bool,
    #[serde(default = "crate::util::serde::const_true")]
    authenticated: bool,
}

impl SignatureAuthMetadata for RegistryAuthMetadata {}

#[derive(Serialize, Deserialize, VisitFields)]
pub struct AdminLogRecord {
    pub timestamp: String,
    pub name: String,
    #[visit(ts(
        type = "z.object({ id: z.union([z.string(), z.number()]).nullable(), method: z.string(), params: z.any() })"
    ))]
    pub request: RpcRequest,
    pub key: AnyVerifyingKey,
}

rpc_toolkit::reflect_ts!(AdminLogRecord);

impl DbContext for RegistryContext {
    type Database = RegistryDatabase;
    fn db(&self) -> &TypedPatchDb<Self::Database> {
        &self.db
    }
}
impl LocalAuthContext for RegistryContext {
    const LOCAL_AUTH_COOKIE_PATH: &str = "/run/startos/registry.authcookie";
    const LOCAL_AUTH_COOKIE_OWNERSHIP: &str = "root:root";
}
impl SignatureAuthContext for RegistryContext {
    fn server_identity(&self) -> Result<crate::sign::Signer, Error> {
        RegistryContext::server_identity(self)
    }
    fn replay_check(&self) -> crate::middleware::auth::signature::RequestReplayCheck {
        RegistryContext::replay_check(self)
    }
    fn with_secure_session(self, session: crate::middleware::auth::session::SecureSession) -> Self {
        RegistryContext::with_secure_session(self, session)
    }
    fn mutate_nonce_cache<F: FnOnce(&mut NonceCache) -> T, T>(&self, f: F) -> T {
        self.auth_sig_nonce_cache.mutate(f)
    }
    /// Trusts the registry host clock.
    async fn clock_synced(&self) -> bool {
        true
    }
    async fn sig_context(
        &self,
        _extensions: &http::Extensions,
    ) -> impl IntoIterator<Item = Result<impl AsRef<str> + Send, Error>> + Send {
        self.hostnames.iter().map(Ok)
    }
}
impl SignatureAuthorizationContext for RegistryContext {
    type AdditionalMetadata = RegistryAuthMetadata;
    type CheckPubkeyRes = Option<(AnyVerifyingKey, SignerInfo)>;
    fn open_authed_continuations(&self) -> &OpenAuthedContinuations<Option<InternedString>> {
        &self.open_authed_continuations
    }
    fn remove_enrolled_keys(
        db: &mut Model<Self::Database>,
        keys: &BTreeSet<InternedString>,
    ) -> Result<(), Error> {
        for (_, signer) in db.as_index_mut().as_signers_mut().as_entries_mut()? {
            signer.as_keys_mut().mutate(|signer_keys| {
                signer_keys.retain(|k| !keys.contains(&*k.interned_der64()));
                Ok(())
            })?;
        }
        Ok(())
    }
    fn check_pubkey(
        &self,
        db: &Model<Self::Database>,
        pubkey: Option<&AnyVerifyingKey>,
        metadata: Self::AdditionalMetadata,
    ) -> Result<Self::CheckPubkeyRes, Error> {
        if !metadata.authenticated {
            return Ok(None);
        }
        if let Some(pubkey) = pubkey {
            let (guid, admin) = db.as_index().as_signers().get_signer_info(pubkey)?;
            if !metadata.admin || db.as_admins().de()?.contains(&guid) {
                return Ok(Some((pubkey.clone(), admin)));
            }
        }

        Err(Error::new(
            eyre!("{}", t!("registry.context.unauthorized")),
            ErrorKind::Authorization,
        ))
    }
    async fn post_auth_hook(
        &self,
        check_pubkey_res: Self::CheckPubkeyRes,
        request: &RpcRequest,
    ) -> Result<(), Error> {
        use tokio::io::AsyncWriteExt;
        if let Some((pubkey, admin)) = check_pubkey_res {
            let mut log = append_file(self.datadir.join("admin.log")).await?;
            log.write_all(
                (serde_json::to_string(&AdminLogRecord {
                    timestamp: Utc::now().to_rfc3339(),
                    name: admin.name,
                    request: request.clone(),
                    key: pubkey,
                })
                .with_kind(ErrorKind::Serialization)?
                    + "\n")
                    .as_bytes(),
            )
            .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_security_notification_config_does_not_gate_registry_subscriptions() {
        let config: RegistryConfig = serde_json::from_value(serde_json::json!({
            "registry-hostname": ["registry.test"],
            "security-notifications": false,
        }))
        .unwrap();
        assert_eq!(
            config.registry_hostname,
            vec![InternedString::from("registry.test")]
        );
        assert!(RegistryConfig::try_parse_from(["registry", "--security-notifications"]).is_err());
    }
}

#[cfg(test)]
pub(crate) async fn test_context(datadir: PathBuf, uploader: AnyVerifyingKey) -> RegistryContext {
    test_context_for_hosts(
        datadir,
        uploader,
        vec!["localhost".into(), "127.0.0.1".into()],
    )
    .await
}

#[cfg(test)]
pub(crate) async fn test_context_for_hosts(
    datadir: PathBuf,
    uploader: AnyVerifyingKey,
    hostnames: Vec<InternedString>,
) -> RegistryContext {
    use crate::rpc_continuations::Guid;

    tokio::fs::create_dir_all(&datadir).await.unwrap();
    let db =
        TypedPatchDb::load_unchecked(PatchDb::open(datadir.join("registry.db")).await.unwrap());
    let mut database = RegistryDatabase::init();
    let guid = Guid::new();
    database.admins.insert(guid.clone());
    database.index.trust_signers.insert(guid.clone());
    database.index.signers.insert(
        guid,
        SignerInfo {
            name: "Uploader".into(),
            contact: vec![],
            keys: [uploader].into(),
            gpg_exports: Default::default(),
        },
    );
    db.put(&ROOT, &database).await.unwrap();
    RegistryContext(
        Arc::new(RegistryContextSeed {
            hostnames,
            listen: "127.0.0.1:0".parse().unwrap(),
            db,
            index_mirrors: Default::default(),
            datadir,
            startwrt: false,
            rpc_continuations: RpcContinuations::new(),
            open_authed_continuations: OpenAuthedContinuations::new(),
            auth_sig_nonce_cache: SyncMutex::new(Default::default()),
            client: Client::new(),
            server_identity: crate::sign::AnySigningKey::generate().into(),
            shutdown: tokio::sync::broadcast::channel(1).0,
            metrics_db: SyncMutex::new(Connection::open_in_memory().unwrap()),
        }),
        None,
    )
}

#[cfg(test)]
#[path = "package/ingestion_tests.rs"]
mod ingestion_tests;
#[cfg(test)]
#[path = "redirect_tests.rs"]
mod redirect_tests;

#[cfg(test)]
#[path = "os/asset/sign_tests.rs"]
mod os_asset_sign_tests;
#[cfg(test)]
#[path = "os/ingestion_tests.rs"]
mod os_ingestion_tests;
#[cfg(test)]
#[path = "test_utils.rs"]
pub(crate) mod test_utils;

#[cfg(test)]
#[path = "query_tests.rs"]
mod query_tests;
