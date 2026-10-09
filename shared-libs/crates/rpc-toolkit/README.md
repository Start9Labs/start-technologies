# rpc-toolkit

A toolkit for creating JSON-RPC 2.0 servers with automatic CLI bindings.

`rpc-toolkit` lets you write typed, composable RPC handlers once and use them two ways: served as a
JSON-RPC 2.0 endpoint (over HTTP, a Unix socket, or TCP) and/or bound to a `clap` command-line
application. Params and results flow through `imbl-value`; sync and async handlers are both
supported; and an optional `ts` feature emits TypeScript bindings for the handler tree,
using separate serde input and output shapes.

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
- `ts` — enables `rpc_toolkit::ts::handler_bindings()` and the typed method tree.
  Struct DTOs derive generic `visit_rs::VisitFields`; enums derive
  `visit_rs::VisitVariants`. Both supply adjacent `rpc_toolkit::reflect_ts!`
  consumer bridges. RPC owns serde normalization,
  TypeScript rendering and typed `ts_export!` roots. See [the TypeScript guide](docs/typescript.md).

Optional `chrono`, `ipnet`, `josekit`, `url`, `yajrc`, `exver` and `patch-db`
features enable consumer-owned scalar/external bridges with `ts`; CBOR bridges
follow `cbor`. Bridges for `imbl-value` and `yasi` use RPC's one-way dependencies.
The `patch-db` bridge enables that crate's generic `reflect` feature. Leaves do
not depend on RPC or expose TypeScript features.

Generate the example module from the monorepo root:

```sh
cargo run -p rpc-toolkit --example generate_ts --features ts
```

## License

MIT. See the `license` field in `Cargo.toml`.

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — how the crate works internally.
- [AGENTS.md](AGENTS.md) — file map and build/test/format workflow.
