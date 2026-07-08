# rpc-toolkit Architecture

`rpc-toolkit` turns a single set of typed Rust handlers into both a JSON-RPC 2.0 server and a
CLI. The same handler tree is registered once and then either served (HTTP / Unix socket / TCP)
or driven from the command line through `clap`. Type information flows end to end, optionally
emitting TypeScript definitions for clients.

## Place in the monorepo

- **Path:** `shared-libs/crates/rpc-toolkit/`
- **Package name:** `rpc-toolkit` (matches the directory). Build/test with `-p rpc-toolkit`.
- **Crate type:** library (`lib`).
- **Consumers:** `start-core`. It is a first-party crate consumed via a direct path dependency —
  no `[patch]` redirect.
- **Notable deps:** `imbl-value` (sibling crate, path dep) for the value model, `yajrc` for the
  JSON-RPC wire types, `axum` for HTTP, `clap` for the CLI, `reqwest` for outbound remote calls,
  and `ts-rs` (optional) for TypeScript generation.

## Handlers

A handler is described by `HandlerTypes` (its `Params`, `InheritedParams`, `Ok`, and `Err`
associated types) and implemented for a context by `HandlerFor<Context>`. `HandlerArgs` bundles
everything a handler receives: the context, the parent method path, the method name, parsed and
raw params, and inherited params.

- **Leaf handlers** come from `from_fn` (sync), `from_fn_blocking`, `from_fn_async`, and
  `from_fn_async_local`, producing `FromFn` / `FromFnAsync` / `FromFnAsyncLocal`. Each can attach
  metadata with `with_metadata`.
- **Parent handlers** are `ParentHandler`, which holds a map of named children added with
  `subcommand(...)` (and a `root_handler(...)` for the unnamed root). This is how a method
  namespace / subcommand tree is built.
- **Adapters** decorate handlers through the `HandlerExt` extension trait: `NoCli` (server-only,
  hidden from the CLI), `NoDisplay` / `CustomDisplay` / `CustomDisplayFn` (control CLI output),
  `WithAbout` (clap help text), `RemoteCaller` / `InheritanceHandler` (delegation and param
  inheritance), and the TS-output controls `NoTS` / `UnknownTS` / `CustomTS`.

Erasure happens through the internal `AnyHandler` and the public `DynHandler`. `WithContext` plus
`Handler::handler_for` do the runtime `TypeId`-checked context binding (see AGENTS.md "Gotchas").
`Empty` is the zero-field params/inherited type; `OrEmpty` lets a handler accept either real
inherited params or `Empty`; `Never` is the uninhabited error type.

## Serving

`Server<Context>` (in `server/mod.rs`) is the dispatch engine. It is constructed from a
context factory plus the root `ParentHandler`, and exposes:

- `handle_command(method, params)` — resolve a dotted method and run it.
- `handle(request)` — decode and run a single or batch JSON-RPC request.
- `stream(requests)` — drive a stream of requests concurrently via `JobRunner`.

Transports wrap a `Server`:

- **HTTP** (`server/http.rs`): `HttpServer` on `axum`, with content-negotiated CBOR/JSON encoding
  and a `Middleware` pipeline (`process_http_request`, `process_rpc_request`, `process_rpc_response`,
  `process_http_response`, all defaulting to no-ops) erased through `DynMiddleware`.
- **Sockets** (`server/socket.rs`): `run_socket` over any `AsyncRead + AsyncWrite`, `run_unix`,
  and `run_tcp`, all line-delimited JSON. `ShutdownHandle` triggers graceful shutdown.

## CLI

`cli.rs` is the mirror image. `CliApp<Context, Config>` takes a `clap` `Config` (a
`CommandFactory + FromArgMatches`) and the same root `ParentHandler`; `run(args)` builds the
`clap::Command` (with each handler contributing a subcommand via its `CliBindings`), parses,
constructs the context, dispatches synchronously, then renders the result with `PrintCliResult`.
`mutate_command` / `into_command` expose the underlying `clap::Command`.

For talking to a _remote_ server, `CallRemote` + `CallRemoteHandler` forward a local invocation
over the wire using `call_remote_http` (CBOR or JSON over `reqwest`) or `call_remote_socket`
(line-delimited JSON over a duplex connection).

## Values, errors, and utilities

Everything on the wire is an `imbl_value::Value`. `util.rs` provides the error constructors that
map failures to `yajrc::RpcError` (`invalid_params`, `invalid_request`, `parse_error`,
`internal_error`), value surgery helpers (`extract`, `without`, `combine`), the `Flat<A, B>`
param-flattening newtype, and the async plumbing (`JobRunner`, `StreamUntil`, `poll_select_all`).
`command_helpers.rs` supplies clap arg/stdin parsers (`default_arg_parser`, `default_stdin_parser`).

## TypeScript generation

Under the `ts-rs` feature, handlers carry type info via `HandlerTS`, and `type_helpers()` returns
the `src/type-helpers.ts` sidecar describing the generated tree (`RpcHandler`, `ParentHandler`,
`LeafHandler`, and the `RpcParamType` / return-type helpers). This lets a TypeScript client be
typed against the same handler tree the server runs.

## Further reading

- [README.md](README.md) — what the crate is and a usage sketch.
- [AGENTS.md](AGENTS.md) — file map and contributor gotchas.
- [CONTRIBUTING.md](CONTRIBUTING.md) — build/test/format and PR conventions.
