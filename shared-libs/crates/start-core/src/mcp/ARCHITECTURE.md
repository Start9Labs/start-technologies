# MCP Server Architecture

The Model Context Protocol server embedded in StartOS (`core/src/mcp/`).

## Transport: Streamable HTTP (MCP 2025-03-26)

The server implements the **Streamable HTTP** transport from the MCP spec, not the older stdio or SSE-only transports. A single route (`/mcp`) handles all three HTTP methods:

| Method      | Purpose                                                                          |
| ----------- | -------------------------------------------------------------------------------- |
| **POST**    | JSON-RPC 2.0 requests from client (initialize, tools/call, resources/read, etc.) |
| **GET**     | Opens an SSE stream for server→client notifications (resource change events)     |
| **DELETE**  | Explicitly ends a session                                                        |
| **OPTIONS** | CORS preflight                                                                   |

A discovery endpoint at `/.well-known/mcp` returns `{"mcp_endpoint":"/mcp"}`.

## Authentication

Every HTTP method (POST, GET, DELETE) validates the caller's session cookie via `ValidSessionToken::from_header` before processing. This reuses the same auth infrastructure as the main StartOS web UI — MCP clients must present a valid session cookie obtained through the normal login flow. Unauthenticated requests get a 401.

## Session Lifecycle

1. **Create**: Client sends `initialize` via POST. Server generates a UUID session ID, creates an `McpSession` with a bounded mpsc channel (256 messages), and returns the ID in the `Mcp-Session-Id` response header.

2. **Connect SSE**: Client opens a GET with the session ID header. The server takes the receiver half of the notification channel (`take_notification_rx`) and streams it as SSE events. Only one GET connection per session is allowed (the rx is moved, not cloned).

3. **Use**: Client sends tool calls, resource reads, subscriptions via POST. All POST requests must include a valid session ID header — the server validates it against the session map before processing.

4. **Teardown**: Three paths:
   - Client sends DELETE -> session is removed, subscription tasks are aborted.
   - SSE stream disconnects -> `CleanupStream`'s `PinnedDrop` impl removes the session.
   - Session is never connected -> background sweep task (every 30s) removes sessions older than 60s that never had a GET stream attached.

## Module Structure

```
core/src/mcp/
├── mod.rs        — HTTP handlers, routing, MCP method dispatch, shell execution, CORS
├── protocol.rs   — JSON-RPC 2.0 types, MCP request/response structs, error codes
├── session.rs    — Session map, create/remove/sweep, resource subscriptions with debounce
└── tools.rs      — Tool registry (67 tools), HashMap<String, ToolEntry> mapping names → RPC methods + schemas
```

## Tool Dispatch

`tool_registry()` returns a `HashMap<String, ToolEntry>`, each mapping:

- An MCP tool name (e.g. `"package.start"`)
- A JSON Schema for input validation (sent to clients via `tools/list`)
- A backing RPC method name (usually identical to the tool name)
- Flags: `sync_db` (whether to flush DB sequence after success), `needs_session` (whether to inject `__Auth_session`)

When `tools/call` arrives:

1. Look up the tool by name via HashMap O(1) lookup.
2. Convert arguments from `serde_json::Value` to `imbl_value::Value`.
3. **Special-case**: If `rpc_method` is `"__shell__"` or `"__package_shell__"`, dispatch to `handle_shell_exec` / `handle_package_shell_exec` directly (no RPC handler). Both set `kill_on_drop(true)` to ensure timed-out processes are terminated.
4. Otherwise, optionally inject `__Auth_session` into params, then call `server.handle_command(rpc_method, params)`.
5. On success: if `sync_db` is true, flush the DB sequence. Return the result pretty-printed as a text content block.
6. On error: return the error as a text content block with `is_error: true`, using `McpResponse::ok` (MCP spec: tool errors are results, not JSON-RPC errors).

## Shell Execution

Two shell tools bypass the RPC layer entirely:

- **`system.shell`** (`__shell__`): Runs `/bin/bash -c <command>` on the host with `kill_on_drop(true)`. 30s default timeout, 300s max.
- **`package.shell`** (`__package_shell__`): Resolves the target package's subcontainer via `Service::resolve_subcontainer`, then runs `/bin/sh -c <command>` inside it via `lxc-attach` (also `kill_on_drop(true)`). Same timeout behavior.

## Resource Subscriptions

Four resources are exposed:

- `startos:///public` — full public DB tree
- `startos:///public/serverInfo` — server metadata
- `startos:///public/packageData` — installed packages
- `startos:///mcp/system-prompt` — curated AI assistant context (text/plain)

Resource URIs are validated to only allow `/public/**` subtrees and the special `/mcp/system-prompt` path. Attempts to access non-public paths (e.g. `startos:///private/...`) are rejected.

`resources/read` parses the URI into a `JsonPointer`, calls `ctx.db.dump(&pointer)`, and returns the JSON. The system prompt resource is handled as a special case, returning server info and version.

`resources/subscribe` creates a `DbSubscriber` that watches the patch-db for changes at the given pointer. Changes are **debounced** (500ms window): the subscriber collects multiple revisions and merges their `DiffPatch`es before sending a single `notifications/resources/updated` notification over the SSE channel. The subscription task runs as a spawned tokio task; its `JoinHandle` is stored in the session so it can be aborted on unsubscribe or session teardown. Re-subscribing to the same URI aborts the prior subscription first.

## CORS

- Preflight (OPTIONS): reflects the request's `Origin`, `Access-Control-Request-Method`, and `Access-Control-Request-Headers` back. Sets `Allow-Credentials: true` and caches for 24h.
- Normal responses (`apply_cors`): reflects the request's `Origin` header when present, falls back to `*` when absent. Exposes the `Mcp-Session-Id` header. This matches the behavior of the rpc-toolkit `Cors` middleware used by the main UI.
- CORS headers are applied to all response types: POST JSON-RPC, GET SSE, DELETE, and error responses.

## Body Size Limits

POST request bodies are limited to 1 MiB:

1. `Content-Length` header is checked **before** reading the body (rejects oversized requests immediately).
2. After reading, the actual body size is re-checked as defense-in-depth for chunked transfers that lack `Content-Length`.
