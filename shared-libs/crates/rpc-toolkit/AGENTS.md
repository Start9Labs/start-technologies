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
- `ts.rs` owns TypeScript expressions, named definitions and handler bindings.
  `type-helpers.ts` owns method-path parameter/return inference.
- `visit-rs` owns serde shape semantics. Extend its `SerdeShape` derive instead
  of parsing serde attributes again in the renderer.

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
The developer guide is [docs/typescript.md](docs/typescript.md).

Handlers are immutable and registered through `Arc`. Mutable request state belongs
in the context. Context binding uses a checked `TypeId` comparison before the
existing `DynHandler` transmute; retain matching context bounds across a tree.
Params and results use `imbl-value`; serde bounds must preserve wire dispatch.
