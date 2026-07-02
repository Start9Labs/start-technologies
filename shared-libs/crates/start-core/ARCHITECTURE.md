# start-core Architecture

The shared Rust backend library for StartOS. Cargo package `start-core`, library name `start_core`,
rooted at `shared-libs/crates/start-core`. All Start9 product binaries link against it.

## Library, not a binary

This crate is a **library** (`src/lib.rs`). It exposes every backend subsystem plus a set of
entrypoints under `src/bins/`. The product crates are thin wrappers that pick which entrypoints
to enable via `start_core::bins::MultiExecutable` and call `.execute()`:

| Binary | Wrapper crate / file | Role |
|--------|----------------------|------|
| `startbox` / `startd` | `projects/start-os/src/bin/startbox.rs` | Main OS daemon |
| `start-container` | `projects/start-os/src/bin/start-container.rs` | Runs inside package LXC containers; talks to the host and manages subcontainers |
| `start-cli` | `projects/start-cli/src/main.rs` | CLI over the daemon's JSON-RPC API |
| `registrybox` | `projects/start-registry/src/main.rs` | Package registry server |
| `tunnelbox` | `projects/start-tunnel/src/main.rs` | StartTunnel VPN/forwarding server |

A sixth bin, `startwrt` (`projects/start-wrt/backend/ctrl/src/bin/startwrt.rs`), also links
against this crate but is not a `MultiExecutable` wrapper — it is a full backend of its own
that imports the crate aliased as `startos` for its `net`/`util` primitives.

`MultiExecutable` also supports invoking a chosen entrypoint by argv[0] (busybox-style), which is
how `startbox` dispatches to `startd`, `start-cli`, etc. The per-entrypoint logic lives in
`src/bins/{startd.rs, start_cli.rs, container_cli.rs, registry.rs, tunnel.rs, start_init.rs, …}`.

## Key Modules

- `src/bins/` — Per-binary entrypoints + `MultiExecutable` dispatch
- `src/context/` — Context types (RpcContext, CliContext, InitContext, DiagnosticContext)
- `src/service/` — Service lifecycle management with actor pattern (`service_actor.rs`)
- `src/db/model/` — Patch-DB models (`public.rs` synced to frontend, `private.rs` backend-only)
- `src/net/` — Networking (DNS, ACME, WiFi, Tor, WireGuard, gateway/NAT)
- `src/s9pk/` — S9PK package format (merkle archive)
- `src/lxc/` — LXC container management for packages
- `src/install/`, `src/update/` — Package install and OS/package update flows
- `src/registry/` — Package registry server and management
- `src/tunnel/` — StartTunnel server logic
- `src/backup/`, `src/sign/` — Backup and signing
- `src/os_install/`, `src/init.rs`, `src/setup.rs` — OS install, init, and first-run setup
- `src/util/` — Shared utilities (process invocation, IO, guards — see `core-rust-patterns.md`)
- `src/version/` — Migrations and version logic (see `VERSION_BUMP.md`, `exver.md`)

## RPC Pattern

The API is JSON-RPC (not REST). All endpoints are RPC methods organized in a hierarchical command structure using [rpc-toolkit](https://github.com/Start9Labs/start-technologies/tree/master/shared-libs/crates/rpc-toolkit). Handlers are registered in a tree of `ParentHandler` nodes, with four handler types: `from_fn_async` (standard), `from_fn_async_local` (non-Send), `from_fn` (sync), and `from_fn_blocking` (blocking). Metadata like `.with_about()` drives middleware and documentation.

See [rpc-toolkit.md](rpc-toolkit.md) for full handler patterns and configuration.

## Patch-DB Patterns

Patch-DB provides diff-based state synchronization. Changes to `db/model/public.rs` automatically sync to the frontend.

**Key patterns:**
- `db.peek().await` — Get a read-only snapshot of the database state
- `db.mutate(|db| { ... }).await` — Apply mutations atomically, returns `MutateResult`
- `#[derive(HasModel)]` — Derive macro for types stored in the database, generates typed accessors

**Generated accessor types** (from `HasModel` derive):
- `as_field()` — Immutable reference: `&Model<T>`
- `as_field_mut()` — Mutable reference: `&mut Model<T>`
- `into_field()` — Owned value: `Model<T>`

**`Model<T>` APIs** (from `db/prelude.rs`):
- `.de()` — Deserialize to `T`
- `.ser(&value)` — Serialize from `T`
- `.mutate(|v| ...)` — Deserialize, mutate, reserialize
- For maps: `.keys()`, `.as_idx(&key)`, `.as_idx_mut(&key)`, `.insert()`, `.remove()`, `.contains_key()`

See [patchdb.md](patchdb.md) for `TypedDbWatch<T>` construction, API, and usage patterns.

## i18n

See [i18n-patterns.md](i18n-patterns.md) for internationalization key conventions and the `t!()` macro.

## Rust Utilities & Patterns

See [core-rust-patterns.md](core-rust-patterns.md) for common utilities (Invoke trait, Guard pattern, mount guards, Apply trait, etc.).

## Cross-layer verification

Rust types marked `#[ts(export)]` are the source of truth for TypeScript consumers (the web UI
and the package container-runtime, via the SDK). They do **not** propagate automatically. From
the repo root:

1. `make ts-bindings` — regenerates `shared-libs/crates/start-core/bindings/` (via `build/build-ts.sh`),
   then rsyncs it into `shared-libs/ts-modules/start-core/lib/osBindings/`.
2. `cd shared-libs/ts-modules/start-core && make dist` (what web imports) and
   `cd projects/start-sdk && make bundle` (the SDK bundle container-runtime imports) — rebuilds the
   TS bundles that downstream actually imports.

Until both steps run, a changed `#[ts(export)]` type is out of sync with everything downstream.

## Further reading

- [README.md](README.md) — what this crate is and how to use it
- [CONTRIBUTING.md](CONTRIBUTING.md) — build, test, format, and contribution workflow
- [AGENTS.md](AGENTS.md) — operating rules for AI/dev work (`CLAUDE.md` is a one-line `@AGENTS.md` import)
- [rpc-toolkit.md](rpc-toolkit.md) — JSON-RPC handler patterns
- [patchdb.md](patchdb.md) — Patch-DB watch patterns and TypedDbWatch
- [i18n-patterns.md](i18n-patterns.md) — Internationalization conventions
- [core-rust-patterns.md](core-rust-patterns.md) — Common Rust utilities
- [s9pk-structure.md](s9pk-structure.md) — S9PK package format
