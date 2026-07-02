use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Response, header};
use axum::extract::DefaultBodyLimit;
use axum::routing::{any, get, post};
use axum::{Extension, Json, Router};
use crate::prelude::*;
use rpc_toolkit::Server;
use serde::Deserialize;
use startos::net::tls::TlsListener;
use startos::net::web_server::{Accept, Acceptor, DynAccept, MetadataVisitor, WebServer};
use std::future::ready;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::signal::unix::SignalKind;
use tokio::sync::mpsc;
use tracing::instrument;
use visit_rs::Visit;

use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

use crate::continuations::{self, RpcContinuations};
use crate::embedded_web::serve_embedded;
use crate::luci_proxy::{self, ProxyClient};
use crate::setup::{self, FlashMode, SetupEvent};
use crate::{init_logging, main_api, middleware::SessionAuth, ssl, ServerContext};

/// Shared state passed to HTTP handlers via Extension.
#[derive(Clone)]
struct AppState {
    /// Guards against concurrent flash operations.
    flash_in_progress: Arc<AtomicBool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlashParams {
    mode: FlashMode,
    password: String,
    /// Browser IANA timezone (e.g. "America/New_York") captured by the wizard.
    /// Carried into the fresh eMMC config since the wizard's live set-timezone
    /// only reaches the throwaway microSD overlay. Optional for older clients.
    #[serde(default)]
    timezone: Option<String>,
}

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

/// GET /ws/rpc/{guid} — WebSocket continuation handler (e.g., update progress).
async fn ws_continuation_handler(
    axum::extract::Path(guid): axum::extract::Path<String>,
    headers: axum::http::HeaderMap,
    ws: axum::extract::ws::WebSocketUpgrade,
    Extension(continuations): Extension<RpcContinuations>,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    if !crate::middleware::validate_session_from_headers(&headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let guid = crate::continuations::Guid::from_str(&guid);
    match continuations.get_ws_handler(&guid).await {
        Some(handler) => ws.on_upgrade(handler),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// POST /api/setup/flash — streams NDJSON progress events.
async fn setup_flash_handler(
    Extension(state): Extension<AppState>,
    Json(params): Json<FlashParams>,
) -> Response<Body> {
    // Prevent concurrent flash operations
    if state
        .flash_in_progress
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return ndjson_error("flash already in progress");
    }

    let flash_flag = state.flash_in_progress.clone();
    let (tx, rx) = mpsc::channel::<SetupEvent>(32);

    // run_setup_flash holds uciedit Arena across .await points, so its future
    // is !Send and cannot be `tokio::spawn`ed. Run it on a dedicated thread
    // with its own single-threaded runtime (which permits !Send futures).
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create setup-flash runtime");
        rt.block_on(async move {
            setup::run_setup_flash(params.mode, &params.password, params.timezone.as_deref(), &tx).await;
            flash_flag.store(false, Ordering::SeqCst);
        });
    });

    let stream = futures::stream::unfold(rx, |mut rx| async move {
        let event = rx.recv().await?;
        let mut json = serde_json::to_string(&event).unwrap();
        json.push('\n');
        Some((Ok::<_, std::convert::Infallible>(json), rx))
    });

    Response::builder()
        .header("content-type", "application/x-ndjson")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// GET /static/root-ca.crt — serves the Root CA certificate for download (no auth required).
async fn root_ca_handler() -> Response<Body> {
    match ssl::read_root_ca_pem() {
        Ok(pem) => Response::builder()
            .header(header::CONTENT_TYPE, "application/x-pem-file")
            .header(
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"startwrt-ca.crt\"",
            )
            .body(Body::from(pem))
            .unwrap(),
        Err(_) => Response::builder()
            .status(500)
            .body(Body::from("Root CA certificate not available"))
            .unwrap(),
    }
}

/// Initialize SSL certificates. Returns true if HTTPS is ready.
async fn init_ssl() -> bool {
    if let Err(e) = ssl::ensure_root_ca().await {
        tracing::error!("Root CA generation failed: {e}");
        return false;
    }

    if let Err(e) = ssl::ensure_intermediate_ca().await {
        tracing::error!("intermediate CA generation failed: {e}");
        return false;
    }

    let addrs = ssl::read_lan_addresses(std::path::Path::new("/etc/config")).await;
    if let Err(e) = ssl::ensure_server_cert(&addrs).await {
        tracing::error!("server cert generation failed: {e}");
        return false;
    }

    // Verify the cert files are valid by attempting to load them as TLS
    // materials. If they're corrupt, force-regenerate once.
    if ssl::TlsMaterials::load_from_disk().is_err() {
        tracing::warn!("TLS materials parse failed with existing certs, regenerating");
        if let Err(e) = ssl::regenerate_server_cert(&addrs).await {
            tracing::error!("cert regeneration failed: {e}");
            return false;
        }
        if ssl::TlsMaterials::load_from_disk().is_err() {
            tracing::error!("TLS materials parse still failing after regeneration");
            return false;
        }
    }

    true
}

/// Identifies which logical listener (HTTP or HTTPS) accepted a given
/// connection. Plumbed through `WebServer`'s metadata pipeline so request
/// extensions can inspect it if needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum WebserverListener {
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
    // Generate local auth cookie so CLI commands over SSH bypass session auth
    crate::auth::init_local_auth_cookie().await?;

    let setup_mode = setup::is_setup_mode().await;

    let continuations = RpcContinuations::new();
    let open_authed = crate::continuations::OpenAuthedContinuations::new();
    let app_state;

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
            if let Err(e) =
                crate::init::configure_wifi("/etc/config", wifi_password, Some(1)).await
            {
                tracing::error!("WiFi AP setup failed: {e}");
            }
            let _ = crate::run_quiet_async(
                tokio::process::Command::new("wifi").arg("reload"),
            )
            .await;

            // Enable captive portal only when WiFi AP can start
            if let Err(e) = crate::captive::enable_captive_portal().await {
                tracing::error!("captive portal setup failed: {e}");
            }
        }

        app_state = AppState {
            flash_in_progress: Arc::new(AtomicBool::new(false)),
        };
    } else {
        // Normal mode: restore WiFi + manage captive portal state

        // Record the outcome of a pending OTA update (if any) in the Activity
        // log now that the new firmware is up.
        crate::update::check_pending_update_marker();

        if let Err(e) = crate::init::restore_wifi_if_needed().await {
            tracing::error!("WiFi auto-restore failed: {e}");
        }

        // Reassert WiFi blackout if we rebooted inside an active window. Runs
        // *after* restore_wifi_if_needed (whose job is to restore missing AP
        // config, applying it via `wifi reload`) so our `wifi down` is the final
        // word and the radio ends down. Nothing after this point brings the radio
        // back up — bootstrap_admin_profile, apply_remote_access, the WAN-schedule
        // and captive-portal steps touch firewall/dnsmasq, not the wifi radios —
        // so the radio stays down until cron's window-end `wifi up`, by which time
        // restore has ensured the AP config is valid. (Note: netifd already
        // brought radios up earlier in boot — this closes the steady-state gap,
        // not the boot-window one; see reconcile_blackout_at_boot.)
        if let Err(e) = crate::wifi::reconcile_blackout_at_boot(&ServerContext::default()).await {
            tracing::error!("Failed to reconcile WiFi blackout at boot: {e}");
        }
        if let Err(e) = crate::profiles::bootstrap_admin_profile("/etc/config").await {
            tracing::error!("Admin profile bootstrap failed: {e}");
        }
        if let Err(e) = crate::system::apply_remote_access(ServerContext::default()).await {
            tracing::error!("Remote access rule apply failed: {e}");
        }
        // Apply WAN schedule enforcement (UCI firewall rules)
        if let Err(e) = crate::profiles::evaluate_and_apply_schedules(&ServerContext::default()).await {
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
        let schedule_ctx = ServerContext::default();
        if let Err(e) = crate::profiles::regenerate_schedule_crontab(&schedule_ctx).await {
            tracing::error!("Failed to regenerate WAN schedule crontab at boot: {e}");
        }
        if let Err(e) = crate::wifi::regenerate_blackout_crontab(&schedule_ctx).await {
            tracing::error!("Failed to regenerate WiFi blackout crontab at boot: {e}");
        }
        if let Err(e) = crate::run_quiet_async(
            tokio::process::Command::new("/etc/init.d/cron").arg("restart"),
        )
        .await
        {
            tracing::error!("Failed to restart cron at boot: {e}");
        }

        if let Err(e) = crate::captive::ensure_captive_portal_state().await {
            tracing::error!("captive portal setup failed: {e}");
        }

        app_state = AppState {
            flash_in_progress: Arc::new(AtomicBool::new(false)),
        };
    }

    // Initialize SSL: ensure Root CA and server cert exist
    let tls_ready = init_ssl().await;

    let ctx = ServerContext {
        continuations: continuations.clone(),
        open_authed_continuations: open_authed,
    };
    let handler = Server::new(move || ready(Ok(ctx.clone())), main_api())
        .middleware(SessionAuth::new());

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

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::mirror_request())
        .allow_methods(AllowMethods::mirror_request())
        .allow_headers(AllowHeaders::mirror_request())
        .allow_credentials(true);

    let app = Router::new()
        // RPC API at /rpc/v1 (matches frontend's RELATIVE_URL)
        .route("/rpc/v1", post(handler))
        // RPC continuation endpoint (binary I/O for backup/restore/diagnostics)
        .route("/rest/rpc/{guid}", any(continuations::continuation_handler)
            .layer(DefaultBodyLimit::max(10 * 1024 * 1024)))
        // WebSocket continuation endpoint (progress streaming for updates).
        // Registered with `any` (not `get`) so HTTP/2 CONNECT requests (RFC
        // 8441 extended CONNECT, used for WebSocket-over-h2) reach the
        // upgrade extractor — same reasoning as /api/logs below.
        .route("/ws/rpc/{guid}", any(ws_continuation_handler))
        // Streaming flash endpoint for setup wizard
        .route("/api/setup/flash", post(setup_flash_handler))
        // WebSocket endpoint for live log streaming. Registered with `any`
        // (not `get`) so HTTP/2 CONNECT requests (RFC 8441 extended CONNECT,
        // used for WebSocket-over-h2) reach the upgrade extractor instead of
        // being rejected with 405 by the method router.
        .route("/api/logs", any(crate::logs::logs_ws_handler))
        // Root CA download (no auth required)
        .route("/static/root-ca.crt", get(root_ca_handler))
        // LuCI reverse proxy — forwards to uhttpd on localhost:8080
        .route("/cgi-bin/{*rest}", any(luci_proxy::handler))
        .route("/luci-static/{*rest}", any(luci_proxy::handler))
        .route("/ubus", any(luci_proxy::handler))
        .route("/ubus/", any(luci_proxy::handler))
        .route("/ubus/{*rest}", any(luci_proxy::handler))
        // Convenience redirect: /luci → /cgi-bin/luci
        .route(
            "/luci",
            get(|| async {
                axum::response::Redirect::temporary("/cgi-bin/luci")
            }),
        )
        // Everything else serves the embedded web UI
        .fallback(axum::routing::any(serve_embedded))
        .layer(cors)
        .layer(Extension(continuations))
        .layer(Extension(proxy_client))
        .layer(Extension(app_state));

    // Build the listener map. start-os's `WebServer` provides the connection-
    // lifecycle defenses we used to need to hand-roll: TCP keepalive on each
    // accepted socket, HTTP/2 PING keepalives (25s/300s), accept retry with
    // backoff on transient errors (EMFILE/ENFILE), GracefulShutdown tracking
    // of in-flight connections, and RFC 8441 extended CONNECT for h2
    // WebSocket upgrades. `TlsListener` adds slow-loris-resistant handshake
    // timeouts (5s ClientHello, 15s full handshake) and runs each handshake
    // in a per-connection task so a stalled client cannot block accept.
    let http_addr = SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 0], 80));
    let http_listener = TcpListener::bind(http_addr)
        .await
        .with_kind(ErrorKind::Network)?;
    tracing::info!("HTTP listening on {}", http_addr);

    let mut listeners: BTreeMap<WebserverListener, DynAccept> = BTreeMap::new();
    listeners.insert(WebserverListener::Http, http_listener.into_dyn());

    if tls_ready {
        let materials = ssl::init_tls_materials()?;
        let https_addr = SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 0], 443));
        let https_listener = TcpListener::bind(https_addr)
            .await
            .with_kind(ErrorKind::Network)?;
        tracing::info!("HTTPS listening on {}", https_addr);
        let tls = TlsListener::new(https_listener, ssl::StaticTlsHandler::new(materials));
        listeners.insert(WebserverListener::Https, tls.into_dyn());
    } else {
        tracing::warn!("HTTPS disabled — TLS setup failed, serving HTTP only");
    }

    let server = WebServer::new(Acceptor::new(listeners), app);

    // Wait for SIGTERM (procd) or SIGINT, then drain in-flight connections.
    let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate())
        .with_kind(ErrorKind::Filesystem)?;
    let mut sigint = tokio::signal::unix::signal(SignalKind::interrupt())
        .with_kind(ErrorKind::Filesystem)?;
    tokio::select! {
        _ = sigterm.recv() => tracing::info!("received SIGTERM, shutting down"),
        _ = sigint.recv() => tracing::info!("received SIGINT, shutting down"),
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
