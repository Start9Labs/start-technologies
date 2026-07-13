use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cookie::{Cookie, Expiration, SameSite};
use cookie_store::CookieStore;
use http::HeaderMap;
use imbl::OrdMap;
use imbl_value::InternedString;
use josekit::jwk::Jwk;
use once_cell::sync::OnceCell;
use reqwest::Proxy;
use reqwest_cookie_store::CookieStoreMutex;
use rpc_toolkit::reqwest::{Client, Url};
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{CallRemote, Context, Empty};
use tokio::net::TcpStream;
use tokio::runtime::Runtime;
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};
use tracing::instrument;

use super::setup::CURRENT_SECRET;
use crate::context::config::{ClientConfig, local_config_path, resolve_target};
use crate::context::{DiagnosticContext, InitContext, RpcContext, SetupContext};
use crate::developer::{OS_DEVELOPER_KEY_PATH, default_developer_key_path, load_signing_key};
use crate::middleware::auth::local::LocalAuthContext;
use crate::net::mdns::pin_mdns_host;
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::s9pk::init::{BUILD_KEY_FILE, STARTOS_DIR};
use crate::util::io::read_file_to_string;

#[derive(Debug)]
pub struct CliContextSeed {
    pub runtime: OnceCell<Arc<Runtime>>,
    pub base_url: Url,
    pub rpc_url: Url,
    pub registry_url: Option<Url>,
    pub registry_hostname: Vec<InternedString>,
    pub registry_listen: Option<SocketAddr>,
    pub s9pk_s3base: Option<Url>,
    pub s9pk_s3bucket: Option<InternedString>,
    pub tunnel_addr: Option<SocketAddr>,
    pub tunnel_listen: Option<SocketAddr>,
    pub client: Client,
    pub cookie_store: Arc<CookieStoreMutex>,
    pub cookie_path: PathBuf,
    pub developer_key_path: PathBuf,
    pub developer_key: OnceCell<ed25519_dalek::SigningKey>,
    pub root_ca: Vec<PathBuf>,
    pub insecure: bool,
}
impl Drop for CliContextSeed {
    fn drop(&mut self) {
        if let Some(rt) = self.runtime.take() {
            if let Ok(rt) = Arc::try_unwrap(rt) {
                rt.shutdown_background();
            }
        }
        // A cookie-cache write failure must not crash an otherwise-successful command.
        self.save_cookie_store().log_err();
    }
}
impl CliContextSeed {
    /// Lock a stable sidecar file (not the temp, which each writer recreates) so
    /// concurrent `start-cli` invocations sharing `$HOME` serialize their saves.
    fn save_cookie_store(&self) -> Result<(), Error> {
        let parent_dir = self.cookie_path.parent().unwrap_or(Path::new("/"));
        if !parent_dir.exists() {
            std::fs::create_dir_all(parent_dir)
                .with_ctx(|_| (ErrorKind::Filesystem, parent_dir.display()))?;
        }
        let lock_path = format!("{}.lock", self.cookie_path.display());
        let _lock = fd_lock_rs::FdLock::lock(
            File::create(&lock_path).with_ctx(|_| (ErrorKind::Filesystem, &lock_path))?,
            fd_lock_rs::LockType::Exclusive,
            true,
        )
        .with_ctx(|_| (ErrorKind::Filesystem, &lock_path))?;

        // Merge onto disk rather than clobber a concurrent writer's other-domain cookies; ours wins conflicts.
        let on_disk = if self.cookie_path.exists() {
            cookie_store::serde::json::load(BufReader::new(
                File::open(&self.cookie_path)
                    .with_ctx(|_| (ErrorKind::Filesystem, self.cookie_path.display()))?,
            ))
            .unwrap_or_default()
        } else {
            CookieStore::default()
        };
        let merged = {
            let store = self.cookie_store.lock().map_err(|_| {
                Error::new(eyre!("cookie store mutex poisoned"), ErrorKind::Unknown)
            })?;
            CookieStore::from_cookies(
                on_disk
                    .iter_unexpired()
                    .chain(store.iter_unexpired())
                    .cloned()
                    .map(Ok::<_, std::convert::Infallible>),
                false,
            )
            .expect("from_cookies is infallible")
        };

        let tmp = format!("{}.tmp", self.cookie_path.display());
        let mut writer = File::create(&tmp).with_ctx(|_| (ErrorKind::Filesystem, &tmp))?;
        cookie_store::serde::json::save(&merged, &mut writer)
            .map_err(|e| Error::new(eyre!("{e}"), ErrorKind::Filesystem))?;
        writer
            .sync_all()
            .with_ctx(|_| (ErrorKind::Filesystem, &tmp))?;
        std::fs::rename(&tmp, &self.cookie_path)
            .with_ctx(|_| (ErrorKind::Filesystem, self.cookie_path.display()))?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct CliContext(Arc<CliContextSeed>);
impl CliContext {
    /// BLOCKING
    #[instrument(skip_all)]
    pub fn init(config: ClientConfig) -> Result<Self, Error> {
        // Follow each namespace's `default` profile to a URL (`load` already seeded
        // -H/-r as `default` and layered every config file in), then localhost / no
        // registry when unset.
        let mut url = match resolve_target(config.host.as_ref())? {
            Some(url) => url,
            None => "http://localhost".parse()?,
        };
        // Before anything derives a URL from this one: a `.local` host is unresolvable from a
        // musl-static binary, so pin it to an address the system resolver found.
        pin_mdns_host(&mut url)?;

        let registry = resolve_target(config.registry.as_ref())?;

        let cookie_path = config.cookie_path.unwrap_or_else(|| {
            local_config_path()
                .as_deref()
                .unwrap_or_else(|| Path::new(super::config::CONFIG_PATH))
                .parent()
                .unwrap_or(Path::new("/"))
                .join(".cookies.json")
        });
        let cookie_store = Arc::new(CookieStoreMutex::new(if cookie_path.exists() {
            cookie_store::serde::json::load(BufReader::new(
                File::open(&cookie_path)
                    .with_ctx(|_| (ErrorKind::Filesystem, cookie_path.display()))?,
            ))
            .unwrap_or_default()
        } else {
            CookieStore::default()
        }));

        Ok(CliContext(Arc::new(CliContextSeed {
            runtime: OnceCell::new(),
            base_url: url.clone(),
            rpc_url: {
                url.path_segments_mut()
                    .map_err(|_| eyre!("Url cannot be base"))
                    .with_kind(crate::ErrorKind::ParseUrl)?
                    .push("rpc")
                    .push("v1");
                url
            },
            registry_url: registry
                .map(|mut registry| {
                    pin_mdns_host(&mut registry)?;
                    registry
                        .path_segments_mut()
                        .map_err(|_| eyre!("Url cannot be base"))
                        .with_kind(crate::ErrorKind::ParseUrl)?
                        .push("rpc")
                        .push("v0");
                    Ok::<_, Error>(registry)
                })
                .transpose()?,
            registry_hostname: config.registry_hostname.unwrap_or_default(),
            registry_listen: config.registry_listen,
            s9pk_s3base: config.s9pk_s3base,
            s9pk_s3bucket: config.s9pk_s3bucket,
            tunnel_addr: config.tunnel,
            tunnel_listen: config.tunnel_listen,
            client: {
                let mut builder = Client::builder().cookie_provider(cookie_store.clone());
                if let Some(proxy) = config.proxy.or_else(|| {
                    config
                        .socks_listen
                        .and_then(|socks| format!("socks5h://{socks}").parse::<Url>().log_err())
                }) {
                    builder =
                        builder.proxy(Proxy::all(proxy).with_kind(crate::ErrorKind::ParseUrl)?)
                }
                if config.insecure {
                    builder = builder.danger_accept_invalid_certs(true);
                }
                for ca_path in config.root_ca.iter().flatten() {
                    let pem = std::fs::read(ca_path)
                        .with_ctx(|_| (crate::ErrorKind::Filesystem, ca_path.display()))?;
                    let cert = reqwest::Certificate::from_pem(&pem)
                        .with_kind(crate::ErrorKind::OpenSsl)?;
                    builder = builder.add_root_certificate(cert);
                }
                builder.build().expect("cannot fail")
            },
            cookie_store,
            cookie_path,
            developer_key_path: config
                .developer_key_path
                .unwrap_or_else(default_developer_key_path),
            developer_key: OnceCell::new(),
            root_ca: config.root_ca.unwrap_or_default(),
            insecure: config.insecure,
        })))
    }

    fn ws_tls_connector(&self) -> Result<Option<Connector>, Error> {
        if self.root_ca.is_empty() && !self.insecure {
            return Ok(None);
        }
        let mut builder = native_tls::TlsConnector::builder();
        if self.insecure {
            builder.danger_accept_invalid_certs(true);
            builder.danger_accept_invalid_hostnames(true);
        }
        for ca_path in &self.root_ca {
            let pem = std::fs::read(ca_path)
                .with_ctx(|_| (crate::ErrorKind::Filesystem, ca_path.display()))?;
            let cert =
                native_tls::Certificate::from_pem(&pem).with_kind(crate::ErrorKind::OpenSsl)?;
            builder.add_root_certificate(cert);
        }
        let connector = builder.build().with_kind(crate::ErrorKind::OpenSsl)?;
        Ok(Some(Connector::NativeTls(connector)))
    }

    /// BLOCKING
    #[instrument(skip_all)]
    pub fn developer_key(&self) -> Result<&ed25519_dalek::SigningKey, Error> {
        self.developer_key.get_or_try_init(|| {
            for path in [Path::new(OS_DEVELOPER_KEY_PATH), &self.developer_key_path] {
                if !path.exists() {
                    continue;
                }
                let pair =
                    <ed25519::KeypairBytes as ed25519::pkcs8::DecodePrivateKey>::from_pkcs8_pem(
                        &std::fs::read_to_string(path)?,
                    )
                    .with_kind(crate::ErrorKind::Pem)?;
                let secret =
                    ed25519_dalek::SecretKey::try_from(&pair.secret_key[..]).map_err(|_| {
                        Error::new(
                            eyre!("{}", t!("context.cli.pkcs8-key-incorrect-length")),
                            ErrorKind::OpenSsl,
                        )
                    })?;
                return Ok(secret.into());
            }
            Err(Error::new(
                eyre!("{}", t!("context.cli.developer-key-does-not-exist")),
                crate::ErrorKind::Uninitialized,
            ))
        })
    }

    /// The workspace's s9pk signing key. Walks up from cwd for `.startos/build-key`
    /// (created by `s9pk init-workspace`) and errors if there's no workspace, since
    /// s9pk signing is workspace-scoped. Distinct from [`Self::developer_key`], which
    /// stays the global identity for registry/server auth.
    pub fn build_key(&self) -> Result<ed25519_dalek::SigningKey, Error> {
        let mut dir = std::env::current_dir().with_kind(ErrorKind::Filesystem)?;
        loop {
            let candidate = dir.join(STARTOS_DIR).join(BUILD_KEY_FILE);
            // EACCES on an inaccessible ancestor (or any other IO error) is treated
            // as "no accessible workspace here" — stop walking rather than either
            // silently stepping past it (`exists()`) or surfacing the error
            // (`try_exists()?`).
            match candidate.try_exists() {
                Ok(true) => return load_signing_key(candidate),
                Ok(false) => {}
                Err(_) => break,
            }
            if !dir.pop() {
                break;
            }
        }
        Err(crate::s9pk::init::no_workspace_error())
    }

    pub async fn ws_continuation(
        &self,
        guid: Guid,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, Error> {
        let mut url = self.base_url.clone();
        let ws_scheme = match url.scheme() {
            "https" => "wss",
            "http" => "ws",
            _ => {
                return Err(Error::new(
                    eyre!("{}", t!("context.cli.cannot-parse-scheme-from-base-url")),
                    crate::ErrorKind::ParseUrl,
                )
                .into());
            }
        };
        url.set_scheme(ws_scheme).map_err(|_| {
            Error::new(
                eyre!("{}", t!("context.cli.cannot-set-url-scheme")),
                crate::ErrorKind::ParseUrl,
            )
        })?;
        url.path_segments_mut()
            .map_err(|_| eyre!("Url cannot be base"))
            .with_kind(crate::ErrorKind::ParseUrl)?
            .push("ws")
            .push("rpc")
            .push(guid.as_ref());
        let connector = self.ws_tls_connector()?;
        let (stream, _) =
            tokio_tungstenite::connect_async_tls_with_config(url, None, false, connector)
                .await
                .with_kind(ErrorKind::Network)?;
        Ok(stream)
    }

    pub async fn rest_continuation(
        &self,
        guid: Guid,
        body: reqwest::Body,
        headers: reqwest::header::HeaderMap,
    ) -> Result<reqwest::Response, Error> {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .map_err(|_| eyre!("Url cannot be base"))
            .with_kind(crate::ErrorKind::ParseUrl)?
            .push("rest")
            .push("rpc")
            .push(guid.as_ref());
        self.client
            .post(url)
            .headers(headers)
            .body(body)
            .send()
            .await
            .with_kind(ErrorKind::Network)
    }

    pub async fn call_remote<RemoteContext>(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, Error>
    where
        Self: CallRemote<RemoteContext>,
    {
        <Self as CallRemote<RemoteContext, Empty>>::call_remote(
            &self,
            method,
            OrdMap::new(),
            params,
            Empty {},
        )
        .await
        .map_err(Error::from)
        .with_ctx(|e| (e.kind, method))
    }
    pub async fn call_remote_with<RemoteContext, T>(
        &self,
        method: &str,
        params: Value,
        extra: T,
    ) -> Result<Value, Error>
    where
        Self: CallRemote<RemoteContext, T>,
    {
        <Self as CallRemote<RemoteContext, T>>::call_remote(
            &self,
            method,
            OrdMap::new(),
            params,
            extra,
        )
        .await
        .map_err(Error::from)
        .with_ctx(|e| (e.kind, method))
    }
}
impl AsRef<Jwk> for CliContext {
    fn as_ref(&self) -> &Jwk {
        &*CURRENT_SECRET
    }
}
impl std::ops::Deref for CliContext {
    type Target = CliContextSeed;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}
impl Context for CliContext {
    fn runtime(&self) -> Option<Arc<Runtime>> {
        Some(
            self.runtime
                .get_or_init(|| {
                    Arc::new(
                        tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .unwrap(),
                    )
                })
                .clone(),
        )
    }
}
impl AsRef<Client> for CliContext {
    fn as_ref(&self) -> &Client {
        &self.client
    }
}

impl CallRemote<RpcContext> for CliContext {
    async fn call_remote(
        &self,
        method: &str,
        _: OrdMap<&'static str, Value>,
        params: Value,
        _: Empty,
    ) -> Result<Value, RpcError> {
        if let Ok(local) = read_file_to_string(RpcContext::LOCAL_AUTH_COOKIE_PATH).await {
            self.cookie_store
                .lock()
                .unwrap()
                .insert_raw(
                    &Cookie::build(("local", local))
                        .domain("localhost")
                        .expires(Expiration::Session)
                        .same_site(SameSite::Strict)
                        .build(),
                    &"http://localhost".parse()?,
                )
                .with_kind(crate::ErrorKind::Network)?;
        }
        crate::middleware::auth::signature::call_remote(
            self,
            self.rpc_url.clone(),
            HeaderMap::new(),
            self.rpc_url.host_str(),
            method,
            params,
        )
        .await
    }
}
impl CallRemote<DiagnosticContext> for CliContext {
    async fn call_remote(
        &self,
        method: &str,
        _: OrdMap<&'static str, Value>,
        params: Value,
        _: Empty,
    ) -> Result<Value, RpcError> {
        crate::middleware::auth::signature::call_remote(
            self,
            self.rpc_url.clone(),
            HeaderMap::new(),
            self.rpc_url.host_str(),
            method,
            params,
        )
        .await
    }
}
impl CallRemote<InitContext> for CliContext {
    async fn call_remote(
        &self,
        method: &str,
        _: OrdMap<&'static str, Value>,
        params: Value,
        _: Empty,
    ) -> Result<Value, RpcError> {
        crate::middleware::auth::signature::call_remote(
            self,
            self.rpc_url.clone(),
            HeaderMap::new(),
            self.rpc_url.host_str(),
            method,
            params,
        )
        .await
    }
}
impl CallRemote<SetupContext> for CliContext {
    async fn call_remote(
        &self,
        method: &str,
        _: OrdMap<&'static str, Value>,
        params: Value,
        _: Empty,
    ) -> Result<Value, RpcError> {
        crate::middleware::auth::signature::call_remote(
            self,
            self.rpc_url.clone(),
            HeaderMap::new(),
            self.rpc_url.host_str(),
            method,
            params,
        )
        .await
    }
}
