pub mod prelude;
pub mod error;
pub mod invoke;

pub mod activity;
pub mod auth;
pub mod backup;
pub mod captive;
pub mod continuations;
pub mod device_names;
pub mod devices;
pub mod diagnostics;
pub mod dns;
pub mod eeprom;
pub mod ethernet;
pub mod exec;
pub mod files;
pub mod flash;
pub mod init;
pub mod lan;
pub mod logs;
pub mod luci_proxy;
pub mod middleware;
pub mod progress;
pub mod profiles;
pub mod published_ports;
pub mod registry;
pub mod setup;
pub mod sign;
pub mod ssh_keys;
pub mod ssl;
pub mod system;
pub mod uci;
pub mod update;
pub mod utils;
pub mod verify;
pub mod wan;
pub mod vpn_client;
pub mod vpn_server;
pub mod wg;
pub mod wifi;
pub mod embedded_web;
pub mod bins;

use std::fs::{File, OpenOptions};
use std::io::BufReader;
use std::ops::Deref;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use std::sync::{Arc, OnceLock};

use clap::Parser;
use color_eyre::eyre::eyre;
use cookie_store::CookieStore;
use reqwest_cookie_store::CookieStoreMutex;
use tokio::runtime::Runtime;

pub use error::{Error, ErrorKind};

/// Non-ambiguous character set for generated passwords.
///
/// 67 chars: A-Z minus I,O (24) + a-z minus l (25) + 2-9 (8) + !@#$%^&*=+? (10)
pub const PASSWORD_CHARS: &str =
    "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#$%^&*=+?";

/// Non-ambiguous alphanumeric subset (no special characters).
///
/// 57 chars: A-Z minus I,O (24) + a-z minus l (25) + 2-9 (8)
pub const PASSWORD_CHARS_ALNUM: &str =
    "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";

/// Generate a random password from the given charset using rejection sampling.
pub fn generate_password(charset: &[u8], len: usize) -> String {
    let limit = 256 - (256 % charset.len());
    let mut password = String::with_capacity(len);
    while password.len() < len {
        let mut buf = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rng(), &mut buf);
        for &b in &buf {
            if (b as usize) < limit {
                password.push(charset[b as usize % charset.len()] as char);
                if password.len() == len {
                    break;
                }
            }
        }
    }
    password
}
use imbl_value::imbl::OrdMap;
use imbl_value::Value;
use rpc_toolkit::yajrc::{GenericRpcMethod, Id, RpcError, RpcRequest};
use rpc_toolkit::{
    from_fn_async,
    reqwest::{self, Client, Url},
    CallRemote, Context, Empty, HandlerExt, ParentHandler, RpcResponse,
};

pub trait CtrlContext: Context + Clone {
    fn uci_root(&self) -> PathBuf;
    fn effectful(&self) -> bool;
}

fn cookies_path() -> PathBuf {
    std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".startwrt").join(".cookies.json"))
        .unwrap_or_else(|_| PathBuf::from("/tmp/.startwrt-cookies.json"))
}

/// CLI arguments parsed by clap
#[derive(Clone, Parser)]
pub struct CliArgs {
    #[clap(long, default_value = "/etc/config")]
    pub config_root: PathBuf,
    #[clap(long)]
    pub configs_only: bool,
    #[clap(long, default_value = "http://router.lan/rpc/v1")]
    pub host: Url,
}

/// Inner context with cookie persistence on Drop
pub struct CliContextSeed {
    pub config_root: PathBuf,
    pub configs_only: bool,
    pub host: Url,
    pub client: Client,
    pub cookie_store: Arc<CookieStoreMutex>,
    pub cookie_path: PathBuf,
    runtime: OnceLock<Arc<Runtime>>,
}

impl Drop for CliContextSeed {
    fn drop(&mut self) {
        let tmp = format!("{}.tmp", self.cookie_path.display());
        let parent_dir = self.cookie_path.parent().unwrap_or(Path::new("/"));
        if !parent_dir.exists() {
            if let Err(e) = std::fs::create_dir_all(parent_dir) {
                tracing::warn!("Failed to create cookie directory: {}", e);
                return;
            }
        }
        let file = match OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
        {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("Failed to create cookie temp file: {}", e);
                return;
            }
        };
        let mut writer = match fd_lock_rs::FdLock::lock(file, fd_lock_rs::LockType::Exclusive, true)
        {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("Failed to lock cookie file: {}", e);
                return;
            }
        };
        let store = self.cookie_store.lock().unwrap();
        if let Err(e) = cookie_store::serde::json::save(&store, &mut *writer) {
            tracing::warn!("Failed to save cookies: {}", e);
            return;
        }
        if let Err(e) = writer.sync_all() {
            tracing::warn!("Failed to sync cookie file: {}", e);
            return;
        }
        if let Err(e) = std::fs::rename(&tmp, &self.cookie_path) {
            tracing::warn!("Failed to rename cookie file: {}", e);
        }
    }
}

/// CLI context with session persistence
#[derive(Clone)]
pub struct CliContext(Arc<CliContextSeed>);

impl Deref for CliContext {
    type Target = CliContextSeed;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl CliContext {
    pub fn init(args: CliArgs) -> Result<Self, Error> {
        let cookie_path = cookies_path();
        let cookie_store = Arc::new(CookieStoreMutex::new(if cookie_path.exists() {
            cookie_store::serde::json::load(BufReader::new(
                File::open(&cookie_path).map_err(|e| {
                    Error::new(eyre!("Failed to open cookie file: {}", e), ErrorKind::Filesystem)
                })?,
            ))
            .unwrap_or_default()
        } else {
            CookieStore::default()
        }));

        // If the local auth cookie exists (running on the router), inject it
        // so the server's auth middleware trusts us without a session.
        if let Ok(local_token) = std::fs::read_to_string(crate::auth::LOCAL_AUTH_COOKIE_PATH) {
            let local_token = local_token.trim();
            let domain = args.host.host_str().unwrap_or("localhost");
            let cookie_value = format!("local={local_token}; Domain={domain}; Path=/; SameSite=Strict");
            if let Ok(cookie) = cookie_store::RawCookie::parse(cookie_value) {
                cookie_store.lock().unwrap().insert_raw(&cookie, &args.host).ok();
            }
        }

        let client = Client::builder()
            .cookie_provider(cookie_store.clone())
            .build()
            .map_err(|e| Error::new(eyre!("Failed to build HTTP client: {}", e), ErrorKind::Network))?;

        Ok(Self(Arc::new(CliContextSeed {
            config_root: args.config_root,
            configs_only: args.configs_only,
            host: args.host,
            client,
            cookie_store,
            cookie_path,
            runtime: OnceLock::new(),
        })))
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Build URL for a continuation endpoint.
    pub fn rest_url(&self, guid: &str) -> Url {
        let mut url = self.host.clone();
        url.set_path(&format!("/rest/rpc/{guid}"));
        url
    }

    /// GET a continuation, returning (bytes, filename).
    pub async fn rest_download(&self, guid: &str) -> Result<(Vec<u8>, String), Error> {
        let url = self.rest_url(guid);
        let res = self.client.get(url).send().await
            .map_err(|e| Error::new(eyre!("Download request failed: {e}"), ErrorKind::Network))?;
        if !res.status().is_success() {
            return Err(Error::new(eyre!("Download failed: {}", res.status()), ErrorKind::Network));
        }
        let filename = res.headers()
            .get(reqwest::header::CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split("filename=\"").nth(1))
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or("download")
            .to_string();
        let bytes = res.bytes().await
            .map_err(|e| Error::new(eyre!("Failed to read response: {e}"), ErrorKind::Network))?
            .to_vec();
        Ok((bytes, filename))
    }

    /// POST data to a continuation.
    pub async fn rest_upload(&self, guid: &str, data: Vec<u8>) -> Result<(), Error> {
        let url = self.rest_url(guid);
        let res = self.client.post(url).body(data).send().await
            .map_err(|e| Error::new(eyre!("Upload request failed: {e}"), ErrorKind::Network))?;
        if !res.status().is_success() {
            let text = res.text().await.unwrap_or_default();
            return Err(Error::new(eyre!("Upload failed: {text}"), ErrorKind::Network));
        }
        Ok(())
    }
}

impl Context for CliContext {
    fn runtime(&self) -> Option<Arc<Runtime>> {
        Some(
            self.runtime
                .get_or_init(|| {
                    // Multi-thread runtime with 1 worker so `block_in_place`
                    // works when handlers call blocking uciedit operations.
                    Arc::new(
                        tokio::runtime::Builder::new_multi_thread()
                            .worker_threads(1)
                            .enable_all()
                            .build()
                            .unwrap(),
                    )
                })
                .clone(),
        )
    }
}

impl CtrlContext for CliContext {
    fn uci_root(&self) -> PathBuf {
        self.config_root.clone()
    }

    fn effectful(&self) -> bool {
        !self.configs_only
    }
}

impl CallRemote<ServerContext> for CliContext {
    async fn call_remote(
        &self,
        method: &str,
        _metadata: OrdMap<&'static str, Value>,
        params: Value,
        _extra: Empty,
    ) -> Result<Value, RpcError> {
        // Mirror start-os's `signature::call_remote`: always speak JSON.
        // rpc-toolkit's own `call_remote_http` sends CBOR when the `cbor`
        // feature is on — and it is, because the start-os submodule pulls
        // rpc-toolkit with default features (`default = ["cbor"]`), which
        // feature-unification forces on for the whole build. The rpc-toolkit
        // HTTP server only ever parses JSON request bodies, so a CBOR body
        // fails to parse before any handler runs. Auth is carried by the
        // cookie store on `self.client` (local auth cookie / loopback), so we
        // don't need start-os's signature header.
        let rpc_req = RpcRequest {
            id: Some(Id::Number(0.into())),
            method: GenericRpcMethod::<_, _, Value>::new(method),
            params,
        };
        let body = serde_json::to_vec(&rpc_req)?;
        let res = self
            .client()
            .request(reqwest::Method::POST, self.host.clone())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::CONTENT_LENGTH, body.len())
            .body(body)
            .send()
            .await?;

        // The rpc-toolkit server answers RPC-level errors with a 200 + a
        // JSON-RPC error body, so a non-2xx here is an HTTP/infrastructure
        // failure (proxy, auth rejection, body limit, …). Surface it clearly
        // rather than feeding the (likely non-JSON) body to the parser below.
        if !res.status().is_success() {
            let status = res.status();
            let txt = res.text().await.unwrap_or_default();
            let reason = status.canonical_reason().unwrap_or_else(|| status.as_str());
            let detail = if txt.is_empty() {
                reason.to_string()
            } else {
                format!("{reason}: {txt}")
            };
            return Err(Error::new(eyre!("{detail}"), ErrorKind::Network).into());
        }

        match res
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
        {
            Some("application/json") => serde_json::from_slice::<RpcResponse>(&*res.bytes().await?)
                .map_err(|e| Error::new(eyre!("{e}"), ErrorKind::Deserialization))?
                .result,
            _ => Err(Error::new(
                eyre!("unexpected content type from daemon"),
                ErrorKind::Network,
            )
            .into()),
        }
    }
}

#[derive(Clone)]
pub struct ServerContext {
    pub continuations: continuations::RpcContinuations,
    pub open_authed_continuations: continuations::OpenAuthedContinuations,
}
impl Default for ServerContext {
    fn default() -> Self {
        Self {
            continuations: continuations::RpcContinuations::new(),
            open_authed_continuations: continuations::OpenAuthedContinuations::new(),
        }
    }
}
impl Context for ServerContext {}
impl CtrlContext for ServerContext {
    fn uci_root(&self) -> PathBuf {
        "/etc/config/".into()
    }

    fn effectful(&self) -> bool {
        true
    }
}

pub fn main_api<C: CtrlContext + Clone>() -> ParentHandler<C> {
    use utils::HandlerExtSerde;

    ParentHandler::new()
        .subcommand("auth", auth::auth::<C>())
        .subcommand("profiles", profiles::profiles::<C>())
        .subcommand("ethernet", ethernet::ethernet::<C>())
        .subcommand("wifi", wifi::wifi::<C>())
        .subcommand("vpn-server", vpn_server::vpn_server::<C>())
        .subcommand("vpn-client", vpn_client::vpn_client::<C>())
        .subcommand("uci", uci::uci::<C>())
        .subcommand("file", files::file::<C>())
        .subcommand("dir", files::dir::<C>())
        .subcommand(
            "exec",
            from_fn_async(exec::exec_command)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand("setup", setup::setup::<C>())
        .subcommand("system", system::system::<C>())
        .subcommand("devices", devices::devices::<C>())
        .subcommand("wan", wan::wan::<C>())
        .subcommand("lan", lan::lan::<C>())
        .subcommand("published-ports", published_ports::published_ports::<C>())
        .subcommand("ssh-keys", ssh_keys::ssh_keys::<C>())
        .subcommand("activity", activity::activity::<C>())
        .subcommand("backup", backup::backup::<C>())
        .subcommand("diagnostics", diagnostics::diagnostics::<C>())
}

/// Spawn a command with stdio redirected to /dev/null and wait for the
/// direct child to exit (NOT `wait_with_output`).
///
/// This exists because `start-os`'s `Invoke` trait defaults to `Stdio::piped()`
/// + `wait_with_output()`, which hangs forever when a forked grandchild
/// (udhcpc, hotplug scripts, etc.) inherits the pipe fds and never closes
/// them. Use this helper for any init-script / service-reload / `ifup`-like
/// call where we don't care about the output and the child may spawn
/// long-lived daemons.
pub async fn run_quiet_async(
    cmd: &mut tokio::process::Command,
) -> std::io::Result<std::process::ExitStatus> {
    use std::process::Stdio;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?
        .wait()
        .await
}

pub fn init_logging(_name: &str) {
    use tracing_rfc_5424::{
        rfc3164::Rfc3164, tracing::TrivialTracingFormatter, transport::UnixSocket,
    };
    use tracing_subscriber::Registry;
    use tracing_subscriber::{
        layer::SubscriberExt,
        EnvFilter,
    };

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn,activity=info"));

    let syslog = tracing_rfc_5424::layer::Layer::<
        tracing_subscriber::Registry,
        Rfc3164,
        TrivialTracingFormatter,
        UnixSocket,
    >::try_default()
    .unwrap();

    let subscriber = Registry::default()
        .with(syslog)
        .with(filter);
    tracing::subscriber::set_global_default(subscriber)
        .expect("failed to set global tracing subscriber");
}
