pub mod error;
pub mod invoke;
pub mod prelude;

pub mod activity;
pub mod auth;
pub mod backup;
pub mod bins;
pub mod boot0;
pub mod captive;
pub mod continuations;
pub mod device_ident;
pub mod device_names;
pub mod devices;
pub mod diagnostics;
pub mod dns;
pub mod dns_inject;
pub mod eeprom;
pub mod embedded_web;
pub mod ethernet;
pub mod exec;
pub mod files;
pub mod flash;
pub mod http_redirect;
pub mod init;
pub mod ipv6_tracker;
pub mod lan;
pub mod lan_guard;
pub mod logs;
pub mod luci_proxy;
pub mod migrate;
pub mod port_control;
pub mod profiles;
pub mod progress;
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
pub mod vpn_client;
pub mod vpn_server;
pub mod wan;
pub mod wg;
pub mod wifi;

use std::io::IsTerminal;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use clap::Parser;
use color_eyre::eyre::eyre;
pub use error::{Error, ErrorKind};
use http::header::AUTHORIZATION;
use http::HeaderMap;
use startos::auth::IdKeyContext;
use startos::middleware::auth::local::{is_loopback, local_auth_header};
use startos::middleware::auth::signature::SigningContext;
use startos::sign::{AnySigningKey, Signer};
use tokio::runtime::Runtime;

/// Non-ambiguous character set for generated passwords.
///
/// 68 chars: A-Z minus I,O (24) + a-z minus l (25) + 2-9 (8) + !@#$%^&*=+? (11)
pub const PASSWORD_CHARS: &str =
    "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#$%^&*=+?";

/// Non-ambiguous alphanumeric subset (no special characters).
///
/// 57 chars: A-Z minus I,O (24) + a-z minus l (25) + 2-9 (8)
pub const PASSWORD_CHARS_ALNUM: &str = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";

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
use rpc_toolkit::reqwest::{self, Client, Url};
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{from_fn_async, CallRemote, Context, Empty, HandlerExt, ParentHandler};

pub trait CtrlContext: Context + Clone {
    fn uci_root(&self) -> PathBuf;
    fn effectful(&self) -> bool;
    /// The router's WAN IPv4 addresses: the public side of every IPv4
    /// published port. Empty when unknown.
    fn wan_ipv4_addrs(&self) -> Vec<std::net::Ipv4Addr> {
        if self.effectful() {
            crate::system::wan_ipv4_addrs()
        } else {
            Vec::new()
        }
    }
}

fn id_key_path() -> PathBuf {
    std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".startwrt").join("id.key.pem"))
        .unwrap_or_else(|_| PathBuf::from("/root/.startwrt/id.key.pem"))
}

/// CLI arguments parsed by clap
#[derive(Clone, Parser)]
#[command(name = "startwrt", version)]
pub struct CliArgs {
    #[clap(long, default_value = "/etc/config")]
    pub config_root: PathBuf,
    #[clap(long)]
    pub configs_only: bool,
    /// Server URL [default: http://127.0.0.1/rpc/v1 on the router, else http://router.lan/rpc/v1]
    #[clap(long)]
    pub host: Option<Url>,
}

pub struct CliContextSeed {
    pub config_root: PathBuf,
    pub configs_only: bool,
    pub host: Url,
    pub client: Client,
    pub id_key_path: PathBuf,
    id_key: OnceLock<Signer>,
    runtime: OnceLock<Arc<Runtime>>,
}

#[derive(Clone)]
pub struct CliContext(Arc<CliContextSeed>);

impl Deref for CliContext {
    type Target = CliContextSeed;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn default_host(on_router: bool) -> Url {
    if on_router {
        "http://127.0.0.1/rpc/v1"
    } else {
        "http://router.lan/rpc/v1"
    }
    .parse()
    .unwrap()
}

impl CliContext {
    pub fn init(args: CliArgs) -> Result<Self, Error> {
        let on_router = std::fs::read_to_string(crate::auth::LOCAL_AUTH_COOKIE_PATH).is_ok();
        let host = args.host.unwrap_or_else(|| default_host(on_router));

        let client = Client::builder().build().map_err(|e| {
            Error::new(
                eyre!("Failed to build HTTP client: {}", e),
                ErrorKind::Network,
            )
        })?;

        Ok(Self(Arc::new(CliContextSeed {
            config_root: args.config_root,
            configs_only: args.configs_only,
            host,
            client,
            id_key_path: id_key_path(),
            id_key: OnceLock::new(),
            runtime: OnceLock::new(),
        })))
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    /// The key enrolled by `auth login`.
    pub fn id_key(&self) -> Result<&Signer, startos::Error> {
        if let Some(key) = self.id_key.get() {
            return Ok(key);
        }
        let key = startos::developer::load_signing_key(&self.id_key_path)?;
        Ok(self
            .id_key
            .get_or_init(|| Signer::Key(AnySigningKey::Ed25519(key))))
    }

    /// Enrolls the identity key by password, generating it first if absent.
    pub async fn login(&self) -> Result<(), RpcError> {
        Ok(startos::auth::login::<ServerContext, _>(self, LOGIN_METHOD).await?)
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let mut headers = HeaderMap::new();
        if is_loopback(&self.host) {
            if let Some(auth) = local_auth_header::<ServerContext>().await {
                headers.insert(AUTHORIZATION, auth);
            }
        }
        startos::middleware::auth::signature::call_remote(
            self,
            self.host.clone(),
            headers,
            self.host.host_str(),
            method,
            params,
        )
        .await
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
        let res =
            self.client.get(url).send().await.map_err(|e| {
                Error::new(eyre!("Download request failed: {e}"), ErrorKind::Network)
            })?;
        if !res.status().is_success() {
            return Err(Error::new(
                eyre!("Download failed: {}", res.status()),
                ErrorKind::Network,
            ));
        }
        let filename = res
            .headers()
            .get(reqwest::header::CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split("filename=\"").nth(1))
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or("download")
            .to_string();
        let bytes = res
            .bytes()
            .await
            .map_err(|e| Error::new(eyre!("Failed to read response: {e}"), ErrorKind::Network))?
            .to_vec();
        Ok((bytes, filename))
    }

    /// POST data to a continuation.
    pub async fn rest_upload(&self, guid: &str, data: Vec<u8>) -> Result<(), Error> {
        let url = self.rest_url(guid);
        let res = self
            .client
            .post(url)
            .body(data)
            .send()
            .await
            .map_err(|e| Error::new(eyre!("Upload request failed: {e}"), ErrorKind::Network))?;
        if !res.status().is_success() {
            let text = res.text().await.unwrap_or_default();
            return Err(Error::new(
                eyre!("Upload failed: {text}"),
                ErrorKind::Network,
            ));
        }
        Ok(())
    }
}

const LOGIN_METHOD: &str = "auth.login";

impl AsRef<Client> for CliContext {
    fn as_ref(&self) -> &Client {
        &self.client
    }
}

impl SigningContext for CliContext {
    fn signing_key(&self) -> Result<Signer, startos::Error> {
        Ok(self.id_key()?.clone())
    }
}

impl IdKeyContext for CliContext {
    fn id_key(&self) -> Result<&Signer, startos::Error> {
        CliContext::id_key(self)
    }
    fn id_key_path(&self) -> &Path {
        &self.id_key_path
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
        match self.call(method, params.clone()).await {
            Err(e)
                if e.code == ErrorKind::Authorization as i32
                    && method != LOGIN_METHOD
                    && std::io::stdin().is_terminal() =>
            {
                eprintln!(
                    "Not logged in to {}.",
                    self.host.host_str().unwrap_or_default()
                );
                self.login().await?;
                self.call(method, params).await
            }
            res => res,
        }
    }
}

#[derive(Clone)]
pub struct ServerContext {
    pub continuations: Arc<continuations::RpcContinuations>,
    pub auth: Arc<auth::AuthState>,
}
impl Default for ServerContext {
    fn default() -> Self {
        Self {
            continuations: Arc::new(continuations::RpcContinuations::new()),
            auth: Arc::new(auth::AuthState::in_memory()),
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
        .subcommand("dns", dns_inject::dns::<C>())
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

/// Make odhcpd re-read its config, and give it a moment to act on the change,
/// **before** the network is torn down or reconfigured.
///
/// This is the only way to revoke an IPv6 prefix from LAN clients. SLAAC has no
/// lease to expire, so a prefix is withdrawn by advertising it one last time
/// with zero lifetimes (RFC 9096 §3.5). odhcpd emits that RA from
/// `router_setup_interface(iface, false)`, which it reaches when a config reload
/// finds RA newly disabled on an interface — i.e. on **SIGHUP**, which is what
/// `/etc/init.d/odhcpd reload` sends. A `restart` does not: odhcpd's exit path
/// tears down the process without touching the interfaces, so the old process
/// dies silently and the new one starts with RA already off, having never told
/// anyone. Clients then keep the address for the rest of its advertised valid
/// lifetime (odhcpd caps this at 5400 s), which is why "IPv6 disabled" used to
/// leave devices holding addresses for up to 90 minutes.
///
/// Two ordering constraints make this a helper rather than a one-line call:
///
/// * The UCI change must already be on disk — the reload is what reads it.
/// * The prefix must still be on the interface, so this has to run *before*
///   `network restart`/`reload`. There is nothing to wait on (`procd_send_signal`
///   returns as soon as the signal is delivered, while the RA goes out later
///   from odhcpd's event loop), so we settle for a fixed pause.
///
/// Best-effort by nature: odhcpd sends exactly one final RA, and a client that
/// misses it — or is asleep — falls back to waiting out the valid lifetime.
pub async fn deprecate_odhcpd_prefixes() {
    let _ = run_quiet_async(tokio::process::Command::new("/etc/init.d/odhcpd").arg("reload")).await;
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}

pub fn init_logging(name: &str) {
    use std::ffi::CString;

    use syslog_tracing::{Facility, Options, Syslog};
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::{fmt, EnvFilter, Registry};

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,activity=info"));

    // openlog(3) keeps the identity pointer, so it has to outlive the process.
    let identity: &'static _ = Box::leak(
        CString::new(name)
            .expect("logging identity contains a nul byte")
            .into_boxed_c_str(),
    );
    let syslog = Syslog::new(identity, Options::default(), Facility::default())
        .expect("failed to open syslog");

    // syslogd stamps and tags each line itself, so emit only the message.
    let subscriber = Registry::default()
        .with(
            fmt::layer()
                .with_writer(syslog)
                .with_ansi(false)
                .without_time(),
        )
        .with(filter);
    tracing::subscriber::set_global_default(subscriber)
        .expect("failed to set global tracing subscriber");
}
