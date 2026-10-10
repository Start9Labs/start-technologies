# AGENTS.md — rpc-toolkit

This library composes typed handlers into JSON-RPC servers and clap CLIs. Read
root and `shared-libs/` guidance first. `CLAUDE.md` imports this file.

## Ownership

- `handler/mod.rs` owns handler contracts and type erasure; `parent.rs` owns
  registration, inheritance and directly callable parents.
- `handler/adapters.rs` owns decorations. `Adapter` and the passthrough traits
  share forwarding without changing caller-specific behavior.
- `handler/from_fn.rs` owns sync, blocking, async and local-async factories.
- `server/` owns dispatch and HTTP/socket transports; `cli.rs` owns clap and
  remote calls.
- `ts.rs` owns handler traversal, deferred parameter/result writers and binding
  composition. `type-helpers.ts` owns method-path parameter/return inference.
- `rpc_toolkit::ts` owns the `TS` trait, renderer, checked declarations, scalar
  bridges and exports. It normalizes `TypeInfo::DECLARATION.source` through pinned
  `serde_derive_internals` for input/output layout, names, defaults and hooks;
  handler and standalone roots share that owner. Generic reflection retains raw
  Rust facts and opaque hints, without serde or TypeScript policy.
- `rpc_toolkit::reflect_ts!`, `ts_export!` and `impl_ts_shape!` are consumer-owned
  `macro_rules!` bridges. Keep them beside DTOs/standalone roots. Generic bridges
  state payload bounds, not recursive whole-root reflection bounds. Reserve
  pending identities before descent; do not impose target `'static` bounds.
- Type-valued hints need explicit local `type_attributes` selectors; hidden storage
  needs explicit `visit(opaque)`. Every selected type needs consumer support.
  Typed `TS::IS_OPTION`, including aliases and wrappers, combines with serde policy.
  Non-omitted fields need typed facts from storage or a selected target, including
  literal replacements; report missing support rather than guessing optionality.
  Actual decoder DTOs and authored hints
  remain authoritative for custom hooks.

## Contributor workflow

Run from the repo root:

```sh
cargo build -p rpc-toolkit
cargo build -p rpc-toolkit --features ts
make rpc-toolkit-test
make start-core-format
make start-core-format-check
cargo check -p start-core
```

`make rpc-toolkit-test` provisions the pinned TypeScript compiler and tests both
CBOR-enabled and JSON-only configurations, including real serde fixtures,
negative TypeScript assertions and RPC dispatch. Dispatch tests run even with
TypeScript disabled. Changes to handler composition also need backend tests.

This crate uses edition 2018. `cbor` is enabled by default; `ts` is opt-in.
Enabling `ts` requires registered handlers to provide bindings or use `no_ts()`.
Without it, the bridge macros expand to nothing, `TS` and `HandlerTSBindings` are
blanket traits and the TS builder methods return the handler unchanged. Consumers
gate the TS-only items they implement behind their own `ts` feature; any build that
enables `rpc-toolkit/ts` must enable those consumer features too.
The developer guide is [docs/typescript.md](docs/typescript.md).

Handlers are immutable and registered through `Arc`. Mutable request state belongs
in the context. Context binding uses a checked `TypeId` comparison before the
existing `DynHandler` transmute; retain matching context bounds across a tree.
Params and results use `imbl-value`; serde bounds must preserve wire dispatch.
