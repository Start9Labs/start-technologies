# AGENTS.md — rpc-toolkit

`rpc-toolkit` is a first-party library crate in the start-technologies monorepo. It provides a toolkit
for building JSON-RPC 2.0 servers whose operations double as CLI subcommands: you write
typed, composable handlers once, then serve them over HTTP, a Unix socket, or TCP, and/or
bind them to a `clap` command-line application. `CLAUDE.md` is a one-line `@AGENTS.md` import.
See [ARCHITECTURE.md](ARCHITECTURE.md) for how it fits together.

## Layout

- `src/lib.rs` — crate root. Re-exports `cli`, `context`, `handler`, and `server`; re-exports the
  upstream crates `clap`, `futures`, `reqwest`, `serde`, `serde_json`, `tokio`, `url`, `yajrc`.
  Defines `type_helpers()` (only under the `ts-rs` feature).
- `src/context.rs` — the `Context` trait: the minimal interface a handler's shared state must
  satisfy (`Send + Sync + 'static`, optional `runtime()`).
- `src/handler/mod.rs` — core handler traits and the dispatch machinery: `HandlerTypes`,
  `HandlerFor`, `Handler`, `CliBindings`, `PrintCliResult`, `HandlerTS`, `HandlerArgs`,
  `DynHandler`, `WithContext`, `Empty`, `Never`, `OrEmpty`, and the internal `AnyHandler`.
- `src/handler/from_fn.rs` — leaf-handler factories: `from_fn`, `from_fn_blocking`,
  `from_fn_async`, `from_fn_async_local`, returning `FromFn` / `FromFnAsync` / `FromFnAsyncLocal`
  (each carries optional `with_metadata`).
- `src/handler/parent.rs` — `ParentHandler`, for composing a tree of named subcommands with
  `subcommand(...)`, `root_handler(...)`, and `with_metadata(...)`.
- `src/handler/adapters.rs` — the `HandlerExt` decorator trait and its wrappers: `NoCli`,
  `NoDisplay`, `CustomDisplay`, `CustomDisplayFn`, `RemoteCaller`, `InheritanceHandler`,
  `WithAbout`, `NoTS`, `UnknownTS`, `CustomTS`.
- `src/server/mod.rs` — `Server`: the JSON-RPC engine. `handle_command`, `handle` (single or
  batch), and `stream` (over a stream of requests via `JobRunner`). Defines the `GenericRpcMethod`
  / `RpcRequest` / `RpcResponse` / `SingleOrBatchRpcRequest` type aliases.
- `src/server/http.rs` — HTTP transport on `axum`: `HttpServer`, the `Middleware` trait (with
  no-op defaults) and `DynMiddleware`, plus `json_http_response` / `fallback_rpc_error_response`.
  CBOR or JSON request/response encoding.
- `src/server/socket.rs` — Unix-socket and TCP transports: `Server::run_socket`, `run_unix`,
  `run_tcp`, and `ShutdownHandle`. Line-delimited JSON RPC.
- `src/cli.rs` — the CLI side: `CliApp` (binds a `ParentHandler` tree to `clap`), the
  `CallRemote` trait, `call_remote_http` / `call_remote_socket`, and `CallRemoteHandler`.
- `src/command_helpers.rs` — `default_arg_parser` and `default_stdin_parser` for clap args/stdin.
- `src/util.rs` — error constructors (`invalid_params`, `invalid_request`, `parse_error`,
  `internal_error`), `Value` helpers (`extract`, `without`, `combine`), `Flat<A, B>`,
  `JobRunner`, `StreamUntil`, `poll_select_all`, and a context-marker `PhantomData<T>`.
- `src/type-helpers.ts` — the TypeScript sidecar returned verbatim by `type_helpers()`; describes
  the shape of generated RPC handler trees (`RpcHandler` / `ParentHandler` / `LeafHandler`).
- `tests/test.rs` — integration tests exercising server dispatch and handler composition.

## Build & test (run from the repo root)

```bash
cargo build -p rpc-toolkit
cargo build -p rpc-toolkit --features ts-rs        # enable TypeScript type generation
cargo build -p rpc-toolkit --no-default-features   # JSON-only, no CBOR transport
cargo test  -p rpc-toolkit
```

## Gotchas

- **Edition 2018.** This crate is on `edition = "2018"`, older than the workspace default. Don't
  assume 2021/2024 conveniences (e.g. disjoint closure captures, `IntoIterator` for arrays in
  older forms) are available here.
- **`cbor` feature is on by default.** It pulls in `serde_cbor` and makes the HTTP transport
  negotiate `application/cbor`. With `--no-default-features` only JSON transport exists.
- **`ts-rs` feature is off by default.** It enables `type_helpers()` (returns `type-helpers.ts`)
  and the `HandlerTS::type_info` machinery for emitting TypeScript bindings. Handlers gate TS
  output through `NoTS` / `UnknownTS` / `CustomTS`; nothing TS-related compiles without the feature.
- **Runtime context type-check uses `unsafe transmute`.** `Handler::handler_for` compares
  `TypeId`s and `std::mem::transmute`s the `DynHandler` when they match. A handler whose `Context`
  bound disagrees with the `Server`/`CliApp` it is registered into simply yields `None` (the
  subcommand is dropped); the transmute only runs on a verified `TypeId` match. Keep `Context`
  bounds consistent across a handler tree.
- **`Middleware` defaults are no-ops.** The four HTTP/RPC hooks have default implementations, so a
  `Middleware` impl only needs to override the stages it cares about.
- **Handlers are immutable.** Registration uses `Arc`-wrapped, cloned handlers — no `RwLock`/`Mutex`
  around the handler tree. Per-request mutable state belongs in your `Context`.
- **`imbl_value` everywhere.** Params and results flow through `imbl_value::Value`; derive ordering
  and `Serialize`/`DeserializeOwned` bounds on your param/return types must line up or dispatch
  fails at runtime with `invalid_params` / `internal_error`.

## Consumer verification

Exercise dispatch, composition, and CLI/server binding changes in `tests/test.rs`.
Update consumers of a changed public API in the same change and run
`cargo check -p start-core`.
