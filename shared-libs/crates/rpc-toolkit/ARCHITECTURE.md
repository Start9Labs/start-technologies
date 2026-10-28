# rpc-toolkit Architecture

`rpc-toolkit` turns a single set of typed Rust handlers into both a JSON-RPC 2.0 server and a
CLI. The same handler tree is registered once and then either served (HTTP / Unix socket / TCP)
or driven from the command line through `clap`. Type information flows end to end, optionally
emitting zod schemas and TypeScript types for clients.

## Place in the monorepo

- **Path:** `shared-libs/crates/rpc-toolkit/`
- **Package name:** `rpc-toolkit` (matches the directory). Build/test with `-p rpc-toolkit`.
- **Crate type:** library (`lib`).
- **Consumers:** `start-core`. It is a first-party crate consumed via a direct path dependency —
  no `[patch]` redirect.
- **Notable deps:** `imbl-value` (sibling crate, path dep) for the value model, `yajrc` for the
  JSON-RPC wire types, `axum` for HTTP, `rmcp` (optional) for MCP, `clap` for the CLI, `reqwest` for outbound remote calls,
  and `visit-rs` (optional) for generic raw type reflection.

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
  inheritance), and `NoTS` / `OverrideParamsTS` / `OverrideReturnTS` for binding control.

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
- **MCP** (`server/mcp.rs`, opt-in `mcp` feature): `McpServer` offers methods of an `HttpServer`'s
  tree as [MCP](https://modelcontextprotocol.io) tools over streamable HTTP via `rmcp`, served
  statelessly with JSON responses. The transport overrides session and response-mode
  configuration; response middleware runs after tool dispatch and before HTTP headers are sent.
  The tools are a catalog of `rmcp::model::Tool` descriptions — generated at build
  time by the schema module's `toolCatalog` (`src/tool-catalog.ts`) — checked against the
  tree at construction. A
  `ToolPolicy` decides from each method's metadata whether it is offered and which
  `ToolAnnotations` it carries. `McpService` runs the `HttpServer`'s middleware unchanged:
  `process_http_request` once per HTTP request (shared with the JSON-RPC path through
  `begin_http_request`), then `process_rpc_request`, dispatch and `process_rpc_response` per tool
  call, then `process_http_response`. Handler results become text content, plus
  `structuredContent` when the tool declares an `outputSchema`; RPC errors become `isError`
  results carrying the serialized `RpcError`.
- **Sockets** (`server/socket.rs`): `run_socket` over any `AsyncRead + AsyncWrite`, `run_unix`,
  and `run_tcp`, all line-delimited JSON. `ShutdownHandle` triggers graceful shutdown.

## CLI

`cli.rs` is the mirror image. `CliApp<Context, Config>` takes a `clap` `Config` (a
`CommandFactory + FromArgMatches`) and the same root `ParentHandler`; `run(args)` builds the
`clap::Command` (with each handler contributing a subcommand via its `CliBindings`), parses,
constructs the context, dispatches synchronously, then renders the result with `PrintCliResult`.
`mutate_command` / `into_command` expose the underlying `clap::Command`.

`output.rs` owns stdout/stderr writers, formatted printing, and the exported `print!`,
`println!`, `eprint!` and `eprintln!` macros. A broken output pipe exits with status 141
without changing signal dispositions. Default handler displays and clap diagnostics use
that policy; consumers import the macros with `#[macro_use]` to shadow `std`'s and use the same writers for
custom formatters and serialization.

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

Under the opt-in `ts` feature, handler bindings compose parameter, return and child
writers through `HandlerTSBindings`. Each Rust type renders once, as a zod schema
whose `z.input` is serde's deserialization shape and whose `z.output` is its
serialization shape. RPC parses raw `DeclarationInfo::source` and owns one
normalization plan per direction through pinned `serde_derive_internals`
(`ts/normalize.rs`); `ts/render.rs` renders the two plans jointly and wraps the
narrowest point where they differ in the `wire(input, output)` identity codec.
Normalization owns layout, renames, skips, defaults, hooks, literals and docs.
The parsed AST is normalization input, not a synthetic DTO or Rust
implementation. Whole-root explicit wire overrides precede serde validation.

`VisitFields` and `VisitVariants` supply unconditional `TypeInfo` declaration facts.
RPC collects struct storage through `VisitFieldsStaticNamed`; enum storage uses
`VisitVariantsStatic` and `VisitVariantFieldsStaticNamed`. `FieldInfo::visit_index`
pairs filtered callbacks with original coordinates. RPC's `ShapeKind` dispatcher
uses the method-free `StructKind` and `EnumKind` markers from `TypeInfo::Kind`.
Explicit opacity produces ordinary `Opaque<T>` marker callbacks without storage
bounds. Independent `visit_rs::VisitTypeAttributes` traversal at the runtime root
visits `TypeAttribute<T>` carriers for locally selected metadata literals in their
owner scope. Selected-target support does not constrain field visitors.
RPC selects typed callbacks by
metadata coordinate and registers definitions lazily, avoiding unused or
literal-replaced dependencies. Typed `TS::IS_OPTION` propagates through aliases,
references and Box/Arc/Rc/Cow and combines with serde policy. Hook JSON shape
agreement remains the actual decoder/serializer and authored DTO hint contract.

`rpc_toolkit::reflect_ts!`, `ts_export!` and `impl_ts_shape!` are consumer-owned
`macro_rules!` bridges. Generic roots keep payload bounds and inline by default;
borrowed types retain their lifetimes. Non-omitted fields require typed storage or
selected-target facts, including literal replacements. Missing facts produce an
error rather than guessed optionality. RPC owns scalar/external bridges:
optional chrono/ipnet/josekit/url/yajrc/exver/patch-db integrations, CBOR integration,
and imbl-value/yasi bridges. Dependency edges point from RPC to leaves; patch-db
exposes generic `reflect` support rather than TypeScript facilities.

`TSVisitor` owns the definition registry. Every expression carries its zod schema
type beside its value (`Expr`), which annotates every declaration — so member
JSDoc survives declaration emit — and the getters that recursive object members
become. Literal overrides are hoisted into typed module constants. It registers a named type before expanding its fields, tracks
which declarations each one references eagerly and emits the schema module in
dependency order, rejecting cycles that no object member defers. Type aliases
project `z.output` and `z.input` of each schema for the directions it is reached
from; input aliases receive an `Input` suffix. `into_bindings()` assembles the
schema module and the type-only module, which `handler_bindings()` completes with
the root tree, `RpcSchemaTree` and the `type-helpers.ts` inference helpers.
Generation errors are returned rather than hidden in release builds.

The tree emitter folds each parent's parameters into its children's `_PARAMS`, so
the helpers resolve every dotted segment against actual children and distinguish
namespaces from callable parents without combining anything. Adapter passthrough
preserves the binding opt-out regardless of decoration order. See
[docs/typescript.md](docs/typescript.md) for the supported surface and overrides.

## Further reading

- [README.md](README.md) — what the crate is and a usage sketch.
- [AGENTS.md](AGENTS.md) — file map and build/test/format workflow.
