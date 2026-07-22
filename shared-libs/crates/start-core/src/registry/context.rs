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
use ts_rs::TS;
use url::Url;

use crate::context::config::{CONFIG_PATH, ContextConfig};
use crate::context::{CliContext, RpcContext};
use crate::middleware::auth::DbContext;
use crate::middleware::auth::local::{LocalAuthContext, is_loopback, local_auth_header};
use crate::middleware::auth::signature::{NonceCache, SignatureAuthContext};
use crate::prelude::*;
use crate::registry::RegistryDatabase;
use crate::registry::device_info::{DEVICE_INFO_HEADER, DeviceInfo};
use crate::registry::migrations::run_migrations;
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
    pub datadir: PathBuf,
    pub rpc_continuations: RpcContinuations,
    pub open_authed_continuations: OpenAuthedContinuations<Option<InternedString>>,
    pub auth_sig_nonce_cache: SyncMutex<NonceCache>,
    pub client: Client,
    pub shutdown: Sender<()>,
    pub metrics_db: SyncMutex<Connection>,
}

#[derive(Clone)]
pub struct RegistryContext(Arc<RegistryContextSeed>);
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
        db.mutate(|db| run_migrations(db)).await.result?;

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
        Ok(Self(Arc::new(RegistryContextSeed {
            hostnames: config.registry_hostname.clone(),
            listen: config.registry_listen.unwrap_or(DEFAULT_REGISTRY_LISTEN),
            db,
            datadir,
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
            shutdown,
            metrics_db,
        })))
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

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
pub struct RegistryUrlParams {
    #[arg(help = "help.arg.registry-url")]
    pub registry: Url,
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

        let url = if let Some(url) = self.registry_url.clone() {
            url
        } else if local_auth.is_some() || !self.registry_hostname.is_empty() {
            let mut url: Url = format!(
                "http://{}",
                self.registry_listen.unwrap_or(DEFAULT_REGISTRY_LISTEN)
            )
            .parse()
            .map_err(Error::from)?;
            url.path_segments_mut()
                .map_err(|_| eyre!("Url cannot be base"))
                .with_kind(crate::ErrorKind::ParseUrl)?
                .push("rpc")
                .push("v0");
            url
        } else {
            return Err(Error::new(
                eyre!("{}", t!("registry.context.registry-required")),
                ErrorKind::InvalidRequest,
            )
            .into());
        };

        let mut headers = HeaderMap::new();
        if is_loopback(&url) {
            if let Some(auth) = local_auth {
                headers.insert(AUTHORIZATION, auth);
            }
        }

        method = method.strip_prefix("registry.").unwrap_or(method);
        let sig_context = self
            .registry_hostname
            .get(0)
            .cloned()
            .or_else(|| url.host().as_ref().map(InternedString::from_display));

        crate::middleware::auth::signature::call_remote(
            self,
            url,
            headers,
            sig_context.as_deref(),
            method,
            params,
        )
        .await
    }
}

impl CallRemote<RegistryContext, RegistryUrlParams> for RpcContext {
    async fn call_remote(
        &self,
        mut method: &str,
        metadata: OrdMap<&'static str, Value>,
        params: Value,
        RegistryUrlParams { mut registry }: RegistryUrlParams,
    ) -> Result<Value, RpcError> {
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

        registry
            .path_segments_mut()
            .map_err(|_| Error::new(eyre!("cannot extend URL path"), ErrorKind::ParseUrl))?
            .push("rpc")
            .push("v0");

        method = method.strip_prefix("registry.").unwrap_or(method);
        let sig_context = registry.host_str().map(InternedString::from);
        let is_onion = registry.host_str().map_or(false, |h| h.ends_with(".onion"));

        let mut res = match crate::middleware::auth::signature::call_remote(
            self,
            registry,
            headers,
            sig_context.as_deref(),
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

/// Reaching a `.onion` registry requires the host to proxy through the Tor
/// service's SOCKS proxy (`tor.startos:9050`). When that service is missing or
/// stopped, the request fails with reqwest's opaque "error sending request" —
/// replace it with actionable guidance. If Tor is up, the failure belongs to the
/// onion service itself, so the original error is preserved.
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
}

#[derive(Serialize, Deserialize, TS)]
pub struct AdminLogRecord {
    pub timestamp: String,
    pub name: String,
    #[ts(type = "{ id: string | number | null; method: string; params: any }")]
    pub request: RpcRequest,
    pub key: AnyVerifyingKey,
}

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
    type AdditionalMetadata = RegistryAuthMetadata;
    type CheckPubkeyRes = Option<(AnyVerifyingKey, SignerInfo)>;
    fn mutate_nonce_cache<F: FnOnce(&mut NonceCache) -> T, T>(&self, f: F) -> T {
        self.auth_sig_nonce_cache.mutate(f)
    }
    async fn clock_synced(&self) -> bool {
        true // Assume. Validating registry clock sync out of scope for now,
    }
    fn open_authed_continuations(&self) -> &OpenAuthedContinuations<Option<InternedString>> {
        &self.open_authed_continuations
    }
    fn remove_enrolled_keys(
        db: &mut Model<Self::Database>,
        keys: &BTreeSet<InternedString>,
    ) -> Result<(), Error> {
        for (_, signer) in db.as_index_mut().as_signers_mut().as_entries_mut()? {
            signer.as_keys_mut().mutate(|signer_keys| {
                signer_keys.retain(|k| !keys.contains(&*k.interned_pem()));
                Ok(())
            })?;
        }
        Ok(())
    }
    async fn sig_context(
        &self,
    ) -> impl IntoIterator<Item = Result<impl AsRef<str> + Send, Error>> + Send {
        self.hostnames.iter().map(Ok)
    }
    fn check_pubkey(
        &self,
        db: &Model<Self::Database>,
        pubkey: Option<&AnyVerifyingKey>,
        metadata: Self::AdditionalMetadata,
    ) -> Result<Self::CheckPubkeyRes, Error> {
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
