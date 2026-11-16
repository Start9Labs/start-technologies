use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsString;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::DefaultBodyLimit;
use axum::http::{header, Response};
use axum::routing::{any, get, post};
use axum::{Extension, Json};
use startos::middleware::auth::local::LocalAuthContext;
use startos::middleware::auth::session::RpcSessionUpgrader;
use startos::net::tls::TlsListener;
use startos::net::web_server::{Accept, Acceptor, DynAccept, MetadataVisitor, WebServer};
use tokio::signal::unix::SignalKind;
use tracing::instrument;
use visit_rs::Visit;

use crate::continuations::{self, RpcContinuations};
use crate::embedded_web::serve_embedded;
use crate::luci_proxy::{self, ProxyClient};
use crate::prelude::*;
use crate::setup::{self, FlashParams, SetupEvent};
use crate::{init_logging, ssl, ServerContext};

fn ndjson_error(message: &str) -> Response<Body> {
    let event = SetupEvent::Error {
        message: message.into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    Response::builder()
        .header("content-type", "application/x-ndjson")
        .body(Body::from(format!("{json}\n")))
        .unwrap()
}

async fn ws_continuation_handler(
    axum::extract::Path(guid): axum::extract::Path<String>,
    ws: axum::extract::ws::WebSocketUpgrade,
    Extension(continuations): Extension<Arc<RpcContinuations>>,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    let Some(guid) = continuations::Guid::from(&guid) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match continuations.get_ws_handler(&guid).await {
        Some(handler) => handler.on_upgrade(ws, guid),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(crate) async fn setup_flash_handler(
    Extension(ctx): Extension<ServerContext>,
    Json(params): Json<FlashParams>,
) -> Response<Body> {
    use futures::StreamExt;

    let operation = match setup::start_flash(&ctx, params) {
        Ok(operation) => operation,
        Err(error) => return ndjson_error(&error.to_string()),
    };
    let stream = setup::legacy_flash_events(operation).map(|event| {
        let mut json = serde_json::to_string(&event).unwrap();
        json.push('\n');
        Ok::<_, std::convert::Infallible>(json)
    });

    Response::builder()
        .header("content-type", "application/x-ndjson")
        .body(Body::from_stream(stream))
        .unwrap()
}

fn root_ca_response(
    file: Result<String, Error>,
    content_type: &'static str,
    filename: &'static str,
) -> Response<Body> {
    match file {
        Ok(body) => Response::builder()
            .header(header::CONTENT_TYPE, content_type)
            .header(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            )
            .body(Body::from(body))
            .unwrap(),
        Err(_) => Response::builder()
            .status(500)
            .body(Body::from("Root CA certificate not available"))
            .unwrap(),
    }
}

/// Sets up the CA and the HTTPS certificate store. `None` serves HTTP alone.
async fn init_tls(ctx: ServerContext) -> Option<crate::tls::RouterTlsHandler> {
    if let Err(e) = ssl::ensure_root_ca().await {
        tracing::error!("Root CA generation failed: {e}");
        return None;
    }
    if let Err(e) = ssl::ensure_intermediate_ca().await {
        tracing::error!("intermediate CA generation failed: {e}");
        return None;
    }
    crate::tls::init(ctx)
        .await
        .map_err(|e| tracing::error!("certificate store setup failed: {e}"))
        .ok()
}

/// Identifies which logical listener (HTTP or HTTPS) accepted a given
/// connection. Plumbed through `WebServer`'s metadata pipeline so request
/// extensions can inspect it if needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WebserverListener {
    Http,
    Https,
}
impl<V: MetadataVisitor> Visit<V> for WebserverListener {
    fn visit(&self, visitor: &mut V) -> <V as visit_rs::Visitor>::Result {
        visitor.visit(self)
    }
}

#[instrument(skip_all)]
async fn inner_main() -> Result<(), Error> {
    <ServerContext as LocalAuthContext>::init_auth_cookie().await?;

    let setup_mode = setup::is_setup_mode().await;

    let continuations = Arc::new(RpcContinuations::new());
    let auth = Arc::new(crate::auth::AuthState::open().await?);
    let server_identity = auth.server_identity().await?;
    let ctx = ServerContext {
        setup_flash: Arc::new(tokio::sync::Mutex::new(())),
        secure_session: None,
        continuations: continuations.clone(),
        auth,
        server_identity,
    };

    if setup_mode {
        tracing::info!("setup mode detected (booted from removable media)");

        // Resolve WiFi password from EEPROM tag 0x2F
        let pwd = match crate::eeprom::read_wifi_password() {
            Ok(Some(p)) => Some(p),
            Ok(None) => {
                tracing::warn!(
                    "no valid WiFi password in EEPROM; setup AP not configured. \
                     Wizard remains reachable over ethernet."
                );
                None
            }
            Err(e) => {
                tracing::error!("EEPROM read failed: {e}");
                None
            }
        };

        if let Some(ref wifi_password) = pwd {
            // Configure WiFi AP with the EEPROM password (single-client limit).
            if let Err(e) = crate::init::configure_wifi("/etc/config", wifi_password, Some(1)).await
            {
                tracing::error!("WiFi AP setup failed: {e}");
            }
            if let Err(e) =
                crate::wifi::reload_wifi("/etc/config", crate::wifi::WifiReload::Reload).await
            {
                tracing::error!("WiFi reload failed: {e}");
            }

            // Enable captive portal only when WiFi AP can start
            if let Err(e) = crate::captive::enable_captive_portal().await {
                tracing::error!("captive portal setup failed: {e}");
            }
        }
    } else {
        // Normal mode: restore WiFi + manage captive portal state

        crate::boot::record_start(crate::update::update_pending());
        // Record the outcome of a pending OTA update (if any) in the Activity
        // log now that the new firmware is up.
        crate::update::check_pending_update_marker();

        if let Err(e) = crate::init::restore_wifi_if_needed().await {
            tracing::error!("WiFi auto-restore failed: {e}");
        }
        if let Err(e) = crate::wifi::heal_channel_widths("/etc/config").await {
            tracing::error!("WiFi channel width repair failed: {e}");
        }

        // netifd raises the radios at boot whatever the blackout schedule says.
        match crate::wifi::apply_blackout("/etc/config").await {
            Ok(true) => crate::activity::log(
                "wifi",
                "blackout-reasserted",
                true,
                "Reasserted WiFi blackout after reboot (inside active window)",
                None,
            ),
            Ok(false) => {}
            Err(e) => tracing::error!("Failed to apply WiFi blackout at boot: {e}"),
        }
        if let Err(e) = crate::profiles::bootstrap_admin_profile("/etc/config").await {
            tracing::error!("Admin profile bootstrap failed: {e}");
        }
        // Must follow bootstrap_admin_profile: the heal enumerates profiles from
        // the `startwrt` config, so the admin profile has to be registered first.
        if let Err(e) = crate::profiles::heal_ipv6_state("/etc/config").await {
            tracing::error!("IPv6 state repair failed: {e}");
        }
        // Installs the hairpin accept and the IPv6 MAC binding, and re-derives
        // every hairpin projection (fw4 drops a redirect whose list names a
        // deleted zone). Reloads the firewall only when something changed.
        if let Err(e) = crate::published_ports::heal_published_ports("/etc/config").await {
            tracing::error!("Published-port firewall repair failed: {e}");
        }
        if let Err(e) = crate::lan_guard::heal_ra_allow("/etc/config").await {
            tracing::error!("LAN guard repair failed: {e}");
        }
        if let Err(e) = crate::vpn_client::heal_vpn_clients("/etc/config").await {
            tracing::error!("Outbound VPN repair failed: {e}");
        }
        if let Err(e) = crate::system::apply_remote_access(ctx.clone()).await {
            tracing::error!("Remote access rule apply failed: {e}");
        }
        if let Err(e) = crate::dns::heal_smartdns_conf("/etc/config").await {
            tracing::error!("SmartDNS config repair failed: {e}");
        }
        if let Err(e) = crate::profiles::heal_dns_forwarding(ctx.clone()).await {
            tracing::error!("DNS forwarding repair failed: {e}");
        }
        // Repairs a reservation name from a release that let one through, which
        // dnsmasq refuses to start on. Must precede the fingerprint hook: both
        // reload dnsmasq, and this one decides whether it can come up at all.
        if let Err(e) = crate::devices::heal_dhcp_host_names("/etc/config").await {
            tracing::error!("DHCP reservation name repair failed: {e}");
        }
        // Install the DHCP-fingerprint hook (script + `dhcpscript` on every
        // dnsmasq section) — daemon-side so OTA-updated routers converge on
        // first boot. Reloads dnsmasq only when something actually changed.
        if let Err(e) = crate::device_ident::ensure_dhcp_fingerprint_hook().await {
            tracing::error!("DHCP fingerprint hook setup failed: {e}");
        }
        // Apply WAN schedule enforcement (UCI firewall rules)
        if let Err(e) = crate::profiles::evaluate_and_apply_schedules(&ctx).await {
            tracing::error!("Failed to evaluate WAN schedules at boot: {e}");
        }

        // The reconcilers above fix the *current* state ("we booted mid-window");
        // these rebuild *future* cron edges. /etc/crontabs/root is a disposable
        // projection of the UCI schedule stores and is wiped by sysupgrade (the
        // stores themselves persist), so regenerate both projections — WAN schedule
        // + WiFi blackout — from UCI and reload cron once, or schedule edges would
        // stop firing after an upgrade. No-op when there are no windows; also
        // self-heals crontab drift on any boot. Both regenerators strip only their
        // own tagged lines, so running them in sequence over the shared file is safe.
        if let Err(e) = crate::profiles::regenerate_schedule_crontab(&ctx).await {
            tracing::error!("Failed to regenerate WAN schedule crontab at boot: {e}");
        }
        if let Err(e) = crate::wifi::regenerate_blackout_crontab(&ctx).await {
            tracing::error!("Failed to regenerate WiFi blackout crontab at boot: {e}");
        }
        if let Err(e) =
            crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/cron").arg("restart"))
                .await
        {
            tracing::error!("Failed to restart cron at boot: {e}");
        }

        if let Err(e) = crate::captive::ensure_captive_portal_state().await {
            tracing::error!("captive portal setup failed: {e}");
        }

        // Follow the LAN neighbor table so IPv6 published-port rules track
        // device address changes (SLAAC devices renumber themselves). Runs for
        // the daemon's lifetime; harmless when no IPv6 forwards exist. On its
        // own single-threaded runtime because the reconcile it drives holds a
        // uciedit `Arena` across awaits (!Send), which `tokio::spawn` rejects.
        if let Err(e) = std::thread::Builder::new()
            .name("ipv6-tracker".into())
            .spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("ipv6-tracker runtime")
                    .block_on(crate::ipv6_tracker::run());
            })
        {
            tracing::error!("Failed to start IPv6 tracker: {e}");
        }
        if let Err(e) = std::thread::Builder::new()
            .name("usb-ports".into())
            .spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("usb-ports runtime")
                    .block_on(crate::usb_ports::run());
            })
        {
            tracing::error!("Failed to start USB port watcher: {e}");
        }
    }

    let tls = init_tls(ctx.clone()).await;

    if !setup_mode {
        // The IGD UUID derives from the initialized root CA.
        // Configure reply diversion before constructing the SNI demux.
        startos::net::transparent::set_divert_config(startos::net::transparent::DivertConfig {
            route_table: startos::net::transparent::DIVERT_TABLE,
            rule_priority: 49,
            masked_fwmark: true,
            manage_nft: false,
        })
        .map_err(|config| {
            Error::new(
                eyre!("SNI divert config rejected: {config:?}"),
                ErrorKind::Network,
            )
        })?;
        // Seeded before port control raises the gate.
        crate::http_redirect::seed("/etc/config".into()).await;
        let pc = crate::port_control::PortControl::new("/etc/config".into());
        if crate::port_control::PORT_CONTROL.set(pc.clone()).is_ok() {
            tokio::spawn(crate::port_control::run(pc));
        }
        let di = crate::dns_inject::DnsInject::new("/etc/config".into());
        if crate::dns_inject::DNS_INJECT.set(di.clone()).is_ok() {
            tokio::spawn(crate::dns_inject::run(di));
        }
    }

    if !setup_mode {
        tokio::spawn(crate::auth::reap_idle_keys(ctx.clone()));
        tokio::spawn(crate::ssh_sessions::run());
    }
    let proxy_client = ProxyClient(
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .build()
            .expect("failed to build proxy HTTP client"),
    );

    let cors = startos::middleware::cors::layer().expose_headers([crate::rpc::GIT_HASH_HEADER]);

    let app = crate::rpc::router(ctx.clone())
        // RPC continuation endpoint (binary I/O for backup/restore/diagnostics)
        .route(
            "/rest/rpc/{guid}",
            any(continuations::continuation_handler)
                .layer(DefaultBodyLimit::max(continuations::MAX_UPLOAD_BYTES)),
        )
        // HTTP/2 WebSockets upgrade with CONNECT.
        .route(
            &format!("{}/{{guid}}", RpcSessionUpgrader::PATH),
            any(ws_continuation_handler),
        )
        .route(
            &format!("{}/{{guid}}", RpcContinuations::LEGACY_WS_PATH),
            any(ws_continuation_handler),
        )
        .route("/api/setup/flash", post(setup_flash_handler))
        // Root CA download (no auth required)
        .route(
            "/static/local-root-ca.crt",
            get(|| async {
                root_ca_response(
                    ssl::read_root_ca_pem(),
                    "application/x-x509-ca-cert",
                    "startwrt-ca.crt",
                )
            }),
        )
        .route(
            "/static/local-root-ca.mobileconfig",
            get(|| async {
                root_ca_response(
                    ssl::read_root_ca_mobileconfig(),
                    "application/x-apple-aspen-config",
                    "startwrt-ca.mobileconfig",
                )
            }),
        )
        // LuCI reverse proxy — forwards to uhttpd on localhost:8080
        .route("/cgi-bin/{*rest}", any(luci_proxy::handler))
        .route("/luci-static/{*rest}", any(luci_proxy::handler))
        .route("/ubus", any(luci_proxy::handler))
        .route("/ubus/", any(luci_proxy::handler))
        .route("/ubus/{*rest}", any(luci_proxy::handler))
        // Convenience redirect: /luci → /cgi-bin/luci
        .route(
            "/luci",
            get(|| async { axum::response::Redirect::temporary("/cgi-bin/luci") }),
        )
        // Everything else serves the embedded web UI
        .fallback(axum::routing::any(serve_embedded))
        .layer(cors)
        .layer(Extension(continuations))
        .layer(Extension(proxy_client))
        .layer(Extension(ctx));

    let app = crate::captive::redirect_to_router(app);
    // Must stay outermost.
    let app = crate::http_redirect::redirect_public_http(app);

    // WAN-specific demux listeners require every wildcard listener to use SO_REUSEPORT.
    let http_addr = SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 0], 80));
    let http_listener = startos::net::utils::bind_tokio_listener_reuse_port(http_addr)
        .with_kind(ErrorKind::Network)?;
    tracing::info!("HTTP listening on {}", http_addr);

    let mut listeners: BTreeMap<WebserverListener, DynAccept> = BTreeMap::new();
    listeners.insert(WebserverListener::Http, http_listener.into_dyn());

    if let Some(tls) = tls {
        let https_addr = SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 0], 443));
        let https_listener = startos::net::utils::bind_tokio_listener_reuse_port(https_addr)
            .with_kind(ErrorKind::Network)?;
        tracing::info!("HTTPS listening on {}", https_addr);
        let tls = TlsListener::new(https_listener, tls);
        listeners.insert(WebserverListener::Https, tls.into_dyn());
    } else {
        tracing::warn!("HTTPS disabled — TLS setup failed, serving HTTP only");
    }

    let server = WebServer::new(Acceptor::new(listeners), app);

    // Wait for SIGTERM (procd) or SIGINT, then drain in-flight connections.
    let mut sigterm =
        tokio::signal::unix::signal(SignalKind::terminate()).with_kind(ErrorKind::Filesystem)?;
    let mut sigint =
        tokio::signal::unix::signal(SignalKind::interrupt()).with_kind(ErrorKind::Filesystem)?;
    if !setup_mode {
        crate::boot::record_running();
    }
    tokio::select! {
        _ = sigterm.recv() => tracing::info!("received SIGTERM, shutting down"),
        _ = sigint.recv() => tracing::info!("received SIGINT, shutting down"),
    }
    if !setup_mode {
        crate::boot::record_stop();
    }
    server.shutdown().await;

    Ok(())
}

pub fn main(_args: VecDeque<OsString>) {
    init_logging("startwrt-ctrld");
    tracing::info!("startwrt-ctrld starting (luci proxy v10)");

    // The start-os dep transitively enables rustls's `aws-lc-rs` feature
    // (via lettre), so rustls is compiled with both `ring` and `aws-lc-rs`
    // and its auto-select panics. Install ring explicitly before anything
    // touches rustls.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");

    let res = {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to initialize runtime");
        let res = rt.block_on(inner_main());
        rt.shutdown_timeout(Duration::from_secs(60));
        res
    };

    match res {
        Ok(_) => (),
        Err(e) => {
            eprintln!("{}", e);
            tracing::debug!("{:?}", e);
            std::process::exit(1)
        }
    }
}
