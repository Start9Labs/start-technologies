# rpc-toolkit

StartOS uses [rpc-toolkit](https://github.com/Start9Labs/start-technologies/tree/master/shared-libs/crates/rpc-toolkit) for its JSON-RPC API. This document covers the patterns used in this codebase.

## Overview

The API is JSON-RPC (not REST). All endpoints are RPC methods organized in a hierarchical command structure.

## Handler Functions

There are four types of handler functions, chosen based on the function's characteristics:

### `from_fn_async` - Async handlers

For standard async functions. Most handlers use this.

```rust
pub async fn my_handler(ctx: RpcContext, params: MyParams) -> Result<MyResponse, Error> {
    // Can use .await
}

from_fn_async(my_handler)
```

If a handler takes no params, simply omit the params argument entirely (no need for `_: Empty`):

```rust
pub async fn no_params_handler(ctx: RpcContext) -> Result<MyResponse, Error> {
    // ...
}
```

### `from_fn_async_local` - Non-thread-safe async handlers

For async functions that are not `Send` (cannot be safely moved between threads). Use when working with non-thread-safe types.

```rust
pub async fn cli_download(ctx: CliContext, params: Params) -> Result<(), Error> {
    // Non-Send async operations
}

from_fn_async_local(cli_download)
```

### `from_fn_blocking` - Sync blocking handlers

For synchronous functions that perform blocking I/O or long computations.

```rust
pub fn query_dns(ctx: RpcContext, params: DnsParams) -> Result<DnsResponse, Error> {
    // Blocking operations (file I/O, DNS lookup, etc.)
}

from_fn_blocking(query_dns)
```

### `from_fn` - Sync non-blocking handlers

For pure functions or quick synchronous operations with no I/O.

```rust
pub fn echo(ctx: RpcContext, params: EchoParams) -> Result<String, Error> {
    Ok(params.message)
}

from_fn(echo)
```

## ParentHandler

Groups related RPC methods into a hierarchy:

```rust
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};

pub fn my_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("list", from_fn_async(list_handler).with_call_remote::<CliContext>())
        .subcommand("create", from_fn_async(create_handler).with_call_remote::<CliContext>())
}
```

## Handler Extensions

Chain methods to configure handler behavior.

**Ordering rules:**

1. `with_about()` must come AFTER other CLI modifiers (`no_display()`, `with_custom_display_fn()`, etc.)
2. `with_call_remote()` must be the LAST adapter in the chain

| Method                                    | Purpose                                            |
| ----------------------------------------- | -------------------------------------------------- |
| `.with_metadata("key", Value)`            | Attach metadata for middleware                     |
| `.no_cli()`                               | RPC-only, not available via CLI                    |
| `.no_display()`                           | No CLI output                                      |
| `.with_display_serializable()`            | Default JSON/YAML output for CLI                   |
| `.with_custom_display_fn(\|_, res\| ...)` | Custom CLI output formatting                       |
| `.with_about("about.description")`        | Add help text (i18n key) - **after CLI modifiers** |
| `.with_call_remote::<CliContext>()`       | Enable CLI to call remotely - **must be last**     |

### Correct ordering example:

```rust
from_fn_async(my_handler)
    .with_metadata("sync_db", Value::Bool(true))  // metadata early
    .no_display()                                  // CLI modifier
    .with_about("about.my-handler")                // after CLI modifiers
    .with_call_remote::<CliContext>()              // always last
```

## Metadata by Middleware

Metadata tags are processed by different middleware. Group them logically:

### Auth Middleware (`middleware/auth/mod.rs`)

| Metadata        | Default | Description                                                                    |
| --------------- | ------- | ------------------------------------------------------------------------------ |
| `authenticated` | `true`  | Whether endpoint requires authentication. Set to `false` for public endpoints. |

### Session Auth Middleware (`middleware/auth/session.rs`)

| Metadata      | Default | Description                                                          |
| ------------- | ------- | -------------------------------------------------------------------- |
| `login`       | `false` | Special handling for login endpoints (rate limiting, cookie setting) |
| `get_session` | `false` | Inject session ID into params as `__Auth_session`                    |

### Signature Auth Middleware (`middleware/auth/signature.rs`)

| Metadata     | Default | Description                                             |
| ------------ | ------- | ------------------------------------------------------- |
| `get_signer` | `false` | Inject signer public key into params as `__Auth_signer` |

### Registry Auth (extends Signature Auth)

| Metadata          | Default | Description                                             |
| ----------------- | ------- | ------------------------------------------------------- |
| `admin`           | `false` | Require admin privileges (signer must be in admin list) |
| `get_device_info` | `false` | Inject device info header for hardware filtering        |

### Database Middleware (`middleware/db.rs`)

| Metadata  | Default | Description                                                 |
| --------- | ------- | ----------------------------------------------------------- |
| `sync_db` | `false` | Sync database after mutation, add `X-Patch-Sequence` header |

## Context Types

Different contexts for different execution environments:

- `RpcContext` - Web/RPC requests with full service access
- `CliContext` - CLI operations, calls remote RPC
- `InitContext` - During system initialization
- `DiagnosticContext` - Diagnostic/recovery mode
- `RegistryContext` - Registry daemon context
- `EffectContext` - Service effects context (container-to-host calls)

## Parameter Structs

Parameters use derive macros for JSON-RPC, CLI parsing, and TypeScript generation:

```rust
#[derive(Deserialize, Serialize, Parser, TS)]
#[serde(rename_all = "camelCase")]  // JSON-RPC uses camelCase
#[command(rename_all = "kebab-case")]  // CLI uses kebab-case
#[ts(export)]  // Generate TypeScript types
pub struct MyParams {
    pub package_id: PackageId,
}
```

### Middleware Injection

Auth middleware can inject values into params using special field names:

```rust
#[derive(Deserialize, Serialize, Parser, TS)]
pub struct MyParams {
    #[ts(skip)]
    #[serde(rename = "__Auth_session")]  // Injected by session auth
    session: InternedString,

    #[ts(skip)]
    #[serde(rename = "__Auth_signer")]   // Injected by signature auth
    signer: AnyVerifyingKey,

    #[ts(skip)]
    #[serde(rename = "__Auth_userAgent")] // Injected during login
    user_agent: Option<String>,
}
```

## Common Patterns

### Adding a New RPC Endpoint

1. Define params struct with `Deserialize, Serialize, Parser, TS` (skip if no params needed)
2. Choose handler type based on sync/async and thread-safety
3. Write handler function taking `(Context, Params) -> Result<Response, Error>` (omit Params if none needed)
4. Add to parent handler with appropriate extensions (display modifiers before `with_about`)
5. TypeScript types auto-generated via `make start-core-ts-bindings`

### Public (Unauthenticated) Endpoint

```rust
from_fn_async(get_info)
    .with_metadata("authenticated", Value::Bool(false))
    .with_display_serializable()
    .with_about("about.get-info")
    .with_call_remote::<CliContext>()  // last
```

### Mutating Endpoint with DB Sync

```rust
from_fn_async(update_config)
    .with_metadata("sync_db", Value::Bool(true))
    .no_display()
    .with_about("about.update-config")
    .with_call_remote::<CliContext>()  // last
```

### Session-Aware Endpoint

```rust
from_fn_async(logout)
    .with_metadata("get_session", Value::Bool(true))
    .no_display()
    .with_about("about.logout")
    .with_call_remote::<CliContext>()  // last
```

## File Locations

- Handler definitions: Throughout `core/src/` modules
- Main API tree: `core/src/lib.rs` (`main_api()`, `server()`, `package()`)
- Auth middleware: `core/src/middleware/auth/`
- DB middleware: `core/src/middleware/db.rs`
- Context types: `core/src/context/`
