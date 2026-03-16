pub mod protocol;
pub mod session;
pub mod tools;

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::Request;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::MethodRouter;
use futures::future::ready;
use http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use http::{HeaderValue, StatusCode};
use patch_db::Dump;
use pin_project::pin_project;
use rpc_toolkit::Server;
use serde_json::Value as JsonValue;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt as _;

use crate::context::RpcContext;
use crate::middleware::auth::session::ValidSessionToken;
use crate::net::static_server::unauthorized;
use crate::prelude::*;
use imbl_value::InternedString;

use self::protocol::*;
use self::session::{
    SessionMap, create_session, remove_session, session_exists, sweep_stale_sessions_if_needed,
    take_notification_rx,
};

use self::tools::{ToolEntry, tool_registry};

/// Maximum request body size (1 MiB).
const MAX_BODY_SIZE: u64 = 1024 * 1024;

pub fn mcp_router(ctx: RpcContext) -> MethodRouter {
    let rpc_server = Server::new(
        {
            let ctx = ctx.clone();
            move || {
                let ctx = ctx.clone();
                ready(Ok(ctx))
            }
        },
        crate::main_api::<RpcContext>(),
    );

    let registry: Arc<HashMap<String, ToolEntry>> = Arc::new(tool_registry());
    let sessions: SessionMap = Arc::new(Default::default());

    // Background task: sweep stale MCP sessions every 30 seconds.
    // This task runs for the lifetime of the process. mcp_router() is called once
    // at server startup, so this does not leak.
    {
        let sessions = sessions.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                sweep_stale_sessions_if_needed(&sessions);
            }
        });
    }

    let post_handler = {
        let ctx = ctx.clone();
        let server = rpc_server.clone();
        let registry = registry.clone();
        let sessions = sessions.clone();
        axum::routing::post(move |request: Request| {
            let ctx = ctx.clone();
            let server = server.clone();
            let registry = registry.clone();
            let sessions = sessions.clone();
            async move { handle_post(ctx, server, &registry, &sessions, request).await }
        })
    };

    let get_handler = {
        let ctx = ctx.clone();
        let sessions = sessions.clone();
        axum::routing::get(move |request: Request| {
            let ctx = ctx.clone();
            let sessions = sessions.clone();
            async move { handle_get(ctx, &sessions, request).await }
        })
    };

    let delete_handler = {
        let sessions = sessions.clone();
        axum::routing::delete(move |request: Request| {
            let ctx = ctx.clone();
            let sessions = sessions.clone();
            async move { handle_delete(ctx, &sessions, request).await }
        })
    };

    let options_handler = axum::routing::options(|request: Request| async move {
        cors_preflight(&request)
    });

    post_handler
        .merge(get_handler)
        .merge(delete_handler)
        .merge(options_handler)
}

// =============================================================================
// POST /mcp — JSON-RPC requests (initialize, tools/*, resources/*)
// =============================================================================

async fn handle_post(
    ctx: RpcContext,
    server: Server<RpcContext>,
    registry: &HashMap<String, ToolEntry>,
    sessions: &SessionMap,
    request: Request,
) -> Response {
    // Extract origin for CORS before consuming the request
    let origin = extract_origin(&request);

    // Auth check — preserve the session hash for tools that need __Auth_session
    let session_hash = match ValidSessionToken::from_header(
        request.headers().get(http::header::COOKIE),
        &ctx,
    )
    .await
    {
        Ok(valid) => valid.0.hashed().clone(),
        Err(e) => return unauthorized(e, "/mcp"),
    };

    // Content-Type check
    if let Some(ct) = request.headers().get(CONTENT_TYPE) {
        if let Ok(ct_str) = ct.to_str() {
            if !ct_str.starts_with("application/json") {
                return bad_request("Content-Type must be application/json", origin.as_ref());
            }
        }
    }

    let session_id = extract_session_id(&request);

    // Pre-check Content-Length before reading the body (C2)
    if let Some(cl) = request.headers().get(CONTENT_LENGTH) {
        if let Ok(len) = cl.to_str().unwrap_or("0").parse::<u64>() {
            if len > MAX_BODY_SIZE {
                return bad_request("Request body too large (max 1 MiB)", origin.as_ref());
            }
        }
    }

    // Read body
    let body_bytes = {
        use http_body_util::BodyExt;
        let body = request.into_body();
        match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => return bad_request("Failed to read request body", origin.as_ref()),
        }
    };

    // Defense-in-depth: also check after read (for chunked transfers without Content-Length)
    if body_bytes.len() as u64 > MAX_BODY_SIZE {
        return bad_request("Request body too large (max 1 MiB)", origin.as_ref());
    }

    // Parse JSON-RPC request
    let mcp_req: McpRequest = match serde_json::from_slice(&body_bytes) {
        Ok(r) => r,
        Err(e) => {
            return json_response(
                &McpResponse::error(None, PARSE_ERROR, format!("Parse error: {e}"), None),
                None,
                origin.as_ref(),
            );
        }
    };

    if mcp_req.jsonrpc != "2.0" {
        return json_response(
            &McpResponse::error(
                mcp_req.id,
                INVALID_REQUEST,
                "jsonrpc must be \"2.0\"".into(),
                None,
            ),
            None,
            origin.as_ref(),
        );
    }

    // Session ID validation: all methods except initialize and notifications/initialized
    // require a valid session ID (N8)
    if mcp_req.method != "initialize" && mcp_req.method != "notifications/initialized" {
        match session_id.as_deref() {
            Some(id) if session_exists(sessions, id) => {} // valid
            Some(_) => {
                return json_response(
                    &McpResponse::error(
                        mcp_req.id,
                        INVALID_REQUEST,
                        "Invalid Mcp-Session-Id".into(),
                        None,
                    ),
                    None,
                    origin.as_ref(),
                );
            }
            None => {
                return json_response(
                    &McpResponse::error(
                        mcp_req.id,
                        INVALID_REQUEST,
                        "Missing Mcp-Session-Id header".into(),
                        None,
                    ),
                    None,
                    origin.as_ref(),
                );
            }
        }
    }

    // Dispatch by MCP method
    match mcp_req.method.as_str() {
        "initialize" => {
            let new_session_id = create_session(sessions);
            let response = handle_initialize(mcp_req.id, mcp_req.params);
            json_response(&response, Some(&new_session_id), origin.as_ref())
        }
        "notifications/initialized" => apply_cors(
            Response::builder()
                .status(StatusCode::ACCEPTED)
                .body(Body::empty())
                .unwrap(),
            origin.as_ref(),
        ),
        "tools/list" => json_response(
            &handle_tools_list(mcp_req.id, registry),
            session_id.as_deref(),
            origin.as_ref(),
        ),
        "tools/call" => {
            let response = handle_tools_call(
                &ctx,
                &server,
                registry,
                mcp_req.id,
                mcp_req.params,
                &session_hash,
            )
            .await;
            json_response(&response, session_id.as_deref(), origin.as_ref())
        }
        "resources/list" => json_response(
            &handle_resources_list(mcp_req.id),
            session_id.as_deref(),
            origin.as_ref(),
        ),
        "resources/read" => {
            let response = handle_resources_read(&ctx, mcp_req.id, mcp_req.params).await;
            json_response(&response, session_id.as_deref(), origin.as_ref())
        }
        "resources/subscribe" => {
            let response = handle_resources_subscribe(
                &ctx,
                sessions,
                session_id.as_deref(),
                mcp_req.id,
                mcp_req.params,
            )
            .await;
            json_response(&response, session_id.as_deref(), origin.as_ref())
        }
        "resources/unsubscribe" => {
            let response = handle_resources_unsubscribe(
                sessions,
                session_id.as_deref(),
                mcp_req.id,
                mcp_req.params,
            );
            json_response(&response, session_id.as_deref(), origin.as_ref())
        }
        _ => json_response(
            &McpResponse::error(
                mcp_req.id,
                METHOD_NOT_FOUND,
                format!("Unknown method: {}", mcp_req.method),
                None,
            ),
            session_id.as_deref(),
            origin.as_ref(),
        ),
    }
}

// =============================================================================
// GET /mcp — SSE stream for server→client notifications
// =============================================================================

async fn handle_get(ctx: RpcContext, sessions: &SessionMap, request: Request) -> Response {
    let origin = extract_origin(&request);

    // Auth check
    if let Err(e) =
        ValidSessionToken::from_header(request.headers().get(http::header::COOKIE), &ctx).await
    {
        return unauthorized(e, "/mcp");
    }

    let session_id = match extract_session_id(&request) {
        Some(id) => id,
        None => return bad_request("Missing Mcp-Session-Id header", origin.as_ref()),
    };

    // Take the notification receiver from the session
    let rx = match take_notification_rx(sessions, &session_id) {
        Some(rx) => rx,
        None => return bad_request("Invalid or already-connected session", origin.as_ref()),
    };

    let sessions_cleanup = sessions.clone();
    let session_id_cleanup = session_id.clone();

    // Convert bounded receiver to a Stream of SSE Events
    let stream = ReceiverStream::new(rx).map(move |notification| {
        let data = serde_json::to_string(&notification).unwrap_or_default();
        Ok::<_, Infallible>(Event::default().event("message").data(data))
    });

    // Wrap in a stream that cleans up the session on drop
    let stream = CleanupStream {
        inner: stream,
        sessions: Some(sessions_cleanup),
        session_id: Some(session_id_cleanup),
    };

    apply_cors(
        Sse::new(stream)
            .keep_alive(KeepAlive::default())
            .into_response(),
        origin.as_ref(),
    )
}

/// A stream wrapper that removes the MCP session when dropped (client disconnect).
#[pin_project(PinnedDrop)]
struct CleanupStream<S> {
    #[pin]
    inner: S,
    sessions: Option<SessionMap>,
    session_id: Option<String>,
}

#[pin_project::pinned_drop]
impl<S> PinnedDrop for CleanupStream<S> {
    fn drop(self: std::pin::Pin<&mut Self>) {
        let this = self.project();
        if let (Some(sessions), Some(id)) = (this.sessions.take(), this.session_id.take()) {
            tracing::info!(target: "mcp_audit", session_id = %id, "MCP SSE stream disconnected, cleaning up session");
            remove_session(&sessions, &id);
        }
    }
}

impl<S: futures::Stream> futures::Stream for CleanupStream<S> {
    type Item = S::Item;
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}

// =============================================================================
// DELETE /mcp — end session
// =============================================================================

async fn handle_delete(ctx: RpcContext, sessions: &SessionMap, request: Request) -> Response {
    let origin = extract_origin(&request);

    if let Err(e) =
        ValidSessionToken::from_header(request.headers().get(http::header::COOKIE), &ctx).await
    {
        return unauthorized(e, "/mcp");
    }

    if let Some(session_id) = extract_session_id(&request) {
        remove_session(sessions, &session_id);
    }

    apply_cors(
        Response::builder()
            .status(StatusCode::OK)
            .body(Body::empty())
            .unwrap(),
        origin.as_ref(),
    )
}

// =============================================================================
// Method handlers
// =============================================================================

fn handle_initialize(id: Option<JsonValue>, params: Option<JsonValue>) -> McpResponse {
    if let Some(params) = params {
        match serde_json::from_value::<InitializeParams>(params) {
            Ok(p) => {
                // Validate protocol version (N6)
                if p.protocol_version != PROTOCOL_VERSION {
                    return McpResponse::error(
                        id,
                        INVALID_PARAMS,
                        format!(
                            "Unsupported protocol version: {}. Server supports: {}",
                            p.protocol_version, PROTOCOL_VERSION
                        ),
                        None,
                    );
                }
                if let Some(ref info) = p.client_info {
                    tracing::info!(
                        target: "mcp_audit",
                        client_name = %info.name,
                        client_version = info.version.as_deref().unwrap_or("unknown"),
                        "MCP client connected"
                    );
                }
            }
            Err(e) => {
                return McpResponse::error(id, INVALID_PARAMS, format!("{e}"), None);
            }
        }
    }

    McpResponse::ok(
        id,
        serde_json::to_value(&InitializeResult {
            protocol_version: PROTOCOL_VERSION,
            capabilities: ServerCapabilities {
                tools: ToolsCapability {},
                resources: ResourcesCapability { subscribe: true },
            },
            server_info: ServerInfo {
                name: "StartOS",
                version: env!("CARGO_PKG_VERSION").into(),
            },
        })
        .unwrap(),
    )
}

fn handle_tools_list(id: Option<JsonValue>, registry: &HashMap<String, ToolEntry>) -> McpResponse {
    let tools: Vec<_> = registry.values().map(|t| t.definition.clone()).collect();
    McpResponse::ok(
        id,
        serde_json::to_value(&ToolsListResult { tools }).unwrap(),
    )
}

async fn handle_tools_call(
    ctx: &RpcContext,
    server: &Server<RpcContext>,
    registry: &HashMap<String, ToolEntry>,
    id: Option<JsonValue>,
    params: Option<JsonValue>,
    session_hash: &InternedString,
) -> McpResponse {
    let call_params: ToolsCallParams = match params.map(serde_json::from_value).transpose() {
        Ok(Some(p)) => p,
        Ok(None) => {
            return McpResponse::error(id, INVALID_PARAMS, "Missing params".into(), None)
        }
        Err(e) => return McpResponse::error(id, INVALID_PARAMS, format!("{e}"), None),
    };

    let tool = match registry.get(&call_params.name) {
        Some(t) => t,
        None => {
            return McpResponse::error(
                id,
                INVALID_PARAMS,
                format!("Unknown tool: {}", call_params.name),
                None,
            )
        }
    };

    let start = Instant::now();

    tracing::info!(
        target: "mcp_audit",
        tool = %call_params.name,
        rpc_method = %tool.rpc_method,
        "MCP tool invoked"
    );

    let mut imbl_params = match imbl_value::to_value(&call_params.arguments) {
        Ok(v) => v,
        Err(e) => {
            return McpResponse::error(
                id,
                INVALID_PARAMS,
                format!("Failed to convert params: {e}"),
                None,
            );
        }
    };

    // Special-case: shell execution (no RPC handler for these)
    if tool.rpc_method == "__shell__" {
        return handle_shell_exec(id, call_params.arguments, start).await;
    }
    if tool.rpc_method == "__package_shell__" {
        return handle_package_shell_exec(ctx, id, call_params.arguments, start).await;
    }

    // Inject __Auth_session for handlers that need it
    if tool.needs_session {
        if let imbl_value::Value::Object(ref mut map) = imbl_params {
            map.insert(
                imbl_value::InternedString::intern("__Auth_session"),
                imbl_value::to_value(session_hash).unwrap(),
            );
        }
    }

    match server.handle_command(tool.rpc_method, imbl_params).await {
        Ok(result) => {
            if tool.sync_db {
                let seq = ctx.db.sequence().await;
                ctx.sync_db.send_replace(seq);
            }

            let duration = start.elapsed();
            tracing::info!(
                target: "mcp_audit",
                tool = %call_params.name,
                duration_ms = duration.as_millis() as u64,
                "MCP tool completed"
            );

            let json_result = serde_json::to_value(&result).unwrap_or(JsonValue::Null);
            let text = serde_json::to_string_pretty(&json_result).unwrap_or_default();

            McpResponse::ok(
                id,
                serde_json::to_value(&ToolsCallResult {
                    content: vec![ContentBlock::Text { text }],
                    is_error: None,
                })
                .unwrap(),
            )
        }
        Err(rpc_err) => {
            let duration = start.elapsed();
            tracing::warn!(
                target: "mcp_audit",
                tool = %call_params.name,
                error_code = rpc_err.code,
                error_msg = %rpc_err.message,
                duration_ms = duration.as_millis() as u64,
                "MCP tool call failed"
            );

            McpResponse::ok(
                id,
                serde_json::to_value(&ToolsCallResult {
                    content: vec![ContentBlock::Text {
                        text: format!("Error ({}): {}", rpc_err.code, rpc_err.message),
                    }],
                    is_error: Some(true),
                })
                .unwrap(),
            )
        }
    }
}

async fn handle_shell_exec(
    id: Option<JsonValue>,
    arguments: JsonValue,
    start: Instant,
) -> McpResponse {
    let command = match arguments.get("command").and_then(|v| v.as_str()) {
        Some(c) => c.to_string(),
        None => {
            return McpResponse::error(
                id,
                INVALID_PARAMS,
                "Missing required parameter: command".into(),
                None,
            )
        }
    };

    let timeout_secs = arguments
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(30)
        .min(300);

    tracing::info!(
        target: "mcp_audit",
        command = %command,
        timeout_secs = timeout_secs,
        "MCP shell command executing"
    );

    let result = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        tokio::process::Command::new("/bin/bash")
            .arg("-c")
            .arg(&command)
            .kill_on_drop(true) // N10: ensure timed-out processes are killed
            .output(),
    )
    .await;

    let duration = start.elapsed();

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let exit_code = output.status.code().unwrap_or(-1);

            tracing::info!(
                target: "mcp_audit",
                command = %command,
                exit_code = exit_code,
                stdout_len = output.stdout.len(),
                stderr_len = output.stderr.len(),
                duration_ms = duration.as_millis() as u64,
                "MCP shell command completed"
            );

            let mut text = String::new();
            if !stdout.is_empty() {
                text.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str("[stderr]\n");
                text.push_str(&stderr);
            }
            if text.is_empty() {
                text.push_str("(no output)");
            }
            text.push_str(&format!("\n[exit code: {}]", exit_code));

            McpResponse::ok(
                id,
                serde_json::to_value(&ToolsCallResult {
                    content: vec![ContentBlock::Text { text }],
                    is_error: if exit_code != 0 { Some(true) } else { None },
                })
                .unwrap(),
            )
        }
        Ok(Err(e)) => {
            tracing::warn!(
                target: "mcp_audit",
                command = %command,
                error = %e,
                "MCP shell command failed to execute"
            );
            McpResponse::ok(
                id,
                serde_json::to_value(&ToolsCallResult {
                    content: vec![ContentBlock::Text {
                        text: format!("Failed to execute command: {e}"),
                    }],
                    is_error: Some(true),
                })
                .unwrap(),
            )
        }
        Err(_) => {
            tracing::warn!(
                target: "mcp_audit",
                command = %command,
                timeout_secs = timeout_secs,
                "MCP shell command timed out"
            );
            McpResponse::ok(
                id,
                serde_json::to_value(&ToolsCallResult {
                    content: vec![ContentBlock::Text {
                        text: format!("Command timed out after {timeout_secs} seconds"),
                    }],
                    is_error: Some(true),
                })
                .unwrap(),
            )
        }
    }
}

async fn handle_package_shell_exec(
    ctx: &RpcContext,
    id: Option<JsonValue>,
    arguments: JsonValue,
    start: Instant,
) -> McpResponse {
    let pkg_id_str = match arguments.get("id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => {
            return McpResponse::error(
                id,
                INVALID_PARAMS,
                "Missing required parameter: id".into(),
                None,
            )
        }
    };

    let command = match arguments.get("command").and_then(|v| v.as_str()) {
        Some(c) => c.to_string(),
        None => {
            return McpResponse::error(
                id,
                INVALID_PARAMS,
                "Missing required parameter: command".into(),
                None,
            )
        }
    };

    let timeout_secs = arguments
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(30)
        .min(300);

    let subcontainer_filter = arguments
        .get("subcontainer")
        .and_then(|v| v.as_str())
        .map(InternedString::intern);
    let name_filter = arguments
        .get("name")
        .and_then(|v| v.as_str())
        .map(InternedString::intern);

    let pkg_id: crate::PackageId = match pkg_id_str.parse() {
        Ok(id) => id,
        Err(e) => {
            return McpResponse::error(
                id,
                INVALID_PARAMS,
                format!("Invalid package ID: {e}"),
                None,
            )
        }
    };

    // Resolve the subcontainer using the shared logic from service/mod.rs
    let resolved = {
        let service = ctx.services.get(&pkg_id).await;
        let service_ref = match service.as_ref() {
            Some(s) => s,
            None => {
                return McpResponse::ok(
                    id,
                    serde_json::to_value(&ToolsCallResult {
                        content: vec![ContentBlock::Text {
                            text: format!("Package '{pkg_id}' is not installed or not running"),
                        }],
                        is_error: Some(true),
                    })
                    .unwrap(),
                )
            }
        };
        match service_ref
            .resolve_subcontainer(subcontainer_filter, name_filter, None, None)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return McpResponse::ok(
                    id,
                    serde_json::to_value(&ToolsCallResult {
                        content: vec![ContentBlock::Text {
                            text: format!("{e}"),
                        }],
                        is_error: Some(true),
                    })
                    .unwrap(),
                )
            }
        }
    };

    tracing::info!(
        target: "mcp_audit",
        package = %pkg_id_str,
        command = %command,
        subcontainer = %resolved.subcontainer_id,
        timeout_secs = timeout_secs,
        "MCP package shell command executing"
    );

    // Build the command using shared logic (kill_on_drop already set by build_subcontainer_command)
    let mut cmd = crate::service::Service::build_subcontainer_command(
        &resolved,
        &["/bin/sh", "-c", &command],
    );

    let result = tokio::time::timeout(Duration::from_secs(timeout_secs), cmd.output()).await;

    let duration = start.elapsed();

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let exit_code = output.status.code().unwrap_or(-1);

            tracing::info!(
                target: "mcp_audit",
                package = %pkg_id_str,
                command = %command,
                exit_code = exit_code,
                duration_ms = duration.as_millis() as u64,
                "MCP package shell command completed"
            );

            let mut text = String::new();
            if !stdout.is_empty() {
                text.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str("[stderr]\n");
                text.push_str(&stderr);
            }
            if text.is_empty() {
                text.push_str("(no output)");
            }
            text.push_str(&format!("\n[exit code: {}]", exit_code));

            McpResponse::ok(
                id,
                serde_json::to_value(&ToolsCallResult {
                    content: vec![ContentBlock::Text { text }],
                    is_error: if exit_code != 0 { Some(true) } else { None },
                })
                .unwrap(),
            )
        }
        Ok(Err(e)) => McpResponse::ok(
            id,
            serde_json::to_value(&ToolsCallResult {
                content: vec![ContentBlock::Text {
                    text: format!("Failed to execute in container: {e}"),
                }],
                is_error: Some(true),
            })
            .unwrap(),
        ),
        Err(_) => McpResponse::ok(
            id,
            serde_json::to_value(&ToolsCallResult {
                content: vec![ContentBlock::Text {
                    text: format!("Command timed out after {timeout_secs} seconds"),
                }],
                is_error: Some(true),
            })
            .unwrap(),
        ),
    }
}

fn handle_resources_list(id: Option<JsonValue>) -> McpResponse {
    let resources = vec![
        ResourceDefinition {
            uri: "startos:///public".into(),
            name: "System State".into(),
            description: "The full public database tree. Contains server info, package data, \
                network configuration, and all other system state. Subscribe to this for \
                real-time updates."
                .into(),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: "startos:///public/serverInfo".into(),
            name: "Server Info".into(),
            description: "Server metadata: hostname, version, network addresses, resource usage."
                .into(),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: "startos:///public/packageData".into(),
            name: "Package Data".into(),
            description: "All installed packages with their status, version, and configuration."
                .into(),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: "startos:///mcp/system-prompt".into(),
            name: "System Prompt".into(),
            description: "A curated system context for AI assistants: server identity, version, \
                and installed packages summary."
                .into(),
            mime_type: Some("text/plain".into()),
        },
    ];

    McpResponse::ok(
        id,
        serde_json::to_value(&ResourcesListResult { resources }).unwrap(),
    )
}

async fn handle_resources_read(
    ctx: &RpcContext,
    id: Option<JsonValue>,
    params: Option<JsonValue>,
) -> McpResponse {
    let read_params: ResourcesReadParams = match params.map(serde_json::from_value).transpose() {
        Ok(Some(p)) => p,
        Ok(None) => {
            return McpResponse::error(id, INVALID_PARAMS, "Missing params".into(), None)
        }
        Err(e) => return McpResponse::error(id, INVALID_PARAMS, format!("{e}"), None),
    };

    // Validate resource URI (N7)
    if let Err(msg) = validate_resource_uri(&read_params.uri) {
        return McpResponse::error(id, INVALID_PARAMS, msg, None);
    }

    // Special-case: system prompt resource (S-B)
    if read_params.uri == "startos:///mcp/system-prompt" {
        let prompt = build_system_prompt(ctx).await;
        return McpResponse::ok(
            id,
            serde_json::to_value(&ResourcesReadResult {
                contents: vec![ResourceContent {
                    uri: read_params.uri,
                    mime_type: Some("text/plain".into()),
                    text: Some(prompt),
                }],
            })
            .unwrap(),
        );
    }

    let pointer = match read_params
        .uri
        .strip_prefix("startos://")
        .and_then(|p| p.parse::<patch_db::json_ptr::JsonPointer>().ok())
    {
        Some(p) => p,
        None => {
            return McpResponse::error(
                id,
                INVALID_PARAMS,
                format!("Invalid resource URI: {}", read_params.uri),
                None,
            )
        }
    };

    let dump: Dump = ctx.db.dump(&pointer).await;
    let json_value = serde_json::to_value(&dump.value).unwrap_or(JsonValue::Null);
    let text = serde_json::to_string_pretty(&json_value).unwrap_or_default();

    McpResponse::ok(
        id,
        serde_json::to_value(&ResourcesReadResult {
            contents: vec![ResourceContent {
                uri: read_params.uri,
                mime_type: Some("application/json".into()),
                text: Some(text),
            }],
        })
        .unwrap(),
    )
}

async fn handle_resources_subscribe(
    ctx: &RpcContext,
    sessions: &SessionMap,
    session_id: Option<&str>,
    id: Option<JsonValue>,
    params: Option<JsonValue>,
) -> McpResponse {
    let session_id = match session_id {
        Some(s) => s,
        None => {
            return McpResponse::error(
                id,
                INVALID_REQUEST,
                "Missing Mcp-Session-Id header".into(),
                None,
            )
        }
    };

    let sub_params: ResourcesSubscribeParams =
        match params.map(serde_json::from_value).transpose() {
            Ok(Some(p)) => p,
            Ok(None) => {
                return McpResponse::error(id, INVALID_PARAMS, "Missing params".into(), None)
            }
            Err(e) => return McpResponse::error(id, INVALID_PARAMS, format!("{e}"), None),
        };

    // Validate resource URI (N7)
    if let Err(msg) = validate_resource_uri(&sub_params.uri) {
        return McpResponse::error(id, INVALID_PARAMS, msg, None);
    }

    if let Err(e) =
        session::subscribe_resource(ctx, sessions, session_id, &sub_params.uri).await
    {
        return McpResponse::error(id, INTERNAL_ERROR, format!("{e}"), None);
    }

    tracing::info!(
        target: "mcp_audit",
        uri = %sub_params.uri,
        session_id = %session_id,
        "MCP resource subscription started"
    );

    McpResponse::ok(id, serde_json::json!({}))
}

fn handle_resources_unsubscribe(
    sessions: &SessionMap,
    session_id: Option<&str>,
    id: Option<JsonValue>,
    params: Option<JsonValue>,
) -> McpResponse {
    let session_id = match session_id {
        Some(s) => s,
        None => {
            return McpResponse::error(
                id,
                INVALID_REQUEST,
                "Missing Mcp-Session-Id header".into(),
                None,
            )
        }
    };

    let unsub_params: ResourcesUnsubscribeParams =
        match params.map(serde_json::from_value).transpose() {
            Ok(Some(p)) => p,
            Ok(None) => {
                return McpResponse::error(id, INVALID_PARAMS, "Missing params".into(), None)
            }
            Err(e) => return McpResponse::error(id, INVALID_PARAMS, format!("{e}"), None),
        };

    session::unsubscribe_resource(sessions, session_id, &unsub_params.uri);

    tracing::info!(
        target: "mcp_audit",
        uri = %unsub_params.uri,
        session_id = %session_id,
        "MCP resource subscription stopped"
    );

    McpResponse::ok(id, serde_json::json!({}))
}

// =============================================================================
// Helpers
// =============================================================================

/// Extract the Origin header value from a request for CORS reflection.
fn extract_origin(request: &Request) -> Option<HeaderValue> {
    request.headers().get("Origin").cloned()
}

fn extract_session_id(request: &Request) -> Option<String> {
    request
        .headers()
        .get("Mcp-Session-Id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Validate that a resource URI is within the allowed set.
/// Only `startos:///public/**` subtrees and the special `startos:///mcp/system-prompt`
/// resource are accessible.
fn validate_resource_uri(uri: &str) -> Result<(), String> {
    match uri.strip_prefix("startos://") {
        Some("/mcp/system-prompt") => Ok(()),
        Some(path) if path.starts_with("/public") => Ok(()),
        Some(path) => Err(format!(
            "Access denied: resource URI must start with startos:///public, got path: {path}"
        )),
        None => Err(format!(
            "Invalid resource URI: must start with startos://, got: {uri}"
        )),
    }
}

/// Build a curated system prompt for AI assistants (S-B).
async fn build_system_prompt(ctx: &RpcContext) -> String {
    let dump = ctx
        .db
        .dump(&"/public/serverInfo".parse().unwrap())
        .await;
    let server_info = serde_json::to_string_pretty(&dump.value).unwrap_or_default();
    format!(
        "You are managing a StartOS server.\n\
         \n\
         Server info:\n{server_info}\n\
         \n\
         StartOS version: {}\n\
         \n\
         Use the available MCP tools to inspect, configure, and manage this server. \
         Always confirm destructive operations with the user before executing them.",
        env!("CARGO_PKG_VERSION"),
    )
}

fn json_response(
    response: &McpResponse,
    session_id: Option<&str>,
    origin: Option<&HeaderValue>,
) -> Response {
    let body = serde_json::to_vec(response).unwrap_or_else(|_| {
        br#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"Internal error"}}"#.to_vec()
    });
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/json");
    if let Some(id) = session_id {
        builder = builder.header("Mcp-Session-Id", id);
    }
    apply_cors(builder.body(Body::from(body)).unwrap(), origin)
}

fn bad_request(msg: &str, origin: Option<&HeaderValue>) -> Response {
    apply_cors(
        Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header(CONTENT_TYPE, "text/plain")
            .body(Body::from(msg.to_string()))
            .unwrap(),
        origin,
    )
}

/// Apply CORS headers to any response. Reflects the origin when present,
/// falls back to `*` when absent (matching the rpc-toolkit Cors middleware).
fn apply_cors(mut response: Response, origin: Option<&HeaderValue>) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        "Access-Control-Allow-Origin",
        origin
            .cloned()
            .unwrap_or_else(|| HeaderValue::from_static("*")),
    );
    headers.insert(
        "Access-Control-Allow-Credentials",
        HeaderValue::from_static("true"),
    );
    headers.insert(
        "Access-Control-Expose-Headers",
        HeaderValue::from_static("Mcp-Session-Id"),
    );
    response
}

fn cors_preflight(request: &Request) -> Response {
    let origin = request
        .headers()
        .get("Origin")
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("*"));
    let methods = request
        .headers()
        .get("Access-Control-Request-Method")
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("POST, GET, DELETE, OPTIONS"));
    let req_headers = request
        .headers()
        .get("Access-Control-Request-Headers")
        .cloned()
        .unwrap_or_else(|| {
            HeaderValue::from_static("Content-Type, Mcp-Session-Id, Cookie")
        });

    Response::builder()
        .status(StatusCode::OK)
        .header("Access-Control-Allow-Origin", origin)
        .header("Access-Control-Allow-Methods", methods)
        .header("Access-Control-Allow-Headers", req_headers)
        .header("Access-Control-Allow-Credentials", "true")
        .header("Access-Control-Max-Age", "86400")
        .body(Body::empty())
        .unwrap()
}
