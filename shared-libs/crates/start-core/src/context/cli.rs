use std::io::IsTerminal;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use http::HeaderMap;
use http::header::AUTHORIZATION;
use imbl::OrdMap;
use imbl_value::InternedString;
use josekit::jwk::Jwk;
use once_cell::sync::OnceCell;
use reqwest::Proxy;
use rpc_toolkit::reqwest::{Client, Url};
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{CallRemote, Context, Empty};
use tokio::runtime::Runtime;
use tracing::instrument;

use super::setup::CURRENT_SECRET;
use crate::context::config::{ClientConfig, Profiles, resolve_target};
use crate::context::{DiagnosticContext, InitContext, RpcContext, SetupContext};
use crate::developer::{
    OS_ID_KEY_PATH, default_id_key_path, load_signing_key, migrate_legacy_key_file,
};
use crate::middleware::auth::local::local_auth_header_for_url;
use crate::middleware::auth::signature::SigningContext;
use crate::net::mdns::pin_mdns_host;
use crate::prelude::*;
use crate::registry::package::promote::registry_rpc_url;
use crate::rpc_continuations::Guid;
use crate::s9pk::init::{
    BUILD_KEY_FILE, STARTOS_DIR, complete_signing_workspace, find_signing_workspace,
};
use crate::sign::gpg_agent::GpgAgentKey;
use crate::sign::{AnySigningKey, Signer};

mod continuation;
#[cfg(test)]
mod progress_tests;
pub use continuation::CliWebSocketContext;

/// A configured target whose `.local` host is resolved on first use, so an
/// unreachable one never fails a command that doesn't contact it.
#[derive(Debug)]
pub struct LazyUrl {
    unpinned: Url,
    pinned: OnceCell<Url>,
}
impl LazyUrl {
    pub(crate) fn selected_host(&self) -> Option<&str> {
        self.unpinned.host_str()
    }

    /// BLOCKING
    pub fn get(&self) -> Result<&Url, Error> {
        self.pinned.get_or_try_init(|| {
            let mut url = self.unpinned.clone();
            pin_mdns_host(&mut url)?;
            Ok(url)
        })
    }
}
impl From<Url> for LazyUrl {
    fn from(unpinned: Url) -> Self {
        Self {
            unpinned,
            pinned: OnceCell::new(),
        }
    }
}

#[derive(Debug)]
pub struct CliContextSeed {
    pub runtime: OnceCell<Arc<Runtime>>,
    pub base_url: LazyUrl,
    /// Selected server name before `.local` resolution changes the dial address.
    pub host_identity: Option<InternedString>,
    pub registry_url: Option<LazyUrl>,
    pub registry_profiles: Option<Profiles>,
    pub registry_hostname: Vec<InternedString>,
    pub registry_listen: Option<SocketAddr>,
    pub s9pk_s3base: Option<Url>,
    pub s9pk_s3bucket: Option<InternedString>,
    pub tunnel_addr: Option<SocketAddr>,
    pub tunnel_listen: Option<SocketAddr>,
    pub client: Client,
    pub websocket_client: Client,
    pub rpc_client: crate::middleware::auth::session::client::SessionClient,
    pub id_key_path: PathBuf,
    pub id_key_gpg: Option<String>,
    pub id_key: OnceCell<Signer>,
}
impl Drop for CliContextSeed {
    fn drop(&mut self) {
        if let Some(rt) = self.runtime.take() {
            if let Ok(rt) = Arc::try_unwrap(rt) {
                rt.shutdown_background();
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct CliContext(Arc<CliContextSeed>);
impl CliContext {
    /// BLOCKING
    #[instrument(skip_all)]
    pub fn init(config: ClientConfig) -> Result<Self, Error> {
        if let Ok(cwd) = std::env::current_dir() {
            complete_signing_workspace(&cwd);
        }
        let url = match resolve_target(config.host.as_ref())? {
            Some(url) => url,
            None => "http://localhost".parse()?,
        };
        let host_identity = url.host_str().map(InternedString::intern);
        let proxy = config
            .proxy
            .clone()
            .or_else(|| {
                config
                    .socks_listen
                    .and_then(|socks| format!("socks5h://{socks}").parse::<Url>().log_err())
            })
            .map(Proxy::all)
            .transpose()
            .with_kind(ErrorKind::ParseUrl)?;
        let certificates = config
            .root_ca
            .iter()
            .flatten()
            .map(|ca_path| {
                let pem = std::fs::read(ca_path)
                    .with_ctx(|_| (ErrorKind::Filesystem, ca_path.display()))?;
                reqwest::Certificate::from_pem(&pem).with_kind(ErrorKind::OpenSsl)
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let client_builder = || {
            let mut builder = Client::builder();
            if let Some(proxy) = &proxy {
                builder = builder.proxy(proxy.clone());
            }
            if config.insecure {
                builder = builder.danger_accept_invalid_certs(true);
            }
            for cert in &certificates {
                builder = builder.add_root_certificate(cert.clone());
            }
            builder
        };
        let client = client_builder().build().with_kind(ErrorKind::Network)?;
        let websocket_client = client_builder()
            .http1_only()
            .danger_accept_invalid_hostnames(config.insecure)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .with_kind(ErrorKind::Network)?;

        Ok(CliContext(Arc::new(CliContextSeed {
            runtime: OnceCell::new(),
            base_url: url.into(),
            registry_url: resolve_target(config.registry.as_ref())?
                .map(|registry| registry_rpc_url(&registry).map(LazyUrl::from))
                .transpose()?,
            registry_profiles: config.registry,
            registry_hostname: config.registry_hostname.unwrap_or_default(),
            registry_listen: config.registry_listen,
            s9pk_s3base: config.s9pk_s3base,
            s9pk_s3bucket: config.s9pk_s3bucket,
            tunnel_addr: config.tunnel,
            tunnel_listen: config.tunnel_listen,
            host_identity,
            client,
            rpc_client: crate::middleware::auth::session::client::SessionClient::new(
                websocket_client.clone(),
            ),
            websocket_client,
            id_key_path: config.id_key_path.unwrap_or_else(default_id_key_path),
            id_key_gpg: config.id_key_gpg,
            id_key: OnceCell::new(),
        })))
    }

    /// BLOCKING
    #[instrument(skip_all)]
    pub fn id_key(&self) -> Result<&Signer, Error> {
        self.id_key.get_or_try_init(|| {
            if let Some(key) = &self.id_key_gpg {
                return Ok(Signer::GpgAgent(GpgAgentKey::new(key)));
            }
            migrate_legacy_key_file(
                &self.id_key_path,
                &self.id_key_path.with_file_name("developer.key.pem"),
            );
            for path in [Path::new(OS_ID_KEY_PATH), &self.id_key_path] {
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
                return Ok(Signer::Key(AnySigningKey::Ed25519(secret.into())));
            }
            Err(Error::new(
                eyre!("{}", t!("context.cli.id-key-does-not-exist")),
                crate::ErrorKind::Uninitialized,
            ))
        })
    }

    /// The workspace's s9pk signing key. Walks up from cwd for
    /// `.startos/build.key.pem` (created by `s9pk init-workspace`) and errors if
    /// there's no workspace, since s9pk signing is workspace-scoped. Distinct
    /// from [`Self::id_key`], which stays the global identity for
    /// registry/server auth.
    pub fn build_key(&self) -> Result<ed25519_dalek::SigningKey, Error> {
        let cwd = std::env::current_dir().with_kind(ErrorKind::Filesystem)?;
        let root =
            find_signing_workspace(&cwd).ok_or_else(crate::s9pk::init::no_workspace_error)?;
        load_signing_key(root.join(STARTOS_DIR).join(BUILD_KEY_FILE))
    }

    /// BLOCKING
    pub fn rpc_url(&self) -> Result<Url, Error> {
        crate::util::net::rpc_url(self.base_url.get()?, 1)
    }

    pub async fn rest_continuation(
        &self,
        guid: Guid,
        body: reqwest::Body,
        headers: reqwest::header::HeaderMap,
    ) -> Result<reqwest::Response, Error> {
        let mut url = self.base_url.get()?.clone();
        url.path_segments_mut()
            .map_err(|_| eyre!("Url cannot be base"))
            .with_kind(crate::ErrorKind::ParseUrl)?
            .pop_if_empty()
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

    pub(crate) async fn call_rpc(
        &self,
        url: Url,
        headers: HeaderMap,
        address: &str,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        if let Some(signer) = self.rpc_signer(&headers)? {
            return self
                .rpc_client
                .call(
                    &signer,
                    crate::middleware::auth::session::client::websocket_url(url)?,
                    address,
                    headers,
                    method,
                    params,
                )
                .await;
        }
        crate::middleware::auth::signature::call_http(
            self.client.clone(),
            url,
            headers,
            method,
            params,
        )
        .await
    }

    /// Observes an encrypted operation; independent local credentials retain precedence.
    pub async fn call_remote_stream<
        Item: serde::de::DeserializeOwned,
        Final: serde::de::DeserializeOwned,
    >(
        &self,
        method: &str,
        params: Value,
    ) -> Result<crate::util::rpc_client::RpcResponseStream<Value, Item, Final>, Error> {
        let url = self.rpc_url()?;
        let mut headers = HeaderMap::new();
        if let Some(auth) = local_auth_header_for_url::<RpcContext>(&url).await {
            headers.insert(AUTHORIZATION, auth);
        }
        let call = || self.stream_rpc(url.clone(), headers.clone(), method, params.clone());
        let result = match call().await {
            Err(error) if self.login_on_rejection(&error, method, &url).await? => call().await,
            result => result,
        };
        result
            .map_err(Error::from)
            .with_ctx(|error| (error.kind, method))
    }

    pub(crate) async fn stream_rpc<
        Item: serde::de::DeserializeOwned,
        Final: serde::de::DeserializeOwned,
    >(
        &self,
        url: Url,
        headers: HeaderMap,
        method: &str,
        params: Value,
    ) -> Result<crate::util::rpc_client::RpcResponseStream<Value, Item, Final>, RpcError> {
        let signer = self.rpc_signer(&headers)?;
        let signer = signer
            .as_ref()
            .unwrap_or_else(|| self.rpc_client.ephemeral_signer());
        let address = self
            .host_identity
            .as_deref()
            .or_else(|| url.host_str())
            .or_not_found("server hostname")?;
        self.rpc_client
            .stream(
                signer,
                crate::middleware::auth::session::client::websocket_url(url.clone())?,
                address,
                headers,
                method,
                params,
            )
            .await
    }

    async fn login_on_rejection(
        &self,
        error: &RpcError,
        method: &str,
        url: &Url,
    ) -> Result<bool, RpcError> {
        if error.code != ErrorKind::Authorization as i32
            || method == "auth.login"
            || !std::io::stdin().is_terminal()
        {
            return Ok(false);
        }
        eprintln!(
            "{}",
            t!(
                "context.cli.not-logged-in",
                host = self
                    .host_identity
                    .as_deref()
                    .or_else(|| url.host_str())
                    .unwrap_or_default()
            )
        );
        crate::auth::login::<RpcContext, _>(self, "auth.login").await?;
        Ok(true)
    }

    pub(crate) async fn call_startup_rpc(
        &self,
        method: &str,
        params: Value,
        headers: HeaderMap,
    ) -> Result<Value, RpcError> {
        self.rpc_client
            .call(
                self.rpc_client.ephemeral_signer(),
                crate::middleware::auth::session::client::websocket_url(self.rpc_url()?)?,
                self.host_identity.as_deref().unwrap_or_default(),
                headers,
                method,
                params,
            )
            .await
    }

    pub(crate) async fn call_startup_stream<
        Item: serde::de::DeserializeOwned,
        Final: serde::de::DeserializeOwned,
    >(
        &self,
        method: &str,
        params: Value,
        headers: HeaderMap,
    ) -> Result<crate::util::rpc_client::RpcResponseStream<Value, Item, Final>, RpcError> {
        self.rpc_client
            .stream(
                self.rpc_client.ephemeral_signer(),
                crate::middleware::auth::session::client::websocket_url(self.rpc_url()?)?,
                self.host_identity.as_deref().unwrap_or_default(),
                headers,
                method,
                params,
            )
            .await
    }

    pub(crate) async fn call_diagnostic_stream<
        Item: serde::de::DeserializeOwned,
        Final: serde::de::DeserializeOwned,
    >(
        &self,
        method: &str,
        params: Value,
    ) -> Result<crate::util::rpc_client::RpcResponseStream<Value, Item, Final>, Error> {
        let url = self.rpc_url()?;
        let mut headers = HeaderMap::new();
        if let Some(auth) = local_auth_header_for_url::<DiagnosticContext>(&url).await {
            headers.insert(AUTHORIZATION, auth);
        }
        self.call_startup_stream(method, params, headers)
            .await
            .map_err(Error::from)
            .with_ctx(|error| (error.kind, method))
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
        let rpc_url = self.rpc_url()?;
        let mut headers = HeaderMap::new();
        if let Some(auth) = local_auth_header_for_url::<RpcContext>(&rpc_url).await {
            headers.insert(AUTHORIZATION, auth);
        }
        let call = || {
            self.call_rpc(
                rpc_url.clone(),
                headers.clone(),
                self.host_identity.as_deref().unwrap_or_default(),
                method,
                params.clone(),
            )
        };
        match call().await {
            Err(error) if self.login_on_rejection(&error, method, &rpc_url).await? => call().await,
            res => res,
        }
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
        let rpc_url = self.rpc_url()?;
        let mut headers = HeaderMap::new();
        if let Some(auth) = local_auth_header_for_url::<DiagnosticContext>(&rpc_url).await {
            headers.insert(AUTHORIZATION, auth);
        }
        self.call_startup_rpc(method, params, headers).await
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
        self.call_startup_rpc(method, params, HeaderMap::new())
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
        self.call_startup_rpc(method, params, HeaderMap::new())
            .await
    }
}
