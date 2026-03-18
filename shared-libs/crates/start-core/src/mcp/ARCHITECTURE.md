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
└── tools.rs      — Tool registry (89 tools), HashMap<String, ToolEntry> mapping names → RPC methods + schemas
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
3. **Special-case**: If `rpc_method` is `"__package_shell__"`, dispatch to `handle_package_shell_exec` directly (no RPC handler). Sets `kill_on_drop(true)` to ensure timed-out processes are terminated.
4. Otherwise, optionally inject `__Auth_session` into params, then call `server.handle_command(rpc_method, params)`.
5. On success: if `sync_db` is true, flush the DB sequence. Return the result pretty-printed as a text content block.
6. On error: return the error as a text content block with `is_error: true`, using `McpResponse::ok` (MCP spec: tool errors are results, not JSON-RPC errors).

## Shell Execution

One shell tool bypasses the RPC layer entirely:

- **`package.shell`** (`__package_shell__`): Resolves the target package's subcontainer via `Service::resolve_subcontainer`, then runs `/bin/sh -c <command>` inside it via `lxc-attach` with `kill_on_drop(true)`. 30s default timeout, 300s max. Host-level shell access (`system.shell`) is intentionally excluded — agents operate within package containers only.

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

## Excluded RPC Methods

Of the ~195 RPC methods registered in the StartOS backend, 88 are exposed as MCP tools (plus 1 MCP-only tool: `package.shell`). The remaining 105 are excluded for the following reasons.

### Wrong context — Setup / Init / Diagnostic modes

These methods belong to the setup wizard, initial install, or diagnostic recovery mode — entirely different server states that are not reachable during normal operation when the MCP server is running.

| Method | Reason |
|--------|--------|
| `setup.*` (15 methods) | Setup wizard only runs during initial OS configuration |
| `init.*` (14 methods) | Initial disk/install flow, not reachable post-boot |
| `diagnostic.*` (7 methods) | Diagnostic recovery mode, separate HTTP server |
| `flash-os` | Bare-metal OS flashing |

### Wrong context — CLI / Developer tooling

These are developer-facing commands invoked via the CLI, not the web UI. They operate on local files or require local filesystem access.

| Method | Reason |
|--------|--------|
| `s9pk.*` (9 methods) | Package building/inspection — CLI tool for developers |
| `util.b3sum` | BLAKE3 checksum utility — CLI helper |
| `init-key`, `pubkey` | Key management — CLI operations |

### Wrong context — Registry administration

These manage the package registry (a separate server-side component), not the local StartOS instance.

| Method | Reason |
|--------|--------|
| `registry.*` (20 methods) | Registry server administration, not local device management |

### Wrong context — Tunnel management

These configure the Start9 tunnel service, which has its own management interface.

| Method | Reason |
|--------|--------|
| `tunnel.*` (12 methods) | Tunnel server management, separate from local OS control |

### Replaced by MCP-native functionality

| Method | Reason |
|--------|--------|
| `db.subscribe` | Replaced by MCP `resources/subscribe` which calls `ctx.db.dump_and_sub()` directly with 500ms debounce |
| `server.metrics.follow` | WebSocket continuation for streaming metrics — use `server.metrics` (polling) instead |

### Requires middleware injection not available via MCP dispatch

| Method | Reason |
|--------|--------|
| `package.sideload` | Requires multipart file upload via REST continuation, not JSON-RPC params. Use `package.sideload-by-url` MCP tool (backed by `package.sideload-url` RPC) which accepts a URL instead |

### Security — host-level shell access excluded

| Method | Reason |
|--------|--------|
| `system.shell` | Arbitrary host-level command execution is too broad a privilege for MCP agents. Agents can execute commands inside package subcontainers via `package.shell`, which is scoped to the service's filesystem and processes |

### Auth methods — intentionally excluded

| Method | Reason |
|--------|--------|
| `auth.login` | MCP clients authenticate via session cookie before reaching the MCP server — login is a prerequisite, not an MCP operation |
| `auth.logout` | Logging out the session that the MCP client is using would break the connection. Clients should disconnect (DELETE) instead |

### Internal / low-value

| Method | Reason |
|--------|--------|
| `echo` | Debug echo — no agent value |
| `git-info` | Build metadata — available via `server.device-info` |
| `state` | Returns server state enum — available via DB resources |
| `notification.create` | Internal: creates notifications from backend code, not user-facing |
| `db.apply` | Bulk DB mutation — CLI-specific params (`apply_receipt`) not suitable for MCP |
| `kiosk.set` | Kiosk mode toggle — physical display setting, not agent-relevant |

### Deep host/binding management — not yet exposed

These methods manage individual domain bindings and address assignments at a granular level. The list (`server.host.address.list`, `server.host.binding.list`, `package.host.list`) and read operations are exposed; the mutation operations below are deferred until agent workflows demonstrate a need.

| Method | Reason |
|--------|--------|
| `server.host.address.domain.public.add` | Granular domain management — deferred |
| `server.host.address.domain.public.remove` | Granular domain management — deferred |
| `server.host.address.domain.private.add` | Granular domain management — deferred |
| `server.host.address.domain.private.remove` | Granular domain management — deferred |
| `server.host.binding.set-address-enabled` | Granular binding management — deferred |
| `package.host.address.domain.public.add` | Granular domain management — deferred |
| `package.host.address.domain.public.remove` | Granular domain management — deferred |
| `package.host.address.domain.private.add` | Granular domain management — deferred |
| `package.host.address.domain.private.remove` | Granular domain management — deferred |
| `package.host.address.list` | Per-package address listing — deferred |
| `package.host.binding.list` | Per-package binding listing — deferred |
| `package.host.binding.set-address-enabled` | Granular binding management — deferred |
| `net.gateway.set-default-outbound` | Gateway default route — deferred |

## Body Size Limits

POST request bodies are limited to 1 MiB:

1. `Content-Length` header is checked **before** reading the body (rejects oversized requests immediately).
2. After reading, the actual body size is re-checked as defense-in-depth for chunked transfers that lack `Content-Length`.
