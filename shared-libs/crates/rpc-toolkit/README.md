# rpc-toolkit

A toolkit for creating JSON-RPC 2.0 servers with automatic CLI bindings.

`rpc-toolkit` lets you write typed, composable RPC handlers once and use them two ways: served as a
JSON-RPC 2.0 endpoint (over HTTP, a Unix socket, or TCP) and/or bound to a `clap` command-line
application. Params and results flow through `imbl-value`; sync and async handlers are both
supported; and an optional `ts-rs` feature emits TypeScript type definitions for the handler tree.

## Place in the monorepo

- **Path:** `shared-libs/crates/rpc-toolkit/`
- **Package name:** `rpc-toolkit` (same as the directory). Build/test from the repo root with
  `-p rpc-toolkit`.
- **Crate type:** library.
- **Consumers:** `start-core`.
- **First-party:** consumed via a direct path dependency — no `[patch]` redirect.

## Usage

Define a context, write handlers, compose them into a `ParentHandler`, and serve:

```rust
use rpc_toolkit::{from_fn_async, Context, ParentHandler, Server};
use serde::{Deserialize, Serialize};
use yajrc::RpcError;

#[derive(Clone)]
struct MyContext;
impl Context for MyContext {}

#[derive(Debug, Deserialize, Serialize, clap::Parser)]
struct GreetParams {
    name: String,
}

async fn greet(_ctx: MyContext, params: GreetParams) -> Result<String, RpcError> {
    Ok(format!("hello, {}", params.name))
}

let root = ParentHandler::<MyContext>::new()
    .subcommand("greet", from_fn_async(greet));

let server = Server::new(|| async { Ok(MyContext) }, root);
```

The same `ParentHandler` can be handed to `CliApp` (see `cli` module) to expose the handlers as
CLI subcommands, or served over HTTP via `HttpServer` / over a socket via `Server::run_unix` /
`run_tcp`. See [ARCHITECTURE.md](ARCHITECTURE.md) for the full surface.

## Features

- `cbor` _(default)_ — enables CBOR request/response encoding alongside JSON over HTTP.
- `ts-rs` — emits TypeScript type definitions; exposes `type_helpers()`.

## License

MIT. See the `license` field in `Cargo.toml`.

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — how the crate works internally.
- [CONTRIBUTING.md](CONTRIBUTING.md) — build, test, format, and PR conventions.
- [AGENTS.md](AGENTS.md) — file map and gotchas for agents (`CLAUDE.md` is a one-line `@AGENTS.md`
  import).
